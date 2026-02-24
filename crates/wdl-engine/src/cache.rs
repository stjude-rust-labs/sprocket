//! Implementation of the call and digest caches.

use std::collections::BTreeMap;
use std::collections::HashMap;
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
use tracing::info;
use url::Url;

use crate::ContentKind;
use crate::EvaluationPath;
use crate::PrimitiveValue;
use crate::Value;
use crate::backend::Input;
use crate::backend::TaskExecutionResult;
use crate::cache::hash::hash_sequence;
use crate::cache::lock::LockedFile;
use crate::config::ContentDigestMode;
use crate::http::Transferer;

/// The current cache entry version.
///
/// This is a monotonic value that should increase whenever the serialization of
/// call cache entries change.
const CURRENT_CACHE_VERSION: u32 = 0;

/// The default cache subdirectory for the call cache.
const CALL_CACHE_SUBDIR: &str = "calls";

/// The name of the global cache lock file.
const CACHE_LOCK_FILE_NAME: &str = ".lock";

mod hash;
mod lock;

pub use hash::Hashable;

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
    /// The monotonic version of the cache entry.
    version: u32,
    /// The digest of the command's evaluated task.
    command: ArrayString<64>,
    /// The container used by the task.
    container: String,
    /// The shell used by the task.
    shell: String,
    /// The requirement digests of the task.
    requirements: HashMap<String, ArrayString<64>>,
    /// The hint digests of the task.
    hints: HashMap<String, ArrayString<64>>,
    /// The digests of the backend inputs of the task.
    inputs: HashMap<String, ArrayString<64>>,
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
    /// The container used by the task.
    container: String,
    /// The shell used by the task.
    shell: String,
    /// The requirement digests of the task.
    requirements: HashMap<String, ArrayString<64>>,
    /// The hint digests of the task.
    hints: HashMap<String, ArrayString<64>>,
    /// The content digests of the backend inputs to the task.
    backend_inputs: HashMap<String, ArrayString<64>>,
}

impl Key {
    /// Gets the string representation of the key.
    pub fn as_str(&self) -> &str {
        self.key.as_str()
    }

