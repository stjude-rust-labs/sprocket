//! Implementation of evaluation for V1 tasks.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::mem;
use std::path::Path;
use std::path::absolute;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use bimap::BiHashMap;
use indexmap::IndexMap;
use petgraph::algo::toposort;
use tokio::task::JoinSet;
use tracing::Level;
use tracing::debug;
use tracing::enabled;
use tracing::error;
use tracing::info;
use tracing::warn;
use wdl_analysis::Document;
use wdl_analysis::diagnostics::Io;
use wdl_analysis::diagnostics::multiple_type_mismatch;
use wdl_analysis::diagnostics::unknown_name;
use wdl_analysis::document::TASK_VAR_NAME;
use wdl_analysis::document::Task;
use wdl_analysis::eval::v1::TaskGraphBuilder;
use wdl_analysis::eval::v1::TaskGraphNode;
use wdl_analysis::types::Optional;
use wdl_analysis::types::PrimitiveType;
use wdl_analysis::types::Type;
use wdl_analysis::types::v1::task_hint_types;
use wdl_analysis::types::v1::task_requirement_types;
use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::Decl;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::StrippedCommandPart;
use wdl_ast::v1::TASK_HINT_CACHEABLE;
use wdl_ast::v1::TASK_HINT_DISKS;
use wdl_ast::v1::TASK_HINT_GPU;
use wdl_ast::v1::TASK_HINT_MAX_CPU;
use wdl_ast::v1::TASK_HINT_MAX_CPU_ALIAS;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CPU;
use wdl_ast::v1::TASK_REQUIREMENT_DISKS;
use wdl_ast::v1::TASK_REQUIREMENT_GPU;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_MEMORY;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::version::V1;

use super::TopLevelEvaluator;
use super::validators::SettingSource;
use super::validators::ensure_non_negative_i64;
use super::validators::invalid_numeric_value_message;
use crate::CancellationContextState;
use crate::Coercible;
use crate::ContentKind;
use crate::EngineEvent;
use crate::EvaluationContext;
use crate::EvaluationError;
use crate::EvaluationResult;
use crate::GuestPath;
use crate::HiddenValue;
use crate::HostPath;
use crate::Input;
use crate::ONE_GIBIBYTE;
use crate::Object;
use crate::Outputs;
use crate::Scope;
use crate::ScopeIndex;
use crate::ScopeRef;
use crate::StorageUnit;
use crate::TaskInputs;
use crate::TaskPostEvaluationData;
use crate::TaskPostEvaluationValue;
use crate::TaskPreEvaluationValue;
use crate::Value;
use crate::backend::TaskSpawnInfo;
use crate::backend::TaskSpawnRequest;
use crate::cache::KeyRequest;
use crate::config::CallCachingMode;
use crate::config::Config;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::MAX_RETRIES;
use crate::convert_unit_string;
use crate::diagnostics::decl_evaluation_failed;
use crate::diagnostics::runtime_type_mismatch;
use crate::diagnostics::task_execution_failed;
use crate::diagnostics::task_localization_failed;
use crate::diagnostics::unknown_enum;
use crate::eval::EvaluatedTask;
use crate::eval::trie::InputTrie;
use crate::http::Transferer;
use crate::path::EvaluationPath;
use crate::path::is_file_url;
use crate::path::is_supported_url;
use crate::tree::SyntaxNode;
use crate::v1::INPUTS_FILE;
use crate::v1::OUTPUTS_FILE;
use crate::v1::expr::ExprEvaluator;
use crate::v1::write_json_file;

/// The default container requirement.
pub const DEFAULT_TASK_REQUIREMENT_CONTAINER: &str = "ubuntu:latest";
/// The default value for the `cpu` requirement.
pub const DEFAULT_TASK_REQUIREMENT_CPU: f64 = 1.0;
/// The default value for the `memory` requirement.
pub const DEFAULT_TASK_REQUIREMENT_MEMORY: i64 = 2 * (ONE_GIBIBYTE as i64);
/// The default value for the `max_retries` requirement.
pub const DEFAULT_TASK_REQUIREMENT_MAX_RETRIES: u64 = 0;
/// The default value for the `disks` requirement (in GiB).
pub const DEFAULT_TASK_REQUIREMENT_DISKS: f64 = 1.0;
/// The default GPU count when a GPU is required but no supported hint is
/// provided.
pub const DEFAULT_GPU_COUNT: u64 = 1;

/// The index of a task's root scope.
const ROOT_SCOPE_INDEX: ScopeIndex = ScopeIndex::new(0);
/// The index of a task's output scope.
const OUTPUT_SCOPE_INDEX: ScopeIndex = ScopeIndex::new(1);
/// The index of the evaluation scope where the WDL 1.2 `task` variable is
/// visible.
const TASK_SCOPE_INDEX: ScopeIndex = ScopeIndex::new(2);

/// Returns the first entry in `map` that matches the provided keys.
fn lookup_entry<'a>(
    map: &'a HashMap<String, Value>,
    keys: &[&'static str],
) -> Option<(&'static str, &'a Value)> {
    keys.iter()
        .find_map(|key| map.get(*key).map(|value| (*key, value)))
}

/// Parses an integer or byte-unit string into a byte count using the supplied
/// `error_message` formatter when conversion fails.
///
/// # Panics
///
/// Panics if the given value is not an integer or string.
fn parse_storage_value(value: &Value, error_message: impl Fn(&str) -> String) -> Result<i64> {
    if let Some(v) = value.as_integer() {
        return Ok(v);
    }

    if let Some(s) = value.as_string() {
        return convert_unit_string(s)
            .and_then(|v| v.try_into().ok())
            .with_context(|| error_message(s));
    }

    unreachable!("value should be an integer or string");
}

/// Gets the `container` requirement from a requirements map.
pub(crate) fn container<'a>(
    requirements: &'a HashMap<String, Value>,
    default: Option<&'a str>,
) -> Cow<'a, str> {
    requirements
        .get(TASK_REQUIREMENT_CONTAINER)
        .or_else(|| requirements.get(TASK_REQUIREMENT_CONTAINER_ALIAS))
        .and_then(|v| -> Option<Cow<'_, str>> {
            // If the value is an array, use the first element or the default
            // Note: in the future we should be resolving which element in the array is
            // usable; this will require some work in Crankshaft to enable
            if let Some(array) = v.as_array() {
                return array.as_slice().first().map(|v| {
                    v.as_string()
                        .expect("type should be string")
                        .as_ref()
                        .into()
                });
            }

            Some(
                v.coerce(None, &PrimitiveType::String.into())
                    .expect("type should coerce")
                    .unwrap_string()
                    .as_ref()
                    .clone()
                    .into(),
            )
        })
        .and_then(|v| {
            // Treat star as the default
            if v == "*" { None } else { Some(v) }
        })
        .unwrap_or_else(|| {
            default
                .map(Into::into)
                .unwrap_or(DEFAULT_TASK_REQUIREMENT_CONTAINER.into())
        })
}

/// Gets the `cpu` requirement from a requirements map.
pub(crate) fn cpu(requirements: &HashMap<String, Value>) -> f64 {
    requirements
        .get(TASK_REQUIREMENT_CPU)
        .map(|v| {
            v.coerce(None, &PrimitiveType::Float.into())
                .expect("type should coerce")
                .unwrap_float()
        })
        .unwrap_or(DEFAULT_TASK_REQUIREMENT_CPU)
}

/// Gets the `max_cpu` hint from a hints map.
pub(crate) fn max_cpu(hints: &HashMap<String, Value>) -> Option<f64> {
    hints
        .get(TASK_HINT_MAX_CPU)
        .or_else(|| hints.get(TASK_HINT_MAX_CPU_ALIAS))
        .map(|v| {
            v.coerce(None, &PrimitiveType::Float.into())
                .expect("type should coerce")
                .unwrap_float()
        })
}

/// Gets the `memory` requirement from a requirements map.
pub(crate) fn memory(requirements: &HashMap<String, Value>) -> Result<i64> {
    if let Some((key, value)) = lookup_entry(requirements, &[TASK_REQUIREMENT_MEMORY]) {
        let bytes = parse_storage_value(value, |raw| {
            invalid_numeric_value_message(SettingSource::Requirement, key, raw)
        })?;

        return ensure_non_negative_i64(SettingSource::Requirement, key, bytes);
    }

    Ok(DEFAULT_TASK_REQUIREMENT_MEMORY)
}

/// Gets the `max_memory` hint from a hints map.
pub(crate) fn max_memory(hints: &HashMap<String, Value>) -> Result<Option<i64>> {
    match lookup_entry(hints, &[TASK_HINT_MAX_MEMORY, TASK_HINT_MAX_MEMORY_ALIAS]) {
        Some((key, value)) => {
            let bytes = parse_storage_value(value, |raw| {
                invalid_numeric_value_message(SettingSource::Hint, key, raw)
            })?;
            ensure_non_negative_i64(SettingSource::Hint, key, bytes).map(Some)
        }
        None => Ok(None),
    }
}

