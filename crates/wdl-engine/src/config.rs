//! Implementation of engine configuration.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use bytesize::ByteSize;
use indexmap::IndexMap;
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command;
use tracing::error;
use tracing::warn;
use url::Url;

use crate::CancellationContext;
use crate::Events;
use crate::SYSTEM;
use crate::Value;
use crate::backend::TaskExecutionBackend;
use crate::convert_unit_string;
use crate::path::is_supported_url;

/// The inclusive maximum number of task retries the engine supports.
pub(crate) const MAX_RETRIES: u64 = 100;

/// The default task shell.
pub(crate) const DEFAULT_TASK_SHELL: &str = "bash";

/// The default backend name.
pub(crate) const DEFAULT_BACKEND_NAME: &str = "default";

/// The maximum size, in bytes, for an LSF job name prefix.
const MAX_LSF_JOB_NAME_PREFIX: usize = 100;

/// The string that replaces redacted serialization fields.
const REDACTED: &str = "<REDACTED>";

/// Gets tne default root cache directory for the user.
pub(crate) fn cache_dir() -> Result<PathBuf> {
    /// The subdirectory within the user's cache directory for all caches
    const CACHE_DIR_ROOT: &str = "sprocket";

    Ok(dirs::cache_dir()
        .context("failed to determine user cache directory")?
        .join(CACHE_DIR_ROOT))
}

/// Represents a secret string that is, by default, redacted for serialization.
///
/// This type is a wrapper around [`secrecy::SecretString`].
#[derive(Debug, Clone)]
pub struct SecretString {
    /// The inner secret string.
    ///
    /// This type is not serializable.
    inner: secrecy::SecretString,
    /// Whether or not the secret string is redacted for serialization.
    ///
    /// If `true` (the default), `<REDACTED>` is serialized for the string's
    /// value.
    ///
    /// If `false`, the inner secret string is exposed for serialization.
    redacted: bool,
}

impl SecretString {
    /// Redacts the secret for serialization.
    ///
    /// By default, a [`SecretString`] is redacted; when redacted, the string is
    /// replaced with `<REDACTED>` when serialized.
    pub fn redact(&mut self) {
        self.redacted = true;
    }

    /// Unredacts the secret for serialization.
    pub fn unredact(&mut self) {
        self.redacted = false;
    }

    /// Gets the inner [`secrecy::SecretString`].
    pub fn inner(&self) -> &secrecy::SecretString {
        &self.inner
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self {
            inner: s.into(),
            redacted: true,
        }
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self {
            inner: s.into(),
            redacted: true,
        }
    }
}

impl Default for SecretString {
    fn default() -> Self {
        Self {
            inner: Default::default(),
            redacted: true,
        }
    }
}

impl serde::Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use secrecy::ExposeSecret;

        if self.redacted {
            serializer.serialize_str(REDACTED)
        } else {
            serializer.serialize_str(self.inner.expose_secret())
        }
    }
}

impl<'de> serde::Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = secrecy::SecretString::deserialize(deserializer)?;
        Ok(Self {
            inner,
            redacted: true,
        })
    }
}

/// Represents how an evaluation error or cancellation should be handled by the
/// engine.
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    /// When an error is encountered or evaluation is canceled, evaluation waits
    /// for any outstanding tasks to complete.
    #[default]
    Slow,
    /// When an error is encountered or evaluation is canceled, any outstanding
    /// tasks that are executing are immediately canceled and evaluation waits
    /// for cancellation to complete.
    Fast,
}

/// Represents WDL evaluation configuration.
///
/// <div class="warning">
///
/// By default, serialization of [`Config`] will redact the values of secrets.
///
/// Use the [`Config::unredact`] method before serialization to prevent the
/// secrets from being redacted.
///
/// </div>
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    /// HTTP configuration.
    #[serde(default)]
    pub http: HttpConfig,
    /// Workflow evaluation configuration.
    #[serde(default)]
    pub workflow: WorkflowConfig,
    /// Task evaluation configuration.
    #[serde(default)]
    pub task: TaskConfig,
    /// The name of the backend to use.
    ///
    /// If not specified and `backends` has multiple entries, it will use a name
    /// of `default`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Task execution backends configuration.
    ///
    /// If the collection is empty and `backend` is not specified, the engine
    /// default backend is used.
    ///
    /// If the collection has exactly one entry and `backend` is not specified,
    /// the singular entry will be used.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub backends: IndexMap<String, BackendConfig>,
    /// Storage configuration.
    #[serde(default)]
    pub storage: StorageConfig,
    /// (Experimental) Avoid environment-specific output; default is `false`.
    ///
    /// If this option is `true`, selected error messages and log output will
    /// avoid emitting environment-specific output such as absolute paths
    /// and system resource counts.
    ///
    /// This is largely meant to support "golden testing" where a test's success
    /// depends on matching an expected set of outputs exactly. Cues that
    /// help users overcome errors, such as the path to a temporary
    /// directory or the number of CPUs available to the system, confound this
    /// style of testing. This flag is a best-effort experimental attempt to
    /// reduce the impact of these differences in order to allow a wider
    /// range of golden tests to be written.
    #[serde(default)]
    pub suppress_env_specific_output: bool,
    /// (Experimental) Whether experimental features are enabled; default is
    /// `false`.
    ///
    /// Experimental features are provided to users with heavy caveats about
    /// their stability and rough edges. Use at your own risk, but feedback
    /// is quite welcome.
    #[serde(default)]
    pub experimental_features_enabled: bool,
    /// The failure mode for workflow or task evaluation.
    ///
    /// A value of [`FailureMode::Slow`] will result in evaluation waiting for
    /// executing tasks to complete upon error or interruption.
    ///
    /// A value of [`FailureMode::Fast`] will immediately attempt to cancel
    /// executing tasks upon error or interruption.
    #[serde(default, rename = "fail")]
    pub failure_mode: FailureMode,
}

impl Config {
    /// Validates the evaluation configuration.
    pub async fn validate(&self) -> Result<()> {
        self.http.validate()?;
        self.workflow.validate()?;
        self.task.validate()?;

        if self.backend.is_none() && self.backends.len() < 2 {
            // This is OK, we'll use either the singular backends entry (1) or
            // the default (0)
        } else {
            // Check the backends map for the backend name (or "default")
            let backend = self.backend.as_deref().unwrap_or(DEFAULT_BACKEND_NAME);
            if !self.backends.contains_key(backend) {
                bail!("a backend named `{backend}` is not present in the configuration");
            }
        }

        for backend in self.backends.values() {
            backend.validate(self).await?;
        }

        self.storage.validate()?;

        if self.suppress_env_specific_output && !self.experimental_features_enabled {
            bail!("`suppress_env_specific_output` requires enabling experimental features");
        }

        Ok(())
    }

