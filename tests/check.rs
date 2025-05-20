use std::path::PathBuf;

use assert_cmd::{Command, assert::Assert};

fn get_test_file_path(file_name: &str) -> String {
    let mut source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    source_file.push("tests/inputs");
    source_file.push(file_name);
    source_file.as_path().to_str().unwrap().to_string()
}

fn call_sprocket<I, S>(inputs: I) -> Assert
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::cargo_bin("sprocket").unwrap();
    command.args(inputs);

    command.assert().success()
}

#[test]
fn can_check_empty_file() {
    let output = call_sprocket(&["check", &get_test_file_path("empty.wdl")]);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert_eq!("", stdout);
    assert_eq!("", stderr);
}

#[test]
fn can_check_unused_input() {
    let output = call_sprocket(&["check", &get_test_file_path("unused-input.wdl")]);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert_eq!("", stdout);
    assert!(stderr.contains("warning[UnusedInput]: unused input `x`"));
}

#[test]
fn can_skip_unused_input_check() {
    let output = call_sprocket(&[
        "check",
        "--except",
        "UnusedInput",
        &get_test_file_path("unused-input.wdl"),
    ]);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert_eq!("", stdout);
    assert_eq!("", stderr);
}

#[test]
fn can_skip_unused_input_check_key_case_insensitive() {
    let output = call_sprocket(&[
        "check",
        "--except",
        "UnUsEdInPuT",
        &get_test_file_path("unused-input.wdl"),
    ]);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert_eq!("", stdout);
    assert_eq!("", stderr);
}
