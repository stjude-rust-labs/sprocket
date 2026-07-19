//! Integration tests for `sprocket dev module init` and related scaffolding
//! checks.

use std::fs;

use git2::Repository;
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
fn init_creates_a_missing_target_directory() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("nested").join("module");
    let target_arg = target.to_string_lossy().into_owned();
    let output = sprocket(&["dev", "module", "init", &target_arg, "--name", "nested"]).output()?;

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.join("module.json").is_file());
    assert!(target.join("index.wdl").is_file());
    assert!(target.join("README.md").is_file());
    Ok(())
}

#[test]
fn init_invalid_manifest_does_not_create_target_directory() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("invalid");
    let target_arg = target.to_string_lossy().into_owned();
    let output = sprocket(&["dev", "module", "init", &target_arg, "--license", "foo"]).output()?;

    assert!(!output.status.success());
    assert!(!target.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid SPDX license expression"));
    Ok(())
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

#[test]
fn init_configuration_overrides_git_identity() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = Repository::init(dir.path())?;
    let mut git = repo.config()?;
    git.set_str("user.name", "Git Author")?;
    git.set_str("user.email", "git@example.com")?;
    let config = dir.path().join("sprocket.toml");
    fs::write(
        &config,
        "[module.init]\nauthor = \"Configured Author\"\nemail = \"configured@example.com\"\n",
    )?;

    let output = sprocket_with_config(&config, &["dev", "module", "init", "--name", "demo"])
        .current_dir(dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("module.json"))?)?;
    assert_eq!(
        value["authors"],
        serde_json::json!(["Configured Author <configured@example.com>"])
    );
    Ok(())
}

#[test]
fn init_cli_fields_override_configuration_independently() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("sprocket.toml");
    fs::write(
        &config,
        "[module.init]\nauthor = \"Configured Author\"\nemail = \"configured@example.com\"\n",
    )?;

    let output = sprocket_with_config(
        &config,
        &[
            "dev",
            "module",
            "init",
            "--name",
            "demo",
            "--author",
            " CLI Author ",
        ],
    )
    .current_dir(dir.path())
    .output()?;

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("module.json"))?)?;
    assert_eq!(
        value["authors"],
        serde_json::json!(["CLI Author <configured@example.com>"])
    );
    Ok(())
}

#[test]
fn init_cli_email_overrides_configuration_and_git() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = Repository::init(dir.path())?;
    let mut git = repo.config()?;
    git.set_str("user.name", "Git Author")?;
    git.set_str("user.email", "git@example.com")?;
    let config = dir.path().join("sprocket.toml");
    fs::write(
        &config,
        "[module.init]\nemail = \"configured@example.com\"\n",
    )?;

    let output = sprocket_with_config(
        &config,
        &[
            "dev",
            "module",
            "init",
            "--name",
            "demo",
            "--email",
            " cli@example.com ",
        ],
    )
    .current_dir(dir.path())
    .output()?;

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("module.json"))?)?;
    assert_eq!(
        value["authors"],
        serde_json::json!(["Git Author <cli@example.com>"])
    );
    Ok(())
}

#[test]
fn init_configuration_and_git_fields_resolve_independently() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = Repository::init(dir.path())?;
    let mut git = repo.config()?;
    git.set_str("user.name", "Git Author")?;
    git.set_str("user.email", "git@example.com")?;
    let config = dir.path().join("sprocket.toml");
    fs::write(
        &config,
        "[module.init]\nemail = \"configured@example.com\"\n",
    )?;

    let output = sprocket_with_config(&config, &["dev", "module", "init", "--name", "demo"])
        .current_dir(dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("module.json"))?)?;
    assert_eq!(
        value["authors"],
        serde_json::json!(["Git Author <configured@example.com>"])
    );
    Ok(())
}

#[test]
fn init_writes_email_only_identity() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let empty_git_config = dir.path().join("empty-gitconfig");
    fs::write(&empty_git_config, "")?;
    let config = dir.path().join("sprocket.toml");
    fs::write(&config, "[module.init]\nemail = \"only@example.com\"\n")?;

    let output = sprocket_with_config(&config, &["dev", "module", "init", "--name", "demo"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", &empty_git_config)
        .current_dir(dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("module.json"))?)?;
    assert_eq!(value["authors"], serde_json::json!(["<only@example.com>"]));
    Ok(())
}

#[test]
fn init_rejects_blank_cli_identity_before_creating_target() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("module");
    let target_arg = target.to_string_lossy().into_owned();
    let output = sprocket(&["dev", "module", "init", &target_arg, "--author", "   "]).output()?;

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("`--author` cannot be empty"));
    assert!(!target.exists());
    Ok(())
}

#[test]
fn init_rejects_blank_config_identity_before_creating_target() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("module");
    let target_arg = target.to_string_lossy().into_owned();
    let config = dir.path().join("sprocket.toml");
    fs::write(&config, "[module.init]\nemail = \"   \"\n")?;
    let output = sprocket_with_config(&config, &["dev", "module", "init", &target_arg]).output()?;

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("`module.init.email` cannot be empty")
    );
    assert!(!target.exists());
    Ok(())
}

#[test]
fn init_infers_git_author_and_sanitized_repository() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Module Author").unwrap();
    config.set_str("user.email", "author@example.com").unwrap();
    repo.remote(
        "origin",
        "https://fixture:secret@example.com/acme/module.git",
    )
    .unwrap();

    let output = sprocket(&[
        "dev",
        "module",
        "init",
        "--name",
        "demo",
        "--license",
        "MIT",
    ])
    .current_dir(dir.path())
    .output()
    .expect("failed to run sprocket");
    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("module.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["authors"],
        serde_json::json!(["Module Author <author@example.com>"])
    );
    assert_eq!(
        manifest["repository"],
        "https://example.com/acme/module.git"
    );
}
