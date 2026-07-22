//! Command-line signer-trust policy for module porcelain commands.
//!
//! This module owns signer-change decisions, interactive prompting, and
//! rendering. Trust-store persistence and lockfile signer collection live in
//! [`super::trust_store`].

use clap::ValueEnum;
use wdl_modules::Lockfile;
use wdl_modules::resolver::TrustMode;
use wdl_modules::resolver::TrustStore;
use wdl_modules::resolver::lock::ChangedSigner;
use wdl_modules::resolver::lock::LockfileDiff;
use wdl_modules::resolver::lock::NewSigner;
use wdl_modules::resolver::lock::RemovedSigner;
use wdl_modules::resolver::lock::SignerIdentityMap;
use wdl_modules::signing::SignerIdentity;
use wdl_modules::signing::VerifyingKey;

use super::trust_store::TrustStoreFile;
use super::trust_store::upsert_signer_identity;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
use crate::commands::output::count_noun;
use crate::config::Config;

const ACCEPT: Action = Action::new("Accepted", "accept");
const TRUST: Action = Action::new("Trusted", "trust");

/// Command-line override for module signer trust mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub(super) enum TrustModeArg {
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

/// How signer changes should be handled while writing a refreshed lockfile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SignerChangeMode {
    /// Refuse signer keys unless already accepted through the trust store.
    Strict,
    /// Prompt to trust new or changed signer keys. The default answer is no.
    Confirm,
    /// Trust new signer keys without prompting but prompt on key changes.
    Tofu,
    /// Trust new or changed signer keys without prompting.
    AutoAccept,
}

impl SignerChangeMode {
    /// Selects the interactive lock-writing mode for module commands.
    pub(super) fn from_trust_mode(trust_mode: TrustMode) -> Self {
        match trust_mode {
            TrustMode::Confirm => Self::Confirm,
            TrustMode::Tofu => Self::Tofu,
            TrustMode::AutoAccept => Self::AutoAccept,
        }
    }
}

/// Resolves the signer trust mode using CLI override first, then config.
pub(super) fn signer_change_mode(
    config: &Config,
    trust_mode: Option<TrustModeArg>,
) -> SignerChangeMode {
    SignerChangeMode::from_trust_mode(
        trust_mode
            .map(TrustMode::from)
            .unwrap_or(config.modules.trust_mode),
    )
}

/// A single owned signer transition found while diffing lockfiles.
///
/// The changes are cloned out of the diff so decision plans never borrow a
/// temporary [`LockfileDiff`].
#[derive(Clone, Debug)]
enum SignerChange {
    /// A dependency gained a signer it did not have before.
    Added(NewSigner),
    /// A dependency's signer key changed.
    Changed(ChangedSigner),
    /// A dependency lost its signer.
    Removed(RemovedSigner),
}

/// What policy requires for one signer change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SignerDecision {
    /// The change is refused outright.
    Refuse,
    /// The change requires interactive confirmation.
    Prompt,
    /// The change is accepted without prompting.
    AutoAccept,
}

impl SignerChange {
    /// The key that acceptance would add to (or removal leaves pinned in)
    /// the trust store.
    fn key(&self) -> VerifyingKey {
        match self {
            Self::Added(added) => added.key,
            Self::Changed(changed) => changed.new_key,
            Self::Removed(removed) => removed.key,
        }
    }

    /// Signer identity metadata carried by the change, if any.
    fn identity(&self) -> Option<SignerIdentity> {
        match self {
            Self::Added(added) => added.identity.clone(),
            Self::Changed(changed) => changed.identity.clone(),
            Self::Removed(_) => None,
        }
    }

    /// Whether accepting this change should add its key to the trust store.
    fn adds_trust(&self) -> bool {
        !matches!(self, Self::Removed(_))
    }

