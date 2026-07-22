//! Content hashing per the WDL module spec.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use serde_with::DeserializeFromStr;
use serde_with::SerializeDisplay;
use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;

use crate::module_walk::ModuleWalkError;
use crate::relative_path::RelativePath;
use crate::relative_path::RelativePathError;
use crate::tree::TreeError;

/// An error during content hashing.
#[derive(Debug, Error)]
pub enum HashError {
    /// A path supplied to [`Hasher::try_add`] failed relative-path
    /// validation.
    #[error(transparent)]
    InvalidPath(#[from] RelativePathError),

    /// An absolute path was supplied that does not live under the module
    /// root.
    #[error("absolute path `{0}` is not under the module root")]
    AbsoluteNotUnderRoot(String),

    /// A new path collides under Unicode Normalization Form C (NFC) with a
    /// path that was already recorded. The spec requires the module's set
    /// of relative paths to be unique under NFC.
    #[error("path `{path}` collides with an already-recorded path under NFC form `{nfc}`")]
    AmbiguousPath {
        /// The newly-submitted path that collided.
        path: String,
        /// The shared NFC form.
        nfc: String,
    },

    /// I/O failure while reading a file.
    #[error("failed to read `{path}`")]
    Io {
        /// The path of the file that failed to read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// A tree walk error (symlink containment, metadata target, etc.).
    #[error(transparent)]
    Walk(#[from] ModuleWalkError),

    /// A module file-tree validation error (reserved-filename placement,
    /// NFC duplicate paths).
    #[error(transparent)]
    Tree(#[from] TreeError),
}

/// An error parsing a [`ContentHash`].
#[derive(Debug, Error)]
pub enum ContentHashError {
    /// The string does not start with the required `sha256:` prefix.
    #[error("content hash must start with `sha256:`")]
    MissingPrefix,

    /// The hex portion of the hash is not 64 characters.
    #[error("content hash must be exactly 64 hex characters; got {0}")]
    WrongLength(usize),

    /// The hex portion contains non-hex characters.
    #[error("content hash contains non-hex characters")]
    InvalidHex,
}

/// The prefix used in the wire form of a [`ContentHash`].
const SHA256_PREFIX: &str = "sha256:";

/// Domain-separation magic prepended to the SHA-256 input by
/// [`Hasher::finalize`].
const CONTENT_HASH_MAGIC: &[u8] = b"wdl-module-content\0v1\0";

/// A 32-byte SHA-256 module content hash.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, SerializeDisplay, DeserializeFromStr,
)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Returns the raw 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the hash as a 64-character lowercase hex string (without the
    /// `sha256:` prefix).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl From<[u8; 32]> for ContentHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<ContentHash> for String {
    fn from(hash: ContentHash) -> Self {
        hash.to_string()
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SHA256_PREFIX}{}", hex::encode(self.0))
    }
}

impl FromStr for ContentHash {
    type Err = ContentHashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s
            .strip_prefix(SHA256_PREFIX)
            .ok_or(ContentHashError::MissingPrefix)?;
        if hex.len() != 64 {
            return Err(ContentHashError::WrongLength(hex.len()));
        }
        let bytes: [u8; 32] = hex::decode(hex)
            .map_err(|_| ContentHashError::InvalidHex)?
            .try_into()
            .map_err(|_| ContentHashError::WrongLength(hex.len()))?;
        Ok(Self(bytes))
    }
}

/// An incremental content hasher for a module directory.
///
/// `try_add` records relative paths into a [`BTreeSet`], so they are kept in
/// lexicographic order as they are inserted. `finalize` walks them in that
/// order, opens each file under the configured root, and feeds path bytes
/// plus raw file contents into a single SHA-256 state.
#[derive(Debug)]
pub struct Hasher {
    /// The directory under which all recorded relative paths resolve.
    root: PathBuf,
    /// The set of relative paths recorded so far, kept sorted.
    paths: BTreeSet<RelativePath>,
}

impl Hasher {
    /// Creates a new [`Hasher`] rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            paths: BTreeSet::new(),
        }
    }

    /// Returns an iterator over the recorded paths in lexicographic order.
    pub fn paths(&self) -> impl Iterator<Item = &RelativePath> {
        self.paths.iter()
    }

    /// Records a path relative to the hasher's root.
    ///
    /// Accepts an absolute path under `root` (which is converted to a relative
    /// form) or any [`RelativePath`]-convertible input.
    pub fn try_add(&mut self, path: impl Into<String>) -> Result<&mut Self, HashError> {
        let raw = path.into();

        let candidate = Path::new(&raw);
        let relative = if candidate.is_absolute() {
            candidate
                .strip_prefix(&self.root)
                .map_err(|_| HashError::AbsoluteNotUnderRoot(raw.clone()))?
        } else {
            candidate
        };

        let rel = RelativePath::try_from(relative)?;

        let nfc = rel.as_str().to_string();
        if !self.paths.insert(rel) {
            return Err(HashError::AmbiguousPath { path: raw, nfc });
        }
        Ok(self)
    }

    /// Computes the [`ContentHash`] of the recorded paths.
    ///
    /// Each file's full path is canonicalized before reading; if the
    /// resolved target falls outside the module root, the module is
    /// rejected. The tree walk already forbids symbolic links anywhere
    /// in a module, so this is a defensive backstop.
    pub fn finalize(self) -> Result<ContentHash, HashError> {
        crate::tree::validate_tree(self.paths())?;

        let canonical_root = std::fs::canonicalize(&self.root).map_err(|source| HashError::Io {
            path: self.root.clone(),
            source,
        })?;

        let mut sha = Sha256::new();
        sha.update(CONTENT_HASH_MAGIC);
        // NOTE: paths are sorted by the [`BTreeSet`].
        for relative in &self.paths {
            let bytes = relative.as_str().as_bytes();
            sha.update((bytes.len() as u64).to_le_bytes());
            sha.update(bytes);

            let abs = self.root.join(relative);
            let canonical_abs = std::fs::canonicalize(&abs).map_err(|source| HashError::Io {
                path: abs.clone(),
                source,
            })?;

            if !canonical_abs.starts_with(&canonical_root) {
                return Err(ModuleWalkError::Symlink(relative.as_str().to_string()).into());
            }

            let mut file = File::open(&canonical_abs).map_err(|source| HashError::Io {
                path: canonical_abs.clone(),
                source,
            })?;
            let len = file
                .metadata()
                .map_err(|source| HashError::Io {
                    path: canonical_abs.clone(),
                    source,
                })?
                .len();
            sha.update(len.to_le_bytes());
            let mut buffer = [0; 8192];
            loop {
                let bytes = file.read(&mut buffer).map_err(|source| HashError::Io {
                    path: canonical_abs.clone(),
                    source,
                })?;

                if bytes == 0 {
                    break;
                }

                sha.update(&buffer[..bytes]);
            }
        }

        sha.update((self.paths.len() as u64).to_le_bytes());
        Ok(ContentHash::from(<[u8; 32]>::from(sha.finalize())))
    }
}

