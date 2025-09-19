//! Implementation of engine configuration.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use crankshaft::events::Event;
use indexmap::IndexMap;
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::warn;
use url::Url;

use crate::DockerBackend;
use crate::LocalBackend;
use crate::LsfApptainerBackend;
use crate::LsfApptainerBackendConfig;
use crate::SYSTEM;
use crate::TaskExecutionBackend;
use crate::TesBackend;
use crate::convert_unit_string;
use crate::path::is_url;

/// The inclusive maximum number of task retries the engine supports.
pub const MAX_RETRIES: u64 = 100;

/// The default task shell.
pub const DEFAULT_TASK_SHELL: &str = "bash";

/// The default backend name.
pub const DEFAULT_BACKEND_NAME: &str = "default";

/// The string that replaces redacted serialization fields.
const REDACTED: &str = "<REDACTED>";

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
}

impl Config {
    /// Validates the evaluation configuration.
    pub fn validate(&self) -> Result<()> {
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
            backend.validate()?;
        }

        self.storage.validate()?;
        Ok(())
    }

    /// Redacts the secrets contained in the configuration.
    ///
    /// By default, secrets are redacted for serialization.
    pub fn redact(&mut self) {
        for backend in self.backends.values_mut() {
            backend.redact();
        }

        self.storage.azure.redact();

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

        self.storage.azure.unredact();

        if let Some(auth) = &mut self.storage.s3.auth {
            auth.unredact();
        }

        if let Some(auth) = &mut self.storage.google.auth {
            auth.unredact();
        }
    }

    /// Creates a new task execution backend based on this configuration.
    pub async fn create_backend(
        self: &Arc<Self>,
        events: Option<broadcast::Sender<Event>>,
    ) -> Result<Arc<dyn TaskExecutionBackend>> {
        let config = if self.backend.is_none() && self.backends.len() < 2 {
            if self.backends.len() == 1 {
                // Use the singular entry
                Cow::Borrowed(self.backends.values().next().unwrap())
            } else {
                // Use the default
                Cow::Owned(BackendConfig::default())
            }
        } else {
            // Lookup the backend to use
            let backend = self.backend.as_deref().unwrap_or(DEFAULT_BACKEND_NAME);
            Cow::Borrowed(self.backends.get(backend).ok_or_else(|| {
                anyhow!("a backend named `{backend}` is not present in the configuration")
            })?)
        };

        match config.as_ref() {
            BackendConfig::Local(config) => {
                warn!(
                    "the engine is configured to use the local backend: tasks will not be run \
                     inside of a container"
                );
                Ok(Arc::new(LocalBackend::new(self.clone(), config, events)?))
            }
            BackendConfig::Docker(config) => Ok(Arc::new(
                DockerBackend::new(self.clone(), config, events).await?,
            )),
            BackendConfig::Tes(config) => Ok(Arc::new(
                TesBackend::new(self.clone(), config, events).await?,
            )),
            BackendConfig::LsfApptainer(config) => Ok(Arc::new(LsfApptainerBackend::new(
                self.clone(),
                config.clone(),
            ))),
        }
    }
}

/// Represents HTTP configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct HttpConfig {
    /// The HTTP download cache location.
    ///
    /// Defaults to using the system cache directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PathBuf>,
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

/// Represents configuration for Azure Blob Storage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AzureStorageConfig {
    /// The Azure Blob Storage authentication configuration.
    ///
    /// The key for the outer map is the Azure Storage account name.
    ///
    /// The key for the inner map is the Azure Storage container name.
    ///
    /// The value for the inner map is the SAS token to apply for requests to
    /// the Azure Storage container.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub auth: IndexMap<String, IndexMap<String, SecretString>>,
}

impl AzureStorageConfig {
    /// Validates the Azure Blob Storage configuration.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Redacts the secrets contained in the Azure Blob Storage configuration.
    pub fn redact(&mut self) {
        for v in self.auth.values_mut() {
            for v in v.values_mut() {
                v.redact();
            }
        }
    }

    /// Unredacts the secrets contained in the Azure Blob Storage configuration.
    pub fn unredact(&mut self) {
        for v in self.auth.values_mut() {
            for v in v.values_mut() {
                v.unredact();
            }
        }
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
    /// By default, the value is the parallelism supported by the task
    /// execution backend.
    ///
    /// A value of `0` is invalid.
    ///
    /// Lower values use less memory for evaluation and higher values may better
    /// saturate the task execution backend with tasks to execute.
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
    /// not be portable to other execution engines.</div>
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// The behavior when a task's `cpu` requirement cannot be met.
    #[serde(default)]
    pub cpu_limit_behavior: TaskResourceLimitBehavior,
    /// The behavior when a task's `memory` requirement cannot be met.
    #[serde(default)]
    pub memory_limit_behavior: TaskResourceLimitBehavior,
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
    Tes(Box<TesBackendConfig>),
    LsfApptainer(Arc<LsfApptainerBackendConfig>),
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self::Docker(Default::default())
    }
}

