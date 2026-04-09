//! Lint example tests.
//!
//! This checks the lint rules examples in `wdl-lint` and `wdl-analysis`, making
//! sure that they:
//!
//! 1. Parse without error
//! 2. Trigger the lint they're supposed to
//! 3. Are well-formatted

use std::collections::HashMap;
use std::sync::Arc;

use libtest_mimic::Failed;
use libtest_mimic::Trial;
use tempfile::TempDir;
use tempfile::tempdir;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use wdl::analysis::Analyzer;
use wdl::analysis::Config as AnalysisConfig;
use wdl::analysis::DiagnosticsConfig;
use wdl::analysis::Example;
use wdl::analysis::Rule as AnalysisRule;
use wdl::analysis::Validator;
use wdl::ast::AstNode;
use wdl::ast::Node;
use wdl::ast::version::V1;
use wdl::format::Config as FormatConfig;
use wdl::format::Formatter;
use wdl::format::element::node::AstNodeFormatExt;
use wdl::grammar::SupportedVersion;
use wdl::lint::Linter;
use wdl::lint::Rule as LintRule;

/// Lints to skip the formatting check for, as they (intentionally) clash with
/// `wdl-format` defaults.
const BAD_FORMATTING_EXPECTED: &[&str] = &[
    "SectionOrdering",
    "HereDocCommands",
    "DeprecatedPlaceholder",
    "ImportPlacement",
    "CommandSectionIndentation",
    // TODO: https://github.com/stjude-rust-labs/sprocket/issues/800
    "DoubleQuotes",
    // TODO: https://github.com/stjude-rust-labs/sprocket/issues/799
    "DocCommentTabs",
    "UnusedDocComments",
    "EmptyDocComment",
];

/// Examples that need a dummy file to import.
const NEEDS_IMPORT: &[&str] = &["UnusedImport"];

/// The name of the dummy import file.
const IMPORT_DOC_NAME: &str = "foo.wdl";

/// The version to use for examples needing fallback versions.
const FALLBACK_VERSION: SupportedVersion = SupportedVersion::V1(V1::Three);

/// Lint rule testing context.
struct TestContext {
    /// Collection of all rules and their [`Analyzer`]s.
    tests: Mutex<HashMap<&'static str, Analyzer<()>>>,
    /// A default-configured formatter.
    formatter: Formatter,
    /// The base temp directory.
    tmp: TempDir,
}

impl TestContext {
    /// Create a new `TestContext`.
    fn new() -> anyhow::Result<Self> {
        let tmp = tempdir()?;
        let tests = HashMap::new();
        let formatter = Formatter::new(FormatConfig::default());

        Ok(Self {
            tests: Mutex::new(tests),
            formatter,
            tmp,
        })
    }

    /// Setup the test directory for a lint rule.
    async fn add_rule(
        &self,
        analyzer: &Analyzer<()>,
        rule: &'static str,
        examples: &[Example],
    ) -> anyhow::Result<()> {
        let asset_dir = self.tmp.path().join(rule);
        std::fs::create_dir(&asset_dir)?;

        for (index, example) in examples.iter().enumerate() {
            std::fs::write(
                asset_dir.join(format!("negative-{index}.wdl")),
                example.negative.snippet,
            )?;
            if let Some(revised) = example.revised {
                std::fs::write(
                    asset_dir.join(format!("revised-{index}.wdl")),
                    revised.snippet,
                )?;
            }
        }

        if NEEDS_IMPORT.contains(&rule) {
            std::fs::write(
                asset_dir.join(IMPORT_DOC_NAME),
                include_str!("../crates/wdl-grammar/tests/parsing/enums/source.wdl"),
            )?;
        }

        analyzer.add_directory(asset_dir).await?;

        Ok(())
    }

    /// Add a `wdl-lint` rule to the context.
    async fn add_lint_rule(&self, rule: Box<dyn LintRule + Send + Sync>) -> anyhow::Result<()> {
        let id = rule.id();
        let examples = rule.examples();

        let validator = Box::new(move || {
            let mut validator = Validator::empty();
            validator.add_visitor(Linter::new(std::iter::once(
                rule.clone() as Box<dyn LintRule>
            )));

            validator
        });

        let analyzer = Analyzer::new_with_validator(
            AnalysisConfig::default(),
            |_, _, _, _| async {},
            validator,
        );

        self.add_rule(&analyzer, id, examples).await?;
        self.tests.lock().await.insert(id, analyzer);

        Ok(())
    }

