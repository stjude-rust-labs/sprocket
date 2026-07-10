//! Signer-trust policy applied before rewriting `module-lock.json`.

use std::path::Path;

use anyhow::Context as _;
use wdl_modules::Lockfile;
use wdl_modules::lockfile::DependencyMap;
use wdl_modules::resolver::ChangedSigner;
use wdl_modules::resolver::LockfileDiff;
use wdl_modules::resolver::NewSigner;
use wdl_modules::resolver::RemovedSigner;
use wdl_modules::resolver::SignerIdentityMap;
use wdl_modules::resolver::TrustMode;
use wdl_modules::resolver::TrustStore;
use wdl_modules::signing::SignerIdentity;
use wdl_modules::signing::VerifyingKey;

use crate::commands::module::ActionColor;
use crate::commands::module::print_action;

/// How signer changes should be handled while writing a refreshed lockfile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignerChangeMode {
    /// Refuse signer keys unless already accepted through the trust store.
    Strict,
    /// Prompt to trust new or changed signer keys. The default answer is no.
    Confirm,
    /// Trust new signer keys without prompting but prompt on key changes.
    Tofu,
    /// Trust new or changed signer keys without prompting.
    Auto,
}

impl SignerChangeMode {
    /// Selects the interactive lock-writing mode for module commands.
    pub(crate) fn from_trust_mode(trust_mode: TrustMode) -> Self {
        match trust_mode {
            TrustMode::Confirm => Self::Confirm,
            TrustMode::Tofu => Self::Tofu,
            TrustMode::Auto => Self::Auto,
        }
    }
}

/// A single signer transition found while diffing lockfiles.
enum SignerChange<'a> {
    /// A dependency gained a signer it did not have before.
    Added(&'a NewSigner),
    /// A dependency's signer key changed.
    Changed(&'a ChangedSigner),
    /// A dependency lost its signer.
    Removed(&'a RemovedSigner),
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

impl SignerChange<'_> {
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
                Mode::Tofu | Mode::Auto => Decision::AutoAccept,
            }),
            Self::Changed(_) => Some(match mode {
                Mode::Strict => Decision::Refuse,
                Mode::Confirm | Mode::Tofu => Decision::Prompt,
                Mode::Auto => Decision::AutoAccept,
            }),
            // A removed signature only matters while its key is pinned.
            Self::Removed(_) if !trust.contains_key(&self.key()) => None,
            Self::Removed(_) => Some(match mode {
                Mode::Strict => Decision::Refuse,
                Mode::Confirm | Mode::Tofu => Decision::Prompt,
                Mode::Auto => Decision::AutoAccept,
            }),
        }
    }
}

