//! Implementation of task execution backends.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use indexmap::IndexMap;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;

use crate::Value;

pub mod local;

/// Represents constraints applied to a task's execution.
pub struct TaskExecutionConstraints {
    /// The container the task will run in.
    ///
    /// A value of `None` indicates the task will run on the host.
    pub container: Option<String>,
    /// The allocated number of CPUs; must be greater than 0.
    pub cpu: f64,
    /// The allocated memory in bytes; must be greater than 0.
    pub memory: i64,
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

/// Represents the root directory of a task execution.
///
/// The directory layout for task execution is:
///
/// ```text
/// <root>/
/// ├─ tmp/             # Where files are created by the stdlib before/after the command evaluation
/// ├─ attempt/         # Stores the execution attempts
/// │  ├─ 0/            # First attempt
/// │  │  ├─ work/      # Working directory for the task's first execution
/// │  │  ├─ tmp/       # Where files are created by the stdlib during command evaluation
/// │  │  ├─ command    # The evaluated command for the first execution
/// │  │  ├─ stdout     # The standard output of the first execution
/// │  │  ├─ stderr     # The standard error of the first execution
/// │  ├─ 1/            # Second attempt (first retry)
/// │  │  ├─ ...
/// ```
#[derive(Debug)]
pub struct TaskExecutionRoot {
    /// The path to the directory for files created by the stdlib before and
    /// after command evaluation.
    temp_dir: PathBuf,
    /// The path to the directory for files created by the stdlib during command
    /// evaluation.
    ///
    /// This needs to be a different location from `temp_dir` because commands
    /// are re-evaluated on retry.
    command_temp_dir: PathBuf,
    /// The path to the working directory for the execution.
    work_dir: PathBuf,
    /// The path to the command file.
    command: PathBuf,
    /// The path to the stdout file.
    stdout: PathBuf,
    /// The path to the stderr file.
    stderr: PathBuf,
}

impl TaskExecutionRoot {
    /// Creates a task execution root for the given path and execution attempt.
    pub fn new(path: &Path, attempt: u64) -> Result<Self> {
        let path = absolute(path).with_context(|| {
            format!(
                "failed to determine absolute path of `{path}`",
                path = path.display()
            )
        })?;

        let mut attempts = path.join("attempts");
        attempts.push(attempt.to_string());

        // Create both temp directories now as it may be needed for task evaluation
        let temp_dir = path.join("tmp");
        fs::create_dir_all(&temp_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        let command_temp_dir = attempts.join("tmp");
        fs::create_dir_all(&command_temp_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = command_temp_dir.display()
            )
        })?;

        Ok(Self {
            temp_dir,
            command_temp_dir,
            work_dir: attempts.join("work"),
            command: attempts.join("command"),
            stdout: attempts.join("stdout"),
            stderr: attempts.join("stderr"),
        })
    }

    /// Gets the temporary directory path for task evaluation before and after
    /// command evaluation.
    ///
    /// The temporary directory is created before spawning the task so that it
    /// is available for task evaluation.
    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    /// Gets the temporary directory path for the current task attempt.
    ///
    /// This is the location for storing files created during evaluation of the
    /// command.
    ///
    /// The temporary directory is created before spawning the task so that it
    /// is available for task evaluation.
    pub fn attempt_temp_dir(&self) -> &Path {
        &self.command_temp_dir
    }

    /// Gets the working directory for task execution.
    ///
    /// The working directory will be created upon spawning the task.
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    //// Gets the command file path.
    /// The command file is created upon spawning the task.
    pub fn command(&self) -> &Path {
        &self.command
    }

    /// Gets the stdout file path.
    ///
    /// The stdout file is created upon spawning the task.
    pub fn stdout(&self) -> &Path {
        &self.stdout
    }

    /// Gets the stderr file path.
    ///
    /// The stderr file is created upon spawning the task.
    pub fn stderr(&self) -> &Path {
        &self.stderr
    }
}

/// Represents a request to spawn a task.
#[derive(Debug)]
pub struct TaskSpawnRequest {
    /// The execution root of the task.
    root: Arc<TaskExecutionRoot>,
    /// The command of the task.
    command: String,
    /// The requirements of the task.
    requirements: HashMap<String, Value>,
    /// The hints of the task.
    hints: HashMap<String, Value>,
    /// The environment variables of the task.
    env: HashMap<String, String>,
    /// The mapping between host paths and guest paths.
    ///
    /// This is only populated for backends that have a container root.
    mapping: HashMap<String, String>,
    /// The channel to send a message on when the task is spawned.
    ///
    /// This value will be `None` once the task is spawned.
    spawned: Option<oneshot::Sender<()>>,
}

impl TaskSpawnRequest {
    /// Creates a new task spawn request.
    ///
    /// Returns the new request along with a receiver that is notified when the
    /// task is spawned.
    pub fn new(
        root: Arc<TaskExecutionRoot>,
        command: String,
        requirements: HashMap<String, Value>,
        hints: HashMap<String, Value>,
        env: HashMap<String, String>,
        mapping: HashMap<String, String>,
    ) -> (Self, oneshot::Receiver<()>) {
        let (tx, rx) = oneshot::channel();

        (
            Self {
                root,
                command,
                requirements,
                hints,
                env,
                mapping,
                spawned: Some(tx),
            },
            rx,
        )
    }

    /// Gets the execution root to spawn the task with.
    pub fn root(&self) -> &TaskExecutionRoot {
        &self.root
    }

    /// Gets the command for the task.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Gets the requirements of the task.
    pub fn requirements(&self) -> &HashMap<String, Value> {
        &self.requirements
    }

    /// Gets the hints of the task.
    pub fn hints(&self) -> &HashMap<String, Value> {
        &self.hints
    }

    /// Gets the environment variables of the task.
    pub fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    /// Gets the mapping between host paths and guest paths.
    ///
    /// This is only populated for backends that have a container root.
    pub fn mapping(&self) -> &HashMap<String, String> {
        &self.mapping
    }
}

/// Represents the response from spawning a task.
#[derive(Debug)]
pub struct TaskSpawnResponse {
    /// The requirements the task was spawned with.
    pub requirements: HashMap<String, Value>,
    /// The hints the task was spawned with.
    pub hints: HashMap<String, Value>,
    /// The environment the task was spawned with.
    pub env: HashMap<String, String>,
    /// The status code of the task's execution.
    ///
    /// This may be `Err` if the task failed to spawn.
    pub status_code: Result<i32>,
}

/// Represents a task execution backend.
pub trait TaskExecutionBackend: Send + Sync {
    /// Gets the maximum concurrent tasks supported by the backend.
    fn max_concurrency(&self) -> u64;

    /// Gets the execution constraints given a task's requirements and hints.
    ///
    /// Returns an error if the task cannot be constrained for the execution
    /// environment or if the task specifies invalid requirements.
    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints>;

    /// Gets the container root directory for the backend (e.g. `/mnt/task`)
    ///
    /// Returns `None` if the task execution does not use a container.
    fn container_root(&self) -> Option<&Path>;

    /// Spawns a task with the execution backend.
    ///
    /// Upon success, returns a receiver for receiving the response.
    fn spawn(&self, request: TaskSpawnRequest) -> Result<Receiver<TaskSpawnResponse>>;
}
