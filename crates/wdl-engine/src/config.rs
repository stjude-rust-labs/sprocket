//! Implementation of engine configuration.

use std::borrow::Cow;
use std::fmt;
use std::fs;
use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::available_parallelism;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use bytesize::ByteSize;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::Buffer;
use indexmap::IndexMap;
use rowan::GreenNode;
use schemars::JsonSchema;
use secrecy::ExposeSecret;
use tokio::process::Command;
use toml_spanner::Arena;
use toml_spanner::ErrorKind;
use toml_spanner::Failed;
use toml_spanner::FromToml;
use toml_spanner::Item;
use toml_spanner::Table;
use toml_spanner::ToToml;
use toml_spanner::ToTomlError;
use toml_spanner::Toml;
use toml_spanner::helper::display;
use toml_spanner::helper::flatten_any;
use toml_spanner::helper::parse_string;
use tracing::error;
use tracing::warn;
use url::Url;
use wdl_analysis::Diagnostics;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::Exceptable;
use wdl_analysis::diagnostics::unknown_name;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::document::Task;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_analysis::types::v1::ExprTypeEvaluator;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::TreeNode;
use wdl_ast::lexer::Lexer;
use wdl_ast::v1::Expr;
use wdl_grammar::SyntaxKind;
use wdl_grammar::construct_tree;
use wdl_grammar::grammar::v1;
use wdl_grammar::grammar::v1::Parser;

use crate::CancellationContext;
use crate::EvaluationContext;
use crate::EvaluationPath;
use crate::Events;
use crate::NoneValue;
use crate::Object;
use crate::SYSTEM;
use crate::Value;
use crate::backend::ExecuteTaskRequest;
use crate::backend::TaskExecutionBackend;
use crate::convert_unit_string;
use crate::diagnostics::unknown_enum_choice;
use crate::http::Transferer;
use crate::path::is_supported_url;
use crate::tree::SyntaxNode;
use crate::v1::DEFAULT_TASK_REQUIREMENT_MAX_RETRIES;
use crate::v1::ExprEvaluator;

/// The inclusive maximum number of task retries the engine supports.
pub(crate) const MAX_RETRIES: u64 = 100;

/// The default task shell.
pub(crate) const DEFAULT_TASK_SHELL: &str = "bash";

/// The default task shell.
pub(crate) const fn default_task_shell() -> &'static str {
    DEFAULT_TASK_SHELL
}

/// The default task container.
pub(crate) const DEFAULT_TASK_CONTAINER: &str = "ubuntu:latest";

/// The default task container.
pub(crate) const fn default_task_container() -> &'static str {
    DEFAULT_TASK_CONTAINER
}

/// The default backend name.
const fn default_backend_name() -> &'static str {
    "default"
}

/// The maximum size, in bytes, for an LSF job name prefix.
const MAX_LSF_JOB_NAME_PREFIX: usize = 100;

/// The string that replaces redacted serialization fields.
const REDACTED: &str = "<REDACTED>";

/// Configuration sentinel value indicating use a system cache directory.
const fn cache_dir_sentinel() -> &'static str {
    "system"
}

/// The default for HTTP retries.
///
/// Same default as defined in `cloud_copy`
const fn default_http_retries() -> u32 {
    5
}

/// The default Apptainer executable name.
const fn default_apptainer_executable() -> &'static str {
    "apptainer"
}

/// The default number of elements to concurrently process for a scatter
/// statement.
const fn default_scatter_concurrency() -> u64 {
    1000
}

/// Gets the default root cache directory for the user.
pub(crate) fn cache_dir() -> Result<PathBuf> {
    /// The subdirectory within the user's cache directory for all caches
    const CACHE_DIR_ROOT: &str = "sprocket";

    Ok(dirs::cache_dir()
        .context("failed to determine user cache directory")?
        .join(CACHE_DIR_ROOT))
}

/// Creates a mapping of byte indexes in an unescaped TOML string to the
/// corresponding index in the escaped TOML string.
///
/// Only indexes that immediately follow an escape sequence are included in the
/// set.
///
/// All other mapping indexes can be synthesized by doing a binary search for
/// the unescaped index and either using the found entry's escaped index or
/// offset the unescaped index by the difference between the escaped and
/// unescaped indexes for the immediately preceding entry in the map at the
/// binary search insertion index (or zero if the insertion index is 0).
///
/// This is used as part of generating diagnostics for WDL expressions stored as
/// TOML strings.
///
/// The returned list is guaranteed sorted in both index spaces.
///
/// Note that if the string ends with an escape sequence, an additional mapping
/// of the exclusive end of the string will be included in the set.
///
/// # Panics
///
/// Panics if the TOML string contains invalid escape sequences.
///
/// Only use this function after the TOML has been validated.
///
/// # Examples
///
/// `foo\tbar` -> [(4, 5)]
/// `\"foo\" == \"bar\"` -> [(1, 2), (5, 7), (10, 13), (14, 18)]
fn escape_mapping(toml: &str) -> Vec<(usize, usize)> {
    let mut iter = toml.char_indices();
    let mut mapping = Vec::new();
    let mut new = 0;
    while let Some((old, c)) = iter.next() {
        if c != '\\' {
            new += c.len_utf8();
            continue;
        }

        match iter.next() {
            Some((_, 'u')) => {
                let c = u32::from_str_radix(&toml[old + 2 /* \u */..old + 6 /* \uXXXX */], 16)
                    .map(char::from_u32)
                    .expect("invalid TOML escape sequence")
                    .expect("invalid TOML escape character");
                new += c.len_utf8();

                // Move past the rest of the sequence
                iter.nth(3);
                mapping.push((new, old + 6 /* \uXXXX */));
            }
            Some((_, 'U')) => {
                let c = u32::from_str_radix(&toml[old + 2 /* \U */..old + 10 /* \UXXXXXXXX */], 16)
                    .map(char::from_u32)
                    .expect("invalid TOML escape sequence")
                    .expect("invalid TOML escape character");
                new += c.len_utf8();

                // Move past the rest of the sequence
                iter.nth(7);
                mapping.push((new, old + 10 /* \UXXXXXXXX */));
            }
            Some(_) => {
                // All other escape sequences are single byte replacements
                new += 1;
                mapping.push((new, old + 2 /* \? */));
            }
            None => break,
        }
    }

    mapping
}

/// Represents a secret string that is, by default, redacted for serialization.
///
/// This type is a wrapper around [`secrecy::SecretString`].
#[derive(Default, Debug, Clone, JsonSchema)]
#[schemars(with = "String")]
pub struct SecretString {
    /// The inner secret string.
    ///
    /// This type is not serializable.
    inner: secrecy::SecretString,
    /// Whether or not the secret string is redacted for serialization.
    ///
    /// If `true`, `<REDACTED>` is serialized for the string's value.
    ///
    /// If `false`, the inner secret string is exposed for serialization.
    ///
    /// Defaults to unredacted; users should call `redacted` on the [`Config`]
    /// prior to serialization.
    redacted: bool,
}

impl SecretString {
    /// Redacts the secret for serialization.
    ///
    /// By default, a [`SecretString`] is unredacted; when redacted, the string
    /// is replaced with `<REDACTED>` when serialized.
    pub fn redact(mut self) -> Self {
        self.redacted = true;
        self
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
            redacted: false,
        }
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self {
            inner: s.into(),
            redacted: false,
        }
    }
}

impl<'de> FromToml<'de> for SecretString {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        Ok(String::from_toml(ctx, item)?.into())
    }
}

impl ToToml for SecretString {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        use secrecy::ExposeSecret;

        if self.redacted {
            REDACTED.to_toml(arena)
        } else {
            self.inner.expose_secret().to_toml(arena)
        }
    }
}

impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        use secrecy::ExposeSecret;

        // Compare just on the string, ignoring the redaction flag
        self.inner.expose_secret() == other.inner.expose_secret()
    }
}

impl Eq for SecretString {}

/// Represents how an evaluation error or cancellation should be handled by the
/// engine.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
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

/// Helper functions for `IndexMap` TOML serialization.
mod index_map {
    use indexmap::IndexMap;
    use toml_spanner::Arena;
    use toml_spanner::Context;
    use toml_spanner::Failed;
    use toml_spanner::FromToml;
    use toml_spanner::Item;
    use toml_spanner::Key;
    use toml_spanner::Table;
    use toml_spanner::TableStyle;
    use toml_spanner::ToToml;
    use toml_spanner::ToTomlError;

    /// Helper function for serializing an `IndexMap` to TOML.
    pub fn from_toml<'de, V>(
        ctx: &mut Context<'de>,
        item: &Item<'de>,
    ) -> Result<IndexMap<String, V>, Failed>
    where
        V: FromToml<'de>,
    {
        let table = item.require_table(ctx)?;
        let mut map = IndexMap::default();
        let mut had_error = false;
        for (key, item) in table {
            match V::from_toml(ctx, item) {
                Ok(v) => {
                    map.insert(key.name.into(), v);
                }
                Err(_) => had_error = true,
            }
        }

        if had_error { Err(Failed) } else { Ok(map) }
    }

    /// Helper function for deserializing an `IndexMap` from TOML.
    pub fn to_toml<'a, V>(
        value: &'a IndexMap<String, V>,
        arena: &'a Arena,
    ) -> Result<Item<'a>, ToTomlError>
    where
        V: ToToml,
    {
        let Some(mut table) = Table::try_with_capacity(value.len(), arena) else {
            return Err(ToTomlError::from(
                "length of table exceeded maximum capacity",
            ));
        };

        table.set_style(TableStyle::Implicit);

        for (k, v) in value {
            table.insert_unique(Key::new(k), v.to_toml(arena)?, arena);
        }

        Ok(table.into_item())
    }
}

/// Represents WDL evaluation configuration.
///
/// <div class="warning">
///
/// By default, serialization of [`Config`] will not redact the values of
/// secrets.
///
/// Use the [`Config::redact`] method prior to serialization to redact secrets.
///
/// </div>
#[derive(Debug, Clone, Toml, PartialEq, Eq, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(
    rename = "WdlEngineConfig",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub struct Config {
    /// HTTP configuration.
    #[toml(default, style = Header)]
    #[schemars(default)]
    pub http: HttpConfig,
    /// Workflow evaluation configuration.
    #[toml(default, style = Header)]
    #[schemars(default)]
    pub workflow: WorkflowConfig,
    /// Task evaluation configuration.
    #[toml(default, style = Header)]
    #[schemars(default)]
    pub task: TaskConfig,
    /// The name of the backend to use.
    #[toml(default = String::from(default_backend_name()))]
    #[schemars(default = "default_backend_name")]
    pub backend: String,
    /// Task execution backends configuration.
    ///
    /// If the collection is empty and `backend` has the default value, the
    /// engine default backend is used.
    #[toml(default, with = index_map)]
    #[schemars(default)]
    pub backends: IndexMap<String, BackendConfig>,
    /// Storage configuration.
    #[toml(default, style = Header)]
    #[schemars(default)]
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
    #[toml(default)]
    #[schemars(default)]
    pub suppress_env_specific_output: bool,
    /// (Experimental) Whether experimental features are enabled; default is
    /// `false`.
    ///
    /// Experimental features are provided to users with heavy caveats about
    /// their stability and rough edges. Use at your own risk, but feedback
    /// is quite welcome.
    #[toml(default)]
    #[schemars(default)]
    pub experimental_features_enabled: bool,
    /// The failure mode for workflow or task evaluation.
    ///
    /// A value of [`FailureMode::Slow`] will result in evaluation waiting for
    /// executing tasks to complete upon error or interruption.
    ///
    /// A value of [`FailureMode::Fast`] will immediately attempt to cancel
    /// executing tasks upon error or interruption.
    #[toml(default, rename = "fail")]
    #[schemars(default, rename = "fail")]
    pub failure_mode: FailureMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            http: Default::default(),
            workflow: Default::default(),
            task: Default::default(),
            backend: default_backend_name().into(),
            backends: Default::default(),
            storage: Default::default(),
            suppress_env_specific_output: Default::default(),
            experimental_features_enabled: Default::default(),
            failure_mode: Default::default(),
        }
    }
}

