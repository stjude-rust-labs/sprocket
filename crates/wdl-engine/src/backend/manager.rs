//! Implementation of a local task manager used by some backends.

use std::collections::VecDeque;
use std::ops::Add;
use std::ops::Range;
use std::ops::Sub;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Result;
use anyhow::bail;
use futures::channel::oneshot;
use ordered_float::OrderedFloat;
use tracing::debug;

use crate::CancellationContext;
use crate::CancellationContextState;
use crate::EngineEvent;
use crate::Events;

/// Represents a parked task.
struct ParkedTask {
    /// The id of the parked task.
    id: usize,
    /// The requested CPU for the task.
    cpu: f64,
    /// The requested memory, in bytes, for the task.
    memory: u64,
    /// The channel that receives a message when the task is unparked.
    notify: oneshot::Sender<()>,
}

/// Represents a limits for a task manager.
struct Limits {
    /// The mutable limits state.
    state: Arc<Mutex<LimitsState>>,
    /// The engine events.
    events: Events,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
}

/// Represents state used by a limited task manager.
struct LimitsState {
    /// The next parked task id to use.
    next_id: usize,
    /// The amount of available CPU remaining.
    cpu: OrderedFloat<f64>,
    /// The amount of available memory remaining, in bytes.
    memory: u64,
    /// The queue of parked requests.
    parked: VecDeque<ParkedTask>,
}

impl LimitsState {
    /// Constructs a limits state with the given total CPU and memory.
    fn new(cpu: f64, memory: u64) -> Self {
        Self {
            next_id: 0,
            cpu: OrderedFloat(cpu),
            memory,
            parked: Default::default(),
        }
    }

    /// Unparks any tasks that can now be run.
    fn unpark_tasks(&mut self) {
        if self.parked.is_empty() {
            return;
        }

        debug!(
            "attempting to unpark tasks with {cpu} CPUs and {memory} bytes of memory available",
            cpu = self.cpu,
            memory = self.memory,
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
            let parked = self.parked.make_contiguous();

            let cpu_by_memory_len = {
                // Start by finding the longest range in the parked set that could run based on
                // CPU reservation
                let range = fit_longest_range(parked, self.cpu, |task| OrderedFloat(task.cpu));

                // Next, find the longest subset of that subset that could run based on memory
                // reservation
                fit_longest_range(&mut parked[range], self.memory, |task| task.memory).len()
            };

            // Next, find the longest range in the parked set that could run based on memory
            // reservation
            let memory_by_cpu = fit_longest_range(parked, self.memory, |task| task.memory);

            // Next, find the longest subset of that subset that could run based on CPU
            // reservation
            let memory_by_cpu = fit_longest_range(&mut parked[memory_by_cpu], self.cpu, |task| {
                OrderedFloat(task.cpu)
            });

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
                let range = fit_longest_range(parked, self.cpu, |task| OrderedFloat(task.cpu));
                fit_longest_range(&mut parked[range], self.memory, |task| task.memory)
            };

            assert_eq!(
                range.start, 0,
                "expected the fit tasks to be at the front of the list"
            );

            for _ in range {
                let task = self.parked.pop_front().unwrap();

                debug!(
                    "unparking task with reservation of {cpu} CPU(s) and {memory} bytes of memory",
                    cpu = task.cpu,
                    memory = task.memory,
                );

                // Unpark the task
                self.cpu -= task.cpu;
                self.memory -= task.memory;
                let _ = task.notify.send(());
            }
        }
    }
}

/// Responsible for managing tasks based on available host resources.
///
/// The task manager is utilized by backends that need to directly schedule
/// tasks, such as the local backend and the Docker backend when not in a swarm.
pub struct TaskManager {
    /// The maximum CPU per task.
    max_cpu: f64,
    /// The maximum memory per task, in bytes.
    max_memory: u64,
    /// The limits for the task manager
    ///
    /// This is `None` if the task manager is unlimited.
    limits: Option<Limits>,
}

impl TaskManager {
    /// Constructs a new task manager with the given total CPU, maximum CPU per
    /// task, total memory, and maximum memory per task.
    pub fn new(
        cpu: f64,
        max_cpu: f64,
        memory: u64,
        max_memory: u64,
        events: Events,
        cancellation: CancellationContext,
    ) -> Self {
        Self {
            max_cpu,
            max_memory,
            limits: Some(Limits {
                state: Arc::new(Mutex::new(LimitsState::new(cpu, memory))),
                events,
                cancellation,
            }),
        }
    }

