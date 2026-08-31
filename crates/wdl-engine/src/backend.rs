//! Implementation of task execution backends.

use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use futures::future::BoxFuture;
use indexmap::IndexMap;

use crate::CancellationContext;
use crate::ContentKind;
use crate::EvaluationHttpClient;
use crate::EvaluationPath;
use crate::Events;
use crate::GuestPath;
use crate::Object;
use crate::TaskInputs;
use crate::Value;
use crate::digest::DigestCalculator;
use crate::http::Location;
use crate::v1::requirements::ImageSource;

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
    /// Whether or not the input is cacheable by the call cache.
    ///
    /// A value of `None` and `Some(true)` indicates cacheable.
    ///
    /// For an input to *not* be cacheable, all calls to `update_cacheable` must
    /// be passed `false`.
    cacheable: Option<bool>,
}

impl Input {
    /// Creates a new input with the given path and guest path.
    pub fn new(kind: ContentKind, path: EvaluationPath, guest_path: Option<GuestPath>) -> Self {
        Self {
            kind,
            path,
            guest_path,
            location: None,
            cacheable: None,
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

    /// Determines if the input is cacheable by the call cache.
    pub fn cacheable(&self) -> bool {
        !matches!(self.cacheable, Some(false))
    }

    /// Updates the cacheability of the input.
    ///
    /// For an input to _not_ be cacheable, every call to `update_cacheable`
    /// must pass `false`.
    pub fn update_cacheable(&mut self, cacheable: bool) {
        match self.cacheable {
            Some(false) if cacheable => {
                self.cacheable = Some(true);
            }
            Some(true) | Some(false) => {
                // No op
            }
            None => {
                self.cacheable = Some(cacheable);
            }
        }
    }
}

/// An ordered list of image pull attempts.
///
/// Entries appear in the order they were attempted. The list stops after the
/// first success, so candidates after a successful pull do not appear.
pub struct PullResults<T>(Vec<(ImageSource, anyhow::Result<T>)>);

impl<T> Default for PullResults<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> PullResults<T> {
    /// Records the outcome of a pull attempt.
    pub fn push(&mut self, source: ImageSource, result: anyhow::Result<T>) {
        self.0.push((source, result));
    }

    /// Returns the successful images and their associated value, if any.
    pub fn successful_images(&self) -> impl Iterator<Item = (&ImageSource, &T)> {
        self.0
            .iter()
            .filter_map(|(source, result)| result.as_ref().ok().map(|value| (source, value)))
    }

    /// Iterates over the failed pull attempts.
    pub fn failures(&self) -> impl Iterator<Item = (&ImageSource, &anyhow::Error)> {
        self.0
            .iter()
            .filter_map(|(source, result)| result.as_ref().err().map(|e| (source, e)))
    }
}

impl<T> fmt::Display for PullResults<T> {
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
    /// The container image sources to try, in priority order.
    ///
    /// If the set is empty, the execution backend does not use containers.
    pub sources: Vec<ImageSource>,
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

/// Represents context of a task's evaluation provided to the backend.
pub trait TaskEvaluationContext: Send + Sync {
    /// Gets the [`EvaluationHttpClient`] used for the task's evaluation.
    fn http_client(&self) -> &EvaluationHttpClient;

    /// Gets the [`Events`] used for the task's evaluation.
    fn events(&self) -> &Events;

    /// Gets the [`CancellationContext`] used for the task's evaluation.
    fn cancellation(&self) -> &CancellationContext;

    /// Gets the [`DigestCalculator`] used for the task's evaluation.
    fn digests(&self) -> &DigestCalculator;

    /// Compiles the given regular expression.
    fn compile_regex(&self, pattern: &str) -> Result<regex::Regex, regex::Error>;
}

/// Represents a request to execute a task.
pub struct ExecuteTaskRequest<'a> {
    /// Gets the task evaluation context associated with the request.
    pub context: &'a dyn TaskEvaluationContext,
    /// The unique name of the task for this execution attempt.
    ///
    /// The name is generated by the evaluator before the task's inputs are
    /// localized so that progress can be attributed to the task before it
    /// reaches the backend. Backends must report this name verbatim in the
    /// events they emit.
    pub name: &'a str,
    /// The command of the task.
    pub command: &'a str,
    /// The original input values to the task.
    pub inputs: &'a TaskInputs,
    /// The backend inputs for task.
    pub backend_inputs: &'a [Input],
    /// The requirements of the task.
    pub requirements: &'a Object,
    /// The hints of the task.
    pub hints: &'a Object,
    /// The environment variables of the task.
    pub env: &'a IndexMap<String, String>,
    /// The constraints for the task's execution.
    pub constraints: &'a TaskExecutionConstraints,
    /// The evaluation base directory (i.e. the document's directory).
    pub base_dir: &'a EvaluationPath,
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
    /// The container image source that was actually used for execution.
    ///
    /// If `None`, the task was not executed in a container.
    pub image: Option<ImageSource>,
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
    /// The unique name of the backend.
    fn name(&self) -> &'static str;

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
        requirements: &Object,
        hints: &Object,
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

    /// Execute a task with the execution backend.
    ///
    /// Returns the result of the task's execution or `None` if the task was
    /// canceled.
    fn execute<'a>(
        &'a self,
        request: &'a ExecuteTaskRequest<'a>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>>;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Config;
    use crate::Engine;
    use crate::http::tests::NotImplementedHttpClient;

    /// Helper task evaluation context for backend tests.
    pub struct EvalContext {
        client: EvaluationHttpClient,
        events: Events,
        cancellation: CancellationContext,
        digests: DigestCalculator,
    }

    impl EvalContext {
        pub async fn new(events: Events, cancellation: CancellationContext) -> Self {
            let engine = Engine::new_with_http_client(Config::default(), NotImplementedHttpClient)
                .await
                .unwrap();

            let client = EvaluationHttpClient::new(&engine, &events, cancellation.clone());
            let digests = DigestCalculator::new(client.clone(), cancellation.clone(), 1000);

            Self {
                client,
                events,
                cancellation,
                digests,
            }
        }
    }

    impl TaskEvaluationContext for EvalContext {
        fn http_client(&self) -> &EvaluationHttpClient {
            &self.client
        }

        fn events(&self) -> &Events {
            &self.events
        }

        fn cancellation(&self) -> &CancellationContext {
            &self.cancellation
        }

        fn digests(&self) -> &DigestCalculator {
            &self.digests
        }

        fn compile_regex(&self, pattern: &str) -> Result<regex::Regex, regex::Error> {
            regex::Regex::new(pattern)
        }
    }

    #[test]
    fn empty_pull_results_has_no_successful_images() {
        let results: PullResults<String> = PullResults::default();
        assert!(results.successful_images().next().is_none());
    }

    #[test]
    fn pull_results_with_success() {
        let mut results = PullResults::default();
        let source = ImageSource::Docker("foo:latest".to_string());
        results.push(source.clone(), Ok("resolved".to_string()));
        assert_eq!(
            results
                .successful_images()
                .map(|(s, v)| (s.clone(), v.clone()))
                .next(),
            Some((source, "resolved".to_string()))
        );
    }

    #[test]
    fn pull_results_with_all_failures() {
        let mut results: PullResults<String> = PullResults::default();
        results.push(
            ImageSource::Docker("a:1".to_string()),
            Err(anyhow::anyhow!("not found")),
        );
        results.push(
            ImageSource::Docker("b:2".to_string()),
            Err(anyhow::anyhow!("timeout")),
        );
        assert!(results.successful_images().next().is_none());
        assert_eq!(results.failures().count(), 2);
    }

    #[test]
    fn pull_results_display_lists_failures() {
        let mut results: PullResults<String> = PullResults::default();
        results.push(
            ImageSource::Docker("a:1".to_string()),
            Err(anyhow::anyhow!("not found")),
        );
        results.push(
            ImageSource::Docker("b:2".to_string()),
            Err(anyhow::anyhow!("timeout")),
        );
        let display = results.to_string();
        assert!(display.contains("a:1"));
        assert!(display.contains("not found"));
        assert!(display.contains("b:2"));
        assert!(display.contains("timeout"));
    }

    #[test]
    fn pull_results_failures_skips_successes() {
        let mut results = PullResults::default();
        results.push(
            ImageSource::Docker("a:1".to_string()),
            Err(anyhow::anyhow!("not found")),
        );
        results.push(
            ImageSource::Docker("b:2".to_string()),
            Ok("resolved".to_string()),
        );
        assert_eq!(results.failures().count(), 1);
    }
}
