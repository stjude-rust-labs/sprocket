//! Implementation of the call and digest caches.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::io::BufReader;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use arrayvec::ArrayString;
use serde::Deserialize;
use serde::Serialize;
use tokio::fs;
use tracing::debug;
use tracing::info;
use url::Url;

use crate::ContentKind;
use crate::EvaluationPath;
use crate::Object;
use crate::PrimitiveValue;
use crate::Value;
use crate::backend::Input;
use crate::backend::TaskExecutionResult;
use crate::cache::hash::hash_sequence;
use crate::cache::lock::LockedFile;
use crate::config::ContentDigestMode;
use crate::http::Transferer;
use crate::v1::requirements::ContainerSource;

/// The current cache entry version.
///
/// This is a monotonic value that should increase whenever the serialization of
/// call cache entries change.
///
/// Bumping the version causes a change to cache key derivation.
const CURRENT_CACHE_VERSION: u32 = 0;

/// The default cache subdirectory for the call cache.
const CALL_CACHE_SUBDIR: &str = "calls";

/// The name of the global cache lock file.
const CACHE_LOCK_FILE_NAME: &str = ".lock";

mod hash;
mod lock;

pub use hash::Hashable;

/// Hashes the evaluated command of a cache key request.
///
/// References to inputs and replaced with the content digest of the input.
///
/// If the input is not a temporary file, the input's file name is also hashed.
fn hash_command(request: &KeyRequest<'_>, input_digests: &[ArrayString<64>]) -> ArrayString<64> {
    let mut hasher = blake3::Hasher::new();
    match request.guest_inputs_dir {
        Some(prefix) => {
            // Find the next prefix in the command string
            let mut current = request.command;
            while let Some(start) = current.find(prefix) {
                // Hash what came before the match
                if start > 0 {
                    (&current[..start]).hash(&mut hasher);
                }

                // Find the longest match from the backend input guest paths
                let end = match request
                    .backend_inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(index, input)| {
                        let p = input.guest_path()?.as_str();
                        current[start..].starts_with(p).then_some((index, p.len()))
                    })
                    .max_by_key(|(_, len)| *len)
                {
                    Some((index, len)) => {
                        let end = start + len;

                        // If the backend input isn't excluded, hash the input kind, content digest,
                        // and file name (non-temporary only)
                        let input = &request.backend_inputs[index];
                        if !input.excluded_from_call_cache() {
                            input.kind().hash(&mut hasher);

                            // Count the number of preceding excluded inputs to offset by
                            let offset = (0..index)
                                .filter(|i| request.backend_inputs[*i].excluded_from_call_cache())
                                .count();
                            input_digests[index - offset].as_bytes().hash(&mut hasher);
                            match input.kind() {
                                ContentKind::File | ContentKind::Directory => {
                                    // Hash the file name
                                    // SAFETY: guest paths are always Unix style and have a slash
                                    let slash = start + current[start..end].rfind('/').unwrap();
                                    (&current[slash + 1..end]).hash(&mut hasher);
                                }
                                ContentKind::TempFile => {
                                    // Ignore the file name
                                }
                            }
                        }

                        end
                    }
                    None => {
                        // Otherwise, no match, so hash just the prefix
                        prefix.hash(&mut hasher);
                        start + prefix.len()
                    }
                };

                current = &current[end..];
            }

            // Hash the remainder of the command
            if !current.is_empty() {
                current.hash(&mut hasher);
            }
        }
        None => {
            request.command.hash(&mut hasher);
        }
    }

    hasher.finalize().to_hex()
}

/// Contains keys that are excluded from cache entry checking.
///
/// This is used to determine which keys to ignore when checking if a cache
/// entry is valid. The `inputs` field is used when computing the cache key for
/// a task, while the `requirements` and `hints` fields are used when checking
/// if a cache entry is valid.
#[derive(Default)]
pub struct CallCacheExclusions {
    /// The list of cache input keys to exclude when computing keys and checking
    /// cache entries.
    pub inputs: HashSet<String>,
    /// The list of cache requirement keys to exclude when checking cache
    /// entries.
    pub requirements: HashSet<String>,
    /// The list of cache hint keys to exclude when checking cache entries.
    pub hints: HashSet<String>,
}

/// Represents the internal state of the call cache.
#[derive(Clone)]
struct State {
    /// The global cache file lock.
    ///
    /// Task and workflow evaluation typically acquires a single shared lock on
    /// the call cache per run.
    ///
    /// Operations to clean the cache will acquire an exclusive lock to ensure
    /// the cache is cleaned only when no evaluations are taking place.
    // This is kept alive as long as a reference to the cache exists; it is not used by the cache
    // itself.
    _lock: Arc<LockedFile>,
    /// The path to the root call cache directory.
    cache_dir: Arc<PathBuf>,
    /// The file transferer that can be used for calculating remote file
    /// digests.
    transferer: Arc<dyn Transferer>,
    /// The content digest mode used by the cache.
    mode: ContentDigestMode,
    /// The keys to exclude when checking cache entries for validity.
    exclusions: Arc<CallCacheExclusions>,
}