    /// Redacts the secrets contained in the configuration.
    ///
    /// By default, secrets are redacted for serialization.
    pub fn redact(&mut self) {
        for backend in self.backends.values_mut() {
            backend.redact();
        }

        if let Some(auth) = &mut self.storage.azure.auth {
            auth.redact();
        }

        if let Some(auth) = &mut self.storage.s3.auth {
            auth.redact();
        }

        if let Some(auth) = &mut self.storage.google.auth {
            auth.redact();
        }
    }

    /// Unredacts the secrets contained in the configuration.
    ///
    /// Calling this method will expose secrets for serialization.
    pub fn unredact(&mut self) {
        for backend in self.backends.values_mut() {
            backend.unredact();
        }

        if let Some(auth) = &mut self.storage.azure.auth {
            auth.unredact();
        }

        if let Some(auth) = &mut self.storage.s3.auth {
            auth.unredact();
        }

        if let Some(auth) = &mut self.storage.google.auth {
            auth.unredact();
        }
    }

    /// Gets the backend configuration.
    ///
    /// Returns an error if the configuration specifies a named backend that
    /// isn't present in the configuration.
    pub fn backend(&self) -> Result<Cow<'_, BackendConfig>> {
        if self.backend.is_some() || self.backends.len() >= 2 {
            // Lookup the backend to use
            let backend = self.backend.as_deref().unwrap_or(DEFAULT_BACKEND_NAME);
            return Ok(Cow::Borrowed(self.backends.get(backend).ok_or_else(
                || anyhow!("a backend named `{backend}` is not present in the configuration"),
            )?));
        }

        if self.backends.len() == 1 {
            // Use the singular entry
            Ok(Cow::Borrowed(self.backends.values().next().unwrap()))
        } else {
            // Use the default
            Ok(Cow::Owned(BackendConfig::default()))
        }
    }

    /// Creates a new task execution backend based on this configuration.
    pub(crate) async fn create_backend(
        self: &Arc<Self>,
        run_root_dir: &Path,
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<Arc<dyn TaskExecutionBackend>> {
        use crate::backend::*;

        match self.backend()?.as_ref() {
            BackendConfig::Local(_) => {
                warn!(
                    "the engine is configured to use the local backend: tasks will not be run \
                     inside of a container"
                );
                Ok(Arc::new(LocalBackend::new(
                    self.clone(),
                    events,
                    cancellation,
                )?))
            }
            BackendConfig::Docker(_) => Ok(Arc::new(
                DockerBackend::new(self.clone(), events, cancellation).await?,
            )),
            BackendConfig::Tes(_) => Ok(Arc::new(
                TesBackend::new(self.clone(), events, cancellation).await?,
            )),
            BackendConfig::LsfApptainer(_) => Ok(Arc::new(LsfApptainerBackend::new(
                self.clone(),
                run_root_dir,
                events,
                cancellation,
            )?)),
            BackendConfig::SlurmApptainer(_) => Ok(Arc::new(SlurmApptainerBackend::new(
                self.clone(),
                run_root_dir,
                events,
                cancellation,
            )?)),
        }
    }
}

/// Represents HTTP configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct HttpConfig {
    /// The HTTP download cache location.
    ///
    /// Defaults to an operating system specific cache directory for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<PathBuf>,
    /// The number of retries for transferring files.
    ///
    /// Defaults to `5`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<usize>,
    /// The maximum parallelism for file transfers.
    ///
    /// Defaults to the host's available parallelism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelism: Option<usize>,
}

impl HttpConfig {
    /// Validates the HTTP configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(parallelism) = self.parallelism
            && parallelism == 0
        {
            bail!("configuration value `http.parallelism` cannot be zero");
        }
        Ok(())
    }
}

/// Represents storage configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct StorageConfig {
    /// Azure Blob Storage configuration.
    #[serde(default)]
    pub azure: AzureStorageConfig,
    /// AWS S3 configuration.
    #[serde(default)]
    pub s3: S3StorageConfig,
    /// Google Cloud Storage configuration.
    #[serde(default)]
    pub google: GoogleStorageConfig,
}

impl StorageConfig {
    /// Validates the HTTP configuration.
    pub fn validate(&self) -> Result<()> {
        self.azure.validate()?;
        self.s3.validate()?;
        self.google.validate()?;
        Ok(())
    }
}

/// Represents authentication information for Azure Blob Storage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AzureStorageAuthConfig {
    /// The Azure Storage account name to use.
    pub account_name: String,
    /// The Azure Storage access key to use.
    pub access_key: SecretString,
}

impl AzureStorageAuthConfig {
    /// Validates the Azure Blob Storage authentication configuration.
    pub fn validate(&self) -> Result<()> {
        if self.account_name.is_empty() {
            bail!("configuration value `storage.azure.auth.account_name` is required");
        }

        if self.access_key.inner.expose_secret().is_empty() {
            bail!("configuration value `storage.azure.auth.access_key` is required");
        }

        Ok(())
    }

    /// Redacts the secrets contained in the Azure Blob Storage storage
    /// authentication configuration.
    pub fn redact(&mut self) {
        self.access_key.redact();
    }

    /// Unredacts the secrets contained in the Azure Blob Storage authentication
    /// configuration.
    pub fn unredact(&mut self) {
        self.access_key.unredact();
    }
}

/// Represents configuration for Azure Blob Storage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AzureStorageConfig {
    /// The Azure Blob Storage authentication configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AzureStorageAuthConfig>,
}

impl AzureStorageConfig {
    /// Validates the Azure Blob Storage configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(auth) = &self.auth {
            auth.validate()?;
        }

        Ok(())
    }
}

/// Represents authentication information for AWS S3 storage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct S3StorageAuthConfig {
    /// The AWS Access Key ID to use.
    pub access_key_id: String,
    /// The AWS Secret Access Key to use.
    pub secret_access_key: SecretString,
}

impl S3StorageAuthConfig {
    /// Validates the AWS S3 storage authentication configuration.
    pub fn validate(&self) -> Result<()> {
        if self.access_key_id.is_empty() {
            bail!("configuration value `storage.s3.auth.access_key_id` is required");
        }

        if self.secret_access_key.inner.expose_secret().is_empty() {
            bail!("configuration value `storage.s3.auth.secret_access_key` is required");
        }

        Ok(())
    }

    /// Redacts the secrets contained in the AWS S3 storage authentication
    /// configuration.
    pub fn redact(&mut self) {
        self.secret_access_key.redact();
    }

