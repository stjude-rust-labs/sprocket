use std::path::PathBuf;

use assert_cmd::Command;

#[test]
fn can_check_empty_file() {
    let mut source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    source_file.push("tests/inputs/empty.wdl");
    let mut command = Command::cargo_bin("sprocket").unwrap();

    command.args(["check", source_file.as_path().to_str().unwrap()]);

    let output = command.assert().success();
    assert_eq!("", String::from_utf8(output.get_output().stdout.clone()).unwrap());
    assert_eq!("", String::from_utf8(output.get_output().stderr.clone()).unwrap());
}

#[test]
fn can_check_unused_input() {
    let mut source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    source_file.push("tests/inputs/unused-input.wdl");
    let mut command = Command::cargo_bin("sprocket").unwrap();

    command.args(["check", source_file.as_path().to_str().unwrap()]);

    let output = command.assert().success();
    assert_eq!("", String::from_utf8(output.get_output().stdout.clone()).unwrap());
    assert!(String::from_utf8(output.get_output().stderr.clone()).unwrap().contains("warning[UnusedInput]: unused input `x`"));
}


#[test]
fn can_skip_unused_input_check() {
    let mut source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    source_file.push("tests/inputs/unused-input.wdl");
    let mut command = Command::cargo_bin("sprocket").unwrap();

    command.args(["check", "--except", "UnusedInput", source_file.as_path().to_str().unwrap()]);

    let output = command.assert().success();
    assert_eq!("", String::from_utf8(output.get_output().stdout.clone()).unwrap());
    assert_eq!("", String::from_utf8(output.get_output().stderr.clone()).unwrap());
}


#[test]
fn can_skip_unused_input_check_key_case_insensitive() {
    let mut source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    source_file.push("tests/inputs/unused-input.wdl");
    let mut command = Command::cargo_bin("sprocket").unwrap();

    command.args(["check", "--except", "UnUsEdInPuT", source_file.as_path().to_str().unwrap()]);

    let output = command.assert().success();
    assert_eq!("", String::from_utf8(output.get_output().stdout.clone()).unwrap());
    assert_eq!("", String::from_utf8(output.get_output().stderr.clone()).unwrap());
}
