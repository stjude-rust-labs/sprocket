//! Implementation of evaluation for V1 documents.

mod expr;
mod task;
mod validators;
mod workflow;

use std::fs::File;
use std::io::BufWriter;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
pub(crate) use expr::*;
use lru::LruCache;
use regex::Regex;
use serde::Serialize;
pub(crate) use task::*;
use wdl_analysis::Document;
use wdl_analysis::types::EnumChoiceCacheKey;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;

use super::CancellationContext;
use super::Events;
use crate::Engine;
use crate::EngineEvent;
use crate::EvaluationHttpClient;
use crate::INITIAL_EXPECTED_NAMES;
use crate::PrimitiveValue;
use crate::Value;
use crate::diagnostics::unknown_enum;
use crate::diagnostics::unknown_enum_choice;
use crate::digest::DigestCalculator;

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

/// The inner state of [`Evaluator`].
struct EvaluatorInner {
    /// The engine associated with the evaluator.
    engine: Engine,
    /// The evaluation HTTP client.
    http_client: EvaluationHttpClient,
    /// The events to use for evaluation.x
    events: Events,
    /// The cancellation context for cancelling task evaluation.
    cancellation: CancellationContext,
    /// The generator for unique task names.
    ///
    /// Task names are minted by the evaluator rather than by the backend so
    /// that a task can be identified by consumers of [`EngineEvent`] before it
    /// is submitted for execution.
    names: Mutex<GeneratorIterator<UniqueAlphanumeric>>,
    /// The cache for evaluated enum choice values to avoid redundant AST
    /// lookups.
    choice_cache: Mutex<LruCache<EnumChoiceCacheKey, Result<Value, Diagnostic>>>,
    /// The cache for compiled regular expressions.
    regex_cache: Mutex<LruCache<String, Result<Regex, regex::Error>>>,
    /// The digest calculator to use for files and directories.
    digests: DigestCalculator,
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
pub struct Evaluator(Arc<EvaluatorInner>);

impl Evaluator {
    /// Constructs a new evaluator with the given engine, events, and
    /// cancellation context.
    pub(crate) fn new(engine: &Engine, events: Events, cancellation: CancellationContext) -> Self {
        let http_client = EvaluationHttpClient::new(engine, &events, cancellation.clone());

        let digests = DigestCalculator::new(
            http_client.clone(),
            cancellation.clone(),
            engine.config().digest_cache_capacity as usize,
        );

        Self(
            EvaluatorInner {
                engine: engine.clone(),
                http_client,
                events,
                cancellation,
                names: Mutex::new(GeneratorIterator::new(
                    UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
                    INITIAL_EXPECTED_NAMES,
                )),
                choice_cache: Mutex::new(LruCache::new(
                    NonZeroUsize::new(engine.config().choice_cache_capacity as usize)
                        .expect("expected a non-zero choice cache capacity"),
                )),
                regex_cache: Mutex::new(LruCache::new(
                    NonZeroUsize::new(engine.config().regex_cache_capacity as usize)
                        .expect("expected a non-zero regex cache capacity"),
                )),
                digests,
            }
            .into(),
        )
    }

    /// Gets the [`Engine`] associated with the evaluator.
    fn engine(&self) -> &Engine {
        &self.0.engine
    }

    /// Gets the [`EvaluationHttpClient`] associated with the evaluator.
    fn http_client(&self) -> &EvaluationHttpClient {
        &self.0.http_client
    }

    /// Gets the [`Events`] associated with the evaluator.
    fn events(&self) -> &Events {
        &self.0.events
    }

    /// Gets the [`CancellationContext`] associated with the evaluator.
    fn cancellation(&self) -> &CancellationContext {
        &self.0.cancellation
    }

    /// Gets the [`DigestCalculator`] associated with the evaluator.
    fn digests(&self) -> &DigestCalculator {
        &self.0.digests
    }

    /// Generates a unique name for an execution attempt of the given task id.
    ///
    /// The name is what identifies the attempt in every event the engine and
    /// the backends emit for it.
    fn generate_task_name(&self, id: &str) -> String {
        format!(
            "{id}-{generated}",
            generated = self
                .0
                .names
                .lock()
                .expect("generator should always acquire")
                .next()
                .expect("generator should never be exhausted")
        )
    }

    /// Notifies that a task has started evaluating an execution attempt.
    fn notify_task_initializing(&self, id: &str, name: &str) {
        if let Some(sender) = &self.0.events.engine {
            let _ = sender.send(EngineEvent::TaskInitializing {
                id: id.to_string(),
                name: name.to_string(),
            });
        }
    }

    /// Notifies that a task has started transferring its inputs.
    fn notify_task_localizing(&self, name: &str) {
        if let Some(sender) = &self.0.events.engine {
            let _ = sender.send(EngineEvent::TaskLocalizing {
                name: name.to_string(),
            });
        }
    }

    /// Gets a value for the given choice name of the given enum.
    fn enum_choice_value(
        &self,
        document: &Document,
        enum_name: &str,
        choice_name: &str,
    ) -> Result<Value, Diagnostic> {
        let cache_key = document
            .get_choice_cache_key(enum_name, choice_name)
            .ok_or_else(|| unknown_enum(enum_name))?;

        let mut cache = self
            .0
            .choice_cache
            .lock()
            .expect("failed to lock choice cache");
        cache
            .get_or_insert_ref(&cache_key, || {
                let e = document
                    .enum_by_name(enum_name)
                    .ok_or(unknown_enum(enum_name))?;

                // SAFETY: we can assume that any type associated with an
                // [`Enum`] entry is an [`EnumType`] at this
                // point in analysis.
                let ty = e.ty().unwrap().as_enum().unwrap();

                let choice = e
                    .definition()
                    .choices()
                    .find(|choice| choice.name().text() == choice_name)
                    .ok_or(unknown_enum_choice(ty.name(), choice_name))?;

                if let Some(value_expr) = choice.value() {
                    // SAFETY: see the panic notice for this function.
                    Ok(expr::parse_constant_value(ty.inner_value_type(), &value_expr).unwrap())
                } else {
                    // NOTE: when no expression is provided, the default is the
                    // choice name as a string.
                    Ok(Value::Primitive(PrimitiveValue::new_string(choice_name)))
                }
            })
            .clone()
    }

    /// Compiles a regular expression.
    ///
    /// If the provided regular expression exists in the cache, the previously
    /// compiled regular expression is returned.
    fn compile_regex(&self, pattern: &str) -> Result<Regex, regex::Error> {
        let mut cache = self
            .0
            .regex_cache
            .lock()
            .expect("failed to lock regex cache");
        cache
            .get_or_insert_ref(pattern, || Regex::new(pattern))
            .clone()
    }
}