impl Config {
    /// Gets a builder for [`Config`].
    pub fn builder() -> ConfigBuilder<Self> {
        ConfigBuilder::default()
    }

    /// Validates the evaluation configuration.
    pub async fn validate(&self) -> Result<()> {
        self.http.validate()?;
        self.workflow.validate()?;
        self.task.validate()?;

        if self.backends.is_empty() && self.backend == default_backend_name() {
            // we'll use the default
        } else {
            let backend = &self.backend;
            if !self.backends.contains_key(backend) {
                bail!("a backend named `{backend}` is not present in the configuration");
            }
        }

        for backend in self.backends.values() {
            backend.validate().await?;
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
    pub fn redact(mut self) -> Self {
        for backend in self.backends.values_mut() {
            *backend = std::mem::take(backend).redact();
        }

        if let Some(auth) = self.storage.azure.auth.take() {
            self.storage.azure.auth = Some(auth.redact());
        }

        if let Some(auth) = self.storage.s3.auth.take() {
            self.storage.s3.auth = Some(auth.redact());
        }

        if let Some(auth) = self.storage.google.auth.take() {
            self.storage.google.auth = Some(auth.redact());
        }

        self
    }

    /// Gets the backend configuration.
    ///
    /// Returns an error if the configuration specifies a named backend that
    /// isn't present in the configuration.
    pub fn backend(&self) -> Result<Cow<'_, BackendConfig>> {
        if !self.backends.is_empty() {
            let backend = &self.backend;
            return Ok(Cow::Borrowed(self.backends.get(backend).ok_or_else(
                || anyhow!("a backend named `{backend}` is not present in the configuration"),
            )?));
        }
        // Use the default
        Ok(Cow::Owned(BackendConfig::default()))
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
            BackendConfig::Local { .. } => {
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
            BackendConfig::Docker { .. } => Ok(Arc::new(
                DockerBackend::new(self.clone(), events, cancellation).await?,
            )),
            BackendConfig::Tes { .. } => Ok(Arc::new(
                TesBackend::new(self.clone(), events, cancellation).await?,
            )),
            BackendConfig::LsfApptainer { .. } => Ok(Arc::new(LsfApptainerBackend::new(
                self.clone(),
                run_root_dir,
                events,
                cancellation,
            )?)),
            BackendConfig::SlurmApptainer { .. } => Ok(Arc::new(SlurmApptainerBackend::new(
                self.clone(),
                run_root_dir,
                events,
                cancellation,
            )?)),
        }
    }
}

/// Represents the parallelism for HTTP downloads.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, JsonSchema)]
#[schemars(rename_all = "lowercase")]
pub enum Parallelism {
    /// Use the available parallelism for the host system.
    #[default]
    Available,
    /// Use the specified parallelism.
    #[schemars(untagged)]
    Use(usize),
}

impl From<usize> for Parallelism {
    fn from(value: usize) -> Self {
        Self::Use(value)
    }
}

impl From<Parallelism> for usize {
    fn from(value: Parallelism) -> Self {
        match value {
            Parallelism::Available => available_parallelism().map(Into::into).unwrap_or(1),
            Parallelism::Use(value) => value,
        }
    }
}

impl<'de> FromToml<'de> for Parallelism {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        if let Some("available") = item.as_str() {
            return Ok(Self::Available);
        }

        if let Some(n) = item.as_u64().and_then(|n| usize::try_from(n).ok())
            && n > 0
        {
            return Ok(Self::Use(n));
        }

        Err(ctx.report_custom_error(
            "expected a positive integer or `available` for parallelism",
            item,
        ))
    }
}

impl ToToml for Parallelism {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        match self {
            Self::Available => Ok(Item::string("available")),
            Self::Use(n) => Ok(i64::try_from(*n)
                .map_err(|e| ToTomlError {
                    message: format!("invalid parallelism: {e}").into(),
                })?
                .into()),
        }
    }
}

/// Represents HTTP configuration.
#[derive(Debug, Clone, Toml, PartialEq, Eq, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct HttpConfig {
    /// The HTTP download cache location.
    ///
    /// Defaults to an operating system specific cache directory for the user.
    #[toml(default = String::from(cache_dir_sentinel()))]
    #[schemars(default = "cache_dir_sentinel")]
    pub cache_dir: String,
    /// The number of retries for transferring files.
    #[toml(default = default_http_retries())]
    #[schemars(default = "default_http_retries")]
    pub retries: u32,
    /// The maximum parallelism for file transfers.
    ///
    /// Defaults to the host's available parallelism.
    #[toml(default)]
    #[schemars(default)]
    pub parallelism: Parallelism,
    /// The hash algorithm to use for calculating content digests for file
    /// uploads.
    ///
    /// Defaults to `sha256`.
    #[toml(default, FromToml with = parse_string, ToToml with = display)]
    #[schemars(default, with = "String")]
    pub hash_algorithm: cloud_copy::HashAlgorithm,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            cache_dir: cache_dir_sentinel().into(),
            retries: default_http_retries(),
            parallelism: Default::default(),
            hash_algorithm: Default::default(),
        }
    }
}

impl HttpConfig {
    /// Validates the HTTP configuration.
    pub fn validate(&self) -> Result<()> {
        if let Parallelism::Use(parallelism) = self.parallelism
            && parallelism == 0
        {
            bail!("configuration value `http.parallelism` cannot be zero");
        }
        Ok(())
    }

    /// Get the HTTP cache dir.
    pub fn cache_dir(&self) -> Result<PathBuf> {
        const DOWNLOADS_CACHE_SUBDIR: &str = "downloads";

        if self.using_system_cache_dir() {
            cache_dir().map(|d| d.join(DOWNLOADS_CACHE_SUBDIR))
        } else {
            Ok(PathBuf::from(&self.cache_dir))
        }
    }

    /// Is this configuration using a system cache dir?
    pub fn using_system_cache_dir(&self) -> bool {
        self.cache_dir == cache_dir_sentinel()
    }
}

/// Represents storage configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct StorageConfig {
    /// Azure Blob Storage configuration.
    #[toml(default, style = Header)]
    #[schemars(default)]
    pub azure: AzureStorageConfig,
    /// AWS S3 configuration.
    #[toml(default, style = Header)]
    #[schemars(default)]
    pub s3: S3StorageConfig,
    /// Google Cloud Storage configuration.
    #[toml(default, style = Header)]
    #[schemars(default)]
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
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
    pub fn redact(mut self) -> Self {
        self.access_key = self.access_key.redact();
        self
    }
}

/// Represents configuration for Azure Blob Storage.
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct AzureStorageConfig {
    /// The Azure Blob Storage authentication configuration.
    #[toml(style = Header)]
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
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
    pub fn redact(mut self) -> Self {
        self.secret_access_key = self.secret_access_key.redact();
        self
    }
}

/// Represents configuration for AWS S3 storage.
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct S3StorageConfig {
    /// The default region to use for S3-schemed URLs (e.g.
    /// `s3://<bucket>/<blob>`).
    ///
    /// Defaults to `us-east-1`.
    pub region: Option<String>,

    /// The AWS S3 storage authentication configuration.
    #[toml(style = Header)]
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
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
    pub fn redact(mut self) -> Self {
        self.secret = self.secret.redact();
        self
    }
}

/// Represents configuration for Google Cloud Storage.
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct GoogleStorageConfig {
    /// The Google Cloud Storage authentication configuration.
    #[toml(style = Header)]
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct WorkflowConfig {
    /// Scatter statement evaluation configuration.
    #[toml(default, style = Header)]
    #[schemars(default)]
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
#[derive(Debug, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
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
    #[toml(default = default_scatter_concurrency())]
    #[schemars(default = "default_scatter_concurrency")]
    pub concurrency: u64,
}

impl Default for ScatterConfig {
    fn default() -> Self {
        Self {
            concurrency: default_scatter_concurrency(),
        }
    }
}

impl ScatterConfig {
    /// Validates the scatter configuration.
    pub fn validate(&self) -> Result<()> {
        if self.concurrency == 0 {
            bail!("configuration value `workflow.scatter.concurrency` cannot be zero");
        }

        Ok(())
    }
}

/// Represents the supported call caching modes.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
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
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum ContentDigestMode {
    /// Use a strong digest for file content.
    ///
    /// Strong digests require hashing all of the contents of a file; this may
    /// noticeably impact performance for very large files.
    ///
    /// This setting guarantees that a modified file will be detected.
    Strong,
    /// Use a "strongish" digest for file content.
    ///
    /// A strongish digest is based off of the file's size, last modified
    /// time, and a hash of only the first 10 MiB of the file's contents;
    /// this is similar to Cromwell's `fingerprint` call caching strategy.
    ///
    /// This setting cannot guarantee the detection of modified files (e.g. a
    /// modification beyond the first 10 MiB of a file without a change to
    /// its size or last modified time will not be detected), but it is
    /// faster than using a strong digest for large files while still taking
    /// file content into account.
    Strongish,
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

/// Represents the maximum number of retries for tasks.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(rename_all = "lowercase")]
pub enum Retries {
    /// Use the default number of retries for task execution.
    #[default]
    Default,
    /// Use the specified number of retries for task execution.
    #[schemars(untagged)]
    Use(u64),
}

impl From<u64> for Retries {
    fn from(value: u64) -> Self {
        Self::Use(value)
    }
}

impl From<Retries> for u64 {
    fn from(value: Retries) -> Self {
        match value {
            Retries::Default => DEFAULT_TASK_REQUIREMENT_MAX_RETRIES,
            Retries::Use(value) => value,
        }
    }
}

impl<'de> FromToml<'de> for Retries {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        if let Some("default") = item.as_str() {
            return Ok(Self::Default);
        }

        if let Some(n) = item.as_u64()
            && n < MAX_RETRIES
        {
            return Ok(Self::Use(n));
        }

        Err(ctx.report_custom_error(
            format!("expected an integer less than {MAX_RETRIES} or `default` for retries"),
            item,
        ))
    }
}

impl ToToml for Retries {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        match self {
            Self::Default => Ok(Item::string("default")),
            Self::Use(n) => Ok(i64::try_from(*n)
                .map_err(|e| ToTomlError {
                    message: format!("invalid retries: {e}").into(),
                })?
                .into()),
        }
    }
}

