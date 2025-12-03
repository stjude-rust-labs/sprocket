#![allow(missing_docs)]

use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use criterion::criterion_group;
use criterion::criterion_main;
use reqwest::StatusCode;
use tempfile::TempDir;
use tempfile::tempdir;
use zip::ZipArchive;

mod analysis;

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

criterion_group!(benches, analysis::bench);
criterion_main!(benches);
