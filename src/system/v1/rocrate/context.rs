//! The data contract collected during a run and consumed by the crate builder.

use crate::system::v1::db::models::Run;
use crate::system::v1::db::models::Session;
use crate::system::v1::exec::Target;
use crate::system::v1::fs::RunDirectory;

/// Identifying metadata for the Sprocket engine, used for the engine
/// `SoftwareApplication` entity.
#[derive(Debug, Clone)]
pub struct EngineInfo {
    /// Package name (e.g. `sprocket`).
    pub name: String,
    /// Package version.
    pub version: String,
    /// Repository URL.
    pub repository: String,
}

impl EngineInfo {
    /// Reads engine identity from build-time Cargo metadata.
    pub fn from_build() -> Self {
        Self {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            repository: option_env!("CARGO_PKG_REPOSITORY")
                .filter(|s| !s.is_empty())
                .unwrap_or("https://github.com/stjude-rust-labs/sprocket")
                .to_string(),
        }
    }
}

/// All typed run state needed to build a Workflow Run RO-Crate. Borrowed so the
/// builder runs without taking ownership of execution state.
#[derive(Debug)]
pub struct RunCrateContext<'a> {
    /// The completed run's DB row.
    pub run: &'a Run,
    /// The session row (submitter attribution), if available.
    pub session: Option<&'a Session>,
    /// The analyzed document (workflow/task interface, imports, source).
    pub document: &'a wdl::analysis::Document,
    /// The selected run target.
    pub target: &'a Target,
    /// Typed inputs supplied to the run.
    pub inputs: &'a wdl::engine::Inputs,
    /// Typed outputs produced by the run.
    pub outputs: &'a wdl::engine::Outputs,
    /// The run directory (crate root).
    pub run_dir: &'a RunDirectory,
    /// The run's task rows (one per WDL task name at the current database
    /// granularity), used for step-level provenance.
    pub tasks: &'a [crate::system::v1::db::models::Task],
    /// Engine identity.
    pub engine: EngineInfo,
}

impl RunCrateContext<'_> {
    /// Iterates the realized inputs as `(name, value)`, flattening the
    /// task/workflow `Inputs` enum.
    pub fn inputs_iter(&self) -> Box<dyn Iterator<Item = (&str, &wdl::engine::Value)> + '_> {
        match self.inputs {
            wdl::engine::Inputs::Task(t) => Box::new(t.iter()),
            wdl::engine::Inputs::Workflow(w) => Box::new(w.iter()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_info_from_env_is_populated() {
        let e = EngineInfo::from_build();
        assert_eq!(e.name, "sprocket");
        assert!(!e.version.is_empty());
        assert!(e.repository.starts_with("http"));
    }
}
