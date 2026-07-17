//! Module-specific value formatting.

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
    format!(
        "Updated `{name}`{}",
        update_details(
            from_path,
            to_path,
            from_selector,
            to_selector,
            from_commit,
            to_commit
        )
    )
}

/// Formats selector, path, and commit changes.
pub(crate) fn update_details(
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

    if !details.is_empty() {
        return format!(" ({})", details.join(", "));
    }

    String::new()
}

fn selector_detail(selector: &str) -> String {
    selector.split_once(' ').map_or_else(
        || format!("`{selector}`"),
        |(kind, value)| format!("{kind} `{value}`"),
    )
}

fn short_commit(commit: &str) -> &str {
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