    /// Unredacts the secrets contained in the AWS S3 storage authentication
    /// configuration.
    pub fn unredact(&mut self) {
        self.secret_access_key.unredact();
    }
}

/// Represents configuration for AWS S3 storage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct S3StorageConfig {
    /// The default region to use for S3-schemed URLs (e.g.
    /// `s3://<bucket>/<blob>`).
    ///
    /// Defaults to `us-east-1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// The AWS S3 storage authentication configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<S3StorageAuthConfig>,
}

impl S3StorageConfig {
    /// Validates the AWS S3 storage configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(auth) = &self.auth {
            auth.validate()?;
        }

        Ok(())
    }
}

/// Represents authentication information for Google Cloud Storage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GoogleStorageAuthConfig {
    /// The HMAC Access Key to use.
    pub access_key: String,
    /// The HMAC Secret to use.
    pub secret: SecretString,
}

impl GoogleStorageAuthConfig {
    /// Validates the Google Cloud Storage authentication configuration.
    pub fn validate(&self) -> Result<()> {
        if self.access_key.is_empty() {
            bail!("configuration value `storage.google.auth.access_key` is required");
        }

        if self.secret.inner.expose_secret().is_empty() {
            bail!("configuration value `storage.google.auth.secret` is required");
        }

        Ok(())
    }

    /// Redacts the secrets contained in the Google Cloud Storage authentication
    /// configuration.
    pub fn redact(&mut self) {
        self.secret.redact();
    }

    /// Unredacts the secrets contained in the Google Cloud Storage
    /// authentication configuration.
    pub fn unredact(&mut self) {
        self.secret.unredact();
    }
}

/// Represents configuration for Google Cloud Storage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GoogleStorageConfig {
    /// The Google Cloud Storage authentication configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<GoogleStorageAuthConfig>,
}

impl GoogleStorageConfig {
    /// Validates the Google Cloud Storage configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(auth) = &self.auth {
            auth.validate()?;
        }

        Ok(())
    }
}

/// Represents workflow evaluation configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WorkflowConfig {
    /// Scatter statement evaluation configuration.
    #[serde(default)]
    pub scatter: ScatterConfig,
}

impl WorkflowConfig {
    /// Validates the workflow configuration.
    pub fn validate(&self) -> Result<()> {
        self.scatter.validate()?;
        Ok(())
    }
}

/// Represents scatter statement evaluation configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ScatterConfig {
    /// The number of scatter array elements to process concurrently.
    ///
    /// Defaults to `1000`.
    ///
    /// A value of `0` is invalid.
    ///
    /// Lower values use less memory for evaluation and higher values may better
    /// saturate the task execution backend with tasks to execute for large
    /// scatters.
    ///
    /// This setting does not change how many tasks an execution backend can run
    /// concurrently, but may affect how many tasks are sent to the backend to
    /// run at a time.
    ///
    /// For example, if `concurrency` was set to 10 and we evaluate the
    /// following scatters:
    ///
    /// ```wdl
    /// scatter (i in range(100)) {
    ///     call my_task
    /// }
    ///
    /// scatter (j in range(100)) {
    ///     call my_task as my_task2
    /// }
    /// ```
    ///
    /// Here each scatter is independent and therefore there will be 20 calls
    /// (10 for each scatter) made concurrently. If the task execution
    /// backend can only execute 5 tasks concurrently, 5 tasks will execute
    /// and 15 will be "ready" to execute and waiting for an executing task
    /// to complete.
    ///
    /// If instead we evaluate the following scatters:
    ///
    /// ```wdl
    /// scatter (i in range(100)) {
    ///     scatter (j in range(100)) {
    ///         call my_task
    ///     }
    /// }
    /// ```
    ///
    /// Then there will be 100 calls (10*10 as 10 are made for each outer
    /// element) made concurrently. If the task execution backend can only
    /// execute 5 tasks concurrently, 5 tasks will execute and 95 will be
    /// "ready" to execute and waiting for an executing task to complete.
    ///
    /// <div class="warning">
    /// Warning: nested scatter statements cause exponential memory usage based
    /// on this value, as each scatter statement evaluation requires allocating
    /// new scopes for scatter array elements being processed. </div>
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u64>,
}

impl ScatterConfig {
    /// Validates the scatter configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(concurrency) = self.concurrency
            && concurrency == 0
        {
            bail!("configuration value `workflow.scatter.concurrency` cannot be zero");
        }

        Ok(())
    }
}

/// Represents the supported call caching modes.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallCachingMode {
    /// Call caching is disabled.
    ///
    /// The call cache is not checked and new entries are not added to the
    /// cache.
    ///
    /// This is the default value.
    #[default]
    Off,
    /// Call caching is enabled.
    ///
    /// The call cache is checked and new entries are added to the cache.
    ///
    /// Defaults the `cacheable` task hint to `true`.
    On,
    /// Call caching is enabled only for tasks that explicitly have a
    /// `cacheable` hint set to `true`.
    ///
    /// The call cache is checked and new entries are added to the cache *only*
    /// for tasks that have the `cacheable` hint set to `true`.
    ///
    /// Defaults the `cacheable` task hint to `false`.
    Explicit,
}

/// Represents the supported modes for calculating content digests.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentDigestMode {
    /// Use a strong digest for file content.
    ///
    /// Strong digests require hashing all of the contents of a file; this may
    /// noticeably impact performance for very large files.
    ///
    /// This setting guarantees that a modified file will be detected.
    Strong,
    /// Use a weak digest for file content.
    ///
    /// A weak digest is based solely off of file metadata, such as size and
    /// last modified time.
    ///
    /// This setting cannot guarantee the detection of modified files and may
    /// result in a modified file not causing a call cache entry to be
    /// invalidated.
    ///
    /// However, it is substantially faster than using a strong digest.
    #[default]
    Weak,
}

/// Represents task evaluation configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct TaskConfig {
    /// The default maximum number of retries to attempt if a task fails.
    ///
    /// A task's `max_retries` requirement will override this value.
    ///
    /// Defaults to 0 (no retries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<u64>,
    /// The default container to use if a container is not specified in a task's
    /// requirements.
    ///
    /// Defaults to `ubuntu:latest`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// The default shell to use for tasks.
    ///
    /// Defaults to `bash`.
    ///
    /// <div class="warning">
    /// Warning: the use of a shell other than `bash` may lead to tasks that may
    /// not be portable to other execution engines.
    ///
    /// The shell must support a `-c` option to run a specific script file (i.e.
    /// an evaluated task command).
    ///
    /// Note that this option affects all task commands, so every container that
    /// is used must contain the specified shell.
    ///
    /// If using this setting causes your tasks to fail, please do not file an
    /// issue. </div>
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// The behavior when a task's `cpu` requirement cannot be met.
    #[serde(default)]
    pub cpu_limit_behavior: TaskResourceLimitBehavior,
    /// The behavior when a task's `memory` requirement cannot be met.
    #[serde(default)]
    pub memory_limit_behavior: TaskResourceLimitBehavior,
    /// The call cache directory to use for caching task execution results.
    ///
    /// Defaults to an operating system specific cache directory for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<PathBuf>,
    /// The call caching mode to use for tasks.
    #[serde(default)]
    pub cache: CallCachingMode,
    /// The content digest mode to use.
    ///
    /// Used as part of call caching.
    #[serde(default)]
    pub digests: ContentDigestMode,
}

