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
    /// The blake3 hash of the trimmed source content at the diagnostic span.
    pub source_hash: String,
    /// The diagnostic message (for human readability, not used in matching).
    pub message: String,
}

impl BaselineEntry {
    /// Creates a new baseline entry by hashing the given source content.
    pub fn new(rule: &str, path: &str, source_content: &str) -> Self {
        let hash = blake3::hash(source_content.trim().as_bytes());
        Self {
            rule: rule.to_string(),
            path: path.to_string(),
            source_hash: hash.to_hex().to_string(),
            message: String::new(),
        }
    }

    /// Creates a new baseline entry with a message.
    pub fn with_message(rule: &str, path: &str, source_content: &str, message: &str) -> Self {
        let mut entry = Self::new(rule, path, source_content);
        entry.message = message.to_string();
        entry
    }

    /// Returns `true` if this entry matches the given diagnostic parameters.
    pub fn matches(&self, rule: &str, path: &str, source_content: &str) -> bool {
        let hash = blake3::hash(source_content.trim().as_bytes());
        self.rule == rule && self.path == path && self.source_hash == hash.to_hex().as_str()
    }
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
    ///
    /// Returns `Ok(None)` if the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read baseline file `{}`", path.display()))?;
        let baseline: Baseline = toml::from_str(&content)
            .with_context(|| format!("failed to parse baseline file `{}`", path.display()))?;
        Ok(Some(baseline))
    }

    /// Writes the baseline to the given file path.
    pub fn write(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self).context("failed to serialize baseline")?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write baseline file `{}`", path.display()))?;
        Ok(())
    }

    /// Returns `true` if the given diagnostic should be suppressed based on
    /// this baseline. Internally tracks which entries have been matched so
    /// that stale entries can be reported via
    /// [`stale_entries()`](Self::stale_entries).
    pub fn suppresses(
        &mut self,
        diagnostic: &wdl_ast::Diagnostic,
        path: &str,
        source: &str,
    ) -> bool {
        if let Some(rule) = diagnostic.rule()
            && let Some(label) = diagnostic.labels().next()
        {
            let span = label.span();
            let start = span.start();
            let end = span.end();
            if end <= source.len() {
                let source_slice = &source[start..end];
                let hash = blake3::hash(source_slice.trim().as_bytes());
                let hash_hex = hash.to_hex();
                for (i, entry) in self.diagnostic.iter().enumerate() {
                    if !self.matched.contains(&i)
                        && entry.rule == rule
                        && entry.path == path
                        && entry.source_hash == hash_hex.as_str()
                    {
                        self.matched.insert(i);
                        return true;
                    }
                }
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
    use wdl_ast::Diagnostic;
    use wdl_ast::Span;

    use super::*;

    /// Helper to build a diagnostic with a rule and a label spanning the given
    /// byte range in the source.
    fn make_diagnostic(rule: &str, message: &str, start: usize, len: usize) -> Diagnostic {
        Diagnostic::warning(message)
            .with_rule(rule)
            .with_label("here", Span::new(start, len))
    }

    #[test]
    fn suppresses_matching_diagnostic() {
        let source = "  runtime {}\n";
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            source,
        )]);
        let d = make_diagnostic("MissingRuntime", "missing runtime", 0, source.len());

        assert!(baseline.suppresses(&d, "tasks/align.wdl", source));
    }

    #[test]
    fn does_not_suppress_different_content() {
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime {}\n",
        )]);
        let different_source = "  runtime { docker: \"ubuntu\" }\n";
        let d = make_diagnostic(
            "MissingRuntime",
            "missing runtime",
            0,
            different_source.len(),
        );

        assert!(!baseline.suppresses(&d, "tasks/align.wdl", different_source));
    }

    #[test]
    fn does_not_suppress_different_rule() {
        let source = "  runtime {}\n";
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            source,
        )]);
        let d = make_diagnostic("MissingOutput", "missing output", 0, source.len());

        assert!(!baseline.suppresses(&d, "tasks/align.wdl", source));
    }

    #[test]
    fn does_not_suppress_different_path() {
        let source = "  runtime {}\n";
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            source,
        )]);
        let d = make_diagnostic("MissingRuntime", "missing runtime", 0, source.len());

        assert!(!baseline.suppresses(&d, "tasks/other.wdl", source));
    }

    #[test]
    fn does_not_suppress_diagnostic_without_rule() {
        let source = "  runtime {}\n";
        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            source,
        )]);
        let d = Diagnostic::warning("no rule").with_label("here", Span::new(0, source.len()));

        assert!(!baseline.suppresses(&d, "tasks/align.wdl", source));
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sprocket-baseline.toml");

        let mut baseline = Baseline::new(vec![BaselineEntry::with_message(
            "MissingRuntime",
            "tasks/align.wdl",
            "  runtime {}\n",
            "task `align` is missing a `runtime` section",
        )]);
        baseline.sort();
        baseline.write(&path).unwrap();

        let loaded = Baseline::load(&path).unwrap().unwrap();
        assert_eq!(loaded.diagnostic.len(), 1);
        assert_eq!(loaded.diagnostic[0].rule, "MissingRuntime");
    }

    #[test]
    fn load_returns_none_for_missing_file() {
        let result = Baseline::load(Path::new("/nonexistent/baseline.toml")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn stale_entries_reported_when_unmatched() {
        let source_a = "content a";
        let source_b = "content b";
        let mut baseline = Baseline::new(vec![
            BaselineEntry::new("RuleA", "a.wdl", source_a),
            BaselineEntry::new("RuleB", "b.wdl", source_b),
        ]);

        let d = make_diagnostic("RuleA", "msg", 0, source_a.len());
        baseline.suppresses(&d, "a.wdl", source_a);

        let stale = baseline.stale_entries();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].rule, "RuleB");
    }

    #[test]
    fn no_stale_entries_when_all_matched() {
        let source = "content";
        let mut baseline = Baseline::new(vec![BaselineEntry::new("RuleA", "a.wdl", source)]);

        let d = make_diagnostic("RuleA", "msg", 0, source.len());
        baseline.suppresses(&d, "a.wdl", source);

        assert!(baseline.stale_entries().is_empty());
    }

    #[test]
    fn sort_is_deterministic() {
        let mut baseline = Baseline::new(vec![
            BaselineEntry::new("RuleB", "z.wdl", "content"),
            BaselineEntry::new("RuleA", "a.wdl", "content"),
            BaselineEntry::new("RuleA", "a.wdl", "other"),
        ]);
        baseline.sort();

        assert_eq!(baseline.diagnostic[0].path, "a.wdl");
        assert_eq!(baseline.diagnostic[0].rule, "RuleA");
        assert_eq!(baseline.diagnostic[2].path, "z.wdl");
    }
}
