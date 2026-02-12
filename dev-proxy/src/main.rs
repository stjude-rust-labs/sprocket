//! Proxy binary for running development versions of `sprocket`.
//!
//! This simply sets up a temp directory with all of the development versions of
//! the `sprocket` components (e.g. `wdl-doc`), and run the `sprocket` binary,
//! pointing it to those components.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use tempfile::TempDir;
use tempfile::tempdir;

fn main() -> std::io::Result<()> {
    let tmpdir = setup_temp()?;

    let status = Command::new("cargo")
        .args(["run", "-p", "sprocket"])
        .args(std::env::args_os().skip(1))
        .env("SPROCKET_DATA_DIR", tmpdir.path())
        .envs(std::env::vars_os())
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .status()?;
    if !status.success() {
        drop(tmpdir);
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

/// Sets up a temp directory with all of the development versions of the
/// required components.
fn setup_temp() -> std::io::Result<TempDir> {
    const ARTIFACTS: &[&str] = &["wdl-doc"];

    let target = std::env::var("CARGO_TARGET_DIR").map_or_else(
        |_| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("target")
        },
        PathBuf::from,
    );
    let profile_dir = if cfg!(debug_assertions) {
        target.join("debug")
    } else {
        target.join("release")
    };

    let tmpdir = tempdir()?;
    let components_dir = tmpdir.path().join("components");
    std::fs::create_dir_all(&components_dir)?;

    for artifact in ARTIFACTS {
        std::fs::copy(profile_dir.join(artifact), components_dir.join(artifact))?;
    }

    Ok(tmpdir)
}