    /// Constructs a new task manager that does not limit tasks based on
    /// available resources.
    pub fn new_unlimited(max_cpu: f64, max_memory: u64) -> Self {
        Self {
            max_cpu,
            max_memory,
            limits: None,
        }
    }

    /// Runs a task.
    ///
    /// The requested memory is specified in bytes.
    ///
    /// If there are not enough local resources available for running the task,
    /// it will be parked until the requested resources become available.
    pub async fn run<T, O>(&self, cpu: f64, memory: u64, task: T) -> Result<Option<O>>
    where
        T: Future<Output = Result<Option<O>>>,
    {
        // Ensure the task does not exceed the maximum CPU
        if cpu > self.max_cpu {
            bail!(
                "requested task CPU count of {cpu} exceeds the maximum CPU count of {max_cpu}",
                max_cpu = self.max_cpu
            );
        }

        // Ensure the task does not exceed the maximum memory
        if memory > self.max_memory {
            bail!(
                "requested task memory of {memory} byte{s} exceeds the maximum memory of \
                 {max_memory}",
                s = if memory == 1 { "" } else { "s" },
                max_memory = self.max_memory
            );
        }

        match &self.limits {
            Some(limits) => {
                let mut parked = {
                    let mut state = limits.state.lock().expect("failed to lock state");

                    // If the task can't run due to unavailable resources, park the task until
                    // resources are available
                    if cpu > state.cpu.into() || memory > state.memory {
                        debug!(
                            "parking task due to insufficient resources: task requests {cpu} \
                             CPU(s) and {memory} bytes of memory but there are only \
                             {cpu_remaining} CPU(s) and {memory_remaining} bytes of memory \
                             available",
                            cpu_remaining = state.cpu,
                            memory_remaining = state.memory
                        );

                        let (notify_tx, notify_rx) = oneshot::channel();

                        let id = state.next_id;
                        state.next_id += 1;

                        state.parked.push_back(ParkedTask {
                            id,
                            cpu,
                            memory,
                            notify: notify_tx,
                        });

                        Some((notify_rx, id))
                    } else {
                        // Decrement the resource counts now and continue on to run the task
                        state.cpu -= cpu;
                        state.memory -= memory;

                        debug!(
                            "running task with {cpu} CPUs and {memory} bytes of memory remaining",
                            cpu = state.cpu,
                            memory = state.memory
                        );

                        None
                    }
                };

                // Run the task, waiting for it to be unparked if neccessary
                let res = match &mut parked {
                    Some((notify, _)) => {
                        if let Some(sender) = limits.events.engine() {
                            let _ = sender.send(EngineEvent::TaskParked);
                        }

                        // Wait for cancellation or notice of being unparked
                        let token = limits.cancellation.first();
                        let canceled = tokio::select! {
                            biased;
                            _ = token.cancelled() => true,
                            r = notify => {
                                r?;
                                false
                            }
                        };

                        if let Some(sender) = limits.events.engine() {
                            let _ = sender.send(EngineEvent::TaskUnparked { canceled });
                        }

                        if canceled { Ok(None) } else { task.await }
                    }
                    None => task.await,
                };

                let mut state = limits.state.lock().expect("failed to lock state");
                match parked {
                    Some((_, id)) if state.parked.iter().any(|t| t.id == id) => {
                        // Task is still parked, it must have been canceled; don't increment the
                        // resource counts
                        assert!(matches!(res, Ok(None)), "task should be canceled");
                    }
                    _ => {
                        // Task was either not parked or previously unparked, increment the resource
                        // counts
                        state.cpu += cpu;
                        state.memory += memory;
                    }
                }

                // If a cancellation has occurred, clear the parked tasks; otherwise, unpark
                // what tasks we can
                if limits.cancellation.state() != CancellationContextState::NotCanceled {
                    state.parked.clear();
                } else {
                    state.unpark_tasks();
                }

                res
            }
            None => {
                // Task manager is unlimited, just await the task
                task.await
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
}
