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
use crate::http::HttpTransferer;
use crate::http::Transferer;
use crate::v1;

/// The inner workings of [`Engine`].
struct EngineInner {
    /// The configuration for evaluation.
    config: Arc<Config>,
    /// The task execution backend for evaluation.
    backend: Box<dyn TaskExecutionBackend>,
    /// The transferer for evaluation.
    transferer: Box<dyn Transferer>,
    /// The call cache for evaluation.
    ///
    /// This is `None` when the call cache is disabled.
    call_cache: Option<CallCache>,
}

/// Represents a WDL evaluation engine.
///
/// A WDL engine will share configuration, a task execution backend, a file
/// transferer, and various caches.
///
/// Typically there will be one WDL evaluation engine per process.
///
/// See the [`create_v1_evaluator`](Self::create_v1_evaluator) method for
/// creating new [`Evaluator`](crate::v1::Evaluator) from the engine.
///
/// This type is cheaply cloned.
///
/// Note: as the engine holds a reference to the provided [`Events`], the engine
/// and all evaluators created by the engine must be dropped prior to waiting
/// for event subscribers to close.
#[derive(Clone)]
pub struct Engine(Arc<EngineInner>);

impl Engine {
    /// Constructs a new engine given the evaluation configuration.
    ///
    /// This method uses the default HTTP transferer for transferring files.
    pub async fn new(config: Config) -> Result<Self> {
        let transferer = HttpTransferer::new(&config)?;
        Self::new_with_transferer(config, transferer).await
    }

    /// Constructs a new engine with the given evaluation configuration and file
    /// transferer.
    pub async fn new_with_transferer<T>(config: Config, transferer: T) -> Result<Self>
    where
        T: Transferer + 'static,
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

        Ok(Self(
            EngineInner {
                config,
                backend,
                transferer: Box::new(transferer),
                call_cache,
            }
            .into(),
        ))
    }

    /// Creates a new WDL 1.x evaluator from the engine using the provided
    /// events and cancellation context.
    pub fn create_v1_evaluator(
        &self,
        events: Events,
        cancellation: CancellationContext,
    ) -> v1::Evaluator {
        v1::Evaluator::new(self.clone(), events, cancellation)
    }

    /// Gets the configuration associated with the engine.
    pub fn config(&self) -> &Config {
        &self.0.config
    }

    /// Gets the file transferer associated with the engine.
    pub(crate) fn transferer(&self) -> &dyn Transferer {
        self.0.transferer.as_ref()
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
