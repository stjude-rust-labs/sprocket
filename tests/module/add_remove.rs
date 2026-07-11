//! Integration tests for `sprocket dev module add` and `sprocket dev module
//! remove`.

use std::fs;
use std::path::PathBuf;

use wdl_modules::Lockfile;
use wdl_modules::Manifest;

use crate::fixtures::*;

#[test]
fn add_local_path_dep_edits_manifest_and_locks() {
    let fixture = ModuleFixture::with_local_dep();
    let output = sprocket(&["dev", "module", "add", "utils", "../dep"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim_end().ends_with("Locked `utils`"));
    assert!(!stdout.contains("Adding utils ("));

    let manifest = fs::read(fixture.consumer().join("module.json")).unwrap();
    let parsed = Manifest::parse(&manifest).unwrap();
    assert!(
        parsed
            .dependencies
            .keys()
            .any(|name| name.manifest() == "utils")
    );

    let lockfile = fs::read(fixture.consumer().join("module-lock.json")).unwrap();
    Lockfile::parse(&lockfile).unwrap();
}

#[test]
fn add_local_path_dep_uses_subpath_for_module_root_and_name() {
    let fixture = ModuleFixture::with_local_dep();
    let collection = fixture.dir.path().join("spellbook");
    let module = collection.join("modules").join("alchemy");
    fs::create_dir_all(&module).unwrap();
    fs::write(
        module.join("module.json"),
        r#"{
  "name": "alchemy",
  "license": "MIT",
  "entrypoint": "index.wdl"
}
"#,
    )
    .unwrap();
    fs::write(module.join("index.wdl"), "version 1.3\n").unwrap();

    let collection_arg = collection.to_string_lossy().into_owned();
    let output = sprocket(&[
        "dev",
        "module",
        "add",
        &collection_arg,
        "--path",
        "modules/alchemy",
    ])
    .current_dir(fixture.consumer())
    .output()
    .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read(fixture.consumer().join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    // Compare as `PathBuf`s rather than strings: `local_dependency_path`
    // appends the module subpath with forward slashes, so on Windows the
    // recorded path mixes the native drive prefix with `/` separators (a valid
    // path). `PathBuf` equality normalizes those separators; string equality
    // would spuriously fail on the separator difference alone.
    let recorded = value["dependencies"]["alchemy"]["path"]
        .as_str()
        .expect("dependency path should be a string");
    assert_eq!(PathBuf::from(recorded), module);

    let lockfile = fs::read(fixture.consumer().join("module-lock.json")).unwrap();
    Lockfile::parse(&lockfile).unwrap();
}

#[test]
fn add_git_dep_without_tags_tracks_default_branch_and_locks() {
    let fixture = GitFixture::without_version_tags();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer("consumer-add-default-branch", "");

    let output = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "add", "dep", &repo_url, "--path", "tasks"],
    )
    .current_dir(&consumer)
    .env("RUST_LOG", "info")
    .output()
    .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read(consumer.join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let dep = &value["dependencies"]["dep"];
    assert_eq!(dep["git"].as_str(), Some(repo_url.as_str()));
    assert_eq!(dep["path"].as_str(), Some("tasks"));
    assert_eq!(dep["branch"].as_str(), Some(default_branch.as_str()));
    assert!(dep.get("version").is_none());

    assert!(consumer.join("module-lock.json").exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("no path-scoped Git version tags found"));
    assert!(!stdout.contains("Adding dep ("));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no path-scoped Git version tags found for `tasks`"));
    assert!(stderr.contains(&format!("tracking branch `{}`", default_branch)));
}

#[test]
fn add_prompts_before_trusting_new_signer_key() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer("consumer-add-signer-prompt", "");
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();

    let mut add_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "add", &repo_url, "--path", "tasks"],
    );
    add_command.current_dir(&consumer);
    let add = output_with_stdin(add_command, "\n");
    assert!(
        !add.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&add.stdout)
    );
    let stderr = String::from_utf8_lossy(&add.stderr);
    assert!(stderr.contains("module signer key requires trust"));
    assert!(stderr.contains("[y/N]"));
    assert!(stderr.contains("refusing to update `module-lock.json`"));
    assert_eq!(
        fs::read(consumer.join("module.json")).unwrap(),
        manifest_before
    );
    assert!(!consumer.join("module-lock.json").exists());
}

