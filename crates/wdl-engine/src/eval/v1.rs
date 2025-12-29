//! Implementation of evaluation for V1 documents.

mod expr;
mod task;
mod validators;
mod workflow;

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
pub(crate) use expr::*;
use parking_lot::RwLock;
use serde::Serialize;
pub(crate) use task::*;
use tokio::sync::broadcast;
use tracing::info;

use super::CancellationContext;
use super::Events;
use crate::EngineEvent;
use crate::backend::TaskExecutionBackend;
use crate::cache::CallCache;
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
    /// Cache for evaluated enum variant values to avoid redundant AST lookups.
    variant_cache: Arc<RwLock<HashMap<(String, String), crate::Value>>>,
}

impl Evaluator {
    /// Constructs a new evaluator with the given evaluation root directory,
    /// evaluation configuration, cancellation context, and events.
    ///
    /// Returns an error if the configuration isn't valid.
    pub async fn new(
        root_dir: impl AsRef<Path>,
        config: Config,
        cancellation: CancellationContext,
        events: Events,
    ) -> Result<Self> {
        config.validate().await?;

        let root_dir = root_dir.as_ref();
        let config = Arc::new(config);
        let backend = config
            .create_backend(root_dir, events.crankshaft().clone())
            .await?;
        let transferer = Arc::new(HttpTransferer::new(
            config.clone(),
            cancellation.token(),
            events.transfer().clone(),
        )?);

        let cache = match config.task.cache {
            CallCachingMode::Off => {
                info!("call caching is disabled");
                None
            }
            _ => Some(
                CallCache::new(
                    config.task.cache_dir.as_deref(),
                    config.task.digests,
                    transferer.clone(),
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
            variant_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}