/// Gets the number of required GPUs from requirements and hints.
pub(crate) fn gpu(
    requirements: &HashMap<String, Value>,
    hints: &HashMap<String, Value>,
) -> Option<u64> {
    // If `requirements { gpu: false }` or there is no `gpu` requirement, return
    // `None`.
    let Some(true) = requirements
        .get(TASK_REQUIREMENT_GPU)
        .and_then(|v| v.as_boolean())
    else {
        return None;
    };

    // If there is no `gpu` hint giving us more detail on the request, use the
    // default count.
    let Some(hint) = hints.get(TASK_HINT_GPU) else {
        return Some(DEFAULT_GPU_COUNT);
    };

    // A string `gpu` hint is allowed by the spec, but we do not support them yet.
    // Fall back to the default count.
    //
    // TODO(clay): support string hints for GPU specifications.
    if let Some(hint) = hint.as_string() {
        warn!(
            %hint,
            "string `gpu` hints are not supported; falling back to {DEFAULT_GPU_COUNT} GPU(s)"
        );
        return Some(DEFAULT_GPU_COUNT);
    }

    match hint.as_integer() {
        Some(count) if count >= 1 => Some(count as u64),
        // If the hint is zero or negative, it's not clear what the user intends. Maybe they have
        // tried to disable GPUs by setting the count to zero, or have made a logic error. Emit a
        // warning, and continue with no GPU request.
        Some(count) => {
            warn!(
                %count,
                "`gpu` hint specified {count} GPU(s); no GPUs will be requested for execution"
            );
            None
        }
        None => {
            // Typechecking should have already validated that the hint is an integer or
            // a string.
            unreachable!("`gpu` hint must be an integer or string")
        }
    }
}

/// Represents the type of a disk.
///
/// Disk types are specified via hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiskType {
    /// The disk type is a solid state drive.
    SSD,
    /// The disk type is a hard disk drive.
    HDD,
}

impl FromStr for DiskType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SSD" => Ok(Self::SSD),
            "HDD" => Ok(Self::HDD),
            _ => Err(()),
        }
    }
}

/// Represents a task disk requirement.
pub struct DiskRequirement {
    /// The size of the disk, in GiB.
    pub size: i64,

    /// The disk type as specified by a corresponding task hint.
    pub ty: Option<DiskType>,
}

/// Gets the `disks` requirement.
///
/// Upon success, returns a mapping of mount point to disk requirement.
pub(crate) fn disks<'a>(
    requirements: &'a HashMap<String, Value>,
    hints: &HashMap<String, Value>,
) -> Result<HashMap<&'a str, DiskRequirement>> {
    /// Helper for looking up a disk type from the hints.
    ///
    /// If we don't recognize the specification, we ignore it.
    fn lookup_type(mount_point: Option<&str>, hints: &HashMap<String, Value>) -> Option<DiskType> {
        hints.get(TASK_HINT_DISKS).and_then(|v| {
            if let Some(ty) = v.as_string() {
                return ty.parse().ok();
            }

            if let Some(map) = v.as_map() {
                // Find the corresponding key; we have to scan the keys because the map is
                // storing primitive values
                if let Some((_, v)) = map.iter().find(|(k, _)| match (k, mount_point) {
                    (None, None) => true,
                    (None, Some(_)) | (Some(_), None) => false,
                    (Some(k), Some(mount_point)) => k
                        .as_string()
                        .map(|k| k.as_str() == mount_point)
                        .unwrap_or(false),
                }) {
                    return v.as_string().and_then(|ty| ty.parse().ok());
                }
            }

            None
        })
    }

    /// Parses a disk specification into a size (in GiB) and optional mount
    /// point.
    fn parse_disk_spec(spec: &str) -> Option<(i64, Option<&str>)> {
        let iter = spec.split_whitespace();
        let mut first = None;
        let mut second = None;
        let mut third = None;

        for part in iter {
            if first.is_none() {
                first = Some(part);
                continue;
            }

            if second.is_none() {
                second = Some(part);
                continue;
            }

            if third.is_none() {
                third = Some(part);
                continue;
            }

            return None;
        }

        match (first, second, third) {
            (None, None, None) => None,
            (Some(size), None, None) => {
                // Specification is `<size>` (in GiB)
                Some((size.parse().ok()?, None))
            }
            (Some(first), Some(second), None) => {
                // Check for `<size> <unit>`; convert from the specified unit to GiB
                if let Ok(size) = first.parse() {
                    let unit: StorageUnit = second.parse().ok()?;
                    let size = unit.bytes(size)? / (ONE_GIBIBYTE as u64);
                    return Some((size.try_into().ok()?, None));
                }

                // Specification is `<mount-point> <size>` (where size is already in GiB)
                // The mount point must be absolute, i.e. start with `/`
                if !first.starts_with('/') {
                    return None;
                }

                Some((second.parse().ok()?, Some(first)))
            }
            (Some(mount_point), Some(size), Some(unit)) => {
                // Specification is `<mount-point> <size> <units>`
                let unit: StorageUnit = unit.parse().ok()?;
                let size = unit.bytes(size.parse().ok()?)? / (ONE_GIBIBYTE as u64);

                // Mount point must be absolute
                if !mount_point.starts_with('/') {
                    return None;
                }

                Some((size.try_into().ok()?, Some(mount_point)))
            }
            _ => unreachable!("should have one, two, or three values"),
        }
    }

    /// Inserts a disk into the disks map.
    fn insert_disk<'a>(
        spec: &'a str,
        hints: &HashMap<String, Value>,
        disks: &mut HashMap<&'a str, DiskRequirement>,
    ) -> Result<()> {
        let (size, mount_point) =
            parse_disk_spec(spec).with_context(|| format!("invalid disk specification `{spec}"))?;

        let prev = disks.insert(
            mount_point.unwrap_or("/"),
            DiskRequirement {
                size,
                ty: lookup_type(mount_point, hints),
            },
        );

        if prev.is_some() {
            bail!(
                "duplicate mount point `{mp}` specified in `disks` requirement",
                mp = mount_point.unwrap_or("/")
            );
        }

        Ok(())
    }

    let mut disks = HashMap::new();
    if let Some(v) = requirements.get(TASK_REQUIREMENT_DISKS) {
        if let Some(size) = v.as_integer() {
            // Disk spec is just the size (in GiB)
            if size < 0 {
                bail!("task requirement `disks` cannot be less than zero");
            }

            disks.insert(
                "/",
                DiskRequirement {
                    size,
                    ty: lookup_type(None, hints),
                },
            );
        } else if let Some(spec) = v.as_string() {
            insert_disk(spec, hints, &mut disks)?;
        } else if let Some(v) = v.as_array() {
            for spec in v.as_slice() {
                insert_disk(
                    spec.as_string().expect("spec should be a string"),
                    hints,
                    &mut disks,
                )?;
            }
        } else {
            unreachable!("value should be an integer, string, or array");
        }
    }

    Ok(disks)
}

/// Gets the `preemptible` hint from a hints map.
///
/// This hint is not part of the WDL standard but is used for compatibility with
/// Cromwell where backends can support preemptible retries before using
/// dedicated instances.
pub(crate) fn preemptible(hints: &HashMap<String, Value>) -> Result<i64> {
    const TASK_HINT_PREEMPTIBLE: &str = "preemptible";
    const DEFAULT_TASK_HINT_PREEMPTIBLE: i64 = 0;

    Ok(hints
        .get(TASK_HINT_PREEMPTIBLE)
        .and_then(|v| {
            v.coerce(None, &PrimitiveType::Integer.into())
                .ok()
                .map(|value| value.unwrap_integer())
        })
        .map(|value| ensure_non_negative_i64(SettingSource::Hint, TASK_HINT_PREEMPTIBLE, value))
        .transpose()?
        .unwrap_or(DEFAULT_TASK_HINT_PREEMPTIBLE))
}

/// Gets the `max_retries` requirement from a requirements map with config
/// fallback.
pub(crate) fn max_retries(requirements: &HashMap<String, Value>, config: &Config) -> Result<u64> {
    if let Some((key, value)) = lookup_entry(
        requirements,
        &[
            TASK_REQUIREMENT_MAX_RETRIES,
            TASK_REQUIREMENT_MAX_RETRIES_ALIAS,
        ],
    ) {
        let retries = value
            .as_integer()
            .expect("`max_retries` requirement should be an integer");
        return ensure_non_negative_i64(SettingSource::Requirement, key, retries)
            .map(|value| value as u64);
    }

    Ok(config
        .task
        .retries
        .unwrap_or(DEFAULT_TASK_REQUIREMENT_MAX_RETRIES))
}

/// Gets the `cacheable` hint from a hints map with config fallback.
pub(crate) fn cacheable(hints: &HashMap<String, Value>, config: &Config) -> bool {
    hints
        .get(TASK_HINT_CACHEABLE)
        .and_then(|v| v.as_boolean())
        .unwrap_or(match config.task.cache {
            CallCachingMode::Off | CallCachingMode::Explicit => false,
            CallCachingMode::On => true,
        })
}

