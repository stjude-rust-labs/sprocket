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

    let project_root_manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("Cargo.toml");
    let status = Command::new("cargo")
        .args(["run", "--quiet", "-p", "sprocket", "--manifest-path"])
        .arg(project_root_manifest)
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
    // (package, binary)
    const ARTIFACTS: &[(&str, &str)] = &[("wdl-doc-bin", "wdl-doc")];

    let target = std::env::var("CARGO_TARGET_DIR").map_or_else(
        |_| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("target")
        },
        PathBuf::from,
    );

    let profile = if cfg!(debug_assertions) {
        "dev"
    } else {
        "release"
    };
    let profile_dir = if cfg!(debug_assertions) {
        target.join("debug")
    } else {
        target.join("release")
    };

    let tmpdir = tempdir()?;
    let components_dir = tmpdir.path().join("bin");
    std::fs::create_dir_all(&components_dir)?;

    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for (package, binary) in ARTIFACTS {
        let status = Command::new("cargo")
            .args(["build", "--quiet", "-p", package, "--bin", binary])
            .args(["--profile", profile])
            .current_dir(&project_root)
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .status()?;
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }

        std::fs::copy(profile_dir.join(binary), components_dir.join(binary))?;
    }

    Ok(tmpdir)
}
