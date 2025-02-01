//! Implementation of engine configuration.

use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;

use crate::SYSTEM;
use crate::TaskExecutionBackend;
use crate::convert_unit_string;
use crate::local::LocalTaskExecutionBackend;

/// The inclusive maximum number of task retries the engine supports.
pub const MAX_RETRIES: u64 = 100;

/// Represents WDL evaluation configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    /// Workflow evaluation configuration.
    #[serde(default)]
    pub workflow: WorkflowConfig,
    /// Task evaluation configuration.
    #[serde(default)]
    pub task: TaskConfig,
    /// Task execution backend configuration.
    #[serde(default)]
    pub backend: BackendConfig,
}

impl Config {
    /// Validates the evaluation configuration.
    pub fn validate(&self) -> Result<()> {
        self.workflow.validate()?;
        self.task.validate()?;
        self.backend.validate()?;
        Ok(())
    }

    /// Creates a new task execution backend based on this configuration.
    pub fn create_backend(&self) -> Result<Arc<dyn TaskExecutionBackend>> {
        match self.backend.default {
            Backend::Local => Ok(Arc::new(LocalTaskExecutionBackend::new(
                &self.backend.local,
            )?)),
        }
    }
}

/// Represents workflow evaluation configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowConfig {
    /// Scatter statement evaluation configuration.
    #[serde(default)]
    pub scatter: ScatterConfig,
}

impl WorkflowConfig {
    /// Validates the workflow configuration.
    pub fn validate(&self) -> Result<()> {
        self.scatter.validate()?;
        Ok(())
    }
}

/// Represents scatter statement evaluation configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScatterConfig {
    /// The number of scatter array elements to process concurrently.
    ///
    /// By default, the value is the parallelism supported by the task
    /// execution backend.
    ///
    /// A value of `0` is invalid.
    ///
    /// Lower values use less memory for evaluation and higher values may better
    /// saturate the task execution backend with tasks to execute.
    ///
    /// This setting does not change how many tasks an execution backend can run
    /// concurrently, but may affect how many tasks are sent to the backend to
    /// run at a time.
    ///
    /// For example, if `concurrency` was set to 10 and we evaluate the
    /// following scatters:
    ///
    /// ```wdl
    /// scatter (i in range(100)) {
    ///     call my_task
    /// }
    ///
    /// scatter (j in range(100)) {
    ///     call my_task as my_task2
    /// }
    /// ```
    ///
    /// Here each scatter is independent and therefore there will be 20 calls
    /// (10 for each scatter) made concurrently. If the task execution
    /// backend can only execute 5 tasks concurrently, 5 tasks will execute
    /// and 15 will be "ready" to execute and waiting for an executing task
    /// to complete.
    ///
    /// If instead we evaluate the following scatters:
    ///
    /// ```wdl
    /// scatter (i in range(100)) {
    ///     scatter (j in range(100)) {
    ///         call my_task
    ///     }
    /// }
    /// ```
    ///
    /// Then there will be 100 calls (10*10 as 10 are made for each outer
    /// element) made concurrently. If the task execution backend can only
    /// execute 5 tasks concurrently, 5 tasks will execute and 95 will be
    /// "ready" to execute and waiting for an executing task to complete.
    ///
    /// <div class="warning">
    /// Warning: nested scatter statements cause exponential memory usage based
    /// on this value, as each scatter statement evaluation requires allocating
    /// new scopes for scatter array elements being processed. </div>
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u64>,
}

impl ScatterConfig {
    /// Validates the scatter configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(concurrency) = self.concurrency {
            if concurrency == 0 {
                bail!("configuration value `workflow.scatter.concurrency` cannot be zero");
            }
        }

        Ok(())
    }
}

/// Represents task evaluation configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskConfig {
    /// The default maximum number of retries to attempt if a task fails.
    ///
    /// A task's `max_retries` requirement will override this value.
    ///
    /// Defaults to 0 (no retries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<u64>,
}