    /// Renders the refusal message for this change.
    fn message(&self, trust: &TrustStore) -> String {
        match self {
            Self::Added(added) => added_signer_message(added, trust),
            Self::Changed(changed) => changed_signer_message(changed, trust),
            Self::Removed(removed) => removed_signer_message(removed, trust),
        }
    }

    /// Renders the accepted-summary detail line for this transition.
    fn accepted_summary(&self) -> String {
        match self {
            Self::Added(signer) => {
                format!("`{}`: signer added", signer.dep().manifest())
            }
            Self::Changed(signer) => {
                let transition = if signer.old_key.is_some() {
                    "signer changed"
                } else {
                    "signer added"
                };
                format!("`{}`: {transition}", signer.dep().manifest())
            }
            Self::Removed(signer) => {
                format!("`{}`: signer removed", signer.dep().manifest())
            }
        }
    }

    /// Decides how `mode` handles this change, or `None` when the trust
    /// store already settles it.
    fn decide(&self, mode: SignerChangeMode, trust: &TrustStore) -> Option<SignerDecision> {
        use SignerChangeMode as Mode;
        use SignerDecision as Decision;

        match self {
            // A new or changed key is settled once it is trusted.
            Self::Added(_) | Self::Changed(_) if trust.contains_key(&self.key()) => None,
            Self::Added(_) => Some(match mode {
                Mode::Strict => Decision::Refuse,
                Mode::Confirm => Decision::Prompt,
                Mode::Tofu | Mode::AutoAccept => Decision::AutoAccept,
            }),
            Self::Changed(_) => Some(match mode {
                Mode::Strict => Decision::Refuse,
                Mode::Confirm | Mode::Tofu => Decision::Prompt,
                Mode::AutoAccept => Decision::AutoAccept,
            }),
            Self::Removed(_) => Some(match mode {
                Mode::Strict => Decision::Refuse,
                Mode::Confirm | Mode::Tofu => Decision::Prompt,
                Mode::AutoAccept => Decision::AutoAccept,
            }),
        }
    }
}

/// The all-or-nothing partition of signer changes produced by evaluating a
/// lockfile diff against the trust store.
#[derive(Debug)]
struct SignerDecisionPlan {
    /// Changes refused outright by policy.
    refused: Vec<SignerChange>,
    /// Changes that require a single interactive confirmation.
    prompted: Vec<SignerChange>,
    /// Changes accepted without prompting.
    accepted: Vec<SignerChange>,
}

impl SignerDecisionPlan {
    /// Whether the diff produced any signer change requiring action.
    fn is_empty(&self) -> bool {
        self.refused.is_empty() && self.prompted.is_empty() && self.accepted.is_empty()
    }

    /// Applies the plan against `trust_file`.
    ///
    /// Application is all-or-nothing: any refusal (including a declined
    /// prompt) voids the whole batch, suppresses trust-store writes, and
    /// bails without mutating anything. Otherwise every accepted addition or
    /// change is trusted, the store is saved once when it changed, and each
    /// accepted transition is reported.
    fn apply(self, trust_file: &mut TrustStoreFile, output: CommandOutput) -> anyhow::Result<()> {
        self.apply_with_confirmation(trust_file, output, confirm_signer_key_upgrade)
    }

