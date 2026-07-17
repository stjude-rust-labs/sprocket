//! Command-line signer-trust policy plumbing for module porcelain commands.

use clap::ValueEnum;
use wdl_modules::Lockfile;
use wdl_modules::resolver::TrustMode;
use wdl_modules::resolver::lock::SignerIdentityMap;

use super::trust_policy::SignerChangeMode;
use crate::commands::output::CommandOutput;
use crate::config::Config;

/// Command-line override for module signer trust mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum TrustModeArg {
    /// Prompt before trusting signer keys.
    Confirm,
    /// Trust first-seen signer keys automatically, then prompt on changes.
    Tofu,
    /// Trust signer keys automatically.
    AutoAccept,
}

impl From<TrustModeArg> for TrustMode {
    fn from(value: TrustModeArg) -> Self {
        match value {
            TrustModeArg::Confirm => TrustMode::Confirm,
            TrustModeArg::Tofu => TrustMode::Tofu,
            TrustModeArg::AutoAccept => TrustMode::AutoAccept,
        }
    }
}

/// Resolves the signer trust mode using CLI override first, then config.
pub(crate) fn signer_change_mode(
    config: &Config,
    trust_mode: Option<TrustModeArg>,
) -> SignerChangeMode {
    SignerChangeMode::from_trust_mode(
        trust_mode
            .map(TrustMode::from)
            .unwrap_or(config.modules.trust_mode),
    )
}

/// Enforces signer-change policy for an existing and refreshed lockfile.
pub(crate) fn enforce_lockfile_signer_policy(
    existing: &Lockfile,
    new: &Lockfile,
    identities: &SignerIdentityMap,
    mode: SignerChangeMode,
    output: CommandOutput,
) -> anyhow::Result<()> {
    let trust_path = crate::analysis::default_trust_path();
    super::trust_policy::enforce_signer_trust(&trust_path, existing, new, identities, mode, output)
}
