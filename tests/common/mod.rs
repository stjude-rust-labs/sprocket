//! Common helpers shared across `sprocket` integration test targets.
//!
//! Kept in `tests/common/mod.rs` rather than `tests/common.rs` so Cargo does
//! not compile it as a standalone integration test binary.

use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use uuid::Uuid;

/// Assigns a unique `LLVM_PROFILE_FILE` path to a spawned `sprocket` child
/// command when running under LLVM source-based coverage (such as
/// `cargo-llvm-cov`).
///
/// On Windows, the kernel aggressively recycles PIDs across the hundreds of
/// short-lived test processes spawned by nextest and CLI integration tests.
/// When a `sprocket` child process receives a recycled PID matching an earlier
/// `.profraw` file under `cargo-llvm-cov`'s `sprocket-%p-%<N>m.profraw`
/// pattern, LLVM's profile runtime attempts an in-process merge and can emit
/// `LLVM Profile Warning` / `LLVM Profile Error` lines to `stderr`, breaking
/// golden `stderr` assertions and dropping coverage output for that process.
///
/// Appending a per-command UUID keeps each child process's `.profraw` file
/// distinct inside the existing `cargo-llvm-cov` profile directory so both
/// `cargo llvm-cov` report merging and `cargo llvm-cov clean` still find it.
pub(crate) fn configure_child_llvm_profile(command: &mut Command) {
    let Some(raw) = env::var_os("LLVM_PROFILE_FILE") else {
        return;
    };
    let Ok(cwd) = env::current_dir() else {
        return;
    };
    if let Some(profile_file) = child_llvm_profile_file(&raw, &cwd, Uuid::new_v4()) {
        command.env("LLVM_PROFILE_FILE", profile_file);
    }
}

/// Computes a per-child `LLVM_PROFILE_FILE` path from an inherited value.
///
/// Returns `None` when `llvm_profile_file` is empty or does not end with a
/// `.profraw` extension (for example `/dev/null` or `NUL`), leaving special
/// destinations untouched. Relative directories without `%` substitutions (such
/// as `%t`) are anchored to `cwd` so changing `Command::current_dir` does not
/// redirect profile output into a test's temporary working directory.
pub(crate) fn child_llvm_profile_file(
    llvm_profile_file: &OsStr,
    cwd: &Path,
    id: Uuid,
) -> Option<PathBuf> {
    if llvm_profile_file.is_empty() {
        return None;
    }

    let path = Path::new(llvm_profile_file);
    let ext = path.extension()?;
    if !ext.eq_ignore_ascii_case("profraw") {
        return None;
    }

    let stem = path.file_stem()?;
    if stem.is_empty() {
        return None;
    }

    let mut file_name = stem.to_os_string();
    file_name.push(format!("-{id}."));
    file_name.push(ext);

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let dir = if parent.as_os_str().is_empty() {
        cwd.to_path_buf()
    } else if parent.is_absolute() || parent.as_os_str().as_encoded_bytes().contains(&b'%') {
        parent.to_path_buf()
    } else {
        cwd.join(parent)
    };

    Some(dir.join(file_name))
}