/// Represents task evaluation configuration.
#[derive(Debug, Clone, Toml, PartialEq, Eq, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct TaskConfig {
    /// The default maximum number of retries to attempt if a task fails.
    ///
    /// A task's `max_retries` requirement will override this value.
    #[toml(default)]
    #[schemars(default)]
    pub retries: Retries,
    /// The default container to use if a container is not specified in a task's
    /// requirements.
    #[toml(default = String::from(default_task_container()))]
    #[schemars(default = "default_task_container")]
    pub container: String,
    /// The default shell to use for tasks.
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
    #[toml(default = String::from(default_task_shell()))]
    #[schemars(default = "default_task_shell")]
    pub shell: String,
    /// The behavior when a task's `cpu` requirement cannot be met.
    #[toml(default)]
    #[schemars(default)]
    pub cpu_limit_behavior: TaskResourceLimitBehavior,
    /// The behavior when a task's `memory` requirement cannot be met.
    #[toml(default)]
    #[schemars(default)]
    pub memory_limit_behavior: TaskResourceLimitBehavior,
    /// The call cache directory to use for caching task execution results.
    ///
    /// Defaults to an operating system specific cache directory for the user.
    #[toml(default = String::from(cache_dir_sentinel()))]
    #[schemars(default = "cache_dir_sentinel")]
    pub cache_dir: String,
    /// The call caching mode to use for tasks.
    #[toml(default)]
    #[schemars(default)]
    pub cache: CallCachingMode,
    /// The content digest mode to use.
    ///
    /// Used as part of call caching.
    #[toml(default)]
    #[schemars(default)]
    pub digests: ContentDigestMode,
    /// Keys of task requirements to exclude from call cache checking.
    ///
    /// When specified, these requirement keys will be ignored when
    /// calculating cache keys and validating cache entries.
    ///
    /// This can be useful for requirements that may vary between runs
    /// but should not invalidate the cache (e.g., dynamic resource
    /// allocation).
    #[toml(default)]
    #[schemars(default)]
    pub excluded_cache_requirements: Vec<String>,
    /// Keys of task hints to exclude from call cache checking.
    ///
    /// When specified, these hint keys will be ignored when
    /// calculating cache keys and validating cache entries.
    ///
    /// This can be useful for hints that may vary between runs
    /// but should not invalidate the cache.
    #[toml(default)]
    #[schemars(default)]
    pub excluded_cache_hints: Vec<String>,
    /// Keys of task inputs to exclude from call cache checking.
    ///
    /// When specified, these input keys will be ignored when
    /// calculating cache keys and validating cache entries.
    ///
    /// This can be useful for inputs that may vary between runs
    /// but should not affect the task's output.
    #[toml(default)]
    #[schemars(default)]
    pub excluded_cache_inputs: Vec<String>,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            retries: Default::default(),
            container: default_task_container().into(),
            shell: default_task_shell().into(),
            cpu_limit_behavior: Default::default(),
            memory_limit_behavior: Default::default(),
            cache_dir: cache_dir_sentinel().into(),
            cache: Default::default(),
            digests: Default::default(),
            excluded_cache_requirements: Default::default(),
            excluded_cache_hints: Default::default(),
            excluded_cache_inputs: Default::default(),
        }
    }
}

impl TaskConfig {
    /// Validates the task evaluation configuration.
    pub fn validate(&self) -> Result<()> {
        if let Retries::Use(value) = self.retries
            && value >= MAX_RETRIES
        {
            bail!("configuration value `task.retries` cannot exceed {MAX_RETRIES}");
        }

        Ok(())
    }

    /// Get the configured cache dir if it is set.
    pub fn cache_dir(&self) -> Option<PathBuf> {
        if self.cache_dir == cache_dir_sentinel() {
            None
        } else {
            Some(PathBuf::from(&self.cache_dir))
        }
    }
}

/// The behavior when a task resource requirement, such as `cpu` or `memory`,
/// cannot be met.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
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
#[derive(Debug, Clone, Toml, PartialEq, Eq, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", tag = "type")]
#[schemars(rename_all = "snake_case", tag = "type")]
pub enum BackendConfig {
    /// Use the local task execution backend.
    Local {
        /// The inner local backend configuration.
        #[toml(default, style = Header, flatten, with = flatten_any)]
        #[schemars(default, flatten)]
        config: LocalBackendConfig,
    },
    /// Use the Docker task execution backend.
    Docker {
        /// The inner Docker backend configuration.
        #[toml(default, style = Header, flatten, with = flatten_any)]
        #[schemars(default, flatten)]
        config: DockerBackendConfig,
    },
    /// Use the TES task execution backend.
    Tes {
        /// The inner TES backend configuration.
        #[toml(default, style = Header, flatten, with = flatten_any)]
        #[schemars(default, flatten)]
        config: TesBackendConfig,
    },
    /// Use the experimental LSF + Apptainer task execution backend.
    ///
    /// Requires enabling experimental features.
    LsfApptainer {
        /// The inner LSF Apptainer backend configuration.
        #[toml(default, style = Header, flatten, with = flatten_any)]
        #[schemars(default, flatten)]
        config: LsfApptainerBackendConfig,
    },
    /// Use the experimental Slurm + Apptainer task execution backend.
    ///
    /// Requires enabling experimental features.
    SlurmApptainer {
        /// The inner Slurm Apptainer backend configuration.
        #[toml(default, style = Header, flatten, with = flatten_any)]
        #[schemars(default, flatten)]
        config: SlurmApptainerBackendConfig,
    },
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self::Docker {
            config: Default::default(),
        }
    }
}

impl BackendConfig {
    /// Validates the backend configuration.
    pub async fn validate(&self) -> Result<()> {
        match self {
            Self::Local { config } => config.validate(),
            Self::Docker { config } => config.validate(),
            Self::Tes { config } => config.validate(),
            Self::LsfApptainer { config } => config.validate().await,
            Self::SlurmApptainer { config } => config.validate().await,
        }
    }

    /// Converts the backend configuration into a local backend configuration
    ///
    /// Returns `None` if the backend configuration is not local.
    pub fn as_local(&self) -> Option<&LocalBackendConfig> {
        match self {
            Self::Local { config } => Some(config),
            _ => None,
        }
    }

    /// Converts the backend configuration into a Docker backend configuration
    ///
    /// Returns `None` if the backend configuration is not Docker.
    pub fn as_docker(&self) -> Option<&DockerBackendConfig> {
        match self {
            Self::Docker { config } => Some(config),
            _ => None,
        }
    }

    /// Converts the backend configuration into a TES backend configuration
    ///
    /// Returns `None` if the backend configuration is not TES.
    pub fn as_tes(&self) -> Option<&TesBackendConfig> {
        match self {
            Self::Tes { config } => Some(config),
            _ => None,
        }
    }

    /// Converts the backend configuration into a LSF Apptainer backend
    /// configuration
    ///
    /// Returns `None` if the backend configuration is not LSF Apptainer.
    pub fn as_lsf_apptainer(&self) -> Option<&LsfApptainerBackendConfig> {
        match self {
            Self::LsfApptainer { config } => Some(config),
            _ => None,
        }
    }

    /// Converts the backend configuration into a Slurm Apptainer backend
    /// configuration
    ///
    /// Returns `None` if the backend configuration is not Slurm Apptainer.
    pub fn as_slurm_apptainer(&self) -> Option<&SlurmApptainerBackendConfig> {
        match self {
            Self::SlurmApptainer { config } => Some(config),
            _ => None,
        }
    }

    /// Redacts the secrets contained in the backend configuration.
    pub fn redact(self) -> Self {
        match self {
            Self::Local { .. }
            | Self::Docker { .. }
            | Self::LsfApptainer { .. }
            | Self::SlurmApptainer { .. } => self,
            Self::Tes { config } => Self::Tes {
                config: config.redact(),
            },
        }
    }
}

impl From<LocalBackendConfig> for BackendConfig {
    fn from(config: LocalBackendConfig) -> Self {
        Self::Local { config }
    }
}

impl From<DockerBackendConfig> for BackendConfig {
    fn from(config: DockerBackendConfig) -> Self {
        Self::Docker { config }
    }
}

impl From<TesBackendConfig> for BackendConfig {
    fn from(config: TesBackendConfig) -> Self {
        Self::Tes { config }
    }
}

impl From<LsfApptainerBackendConfig> for BackendConfig {
    fn from(config: LsfApptainerBackendConfig) -> Self {
        Self::LsfApptainer { config }
    }
}

impl From<SlurmApptainerBackendConfig> for BackendConfig {
    fn from(config: SlurmApptainerBackendConfig) -> Self {
        Self::SlurmApptainer { config }
    }
}

/// Represents configuration for the local task execution backend.
///
/// <div class="warning">
/// Warning: the local task execution backend spawns processes on the host
/// directly without the use of a container; only use this backend on trusted
/// WDL. </div>
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct LocalBackendConfig {
    /// Set the number of CPUs available for task execution.
    ///
    /// Defaults to the number of logical CPUs for the host.
    ///
    /// The value cannot be zero or exceed the host's number of CPUs.
    pub cpu: Option<u64>,

    /// Set the total amount of memory for task execution as a unit string (e.g.
    /// `2 GiB`).
    ///
    /// Defaults to the total amount of memory for the host.
    ///
    /// The value cannot be zero or exceed the host's total amount of memory.
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

/// The default value for the `cleanup` field in [`DockerBackendConfig`].
fn default_docker_cleanup() -> bool {
    true
}

/// Represents configuration for the Docker backend.
#[derive(Debug, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct DockerBackendConfig {
    /// Whether or not to remove a task's container after the task completes.
    ///
    /// Defaults to `true`.
    #[toml(default = default_docker_cleanup())]
    #[schemars(default = "default_docker_cleanup")]
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
        Self {
            cleanup: default_docker_cleanup(),
        }
    }
}

/// Represents HTTP basic authentication configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct BasicAuthConfig {
    /// The HTTP basic authentication username.
    pub username: String,
    /// The HTTP basic authentication password.
    pub password: SecretString,
}

impl BasicAuthConfig {
    /// Validates the HTTP basic auth configuration.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Redacts the secrets contained in the HTTP basic auth configuration.
    pub fn redact(mut self) -> Self {
        self.password = self.password.redact();
        self
    }
}

/// Represents HTTP bearer token authentication configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct BearerAuthConfig {
    /// The HTTP bearer authentication token.
    pub token: SecretString,
}

