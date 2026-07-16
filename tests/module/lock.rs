//! Integration tests for `sprocket dev module lock`.

use std::fs;

use wdl_modules::Lockfile;
use wdl_modules::dependency::DependencyName;

use crate::fixtures::*;

#[test]
fn lock_prompts_before_trusting_new_signer_key() {
    let (fixture, public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-lock-prompt");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-lock-signer-prompt",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = output_with_stdin(lock_command, "\n");
    assert!(
        !lock.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&lock.stdout)
    );
    let stderr = String::from_utf8_lossy(&lock.stderr);
    assert!(stderr.contains("module signer key requires trust"));
    assert!(stderr.contains("[y/N]"));
    assert!(stderr.contains("refusing to update `module-lock.json`"));
    assert!(!consumer.join("module-lock.json").exists());

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    assert!(!String::from_utf8_lossy(&list.stdout).contains(&public_key));
}

#[test]
fn lock_accepts_new_signer_key_when_confirmed() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-lock-accept");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-lock-signer-accept",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

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
    assert!(consumer.join("module-lock.json").exists());
    let lockfile = read_lockfile(&consumer);
    let dep_name: DependencyName = "tasks".parse().unwrap();
    let signer = lockfile
        .dependencies
        .get(&dep_name)
        .and_then(|entry| entry.signer)
        .expect("locked dependency should record a signer")
        .to_openssh();

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains(&signer));
}

#[test]
fn lock_auto_trusts_new_signer_key_without_prompting() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-lock-auto");
    set_fixture_trust_mode(&fixture, "auto");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-lock-signer-auto",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );
    assert!(!String::from_utf8_lossy(&lock.stderr).contains("[y/N]"));

    let lockfile = read_lockfile(&consumer);
    let dep_name: DependencyName = "tasks".parse().unwrap();
    let signer = lockfile
        .dependencies
        .get(&dep_name)
        .and_then(|entry| entry.signer)
        .expect("locked dependency should record a signer")
        .to_openssh();
    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(String::from_utf8_lossy(&list.stdout).contains(&signer));
}

#[test]
fn lock_tofu_trusts_new_signer_key_without_prompting() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-lock-tofu");
    set_fixture_trust_mode(&fixture, "tofu");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-lock-signer-tofu",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );
    assert!(!String::from_utf8_lossy(&lock.stderr).contains("[y/N]"));
}

#[test]
fn lock_trust_mode_flag_auto_trusts_without_prompting() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-lock-auto-flag");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-lock-auto-flag",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let mut lock_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "lock", "--trust-mode", "auto"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );
    assert!(!String::from_utf8_lossy(&lock.stderr).contains("[y/N]"));
}

#[test]
fn lock_dry_run_does_not_write_lockfile_or_trust_store() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-lock-dry-run");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-lock-dry-run",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let mut command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "lock", "--dry-run", "--trust-mode", "auto"],
    );
    command.current_dir(&consumer);
    use_home(&mut command, &home);
    let output = command
        .output()
        .expect("failed to run sprocket dev module lock --dry-run");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    assert!(!consumer.join("module-lock.json").exists());
    assert!(
        !home
            .join(".config")
            .join("sprocket")
            .join("modules-trust.toml")
            .exists()
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("[y/N]"));
}

#[test]
fn lock_writes_lockfile() {
    let fixture = ModuleFixture::with_local_dep();
    let add = sprocket(&["dev", "module", "add", "utils", "../dep", "--no-lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module add --no-lock");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    assert!(!fixture.consumer().join("module-lock.json").exists());

    let output = sprocket(&["dev", "module", "lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    let lockfile = fs::read(fixture.consumer().join("module-lock.json")).unwrap();
    let lock = Lockfile::parse(&lockfile).unwrap();
    assert!(
        lock.dependencies
            .keys()
            .any(|name| name.manifest() == "utils")
    );
}

#[test]
fn lock_rejects_removed_update_and_upgrade_subcommands() {
    for removed in ["update", "upgrade"] {
        let output = sprocket(&["dev", "module", "lock", removed])
            .output()
            .expect("failed to run sprocket dev module lock");
        assert!(
            !output.status.success(),
            "`sprocket dev module lock {removed}` unexpectedly succeeded"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unexpected argument"),
            "unexpected stderr for `{removed}`: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn lock_locked_flag_fails_on_drift() {
    let fixture = ModuleFixture::with_local_dep();
    let add = sprocket(&["dev", "module", "add", "utils", "../dep", "--no-lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module add --no-lock");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let output = sprocket(&["dev", "module", "lock", "--locked"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module lock --locked");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn lock_idempotent_reports_up_to_date() {
    let fixture = ModuleFixture::with_local_dep();
    let first = sprocket(&["dev", "module", "lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run first sprocket dev module lock");
    assert!(
        first.status.success(),
        "command failed {status}: {stderr}",
        status = first.status,
        stderr = String::from_utf8_lossy(&first.stderr)
    );

    let second = sprocket(&["dev", "module", "lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run second sprocket dev module lock");
    assert!(
        second.status.success(),
        "command failed {status}: {stderr}",
        status = second.status,
        stderr = String::from_utf8_lossy(&second.stderr)
    );
    assert!(
        String::from_utf8_lossy(&second.stdout)
            .to_ascii_lowercase()
            .contains("up to date")
    );
}

#[test]
fn lock_locked_flag_succeeds_when_current() {
    let fixture = ModuleFixture::with_local_dep();
    let first = sprocket(&["dev", "module", "lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        first.status.success(),
        "command failed {status}: {stderr}",
        status = first.status,
        stderr = String::from_utf8_lossy(&first.stderr)
    );

    let locked = sprocket(&["dev", "module", "lock", "--locked"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module lock --locked");
    assert!(
        locked.status.success(),
        "command failed {status}: {stderr}",
        status = locked.status,
        stderr = String::from_utf8_lossy(&locked.stderr)
    );
}

/// Regression test for content-address stability across line-ending filters.
///
/// The dependency repository carries a `.gitattributes` demanding CRLF line
/// endings, so a filtered checkout would rewrite the module's text files and
/// change their content hash, failing signature verification during `lock`.
/// The resolver disables filters at checkout, so the bytes match the signed
/// LF content and both `lock` and `verify` succeed. This exercises the same
/// code path that Windows `core.autocrlf=true` would trigger, but is
/// deterministic on every platform because attribute-driven conversion is not
/// OS-dependent.
#[test]
fn lock_and_verify_succeed_despite_crlf_gitattributes() {
    let (fixture, _public_key) = GitFixture::signed_with_crlf_attributes();
    let home = isolated_home(fixture.dir.path(), "home-crlf-attributes");
    set_fixture_trust_mode(&fixture, "auto");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-crlf-attributes",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        lock.status.success(),
        "lock failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    let mut verify_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "verify"]);
    verify_command.current_dir(&consumer);
    use_home(&mut verify_command, &home);
    let verify = verify_command
        .output()
        .expect("failed to run sprocket dev module verify");
    assert!(
        verify.status.success(),
        "verify failed {status}: {stderr}",
        status = verify.status,
        stderr = String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn lock_resolves_file_git_dependency_with_config() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-file-lock",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let output = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    let lock = read_lockfile(&consumer);
    assert_eq!(locked_git_selector(&lock, "tasks"), "version ^1.0");
}

#[test]
fn lock_reports_not_a_sprocket_module_when_module_json_missing() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-missing-manifest",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "missing" }}"#
        ),
    );

    let output = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not a WDL module"));
}
