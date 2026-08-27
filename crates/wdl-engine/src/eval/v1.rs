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
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
pub(crate) use expr::*;
use serde::Serialize;
pub(crate) use task::*;
use wdl_analysis::types::EnumChoiceCacheKey;

use super::CancellationContext;
use super::Events;
use crate::Engine;
use crate::EngineEvent;
use crate::INITIAL_EXPECTED_NAMES;
use crate::Value;

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

/// Represents a WDL 1.x evaluator.
///
/// The evaluator is used to evaluate a task or the workflow of an analyzed
/// document.
///
/// To create an [`Evaluator`], see [`Engine::create_v1_evaluator`].
///
/// This type is cheaply cloned and sendable between threads.
///
/// Note: as the evaluator internally holds a reference to the provided
/// [`Events`], the evaluator must be dropped prior to waiting for event
/// subscribers to close.
#[derive(Clone)]
pub struct Evaluator {
    /// The engine for the evaluator.
    engine: Engine,
    /// The events to use for evaluation.
    events: Events,
    /// The cancellation context for cancelling task evaluation.
    cancellation: CancellationContext,
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
    /// Constructs a new evaluator with the given engine, events, and
    /// cancellation context.
    pub(crate) fn new(engine: Engine, events: Events, cancellation: CancellationContext) -> Self {
        Self {
            engine,
            events,
            cancellation,
            names: Arc::new(Mutex::new(GeneratorIterator::new(
                UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
                INITIAL_EXPECTED_NAMES,
            ))),
            choice_cache: Default::default(),
        }
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
        if let Some(sender) = &self.events.engine {
            let _ = sender.send(EngineEvent::TaskInitializing {
                id: id.to_string(),
                name: name.to_string(),
            });
        }
    }

    /// Notifies that a task has started transferring its inputs.
    fn notify_task_localizing(&self, name: &str) {
        if let Some(sender) = &self.events.engine {
            let _ = sender.send(EngineEvent::TaskLocalizing {
                name: name.to_string(),
            });
        }
    }
}
