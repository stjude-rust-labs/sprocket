//! Diagnostic baseline support.
//!
//! A baseline file records known diagnostics so they can be suppressed from
//! output. This lets teams adopt `sprocket check` incrementally without
//! fixing every existing diagnostic first.
//!
//! To suppress diagnostics during a single check run and later report
//! entries that no longer correspond to any
//! diagnostic, borrow a [`BaselineMatcher`] via [`Baseline::matcher`]. Each
//! run should use a fresh matcher; reusing one across runs would incorrectly
//! carry match state forward, which manifests as suppressed diagnostics
//! reappearing on subsequent LSP pulls.
//!
//! Entry paths are stored relative to the baseline file's directory (or as
//! full URIs for non-`file://` sources like `https://`). At load time each
//! entry's path is resolved to a [`Url`](url::Url), which is compared
//! directly against the document's URI during matching. A [`Baseline`] loaded
//! from disk sets its base directory from the baseline file's parent;
//! callers constructing one in memory (e.g., tests, LSP integrations)
//! provide it via [`Baseline::with_base_dir`].

use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;

use arrayvec::ArrayString;
use fixedbitset::FixedBitSet;
use path_clean::PathClean;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use url::Url;

/// Errors that can occur while loading, parsing, or writing a baseline.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to read a baseline file from disk.
    #[error("failed to read baseline file `{path}`", path = path.display())]
    Read {
        /// The path that was being read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to parse a baseline file as TOML.
    #[error("failed to parse baseline file `{path}`", path = path.display())]
    Parse {
        /// The path that was being parsed.
        path: PathBuf,
        /// The underlying deserialization error.
        #[source]
        source: toml::de::Error,
    },
    /// Failed to serialize a baseline to TOML.
    #[error("failed to serialize baseline")]
    Serialize(#[from] toml::ser::Error),
    /// Failed to write a baseline file to disk.
    #[error("failed to write baseline file `{path}`", path = path.display())]
    Write {
        /// The path that was being written.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    /// Returns `true` if this error represents a missing baseline file.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::Read { source, .. } if source.kind() == std::io::ErrorKind::NotFound
        )
    }
}

/// A specialized result type for baseline operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The default baseline file name.
pub const DEFAULT_BASELINE_FILENAME: &str = "sprocket-baseline.toml";

/// A single baselined diagnostic entry.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BaselineEntry {
    /// The rule ID that produced the diagnostic.
    rule: String,
    /// The source path this entry refers to.
    ///
    /// Either a path relative to the baseline file's directory, or a full
    /// URI (e.g., `https://example.com/foo.wdl`) for non-`file://` sources.
    path: String,
    /// The hex form of the `blake3` hash of the source text at the
    /// diagnostic's primary label span.
    ///
    /// Because this is based on content rather than line numbers, the entry
    /// survives insertions or deletions elsewhere in the file. If the
    /// flagged code itself is edited, the hash changes and the entry no
    /// longer matches.
    source_hash: ArrayString<64>,
}

impl BaselineEntry {
    /// Creates a new baseline entry from a precomputed source hash.
    ///
    /// Typical callers obtain the hash via
    /// [`wdl_analysis::Document::hash_span`].
    pub fn new(
        rule: impl Into<String>,
        path: impl Into<String>,
        source_hash: ArrayString<64>,
    ) -> Self {
        Self {
            rule: rule.into(),
            path: path.into(),
            source_hash,
        }
    }

    /// Returns the rule ID that produced the baselined diagnostic.
    pub fn rule(&self) -> &str {
        &self.rule
    }

    /// Returns the path recorded for this entry.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the hex form of the `blake3` hash of the diagnostic's source
    /// text.
    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }
}

/// A diagnostic baseline loaded from a TOML file.
///
/// To suppress diagnostics during a check run and track which entries were
/// matched, borrow a [`BaselineMatcher`] via [`Baseline::matcher`].
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Baseline {
    /// The baselined diagnostic entries.
    #[serde(default)]
    diagnostic: Vec<BaselineEntry>,
    /// Precomputed lookup structures derived from `diagnostic` plus a base
    /// directory. Rebuilt on every mutation that changes either input.
    #[serde(skip)]
    index: BaselineIndex,
}