/// Computes the content hash of a directory by walking it (excluding the
/// spec-mandated exclusions `module.sig` and `module-lock.json`).
/// Directory and file names that are not module content and should
/// be excluded from hashing, limit checks, and content walks.
pub(crate) const NON_MODULE_CONTENT: &[&str] = &[".git", ".sprocket"];

/// Walks `root` and computes the deterministic content hash of the
/// module directory, skipping non-module content and spec-defined
/// exclusions.
pub fn hash_directory(root: impl AsRef<Path>) -> Result<ContentHash, HashError> {
    let root = root.as_ref();
    let mut hasher = Hasher::new(root.to_path_buf());

    crate::module_walk::walk_module_tree(root, &mut |path: &Path, _size| {
        // SAFETY: the walker only yields paths under `root`.
        let rel_path = path.strip_prefix(root).unwrap();
        let rel = rel_path
            .to_str()
            .ok_or(RelativePathError::NonUtf8)?
            .replace('\\', "/");
        // Spec-defined hash exclusions (present in tree but not hashed).
        if rel == crate::SIGNATURE_FILENAME || rel == crate::LOCKFILE_FILENAME {
            return Ok(());
        }
        hasher.try_add(rel)?;
        Ok(())
    })
    .map_err(|e| match e {
        crate::module_walk::WalkError::Walk(w) => HashError::from(w),
        crate::module_walk::WalkError::Visitor(h) => h,
    })?;

    crate::tree::validate_tree(hasher.paths())?;

    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn round_trips_via_display() {
        let bytes = [0xAB; 32];
        let hash = ContentHash::from(bytes);
        let s = hash.to_string();
        assert!(s.starts_with("sha256:"));
        let parsed: ContentHash = s.parse().unwrap();
        assert_eq!(parsed, hash);
    }

    #[test]
    fn rejects_missing_prefix() {
        assert!(matches!(
            "ab".repeat(32).parse::<ContentHash>(),
            Err(ContentHashError::MissingPrefix)
        ));
    }

    #[test]
    fn rejects_bad_hex() {
        let s = format!("sha256:{}", "g".repeat(64));
        assert!(matches!(
            s.parse::<ContentHash>(),
            Err(ContentHashError::InvalidHex)
        ));
    }

    #[test]
    fn rejects_unrecoverable_paths() {
        let dir = tempdir().unwrap();
        let mut h = Hasher::new(dir.path().to_path_buf());
        for bad in [
            "",                          // empty
            ".",                         // resolves to empty
            "..",                        // escapes root
            "../escape",                 // escapes root
            "/somewhere/not/under/root", // absolute, not under root
            "has\0null",                 // null byte
            "C:/win",                    // Windows drive letter
            "c:\\win",                   // lowercase drive letter
        ] {
            assert!(h.try_add(bad).is_err(), "accepted `{bad}`");
        }
    }

    #[test]
    fn normalizes_relative_paths() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("foo.txt"), b"x").unwrap();

        let mut h_clean = Hasher::new(dir.path().to_path_buf());
        h_clean.try_add("foo.txt").unwrap();

        let mut h_dotty = Hasher::new(dir.path().to_path_buf());
        h_dotty.try_add("./bar/../foo.txt").unwrap();

        assert_eq!(h_clean.finalize().unwrap(), h_dotty.finalize().unwrap());
    }

    #[test]
    fn accepts_absolute_under_root() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("foo.txt"), b"x").unwrap();

        let mut h_rel = Hasher::new(dir.path().to_path_buf());
        h_rel.try_add("foo.txt").unwrap();

        let mut h_abs = Hasher::new(dir.path().to_path_buf());
        h_abs
            .try_add(dir.path().join("foo.txt").to_string_lossy().to_string())
            .unwrap();

        assert_eq!(h_rel.finalize().unwrap(), h_abs.finalize().unwrap());
    }

    #[test]
    fn hashes_two_files_deterministically() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"alpha").unwrap();
        fs::write(dir.path().join("b.txt"), b"beta").unwrap();

        let mut h1 = Hasher::new(dir.path().to_path_buf());
        h1.try_add("a.txt").unwrap().try_add("b.txt").unwrap();
        let d1 = h1.finalize().unwrap();

        // Same files, opposite add order.
        let mut h2 = Hasher::new(dir.path().to_path_buf());
        h2.try_add("b.txt").unwrap().try_add("a.txt").unwrap();
        let d2 = h2.finalize().unwrap();

        assert_eq!(d1, d2, "digests should match regardless of `try_add` order");
    }

    #[test]
    fn detects_path_content_boundary_collision() {
        // Without per-field length prefixes, `{a: "Xbc"}` and `{aXbc: ""}`
        // would both feed the byte stream `aXbc` into the hasher and collide.
        // The path-length and content-length prefixes shift the boundary,
        // making the encoding injective.
        let dir1 = tempdir().unwrap();
        fs::write(dir1.path().join("a"), b"Xbc").unwrap();

        let dir2 = tempdir().unwrap();
        fs::write(dir2.path().join("aXbc"), b"").unwrap();

        let d1 = hash_directory(dir1.path()).unwrap();
        let d2 = hash_directory(dir2.path()).unwrap();
        assert_ne!(d1, d2);
    }

    #[test]
    fn excludes_module_sig_and_lockfile() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"keep").unwrap();
        let d_clean = hash_directory(dir.path()).unwrap();

        fs::write(dir.path().join(crate::SIGNATURE_FILENAME), b"sig").unwrap();
        fs::write(dir.path().join(crate::LOCKFILE_FILENAME), b"lock").unwrap();
        let d_with_excludes = hash_directory(dir.path()).unwrap();

        assert_eq!(d_clean, d_with_excludes);
    }

    #[test]
    fn excludes_sprocket_cache() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"keep").unwrap();
        let d_clean = hash_directory(dir.path()).unwrap();

        let cached_module = dir
            .path()
            .join(".sprocket")
            .join("cache")
            .join("modules")
            .join("github.com")
            .join("example")
            .join("commit")
            .join("nested");
        fs::create_dir_all(&cached_module).unwrap();
        fs::write(cached_module.join(crate::MANIFEST_FILENAME), b"x").unwrap();

        let d_with_cache = hash_directory(dir.path()).unwrap();

        assert_eq!(d_clean, d_with_cache);
    }

    #[test]
    fn hash_directory_rejects_nested_reserved_filename() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(
            dir.path().join("nested").join(crate::MANIFEST_FILENAME),
            b"x",
        )
        .unwrap();
        let err = hash_directory(dir.path()).unwrap_err();
        assert!(matches!(
            err,
            HashError::Tree(crate::tree::TreeError::ReservedFilename {
                name: crate::MANIFEST_FILENAME,
                ..
            })
        ));
    }

    #[test]
    fn finalize_errors_on_missing_file() {
        let dir = tempdir().unwrap();
        let mut h = Hasher::new(dir.path().to_path_buf());
        h.try_add("missing.txt").unwrap();
        assert!(matches!(h.finalize(), Err(HashError::Io { .. })));
    }

    #[test]
    fn finalize_validates_reserved_filenames() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(
            dir.path().join("nested").join(crate::SIGNATURE_FILENAME),
            b"x",
        )
        .unwrap();

        let mut h = Hasher::new(dir.path().to_path_buf());
        h.try_add("nested/module.sig").unwrap();
        let err = h.finalize().unwrap_err();
        assert!(matches!(
            err,
            HashError::Tree(crate::tree::TreeError::ReservedFilename {
                name: crate::SIGNATURE_FILENAME,
                ..
            })
        ));
    }

    #[test]
    fn rejects_paths_colliding_under_nfc() {
        let dir = tempdir().unwrap();
        let mut h = Hasher::new(dir.path().to_path_buf());

        // Both forms of `é` normalize to the same NFC sequence.
        let precomposed = "caf\u{00E9}.wdl";
        let decomposed = "cafe\u{0301}.wdl";

        h.try_add(precomposed).unwrap();
        let err = h.try_add(decomposed).unwrap_err();
        assert!(matches!(err, HashError::AmbiguousPath { .. }));
    }

    #[test]
    fn nfc_normalizes_recorded_paths() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("caf\u{00E9}.wdl"), b"x").unwrap();

        let mut h_nfc = Hasher::new(dir.path().to_path_buf());
        h_nfc.try_add("caf\u{00E9}.wdl").unwrap();

        let mut h_nfd = Hasher::new(dir.path().to_path_buf());
        h_nfd.try_add("cafe\u{0301}.wdl").unwrap();

        assert_eq!(h_nfc.finalize().unwrap(), h_nfd.finalize().unwrap());
    }

    #[test]
    fn hash_stable_despite_dot_git_and_sparse_json() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join(crate::MANIFEST_FILENAME),
            br#"{"name":"x","license":"MIT"}"#,
        )
        .unwrap();
        fs::write(dir.path().join("index.wdl"), b"workflow w {}").unwrap();

        let hash1 = hash_directory(dir.path()).unwrap();

        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(
            dir.path().join(".git").join("HEAD"),
            b"ref: refs/heads/main",
        )
        .unwrap();

        let hash2 = hash_directory(dir.path()).unwrap();

        assert_eq!(hash1, hash2, "`.git` must not affect the content hash");
    }

    fn symlink_file(target: &std::path::Path, link: &std::path::Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(target, link).unwrap();
    }

    #[test]
    fn symlink_to_dot_git_is_rejected() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join(crate::MANIFEST_FILENAME),
            br#"{"name":"x","license":"MIT"}"#,
        )
        .unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git").join("config"), b"[core]").unwrap();
        symlink_file(
            &dir.path().join(".git").join("config"),
            &dir.path().join("sneaky.wdl"),
        );
        let err = hash_directory(dir.path()).unwrap_err();
        assert!(
            matches!(err, HashError::Walk(ModuleWalkError::Symlink(_))),
            "got: {err}"
        );
    }

    #[test]
    fn symlink_within_module_root_is_rejected() {
        // Symbolic links are not permitted anywhere in a module, even
        // when they point at an in-root file.
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join(crate::MANIFEST_FILENAME),
            br#"{"name":"x","license":"MIT"}"#,
        )
        .unwrap();
        fs::write(dir.path().join("real.wdl"), b"workflow w {}").unwrap();
        symlink_file(&dir.path().join("real.wdl"), &dir.path().join("alias.wdl"));
        let err = hash_directory(dir.path()).unwrap_err();
        assert!(
            matches!(err, HashError::Walk(ModuleWalkError::Symlink(_))),
            "got: {err}"
        );
    }

    #[test]
    fn symlink_to_nested_dot_git_is_rejected() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("nested").join(".git")).unwrap();
        fs::write(
            dir.path().join("nested").join(".git").join("config"),
            b"private metadata",
        )
        .unwrap();
        symlink_file(
            &dir.path().join("nested").join(".git").join("config"),
            &dir.path().join("index.wdl"),
        );
        let err = hash_directory(dir.path()).unwrap_err();
        assert!(
            matches!(err, HashError::Walk(ModuleWalkError::Symlink(_))),
            "expected symlink rejection, got: {err}"
        );
    }

    #[test]
    fn windows_and_unix_paths_hash_identically() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("root.wdl"), b"workflow w {}").unwrap();
        fs::write(dir.path().join("sub").join("nested.wdl"), b"task t {}").unwrap();

        // Simulate Unix-style paths (as `hash_directory` would produce on Unix).
        let mut h_unix = Hasher::new(dir.path().to_path_buf());
        for p in ["root.wdl", "sub/nested.wdl"] {
            h_unix.try_add(p).unwrap();
        }

        // Simulate Windows-style paths after the `\` → `/` normalization that
        // `hash_directory` applies before calling `try_add`.
        let mut h_win = Hasher::new(dir.path().to_path_buf());
        for p in ["root.wdl", "sub\\nested.wdl"] {
            h_win.try_add(p.replace('\\', "/")).unwrap();
        }

        assert_eq!(
            h_unix.finalize().unwrap(),
            h_win.finalize().unwrap(),
            "digests must be platform-independent after path-separator normalization"
        );
    }

    #[test]
    fn directory_symlink_cycle_is_rejected() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("real.wdl"), b"version 1.3\n").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("..", dir.path().join("sub").join("loop")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir("..", dir.path().join("sub").join("loop")).unwrap();
        let err = hash_directory(dir.path()).unwrap_err();
        assert!(
            matches!(err, HashError::Walk(ModuleWalkError::Symlink(_))),
            "directory symlinks must be rejected, got: {err}"
        );
    }
}
