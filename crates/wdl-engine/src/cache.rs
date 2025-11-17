//! Implementation of the call and digest caches.

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
    /// The input digests of the task.
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
/// The digests are calculated once prior to accessing the cache.
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
    /// The input content digests of the task.
    inputs: HashMap<String, ArrayString<64>>,
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
        compare_maps(&self.inputs, &entry.inputs, "task input")?;
        Ok(())
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.key.fmt(f)
    }
}

/// Represents a request to calculate a [`Key`].
pub struct KeyRequest<'a> {
    /// The document containing the task.
    pub document: &'a Document,
    /// The task identifier.
    pub task_id: &'a str,
    /// The evaluated command of the task.
    pub command: &'a str,
    /// The container used by the task.
    pub container: &'a str,
    /// The shell used by the task.
    pub shell: &'a str,
    /// The evaluated requirements of the task.
    pub requirements: &'a HashMap<String, Value>,
    /// The evaluated hints of the task.
    pub hints: &'a HashMap<String, Value>,
    /// The backend inputs of the task.
    pub inputs: &'a [Input],
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

        // Calculate the input digests
        let mut input_digests = HashMap::with_capacity(request.inputs.len());
        for input in request.inputs {
            let digest = input
                .path()
                .calculate_digest(self.0.transferer.as_ref(), input.kind())
                .await?;

            input_digests.insert(input.path().to_string(), digest.to_hex());
        }

        let mut hasher = blake3::Hasher::new();
        request.document.uri().as_ref().hash(&mut hasher);
        request.task_id.hash(&mut hasher);
        let key = hasher.finalize().to_hex();

        Ok(Key {
            key,
            command: command_digest,
            container: request.container.into(),
            shell: request.shell.into(),
            requirements: requirement_digests,
            hints: hint_digests,
            inputs: input_digests,
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
