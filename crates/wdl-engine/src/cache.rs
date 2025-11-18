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
use wdl_analysis::Document;

use crate::ContentKind;
use crate::Input;
use crate::PrimitiveValue;
use crate::Value;
use crate::backend::TaskExecutionResult;
use crate::cache::hash::hash_sequence;
use crate::cache::lock::LockedFile;
use crate::http::Transferer;
use crate::path::EvaluationPath;

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
    ) -> Result<Self> {
        let digest = path.calculate_digest(transferer, kind).await?;
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
    ) -> Result<EvaluationPath> {
        let path: EvaluationPath = self.location.parse()?;
        let digest = path.calculate_digest(transferer, kind).await?;
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
    fn ensure_matches(&self, entry: &CallCacheEntry) -> Result<()> {
        fn compare_maps<K, V>(a: &HashMap<K, V>, b: &HashMap<K, V>, kind: &str) -> Result<()>
        where
            K: std::hash::Hash + fmt::Display + Eq,
            V: Eq,
        {
            for (k, v) in a {
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

        compare_maps(&self.requirements, &entry.requirements, "task requirement")?;
        compare_maps(&self.hints, &entry.hints, "task hint")?;
        compare_maps(&self.backend_inputs, &entry.inputs, "task input")?;
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
    /// The document containing the task.
    ///
    /// This field directly contributes to the cache key.
    pub document: &'a Document,
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
    pub async fn new(cache_dir: Option<&Path>, transferer: Arc<dyn Transferer>) -> Result<Self> {
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
                .calculate_digest(self.0.transferer.as_ref(), input.kind())
                .await?;

            backend_inputs.insert(input.path().to_string(), digest.to_hex());
        }

        // Calculate the task's cache key
        let mut hasher = blake3::Hasher::new();
        request.document.uri().as_ref().hash(&mut hasher);
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

        if entry.version != CURRENT_CACHE_VERSION {
            bail!(
                "cache entry `{key}` has a mismatched version: expected version is \
                 {CURRENT_CACHE_VERSION}, but the entry is {version}",
                version = entry.version
            );
        }

        // Ensure the key matches the cache entry
        key.ensure_matches(&entry)?;

        let stdout = entry
            .stdout
            .to_evaluation_path(self.0.transferer.as_ref(), ContentKind::File)
            .await?;
        let stderr = entry
            .stderr
            .to_evaluation_path(self.0.transferer.as_ref(), ContentKind::File)
            .await?;
        let work = entry
            .work
            .to_evaluation_path(self.0.transferer.as_ref(), ContentKind::Directory)
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
        let file = LockedFile::acquire_exclusive(&self.0.entry_path(&key)).await?;

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
            )
            .await?,
            work: Content::from_evaluation_path(
                self.0.transferer.as_ref(),
                result.work_dir.clone(),
                ContentKind::Directory,
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
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;
    use wdl_analysis::Analyzer;
    use wdl_analysis::Config as AnalysisConfig;
    use wdl_analysis::DiagnosticsConfig;

    use super::*;
    use crate::GuestPath;
    use crate::digest::test::DigestTransferer;
    use crate::digest::test::clear_digest_cache;

    #[tokio::test]
    async fn test_call_cache() {
        // Create the cache directory
        let cache_dir = tempdir().unwrap();
        let transfer = Arc::new(DigestTransferer::new([]));
        let cache = CallCache::new(Some(cache_dir.path()), transfer)
            .await
            .unwrap();

        // Populate a root directory
        // We're not actually evaluating the WDL, so we'll create a stdout/stderr file
        // and work directory
        let root_dir = tempdir().expect("failed to create temporary directory");
        fs::write(
            root_dir.path().join("source.wdl"),
            r#"
version 1.2

task test {
    input {
        File file
    }

    requirements {
        container: "ubuntu:latest"
    }

    hints {
        foo: "bar"
    }

    command <<<cat ~{file}>>>
}
"#,
        )
        .await
        .expect("failed to write WDL source file");

        // Write input files to the task
        let input = root_dir.path().join("input");
        fs::write(&input, "hello world!").await.unwrap();
        let input2 = root_dir.path().join("input2");
        fs::write(&input2, "hello world!!!").await.unwrap();

        // Write the stdout as if we evaluated the task
        let stdout = root_dir.path().join("stdout");
        fs::write(&stdout, "hello world!").await.unwrap();

        // Write the stderr as if we evaluated the task
        let stderr = root_dir.path().join("stderr");
        fs::write(&stderr, "").await.unwrap();

        // Create a work directory as if we evaluated the task
        let work_dir = root_dir.path().join("work");
        fs::create_dir(&work_dir).await.unwrap();

        // Analyze the source file
        let analyzer = Analyzer::new(
            AnalysisConfig::default().with_diagnostics_config(DiagnosticsConfig::except_all()),
            |(), _, _, _| async {},
        );
        analyzer
            .add_directory(root_dir.path().to_path_buf())
            .await
            .expect("failed to add directory");
        let results = analyzer
            .analyze(())
            .await
            .expect("failed to analyze document");
        assert_eq!(results.len(), 1, "expected only one result");

        // Store the "evaluated" requirements and hints
        let requirements = HashMap::from_iter([(
            "container".into(),
            PrimitiveValue::new_string("ubuntu:latest").into(),
        )]);
        let hints = HashMap::from_iter([("foo".into(), PrimitiveValue::new_string("bar").into())]);

        // Store the "evaluated" inputs and backend inputs
        let inputs = BTreeMap::from([("file".into(), PrimitiveValue::new_file("input").into())]);
        let backend_input = Input::new(
            ContentKind::File,
            EvaluationPath::Local(input.clone()),
            Some(GuestPath::new("/mnt/task/0/input")),
        );
        let backend_input2 = Input::new(
            ContentKind::File,
            EvaluationPath::Local(input2.clone()),
            Some(GuestPath::new("/mnt/task/0/input2")),
        );

        let request = KeyRequest {
            document: results.first().expect("should have result").document(),
            task_name: "test",
            inputs: &inputs,
            command: "cat /mnt/task/0/input",
            container: "ubuntu:latest",
            shell: "bash",
            requirements: &requirements,
            hints: &hints,
            backend_inputs: std::slice::from_ref(&backend_input),
        };

        // Get a key for the cache (should not exist)
        let key = cache.key(request).await.unwrap();
        assert!(cache.get(&key).await.unwrap().is_none());

        // Cache an execution result
        let result = TaskExecutionResult {
            exit_code: 0,
            work_dir: EvaluationPath::Local(work_dir.clone()),
            stdout: PrimitiveValue::new_file(stdout.to_str().unwrap()).into(),
            stderr: PrimitiveValue::new_file(stderr.to_str().unwrap()).into(),
        };
        cache.put(key, &result).await.unwrap();

        // Get the entry we just put and ensure the same result is returned
        let key = cache.key(request).await.unwrap();
        let cached_result = cache
            .get(&key)
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

        // Check for changed command
        let key = cache
            .key(KeyRequest {
                command: "changed!",
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            "the command of the task was modified"
        );

        // Check for changed container
        let key = cache
            .key(KeyRequest {
                container: "changed!",
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            "the container used by the task was modified"
        );

        // Check for changed shell
        let key = cache
            .key(KeyRequest {
                shell: "changed!",
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            "the shell used by the task was modified"
        );

        // Check for removing a requirement
        let key = cache
            .key(KeyRequest {
                requirements: &Default::default(),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            "task requirement `container` was removed"
        );

        // Check for adding a requirement
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
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            "task requirement `memory` was added"
        );

        // Check for changing a requirement
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
            cache.get(&key).await.unwrap_err().to_string(),
            "task requirement `container` was modified"
        );

        // Check for removing a hint
        let key = cache
            .key(KeyRequest {
                hints: &Default::default(),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            "task hint `foo` was removed"
        );

        // Check for adding a hint
        let key = cache
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
            cache.get(&key).await.unwrap_err().to_string(),
            "task hint `max_memory` was added"
        );

        // Check for changing a hint
        let key = cache
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
            cache.get(&key).await.unwrap_err().to_string(),
            "task hint `foo` was modified"
        );

        // Check for removing a backend input
        let key = cache
            .key(KeyRequest {
                backend_inputs: &[],
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!(
                "task input `{}` was removed",
                backend_input.path().display()
            )
        );

        // Check for adding a backend input
        let key = cache
            .key(KeyRequest {
                backend_inputs: &[backend_input.clone(), backend_input2.clone()],
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!("task input `{}` was added", backend_input2.path().display())
        );

        // Change the input file and clear the digest cache
        fs::write(&input, "changed!").await.unwrap();
        clear_digest_cache();

        // Check for changing a backend input
        let key = cache
            .key(KeyRequest {
                backend_inputs: std::slice::from_ref(&backend_input),
                ..request
            })
            .await
            .unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!(
                "task input `{}` was modified",
                backend_input.path().display()
            )
        );

        // Restore the input file
        fs::write(&input, "hello world!").await.unwrap();

        // Change the cached stdout and clear the digest cache
        fs::write(&stdout, "changed!").await.unwrap();
        clear_digest_cache();

        // Check for changed cached stdout
        let key = cache.key(request).await.unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!("cached content `{}` was modified", stdout.display())
        );

        // Restore the stdout file
        fs::write(&stdout, "hello world!").await.unwrap();

        // Change the cached stderr and clear the digest cache
        fs::write(&stderr, "changed!").await.unwrap();
        clear_digest_cache();

        // Check for changed cached stderr
        let key = cache.key(request).await.unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!("cached content `{}` was modified", stderr.display())
        );

        // Restore the stderr file
        fs::write(&stderr, "").await.unwrap();

        // Modify the work directory by creating a file and clear the digest cache
        fs::write(work_dir.join("foo"), "bar").await.unwrap();
        clear_digest_cache();

        // Check for changed cached work directory
        let key = cache.key(request).await.unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!("cached content `{}` was modified", work_dir.display())
        );

        // Restore the work directory
        fs::remove_file(work_dir.join("foo")).await.unwrap();

        // Delete the stdout file and clear the digest cache
        fs::remove_file(&stdout).await.unwrap();
        clear_digest_cache();

        // Check for missing stdout file
        let key = cache.key(request).await.unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!("failed to read metadata of `{}`", stdout.display())
        );

        // Restore the stdout file
        fs::write(&stdout, "hello world!").await.unwrap();

        // Delete the stderr file and clear the digest cache
        fs::remove_file(&stderr).await.unwrap();
        clear_digest_cache();

        // Check for missing stderr file
        let key = cache.key(request).await.unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!("failed to read metadata of `{}`", stderr.display())
        );

        // Restore the stderr file
        fs::write(&stderr, "").await.unwrap();

        // Delete the work directory and clear the digest cache
        fs::remove_dir_all(&work_dir).await.unwrap();
        clear_digest_cache();

        // Check for missing work directory
        let key = cache.key(request).await.unwrap();
        assert_eq!(
            cache.get(&key).await.unwrap_err().to_string(),
            format!("failed to read metadata of `{}`", work_dir.display())
        );
    }
}