/// Used to evaluate expressions in tasks.
struct TaskEvaluationContext<'a, 'b> {
    /// The associated evaluation state.
    state: &'a mut State<'b>,
    /// The current evaluation scope.
    scope: ScopeIndex,
    /// The task work directory.
    ///
    /// This is `None` unless the output section is being evaluated.
    work_dir: Option<&'a EvaluationPath>,
    /// The standard out value to use.
    ///
    /// This field is only available after task execution.
    stdout: Option<&'a Value>,
    /// The standard error value to use.
    ///
    /// This field is only available after task execution.
    stderr: Option<&'a Value>,
    /// Whether or not the evaluation has associated task information.
    ///
    /// This is `true` when evaluating hints sections.
    task: bool,
}

impl<'a, 'b> TaskEvaluationContext<'a, 'b> {
    /// Constructs a new expression evaluation context.
    pub fn new(state: &'a mut State<'b>, scope: ScopeIndex) -> Self {
        Self {
            state,
            scope,
            work_dir: None,
            stdout: None,
            stderr: None,
            task: false,
        }
    }

    /// Sets the task's work directory to use for the evaluation context.
    pub fn with_work_dir(mut self, work_dir: &'a EvaluationPath) -> Self {
        self.work_dir = Some(work_dir);
        self
    }

    /// Sets the stdout value to use for the evaluation context.
    pub fn with_stdout(mut self, stdout: &'a Value) -> Self {
        self.stdout = Some(stdout);
        self
    }

    /// Sets the stderr value to use for the evaluation context.
    pub fn with_stderr(mut self, stderr: &'a Value) -> Self {
        self.stderr = Some(stderr);
        self
    }

    /// Marks the evaluation as having associated task information.
    ///
    /// This is used in evaluating hints sections.
    pub fn with_task(mut self) -> Self {
        self.task = true;
        self
    }
}

impl EvaluationContext for TaskEvaluationContext<'_, '_> {
    fn version(&self) -> SupportedVersion {
        self.state
            .document
            .version()
            .expect("document should have a version")
    }

    fn resolve_name(&self, name: &str, span: Span) -> Result<Value, Diagnostic> {
        // Check if there are any variables with this name and return if so.
        if let Some(var) = ScopeRef::new(&self.state.scopes, self.scope)
            .lookup(name)
            .cloned()
        {
            return Ok(var);
        }

        // If the name is a reference to a struct, return it as a
        // [`Value::TypeNameRef`].
        if let Some(s) = self.state.document.struct_by_name(name) {
            return Ok(Value::TypeNameRef(
                s.ty().expect("struct should have type").clone(),
            ));
        }

        // If the name is a reference to an enum, return it as a [`Value::TypeNameRef`].
        if let Some(e) = self.state.document.enum_by_name(name) {
            return Ok(Value::TypeNameRef(
                e.ty().expect("enum should have type").clone(),
            ));
        }

        Err(unknown_name(name, span))
    }

    fn resolve_type_name(&self, name: &str, span: Span) -> Result<Type, Diagnostic> {
        crate::resolve_type_name(self.state.document, name, span)
    }

    fn enum_variant_value(&self, enum_name: &str, variant_name: &str) -> Result<Value, Diagnostic> {
        let r#enum = self
            .state
            .document
            .enum_by_name(enum_name)
            .ok_or(unknown_enum(enum_name))?;
        crate::resolve_enum_variant_value(r#enum, variant_name)
    }

    fn base_dir(&self) -> &EvaluationPath {
        self.work_dir.unwrap_or(&self.state.base_dir)
    }

    fn temp_dir(&self) -> &Path {
        self.state.temp_dir
    }

    fn stdout(&self) -> Option<&Value> {
        self.stdout
    }

    fn stderr(&self) -> Option<&Value> {
        self.stderr
    }

    fn task(&self) -> Option<&Task> {
        if self.task {
            Some(self.state.task)
        } else {
            None
        }
    }

    fn transferer(&self) -> &dyn Transferer {
        self.state.transferer().as_ref()
    }

    fn host_path(&self, path: &GuestPath) -> Option<HostPath> {
        self.state.path_map.get_by_right(path).cloned()
    }

    fn guest_path(&self, path: &HostPath) -> Option<GuestPath> {
        self.state.path_map.get_by_left(path).cloned()
    }

    fn notify_file_created(&mut self, path: &HostPath) -> Result<()> {
        self.state.insert_backend_input(ContentKind::File, path)?;
        Ok(())
    }
}

/// Represents task evaluation state.
struct State<'a> {
    /// The top-level evaluation context.
    top_level: &'a TopLevelEvaluator,
    /// The temp directory.
    temp_dir: &'a Path,
    /// The base directory for evaluation.
    ///
    /// This is the document's directory.
    ///
    /// When outputs are evaluated, the task's work directory is used as the
    /// base directory.
    base_dir: EvaluationPath,
    /// The document containing the workflow being evaluated.
    document: &'a Document,
    /// The task being evaluated.
    task: &'a Task,
    /// The scopes of the task being evaluated.
    ///
    /// The first scope is the root scope, the second is the output scope, and
    /// the third is the scope where the "task" variable is visible in 1.2+
    /// evaluations.
    scopes: [Scope; 3],
    /// The environment variables of the task.
    ///
    /// Environment variables do not change between retries.
    env: IndexMap<String, String>,
    /// The map of inputs to evaluated values.
    ///
    /// This is used for calculating the call cache key for the task.
    inputs: BTreeMap<String, Value>,
    /// The trie for mapping backend inputs.
    backend_inputs: InputTrie,
    /// A bi-map of host paths and guest paths.
    path_map: BiHashMap<HostPath, GuestPath>,
}

impl<'a> State<'a> {
    /// Get the [`Transferer`] for this evaluation.
    fn transferer(&self) -> &Arc<dyn Transferer> {
        &self.top_level.transferer
    }

    /// Constructs a new task evaluation state.
    fn new(
        top_level: &'a TopLevelEvaluator,
        document: &'a Document,
        task: &'a Task,
        temp_dir: &'a Path,
    ) -> Result<Self> {
        // Tasks have a root scope (index 0), an output scope (index 1), and a `task`
        // variable scope (index 2). The output scope inherits from the root scope and
        // the task scope inherits from the output scope. Inputs and private
        // declarations are evaluated into the root scope. Outputs are evaluated into
        // the output scope. The task scope is used for evaluating expressions in both
        // the command and output sections. Only the `task` variable in WDL 1.2 is
        // introduced into the task scope; in previous WDL versions, the task scope will
        // not have any local names.
        let scopes = [
            Scope::default(),
            Scope::new(ROOT_SCOPE_INDEX),
            Scope::new(OUTPUT_SCOPE_INDEX),
        ];

        let backend_inputs = if let Some(guest_inputs_dir) = top_level.backend.guest_inputs_dir() {
            InputTrie::new_with_guest_dir(guest_inputs_dir)
        } else {
            InputTrie::new()
        };

        let document_path = document.uri();
        let base_dir = EvaluationPath::parent_of(document_path.as_str()).with_context(|| {
            format!(
                "document `{path}` does not have a parent directory",
                path = document.path()
            )
        })?;

        Ok(Self {
            top_level,
            temp_dir,
            base_dir,
            document,
            task,
            scopes,
            env: Default::default(),
            inputs: Default::default(),
            backend_inputs,
            path_map: Default::default(),
        })
    }

    /// Adds backend inputs to the state for any `File` or `Directory` values
    /// referenced by the given value.
    ///
    /// If the backend doesn't use containers, remote inputs are immediately
    /// localized.
    ///
    /// If the backend does use containers, remote inputs are localized during
    /// the call to `localize_inputs`.
    ///
    /// This method also ensures that a `File` or `Directory` paths exist for
    /// WDL 1.2+.
    async fn add_backend_inputs(
        &mut self,
        is_optional: bool,
        value: &mut Value,
        transferer: Arc<dyn Transferer>,
        needs_local_inputs: bool,
    ) -> Result<()> {
        // For WDL 1.2 documents, start by ensuring paths exist.
        // This will replace any non-existent optional paths with `None`
        if self
            .document
            .version()
            .expect("document should have a version")
            >= SupportedVersion::V1(V1::Two)
        {
            *value = value
                .resolve_paths(
                    is_optional,
                    self.base_dir.as_local(),
                    Some(transferer.as_ref()),
                    &|path| Ok(path.clone()),
                )
                .await?;
        }

        // Add inputs to the backend
        let mut urls = Vec::new();
        value.visit_paths(&mut |is_file, path| {
            // Insert a backend input for the path
            if let Some(index) = self.insert_backend_input(
                if is_file {
                    ContentKind::File
                } else {
                    ContentKind::Directory
                },
                path,
            )? {
                // Check to see if there's no guest path for a remote URL that needs to be
                // localized; if so, we must localize it now
                if needs_local_inputs
                    && self.backend_inputs.as_slice()[index].guest_path.is_none()
                    && is_supported_url(path.as_str())
                    && !is_file_url(path.as_str())
                {
                    urls.push((path.clone(), index));
                }
            }

            Ok(())
        })?;

        if urls.is_empty() {
            return Ok(());
        }

        // Download any necessary files
        let mut downloads = JoinSet::new();
        for (url, index) in urls {
            let transferer = transferer.clone();
            downloads.spawn(async move {
                transferer
                    .download(
                        &url.as_str()
                            .parse()
                            .with_context(|| format!("invalid URL `{url}`"))?,
                    )
                    .await
                    .with_context(|| anyhow!("failed to localize `{url}`"))
                    .map(|l| (url, l, index))
            });
        }

        // Wait for the downloads to complete
        while let Some(result) = downloads.join_next().await {
            let (url, location, index) =
                result.unwrap_or_else(|e| Err(anyhow!("download task failed: {e}")))?;

            let guest_path = GuestPath::new(location.to_str().with_context(|| {
                format!(
                    "download location `{location}` is not UTF-8",
                    location = location.display()
                )
            })?);

            // Map the URL to the guest path
            self.path_map.insert(url, guest_path);

            // Finally, set the location of the input
            self.backend_inputs.as_slice_mut()[index].set_location(location);
        }

        Ok(())
    }

