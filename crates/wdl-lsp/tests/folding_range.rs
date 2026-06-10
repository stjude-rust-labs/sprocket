//! Tests for folding range support in the LSP.

use async_lsp::lsp_types::FoldingRange;
use async_lsp::lsp_types::FoldingRangeKind;
use async_lsp::lsp_types::FoldingRangeParams;
use async_lsp::lsp_types::TextDocumentIdentifier;
use async_lsp::lsp_types::request::FoldingRangeRequest;
use wdl_analysis::handlers::BRACED_COLLAPSED_TEXT;
use wdl_analysis::handlers::DOLLAR_PLACEHOLDER_COLLAPSED_TEXT;
use wdl_analysis::handlers::HEREDOC_COLLAPSED_TEXT;
use wdl_analysis::handlers::TILDE_PLACEHOLDER_COLLAPSED_TEXT;

use crate::common::TestContext;

pub mod common;

async fn folding_range_request(
    ctx: &mut TestContext,
    path: &str,
) -> async_lsp::Result<Option<Vec<FoldingRange>>> {
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

    let ranges = folding_range_request(&mut ctx, "source.wdl")
        .await
        .expect("request should succeed")
        .unwrap();

    let mut expected_ranges = vec![
        // Imports
        FoldingRange {
            start_line: 3,
            start_character: Some(0),
            end_line: 4,
            end_character: Some(16),
            kind: Some(FoldingRangeKind::Imports),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 6,
            start_character: Some(0),
            end_line: 7,
            end_character: Some(16),
            kind: Some(FoldingRangeKind::Imports),
            collapsed_text: None,
        },
        // Line comments
        FoldingRange {
            start_line: 9,
            start_character: Some(0),
            end_line: 13,
            end_character: Some(29),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 15,
            start_character: Some(0),
            end_line: 16,
            end_character: Some(33),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 43,
            start_character: Some(4),
            end_line: 44,
            end_character: Some(20),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 87,
            start_character: Some(0),
            end_line: 88,
            end_character: Some(7),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        // Doc comments
        FoldingRange {
            start_line: 18,
            start_character: Some(0),
            end_line: 19,
            end_character: Some(34),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 21,
            start_character: Some(0),
            end_line: 22,
            end_character: Some(16),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        FoldingRange {
            start_line: 46,
            start_character: Some(4),
            end_line: 47,
            end_character: Some(26),
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        },
        // Task body
        FoldingRange {
            start_line: 23,
            start_character: Some(9),
            end_line: 52,
            end_character: Some(1),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        FoldingRange {
            start_line: 54,
            start_character: Some(9),
            end_line: 71,
            end_character: Some(1),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Meta section
        FoldingRange {
            start_line: 24,
            start_character: Some(9),
            end_line: 26,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Parameter meta section
        FoldingRange {
            start_line: 28,
            start_character: Some(19),
            end_line: 30,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Command section
        FoldingRange {
            start_line: 32,
            start_character: Some(12),
            end_line: 41,
            end_character: Some(7),
            kind: None,
            collapsed_text: Some(HEREDOC_COLLAPSED_TEXT.into()),
        },
        FoldingRange {
            start_line: 55,
            start_character: Some(12),
            end_line: 62,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Placeholders
        FoldingRange {
            start_line: 35,
            start_character: Some(8),
            end_line: 40,
            end_character: Some(9),
            kind: None,
            collapsed_text: Some(TILDE_PLACEHOLDER_COLLAPSED_TEXT.into()),
        },
        FoldingRange {
            start_line: 56,
            start_character: Some(8),
            end_line: 61,
            end_character: Some(9),
            kind: None,
            collapsed_text: Some(DOLLAR_PLACEHOLDER_COLLAPSED_TEXT.into()),
        },
        // Requirements section
        FoldingRange {
            start_line: 49,
            start_character: Some(17),
            end_line: 51,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Hints section
        FoldingRange {
            start_line: 64,
            start_character: Some(10),
            end_line: 66,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Runtime section
        FoldingRange {
            start_line: 68,
            start_character: Some(12),
            end_line: 70,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Workflow body
        FoldingRange {
            start_line: 73,
            start_character: Some(12),
            end_line: 85,
            end_character: Some(1),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Input section
        FoldingRange {
            start_line: 74,
            start_character: Some(10),
            end_line: 76,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Workflow hints section
        FoldingRange {
            start_line: 78,
            start_character: Some(10),
            end_line: 80,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
        },
        // Output section
        FoldingRange {
            start_line: 82,
            start_character: Some(11),
            end_line: 84,
            end_character: Some(5),
            kind: None,
            collapsed_text: Some(BRACED_COLLAPSED_TEXT.into()),
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
