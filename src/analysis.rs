//! Facilities for performing a typical analysis using the `wdl-*` crates.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Error;
use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::files::SimpleFiles;
use futures::future::BoxFuture;
use nonempty::NonEmpty;
use tracing::info;
use wdl::analysis::Analyzer;
use wdl::analysis::DiagnosticsConfig;
use wdl::analysis::ProgressKind;
use wdl::analysis::Validator;
use wdl::analysis::config::FeatureFlags;
use wdl::analysis::find_nearest_rule;
use wdl::ast::SupportedVersion;
use wdl::lint::Linter;

mod results;
mod source;

pub use results::AnalysisResults;
pub use source::*;
use wdl::diagnostics::Mode;
use wdl::diagnostics::get_diagnostics_display_config;
use wdl::lint::Rule;
use wdl::lint::TagSet;

use crate::IGNORE_FILENAME;

/// The type of the initialization callback.
type InitCb = Box<dyn Fn() + Send + 'static>;

/// The type of the progress callback.
type ProgressCb =
    Box<dyn Fn(ProgressKind, usize, usize) -> BoxFuture<'static, ()> + Send + Sync + 'static>;

/// An analysis.
// For some reason, `missing_debug_implementations` fires for this even though a Debug impl is not
// derivable for this type.
#[expect(missing_debug_implementations)]
pub struct Analysis {
    /// The set of root nodes to analyze.
    ///
    /// Can be files, directories, or URLs.
    sources: Vec<Source>,

    /// A list of rules to except.
    exceptions: HashSet<String>,

    /// Which lint rules to enable, as specified via a [`TagSet`].
    enabled_lint_tags: TagSet,

    /// The lint rule configuration.
    lint_config: wdl::lint::Config,

    /// Basename for any ignorefiles which should be respected.
    ignore_filename: Option<String>,

    /// Feature flags for experimental features.
    feature_flags: FeatureFlags,

    /// The fallback version to use when a WDL document declares an
    /// unrecognized version.
    fallback_version: Option<SupportedVersion>,

    /// The `[modules]` config for constructing a resolver when a
    /// `module.json` is found near the sources.
    modules_config: Option<wdl_modules::resolver::ModulesConfig>,

    /// The initialization callback.
    init: InitCb,

    /// The progress callback.
    progress: ProgressCb,
}

impl Analysis {
    /// Adds a source to the analysis.
    pub fn add_source(mut self, source: Source) -> Self {
        self.sources.push(source);
        self
    }

    /// Adds multiple sources to the analysis.
    pub fn extend_sources(mut self, source: impl IntoIterator<Item = Source>) -> Self {
        self.sources.extend(source);
        self
    }

    /// Adds multiple rules to the excepted rules list.
    pub fn extend_exceptions(mut self, rules: impl IntoIterator<Item = String>) -> Self {
        self.exceptions.extend(rules);
        self
    }

    /// Sets the initialization callback.
    pub fn init<F>(mut self, init: F) -> Self
    where
        F: Fn() + Send + 'static,
    {
        self.init = Box::new(init);
        self
    }

