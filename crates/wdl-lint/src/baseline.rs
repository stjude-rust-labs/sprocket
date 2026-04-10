//! Diagnostic baseline support.
//!
//! A baseline file records known diagnostics so they can be suppressed from
//! output. This lets teams adopt `sprocket check` incrementally without
//! fixing every existing diagnostic first.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;

/// The default baseline file name.
pub const DEFAULT_BASELINE_FILENAME: &str = "sprocket-baseline.toml";

/// A single baselined diagnostic entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineEntry {
    /// The rule ID that produced the diagnostic.
    pub rule: String,
    /// The file path relative to the project root.
    pub path: String,
    /// A blake3 hash of the trimmed source text at the diagnostic's primary
    /// label span.
    ///
    /// Because this is based on content rather than line numbers, the entry
    /// survives insertions or deletions elsewhere in the file. If the flagged
    /// code itself is edited, the hash changes and the entry no longer
    /// matches.
    pub source_hash: String,
    /// The diagnostic message (for human readability, not used in matching).
    pub message: String,
}

impl BaselineEntry {
    /// Creates a new baseline entry by hashing the given source content.
    pub fn new(rule: &str, path: &str, source_content: &str, message: &str) -> Self {
        let hash = blake3::hash(source_content.trim().as_bytes());
        Self {
            rule: rule.to_string(),
            path: path.to_string(),
            source_hash: hash.to_hex().to_string(),
            message: message.to_string(),
        }
    }

    /// Returns `true` if this entry matches the given diagnostic parameters.
    pub fn matches(&self, rule: &str, path: &str, source_content: &str) -> bool {
        let hash = blake3::hash(source_content.trim().as_bytes());
        self.rule == rule && self.path == path && self.source_hash == hash.to_hex().as_str()
    }
}

/// Returns `true` if two paths refer to the same file.
///
/// Uses suffix matching so that a relative path like `tools/foo.wdl` matches
/// an absolute path like `/home/user/project/tools/foo.wdl`. The shorter path
/// must be a suffix of the longer path, aligned on a path separator boundary.
fn paths_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }

    let (shorter, longer) = if a.len() <= b.len() { (a, b) } else { (b, a) };

    longer.ends_with(shorter)
        && longer
            .as_bytes()
            .get(longer.len() - shorter.len() - 1)
            .is_some_and(|&c| c == b'/' || c == b'\\')
}

/// A diagnostic baseline loaded from a TOML file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Baseline {
    /// The baselined diagnostic entries.
    #[serde(default)]
    pub diagnostic: Vec<BaselineEntry>,

    /// Tracks which entries have been matched during a check run.
    #[serde(skip)]
    matched: HashSet<usize>,
}

impl Baseline {
    /// Creates a new baseline with the given entries.
    pub fn new(diagnostic: Vec<BaselineEntry>) -> Self {
        Self {
            diagnostic,
            matched: HashSet::new(),
        }
    }

