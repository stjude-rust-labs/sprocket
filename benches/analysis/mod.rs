//! Benchmarks of `wdl-analysis` functions.

use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

use criterion::BenchmarkGroup;
use criterion::Criterion;
use criterion::measurement::Measurement;
use url::Url;
use wdl::analysis::AnalysisResult;
use wdl::analysis::Analyzer;
use wdl::analysis::Config as AnalysisConfig;

use crate::get_workflows_repo;

/// The configuration for running an analysis benchmark.
#[derive(Debug)]
struct AnalyzeWorkflows {
    /// Root dir of the `workflows` repo to run the benchmark upon.
    repo_root: PathBuf,
    /// The tokio runtime for performing the analysis.
    runtime: tokio::runtime::Runtime,
}

impl AnalyzeWorkflows {
    /// Set up an [`AnalyzeWorkflows`] with the given repo root and runtime
    /// settings.
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

/// Benchmark the analysis of a single document from the `workflows` repo.
fn bench_analyze_workflows_document<M: Measurement>(
    group: &mut BenchmarkGroup<'_, M>,
    repo_root: impl AsRef<Path>,
    path: impl AsRef<Path>,
) {
    let analyze = AnalyzeWorkflows::new(repo_root, None, None);
    group.bench_function(path.as_ref().display().to_string(), |b| {
        let path = path.as_ref();
        b.iter(|| analyze.analyze_document(path))
    });
}

/// Benchmarks of `wdl-analysis` functions.
pub fn bench(c: &mut Criterion) {
    let workflows_repo = get_workflows_repo().unwrap();
    // NOTE ACF 2025-12-03: these "analyze the whole repo" benchmarks are disabled
    // for now, as the inclusion of `import https://` statements means the runtime is dominated by fetching those
    // resources. In the future, if these dependencies change or some caching
    // mechanism is introduced for remote imports, this would be a decent metric
    // for "analyze as much real WDL as possible".
    if false {
        // Analyze the whole workflows repo with a varying number of Tokio worker
        // threads and the system default number of blocking threads.
        let mut workers_group = c.benchmark_group("analyze_workflows_with_worker_threads");
        for worker_threads in 1..=std::thread::available_parallelism().unwrap().get() {
            let analyze = AnalyzeWorkflows::new(&workflows_repo, Some(worker_threads), None);
            workers_group.bench_with_input(worker_threads.to_string(), &analyze, |b, analyze| {
                b.iter(|| analyze.analyze_all());
            });
        }
        workers_group.finish();
    }
    if false {
        // Analyze the whole workflows repo with a single worker thread and a varying
        // number of Tokio blocking threads.
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
        blocking_group.finish();
    }
    // Add a bench target to analyze each WDL file in the workflows repo.
    //
    // This includes WDL files that directly or transitively `import https://` documents. The timing
    // of these benchmarks should not be considered stable. The time to fetch remote
    // resources dominates the overall benchmark time, and can vary dramatically
    // based on whether they're run on wifi, with a VPN, etc.
    let mut standalone_documents = c.benchmark_group("analyze_standalone_documents");
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