/// Lookup structures derived from a baseline's entries and base directory.
///
/// `BaselineIndex` owns the parallel-`Vec` invariants that back fast
/// matcher lookups, so the outer [`Baseline`] only sees entries + a
/// computed index.
#[derive(Clone, Debug, Default)]
struct BaselineIndex {
    /// Resolved comparison keys, one per entry in `diagnostic` (in index
    /// order). For local sources this is a `file://` URL built from the
    /// cleaned absolute path; for remote sources it is the parsed URL.
    ///
    /// `None` for entries whose path could not be resolved (e.g., the
    /// referenced file does not exist); such entries will never match and
    /// will be reported as stale.
    ///
    /// # Invariant
    ///
    /// `resolved.len()` equals the length of the backing entries vector at
    /// all times. Maintained by [`Self::rebuild`].
    resolved: Vec<Option<Url>>,
    /// Indices into the backing entries, sorted ascending by the triple
    /// `(resolved[i], entries[i].rule, entries[i].source_hash)`.
    ///
    /// Used by [`BaselineMatcher::matches_entry`] for binary search.
    sorted: Vec<usize>,
    /// The directory entries' relative paths resolve against.
    ///
    /// Set from the baseline file's parent directory by [`Baseline::load`];
    /// set explicitly by callers constructing a baseline in memory via
    /// [`Baseline::with_base_dir`].
    base_dir: Option<PathBuf>,
}

impl BaselineIndex {
    /// Recomputes `resolved` and `sorted` from the given entries.
    fn rebuild(&mut self, entries: &[BaselineEntry]) {
        self.resolved.clear();
        self.resolved.extend(
            entries
                .iter()
                .map(|entry| resolve_entry_ref(entry, self.base_dir.as_deref())),
        );
        self.sorted.clear();
        self.sorted.extend(0..entries.len());
        self.sorted.sort_by(|&a, &b| {
            self.resolved[a]
                .cmp(&self.resolved[b])
                .then_with(|| entries[a].rule.cmp(&entries[b].rule))
                .then_with(|| entries[a].source_hash.cmp(&entries[b].source_hash))
        });
    }
}

impl Baseline {
    /// Creates a new baseline with the given entries and no base directory.
    ///
    /// Entries whose `path` is a relative filesystem path will not match
    /// until a base directory is provided via [`Baseline::with_base_dir`].
    pub fn new(diagnostic: Vec<BaselineEntry>) -> Self {
        let mut baseline = Self {
            diagnostic,
            index: BaselineIndex::default(),
        };
        baseline.index.rebuild(&baseline.diagnostic);
        baseline
    }

    /// Returns a new baseline with `base_dir` set as the directory relative
    /// paths resolve against.
    pub fn with_base_dir(mut self, base_dir: PathBuf) -> Self {
        self.index.base_dir = Some(base_dir);
        self.index.rebuild(&self.diagnostic);
        self
    }

    /// Returns the baselined diagnostic entries.
    pub fn entries(&self) -> &[BaselineEntry] {
        &self.diagnostic
    }

    /// Loads a baseline from the given file path.
    ///
    /// The baseline's base directory is set to the absolute parent of
    /// `path`, so entries' relative paths resolve against that directory.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut baseline: Baseline = toml::from_str(&content).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        let abs_path = absolute(path)
            .map(|p| p.clean())
            .unwrap_or_else(|_| path.to_path_buf());
        baseline.index.base_dir = abs_path.parent().map(Path::to_path_buf);
        baseline.index.rebuild(&baseline.diagnostic);
        Ok(baseline)
    }

    /// Loads a baseline from the given file path, returning `None` when the
    /// file is missing and `required` is `false`.
    ///
    /// This exists for the common case where a default baseline path is
    /// checked opportunistically but an explicitly configured path must
    /// exist.
    pub fn load_or_default(path: &Path, required: bool) -> Result<Option<Self>> {
        match Self::load(path) {
            Ok(baseline) => Ok(Some(baseline)),
            Err(e) if !required && e.is_not_found() => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Writes the baseline to the given file path.
    pub fn write(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Sorts entries deterministically by path, then rule, then hash.
    pub fn sort(&mut self) {
        self.diagnostic.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.rule.cmp(&b.rule))
                .then_with(|| a.source_hash.cmp(&b.source_hash))
        });
        self.index.rebuild(&self.diagnostic);
    }

    /// Creates a matcher for a single check run.
    pub fn matcher(&self) -> BaselineMatcher<'_> {
        BaselineMatcher {
            baseline: self,
            matched: FixedBitSet::with_capacity(self.diagnostic.len()),
        }
    }
}

