//! Implementation of evaluation for V1 documents.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use tracing::info;
use tracing::warn;

use super::CancellationContext;
use crate::Events;
use crate::backend::TaskExecutionBackend;
use crate::cache::CallCache;
use crate::cache::CallCacheExclusions;
use crate::config::BackendConfig;
use crate::config::CallCachingMode;
use crate::config::Config;
use crate::http::DefaultHttpClient;
use crate::http::HttpClient;
use crate::v1;

/// The inner state of [`Engine`].
struct EngineInner {
    /// The configuration for evaluation.
    config: Arc<Config>,
    /// The task execution backend for evaluation.
    backend: Box<dyn TaskExecutionBackend>,
    /// The HTTP client to use for evaluation.
    http_client: Box<dyn HttpClient>,
    /// The call cache for evaluation.
    ///
    /// This is `None` when the call cache is disabled.
    call_cache: Option<CallCache>,
}

/// Represents a WDL evaluation engine.
///
/// A WDL engine will share configuration, a task execution backend, and call
/// cache.
///
/// Typically there will be one WDL evaluation engine per process.
///
/// See the [`create_v1_evaluator`](Self::create_v1_evaluator) method for
/// creating new [`Evaluator`](crate::v1::Evaluator) from the engine.
///
/// This type is cheaply cloned.
#[derive(Clone)]
pub struct Engine(Arc<EngineInner>);

impl Engine {
    /// Constructs a new engine given the evaluation configuration.
    pub async fn new(config: Config) -> Result<Self> {
        let client = DefaultHttpClient::new(&config)?;
        Self::new_with_http_client(config, client).await
    }

    /// Constructs a new engine given the evaluation configuration and HTTP
    /// client to use.
    pub(crate) async fn new_with_http_client<T>(config: Config, client: T) -> Result<Self>
    where
        T: HttpClient + 'static,
    {
        config
            .validate()
            .await
            .context("failed to validate configuration")?;

        let config = Arc::new(config);

        let backend = Self::create_backend(config.clone())
            .await
            .context("failed to create task execution backend")?;

        let call_cache = match config.task.cache {
            CallCachingMode::Off => {
                info!("call caching is disabled");
                None
            }
            _ => Some(
                CallCache::new(
                    config.task.cache_dir()?,
                    config.task.digests,
                    CallCacheExclusions {
                        inputs: HashSet::from_iter(
                            config.task.excluded_cache_inputs.iter().cloned(),
                        ),
                        requirements: HashSet::from_iter(
                            config.task.excluded_cache_requirements.iter().cloned(),
                        ),
                        hints: HashSet::from_iter(config.task.excluded_cache_hints.iter().cloned()),
                    },
                )
                .await?,
            ),
        };

        Ok(Self(Arc::new(EngineInner {
            config,
            backend,
            http_client: Box::new(client),
            call_cache,
        })))
    }

    /// Creates a new WDL 1.x evaluator from the engine using the provided
    /// events and cancellation context.
    pub fn create_v1_evaluator(
        &self,
        events: Events,
        cancellation: CancellationContext,
    ) -> v1::Evaluator {
        v1::Evaluator::new(self, events, cancellation)
    }

    /// Gets the configuration associated with the engine.
    pub fn config(&self) -> &Arc<Config> {
        &self.0.config
    }

    /// Gets the HTTP client associated with the engine.
    pub(crate) fn http_client(&self) -> &dyn HttpClient {
        self.0.http_client.as_ref()
    }

    /// Gets the task execution backend associated with the engine.
    pub(crate) fn backend(&self) -> &dyn TaskExecutionBackend {
        self.0.backend.as_ref()
    }

    /// Gets the call cache associated with the engine.
    pub(crate) fn call_cache(&self) -> Option<&CallCache> {
        self.0.call_cache.as_ref()
    }

    /// Creates a new task execution backend based on the given configuration.
    async fn create_backend(config: Arc<Config>) -> Result<Box<dyn TaskExecutionBackend>> {
        use crate::backend::*;

        match config.backend()?.as_ref() {
            BackendConfig::Local { .. } => {
                warn!(
                    "the engine is configured to use the local backend: tasks will not be run \
                     inside of a container"
                );
                Ok(Box::new(LocalBackend::new(config)?))
            }
            BackendConfig::Docker { .. } => Ok(Box::new(DockerBackend::new(config).await?)),
            BackendConfig::Tes { .. } => Ok(Box::new(TesBackend::new(config).await?)),
            BackendConfig::LsfApptainer { .. } => {
                Ok(Box::new(LsfApptainerBackend::new(config).await?))
            }
            BackendConfig::SlurmApptainer { .. } => {
                Ok(Box::new(SlurmApptainerBackend::new(config).await?))
            }
        }
    }
}