    /// Sets the progress callback.
    pub fn progress<F>(mut self, progress: F) -> Self
    where
        F: Fn(ProgressKind, usize, usize) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        self.progress = Box::new(progress);
        self
    }

    /// Sets the enabled lint tags.
    pub fn enabled_lint_tags(mut self, tags: TagSet) -> Self {
        self.enabled_lint_tags = tags;
        self
    }

    /// Sets the fallback version to use when a WDL document declares an
    /// unrecognized version.
    pub fn fallback_version(mut self, version: Option<SupportedVersion>) -> Self {
        self.fallback_version = version;
        self
    }

    /// Sets the feature flags.
    pub fn feature_flags(mut self, flags: FeatureFlags) -> Self {
        self.feature_flags = flags;
        self
    }

    /// Sets the `[modules]` configuration.
    pub fn modules_config(mut self, config: wdl_modules::resolver::ModulesConfig) -> Self {
        self.modules_config = Some(config);
        self
    }

    /// Determines all local directories to search for a `module.json`.
    fn module_search_dirs(&self) -> Vec<PathBuf> {
        self.sources
            .iter()
            .filter_map(|source| match source {
                Source::Directory(path) => Some(path.clone()),
                Source::File(url) => url
                    .to_file_path()
                    .ok()
                    .and_then(|path| path.parent().map(Path::to_path_buf)),
                Source::Url(_) => None,
            })
            .collect()
    }

    /// Checks the source directories and their ancestors for a `module.json`
    /// and builds a [`ResolutionContext`](wdl::analysis::ResolutionContext) if
    /// one is found and `modules_config` is set.
    ///
    /// Returns the default null-resolver context when modules are disabled or
    /// no manifest governs the sources, and an error if a `module.json` is
    /// present but malformed or if the sources span more than one manifest.
    fn resolution_context_from_sources(&self) -> anyhow::Result<wdl::analysis::ResolutionContext> {
        let Some(ref modules_config) = self.modules_config else {
            return Ok(wdl::analysis::ResolutionContext::default());
        };

        let starts = self.module_search_dirs();
        if starts.is_empty() {
            return Ok(wdl::analysis::ResolutionContext::default());
        }

        resolution_context_from_paths(modules_config, &self.feature_flags, &starts)
    }

    /// Runs the analysis and returns all results (if any exist).
    pub async fn run(
        self,
        report_mode: Mode,
        colorize: bool,
    ) -> std::result::Result<AnalysisResults, NonEmpty<Arc<Error>>> {
        warn_unknown_rules(&self.exceptions, report_mode, colorize);
        if self.enabled_lint_tags.count() > 0 && tracing::enabled!(tracing::Level::INFO) {
            let mut enabled_rules = vec![];
            let mut disabled_rules = vec![];
            for rule in wdl::lint::rules(&wdl::lint::Config::default()) {
                if is_rule_enabled(&self.enabled_lint_tags, &self.exceptions, rule.as_ref()) {
                    enabled_rules.push(rule.id());
                } else {
                    disabled_rules.push(rule.id());
                }
            }
            info!("enabled lint rules: {:?}", enabled_rules);
            info!("disabled lint rules: {:?}", disabled_rules);
        }
        let resolution = self
            .resolution_context_from_sources()
            .map_err(|e| NonEmpty::new(Arc::new(e)))?;

        let config = wdl::analysis::Config::default()
            .with_fallback_version(self.fallback_version)
            .with_diagnostics_config(get_diagnostics_config(&self.exceptions))
            .with_ignore_filename(self.ignore_filename)
            .with_feature_flags(self.feature_flags);

        (self.init)();

        let validator = Box::new(move || {
            let mut validator = Validator::default();

            if self.enabled_lint_tags.count() > 0 {
                let visitor =
                    get_lint_visitor(&self.enabled_lint_tags, &self.exceptions, &self.lint_config);
                validator.add_visitor(visitor);
            } else {
                // So the validator is always *aware* of `wdl-lint` rules, even when the linter
                // isn't added. Keeps `KnownRules` from firing unnecessarily.
                validator.extend_known_rules(wdl::lint::ALL_RULE_IDS.iter().cloned());
            }

            validator
        });

        let mut analyzer = Analyzer::new_with_validator_and_resolution(
            config,
            resolution,
            move |_, kind, count, total| (self.progress)(kind, count, total),
            validator,
        );

        for source in self.sources {
            if let Err(error) = source.register(&mut analyzer).await {
                return Err(NonEmpty::new(Arc::new(error)));
            }
        }

        let results = analyzer
            .analyze(())
            .await
            .map_err(|error| NonEmpty::new(Arc::new(error)))?;

        AnalysisResults::try_new(results)
    }
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            sources: Default::default(),
            exceptions: Default::default(),
            enabled_lint_tags: TagSet::EMPTY,
            lint_config: Default::default(),
            ignore_filename: Some(IGNORE_FILENAME.to_string()),
            feature_flags: FeatureFlags::default(),
            fallback_version: None,
            modules_config: None,
            init: Box::new(|| {}),
            progress: Box::new(|_, _, _| Box::pin(async {})),
        }
    }
}

/// Returns the default cache root, anchored to `manifest_dir` when present.
///
/// Precedence: `manifest_dir/.sprocket/cache/modules` →
/// `config_root/cache/modules` → `dirs::cache_dir()/sprocket/modules` →
/// `.sprocket/cache/modules` (CWD-relative last resort).
pub(crate) fn default_cache_root(manifest_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = manifest_dir {
        return dir.join(".sprocket").join("cache").join("modules");
    }
    if let Some(root) = crate::config::config_root() {
        return root.join("cache").join("modules");
    }
    if let Some(cache) = dirs::cache_dir() {
        return cache.join("sprocket").join("modules");
    }
    PathBuf::from(".sprocket/cache/modules")
}