impl TaskConfig {
    /// Validates the task evaluation configuration.
    pub fn validate(&self) -> Result<()> {
        if self.retries.unwrap_or(0) > MAX_RETRIES {
            bail!("configuration value `task.retries` cannot exceed {MAX_RETRIES}");
        }

        Ok(())
    }
}

/// The behavior when a task resource requirement, such as `cpu` or `memory`,
/// cannot be met.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskResourceLimitBehavior {
    /// Try executing a task with the maximum amount of the resource available
    /// when the task's corresponding requirement cannot be met.
    TryWithMax,
    /// Do not execute a task if its corresponding requirement cannot be met.
    ///
    /// This is the default behavior.
    #[default]
    Deny,
}

/// Represents supported task execution backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BackendConfig {
    /// Use the local task execution backend.
    Local(LocalBackendConfig),
    /// Use the Docker task execution backend.
    Docker(DockerBackendConfig),
    /// Use the TES task execution backend.
    Tes(TesBackendConfig),
    /// Use the experimental LSF + Apptainer task execution backend.
    ///
    /// Requires enabling experimental features.
    LsfApptainer(LsfApptainerBackendConfig),
    /// Use the experimental Slurm + Apptainer task execution backend.
    ///
    /// Requires enabling experimental features.
    SlurmApptainer(SlurmApptainerBackendConfig),
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self::Docker(Default::default())
    }
}

impl BackendConfig {
    /// Validates the backend configuration.
    pub async fn validate(&self, engine_config: &Config) -> Result<()> {
        match self {
            Self::Local(config) => config.validate(),
            Self::Docker(config) => config.validate(),
            Self::Tes(config) => config.validate(),
            Self::LsfApptainer(config) => config.validate(engine_config).await,
            Self::SlurmApptainer(config) => config.validate(engine_config).await,
        }
    }

    /// Converts the backend configuration into a local backend configuration
    ///
    /// Returns `None` if the backend configuration is not local.
    pub fn as_local(&self) -> Option<&LocalBackendConfig> {
        match self {
            Self::Local(config) => Some(config),
            _ => None,
        }
    }

    /// Converts the backend configuration into a Docker backend configuration
    ///
    /// Returns `None` if the backend configuration is not Docker.
    pub fn as_docker(&self) -> Option<&DockerBackendConfig> {
        match self {
            Self::Docker(config) => Some(config),
            _ => None,
        }
    }

    /// Converts the backend configuration into a TES backend configuration
    ///
    /// Returns `None` if the backend configuration is not TES.
    pub fn as_tes(&self) -> Option<&TesBackendConfig> {
        match self {
            Self::Tes(config) => Some(config),
            _ => None,
        }
    }

    /// Converts the backend configuration into a LSF Apptainer backend
    /// configuration
    ///
    /// Returns `None` if the backend configuration is not LSF Apptainer.
    pub fn as_lsf_apptainer(&self) -> Option<&LsfApptainerBackendConfig> {
        match self {
            Self::LsfApptainer(config) => Some(config),
            _ => None,
        }
    }

    /// Converts the backend configuration into a Slurm Apptainer backend
    /// configuration
    ///
    /// Returns `None` if the backend configuration is not Slurm Apptainer.
    pub fn as_slurm_apptainer(&self) -> Option<&SlurmApptainerBackendConfig> {
        match self {
            Self::SlurmApptainer(config) => Some(config),
            _ => None,
        }
    }

    /// Redacts the secrets contained in the backend configuration.
    pub fn redact(&mut self) {
        match self {
            Self::Local(_) | Self::Docker(_) | Self::LsfApptainer(_) | Self::SlurmApptainer(_) => {}
            Self::Tes(config) => config.redact(),
        }
    }

    /// Unredacts the secrets contained in the backend configuration.
    pub fn unredact(&mut self) {
        match self {
            Self::Local(_) | Self::Docker(_) | Self::LsfApptainer(_) | Self::SlurmApptainer(_) => {}
            Self::Tes(config) => config.unredact(),
        }
    }
}

/// Represents configuration for the local task execution backend.
///
/// <div class="warning">
/// Warning: the local task execution backend spawns processes on the host
/// directly without the use of a container; only use this backend on trusted
/// WDL. </div>
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct LocalBackendConfig {
    /// Set the number of CPUs available for task execution.
    ///
    /// Defaults to the number of logical CPUs for the host.
    ///
    /// The value cannot be zero or exceed the host's number of CPUs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<u64>,

    /// Set the total amount of memory for task execution as a unit string (e.g.
    /// `2 GiB`).
    ///
    /// Defaults to the total amount of memory for the host.
    ///
    /// The value cannot be zero or exceed the host's total amount of memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
}

impl LocalBackendConfig {
    /// Validates the local task execution backend configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(cpu) = self.cpu {
            if cpu == 0 {
                bail!("local backend configuration value `cpu` cannot be zero");
            }

            let total = SYSTEM.cpus().len() as u64;
            if cpu > total {
                bail!(
                    "local backend configuration value `cpu` cannot exceed the virtual CPUs \
                     available to the host ({total})"
                );
            }
        }

        if let Some(memory) = &self.memory {
            let memory = convert_unit_string(memory).with_context(|| {
                format!("local backend configuration value `memory` has invalid value `{memory}`")
            })?;

            if memory == 0 {
                bail!("local backend configuration value `memory` cannot be zero");
            }

            let total = SYSTEM.total_memory();
            if memory > total {
                bail!(
                    "local backend configuration value `memory` cannot exceed the total memory of \
                     the host ({total} bytes)"
                );
            }
        }

        Ok(())
    }
}

/// Gets the default value for the docker `cleanup` field.
const fn cleanup_default() -> bool {
    true
}

/// Represents configuration for the Docker backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DockerBackendConfig {
    /// Whether or not to remove a task's container after the task completes.
    ///
    /// Defaults to `true`.
    #[serde(default = "cleanup_default")]
    pub cleanup: bool,
}