    /// Loads a baseline from the given file path.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read baseline file `{}`", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse baseline file `{}`", path.display()))
    }

    /// Writes the baseline to the given file path.
    pub fn write(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self).context("failed to serialize baseline")?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write baseline file `{}`", path.display()))
    }

    /// Returns `true` if the given diagnostic should be suppressed based on
    /// this baseline. Internally tracks which entries have been matched so
    /// that stale entries can be reported via
    /// [`stale_entries()`](Self::stale_entries).
    pub fn suppresses(
        &mut self,
        diagnostic: &wdl_ast::Diagnostic,
        document: &wdl_analysis::Document,
    ) -> bool {
        if let Some(rule) = diagnostic.rule()
            && let Some(label) = diagnostic.labels().next()
        {
            let span = label.span();
            let root = document.root();
            let syntax = wdl_ast::AstNode::inner(&root);
            let span_text = syntax
                .text()
                .slice(rowan::TextRange::new(
                    rowan::TextSize::from(span.start() as u32),
                    rowan::TextSize::from(span.end() as u32),
                ))
                .to_string();
            return self.matches_entry(rule, &document.path(), &span_text);
        }

        false
    }

    /// Attempts to match a diagnostic against a baseline entry by rule, path,
    /// and the source text at the span. Returns `true` if a match was found.
    ///
    /// Path matching is suffix-based so that a baseline generated with
    /// relative paths (e.g., `tools/foo.wdl`) still matches when the
    /// document path is absolute (e.g., `/home/user/project/tools/foo.wdl`)
    /// or vice versa.
    fn matches_entry(&mut self, rule: &str, path: &str, span_text: &str) -> bool {
        let hash = blake3::hash(span_text.trim().as_bytes());
        let hash_hex = hash.to_hex();
        for (i, entry) in self.diagnostic.iter().enumerate() {
            if !self.matched.contains(&i)
                && entry.rule == rule
                && paths_match(&entry.path, path)
                && entry.source_hash == hash_hex.as_str()
            {
                self.matched.insert(i);
                return true;
            }
        }
        false
    }

    /// Returns baseline entries that were never matched by any diagnostic
    /// during the current run. These represent diagnostics that have been
    /// fixed and should be removed from the baseline.
    pub fn stale_entries(&self) -> Vec<&BaselineEntry> {
        self.diagnostic
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                if self.matched.contains(&i) {
                    None
                } else {
                    Some(entry)
                }
            })
            .collect()
    }

    /// Sorts entries deterministically by path, then rule, then hash.
    pub fn sort(&mut self) {
        self.diagnostic.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.rule.cmp(&b.rule))
                .then_with(|| a.source_hash.cmp(&b.source_hash))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_same_content() {
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime {}\n",
            "missing runtime",
        )]);

        assert!(baseline.matches_entry("MissingRuntime", "tasks/align.wdl", "  runtime {}\n"));
    }

    #[test]
    fn does_not_match_different_content() {
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime {}\n",
            "missing runtime",
        )]);

        assert!(!baseline.matches_entry(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime { docker: \"ubuntu\" }\n"
        ));
    }

    #[test]
    fn does_not_match_different_rule() {
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime {}\n",
            "missing runtime",
        )]);

        assert!(!baseline.matches_entry("MissingOutput", "tasks/align.wdl", "  runtime {}\n"));
    }

    #[test]
    fn does_not_match_different_path() {
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime {}\n",
            "missing runtime",
        )]);

        assert!(!baseline.matches_entry("MissingRuntime", "tasks/other.wdl", "  runtime {}\n"));
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sprocket-baseline.toml");

        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime {}\n",
            "task `align` is missing a `runtime` section",
        )]);
        baseline.sort();
        baseline.write(&path).unwrap();

        let loaded = Baseline::load(&path).unwrap();
        assert_eq!(loaded.diagnostic.len(), 1);
        assert_eq!(loaded.diagnostic[0].rule, "MissingRuntime");
    }

    #[test]
    fn load_returns_error_for_missing_file() {
        let err = Baseline::load(Path::new("/nonexistent/baseline.toml")).unwrap_err();
        let io_err = err.downcast_ref::<std::io::Error>().unwrap();
        assert_eq!(io_err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn stale_entries_reported_when_unmatched() {
        let mut baseline = Baseline::new(vec![
            BaselineEntry::new("RuleA", "a.wdl", "content a", "msg a"),
            BaselineEntry::new("RuleB", "b.wdl", "content b", "msg b"),
        ]);

        baseline.matches_entry("RuleA", "a.wdl", "content a");

        let stale = baseline.stale_entries();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].rule, "RuleB");
    }

    #[test]
    fn no_stale_entries_when_all_matched() {
        let mut baseline =
            Baseline::new(vec![BaselineEntry::new("RuleA", "a.wdl", "content", "msg")]);

        baseline.matches_entry("RuleA", "a.wdl", "content");

        assert!(baseline.stale_entries().is_empty());
    }

    #[test]
    fn sort_is_deterministic() {
        let mut baseline = Baseline::new(vec![
            BaselineEntry::new("RuleB", "z.wdl", "content", "msg"),
            BaselineEntry::new("RuleA", "a.wdl", "content", "msg"),
            BaselineEntry::new("RuleA", "a.wdl", "other", "msg"),
        ]);
        baseline.sort();

        assert_eq!(baseline.diagnostic[0].path, "a.wdl");
        assert_eq!(baseline.diagnostic[0].rule, "RuleA");
        assert_eq!(baseline.diagnostic[2].path, "z.wdl");
    }

    #[test]
    fn paths_match_exact() {
        assert!(paths_match("tools/foo.wdl", "tools/foo.wdl"));
    }

    #[test]
    fn paths_match_relative_to_absolute() {
        assert!(paths_match(
            "tools/foo.wdl",
            "/home/user/project/tools/foo.wdl"
        ));
    }

    #[test]
    fn paths_match_absolute_to_relative() {
        assert!(paths_match(
            "/home/user/project/tools/foo.wdl",
            "tools/foo.wdl"
        ));
    }

    #[test]
    fn paths_do_not_match_partial_filename() {
        assert!(!paths_match("foo.wdl", "notfoo.wdl"));
    }

    #[test]
    fn paths_do_not_match_different_files() {
        assert!(!paths_match("tools/foo.wdl", "tools/bar.wdl"));
    }

    #[test]
    fn matches_entry_with_absolute_path() {
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime {}\n",
            "missing runtime",
        )]);

        assert!(baseline.matches_entry(
            "MissingRuntime",
            "/home/user/project/tasks/align.wdl",
            "  runtime {}\n"
        ));
    }
}
