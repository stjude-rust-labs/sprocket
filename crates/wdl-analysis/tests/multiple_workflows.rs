//! Integration tests for multiple workflows in a WDL 1.4 document.

use std::fs;

use tempfile::TempDir;
use wdl_analysis::Analyzer;
use wdl_analysis::Config;
use wdl_analysis::Document;
use wdl_analysis::FeatureFlags;
use wdl_analysis::path_to_uri;
use wdl_analysis::types::CallKind;
use wdl_ast::Severity;

async fn analyze(source: &str) -> Document {
    let dir = TempDir::new().expect("temporary directory should be created");
    let path = dir.path().join("source.wdl");
    fs::write(&path, source).expect("test document should be written");

    let uri = path_to_uri(path).expect("source URI should be valid");
    let config = Config::default().with_feature_flags(FeatureFlags::default().with_wdl_1_4());
    let analyzer = Analyzer::new(config, |(), _, _, _| async {});
    analyzer
        .add_document(uri.clone())
        .await
        .expect("source document should be added");

    analyzer
        .analyze_document((), uri)
        .await
        .expect("analysis should succeed")
        .pop()
        .expect("source result should exist")
        .document()
        .clone()
}

fn errors(document: &Document) -> Vec<String> {
    document
        .diagnostics()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(|diagnostic| diagnostic.message().to_string())
        .collect()
}

#[tokio::test]
async fn versions_before_one_four_allow_at_most_one_workflow() {
    for version in ["1.0", "1.1", "1.2", "1.3"] {
        let zero = analyze(&format!(
            "version {version}\n\ntask only {{\n    command <<<>>>\n}}\n"
        ))
        .await;
        assert!(
            errors(&zero).is_empty(),
            "WDL {version} should allow zero workflows"
        );
        assert_eq!(zero.local_workflows().count(), 0);

        let one = analyze(&format!("version {version}\n\nworkflow first {{}}\n")).await;
        assert!(
            errors(&one).is_empty(),
            "WDL {version} should allow one workflow"
        );
        assert_eq!(one.local_workflows().count(), 1);

        let two = analyze(&format!(
            "version {version}\n\nworkflow first {{}}\n\nworkflow second {{}}\n"
        ))
        .await;
        assert_eq!(
            errors(&two),
            ["cannot define workflow `second` as only one workflow is allowed per source file"],
            "WDL {version} should reject a second workflow"
        );
        assert_eq!(two.local_workflows().count(), 1);
    }
}

#[tokio::test]
async fn one_four_allows_multiple_workflows() {
    let document = analyze(
        r#"version 1.4

workflow first {}

workflow second {}
"#,
    )
    .await;

    assert!(errors(&document).is_empty());
    assert_eq!(
        document
            .local_workflows()
            .map(|workflow| workflow.name())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
}

#[tokio::test]
async fn forward_workflow_call_uses_complete_signature() {
    let document = analyze(
        r#"version 1.4

workflow caller {
    call callee {
        value = 41
    }

    output {
        Int result = callee.result
    }
}

workflow callee {
    input {
        Int value
    }

    output {
        Int result = value + 1
    }
}
"#,
    )
    .await;

    assert!(errors(&document).is_empty());
    let caller = document
        .local_workflow_by_name("caller")
        .expect("caller workflow should exist");
    let call = caller
        .calls()
        .get("callee")
        .expect("forward workflow call should resolve");
    assert_eq!(call.kind(), CallKind::Workflow);
    assert!(call.inputs().contains_key("value"));
    assert!(call.outputs().contains_key("result"));
}

#[tokio::test]
async fn direct_and_transitive_recursive_calls_are_rejected() {
    let direct = analyze(
        r#"version 1.4

workflow direct {
    call direct
}
"#,
    )
    .await;
    assert_eq!(
        errors(&direct),
        ["cannot recursively call workflow `direct`"]
    );

    let transitive = analyze(
        r#"version 1.4

workflow first {
    call second
}

workflow second {
    call third
}

workflow third {
    call first
}
"#,
    )
    .await;
    assert_eq!(
        errors(&transitive),
        ["cannot recursively call workflow `first`"]
    );
}

#[tokio::test]
async fn duplicate_workflow_name_reports_conflict_and_retains_first() {
    let document = analyze(
        r#"version 1.4

workflow duplicate {
    output {
        Int value = 1
    }
}

workflow duplicate {
    output {
        String value = "second"
    }
}
"#,
    )
    .await;

    assert_eq!(errors(&document), ["conflicting workflow name `duplicate`"]);
    let workflows = document.local_workflows().collect::<Vec<_>>();
    assert_eq!(workflows.len(), 1);
    assert_eq!(
        workflows[0]
            .outputs()
            .get("value")
            .expect("first workflow output should be retained")
            .ty()
            .to_string(),
        "Int"
    );
}