/// Resolves an entry's stored path to a [`Url`] for comparison against
/// document URIs.
///
/// Returns `None` if the entry is a relative path but no base directory is
/// set, or if the resulting path cannot be converted to a URL.
fn resolve_entry_ref(entry: &BaselineEntry, base_dir: Option<&Path>) -> Option<Url> {
    if entry.path.contains("://") {
        return Url::parse(&entry.path).ok();
    }
    let base_dir = base_dir?;
    let joined = base_dir.join(&entry.path);
    Url::from_file_path(absolute(joined).ok()?.clean()).ok()
}

/// Tracks which [`Baseline`] entries were matched during a single check run.
///
/// Create one with [`Baseline::matcher`]. Use
/// [`is_suppressed`](Self::is_suppressed) while iterating over diagnostics,
/// then consult [`stale_entries`](Self::stale_entries) once iteration is
/// complete to find entries that no longer correspond to any active diagnostic.
#[derive(Debug)]
pub struct BaselineMatcher<'a> {
    /// The immutable baseline this matcher borrows from.
    baseline: &'a Baseline,
    /// Entries matched by at least one diagnostic during the run. Indexed
    /// by position in `baseline.diagnostic`.
    matched: FixedBitSet,
}

impl<'a> BaselineMatcher<'a> {
    /// Returns `true` if the given diagnostic is suppressed by any entry in
    /// the underlying baseline.
    ///
    /// This also records which entries matched. Multiple diagnostics are
    /// allowed to match the same entry.
    pub fn is_suppressed(
        &mut self,
        diagnostic: &wdl_ast::Diagnostic,
        document: &wdl_analysis::Document,
    ) -> bool {
        if let Some(rule) = diagnostic.rule()
            && let Some(label) = diagnostic.labels().next()
            && let Some(hash) = document.hash_span(label.span())
        {
            return self.matches_entry(rule, document.uri(), hash.as_str());
        }

        false
    }

