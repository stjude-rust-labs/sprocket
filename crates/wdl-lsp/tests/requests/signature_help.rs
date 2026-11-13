//! Integration tests for the `textDocument/signatureHelp` request.

use async_lsp::lsp_types::DidChangeTextDocumentParams;
use async_lsp::lsp_types::ParameterLabel;
use async_lsp::lsp_types::Position;
use async_lsp::lsp_types::SignatureHelp;
use async_lsp::lsp_types::SignatureHelpParams;
use async_lsp::lsp_types::SignatureHelpTriggerKind;
use async_lsp::lsp_types::TextDocumentContentChangeEvent;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::TextDocumentPositionParams;
use async_lsp::lsp_types::VersionedTextDocumentIdentifier;
use async_lsp::lsp_types::notification::DidChangeTextDocument;
use async_lsp::lsp_types::request::SignatureHelpRequest;
use pretty_assertions::assert_eq;

use crate::common::TestContext;
use crate::common::TestContextBuilder;

async fn signature_help_request(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
) -> async_lsp::Result<Option<SignatureHelp>> {
    ctx.request::<SignatureHelpRequest>(SignatureHelpParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri(path),
            },
            position,
        },
        work_done_progress_params: Default::default(),
        context: Some(async_lsp::lsp_types::SignatureHelpContext {
            trigger_kind: SignatureHelpTriggerKind::INVOKED,
            trigger_character: Some("(".to_string()),
            is_retrigger: false,
            active_signature_help: None,
        }),
    })
    .await
}

async fn setup() -> TestContext {
    let mut ctx = TestContextBuilder::new("signature_help").build();
    ctx.initialize().await;
    ctx
}

#[tokio::test]
async fn should_provide_signature_help_for_stdlib_function() {
    let mut ctx = setup().await;

    // Position right after opening parenthesis: read_string(|)
    let response = signature_help_request(&mut ctx, "source.wdl", Position::new(5, 31))
        .await
        .expect("request should succeed");
    let help = response.expect("should have a signature help response");

    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(0));

    let sig_info = &help.signatures[0];
    assert_eq!(sig_info.label, "read_string(file: File) -> String");
    // assert!(sig_info.documentation.is_some());
    assert_eq!(sig_info.parameters.as_ref().unwrap().len(), 1);

    let param_info = &sig_info.parameters.as_ref().unwrap()[0];
    assert_eq!(param_info.label, ParameterLabel::LabelOffsets([12, 22]));
}

#[tokio::test]
async fn should_highlight_active_parameter() {
    let mut ctx = setup().await;

    // Position after the second comma: sub("a", "b", |)
    let response = signature_help_request(&mut ctx, "source.wdl", Position::new(6, 35))
        .await
        .expect("request should succeed");
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
    let params = sig_info.parameters.as_ref().unwrap();
    assert_eq!(params[0].label, ParameterLabel::LabelOffsets([4, 17]));
    assert_eq!(params[1].label, ParameterLabel::LabelOffsets([19, 34]));
    assert_eq!(params[2].label, ParameterLabel::LabelOffsets([36, 51]));
}

#[tokio::test]
async fn should_provide_signature_help_for_polymorphic_function() {
    let mut ctx = setup().await;

    // Position right after opening parenthesis: size(|)
    let response = signature_help_request(&mut ctx, "source.wdl", Position::new(7, 24))
        .await
        .expect("request should succeed");
    let help = response.expect("should have a signature help response");

    assert!(help.signatures.len() > 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(0));

    let sig_info = &help.signatures[1];
    assert_eq!(
        sig_info.label,
        "size(value: File?, <unit: String>) -> Float"
    );
    // assert!(sig_info.documentation.is_some());
    assert_eq!(sig_info.parameters.as_ref().unwrap().len(), 2);

    let param_info = &sig_info.parameters.as_ref().unwrap();
    assert_eq!(param_info[0].label, ParameterLabel::LabelOffsets([5, 17]));
    assert_eq!(param_info[1].label, ParameterLabel::LabelOffsets([20, 32]));
}

#[tokio::test]
async fn should_filter_signature_help_by_version() {
    let mut ctx = setup().await;

    let response = signature_help_request(&mut ctx, "source_v1_0.wdl", Position::new(7, 24))
        .await
        .expect("request should succeed");
    let help = response.expect("should have a signature help response");

    assert_eq!(help.signatures.len(), 2);
    assert!(
        help.signatures
            .iter()
            .all(|signature| !signature.label.contains("Directory"))
    );
    assert!(help.signatures.iter().all(|signature| {
        let Some(async_lsp::lsp_types::Documentation::MarkupContent(documentation)) =
            &signature.documentation
        else {
            return false;
        };
        documentation.value.contains("version 1.0")
            && !documentation.value.contains("files/directories")
    }));

    let response = signature_help_request(&mut ctx, "source_v1_0.wdl", Position::new(8, 36))
        .await
        .expect("request should succeed");
    assert!(response.is_none());
}

