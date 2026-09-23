//! Handler for `textDocument/codeLens` requests in Sprocket test YAML files.

use anyhow::Result;
use async_lsp::lsp_types::CodeLens;
use async_lsp::lsp_types::Range;
use line_index::LineIndex;
use sprocket_test_types::yaml::Spanned;
use url::Url;
use wdl_ast::Span;

use crate::proto::range_from_span;
use crate::server::Command;
use crate::server::ServerState;

/// Determine the range of a test target.
///
/// For the following test YAML:
///
/// ```yaml
/// some_entrypoint:
///   - name: some_test
/// ```
///
/// A range would be produced for the `some_entrypoint` and `some_test`
/// identifiers.
fn section_range(lines: &LineIndex, spanned: &Spanned<String>) -> Option<Range> {
    let span = spanned.0.defined.span();
    // `serde-saphyr` is guaranteed to provide byte information when parsing
    // from a string, which we always do.
    let start_byte = span.byte_offset().expect("byte info should be available");
    let len = span.byte_len().expect("byte info should be available");
    range_from_span(lines, Span::new(start_byte as usize, len as usize)).ok()
}

/// Computes the [`CodeLens`]es for the given Sprocket test YAML, if applicable.
///
/// Implementation of [`textDocument/codeLens`]
///
/// [`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_codeLens
pub async fn code_lens<S>(document: Url, state: &ServerState<S>) -> Result<Option<Vec<CodeLens>>> {
    let Ok(Some(test_yaml)) = state.test_yamls.ensure_parsed(document).await else {
        return Ok(None);
    };

    let Some(tests) = test_yaml.tests() else {
        return Ok(None);
    };

    let Some(associated_wdl) = test_yaml.associated_wdl() else {
        return Ok(None);
    };

    let mut lenses = Vec::new();
    for (target_name, tests) in &tests.targets {
        let Some(range) = section_range(&test_yaml.lines, target_name) else {
            continue;
        };

        lenses.push(CodeLens {
            range,
            command: Some(
                Command::TestEntrypoint {
                    source: associated_wdl.clone(),
                    target: target_name.0.value.to_string(),
                }
                .into(),
            ),
            data: None,
        });

        for test in tests {
            let Some(range) = section_range(&test_yaml.lines, &test.name) else {
                continue;
            };

            lenses.push(CodeLens {
                range,
                command: Some(
                    Command::TestIndividual {
                        source: associated_wdl.clone(),
                        target: target_name.0.value.to_string(),
                        filter: test.name.0.value.to_string(),
                    }
                    .into(),
                ),
                data: None,
            });
        }
    }

    if lenses.is_empty() {
        Ok(None)
    } else {
        Ok(Some(lenses))
    }
}
