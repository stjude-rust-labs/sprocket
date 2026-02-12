//! Integration tests for the `textDocument/completion` request.

mod common;

use common::TestContext;
use pretty_assertions::assert_eq;
use tower_lsp::lsp_types::CompletionContext;
use tower_lsp::lsp_types::CompletionItem;
use tower_lsp::lsp_types::CompletionItemKind;
use tower_lsp::lsp_types::CompletionParams;
use tower_lsp::lsp_types::CompletionResponse;
use tower_lsp::lsp_types::CompletionTriggerKind;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::TextDocumentIdentifier;
use tower_lsp::lsp_types::TextDocumentPositionParams;
use tower_lsp::lsp_types::request::Completion;

async fn completion_request(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
) -> Option<CompletionResponse> {
    ctx.request::<Completion>(CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri(path),
            },
            position,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: Some(CompletionContext {
            trigger_kind: CompletionTriggerKind::INVOKED,
            trigger_character: None,
        }),
    })
    .await
}

fn assert_contains(items: &[CompletionItem], expected_label: &str) {
    assert!(
        items.iter().any(|item| item.label == expected_label),
        "completion items should have contained '{expected_label}'"
    );
}

fn assert_not_contains(items: &[CompletionItem], unexpected_label: &str) {
    assert!(
        !items.iter().any(|item| item.label == unexpected_label),
        "completion items should NOT have contained '{unexpected_label}'"
    );
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("completions");
    ctx.initialize().await;
    ctx
}