/// Returns the default trust-store path, anchored to `manifest_dir` when
/// present.
///
/// Precedence: `manifest_dir/.sprocket/modules-trust.toml` →
/// `config_root/modules-trust.toml` →
/// `dirs::config_dir()/sprocket/modules-trust.toml` →
/// `modules-trust.toml` (CWD-relative last resort).
pub(crate) fn default_trust_path(manifest_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = manifest_dir {
        return dir.join(".sprocket").join("modules-trust.toml");
    }
    if let Some(root) = crate::config::config_root() {
        return root.join("modules-trust.toml");
    }
    if let Some(cfg) = dirs::config_dir() {
        return cfg.join("sprocket").join("modules-trust.toml");
    }
    PathBuf::from("modules-trust.toml")
}

/// Discovers a `module.json` in the given directory.
///
/// Returns `Ok(None)` if no manifest file exists in `cwd`. Returns the
/// manifest path and the parsed [`wdl_modules::Manifest`] on success, or an
/// error if the file exists but cannot be read or parsed.
pub(crate) fn discover_manifest(
    cwd: &Path,
) -> anyhow::Result<Option<(PathBuf, wdl_modules::Manifest)>> {
    use anyhow::Context as _;

    let manifest_path = cwd.join(wdl_modules::MANIFEST_FILENAME);

    if !manifest_path.exists() {
        return Ok(None);
    }

    let bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("reading `{}`", manifest_path.display()))?;
    let manifest = wdl_modules::Manifest::parse(&bytes)
        .with_context(|| format!("parsing `{}`", manifest_path.display()))?;

    Ok(Some((manifest_path, manifest)))
}

/// Discovers a `module.json` at `start` or an ancestor directory, walking
/// upward but never past the enclosing repository root.
///
/// Returns the first manifest found, or `Ok(None)` if none exists at or below
/// the enclosing repository root. The walk stops after examining the first
/// directory that contains a `.git` entry so discovery never reaches an
/// unrelated `module.json` outside the project. Returns an error if a manifest
/// exists but cannot be read or parsed.
pub(crate) fn discover_manifest_upward(
    start: &Path,
) -> anyhow::Result<Option<(PathBuf, wdl_modules::Manifest)>> {
    for dir in start.ancestors() {
        if let Some(found) = discover_manifest(dir)? {
            return Ok(Some(found));
        }

        if dir.join(".git").exists() {
            break;
        }
    }

    Ok(None)
}

