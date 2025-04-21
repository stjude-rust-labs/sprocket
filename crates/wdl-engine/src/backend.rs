//! Implementation of task execution backends.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs;
use std::future::Future;
use std::ops::Add;
use std::ops::Range;
use std::ops::Sub;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use futures::future::BoxFuture;
use indexmap::IndexMap;
use ordered_float::OrderedFloat;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::Input;
use crate::Value;
use crate::http::HttpDownloader;
use crate::path::EvaluationPath;

pub mod crankshaft;
pub mod local;

/// The default work directory name.
pub const WORK_DIR_NAME: &str = "work";

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
/// The directory layout for local task execution is:
///
/// ```text
/// <root>/
/// ├─ tmp/             # Where files are created by the stdlib for task evaluation
/// ├─ attempt/         # Stores the execution attempts
/// │  ├─ 0/            # First attempt root directory
/// │  │  ├─ work/      # Working directory for the task's first execution
/// │  │  ├─ command    # The evaluated command for the first execution
/// │  │  ├─ stdout     # The standard output of the first execution
/// │  │  ├─ stderr     # The standard error of the first execution
/// │  ├─ 1/            # Second attempt (first retry)
/// │  │  ├─ ...
///
/// Note that for remote task execution, the working directory will not be created.
#[derive(Debug)]
pub struct TaskExecutionRoot {
    /// The root directory for task execution.
    root_dir: PathBuf,
    /// The path to the directory for files created by the stdlib before and
    /// after command evaluation.
    temp_dir: PathBuf,
    /// The root directory for the attempt.
    attempt_dir: PathBuf,
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
        let root_dir = absolute(path).with_context(|| {
            format!(
                "failed to determine absolute path of `{path}`",
                path = path.display()
            )
        })?;

        let mut attempt_dir = root_dir.join("attempts");
        attempt_dir.push(attempt.to_string());

        // Create both temp directories now as it may be needed for task evaluation
        let temp_dir = root_dir.join("tmp");
        fs::create_dir_all(&temp_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        let command = attempt_dir.join("command");
        let stdout = attempt_dir.join("stdout");
        let stderr = attempt_dir.join("stderr");

        Ok(Self {
            root_dir,
            temp_dir,
            attempt_dir,
            command,
            stdout,
            stderr,
        })
    }

    /// Gets the path to the root itself.
    pub fn path(&self) -> &Path {
        &self.root_dir
    }

    /// Gets the temporary directory path for task evaluation before and after
    /// command evaluation.
    ///
    /// The temporary directory is created before spawning the task so that it
    /// is available for task evaluation.
    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    /// Gets the attempt directory.
    pub fn attempt_dir(&self) -> &Path {
        &self.attempt_dir
    }

    /// Gets the command file path.
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
    /// The id of the task being spawned.
    id: String,
    /// The command of the task.
    command: String,
    /// The requirements of the task.
    requirements: Arc<HashMap<String, Value>>,
    /// The hints of the task.
    hints: Arc<HashMap<String, Value>>,
    /// The environment variables of the task.
    env: Arc<IndexMap<String, String>>,
    /// The inputs for the spawn request.
    inputs: Arc<[Input]>,
}

impl TaskSpawnRequest {
    /// Creates a new task spawn request.
    pub fn new(
        root: Arc<TaskExecutionRoot>,
        id: String,
        command: String,
        requirements: Arc<HashMap<String, Value>>,
        hints: Arc<HashMap<String, Value>>,
        env: Arc<IndexMap<String, String>>,
        inputs: Arc<[Input]>,
    ) -> Self {
        Self {
            root,
            id,
            command,
            requirements,
            hints,
            env,
            inputs,
        }
    }

    /// Gets the execution root to spawn the task with.
    pub fn root(&self) -> &TaskExecutionRoot {
        &self.root
    }

    /// The identifier of the task being spawned.
    pub fn id(&self) -> &str {
        &self.id
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
    pub fn env(&self) -> &IndexMap<String, String> {
        &self.env
    }

    /// Gets the inputs for the task.
    pub fn inputs(&self) -> &[Input] {
        &self.inputs
    }
}

/// Represents the result of a task's execution.
#[derive(Debug)]
pub struct TaskExecutionResult {
    /// Stores the task process exit code.
    pub exit_code: i32,
    /// The task's working directory.
    pub work_dir: EvaluationPath,
}

/// Represents events that can be awaited on during task execution.
pub struct TaskExecutionEvents {
    /// The event for when the task has spawned and is currently executing.
    pub spawned: Receiver<()>,
    /// The event for when the task has completed.
    ///
    /// Returns the execution result.
    pub completed: Receiver<Result<TaskExecutionResult>>,
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