    /// Inserts a backend input into the state.
    ///
    /// Responsible for mapping host and guest paths.
    fn insert_backend_input(
        &mut self,
        kind: ContentKind,
        path: &HostPath,
    ) -> Result<Option<usize>> {
        // Insert an input for the path
        if let Some(index) = self
            .backend_inputs
            .insert(kind, path.as_str(), &self.base_dir)?
        {
            // If the input has a guest path, map it
            let input = &self.backend_inputs.as_slice()[index];
            if let Some(guest_path) = &input.guest_path {
                self.path_map.insert(path.clone(), guest_path.clone());
            }

            return Ok(Some(index));
        }

        Ok(None)
    }
}

/// Represents the result of evaluating task sections before execution.
struct EvaluatedSections {
    /// The evaluated command.
    command: String,
    /// The evaluated requirements.
    requirements: Arc<HashMap<String, Value>>,
    /// The evaluated hints.
    hints: Arc<HashMap<String, Value>>,
}

impl TopLevelEvaluator {
    /// Evaluates the given task.
    ///
    /// Upon success, returns the evaluated task.
    pub async fn evaluate_task(
        &self,
        document: &Document,
        task: &Task,
        inputs: &TaskInputs,
        task_eval_root: impl AsRef<Path>,
    ) -> EvaluationResult<EvaluatedTask> {
        // We cannot evaluate a document with errors
        if document.has_errors() {
            return Err(anyhow!("cannot evaluate a document with errors").into());
        }

        let result = self
            .perform_task_evaluation(document, task, inputs, task_eval_root.as_ref(), task.name())
            .await;

        if self.cancellation.user_canceled() {
            return Err(EvaluationError::Canceled);
        }

        result
    }

