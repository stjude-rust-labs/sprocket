//! Credential callback setup for Git remotes.

use git2::FetchOptions;
use git2::RemoteCallbacks;

/// Default credential resolver constrained to libgit2's requested credential
/// mask.
fn default_credentials(
    url: &str,
    username: Option<&str>,
    allowed: git2::CredentialType,
) -> Result<git2::Cred, git2::Error> {
    credential_for(select_credential(allowed), url, username)
}

/// Whether Git operations should use credential helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CredentialMode {
    /// Use the user's configured Git credential helpers and ssh-agent.
    Enabled,
    /// Do not attach any credential callbacks.
    Disabled,
}

/// The credential mechanism selected from libgit2's requested credential mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CredentialKind {
    /// Supply only the username requested by libgit2.
    Username,
    /// Supply credentials from the SSH agent.
    SshAgent,
    /// Supply credentials through the configured Git credential helper.
    Helper,
    /// Supply libgit2's default credential.
    Default,
}

/// Selects a credential mechanism compatible with the requested type mask.
pub(crate) fn select_credential(allowed: git2::CredentialType) -> CredentialKind {
    // libgit2 requests USERNAME first for an SSH URL without an embedded user
    // (ssh_libssh2.c:829), and rejects credentials outside the requested mask
    // (ssh_libssh2.c:415-419).
    if allowed.contains(git2::CredentialType::USERNAME) {
        CredentialKind::Username
    } else if allowed.contains(git2::CredentialType::SSH_KEY) {
        CredentialKind::SshAgent
    } else if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
        CredentialKind::Helper
    } else {
        CredentialKind::Default
    }
}

/// Credential and transfer rules applied to one Git operation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FetchPolicy {
    /// Whether credentials may be supplied.
    pub credentials: CredentialMode,
    /// Maximum bytes accepted from the remote.
    pub max_transfer_bytes: Option<u64>,
}

/// Shared state recording transfer progress and cap aborts.
#[derive(Clone, Debug, Default)]
pub(crate) struct TransferWatch(std::rc::Rc<std::cell::Cell<u64>>);

impl TransferWatch {
    /// Returns the received byte count when the transfer exceeded its cap.
    pub(crate) fn aborted_at(&self) -> Option<u64> {
        let received = self.0.get();
        (received > 0).then_some(received)
    }
}

/// Builds remote callbacks wired up with credentials and transfer limits.
pub(crate) fn default_callbacks<'cb>(policy: FetchPolicy) -> (RemoteCallbacks<'cb>, TransferWatch) {
    let watch = TransferWatch::default();
    let progress_watch = watch.clone();
    let mut cb = RemoteCallbacks::new();
    if policy.credentials == CredentialMode::Enabled {
        cb.credentials(default_credentials);
    }
    cb.transfer_progress(move |progress| {
        let received = progress.received_bytes() as u64;
        if let Some(limit) = policy.max_transfer_bytes
            && received > limit
        {
            progress_watch.0.set(received);
            return false;
        }
        true
    });
    (cb, watch)
}

/// Builds fetch options configured with credentials and transfer limits.
pub(crate) fn default_fetch_options<'fo>(
    policy: FetchPolicy,
) -> (FetchOptions<'fo>, TransferWatch) {
    let (callbacks, watch) = default_callbacks(policy);
    let mut opts = FetchOptions::new();
    opts.remote_callbacks(callbacks);
    (opts, watch)
}

/// Builds an authentication error while preserving the acquisition message.
fn authentication_error(error: git2::Error, class: git2::ErrorClass) -> git2::Error {
    git2::Error::new(git2::ErrorCode::Auth, class, error.message())
}

/// Builds a credential for the selected mechanism.
pub(crate) fn credential_for(
    kind: CredentialKind,
    url: &str,
    username: Option<&str>,
) -> Result<git2::Cred, git2::Error> {
    match kind {
        CredentialKind::Username => git2::Cred::username(username.unwrap_or("git")),
        CredentialKind::SshAgent => git2::Cred::ssh_key_from_agent(username.unwrap_or("git")),
        CredentialKind::Helper => {
            let config = git2::Config::open_default()
                .map_err(|error| authentication_error(error, git2::ErrorClass::Config))?;
            git2::Cred::credential_helper(&config, url, username)
                .map_err(|error| authentication_error(error, git2::ErrorClass::Callback))
        }
        CredentialKind::Default => git2::Cred::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::CredentialKind;
    use super::CredentialMode;
    use super::FetchPolicy;
    use super::authentication_error;
    use super::default_fetch_options;
    use super::select_credential;

    #[test]
    fn authentication_error_preserves_message_and_uses_auth_code() {
        let error = authentication_error(
            git2::Error::from_str("missing helper"),
            git2::ErrorClass::Callback,
        );

        assert_eq!(error.code(), git2::ErrorCode::Auth);
        assert!(error.message().contains("missing helper"));
    }

    #[test]
    fn credential_selection_respects_the_allowed_mask() {
        use git2::CredentialType as CT;

        let cases = [
            (CT::USERNAME, CredentialKind::Username),
            (CT::USERNAME | CT::SSH_KEY, CredentialKind::Username),
            (CT::SSH_KEY, CredentialKind::SshAgent),
            (CT::SSH_KEY | CT::SSH_MEMORY, CredentialKind::SshAgent),
            (
                CT::SSH_KEY | CT::USER_PASS_PLAINTEXT,
                CredentialKind::SshAgent,
            ),
            (CT::USER_PASS_PLAINTEXT, CredentialKind::Helper),
            (CT::empty(), CredentialKind::Default),
            (CT::DEFAULT, CredentialKind::Default),
        ];

        for (allowed, expected) in cases {
            assert_eq!(select_credential(allowed), expected);
        }
    }

    #[test]
    fn transfer_cap_aborts_a_fetch() {
        let (upstream, _) =
            super::super::test_support::build_upstream(&[("index.wdl", &[b'x'; 65536])]);
        let url = url::Url::from_directory_path(upstream.path())
            .unwrap()
            .to_string();
        let destination = tempfile::tempdir().unwrap();
        let repository = git2::Repository::init(destination.path()).unwrap();
        let (mut options, watch) = default_fetch_options(FetchPolicy {
            credentials: CredentialMode::Disabled,
            max_transfer_bytes: Some(1),
        });
        let mut remote = repository.remote("origin", &url).unwrap();
        let result = remote.fetch(
            &["+refs/heads/*:refs/remotes/origin/*"],
            Some(&mut options),
            None,
        );
        assert!(result.is_err());
        assert!(watch.aborted_at().is_some_and(|received| received >= 1));
    }

    #[test]
    fn transfer_cap_allows_a_small_fetch() {
        let (upstream, _) =
            super::super::test_support::build_upstream(&[("index.wdl", &[b'x'; 65536])]);
        let url = url::Url::from_directory_path(upstream.path())
            .unwrap()
            .to_string();
        let destination = tempfile::tempdir().unwrap();
        let repository = git2::Repository::init(destination.path()).unwrap();
        let (mut options, watch) = default_fetch_options(FetchPolicy {
            credentials: CredentialMode::Disabled,
            max_transfer_bytes: Some(64 * 1024 * 1024),
        });
        let mut remote = repository.remote("origin", &url).unwrap();
        let result = remote.fetch(
            &["+refs/heads/*:refs/remotes/origin/*"],
            Some(&mut options),
            None,
        );
        assert!(result.is_ok());
        assert_eq!(watch.aborted_at(), None);
    }
}
