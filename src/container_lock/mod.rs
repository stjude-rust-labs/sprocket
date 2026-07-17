use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Result;
use chrono::SecondsFormat;
use chrono::Utc;
use tempfile::NamedTempFile;
use toml_spanner::Toml;
use wdl::engine::ContainerLock;

use crate::analysis::Source;

mod extract;

pub use extract::ContainerUse;
pub use extract::ExtractionMode;
pub use extract::extract;

/// The container lock file name.
pub const LOCK_FILE_NAME: &str = "sprocket.lock";

/// The current version for serialized container locks.
const LOCK_VERSION: u32 = 1;

/// The on-disk representation of a Sprocket container lock.
#[derive(Debug, Toml)]
#[toml(Toml, ToToml, deny_unknown_fields)]
struct LockFile {
    /// The lock file format version.
    #[toml(default)]
    version: Option<u32>,
    /// The generation timestamp string.
    generation_time: String,
    /// Mutable image references mapped to immutable digests.
    #[toml(default)]
    images: BTreeMap<String, String>,
    /// SIF file paths mapped to immutable digests.
    #[toml(default)]
    sif_files: BTreeMap<String, String>,
}

/// A loaded lock file with its parsed policy and exact source bytes.
#[derive(Debug)]
pub struct LoadedLock {
    /// The path to the loaded lock file.
    path: PathBuf,
    /// The exact bytes read from disk.
    snapshot: Vec<u8>,
    /// The validated immutable lock policy.
    policy: Arc<ContainerLock>,
}

impl LoadedLock {
    /// Gets the path to the loaded lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Gets the exact bytes that were parsed into the lock policy.
    pub fn snapshot(&self) -> &[u8] {
        &self.snapshot
    }

    /// Gets the validated immutable lock policy.
    pub fn policy(&self) -> &Arc<ContainerLock> {
        &self.policy
    }
}

/// Discovers the nearest container lock for the given source.
pub fn discover(source: &Source) -> Result<Option<PathBuf>> {
    for directory in discovery_start(source)?.ancestors() {
        let candidate = directory.join(LOCK_FILE_NAME);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }

        if directory.join(".git").exists() {
            break;
        }
    }

    Ok(None)
}

/// Loads, validates, and snapshots a container lock file.
pub fn load(path: &Path) -> Result<LoadedLock> {
    let snapshot =
        std::fs::read(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let text = std::str::from_utf8(&snapshot)
        .with_context(|| format!("lock file `{}` is not utf-8", path.display()))?;
    let file: LockFile = toml_spanner::from_str(text)
        .with_context(|| format!("failed to parse `{}`", path.display()))?;

    match file.version {
        None => {}
        Some(LOCK_VERSION) => {
            chrono::DateTime::parse_from_rfc3339(&file.generation_time).with_context(|| {
                format!(
                    "version {LOCK_VERSION} lock `{}` has an invalid generation time",
                    path.display()
                )
            })?;
        }
        Some(version) => {
            anyhow::bail!(
                "unsupported container lock version {version} in `{}`",
                path.display()
            );
        }
    }

    let policy = Arc::new(
        ContainerLock::try_new(path, file.images, file.sif_files)
            .with_context(|| format!("invalid container lock `{}`", path.display()))?,
    );

    Ok(LoadedLock {
        path: path.to_path_buf(),
        snapshot,
        policy,
    })
}

/// Serializes the version 1 lock file format.
pub fn serialize_version_one(
    images: BTreeMap<String, String>,
    sif_files: BTreeMap<String, String>,
) -> Result<Vec<u8>> {
    let file = LockFile {
        version: Some(LOCK_VERSION),
        generation_time: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        images,
        sif_files,
    };

    Ok(toml_spanner::to_string(&file)
        .context("failed to serialize container lock")?
        .into_bytes())
}

/// Atomically replaces a lock file from a same-directory temporary file.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let destination = path;
    let parent = destination.parent().with_context(|| {
        format!(
            "failed to determine the parent directory for `{}`",
            destination.display()
        )
    })?;
    let mut temporary = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to create a temporary container lock for `{}`",
            destination.display()
        )
    })?;
    temporary.write_all(contents).with_context(|| {
        format!(
            "failed to write the temporary container lock for `{}`",
            destination.display()
        )
    })?;
    temporary.flush().with_context(|| {
        format!(
            "failed to flush the temporary container lock for `{}`",
            destination.display()
        )
    })?;
    temporary.as_file().sync_all().with_context(|| {
        format!(
            "failed to sync the temporary container lock for `{}`",
            destination.display()
        )
    })?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace `{}`", destination.display()))?;

    Ok(())
}