#[tokio::test]
async fn should_complete_top_level_keywords() {
    let mut ctx = setup().await;
    let response = completion_request(&mut ctx, "source.wdl", Position::new(1, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    // top-level keywords
    assert_contains(&items, "version");
    assert_contains(&items, "task");
    assert_contains(&items, "workflow");
    assert_contains(&items, "struct");
    assert_contains(&items, "import");

    assert_not_contains(&items, "input");
    assert_not_contains(&items, "output");
    assert_not_contains(&items, "meta");
    assert_not_contains(&items, "parameter_meta");

    // task keywords
    assert_not_contains(&items, "command");
    assert_not_contains(&items, "requirements");
    assert_not_contains(&items, "hints");
    assert_not_contains(&items, "runtime");

    // workflow keywords
    assert_not_contains(&items, "call");
    assert_not_contains(&items, "scatter");
    assert_not_contains(&items, "if");

    // types
    assert_not_contains(&items, "Boolean");
    assert_not_contains(&items, "Int");
    assert_not_contains(&items, "Float");
    assert_not_contains(&items, "String");
    assert_not_contains(&items, "File");
    assert_not_contains(&items, "Directory");
    assert_not_contains(&items, "Array");
    assert_not_contains(&items, "Map");
    assert_not_contains(&items, "Object");
    assert_not_contains(&items, "Pair");

    // Should not contain stdlib functions at top level
    assert_not_contains(&items, "stdout");
}

#[tokio::test]
async fn should_complete_workflow_keywords() {
    let mut ctx = setup().await;
    let response = completion_request(&mut ctx, "source.wdl", Position::new(13, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    // workflow keywords
    assert_contains(&items, "call");
    assert_contains(&items, "scatter");
    assert_contains(&items, "if");
    assert_contains(&items, "hints");

    assert_contains(&items, "input");
    assert_contains(&items, "output");
    assert_contains(&items, "meta");
    assert_contains(&items, "parameter_meta");

    // types
    assert_contains(&items, "Boolean");
    assert_contains(&items, "Int");
    assert_contains(&items, "Float");
    assert_contains(&items, "String");
    assert_contains(&items, "File");
    assert_contains(&items, "Directory");
    assert_contains(&items, "Array");
    assert_contains(&items, "Map");
    assert_contains(&items, "Object");
    assert_contains(&items, "Pair");

    // top-level keywords
    assert_not_contains(&items, "version");
    assert_not_contains(&items, "task");
    assert_not_contains(&items, "workflow");
    assert_not_contains(&items, "struct");
    assert_not_contains(&items, "import");

    // task-specific keywords
    assert_not_contains(&items, "command");
    assert_not_contains(&items, "requirements");
    assert_not_contains(&items, "runtime");

    // Should contain stdlib functions
    assert_contains(&items, "stdout");
}

#[tokio::test]
async fn should_complete_task_keywords() {
    let mut ctx = setup().await;
    let response = completion_request(&mut ctx, "lib.wdl", Position::new(3, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    // task keywords
    assert_contains(&items, "command");
    assert_contains(&items, "requirements");
    assert_contains(&items, "hints");
    assert_contains(&items, "runtime");

    assert_contains(&items, "input");
    assert_contains(&items, "output");
    assert_contains(&items, "meta");
    assert_contains(&items, "parameter_meta");

    // types
    assert_contains(&items, "Boolean");
    assert_contains(&items, "Int");
    assert_contains(&items, "Float");
    assert_contains(&items, "String");
    assert_contains(&items, "File");
    assert_contains(&items, "Directory");
    assert_contains(&items, "Array");
    assert_contains(&items, "Map");
    assert_contains(&items, "Object");
    assert_contains(&items, "Pair");

    // top-level keywords
    assert_not_contains(&items, "version");
    assert_not_contains(&items, "task");
    assert_not_contains(&items, "workflow");
    assert_not_contains(&items, "struct");
    assert_not_contains(&items, "import");

    // workflow keywords
    assert_not_contains(&items, "call");
    assert_not_contains(&items, "scatter");
    assert_not_contains(&items, "if");

    // Should contain stdlib functions
    assert_contains(&items, "stdout");
}

#[tokio::test]
async fn should_complete_struct_keywords() {
    let mut ctx = setup().await;
    let response = completion_request(&mut ctx, "source.wdl", Position::new(9, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    // types
    assert_contains(&items, "Boolean");
    assert_contains(&items, "Int");
    assert_contains(&items, "Float");
    assert_contains(&items, "String");
    assert_contains(&items, "File");
    assert_contains(&items, "Directory");
    assert_contains(&items, "Array");
    assert_contains(&items, "Map");
    assert_contains(&items, "Object");
    assert_contains(&items, "Pair");

    assert_contains(&items, "meta");
    assert_contains(&items, "parameter_meta");
    assert_not_contains(&items, "input");
    assert_not_contains(&items, "output");

    // other structs
    assert_contains(&items, "Foo");

    // top-level keywords
    assert_not_contains(&items, "version");
    assert_not_contains(&items, "task");
    assert_not_contains(&items, "workflow");
    assert_not_contains(&items, "struct");
    assert_not_contains(&items, "import");

    // task keywords
    assert_not_contains(&items, "command");
    assert_not_contains(&items, "requirements");
    assert_not_contains(&items, "hints");
    assert_not_contains(&items, "runtime");

    // workflow keywords
    assert_not_contains(&items, "call");
    assert_not_contains(&items, "scatter");
    assert_not_contains(&items, "if");

    // Should not contain stdlib functions
    assert_not_contains(&items, "stdout");
}

#[tokio::test]
async fn should_complete_struct_members_access() {
    let mut ctx = setup().await;

    // Position of cursor `String n = my_foo.`
    let response = completion_request(&mut ctx, "source.wdl", Position::new(21, 22)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_eq!(items.len(), 1, "should only complete the single member");
    assert_contains(&items, "bar");
    assert_not_contains(&items, "baz");
}

#[tokio::test]
async fn should_complete_with_partial_word() {
    let mut ctx = setup().await;
    // Position of cursor at `Int out = qux.n`
    let response = completion_request(&mut ctx, "partial.wdl", Position::new(13, 23)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_eq!(items.len(), 1, "should only have a single item");
    assert_contains(&items, "num");
}

#[tokio::test]
async fn should_complete_namespace_members() {
    let mut ctx = setup().await;
    // Position of cursor at `call lib.`
    let response = completion_request(&mut ctx, "namespaces.wdl", Position::new(5, 13)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_eq!(items.len(), 2);
    assert_contains(&items, "greet");
}

#[tokio::test]
async fn should_complete_scope_variables() {
    let mut ctx = setup().await;

    // Workflow scope
    let response = completion_request(&mut ctx, "scopes.wdl", Position::new(10, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    // Struct
    assert_contains(&items, "Person");
    // task from current file
    assert_contains(&items, "A");
    // task from imported file
    assert_contains(&items, "lib.greet");
    // Namespace
    assert_contains(&items, "lib");
    // Stdlib function
    assert_contains(&items, "floor");
    assert_contains(&items, "min");
    assert_contains(&items, "stdout");
    assert_contains(&items, "stderr");

    // Workflow specific keywords
    assert_contains(&items, "call");
    assert_contains(&items, "hints");
    assert_contains(&items, "input");
    assert_contains(&items, "output");
    assert_contains(&items, "meta");
    assert_contains(&items, "parameter_meta");
    assert_not_contains(&items, "runtime");
    assert_not_contains(&items, "requirements");

    // Task scope
    let response = completion_request(&mut ctx, "scopes.wdl", Position::new(17, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    // Variable
    assert_contains(&items, "number");
    // Struct
    assert_contains(&items, "Person");
    // Stdlib function
    assert_contains(&items, "floor");

    // Task specific keywords
    assert_contains(&items, "hints");
    assert_contains(&items, "input");
    assert_contains(&items, "output");
    assert_contains(&items, "meta");
    assert_contains(&items, "parameter_meta");
    assert_contains(&items, "runtime");
    assert_contains(&items, "requirements");
    assert_not_contains(&items, "call");
}

#[tokio::test]
async fn should_complete_task_variable_members() {
    let mut ctx = setup().await;

    // In command section
    let response = completion_request(&mut ctx, "taskvar.wdl", Position::new(4, 21)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_contains(&items, "name");
    assert_contains(&items, "id");
    assert_contains(&items, "cpu");
    assert_contains(&items, "memory");
    assert_contains(&items, "container");
    assert_contains(&items, "meta");

    // In output section
    let response = completion_request(&mut ctx, "taskvar.wdl", Position::new(8, 24)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_contains(&items, "name");
    assert_contains(&items, "id");
    assert_contains(&items, "cpu");
    assert_contains(&items, "memory");
    assert_contains(&items, "container");
    assert_contains(&items, "meta");
    assert_contains(&items, "return_code");

    // Not a member
    assert_not_contains(&items, "foo");
}

#[tokio::test]
async fn should_complete_runtime_keys() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "sections.wdl", Position::new(4, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_contains(&items, "container");
    assert_contains(&items, "cpu");
    assert_contains(&items, "memory");
    assert_contains(&items, "disks");
    assert_contains(&items, "gpu");

    assert_not_contains(&items, "max_retries"); // Not an alias in runtime
}

#[tokio::test]
async fn should_complete_requirements_keys() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "sections.wdl", Position::new(8, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_contains(&items, "container");
    assert_contains(&items, "cpu");
    assert_contains(&items, "memory");
    assert_contains(&items, "disks");
    assert_contains(&items, "gpu");
    assert_contains(&items, "fpga");
    assert_contains(&items, "max_retries");
    assert_contains(&items, "return_codes");
}

#[tokio::test]
async fn should_complete_task_hints_keys() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "sections.wdl", Position::new(12, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_contains(&items, "max_memory");
    assert_contains(&items, "short_task");
}

#[tokio::test]
async fn should_complete_workflow_hints_keys() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "sections.wdl", Position::new(18, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_contains(&items, "allow_nested_inputs");
}

#[tokio::test]
async fn should_complete_versions() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "version.wdl", Position::new(0, 8)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_contains(&items, "1.0");
    assert_contains(&items, "1.1");
    assert_contains(&items, "1.2");
}

#[tokio::test]
async fn should_complete_namespaced_task_as_snippet() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "snippet_ns.wdl", Position::new(5, 8)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    let snippet_label = "lib.greet {...}";
    let snippet_item = items.iter().find(|i| i.label == snippet_label);
    assert!(
        snippet_item.is_some(),
        "completion items should have contained '{}'",
        snippet_label
    );

    let snippet_item = snippet_item.unwrap();
    assert_eq!(snippet_item.kind, Some(CompletionItemKind::SNIPPET));
    assert!(snippet_item.insert_text.is_some());
    let insert_text = snippet_item.insert_text.as_ref().unwrap();
    let expected_snippet = "lib.greet {\n\tname = ${1}\n}";
    assert_eq!(insert_text, expected_snippet);
}

#[tokio::test]
async fn should_complete_local_task_as_snippet() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "snippet_local.wdl", Position::new(12, 8)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    let snippet_label = "local {...}";
    let snippet_item = items.iter().find(|i| i.label == snippet_label);
    assert!(
        snippet_item.is_some(),
        "completion items should have contained '{}'",
        snippet_label
    );

    let snippet_item = snippet_item.unwrap();
    assert_eq!(snippet_item.kind, Some(CompletionItemKind::SNIPPET));
    assert!(snippet_item.insert_text.is_some());
    let insert_text = snippet_item.insert_text.as_ref().unwrap();
    let expected_snippet = "local {\n\tname = ${1}\n}";
    assert_eq!(insert_text, expected_snippet);
}

#[tokio::test]
async fn should_complete_struct_literal_as_snippet() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "snippet_struct.wdl", Position::new(8, 16)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    let snippet_label = "MyStruct { name, age }";
    let snippet_item = items.iter().find(|i| i.label == snippet_label);
    assert!(
        snippet_item.is_some(),
        "completion items should have contained '{}'",
        snippet_label
    );

    let snippet_item = snippet_item.unwrap();
    assert_eq!(snippet_item.kind, Some(CompletionItemKind::SNIPPET));
    assert!(snippet_item.insert_text.is_some());
    let insert_text = snippet_item.insert_text.as_ref().unwrap();
    let expected_snippet = "MyStruct {\n\tname: ${1},\n\tage: ${2}\n}";
    assert_eq!(insert_text, expected_snippet);
}

#[tokio::test]
async fn should_complete_enum_choices() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "snippet_enum.wdl", Position::new(9, 22)).await;

    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    assert_contains(&items, "Active");
    assert_contains(&items, "Inactive");
    assert_contains(&items, "Pending");
}

#[tokio::test]
async fn should_complete_top_level_keyword_as_snippet() {
    let mut ctx = setup().await;

    let response = completion_request(&mut ctx, "snippet_keyword.wdl", Position::new(0, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    let snippet_label = "task";
    let snippet_item = items.iter().find(|i| i.label == snippet_label);
    assert!(
        snippet_item.is_some(),
        "completion items should have contained '{}'",
        snippet_label
    );

    let snippet_item = snippet_item.unwrap();
    assert_eq!(snippet_item.kind, Some(CompletionItemKind::SNIPPET));
    assert!(snippet_item.insert_text.is_some());
}

#[tokio::test]
async fn should_not_complete_shadowed_type_names() {
    let mut ctx = setup().await;

    let response =
        completion_request(&mut ctx, "type_name_shadowing.wdl", Position::new(25, 0)).await;
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected a response, got none");
    };

    // `Salutation` should be in completions as an enum.
    assert!(
        items
            .iter()
            .any(|item| item.label == "Salutation" && item.kind == Some(CompletionItemKind::ENUM)),
        "completions should have contained an enum item for 'Salutation'"
    );

    // `Status` should be in completions as a variable.
    assert!(
        items
            .iter()
            .any(|item| item.label == "Status" && item.kind == Some(CompletionItemKind::VARIABLE)),
        "completions should have contained a variable item for 'Status'"
    );

    // `x` should be in completions as a variable.
    assert!(
        items
            .iter()
            .any(|item| item.label == "x" && item.kind == Some(CompletionItemKind::VARIABLE)),
        "completions should have contained a variable item for 'x'"
    );

    // `Status` should NOT be in completions as an enum.
    assert!(
        !items
            .iter()
            .any(|item| item.label == "Status" && item.kind == Some(CompletionItemKind::ENUM)),
        "completions should NOT have contained an enum item for 'Status'"
    );
}