impl BearerAuthConfig {
    /// Validates the HTTP bearer auth configuration.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Redacts the secrets contained in the HTTP bearer auth configuration.
    pub fn redact(mut self) -> Self {
        self.token = self.token.redact();
        self
    }
}

/// Represents the kind of authentication for a TES backend.
#[derive(Debug, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", tag = "type")]
#[schemars(rename_all = "snake_case", tag = "type")]
pub enum TesBackendAuthConfig {
    /// Use basic authentication for the TES backend.
    Basic {
        /// The inner basic auth configuration.
        #[toml(default, style = Header, flatten, with = flatten_any)]
        #[schemars(default, flatten)]
        config: BasicAuthConfig,
    },
    /// Use bearer token authentication for the TES backend.
    Bearer {
        /// The inner bearer auth configuration.
        #[toml(default, style = Header, flatten, with = flatten_any)]
        #[schemars(default, flatten)]
        config: BearerAuthConfig,
    },
}

impl TesBackendAuthConfig {
    /// Validates the TES backend authentication configuration.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Basic { config } => config.validate(),
            Self::Bearer { config } => config.validate(),
        }
    }

    /// Redacts the secrets contained in the TES backend authentication
    /// configuration.
    pub fn redact(self) -> Self {
        match self {
            Self::Basic { config } => Self::Basic {
                config: config.redact(),
            },
            Self::Bearer { config } => Self::Bearer {
                config: config.redact(),
            },
        }
    }
}

impl From<BasicAuthConfig> for TesBackendAuthConfig {
    fn from(config: BasicAuthConfig) -> Self {
        Self::Basic { config }
    }
}

impl From<BearerAuthConfig> for TesBackendAuthConfig {
    fn from(config: BearerAuthConfig) -> Self {
        Self::Bearer { config }
    }
}

/// Represents configuration for the Task Execution Service (TES) backend.
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct TesBackendConfig {
    /// The URL of the Task Execution Service.
    #[toml(FromToml with = parse_string, ToToml with = display)]
    pub url: Option<Url>,

    /// The authentication configuration for the TES backend.
    #[toml(style = Header)]
    pub auth: Option<TesBackendAuthConfig>,

    /// The root cloud storage URL for storing inputs.
    #[toml(FromToml with = parse_string, ToToml with = display)]
    pub inputs: Option<Url>,

    /// The root cloud storage URL for storing outputs.
    #[toml(FromToml with = parse_string, ToToml with = display)]
    pub outputs: Option<Url>,

    /// The polling interval, in seconds, for checking task status.
    ///
    /// Defaults to 1 second.
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
    pub max_concurrency: Option<u32>,

    /// Whether or not the TES server URL may use an insecure protocol like
    /// HTTP.
    #[toml(default)]
    #[schemars(default)]
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
    pub fn redact(mut self) -> Self {
        if let Some(auth) = self.auth.take() {
            self.auth = Some(auth.redact());
        }

        self
    }
}

/// Configuration for the Apptainer container runtime.
#[derive(Debug, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct ApptainerConfig {
    /// Path to the Apptainer (or Singularity) executable.
    ///
    /// Defaults to `"apptainer"`. Set to `"singularity"` or a full path
    /// (e.g., `/usr/local/bin/apptainer`) if the executable is not on `PATH`
    /// or if using Singularity instead.
    #[toml(default = String::from(default_apptainer_executable()))]
    #[schemars(default = "default_apptainer_executable")]
    pub executable: String,

    /// Path to a shared directory for caching pulled `.sif` images.
    ///
    /// When set, pulled images are stored in this directory and shared
    /// across runs. When unset, images are stored in a per-run directory
    /// that is not shared.
    pub image_cache_dir: Option<PathBuf>,

    /// Additional command-line arguments to pass to `apptainer exec` when
    /// executing tasks.
    #[toml(default)]
    #[schemars(default)]
    pub extra_args: Vec<String>,
}

impl Default for ApptainerConfig {
    fn default() -> Self {
        Self {
            executable: default_apptainer_executable().into(),
            image_cache_dir: None,
            extra_args: Default::default(),
        }
    }
}

impl ApptainerConfig {
    /// Validate that Apptainer is appropriately configured.
    pub async fn validate(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

/// Represents a condition in a conditional argument.
///
/// The expression is evaluated in a context where a task's computed `cpu`,
/// `memory`, `gpu`, `fpga`, and `disks` values and evaluated `hint` object are
/// available as variables.
///
/// The expression is type checked during configuration deserialization to
/// ensure it is a valid WDL expression of type `Boolean`.
#[derive(Debug, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(with = "String")]
pub struct Condition {
    /// The raw WDL expression string.
    pub raw: String,
    /// The parsed and validated conditional expression.
    expr: GreenNode,
}

impl Condition {
    /// Constructs a new `Condition` given a raw WDL expression string.
    pub fn new(raw: impl Into<String>) -> Result<Self, Vec<Diagnostic>> {
        /// Type evaluation context used for resolving the type of conditional
        /// expressions.
        #[derive(Default)]
        struct Context(Diagnostics);

        impl wdl_analysis::types::v1::EvaluationContext for Context {
            fn version(&self) -> SupportedVersion {
                Default::default()
            }

            fn resolve_name(&mut self, name: &str, span: Span) -> Option<Type> {
                match name {
                    "cpu" => Some(PrimitiveType::Float.into()),
                    "memory" => Some(PrimitiveType::Integer.into()),
                    "gpu" | "fpga" => Some(PrimitiveType::Boolean.into()),
                    "disks" => Some(PrimitiveType::Integer.into()),
                    "hint" => Some(Type::Object),
                    _ => {
                        self.add_diagnostic(unknown_name(name, span));
                        None
                    }
                }
            }

            fn resolve_type_name(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
                Err(unknown_type(name, span))
            }

            fn task(&self) -> Option<&Task> {
                None
            }

            fn diagnostics_config(&self) -> DiagnosticsConfig {
                Default::default()
            }

            fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
                self.0.add(diagnostic);
            }

            fn exceptable_add_diagnostic<N: TreeNode + Exceptable>(
                &mut self,
                diagnostic: Diagnostic,
                element: &N,
                exceptable_nodes: &Option<&'static [SyntaxKind]>,
            ) {
                self.0.exceptable_add(diagnostic, element, exceptable_nodes);
            }
        }

        let raw = raw.into();
        let mut parser = Parser::new(Lexer::new(&raw));
        let marker = parser.start();
        match v1::expr(&mut parser, marker) {
            Ok(()) => {
                if let Some((_, span)) = parser.next() {
                    return Err(vec![
                        Diagnostic::error("expected a single WDL expression")
                            .with_label("extraneous WDL source starts here", span),
                    ]);
                }

                let output = parser.finish();
                if !output.diagnostics.is_empty() {
                    return Err(output.diagnostics);
                }

                let expr = Expr::cast(construct_tree(raw.as_ref(), output.events))
                    .expect("node should cast");

                // Determine the type of the expression
                let mut context = Context::default();
                let ty = ExprTypeEvaluator::new(&mut context)
                    .evaluate_expr(&expr)
                    .unwrap_or(Type::Union);

                if !context.0.is_empty() {
                    return Err(context.0.into());
                }

                match ty {
                    Type::Primitive(PrimitiveType::Boolean, false) | Type::Union => {}
                    _ => {
                        return Err(vec![
                            Diagnostic::error(format!(
                                "conditional expression is expected to be type `Boolean`, but \
                                 found type `{ty}`",
                            ))
                            .with_highlight(expr.span()),
                        ]);
                    }
                }

                Ok(Self {
                    raw,
                    expr: expr.inner().green().into_owned(),
                })
            }
            Err((marker, diagnostic)) => {
                marker.abandon(&mut parser);
                Err(vec![diagnostic.into()])
            }
        }
    }

    /// Evaluates the condition's expression for the given execution request and
    /// returns the result.
    ///
    /// Returns an error if the evaluation resulted in an error.
    pub(crate) async fn evaluate(
        &self,
        request: &ExecuteTaskRequest<'_>,
        transferer: &dyn Transferer,
    ) -> Result<bool> {
        /// Helper that implements `EvaluationContext`.
        struct Context<'a> {
            /// The task execution request.
            request: &'a ExecuteTaskRequest<'a>,
            /// The file transferer for evaluation.
            transferer: &'a dyn Transferer,
        }

        impl EvaluationContext for Context<'_> {
            fn version(&self) -> SupportedVersion {
                Default::default()
            }

            fn resolve_name(&self, name: &str, span: Span) -> Result<Value, Diagnostic> {
                match name {
                    "cpu" => Ok(self.request.constraints.cpu.into()),
                    "memory" => Ok((self.request.constraints.memory as i64).into()),
                    "gpu" => Ok((!self.request.constraints.gpu.is_empty()).into()),
                    "fpga" => Ok((!self.request.constraints.fpga.is_empty()).into()),
                    "disks" => Ok(self
                        .request
                        .constraints
                        .disks
                        .iter()
                        .map(|(_, s)| *s)
                        .sum::<i64>()
                        .into()),
                    "hint" => Ok(self.request.hints.clone().into()),
                    _ => Err(unknown_name(name, span)),
                }
            }

            fn resolve_type_name(&self, name: &str, span: Span) -> Result<Type, Diagnostic> {
                Err(unknown_type(name, span))
            }

            fn enum_choice_value(
                &self,
                enum_name: &str,
                choice_name: &str,
            ) -> Result<Value, Diagnostic> {
                Err(unknown_enum_choice(enum_name, choice_name))
            }

            fn base_dir(&self) -> &EvaluationPath {
                self.request.base_dir
            }

            fn temp_dir(&self) -> &Path {
                self.request.temp_dir
            }

            fn transferer(&self) -> &dyn Transferer {
                self.transferer
            }

            fn object_access(&self, object: &Object, name: &str) -> Option<Value> {
                // If the object being accessed is not the hint object, let the access proceed
                // normally
                if !Arc::ptr_eq(&object.members, &self.request.hints.members) {
                    return None;
                }

                // Access to the hints object first checks for a hint override in the inputs and
                // then falls back to the task's hints; if the name is not present in either, a
                // `None` value is returned instead of an error
                Some(
                    self.request
                        .inputs
                        .hint(name)
                        .or_else(|| object.get(name))
                        .cloned()
                        .unwrap_or_else(|| NoneValue::untyped().into()),
                )
            }
        }

        /// Helper for evaluating the given expression.
        ///
        /// Returns a diagnostic that will be converted to any `anyhow::Error`
        /// by the caller.
        async fn eval(context: Context<'_>, expr: &Expr<SyntaxNode>) -> Result<bool, Diagnostic> {
            let mut evaluator = ExprEvaluator::new(context);
            let value = evaluator.evaluate_expr(expr).await?;
            match value.as_boolean() {
                Some(res) => Ok(res),
                None => Err(Diagnostic::error(format!(
                    "conditional expression is expected to be type `Boolean`, but found type \
                     `{ty}`",
                    ty = value.ty()
                ))
                .with_highlight(expr.span())),
            }
        }