/// Constructs a [`GitResolver`](wdl_modules::resolver::GitResolver) from the
/// given `[modules]` config and a discovered `module.json` path.
///
/// The manifest is assumed to exist; discovery establishes that before this is
/// called. Returns an error if the lockfile or trust store exists but cannot be
/// read or parsed.
pub fn build_resolver(
    modules_config: &wdl_modules::resolver::ModulesConfig,
    manifest_path: &std::path::Path,
) -> anyhow::Result<Arc<dyn wdl_modules::Resolver>> {
    use anyhow::Context as _;

    let lockfile_path = manifest_path.with_file_name(wdl_modules::LOCKFILE_FILENAME);
    let lockfile = if lockfile_path.exists() {
        let lock_bytes = std::fs::read(&lockfile_path)
            .with_context(|| format!("reading `{}`", lockfile_path.display()))?;
        wdl_modules::Lockfile::parse(&lock_bytes)
            .with_context(|| format!("parsing `{}`", lockfile_path.display()))?
    } else {
        wdl_modules::Lockfile::default()
    };

    let manifest_dir = manifest_path.parent();

    let cache_root = modules_config
        .cache_path
        .clone()
        .unwrap_or_else(|| default_cache_root(manifest_dir));

    let trust_path = default_trust_path(manifest_dir);

    let trust = wdl_modules::resolver::TrustStore::load_or_default(&trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;

    let resolver = wdl_modules::resolver::GitResolver::builder()
        .cache_root(cache_root)
        .trust(trust)
        .lockfile(lockfile)
        .policy(
            wdl_modules::resolver::ResolverPolicy::try_from(modules_config)?.without_credentials(),
        )
        .build();

    Ok(Arc::new(resolver))
}

/// Builds a [`ResolutionContext`](wdl::analysis::ResolutionContext) for a
/// discovered `module.json` and its already-parsed manifest, or the default
/// null-resolver context when no resolver can be built for it.
///
/// This is the single place that turns a manifest plus `[modules]` config into
/// a resolution context, so the CLI batch analysis and the LSP server stay
/// consistent in how a resolver and manifest become an analyzer input. The
/// caller passes the manifest it already parsed during discovery so the
/// consumer module is built without a second read of `module.json`.
pub(crate) fn resolution_context_for_manifest(
    modules_config: &wdl_modules::resolver::ModulesConfig,
    manifest_path: &Path,
    manifest: wdl_modules::Manifest,
) -> anyhow::Result<wdl::analysis::ResolutionContext> {
    info!(
        manifest = %manifest_path.display(),
        "found `module.json`; symbolic imports will resolve through the module system"
    );
    let resolver = build_resolver(modules_config, manifest_path)?;
    let root = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let module = wdl_modules::module::Module::new(Arc::new(manifest), root);
    Ok(wdl::analysis::ResolutionContext::enabled(resolver, module))
}

/// Discovers the `module.json` governing `starts` and builds a
/// [`ResolutionContext`](wdl::analysis::ResolutionContext) for it.
///
/// This is the single discovery policy shared by the CLI batch analysis and the
/// LSP server, including the gate on the WDL 1.4 feature flag. Each path in
/// `starts` is walked upward (stopping at a repository root) for a
/// `module.json`. The default null-resolver context is returned when the
/// feature is disabled or no manifest is found; an error is returned when a
/// discovered manifest is malformed or when `starts` spans more than one
/// manifest.
pub(crate) fn resolution_context_from_paths(
    modules_config: &wdl_modules::resolver::ModulesConfig,
    feature_flags: &FeatureFlags,
    starts: &[PathBuf],
) -> anyhow::Result<wdl::analysis::ResolutionContext> {
    if !feature_flags.wdl_1_4() {
        return Ok(wdl::analysis::ResolutionContext::default());
    }

    let mut manifests = HashMap::new();
    let mut walked = HashSet::new();
    for start in starts {
        // Skip a start whose directory was already walked; many source paths
        // commonly share the same parent directory, and the upward walk from a
        // directory is deterministic, so re-walking it cannot find anything new.
        if !walked.insert(start.clone()) {
            continue;
        }
        if let Some((path, manifest)) = discover_manifest_upward(start)? {
            manifests.entry(path).or_insert(manifest);
        }
    }

    if manifests.is_empty() {
        return Ok(wdl::analysis::ResolutionContext::default());
    }

    anyhow::ensure!(
        manifests.len() == 1,
        "local sources with symbolic import resolution enabled must be governed by a single \
         `module.json`"
    );

    // SAFETY: the `is_empty` check above returned early, and the `ensure` above
    // verified that exactly one manifest remains.
    let (manifest_path, manifest) = manifests.into_iter().next().unwrap();

    resolution_context_for_manifest(modules_config, &manifest_path, manifest)
}

/// Warns about any unknown rules.
fn warn_unknown_rules(exceptions: &HashSet<String>, report_mode: Mode, colorize: bool) {
    let known_rules = wdl::analysis::ALL_RULE_IDS
        .iter()
        .chain(wdl::lint::ALL_RULE_IDS.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let mut unknown = exceptions
        .iter()
        .filter(|rule| {
            !known_rules
                .iter()
                .any(|name| name.eq_ignore_ascii_case(rule))
        })
        .map(|rule| {
            (
                rule,
                find_nearest_rule(known_rules.iter().map(String::as_str), rule),
            )
        })
        .collect::<Vec<_>>();

    if unknown.is_empty() {
        return;
    }

    unknown.sort();

    let (config, writer) = get_diagnostics_display_config(report_mode, colorize);
    let mut writer = writer.lock();
    let files = SimpleFiles::<String, String>::new();

    for (unknown_rule, nearest_rule) in unknown {
        let mut notes = Vec::new();

        if let Some(nearest_rule) = nearest_rule {
            notes.push(format!("fix: did you mean the `{nearest_rule}` rule?"));
        }

        notes.push(String::from(
            "run `sprocket explain --help` to see available rules",
        ));

        let warning = Diagnostic::warning()
            .with_message(format!(
                "ignoring unknown rule provided via --except: {unknown_rule}",
            ))
            .with_notes(notes);

        codespan_reporting::term::emit_to_write_style(&mut writer, config, &files, &warning)
            .expect("failed to emit unknown rule warning");
    }
}

/// Gets the rules as a diagnostics configuration with the excepted rules
/// removed.
fn get_diagnostics_config(exceptions: &HashSet<String>) -> DiagnosticsConfig {
    DiagnosticsConfig::new(wdl::analysis::rules().into_iter().filter(|rule| {
        !exceptions
            .iter()
            .any(|exception| exception.eq_ignore_ascii_case(rule.id()))
    }))
}

/// Determines if a rule should be enabled.
fn is_rule_enabled(
    enabled_lint_tags: &TagSet,
    exceptions: &HashSet<String>,
    rule: &dyn Rule,
) -> bool {
    if exceptions
        .iter()
        .any(|exception| exception.eq_ignore_ascii_case(rule.id()))
    {
        return false;
    }

    enabled_lint_tags.intersect(rule.tags()) == rule.tags()
}

/// Gets a lint visitor with the rules depending on provided options.
///
/// `enabled_lint_tags` controls which rules are considered for being added to
/// the visitor. `disabled_lint_tags` and `exceptions` act as filters on the set
/// considered by `enabled_lint_tags`.
fn get_lint_visitor(
    enabled_lint_tags: &TagSet,
    exceptions: &HashSet<String>,
    lint_config: &wdl::lint::Config,
) -> Linter {
    Linter::new(
        wdl::lint::rules(lint_config)
            .into_iter()
            .filter_map(|rule| {
                is_rule_enabled(enabled_lint_tags, exceptions, rule.as_ref())
                    .then_some(rule as Box<dyn Rule>)
            }),
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Analysis;
    use super::Source;
    use super::default_cache_root;
    use super::default_trust_path;
    use super::discover_manifest_upward;
    use super::resolution_context_from_paths;

    /// Minimal valid `module.json` contents for discovery tests.
    const MANIFEST: &[u8] = br#"{"name":"example","version":"0.1.0","license":"MIT"}"#;

    #[test]
    fn cache_root_uses_manifest_dir() {
        let dir = Path::new("/some/project");
        let result = default_cache_root(Some(dir));
        assert_eq!(
            result,
            Path::new("/some/project/.sprocket/cache/modules"),
            "`default_cache_root` with `Some(manifest_dir)` should be manifest-anchored"
        );
    }

    #[test]
    fn trust_path_uses_manifest_dir() {
        let dir = Path::new("/some/project");
        let result = default_trust_path(Some(dir));
        assert_eq!(
            result,
            Path::new("/some/project/.sprocket/modules-trust.toml"),
            "`default_trust_path` with `Some(manifest_dir)` should be manifest-anchored"
        );
    }

    #[test]
    fn cache_root_none_falls_back_to_os_or_string() {
        let result = default_cache_root(None);
        // The path always ends with `cache/modules` regardless of which
        // fallback branch was taken.
        assert!(
            result.ends_with("cache/modules"),
            "`default_cache_root(None)` should end with `cache/modules`, got `{}`",
            result.display()
        );
        // When any OS/config dir is available the path must be absolute.
        if crate::config::config_root().is_some() || dirs::cache_dir().is_some() {
            assert!(
                result.is_absolute(),
                "`default_cache_root(None)` should be absolute, got `{}`",
                result.display()
            );
        }
    }

    #[test]
    fn trust_path_none_falls_back_to_os_or_string() {
        let result = default_trust_path(None);
        let file_name = result.file_name().map(|n| n.to_string_lossy().into_owned());
        assert_eq!(
            file_name.as_deref(),
            Some("modules-trust.toml"),
            "`default_trust_path(None)` should always end with `modules-trust.toml`, got `{}`",
            result.display()
        );
        // Must not be CWD-relative when an OS config dir is available.
        if dirs::config_dir().is_some() {
            assert!(
                result.is_absolute(),
                "`default_trust_path(None)` should be absolute when `dirs::config_dir()` is \
                 available, got `{}`",
                result.display()
            );
        }
    }

    #[test]
    fn discover_manifest_upward_finds_ancestor() {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::write(root.path().join(wdl_modules::MANIFEST_FILENAME), MANIFEST).unwrap();
        let nested = root.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();

        let found = discover_manifest_upward(&nested)
            .unwrap()
            .expect("`discover_manifest_upward` should find a `module.json` in an ancestor");
        assert_eq!(
            found.0,
            root.path().join(wdl_modules::MANIFEST_FILENAME),
            "`discover_manifest_upward` should return the ancestor `module.json` path"
        );
    }

    #[test]
    fn discover_manifest_upward_stops_at_git_root() {
        let outer = tempfile::TempDir::new().unwrap();
        // A `module.json` above the repository root must not be discovered.
        std::fs::write(outer.path().join(wdl_modules::MANIFEST_FILENAME), MANIFEST).unwrap();
        let repo = outer.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let nested = repo.join("src");
        std::fs::create_dir_all(&nested).unwrap();

        assert!(
            discover_manifest_upward(&nested).unwrap().is_none(),
            "the walk should stop at the `.git` repository root and ignore an ancestor \
             `module.json`"
        );
    }

    #[test]
    fn module_search_dirs_use_file_parent() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("main.wdl");
        std::fs::write(&file, "version 1.2\n").unwrap();
        let url = url::Url::from_file_path(&file).unwrap();

        let analysis = Analysis::default().add_source(Source::File(url));
        assert_eq!(
            analysis.module_search_dirs(),
            vec![dir.path().to_path_buf()],
            "`module_search_dirs` should return the parent directory of a file source"
        );
    }

    #[test]
    fn resolver_allows_mixed_module_and_nonmodule_sources() {
        let dir = tempfile::TempDir::new().unwrap();
        let module_dir = dir.path().join("module");
        let plain_dir = dir.path().join("plain");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::create_dir_all(&plain_dir).unwrap();
        std::fs::write(module_dir.join(wdl_modules::MANIFEST_FILENAME), MANIFEST).unwrap();
        std::fs::write(module_dir.join("main.wdl"), "version 1.4\n").unwrap();
        std::fs::write(plain_dir.join("main.wdl"), "version 1.4\n").unwrap();

        let module_url = url::Url::from_file_path(module_dir.join("main.wdl")).unwrap();
        let plain_url = url::Url::from_file_path(plain_dir.join("main.wdl")).unwrap();
        let analysis = Analysis::default()
            .add_source(Source::File(module_url))
            .add_source(Source::File(plain_url))
            .feature_flags(super::FeatureFlags::default().with_wdl_1_4())
            .modules_config(wdl_modules::resolver::ModulesConfig::default());

        let resolution = analysis
            .resolution_context_from_sources()
            .expect("resolution context construction should succeed");
        assert_eq!(resolution.module_root(), Some(module_dir.as_path()));
    }

    #[test]
    fn resolution_policy_is_consistent_across_surfaces() {
        // The CLI batch path discovers from source directories while the LSP
        // path discovers from the current working directory. Both go through
        // `resolution_context_from_paths`, so equivalent inputs must produce
        // the same manifest decision.
        let module_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            module_dir.path().join(wdl_modules::MANIFEST_FILENAME),
            MANIFEST,
        )
        .unwrap();
        let nested = module_dir.path().join("workflows").join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        let config = wdl_modules::resolver::ModulesConfig::default();
        let feature_flags = super::FeatureFlags::default().with_wdl_1_4();

        // The CLI surface starts from a source directory at the module root.
        let from_sources = resolution_context_from_paths(
            &config,
            &feature_flags,
            &[module_dir.path().to_path_buf()],
        )
        .expect("resolution context construction should succeed");
        // The LSP surface starts from a working directory nested in the module.
        let from_cwd = resolution_context_from_paths(&config, &feature_flags, &[nested])
            .expect("resolution context construction should succeed");

        assert_eq!(from_sources.module_root(), Some(module_dir.path()));
        assert_eq!(
            from_sources.module_root(),
            from_cwd.module_root(),
            "batch and LSP discovery should resolve to the same `module.json`"
        );
    }
}