impl DockerBackendConfig {
    /// Validates the Docker backend configuration.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl Default for DockerBackendConfig {
    fn default() -> Self {
        Self { cleanup: true }
    }
}

/// Represents HTTP basic authentication configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct BasicAuthConfig {
    /// The HTTP basic authentication username.
    #[serde(default)]
    pub username: String,
    /// The HTTP basic authentication password.
    #[serde(default)]
    pub password: SecretString,
}

impl BasicAuthConfig {
    /// Validates the HTTP basic auth configuration.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Redacts the secrets contained in the HTTP basic auth configuration.
    pub fn redact(&mut self) {
        self.password.redact();
    }

    /// Unredacts the secrets contained in the HTTP basic auth configuration.
    pub fn unredact(&mut self) {
        self.password.unredact();
    }
}

/// Represents HTTP bearer token authentication configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct BearerAuthConfig {
    /// The HTTP bearer authentication token.
    #[serde(default)]
    pub token: SecretString,
}

impl BearerAuthConfig {
    /// Validates the HTTP bearer auth configuration.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Redacts the secrets contained in the HTTP bearer auth configuration.
    pub fn redact(&mut self) {
        self.token.redact();
    }

    /// Unredacts the secrets contained in the HTTP bearer auth configuration.
    pub fn unredact(&mut self) {
        self.token.unredact();
    }
}

/// Represents the kind of authentication for a TES backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TesBackendAuthConfig {
    /// Use basic authentication for the TES backend.
    Basic(BasicAuthConfig),
    /// Use bearer token authentication for the TES backend.
    Bearer(BearerAuthConfig),
}

impl TesBackendAuthConfig {
    /// Validates the TES backend authentication configuration.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Basic(config) => config.validate(),
            Self::Bearer(config) => config.validate(),
        }
    }

    /// Redacts the secrets contained in the TES backend authentication
    /// configuration.
    pub fn redact(&mut self) {
        match self {
            Self::Basic(auth) => auth.redact(),
            Self::Bearer(auth) => auth.redact(),
        }
    }

    /// Unredacts the secrets contained in the TES backend authentication
    /// configuration.
    pub fn unredact(&mut self) {
        match self {
            Self::Basic(auth) => auth.unredact(),
            Self::Bearer(auth) => auth.unredact(),
        }
    }
}

/// Represents configuration for the Task Execution Service (TES) backend.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct TesBackendConfig {
    /// The URL of the Task Execution Service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,

    /// The authentication configuration for the TES backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<TesBackendAuthConfig>,

    /// The root cloud storage URL for storing inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Url>,

    /// The root cloud storage URL for storing outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Url>,

    /// The polling interval, in seconds, for checking task status.
    ///
    /// Defaults to 1 second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<u64>,

    /// The number of retries after encountering an error communicating with the
    /// TES server.
    ///
    /// Defaults to no retries.
    pub retries: Option<u32>,

    /// The maximum number of concurrent requests the backend will send to the
    /// TES server.
    ///
    /// Defaults to 10 concurrent requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<u32>,

    /// Whether or not the TES server URL may use an insecure protocol like
    /// HTTP.
    #[serde(default)]
    pub insecure: bool,
}

impl TesBackendConfig {
    /// Validates the TES backend configuration.
    pub fn validate(&self) -> Result<()> {
        match &self.url {
            Some(url) => {
                if !self.insecure && url.scheme() != "https" {
                    bail!(
                        "TES backend configuration value `url` has invalid value `{url}`: URL \
                         must use a HTTPS scheme"
                    );
                }
            }
            None => bail!("TES backend configuration value `url` is required"),
        }

        if let Some(auth) = &self.auth {
            auth.validate()?;
        }

        if let Some(max_concurrency) = self.max_concurrency
            && max_concurrency == 0
        {
            bail!("TES backend configuration value `max_concurrency` cannot be zero");
        }

        match &self.inputs {
            Some(url) => {
                if !is_supported_url(url.as_str()) {
                    bail!(
                        "TES backend storage configuration value `inputs` has invalid value \
                         `{url}`: URL scheme is not supported"
                    );
                }

                if !url.path().ends_with('/') {
                    bail!(
                        "TES backend storage configuration value `inputs` has invalid value \
                         `{url}`: URL path must end with a slash"
                    );
                }
            }
            None => bail!("TES backend configuration value `inputs` is required"),
        }

        match &self.outputs {
            Some(url) => {
                if !is_supported_url(url.as_str()) {
                    bail!(
                        "TES backend storage configuration value `outputs` has invalid value \
                         `{url}`: URL scheme is not supported"
                    );
                }

                if !url.path().ends_with('/') {
                    bail!(
                        "TES backend storage configuration value `outputs` has invalid value \
                         `{url}`: URL path must end with a slash"
                    );
                }
            }
            None => bail!("TES backend storage configuration value `outputs` is required"),
        }

        Ok(())
    }

    /// Redacts the secrets contained in the TES backend configuration.
    pub fn redact(&mut self) {
        if let Some(auth) = &mut self.auth {
            auth.redact();
        }
    }

    /// Unredacts the secrets contained in the TES backend configuration.
    pub fn unredact(&mut self) {
        if let Some(auth) = &mut self.auth {
            auth.unredact();
        }
    }
}

/// Configuration for the Apptainer container runtime.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ApptainerConfig {
    /// Additional command-line arguments to pass to `apptainer exec` when
    /// executing tasks.
    pub extra_apptainer_exec_args: Option<Vec<String>>,
}

