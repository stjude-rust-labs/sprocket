//! Benchmarks of `sprocket` command executions.

use std::ffi::OsStr;
use std::path::Path;

use criterion::Criterion;
use url::Url;

use crate::get_workflows_repo;

/// Benchmarks of `sprocket` command executions.
pub fn bench(c: &mut Criterion) {
    let workflows_repo = get_workflows_repo().unwrap();
    check_standalone_documents(c, workflows_repo.path());
    drop(workflows_repo);
}

/// Run `sprocket check` on each of the WDL files in the `workflows` repo.
fn check_standalone_documents(c: &mut Criterion, workflows_repo: &Path) {
    let mut standalone_documents = c.benchmark_group("check_standalone_documents");
    for entry in walkdir::WalkDir::new(workflows_repo) {
        if let Ok(e) = entry
            && e.path().extension() == Some(OsStr::new("wdl"))
            // work around a lack of `.sprocketignore` support for now
            && !e.path().to_string_lossy().contains("examples")
        {
            let file_url = Url::from_file_path(e.path()).unwrap();
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap();
            let relative = e.path().strip_prefix(workflows_repo).unwrap();
            standalone_documents.bench_function(relative.display().to_string(), move |b| {
                b.iter(|| {
                    let file_url = file_url.clone();
                    let common = sprocket::commands::check::Common {
                        sources: vec![sprocket::analysis::Source::File(file_url)],
                        except: vec![],
                        all_lint_rules: true,
                        filter_lint_tag: vec![],
                        only_lint_tag: vec![],
                        deny_warnings: false,
                        deny_notes: false,
                        suppress_imports: false,
                        show_remote_diagnostics: false,
                        hide_notes: true,
                        no_color: true,
                        report_mode: None,
                    };
                    let check_args = sprocket::commands::check::CheckArgs { common, lint: true };
                    runtime
                        .block_on(sprocket::commands::check::check(check_args, Default::default()))
                        .unwrap()
                })
            });
        }
    }
}
