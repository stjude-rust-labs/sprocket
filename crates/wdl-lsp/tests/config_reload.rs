//! Integration tests for hot-reloading `sprocket.toml`-derived configuration.
//!
//! These exercise [`wdl_lsp::ServerOptions::reload_config`], which is invoked
//! whenever a file matching [`wdl_lsp::ServerOptions::config_filename`] (by
//! default `sprocket.toml`) is created, changed, or deleted in a watched
//! workspace folder. See <https://github.com/stjude-rust-labs/sprocket/issues/1009>.

pub mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use async_lsp::lsp_types::DidChangeWatchedFilesParams;
use async_lsp::lsp_types::FileChangeType;
use async_lsp::lsp_types::FileEvent;
use async_lsp::lsp_types::WorkspaceDiagnosticReportResult;
use async_lsp::lsp_types::WorkspaceDocumentDiagnosticReport;
use async_lsp::lsp_types::notification::DidChangeWatchedFiles;
use wdl_lint::Baseline;
use wdl_lint::BaselineEntry;
use wdl_lsp::ConfigReload;
use wdl_lsp::LintOptions;
use wdl_lsp::ServerOptions;

use crate::common::TestContext;

/// Extracts all diagnostic rule codes from a workspace diagnostic report.
fn diagnostic_codes(report: &WorkspaceDiagnosticReportResult) -> Vec<String> {
    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        return Vec::new();
    };

    report
        .items
        .iter()
        .flat_map(|item| match item {
            WorkspaceDocumentDiagnosticReport::Full(full) => {
                full.full_document_diagnostic_report.items.clone()
            }
            WorkspaceDocumentDiagnosticReport::Unchanged(_) => Vec::new(),
        })
        .filter_map(|d| match d.code {
            Some(async_lsp::lsp_types::NumberOrString::String(s)) => Some(s),
            _ => None,
        })
        .collect()
}

/// Sends a `workspace/didChangeWatchedFiles` notification reporting that the
/// given file (by name, relative to the workspace root) changed on disk.
fn notify_file_changed(ctx: &mut TestContext, name: &str, typ: FileChangeType) {
    ctx.notify::<DidChangeWatchedFiles>(DidChangeWatchedFilesParams {
        changes: vec![FileEvent {
            uri: ctx.doc_uri(name),
            typ,
        }],
    })
    .expect("notification should succeed");
}