        let expr = Expr::cast(self.expr.clone().into()).expect("should be an expression node");
        match eval(
            Context {
                request,
                transferer,
            },
            &expr,
        )
        .await
        {
            Ok(res) => Ok(res),
            Err(diagnostic) => {
                let file: SimpleFile<_, _> = SimpleFile::new("<condition>", &self.raw);
                let mut buffer = Buffer::no_color();
                term::emit_to_write_style(
                    &mut buffer,
                    &Default::default(),
                    &file,
                    &diagnostic.to_codespan(()),
                )
                .context("failed to write diagnostic to buffer")?;
                let diagnostic = String::from_utf8(buffer.into_inner())
                    .context("diagnostic buffer contents are not UTF-8")?;
                bail!(
                    "failed to evaluate backend dynamic arguments condition: {diagnostic}",
                    diagnostic = diagnostic
                        .strip_prefix("error: ")
                        .unwrap_or(&diagnostic)
                        .trim()
                );
            }
        }
    }
}

impl ToToml for Condition {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        self.raw.to_toml(arena)
    }
}

impl<'de> FromToml<'de> for Condition {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        /// Used to remap an unescaped string index to an index in the
        /// corresponding escaped TOML string.
        fn remap(mapping: &[(usize, usize)], unescaped: usize) -> usize {
            match mapping.binary_search_by_key(&unescaped, |x| x.0) {
                Ok(i) => {
                    // Found in the map, use the escaped index
                    mapping[i].1
                }
                Err(i) => {
                    // Not in the map, need to potentially offset the unescaped position
                    // based on a preceding map entry
                    unescaped
                        + if i == 0 {
                            // No need to offset as this position comes before any
                            // escape sequences
                            0
                        } else {
                            // Offset by the last delta
                            mapping[i - 1].1 - mapping[i - 1].0
                        }
                }
            }
        }

        /// Helper for pushing diagnostics as `toml_spanner::Error` into the
        /// TOML parsing context.
        fn push_errors(
            ctx: &mut toml_spanner::Context<'_>,
            item: &Item<'_>,
            diagnostics: Vec<Diagnostic>,
        ) -> Failed {
            for diagnostic in diagnostics {
                let span = if let Some(label) = diagnostic.labels().next() {
                    let label_span = label.span();
                    let span = item.span();

                    let source = &ctx.source()[span.start as usize..span.end as usize];
                    let offset = if source.starts_with(r#"""""#) | source.starts_with("'''") {
                        3
                    } else {
                        1
                    };

                    let mapping = escape_mapping(
                        source
                            .get(offset..(source.len() - offset))
                            .expect("invalid TOML string"),
                    );

                    // Remap the start and end of the label
                    let label_start = remap(&mapping, label_span.start());
                    let label_end = remap(&mapping, label_span.end());

                    toml_spanner::Span::new(
                        span.start + offset as u32 + label_start as u32,
                        span.start + offset as u32 + label_end as u32,
                    )
                } else {
                    item.span()
                };

                ctx.errors
                    .push(toml_spanner::Error::custom(diagnostic.message(), span));
            }

            Failed
        }

        Self::new(String::from_toml(ctx, item)?).map_err(|diags| push_errors(ctx, item, diags))
    }
}

/// Represents a set of conditional arguments for the LSF and Slurm backends.
///
/// Conditional arguments are passed to the program responsible for queuing a
/// task when the associated conditional expression evaluates to `true`.
#[derive(Debug, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct ConditionalArgs {
    /// The condition for including the arguments.
    pub condition: Condition,
    /// The arguments to use when the condition evaluates to `true`.
    #[toml(default)]
    #[schemars(default)]
    pub args: Vec<String>,
}

impl ConditionalArgs {
    /// Validates the conditional arguments.
    ///
    /// This ensures that the conditional expression is valid WDL and the
    /// specified arguments are not empty.
    pub fn validate(&self) -> Result<()> {
        if self.args.is_empty() {
            bail!("backend conditional arguments must have at least one argument specified");
        }

        Ok(())
    }
}

/// Represents additional arguments to the Slurm and LSF backends.
///
/// These arguments are passed to the executable responsible for queuing a task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct AdditionalArgs {
    /// The additional arguments to pass to the backend program.
    #[toml(default)]
    #[schemars(default)]
    pub args: Vec<String>,
    /// The conditional arguments to pass to the backend program.
    ///
    /// The first conditional argument with an associated conditional expression
    /// that evaluates to `true` will be passed to the backend program.
    #[toml(default)]
    #[schemars(default)]
    pub conditional: Vec<ConditionalArgs>,
}

impl AdditionalArgs {
    /// Validates the additional arguments.
    pub fn validate(&self) -> Result<()> {
        for arg in &self.conditional {
            arg.validate()?;
        }

        Ok(())
    }
}

/// Helper functions for `ByteSize` TOML serialization.
mod byte_size {
    use bytesize::ByteSize;
    use toml_spanner::Context;
    use toml_spanner::Failed;
    use toml_spanner::Item;

    /// Helper function for serializing `ByteSize` to TOML.
    pub fn from_toml<'de>(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<ByteSize, Failed> {
        if let Some(s) = item.as_u64() {
            return Ok(ByteSize(s));
        }

        if let Some(s) = item.as_str() {
            return s
                .parse()
                .map_err(|e| ctx.report_custom_error(format!("invalid byte size: {e}"), item));
        }