impl State {
    /// Gets the path to an entry in the cache given the [`Key`].
    fn entry_path(&self, key: &Key) -> PathBuf {
        self.cache_dir.join(key.as_str())
    }
}

/// Represents information about content within a call cache entry.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Content {
    /// The location of the content.
    ///
    /// May be a local path or a remote URL.
    location: String,
    /// The digest of the content, as a hex string.
    digest: ArrayString<64>,
}

impl Content {
    /// Constructs a new [`Content`] from the given evaluation path.
    ///
    /// The content digest of the path will be calculated.
    async fn from_evaluation_path(
        transferer: &dyn Transferer,
        path: EvaluationPath,
        kind: ContentKind,
        mode: ContentDigestMode,
    ) -> Result<Self> {
        let digest = path.calculate_digest(transferer, kind, mode).await?;
        Ok(Self {
            location: path.try_into()?,
            digest: digest.to_hex(),
        })
    }

    /// Converts the [`Content`] to an evaluation path.
    ///
    /// Returns an error if the current (as it was first calculated and cached
    /// during evaluation) digest of the location does not match the stored
    /// digest.
    async fn to_evaluation_path(
        &self,
        transferer: &dyn Transferer,
        kind: ContentKind,
        mode: ContentDigestMode,
    ) -> Result<EvaluationPath> {
        let path: EvaluationPath = self.location.parse()?;
        let digest = path.calculate_digest(transferer, kind, mode).await?;
        if digest.to_hex() != self.digest {
            bail!(
                "cached content `{location}` was modified",
                location = self.location
            );
        }

        Ok(path)
    }
}

/// Represents the serialization of a call cache entry.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CallCacheEntry {
    /// The digest of the command's evaluated task.
    command: ArrayString<64>,
    /// The container image that was actually used during execution.
    ///
    /// `None` for tasks that ran directly on the host.
    container: Option<ContainerSource>,
    /// The configured default container at the time the entry was written.
    ///
    /// Only populated (and only compared) when the task declares no `container`
    /// requirement; otherwise the requirement digest covers it.
    default_container: Option<String>,
    /// The shell used by the task.
    shell: String,
    /// The requirement digests of the task.
    requirements: HashMap<String, ArrayString<64>>,
    /// The hint digests of the task.
    hints: HashMap<String, ArrayString<64>>,
    /// The sorted digests of the backend inputs of the task.
    inputs: Vec<ArrayString<64>>,
    /// The task's last exit code.
    exit: i32,
    /// The task's last stdout content.
    stdout: Content,
    /// The task's last stderr content.
    stderr: Content,
    /// The task's last work directory content.
    work: Content,
}

/// Represents a key for a [`CallCache`].
///
/// This type additionally stores digests used to validate cache entries during
/// a call to [`Cache::get`].
///
/// The digests are calculated once prior to accessing the cache and reused for
/// putting an entry into the cache.
///
/// If the digests match, the entry is considered valid and returned.
///
/// If the digests do not match, the entry is considered invalid and these
/// digests are used to overwrite the existing cache entry.
#[derive(Debug)]
pub struct Key {
    /// The cache key for the task.
    key: ArrayString<64>,
    /// The digest of the command's evaluated task.
    command: ArrayString<64>,
    /// The configured default container for the task.
    ///
    /// Only populated (and only compared) when the task declares no `container`
    /// requirement; otherwise the requirement digest covers it.
    default_container: Option<String>,
    /// The shell used by the task.
    shell: String,
    /// The requirement digests of the task.
    requirements: HashMap<String, ArrayString<64>>,
    /// The hint digests of the task.
    hints: HashMap<String, ArrayString<64>>,
    /// The sorted content digests of the backend inputs to the task.
    inputs: Vec<ArrayString<64>>,
}

impl Key {
    /// Gets the string representation of the key.
    pub fn as_str(&self) -> &str {
        self.key.as_str()
    }