/// Determines the directory where lock discovery should begin.
fn discovery_start(source: &Source) -> Result<PathBuf> {
    match source {
        Source::Directory(path) => Ok(path.clone()),
        Source::File(url) => url
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("failed to convert source url `{url}` to a path"))?
            .parent()
            .context("source file has no parent directory")
            .map(Path::to_path_buf),
        Source::Url(_) => std::env::current_dir().context("failed to get current directory"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use anyhow::Result;
    use wdl::engine::ContainerLock;

    use super::LOCK_FILE_NAME;
    use super::discover;
    use super::discovery_start;
    use super::load;
    use super::serialize_version_one;
    use super::write_atomic;
    use crate::analysis::Source;

    #[test]
    fn loads_legacy_lock_and_retains_exact_snapshot() -> Result<()> {
        let root = tempfile::tempdir()?;
        let digest = format!("sha256:{}", "a".repeat(64));
        let path = root.path().join(LOCK_FILE_NAME);
        let contents = format!(
            "generation_time = \"2025-06-25 13:00:00 UTC\"\n[images]\n\"ubuntu:24.04\" = \
             \"ubuntu@{digest}\"\n"
        );
        std::fs::write(&path, &contents)?;

        let loaded = load(&path)?;

        let _: &Arc<ContainerLock> = loaded.policy();
        let _: &[u8] = loaded.snapshot();
        assert_eq!(loaded.path(), path.as_path());
        assert_eq!(loaded.snapshot(), contents.as_bytes());
        Ok(())
    }

    #[test]
    fn rejects_legacy_lock_with_duplicate_canonical_images() -> Result<()> {
        let root = tempfile::tempdir()?;
        let digest = format!("sha256:{}", "a".repeat(64));
        let path = root.path().join(LOCK_FILE_NAME);
        std::fs::write(
            &path,
            format!(
                "generation_time = \"2025-06-25 13:00:00 UTC\"\n[images]\n\"ubuntu:24.04\" = \
                 \"ubuntu@{digest}\"\n\"docker://docker.io/library/ubuntu:24.04\" = \
                 \"docker://docker.io/library/ubuntu@{digest}\"\n"
            ),
        )?;

        let error = load(&path).expect_err("canonical duplicate image keys should be rejected");
        assert!(format!("{error:#}").contains("duplicate canonical image key"));
        Ok(())
    }

    #[test]
    fn rejects_unknown_version() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join(LOCK_FILE_NAME);
        std::fs::write(
            &path,
            "version = 2\ngeneration_time = \"2026-07-17T20:00:00Z\"\n",
        )?;

        let error = load(&path).expect_err("version 2 should be rejected");
        assert!(error.to_string().contains("version 2"));
        Ok(())
    }

    #[test]
    fn rejects_invalid_version_one_generation_time() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join(LOCK_FILE_NAME);
        std::fs::write(
            &path,
            "version = 1\ngeneration_time = \"2025-06-25 13:00:00 UTC\"\n",
        )?;

        let error = load(&path).expect_err("version 1 should require rfc 3339 timestamps");
        assert!(error.to_string().contains("invalid generation time"));
        Ok(())
    }

    #[test]
    fn serializes_sorted_version_one_toml() -> Result<()> {
        let data = serialize_version_one(
            BTreeMap::from([
                (
                    "docker://z.example/a:v1".into(),
                    format!("docker://z.example/a@sha256:{}", "b".repeat(64)),
                ),
                (
                    "docker://a.example/a:v1".into(),
                    format!("docker://a.example/a@sha256:{}", "a".repeat(64)),
                ),
            ]),
            BTreeMap::new(),
        )?;
        let text = String::from_utf8(data)?;
        let a_index = text
            .find("a.example")
            .expect("serialized lock should contain a.example");
        let z_index = text
            .find("z.example")
            .expect("serialized lock should contain z.example");

        assert!(text.starts_with("version = 1\n"));
        assert!(a_index < z_index);
        Ok(())
    }

    #[test]
    fn discovers_nearest_lock_without_crossing_git_root() -> Result<()> {
        let outer = tempfile::tempdir()?;
        std::fs::write(outer.path().join(LOCK_FILE_NAME), b"outer")?;

        let repo = outer.path().join("repo");
        std::fs::create_dir_all(repo.join(".git"))?;
        std::fs::write(repo.join(LOCK_FILE_NAME), b"repo")?;
        let nested = repo.join("a/b");
        std::fs::create_dir_all(&nested)?;
        std::fs::write(nested.join(LOCK_FILE_NAME), b"nested")?;

        let source = Source::Directory(nested.clone());

        assert_eq!(discover(&source)?, Some(nested.join(LOCK_FILE_NAME)));
        Ok(())
    }

    #[test]
    fn discovers_lock_from_file_source_parent_directory() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source_dir = root.path().join("wdl");
        std::fs::create_dir_all(&source_dir)?;
        std::fs::create_dir(root.path().join(".git"))?;
        std::fs::write(source_dir.join(LOCK_FILE_NAME), b"lock")?;
        let source_path = source_dir.join("main.wdl");
        std::fs::write(&source_path, "version 1.0")?;
        let source = Source::File(
            url::Url::from_file_path(&source_path)
                .map_err(|_| anyhow::anyhow!("failed to build a file url for the test source"))?,
        );

        assert_eq!(discover(&source)?, Some(source_dir.join(LOCK_FILE_NAME)));
        Ok(())
    }

    #[test]
    fn starts_remote_discovery_from_current_directory() -> Result<()> {
        let source = Source::Url("https://example.com/workflow.wdl".parse()?);

        assert_eq!(discovery_start(&source)?, std::env::current_dir()?);
        Ok(())
    }

    #[test]
    fn atomic_write_replaces_existing_file() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join(LOCK_FILE_NAME);
        std::fs::write(&path, b"old")?;

        write_atomic(&path, b"new")?;

        assert_eq!(std::fs::read(&path)?, b"new");
        Ok(())
    }

    #[test]
    fn atomic_write_errors_include_destination_path() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("missing").join(LOCK_FILE_NAME);

        let error = write_atomic(&path, b"new").expect_err("missing parent should fail");

        assert!(error.to_string().contains(&path.display().to_string()));
        Ok(())
    }
}
