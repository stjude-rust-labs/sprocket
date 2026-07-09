//! Implementation of the `retry` subcommand.

use std::collections::BTreeMap;

use anyhow::Context;
use clap::Parser;
use serde_json::Value as JsonValue;
use wdl::analysis::Document;
use wdl::diagnostics::Mode;
use wdl::engine::EvaluationPath;
use wdl::engine::Inputs as EngineInputs;

use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::client::ServerConnectionArgs;
use crate::commands::client::get_json;
use crate::commands::client::resolve_run_id;
use crate::commands::client::send_json;
use crate::commands::run::inputs_to_json;
use crate::commands::validate::analyze_source;
use crate::commands::validate::ensure_no_analysis_errors;
use crate::config::Config;
use crate::inputs::Invocation;
use crate::inputs::join_paths_for_target;
use crate::server::RunResponse;
use crate::server::SubmitRunRequest;
use crate::server::paths;

/// Arguments for the `retry` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The run to retry.
    ///
    /// May be a UUID or the human-readable generated name of the run (e.g.
    /// `happy-dolphin-42`). The original run's source, target, and inputs are
    /// reused as the base for the new submission.
    #[clap(value_name = "RUN")]
    run_id: String,

    /// Input overrides for the new run.
    ///
    /// Overrides use the same syntax as `dev server submit`: key-value pairs
    /// (e.g. `task.name=value`), input files prefixed with `@` (e.g.
    /// `@inputs.json`), or bare values appended to the preceding key's array.
    /// Any key provided here takes precedence over the value from the original
    /// run.
    pub overrides: Vec<String>,

    /// Override the target task or workflow name.
    #[clap(short, long, value_name = "NAME")]
    target: Option<String>,

    /// Override the output name to index on.
    #[clap(long, value_name = "OUTPUT_NAME")]
    index_on: Option<String>,

    /// Skip local re-analysis of the WDL source file.
    ///
    /// By default, `retry` re-analyzes the source file to catch errors before
    /// submitting. Use this flag if the source file is no longer accessible
    /// from the client. When this flag is set, override `File`/`Directory`
    /// values are not path-resolved.
    #[clap(long)]
    no_validate: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    report_mode: Option<Mode>,

    #[command(flatten)]
    client_args: ServerConnectionArgs,
}

/// Handles the `retry` subcommand.
///
/// Fetches the original run's details, optionally re-analyzes the source,
/// merges any input overrides, then submits a new run.
pub async fn retry(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let base_url = args.client_args.base_url(&config);
    let uuid = resolve_run_id(&args.run_id, &base_url).await?;

    // Fetch the original run.
    let url = format!("{base_url}{path}", path = paths::get_run(uuid));
    let body: RunResponse = get_json(&url, "run").await?;

    let original = &body.run;

    // Resolve the effective target: CLI override > stored target > None.
    let effective_target = args.target.clone().or_else(|| original.target.clone());

    // Parse the original inputs JSON. Keys are stored with the target prefix
    // (e.g. `align.read_one_fastq_gz`); keep them as-is — that is the format
    // expected by `SubmitRunRequest.inputs` and by the server's parser.
    let mut merged_inputs: serde_json::Map<String, JsonValue> =
        serde_json::from_str(&original.inputs)
            .context("failed to deserialize stored run inputs")?;

    // Parse the source string into an analysis `Source`.
    let source: Source = original
        .source
        .parse()
        .with_context(|| format!("failed to parse source `{}`", original.source))?;

    // Re-analyze the WDL source locally unless --no-validate is set. The
    // resulting `Document` is also used to drive path resolution on
    // override `File`/`Directory` values.
    let document = if !args.no_validate {
        let document = analyze_source(
            &source,
            config.common.wdl.fallback_version.into(),
            config.modules.clone(),
            config.common.wdl.feature_flags,
        )
        .await
        .map_err(|e| {
            // Wrap with a hint to use --no-validate if the file is unreachable.
            match e {
                CommandError::Single(inner) => CommandError::Single(inner.context(format!(
                    "cannot re-analyze source `{source}`; use --no-validate to skip local analysis",
                    source = original.source
                ))),
                other => other,
            }
        })?;

        ensure_no_analysis_errors(&document, args.report_mode.unwrap_or_default(), colorize)?;

        Some(document)
    } else {
        None
    };

    // Apply overrides on top of the stored inputs.
    //
    // The override syntax mirrors `dev server submit`: `key=value` pairs,
    // `@file` to load a JSON/YAML inputs file, and bare values that append
    // to the preceding key's array. Paths in overrides are resolved against
    // the re-analyzed WDL document; when `--no-validate` is set, override
    // values are passed through unchanged.
    //
    // Completeness validation is still skipped here: the stored inputs were
    // already validated at original submit time, and validating a partial
    // override set against the WDL would fail on every untouched required
    // input. The server validates the merged value set at execution time.
    merge_overrides_into(
        &mut merged_inputs,
        &args.overrides,
        effective_target.clone(),
        document.as_ref(),
    )
    .await?;

    // Submit the new run.
    let submit_url = format!("{base_url}{path}", path = paths::SUBMIT_RUN);
    let request = SubmitRunRequest {
        source: original.source.clone(),
        inputs: JsonValue::Object(merged_inputs),
        target: effective_target,
        index_on: args.index_on,
    };

    let submit_response: JsonValue = send_json(
        reqwest::Client::new().post(&submit_url).json(&request),
        "retry submission",
    )
    .await?;

    println!(
        "{}",
        serde_json::to_string_pretty(&submit_response)
            .context("failed to pretty-print response")?
    );

    Ok(())
}