#[tokio::test]
async fn should_provide_signature_help_during_incremental_change() {
    let mut ctx = setup().await;
    let path = "source.wdl";
    let mut text = std::fs::read_to_string(ctx.doc_path(path)).expect("source should be readable");
    text.push('\n');

    ctx.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: ctx.doc_uri(path),
            version: 1,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text,
        }],
    })
    .expect("notification should succeed");

    let response = signature_help_request(&mut ctx, path, Position::new(7, 24))
        .await
        .expect("request should succeed");
    assert!(response.is_some());
}

#[tokio::test]
async fn should_provide_signature_help_for_local_task_call() {
    let mut ctx = setup().await;

    // Position inside the `name` input: name = "world"
    let response = signature_help_request(&mut ctx, "calls.wdl", Position::new(38, 9))
        .await
        .expect("request should succeed");
    let help = response.expect("should have a signature help response");

    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(0));

    let sig_info = &help.signatures[0];
    assert_eq!(
        sig_info.label,
        "task greet(name: String, <punctuation: String>, <loud: Boolean?>)"
    );
    assert!(sig_info.documentation.is_some());
    assert_eq!(sig_info.parameters.as_ref().unwrap().len(), 3);

    let params = sig_info.parameters.as_ref().unwrap();
    assert_eq!(params[0].label, ParameterLabel::LabelOffsets([11, 23]));
    assert_eq!(params[1].label, ParameterLabel::LabelOffsets([26, 45]));
    assert_eq!(params[2].label, ParameterLabel::LabelOffsets([49, 63]));

    let Some(async_lsp::lsp_types::Documentation::MarkupContent(documentation)) =
        &params[0].documentation
    else {
        panic!("first parameter should have documentation");
    };
    assert_eq!(documentation.value, "The name of the person to greet");

    assert!(params[2].documentation.is_none());
}

#[tokio::test]
async fn should_highlight_active_input_parameter() {
    let mut ctx = setup().await;

    // Position inside the `punctuation` input
    let response = signature_help_request(&mut ctx, "calls.wdl", Position::new(39, 8))
        .await
        .expect("request should succeed");
    let help = response.expect("should have a signature help response");

    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(1));

    let params = &help.signatures[0].parameters.as_ref().unwrap();
    let Some(async_lsp::lsp_types::Documentation::MarkupContent(documentation)) =
        &params[1].documentation
    else {
        panic!("second parameter should have documentation");
    };
    assert_eq!(documentation.value, "An optional punctuation mark");
}

#[tokio::test]
async fn should_provide_signature_help_for_namespaced_call() {
    let mut ctx = setup().await;

    // Position inside the `file` input of `call lib.lib_task`
    let response = signature_help_request(&mut ctx, "calls.wdl", Position::new(43, 10))
        .await
        .expect("request should succeed");
    let help = response.expect("should have a signature help response");

    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(0));

    let sig_info = &help.signatures[0];
    assert_eq!(sig_info.label, "task lib_task(file: File, <flag: Boolean>)");
    assert_eq!(sig_info.parameters.as_ref().unwrap().len(), 2);

    let params = sig_info.parameters.as_ref().unwrap();
    assert_eq!(params[0].label, ParameterLabel::LabelOffsets([14, 24]));
    assert_eq!(params[1].label, ParameterLabel::LabelOffsets([27, 40]));

    let Some(async_lsp::lsp_types::Documentation::MarkupContent(documentation)) =
        &params[0].documentation
    else {
        panic!("first parameter should have documentation");
    };
    assert_eq!(documentation.value, "The input file");
}

#[tokio::test]
async fn should_provide_signature_help_for_local_workflow_call() {
    let mut ctx = setup().await;

    // Position inside the `values` input of `call accumulate`
    let response = signature_help_request(&mut ctx, "calls.wdl", Position::new(48, 9))
        .await
        .expect("request should succeed");
    let help = response.expect("should have a signature help response");

    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(0));

    let sig_info = &help.signatures[0];
    assert_eq!(
        sig_info.label,
        "workflow accumulate(values: Array[Int], <initial: Int>)"
    );
    assert_eq!(sig_info.parameters.as_ref().unwrap().len(), 2);

    let params = sig_info.parameters.as_ref().unwrap();
    assert_eq!(params[0].label, ParameterLabel::LabelOffsets([20, 38]));
    assert_eq!(params[1].label, ParameterLabel::LabelOffsets([41, 53]));

    let Some(async_lsp::lsp_types::Documentation::MarkupContent(documentation)) =
        &params[0].documentation
    else {
        panic!("first parameter should have documentation");
    };
    assert_eq!(documentation.value, "The values to accumulate");
}

#[tokio::test]
async fn should_not_provide_signature_help_outside_input_region() {
    let mut ctx = setup().await;

    // Position on the call target: call greet
    let response = signature_help_request(&mut ctx, "calls.wdl", Position::new(37, 11))
        .await
        .expect("request should succeed");
    assert!(response.is_none());
}

#[tokio::test]
async fn should_not_provide_signature_help_for_unresolved_target() {
    let mut ctx = setup().await;

    // Position inside the input of `call missing_task`
    let response = signature_help_request(&mut ctx, "calls.wdl", Position::new(52, 9))
        .await
        .expect("request should succeed");
    assert!(response.is_none());
}