#[test]
fn add_trust_mode_flag_auto_trusts_without_prompting() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-add-auto-flag");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer("consumer-add-auto-flag", "");

    let mut add_command = sprocket_with_config(
        fixture.config_path(),
        &[
            "dev",
            "module",
            "add",
            &repo_url,
            "--path",
            "tasks",
            "--trust-mode",
            "auto",
        ],
    );
    add_command.current_dir(&consumer);
    use_home(&mut add_command, &home);
    let add = add_command
        .output()
        .expect("failed to run sprocket dev module add");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );
    assert!(!String::from_utf8_lossy(&add.stderr).contains("[y/N]"));
    assert!(consumer.join("module-lock.json").exists());
}

#[test]
fn add_hosted_git_shorthand_infers_repo_name() {
    let fixture = ModuleFixture::with_local_dep();
    let output = sprocket(&[
        "dev",
        "module",
        "add",
        "stjudecloud/workflows",
        "--branch",
        "main",
        "--no-lock",
    ])
    .current_dir(fixture.consumer())
    .output()
    .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read(fixture.consumer().join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let dep = &value["dependencies"]["workflows"];
    assert_eq!(
        dep["git"].as_str(),
        Some("https://github.com/stjudecloud/workflows.git")
    );
    assert_eq!(dep["branch"].as_str(), Some("main"));
}

#[test]
fn add_git_path_infers_dependency_name() {
    let fixture = ModuleFixture::with_local_dep();
    let output = sprocket(&[
        "dev",
        "module",
        "add",
        "stjudecloud/workflows",
        "--path",
        "modules/alchemy",
        "--branch",
        "main",
        "--no-lock",
    ])
    .current_dir(fixture.consumer())
    .output()
    .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read(fixture.consumer().join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let dep = &value["dependencies"]["alchemy"];
    assert_eq!(
        dep["git"].as_str(),
        Some("https://github.com/stjudecloud/workflows.git")
    );
    assert_eq!(dep["path"].as_str(), Some("modules/alchemy"));
    assert_eq!(dep["branch"].as_str(), Some("main"));
}

#[test]
fn add_hosted_git_shorthand_respects_configured_platform_and_name() {
    let fixture = ModuleFixture::with_local_dep();
    let config_path = fixture.dir.path().join("sprocket.toml");
    fs::write(
        &config_path,
        "[modules]\ndefault_git_platform = \"gitlab\"\n",
    )
    .unwrap();
    let output = sprocket_with_config(
        &config_path,
        &[
            "dev",
            "module",
            "add",
            "stjudecloud/workflows",
            "--name",
            "wf",
            "--tag",
            "v1.0.0",
            "--no-lock",
        ],
    )
    .current_dir(fixture.consumer())
    .output()
    .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read(fixture.consumer().join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let dep = &value["dependencies"]["wf"];
    assert_eq!(
        dep["git"].as_str(),
        Some("https://gitlab.com/stjudecloud/workflows.git")
    );
    assert_eq!(dep["tag"].as_str(), Some("v1.0.0"));
}

#[test]
fn add_hosted_git_shorthand_respects_platform_flag() {
    let fixture = ModuleFixture::with_local_dep();
    let output = sprocket(&[
        "dev",
        "module",
        "add",
        "stjudecloud/workflows",
        "--git-platform",
        "bitbucket",
        "--tag",
        "v1.0.0",
        "--no-lock",
    ])
    .current_dir(fixture.consumer())
    .output()
    .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read(fixture.consumer().join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let dep = &value["dependencies"]["workflows"];
    assert_eq!(
        dep["git"].as_str(),
        Some("https://bitbucket.org/stjudecloud/workflows.git")
    );
    assert_eq!(dep["tag"].as_str(), Some("v1.0.0"));
}

#[test]
fn add_direct_git_url_infers_repo_name() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer("consumer-add-direct-url", "");

    let output = sprocket_with_config(
        fixture.config_path(),
        &[
            "dev",
            "module",
            "add",
            &repo_url,
            "--branch",
            &default_branch,
            "--no-lock",
        ],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read(consumer.join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let dep = &value["dependencies"]["tasks-repo"];
    assert_eq!(dep["git"].as_str(), Some(repo_url.as_str()));
    assert_eq!(dep["branch"].as_str(), Some(default_branch.as_str()));
}

#[test]
fn add_rejects_invalid_dependency_name() {
    let fixture = ModuleFixture::with_local_dep();
    let output = sprocket(&["dev", "module", "add", "1bad", "../dep"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module add");

    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn add_existing_identical_dep_reports_skipped_and_logs_noop() {
    let fixture = ModuleFixture::with_local_dep_added();
    let before = fs::read(fixture.consumer().join("module.json")).unwrap();
    let output = sprocket(&["dev", "module", "add", "utils", "../dep"])
        .current_dir(fixture.consumer())
        .env("RUST_LOG", "info")
        .output()
        .expect("failed to run sprocket dev module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .trim_end()
            .ends_with("Skipped `utils` already exists in the module's dependencies")
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("dependency already exists with the same source"));
    assert_eq!(
        fs::read(fixture.consumer().join("module.json")).unwrap(),
        before
    );
}

#[test]
fn remove_drops_dep_and_relocks() {
    let fixture = ModuleFixture::with_local_dep_added();
    let output = sprocket(&["dev", "module", "remove", "utils"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module remove");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read(fixture.consumer().join("module.json")).unwrap();
    let parsed = Manifest::parse(&manifest).unwrap();
    assert!(parsed.dependencies.is_empty());
}

#[test]
fn remove_leaves_manifest_untouched_when_relock_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("consumer");
    fs::create_dir_all(&root).unwrap();
    // `broken` points at a missing local module, so any relock fails.
    fs::write(
        root.join("module.json"),
        r#"{
  "name": "consumer",
  "license": "MIT",
  "dependencies": {
    "broken": { "path": "../missing" },
    "drop": { "path": "../also-missing" }
  }
}
"#,
    )
    .unwrap();
    let manifest_before = fs::read(root.join("module.json")).unwrap();

    let output = sprocket(&["dev", "module", "remove", "drop"])
        .current_dir(&root)
        .output()
        .expect("failed to run sprocket dev module remove");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read(root.join("module.json")).unwrap(),
        manifest_before,
        "a failed relock must not modify `module.json`"
    );
}

#[test]
fn add_new_signer_matrix_respects_trust_mode() {
    let cases = [
        (CliTrustMode::Confirm, false, true),
        (CliTrustMode::Tofu, true, false),
        (CliTrustMode::Auto, true, false),
    ];

    for (mode, expect_success, expect_prompt) in cases {
        let (fixture, _old_key) = GitFixture::signed_initial_version();
        let repo_url = fixture.repo_url();
        let consumer = fixture.write_consumer("consumer-add-matrix", "");
        let mut command = sprocket_with_config(
            fixture.config_path(),
            &[
                "dev",
                "module",
                "add",
                "tasks",
                &repo_url,
                "--version",
                "=1.0.0",
                "--path",
                "tasks",
                "--trust-mode",
                mode.as_arg(),
            ],
        );
        command.current_dir(&consumer);
        let output = output_with_stdin(command, "\n");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.success(),
            expect_success,
            "mode={mode:?} stderr={stderr}"
        );
        assert_eq!(
            stderr.contains("[y/N]"),
            expect_prompt,
            "mode={mode:?} stderr={stderr}"
        );
        if expect_prompt {
            assert!(
                stderr.contains("signer key added"),
                "mode={mode:?} stderr={stderr}"
            );
        }
    }
}

#[test]
fn add_rejects_conflicting_selector_flags() {
    let dir = tempfile::tempdir().unwrap();
    let output = sprocket(&[
        "dev",
        "module",
        "add",
        "https://example.com/org/repo",
        "--tag",
        "v1",
        "--branch",
        "main",
    ])
    .current_dir(dir.path())
    .output()
    .expect("failed to run sprocket");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be used with"), "stderr: {stderr}");
}

#[test]
fn add_rejects_scp_style_git_urls() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("module.json"),
        r#"{"name":"demo","license":"MIT"}"#,
    )
    .unwrap();
    let output = sprocket(&[
        "dev",
        "module",
        "add",
        "git@github.com:org/repo.git",
        "--no-lock",
    ])
    .current_dir(dir.path())
    .output()
    .expect("failed to run sprocket");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ssh://git@github.com/org/repo.git"),
        "stderr should suggest the ssh:// form: {stderr}"
    );
}
