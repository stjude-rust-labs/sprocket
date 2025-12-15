//! Implementation of evaluation for V1 documents.

mod expr;
mod task;
mod validators;
mod workflow;

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
pub use expr::*;
use serde::Serialize;
pub use task::*;
use tokio::sync::broadcast;
use tracing::info;

use super::CancellationContext;
use super::Events;
use crate::EngineEvent;
use crate::TaskExecutionBackend;
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

/// The top-level evaluation context.
///
/// "Top-level" here means the outermost invocation of a task or workflow across
/// an entire execution. This type is suitable for once-per-execution values
/// like a shared container image cache, and new instances should not be created
/// for evaluating subgraphs of the initial execution.
///
/// This type is meant to be cheaply cloned and sendable between threads. When
/// adding to it, make sure to use an `Arc` for any non-trivially-sized data.
#[derive(Clone)]
pub struct TopLevelEvaluator {
    /// The root directory of this evaluation.
    #[expect(unused, reason = "future refactoring will remove redundant arguments")]
    root_dir: PathBuf,
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
}

impl TopLevelEvaluator {
    /// Constructs a new task evaluator with the given evaluation root
    /// directory, evaluation configuration, cancellation context, and
    /// events sender.
    ///
    /// Returns an error if the configuration isn't valid.
    pub async fn new(
        root_dir: &Path,
        config: Arc<Config>,
        cancellation: CancellationContext,
        events: Events,
    ) -> Result<Self> {
        config.validate().await?;

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
            _ => Some(CallCache::new(config.task.cache_dir.as_deref(), transferer.clone()).await?),
        };

        Ok(Self {
            root_dir: root_dir.to_path_buf(),
            config,
            backend,
            cancellation,
            transferer,
            cache,
            events: events.engine().clone(),
        })
    }
}
