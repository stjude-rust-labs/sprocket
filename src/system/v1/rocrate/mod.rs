//! Workflow Run RO-Crate emission for completed runs.

mod context;
mod formal;
mod source;
mod value;

pub use context::EngineInfo;
pub use context::RunCrateContext;
pub use formal::formal_parameter;
pub use source::materialize_sources;
pub use value::value_to_entities;

/// RO-Crate 1.1 base context.
pub const ROCRATE_CONTEXT: &str = "https://w3id.org/ro/crate/1.1/context";
/// Workflow Run RO-Crate term context.
pub const WFRUN_CONTEXT: &str = "https://w3id.org/ro/terms/workflow-run/context";
/// Profiles the root dataset conforms to in M1 (workflow + workflow-ro-crate).
/// M2 adds the Provenance Run Crate profile.
pub const PROFILES: &[&str] = &[
    "https://w3id.org/ro/wfrun/workflow/0.1",
    "https://w3id.org/workflowhub/workflow-ro-crate/1.0",
];
/// The metadata descriptor filename.
pub const METADATA_FILE: &str = "ro-crate-metadata.json";
/// The main workflow entity id, materialized under `workflow/`.
pub const WORKFLOW_ID: &str = "workflow/workflow.wdl";

/// Controls whether and how a run emits an RO-Crate.
#[derive(Debug, Clone, Copy)]
pub struct RoCrateOptions {
    /// Whether to emit `ro-crate-metadata.json` at all.
    pub enabled: bool,
    /// Whether an emission failure should fail the command.
    pub strict: bool,
    /// Whether to compute SHA-256 digests for `File`/`Directory` entities.
    pub checksums: bool,
    /// Whether to localize input/output data values into crate-relative paths.
    pub localize: bool,
}

impl RoCrateOptions {
    /// Builds options from the `--ro-crate`, `--ro-crate-strict`,
    /// `--no-ro-crate-checksums`, and `--no-ro-crate-localize` flags. The `no_*`
    /// values are raw flags, so each behavior is enabled when its flag is
    /// `false`.
    pub fn from_flags(enabled: bool, strict: bool, no_checksums: bool, no_localize: bool) -> Self {
        Self {
            enabled,
            strict,
            checksums: !no_checksums,
            localize: !no_localize,
        }
    }

    /// Options that emit nothing (used by non-CLI run paths).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            strict: false,
            checksums: false,
            localize: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_from_flags_maps_correctly() {
        let o = RoCrateOptions::from_flags(true, false, false, false);
        assert!(o.enabled && o.checksums && o.localize && !o.strict);
        let off = RoCrateOptions::disabled();
        assert!(!off.enabled);
    }
}