impl ApptainerConfig {
    /// Validate that Apptainer is appropriately configured.
    pub async fn validate(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

/// Configuration for an LSF queue.
///
/// Each queue can optionally have per-task CPU and memory limits set so that
/// tasks which are too large to be scheduled on that queue will fail
/// immediately instead of pending indefinitely. In the future, these limits may
/// be populated or validated by live information from the cluster, but
/// for now they must be manually based on the user's understanding of the
/// cluster configuration.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct LsfQueueConfig {
    /// The name of the queue; this is the string passed to `bsub -q
    /// <queue_name>`.
    pub name: String,
    /// The maximum number of CPUs this queue can provision for a single task.
    pub max_cpu_per_task: Option<u64>,
    /// The maximum memory this queue can provision for a single task.
    pub max_memory_per_task: Option<ByteSize>,
}

impl LsfQueueConfig {
    /// Validate that this LSF queue exists according to the local `bqueues`.
    pub async fn validate(&self, name: &str) -> Result<(), anyhow::Error> {
        let queue = &self.name;
        ensure!(!queue.is_empty(), "{name}_lsf_queue name cannot be empty");
        if let Some(max_cpu_per_task) = self.max_cpu_per_task {
            ensure!(
                max_cpu_per_task > 0,
                "{name}_lsf_queue `{queue}` must allow at least 1 CPU to be provisioned"
            );
        }
        if let Some(max_memory_per_task) = self.max_memory_per_task {
            ensure!(
                max_memory_per_task.as_u64() > 0,
                "{name}_lsf_queue `{queue}` must allow at least some memory to be provisioned"
            );
        }
        match tokio::time::timeout(
            // 10 seconds is rather arbitrary; `bqueues` ordinarily returns extremely quickly, but
            // we don't want things to run away on a misconfigured system
            std::time::Duration::from_secs(10),
            Command::new("bqueues").arg(queue).output(),
        )
        .await
        {
            Ok(output) => {
                let output = output.context("validating LSF queue")?;
                if !output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!(%stdout, %stderr, %queue, "failed to validate {name}_lsf_queue");
                    Err(anyhow!("failed to validate {name}_lsf_queue `{queue}`"))
                } else {
                    Ok(())
                }
            }
            Err(_) => Err(anyhow!(
                "timed out trying to validate {name}_lsf_queue `{queue}`"
            )),
        }
    }
}

/// Configuration for the LSF + Apptainer backend.
// TODO ACF 2025-09-23: add a Apptainer/Singularity mode config that switches around executable
// name, env var names, etc.
#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct LsfApptainerBackendConfig {
    /// The task monitor polling interval, in seconds.
    ///
    /// Defaults to 30 seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<u64>,
    /// The maximum number of concurrent LSF operations the backend will
    /// perform.
    ///
    /// This controls the maximum concurrent number of `bsub` processes the
    /// backend will spawn to queue tasks.
    ///
    /// Defaults to 10 concurrent operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<u32>,
    /// Which queue, if any, to specify when submitting normal jobs to LSF.
    ///
    /// This may be superseded by
    /// [`short_task_lsf_queue`][Self::short_task_lsf_queue],
    /// [`gpu_lsf_queue`][Self::gpu_lsf_queue], or
    /// [`fpga_lsf_queue`][Self::fpga_lsf_queue] for corresponding tasks.
    pub default_lsf_queue: Option<LsfQueueConfig>,
    /// Which queue, if any, to specify when submitting [short
    /// tasks](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#short_task) to LSF.
    ///
    /// This may be superseded by [`gpu_lsf_queue`][Self::gpu_lsf_queue] or
    /// [`fpga_lsf_queue`][Self::fpga_lsf_queue] for tasks which require
    /// specialized hardware.
    pub short_task_lsf_queue: Option<LsfQueueConfig>,
    /// Which queue, if any, to specify when submitting [tasks which require a
    /// GPU](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to LSF.
    pub gpu_lsf_queue: Option<LsfQueueConfig>,
    /// Which queue, if any, to specify when submitting [tasks which require an
    /// FPGA](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to LSF.
    pub fpga_lsf_queue: Option<LsfQueueConfig>,
    /// Additional command-line arguments to pass to `bsub` when submitting jobs
    /// to LSF.
    pub extra_bsub_args: Option<Vec<String>>,
    /// Prefix to add to every LSF job name before the task identifier. This is
    /// truncated as needed to satisfy the byte-oriented LSF job name limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_name_prefix: Option<String>,
    /// The configuration of Apptainer, which is used as the container runtime
    /// on the compute nodes where LSF dispatches tasks.
    ///
    /// Note that this will likely be replaced by an abstraction over multiple
    /// container execution runtimes in the future, rather than being
    /// hardcoded to Apptainer.
    #[serde(default)]
    // TODO ACF 2025-10-16: temporarily flatten this into the overall config so that it doesn't
    // break existing serialized configs. We'll save breaking the config file format for when we
    // actually have meaningful composition of in-place runtimes.
    #[serde(flatten)]
    pub apptainer_config: ApptainerConfig,
}

impl LsfApptainerBackendConfig {
    /// Validate that the backend is appropriately configured.
    pub async fn validate(&self, engine_config: &Config) -> Result<(), anyhow::Error> {
        if cfg!(not(unix)) {
            bail!("LSF + Apptainer backend is not supported on non-unix platforms");
        }

        if !engine_config.experimental_features_enabled {
            bail!("LSF + Apptainer backend requires enabling experimental features");
        }

        // Do what we can to validate options that are dependent on the dynamic
        // environment. These are a bit fraught, particularly if the behavior of
        // the external tools changes based on where a job gets dispatched, but
        // querying from the perspective of the current node allows
        // us to get better error messages in circumstances typical to a cluster.
        if let Some(queue) = &self.default_lsf_queue {
            queue.validate("default").await?;
        }

        if let Some(queue) = &self.short_task_lsf_queue {
            queue.validate("short_task").await?;
        }

        if let Some(queue) = &self.gpu_lsf_queue {
            queue.validate("gpu").await?;
        }

        if let Some(queue) = &self.fpga_lsf_queue {
            queue.validate("fpga").await?;
        }

        if let Some(prefix) = &self.job_name_prefix
            && prefix.len() > MAX_LSF_JOB_NAME_PREFIX
        {
            bail!(
                "LSF job name prefix `{prefix}` exceeds the maximum {MAX_LSF_JOB_NAME_PREFIX} \
                 bytes"
            );
        }

        self.apptainer_config.validate().await?;

        Ok(())
    }

    /// Get the appropriate LSF queue for a task under this configuration.
    ///
    /// Specialized hardware requirements are prioritized over other
    /// characteristics, with FPGA taking precedence over GPU.
    pub(crate) fn lsf_queue_for_task(
        &self,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Option<&LsfQueueConfig> {
        // Specialized hardware gets priority.
        if let Some(queue) = self.fpga_lsf_queue.as_ref()
            && let Some(true) = requirements
                .get(wdl_ast::v1::TASK_REQUIREMENT_FPGA)
                .and_then(Value::as_boolean)
        {
            return Some(queue);
        }

        if let Some(queue) = self.gpu_lsf_queue.as_ref()
            && let Some(true) = requirements
                .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
                .and_then(Value::as_boolean)
        {
            return Some(queue);
        }

        // Then short tasks.
        if let Some(queue) = self.short_task_lsf_queue.as_ref()
            && let Some(true) = hints
                .get(wdl_ast::v1::TASK_HINT_SHORT_TASK)
                .and_then(Value::as_boolean)
        {
            return Some(queue);
        }

        // Finally the default queue. If this is `None`, `bsub` gets run without a queue
        // argument and the cluster's default is used.
        self.default_lsf_queue.as_ref()
    }
}

/// Configuration for a Slurm partition.
///
/// Each partition can optionally have per-task CPU and memory limits set so
/// that tasks which are too large to be scheduled on that partition will fail
/// immediately instead of pending indefinitely. In the future, these limits may
/// be populated or validated by live information from the cluster, but
/// for now they must be manually based on the user's understanding of the
/// cluster configuration.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct SlurmPartitionConfig {
    /// The name of the partition; this is the string passed to `sbatch
    /// --partition=<partition_name>`.
    pub name: String,
    /// The maximum number of CPUs this partition can provision for a single
    /// task.
    pub max_cpu_per_task: Option<u64>,
    /// The maximum memory this partition can provision for a single task.
    pub max_memory_per_task: Option<ByteSize>,
}

