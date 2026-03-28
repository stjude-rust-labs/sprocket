//! Validates lint rule `examples()` documentation strings.
//!
//! # Convention
//!
//! Each entry in [`Rule::examples`](wdl_lint::Rule::examples) is Markdown that may contain one or
//! more ` ```wdl ` … ` ``` ` fenced blocks. **All blocks in the same entry share the same
//! expectation:**
//!
//! - **Even indices** (0, 2, …): *negative* examples — the WDL must emit **at least one**
//!   diagnostic with **this rule’s id** when only that lint rule is enabled (analysis “unused *”
//!   diagnostics are suppressed so they do not drown out the lint under test).
//! - **Odd indices** (1, 3, …): *positive* examples — the WDL must **not** emit a diagnostic with
//!   this rule’s id.
//!
//! # Exceptions
//!
//! - **ShellCheck**: skipped when the `shellcheck` executable is not on `PATH`, since the rule’s
//!   behavior depends on that tool.

use std::process::Command;

use tempfile::NamedTempFile;
use wdl_analysis::Analyzer;
use wdl_analysis::Config as AnalysisConfig;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::Validator;
use wdl_analysis::path_to_uri;
use wdl_lint::Config as LintConfig;
use wdl_lint::Linter;
use wdl_lint::rules as lint_rules;

fn wdl_snippets_from_example(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = markdown;
    while let Some(idx) = rest.find("```wdl") {
        let after = &rest[idx + "```wdl".len()..];
        let body = after
            .strip_prefix("\r\n")
            .or_else(|| after.strip_prefix('\n'))
            .unwrap_or(after);
        let end_rel = body
            .find("```")
            .expect("unclosed ```wdl fence in rule example");
        let snippet = body[..end_rel].trim();
        assert!(
            !snippet.is_empty(),
            "empty ```wdl block in rule example:\n{markdown}"
        );
        out.push(snippet.to_string());
        rest = &body[end_rel + 3..];
    }
    out
}

fn shellcheck_on_path() -> bool {
    Command::new("shellcheck")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

async fn lint_snippet_with_only_rule(source: &str, rule_id: &'static str) -> Vec<String> {
    let temp = NamedTempFile::new().expect("temp file");
    let path = temp.path().to_path_buf();
    std::fs::write(&path, source).expect("write WDL");
    let uri = path_to_uri(&path).expect("file URI");

    let analyzer = Analyzer::new_with_validator(
        AnalysisConfig::default().with_diagnostics_config(DiagnosticsConfig::except_all()),
        |_, _, _, _| async {},
        move || {
            let mut validator = Validator::default();
            let rule = lint_rules(&LintConfig::default())
                .into_iter()
                .find(|r| r.id() == rule_id)
                .unwrap_or_else(|| panic!("lint rule `{rule_id}` not found in registry"));
            validator.add_visitor(Linter::new(std::iter::once(rule)));
            validator
        },
    );

    analyzer
        .add_document(uri.clone())
        .await
        .expect("add_document");
    let results = analyzer.analyze(()).await.expect("analyze");
    let result = results
        .into_iter()
        .find(|r| r.document().uri().as_ref() == &uri)
        .expect("analysis result for temp file");

    assert!(
        result.error().is_none(),
        "rule `{rule_id}` example failed to load: {:?}",
        result.error()
    );

    let parse = result.document().parse_diagnostics();
    assert!(
        parse.is_empty(),
        "rule `{rule_id}` example has parse diagnostics:\n{source}\n{}",
        parse
            .iter()
            .map(|d| d.message().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );

    result
        .document()
        .diagnostics()
        .filter_map(|d| d.rule().map(ToString::to_string))
        .collect()
}

#[tokio::test]
async fn lint_rule_examples_parse_and_match_expectations() {
    for rule in lint_rules(&LintConfig::default()) {
        let id = rule.id();
        if id == "ShellCheck" && !shellcheck_on_path() {
            eprintln!("skipping ShellCheck examples: `shellcheck` not on PATH");
            continue;
        }

        let examples = rule.examples();
        assert!(!examples.is_empty(), "lint rule `{id}` has no examples()");

        for (example_idx, md) in examples.iter().enumerate() {
            let snippets = wdl_snippets_from_example(md);
            assert!(
                !snippets.is_empty(),
                "lint rule `{id}` example[{example_idx}] has no ```wdl blocks"
            );

            for (snip_idx, source) in snippets.iter().enumerate() {
                let rule_ids = lint_snippet_with_only_rule(source, id).await;
                let hits: Vec<_> = rule_ids.iter().filter(|r| *r == id).collect();

                if example_idx % 2 == 0 {
                    assert!(
                        !hits.is_empty(),
                        "lint rule `{id}` negative example[{example_idx}] snippet[{snip_idx}] \
                         should trigger `{id}` but did not.\n\
                         --- WDL ---\n{source}\n\
                         --- emitted rule ids ---\n{rule_ids:?}"
                    );
                } else {
                    assert!(
                        hits.is_empty(),
                        "lint rule `{id}` positive example[{example_idx}] snippet[{snip_idx}] \
                         should not trigger `{id}` but did.\n\
                         --- WDL ---\n{source}\n\
                         --- emitted rule ids ---\n{rule_ids:?}"
                    );
                }
            }
        }
    }
}