    /// Performs the evaluation of the given task.
    ///
    /// This method skips checking the document (and its transitive imports) for
    /// analysis errors as the check occurs at the `evaluate` entrypoint.
    pub(crate) async fn perform_task_evaluation(
        &self,
        document: &Document,
        task: &Task,
        inputs: &TaskInputs,
        task_eval_root: &Path,
        id: &str,
    ) -> EvaluationResult<EvaluatedTask> {
        inputs.validate(document, task, None).with_context(|| {
            format!(
                "failed to validate the inputs to task `{task}`",
                task = task.name()
            )
        })?;

        let ast = match document.root().morph().ast() {
            Ast::V1(ast) => ast,
            _ => {
                return Err(
                    anyhow!("task evaluation is only supported for WDL 1.x documents").into(),
                );
            }
        };

        // Find the task in the AST
        let definition = ast
            .tasks()
            .find(|t| t.name().text() == task.name())
            .expect("task should exist in the AST");

        let version = document.version().expect("document should have version");

        // Build an evaluation graph for the task
        let mut diagnostics = Vec::new();
        let graph =
            TaskGraphBuilder::default().build(version, &definition, &mut diagnostics, |name| {
                document.struct_by_name(name).is_some() || document.enum_by_name(name).is_some()
            });
        assert!(
            diagnostics.is_empty(),
            "task evaluation graph should have no diagnostics"
        );

        debug!(
            task_id = id,
            task_name = task.name(),
            document = document.uri().as_str(),
            "evaluating task"
        );

        let task_eval_root = absolute(task_eval_root).with_context(|| {
            format!(
                "failed to determine absolute path of `{path}`",
                path = task_eval_root.display()
            )
        })?;

        // Create the temp directory now as it may be needed for task evaluation
        let temp_dir = task_eval_root.join("tmp");
        fs::create_dir_all(&temp_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        // Write the inputs to the task's root directory
        write_json_file(task_eval_root.join(INPUTS_FILE), inputs)?;

        let mut state = State::new(self, document, task, &temp_dir)?;
        let nodes = toposort(&graph, None).expect("graph should be acyclic");
        let mut current = 0;
        while current < nodes.len() {
            match &graph[nodes[current]] {
                TaskGraphNode::Input(decl) => {
                    state
                        .evaluate_input(id, decl, inputs)
                        .await
                        .map_err(|d| EvaluationError::new(state.document.clone(), d))?;
                }
                TaskGraphNode::Decl(decl) => {
                    state
                        .evaluate_decl(id, decl)
                        .await
                        .map_err(|d| EvaluationError::new(state.document.clone(), d))?;
                }
                TaskGraphNode::Output(_) => {
                    // Stop at the first output
                    break;
                }
                TaskGraphNode::Command(_)
                | TaskGraphNode::Runtime(_)
                | TaskGraphNode::Requirements(_)
                | TaskGraphNode::Hints(_) => {
                    // Skip these sections for now; they'll evaluate in the
                    // retry loop
                }
            }

            current += 1;
        }

        let mut cached;
        let env = Arc::new(mem::take(&mut state.env));
        // Spawn the task in a retry loop
        let mut attempt = 0;
        let mut previous_task_data: Option<Arc<TaskPostEvaluationData>> = None;
        let mut evaluated = loop {
            if self.cancellation.state() != CancellationContextState::NotCanceled {
                return Err(EvaluationError::Canceled);
            }

            let EvaluatedSections {
                command,
                requirements,
                hints,
            } = state
                .evaluate_sections(id, &definition, inputs, attempt, previous_task_data.clone())
                .await?;

            // Get the maximum number of retries, either from the task's requirements or
            // from configuration
            let max_retries = max_retries(&requirements, &self.config)?;

            if max_retries > MAX_RETRIES {
                return Err(anyhow!(
                    "task `max_retries` requirement of {max_retries} cannot exceed {MAX_RETRIES}"
                )
                .into());
            }

            let backend_inputs = state.localize_inputs(id).await?;

            // Calculate the cache key on the first attempt only
            let mut key = if attempt == 0
                && let Some(cache) = &self.cache
            {
                if cacheable(&hints, &self.config) {
                    let request = KeyRequest {
                        document_uri: state.document.uri().as_ref(),
                        task_name: task.name(),
                        inputs: &state.inputs,
                        command: &command,
                        requirements: requirements.as_ref(),
                        hints: hints.as_ref(),
                        container: &container(&requirements, self.config.task.container.as_deref()),
                        shell: self
                            .config
                            .task
                            .shell
                            .as_deref()
                            .unwrap_or(DEFAULT_TASK_SHELL),
                        backend_inputs: &backend_inputs,
                    };

                    match cache.key(request).await {
                        Ok(key) => {
                            debug!(
                                task_id = id,
                                task_name = state.task.name(),
                                document = state.document.uri().as_str(),
                                "task cache key is `{key}`"
                            );
                            Some(key)
                        }
                        Err(e) => {
                            warn!(
                                task_id = id,
                                task_name = state.task.name(),
                                document = state.document.uri().as_str(),
                                "call caching disabled due to cache key calculation failure: {e:#}"
                            );
                            None
                        }
                    }
                } else {
                    // Task wasn't cacheable, explain why.
                    match self.config.task.cache {
                        CallCachingMode::Off => {
                            unreachable!("cache was used despite not being enabled")
                        }
                        CallCachingMode::On => debug!(
                            task_id = id,
                            task_name = state.task.name(),
                            document = state.document.uri().as_str(),
                            "task is not cacheable due to `cacheable` hint being set to `false`"
                        ),
                        CallCachingMode::Explicit => debug!(
                            task_id = id,
                            task_name = state.task.name(),
                            document = state.document.uri().as_str(),
                            "task is not cacheable due to `cacheable` hint not being explicitly \
                             set to `true`"
                        ),
                    }

                    None
                }
            } else {
                None
            };

            // Lookup the results from the cache
            cached = false;
            let result = if let Some(cache_key) = &key {
                match self
                    .cache
                    .as_ref()
                    .expect("should have cache")
                    .get(cache_key)
                    .await
                {
                    Ok(Some(results)) => {
                        info!(
                            task_id = id,
                            task_name = state.task.name(),
                            document = state.document.uri().as_str(),
                            "task execution was skipped due to previous result being present in \
                             the call cache"
                        );

                        // Notify that we've reused a cached execution result.
                        cached = true;
                        if let Some(sender) = &self.events {
                            let _ = sender.send(EngineEvent::ReusedCachedExecutionResult {
                                id: id.to_string(),
                            });
                        }

                        // We're serving the results from the call cache; no need to update, so set
                        // the key to `None`
                        key = None;
                        Some(results)
                    }
                    Ok(None) => {
                        debug!(
                            task_id = id,
                            task_name = state.task.name(),
                            document = state.document.uri().as_str(),
                            "call cache miss for key `{cache_key}`"
                        );
                        None
                    }
                    Err(e) => {
                        info!(
                            task_id = id,
                            task_name = state.task.name(),
                            document = state.document.uri().as_str(),
                            "ignoring call cache entry: {e:#}"
                        );
                        None
                    }
                }
            } else {
                None
            };

            let result = match result {
                Some(result) => result,
                None => {
                    let mut attempt_dir = task_eval_root.clone();
                    attempt_dir.push("attempts");
                    attempt_dir.push(attempt.to_string());
                    let request = TaskSpawnRequest::new(
                        id.to_string(),
                        TaskSpawnInfo::new(
                            command,
                            backend_inputs,
                            requirements.clone(),
                            hints.clone(),
                            env.clone(),
                            self.transferer.clone(),
                        ),
                        attempt,
                        attempt_dir.clone(),
                        task_eval_root.clone(),
                        temp_dir.clone(),
                    );

                    self.backend
                        .spawn(request, self.cancellation.token())
                        .with_context(|| {
                            format!(
                                "failed to spawn task `{name}` in `{path}` (task id `{id}`)",
                                name = task.name(),
                                path = document.path(),
                            )
                        })?
                        .await
                        .expect("failed to receive response from spawned task")
                        .map_err(|e| {
                            EvaluationError::new(
                                state.document.clone(),
                                task_execution_failed(e, task.name(), id, task.name_span()),
                            )
                        })?
                }
            };

            // Update the task variable
            let evaluated = EvaluatedTask::new(cached, result);
            if version >= SupportedVersion::V1(V1::Two) {
                let task = state.scopes[TASK_SCOPE_INDEX.0]
                    .get_mut(TASK_VAR_NAME)
                    .expect("task variable should exist in scope for WDL v1.2+")
                    .as_task_post_evaluation_mut()
                    .expect("task should be a post evaluation task at this point");

                task.set_attempt(attempt.try_into().with_context(|| {
                    format!(
                        "too many attempts were made to run task `{task}`",
                        task = state.task.name()
                    )
                })?);
                task.set_return_code(evaluated.result.exit_code);
            }

            if let Err(e) = evaluated
                .handle_exit(&requirements, state.transferer().as_ref())
                .await
            {
                if attempt >= max_retries {
                    return Err(EvaluationError::new(
                        state.document.clone(),
                        task_execution_failed(e, task.name(), id, task.name_span()),
                    ));
                }

                attempt += 1;

                if let Some(task) = state.scopes[TASK_SCOPE_INDEX.0].names.get(TASK_VAR_NAME) {
                    // SAFETY: task variable should always be TaskPostEvaluation at this point
                    let task = task.as_task_post_evaluation().unwrap();
                    previous_task_data = Some(task.data().clone());
                }

                info!(
                    "retrying execution of task `{name}` (retry {attempt})",
                    name = state.task.name()
                );
                continue;
            }

            // Task execution succeeded; update the cache entry if we have a key
            if let Some(key) = key {
                match self
                    .cache
                    .as_ref()
                    .expect("should have cache")
                    .put(key, &evaluated.result)
                    .await
                {
                    Ok(key) => {
                        debug!(
                            task_id = id,
                            task_name = state.task.name(),
                            document = state.document.uri().as_str(),
                            "updated call cache entry for key `{key}`"
                        );
                    }
                    Err(e) => {
                        error!(
                            "failed to update call cache entry for task `{name}` (task id \
                             `{id}`): cache entry has been discard: {e:#}",
                            name = task.name()
                        );
                    }
                }
            }

            break evaluated;
        };

        // Perform backend cleanup before output evaluation
        if !cached
            && let Some(cleanup) = self
                .backend
                .cleanup(&evaluated.result.work_dir, self.cancellation.token())
        {
            cleanup.await;
        }

        // Evaluate the remaining inputs (unused), and decls, and outputs
        for index in &nodes[current..] {
            match &graph[*index] {
                TaskGraphNode::Decl(decl) => {
                    state
                        .evaluate_decl(id, decl)
                        .await
                        .map_err(|d| EvaluationError::new(state.document.clone(), d))?;
                }
                TaskGraphNode::Output(decl) => {
                    state
                        .evaluate_output(id, decl, &evaluated)
                        .await
                        .map_err(|d| EvaluationError::new(state.document.clone(), d))?;
                }
                _ => {
                    unreachable!(
                        "only declarations and outputs should be evaluated after the command"
                    )
                }
            }
        }

        // Take the output scope and return it in declaration sort order
        let mut outputs: Outputs = mem::take(&mut state.scopes[OUTPUT_SCOPE_INDEX.0]).into();
        if let Some(section) = definition.output() {
            let indexes: HashMap<_, _> = section
                .declarations()
                .enumerate()
                .map(|(i, d)| (d.name().hashable(), i))
                .collect();
            outputs.sort_by(move |a, b| indexes[a].cmp(&indexes[b]))
        }

        // Write the outputs to the task's root directory
        write_json_file(task_eval_root.join(OUTPUTS_FILE), &outputs)?;

        evaluated.outputs = Ok(outputs);
        Ok(evaluated)
    }
}

impl<'a> State<'a> {
    /// Evaluates a task input.
    async fn evaluate_input(
        &mut self,
        id: &str,
        decl: &Decl<SyntaxNode>,
        inputs: &TaskInputs,
    ) -> Result<(), Diagnostic> {
        let name = decl.name();
        let decl_ty = decl.ty();
        let expected_ty = crate::convert_ast_type_v1(self.document, &decl_ty)?;

        // Evaluate the input if not provided one
        let (value, span) = match inputs.get(name.text()) {
            Some(input) => {
                // For WDL 1.2 evaluation, a `None` value when the expected type is non-optional
                // will invoke the default expression
                if input.is_none()
                    && !expected_ty.is_optional()
                    && self
                        .document
                        .version()
                        .map(|v| v >= SupportedVersion::V1(V1::Two))
                        .unwrap_or(false)
                    && let Some(expr) = decl.expr()
                {
                    debug!(
                        task_id = id,
                        task_name = self.task.name(),
                        document = self.document.uri().as_str(),
                        input_name = name.text(),
                        "evaluating input default expression"
                    );

                    let mut evaluator =
                        ExprEvaluator::new(TaskEvaluationContext::new(self, ROOT_SCOPE_INDEX));
                    (evaluator.evaluate_expr(&expr).await?, expr.span())
                } else {
                    (input.clone(), name.span())
                }
            }
            None => match decl.expr() {
                Some(expr) => {
                    debug!(
                        task_id = id,
                        task_name = self.task.name(),
                        document = self.document.uri().as_str(),
                        input_name = name.text(),
                        "evaluating input default expression"
                    );

                    let mut evaluator =
                        ExprEvaluator::new(TaskEvaluationContext::new(self, ROOT_SCOPE_INDEX));
                    (evaluator.evaluate_expr(&expr).await?, expr.span())
                }
                _ => {
                    assert!(expected_ty.is_optional(), "type should be optional");
                    (Value::new_none(expected_ty.clone()), name.span())
                }
            },
        };

        // Coerce the value to the expected type
        let mut value = value
            .coerce(
                Some(&TaskEvaluationContext::new(self, ROOT_SCOPE_INDEX)),
                &expected_ty,
            )
            .map_err(|e| runtime_type_mismatch(e, &expected_ty, name.span(), &value.ty(), span))?;

        // Add any file or directory backend inputs
        self.add_backend_inputs(
            decl_ty.is_optional(),
            &mut value,
            self.transferer().clone(),
            self.top_level.backend.needs_local_inputs(),
        )
        .await
        .map_err(|e| {
            decl_evaluation_failed(
                e,
                self.task.name(),
                true,
                name.text(),
                Some(Io::Input),
                name.span(),
            )
        })?;

        // Insert the name into the scope
        self.scopes[ROOT_SCOPE_INDEX.0].insert(name.text(), value.clone());
        self.inputs.insert(name.text().to_string(), value.clone());

        // Insert an environment variable, if it is one
        if decl.env().is_some() {
            let value = value
                .as_primitive()
                .expect("value should be primitive")
                .raw(Some(&TaskEvaluationContext::new(self, ROOT_SCOPE_INDEX)))
                .to_string();
            self.env.insert(name.text().to_string(), value);
        }

        Ok(())
    }

    /// Evaluates a task private declaration.
    async fn evaluate_decl(&mut self, id: &str, decl: &Decl<SyntaxNode>) -> Result<(), Diagnostic> {
        let name = decl.name();
        debug!(
            task_id = id,
            task_name = self.task.name(),
            document = self.document.uri().as_str(),
            decl_name = name.text(),
            "evaluating private declaration",
        );

        let decl_ty = decl.ty();
        let ty = crate::convert_ast_type_v1(self.document, &decl_ty)?;

        let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(self, ROOT_SCOPE_INDEX));