impl SlurmPartitionConfig {
    /// Validate that this Slurm partition exists according to the local
    /// `sinfo`.
    pub async fn validate(&self, name: &str) -> Result<(), anyhow::Error> {
        let partition = &self.name;
        ensure!(
            !partition.is_empty(),
            "{name}_slurm_partition name cannot be empty"
        );
        if let Some(max_cpu_per_task) = self.max_cpu_per_task {
            ensure!(
                max_cpu_per_task > 0,
                "{name}_slurm_partition `{partition}` must allow at least 1 CPU to be provisioned"
            );
        }
        if let Some(max_memory_per_task) = self.max_memory_per_task {
            ensure!(
                max_memory_per_task.as_u64() > 0,
                "{name}_slurm_partition `{partition}` must allow at least some memory to be \
                 provisioned"
            );
        }
        match tokio::time::timeout(
            // 10 seconds is rather arbitrary; `scontrol` ordinarily returns extremely quickly, but
            // we don't want things to run away on a misconfigured system
            std::time::Duration::from_secs(10),
            Command::new("scontrol")
                .arg("show")
                .arg("partition")
                .arg(partition)
                .output(),
        )
        .await
        {
            Ok(output) => {
                let output = output.context("validating Slurm partition")?;
                if !output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!(%stdout, %stderr, %partition, "failed to validate {name}_slurm_partition");
                    Err(anyhow!(
                        "failed to validate {name}_slurm_partition `{partition}`"
                    ))
                } else {
                    Ok(())
                }
            }
            Err(_) => Err(anyhow!(
                "timed out trying to validate {name}_slurm_partition `{partition}`"
            )),
        }
    }
}