/// Refuses to rewrite the lockfile when regeneration would introduce,
/// change, or remove a module signer unless explicitly accepted.
///
/// New and changed signer keys require a trusted key or an interactive
/// confirmation, depending on `mode`. Removed signatures are handled by
/// mode too; strict mode refuses while interactive modes can accept them.
pub(crate) fn enforce_signer_trust(
    trust_path: &Path,
    existing: &Lockfile,
    new: &Lockfile,
    identities: &SignerIdentityMap,
    mode: SignerChangeMode,
    colorize: bool,
) -> anyhow::Result<()> {
    let diff = LockfileDiff::compute_with_identities(existing, new, identities);
    if !diff.has_new_signers() && !diff.has_signer_changes() {
        return Ok(());
    }

    let mut trust = TrustStore::load_or_default(trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;

    let changes = diff
        .new_signers
        .iter()
        .map(SignerChange::Added)
        .chain(diff.changed_signers.iter().map(SignerChange::Changed))
        .chain(diff.removed_signers.iter().map(SignerChange::Removed));

    let mut refused = Vec::new();
    let mut prompted = Vec::new();
    let mut accepted = Vec::new();
    for change in changes {
        match change.decide(mode, &trust) {
            None => {}
            Some(SignerDecision::Refuse) => refused.push(change),
            Some(SignerDecision::Prompt) => prompted.push(change),
            Some(SignerDecision::AutoAccept) => accepted.push(change),
        }
    }

    // Prompted changes are accepted or refused as a batch; any hard
    // refusal skips the prompt entirely.
    if !prompted.is_empty() {
        if refused.is_empty() && confirm_signer_key_upgrade(&prompted, &trust)? {
            accepted.append(&mut prompted);
        } else {
            refused.append(&mut prompted);
        }
    }

    // Nothing is accepted while any change is refused.
    if !refused.is_empty() {
        refused.append(&mut accepted);
        let offenders = refused
            .iter()
            .map(|change| change.message(&trust))
            .collect::<Vec<_>>();
        anyhow::bail!(
            "refusing to update `module-lock.json`; signer trust changes require acceptance:\n  \
             {}\n  accept signer trust changes with `sprocket module trust all`",
            offenders.join("\n  ")
        );
    }

    if accepted.is_empty() {
        return Ok(());
    }

    let mut trusted_keys = 0usize;
    let mut trust_dirty = false;
    for change in &accepted {
        if change.adds_trust() {
            trusted_keys += usize::from(trust.insert_key(change.key()));
            upsert_signer_identity(&mut trust, change.key(), change.identity());
            trust_dirty = true;
        }
    }
    if trust_dirty {
        trust
            .save(trust_path)
            .with_context(|| format!("saving trust store at `{}`", trust_path.display()))?;
    }
    print_trust_change_summary(trusted_keys, colorize);
    Ok(())
}

/// Prints the pending signer changes and reads a y/N answer from stdin.
fn confirm_signer_key_upgrade(
    changes: &[SignerChange<'_>],
    trust: &TrustStore,
) -> anyhow::Result<bool> {
    eprintln!("module signer key requires trust changes:");
    for change in changes {
        match change {
            SignerChange::Added(signer) => eprintln!(
                "  `{}` signer key added: {}",
                signer.dep().manifest(),
                render_signer_with_trust(&signer.key, signer.identity.as_ref(), trust)
            ),
            SignerChange::Changed(signer) => match signer.old_key {
                Some(old_key) => eprintln!(
                    "  `{}` signer key changed: {} -> {}",
                    signer.dep().manifest(),
                    render_signer_with_trust(&old_key, None, trust),
                    render_signer_with_trust(&signer.new_key, signer.identity.as_ref(), trust)
                ),
                None => eprintln!(
                    "  `{}` signer key added to previously unsigned module: {}",
                    signer.dep().manifest(),
                    render_signer_with_trust(&signer.new_key, signer.identity.as_ref(), trust)
                ),
            },
            SignerChange::Removed(signer) => eprintln!(
                "  `{}` signer key removed: {}",
                signer.dep().manifest(),
                render_signer_with_trust(&signer.key, None, trust)
            ),
        }
    }
    eprint!("Accept these signer trust changes and update the lockfile? [y/N] ");
    std::io::Write::flush(&mut std::io::stderr()).context("flushing prompt")?;

    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("reading prompt response")?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

/// Signer key and optional identity queued for a trust-store change hint.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SignerTrustHint {
    key: VerifyingKey,
    identity: Option<SignerIdentity>,
}

fn added_signer_message(signer: &NewSigner, trust: &TrustStore) -> String {
    format!(
        "`{}` signer key added ({})",
        signer.dep().manifest(),
        render_signer_with_trust(&signer.key, signer.identity.as_ref(), trust),
    )
}

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

fn removed_signer_message(removed: &RemovedSigner, trust: &TrustStore) -> String {
    format!(
        "`{}` signer key removed '{}'",
        removed.dep().manifest(),
        render_signer_with_trust(&removed.key, None, trust),
    )
}

fn render_signer_with_trust(
    key: &VerifyingKey,
    identity: Option<&SignerIdentity>,
    trust: &TrustStore,
) -> String {
    match identity {
        Some(identity) => render_signer(key, Some(identity)),
        None => match trust.identity(key) {
            Some(identity) => {
                render_identity_fields(key, identity.name.as_deref(), identity.email.as_deref())
            }
            None => render_signer(key, None),
        },
    }
}

pub(crate) fn render_signer(key: &VerifyingKey, identity: Option<&SignerIdentity>) -> String {
    match identity {
        Some(identity) => {
            render_identity_fields(key, identity.name.as_deref(), identity.email.as_deref())
        }
        None => key.to_openssh(),
    }
}

fn render_identity_fields(key: &VerifyingKey, name: Option<&str>, email: Option<&str>) -> String {
    let key = key.to_openssh();
    match (name, email) {
        (Some(name), Some(email)) => format!("{key} {name} <{email}>"),
        (Some(name), None) => format!("{key} {name}"),
        (None, Some(email)) => format!("{key} <{email}>"),
        (None, None) => key,
    }
}

fn push_unique_signer(
    signers: &mut Vec<SignerTrustHint>,
    key: VerifyingKey,
    identity: Option<SignerIdentity>,
) {
    if let Some(existing) = signers.iter_mut().find(|signer| signer.key == key) {
        if existing.identity.is_none() {
            existing.identity = identity;
        }
        return;
    }
    signers.push(SignerTrustHint { key, identity });
}

fn print_trust_change_summary(trusted: usize, colorize: bool) {
    if trusted == 0 {
        print_action(
            "Accepted",
            "signer trust changes",
            colorize,
            ActionColor::Green,
        );
        return;
    }

    print_action(
        "Trusted",
        format!("{trusted} signer keys"),
        colorize,
        ActionColor::Green,
    );
}

/// Adds every signer key recorded in a lockfile to the trust store.
pub(crate) fn accept_lockfile_signers(
    trust_path: &Path,
    lockfile: &Lockfile,
) -> anyhow::Result<usize> {
    let mut trust = TrustStore::load_or_default(trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;
    let mut accepted = 0usize;

    for signer in lockfile_signers(lockfile, &SignerIdentityMap::new()) {
        if trust.insert_key(signer.key) {
            accepted += 1;
        }
        upsert_signer_identity(&mut trust, signer.key, signer.identity);
    }

    trust
        .save(trust_path)
        .with_context(|| format!("saving trust store at `{}`", trust_path.display()))?;
    Ok(accepted)
}

fn lockfile_signers(lockfile: &Lockfile, identities: &SignerIdentityMap) -> Vec<SignerTrustHint> {
    let mut signers = Vec::new();
    collect_lockfile_signers(
        &lockfile.dependencies,
        &mut Vec::new(),
        identities,
        &mut signers,
    );
    signers
}

fn collect_lockfile_signers(
    deps: &DependencyMap,
    chain: &mut Vec<wdl_modules::dependency::DependencyName>,
    identities: &SignerIdentityMap,
    signers: &mut Vec<SignerTrustHint>,
) {
    for (name, entry) in deps {
        chain.push(name.clone());
        if let Some(key) = entry.signer {
            push_unique_signer(signers, key, identities.get(chain).cloned());
        }
        collect_lockfile_signers(&entry.dependencies, chain, identities, signers);
        chain.pop();
    }
}

fn upsert_signer_identity(
    trust: &mut TrustStore,
    key: VerifyingKey,
    identity: Option<SignerIdentity>,
) {
    if let Some(identity) = identity {
        trust.upsert_identity(key, identity.name, identity.email);
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

    fn trust_path(store: &TrustStore) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        store.save(&path).unwrap();
        (dir, path)
    }

    fn empty_trust_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        (dir, path)
    }

    #[test]
    fn enforce_signer_trust_refuses_untrusted_key_change() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, Some(vkey(2)));

        // With an empty trust store the changed key is not accepted.
        let (_dir, path) = empty_trust_path();
        let err = enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("signer key changed"),
            "unexpected error: {err}"
        );

        // Once the new key is trusted, the change is allowed.
        let (_dir, path) = trust_path(&trust_for(vkey(2)));
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .expect("a trusted new key should be accepted");
    }

    #[test]
    fn enforce_signer_trust_confirm_allows_trusted_key_change() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, Some(vkey(2)));

        let (_dir, path) = trust_path(&trust_for(vkey(2)));
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Confirm,
            false,
        )
        .expect("a globally trusted replacement key should not prompt or fail");
    }

    #[test]
    fn enforce_signer_trust_refuses_removal_while_pinned() {
        let url = "https://example.com/repo";
        let existing = signed_lockfile("dep", url, Some(vkey(1)));
        let new = signed_lockfile("dep", url, None);

        // While the key is still pinned, the downgrade to unsigned is refused.
        let (_dir, path) = trust_path(&trust_for(vkey(1)));
        let err = enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("signer key removed"),
            "unexpected error: {err}"
        );

        // With no pin, the downgrade is accepted.
        let (_dir, path) = empty_trust_path();
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .expect("an unpinned downgrade should be accepted");
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

        let (_dir, path) = trust_path(&trust_for(key));
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Auto,
            false,
        )
        .expect("auto mode should accept the batch");
        let trust = TrustStore::load_or_default(&path).unwrap();
        assert!(
            trust.contains_key(&key),
            "key should remain trusted when another dependency still uses it"
        );
    }

    #[test]
    fn enforce_signer_trust_auto_keeps_removed_signer_trusted() {
        let url = "https://example.com/repo";
        let key = vkey(7);
        let existing = signed_lockfile("dep", url, Some(key));
        let new = signed_lockfile("dep", url, None);

        let (_dir, path) = trust_path(&trust_for(key));
        enforce_signer_trust(
            &path,
            &existing,
            &new,
            &SignerIdentityMap::new(),
            SignerChangeMode::Auto,
            false,
        )
        .expect("auto mode should accept the removed signature");
        let trust = TrustStore::load_or_default(&path).unwrap();
        assert!(
            trust.contains_key(&key),
            "accepting a removed module signature should not remove global trust for the signer \
             key"
        );
    }

    #[test]
    fn enforce_signer_trust_allows_unchanged_and_refuses_new_untrusted_signer() {
        let url = "https://example.com/repo";
        let signed = signed_lockfile("dep", url, Some(vkey(1)));
        // Unchanged signer: no-op.
        let (_dir, path) = empty_trust_path();
        enforce_signer_trust(
            &path,
            &signed,
            &signed,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .unwrap();

        // A new signer requires explicit trust.
        let empty = Lockfile::default();
        let err = enforce_signer_trust(
            &path,
            &empty,
            &signed,
            &SignerIdentityMap::new(),
            SignerChangeMode::Strict,
            false,
        )
        .expect_err("a newly introduced signer should require trust");
        assert!(
            err.to_string().contains("signer key added"),
            "unexpected error: {err}"
        );
    }
}
