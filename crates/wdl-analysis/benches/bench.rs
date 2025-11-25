#![allow(missing_docs)]

use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use reqwest::StatusCode;
use tempfile::TempDir;
use tempfile::tempdir;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::Config as AnalysisConfig;
use zip::ZipArchive;

#[derive(Debug, Clone)]
struct AnalyzeWorkflows {
    /// Root dir of the `workflows` repo to run the benchmark upon.
    repo_root: PathBuf,
    /// A specific number of worker threads for the Tokio runtime.
    ///
    /// `None` means to let the runtime use the default number of threads.
    worker_threads: Option<usize>,
    /// A specific max number of blocking threads for the Tokio runtime.
    ///
    /// `None` means to let the runtime use the default number of threads.
    blocking_threads: Option<usize>,
}

impl AnalyzeWorkflows {
    /// Create a new Tokio runtime with the given parameters, and run the
    /// analysis on the entire `workflows` repo.
    fn analyze(&self) -> Vec<AnalysisResult> {
        let mut runtime_builder = tokio::runtime::Builder::new_multi_thread();
        if let Some(worker_threads) = self.worker_threads {
            runtime_builder.worker_threads(worker_threads);
        }
        if let Some(max_blocking_threads) = self.blocking_threads {
            runtime_builder.max_blocking_threads(max_blocking_threads);
        }
        let runtime = runtime_builder.enable_all().build().unwrap();
        runtime.block_on(async {
            let config = AnalysisConfig::default();
            let analyzer = Analyzer::new(config, |_, _, _, _| async {});
            analyzer.add_directory(&self.repo_root).await.unwrap();
            analyzer.analyze(()).await.unwrap()
        })
    }
}

/// Download a copy of the `workflows` repo to a temporary directory under
/// `/target`.
///
/// This doesn't do any validation on the resulting file, just that the HTTP
/// status code is `200 OK` and that the body bytes get copied out without
/// erroring. If something weird starts happening like unzip errors, a `cargo
/// clean` should make it retry the download. Don't do stuff like this in
/// production!
fn download_workflows_zip() -> Result<PathBuf, anyhow::Error> {
    const WORKFLOWS_REPO_HASH: &str = "415a7e21e9c64f3f11eae0c1cf2cd0bea9b33ef0";
    let workflows_zip_path =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("workflows-{WORKFLOWS_REPO_HASH}.zip"));
    if !workflows_zip_path.exists() {
        let url =
            format!("https://github.com/stjudecloud/workflows/archive/{WORKFLOWS_REPO_HASH}.zip");
        println!(
            "workflows repo zip not found at {}, downloading from {url}",
            workflows_zip_path.display()
        );
        let mut workflows_resp = reqwest::blocking::get(url)?;
        assert_eq!(
            workflows_resp.status(),
            StatusCode::OK,
            "workflows zip downloaded successfully"
        );
        let mut workflows_zip = std::fs::File::create_new(&workflows_zip_path)?;
        workflows_resp.copy_to(&mut workflows_zip)?;
        println!("workflows repo zip download successful");
    } else {
        println!(
            "workflows repo zip already exists at {}, skipping download",
            workflows_zip_path.display()
        );
    }
    Ok(workflows_zip_path)
}

fn get_workflows_repo() -> Result<TempDir, anyhow::Error> {
    let workflows_zip_path = download_workflows_zip()?;
    let mut zip = ZipArchive::new(File::open(workflows_zip_path)?)?;
    let tempdir = tempdir()?;
    zip.extract_unwrapped_root_dir(tempdir.path(), zip::read::root_dir_common_filter)?;
    Ok(tempdir)
}

fn bench_analyze_workflows(c: &mut Criterion) {
    let workflows_repo = get_workflows_repo().unwrap();
    {
        let mut workers_group = c.benchmark_group("analyze_workflows_with_worker_threads");
        for worker_threads in 1..std::thread::available_parallelism().unwrap().get() {
            let analyze = AnalyzeWorkflows {
                repo_root: workflows_repo.path().to_path_buf(),
                worker_threads: Some(worker_threads),
                blocking_threads: None,
            };
            workers_group.bench_with_input(worker_threads.to_string(), &analyze, |b, analyze| {
                b.iter(|| analyze.analyze());
            });
        }
    }
    {
        let mut blocking_group = c.benchmark_group("analyze_workflows_with_blocking_threads");
        for blocking_threads_exponent in 0..10 {
            let blocking_threads = 2usize.pow(blocking_threads_exponent);
            let analyze = AnalyzeWorkflows {
                repo_root: workflows_repo.path().to_path_buf(),
                worker_threads: Some(1),
                blocking_threads: Some(blocking_threads),
            };
            blocking_group.bench_with_input(
                blocking_threads.to_string(),
                &analyze,
                |b, analyze| {
                    b.iter(|| analyze.analyze());
                },
            );
        }
    }
    drop(workflows_repo);
}

criterion_group!(benches, bench_analyze_workflows);
criterion_main!(benches);