    /// Gets the guest path the task working directory (e.g. `/mnt/work`).
    ///
    /// Returns `None` if the task execution does not use a container.
    fn guest_work_dir(&self) -> Option<&Path>;

    /// Localizes the given set of inputs for the backend.
    ///
    /// This may involve downloading remote inputs to the host and updating the
    /// input's guest paths.
    fn localize_inputs<'a, 'b, 'c, 'd>(
        &'a self,
        downloader: &'b HttpDownloader,
        inputs: &'c mut [Input],
    ) -> BoxFuture<'d, Result<()>>
    where
        'a: 'd,
        'b: 'd,
        'c: 'd,
        Self: 'd;
    /// Spawns a task with the execution backend.
    ///
    /// Returns the task execution event receives upon success.
    fn spawn(
        &self,
        request: TaskSpawnRequest,
        token: CancellationToken,
    ) -> Result<TaskExecutionEvents>;

    /// Performs cleanup operations after top-level workflow or task evaluation
    /// completes.
    ///
    /// Returns `None` if no cleanup is required.
    fn cleanup<'a, 'b, 'c>(
        &'a self,
        _output_dir: &'b Path,
        _token: CancellationToken,
    ) -> Option<BoxFuture<'c, ()>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        None
    }
}

/// A trait implemented by backend requests.
trait TaskManagerRequest: Send + Sync + 'static {
    /// Gets the requested CPU allocation from the request.
    fn cpu(&self) -> f64;

    /// Gets the requested memory allocation from the request, in bytes.
    fn memory(&self) -> u64;

    /// Runs the request.
    fn run(
        self,
        spawned: oneshot::Sender<()>,
    ) -> impl Future<Output = Result<TaskExecutionResult>> + Send;
}

/// Represents a response internal to the task manager.
struct TaskManagerResponse {
    /// The previous CPU allocation from the request.
    cpu: f64,
    /// The previous memory allocation from the request.
    memory: u64,
    /// The result of the task's execution.
    result: Result<TaskExecutionResult>,
    /// The channel to send the task's execution result back on.
    tx: oneshot::Sender<Result<TaskExecutionResult>>,
}

/// Represents state used by the task manager.
struct TaskManagerState<Req> {
    /// The amount of available CPU remaining.
    cpu: OrderedFloat<f64>,
    /// The amount of available memory remaining, in bytes.
    memory: u64,
    /// The set of spawned tasks.
    spawned: JoinSet<TaskManagerResponse>,
    /// The queue of parked spawn requests.
    parked: VecDeque<(
        Req,
        oneshot::Sender<()>,
        oneshot::Sender<Result<TaskExecutionResult>>,
    )>,
}

impl<Req> TaskManagerState<Req> {
    /// Constructs a new task manager state with the given total CPU and memory.
    fn new(cpu: u64, memory: u64) -> Self {
        Self {
            cpu: OrderedFloat(cpu as f64),
            memory,
            spawned: Default::default(),
            parked: Default::default(),
        }
    }

    /// Determines if the resources are unlimited.
    fn unlimited(&self) -> bool {
        self.cpu == u64::MAX as f64 && self.memory == u64::MAX
    }
}

/// Responsible for managing tasks based on available host resources.
struct TaskManager<Req> {
    /// The sender for new spawn requests.
    tx: mpsc::UnboundedSender<(
        Req,
        oneshot::Sender<()>,
        oneshot::Sender<Result<TaskExecutionResult>>,
    )>,
}

