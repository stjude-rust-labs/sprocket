#![allow(missing_docs)]

use std::ffi::OsStr;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use criterion::BenchmarkGroup;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use criterion::measurement::Measurement;
use reqwest::StatusCode;
use tempfile::TempDir;
use tempfile::tempdir;
use url::Url;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::Config as AnalysisConfig;
use zip::ZipArchive;

#[derive(Debug)]
struct AnalyzeWorkflows {
    /// Root dir of the `workflows` repo to run the benchmark upon.
    repo_root: PathBuf,
    /// The tokio runtime for performing the analysis.
    runtime: tokio::runtime::Runtime,
}

impl AnalyzeWorkflows {
    fn new(
        repo_root: impl AsRef<Path>,
        worker_threads: Option<usize>,
        blocking_threads: Option<usize>,
    ) -> Self {
        let mut runtime_builder = tokio::runtime::Builder::new_multi_thread();
        if let Some(worker_threads) = worker_threads {
            runtime_builder.worker_threads(worker_threads);
        }
        if let Some(max_blocking_threads) = blocking_threads {
            runtime_builder.max_blocking_threads(max_blocking_threads);
        }
        let runtime = runtime_builder.enable_all().build().unwrap();
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            runtime,
        }
    }

    /// Create a new Tokio runtime with the given parameters, and run the
    /// analysis on the entire `workflows` repo.
    fn analyze_all(&self) -> Vec<AnalysisResult> {
        self.runtime.block_on(async {
            let config = AnalysisConfig::default();
            let analyzer = Analyzer::new(config, |_, _, _, _| async {});
            analyzer.add_directory(&self.repo_root).await.unwrap();
            analyzer.analyze(()).await.unwrap()
        })
    }

    /// Create a new Tokio runtime with the given parameters, and run the
    /// analysis on a single document within the `workflows` repo specified
    /// by relative path.
    fn analyze_document(&self, path: impl AsRef<Path>) -> Vec<AnalysisResult> {
        assert!(path.as_ref().is_relative());
        self.runtime.block_on(async {
            let config = AnalysisConfig::default();
            let analyzer = Analyzer::new(config, |_, _, _, _| async {});
            let document = Url::from_file_path(self.repo_root.join(path)).unwrap();
            analyzer.add_document(document).await.unwrap();
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

fn bench_analyze_workflows_document<M: Measurement>(
    group: &mut BenchmarkGroup<'_, M>,
    repo_root: impl AsRef<Path>,
    path: impl AsRef<Path>,
) {
    let analyze = AnalyzeWorkflows::new(repo_root, None, None);
    group.bench_function(&path.as_ref().display().to_string(), |b| {
        let path = path.as_ref();
        b.iter(|| analyze.analyze_document(path))
    });
}

fn bench_analyze_workflows(c: &mut Criterion) {
    let workflows_repo = get_workflows_repo().unwrap();
    {
        let mut workers_group = c.benchmark_group("analyze_workflows_with_worker_threads");
        for worker_threads in 1..=std::thread::available_parallelism().unwrap().get() {
            let analyze = AnalyzeWorkflows::new(&workflows_repo, Some(worker_threads), None);
            workers_group.bench_with_input(worker_threads.to_string(), &analyze, |b, analyze| {
                b.iter(|| analyze.analyze_all());
            });
        }
    }
    {
        let mut blocking_group = c.benchmark_group("analyze_workflows_with_blocking_threads");
        for blocking_threads_exponent in 0..10 {
            let blocking_threads = 2usize.pow(blocking_threads_exponent);
            let analyze = AnalyzeWorkflows::new(&workflows_repo, Some(1), Some(blocking_threads));
            blocking_group.bench_with_input(
                blocking_threads.to_string(),
                &analyze,
                |b, analyze| {
                    b.iter(|| analyze.analyze_all());
                },
            );
        }
    }
    let mut standalone_documents = c.benchmark_group("standalone_documents");
    for entry in walkdir::WalkDir::new(workflows_repo.path()) {
        if let Ok(e) = entry
            && e.path().extension() == Some(OsStr::new("wdl"))
        {
            let relative = e.path().strip_prefix(workflows_repo.path()).unwrap();
            bench_analyze_workflows_document(
                &mut standalone_documents,
                workflows_repo.path(),
                relative,
            );
        }
    }
    drop(workflows_repo);
}

criterion_group!(benches, bench_analyze_workflows);
criterion_main!(benches);
