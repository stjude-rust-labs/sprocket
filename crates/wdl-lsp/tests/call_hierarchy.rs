//! Integration tests for call hierarchy requests.

use async_lsp::lsp_types::CallHierarchyIncomingCall;
use async_lsp::lsp_types::CallHierarchyIncomingCallsParams;
use async_lsp::lsp_types::CallHierarchyItem;
use async_lsp::lsp_types::CallHierarchyOutgoingCall;
use async_lsp::lsp_types::CallHierarchyOutgoingCallsParams;
use async_lsp::lsp_types::CallHierarchyPrepareParams;
use async_lsp::lsp_types::Position;
use async_lsp::lsp_types::Range;
use async_lsp::lsp_types::SymbolKind;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::TextDocumentPositionParams;
use async_lsp::lsp_types::request::CallHierarchyIncomingCalls;
use async_lsp::lsp_types::request::CallHierarchyOutgoingCalls;
use async_lsp::lsp_types::request::CallHierarchyPrepare;

use crate::common::TestContext;

pub mod common;

// textDocument/prepareCallHierarchy
async fn call_hierarchy_request(
    ctx: &mut TestContext,
    path: &str,
    position: Position,
) -> async_lsp::Result<Option<Vec<CallHierarchyItem>>> {
    ctx.request::<CallHierarchyPrepare>(CallHierarchyPrepareParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: ctx.doc_uri(path),
            },
            position,
        },
        work_done_progress_params: Default::default(),
    })
    .await
}