impl<Req> TaskManager<Req>
where
    Req: TaskManagerRequest,
{
    /// Constructs a new task manager with the given total CPU, maximum CPU per
    /// request, total memory, and maximum memory per request.
    fn new(cpu: u64, max_cpu: u64, memory: u64, max_memory: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            Self::run_request_queue(rx, cpu, max_cpu, memory, max_memory).await;
        });

        Self { tx }
    }

    /// Constructs a new task manager that does not limit requests based on
    /// available resources.
    fn new_unlimited(max_cpu: u64, max_memory: u64) -> Self {
        Self::new(u64::MAX, max_cpu, u64::MAX, max_memory)
    }

    /// Sends a request to the task manager's queue.
    fn send(
        &self,
        request: Req,
        spawned: oneshot::Sender<()>,
        completed: oneshot::Sender<Result<TaskExecutionResult>>,
    ) {
        self.tx.send((request, spawned, completed)).ok();
    }

    /// Runs the request queue.
    async fn run_request_queue(
        mut rx: mpsc::UnboundedReceiver<(
            Req,
            oneshot::Sender<()>,
            oneshot::Sender<Result<TaskExecutionResult>>,
        )>,
        cpu: u64,
        max_cpu: u64,
        memory: u64,
        max_memory: u64,
    ) {
        let mut state = TaskManagerState::new(cpu, memory);

        loop {
            // If there aren't any spawned tasks, wait for a spawn request only
            if state.spawned.is_empty() {
                assert!(
                    state.parked.is_empty(),
                    "there can't be any parked requests if there are no spawned tasks"
                );
                match rx.recv().await {
                    Some((req, spawned, completed)) => {
                        Self::handle_spawn_request(
                            &mut state, max_cpu, max_memory, req, spawned, completed,
                        );
                        continue;
                    }
                    None => break,
                }
            }

            // Otherwise, wait for a spawn request or a completed task
            tokio::select! {
                request = rx.recv() => {
                    match request {
                        Some((req, spawned, completed)) => {
                            Self::handle_spawn_request(&mut state, max_cpu, max_memory, req, spawned, completed);
                        }
                        None => break,
                    }
                }
                Some(Ok(response)) = state.spawned.join_next() => {
                    if !state.unlimited() {
                        state.cpu += response.cpu;
                        state.memory += response.memory;
                    }

                    response.tx.send(response.result).ok();
                    Self::spawn_parked_tasks(&mut state, max_cpu, max_memory);
                }
            }
        }
    }

    /// Handles a spawn request by either parking it (not enough resources
    /// currently available) or by spawning it.
    fn handle_spawn_request(
        state: &mut TaskManagerState<Req>,
        max_cpu: u64,
        max_memory: u64,
        request: Req,
        spawned: oneshot::Sender<()>,
        completed: oneshot::Sender<Result<TaskExecutionResult>>,
    ) {
        // Ensure the request does not exceed the maximum CPU
        let cpu = request.cpu();
        if cpu > max_cpu as f64 {
            completed
                .send(Err(anyhow!(
                    "requested task CPU count of {cpu} exceeds the maximum CPU count of {max_cpu}",
                )))
                .ok();
            return;
        }

        // Ensure the request does not exceed the maximum memory
        let memory = request.memory();
        if memory > max_memory {
            completed
                .send(Err(anyhow!(
                    "requested task memory of {memory} byte{s} exceeds the maximum memory of \
                     {max_memory}",
                    s = if memory == 1 { "" } else { "s" }
                )))
                .ok();
            return;
        }

        if !state.unlimited() {
            // If the request can't be processed due to resource constraints, park the
            // request for now. When a task completes and resources become available,
            // we'll unpark the request
            if cpu > state.cpu.into() || memory > state.memory {
                debug!(
                    "parking task due to insufficient resources: task reserves {cpu} CPU(s) and \
                     {memory} bytes of memory but there are only {cpu_remaining} CPU(s) and \
                     {memory_remaining} bytes of memory available",
                    cpu_remaining = state.cpu,
                    memory_remaining = state.memory
                );
                state.parked.push_back((request, spawned, completed));
                return;
            }

            // Decrement the resource counts and spawn the task
            state.cpu -= cpu;
            state.memory -= memory;
            debug!(
                "spawning task with {cpu} CPUs and {memory} bytes of memory remaining",
                cpu = state.cpu,
                memory = state.memory
            );
        }

        state.spawned.spawn(async move {
            TaskManagerResponse {
                cpu: request.cpu(),
                memory: request.memory(),
                result: request.run(spawned).await,
                tx: completed,
            }
        });
    }

    /// Responsible for spawning parked tasks.
    fn spawn_parked_tasks(state: &mut TaskManagerState<Req>, max_cpu: u64, max_memory: u64) {
        if state.parked.is_empty() {
            return;
        }

        debug!(
            "attempting to unpark tasks with {cpu} CPUs and {memory} bytes of memory available",
            cpu = state.cpu,
            memory = state.memory,
        );

        // This algorithm is intended to unpark the greatest number of tasks.
        //
        // It first finds the greatest subset of tasks that are constrained by CPU and
        // then by memory.
        //
        // Next it finds the greatest subset of tasks that are constrained by memory and
        // then by CPU.
        //
        // It then unparks whichever subset is greater.
        //
        // The process is repeated until both subsets reach zero length.
        loop {
            let cpu_by_memory_len = {
                // Start by finding the longest range in the parked set that could run based on
                // CPU reservation
                let range =
                    fit_longest_range(state.parked.make_contiguous(), state.cpu, |(r, ..)| {
                        OrderedFloat(r.cpu())
                    });

                // Next, find the longest subset of that subset that could run based on memory
                // reservation
                fit_longest_range(
                    &mut state.parked.make_contiguous()[range],
                    state.memory,
                    |(r, ..)| r.memory(),
                )
                .len()
            };

            // Next, find the longest range in the parked set that could run based on memory
            // reservation
            let memory_by_cpu =
                fit_longest_range(state.parked.make_contiguous(), state.memory, |(r, ..)| {
                    r.memory()
                });

            // Next, find the longest subset of that subset that could run based on CPU
            // reservation
            let memory_by_cpu = fit_longest_range(
                &mut state.parked.make_contiguous()[memory_by_cpu],
                state.cpu,
                |(r, ..)| OrderedFloat(r.cpu()),
            );

            // If both subsets are empty, break out
            if cpu_by_memory_len == 0 && memory_by_cpu.is_empty() {
                break;
            }

            // Check to see which subset is greater (for equivalence, use the one we don't
            // need to refit for)
            let range = if memory_by_cpu.len() >= cpu_by_memory_len {
                memory_by_cpu
            } else {
                // We need to refit because the above calculation of `memory_by_cpu` mutated the
                // parked list
                let range =
                    fit_longest_range(state.parked.make_contiguous(), state.cpu, |(r, ..)| {
                        OrderedFloat(r.cpu())
                    });

                fit_longest_range(
                    &mut state.parked.make_contiguous()[range],
                    state.memory,
                    |(r, ..)| r.memory(),
                )
            };

            debug!("unparking {len} task(s)", len = range.len());

            assert_eq!(
                range.start, 0,
                "expected the fit tasks to be at the front of the queue"
            );
            for _ in range {
                let (request, spawned, completed) = state.parked.pop_front().unwrap();

                debug!(
                    "unparking task with reservation of {cpu} CPU(s) and {memory} bytes of memory",
                    cpu = request.cpu(),
                    memory = request.memory(),
                );

                Self::handle_spawn_request(state, max_cpu, max_memory, request, spawned, completed);
            }
        }
    }
}

