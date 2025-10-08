//! Integration tests for the `textDocument/signatureHelp` request.

mod common;

use common::TestContext;
use pretty_assertions::assert_eq;
use tower_lsp::lsp_types::ParameterLabel;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::SignatureHelp;
use tower_lsp::lsp_types::SignatureHelpParams;
use tower_lsp::lsp_types::SignatureHelpTriggerKind;
use tower_lsp::lsp_types::TextDocumentIdentifier;
use tower_lsp::lsp_types::TextDocumentPositionParams;
use tower_lsp::lsp_types::request::SignatureHelpRequest;

async fn signature_help_request(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
) -> Option<SignatureHelp> {
    ctx.request::<SignatureHelpRequest>(SignatureHelpParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri(path),
            },
            position,
        },
        work_done_progress_params: Default::default(),
        context: Some(tower_lsp::lsp_types::SignatureHelpContext {
            trigger_kind: SignatureHelpTriggerKind::INVOKED,
            trigger_character: Some("(".to_string()),
            is_retrigger: false,
            active_signature_help: None,
        }),
    })
    .await
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("signature_help");
    ctx.initialize().await;
    ctx
}

#[tokio::test]
async fn should_provide_signature_help_for_stdlib_function() {
    let mut ctx = setup().await;

    // Position right after opening parenthesis: read_string(|)
    let response = signature_help_request(&mut ctx, "source.wdl", Position::new(5, 31)).await;
    let help = response.expect("should have a signature help response");

    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(0));

    let sig_info = &help.signatures[0];
    assert_eq!(sig_info.label, "read_string(file: File) -> String");
    // assert!(sig_info.documentation.is_some());
    assert_eq!(sig_info.parameters.as_ref().unwrap().len(), 1);

    let param_info = &sig_info.parameters.as_ref().unwrap()[0];
    assert_eq!(
        param_info.label,
        ParameterLabel::Simple("file: File".to_string())
    );
}

#[tokio::test]
async fn should_highlight_active_parameter() {
    let mut ctx = setup().await;

    // Position after the second comma: sub("a", "b", |)
    let response = signature_help_request(&mut ctx, "source.wdl", Position::new(6, 35)).await;
    let help = response.expect("should have a signature help response");

    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(2));

    let sig_info = &help.signatures[0];
    assert_eq!(
        sig_info.label,
        "sub(input: String, pattern: String, replace: String) -> String"
    );
    assert_eq!(sig_info.parameters.as_ref().unwrap().len(), 3);
}

#[tokio::test]
async fn should_provide_signature_help_for_polymorphic_function() {
    let mut ctx = setup().await;

    // Position right after opening parenthesis: size(|)
    let response = signature_help_request(&mut ctx, "source.wdl", Position::new(7, 24)).await;
    let help = response.expect("should have a signature help response");

    assert!(help.signatures.len() > 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(0));

    let sig_info = &help.signatures[1];
    assert_eq!(sig_info.label, "size(value: File?, <unit: String>) -> Float");
    // assert!(sig_info.documentation.is_some());
    assert_eq!(sig_info.parameters.as_ref().unwrap().len(), 2);

    let param_info = &sig_info.parameters.as_ref().unwrap()[0];
    assert_eq!(
        param_info.label,
        ParameterLabel::Simple("value: File?".to_string())
    );
}
