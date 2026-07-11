//! Integration tests for `sprocket module tree`, `list`, `fetch`, `cache`, and
//! `check`.

use std::fs;

use crate::fixtures::*;

#[test]
fn tree_prints_dependency() {
    let fixture = ModuleFixture::with_local_dep_added();
    let output = sprocket(&["module", "tree"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module tree");
    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("utils"));
    assert!(!stdout.contains("1.0.0"));
}

#[test]
fn tree_without_lockfile_errors() {
    let fixture = ModuleFixture::with_local_dep();
    let output = sprocket(&["module", "tree"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module tree");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sprocket module lock"));
}

#[test]
fn list_prints_dependency() {
    let fixture = ModuleFixture::with_local_dep_added();
    let output = sprocket(&["module", "list"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module list");
    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("name"));
    assert!(stdout.contains("utils"));
    assert!(stdout.contains("source"));
}

#[test]
fn list_without_lockfile_errors() {
    let fixture = ModuleFixture::with_local_dep();
    let output = sprocket(&["module", "list"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module list");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sprocket module lock"));
}

#[test]
fn check_warns_on_lock_drift() {
    let fixture = ModuleFixture::with_local_dep_added();
    let manifest_path = fixture.consumer().join("module.json");
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let deps = manifest
        .get_mut("dependencies")
        .and_then(serde_json::Value::as_object_mut)
        .expect("manifest created by `module add` should include dependencies");
    deps.insert(
        "extra2".to_owned(),
        serde_json::json!({
            "path": "../dep"
        }),
    );
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let config_path = fixture.dir.path().join("sprocket.toml");
    fs::write(
        &config_path,
        "[common.wdl.feature_flags]\nwdl_1_4 = true\n\n[modules]\n",
    )
    .unwrap();

    let entrypoint = fixture.consumer().join("index.wdl");
    fs::write(
        &entrypoint,
        "version 1.2\ntask t { command <<< echo hi >>> }\n",
    )
    .unwrap();
    let entrypoint_arg = entrypoint.to_string_lossy().into_owned();
    let output = sprocket_with_config(&config_path, &["check", &entrypoint_arg])
        .current_dir(fixture.consumer())
        .env("RUST_LOG", "warn")
        .output()
        .expect("failed to run sprocket check");
    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("out of date"));
    assert!(stderr.contains("sprocket module lock"));
}

#[test]
fn check_does_not_warn_on_current_branch_dependency_lock() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-branch-check",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let lock = sprocket_with_config(fixture.config_path(), &["module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    let entrypoint = consumer.join("index.wdl");
    fs::write(
        &entrypoint,
        "version 1.2\ntask t { command <<< echo hi >>> }\n",
    )
    .unwrap();
    let entrypoint_arg = entrypoint.to_string_lossy().into_owned();
    let output = sprocket_with_config(fixture.config_path(), &["check", &entrypoint_arg])
        .current_dir(&consumer)
        .env("RUST_LOG", "warn")
        .output()
        .expect("failed to run sprocket check");
    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("out of date"));
}

#[test]
fn cache_clean_all_works_outside_a_module() {
    let dir = tempfile::tempdir().unwrap();
    let home = isolated_home(dir.path(), "home-global-cache");
    let mut command = sprocket(&["module", "cache", "clean", "--all"]);
    command.current_dir(dir.path());
    use_home(&mut command, &home);
    let output = command.output().expect("failed to run sprocket");
    assert!(
        output.status.success(),
        "cache clean --all should not require a `module.json`: {stderr}",
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Removed 0 cached modules"));
}

#[test]
fn fetch_populates_cache_then_verify_succeeds() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-fetch",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let lock = sprocket_with_config(fixture.config_path(), &["module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    fs::remove_dir_all(fixture.cache_path()).unwrap();

    let fetch = sprocket_with_config(fixture.config_path(), &["module", "fetch"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module fetch");
    assert!(
        fetch.status.success(),
        "command failed {status}: {stderr}",
        status = fetch.status,
        stderr = String::from_utf8_lossy(&fetch.stderr)
    );
    assert!(String::from_utf8_lossy(&fetch.stdout).contains("Fetched 1 dependency"));

    let second_fetch = sprocket_with_config(fixture.config_path(), &["module", "fetch"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module fetch");
    assert!(
        second_fetch.status.success(),
        "command failed {status}: {stderr}",
        status = second_fetch.status,
        stderr = String::from_utf8_lossy(&second_fetch.stderr)
    );
    assert!(String::from_utf8_lossy(&second_fetch.stdout).contains("Fetched 0 dependencies"));

    let verify = sprocket_with_config(fixture.config_path(), &["module", "verify"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module verify");
    assert!(
        verify.status.success(),
        "command failed {status}: {stderr}",
        status = verify.status,
        stderr = String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn fetch_without_lockfile_errors() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-fetch-no-lock",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let fetch = sprocket_with_config(fixture.config_path(), &["module", "fetch"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module fetch");
    assert!(
        !fetch.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&fetch.stdout)
    );
    assert!(String::from_utf8_lossy(&fetch.stderr).contains("sprocket module lock"));
}

#[test]
fn cache_clean_default_removes_current_lock_tree_only() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-cache-clean",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let lock = sprocket_with_config(fixture.config_path(), &["module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    let unrelated = fixture
        .cache_path()
        .join("unrelated")
        .join("1111111111111111111111111111111111111111");
    fs::create_dir_all(&unrelated).unwrap();
    fs::write(unrelated.join("sentinel.txt"), "keep").unwrap();

    let clean = sprocket_with_config(fixture.config_path(), &["module", "cache", "clean"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module cache clean");
    assert!(
        clean.status.success(),
        "command failed {status}: {stderr}",
        status = clean.status,
        stderr = String::from_utf8_lossy(&clean.stderr)
    );
    assert!(
        unrelated.exists(),
        "expected unrelated cache leaf to remain"
    );
    let stdout = String::from_utf8_lossy(&clean.stdout);
    assert!(stdout.contains("Removed 1 cached module"));
    assert!(
        !stdout.contains("GiB"),
        "small caches must not be rounded up to GiB: {stdout}"
    );

    let verify = sprocket_with_config(fixture.config_path(), &["module", "verify"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    assert!(String::from_utf8_lossy(&verify.stderr).contains("sprocket module fetch"));
}

#[test]
fn cache_clean_all_removes_entire_cache() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-cache-clean-all",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let lock = sprocket_with_config(fixture.config_path(), &["module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    let unrelated = fixture
        .cache_path()
        .join("unrelated")
        .join("1111111111111111111111111111111111111111");
    fs::create_dir_all(&unrelated).unwrap();
    fs::write(unrelated.join("sentinel.txt"), "remove").unwrap();

    let clean = sprocket_with_config(
        fixture.config_path(),
        &["module", "cache", "clean", "--all"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket module cache clean --all");
    assert!(
        clean.status.success(),
        "command failed {status}: {stderr}",
        status = clean.status,
        stderr = String::from_utf8_lossy(&clean.stderr)
    );
    assert!(
        !fixture.cache_path().exists(),
        "expected entire cache root to be removed"
    );
    let stdout = String::from_utf8_lossy(&clean.stdout);
    assert!(stdout.contains("Removed 2 cached modules"));
}

#[test]
fn module_clean_top_level_command_is_removed() {
    let output = sprocket(&["module", "clean"])
        .output()
        .expect("failed to run sprocket module clean");
    assert!(
        !output.status.success(),
        "`sprocket module clean` unexpectedly succeeded"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
}