impl BackendConfig {
    /// Validates the backend configuration.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Local(config) => config.validate(),
            Self::Docker(config) => config.validate(),
            Self::Tes(config) => config.validate(),
            Self::LsfApptainer(config) => config.validate(),
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

    /// Redacts the secrets contained in the backend configuration.
    pub fn redact(&mut self) {
        match self {
            Self::Local(_) | Self::Docker(_) | Self::LsfApptainer(_) => {}
            Self::Tes(config) => config.redact(),
        }
    }

    /// Unredacts the secrets contained in the backend configuration.
    pub fn unredact(&mut self) {
        match self {
            Self::Local(_) | Self::Docker(_) | Self::LsfApptainer(_) => {}
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
    /// Defaults to 60 second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<u64>,

    /// The maximum task concurrency for the backend.
    ///
    /// Defaults to unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<u64>,

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

        match &self.inputs {
            Some(url) => {
                if !is_url(url.as_str()) {
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
                if !is_url(url.as_str()) {
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
                    BackendConfig::Tes(
                        TesBackendConfig {
                            auth: Some(TesBackendAuthConfig::Basic(BasicAuthConfig {
                                username: "foo".into(),
                                password: "secret".into(),
                            })),
                            ..Default::default()
                        }
                        .into(),
                    ),
                ),
                (
                    "second".to_string(),
                    BackendConfig::Tes(
                        TesBackendConfig {
                            auth: Some(TesBackendAuthConfig::Bearer(BearerAuthConfig {
                                token: "secret".into(),
                            })),
                            ..Default::default()
                        }
                        .into(),
                    ),
                ),
            ]
            .into(),
            storage: StorageConfig {
                azure: AzureStorageConfig {
                    auth: [("foo".into(), [("bar".into(), "secret".into())].into())].into(),
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

    #[test]
    fn test_config_validate() {
        // Test invalid task config
        let mut config = Config::default();
        config.task.retries = Some(1000000);
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "configuration value `task.retries` cannot exceed 100"
        );

        // Test invalid scatter concurrency config
        let mut config = Config::default();
        config.workflow.scatter.concurrency = Some(0);
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "configuration value `workflow.scatter.concurrency` cannot be zero"
        );

        // Test invalid backend name
        let config = Config {
            backend: Some("foo".into()),
            ..Default::default()
        };
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "a backend named `foo` is not present in the configuration"
        );
        let config = Config {
            backend: Some("bar".into()),
            backends: [("foo".to_string(), BackendConfig::default())].into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "a backend named `bar` is not present in the configuration"
        );

        // Test a singular backend
        let config = Config {
            backends: [("foo".to_string(), BackendConfig::default())].into(),
            ..Default::default()
        };
        config.validate().expect("config should validate");

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
            config.validate().unwrap_err().to_string(),
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
        assert!(config.validate().unwrap_err().to_string().starts_with(
            "local backend configuration value `cpu` cannot exceed the virtual CPUs available to \
             the host"
        ));

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
            config.validate().unwrap_err().to_string(),
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
            config.validate().unwrap_err().to_string(),
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
        assert!(config.validate().unwrap_err().to_string().starts_with(
            "local backend configuration value `memory` cannot exceed the total memory of the host"
        ));

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
            config.validate().unwrap_err().to_string(),
            "TES backend configuration value `url` is required"
        );

        // Insecure TES URL
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Tes(
                    TesBackendConfig {
                        url: Some("http://example.com".parse().unwrap()),
                        inputs: Some("http://example.com".parse().unwrap()),
                        outputs: Some("http://example.com".parse().unwrap()),
                        ..Default::default()
                    }
                    .into(),
                ),
            )]
            .into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "TES backend configuration value `url` has invalid value `http://example.com/`: URL \
             must use a HTTPS scheme"
        );

        // Allow insecure URL
        let config = Config {
            backends: [(
                "default".to_string(),
                BackendConfig::Tes(
                    TesBackendConfig {
                        url: Some("http://example.com".parse().unwrap()),
                        inputs: Some("http://example.com".parse().unwrap()),
                        outputs: Some("http://example.com".parse().unwrap()),
                        insecure: true,
                        ..Default::default()
                    }
                    .into(),
                ),
            )]
            .into(),
            ..Default::default()
        };
        config.validate().expect("configuration should validate");

        let mut config = Config::default();
        config.http.parallelism = Some(0);
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "configuration value `http.parallelism` cannot be zero"
        );

        let mut config = Config::default();
        config.http.parallelism = Some(5);
        assert!(
            config.validate().is_ok(),
            "should pass for valid configuration"
        );

        let mut config = Config::default();
        config.http.parallelism = None;
        assert!(config.validate().is_ok(), "should pass for default (None)");
    }
}