// callHierarchy/incomingCalls
async fn incoming_calls_request(
    ctx: &mut TestContext,
    item: CallHierarchyItem,
) -> async_lsp::Result<Option<Vec<CallHierarchyIncomingCall>>> {
    ctx.request::<CallHierarchyIncomingCalls>(CallHierarchyIncomingCallsParams {
        item,
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
}

// callHierarchy/outgoingCalls
async fn outgoing_calls_request(
    ctx: &mut TestContext,
    item: CallHierarchyItem,
) -> async_lsp::Result<Option<Vec<CallHierarchyOutgoingCall>>> {
    ctx.request::<CallHierarchyOutgoingCalls>(CallHierarchyOutgoingCallsParams {
        item,
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
}

async fn setup() -> TestContext {
    let mut ctx = TestContext::new("call_hierarchy");
    ctx.initialize().await;
    ctx
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedCallHierarchyItem {
    name: String,
    kind: SymbolKind,
}

impl ExpectedCallHierarchyItem {
    fn first() -> Self {
        ExpectedCallHierarchyItem {
            name: String::from("first"),
            kind: SymbolKind::METHOD,
        }
    }

    fn second() -> Self {
        ExpectedCallHierarchyItem {
            name: String::from("second"),
            kind: SymbolKind::FUNCTION,
        }
    }

    fn third() -> Self {
        ExpectedCallHierarchyItem {
            name: String::from("third"),
            kind: SymbolKind::FUNCTION,
        }
    }

    fn all_together() -> Self {
        ExpectedCallHierarchyItem {
            name: String::from("all_together"),
            kind: SymbolKind::FUNCTION,
        }
    }
}

impl PartialEq<CallHierarchyItem> for ExpectedCallHierarchyItem {
    fn eq(&self, other: &CallHierarchyItem) -> bool {
        self.name == other.name && self.kind == other.kind
    }
}

fn verify_call_hierarchy(
    mut expected: Vec<ExpectedCallHierarchyItem>,
    received: &[CallHierarchyItem],
) {
    for item in received {
        let matched = expected.iter().position(|expected| expected == item);

        if let Some(index) = matched {
            expected.remove(index);
        } else {
            panic!("unexpected call hierarchy item returned: {item:?}");
        }
    }

    assert!(
        expected.is_empty(),
        "some expected items were not returned: {expected:?}"
    );
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedCallHierarchyOutgoingCall {
    to: ExpectedCallHierarchyItem,
    from_ranges: Vec<Range>,
}

fn verify_outgoing_calls(
    mut expected: Vec<ExpectedCallHierarchyOutgoingCall>,
    received: &[CallHierarchyOutgoingCall],
) {
    for call in received {
        let matched = expected.iter().position(|expected| {
            expected.to == call.to
                && expected.from_ranges.len() == call.from_ranges.len()
                && expected
                    .from_ranges
                    .iter()
                    .all(|expected_range| call.from_ranges.contains(expected_range))
        });

        if let Some(index) = matched {
            expected.remove(index);
        } else {
            panic!("unexpected outgoing call returned: {call:?}");
        }
    }

    assert!(
        expected.is_empty(),
        "some expected items were not returned: {expected:?}"
    );
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedCallHierarchyIncomingCall {
    from: ExpectedCallHierarchyItem,
    from_ranges: Vec<Range>,
}

fn verify_incoming_calls(
    mut expected: Vec<ExpectedCallHierarchyIncomingCall>,
    received: &[CallHierarchyIncomingCall],
) {
    for call in received {
        let matched = expected.iter().position(|expected| {
            expected.from == call.from && expected.from_ranges == call.from_ranges
        });

        if let Some(index) = matched {
            expected.remove(index);
        } else {
            panic!("unexpected incoming call returned: {call:?}");
        }
    }

    assert!(
        expected.is_empty(),
        "some expected items were not returned: {expected:?}"
    );
}

#[tokio::test]
async fn should_prepare_call_hierarchy() {
    let mut ctx = setup().await;

    for (file, item, pos) in [
        (
            "first.wdl",
            ExpectedCallHierarchyItem::first(),
            Position::new(2, 6),
        ),
        (
            "second.wdl",
            ExpectedCallHierarchyItem::second(),
            Position::new(4, 10),
        ),
        (
            "third.wdl",
            ExpectedCallHierarchyItem::third(),
            Position::new(4, 10),
        ),
        (
            "source.wdl",
            ExpectedCallHierarchyItem::all_together(),
            Position::new(6, 10),
        ),
    ] {
        let Some(hierarchy) = call_hierarchy_request(&mut ctx, file, pos)
            .await
            .expect("request should succeed")
        else {
            panic!("expected call hierarchy in {file}")
        };

        verify_call_hierarchy(vec![item], &hierarchy);
    }
}

#[tokio::test]
async fn should_not_prepare_call_hierarchy() {
    let mut ctx = setup().await;

    assert!(
        call_hierarchy_request(
            &mut ctx,
            "second.wdl",
            // Some random position
            Position::new(5, 0)
        )
        .await
        .expect("request should succeed")
        .is_none()
    );
}

#[tokio::test]
async fn should_determine_outgoing_calls() {
    let mut ctx = setup().await;
    let Some(mut hierarchy) = call_hierarchy_request(&mut ctx, "source.wdl", Position::new(6, 10))
        .await
        .expect("request should succeed")
    else {
        panic!("expected call hierarchy")
    };

    verify_call_hierarchy(vec![ExpectedCallHierarchyItem::all_together()], &hierarchy);

    let item = hierarchy.remove(0);
    let Some(outgoing_calls) = outgoing_calls_request(&mut ctx, item)
        .await
        .expect("request should succeed")
    else {
        panic!("expected outgoing calls");
    };

    verify_outgoing_calls(
        vec![
            ExpectedCallHierarchyOutgoingCall {
                to: ExpectedCallHierarchyItem::first(),
                from_ranges: vec![Range {
                    start: Position::new(7, 15),
                    end: Position::new(7, 20),
                }],
            },
            ExpectedCallHierarchyOutgoingCall {
                to: ExpectedCallHierarchyItem::second(),
                from_ranges: vec![Range {
                    start: Position::new(8, 16),
                    end: Position::new(8, 22),
                }],
            },
            ExpectedCallHierarchyOutgoingCall {
                to: ExpectedCallHierarchyItem::third(),
                // Aliased calls are consolidated
                from_ranges: vec![
                    Range {
                        start: Position::new(9, 15),
                        end: Position::new(9, 20),
                    },
                    Range {
                        start: Position::new(10, 24),
                        end: Position::new(10, 30),
                    },
                ],
            },
        ],
        &outgoing_calls,
    );
}

#[tokio::test]
async fn should_determine_incoming_calls() {
    let mut ctx = setup().await;
    let Some(mut hierarchy) = call_hierarchy_request(&mut ctx, "first.wdl", Position::new(2, 6))
        .await
        .expect("request should succeed")
    else {
        panic!("expected call hierarchy")
    };

    verify_call_hierarchy(vec![ExpectedCallHierarchyItem::first()], &hierarchy);

    let item = hierarchy.remove(0);
    let Some(incoming_calls) = incoming_calls_request(&mut ctx, item)
        .await
        .expect("request should succeed")
    else {
        panic!("should return incoming calls")
    };

    verify_incoming_calls(
        vec![
            ExpectedCallHierarchyIncomingCall {
                from: ExpectedCallHierarchyItem::second(),
                from_ranges: vec![Range {
                    start: Position::new(5, 15),
                    end: Position::new(5, 20),
                }],
            },
            ExpectedCallHierarchyIncomingCall {
                from: ExpectedCallHierarchyItem::all_together(),
                from_ranges: vec![Range {
                    start: Position::new(7, 15),
                    end: Position::new(7, 20),
                }],
            },
        ],
        &incoming_calls,
    );
}