    /// Ensure this [`Key`] matches the given [`CallCacheEntry`].
    ///
    /// Returns an error if there is a mismatch.
    fn ensure_matches(
        &self,
        entry: &CallCacheEntry,
        exclusions: &CallCacheExclusions,
    ) -> Result<()> {
        fn compare_maps<K, V>(
            a: &HashMap<K, V>,
            b: &HashMap<K, V>,
            kind: &str,
            excluded: &HashSet<String>,
        ) -> Result<()>
        where
            K: std::hash::Hash + fmt::Display + Eq,
            V: Eq,
        {
            for (k, v) in a {
                // Skip excluded keys
                let key_str = k.to_string();
                if excluded.contains(&key_str) {
                    debug!("{} `{}` is excluded from cache checking, skipping", kind, k);
                    continue;
                }

                match b.get(k) {
                    Some(ov) => {
                        if v != ov {
                            bail!("{kind} `{k}` was modified")
                        }
                    }
                    None => bail!("{kind} `{k}` was added"),
                }
            }

            for k in b.keys() {
                // Skip excluded keys
                let key_str = k.to_string();
                if excluded.contains(&key_str) {
                    debug!("{} `{}` is excluded from cache checking, skipping", kind, k);
                    continue;
                }

                if !a.contains_key(k) {
                    bail!("{kind} `{k}` was removed");
                }
            }

            Ok(())
        }

        if self.inputs.len() < entry.inputs.len() {
            bail!("a file or directory input was removed since last evaluation");
        }

        if self.inputs.len() > entry.inputs.len() {
            bail!("a file or directory input was added since last evaluation");
        }

        if self.inputs != entry.inputs {
            bail!("the content of a file or directory input was modified");
        }

        if self.command != entry.command {
            bail!("the command of the task was modified");
        }

        if self.default_container != entry.default_container {
            bail!("the default container for the task was modified");
        }

        if self.shell != entry.shell {
            bail!("the shell used by the task was modified");
        }

        compare_maps(
            &self.requirements,
            &entry.requirements,
            "task requirement",
            &exclusions.requirements,
        )?;
        compare_maps(&self.hints, &entry.hints, "task hint", &exclusions.hints)?;

        Ok(())
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.key.fmt(f)
    }
}

/// Represents a request to calculate a [`Key`].
#[derive(Debug, Copy, Clone)]
pub struct KeyRequest<'a> {
    /// The URI of the document containing the task.
    ///
    /// This field directly contributes to the cache key.
    pub document_uri: &'a Url,
    /// The name of the backend that is executing the task.
    ///
    /// This field directly contributes to the cache key.
    pub backend: &'a str,
    /// The name of the task.
    ///
    /// This field directly contributes to the cache key.
    pub task_name: &'a str,
    /// The map of evaluated input values for the task.
    ///
    /// This field directly contributes to the cache key.
    pub inputs: &'a BTreeMap<String, Value>,
    /// The evaluated command of the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub command: &'a str,
    /// The configured default container for the task.
    ///
    /// This field is only meaningful when the task has no `container` (or
    /// `docker`) requirement; in that case, a change to the default container
    /// invalidates cached entries. When the task declares a `container`
    /// requirement, the requirement digest already captures the container, so
    /// this field should be `None`.
    pub default_container: Option<&'a str>,
    /// The shell used by the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub shell: &'a str,
    /// The evaluated requirements of the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub requirements: &'a Object,
    /// The evaluated hints of the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub hints: &'a Object,
    /// The backend's guest input directory.
    ///
    /// This field is used to detect guest input paths in the evaluated command.
    pub guest_inputs_dir: Option<&'a str>,
    /// The backend inputs of the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub backend_inputs: &'a [Input],
}

/// Represents an evaluation call cache.
///
/// A call cache can be used to cache the result of task executions so previous
/// results can be reused and a task's execution skipped.
///
/// A [`CallCache`] can be cheaply cloned.
#[derive(Clone)]
pub struct CallCache(State);

impl CallCache {
    /// Creates a new call cache for the given cache directory and file
    /// transferer to use.
    ///
    /// If `cache_dir` is `None`, the default operating system specified cache
    /// directory for the user is used.
    pub async fn new(
        cache_dir: Option<&Path>,
        mode: ContentDigestMode,
        transferer: Arc<dyn Transferer>,
        exclusions: Arc<CallCacheExclusions>,
    ) -> Result<Self> {
        let cache_dir = match cache_dir {
            Some(cache_dir) => cache_dir.into(),
            None => crate::config::cache_dir()?.join(CALL_CACHE_SUBDIR),
        };

        info!(
            "using call cache directory `{cache_dir}`",
            cache_dir = cache_dir.display()
        );

        fs::create_dir_all(&cache_dir).await.with_context(|| {
            format!(
                "failed to create call cache directory `{dir}`",
                dir = cache_dir.display()
            )
        })?;

        Ok(Self(State {
            _lock: LockedFile::acquire_shared(&cache_dir.join(CACHE_LOCK_FILE_NAME), true)
                .await?
                .expect("file should exist")
                .into(),
            cache_dir: cache_dir.into(),
            transferer,
            mode,
            exclusions,
        }))
    }