/// Parses retry override CLI arguments and merges them into `base`.
///
/// `base` is the stored input set from the original run (already absolute
/// paths). `overrides` is the raw CLI override slice. `target` is the
/// effective target (CLI override, or the original run's target). `document`
/// is the (optionally re-analyzed) WDL document; when `None`, path
/// resolution is skipped and override values are passed through unchanged
/// (matches `--no-validate` semantics).
///
/// Override syntax mirrors `dev server submit`: `key=value`, `@file` to load
/// JSON/YAML inputs, bare values that append to the preceding key's array.
/// Per-key, the override replaces the stored value entirely (override arrays
/// do not append to stored arrays).
async fn merge_overrides_into(
    base: &mut serde_json::Map<String, JsonValue>,
    overrides: &[String],
    target: Option<String>,
    document: Option<&Document>,
) -> CommandResult<()> {
    if overrides.is_empty() {
        return Ok(());
    }

    let invocation = Invocation::coalesce(overrides, target)
        .await
        .context("failed to parse override inputs")?;

    let (origins, override_map) = invocation.into_json_with_origins();

    if override_map.is_empty() {
        return Ok(());
    }

    // Resolve paths via wdl-engine when a document is available.
    let resolved_map = if let Some(document) = document {
        resolve_override_paths(document, override_map, origins).await?
    } else {
        override_map
    };

    for (key, value) in resolved_map {
        base.insert(key, value);
    }

    Ok(())
}