        let expr = decl.expr().expect("private decls should have expressions");
        let value = evaluator.evaluate_expr(&expr).await?;
        let mut value = value
            .coerce(
                Some(&TaskEvaluationContext::new(self, ROOT_SCOPE_INDEX)),
                &ty,
            )
            .map_err(|e| runtime_type_mismatch(e, &ty, name.span(), &value.ty(), expr.span()))?;

        // Add any file or directory backend inputs
        self.add_backend_inputs(
            decl_ty.is_optional(),
            &mut value,
            self.transferer().clone(),
            self.top_level.backend.needs_local_inputs(),
        )
        .await
        .map_err(|e| {
            decl_evaluation_failed(e, self.task.name(), true, name.text(), None, name.span())
        })?;

        self.scopes[ROOT_SCOPE_INDEX.0].insert(name.text(), value.clone());

        // Insert an environment variable, if it is one
        if decl.env().is_some() {
            let value = value
                .as_primitive()
                .expect("value should be primitive")
                .raw(Some(&TaskEvaluationContext::new(self, ROOT_SCOPE_INDEX)))
                .to_string();
            self.env.insert(name.text().to_string(), value);
        }

        Ok(())
    }

    /// Evaluates the runtime section.
    ///
    /// Returns both the task's hints and requirements.
    async fn evaluate_runtime_section(
        &mut self,
        id: &str,
        section: &RuntimeSection<SyntaxNode>,
        inputs: &TaskInputs,
    ) -> Result<(HashMap<String, Value>, HashMap<String, Value>), Diagnostic> {
        debug!(
            task_id = id,
            task_name = self.task.name(),
            document = self.document.uri().as_str(),
            "evaluating runtimes section",
        );

        let mut requirements = HashMap::new();
        let mut hints = HashMap::new();

        let version = self
            .document
            .version()
            .expect("document should have version");

        // In WDL 1.3+, use `TASK_SCOPE_INDEX` to access the `task` variable.
        let scope_index = if version >= SupportedVersion::V1(V1::Three) {
            TASK_SCOPE_INDEX
        } else {
            ROOT_SCOPE_INDEX
        };

        for item in section.items() {
            let name = item.name();
            match inputs.requirement(name.text()) {
                Some(value) => {
                    requirements.insert(name.text().to_string(), value.clone());
                    continue;
                }
                _ => {
                    if let Some(value) = inputs.hint(name.text()) {
                        hints.insert(name.text().to_string(), value.clone());
                        continue;
                    }
                }
            }

            let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(self, scope_index));

            let (types, requirement) = match task_requirement_types(version, name.text()) {
                Some(types) => (Some(types), true),
                None => match task_hint_types(version, name.text(), false) {
                    Some(types) => (Some(types), false),
                    None => (None, false),
                },
            };

            // Evaluate and coerce to the expected type
            let expr = item.expr();
            let mut value = evaluator.evaluate_expr(&expr).await?;
            if let Some(types) = types {
                value = types
                    .iter()
                    .find_map(|ty| {
                        value
                            .coerce(Some(&TaskEvaluationContext::new(self, scope_index)), ty)
                            .ok()
                    })
                    .ok_or_else(|| {
                        multiple_type_mismatch(types, name.span(), &value.ty(), expr.span())
                    })?;
            }

            if requirement {
                requirements.insert(name.text().to_string(), value);
            } else {
                hints.insert(name.text().to_string(), value);
            }
        }

