//! Command helpers for module integration tests.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::OnceLock;

pub(crate) fn sprocket(args: &[&str]) -> Command {
    sprocket_with_global_args(&[], args)
}

/// A per-process temporary configuration root shared by every spawned
/// `sprocket` command.
///
/// The `sprocket` binary resolves its trust store and module cache under
/// `SPROCKET_CONFIG_ROOT` (see `sprocket::config::config_root`). Pointing that
/// at a fresh temporary directory keeps tests from touching (or racing on) the
/// developer's or CI runner's real Sprocket config directory. Under nextest
/// each test runs in its own process, so this yields full per-test isolation;
/// under plain `cargo test` it is a single fresh directory per run, which is
/// still strictly better than the real config directory. The `TempDir` is kept
/// alive for the lifetime of the test process so the directory is not deleted
/// out from under running commands.
pub(crate) static SHARED_CONFIG_ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();

pub(crate) fn sprocket_with_global_args(global_args: &[&str], args: &[&str]) -> Command {
    let config_root = SHARED_CONFIG_ROOT
        .get_or_init(|| tempfile::tempdir().expect("failed to create shared config root"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_sprocket"));
    command
        .arg("--skip-config-search")
        .args(global_args)
        .args(args)
        .env("SPROCKET_CONFIG_ROOT", config_root.path())
        .env("RUST_LOG", "none")
        .env_remove("RUST_BACKTRACE");
    command
}

pub(crate) fn sprocket_with_config(config_path: &Path, args: &[&str]) -> Command {
    let config_path = config_path.to_string_lossy().into_owned();
    sprocket_with_global_args(&["--config", &config_path], args)
}

pub(crate) fn output_with_stdin(mut command: Command, input: &str) -> std::process::Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn sprocket");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(input.as_bytes())
        .expect("failed to write prompt input");
    child
        .wait_with_output()
        .expect("failed to wait on sprocket")
}

pub(crate) fn isolated_home(base: &Path, name: &str) -> PathBuf {
    let home = base.join(name);
    fs::create_dir_all(&home).unwrap();
    home
}

pub(crate) fn use_home(command: &mut Command, home: &Path) {
    // Also override the config root so home-isolated tests keep their own trust
    // store and cache under `$HOME/.config/sprocket`. This re-sets the
    // `SPROCKET_CONFIG_ROOT` set in `sprocket_with_global_args`; a later `.env`
    // for the same key overrides the earlier one for `std::process::Command`.
    command.env("HOME", home).env("USERPROFILE", home).env(
        "SPROCKET_CONFIG_ROOT",
        home.join(".config").join("sprocket"),
    );
}