    /// Calculates a new [`Key`] to use for the cache.
    ///
    /// This will calculate digests for the command, requirements, hints, and
    /// inputs.
    pub async fn key(&self, request: &KeyRequest<'_>) -> Result<Key> {
        // Calculate the requirement digests
        let requirement_digests = request
            .requirements
            .iter()
            .map(|(k, v)| {
                let mut hasher = blake3::Hasher::new();
                v.hash(&mut hasher);
                (k.to_string(), hasher.finalize().to_hex())
            })
            .collect();

        // Calculate the hint digests
        let hint_digests = request
            .hints
            .iter()
            .map(|(k, v)| {
                let mut hasher = blake3::Hasher::new();
                v.hash(&mut hasher);
                (k.to_string(), hasher.finalize().to_hex())
            })
            .collect();

        // Calculate the digests of the backend inputs
        let mut inputs = Vec::with_capacity(request.backend_inputs.len());
        for input in request
            .backend_inputs
            .iter()
            .filter(|i| !i.excluded_from_call_cache())
        {
            let digest = input
                .path()
                .calculate_digest(self.0.transferer.as_ref(), input.kind(), self.0.mode)
                .await?;

            inputs.push(digest.to_hex());
        }

        // Calculate the command digest
        let command_digest = hash_command(request, &inputs);

        // Sort the input digests so that they can be easily compared with an entry
        inputs.sort();

        // Calculate the task's cache key
        let mut hasher = blake3::Hasher::new();
        CURRENT_CACHE_VERSION
            .to_le_bytes()
            .as_ref()
            .hash(&mut hasher);
        request.document_uri.hash(&mut hasher);
        request.backend.hash(&mut hasher);
        request.task_name.hash(&mut hasher);
        hash_sequence(
            &mut hasher,
            request
                .inputs
                .iter()
                .filter(|(k, _)| !self.0.exclusions.inputs.contains(*k))
                .collect::<Vec<_>>()
                .into_iter(),
        );
        let key = hasher.finalize().to_hex();

        Ok(Key {
            key,
            command: command_digest,
            default_container: request.default_container.map(Into::into),
            shell: request.shell.into(),
            requirements: requirement_digests,
            hints: hint_digests,
            inputs,
        })
    }

    /// Gets an entry from the [`CallCache`] given the cache key and information
    /// about the current task.
    ///
    /// Returns `Ok(None)` if a cache entry with the given key does not exist.
    ///
    /// Returns an error if the entry could not be read or if the entry is no
    /// longer valid.
    pub async fn get(&self, key: &Key) -> Result<Option<TaskExecutionResult>> {
        // Take a shared lock on the entry file
        let path = self.0.entry_path(key);
        let file = match LockedFile::acquire_shared(&path, false).await? {
            Some(file) => file,
            None => return Ok(None),
        };

        // Deserialize the entry and ensure it matches the current evaluation
        let entry: CallCacheEntry = serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("failed to deserialize call cache entry `{key}`"))?;

        // Ensure the key matches the cache entry
        key.ensure_matches(&entry, &self.0.exclusions)?;

        let stdout = entry
            .stdout
            .to_evaluation_path(self.0.transferer.as_ref(), ContentKind::File, self.0.mode)
            .await?;
        let stderr = entry
            .stderr
            .to_evaluation_path(self.0.transferer.as_ref(), ContentKind::File, self.0.mode)
            .await?;
        let work = entry
            .work
            .to_evaluation_path(
                self.0.transferer.as_ref(),
                ContentKind::Directory,
                self.0.mode,
            )
            .await?;

