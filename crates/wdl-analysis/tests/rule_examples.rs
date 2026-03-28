//! Validates analysis rule `examples()` documentation strings.
//!
//! Each example may contain one or more ` ```wdl ` … ` ``` ` fenced blocks. **Every** block must:
//!
//! 1. Parse without parse diagnostics on the primary document being analyzed.
//! 2. Produce **at least one** diagnostic whose rule id matches the rule being documented.
//!
//! # Special cases
//!
//! - **Temp directory**: snippets are written as `main.wdl` inside a [`tempfile::TempDir`] that is
//!   kept alive until analysis finishes (a bare [`NamedTempFile`] would delete the path too early).
//! - **UnusedImport**: the snippet imports `example2.wdl`. The test writes a minimal
//!   `example2.wdl` next to `main.wdl` so resolution succeeds.
//! - **UsingFallbackVersion**: the snippet uses an unsupported `version development` line. The test
//!   runs the analyzer with `with_fallback_version(Some(1.2))` so the documented behavior occurs.
//!   The expected diagnostic is reported as a parse diagnostic with that rule id; those messages
//!   are excluded from the “must parse cleanly” assertion.

use std::str::FromStr as _;

use tempfile::tempdir;
use wdl_analysis::Analyzer;
use wdl_analysis::Config;
use wdl_analysis::UNUSED_IMPORT_RULE_ID;
use wdl_analysis::USING_FALLBACK_VERSION;
use wdl_analysis::path_to_uri;
use wdl_analysis::rules;
use wdl_ast::SupportedVersion;

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

const IMPORT_STUB: &str = r#"version 1.2

workflow imported_stub {
    meta {}

    output {}
}
"#;

async fn analyze_snippet_for_rule(rule_id: &str, source: &str) -> (Vec<String>, Vec<String>) {
    let (uri, _temp_dir): (_, Option<tempfile::TempDir>) = if rule_id == UNUSED_IMPORT_RULE_ID {
        let dir = tempdir().expect("tempdir");
        let main_path = dir.path().join("main.wdl");
        std::fs::write(dir.path().join("example2.wdl"), IMPORT_STUB).expect("write stub");
        std::fs::write(&main_path, source).expect("write WDL");
        (path_to_uri(&main_path).expect("URI"), Some(dir))
    } else {
        let dir = tempdir().expect("tempdir");
        let main_path = dir.path().join("main.wdl");
        std::fs::write(&main_path, source).expect("write WDL");
        (path_to_uri(&main_path).expect("URI"), Some(dir))
    };

    let analyzer = if rule_id == USING_FALLBACK_VERSION {
        Analyzer::new(
            Config::default()
                .with_fallback_version(Some(SupportedVersion::from_str("1.2").expect("1.2"))),
            |_, _, _, _| async {},
        )
    } else {
        Analyzer::default()
    };

    analyzer
        .add_document(uri.clone())
        .await
        .expect("add_document");
    let results = analyzer.analyze(()).await.expect("analyze");
    let result = results
        .into_iter()
        .find(|r| r.document().uri().as_ref() == &uri)
        .expect("analysis result for temp file");

    let parse_msgs: Vec<String> = result
        .document()
        .parse_diagnostics()
        .iter()
        .filter(|d| d.rule() != Some(USING_FALLBACK_VERSION))
        .map(|d| d.message().to_string())
        .collect();

    let rule_ids: Vec<String> = result
        .document()
        .diagnostics()
        .filter_map(|d| d.rule().map(ToString::to_string))
        .collect();

    (rule_ids, parse_msgs)
}

#[tokio::test]
async fn analysis_rule_examples_parse_and_trigger_rule() {
    for rule in rules() {
        let id = rule.id();
        let examples = rule.examples();
        assert!(
            !examples.is_empty(),
            "analysis rule `{id}` has no examples()"
        );

        for (example_idx, md) in examples.iter().enumerate() {
            let snippets = wdl_snippets_from_example(md);
            assert!(
                !snippets.is_empty(),
                "analysis rule `{id}` example[{example_idx}] has no ```wdl blocks"
            );

            for (snip_idx, source) in snippets.iter().enumerate() {
                let (rule_ids, parse_msgs) = analyze_snippet_for_rule(id, source).await;
                assert!(
                    parse_msgs.is_empty(),
                    "analysis rule `{id}` example[{example_idx}] snippet[{snip_idx}] must parse; \
                     snippet:\n{source}\nparse diagnostics:\n{}",
                    parse_msgs.join("\n")
                );

                let hits: Vec<_> = rule_ids.iter().filter(|r| *r == id).collect();
                assert!(
                    !hits.is_empty(),
                    "analysis rule `{id}` example[{example_idx}] snippet[{snip_idx}] should \
                     trigger `{id}` but did not.\n\
                     --- WDL ---\n{source}\n\
                     --- emitted rule ids ---\n{rule_ids:?}"
                );
            }
        }
    }
}