    /// Applies the plan, resolving the batched prompt through `confirm`.
    ///
    /// This is the injection seam behind [`SignerDecisionPlan::apply`], which
    /// supplies the production [`confirm_signer_key_upgrade`]. Isolating the
    /// confirmation lets tests prove that a hard refusal short-circuits the
    /// prompt without ever consulting it, and that the batch mutates nothing.
    fn apply_with_confirmation<F>(
        mut self,
        trust_file: &mut TrustStoreFile,
        output: CommandOutput,
        confirm: F,
    ) -> anyhow::Result<()>
    where
        F: FnOnce(&[SignerChange], &TrustStore, CommandOutput) -> anyhow::Result<bool>,
    {
        // Prompted changes are accepted or refused as a batch; any hard
        // refusal skips the prompt entirely.
        if !self.prompted.is_empty() {
            if self.refused.is_empty() && confirm(&self.prompted, trust_file.store(), output)? {
                self.accepted.append(&mut self.prompted);
            } else {
                self.refused.append(&mut self.prompted);
            }
        }

        // Nothing is accepted while any change is refused.
        if !self.refused.is_empty() {
            self.refused.append(&mut self.accepted);
            let offenders = self
                .refused
                .iter()
                .map(|change| change.message(trust_file.store()))
                .collect::<Vec<_>>();
            anyhow::bail!(
                "refusing to update `module-lock.json`; signer trust changes require \
                 acceptance:\n  {}\n  accept signer trust changes with `sprocket dev module trust \
                 all`",
                offenders.join("\n  ")
            );
        }

        if self.accepted.is_empty() {
            return Ok(());
        }

        let mut trusted_keys = 0usize;
        let mut trust_dirty = false;
        {
            let trust = trust_file.store_mut();
            for change in &self.accepted {
                if change.adds_trust() {
                    trusted_keys += usize::from(trust.insert_key(change.key()));
                    upsert_signer_identity(trust, change.key(), change.identity());
                    trust_dirty = true;
                }
            }
        }
        if trust_dirty {
            trust_file.save()?;
        }
        print_trust_change_summary(&self.accepted, trusted_keys, output);
        Ok(())
    }
}

/// Enforces signer-change policy for an existing and refreshed lockfile,
/// loading and persisting through the default trust store.
pub(super) fn enforce_lockfile_signer_policy(
    existing: &Lockfile,
    new: &Lockfile,
    identities: &SignerIdentityMap,
    mode: SignerChangeMode,
    output: CommandOutput,
) -> anyhow::Result<()> {
    let mut trust_file = TrustStoreFile::load(crate::analysis::default_trust_path())?;
    SignerTrustPolicy::new(mode).enforce(existing, new, identities, &mut trust_file, output)
}

/// The signer-trust decision engine bound to a single change mode.
#[derive(Clone, Copy, Debug)]
pub(super) struct SignerTrustPolicy {
    /// The mode governing how signer changes are handled.
    mode: SignerChangeMode,
}

impl SignerTrustPolicy {
    /// Creates a policy for the given signer change mode.
    pub(super) fn new(mode: SignerChangeMode) -> Self {
        Self { mode }
    }

    /// Evaluates a lockfile diff against the trust store, partitioning every
    /// signer change into refused, prompted, and accepted buckets. This does
    /// no terminal or filesystem I/O.
    fn evaluate(&self, diff: &LockfileDiff, trust: &TrustStore) -> SignerDecisionPlan {
        let changes = diff
            .new_signers
            .iter()
            .cloned()
            .map(SignerChange::Added)
            .chain(
                diff.changed_signers
                    .iter()
                    .cloned()
                    .map(SignerChange::Changed),
            )
            .chain(
                diff.removed_signers
                    .iter()
                    .cloned()
                    .map(SignerChange::Removed),
            );

        let mut refused = Vec::new();
        let mut prompted = Vec::new();
        let mut accepted = Vec::new();
        for change in changes {
            match change.decide(self.mode, trust) {
                None => {}
                Some(SignerDecision::Refuse) => refused.push(change),
                Some(SignerDecision::Prompt) => prompted.push(change),
                Some(SignerDecision::AutoAccept) => accepted.push(change),
            }
        }

        SignerDecisionPlan {
            refused,
            prompted,
            accepted,
        }
    }