/// A default [`ConfigReload`] with linting enabled and no exceptions or
/// baseline, matching the "everything is reported" defaults used elsewhere in
/// these tests.
fn default_reload() -> ConfigReload {
    ConfigReload {
        lint: LintOptions {
            enabled: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A [`wdl_lsp::UserOptions`] with linting enabled, matching the "everything
/// is reported" defaults used elsewhere in these tests.
fn lint_enabled_user_options() -> wdl_lsp::UserOptions {
    wdl_lsp::UserOptions {
        lint: wdl_lsp::LintOptions {
            enabled: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn sprocket_toml_change_reloads_exceptions() {
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_closure = calls.clone();

    let mut ctx = TestContext::with_options(
        "baseline",
        ServerOptions {
            // Start out excepting `UnusedInput`, simulating a `sprocket.toml`
            // with `analyzer.except = ["UnusedInput"]`.
            exceptions: vec![String::from("UnusedInput")],
            reload_config: Some(Arc::new(move || {
                calls_for_closure.fetch_add(1, Ordering::SeqCst);
                // Simulate the on-disk `sprocket.toml` no longer excepting
                // anything.
                Ok(default_reload())
            })),
            ..Default::default()
        },
        lint_enabled_user_options(),
    );

    let (_, report) = ctx.initialize().await;
    let codes = diagnostic_codes(&report);
    assert!(
        !codes.contains(&"UnusedInput".to_string()),
        "`UnusedInput` should initially be excepted; got: {codes:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    notify_file_changed(&mut ctx, "sprocket.toml", FileChangeType::CHANGED);

    let report = ctx.workspace_diagnostic().await;
    let codes = diagnostic_codes(&report);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "reload_config should have been invoked exactly once"
    );
    assert!(
        codes.contains(&"UnusedInput".to_string()),
        "`UnusedInput` should be reported after the exception is removed; got: {codes:?}"
    );
}

#[tokio::test]
async fn sprocket_toml_creation_and_deletion_also_reload() {
    let calls = Arc::new(AtomicUsize::new(0));

    for typ in [FileChangeType::CREATED, FileChangeType::DELETED] {
        let calls_for_closure = calls.clone();
        let mut ctx = TestContext::with_options(
            "baseline",
            ServerOptions {
                reload_config: Some(Arc::new(move || {
                    calls_for_closure.fetch_add(1, Ordering::SeqCst);
                    Ok(default_reload())
                })),
                ..Default::default()
            },
            lint_enabled_user_options(),
        );

        ctx.initialize().await;
        notify_file_changed(&mut ctx, "sprocket.toml", typ);
        ctx.workspace_diagnostic().await;
    }

    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "both creation and deletion of `sprocket.toml` should trigger a reload"
    );
}

#[tokio::test]
async fn unrelated_file_change_does_not_reload_config() {
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_closure = calls.clone();

    let mut ctx = TestContext::with_options(
        "baseline",
        ServerOptions {
            exceptions: vec![String::from("UnusedInput")],
            reload_config: Some(Arc::new(move || {
                calls_for_closure.fetch_add(1, Ordering::SeqCst);
                Ok(default_reload())
            })),
            ..Default::default()
        },
        lint_enabled_user_options(),
    );

    ctx.initialize().await;
    notify_file_changed(&mut ctx, "notes.txt", FileChangeType::CHANGED);
    let report = ctx.workspace_diagnostic().await;
    let codes = diagnostic_codes(&report);

    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "changes to unrelated files must not trigger a configuration reload"
    );
    assert!(
        !codes.contains(&"UnusedInput".to_string()),
        "the original exception should remain in effect; got: {codes:?}"
    );
}

#[tokio::test]
async fn sprocket_toml_change_reloads_baseline() {
    let mut ctx = TestContext::with_options_fn("baseline", |workspace| {
        let workspace = workspace.to_path_buf();
        (
            ServerOptions {
                reload_config: Some(Arc::new(move || {
                    let hash = blake3::hash(b"x").to_hex();
                    let baseline = Baseline::new(vec![
                        BaselineEntry::new("InputName", "source.wdl", hash),
                        BaselineEntry::new("UnusedInput", "source.wdl", hash),
                    ])
                    .with_base_dir(workspace.clone());

                    Ok(ConfigReload {
                        baseline: Some(baseline),
                        ..default_reload()
                    })
                })),
                ..Default::default()
            },
            lint_enabled_user_options(),
        )
    });

    let (_, report) = ctx.initialize().await;
    let codes = diagnostic_codes(&report);
    assert!(
        codes.contains(&"InputName".to_string()) && codes.contains(&"UnusedInput".to_string()),
        "diagnostics should be unfiltered before any baseline is configured; got: {codes:?}"
    );

    notify_file_changed(&mut ctx, "sprocket.toml", FileChangeType::CHANGED);

    let report = ctx.workspace_diagnostic().await;
    let codes = diagnostic_codes(&report);
    assert!(
        codes.contains(&"MetaSections".to_string()),
        "`MetaSections` should not be suppressed by the reloaded baseline; got: {codes:?}"
    );
    assert!(
        !codes.contains(&"InputName".to_string()) && !codes.contains(&"UnusedInput".to_string()),
        "the reloaded baseline should suppress `InputName`/`UnusedInput`; got: {codes:?}"
    );
}

#[tokio::test]
async fn reload_error_leaves_previous_config_in_effect() {
    let mut ctx = TestContext::with_options(
        "baseline",
        ServerOptions {
            exceptions: vec![String::from("UnusedInput")],
            reload_config: Some(Arc::new(|| {
                anyhow::bail!("simulated failure reading `sprocket.toml`")
            })),
            ..Default::default()
        },
        lint_enabled_user_options(),
    );

    let (_, report) = ctx.initialize().await;
    let codes = diagnostic_codes(&report);
    assert!(!codes.contains(&"UnusedInput".to_string()));

    notify_file_changed(&mut ctx, "sprocket.toml", FileChangeType::CHANGED);

    // The server should still be alive and functioning, applying the
    // previous (unchanged) configuration.
    let report = ctx.workspace_diagnostic().await;
    let codes = diagnostic_codes(&report);
    assert!(
        !codes.contains(&"UnusedInput".to_string()),
        "a failed reload should leave the previous exceptions in effect; got: {codes:?}"
    );
}
