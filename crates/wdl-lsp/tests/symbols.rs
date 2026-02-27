//! Integration tests for the `textDocument/documentSymbol` request.

mod common;

use common::TestContext;
use tower_lsp_server::ls_types::DocumentSymbol;
use tower_lsp_server::ls_types::DocumentSymbolParams;
use tower_lsp_server::ls_types::DocumentSymbolResponse;
use tower_lsp_server::ls_types::SymbolInformation;
use tower_lsp_server::ls_types::SymbolKind;
use tower_lsp_server::ls_types::TextDocumentIdentifier;
use tower_lsp_server::ls_types::WorkspaceSymbolParams;
use tower_lsp_server::ls_types::WorkspaceSymbolResponse;
use tower_lsp_server::ls_types::request::DocumentSymbolRequest;
use tower_lsp_server::ls_types::request::WorkspaceSymbolRequest;

async fn document_symbol_request(
    ctx: &mut TestContext,
    path: &str,
) -> Option<DocumentSymbolResponse> {
    ctx.request::<DocumentSymbolRequest>(DocumentSymbolParams {
        text_document: TextDocumentIdentifier {
            uri: ctx.doc_uri(path),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
}

async fn workspace_symbol_request(
    ctx: &mut TestContext,
    query: &str,
) -> Option<WorkspaceSymbolResponse> {
    ctx.request::<WorkspaceSymbolRequest>(WorkspaceSymbolParams {
        query: query.to_string(),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
}

fn assert_symbol(symbols: &[DocumentSymbol], name: &str, kind: SymbolKind) {
    assert!(
        symbols.iter().any(|s| s.name == name && s.kind == kind),
        "should have contained a symbol with name `{}` and kind `{:?}`",
        name,
        kind
    );
}

fn assert_symbol_found(symbols: &[SymbolInformation], name: &str) {
    assert!(
        symbols.iter().any(|s| s.name == name),
        "should have found symbol with name `{}`",
        name,
    );
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("symbols");
    ctx.initialize().await;
    ctx
}

#[tokio::test]
#[test_log::test]
async fn should_provide_document_symbols() {
    let mut ctx = setup().await;
    let response = document_symbol_request(&mut ctx, "source.wdl").await;
    let Some(DocumentSymbolResponse::Nested(symbols)) = response else {
        panic!("expected a response, got none");
    };

    assert_eq!(symbols.len(), 5);
    assert_symbol(&symbols, "lib", SymbolKind::NAMESPACE);
    assert_symbol(&symbols, "lib_alias", SymbolKind::NAMESPACE);
    assert_symbol(&symbols, "Person", SymbolKind::STRUCT);
    assert_symbol(&symbols, "greet", SymbolKind::FUNCTION);
    assert_symbol(&symbols, "main", SymbolKind::FUNCTION);

    let greet_task = symbols.iter().find(|s| s.name == "greet").unwrap();
    let greet_children = greet_task.children.as_ref().unwrap();
    assert_eq!(greet_children.len(), 3);
    assert_symbol(greet_children, "inputs", SymbolKind::NAMESPACE);
    assert_symbol(greet_children, "outputs", SymbolKind::NAMESPACE);

    let main_workflow = symbols.iter().find(|s| s.name == "main").unwrap();
    let main_children = main_workflow.children.as_ref().unwrap();

    // input p, call greet, output result
    // 1 inputs, 1 if, 1 scatter, 1 call, 1 output
    assert_eq!(main_children.len(), 5);
    assert_symbol(main_children, "inputs", SymbolKind::NAMESPACE);
    assert_symbol(main_children, "if (condition)", SymbolKind::OPERATOR);
    assert_symbol(
        main_children,
        "scatter (i in numbers)",
        SymbolKind::OPERATOR,
    );
    assert_symbol(main_children, "greet", SymbolKind::FUNCTION);
    assert_symbol(main_children, "outputs", SymbolKind::NAMESPACE);

    let if_block = main_children
        .iter()
        .find(|s| s.name == "if (condition)")
        .unwrap();
    let if_children = if_block.children.as_ref().unwrap();
    assert_eq!(if_children.len(), 1);
    assert_symbol(if_children, "greet_in_if", SymbolKind::FUNCTION);

    let scatter_block = main_children
        .iter()
        .find(|s| s.name == "scatter (i in numbers)")
        .unwrap();
    let scatter_children = scatter_block.children.as_ref().unwrap();
    assert_eq!(scatter_children.len(), 1);
    assert_symbol(scatter_children, "greet_in_scatter", SymbolKind::FUNCTION);

    let person_struct = symbols.iter().find(|s| s.name == "Person").unwrap();
    let person_children = person_struct.children.as_ref().unwrap();
    assert_eq!(person_children.len(), 2);
    assert_symbol(person_children, "name", SymbolKind::FIELD);
    assert_symbol(person_children, "age", SymbolKind::FIELD);
}

#[tokio::test]
#[test_log::test]
async fn should_provide_workspace_symbols() {
    let mut ctx = setup().await;
    let response = workspace_symbol_request(&mut ctx, "").await;
    let Some(WorkspaceSymbolResponse::Flat(symbols)) = response else {
        panic!("expected a response, got none");
    };

    assert_symbol_found(&symbols, "lib");
    assert_symbol_found(&symbols, "lib_alias");
    assert_symbol_found(&symbols, "Person");
    assert_symbol_found(&symbols, "greet");
    assert_symbol_found(&symbols, "main");
    assert_symbol_found(&symbols, "temp");
}

#[tokio::test]
#[test_log::test]
async fn should_filter_workspace_symbols() {
    let mut ctx = setup().await;
    let response = workspace_symbol_request(&mut ctx, "greet").await;
    let Some(WorkspaceSymbolResponse::Flat(symbols)) = response else {
        panic!("expected a response, got none");
    };

    assert_eq!(
        symbols.len(),
        4,
        "should find greet, greet_in_if, greet_in_scatter, and the greet task symbol"
    );
    assert!(symbols.iter().all(|s| s.name.contains("greet")));
}

#[tokio::test]
#[test_log::test]
async fn should_provide_enum_symbols() {
    let mut ctx = setup().await;
    let response = document_symbol_request(&mut ctx, "enum.wdl").await;
    let Some(DocumentSymbolResponse::Nested(symbols)) = response else {
        panic!("expected a response, got none");
    };

    assert_symbol(&symbols, "Status", SymbolKind::ENUM);

    let status_enum = symbols.iter().find(|s| s.name == "Status").unwrap();
    let enum_children = status_enum.children.as_ref().unwrap();
    assert_eq!(enum_children.len(), 3);
    assert_symbol(enum_children, "Active", SymbolKind::ENUM_MEMBER);
    assert_symbol(enum_children, "Inactive", SymbolKind::ENUM_MEMBER);
    assert_symbol(enum_children, "Pending", SymbolKind::ENUM_MEMBER);
}
