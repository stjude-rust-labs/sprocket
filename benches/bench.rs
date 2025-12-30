//! Sprocket + `wdl-*` microbenchmark suite.
//!
//! This is the entrypoint for a [`criterion`]-based benchmark suite. See the
//! [Criterion docs](https://bheisler.github.io/criterion.rs/book/criterion_rs.html) for a more detailed
//! description of the framework.
//!
//! Microbenchmark suites like this are well-suited for measuring small routines
//! that run on the order of milliseconds. Criterion can provide results for
//! longer operations, but its strength is statistical analysis and performance
//! regression detection for small operations.
//!
//! This suite is not run in CI, in large part due to the performance
//! unpredictability of CI test runners. It's possible this may be run on
//! private instances in the future, but for the moment the best use is for
//! local comparisons of performance before and after a change.
//!
//! # Basic usage
//!
//! Run `cargo bench --bench bench` (ha) to run all of the benchmark targets in
//! the suite and compare them against the previous local run.
//!
//! Run `cargo bench --bench bench -- --help` to see the various options
//! available from the `criterion` entrypoint.
//!
//! # Comparing performance against named baselines
//!
//! A suite like this is particularly helpful for differentiating performance
//! between two branches, or against a baseline while working on a change. While
//! `criterion` automatically compares against the previous local run, using a
//! named baseline can help avoid unnecessarily rerunning the suite on, for
//! example, the current `main` branch while doing development on a feature
//! branch:
//!
//! ```bash
//! git checkout main
//! cargo bench --bench bench -- --save-baseline main
//! ```
//!
//! You should see a full set of benchmark output without any comparisons to a
//! previous baseline. These results are now saved and can be used for
//! comparison with the `--baseline` option:
//!
//! ```bash
//! git checkout myfeature
//! cargo bench --bench bench -- --baseline main
//! ```
//!
//! You should now see a full set of new benchmarks with statistical comparisons
//! against the baseline saved as `main`. Running the full suite can take a long
//! time, so for quick development iteration, you can run a subset of the
//! benches, but still get the comparisons against the `main` baseline:
//!
//! ```bash
//! cargo bench --bench bench -- --baseline main tools/star.wdl
//! ```
//!
//! # Viewing reports
//!
//! After running any of the benchmarks, open
//! `target/criterion/report/index.html` for links to summaries and plots for
//! the various benchmarks. If you have recently run benchmarks more than once,
//! these reports will include a comparison against the previous run. The plots
//! are great fodder for slides!
//!
//! # Implementation status
//!
//! As of 2025-12-03, this is a starting point for a larger suite, and is not
//! meant to provide a comprehensive picture of our performance. The suite is
//! divided into smaller categories, with some top-level helper functions for
//! pulling a snapshot of the [`stjudecloud/workflows`](https://github.com/stjudecloud/workflows) repository for the sake of
//! using its realistic WDL documents.

use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use criterion::criterion_main;
use reqwest::StatusCode;
use tempfile::TempDir;
use tempfile::tempdir;
use zip::ZipArchive;

mod analysis;
mod sprocket;

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

/// Get a [`TempDir`] containing the unzipped contents of the `workflows` repo,
/// downloading the zip archive if needed.
fn get_workflows_repo() -> Result<TempDir, anyhow::Error> {
    let workflows_zip_path = download_workflows_zip()?;
    let mut zip = ZipArchive::new(File::open(workflows_zip_path)?)?;
    let tempdir = tempdir()?;
    zip.extract_unwrapped_root_dir(tempdir.path(), zip::read::root_dir_common_filter)?;
    Ok(tempdir)
}

/// A short inline module gives us a place to hang this `allow` attribute, so
/// that we don't end up with a missing-docs warning for macro-generated code we
/// don't control.
#[allow(missing_docs)]
mod benches {
    criterion::criterion_group!(benches, crate::analysis::bench, crate::sprocket::bench);
}
criterion_main!(benches::benches);
