//! Implementation of task execution backends.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use futures::future::BoxFuture;
use indexmap::IndexMap;

use crate::ContentKind;
use crate::EvaluationPath;
use crate::GuestPath;
use crate::TaskInputs;
use crate::Value;
use crate::http::Location;
use crate::http::Transferer;
use crate::v1::requirements::ContainerSource;

mod apptainer;
mod docker;
mod local;
mod lsf_apptainer;
pub(crate) mod manager;
mod slurm_apptainer;
mod tes;

pub use apptainer::*;
pub use docker::*;
pub use local::*;
pub use lsf_apptainer::*;
pub use slurm_apptainer::*;
pub use tes::*;

/// The default root guest path for inputs.
const GUEST_INPUTS_DIR: &str = "/mnt/task/inputs/";

/// The default work directory name.
pub(crate) const WORK_DIR_NAME: &str = "work";

/// The default command file name.
pub(crate) const COMMAND_FILE_NAME: &str = "command";

/// The default stdout file name.
pub(crate) const STDOUT_FILE_NAME: &str = "stdout";

/// The default stderr file name.
pub(crate) const STDERR_FILE_NAME: &str = "stderr";

/// The number of initial expected task names.
///
/// This controls the initial size of the bloom filter and how many names are
/// prepopulated into a name generator.
const INITIAL_EXPECTED_NAMES: usize = 1000;

/// Represents a `File` or `Directory` input to a backend.
#[derive(Debug, Clone)]
pub(crate) struct Input {
    /// The content kind of the input.
    kind: ContentKind,
    /// The path for the input.
    path: EvaluationPath,
    /// The guest path for the input.
    ///
    /// This is `None` when the backend isn't mapping input paths.
    guest_path: Option<GuestPath>,
    /// The download location for the input.
    ///
    /// This is `Some` if the input has been downloaded to a known location.
    location: Option<Location>,
}

impl Input {
    /// Creates a new input with the given path and guest path.
    pub fn new(kind: ContentKind, path: EvaluationPath, guest_path: Option<GuestPath>) -> Self {
        Self {
            kind,
            path,
            guest_path,
            location: None,
        }
    }

    /// Gets the content kind of the input.
    pub fn kind(&self) -> ContentKind {
        self.kind
    }

    /// Gets the path to the input.
    ///
    /// The path of the input may be local or remote.
    pub fn path(&self) -> &EvaluationPath {
        &self.path
    }

    /// Gets the guest path for the input.
    ///
    /// This is `None` for inputs to backends that don't use containers.
    pub fn guest_path(&self) -> Option<&GuestPath> {
        self.guest_path.as_ref()
    }

    /// Gets the local path of the input.
    ///
    /// Returns `None` if the input is remote and has not been localized.
    pub fn local_path(&self) -> Option<&Path> {
        self.location.as_deref().or_else(|| self.path.as_local())
    }

    /// Sets the location of the input.
    ///
    /// This is used during localization to set a local path for remote inputs.
    pub fn set_location(&mut self, location: Location) {
        self.location = Some(location);
    }
}

/// Represents constraints applied to a task's execution.
#[derive(Debug)]
pub struct TaskExecutionConstraints {
    /// The container the task will run in.
    ///
    /// A value of `None` indicates the task will run on the host.
    pub container: Option<ContainerSource>,
    /// The allocated number of CPUs; must be greater than 0.
    pub cpu: f64,
    /// The allocated memory in bytes; must be greater than 0.
    pub memory: u64,
    /// A list with one specification per allocated GPU.
    ///
    /// The specification is execution engine-specific.
    ///
    /// If no GPUs were allocated, then the value must be an empty list.
    pub gpu: Vec<String>,
    /// A list with one specification per allocated FPGA.
    ///
    /// The specification is execution engine-specific.
    ///
    /// If no FPGAs were allocated, then the value must be an empty list.
    pub fpga: Vec<String>,
    /// A map with one entry for each disk mount point.
    ///
    /// The key is the mount point and the value is the initial amount of disk
    /// space allocated, in bytes.
    ///
    /// The execution engine must, at a minimum, provide one entry for each disk
    /// mount point requested, but may provide more.
    ///
    /// The amount of disk space available for a given mount point may increase
    /// during the lifetime of the task (e.g., autoscaling volumes provided by
    /// some cloud services).
    pub disks: IndexMap<String, i64>,
}

/// Represents information for spawning a task.
#[derive(Debug)]
pub(crate) struct TaskSpawnInfo {
    /// The command of the task.
    command: String,
    /// The inputs for task.
    inputs: Vec<Input>,
    /// The requirements of the task.
    requirements: Arc<HashMap<String, Value>>,
    /// The hints of the task.
    hints: Arc<HashMap<String, Value>>,
    /// The environment variables of the task.
    env: Arc<IndexMap<String, String>>,
}

