//! Integration tests for `sprocket run` against modules with signed
//! dependencies.

use std::fs;

use wdl_modules::dependency::DependencyName;

use crate::fixtures::*;

#[test]
fn run_ignores_excluded_dependency_harness_with_escaping_import() {
    let fixture = GitFixture::without_version_tags();
    let module_root = fixture.repo_dir.join("tasks");
    fs::write(
        module_root.join("module.json"),
        r#"{
  "name": "tasks",
  "license": "MIT",
  "entrypoint": "index.wdl",
  "exclude": ["testrun.wdl"]
}
"#,
    )
    .unwrap();
    fs::write(
        module_root.join("testrun.wdl"),
        "version 1.3\nimport \"../outside.wdl\"\n",
    )
    .unwrap();
    commit_without_tags(
        &git2::Repository::open(&fixture.repo_dir).unwrap(),
        "add excluded test harness",
    );

    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-run-excluded-harness",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );
    fs::write(
        consumer.join("index.wdl"),
        "version 1.4\nimport tasks\nworkflow wf {\n  output {\n    String ok = \"ok\"\n  }\n}\n",
    )
    .unwrap();
    let cache_path = serde_json::to_string(&fixture.cache_path().to_string_lossy()).unwrap();
    fs::write(
        fixture.config_path(),
        format!(
            "[common.wdl.feature_flags]\nwdl_1_4 = true\n\n[modules]\ncache_path = \
             {cache_path}\nallowed_schemes = [\"file\", \"https\", \"ssh\"]\ndenied_hosts = \
             []\n\n[run.backends.default]\ntype = \"local\"\n"
        ),
    )
    .unwrap();

    let run = sprocket_with_config(fixture.config_path(), &["run", "."])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket run");
    assert!(
        run.status.success(),
        "command failed {status}: {stderr}",
        status = run.status,
        stderr = String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn run_allows_excluded_local_harness_to_import_sibling() {
    let fixture = ModuleFixture::with_local_dep();
    let config = fixture.dir.path().join("sprocket.toml");
    fs::write(&config, "[run.backends.default]\ntype = \"local\"\n").unwrap();
    fs::write(
        fixture.dep().join("index.wdl"),
        "version 1.3\nstruct Marker { String value }\n",
    )
    .unwrap();
    fs::write(
        fixture.consumer().join("module.json"),
        r#"{
  "name": "consumer",
  "license": "MIT",
  "entrypoint": "index.wdl",
  "exclude": ["testrun.wdl"]
}
"#,
    )
    .unwrap();
    fs::write(
        fixture.consumer().join("testrun.wdl"),
        r#"version 1.3
import "../dep/index.wdl" as library

workflow harness {
  output {
    String ok = "ok"
  }
}
"#,
    )
    .unwrap();

    let run = sprocket_with_config(&config, &["run", "testrun.wdl"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket run");
    assert!(
        run.status.success(),
        "command failed {status}: {stderr}",
        status = run.status,
        stderr = String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn run_fails_when_locked_signer_key_is_removed_from_trust_store() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-run-revoked");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-run-revoked-trust",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );
    fs::write(
        consumer.join("index.wdl"),
        "version 1.4\nimport { t } from tasks\nworkflow wf {\n  call t\n}\n",
    )
    .unwrap();

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = output_with_stdin(lock_command, "y\n");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );
    let lockfile = read_lockfile(&consumer);
    let dep_name: DependencyName = "tasks".parse().unwrap();
    let public_key = lockfile
        .dependencies
        .get(&dep_name)
        .and_then(|entry| entry.signer)
        .expect("locked dependency should record a signer")
        .to_openssh();

    let mut remove_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "trust", "remove", &public_key],
    );
    remove_command.current_dir(&consumer);
    use_home(&mut remove_command, &home);
    let remove = remove_command
        .output()
        .expect("failed to run sprocket dev module trust remove");
    assert!(
        remove.status.success(),
        "command failed {status}: {stderr}",
        status = remove.status,
        stderr = String::from_utf8_lossy(&remove.stderr)
    );

    let mut run_command = sprocket_with_config(fixture.config_path(), &["run", "."]);
    run_command.current_dir(&consumer);
    use_home(&mut run_command, &home);
    let run = run_command.output().expect("failed to run sprocket run");
    assert!(
        !run.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&run.stdout)
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("signed by an untrusted key"));
    assert!(stderr.contains("sprocket dev module trust all"));
    assert!(!stderr.contains("unknown task or workflow `t`"));
}

#[test]
fn run_fails_when_required_signature_dependency_is_unsigned() {
    let fixture = GitFixture::without_version_tags();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer("consumer-run-require-signed", "");
    fs::write(
        consumer.join("index.wdl"),
        "version 1.4\nimport tasks\nworkflow wf {\n  output {\n    String ok = \"ok\"\n  }\n}\n",
    )
    .unwrap();

    let cache_path = serde_json::to_string(&fixture.cache_path().to_string_lossy()).unwrap();
    fs::write(
        fixture.config_path(),
        format!(
            "[common.wdl.feature_flags]\nwdl_1_4 = true\n\n[modules]\ncache_path = \
             {cache_path}\nallowed_schemes = [\"file\", \"https\", \"ssh\"]\ndenied_hosts = \
             []\nrequire_signed = true\n"
        ),
    )
    .unwrap();

    let add = sprocket_with_config(
        fixture.config_path(),
        &[
            "dev",
            "module",
            "add",
            &repo_url,
            "--path",
            "tasks",
            "--branch",
            &default_branch,
            "--no-lock",
        ],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket dev module add");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let run = sprocket_with_config(fixture.config_path(), &["run", "."])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket run");
    assert!(
        !run.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&run.stdout)
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("unsigned"));
    assert!(stderr.contains("require_signed"));
}
