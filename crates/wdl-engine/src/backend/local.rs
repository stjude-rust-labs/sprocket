//! Implementation of the local backend.

use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::process::Stdio;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::process::Command;
use tracing::info;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_ast::v1::TASK_REQUIREMENT_CPU;
use wdl_ast::v1::TASK_REQUIREMENT_MEMORY;

use super::TaskExecution;
use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use crate::Coercible;
use crate::Engine;
use crate::Value;
use crate::convert_unit_string;

/// Represents a local task execution.
///
/// Local executions directly execute processes on the host without a container.
#[derive(Debug)]
pub struct LocalTaskExecution {
    /// The path to the working directory for the execution.
    work_dir: PathBuf,
    /// The path to the temp directory for the execution.
    temp_dir: PathBuf,
    /// The path to the command file.
    command: PathBuf,
    /// The path to the stdout file.
    stdout: PathBuf,
    /// The path to the stderr file.
    stderr: PathBuf,
}

impl LocalTaskExecution {
    /// Creates a new local task execution with the given execution root
    /// directory to use.
    pub fn new(root: &Path) -> Result<Self> {
        let root = absolute(root).with_context(|| {
            format!(
                "failed to determine absolute path of `{path}`",
                path = root.display()
            )
        })?;

        // Create the temp directory now as it may be needed for task evaluation
        let temp_dir = root.join("tmp");
        fs::create_dir_all(&temp_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        Ok(Self {
            work_dir: root.join("work"),
            temp_dir,
            command: root.join("command"),
            stdout: root.join("stdout"),
            stderr: root.join("stderr"),
        })
    }
}

impl TaskExecution for LocalTaskExecution {
    fn map_path(&mut self, _: &Path) -> Option<PathBuf> {
        // Local execution doesn't use guest paths
        None
    }

    fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    fn command(&self) -> &Path {
        &self.command
    }

    fn stdout(&self) -> &Path {
        &self.stdout
    }

    fn stderr(&self) -> &Path {
        &self.stderr
    }

    fn constraints(
        &self,
        engine: &Engine,
        requirements: &HashMap<String, Value>,
        _: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let num_cpus: f64 = engine.system().cpus().len() as f64;
        let min_cpu = requirements
            .get(TASK_REQUIREMENT_CPU)
            .map(|v| {
                v.coerce(engine.types(), PrimitiveTypeKind::Float.into())
                    .expect("type should coerce")
                    .unwrap_float()
            })
            .unwrap_or(1.0);
        if num_cpus < min_cpu {
            bail!(
                "task requires at least {min_cpu} CPU{s}, but the host only has {num_cpus} \
                 available",
                s = if min_cpu == 1.0 { "" } else { "s" },
            );
        }

        let memory: i64 = engine
            .system()
            .total_memory()
            .try_into()
            .context("system has too much memory to describe as a WDL value")?;

        // The default value for `memory` is 2 GiB
        let min_memory = requirements
            .get(TASK_REQUIREMENT_MEMORY)
            .map(|v| {
                if let Some(v) = v.as_integer() {
                    return Ok(v);
                }

                if let Some(s) = v.as_string() {
                    return convert_unit_string(s)
                        .and_then(|v| v.try_into().ok())
                        .with_context(|| {
                            format!("task specifies an invalid `memory` requirement `{s}`")
                        });
                }

                unreachable!("value should be an integer or string");
            })
            .transpose()?
            .unwrap_or(2 * 1024 * 1024 * 1024); // 2GiB is the default for `memory`

        if memory < min_memory {
            // Display the error in GiB, as it is the most common unit for memory
            let memory = memory as f64 / (1024.0 * 1024.0 * 1024.0);
            let min_memory = min_memory as f64 / (1024.0 * 1024.0 * 1024.0);

            bail!(
                "task requires at least {min_memory} GiB of memory, but the host only has \
                 {memory} GiB available",
            );
        }

        Ok(TaskExecutionConstraints {
            container: None,
            cpu: num_cpus,
            memory,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn spawn(
        &self,
        command: String,
        _: &HashMap<String, Value>,
        _: &HashMap<String, Value>,
    ) -> Result<BoxFuture<'static, Result<i32>>> {
        // Recreate the working directory
        if self.work_dir.exists() {
            fs::remove_dir_all(&self.work_dir).with_context(|| {
                format!(
                    "failed to remove directory `{path}`",
                    path = self.work_dir.display()
                )
            })?;
        }

        fs::create_dir_all(&self.work_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = self.work_dir.display()
            )
        })?;

        // Write the evaluated command to disk
        fs::write(&self.command, command).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = self.command.display()
            )
        })?;

        // Create a file for the stdout
        let stdout = File::create(&self.stdout).with_context(|| {
            format!(
                "failed to create stdout file `{path}`",
                path = self.stdout.display()
            )
        })?;

        // Create a file for the stderr
        let stderr = File::create(&self.stderr).with_context(|| {
            format!(
                "failed to create stderr file `{path}`",
                path = self.stderr.display()
            )
        })?;

        let mut command = Command::new("bash");
        command
            .current_dir(&self.work_dir)
            .arg("-C")
            .arg(&self.command)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);

        // Set an environment variable on Windows to get consistent PATH searching
        // See: https://github.com/rust-lang/rust/issues/122660
        #[cfg(windows)]
        command.env("WDL_TASK_EVALUATION", "1");

        #[cfg(unix)]
        let stderr = self.stderr.clone();

        let mut child = command.spawn().context("failed to spawn `bash`")?;
        Ok(async move {
            let id = child.id().expect("should have id");
            info!("spawning local `bash` process {id} for task execution");

            let status = child.wait().await.with_context(|| {
                format!("failed to wait for termination of task child process {id}")
            })?;

            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    bail!(
                        "task child process {id} has terminated with signal {signal}; see stderr \
                         file `{path}` for more details",
                        path = stderr.display()
                    );
                }
            }

            let status_code = status.code().expect("process should have exited");
            Ok(status_code)
        }
        .boxed())
    }
}

/// Represents a task execution backend that locally executes tasks.
///
/// This backend will directly spawn processes without using a container.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalTaskExecutionBackend;

impl LocalTaskExecutionBackend {
    /// Constructs a new local task execution backend.
    pub fn new() -> Self {
        Self
    }
}

impl TaskExecutionBackend for LocalTaskExecutionBackend {
    fn create_execution(&self, root: &Path) -> Result<Box<dyn TaskExecution>> {
        Ok(Box::new(LocalTaskExecution::new(root)?))
    }
}