        Ok((requirements, hints))
    }

    /// Evaluates the requirements section.
    async fn evaluate_requirements_section(
        &mut self,
        id: &str,
        section: &RequirementsSection<SyntaxNode>,
        inputs: &TaskInputs,
    ) -> Result<HashMap<String, Value>, Diagnostic> {
        debug!(
            task_id = id,
            task_name = self.task.name(),
            document = self.document.uri().as_str(),
            "evaluating requirements",
        );

        let mut requirements = HashMap::new();

        let version = self
            .document
            .version()
            .expect("document should have version");

        // In WDL 1.3+, use `TASK_SCOPE_INDEX` to access the `task` variable.
        let scope_index = if version >= SupportedVersion::V1(V1::Three) {
            TASK_SCOPE_INDEX
        } else {
            ROOT_SCOPE_INDEX
        };

        for item in section.items() {
            let name = item.name();
            if let Some(value) = inputs.requirement(name.text()) {
                requirements.insert(name.text().to_string(), value.clone());
                continue;
            }

            let mut evaluator = ExprEvaluator::new(TaskEvaluationContext::new(self, scope_index));

            let types =
                task_requirement_types(version, name.text()).expect("requirement should be known");

            // Evaluate and coerce to the expected type
            let expr = item.expr();
            let value = evaluator.evaluate_expr(&expr).await?;
            let value = types
                .iter()
                .find_map(|ty| {
                    value
                        .coerce(Some(&TaskEvaluationContext::new(self, scope_index)), ty)
                        .ok()
                })
                .ok_or_else(|| {
                    multiple_type_mismatch(types, name.span(), &value.ty(), expr.span())
                })?;

            requirements.insert(name.text().to_string(), value);
        }

        Ok(requirements)
    }

    /// Evaluates the hints section.
    async fn evaluate_hints_section(
        &mut self,
        id: &str,
        section: &TaskHintsSection<SyntaxNode>,
        inputs: &TaskInputs,
    ) -> Result<HashMap<String, Value>, Diagnostic> {
        debug!(
            task_id = id,
            task_name = self.task.name(),
            document = self.document.uri().as_str(),
            "evaluating hints section",
        );

        let mut hints = HashMap::new();

        let version = self
            .document
            .version()
            .expect("document should have version");

        // In WDL 1.3+, use `TASK_SCOPE_INDEX` to access task.attempt and task.previous
        let scope_index = if version >= SupportedVersion::V1(V1::Three) {
            TASK_SCOPE_INDEX
        } else {
            ROOT_SCOPE_INDEX
        };

        for item in section.items() {
            let name = item.name();
            if let Some(value) = inputs.hint(name.text()) {
                hints.insert(name.text().to_string(), value.clone());
                continue;
            }

            let mut evaluator =
                ExprEvaluator::new(TaskEvaluationContext::new(self, scope_index).with_task());

            let value = evaluator.evaluate_hints_item(&name, &item.expr()).await?;
            hints.insert(name.text().to_string(), value);
        }

        Ok(hints)
    }

    /// Evaluates the command of a task.
    ///
    /// Returns the evaluated command as a string.
    async fn evaluate_command(
        &mut self,
        id: &str,
        section: &CommandSection<SyntaxNode>,
    ) -> EvaluationResult<String> {
        debug!(
            task_id = id,
            task_name = self.task.name(),
            document = self.document.uri().as_str(),
            "evaluating command section",
        );

        let document = self.document.clone();
        let mut command = String::new();
        match section.strip_whitespace() {
            Some(parts) => {
                let mut evaluator =
                    ExprEvaluator::new(TaskEvaluationContext::new(self, TASK_SCOPE_INDEX));

                for part in parts {
                    match part {
                        StrippedCommandPart::Text(t) => {
                            command.push_str(t.as_str());
                        }
                        StrippedCommandPart::Placeholder(placeholder) => {
                            evaluator
                                .evaluate_placeholder(&placeholder, &mut command)
                                .await
                                .map_err(|d| EvaluationError::new(document.clone(), d))?;
                        }
                    }
                }
            }
            _ => {
                warn!(
                    "command for task `{task}` in `{uri}` has mixed indentation; whitespace \
                     stripping was skipped",
                    task = self.task.name(),
                    uri = self.document.uri(),
                );

                let mut evaluator =
                    ExprEvaluator::new(TaskEvaluationContext::new(self, TASK_SCOPE_INDEX));

                let heredoc = section.is_heredoc();
                for part in section.parts() {
                    match part {
                        CommandPart::Text(t) => {
                            t.unescape_to(heredoc, &mut command);
                        }
                        CommandPart::Placeholder(placeholder) => {
                            evaluator
                                .evaluate_placeholder(&placeholder, &mut command)
                                .await
                                .map_err(|d| EvaluationError::new(document.clone(), d))?;
                        }
                    }
                }
            }
        }

        Ok(command)
    }

    /// Evaluates sections prior to spawning the command.
    ///
    /// This method evaluates the following sections:
    ///   * runtime
    ///   * requirements
    ///   * hints
    ///   * command
    async fn evaluate_sections(
        &mut self,
        id: &str,
        definition: &TaskDefinition<SyntaxNode>,
        inputs: &TaskInputs,
        attempt: u64,
        previous_task_data: Option<Arc<TaskPostEvaluationData>>,
    ) -> EvaluationResult<EvaluatedSections> {
        let version = self.document.version();

        // Extract task metadata once to avoid walking the AST multiple times
        let task_meta = definition
            .metadata()
            .map(|s| Object::from_v1_metadata(s.items()))
            .unwrap_or_else(Object::empty);
        let task_parameter_meta = definition
            .parameter_metadata()
            .map(|s| Object::from_v1_metadata(s.items()))
            .unwrap_or_else(Object::empty);
        // Note: Sprocket does not currently support workflow-level extension metadata,
        // so `ext` is always an empty object.
        let task_ext = Object::empty();

        // In WDL 1.3+, insert a [`TaskPreEvaluation`] before evaluating the
        // requirements/hints/runtime section.
        if version >= Some(SupportedVersion::V1(V1::Three)) {
            let mut task = TaskPreEvaluationValue::new(
                self.task.name(),
                id,
                attempt.try_into().expect("attempt should fit in i64"),
                task_meta.clone(),
                task_parameter_meta.clone(),
                task_ext.clone(),
            );

            if let Some(prev_data) = &previous_task_data {
                task.set_previous(prev_data.clone());
            }

            let scope = &mut self.scopes[TASK_SCOPE_INDEX.0];
            if let Some(v) = scope.get_mut(TASK_VAR_NAME) {
                *v = HiddenValue::TaskPreEvaluation(task).into();
            } else {
                scope.insert(TASK_VAR_NAME, HiddenValue::TaskPreEvaluation(task));
            }
        }

        // Evaluate requirements and hints
        let (requirements, hints) = match definition.runtime() {
            Some(section) => self
                .evaluate_runtime_section(id, &section, inputs)
                .await
                .map_err(|d| EvaluationError::new(self.document.clone(), d))?,
            _ => (
                match definition.requirements() {
                    Some(section) => self
                        .evaluate_requirements_section(id, &section, inputs)
                        .await
                        .map_err(|d| EvaluationError::new(self.document.clone(), d))?,
                    None => Default::default(),
                },
                match definition.hints() {
                    Some(section) => self
                        .evaluate_hints_section(id, &section, inputs)
                        .await
                        .map_err(|d| EvaluationError::new(self.document.clone(), d))?,
                    None => Default::default(),
                },
            ),
        };

        // Now that those are evaluated, insert a [`TaskPostEvaluation`] for
        // `task` which includes those calculated requirements before the
        // command/output sections are evaluated.
        if version >= Some(SupportedVersion::V1(V1::Two)) {
            // Get the execution constraints
            let constraints = self
                .top_level
                .backend
                .constraints(&requirements, &hints)
                .with_context(|| {
                    format!(
                        "failed to get constraints for task `{task}`",
                        task = self.task.name()
                    )
                })?;

            let max_retries = max_retries(&requirements, &self.top_level.config)?;

            let mut task = TaskPostEvaluationValue::new(
                self.task.name(),
                id,
                constraints,
                max_retries.try_into().with_context(|| {
                    format!(
                        "the number of max retries is too large to run task `{task}`",
                        task = self.task.name()
                    )
                })?,
                attempt.try_into().with_context(|| {
                    format!(
                        "too many attempts were made to run task `{task}`",
                        task = self.task.name()
                    )
                })?,
                task_meta,
                task_parameter_meta,
                task_ext,
            );

            // In WDL 1.3+, insert the previous requirements.
            if let Some(version) = version
                && version >= SupportedVersion::V1(V1::Three)
                && let Some(prev_data) = &previous_task_data
            {
                task.set_previous(prev_data.clone());
            }

            let scope = &mut self.scopes[TASK_SCOPE_INDEX.0];
            if let Some(v) = scope.get_mut(TASK_VAR_NAME) {
                *v = HiddenValue::TaskPostEvaluation(task).into();
            } else {
                scope.insert(TASK_VAR_NAME, HiddenValue::TaskPostEvaluation(task));
            }
        }

        let command = self
            .evaluate_command(
                id,
                &definition.command().expect("must have command section"),
            )
            .await?;

        Ok(EvaluatedSections {
            command,
            requirements: Arc::new(requirements),
            hints: Arc::new(hints),
        })
    }

    /// Evaluates a task output.
    async fn evaluate_output(
        &mut self,
        id: &str,
        decl: &Decl<SyntaxNode>,
        evaluated: &EvaluatedTask,
    ) -> Result<(), Diagnostic> {
        let name = decl.name();
        debug!(
            task_id = id,
            task_name = self.task.name(),
            document = self.document.uri().as_str(),
            output_name = name.text(),
            "evaluating output",
        );

        let decl_ty = decl.ty();
        let ty = crate::convert_ast_type_v1(self.document, &decl_ty)?;
        let mut evaluator = ExprEvaluator::new(
            TaskEvaluationContext::new(self, TASK_SCOPE_INDEX)
                .with_work_dir(&evaluated.result.work_dir)
                .with_stdout(&evaluated.result.stdout)
                .with_stderr(&evaluated.result.stderr),
        );

        let expr = decl.expr().expect("outputs should have expressions");
        let value = evaluator.evaluate_expr(&expr).await?;

        // Coerce the output value to the expected type
        let mut value = value
            .coerce(Some(evaluator.context()), &ty)
            .map_err(|e| runtime_type_mismatch(e, &ty, name.span(), &value.ty(), expr.span()))?;
        value = value
            .resolve_paths(
                ty.is_optional(),
                self.base_dir.as_local(),
                Some(self.transferer().as_ref()),
                &|path| {
                    // Join the path with the work directory.
                    let mut output_path = evaluated.result.work_dir.join(path.as_str())?;

                    // Ensure the output's path is valid
                    let output_path = match (&mut output_path, &evaluated.result.work_dir) {
                        (EvaluationPath::Local(joined), EvaluationPath::Local(base))
                            if joined.starts_with(base)
                                || joined == evaluated.stdout().as_file().unwrap().as_str()
                                || joined == evaluated.stderr().as_file().unwrap().as_str() =>
                        {
                            // The joined path is contained within the work directory or is
                            // stdout/stderr
                            HostPath::new(String::try_from(output_path)?)
                        }
                        (EvaluationPath::Local(_), EvaluationPath::Local(_)) => {
                            // The joined path is not within the work or attempt directory;
                            // therefore, it is required to be an input
                            self.path_map
                                .get_by_left(path)
                                .ok_or_else(|| {
                                    anyhow!(
                                        "guest path `{path}` is not an input or within the task's \
                                         working directory"
                                    )
                                })?
                                .0
                                .clone()
                                .into()
                        }
                        (EvaluationPath::Local(_), EvaluationPath::Remote(_)) => {
                            // Path is local (and absolute) and the working directory is remote
                            bail!(
                                "cannot access guest path `{path}` from a remotely executing task"
                            )
                        }
                        (EvaluationPath::Remote(_), _) => {
                            HostPath::new(String::try_from(output_path)?)
                        }
                    };

                    Ok(output_path)
                },
            )
            .await
            .map_err(|e| {
                decl_evaluation_failed(
                    e,
                    self.task.name(),
                    true,
                    name.text(),
                    Some(Io::Output),
                    name.span(),
                )
            })?;

        self.scopes[OUTPUT_SCOPE_INDEX.0].insert(name.text(), value);
        Ok(())
    }

    /// Localizes inputs for execution.
    ///
    /// Returns the inputs to pass to the backend.
    async fn localize_inputs(&mut self, task_id: &str) -> EvaluationResult<Vec<Input>> {
        // If the backend needs local inputs, download them now
        if self.top_level.backend.needs_local_inputs() {
            let mut downloads = JoinSet::new();

            // Download any necessary files
            for (idx, input) in self.backend_inputs.as_slice_mut().iter_mut().enumerate() {
                if input.local_path().is_some() {
                    continue;
                }

                if let EvaluationPath::Remote(url) = input.path() {
                    let transferer = self.top_level.transferer.clone();
                    let url = url.clone();
                    downloads.spawn(async move {
                        transferer
                            .download(&url)
                            .await
                            .map(|l| (idx, l))
                            .with_context(|| anyhow!("failed to localize `{url}`"))
                    });
                }
            }

            // Wait for the downloads to complete
            while let Some(result) = downloads.join_next().await {
                match result.unwrap_or_else(|e| Err(anyhow!("download task failed: {e}"))) {
                    Ok((idx, location)) => {
                        self.backend_inputs.as_slice_mut()[idx].set_location(location);
                    }
                    Err(e) => {
                        return Err(EvaluationError::new(
                            self.document.clone(),
                            task_localization_failed(e, self.task.name(), self.task.name_span()),
                        ));
                    }
                }
            }
        }

        if enabled!(Level::DEBUG) {
            for input in self.backend_inputs.as_slice() {
                match (
                    input.path().as_local().is_some(),
                    input.local_path(),
                    input.guest_path(),
                ) {
                    // Input is unmapped and either local or remote and not downloaded
                    (true, _, None) | (false, None, None) => {}
                    // Input is local and was mapped to a guest path
                    (true, _, Some(guest_path)) => {
                        debug!(
                            task_id,
                            task_name = self.task.name(),
                            document = self.document.uri().as_str(),
                            "task input `{path}` mapped to `{guest_path}`",
                            path = input.path().display(),
                        );
                    }
                    // Input is remote and was downloaded to a local path
                    (false, Some(local_path), None) => {
                        debug!(
                            task_id,
                            task_name = self.task.name(),
                            document = self.document.uri().as_str(),
                            "task input `{path}` downloaded to `{local_path}`",
                            path = input.path().display(),
                            local_path = local_path.display()
                        );
                    }
                    // Input is remote and was not downloaded, but mapped to a guest path
                    (false, None, Some(guest_path)) => {
                        debug!(
                            task_id,
                            task_name = self.task.name(),
                            document = self.document.uri().as_str(),
                            "task input `{path}` mapped to `{guest_path}`",
                            path = input.path().display(),
                        );
                    }
                    // Input is remote and was both downloaded and mapped to a guest path
                    (false, Some(local_path), Some(guest_path)) => {
                        debug!(
                            task_id,
                            task_name = self.task.name(),
                            document = self.document.uri().as_str(),
                            "task input `{path}` downloaded to `{local_path}` and mapped to \
                             `{guest_path}`",
                            path = input.path().display(),
                            local_path = local_path.display(),
                        );
                    }
                }
            }
        }

        Ok(self.backend_inputs.as_slice().into())
    }
}