    /// Add a `wdl-analysis` rule to the context.
    async fn add_analysis_rule(&self, rule: Box<dyn AnalysisRule>) -> anyhow::Result<()> {
        let id = rule.id();
        let examples = rule.examples();

        let validator = Box::new(Validator::empty);

        let analyzer = Analyzer::new_with_validator(
            AnalysisConfig::default()
                .with_diagnostics_config(DiagnosticsConfig::new(std::iter::once(rule)))
                .with_fallback_version(Some(FALLBACK_VERSION)),
            |_, _, _, _| async {},
            validator,
        );

        self.add_rule(&analyzer, id, examples).await?;
        self.tests.lock().await.insert(id, analyzer);

        Ok(())
    }
}

/// Verify a lint's examples.
///
/// This verifies that:
///
/// 1. Negative examples trigger the expected rule.
/// 2. Revised examples no longer trigger the rule.
/// 3. The examples are well-formatted.
async fn verify_examples(ctx: Arc<TestContext>, expected_rule: &str) -> Result<(), Failed> {
    let results = {
        let tests = ctx.tests.lock().await;
        let analyzer = tests.get(expected_rule).expect("should exist");

        analyzer.analyze(()).await?
    };

    if results.is_empty() {
        return Err("Analysis returned no results".into());
    }

    for result in results {
        if result.document().path().ends_with(IMPORT_DOC_NAME) {
            continue;
        }

        if let Some(parse_error) = result.error() {
            return Err(parse_error.into());
        }

        // TODO: Actually emit the diagnostics, https://github.com/stjude-rust-labs/sprocket/pull/686
        if result
            .document()
            .parse_diagnostics()
            .iter()
            .filter(|d| d.severity().is_error())
            .count()
            > 1
        {
            return Err("Document has errors".into());
        }

        let document_path = result.document().path();
        let has_expected_rule = result
            .document()
            .diagnostics()
            .any(|d| d.rule() == Some(expected_rule));
        let is_negative_snippet = result.document().path().contains("negative-");

        let example_index = document_path
            .trim_end_matches(".wdl")
            .rsplit_once('-')
            .unwrap()
            .1;

        if has_expected_rule {
            if !is_negative_snippet {
                return Err(format!(
                    "the revision in example {example_index} still triggers the rule"
                )
                .into());
            }
        } else if is_negative_snippet {
            return Err(format!(
                "example {example_index}'s negative snippet did not trigger the expected rule"
            )
            .into());
        }

        if BAD_FORMATTING_EXPECTED.contains(&expected_rule) {
            continue;
        }

        let original_source = result.document().root().text().to_string();
        let ast = result
            .document()
            .root()
            .ast_with_version_fallback(result.document().config().fallback_version())
            .unwrap_v1();
        let element = Node::Ast(ast).into_format_element();

        let formatted = ctx.formatter.format(&element)?;
        if formatted != original_source {
            eprint!(
                "{}",
                pretty_assertions::StrComparison::new(&original_source, &formatted)
            );
            return Err(format!(
                "example {example_index}'s {snippet_ty} is not properly formatted",
                snippet_ty = if is_negative_snippet {
                    "negative snippet"
                } else {
                    "revision snippet"
                }
            )
            .into());
        }
    }

    Ok(())
}

/// Collect all rules from `wdl-lint` and `wdl-analysis` and create [`Trial`]s
/// for their examples.
fn find_tests(ctx: Arc<TestContext>, handle: &Handle) -> Vec<Trial> {
    wdl::analysis::rules()
        .into_iter()
        .map(|r| {
            let handle = handle.clone();
            let ctx = ctx.clone();
            let id = r.id();
            Trial::test(format!("examples-analysis-{id}"), move || {
                handle.block_on(async {
                    ctx.add_analysis_rule(r).await?;
                    verify_examples(ctx, id).await
                })
            })
        })
        .chain(
            wdl::lint::rules(&wdl::lint::Config::default())
                .into_iter()
                // TODO: Remove the `ConsistentNewlines` rule
                //       https://github.com/stjude-rust-labs/sprocket/issues/667
                .filter(|lint| lint.id() != "ConsistentNewlines")
                .map(|r| {
                    let handle = handle.clone();
                    let ctx = ctx.clone();
                    let id = r.id();
                    Trial::test(format!("examples-lint-{id}"), move || {
                        handle.block_on(async {
                            ctx.add_lint_rule(r).await?;
                            verify_examples(ctx, id).await
                        })
                    })
                }),
        )
        .collect()
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let runtime = tokio::runtime::Runtime::new()?;

    let args = libtest_mimic::Arguments::from_args();
    let ctx = TestContext::new()?;

    libtest_mimic::run(&args, find_tests(Arc::new(ctx), runtime.handle())).exit();
}