impl TaskConfig {
    /// Validates the task evaluation configuration.
    pub fn validate(&self) -> Result<()> {
        if self.retries.unwrap_or(0) > MAX_RETRIES {
            bail!("configuration value `task.retries` cannot exceed {MAX_RETRIES}");
        }
        Ok(())
    }
}

/// Represents supported task execution backends.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Use the local task execution backend.
    // TODO: change this to docker when supported.
    #[default]
    Local,
}

impl Backend {
    /// Determines if the backend is the local task execution backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

/// Represents task execution backend configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BackendConfig {
    /// The default execution backend to use.
    // TODO: update this once local is no longer the default
    #[serde(default, skip_serializing_if = "Backend::is_local")]
    pub default: Backend,
    /// Local task execution backend configuration.
    pub local: LocalBackendConfig,
}

impl BackendConfig {
    /// Validates the backend configuration.
    pub fn validate(&self) -> Result<()> {
        self.local.validate()?;
        Ok(())
    }
}

/// Represents configuration for the local task execution backend.
///
/// <div class="warning">
/// Warning: the local task execution backend spawns processes on the host
/// directly without the use of a container; only use this backend on trusted
/// WDL. </div>
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalBackendConfig {
    /// Set the number of CPUs available for task execution.
    ///
    /// Defaults to the number of logical CPUs for the host.
    ///
    /// The value cannot be zero or exceed the host's number of CPUs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<u64>,

    /// Set the total amount of memory for task execution as a unit string (e.g.
    /// `2 GiB`).
    ///
    /// Defaults to the total amount of memory for the host.
    ///
    /// The value cannot be zero or exceed the host's total amount of memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
}

impl LocalBackendConfig {
    /// Validates the local task execution backend configuration.
    pub fn validate(&self) -> Result<()> {
        if let Some(cpu) = self.cpu {
            if cpu == 0 {
                bail!("configuration value `backend.local.cpu` cannot be zero");
            }

            let total = SYSTEM.cpus().len() as u64;
            if cpu > total {
                bail!(
                    "configuration value `backend.local.cpu` cannot exceed the virtual CPUs \
                     available to the host ({total})"
                );
            }
        }

        if let Some(memory) = &self.memory {
            let memory = convert_unit_string(memory).with_context(|| {
                format!("configuration value `backend.local.memory` has invalid value `{memory}`")
            })?;

            if memory == 0 {
                bail!("configuration value `backend.local.memory` cannot be zero");
            }

            let total = SYSTEM.total_memory();
            if memory > total {
                bail!(
                    "configuration value `backend.local.memory` cannot exceed the total memory of \
                     the host ({total} bytes)"
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_config_validate() {
        // Test invalid task config
        let mut config = Config::default();
        config.task.retries = Some(1000000);
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "configuration value `task.retries` cannot exceed 100"
        );

        // Test invalid scatter concurrency config
        let mut config = Config::default();
        config.workflow.scatter.concurrency = Some(0);
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "configuration value `workflow.scatter.concurrency` cannot be zero"
        );

        // Test invalid local backend cpu config
        let mut config = Config::default();
        config.backend.local.cpu = Some(0);
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "configuration value `backend.local.cpu` cannot be zero"
        );
        let mut config = Config::default();
        config.backend.local.cpu = Some(10000000);
        assert!(config.validate().unwrap_err().to_string().starts_with(
            "configuration value `backend.local.cpu` cannot exceed the virtual CPUs available to \
             the host"
        ));

        // Test invalid local backend memory config
        let mut config = Config::default();
        config.backend.local.memory = Some("0 GiB".to_string());
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "configuration value `backend.local.memory` cannot be zero"
        );
        let mut config = Config::default();
        config.backend.local.memory = Some("100 meows".to_string());
        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "configuration value `backend.local.memory` has invalid value `100 meows`"
        );
        let mut config = Config::default();
        config.backend.local.memory = Some("10000 TiB".to_string());
        assert!(config.validate().unwrap_err().to_string().starts_with(
            "configuration value `backend.local.memory` cannot exceed the total memory of the host"
        ));
    }
}