    /// Ensure this [`Key`] matches the given [`CallCacheEntry`].
    ///
    /// Returns an error if there is a mismatch.
    fn ensure_matches(&self, entry: &CallCacheEntry, excluded_hints: &[String], excluded_inputs: &[String], excluded_requirements: &[String]) -> Result<()> {
        fn compare_maps<K, V>(
            a: &HashMap<K, V>,
            b: &HashMap<K, V>,
            kind: &str,
            excluded: &[String],
        ) -> Result<()>
        where
            K: std::hash::Hash + fmt::Display + Eq,
            V: Eq,
        {
            for (k, v) in a {
                // Skip excluded keys
                let key_str = k.to_string();
                if excluded.contains(&key_str) {
                    info!("{} `{}` is excluded from cache checking, skipping", kind, k);
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
                    info!("{} `{}` is excluded from cache checking, skipping", kind, k);
                    continue;
                }

                if !a.contains_key(k) {
                    bail!("{kind} `{k}` was removed");
                }
            }

            Ok(())
        }

        if self.command != entry.command {
            bail!("the command of the task was modified");
        }

        if self.container != entry.container {
            bail!("the container used by the task was modified");
        }

        if self.shell != entry.shell {
            bail!("the shell used by the task was modified");
        }

        compare_maps(
            &self.requirements,
            &entry.requirements,
            "task requirement",
            excluded_requirements,
        )?;
        compare_maps(&self.hints, &entry.hints, "task hint", excluded_hints)?;
        compare_maps(
            &self.backend_inputs,
            &entry.inputs,
            "task input",
            excluded_inputs,
        )?;
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
    /// The container used by the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub container: &'a str,
    /// The shell used by the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub shell: &'a str,
    /// The evaluated requirements of the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub requirements: &'a HashMap<String, Value>,
    /// The evaluated hints of the task.
    ///
    /// This field contributes to the digests stored in a cache entry.
    pub hints: &'a HashMap<String, Value>,
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
        }))
    }

    /// Calculates a new [`Key`] to use for the cache.
    ///
    /// This will calculate digests for the command, requirements, hints, and
    /// inputs.
    pub async fn key(&self, request: KeyRequest<'_>) -> Result<Key> {
        // Calculate the command digest.
        let mut hasher = blake3::Hasher::new();
        request.command.hash(&mut hasher);
        let command_digest = hasher.finalize().to_hex();

        // Calculate the requirement digests
        let requirement_digests = request
            .requirements
            .iter()
            .map(|(k, v)| {
                let mut hasher = blake3::Hasher::new();
                v.hash(&mut hasher);
                (k.clone(), hasher.finalize().to_hex())
            })
            .collect();

        // Calculate the hint digests
        let hint_digests = request
            .hints
            .iter()
            .map(|(k, v)| {
                let mut hasher = blake3::Hasher::new();
                v.hash(&mut hasher);
                (k.clone(), hasher.finalize().to_hex())
            })
            .collect();

        // Calculate the digests of the backend inputs
        let mut backend_inputs = HashMap::with_capacity(request.backend_inputs.len());
        for input in request.backend_inputs {
            let digest = input
                .path()
                .calculate_digest(self.0.transferer.as_ref(), input.kind(), self.0.mode)
                .await?;

            backend_inputs.insert(input.path().to_string(), digest.to_hex());
        }

        // Calculate the task's cache key
        let mut hasher = blake3::Hasher::new();
        request.document_uri.hash(&mut hasher);
        request.task_name.hash(&mut hasher);
        hash_sequence(&mut hasher, request.inputs.iter());
        let key = hasher.finalize().to_hex();

        Ok(Key {
            key,
            command: command_digest,
            container: request.container.into(),
            shell: request.shell.into(),
            requirements: requirement_digests,
            hints: hint_digests,
            backend_inputs,
        })
    }

    /// Gets an entry from the [`CallCache`] given the cache key and information
    /// about the current task.
    ///
    /// Returns `Ok(None)` if a cache entry with the given key does not exist.
    ///
    /// Returns an error if the entry could not be read or if the entry is no
    /// longer valid.
    pub async fn get(
        &self,
        key: &Key,
        excluded_hints: Vec<String>,
        excluded_inputs: Vec<String>,
        excluded_requirements: Vec<String>,
    ) -> Result<Option<TaskExecutionResult>> {
        // Take a shared lock on the entry file
        let path = self.0.entry_path(key);
        let file = match LockedFile::acquire_shared(&path, false).await? {
            Some(file) => file,
            None => return Ok(None),
        };

        // Deserialize the entry and ensure it matches the current evaluation
        let entry: CallCacheEntry = serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("failed to deserialize call cache entry `{key}`"))?;

        if entry.version != CURRENT_CACHE_VERSION {
            bail!(
                "cache entry `{key}` has a mismatched version: expected version is \
                 {CURRENT_CACHE_VERSION}, but the entry is {version}",
                version = entry.version
            );
        }

        // Ensure the key matches the cache entry
        key.ensure_matches(&entry, &excluded_hints, &excluded_inputs, &excluded_requirements)?;

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
            version: CURRENT_CACHE_VERSION,
            command: key.command,
            container: key.container,
            shell: key.shell,
            requirements: key.requirements,
            hints: key.hints,
            inputs: key.backend_inputs,
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
}

#[cfg(test)]
mod test {
    use std::vec;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;
    use tempfile::tempdir;

    use super::*;
    use crate::GuestPath;
    use crate::digest::test::DigestTransferer;
    use crate::digest::test::clear_digest_cache;

    /// Stores paths used in the test.
    struct Paths {
        /// The path to the WDL source being analyzed.
        source: PathBuf,
        /// The input file.
        input: PathBuf,
        /// The path to the stdout file.
        stdout: PathBuf,
        /// The path to the stderr file.
        stderr: PathBuf,
        /// The path to the task's working directory.
        work_dir: PathBuf,
    }

