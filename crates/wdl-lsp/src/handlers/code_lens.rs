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
use crate::test::Document;

/// Get the WDL file path associated with a test definition YAML file.
///
/// A test YAML file *must* have an associated WDL file, otherwise we don't
/// consider it valid.
///
/// See [`is_sprocket_test_file()`](crate::test::is_sprocket_test_file)
fn associated_wdl_file_path(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let base_name = path.file_stem()?;
    let expected_wdl = std::path::Path::new(base_name).with_extension("wdl");
    let parent = path.parent()?;

    let in_test_dir =
        parent.is_dir() && parent.file_name().and_then(|s| s.to_str()) == Some("test");
    let wdl_dir = if in_test_dir {
        parent.parent()?
    } else {
        parent
    };

    Some(wdl_dir.join(expected_wdl))
}

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

    let Some(Document::Parsed(tests)) = test_yaml.document.as_ref() else {
        return Ok(None);
    };

    let associated_wdl = match associated_wdl_file_path(&test_yaml.path)
        .and_then(|path| Url::from_file_path(path).ok())
    {
        Some(wdl) => wdl,
        None => {
            return Ok(None);
        }
    };

    let mut lenses = Vec::new();
    for (target_name, tests) in &tests.0.targets {
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
