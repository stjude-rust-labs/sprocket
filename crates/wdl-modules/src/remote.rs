//! Conversion of Git remote strings into URLs.

use std::path::Path;

use url::Url;

/// The URL schemes Git uses to name remote transports.
const TRANSPORT_SCHEMES: [&str; 6] = ["file", "git", "git+ssh", "http", "https", "ssh"];

/// The syntax a Git remote string is written in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitRemoteKind {
    /// An absolute URL carrying a Git transport scheme, such as
    /// `https://github.com/openwdl/wdl.git`.
    Url,
    /// Git's scp-like syntax, such as `git@github.com:openwdl/wdl.git`.
    Scp,
    /// A filesystem path, either absolute or relative.
    Path,
}

/// Reports which syntax `remote` is written in.
pub fn git_remote_kind(remote: &str) -> GitRemoteKind {
    if transport_url(remote).is_some() {
        GitRemoteKind::Url
    } else if scp_parts(remote).is_some() {
        GitRemoteKind::Scp
    } else {
        GitRemoteKind::Path
    }
}

/// Converts a Git remote into an absolute URL.
///
/// A remote that already carries a Git transport scheme is returned unchanged.
/// Git's scp-like syntax becomes an `ssh` URL, and a filesystem path becomes a
/// `file` URL. Returns `None` for a relative path that does not resolve and for
/// anything else that cannot be expressed as a URL.
///
/// An scp-like path becomes an absolute URL path whether or not it began with a
/// separator, because that is the form hosted forges expect. Git itself reads a
/// separator-less scp path as relative to the login user's home directory, so a
/// self-hosted remote of that shape needs its URL written out by hand.
///
/// # Examples
///
/// ```rust
/// # use wdl_modules::normalize_git_remote;
/// assert_eq!(
///     normalize_git_remote("git@github.com:openwdl/wdl.git").map(|url| url.to_string()),
///     Some("ssh://git@github.com/openwdl/wdl.git".to_string())
/// );
/// ```
pub fn normalize_git_remote(remote: &str) -> Option<Url> {
    let path = Path::new(remote);
    if path.is_absolute() {
        return Url::from_file_path(path).ok();
    }
    if let Some(url) = transport_url(remote) {
        return Some(url);
    }
    if let Some((host, path)) = scp_parts(remote) {
        // A server-absolute scp path already carries its leading separator, and
        // `ssh://host//path` would ask the server for `//path`.
        let path = path.strip_prefix('/').unwrap_or(path);
        return Url::parse(&format!("ssh://{host}/{path}")).ok();
    }
    Url::from_file_path(path.canonicalize().ok()?).ok()
}

/// Parses `remote` as a URL carrying one of Git's transport schemes.
///
/// A bare `host:path` remote parses as a URL whose scheme is the host, so the
/// scheme has to be checked against the transports Git understands rather than
/// trusting [`Url::parse`] alone.
fn transport_url(remote: &str) -> Option<Url> {
    let url = Url::parse(remote).ok()?;
    TRANSPORT_SCHEMES.contains(&url.scheme()).then_some(url)
}

/// Splits Git's scp-like `[user@]host:path` syntax into its host and path.
///
/// The path may be server-absolute, as in `git@host:/srv/repo.git`. A path
/// starting with `//` is rejected because that is how a URL looks once it has
/// been split on its scheme separator.
fn scp_parts(remote: &str) -> Option<(&str, &str)> {
    if starts_with_windows_drive(remote) {
        return None;
    }
    let (host, path) = remote.split_once(':')?;
    (!host.is_empty() && !host.contains(['/', '\\']) && !path.is_empty() && !path.starts_with("//"))
        .then_some((host, path))
}

/// Returns `true` when `remote` starts with a Windows drive prefix such as
/// `C:`.
fn starts_with_windows_drive(remote: &str) -> bool {
    let mut bytes = remote.bytes();
    matches!(
        (bytes.next(), bytes.next()),
        (Some(b'A'..=b'Z' | b'a'..=b'z'), Some(b':'))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(remote: &str) -> Option<String> {
        normalize_git_remote(remote).map(|url| url.to_string())
    }

    #[test]
    fn scp_syntax_becomes_an_ssh_url() {
        assert_eq!(
            normalized("git@github.com:stjudecloud/workflows.git").as_deref(),
            Some("ssh://git@github.com/stjudecloud/workflows.git")
        );
    }

    #[test]
    fn scp_syntax_without_a_user_becomes_an_ssh_url() {
        assert_eq!(
            normalized("github.com:stjudecloud/workflows.git").as_deref(),
            Some("ssh://github.com/stjudecloud/workflows.git")
        );
    }

    #[test]
    fn a_server_absolute_scp_path_keeps_one_separator() {
        assert_eq!(
            normalized("git@example.com:/srv/git/workflows.git").as_deref(),
            Some("ssh://git@example.com/srv/git/workflows.git")
        );
    }

    #[test]
    fn transport_urls_pass_through_unchanged() {
        assert_eq!(
            normalized("https://github.com/stjudecloud/workflows.git").as_deref(),
            Some("https://github.com/stjudecloud/workflows.git")
        );
    }

    #[test]
    fn absolute_paths_become_file_urls() {
        let remote = if cfg!(windows) {
            "C:\\repos\\workflows"
        } else {
            "/repos/workflows"
        };

        let normalized = normalized(remote);

        assert!(
            normalized
                .as_deref()
                .is_some_and(|url| url.starts_with("file://")),
            "expected a file URL, got {normalized:?}"
        );
    }

    #[test]
    fn relative_paths_that_do_not_exist_are_rejected() {
        assert_eq!(normalized("relative/path-that-does-not-exist"), None);
    }

    #[test]
    fn scp_syntax_does_not_swallow_a_windows_drive_path() {
        assert_eq!(git_remote_kind("C:\\repos\\workflows"), GitRemoteKind::Path);
    }

    #[test]
    fn a_bare_host_and_path_is_scp_syntax_rather_than_a_url() {
        assert_eq!(
            git_remote_kind("github.com:stjudecloud/workflows.git"),
            GitRemoteKind::Scp
        );
    }
}