    /// Returns baseline entries that were never matched by any diagnostic
    /// during this run.
    pub fn stale_entries(&self) -> impl Iterator<Item = &'a BaselineEntry> + '_ {
        self.baseline
            .diagnostic
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| (!self.matched.contains(i)).then_some(entry))
    }

    /// Attempts to match a diagnostic against an unmarked entry by rule,
    /// resolved comparison key, and source-text hash. Returns `true` when
    /// a fresh entry was marked; returns `false` when no entry in the
    /// baseline shares the target triple or when every matching entry has
    /// already been marked by a prior call this run.
    ///
    /// Uses `partition_point` twice on the baseline's sorted index to
    /// locate the contiguous range of entries with a matching
    /// `(resolved, rule, source_hash)` triple. Marking is strictly 1:1; if
    /// the baseline has N entries sharing a triple and the run produces M
    /// runtime diagnostics with that triple, the first `min(N, M)` calls
    /// return `true` and the remaining `max(0, M - N)` calls return
    /// `false`. Any unmarked entries after the run are reported as stale
    /// by [`Self::stale_entries`].
    fn matches_entry(&mut self, rule: &str, doc_ref: &Url, hash: &str) -> bool {
        let sorted = &self.baseline.index.sorted;
        let key = |i: usize| {
            (
                self.baseline.index.resolved[i].as_ref(),
                self.baseline.diagnostic[i].rule.as_str(),
                self.baseline.diagnostic[i].source_hash.as_str(),
            )
        };
        let target = (Some(doc_ref), rule, hash);
        let start = sorted.partition_point(|&i| key(i) < target);
        let end = sorted.partition_point(|&i| key(i) <= target);
        for &i in &sorted[start..end] {
            if !self.matched.contains(i) {
                self.matched.insert(i);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_source(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path
    }

    fn doc_ref(path: &Path) -> Url {
        Url::from_file_path(absolute(path).unwrap().clean()).unwrap()
    }

    fn hash(content: &str) -> ArrayString<64> {
        blake3::hash(content.as_bytes()).to_hex()
    }

    #[test]
    fn matches_same_content() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_source(dir.path(), "tasks/align.wdl", "stub");
        let baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            hash("  runtime {}\n"),
        )])
        .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        assert!(matcher.matches_entry(
            "MissingRuntime",
            &doc_ref(&source),
            &hash("  runtime {}\n")
        ));
    }

    #[test]
    fn does_not_match_different_content() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_source(dir.path(), "tasks/align.wdl", "stub");
        let baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            hash("  runtime {}\n"),
        )])
        .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        assert!(!matcher.matches_entry(
            "MissingRuntime",
            &doc_ref(&source),
            &hash("  runtime { docker: \"ubuntu\" }\n")
        ));
    }

    #[test]
    fn does_not_match_different_rule() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_source(dir.path(), "tasks/align.wdl", "stub");
        let baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            hash("  runtime {}\n"),
        )])
        .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        assert!(!matcher.matches_entry(
            "MissingOutput",
            &doc_ref(&source),
            &hash("  runtime {}\n")
        ));
    }

    #[test]
    fn does_not_match_different_path() {
        let dir = tempfile::tempdir().unwrap();
        write_source(dir.path(), "tasks/align.wdl", "stub");
        let other = write_source(dir.path(), "tasks/other.wdl", "stub");
        let baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            hash("  runtime {}\n"),
        )])
        .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        assert!(!matcher.matches_entry(
            "MissingRuntime",
            &doc_ref(&other),
            &hash("  runtime {}\n")
        ));
    }

    #[test]
    fn single_entry_suppresses_one_diagnostic_then_stops() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_source(dir.path(), "tasks/align.wdl", "stub");
        let baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            hash("  runtime {}\n"),
        )])
        .with_base_dir(dir.path().to_path_buf());

        let uri = doc_ref(&source);
        let mut matcher = baseline.matcher();
        assert!(matcher.matches_entry("MissingRuntime", &uri, &hash("  runtime {}\n")));
        assert!(
            !matcher.matches_entry("MissingRuntime", &uri, &hash("  runtime {}\n")),
            "second `MissingRuntime` diagnostic should not be suppressed by a single entry"
        );
    }

    #[test]
    fn fresh_matcher_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_source(dir.path(), "tasks/align.wdl", "stub");
        let baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            hash("  runtime {}\n"),
        )])
        .with_base_dir(dir.path().to_path_buf());

        let uri = doc_ref(&source);
        {
            let mut matcher = baseline.matcher();
            assert!(matcher.matches_entry("MissingRuntime", &uri, &hash("  runtime {}\n")));
            assert_eq!(matcher.stale_entries().count(), 0);
        }

        let mut matcher = baseline.matcher();
        assert_eq!(matcher.stale_entries().count(), 1);
        assert!(matcher.matches_entry("MissingRuntime", &uri, &hash("  runtime {}\n")));
        assert_eq!(matcher.stale_entries().count(), 0);
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let baseline_path = dir.path().join("sprocket-baseline.toml");
        write_source(dir.path(), "tasks/align.wdl", "stub");

        let mut baseline = Baseline::new(vec![BaselineEntry::new(
            "MissingRuntime",
            "tasks/align.wdl",
            hash("  runtime {}\n"),
        )]);
        baseline.sort();
        baseline.write(&baseline_path).unwrap();

        let loaded = Baseline::load(&baseline_path).unwrap();
        assert_eq!(loaded.entries().len(), 1);
        assert_eq!(loaded.entries()[0].rule(), "MissingRuntime");
    }

    #[test]
    fn load_returns_error_for_missing_file() {
        let err = Baseline::load(Path::new("/nonexistent/baseline.toml")).unwrap_err();
        assert!(err.is_not_found());
    }

    #[test]
    fn load_or_default_returns_none_when_missing_and_not_required() {
        let result = Baseline::load_or_default(Path::new("/nonexistent/baseline.toml"), false)
            .expect("missing optional baseline should be `Ok(None)`");
        assert!(result.is_none());
    }

    #[test]
    fn load_or_default_returns_error_when_missing_and_required() {
        let err = Baseline::load_or_default(Path::new("/nonexistent/baseline.toml"), true)
            .expect_err("missing required baseline should error");
        assert!(err.is_not_found());
    }

    #[test]
    fn load_or_default_propagates_parse_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sprocket-baseline.toml");
        std::fs::write(&path, "not valid toml = = =").unwrap();

        let err = Baseline::load_or_default(&path, false)
            .expect_err("malformed TOML should error even when not required");
        assert!(!err.is_not_found());
        assert!(matches!(err, Error::Parse { .. }));
    }

    #[test]
    fn stale_entries_reported_when_unmatched() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_source(dir.path(), "a.wdl", "stub");
        write_source(dir.path(), "b.wdl", "stub");
        let baseline = Baseline::new(vec![
            BaselineEntry::new("RuleA", "a.wdl", hash("content a")),
            BaselineEntry::new("RuleB", "b.wdl", hash("content b")),
        ])
        .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        matcher.matches_entry("RuleA", &doc_ref(&a), &hash("content a"));

        let stale: Vec<_> = matcher.stale_entries().collect();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].rule(), "RuleB");
    }

    #[test]
    fn no_stale_entries_when_all_matched() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_source(dir.path(), "a.wdl", "stub");
        let baseline = Baseline::new(vec![BaselineEntry::new("RuleA", "a.wdl", hash("content"))])
            .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        matcher.matches_entry("RuleA", &doc_ref(&a), &hash("content"));

        assert_eq!(matcher.stale_entries().count(), 0);
    }

    #[test]
    fn surplus_duplicate_entries_are_stale() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_source(dir.path(), "a.wdl", "stub");
        let baseline = Baseline::new(vec![
            BaselineEntry::new("RuleA", "a.wdl", hash("content")),
            BaselineEntry::new("RuleA", "a.wdl", hash("content")),
        ])
        .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        assert!(matcher.matches_entry("RuleA", &doc_ref(&a), &hash("content")));

        let stale: Vec<_> = matcher.stale_entries().collect();
        assert_eq!(
            stale.len(),
            1,
            "one surplus `RuleA` entry should be stale; got: {count}",
            count = stale.len()
        );
        assert_eq!(stale[0].rule(), "RuleA");
    }

    #[test]
    fn surplus_diagnostics_beyond_duplicate_entries_are_not_suppressed() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_source(dir.path(), "a.wdl", "stub");
        let baseline = Baseline::new(vec![
            BaselineEntry::new("RuleA", "a.wdl", hash("content")),
            BaselineEntry::new("RuleA", "a.wdl", hash("content")),
        ])
        .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        assert!(matcher.matches_entry("RuleA", &doc_ref(&a), &hash("content")));
        assert!(matcher.matches_entry("RuleA", &doc_ref(&a), &hash("content")));
        assert!(
            !matcher.matches_entry("RuleA", &doc_ref(&a), &hash("content")),
            "third `RuleA` diagnostic should not be suppressed once both entries are marked"
        );
        let stale: Vec<_> = matcher.stale_entries().collect();
        assert!(
            stale.is_empty(),
            "both duplicate entries should be marked; got stale: {stale:?}",
        );
    }

    #[test]
    fn sort_is_deterministic() {
        let mut baseline = Baseline::new(vec![
            BaselineEntry::new("RuleB", "z.wdl", hash("content")),
            BaselineEntry::new("RuleA", "a.wdl", hash("content")),
            BaselineEntry::new("RuleA", "a.wdl", hash("other")),
        ]);
        baseline.sort();

        let entries = baseline.entries();
        assert_eq!(entries[0].path(), "a.wdl");
        assert_eq!(entries[0].rule(), "RuleA");
        assert_eq!(entries[2].path(), "z.wdl");
    }

    #[test]
    fn matches_entry_with_full_uri_path() {
        let baseline = Baseline::new(vec![BaselineEntry::new(
            "ContainerUri",
            "https://example.com/lib/foo.wdl",
            hash("task foo {}"),
        )]);

        let mut matcher = baseline.matcher();
        let uri = Url::parse("https://example.com/lib/foo.wdl").unwrap();
        assert!(matcher.matches_entry("ContainerUri", &uri, &hash("task foo {}")));
    }

    #[test]
    fn unresolvable_entry_never_matches() {
        let dir = tempfile::tempdir().unwrap();
        let baseline = Baseline::new(vec![BaselineEntry::new(
            "RuleA",
            "missing-file.wdl",
            hash("content"),
        )])
        .with_base_dir(dir.path().to_path_buf());

        let mut matcher = baseline.matcher();
        let uri = Url::parse("file:///whatever/missing-file.wdl").unwrap();
        assert!(!matcher.matches_entry("RuleA", &uri, &hash("content")));
    }
}