/// Round-trips an override JSON map through `EngineInputs::parse_json_object`,
/// `join_paths_for_target`, and `inputs_to_json` to resolve relative
/// `File`/`Directory` paths in override values to absolute paths.
///
/// `parse_json_object` accepts a partial input set (it does not error on
/// missing required inputs; that check lives in `validate()`, which we
/// intentionally skip for the override case). Path resolution iterates only
/// over keys present in the typed inputs, so it works correctly with a
/// subset.
async fn resolve_override_paths(
    document: &Document,
    overrides: serde_json::Map<String, JsonValue>,
    origins: BTreeMap<String, Vec<EvaluationPath>>,
) -> CommandResult<serde_json::Map<String, JsonValue>> {
    let Some((target, mut inputs)) =
        EngineInputs::parse_json_object(document, overrides.clone())
            .context("failed to parse override inputs against the WDL document")?
    else {
        return Ok(overrides);
    };

    join_paths_for_target(document, &target, &mut inputs, &origins).await?;

    let json_str =
        inputs_to_json(&target, &inputs).context("failed to serialize override inputs")?;
    let resolved: serde_json::Map<String, JsonValue> =
        serde_json::from_str(&json_str).context("failed to deserialize override inputs")?;
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::Path;

    use serde_json::json;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;

    use super::*;
    use crate::analysis::Source;
    use crate::commands::validate::analyze_source;

    /// Builds an empty base input map.
    fn base() -> serde_json::Map<String, JsonValue> {
        serde_json::Map::new()
    }

    /// Builds a string vec from a slice of `&str` for the overrides arg.
    fn overrides(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    /// Writes the given WDL source to a tempdir and returns the analyzed
    /// document along with the tempdir (to keep it alive).
    async fn analyze(wdl: &str) -> (Document, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("source.wdl");
        std::fs::write(&path, wdl).unwrap();
        let source: Source = path.to_str().unwrap().parse().unwrap();
        let document = analyze_source(
            &source,
            None,
            wdl_modules::resolver::ModulesConfig::default(),
            wdl::analysis::FeatureFlags::default(),
        )
        .await
        .unwrap();
        (document, dir)
    }

    #[tokio::test]
    async fn merge_no_overrides_is_noop() {
        let mut base = base();
        base.insert("wf.x".to_string(), json!(1));
        merge_overrides_into(&mut base, &[], Some("wf".to_string()), None)
            .await
            .unwrap();
        assert_eq!(base["wf.x"], json!(1));
        assert_eq!(base.len(), 1);
    }

    #[tokio::test]
    async fn merge_key_value_pair() {
        let mut base = base();
        base.insert("wf.foo".to_string(), json!("old"));
        merge_overrides_into(
            &mut base,
            &overrides(&["wf.foo=42"]),
            Some("wf".to_string()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(base["wf.foo"], json!(42));
    }

    #[tokio::test]
    async fn merge_target_prefix_idempotent() {
        // When target is `wf` and the user types `wf.foo=1`, the key should
        // stay as `wf.foo` (not become `wf.wf.foo`).
        let mut base = base();
        merge_overrides_into(
            &mut base,
            &overrides(&["wf.foo=1"]),
            Some("wf".to_string()),
            None,
        )
        .await
        .unwrap();
        assert!(base.contains_key("wf.foo"));
        assert!(!base.contains_key("wf.wf.foo"));
    }

    #[tokio::test]
    async fn merge_repeated_key_collects_into_array() {
        let mut base = base();
        merge_overrides_into(
            &mut base,
            &overrides(&["wf.files=a.txt", "wf.files=b.txt"]),
            Some("wf".to_string()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(base["wf.files"], json!(["a.txt", "b.txt"]));
    }

    #[tokio::test]
    async fn merge_bare_args_append_to_last_key() {
        let mut base = base();
        merge_overrides_into(
            &mut base,
            &overrides(&["wf.files=a.txt", "b.txt", "c.txt"]),
            Some("wf".to_string()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(base["wf.files"], json!(["a.txt", "b.txt", "c.txt"]));
    }

    #[tokio::test]
    async fn merge_at_file() {
        // Write a temporary inputs file with two bare keys.
        let mut file = NamedTempFile::new().unwrap();
        write!(file, r#"{{"foo": "bar", "count": 42}}"#).unwrap();
        let path = file.path().to_path_buf();
        // Rename to a .json extension so the loader accepts it.
        let renamed = path.with_extension("json");
        std::fs::rename(&path, &renamed).unwrap();

        let mut base = base();
        let arg = format!("@{}", renamed.display());
        merge_overrides_into(&mut base, &overrides(&[&arg]), Some("wf".to_string()), None)
            .await
            .unwrap();
        assert_eq!(base["wf.foo"], json!("bar"));
        assert_eq!(base["wf.count"], json!(42));

        // Cleanup.
        let _ = std::fs::remove_file(&renamed);
    }

    #[tokio::test]
    async fn merge_file_array_extended_by_cli() {
        // @file contributes an array, CLI appends additional elements.
        let mut file = NamedTempFile::new().unwrap();
        write!(file, r#"{{"files": ["a.txt", "b.txt"]}}"#).unwrap();
        let path = file.path().to_path_buf();
        let renamed = path.with_extension("json");
        std::fs::rename(&path, &renamed).unwrap();

        let mut base = base();
        let file_arg = format!("@{}", renamed.display());
        merge_overrides_into(
            &mut base,
            &overrides(&[&file_arg, "wf.files=c.txt"]),
            Some("wf".to_string()),
            None,
        )
        .await
        .unwrap();
        // The file contributed an array `[a.txt, b.txt]`; the CLI added a
        // scalar `c.txt`. The flattener spreads the file's array, then
        // appends the CLI scalar.
        assert_eq!(base["wf.files"], json!(["a.txt", "b.txt", "c.txt"]));

        let _ = std::fs::remove_file(&renamed);
    }

    #[tokio::test]
    async fn merge_path_resolution_against_document() {
        // Tiny WDL with a single File input.
        let wdl = r#"
version 1.2

task t {
    input {
        File f
    }
    command <<< >>>
}
"#;
        let (document, _wdl_dir) = analyze(wdl).await;

        // Create a real input file so existence-check in resolve_paths
        // succeeds. Use the canonicalized absolute path so the test is
        // independent of CWD.
        let file_dir = TempDir::new().unwrap();
        let input_path = file_dir.path().join("input.txt");
        std::fs::write(&input_path, "hello").unwrap();
        let input_path_canon = input_path.canonicalize().unwrap();
        let input_path_str = input_path_canon.to_str().unwrap();

        let mut base = base();
        merge_overrides_into(
            &mut base,
            &overrides(&[&format!("t.f={input_path_str}")]),
            Some("t".to_string()),
            Some(&document),
        )
        .await
        .unwrap();

        // The resolved value should be the input path (already absolute, so
        // resolution is a pass-through but it confirms the round-trip
        // through parse_json_object + join_paths + inputs_to_json works
        // end-to-end).
        let resolved = base["t.f"].as_str().unwrap();
        let resolved_canon = Path::new(resolved).canonicalize().unwrap();
        assert_eq!(resolved_canon, input_path_canon);
    }

    #[tokio::test]
    async fn merge_skips_path_resolution_when_no_document() {
        // Without a document, override values pass through verbatim,
        // including relative paths.
        let mut base = base();
        merge_overrides_into(
            &mut base,
            &overrides(&["t.f=./relative/path.txt"]),
            Some("t".to_string()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(base["t.f"], json!("./relative/path.txt"));
    }

    #[tokio::test]
    async fn merge_unknown_override_key_errors() {
        let wdl = r#"
version 1.2

task t {
    input {
        File f
    }
    command <<< >>>
}
"#;
        let (document, _wdl_dir) = analyze(wdl).await;

        let mut base = base();
        let result = merge_overrides_into(
            &mut base,
            &overrides(&["t.does_not_exist=1"]),
            Some("t".to_string()),
            Some(&document),
        )
        .await;

        assert!(result.is_err(), "expected error for unknown override key");
    }
}