/// Determines the longest range in a slice where the sum of the weights of the
/// elements in the returned range is less than or equal to the supplied total
/// weight.
///
/// The returned range always starts at zero as this algorithm will partially
/// sort the slice.
///
/// Due to the partial sorting, the provided slice will have its elements
/// rearranged. As the function modifies the slice in-place, this function does
/// not make any allocations.
///
/// # Implementation
///
/// This function is implemented using a modified quick sort algorithm as a
/// solution to the more general "0/1 knapsack" problem where each item has an
/// equal profit value; this maximizes for the number of items to put
/// into the knapsack (i.e. longest range that fits).
///
/// Using a uniform random pivot point, it partitions the input into two sides:
/// the left side where all weights are less than the pivot and the right side
/// where all weights are equal to or greater than the pivot.
///
/// It then checks to see if the total weight of the left side is less than or
/// equal to the total remaining weight; if it is, every element in
/// the left side is considered as part of the output and it recurses on the
/// right side.
///
/// If the total weight of the left side is greater than the remaining weight
/// budget, it can completely ignore the right side and instead recurse on the
/// left side.
///
/// The algorithm stops when the partition size reaches zero.
///
/// # Panics
///
/// Panics if the supplied weight is a negative value.
fn fit_longest_range<T, F, W>(slice: &mut [T], total_weight: W, mut weight_fn: F) -> Range<usize>
where
    F: FnMut(&T) -> W,
    W: Ord + Add<Output = W> + Sub<Output = W> + Default,
{
    /// Partitions the slice so that the weight of every element to the left
    /// of the pivot is less than the pivot's weight and every element to the
    /// right of the pivot is greater than or equal to the pivot's weight.
    ///
    /// Returns the pivot index, pivot weight, and the sum of the left side
    /// element's weights.
    fn partition<T, F, W>(
        slice: &mut [T],
        weight_fn: &mut F,
        mut low: usize,
        high: usize,
    ) -> (usize, W, W)
    where
        F: FnMut(&T) -> W,
        W: Ord + Add<Output = W> + Sub<Output = W> + Default,
    {
        assert!(low < high);

        // Swap a random element (the pivot) in the remaining range with the high
        slice.swap(high, rand::random_range(low..high));

        let pivot_weight = weight_fn(&slice[high]);
        let mut sum_weight = W::default();
        let range = low..=high;
        for i in range {
            let weight = weight_fn(&slice[i]);
            // If the weight belongs on the left side of the pivot, swap
            if weight < pivot_weight {
                slice.swap(i, low);
                low += 1;
                sum_weight = sum_weight.add(weight);
            }
        }

        slice.swap(low, high);
        (low, pivot_weight, sum_weight)
    }

    fn recurse_fit_maximal_range<T, F, W>(
        slice: &mut [T],
        mut remaining_weight: W,
        weight_fn: &mut F,
        low: usize,
        high: usize,
        end: &mut usize,
    ) where
        F: FnMut(&T) -> W,
        W: Ord + Add<Output = W> + Sub<Output = W> + Default,
    {
        if low == high {
            let weight = weight_fn(&slice[low]);
            if weight <= remaining_weight {
                *end += 1;
            }

            return;
        }

        if low < high {
            let (pivot, pivot_weight, sum) = partition(slice, weight_fn, low, high);
            if sum <= remaining_weight {
                // Everything up to the pivot can be included
                *end += pivot - low;
                remaining_weight = remaining_weight.sub(sum);

                // Check to see if the pivot itself can be included
                if pivot_weight <= remaining_weight {
                    *end += 1;
                    remaining_weight = remaining_weight.sub(pivot_weight);
                }

                // Recurse on the right side
                recurse_fit_maximal_range(slice, remaining_weight, weight_fn, pivot + 1, high, end);
            } else if pivot > 0 {
                // Otherwise, we can completely disregard the right side (including the pivot)
                // and recurse on the left
                recurse_fit_maximal_range(slice, remaining_weight, weight_fn, low, pivot - 1, end);
            }
        }
    }

    assert!(
        total_weight >= W::default(),
        "total weight cannot be negative"
    );

    if slice.is_empty() {
        return 0..0;
    }

    let mut end = 0;
    recurse_fit_maximal_range(
        slice,
        total_weight,
        &mut weight_fn,
        0,
        slice.len() - 1, // won't underflow due to empty check
        &mut end,
    );

    0..end
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fit_empty_slice() {
        let r = fit_longest_range(&mut [], 100, |i| *i);
        assert!(r.is_empty());
    }

    #[test]
    #[should_panic(expected = "total weight cannot be negative")]
    fn fit_negative_panic() {
        fit_longest_range(&mut [0], -1, |i| *i);
    }

    #[test]
    fn no_fit() {
        let r = fit_longest_range(&mut [100, 101, 102], 99, |i| *i);
        assert!(r.is_empty());
    }

    #[test]
    fn fit_all() {
        let r = fit_longest_range(&mut [1, 2, 3, 4, 5], 15, |i| *i);
        assert_eq!(r.len(), 5);

        let r = fit_longest_range(&mut [5, 4, 3, 2, 1], 20, |i| *i);
        assert_eq!(r.len(), 5);
    }

    #[test]
    fn fit_some() {
        let s = &mut [8, 2, 2, 3, 2, 1, 2, 4, 1];
        let r = fit_longest_range(s, 10, |i| *i);
        assert_eq!(r.len(), 6);
        assert_eq!(s[r.start..r.end].iter().copied().sum::<i32>(), 10);
        assert!(s[r.end..].contains(&8));
        assert!(s[r.end..].contains(&4));
        assert!(s[r.end..].contains(&3));
    }

    #[test]
    fn unlimited_state() {
        let manager_state = TaskManagerState::<()>::new(u64::MAX, u64::MAX);
        assert!(manager_state.unlimited());
    }
}