        Ok(Some(TaskExecutionResult {
            container: entry.container,
            exit_code: entry.exit,
            work_dir: work,
            stdout: PrimitiveValue::new_file(String::try_from(stdout)?).into(),
            stderr: PrimitiveValue::new_file(String::try_from(stderr)?).into(),
        }))
    }

    /// Puts an entry into the call cache.
    ///
    /// Upon a successful update of the key, returns the key as an
    /// [`ArrayString`].
    pub async fn put(&self, key: Key, result: &TaskExecutionResult) -> Result<ArrayString<64>> {
        let file = LockedFile::acquire_exclusive_truncated(&self.0.entry_path(&key)).await?;

        let entry = CallCacheEntry {
            command: key.command,
            container: result.container.clone(),
            default_container: key.default_container,
            shell: key.shell,
            requirements: key.requirements,
            hints: key.hints,
            inputs: key.inputs,
            exit: result.exit_code,
            stdout: Content::from_evaluation_path(
                self.0.transferer.as_ref(),
                result
                    .stdout
                    .as_file()
                    .expect("value should be a `File`")
                    .as_str()
                    .parse()?,
                ContentKind::File,
                self.0.mode,
            )
            .await?,
            stderr: Content::from_evaluation_path(
                self.0.transferer.as_ref(),
                result
                    .stderr
                    .as_file()
                    .expect("value should be a `File`")
                    .as_str()
                    .parse()?,
                ContentKind::File,
                self.0.mode,
            )
            .await?,
            work: Content::from_evaluation_path(
                self.0.transferer.as_ref(),
                result.work_dir.clone(),
                ContentKind::Directory,
                self.0.mode,
            )
            .await?,
        };

        serde_json::to_writer(BufWriter::new(file), &entry).with_context(|| {
            format!(
                "failed to serialize call cache entry `{key}`",
                key = key.key
            )
        })?;
        Ok(key.key)
    }

    /// Determines if an input is excluded from the cache.
    pub(crate) fn is_input_excluded(&self, input: &str) -> bool {
        self.0.exclusions.inputs.contains(input)
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;
    use tempfile::tempdir;

    use super::*;
    use crate::GuestPath;
    use crate::digest::test::DigestTransferer;
    use crate::digest::test::clear_digest_cache;

    /// Stores a call cache for testing
    struct TestCache {
        /// The root directory for the test.
        root_dir: TempDir,
        /// The path to a dummy stdout file.
        stdout: PathBuf,
        /// The path to a dummy stderr file.
        stderr: PathBuf,
        /// The path to a dummy working directory.
        work_dir: PathBuf,
        /// The call cache used by the test.
        inner: CallCache,
    }

    impl TestCache {
        /// Constructs a new test cache.
        async fn new() -> Self {
            Self::new_with_exclusions(Default::default()).await
        }

        /// Constructs a new test cache with the given exclusions
        async fn new_with_exclusions(exclusions: CallCacheExclusions) -> Self {
            // Create a root directory for the test
            let root_dir = tempdir().expect("failed to create temporary directory");

            // Create the inner cache
            let inner = CallCache::new(
                Some(&root_dir.path().join(".cache")),
                ContentDigestMode::Strong,
                Arc::new(DigestTransferer::new([])),
                Arc::new(exclusions),
            )
            .await
            .unwrap();

            let stdout = root_dir.path().join("stdout");
            let stderr = root_dir.path().join("stderr");
            let work_dir = root_dir.path().join("work");

            Self {
                root_dir,
                stdout,
                stderr,
                work_dir,
                inner,
            }
        }

        /// Creates a dummy task execution result.
        ///
        /// This will also create dummy files for the task execution result to
        /// reference.
        async fn create_execution_result(&self) -> TaskExecutionResult {
            // Write a stdout file
            if let Some(parent) = self.stdout.parent() {
                fs::create_dir_all(parent).await.unwrap();
            }

            fs::write(&self.stdout, "stdout").await.unwrap();

            // Write a stderr file
            if let Some(parent) = self.stdout.parent() {
                fs::create_dir_all(parent).await.unwrap();
            }

            fs::write(&self.stderr, "stderr").await.unwrap();

            // Create a work directory
            fs::create_dir(&self.work_dir).await.unwrap();

            TaskExecutionResult {
                container: Some("ubuntu:latest".parse().unwrap()),
                exit_code: 0,
                work_dir: EvaluationPath::from_local_path(self.work_dir.clone()),
                stdout: PrimitiveValue::new_file(self.stdout.to_str().unwrap()).into(),
                stderr: PrimitiveValue::new_file(self.stderr.to_str().unwrap()).into(),
            }
        }

        /// Populates a dummy execution result into the cache for the given key
        /// request.
        async fn populate(&self, request: &KeyRequest<'_>) {
            // Get a key for the cache (should not exist)
            let key = self.inner.key(request).await.unwrap();
            assert!(self.inner.get(&key).await.unwrap().is_none());

            // Cache a dummy execution result
            self.inner
                .put(key, &self.create_execution_result().await)
                .await
                .unwrap();

            // Get the entry we just put and ensure it is returned
            self.inner.key(request).await.unwrap();
        }
    }

    #[tokio::test]
    async fn modified_command() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Check for modified command
        let key = cache
            .inner
            .key(&KeyRequest {
                command: "modified!",
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "the command of the task was modified"
        );
    }

    #[tokio::test]
    async fn modified_guest_path() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        // Create an input file
        let input_file_path = cache.root_dir.path().join("input");
        fs::write(&input_file_path, "hello world!").await.unwrap();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "cat /mnt/task/inputs/0/input",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[Input::new(
                ContentKind::File,
                EvaluationPath::from_local_path(input_file_path.clone()),
                Some(GuestPath::new("/mnt/task/inputs/0/input")),
            )],
        };

        cache.populate(&request).await;

        // Change the input's guest path, but keep the file name the same
        // The entry should be valid as the input's contents and file name remained the
        // same
        let key = cache
            .inner
            .key(&KeyRequest {
                command: "cat /mnt/task/inputs/100/input",
                backend_inputs: &[Input::new(
                    ContentKind::File,
                    EvaluationPath::from_local_path(input_file_path.clone()),
                    Some(GuestPath::new("/mnt/task/inputs/100/input")),
                )],
                ..request
            })
            .await
            .unwrap();
        cache.inner.get(&key).await.unwrap().unwrap();

        // Change the input's file name
        let key = cache
            .inner
            .key(&KeyRequest {
                command: "cat /mnt/task/inputs/0/foo",
                backend_inputs: &[Input::new(
                    ContentKind::File,
                    EvaluationPath::from_local_path(input_file_path),
                    Some(GuestPath::new("/mnt/task/inputs/0/foo")),
                )],
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "the command of the task was modified"
        );
    }

    #[tokio::test]
    async fn modified_temp_file_name() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        // Create an input file
        let input_file_path = cache.root_dir.path().join("input");
        fs::write(&input_file_path, "hello world!").await.unwrap();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "cat /mnt/task/inputs/0/input",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[Input::new(
                ContentKind::TempFile,
                EvaluationPath::from_local_path(input_file_path.clone()),
                Some(GuestPath::new("/mnt/task/inputs/0/input")),
            )],
        };

        cache.populate(&request).await;

        // Change the input's file name doesn't invalidate the entry because the file
        // contents remained the same and file names are ignored for temporary files
        let key = cache
            .inner
            .key(&KeyRequest {
                command: "cat /mnt/task/inputs/0/foo",
                backend_inputs: &[Input::new(
                    ContentKind::TempFile,
                    EvaluationPath::from_local_path(input_file_path.clone()),
                    Some(GuestPath::new("/mnt/task/inputs/0/foo")),
                )],
                ..request
            })
            .await
            .unwrap();
        cache.inner.get(&key).await.unwrap().unwrap();

        // Changing the temp file's contents invalidates the entry
        fs::write(&input_file_path, "changed!").await.unwrap();
        clear_digest_cache();

        assert_eq!(
            cache
                .inner
                .get(&cache.inner.key(&request).await.unwrap())
                .await
                .unwrap_err()
                .to_string(),
            "the content of a file or directory input was modified"
        );
    }

    #[tokio::test]
    async fn modified_default_container() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: Some("ubuntu:latest"),
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                default_container: Some("ubuntu:cthulhu"),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "the default container for the task was modified"
        );
    }

    #[tokio::test]
    async fn modified_shell() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: Some("ubuntu:latest"),
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                shell: "zsh",
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "the shell used by the task was modified"
        );
    }

    #[tokio::test]
    async fn requirement_removed() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Object::new(IndexMap::from_iter([(
            "container".into(),
            PrimitiveValue::new_string("ubuntu:latest").into(),
        )]));
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                requirements: &Object::default(),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "task requirement `container` was removed"
        );
    }

    #[tokio::test]
    async fn requirement_added() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                requirements: &Object::new(IndexMap::from_iter([(
                    "container".into(),
                    PrimitiveValue::new_string("ubuntu:latest").into(),
                )])),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "task requirement `container` was added"
        );
    }

    #[tokio::test]
    async fn requirement_modified() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Object::new(IndexMap::from_iter([(
            "container".into(),
            PrimitiveValue::new_string("ubuntu:latest").into(),
        )]));
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                requirements: &Object::new(IndexMap::from_iter([(
                    "container".into(),
                    PrimitiveValue::new_string("ubuntu:cthulhu").into(),
                )])),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "task requirement `container` was modified"
        );
    }

    #[tokio::test]
    async fn hint_removed() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Object::new(IndexMap::from_iter([(
            "foo".into(),
            PrimitiveValue::new_string("bar").into(),
        )]));

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                hints: &Object::default(),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "task hint `foo` was removed"
        );
    }

    #[tokio::test]
    async fn hint_added() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                hints: &Object::new(IndexMap::from_iter([(
                    "foo".into(),
                    PrimitiveValue::new_string("bar").into(),
                )])),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "task hint `foo` was added"
        );
    }

    #[tokio::test]
    async fn hint_modified() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Object::new(IndexMap::from_iter([(
            "foo".into(),
            PrimitiveValue::new_string("bar").into(),
        )]));

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                hints: &Object::new(IndexMap::from_iter([(
                    "foo".into(),
                    PrimitiveValue::new_string("baz").into(),
                )])),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "task hint `foo` was modified"
        );
    }

    #[tokio::test]
    async fn backend_input_removed() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        // Create an input file
        let input_file_path = cache.root_dir.path().join("input");
        fs::write(&input_file_path, "hello world!").await.unwrap();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[Input::new(
                ContentKind::File,
                EvaluationPath::from_local_path(input_file_path),
                Some(GuestPath::new("/mnt/task/inputs/0/input")),
            )],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                backend_inputs: &[],
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "a file or directory input was removed since last evaluation"
        );
    }

    #[tokio::test]
    async fn backend_input_added() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        // Create an input file
        let input_file_path = cache.root_dir.path().join("input");
        fs::write(&input_file_path, "hello world!").await.unwrap();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        let key = cache
            .inner
            .key(&KeyRequest {
                backend_inputs: &[Input::new(
                    ContentKind::File,
                    EvaluationPath::from_local_path(input_file_path),
                    Some(GuestPath::new("/mnt/task/inputs/0/input")),
                )],
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "a file or directory input was added since last evaluation"
        );
    }

    #[tokio::test]
    async fn backend_input_modified() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        // Create an input file
        let input_file_path = cache.root_dir.path().join("input");
        fs::write(&input_file_path, "hello world!").await.unwrap();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[Input::new(
                ContentKind::File,
                EvaluationPath::from_local_path(input_file_path.clone()),
                Some(GuestPath::new("/mnt/task/inputs/0/input")),
            )],
        };

        cache.populate(&request).await;

        // Changing the file's contents invalidates the entry
        fs::write(&input_file_path, "changed!").await.unwrap();
        clear_digest_cache();

        let key = cache.inner.key(&request).await.unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "the content of a file or directory input was modified"
        );
    }

    #[tokio::test]
    async fn stdout_modified() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Changing the stdout file invalidates the entry
        fs::write(&cache.stdout, "changed!").await.unwrap();
        clear_digest_cache();

        let key = cache.inner.key(&request).await.unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            format!(
                "cached content `{stdout}` was modified",
                stdout = cache.stdout.display()
            )
        );
    }

    #[tokio::test]
    async fn stdout_missing() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Deleting the stdout file invalidates the entry
        fs::remove_file(&cache.stdout).await.unwrap();
        clear_digest_cache();

        let key = cache.inner.key(&request).await.unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            format!(
                "failed to read metadata of `{stdout}`",
                stdout = cache.stdout.display()
            )
        );
    }

    #[tokio::test]
    async fn stderr_modified() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Changing the stderr file invalidates the entry
        fs::write(&cache.stderr, "changed!").await.unwrap();
        clear_digest_cache();

        let key = cache.inner.key(&request).await.unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            format!(
                "cached content `{stderr}` was modified",
                stderr = cache.stderr.display()
            )
        );
    }

    #[tokio::test]
    async fn stderr_missing() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Deleting the stderr file invalidates the entry
        fs::remove_file(&cache.stderr).await.unwrap();
        clear_digest_cache();

        let key = cache.inner.key(&request).await.unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            format!(
                "failed to read metadata of `{stderr}`",
                stderr = cache.stderr.display()
            )
        );
    }

    #[tokio::test]
    async fn work_dir_modified() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Changing the work directory (by adding a file) invalidates the entry
        fs::write(&cache.work_dir.join("foo"), "added!")
            .await
            .unwrap();
        clear_digest_cache();

        let key = cache.inner.key(&request).await.unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            format!(
                "cached content `{work_dir}` was modified",
                work_dir = cache.work_dir.display()
            )
        );
    }

    #[tokio::test]
    async fn work_dir_missing() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Deleting the working directory invalidates the entry
        fs::remove_dir_all(&cache.work_dir).await.unwrap();
        clear_digest_cache();

        let key = cache.inner.key(&request).await.unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            format!(
                "failed to read metadata of `{work_dir}`",
                work_dir = cache.work_dir.display()
            )
        );
    }

    #[tokio::test]
    async fn excluded_requirement_modified() {
        // Exclude the memory requirement
        let cache = TestCache::new_with_exclusions(CallCacheExclusions {
            requirements: HashSet::from_iter(["memory".to_string()]),
            ..Default::default()
        })
        .await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Object::new(IndexMap::from_iter([
            (
                "container".into(),
                PrimitiveValue::new_string("ubuntu:latest").into(),
            ),
            ("memory".into(), 1.into()),
        ]));
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Modify the memory requirement; this should not affect the entry
        let key = cache
            .inner
            .key(&KeyRequest {
                requirements: &Object::new(IndexMap::from_iter([
                    (
                        "container".into(),
                        PrimitiveValue::new_string("ubuntu:latest").into(),
                    ),
                    ("memory".into(), 1000.into()),
                ])),
                ..request
            })
            .await
            .unwrap();
        cache.inner.get(&key).await.unwrap().unwrap();

        // Modify the container requirement; this should affect the entry
        let key = cache
            .inner
            .key(&KeyRequest {
                requirements: &Object::new(IndexMap::from_iter([
                    (
                        "container".into(),
                        PrimitiveValue::new_string("ubuntu:cthulhu").into(),
                    ),
                    ("memory".into(), 1.into()),
                ])),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "task requirement `container` was modified"
        );
    }

    #[tokio::test]
    async fn excluded_hint_modified() {
        // Exclude the `localization_optional` hint
        let cache = TestCache::new_with_exclusions(CallCacheExclusions {
            hints: HashSet::from_iter(["localization_optional".into()]),
            ..Default::default()
        })
        .await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Object::new(IndexMap::from_iter([
            ("foo".into(), PrimitiveValue::new_string("bar").into()),
            ("localization_optional".into(), true.into()),
        ]));

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        cache.populate(&request).await;

        // Modify the `localization_optional` hint; this should not affect the entry
        let key = cache
            .inner
            .key(&KeyRequest {
                hints: &Object::new(IndexMap::from_iter([
                    ("foo".into(), PrimitiveValue::new_string("bar").into()),
                    ("localization_optional".into(), false.into()),
                ])),
                ..request
            })
            .await
            .unwrap();
        cache.inner.get(&key).await.unwrap().unwrap();

        // Modify the `foo` hint; this should affect the entry
        let key = cache
            .inner
            .key(&KeyRequest {
                hints: &Object::new(IndexMap::from_iter([
                    ("foo".into(), PrimitiveValue::new_string("baz").into()),
                    ("localization_optional".into(), true.into()),
                ])),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.inner.get(&key).await.unwrap_err().to_string(),
            "task hint `foo` was modified"
        );
    }

    #[tokio::test]
    async fn excluded_input_modified() {
        // Exclude the `foo` input
        let cache = TestCache::new_with_exclusions(CallCacheExclusions {
            inputs: HashSet::from_iter(["foo".into()]),
            ..Default::default()
        })
        .await;

        // Create an input file
        let input_file_path = cache.root_dir.path().join("input");
        fs::write(&input_file_path, "hello world!").await.unwrap();

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = BTreeMap::from_iter([
            (
                "foo".to_string(),
                Value::from(PrimitiveValue::new_file(input_file_path.to_str().unwrap())),
            ),
            ("bar".into(), PrimitiveValue::new_string("baz").into()),
        ]);
        let requirements = Default::default();
        let hints = Default::default();

        let mut input = Input::new(
            ContentKind::File,
            EvaluationPath::from_local_path(input_file_path.clone()),
            Some(GuestPath::new("/mnt/task/inputs/0/input")),
        );
        input.mark_excluded_from_call_cache();
        let backend_inputs = [input];

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "cat /mnt/task/inputs/0/input",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &backend_inputs,
        };

        cache.populate(&request).await;

        // Modify the `foo` input; this should not affect the entry
        let key = cache
            .inner
            .key(&KeyRequest {
                inputs: &BTreeMap::from_iter([
                    ("foo".into(), 1.into()),
                    ("bar".into(), PrimitiveValue::new_string("baz").into()),
                ]),
                ..request
            })
            .await
            .unwrap();
        cache.inner.get(&key).await.unwrap().unwrap();

        // Changing the file's contents should not invalidate the entry
        fs::write(&input_file_path, "changed!").await.unwrap();
        clear_digest_cache();

        // Modify the `foo` input; this should not affect the entry as the backend input
        // was excluded
        let key = cache.inner.key(&request).await.unwrap();
        cache.inner.get(&key).await.unwrap().unwrap();

        // Modify the `bar` input; the key should change and the entry should not exist
        let key = cache
            .inner
            .key(&KeyRequest {
                inputs: &BTreeMap::from_iter([
                    (
                        "foo".to_string(),
                        Value::from(PrimitiveValue::new_file(input_file_path.to_str().unwrap())),
                    ),
                    ("bar".into(), PrimitiveValue::new_string("qux").into()),
                ]),
                ..request
            })
            .await
            .unwrap();
        assert!(cache.inner.get(&key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn backend_in_cache_key() {
        let cache = TestCache::new().await;

        let document_uri = Url::from_file_path(cache.root_dir.path().join("source.wdl")).unwrap();
        let inputs = Default::default();
        let requirements = Default::default();
        let hints = Default::default();

        let request = KeyRequest {
            document_uri: &document_uri,
            backend: "backend",
            task_name: "test",
            inputs: &inputs,
            command: "echo hello world!",
            default_container: None,
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            guest_inputs_dir: Some("/mnt/task/inputs/"),
            backend_inputs: &[],
        };

        // Compute the cache key
        let original = cache.inner.key(&request).await.unwrap();

        // Compute a key with a different backend
        let modified = cache
            .inner
            .key(&KeyRequest {
                backend: "different",
                ..request
            })
            .await
            .unwrap();

        assert_ne!(
            original.key, modified.key,
            "expected different cache keys for different backends"
        );
    }
}