    /// Refuses to rewrite the lockfile when regeneration would introduce,
    /// change, or remove a module signer unless explicitly accepted.
    ///
    /// New and changed signer keys require a trusted key or an interactive
    /// confirmation, depending on the mode. Removed signatures are handled by
    /// mode too; strict mode refuses while interactive modes can accept them.
    pub(super) fn enforce(
        &self,
        existing: &Lockfile,
        new: &Lockfile,
        identities: &SignerIdentityMap,
        trust_file: &mut TrustStoreFile,
        output: CommandOutput,
    ) -> anyhow::Result<()> {
        let diff = LockfileDiff::compute_with_identities(existing, new, identities);
        if !diff.has_new_signers() && !diff.has_signer_changes() {
            return Ok(());
        }

        let plan = self.evaluate(&diff, trust_file.store());
        if plan.is_empty() {
            return Ok(());
        }
        plan.apply(trust_file, output)
    }
}

/// Prints the pending signer changes and reads a y/N answer from stdin.
fn confirm_signer_key_upgrade(
    changes: &[SignerChange],
    trust: &TrustStore,
    output: CommandOutput,
) -> anyhow::Result<bool> {
    output.diagnostic("module signer key requires trust changes");
    for change in changes {
        match change {
            SignerChange::Added(signer) => {
                output.diagnostic_blank();
                output.diagnostic(format!("  Module     `{}`", signer.dep().manifest()));
                output.diagnostic("  Change     signer added");
                output.diagnostic(format!(
                    "  Signer     {}",
                    render_signer_with_trust(&signer.key, signer.identity.as_ref(), trust)
                ));
            }
            SignerChange::Changed(signer) => match signer.old_key {
                Some(old_key) => {
                    output.diagnostic_blank();
                    output.diagnostic(format!("  Module     `{}`", signer.dep().manifest()));
                    output.diagnostic("  Change     signer changed");
                    output.diagnostic(format!(
                        "  Previous   {}",
                        render_signer_with_trust(&old_key, None, trust)
                    ));
                    output.diagnostic(format!(
                        "  Current    {}",
                        render_signer_with_trust(&signer.new_key, signer.identity.as_ref(), trust)
                    ));
                }
                None => {
                    output.diagnostic_blank();
                    output.diagnostic(format!("  Module     `{}`", signer.dep().manifest()));
                    output.diagnostic("  Change     previously unsigned module gained a signer");
                    output.diagnostic(format!(
                        "  Signer     {}",
                        render_signer_with_trust(&signer.new_key, signer.identity.as_ref(), trust)
                    ));
                }
            },
            SignerChange::Removed(signer) => {
                output.diagnostic_blank();
                output.diagnostic(format!("  Module     `{}`", signer.dep().manifest()));
                output.diagnostic("  Change     signer removed; dependency is now unsigned");
                output.diagnostic(format!(
                    "  Previous   {}",
                    render_signer_with_trust(&signer.key, None, trust)
                ));
            }
        }
    }
    output.diagnostic_blank();
    output.confirm("Accept these signer trust changes and update the lockfile?")
}

/// Renders the refusal message for a newly introduced signer.
fn added_signer_message(signer: &NewSigner, trust: &TrustStore) -> String {
    format!(
        "`{}` signer key added ({})",
        signer.dep().manifest(),
        render_signer_with_trust(&signer.key, signer.identity.as_ref(), trust),
    )
}

/// Renders the refusal message for a changed or newly signed signer.
fn changed_signer_message(changed: &ChangedSigner, trust: &TrustStore) -> String {
    match changed.old_key {
        Some(old_key) => format!(
            "`{}` signer key changed from '{}' to '{}'",
            changed.dep().manifest(),
            render_signer_with_trust(&old_key, None, trust),
            render_signer_with_trust(&changed.new_key, changed.identity.as_ref(), trust),
        ),
        None => format!(
            "`{}` signer key added to previously unsigned module ({})",
            changed.dep().manifest(),
            render_signer_with_trust(&changed.new_key, changed.identity.as_ref(), trust),
        ),
    }
}

/// Renders the refusal message for a removed signer.
fn removed_signer_message(removed: &RemovedSigner, trust: &TrustStore) -> String {
    format!(
        "`{}` signer key removed '{}'",
        removed.dep().manifest(),
        render_signer_with_trust(&removed.key, None, trust),
    )
}