        Err(ctx.report_expected_but_found(&"integer or string", item))
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
#[derive(Debug, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct LsfQueueConfig {
    /// The name of the queue; this is the string passed to `bsub -q
    /// <queue_name>`.
    pub name: String,
    /// The maximum number of CPUs this queue can provision for a single task.
    pub max_cpu_per_task: Option<u64>,
    /// The maximum memory this queue can provision for a single task.
    #[toml(FromToml with = byte_size, ToToml with = display)]
    #[schemars(with = "Option<u64>")]
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct LsfApptainerBackendConfig {
    /// The task monitor polling interval, in seconds.
    ///
    /// Defaults to 30 seconds.
    pub interval: Option<u64>,
    /// The maximum number of concurrent LSF operations the backend will
    /// perform.
    ///
    /// This controls the maximum concurrent number of `bsub` processes the
    /// backend will spawn to queue tasks.
    ///
    /// Defaults to 10 concurrent operations.
    pub max_concurrency: Option<u32>,
    /// Which queue, if any, to specify when submitting normal jobs to LSF.
    ///
    /// This may be superseded by
    /// [`short_task_lsf_queue`][Self::short_task_lsf_queue],
    /// [`gpu_lsf_queue`][Self::gpu_lsf_queue], or
    /// [`fpga_lsf_queue`][Self::fpga_lsf_queue] for corresponding tasks.
    #[toml(style = Header)]
    pub default_lsf_queue: Option<LsfQueueConfig>,
    /// Which queue, if any, to specify when submitting [short
    /// tasks](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#short_task) to LSF.
    ///
    /// This may be superseded by [`gpu_lsf_queue`][Self::gpu_lsf_queue] or
    /// [`fpga_lsf_queue`][Self::fpga_lsf_queue] for tasks which require
    /// specialized hardware.
    #[toml(style = Header)]
    pub short_task_lsf_queue: Option<LsfQueueConfig>,
    /// Which queue, if any, to specify when submitting [tasks which require a
    /// GPU](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to LSF.
    #[toml(style = Header)]
    pub gpu_lsf_queue: Option<LsfQueueConfig>,
    /// Which queue, if any, to specify when submitting [tasks which require an
    /// FPGA](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to LSF.
    #[toml(style = Header)]
    pub fpga_lsf_queue: Option<LsfQueueConfig>,
    /// Prefix to add to every LSF job name before the task identifier. This is
    /// truncated as needed to satisfy the byte-oriented LSF job name limit.
    pub job_name_prefix: Option<String>,
    /// The additional arguments to `bsub` used to queue a new task.
    #[toml(default)]
    #[schemars(default)]
    pub bsub: AdditionalArgs,
    /// The configuration of Apptainer, which is used as the container runtime
    /// on the compute nodes where LSF dispatches tasks.
    ///
    /// Note that this will likely be replaced by an abstraction over multiple
    /// container execution runtimes in the future, rather than being
    /// hardcoded to Apptainer.
    #[toml(default)]
    #[schemars(default)]
    pub apptainer: ApptainerConfig,
}

impl LsfApptainerBackendConfig {
    /// Validate that the backend is appropriately configured.
    pub async fn validate(&self) -> Result<(), anyhow::Error> {
        if cfg!(not(unix)) {
            bail!("LSF + Apptainer backend is not supported on non-unix platforms");
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

        // Validate the additional arguments
        self.bsub.validate()?;

        // Validate the apptainer configuration
        self.apptainer.validate().await?;

        Ok(())
    }

    /// Get the appropriate LSF queue for a task under this configuration.
    ///
    /// Specialized hardware requirements are prioritized over other
    /// characteristics, with FPGA taking precedence over GPU.
    pub(crate) fn lsf_queue_for_task(
        &self,
        requirements: &Object,
        hints: &Object,
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
#[derive(Debug, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct SlurmPartitionConfig {
    /// The name of the partition; this is the string passed to `sbatch
    /// --partition=<partition_name>`.
    pub name: String,
    /// The maximum number of CPUs this partition can provision for a single
    /// task.
    pub max_cpu_per_task: Option<u64>,
    /// The maximum memory this partition can provision for a single task.
    #[toml(FromToml with = byte_size, ToToml with = display)]
    #[schemars(with = "Option<u64>")]
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename_all = "snake_case", deny_unknown_fields)]
pub struct SlurmApptainerBackendConfig {
    /// The task monitor polling interval, in seconds.
    ///
    /// Defaults to 30 seconds.
    pub interval: Option<u64>,
    /// The maximum number of concurrent Slurm operations the backend will
    /// perform.
    ///
    /// This controls the maximum concurrent number of `sbatch` processes the
    /// backend will spawn to queue tasks.
    ///
    /// Defaults to 10 concurrent operations.
    pub max_concurrency: Option<u32>,
    /// Which partition, if any, to specify when submitting normal jobs to
    /// Slurm.
    ///
    /// This may be superseded by
    /// [`short_task_slurm_partition`][Self::short_task_slurm_partition],
    /// [`gpu_slurm_partition`][Self::gpu_slurm_partition], or
    /// [`fpga_slurm_partition`][Self::fpga_slurm_partition] for corresponding
    /// tasks.
    #[toml(style = Header)]
    pub default_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [short
    /// tasks](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#short_task) to Slurm.
    ///
    /// This may be superseded by
    /// [`gpu_slurm_partition`][Self::gpu_slurm_partition] or
    /// [`fpga_slurm_partition`][Self::fpga_slurm_partition] for tasks which
    /// require specialized hardware.
    #[toml(style = Header)]
    pub short_task_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [tasks which require
    /// a GPU](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to Slurm.
    #[toml(style = Header)]
    pub gpu_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [tasks which require
    /// an FPGA](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to Slurm.
    #[toml(style = Header)]
    pub fpga_slurm_partition: Option<SlurmPartitionConfig>,
    /// The additional arguments to `sbatch` used to queue a new task.
    #[toml(default)]
    #[schemars(default)]
    pub sbatch: AdditionalArgs,
    /// Prefix to add to every Slurm job name before the task identifier.
    pub job_name_prefix: Option<String>,
    /// The configuration of Apptainer, which is used as the container runtime
    /// on the compute nodes where Slurm dispatches tasks.
    ///
    /// Note that this will likely be replaced by an abstraction over multiple
    /// container execution runtimes in the future, rather than being
    /// hardcoded to Apptainer.
    #[toml(default)]
    #[schemars(default)]
    pub apptainer: ApptainerConfig,
}

impl SlurmApptainerBackendConfig {
    /// Validate that the backend is appropriately configured.
    pub async fn validate(&self) -> Result<(), anyhow::Error> {
        if cfg!(not(unix)) {
            bail!("Slurm + Apptainer backend is not supported on non-unix platforms");
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

        // Validate the additional arguments
        self.sbatch.validate()?;

        // Validate the apptainer configuration
        self.apptainer.validate().await?;

        Ok(())
    }

    /// Get the appropriate Slurm partition for a task under this configuration.
    ///
    /// Specialized hardware requirements are prioritized over other
    /// characteristics, with FPGA taking precedence over GPU.
    pub(crate) fn slurm_partition_for_task(
        &self,
        requirements: &Object,
        hints: &Object,
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

/// Represents an error encountered during merging TOML configuration.
#[derive(Debug, thiserror::Error)]
pub enum BuilderMergeError {
    /// An error occurred while serializing merged configuration.
    #[error("failed to serialize after merging configuration")]
    Serialize(#[from] ToTomlError),
    /// An error occurred while attempting to parse merged configuration.
    #[error("failed to parse after merging configuration")]
    Parse {
        /// The merged source.
        source: String,
        /// The error that was encountered.
        #[source]
        error: toml_spanner::Error,
    },
    /// An error occurred while deserializing merged configuration.
    #[error("failed to deserialize after merging configuration")]
    Deserialize {
        /// The merged source.
        source: String,
        /// The error that was encountered.
        #[source]
        error: toml_spanner::FromTomlError,
    },
}

/// Helper for displaying certain builder error messages.
struct BuilderErrorDisplay<'a>(&'static str, &'a Option<PathBuf>);

impl fmt::Display for BuilderErrorDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            Some(path) => write!(
                f,
                "failed to {op} configuration file `{path}`",
                op = self.0,
                path = path.display()
            ),
            None => write!(
                f,
                "failed to {op} in-memory configuration string",
                op = self.0
            ),
        }
    }
}

/// Represents an error encountered while building a configuration.
#[derive(Debug, thiserror::Error)]
pub enum BuilderError {
    /// Failed to read the provided configuration file.
    #[error("failed to read configuration file `{path}`")]
    Io {
        /// The path to the file.
        path: PathBuf,
        /// The error that was encountered.
        #[source]
        error: std::io::Error,
    },
    /// Failed to parse the provided configuration file.
    #[error("{}", BuilderErrorDisplay("parse", .path))]
    Parse {
        /// The path to the file.
        ///
        /// This is `None` when the source was a string.
        path: Option<PathBuf>,
        /// The TOML source that was parsed.
        source: String,
        /// The error that was encountered.
        #[source]
        error: toml_spanner::Error,
    },
    /// Failed to deserialize the provided configuration file.
    #[error("{}", BuilderErrorDisplay("deserialize", .path))]
    Deserialize {
        /// The path to the file.
        ///
        /// This is `None` when the source was a string.
        path: Option<PathBuf>,
        /// The TOML source that was parsed.
        source: String,
        /// The error that was encountered.
        #[source]
        error: toml_spanner::FromTomlError,
    },
    /// Failed to merge configuration.
    #[error(transparent)]
    Merge(#[from] BuilderMergeError),
}

impl BuilderError {
    /// Gets the path associated with the error.
    ///
    /// If the error relates to parsing or deserializing an in-memory string,
    /// the path will be `<string>`.
    ///
    /// If the error relates to parsing or deserializing the merged
    /// configuration, the path will be `<merged>`.
    pub fn path(&self) -> impl fmt::Display + Clone {
        #[derive(Clone)]
        struct Helper<'a>(&'a BuilderError);

        impl fmt::Display for Helper<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.0 {
                    BuilderError::Io { path, .. }
                    | BuilderError::Parse {
                        path: Some(path), ..
                    }
                    | BuilderError::Deserialize {
                        path: Some(path), ..
                    } => path.display().fmt(f),
                    BuilderError::Parse { path: None, .. }
                    | BuilderError::Deserialize { path: None, .. } => write!(f, "<string>"),
                    BuilderError::Merge(_) => write!(f, "<merged>"),
                }
            }
        }

        Helper(self)
    }

    /// Gets the TOML error associated with the error.
    ///
    /// Returns `None` if the error is not associated with parsing or
    /// deserializing TOML.
    pub fn toml_error(&self) -> Option<&toml_spanner::Error> {
        match &self {
            Self::Parse { error, .. } | Self::Merge(BuilderMergeError::Parse { error, .. }) => {
                Some(error)
            }
            Self::Deserialize { error, .. }
            | Self::Merge(BuilderMergeError::Deserialize { error, .. }) => error.errors.first(),
            _ => None,
        }
    }

    /// Gets the TOML source code associated with the error.
    ///
    /// Returns `None` if the error is not associated with parsing or
    /// deserializing TOML.
    pub fn source(&self) -> Option<&str> {
        match &self {
            Self::Parse { source, .. }
            | Self::Deserialize { source, .. }
            | Self::Merge(BuilderMergeError::Parse { source, .. })
            | Self::Merge(BuilderMergeError::Deserialize { source, .. }) => Some(source),
            _ => None,
        }
    }

    /// Converts the error into a [`Diagnostic`].
    pub fn to_diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::error(self.to_string());

        if let Some(e) = self.toml_error() {
            for (span, text) in [e.primary_label(), e.secondary_label()]
                .into_iter()
                .flatten()
            {
                let span = Span::new(span.start as usize, (span.end - span.start) as usize);
                let text: &str = text.trim();

                // For some reason label text isn't returned for certain errors
                let text = if text.is_empty() {
                    match e.kind() {
                        ErrorKind::UnexpectedEof => "unexpected end of file",
                        ErrorKind::RedefineAsArray { .. } => {
                            "a previously defined table was redefined as an array"
                        }
                        ErrorKind::FileTooLarge => "file is too large",
                        ErrorKind::Custom(message) => message,
                        _ => text,
                    }
                } else {
                    text
                };

                if text.is_empty() {
                    diagnostic = diagnostic.with_highlight(span);
                } else {
                    diagnostic = diagnostic.with_label(text, span);
                }
            }
        }

        if matches!(self, Self::Merge(_)) {
            diagnostic = diagnostic.with_help("reported line numbers reflect merged TOML source");
        }

        diagnostic
    }
}

/// Represents a possible source of configuration.
#[derive(Debug)]
enum Source {
    /// A configuration exists as a file on disk.
    Path(PathBuf),
    /// The configuration exists as a TOML string.
    String(String),
}

/// Implements a configuration builder.
///
/// The builder supports merging multiple TOML configuration files together.
///
/// Merging works by the following:
///
/// * Matching table keys that are arrays get appended.
/// * Matching table keys that are tables are recursively merged.
/// * Otherwise, the value associated with the key is replaced by the
///   configuration being merged in.
///
/// The configuration builder is generic so that it can build any type that
/// supports TOML serialization.
#[derive(Default, Debug)]
pub struct ConfigBuilder<T> {
    /// The sources for building the configuration.
    sources: Vec<Source>,
    /// Phantom data to store the type parameter.
    _phantom: PhantomData<T>,
}

impl<T> ConfigBuilder<T> {
    /// Adds a TOML string source to the builder.
    pub fn with_string_source(mut self, toml: impl Into<String>) -> Self {
        self.sources.push(Source::String(toml.into()));
        self
    }

    /// Adds a TOML configuration file source to the builder.
    pub fn with_file_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(Source::Path(path.into()));
        self
    }