/// Configuration for the Slurm + Apptainer backend.
// TODO ACF 2025-09-23: add a Apptainer/Singularity mode config that switches around executable
// name, env var names, etc.
#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct SlurmApptainerBackendConfig {
    /// Which partition, if any, to specify when submitting normal jobs to
    /// Slurm.
    ///
    /// This may be superseded by
    /// [`short_task_slurm_partition`][Self::short_task_slurm_partition],
    /// [`gpu_slurm_partition`][Self::gpu_slurm_partition], or
    /// [`fpga_slurm_partition`][Self::fpga_slurm_partition] for corresponding
    /// tasks.
    pub default_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [short
    /// tasks](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#short_task) to Slurm.
    ///
    /// This may be superseded by
    /// [`gpu_slurm_partition`][Self::gpu_slurm_partition] or
    /// [`fpga_slurm_partition`][Self::fpga_slurm_partition] for tasks which
    /// require specialized hardware.
    pub short_task_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [tasks which require
    /// a GPU](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to Slurm.
    pub gpu_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [tasks which require
    /// an FPGA](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to Slurm.
    pub fpga_slurm_partition: Option<SlurmPartitionConfig>,
    /// Additional command-line arguments to pass to `sbatch` when submitting
    /// jobs to Slurm.
    pub extra_sbatch_args: Option<Vec<String>>,
    /// The configuration of Apptainer, which is used as the container runtime
    /// on the compute nodes where Slurm dispatches tasks.
    ///
    /// Note that this will likely be replaced by an abstraction over multiple
    /// container execution runtimes in the future, rather than being
    /// hardcoded to Apptainer.
    #[serde(default)]
    // TODO ACF 2025-10-16: temporarily flatten this into the overall config so that it doesn't
    // break existing serialized configs. We'll save breaking the config file format for when we
    // actually have meaningful composition of in-place runtimes.
    #[serde(flatten)]
    pub apptainer_config: ApptainerConfig,
}

impl SlurmApptainerBackendConfig {
    /// Validate that the backend is appropriately configured.
    pub async fn validate(&self, engine_config: &Config) -> Result<(), anyhow::Error> {
        if cfg!(not(unix)) {
            bail!("Slurm + Apptainer backend is not supported on non-unix platforms");
        }
        if !engine_config.experimental_features_enabled {
            bail!("Slurm + Apptainer backend requires enabling experimental features");
        }

        // Do what we can to validate options that are dependent on the dynamic
        // environment. These are a bit fraught, particularly if the behavior of
        // the external tools changes based on where a job gets dispatched, but
        // querying from the perspective of the current node allows
        // us to get better error messages in circumstances typical to a cluster.
        if let Some(partition) = &self.default_slurm_partition {
            partition.validate("default").await?;
        }
        if let Some(partition) = &self.short_task_slurm_partition {
            partition.validate("short_task").await?;
        }
        if let Some(partition) = &self.gpu_slurm_partition {
            partition.validate("gpu").await?;
        }
        if let Some(partition) = &self.fpga_slurm_partition {
            partition.validate("fpga").await?;
        }

        self.apptainer_config.validate().await?;

        Ok(())
    }

    /// Get the appropriate Slurm partition for a task under this configuration.
    ///
    /// Specialized hardware requirements are prioritized over other
    /// characteristics, with FPGA taking precedence over GPU.
    pub(crate) fn slurm_partition_for_task(
        &self,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Option<&SlurmPartitionConfig> {
        // TODO ACF 2025-09-26: what's the relationship between this code and
        // `TaskExecutionConstraints`? Should this be there instead, or be pulling
        // values from that instead of directly from `requirements` and `hints`?

        // Specialized hardware gets priority.
        if let Some(partition) = self.fpga_slurm_partition.as_ref()
            && let Some(true) = requirements
                .get(wdl_ast::v1::TASK_REQUIREMENT_FPGA)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        if let Some(partition) = self.gpu_slurm_partition.as_ref()
            && let Some(true) = requirements
                .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        // Then short tasks.
        if let Some(partition) = self.short_task_slurm_partition.as_ref()
            && let Some(true) = hints
                .get(wdl_ast::v1::TASK_HINT_SHORT_TASK)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        // Finally the default partition. If this is `None`, `sbatch` gets run without a
        // partition argument and the cluster's default is used.
        self.default_slurm_partition.as_ref()
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn redacted_secret() {
        let mut secret: SecretString = "secret".into();

        assert_eq!(
            serde_json::to_string(&secret).unwrap(),
            format!(r#""{REDACTED}""#)
        );

        secret.unredact();
        assert_eq!(serde_json::to_string(&secret).unwrap(), r#""secret""#);

        secret.redact();
        assert_eq!(
            serde_json::to_string(&secret).unwrap(),
            format!(r#""{REDACTED}""#)
        );
    }

    #[test]
    fn redacted_config() {
        let config = Config {
            backends: [
                (
                    "first".to_string(),
                    BackendConfig::Tes(TesBackendConfig {
                        auth: Some(TesBackendAuthConfig::Basic(BasicAuthConfig {
                            username: "foo".into(),
                            password: "secret".into(),
                        })),
                        ..Default::default()
                    }),
                ),
                (
                    "second".to_string(),
                    BackendConfig::Tes(TesBackendConfig {
                        auth: Some(TesBackendAuthConfig::Bearer(BearerAuthConfig {
                            token: "secret".into(),
                        })),
                        ..Default::default()
                    }),
                ),
            ]
            .into(),
            storage: StorageConfig {
                azure: AzureStorageConfig {
                    auth: Some(AzureStorageAuthConfig {
                        account_name: "foo".into(),
                        access_key: "secret".into(),
                    }),
                },
                s3: S3StorageConfig {
                    auth: Some(S3StorageAuthConfig {
                        access_key_id: "foo".into(),
                        secret_access_key: "secret".into(),
                    }),
                    ..Default::default()
                },
                google: GoogleStorageConfig {
                    auth: Some(GoogleStorageAuthConfig {
                        access_key: "foo".into(),
                        secret: "secret".into(),
                    }),
                },
            },
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("secret"), "`{json}` contains a secret");
    }

    #[tokio::test]
    async fn test_config_validate() {
        // Test invalid task config
        let mut config = Config::default();
        config.task.retries = Some(1000000);
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "configuration value `task.retries` cannot exceed 100"
        );

        // Test invalid scatter concurrency config
        let mut config = Config::default();
        config.workflow.scatter.concurrency = Some(0);
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "configuration value `workflow.scatter.concurrency` cannot be zero"
        );

        // Test invalid backend name
        let config = Config {
            backend: Some("foo".into()),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "a backend named `foo` is not present in the configuration"
        );
        let config = Config {
            backend: Some("bar".into()),
            backends: [("foo".to_string(), BackendConfig::default())].into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "a backend named `bar` is not present in the configuration"
        );

        // Test a singular backend
        let config = Config {
            backends: [("foo".to_string(), BackendConfig::default())].into(),
            ..Default::default()
        };
        config.validate().await.expect("config should validate");

        // Test invalid local backend cpu config
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Local(LocalBackendConfig {
                    cpu: Some(0),
                    ..Default::default()
                }),
            )]
            .into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "local backend configuration value `cpu` cannot be zero"
        );
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Local(LocalBackendConfig {
                    cpu: Some(10000000),
                    ..Default::default()
                }),
            )]
            .into(),
            ..Default::default()
        };
        assert!(
            config
                .validate()
                .await
                .unwrap_err()
                .to_string()
                .starts_with(
                    "local backend configuration value `cpu` cannot exceed the virtual CPUs \
                     available to the host"
                )
        );

        // Test invalid local backend memory config
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Local(LocalBackendConfig {
                    memory: Some("0 GiB".to_string()),
                    ..Default::default()
                }),
            )]
            .into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "local backend configuration value `memory` cannot be zero"
        );
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Local(LocalBackendConfig {
                    memory: Some("100 meows".to_string()),
                    ..Default::default()
                }),
            )]
            .into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "local backend configuration value `memory` has invalid value `100 meows`"
        );

        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Local(LocalBackendConfig {
                    memory: Some("1000 TiB".to_string()),
                    ..Default::default()
                }),
            )]
            .into(),
            ..Default::default()
        };
        assert!(
            config
                .validate()
                .await
                .unwrap_err()
                .to_string()
                .starts_with(
                    "local backend configuration value `memory` cannot exceed the total memory of \
                     the host"
                )
        );

        // Test missing TES URL
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Tes(Default::default()),
            )]
            .into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "TES backend configuration value `url` is required"
        );

        // Test TES invalid max concurrency
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Tes(TesBackendConfig {
                    url: Some("https://example.com".parse().unwrap()),
                    max_concurrency: Some(0),
                    ..Default::default()
                }),
            )]
            .into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "TES backend configuration value `max_concurrency` cannot be zero"
        );

        // Insecure TES URL
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Tes(TesBackendConfig {
                    url: Some("http://example.com".parse().unwrap()),
                    inputs: Some("http://example.com".parse().unwrap()),
                    outputs: Some("http://example.com".parse().unwrap()),
                    ..Default::default()
                }),
            )]
            .into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "TES backend configuration value `url` has invalid value `http://example.com/`: URL \
             must use a HTTPS scheme"
        );

        // Allow insecure URL
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Tes(TesBackendConfig {
                    url: Some("http://example.com".parse().unwrap()),
                    inputs: Some("http://example.com".parse().unwrap()),
                    outputs: Some("http://example.com".parse().unwrap()),
                    insecure: true,
                    ..Default::default()
                }),
            )]
            .into(),
            ..Default::default()
        };
        config
            .validate()
            .await
            .expect("configuration should validate");

        let mut config = Config::default();
        config.http.parallelism = Some(0);
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "configuration value `http.parallelism` cannot be zero"
        );

        let mut config = Config::default();
        config.http.parallelism = Some(5);
        assert!(
            config.validate().await.is_ok(),
            "should pass for valid configuration"
        );

        let mut config = Config::default();
        config.http.parallelism = None;
        assert!(
            config.validate().await.is_ok(),
            "should pass for default (None)"
        );

        // Test invalid LSF job name prefix
        #[cfg(unix)]
        {
            let job_name_prefix = "A".repeat(MAX_LSF_JOB_NAME_PREFIX * 2);
            let mut config = Config {
                experimental_features_enabled: true,
                ..Default::default()
            };
            config.backends.insert(
                "default".to_string(),
                BackendConfig::LsfApptainer(LsfApptainerBackendConfig {
                    job_name_prefix: Some(job_name_prefix.clone()),
                    ..Default::default()
                }),
            );
            assert_eq!(
                config.validate().await.unwrap_err().to_string(),
                format!("LSF job name prefix `{job_name_prefix}` exceeds the maximum 100 bytes")
            );
        }
    }
}