/// Renders a signer key, preferring change-supplied identity metadata and
/// falling back to any identity recorded in the trust store.
fn render_signer_with_trust(
    key: &VerifyingKey,
    identity: Option<&SignerIdentity>,
    trust: &TrustStore,
) -> String {
    match identity {
        Some(identity) => render_signer(key, Some(identity)),
        None => match trust.identity(key) {
            Some(identity) => render_identity_fields(
                key,
                identity.name.as_deref(),
                identity.email.as_deref(),
                identity.comment.as_deref(),
            ),
            None => render_signer(key, None),
        },
    }
}

/// Renders a signer key with its optional identity metadata.
pub(super) fn render_signer(key: &VerifyingKey, identity: Option<&SignerIdentity>) -> String {
    match identity {
        Some(SignerIdentity::Signer { name, email }) => {
            render_identity_fields(key, Some(name), Some(email), None)
        }
        Some(SignerIdentity::Comment { comment }) => {
            render_identity_fields(key, None, None, Some(comment))
        }
        None => key.to_openssh(),
    }
}

/// Renders a signer key annotated with any available name and email.
fn render_identity_fields(
    key: &VerifyingKey,
    name: Option<&str>,
    email: Option<&str>,
    comment: Option<&str>,
) -> String {
    let key = key.to_openssh();
    if let Some(comment) = comment {
        return format!("{key} {comment}");
    }
    match (name, email) {
        (Some(name), Some(email)) => format!("{key} {name} <{email}>"),
        (Some(name), None) => format!("{key} {name}"),
        (None, Some(email)) => format!("{key} <{email}>"),
        (None, None) => key,
    }
}

/// Prints a summary action line for accepted signer trust changes.
fn print_trust_change_summary(accepted: &[SignerChange], trusted: usize, output: CommandOutput) {
    output.completed(
        ACCEPT,
        count_noun(accepted.len(), "signer change", "signer changes"),
    );
    for change in accepted {
        output.detail("Signer", change.accepted_summary());
    }
    if trusted > 0 {
        output.completed(TRUST, count_noun(trusted, "signer key", "signer keys"));
    }
}

#[cfg(test)]
mod tests {
    use wdl_modules::dependency::DependencyName;

    use super::*;

    /// Builds a one-entry lockfile whose Git dependency `dep` from `url`
    /// carries the given optional signer.
    fn signed_lockfile(
        dep: &str,
        url: &str,
        signer: Option<wdl_modules::signing::VerifyingKey>,
    ) -> Lockfile {
        use wdl_modules::lockfile::DependencyEntry;
        use wdl_modules::lockfile::ResolvedSource;

        let mut dependencies = std::collections::BTreeMap::new();
        dependencies.insert(
            dep.parse().unwrap(),
            DependencyEntry {
                source: ResolvedSource::Git {
                    git: url.parse().unwrap(),
                    sha: "0000000000000000000000000000000000000000".parse().unwrap(),
                    selector: wdl_modules::dependency::GitSelector::Version("^1".parse().unwrap()),
                    path: None,
                },
                checksum: Some(
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                        .parse()
                        .unwrap(),
                ),
                signer,
                dependencies: std::collections::BTreeMap::new(),
            },
        );
        Lockfile {
            version: wdl_modules::lockfile::LOCKFILE_VERSION,
            dependencies,
        }
    }

    fn vkey(seed: u64) -> wdl_modules::signing::VerifyingKey {
        wdl_modules::signing::test_utils::signing_key_from_seed(seed).verifying_key()
    }

    fn trust_for(key: wdl_modules::signing::VerifyingKey) -> TrustStore {
        let mut store = TrustStore::default();
        store.insert_key(key);
        store
    }

