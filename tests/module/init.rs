//! Integration tests for `sprocket dev module init` and related scaffolding
//! checks.

use std::fs;

use wdl_modules::Manifest;

use crate::fixtures::*;

#[test]
fn init_scaffolds_a_parseable_module() {
    let dir = tempfile::tempdir().unwrap();
    let output = sprocket(&["dev", "module", "init", "--name", "demo"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run sprocket");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read(dir.path().join("module.json")).unwrap();
    Manifest::parse(&manifest).expect("scaffold parses");
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(value["description"], "The `demo` WDL module.");
    assert!(value.get("version").is_none());
    assert!(dir.path().join("index.wdl").exists());
    assert!(dir.path().join("README.md").exists());

    assert!(!dir.path().join(".gitignore").exists());
}

#[test]
fn directory_module_entrypoint_does_not_require_wdl_1_4() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("module.json"),
        r#"{"name":"demo","license":"MIT","entrypoint":"main.wdl"}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.wdl"),
        "version 1.3\nworkflow wf {\n  input {\n    String name\n  }\n}\n",
    )
    .unwrap();

    let module_arg = dir.path().to_string_lossy().into_owned();
    let output = sprocket(&["inputs", &module_arg])
        .output()
        .expect("failed to run sprocket inputs");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"wf.name\""));
}

#[test]
fn init_preserves_existing_scaffold_files() {
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.wdl");
    let readme = dir.path().join("README.md");
    let gitignore = dir.path().join(".gitignore");
    fs::write(&index, "version 1.0\n").unwrap();
    fs::write(&readme, "# custom\n").unwrap();
    fs::write(&gitignore, "target/\n").unwrap();

    let output = sprocket(&["dev", "module", "init", "--name", "demo"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run sprocket");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(fs::read_to_string(&index).unwrap(), "version 1.0\n");
    assert_eq!(fs::read_to_string(&readme).unwrap(), "# custom\n");
    assert_eq!(fs::read_to_string(&gitignore).unwrap(), "target/\n");

    assert!(String::from_utf8_lossy(&output.stdout).contains("Initialized module `demo`"));
}

#[test]
fn module_commands_reject_missing_manifest_path() {
    let dir = tempfile::tempdir().unwrap();
    // A valid module in the current directory must not be silently used
    // when `--manifest-path` points somewhere that does not exist.
    fs::write(
        dir.path().join("module.json"),
        r#"{"name":"demo","license":"MIT"}"#,
    )
    .unwrap();
    let missing = dir.path().join("nope").join("module.json");
    let missing = missing.to_string_lossy().into_owned();
    let output = sprocket(&["dev", "module", "list", "--manifest-path", &missing])
        .current_dir(dir.path())
        .output()
        .expect("failed to run sprocket");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {stderr}");
}