#[cfg(test)]
mod resource_validation_tests {
    use std::collections::HashMap;

    use super::*;

    fn map_with_value(key: &str, value: Value) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        map.insert(key.to_string(), value);
        map
    }

    #[test]
    fn memory_disallows_negative_values() {
        let requirements = map_with_value(TASK_REQUIREMENT_MEMORY, Value::from(-1));
        let err = memory(&requirements).expect_err("`memory` should reject negatives");
        assert!(
            err.to_string()
                .contains("task requirement `memory` cannot be less than zero")
        );
    }

    #[test]
    fn max_retries_disallows_negative_values() {
        let requirements = map_with_value(TASK_REQUIREMENT_MAX_RETRIES, Value::from(-2));
        let err = max_retries(&requirements, &Config::default())
            .expect_err("`max_retries` should reject negatives");
        assert!(
            err.to_string()
                .contains("task requirement `max_retries` cannot be less than zero")
        );
    }

    #[test]
    fn preemptible_disallows_negative_values() {
        let mut hints = HashMap::new();
        hints.insert("preemptible".to_string(), Value::from(-3));
        let err = preemptible(&hints).expect_err("`preemptible` should reject negatives");
        assert!(
            err.to_string()
                .contains("task hint `preemptible` cannot be less than zero")
        );
    }
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::path::Path;

    use pretty_assertions::assert_eq;
    use tempfile::tempdir;
    use tracing_test::traced_test;
    use wdl_analysis::Analyzer;
    use wdl_analysis::Config as AnalysisConfig;
    use wdl_analysis::DiagnosticsConfig;

    use crate::CancellationContext;
    use crate::EvaluatedTask;
    use crate::Events;
    use crate::TaskInputs;
    use crate::config::BackendConfig;
    use crate::config::CallCachingMode;
    use crate::config::Config;
    use crate::v1::TopLevelEvaluator;

    /// Helper for evaluating a simple task with the given call cache mode.
    async fn evaluate_task(mode: CallCachingMode, root_dir: &Path, source: &str) -> EvaluatedTask {
        fs::write(root_dir.join("source.wdl"), source).expect("failed to write WDL source file");

        // Analyze the source file
        let analyzer = Analyzer::new(
            AnalysisConfig::default().with_diagnostics_config(DiagnosticsConfig::except_all()),
            |(), _, _, _| async {},
        );
        analyzer
            .add_directory(root_dir)
            .await
            .expect("failed to add directory");
        let results = analyzer
            .analyze(())
            .await
            .expect("failed to analyze document");
        assert_eq!(results.len(), 1, "expected only one result");

        let document = results.first().expect("should have result").document();

        let mut config = Config::default();
        config.task.cache = mode;
        config.task.cache_dir = Some(root_dir.join("cache"));
        config
            .backends
            .insert("default".into(), BackendConfig::Local(Default::default()));

        let evaluator = TopLevelEvaluator::new(
            &root_dir.join("runs"),
            config,
            CancellationContext::default(),
            Events::disabled(),
        )
        .await
        .unwrap();

        let runs_dir = root_dir.join("runs");
        evaluator
            .evaluate_task(
                document,
                document.task_by_name("test").expect("should have task"),
                &TaskInputs::default(),
                &runs_dir,
            )
            .await
            .unwrap()
    }

    /// Tests task evaluation when call caching is disabled.
    #[tokio::test]
    #[traced_test]
    async fn cache_off() {
        const SOURCE: &str = r#"
version 1.2

task test {
    input {
        String name = "friend"
    }

    command <<<echo "hello, ~{name}!">>>

    output {
        String message = read_string(stdout())
    }
}
"#;

        let root_dir = tempdir().expect("failed to create temporary directory");
        let evaluated = evaluate_task(CallCachingMode::Off, root_dir.path(), SOURCE).await;
        assert!(!evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(
            logs_contain("call caching is disabled"),
            "expected cache to be off"
        );
    }

    /// Tests task evaluation when call caching is enabled.
    #[tokio::test]
    #[traced_test]
    async fn cache_on() {
        const SOURCE: &str = r#"
version 1.2

task test {
    input {
        String name = "friend"
    }

    command <<<echo "hello, ~{name}!">>>

    output {
        String message = read_string(stdout())
    }
}
"#;

        let root_dir = tempdir().expect("failed to create temporary directory");
        let evaluated = evaluate_task(CallCachingMode::On, root_dir.path(), SOURCE).await;
        assert!(!evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(logs_contain("using call cache"), "expected cache to be on");
        assert!(
            logs_contain("call cache miss"),
            "expected first run to miss the cache"
        );
        assert!(logs_contain("spawning task"), "expected the task to spawn");

        let evaluated = evaluate_task(CallCachingMode::On, root_dir.path(), SOURCE).await;
        assert!(evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(
            logs_contain("task execution was skipped"),
            "expected second run to skip execution"
        );
    }

    /// Tests task evaluation when call caching is enabled, but the task is not
    /// cacheable.
    #[tokio::test]
    #[traced_test]
    async fn cache_on_not_cacheable() {
        const SOURCE: &str = r#"
version 1.2

task test {
    input {
        String name = "friend"
    }

    command <<<echo "hello, ~{name}!">>>

    hints {
        cacheable: false
    }

    output {
        String message = read_string(stdout())
    }
}
"#;

        let root_dir = tempdir().expect("failed to create temporary directory");
        let evaluated = evaluate_task(CallCachingMode::On, root_dir.path(), SOURCE).await;
        assert!(!evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(logs_contain("using call cache"), "expected cache to be on");
        assert!(
            logs_contain("task is not cacheable due to `cacheable` hint being set to `false`"),
            "expected task to not be cacheable"
        );

        let evaluated = evaluate_task(CallCachingMode::On, root_dir.path(), SOURCE).await;
        assert!(!evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(
            !logs_contain("task execution was skipped"),
            "expected second run to not skip execution"
        );
    }

    /// Tests task evaluation when call caching is enabled in explicit mode and
    /// the task is not explicitly marked cacheable.
    #[tokio::test]
    #[traced_test]
    async fn cache_explicit() {
        const SOURCE: &str = r#"
version 1.2

task test {
    input {
        String name = "friend"
    }

    command <<<echo "hello, ~{name}!">>>

    output {
        String message = read_string(stdout())
    }
}
"#;

        let root_dir = tempdir().expect("failed to create temporary directory");
        let evaluated = evaluate_task(CallCachingMode::Explicit, root_dir.path(), SOURCE).await;
        assert!(!evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(logs_contain("using call cache"), "expected cache to be on");
        assert!(
            logs_contain(
                "task is not cacheable due to `cacheable` hint not being explicitly set to `true`"
            ),
            "expected task to not be cacheable"
        );

        let evaluated = evaluate_task(CallCachingMode::Explicit, root_dir.path(), SOURCE).await;
        assert!(!evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(
            !logs_contain("task execution was skipped"),
            "expected second run to not skip execution"
        );
    }

    /// Tests task evaluation when call caching is enabled in explicit mode and
    /// the task is explicitly marked cacheable.
    #[tokio::test]
    #[traced_test]
    async fn cache_explicit_cacheable() {
        const SOURCE: &str = r#"
version 1.2

task test {
    input {
        String name = "friend"
    }

    command <<<echo "hello, ~{name}!">>>

    hints {
        cacheable: true
    }

    output {
        String message = read_string(stdout())
    }
}
"#;

        let root_dir = tempdir().expect("failed to create temporary directory");
        let evaluated = evaluate_task(CallCachingMode::Explicit, root_dir.path(), SOURCE).await;
        assert!(!evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(logs_contain("using call cache"), "expected cache to be on");
        assert!(
            logs_contain("call cache miss"),
            "expected first run to miss the cache"
        );
        assert!(logs_contain("spawning task"), "expected the task to spawn");

        let evaluated = evaluate_task(CallCachingMode::Explicit, root_dir.path(), SOURCE).await;
        assert!(evaluated.cached());
        assert_eq!(evaluated.exit_code(), 0);
        assert_eq!(
            fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str())
                .unwrap()
                .trim(),
            "hello, friend!"
        );
        assert_eq!(
            fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).unwrap(),
            ""
        );
        assert!(
            logs_contain("task execution was skipped"),
            "expected second run to skip execution"
        );
    }
}
