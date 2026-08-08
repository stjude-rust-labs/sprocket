//! Implementation of evaluation for V1 documents.

mod expr;
mod task;
mod validators;
mod workflow;

use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
pub(crate) use expr::*;
use serde::Serialize;
pub(crate) use task::*;
use tokio::sync::broadcast;
use tracing::info;
use wdl_analysis::types::EnumChoiceCacheKey;

use super::CancellationContext;
use super::Events;
use crate::EngineEvent;
use crate::INITIAL_EXPECTED_NAMES;
use crate::Value;
use crate::backend::TaskExecutionBackend;
use crate::cache::CallCache;
use crate::cache::CallCacheExclusions;
use crate::config::CallCachingMode;
use crate::config::Config;
use crate::http::HttpTransferer;
use crate::http::Transferer;

/// The name of the inputs file to write for each task and workflow in the
/// outputs directory.
const INPUTS_FILE: &str = "inputs.json";

/// The name of the outputs file to write for each task and workflow in the
/// outputs directory.
const OUTPUTS_FILE: &str = "outputs.json";

/// Serializes a value into a JSON file.
fn write_json_file(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    let path = path.as_ref();
    let file = File::create(path)
        .with_context(|| format!("failed to create file `{path}`", path = path.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), value)
        .with_context(|| format!("failed to write file `{path}`", path = path.display()))
}

/// Represents a WDL evaluator.
///
/// The evaluator is used to evaluate a specific task or the workflow of an
/// analyzed document.
///
/// This type is cheaply cloned and sendable between threads.
#[derive(Clone)]
pub struct Evaluator {
    /// The associated evaluation configuration.
    config: Arc<Config>,
    /// The associated task execution backend.
    backend: Arc<dyn TaskExecutionBackend>,
    /// The cancellation context for cancelling task evaluation.
    cancellation: CancellationContext,
    /// The transferer to use for expression evaluation.
    transferer: Arc<dyn Transferer>,
    /// The call cache to use for task evaluation.
    cache: Option<CallCache>,
    /// The events for evaluation.
    events: Option<broadcast::Sender<EngineEvent>>,
    /// The generator for unique task names.
    ///
    /// Task names are minted by the evaluator rather than by the backend so
    /// that a task can be identified by consumers of [`EngineEvent`] before it
    /// is submitted for execution.
    names: Arc<Mutex<GeneratorIterator<UniqueAlphanumeric>>>,
    /// Cache for evaluated enum choice values to avoid redundant AST lookups.
    choice_cache: Arc<Mutex<HashMap<EnumChoiceCacheKey, Value>>>,
}

impl Evaluator {
    /// Constructs a new evaluator with the given evaluation root directory,
    /// evaluation configuration, cancellation context, and events.
    ///
    /// Returns an error if the configuration isn't valid.
    pub async fn new(
        root_dir: impl AsRef<Path>,
        config: Arc<Config>,
        cancellation: CancellationContext,
        events: Events,
    ) -> Result<Self> {
        config
            .validate()
            .await
            .context("failed to validate configuration")?;

        let root_dir = root_dir.as_ref();
        let backend = config
            .create_backend(root_dir, events.clone(), cancellation.clone())
            .await
            .context("failed to create task execution backend")?;

        let transferer = Arc::new(HttpTransferer::new(
            config.clone(),
            cancellation.first(),
            events.transfer().clone(),
        )?);

        let cache = match config.task.cache {
            CallCachingMode::Off => {
                info!("call caching is disabled");
                None
            }
            _ => Some(
                CallCache::new(
                    config.task.cache_dir().as_deref(),
                    config.task.digests,
                    transferer.clone(),
                    Arc::new(CallCacheExclusions {
                        inputs: HashSet::from_iter(
                            config.task.excluded_cache_inputs.iter().cloned(),
                        ),
                        requirements: HashSet::from_iter(
                            config.task.excluded_cache_requirements.iter().cloned(),
                        ),
                        hints: HashSet::from_iter(config.task.excluded_cache_hints.iter().cloned()),
                    }),
                )
                .await?,
            ),
        };

        Ok(Self {
            config,
            backend,
            cancellation,
            transferer,
            cache,
            events: events.engine().clone(),
            names: Arc::new(Mutex::new(GeneratorIterator::new(
                UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
                INITIAL_EXPECTED_NAMES,
            ))),
            choice_cache: Default::default(),
        })
    }

    /// Generates a unique name for an execution attempt of the given task id.
    ///
    /// The name is what identifies the attempt in every event the engine and
    /// the backends emit for it.
    fn generate_task_name(&self, id: &str) -> String {
        format!(
            "{id}-{generated}",
            generated = self
                .names
                .lock()
                .expect("generator should always acquire")
                .next()
                .expect("generator should never be exhausted")
        )
    }

    /// Notifies that a task has started evaluating an execution attempt.
    fn notify_task_initializing(&self, id: &str, name: &str) {
        if let Some(sender) = &self.events {
            let _ = sender.send(EngineEvent::TaskInitializing {
                id: id.to_string(),
                name: name.to_string(),
            });
        }
    }

    /// Notifies that a task has started transferring its inputs.
    fn notify_task_localizing(&self, name: &str) {
        if let Some(sender) = &self.events {
            let _ = sender.send(EngineEvent::TaskLocalizing {
                name: name.to_string(),
            });
        }
    }
}
