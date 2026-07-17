//! The WDL format tests.
//!
//! This test looks for files in `tests/format`.

use std::fs;
use std::path::Path;

use anyhow::Context;
use wdl_analysis::Analyzer;
use wdl_analysis::Config;
use wdl_analysis::FormatConfig;
use wdl_analysis::path_to_uri;
use wdl_format::Indent;

/// Normalizes a result.
fn normalize(s: &str) -> String {
    // Just normalize line endings
    s.replace("\r\n", "\n")
}

#[tokio::test]
async fn spaces() {
    let dir = Path::new("tests").join("format");
    let two_space_wdl_path = dir.join("two_spaces.wdl");
    let four_space_wdl_path = dir.join("four_spaces.wdl");

    let two_space_wdl = fs::read_to_string(&two_space_wdl_path).expect("cannot read two space wdl");
    let four_space_wdl =
        fs::read_to_string(&four_space_wdl_path).expect("cannot read four space wdl");

    let two_space_format_config = FormatConfig::default().indent(Indent::Spaces(2));
    let four_space_format_config = FormatConfig::default();

    // 4 => 2
    format_wdl(
        &four_space_wdl_path,
        &two_space_wdl,
        two_space_format_config,
    )
    .await;

    // 2 => 2
    format_wdl(&two_space_wdl_path, &two_space_wdl, two_space_format_config).await;

    // 2 => 4 (default)
    format_wdl(
        &two_space_wdl_path,
        &four_space_wdl,
        four_space_format_config,
    )
    .await;

    // 4 => 4 (default)
    format_wdl(
        &four_space_wdl_path,
        &four_space_wdl,
        four_space_format_config,
    )
    .await;
}

async fn format_wdl(wdl: &Path, expected: &str, format_config: FormatConfig) {
    let config = Config::default().with_format_config(format_config);
    let analyzer: Analyzer<()> = Analyzer::new(config, |_, _, _, _| async {});
    let uri = path_to_uri(wdl).expect("should be valid URI");

    analyzer
        .add_document(uri.clone())
        .await
        .context("adding test document")
        .expect("should add test WDL");

    analyzer
        .analyze_document((), uri.clone())
        .await
        .context("analyzing document")
        .expect("should analyze WDL");

    let results = analyzer
        .format_document(uri)
        .await
        .context("formatting WDL")
        .expect("couldn't collect format results");

    let (_, _, formatted) = results.expect("couldn't collect results");
    assert_eq!(normalize(&formatted), normalize(expected));
}