    /// Represents the "evaluated" task for the tests.
    struct Task {
        paths: Paths,
        document_uri: Url,
        inputs: BTreeMap<String, Value>,
        requirements: HashMap<String, Value>,
        hints: HashMap<String, Value>,
        backend_inputs: [Input; 1],
    }

    impl Task {
        /// Constructs a new "evaluated" task.
        fn new(paths: Paths) -> Self {
            // These values correspond to what would be evaluated for the WDL source in
            // `prepare_task`.
            let document_uri = Url::from_file_path(&paths.source).unwrap();
            let input = paths.input.clone();
            Self {
                paths,
                document_uri,
                inputs: BTreeMap::from([(
                    "file".into(),
                    PrimitiveValue::new_file(input.to_str().unwrap()).into(),
                )]),
                requirements: HashMap::from_iter([(
                    "container".into(),
                    PrimitiveValue::new_string("ubuntu:latest").into(),
                )]),
                hints: HashMap::from_iter([(
                    "foo".into(),
                    PrimitiveValue::new_string("bar").into(),
                )]),
                backend_inputs: [Input::new(
                    ContentKind::File,
                    EvaluationPath::from_local_path(input),
                    Some(GuestPath::new("/mnt/task/0/input")),
                )],
            }
        }

        /// Gets a cache key request from the "evaluated" task.
        fn key_request(&self) -> KeyRequest<'_> {
            // These values correspond to what would be evaluated for the WDL source in
            // `prepare_task`.
            KeyRequest {
                document_uri: &self.document_uri,
                task_name: "test",
                inputs: &self.inputs,
                command: "cat /mnt/task/0/input",
                container: "ubuntu:latest",
                shell: "bash",
                requirements: &self.requirements,
                hints: &self.hints,
                backend_inputs: &self.backend_inputs,
            }
        }
    }

    /// Prepares an "evaluated" task.
    ///
    /// This populates the root directory with a source file, inputs, and
    /// outputs.
    ///
    /// This does not actually evaluate any WDL; instead it returns enough
    /// information for interacting with a call cache as if an evaluation
    /// occurred.
    async fn prepare_task(root_dir: &Path) -> Task {
        let source_dir = root_dir.join("src");
        fs::create_dir_all(&source_dir).await.unwrap();

        let inputs_dir = root_dir.join("inputs");
        fs::create_dir_all(&inputs_dir).await.unwrap();

        let outputs_dir = root_dir.join("outputs");
        fs::create_dir_all(&outputs_dir).await.unwrap();

        let paths = Paths {
            source: source_dir.join("source.wdl"),
            input: inputs_dir.join("input"),
            stdout: outputs_dir.join("stdout"),
            stderr: outputs_dir.join("stderr"),
            work_dir: outputs_dir.join("work"),
        };

        // The content of the source file doesn't matter for the purpose of the call
        // cache tests
        fs::write(&paths.source, "").await.unwrap();

        // Write the input file
        fs::write(&paths.input, "hello world!").await.unwrap();

        // Write the stdout as if we evaluated the task
        fs::write(&paths.stdout, "hello world!").await.unwrap();

        // Write the stderr as if we evaluated the task
        fs::write(&paths.stderr, "").await.unwrap();

        // Create a work directory as if we evaluated the task
        fs::create_dir(&paths.work_dir).await.unwrap();

        Task::new(paths)
    }

    /// Populates a call cache with the baseline cache entry.
    async fn populate_cache(cache: &CallCache, task: &Task) {
        // Get a key for the cache (should not exist)
        let key = cache.key(task.key_request()).await.unwrap();
        assert!(cache.get(&key, vec![], vec![], vec![]).await.unwrap().is_none());

        // Cache an execution result
        let result = TaskExecutionResult {
            exit_code: 0,
            work_dir: EvaluationPath::from_local_path(task.paths.work_dir.clone()),
            stdout: PrimitiveValue::new_file(task.paths.stdout.to_str().unwrap()).into(),
            stderr: PrimitiveValue::new_file(task.paths.stderr.to_str().unwrap()).into(),
        };
        cache.put(key, &result).await.unwrap();

        // Get the entry we just put and ensure the same result is returned
        let key = cache.key(task.key_request()).await.unwrap();
        let cached_result = cache
            .get(&key, vec![], vec![], vec![])
            .await
            .unwrap()
            .expect("should have cache entry");
        assert_eq!(
            result.exit_code, cached_result.exit_code,
            "exit code mismatch"
        );
        assert_eq!(
            result.work_dir, cached_result.work_dir,
            "work directory mismatch"
        );
        assert_eq!(
            result.stdout.as_file().unwrap(),
            cached_result.stdout.as_file().unwrap(),
            "stdout mismatch"
        );
        assert_eq!(
            result.stderr.as_file().unwrap(),
            cached_result.stderr.as_file().unwrap(),
            "stderr mismatch"
        );
    }

    /// Stores context for each call cache test case.
    struct TestContext {
        /// The root directory for the test.
        _root_dir: TempDir,
        /// An "evaluated" task to insert into the cache.
        task: Task,
        /// The call cache used by the test.
        cache: CallCache,
    }

    impl TestContext {
        /// Constructs a new test context.
        async fn new() -> Self {
            // Prepare an evaluated task for the test
            let root_dir = tempdir().expect("failed to create temporary directory");
            let task = prepare_task(root_dir.path()).await;

            // Create the cache
            let transfer = Arc::new(DigestTransferer::new([]));
            let cache = CallCache::new(
                Some(&root_dir.path().join("cache")),
                ContentDigestMode::Strong,
                transfer,
            )
            .await
            .unwrap();

            // Populate the cache with the initial entry
            populate_cache(&cache, &task).await;

            Self {
                _root_dir: root_dir,
                task,
                cache,
            }
        }
    }

    #[tokio::test]
    async fn modified_command() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for modified command
        let key = ctx
            .cache
            .key(KeyRequest {
                command: "modified!",
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "the command of the task was modified"
        );
    }

    #[tokio::test]
    async fn modified_container() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for modified container
        let key = ctx
            .cache
            .key(KeyRequest {
                container: "modified!",
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "the container used by the task was modified"
        );
    }

    #[tokio::test]
    async fn modified_shell() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for modified shell
        let key = ctx
            .cache
            .key(KeyRequest {
                shell: "modified!",
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "the shell used by the task was modified"
        );
    }

    #[tokio::test]
    async fn requirement_removed() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for removing a requirement
        let key = ctx
            .cache
            .key(KeyRequest {
                requirements: &Default::default(),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "task requirement `container` was removed"
        );
    }

    #[tokio::test]
    async fn requirement_added() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for adding a requirement
        let key = ctx
            .cache
            .key(KeyRequest {
                requirements: &HashMap::from_iter([
                    (
                        "container".into(),
                        PrimitiveValue::new_string("ubuntu:latest").into(),
                    ),
                    ("memory".into(), 1000.into()),
                ]),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "task requirement `memory` was added"
        );
    }

    #[tokio::test]
    async fn requirement_modified() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for modifying a requirement
        let key = ctx
            .cache
            .key(KeyRequest {
                requirements: &HashMap::from_iter([(
                    "container".into(),
                    PrimitiveValue::new_string("ubuntu:cthulhu").into(),
                )]),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "task requirement `container` was modified"
        );
    }

    #[tokio::test]
    async fn hint_removed() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for removing a hint
        let key = ctx
            .cache
            .key(KeyRequest {
                hints: &Default::default(),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "task hint `foo` was removed"
        );
    }

    #[tokio::test]
    async fn hint_added() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for adding a hint
        let key = ctx
            .cache
            .key(KeyRequest {
                hints: &HashMap::from_iter([
                    ("foo".into(), PrimitiveValue::new_string("bar").into()),
                    ("max_memory".into(), 1000.into()),
                ]),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "task hint `max_memory` was added"
        );
    }

    #[tokio::test]
    async fn hint_modified() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for modifying a hint
        let key = ctx
            .cache
            .key(KeyRequest {
                hints: &HashMap::from_iter([(
                    "foo".into(),
                    PrimitiveValue::new_string("baz!").into(),
                )]),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            "task hint `foo` was modified"
        );
    }

    #[tokio::test]
    async fn backend_input_removed() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Check for removing a backend input
        let key = ctx
            .cache
            .key(KeyRequest {
                backend_inputs: &[],
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!(
                "task input `{}` was removed",
                ctx.task.paths.input.display()
            )
        );
    }

    #[tokio::test]
    async fn backend_input_added() {
        let ctx = TestContext::new().await;
        let request = ctx.task.key_request();

        // Create a new input file
        let input2 = ctx.task.paths.input.with_file_name("input2");
        fs::write(&input2, "hello world!!!").await.unwrap();

        // Check for adding a backend input
        let key = ctx
            .cache
            .key(KeyRequest {
                backend_inputs: &[
                    ctx.task.backend_inputs[0].clone(),
                    Input::new(
                        ContentKind::File,
                        EvaluationPath::from_local_path(input2.clone()),
                        Some(GuestPath::new("/mnt/task/0/input2")),
                    ),
                ],
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!("task input `{}` was added", input2.display())
        );
    }

    #[tokio::test]
    async fn backend_input_modified() {
        let ctx = TestContext::new().await;

        // Modify the input file
        fs::write(&ctx.task.paths.input, "modified!").await.unwrap();

        // Check for modifying a backend input
        clear_digest_cache();
        let key = ctx.cache.key(ctx.task.key_request()).await.unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!(
                "task input `{}` was modified",
                ctx.task.paths.input.display()
            )
        );
    }

    #[tokio::test]
    async fn stdout_modified() {
        let ctx = TestContext::new().await;

        // Modify the stdout file
        fs::write(&ctx.task.paths.stdout, "modified!")
            .await
            .unwrap();

        // Check for changed cached stdout
        clear_digest_cache();
        let key = ctx.cache.key(ctx.task.key_request()).await.unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!(
                "cached content `{}` was modified",
                ctx.task.paths.stdout.display()
            )
        );
    }

    #[tokio::test]
    async fn stdout_missing() {
        let ctx = TestContext::new().await;

        // Delete the stdout file
        fs::remove_file(&ctx.task.paths.stdout).await.unwrap();

        // Check for deleted cached stdout
        clear_digest_cache();
        let key = ctx.cache.key(ctx.task.key_request()).await.unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!(
                "failed to read metadata of `{}`",
                ctx.task.paths.stdout.display()
            )
        );
    }

    #[tokio::test]
    async fn stderr_modified() {
        let ctx = TestContext::new().await;

        // Modify the stderr file
        fs::write(&ctx.task.paths.stderr, "modified!")
            .await
            .unwrap();

        // Check for changed cached stderr
        clear_digest_cache();
        let key = ctx.cache.key(ctx.task.key_request()).await.unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!(
                "cached content `{}` was modified",
                ctx.task.paths.stderr.display()
            )
        );
    }

    #[tokio::test]
    async fn stderr_missing() {
        let ctx = TestContext::new().await;

        // Delete the stderr file
        fs::remove_file(&ctx.task.paths.stderr).await.unwrap();

        // Check for deleted cached stderr
        clear_digest_cache();
        let key = ctx.cache.key(ctx.task.key_request()).await.unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!(
                "failed to read metadata of `{}`",
                ctx.task.paths.stderr.display()
            )
        );
    }

    #[tokio::test]
    async fn work_dir_modified() {
        let ctx = TestContext::new().await;

        // Modify the working directory by creating a new file in it
        fs::write(&ctx.task.paths.work_dir.join("foo"), "added!")
            .await
            .unwrap();

        // Check for changed cached work dir
        clear_digest_cache();
        let key = ctx.cache.key(ctx.task.key_request()).await.unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!(
                "cached content `{}` was modified",
                ctx.task.paths.work_dir.display()
            )
        );
    }

    #[tokio::test]
    async fn work_dir_deleted() {
        let ctx = TestContext::new().await;

        // Delete the working directory
        fs::remove_dir_all(&ctx.task.paths.work_dir).await.unwrap();

        // Check for deleted cached work dir
        clear_digest_cache();
        let key = ctx.cache.key(ctx.task.key_request()).await.unwrap();
        assert_eq!(
            ctx.cache.get(&key, vec![], vec![], vec![]).await.unwrap_err().to_string(),
            format!(
                "failed to read metadata of `{}`",
                ctx.task.paths.work_dir.display()
            )
        );
    }

    #[tokio::test]
    async fn excluded_requirement_modified() {
        // Create a context with a test task
        let root_dir = tempdir().expect("failed to create temporary directory");
        let task = prepare_task(root_dir.path()).await;

        // Create the cache with "memory" excluded from requirements checking
        let transfer = Arc::new(DigestTransferer::new([]));
        let cache = CallCache::new(
            Some(&root_dir.path().join("cache")),
            ContentDigestMode::Strong,
            transfer,
        )
        .await
        .unwrap();

        // Populate the cache with an initial entry
        populate_cache(&cache, &task).await;

        let request = task.key_request();

        // Add a "memory" requirement - this should NOT invalidate the cache
        // since "memory" is in the exclusion list
        let key = cache
            .key(KeyRequest {
                requirements: &HashMap::from_iter([
                    (
                        "container".into(),
                        PrimitiveValue::new_string("ubuntu:latest").into(),
                    ),
                    ("memory".into(), 1000.into()),
                ]),
                ..request
            })
            .await
            .unwrap();

        // This should succeed (not return an error) because "memory" is excluded
        assert!(
            cache.get(&key, vec![], vec![], vec!["memory".to_string()]).await.is_ok(),
            "Expected cache hit when excluded requirement is added"
        );

        // Modify a non-excluded requirement - this SHOULD invalidate the cache
        let key = cache
            .key(KeyRequest {
                requirements: &HashMap::from_iter([(
                    "container".into(),
                    PrimitiveValue::new_string("ubuntu:cthulhu").into(),
                )]),
                ..request
            })
            .await
            .unwrap();

        assert_eq!(
            cache.get(&key, vec![], vec![], vec!["memory".to_string()]).await.unwrap_err().to_string(),
            "task requirement `container` was modified"
        );
    }

    #[tokio::test]
    async fn excluded_hint_modified() {
        // Create a context with a test task
        let root_dir = tempdir().expect("failed to create temporary directory");
        let task = prepare_task(root_dir.path()).await;

        // Create the cache with "localization_optional" excluded from hints checking
        let transfer = Arc::new(DigestTransferer::new([]));
        let cache = CallCache::new(
            Some(&root_dir.path().join("cache")),
            ContentDigestMode::Strong,
            transfer,
        )
        .await
        .unwrap();

        // Populate the cache with an initial entry
        populate_cache(&cache, &task).await;

        let request = task.key_request();

        // Add a "localization_optional" hint - this should NOT invalidate the cache
        // since "localization_optional" is in the exclusion list
        let key = cache
            .key(KeyRequest {
                hints: &HashMap::from_iter([
                    ("foo".into(), PrimitiveValue::new_string("bar").into()),
                    (
                        "localization_optional".into(),
                        PrimitiveValue::new_string("true").into(),
                    ),
                ]),
                ..request
            })
            .await
            .unwrap();

        // This should succeed (not return an error) because "localization_optional" is
        // excluded
        assert!(
            cache.get(&key, vec!["localization_optional".to_string()], vec![], vec![]).await.is_ok(),
            "Expected cache hit when excluded hint is added"
        );

        // Modify a non-excluded hint - this SHOULD invalidate the cache
        let key = cache
            .key(KeyRequest {
                hints: &HashMap::from_iter([(
                    "foo".into(),
                    PrimitiveValue::new_string("baz").into(),
                )]),
                ..request
            })
            .await
            .unwrap();

        assert_eq!(
            cache.get(&key, vec!["localization_optional".to_string()], vec![], vec![]).await.unwrap_err().to_string(),
            "task hint `foo` was modified"
        );
    }
}
