//! Implementation of task execution backends.

use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;
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

/// The result of attempting to pull a single container image.
pub type PullResult<T> = anyhow::Result<T>;

/// An ordered map of container pull attempts.
///
/// Entries appear in the order they were attempted. The map stops after the
/// first success, so candidates after a successful pull do not appear.
pub struct PullResultMap<T>(IndexMap<ContainerSource, PullResult<T>>);

impl<T> Default for PullResultMap<T> {
    fn default() -> Self {
        Self(IndexMap::new())
    }
}

impl<T> PullResultMap<T> {
    /// Returns the first successful container and its associated value, if any.
    pub fn successful_container(&self) -> Option<(&ContainerSource, &T)> {
        self.0
            .iter()
            .find_map(|(source, result)| result.as_ref().ok().map(|value| (source, value)))
    }

    /// Returns `true` if every attempt failed (or the map is empty).
    pub fn all_failed(&self) -> bool {
        self.0.values().all(|r| r.is_err())
    }

    /// Iterates over the failed pull attempts.
    pub fn failures(&self) -> impl Iterator<Item = (&ContainerSource, &anyhow::Error)> {
        self.0
            .iter()
            .filter_map(|(source, result)| result.as_ref().err().map(|e| (source, e)))
    }
}

impl<T> Deref for PullResultMap<T> {
    type Target = IndexMap<ContainerSource, PullResult<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for PullResultMap<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> fmt::Display for PullResultMap<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "all container image candidates failed to pull:")?;
        for (source, error) in self.failures() {
            write!(f, "\n  - `{source:#}`: {error:#}")?;
        }
        Ok(())
    }
}

/// Represents constraints applied to a task's execution.
#[derive(Debug)]
pub struct TaskExecutionConstraints {
    /// The container images to try, in priority order.
    ///
    /// A value of `None` indicates the task will run on the host.
    pub container: Option<Vec<ContainerSource>>,
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

/// Represents a request to execute a task.
#[derive(Debug)]
pub struct ExecuteTaskRequest<'a> {
    /// The id of the task being executed.
    pub id: &'a str,
    /// The command of the task.
    pub command: &'a str,
    /// The original input values to the task.
    pub inputs: &'a TaskInputs,
    /// The backend inputs for task.
    pub backend_inputs: &'a [Input],
    /// The requirements of the task.
    pub requirements: &'a HashMap<String, Value>,
    /// The hints of the task.
    pub hints: &'a HashMap<String, Value>,
    /// The environment variables of the task.
    pub env: &'a IndexMap<String, String>,
    /// The constraints for the task's execution.
    pub constraints: &'a TaskExecutionConstraints,
    /// The attempt directory for the task's execution.
    pub attempt_dir: &'a Path,
    /// The temp directory for the evaluation.
    pub temp_dir: &'a Path,
}

impl<'a> ExecuteTaskRequest<'a> {
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
    /// The container image that was actually used for execution.
    pub container: Option<ContainerSource>,
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

    /// Execute a task with the execution backend using the provided file
    /// transferer.
    ///
    /// Returns the result of the task's execution or `None` if the task was
    /// canceled.
    fn execute<'a>(
        &'a self,
        transferer: &'a Arc<dyn Transferer>,
        request: ExecuteTaskRequest<'a>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pull_result_map_has_no_successful_container() {
        let map: PullResultMap<String> = PullResultMap::default();
        assert!(map.successful_container().is_none());
    }

    #[test]
    fn empty_pull_result_map_reports_all_failed() {
        let map: PullResultMap<String> = PullResultMap::default();
        assert!(map.all_failed());
    }

    #[test]
    fn pull_result_map_with_success() {
        let mut map = PullResultMap::default();
        let source = ContainerSource::Docker("foo:latest".to_string());
        map.insert(source.clone(), Ok("resolved".to_string()));
        assert_eq!(
            map.successful_container()
                .map(|(s, v)| (s.clone(), v.clone())),
            Some((source, "resolved".to_string()))
        );
        assert!(!map.all_failed());
    }

    #[test]
    fn pull_result_map_with_all_failures() {
        let mut map: PullResultMap<String> = PullResultMap::default();
        map.insert(
            ContainerSource::Docker("a:1".to_string()),
            Err(anyhow::anyhow!("not found")),
        );
        map.insert(
            ContainerSource::Docker("b:2".to_string()),
            Err(anyhow::anyhow!("timeout")),
        );
        assert!(map.successful_container().is_none());
        assert!(map.all_failed());
        assert_eq!(map.failures().count(), 2);
    }

    #[test]
    fn pull_result_map_display_lists_failures() {
        let mut map: PullResultMap<String> = PullResultMap::default();
        map.insert(
            ContainerSource::Docker("a:1".to_string()),
            Err(anyhow::anyhow!("not found")),
        );
        map.insert(
            ContainerSource::Docker("b:2".to_string()),
            Err(anyhow::anyhow!("timeout")),
        );
        let display = map.to_string();
        assert!(display.contains("a:1"));
        assert!(display.contains("not found"));
        assert!(display.contains("b:2"));
        assert!(display.contains("timeout"));
    }

    #[test]
    fn pull_result_map_failures_skips_successes() {
        let mut map = PullResultMap::default();
        map.insert(
            ContainerSource::Docker("a:1".to_string()),
            Err(anyhow::anyhow!("not found")),
        );
        map.insert(
            ContainerSource::Docker("b:2".to_string()),
            Ok("resolved".to_string()),
        );
        assert_eq!(map.failures().count(), 1);
    }
}
