//! Tests for logging with `sprocket run`.

use std::fs;
use std::process::Command;
use std::process::Stdio;

use tempfile::tempdir;

/// A test that ensures that `sprocket run` outputs logging to both stderr and
/// to `output.log`.
#[test]
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
fn logging() {
    let dir = tempdir().unwrap();

    // Create a simple hello world WDL file
    fs::write(
        dir.path().join("source.wdl"),
        r#"
version 1.3

task hello {
    command <<<
        echo 'hello world!'
    >>>

    output {
        String message = read_string(stdout())
    }
}
"#,
    )
    .unwrap();

    for (opt, level) in [("-vvv", " TRACE "), ("-vv", " DEBUG "), ("-v", " INFO ")] {
        // Spawn sprocket with the requested logging option
        let result = Command::new(env!("CARGO_BIN_EXE_sprocket"))
            .args(["run", "-s", "source.wdl", "-t", "hello", opt])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(dir.path())
            .env_remove("RUST_LOG")
            .env_remove("RUST_BACKTRACE")
            .spawn()
            .expect("failed to spawn command")
            .wait_with_output()
            .expect("failed while waiting for command to finish");

        assert!(
            result.status.success(),
            "command failed {status}: {stderr}",
            status = result.status,
            stderr = str::from_utf8(&result.stderr).unwrap_or("<not UTF-8>")
        );
        assert_eq!(
            str::from_utf8(&result.stdout).unwrap(),
            "{\n  \"hello.message\": \"hello world!\"\n}\n"
        );

        // Ensure stderr has at least one message at the level
        assert!(str::from_utf8(&result.stderr).unwrap().contains(level));

        // Find the run directory (`out/runs/hello/<timestamp>/`) and ensure
        // the log file has at least one message at the level
        let runs_dir = dir.path().join("out").join("runs").join("hello");
        let run_dir = fs::read_dir(&runs_dir)
            .expect("should have runs directory")
            .filter_map(|e| e.ok())
            .find(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .expect("should have at least one run directory");
        let log = fs::read_to_string(run_dir.path().join("output.log"))
            .expect("should have output log");
        assert!(log.contains(level));
    }
}
