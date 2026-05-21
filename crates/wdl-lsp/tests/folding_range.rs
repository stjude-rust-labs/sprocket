//! Tests for folding range support in the LSP.

use tower_lsp::lsp_types::FoldingRange;
use tower_lsp::lsp_types::FoldingRangeKind;
use tower_lsp::lsp_types::FoldingRangeParams;
use tower_lsp::lsp_types::TextDocumentIdentifier;
use tower_lsp::lsp_types::request::FoldingRangeRequest;

use crate::common::TestContext;

mod common;

async fn folding_range_request(ctx: &mut TestContext, path: &str) -> Option<Vec<FoldingRange>> {
    ctx.request::<FoldingRangeRequest>(FoldingRangeParams {
        text_document: TextDocumentIdentifier {
            uri: ctx.doc_uri(path),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    })
    .await
}

#[tokio::test]
async fn should_fold_content() {
    let mut ctx = TestContext::new("folding_range");
    ctx.initialize().await;

    let ranges = folding_range_request(&mut ctx, "source.wdl").await.unwrap();

    let mut expected_ranges = vec![
        // Imports
        FoldingRange {
            start_line: 2,
            start_character: Some(0),
            end_line: 3,
            end_character: Some(16),
            kind: Some(FoldingRangeKind::Imports),
            collapsed_text: None,
        },
        // Line comments
        FoldingRange {
            start_line: 5,
            start_character: Some(0),
            end_line: 9,
            end_character: Some(29),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 11,
            start_character: Some(0),
            end_line: 12,
            end_character: Some(33),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 16,
            start_character: Some(0),
            end_line: 16,
            end_character: Some(24),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        // Doc comments
        FoldingRange {
            start_line: 14,
            start_character: Some(0),
            end_line: 15,
            end_character: Some(34),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 17,
            start_character: Some(0),
            end_line: 18,
            end_character: Some(16),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        // Task body
        FoldingRange {
            start_line: 19,
            start_character: Some(9),
            end_line: 31,
            end_character: Some(1),
            kind: None,
            collapsed_text: None,
        },
        // Meta section
        FoldingRange {
            start_line: 20,
            start_character: Some(9),
            end_line: 22,
            end_character: Some(5),
            kind: None,
            collapsed_text: None,
        },
        // Parameter meta section
        FoldingRange {
            start_line: 24,
            start_character: Some(19),
            end_line: 26,
            end_character: Some(5),
            kind: None,
            collapsed_text: None,
        },
        // Command section
        FoldingRange {
            start_line: 28,
            start_character: Some(12),
            end_line: 30,
            end_character: Some(7),
            kind: None,
            collapsed_text: None,
        },
    ];

    for range in ranges {
        let matched = expected_ranges.iter().position(|expected| {
            expected.start_line == range.start_line
                && expected.end_line == range.end_line
                && expected.start_character == range.start_character
                && expected.end_character == range.end_character
                && expected.kind == range.kind
        });

        if let Some(index) = matched {
            expected_ranges.remove(index);
        } else {
            panic!("Unexpected folding range returned: {range:?}");
        }
    }

    assert!(
        expected_ranges.is_empty(),
        "Some expected folding ranges were not returned: {expected_ranges:?}"
    );
}
