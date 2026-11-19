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
    crate::common::configure_child_llvm_profile(&mut command);
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

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use uuid::Uuid;

    use crate::common::child_llvm_profile_file;

    #[test]
    fn rewrites_llvm_profile_file_with_unique_id() {
        let cwd = if cfg!(windows) {
            Path::new(r"C:\workspace\sprocket")
        } else {
            Path::new("/workspace/sprocket")
        };
        let abs_dir = if cfg!(windows) {
            Path::new(r"D:\a\sprocket\target\llvm-cov-target")
        } else {
            Path::new("/tmp/sprocket/target/llvm-cov-target")
        };
        let id = Uuid::from_u128(0x01234567_89AB_CDEF_0123_456789ABCDEF);

        assert_eq!(child_llvm_profile_file(OsStr::new(""), cwd, id), None);
        assert_eq!(
            child_llvm_profile_file(OsStr::new("/dev/null"), cwd, id),
            None
        );
        assert_eq!(child_llvm_profile_file(OsStr::new("NUL"), cwd, id), None);

        let cargo_llvm_cov_input = abs_dir.join("sprocket-%p-%4m.profraw");
        assert_eq!(
            child_llvm_profile_file(cargo_llvm_cov_input.as_os_str(), cwd, id),
            Some(abs_dir.join(format!("sprocket-%p-%4m-{id}.profraw")))
        );

        assert_eq!(
            child_llvm_profile_file(OsStr::new("sprocket-%p-%c.profraw"), cwd, id),
            Some(cwd.join(format!("sprocket-%p-%c-{id}.profraw")))
        );

        assert_eq!(
            child_llvm_profile_file(OsStr::new("profiles/sprocket-%p.profraw"), cwd, id),
            Some(
                cwd.join("profiles")
                    .join(format!("sprocket-%p-{id}.profraw"))
            )
        );

        assert_eq!(
            child_llvm_profile_file(OsStr::new("%t/sprocket-%p-%m.profraw"), cwd, id),
            Some(Path::new("%t").join(format!("sprocket-%p-%m-{id}.profraw")))
        );
    }

    #[test]
    fn child_sprocket_writes_isolated_profile_outside_working_dir() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path().join("workdir");
        let prof_dir = temp.path().join("profraw");
        fs::create_dir_all(&work_dir).unwrap();
        fs::create_dir_all(&prof_dir).unwrap();

        let base_pattern = Path::new("profraw").join("sprocket-%p-%m.profraw");
        for _ in 0..2 {
            let profile_file =
                child_llvm_profile_file(base_pattern.as_os_str(), temp.path(), Uuid::new_v4())
                    .expect("expected rewritten profile path");
            let output = Command::new(env!("CARGO_BIN_EXE_sprocket"))
                .arg("--version")
                .current_dir(&work_dir)
                .env("LLVM_PROFILE_FILE", &profile_file)
                .output()
                .expect("failed to run sprocket --version");

            assert!(
                output.status.success(),
                "sprocket --version failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(String::from_utf8_lossy(&output.stderr), "");
        }

        // Changing `current_dir` must never spill `.profraw` files into
        // `work_dir`.
        assert_eq!(fs::read_dir(&work_dir).unwrap().count(), 0);

        // When compiled with `-C instrument-coverage` (e.g. `cargo llvm-cov`),
        // each invocation writes its own distinct `.profraw` file in
        // `prof_dir`.
        let profraw_files = fs::read_dir(&prof_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert!(
            profraw_files.is_empty() || profraw_files.len() == 2,
            "expected 0 (uninstrumented) or 2 (instrumented) .profraw files, found: \
             {profraw_files:?}"
        );
    }
}