impl TaskSpawnInfo {
    /// Constructs a new task spawn information.
    pub fn new(
        command: String,
        inputs: Vec<Input>,
        requirements: Arc<HashMap<String, Value>>,
        hints: Arc<HashMap<String, Value>>,
        env: Arc<IndexMap<String, String>>,
    ) -> Self {
        Self {
            command,
            inputs,
            requirements,
            hints,
            env,
        }
    }
}

/// Represents a request to spawn a task.
#[derive(Debug)]
pub(crate) struct TaskSpawnRequest {
    /// The id of the task being spawned.
    id: String,
    /// The information for the task to spawn.
    info: TaskSpawnInfo,
    /// The constraints for the task's execution.
    constraints: TaskExecutionConstraints,
    /// The attempt directory for the task's execution.
    attempt_dir: PathBuf,
    /// The temp directory for the evaluation.
    temp_dir: PathBuf,
}

impl TaskSpawnRequest {
    /// Creates a new task spawn request.
    pub fn new(
        id: String,
        info: TaskSpawnInfo,
        constraints: TaskExecutionConstraints,
        attempt_dir: PathBuf,
        temp_dir: PathBuf,
    ) -> Self {
        Self {
            id,
            info,
            constraints,
            attempt_dir,
            temp_dir,
        }
    }

    /// The identifier of the task being spawned.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Gets the command for the task.
    pub fn command(&self) -> &str {
        &self.info.command
    }

    /// Gets the inputs for the task.
    pub fn inputs(&self) -> &[Input] {
        &self.info.inputs
    }

    /// Gets the requirements of the task.
    pub fn requirements(&self) -> &HashMap<String, Value> {
        &self.info.requirements
    }

    /// Gets the hints of the task.
    pub fn hints(&self) -> &HashMap<String, Value> {
        &self.info.hints
    }

    /// Gets the environment variables of the task.
    pub fn env(&self) -> &IndexMap<String, String> {
        &self.info.env
    }

    /// Gets the constraints to apply to the task's execution.
    pub fn constraints(&self) -> &TaskExecutionConstraints {
        &self.constraints
    }

    /// Gets the attempt directory for the task's execution.
    pub fn attempt_dir(&self) -> &Path {
        &self.attempt_dir
    }

    /// The temp directory for the evaluation.
    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    /// The host path for the command to store the task's evaluated command.
    pub fn command_path(&self) -> PathBuf {
        self.attempt_dir.join(COMMAND_FILE_NAME)
    }

    /// The default work directory host path.
    ///
    /// This is used by backends that support local or shared file systems.
    pub fn work_dir(&self) -> PathBuf {
        self.attempt_dir.join(WORK_DIR_NAME)
    }

    /// The default stdout file host path.
    ///
    /// This is used by backends that support local or shared file systems.
    pub fn stdout_path(&self) -> PathBuf {
        self.attempt_dir.join(STDOUT_FILE_NAME)
    }

    /// The default stderr file host path.
    ///
    /// This is used by backends that support local or shared file systems.
    pub fn stderr_path(&self) -> PathBuf {
        self.attempt_dir.join(STDERR_FILE_NAME)
    }
}

/// Represents the result of a task's execution.
#[derive(Debug)]
pub struct TaskExecutionResult {
    /// Stores the task process exit code.
    pub exit_code: i32,
    /// The task's working directory.
    pub work_dir: EvaluationPath,
    /// The value of the task's stdout file.
    pub stdout: Value,
    /// The value of the task's stderr file.
    pub stderr: Value,
}

/// Represents a task execution backend.
pub(crate) trait TaskExecutionBackend: Send + Sync {
    /// Gets the execution constraints given a task's inputs, requirements, and
    /// hints.
    ///
    /// The returned constraints are used to populate the `task` variable in WDL
    /// 1.2+.
    ///
    /// Returns an error if the task cannot be constrained for the execution
    /// environment or if the task specifies invalid requirements.
    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints>;

    /// Gets the guest (container) inputs directory of the backend.
    ///
    /// Returns `None` if the backend does not execute tasks in a container.
    ///
    /// The returned path is expected to be Unix style and end with a backslash.
    fn guest_inputs_dir(&self) -> Option<&'static str> {
        Some(GUEST_INPUTS_DIR)
    }

    /// Determines if the backend needs local inputs.
    ///
    /// Backends that run tasks remotely should return `false`.
    fn needs_local_inputs(&self) -> bool {
        true
    }

    /// Spawns a task with the execution backend.
    ///
    /// Returns the result of the task's execution or `None` if the task was
    /// canceled.
    fn spawn<'a>(
        &'a self,
        inputs: &'a TaskInputs,
        request: TaskSpawnRequest,
        transferer: Arc<dyn Transferer>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>>;
}