    /// A [`TrustStoreFile`] backed by a temporary directory seeded with the
    /// given store.
    fn trust_file(store: &TrustStore) -> (tempfile::TempDir, TrustStoreFile) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        store.save(&path).unwrap();
        let file = TrustStoreFile::load(path).unwrap();
        (dir, file)
    }

    /// A [`TrustStoreFile`] backed by a temporary directory with no store on
    /// disk yet.
    fn empty_trust_file() -> (tempfile::TempDir, TrustStoreFile) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        let file = TrustStoreFile::load(path).unwrap();
        (dir, file)
    }

    fn enforce(
        existing: &Lockfile,
        new: &Lockfile,
        mode: SignerChangeMode,
        trust_file: &mut TrustStoreFile,
    ) -> anyhow::Result<()> {
        SignerTrustPolicy::new(mode).enforce(
            existing,
            new,
            &SignerIdentityMap::new(),
            trust_file,
            CommandOutput::new(false),
        )
    }

    #[test]
    fn render_signer_includes_identity() {
        let key: VerifyingKey =
            match "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINiRUmfYzFTjksGItM2fSm9s1eCL8NnMJGQgW724Uph1"
                .parse()
            {
                Ok(key) => key,
                Err(err) => panic!("failed to parse key: {err}"),
            };
        let identity = SignerIdentity::Signer {
            name: "Spellbook Maintainer".to_string(),
            email: "spellbook-fixture@example.com".to_string(),
        };
        assert_eq!(
            render_signer(&key, Some(&identity)),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINiRUmfYzFTjksGItM2fSm9s1eCL8NnMJGQgW724Uph1 \
             Spellbook Maintainer <spellbook-fixture@example.com>"
        );
    }

    #[test]
    fn enforce_signer_trust_refuses_untrusted_key_change() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, Some(vkey(2)));

        // With an empty trust store the changed key is not accepted.
        let (_dir, mut file) = empty_trust_file();
        let err = enforce(&existing, &new, SignerChangeMode::Strict, &mut file).unwrap_err();
        assert!(
            err.to_string().contains("signer key changed"),
            "unexpected error: {err}"
        );

        // Once the new key is trusted, the change is allowed.
        let (_dir, mut file) = trust_file(&trust_for(vkey(2)));
        enforce(&existing, &new, SignerChangeMode::Strict, &mut file)
            .expect("a trusted new key should be accepted");
    }

    #[test]
    fn enforce_signer_trust_confirm_allows_trusted_key_change() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, Some(vkey(2)));

        let (_dir, mut file) = trust_file(&trust_for(vkey(2)));
        enforce(&existing, &new, SignerChangeMode::Confirm, &mut file)
            .expect("a globally trusted replacement key should not prompt or fail");
    }

    #[test]
    fn enforce_signer_trust_refuses_removal_regardless_of_global_trust() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, None);

        let (_dir, mut file) = trust_file(&trust_for(vkey(1)));
        let err = enforce(&existing, &new, SignerChangeMode::Strict, &mut file).unwrap_err();
        assert!(
            err.to_string().contains("signer key removed"),
            "unexpected error: {err}"
        );

        let (_dir, mut file) = empty_trust_file();
        let err = enforce(&existing, &new, SignerChangeMode::Strict, &mut file).unwrap_err();
        assert!(
            err.to_string().contains("signer key removed"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn enforce_signer_trust_auto_keeps_key_when_added_and_removed_together() {
        let url = "https://example.com/repo";
        let key = vkey(7);
        let dep_added: DependencyName = "added".parse().unwrap();
        let dep_removed: DependencyName = "removed".parse().unwrap();

        let mut existing = Lockfile::default();
        let mut existing_added = signed_lockfile("added", url, None);
        existing.dependencies.insert(
            dep_added.clone(),
            existing_added.dependencies.remove(&dep_added).unwrap(),
        );
        let mut existing_removed = signed_lockfile("removed", url, Some(key));
        existing.dependencies.insert(
            dep_removed.clone(),
            existing_removed.dependencies.remove(&dep_removed).unwrap(),
        );

        let mut new = Lockfile::default();
        let mut new_added = signed_lockfile("added", url, Some(key));
        new.dependencies.insert(
            dep_added.clone(),
            new_added.dependencies.remove(&dep_added).unwrap(),
        );
        let mut new_removed = signed_lockfile("removed", url, None);
        new.dependencies.insert(
            dep_removed.clone(),
            new_removed.dependencies.remove(&dep_removed).unwrap(),
        );

        let (_dir, mut file) = trust_file(&trust_for(key));
        enforce(&existing, &new, SignerChangeMode::AutoAccept, &mut file)
            .expect("auto-accept mode should accept the batch");
        assert!(
            file.store().contains_key(&key),
            "key should remain trusted when another dependency still uses it"
        );
    }

    #[test]
    fn enforce_signer_trust_auto_keeps_removed_signer_trusted() {
        let url = "https://example.com/repo";
        let key = vkey(7);
        let existing = signed_lockfile("dep", url, Some(key));
        let new = signed_lockfile("dep", url, None);

        let (_dir, mut file) = trust_file(&trust_for(key));
        enforce(&existing, &new, SignerChangeMode::AutoAccept, &mut file)
            .expect("auto-accept mode should accept the removed signature");
        assert!(
            file.store().contains_key(&key),
            "accepting a removed module signature should not remove global trust for the signer \
             key"
        );
    }

    #[test]
    fn mixed_batch_refuses_atomically_without_prompting_or_persisting() {
        // A batch that pairs an auto-acceptable addition (a brand-new signed
        // dependency, which TOFU would trust on its own) with a refused
        // transition is all-or-nothing:
        //   * no prompt is shown, because a hard refusal short-circuits the
        //     confirmation entirely (`prompted` stays untouched);
        //   * the otherwise auto-accepted key and identity are never inserted;
        //   * `apply` returns an error, so the caller never writes the proposed
        //     lockfile.
        let auto_key = vkey(41);
        let auto_identity = SignerIdentity::Signer {
            name: "Auto Trusted".to_string(),
            email: "auto-trusted@example.com".to_string(),
        };
        let refused_key = vkey(42);

        // Matrix: the auto-acceptable addition is paired with each refusable
        // "other" transition kind.
        let refused_variants = [
            SignerChange::Removed(RemovedSigner {
                dep_chain: vec!["beta".parse().unwrap()],
                key: refused_key,
            }),
            SignerChange::Changed(ChangedSigner {
                dep_chain: vec!["beta".parse().unwrap()],
                old_key: Some(vkey(43)),
                new_key: refused_key,
                identity: None,
            }),
        ];

        for refused in refused_variants {
            let accepted = SignerChange::Added(NewSigner {
                dep_chain: vec!["alpha".parse().unwrap()],
                key: auto_key,
                identity: Some(auto_identity.clone()),
            });
            let plan = SignerDecisionPlan {
                refused: vec![refused],
                prompted: Vec::new(),
                accepted: vec![accepted],
            };

            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("trust.toml");
            let mut file = TrustStoreFile::load(path.clone()).unwrap();

            let err = plan
                .apply(&mut file, CommandOutput::new(false))
                .expect_err("a batch containing a refusal must be refused as a whole");
            assert!(
                err.to_string().contains("signer trust changes require"),
                "unexpected error: {err}"
            );

            // The otherwise auto-accepted key and identity are not persisted in
            // memory...
            assert!(
                !file.store().contains_key(&auto_key),
                "auto-accepted key must not be trusted when the batch is refused"
            );
            assert!(
                file.store().identity(&auto_key).is_none(),
                "auto-accepted identity must not be recorded when the batch is refused"
            );
            // ...nor on disk: the refused batch writes no trust store at all.
            let reloaded = TrustStore::load_or_default(&path).unwrap();
            assert!(
                !reloaded.contains_key(&auto_key),
                "refused batch must not write the trust store"
            );
        }
    }

    #[test]
    fn refused_batch_never_invokes_confirmation_and_persists_nothing() {
        // A plan that pairs a hard refusal with a prompted change *and* an
        // auto-accepted addition must void the entire batch without ever
        // consulting the interactive confirmation: the hard refusal
        // short-circuits the prompt. This drives `apply` through its
        // `apply_with_confirmation` seam with a closure that panics if called,
        // proving the callback is never invoked while every change lands in the
        // refusal report and nothing is mutated or persisted.
        let refused = SignerChange::Removed(RemovedSigner {
            dep_chain: vec!["refused-dep".parse().unwrap()],
            key: vkey(51),
        });
        let prompted = SignerChange::Changed(ChangedSigner {
            dep_chain: vec!["prompted-dep".parse().unwrap()],
            old_key: Some(vkey(52)),
            new_key: vkey(53),
            identity: None,
        });
        let auto_key = vkey(54);
        let accepted = SignerChange::Added(NewSigner {
            dep_chain: vec!["accepted-dep".parse().unwrap()],
            key: auto_key,
            identity: Some(SignerIdentity::Signer {
                name: "Auto Trusted".to_string(),
                email: "auto-trusted@example.com".to_string(),
            }),
        });

        let plan = SignerDecisionPlan {
            refused: vec![refused],
            prompted: vec![prompted],
            accepted: vec![accepted],
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        let mut file = TrustStoreFile::load(path.clone()).unwrap();

        // The confirmation closure both counts invocations and panics, so any
        // call fails the test immediately and unambiguously.
        let mut confirmations = 0usize;
        let err = plan
            .apply_with_confirmation(&mut file, CommandOutput::new(false), |_, _, _| {
                confirmations += 1;
                panic!("confirmation must not be consulted when a change is hard-refused");
            })
            .expect_err("a batch containing a refusal must be refused as a whole");

        assert_eq!(
            confirmations, 0,
            "the confirmation closure must never be invoked for a refused batch"
        );

        // Every change - refused, prompted, and auto-accepted - is surfaced in
        // the single refusal report.
        let message = err.to_string();
        assert!(
            message.contains("signer trust changes require"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains("refused-dep"),
            "the refused change must appear in the refusal report: {message}"
        );
        assert!(
            message.contains("prompted-dep"),
            "the prompted change must appear in the refusal report: {message}"
        );
        assert!(
            message.contains("accepted-dep"),
            "the auto-accepted change must appear in the refusal report: {message}"
        );

        // No trust mutation in memory...
        assert!(
            !file.store().contains_key(&auto_key),
            "auto-accepted key must not be trusted when the batch is refused"
        );
        assert!(
            file.store().identity(&auto_key).is_none(),
            "auto-accepted identity must not be recorded when the batch is refused"
        );
        // ...nor on disk: the refused batch writes no trust store at all.
        let reloaded = TrustStore::load_or_default(&path).unwrap();
        assert!(
            !reloaded.contains_key(&auto_key),
            "refused batch must not write the trust store"
        );
    }

    #[test]
    fn enforce_signer_trust_allows_unchanged_and_refuses_new_untrusted_signer() {
        let url = "https://example.com/repo";
        let signed = signed_lockfile("dep", url, Some(vkey(1)));
        // Unchanged signer: no-op.
        let (_dir, mut file) = empty_trust_file();
        enforce(&signed, &signed, SignerChangeMode::Strict, &mut file).unwrap();

        // A new signer requires explicit trust.
        let empty = Lockfile::default();
        let err = enforce(&empty, &signed, SignerChangeMode::Strict, &mut file)
            .expect_err("a newly introduced signer should require trust");
        assert!(
            err.to_string().contains("signer key added"),
            "unexpected error: {err}"
        );
    }
}
