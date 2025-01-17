//! Implementation of task execution backends.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use futures::future::BoxFuture;
use indexmap::IndexMap;

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

/// Represents the execution of a particular task.
pub trait TaskExecution: Send {
    /// Maps a host path to a guest path.
    ///
    /// Returns `None` if the execution directly uses host paths.
    fn map_path(&mut self, path: &Path) -> Option<PathBuf>;

    /// Gets the working directory path for the task's execution.
    ///
    /// The working directory will be created upon spawning the task.
    fn work_dir(&self) -> &Path;

    /// Gets the temporary directory path for the task's execution.
    ///
    /// The temporary directory is created before spawning the task so that it
    /// is available for task evaluation.
    fn temp_dir(&self) -> &Path;

    /// Gets the command file path.
    ///
    /// The command file is created upon spawning the task.
    fn command(&self) -> &Path;

    /// Gets the stdout file path.
    ///
    /// The stdout file is created upon spawning the task.
    fn stdout(&self) -> &Path;

    /// Gets the stderr file path.
    ///
    /// The stderr file is created upon spawning the task.
    fn stderr(&self) -> &Path;

    /// Gets the execution constraints for the task given the task's
    /// requirements and hints.
    ///
    /// Returns an error if the task cannot be constrained for the execution
    /// environment or if the task specifies invalid requirements.
    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints>;

    /// Spawns the execution of a task given the task's command, requirements,
    /// and hints.
    ///
    /// Upon success, returns a future that will complete when the task's
    /// execution has finished; the future returns the exit status code of the
    /// task's process.
    fn spawn(
        &self,
        command: &str,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
        env: &[(String, String)],
    ) -> Result<BoxFuture<'static, Result<i32>>>;
}

/// Represents a task execution backend.
pub trait TaskExecutionBackend: Send + Sync {
    /// Gets the maximum concurrent tasks supported by the backend.
    fn max_concurrency(&self) -> usize;

    /// Creates a new task execution.
    ///
    /// The specified directory serves as the root location of where a task
    /// execution may keep its files.
    ///
    /// Note that this does not spawn the task's execution; see
    /// [TaskExecution::spawn](TaskExecution::spawn).
    fn create_execution(&self, root: &Path) -> Result<Box<dyn TaskExecution>>;
}
