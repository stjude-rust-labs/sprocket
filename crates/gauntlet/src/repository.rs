//! A local repository of files from a remote GitHub repository.

use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use faster_hex;
use git2::FetchOptions;
use git2::build::RepoBuilder;
use serde::Deserialize;
use serde::Serialize;
use tracing::info;

pub mod identifier;
pub mod work_dir;

pub use identifier::Identifier;
pub use work_dir::WorkDir;

/// Fetch up to this many commits when cloning a repository.
const FETCH_DEPTH: i32 = 25;

/// A byte slice that can be converted to a [`git2::Oid`].
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RawHash([u8; 20]);

impl Serialize for RawHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        faster_hex::hex_string(&self.0).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RawHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.len() != 40 {
            return Err(serde::de::Error::custom(
                "a commit hash must have 40 characters",
            ));
        }

        let mut hash = [0u8; 20];
        faster_hex::hex_decode(s.as_bytes(), &mut hash).map_err(serde::de::Error::custom)?;
        Ok(Self(hash))
    }
}

impl fmt::Display for RawHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

/// A GitHub repository of WDL files.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Repository {
    /// The name for the [`Repository`] expressed as an [`Identifier`].
    identifier: Identifier,

    /// The commit hash for the [`Repository`].
    commit_hash: Option<RawHash>,

    /// A list of documents that should be filtered out from the repository.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    filters: Vec<String>,
}

impl Repository {
    /// Create a new [`Repository`].
    /// Repositories initialized with this method will _always_ have
    /// `Some(commit_hash)`.
    pub fn new(identifier: Identifier, commit_hash: Option<RawHash>, work_dir: &Path) -> Self {
        let repo_root = work_dir
            .join(identifier.organization())
            .join(identifier.name());

        info!("cloning repository: {:?}", identifier);
        let mut fo = FetchOptions::new();
        fo.depth(FETCH_DEPTH);
        let git_repo = RepoBuilder::new()
            .fetch_options(fo)
            .clone(
                format!("https://github.com/{}.git", identifier).as_str(),
                &repo_root,
            )
            .expect("failed to clone repository");

        let commit_hash = match commit_hash {
            Some(hash) => {
                let obj = git_repo
                    .find_object(
                        git2::Oid::from_bytes(&hash.0).expect("failed to convert hash"),
                        Some(git2::ObjectType::Commit),
                    )
                    .expect("failed to find object");
                git_repo
                    .set_head_detached(obj.id())
                    .expect("failed to set head detached");
                hash
            }
            None => {
                let head = git_repo.head().expect("failed to get head");
                let commit = head.peel_to_commit().expect("failed to peel to commit");

                let mut bytes = [0u8; 20];
                bytes.copy_from_slice(commit.id().as_bytes());
                RawHash(bytes)
            }
        };

        Self {
            identifier,
            commit_hash: Some(commit_hash),
            filters: Default::default(),
        }
    }

    /// Gets the repository identifier from the [`Repository`] by reference.
    pub fn identifier(&self) -> &Identifier {
        &self.identifier
    }

    /// Gets the commit hash from the [`Repository`] by reference.
    pub fn commit_hash(&self) -> &Option<RawHash> {
        &self.commit_hash
    }

    /// Gets the file path filters for the repository.
    pub fn filters(&self) -> &[String] {
        &self.filters
    }

    /// Check out files from the [`Repository`].
    pub fn checkout(&self, root: &Path) -> PathBuf {
        let repo_root = root
            .join(self.identifier.organization())
            .join(self.identifier.name());

        let git_repo = match git2::Repository::open(&repo_root) {
            Ok(repo) => {
                info!("opening existing repository: {:?}", repo_root);
                repo
            }
            Err(_) => {
                info!("cloning repository: {:?}", self.identifier);
                let mut fo = FetchOptions::new();
                fo.depth(FETCH_DEPTH);
                RepoBuilder::new()
                    .fetch_options(fo)
                    .clone(
                        format!("https://github.com/{}.git", self.identifier).as_str(),
                        &repo_root,
                    )
                    .expect("failed to clone repository")
            }
        };

        match self.commit_hash.clone() {
            Some(hash) => {
                let obj = git_repo
                    .find_object(
                        git2::Oid::from_bytes(&hash.0).expect("failed to convert hash"),
                        Some(git2::ObjectType::Commit),
                    )
                    .expect("failed to find object");
                git_repo
                    .set_head_detached(obj.id())
                    .expect("failed to set head detached");
                let mut co = git2::build::CheckoutBuilder::new();
                co.force();
                git_repo
                    .checkout_head(Some(&mut co))
                    .expect("failed to checkout head");
            }
            None => {
                unreachable!("commit hash must be set");
            }
        }

        for mut submodule in git_repo
            .submodules()
            .expect("failed to load repository submodules")
        {
            let mut co = git2::build::CheckoutBuilder::new();
            co.force();
            let mut opts = git2::SubmoduleUpdateOptions::new();
            opts.checkout(co);

            submodule
                .update(true, Some(&mut opts))
                .expect("failed to update submodule");
        }

        repo_root
    }

    /// Update to the latest commit hash for the [`Repository`].
    pub fn update(&mut self, root: &Path) {
        let repo_root = root
            .join(self.identifier.organization())
            .join(self.identifier.name());

        // Clone the repository.
        info!("cloning repository: {:?}", self.identifier);
        let mut fo = FetchOptions::new();
        fo.depth(FETCH_DEPTH);
        let git_repo = RepoBuilder::new()
            .fetch_options(fo)
            .clone(
                format!("https://github.com/{}.git", self.identifier).as_str(),
                &repo_root,
            )
            .expect("failed to clone repository");

        // Update the commit hash.
        let head = git_repo.head().expect("failed to get head");
        let commit = head.peel_to_commit().expect("failed to peel to commit");

        let mut bytes = [0u8; 20];
        bytes.copy_from_slice(commit.id().as_bytes());
        self.commit_hash = Some(RawHash(bytes));
    }
}