    /// Attempts to build the configuration.
    ///
    /// Each configuration file is merged with the previous one in the order
    /// they were added to the builder.
    pub fn try_build(self) -> Result<T, BuilderError>
    where
        T: ToToml + for<'de> FromToml<'de>,
    {
        // Read all of the files up front so that the source outlives the arena
        let sources: Vec<(Option<PathBuf>, String)> = self
            .sources
            .into_iter()
            .map(|s| match s {
                Source::Path(path) => {
                    let source = fs::read_to_string(&path).map_err(|e| BuilderError::Io {
                        path: path.clone(),
                        error: e,
                    })?;

                    Ok((Some(path), source))
                }
                Source::String(source) => Ok((None, source)),
            })
            .collect::<Result<_, BuilderError>>()?;

        // Parse all of the documents
        let arena = Arena::new();
        let documents = sources
            .iter()
            .map(|(path, source)| {
                toml_spanner::parse(source, &arena).map_err(|e| BuilderError::Parse {
                    path: path.clone(),
                    source: source.clone(),
                    error: e,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Merge the documents as TOML tables
        let mut merged_table: Table<'_> = Table::new();
        for (index, mut document) in documents.into_iter().enumerate() {
            // Start by deserializing the document to ensure it is a valid standalone
            // configuration
            document.to::<T>().map_err(|e| {
                let (path, source) = &sources[index];
                BuilderError::Deserialize {
                    path: path.clone(),
                    source: source.clone(),
                    error: e,
                }
            })?;

            // Merge the tables
            Self::merge_tables(document.into_table(), &mut merged_table, &arena);
        }

        // Unfortunately, there's no way to go from `Table` to `T`, so we must
        // round-trip through a string; serialize the merged table back to a
        // string first
        let source =
            toml_spanner::to_string(&merged_table).map_err(BuilderMergeError::Serialize)?;

        // Deserialize the merged contents back to the underlying config type
        Ok(toml_spanner::parse(&source, &arena)
            .map_err(|e| BuilderMergeError::Parse {
                source: source.clone(),
                error: e,
            })?
            .to()
            .map_err(|e| BuilderMergeError::Deserialize {
                source: source.clone(),
                error: e,
            })?)
    }

    /// Merges the `src` table with the `dest` table.
    ///
    /// If a key in `src` exists in `dest` and both items are tables, the tables
    /// are recursively merged together.
    ///
    /// If a key in `src` exists in `dest` and both items are arrays, the array
    /// in `dest` is extended with the elements of the array in `src`.
    ///
    /// Otherwise, the item in the `src` table replaces the item in the `dest`
    /// table.
    fn merge_tables<'de>(src: Table<'de>, dest: &mut Table<'de>, arena: &'de Arena) {
        for (key, src_item) in src {
            let Some(dest_item) = dest.get_mut(key.name) else {
                dest.insert(key, src_item, arena);
                continue;
            };

            // Merge arrays by appending
            if let Some(src_array) = src_item.as_array()
                && let Some(dest_array) = dest_item.as_array_mut()
            {
                for element in src_array {
                    dest_array.push(element.clone_in(arena), arena);
                }
                continue;
            }

            // Merge tables by recursing
            if let Some(_) = src_item.as_table()
                && let Some(dest_table) = dest_item.as_table_mut()
            {
                Self::merge_tables(src_item.into_table().unwrap(), dest_table, arena);
                continue;
            }

            // Overwrite the item
            dest.insert(key, src_item, arena);
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use std::io::Write;

    use codespan_reporting::files::SimpleFile;
    use codespan_reporting::term::DisplayStyle;
    use codespan_reporting::term::emit_into_string;
    use codespan_reporting::term::{self};
    use futures::future::BoxFuture;
    use pretty_assertions::assert_eq;
    use tempfile::TempPath;
    use tempfile::tempdir;

    use super::*;
    use crate::ONE_GIBIBYTE;
    use crate::TaskInputs;
    use crate::backend::TaskExecutionConstraints;
    use crate::http::Location;
    use crate::v1::DEFAULT_TASK_REQUIREMENT_CPU;
    use crate::v1::DEFAULT_TASK_REQUIREMENT_DISKS;
    use crate::v1::DEFAULT_TASK_REQUIREMENT_MEMORY;

    #[test]
    fn redacted_secret() {
        let mut map: HashMap<_, SecretString> = HashMap::new();
        map.insert(
            "foo",
            SecretString {
                inner: "secret".into(),
                redacted: false,
            },
        );

        assert_eq!(
            toml_spanner::to_string(&map).unwrap().trim(),
            format!(r#"foo = "secret""#)
        );

        map.insert(
            "foo",
            SecretString {
                inner: "secret".into(),
                redacted: true,
            },
        );
        assert_eq!(
            toml_spanner::to_string(&map).unwrap().trim(),
            format!(r#"foo = "{REDACTED}""#)
        );
    }

    #[test]
    fn redacted_config() {
        let config = Config {
            backends: [
                (
                    "first".to_string(),
                    TesBackendConfig {
                        auth: Some(TesBackendAuthConfig::Basic {
                            config: BasicAuthConfig {
                                username: "foo".into(),
                                password: "secret".into(),
                            },
                        }),
                        ..Default::default()
                    }
                    .into(),
                ),
                (
                    "second".to_string(),
                    TesBackendConfig {
                        auth: Some(
                            BearerAuthConfig {
                                token: "secret".into(),
                            }
                            .into(),
                        ),
                        ..Default::default()
                    }
                    .into(),
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

        let toml = toml_spanner::to_string(&config).unwrap();
        assert!(toml.contains("secret"), "`{toml}` contains a secret");
    }

    #[tokio::test]
    async fn test_config_validate() {
        // Test invalid task config
        let mut config = Config::default();
        config.task.retries = 255.into();
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "configuration value `task.retries` cannot exceed 100"
        );

        // Test invalid scatter concurrency config
        let mut config = Config::default();
        config.workflow.scatter.concurrency = 0;
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "configuration value `workflow.scatter.concurrency` cannot be zero"
        );

        // Test invalid backend name
        let config = Config {
            backend: "foo".into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "a backend named `foo` is not present in the configuration"
        );
        let config = Config {
            backend: "bar".into(),
            backends: [("foo".to_string(), BackendConfig::default())].into(),
            ..Default::default()
        };
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "a backend named `bar` is not present in the configuration"
        );

        // Test a singular backend
        let config = Config {
            backend: "foo".to_string(),
            backends: [("foo".to_string(), BackendConfig::default())].into(),
            ..Default::default()
        };
        config.validate().await.expect("config should validate");

        // Test invalid local backend cpu config
        let config = Config {
            backends: [(
                "default".to_string(),
                LocalBackendConfig {
                    cpu: Some(0),
                    ..Default::default()
                }
                .into(),
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
                LocalBackendConfig {
                    cpu: Some(10000000),
                    ..Default::default()
                }
                .into(),
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
                LocalBackendConfig {
                    memory: Some("0 GiB".to_string()),
                    ..Default::default()
                }
                .into(),
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
                LocalBackendConfig {
                    memory: Some("100 meows".to_string()),
                    ..Default::default()
                }
                .into(),
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
                LocalBackendConfig {
                    memory: Some("1000 TiB".to_string()),
                    ..Default::default()
                }
                .into(),
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
            backends: [("default".to_string(), TesBackendConfig::default().into())].into(),
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
                TesBackendConfig {
                    url: Some("https://example.com".parse().unwrap()),
                    max_concurrency: Some(0),
                    ..Default::default()
                }
                .into(),
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
                TesBackendConfig {
                    url: Some("http://example.com".parse().unwrap()),
                    inputs: Some("http://example.com".parse().unwrap()),
                    outputs: Some("http://example.com".parse().unwrap()),
                    ..Default::default()
                }
                .into(),
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
                TesBackendConfig {
                    url: Some("http://example.com".parse().unwrap()),
                    inputs: Some("http://example.com".parse().unwrap()),
                    outputs: Some("http://example.com".parse().unwrap()),
                    insecure: true,
                    ..Default::default()
                }
                .into(),
            )]
            .into(),
            ..Default::default()
        };
        config
            .validate()
            .await
            .expect("configuration should validate");

        // invalid Parallelism
        let mut config = Config::default();
        config.http.parallelism = 0.into();
        assert_eq!(
            config.validate().await.unwrap_err().to_string(),
            "configuration value `http.parallelism` cannot be zero"
        );

        // valid Parallelism
        let mut config = Config::default();
        config.http.parallelism = 5.into();
        assert!(
            config.validate().await.is_ok(),
            "should pass for valid configuration"
        );
        let mut config = Config::default();
        config.http.parallelism = Parallelism::default();
        assert!(config.validate().await.is_ok(), "should pass for default");

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
                LsfApptainerBackendConfig {
                    job_name_prefix: Some(job_name_prefix.clone()),
                    ..Default::default()
                }
                .into(),
            );
            assert_eq!(
                config.validate().await.unwrap_err().to_string(),
                format!("LSF job name prefix `{job_name_prefix}` exceeds the maximum 100 bytes")
            );
        }
    }

    fn create_temp_file(contents: &str) -> TempPath {
        let mut file: tempfile::NamedTempFile =
            tempfile::NamedTempFile::new().expect("failed to create temporary file");
        file.write_all(contents.as_bytes())
            .expect("failed to write temporary file");
        file.into_temp_path()
    }

    #[test]
    fn it_builds_with_no_sources() {
        let config = Config::builder().try_build().expect("should build");
        assert_eq!(config, Config::default(), "should be equal");
    }

    #[test]
    fn it_builds_with_one_source() {
        let path = create_temp_file("backend = 'foo'");

        let config = Config::builder()
            .with_file_source(&path)
            .try_build()
            .expect("should build");
        assert_eq!(
            config,
            Config {
                backend: "foo".into(),
                ..Default::default()
            },
            "should be equal"
        );
    }

    #[test]
    fn it_errors_on_invalid_parse() {
        let path = create_temp_file("invalid");

        let e = Config::builder()
            .with_file_source(&path)
            .try_build()
            .expect_err("should fail");

        let source = e.source().expect("should have source");

        let diagnostic = e.to_diagnostic();
        let error = emit_into_string(
            &term::Config {
                display_style: DisplayStyle::Rich,
                ..Default::default()
            },
            &SimpleFile::new(e.path(), source),
            &diagnostic.to_codespan(()),
        )
        .expect("should emit");

        assert!(
            error.contains("expected an equals"),
            "the error `{error}` does not contain the expected message"
        );
    }

    #[test]
    fn it_errors_on_invalid_deserialization() {
        let path = create_temp_file("backend = 42");

        let e = Config::builder()
            .with_file_source(&path)
            .try_build()
            .expect_err("should fail");

        let source = e.source().expect("should have source");

        let diagnostic = e.to_diagnostic();
        let error = emit_into_string(
            &term::Config {
                display_style: DisplayStyle::Rich,
                ..Default::default()
            },
            &SimpleFile::new(e.path(), source),
            &diagnostic.to_codespan(()),
        )
        .expect("should emit");

        assert!(
            error.contains("expected a string"),
            "the error `{error}` does not contain the expected message"
        );
    }

    #[test]
    fn it_merges_sources() {
        let first = create_temp_file(
            r#"
backend = 'foo'

[task]
excluded_cache_inputs = ['1', '2', '3']

[backends.foo]
type = 'local'
"#,
        );

        let second = create_temp_file(
            r#"
[task]
excluded_cache_inputs = ['4', '5']

[backends.bar]
type = 'docker'
"#,
        );

        let third: TempPath = create_temp_file(
            r#"
backend = 'baz'

[task]
excluded_cache_inputs = ['6', '7', '8']

[backends.baz]
type = 'tes'
"#,
        );

        let fourth = r#"
backend = 'qux'

[task]
excluded_cache_inputs = ['9', '10']

[backends.qux]
type = 'lsf_apptainer'
"#;

        let config = Config::builder()
            .with_file_source(&first)
            .with_file_source(&second)
            .with_file_source(&third)
            .with_string_source(fourth)
            .try_build()
            .expect("should build");

        assert_eq!(
            config,
            Config {
                backend: "qux".into(),
                task: TaskConfig {
                    excluded_cache_inputs: vec![
                        "1".to_string(),
                        "2".to_string(),
                        "3".to_string(),
                        "4".to_string(),
                        "5".to_string(),
                        "6".to_string(),
                        "7".to_string(),
                        "8".to_string(),
                        "9".to_string(),
                        "10".to_string(),
                    ],
                    ..Default::default()
                },
                backends: IndexMap::from_iter([
                    ("foo".to_string(), LocalBackendConfig::default().into()),
                    ("bar".to_string(), DockerBackendConfig::default().into()),
                    ("baz".to_string(), TesBackendConfig::default().into()),
                    (
                        "qux".to_string(),
                        LsfApptainerBackendConfig::default().into()
                    ),
                ]),
                ..Default::default()
            },
            "should be equal"
        );
    }

    #[test]
    fn parallelism_serialization() {
        let map: HashMap<&str, Parallelism> =
            HashMap::from_iter([("value", Parallelism::Available)]);
        assert_eq!(
            toml_spanner::to_string(&map).unwrap(),
            format!("value = \"available\"\n")
        );

        let map: HashMap<&str, Parallelism> =
            HashMap::from_iter([("value", Parallelism::Use(123))]);
        assert_eq!(toml_spanner::to_string(&map).unwrap(), "value = 123\n");
    }

    #[test]
    fn parallelism_deserialization() {
        let map: HashMap<String, Parallelism> =
            toml_spanner::from_str("value = 'available'").unwrap();
        assert_eq!(map["value"], Parallelism::Available);

        let map: HashMap<String, Parallelism> = toml_spanner::from_str("value = 123").unwrap();
        assert_eq!(map["value"], Parallelism::Use(123));

        let expected_error =
            "expected a positive integer or `available` for parallelism at `value`";

        let error =
            toml_spanner::from_str::<HashMap<String, Parallelism>>("value = 'wrong'").unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error =
            toml_spanner::from_str::<HashMap<String, Parallelism>>("value = 0").unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error =
            toml_spanner::from_str::<HashMap<String, Parallelism>>("value = -10").unwrap_err();
        assert_eq!(error.to_string(), expected_error);
    }

    #[test]
    fn retries_serialization() {
        let map: HashMap<&str, Retries> = HashMap::from_iter([("value", Retries::Default)]);
        assert_eq!(
            toml_spanner::to_string(&map).unwrap(),
            format!("value = \"default\"\n")
        );

        let map: HashMap<&str, Retries> = HashMap::from_iter([("value", Retries::Use(123))]);
        assert_eq!(toml_spanner::to_string(&map).unwrap(), "value = 123\n");
    }

    #[test]
    fn retries_deserialization() {
        let map: HashMap<String, Retries> = toml_spanner::from_str("value = 'default'").unwrap();
        assert_eq!(map["value"], Retries::Default);

        let map: HashMap<String, Retries> = toml_spanner::from_str("value = 12").unwrap();
        assert_eq!(map["value"], Retries::Use(12));

        let map: HashMap<String, Retries> = toml_spanner::from_str("value = 0").unwrap();
        assert_eq!(map["value"], Retries::Use(0));

        let expected_error = format!(
            "expected an integer less than {MAX_RETRIES} or `default` for retries at `value`"
        );

        let error =
            toml_spanner::from_str::<HashMap<String, Retries>>("value = 'wrong'").unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error = toml_spanner::from_str::<HashMap<String, Retries>>("value = 101").unwrap_err();
        assert_eq!(error.to_string(), expected_error);

        let error = toml_spanner::from_str::<HashMap<String, Retries>>("value = -10").unwrap_err();
        assert_eq!(error.to_string(), expected_error);
    }

    #[test]
    fn mapping_escape_indexes() {
        // Check for empty string
        assert!(escape_mapping("").is_empty());

        // Check a string with no escape sequences
        assert!(escape_mapping("hello world!").is_empty());

        // Check for a string containing only an escape sequences (should contain an
        // exclusive-end mapping)
        assert_eq!(escape_mapping(r#"\"\""#), &[(1, 2), (2, 4)]);
        assert_eq!(escape_mapping(r#"\u0022\u0022"#), &[(1, 6), (2, 12)]);
        assert_eq!(
            escape_mapping(r#"\U00000022\U00000022"#),
            &[(1, 10), (2, 20)]
        );

        // Check a complex string
        assert_eq!(
            escape_mapping(r#"\"foo\u0022 == \U00000022bar\" && \"\" == \"\n\""#),
            &[
                (1, 2),   // f
                (5, 11),  // <space>
                (10, 25), // b
                (14, 30), // <space>
                (19, 36), // \"
                (20, 38), // <space>
                (25, 44), // \n
                (26, 46), // \"
                (27, 48)  // <end of string>
            ]
        );
    }

    #[test]
    fn conditional_args_serialization() {
        // Test for invalid type
        let error = toml_spanner::from_str::<ConditionalArgs>("condition = 1").unwrap_err();
        assert_eq!(
            error.to_string(),
            "expected a string, found integer at `condition`"
        );

        // Test for not a single WDL expression
        let error =
            toml_spanner::from_str::<ConditionalArgs>(r#"condition = "foo bar""#).unwrap_err();
        assert_eq!(error.to_string(), "expected a single WDL expression");

        // Test for parse error
        let error =
            toml_spanner::from_str::<ConditionalArgs>(r#"condition = "{ foo: }""#).unwrap_err();
        assert_eq!(error.to_string(), "expected expression, but found `}`");

        // Test for not a `Boolean` expression
        let error = toml_spanner::from_str::<ConditionalArgs>(r#"condition = "1""#).unwrap_err();
        assert_eq!(
            error.to_string(),
            "conditional expression is expected to be type `Boolean`, but found type `Int`"
        );
        let error = toml_spanner::from_str::<ConditionalArgs>(r#"condition = "hint""#).unwrap_err();
        assert_eq!(
            error.to_string(),
            "conditional expression is expected to be type `Boolean`, but found type `Object`"
        );

        // Test for unknown name
        let error = toml_spanner::from_str::<ConditionalArgs>(r#"condition = "foo""#).unwrap_err();
        assert_eq!(error.to_string(), "unknown name `foo`");

        // Test for unknown type name
        let error =
            toml_spanner::from_str::<ConditionalArgs>(r#"condition = "Foo {}""#).unwrap_err();
        assert_eq!(error.to_string(), "unknown type name `Foo`");

        // Test for valid conditions
        let args: ConditionalArgs = toml_spanner::from_str(r#"condition = "true""#).unwrap();
        assert_eq!(args.condition.raw, "true");
        assert_eq!(
            toml_spanner::to_string(&args).unwrap(),
            "condition = \"true\"\nargs = []\n"
        );

        let args: ConditionalArgs =
            toml_spanner::from_str(r#"condition = "cpu == 1 && hint.bar == \"foo\"""#).unwrap();
        assert_eq!(args.condition.raw, r#"cpu == 1 && hint.bar == "foo""#);
        assert_eq!(
            toml_spanner::to_string(&args).unwrap(),
            "condition = 'cpu == 1 && hint.bar == \"foo\"'\nargs = []\n"
        );
    }

    #[test]
    fn validate_conditional_args() {
        // Check for empty args
        let args: ConditionalArgs = toml_spanner::from_str(r#"condition = "true""#).unwrap();
        assert_eq!(
            args.validate().unwrap_err().to_string(),
            "backend conditional arguments must have at least one argument specified"
        );
    }

    #[tokio::test]
    async fn evaluate_conditions() {
        /// Helper to represent the context for `Condition` evaluation.
        struct Context {
            cpu: f64,
            memory: u64,
            gpu: bool,
            fpga: bool,
            disks: i64,
            inputs: TaskInputs,
            hints: Object,
        }

        impl Default for Context {
            fn default() -> Self {
                Self {
                    cpu: DEFAULT_TASK_REQUIREMENT_CPU,
                    memory: DEFAULT_TASK_REQUIREMENT_MEMORY as u64,
                    gpu: false,
                    fpga: false,
                    disks: (DEFAULT_TASK_REQUIREMENT_DISKS * ONE_GIBIBYTE) as i64,
                    inputs: Default::default(),
                    hints: Default::default(),
                }
            }
        }

        struct Transferer;

        impl crate::http::Transferer for Transferer {
            fn download<'a>(&'a self, _: &'a Url) -> BoxFuture<'a, Result<Location>> {
                unimplemented!()
            }

            fn upload<'a>(&'a self, _: &'a Path, _: &'a Url) -> BoxFuture<'a, Result<()>> {
                unimplemented!()
            }

            fn size<'a>(&'a self, _: &'a Url) -> BoxFuture<'a, anyhow::Result<Option<u64>>> {
                unimplemented!()
            }

            fn walk<'a>(&'a self, _: &'a Url) -> BoxFuture<'a, Result<Arc<[String]>>> {
                unimplemented!()
            }

            fn exists<'a>(&'a self, _: &'a Url) -> BoxFuture<'a, Result<bool>> {
                unimplemented!()
            }

            fn digest<'a>(
                &'a self,
                _: &'a Url,
            ) -> BoxFuture<'a, Result<Option<Arc<cloud_copy::ContentDigest>>>> {
                unimplemented!()
            }
        }

        /// Helper for evaluating `Condition` from a WDL expression string.
        ///
        /// The string is expected to be a valid WDL expression.
        async fn eval(context: Context, expression: &str) -> Result<bool> {
            let dir = tempdir().context("failed to create temporary directory")?;
            let condition = Condition::new(expression).expect("invalid expression");
            condition
                .evaluate(
                    &ExecuteTaskRequest {
                        id: "test",
                        command: "",
                        inputs: &context.inputs,
                        backend_inputs: &[],
                        requirements: &Object::empty(),
                        hints: &context.hints,
                        env: &Default::default(),
                        constraints: &TaskExecutionConstraints {
                            container: None,
                            cpu: context.cpu,
                            memory: context.memory,
                            gpu: if context.gpu {
                                vec![String::new()]
                            } else {
                                Default::default()
                            },
                            fpga: if context.fpga {
                                vec![String::new()]
                            } else {
                                Default::default()
                            },
                            disks: IndexMap::from_iter([("".into(), context.disks)]),
                        },
                        base_dir: &EvaluationPath::from_local_path(dir.path().into()),
                        attempt_dir: &dir.path().join("0"),
                        temp_dir: &dir.path().join("tmp"),
                    },
                    &Transferer,
                )
                .await
        }

        // Check for the simple expressions
        assert_eq!(eval(Context::default(), "true").await.unwrap(), true);
        assert_eq!(eval(Context::default(), "false").await.unwrap(), false);
        assert_eq!(eval(Context::default(), "cpu == 1").await.unwrap(), true);
        assert_eq!(
            eval(Context::default(), "memory == 2147483648")
                .await
                .unwrap(),
            true
        );
        assert_eq!(eval(Context::default(), "gpu").await.unwrap(), false);
        assert_eq!(eval(Context::default(), "fpga").await.unwrap(), false);
        assert_eq!(
            eval(Context::default(), "disks == 1073741824")
                .await
                .unwrap(),
            true
        );
        assert_eq!(
            eval(Context::default(), "defined(hint.foo)").await.unwrap(),
            false
        );

        // Check a comprehensive expression
        assert_eq!(
            eval(
                Context {
                    cpu: 10.,
                    memory: 10 * 1024 * 1024,
                    gpu: true,
                    fpga: true,
                    disks: 1024 * 1024,
                    inputs: Default::default(),
                    hints: Object::new(IndexMap::from_iter([(
                        "foo".into(),
                        "hi".to_string().into()
                    )]))
                },
                r#"cpu == 10 && memory == 10*1024*1024 && gpu && fpga && disks == 1024 * 1024 && hint.foo == "hi""#
            )
            .await
            .unwrap(),
            true
        );

        // Check for input hint override
        let mut context = Context {
            hints: Object::new(IndexMap::from_iter([(
                "foo".into(),
                "hi".to_string().into(),
            )])),
            ..Default::default()
        };
        context
            .inputs
            .override_hint("foo", "overridden!".to_string());
        assert_eq!(
            eval(context, r#"hint.foo == "overridden!""#).await.unwrap(),
            true
        );
    }
}
