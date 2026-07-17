//! Module-specific value formatting.

use wdl_modules::dependency::GitSelector;
use wdl_modules::lockfile::ResolvedSource;
use wdl_modules::resolver::DependencyUpdate;

/// Formats a dependency update for an action line.
#[cfg(test)]
fn update_message(
    name: &impl std::fmt::Display,
    from_path: Option<&str>,
    to_path: Option<&str>,
    from_selector: Option<&str>,
    to_selector: Option<&str>,
    from_commit: Option<&str>,
    to_commit: Option<&str>,
) -> String {
    let details = update_details(
        from_path,
        to_path,
        from_selector,
        to_selector,
        from_commit,
        to_commit,
    );
    if details.is_empty() {
        format!("Updated `{name}`")
    } else {
        format!("Updated `{name}` ({details})")
    }
}

/// Formats a Git selector for user-facing output.
pub(crate) fn git_selector(selector: &GitSelector) -> String {
    selector_detail(&selector.to_string())
}

/// Formats a resolved dependency source for tree and table output.
pub(crate) fn resolved_source(source: &ResolvedSource) -> String {
    match source {
        ResolvedSource::Git {
            git,
            sha,
            path,
            selector,
        } => {
            let mut parts = vec![
                format!("source: {git}"),
                format!(
                    "selector: {} @{}",
                    git_selector(selector),
                    short_commit(sha.as_str())
                ),
            ];
            if let Some(path) = path {
                parts.push(format!("path: {path}"));
            }
            format!("({})", parts.join(", "))
        }
        ResolvedSource::Path { path } => format!("(source: {})", path.display()),
    }
}

/// Formats a dependency lockfile update without surrounding delimiters.
pub(crate) fn dependency_update(change: &DependencyUpdate) -> String {
    update_details(
        change.from_path.as_deref(),
        change.to_path.as_deref(),
        change.from_selector.as_deref(),
        change.to_selector.as_deref(),
        change.from_commit.as_deref(),
        change.to_commit.as_deref(),
    )
}

/// Formats a version constraint as a release label.
pub(crate) fn version_constraint(requirement: &str) -> String {
    let version = requirement
        .trim()
        .trim_start_matches(['^', '=', '~', '>', '<'])
        .trim_start_matches('=');
    format!("v{version}")
}

fn update_details(
    from_path: Option<&str>,
    to_path: Option<&str>,
    from_selector: Option<&str>,
    to_selector: Option<&str>,
    from_commit: Option<&str>,
    to_commit: Option<&str>,
) -> String {
    let mut details = Vec::new();

    match (from_selector, to_selector) {
        (Some(from), Some(to)) if from == to => {
            details.push(format!("selector: {}", selector_detail(from)));
        }
        (Some(from), Some(to)) => {
            details.push(format!(
                "selector: {} -> {}",
                selector_detail(from),
                selector_detail(to)
            ));
        }
        _ => {}
    }

    match (from_path, to_path) {
        (None, None) => {}
        (from, to) if from == to => {
            details.push(format!("path: `{}`", from.unwrap_or("/")));
        }
        (from, to) => {
            details.push(format!(
                "path: `{}` -> `{}`",
                from.unwrap_or("/"),
                to.unwrap_or("/")
            ));
        }
    }

    if let (Some(from_commit), Some(to_commit)) = (from_commit, to_commit)
        && from_commit != to_commit
    {
        details.push(format!(
            "commit: `{}` -> `{}`",
            short_commit(from_commit),
            short_commit(to_commit)
        ));
    }

    details.join(", ")
}

fn selector_detail(selector: &str) -> String {
    selector.split_once(' ').map_or_else(
        || format!("`{selector}`"),
        |(kind, value)| format!("{kind} `{value}`"),
    )
}

pub(crate) fn short_commit(commit: &str) -> &str {
    &commit[..7.min(commit.len())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_message_describes_path_changes_clearly() {
        assert_eq!(
            update_message(
                &"ww-bwa",
                Some("modules/ww-bwa"),
                Some("modules/ww-gatk"),
                Some("branch main"),
                Some("branch main"),
                None,
                None
            ),
            "Updated `ww-bwa` (selector: branch `main`, path: `modules/ww-bwa` -> \
             `modules/ww-gatk`)"
        );
        assert_eq!(
            update_message(
                &"ww-bwa",
                None,
                None,
                Some("version ^1"),
                Some("version ^2"),
                None,
                None
            ),
            "Updated `ww-bwa` (selector: version `^1` -> version `^2`)"
        );
        assert_eq!(
            update_message(
                &"ww-bwa",
                Some("modules/ww-bwa"),
                Some("modules/ww-bwa"),
                Some("version ^1"),
                Some("version ^2"),
                None,
                None
            ),
            "Updated `ww-bwa` (selector: version `^1` -> version `^2`, path: `modules/ww-bwa`)"
        );
        assert_eq!(
            update_message(
                &"ww-bwa",
                Some("modules/ww-bwa"),
                Some("modules/ww-bwa"),
                Some("branch main"),
                Some("branch main"),
                Some("a5805f5f2a1cbe64d28365424870d585f883bd0f"),
                Some("8797145982ba1b1b7adb5ea716c03a7e4e9dd412")
            ),
            "Updated `ww-bwa` (selector: branch `main`, path: `modules/ww-bwa`, commit: `a5805f5` \
             -> `8797145`)"
        );
    }
}
