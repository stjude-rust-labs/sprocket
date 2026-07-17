//! Integration tests for `sprocket dev module update`.

use std::fs;

use wdl_modules::signing::SigningKey;

use crate::fixtures::*;

#[test]
fn update_moves_pin_to_newest_satisfying_version_and_is_idempotent() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-update",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let first_lock = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        first_lock.status.success(),
        "command failed {status}: {stderr}",
        status = first_lock.status,
        stderr = String::from_utf8_lossy(&first_lock.stderr)
    );
    let lock_before_update = read_lockfile(&consumer);
    let commit_before_update = locked_git_commit(&lock_before_update, "tasks");

    add_unsigned_git_version(&fixture.repo_dir, "1.1.1");
    let newest_commit = fixture.head_commit();

    let first_update = sprocket_with_config(fixture.config_path(), &["dev", "module", "update"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run first sprocket dev module update");
    assert!(
        first_update.status.success(),
        "command failed {status}: {stderr}",
        status = first_update.status,
        stderr = String::from_utf8_lossy(&first_update.stderr)
    );

    let lock_after_first_update = read_lockfile(&consumer);
    assert_eq!(
        locked_git_selector(&lock_after_first_update, "tasks"),
        "version ^1.0"
    );
    assert_eq!(
        locked_git_commit(&lock_after_first_update, "tasks"),
        newest_commit
    );
    assert_ne!(commit_before_update, newest_commit);
    let stdout = String::from_utf8_lossy(&first_update.stdout);
    assert!(stdout.contains("Updated 1 dependency"));
    assert!(stdout.contains(&format!(
        "commit: `{}` -> `{}`",
        &commit_before_update[..7],
        &newest_commit[..7]
    )));
    let first_bytes = fs::read(consumer.join("module-lock.json")).unwrap();

    let second_update = sprocket_with_config(fixture.config_path(), &["dev", "module", "update"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run second sprocket dev module update");
    assert!(
        second_update.status.success(),
        "command failed {status}: {stderr}",
        status = second_update.status,
        stderr = String::from_utf8_lossy(&second_update.stderr)
    );

    let second_bytes = fs::read(consumer.join("module-lock.json")).unwrap();
    assert_eq!(second_bytes, first_bytes);
}

#[test]
fn lock_update_updates_out_of_date_git_dependency() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let latest = fixture.head_commit();
    let stale = fixture.head_parent_commit();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-stale",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let lock = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );
    set_locked_git_commit(&consumer, "tasks", &stale);

    let update = sprocket_with_config(fixture.config_path(), &["dev", "module", "update"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Updated 1 dependency"));
    assert!(stdout.contains("tasks"));
    assert!(stdout.contains("selector: branch"));
    assert!(stdout.contains(&format!("commit: `{}` -> `{}`", &stale[..7], &latest[..7])));

    let lock = read_lockfile(&consumer);
    assert_eq!(locked_git_commit(&lock, "tasks"), latest);
}

#[test]
fn update_dry_run_resolves_changes_without_writing() -> anyhow::Result<()> {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let latest = fixture.head_commit();
    let stale = fixture.head_parent_commit();
    let consumer = fixture.write_consumer(
        "consumer-update-dry-run",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let lock = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"])
        .current_dir(&consumer)
        .output()?;
    assert!(lock.status.success());
    set_locked_git_commit(&consumer, "tasks", &stale);
    let manifest_before = fs::read(consumer.join("module.json"))?;
    let lock_before = fs::read(consumer.join("module-lock.json"))?;

    let update = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "update", "--dry-run"],
    )
    .current_dir(&consumer)
    .output()?;

    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Would update 1 dependency"));
    assert!(stdout.contains(&format!("commit: `{}` -> `{}`", &stale[..7], &latest[..7])));
    assert_eq!(fs::read(consumer.join("module.json"))?, manifest_before);
    assert_eq!(fs::read(consumer.join("module-lock.json"))?, lock_before);
    assert!(!consumer.join(".sprocket").join("module-mutation").exists());
    Ok(())
}

#[test]
fn lock_update_prompts_before_accepting_changed_signer_key() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-prompt");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-signer-prompt",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
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
    let before = fs::read(consumer.join("module-lock.json")).unwrap();

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    let new_public_key = new_key.verifying_key().to_openssh();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut update_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = output_with_stdin(update_command, "\n");
    assert!(
        !update.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&update.stdout)
    );
    let stderr = String::from_utf8_lossy(&update.stderr);
    assert!(stderr.contains("module signer key requires trust"));
    assert!(stderr.contains("[y/N]"));
    assert!(stderr.contains("refusing to update `module-lock.json`"));
    assert_eq!(fs::read(consumer.join("module-lock.json")).unwrap(), before);

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
    assert!(!String::from_utf8_lossy(&list.stdout).contains(&new_public_key));
}

#[test]
fn lock_update_does_not_prompt_for_globally_trusted_changed_signer() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-pretrusted-change");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-pretrusted-change",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
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

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    let new_public_key = new_key.verifying_key().to_openssh();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut trust_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "trust", "add", &new_public_key],
    );
    trust_command.current_dir(&consumer);
    use_home(&mut trust_command, &home);
    let trust = trust_command
        .output()
        .expect("failed to run sprocket dev module trust add");
    assert!(
        trust.status.success(),
        "command failed {status}: {stderr}",
        status = trust.status,
        stderr = String::from_utf8_lossy(&trust.stderr)
    );

    let mut update_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket dev module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
    assert!(!String::from_utf8_lossy(&update.stdout).contains("Trusted"));

    let lock = read_lockfile(&consumer);
    assert_eq!(
        lock.dependencies
            .get(&"tasks".parse().unwrap())
            .and_then(|entry| entry.signer)
            .map(|key| key.to_openssh()),
        Some(new_public_key)
    );
}

#[test]
fn lock_update_prompts_when_dependency_becomes_signed() {
    let fixture = GitFixture::new();
    let home = isolated_home(fixture.dir.path(), "home-update-unsigned-to-signed");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-unsigned-to-signed",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(lock.status.success());
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut update_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = output_with_stdin(update_command, "\n");
    assert!(
        !update.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&update.stdout)
    );
    let stderr = String::from_utf8_lossy(&update.stderr);
    assert!(stderr.contains("module signer key requires trust"));
    assert!(stderr.contains("[y/N]"));
    assert!(stderr.contains("refusing to update `module-lock.json`"));
    assert_eq!(
        fs::read(consumer.join("module-lock.json")).unwrap(),
        lock_before
    );
}

#[test]
fn lock_update_prompts_before_accepting_removed_signer_key() {
    let (fixture, old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-remove-prompt");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-signer-remove-prompt",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = output_with_stdin(lock_command, "y\n");
    assert!(lock.status.success());
    let before = fs::read(consumer.join("module-lock.json")).unwrap();

    add_unsigned_git_version(&fixture.repo_dir, "1.1.3");

    let mut update_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = output_with_stdin(update_command, "\n");
    assert!(
        !update.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&update.stdout)
    );
    let stderr = String::from_utf8_lossy(&update.stderr);
    assert!(stderr.contains("[y/N]"));
    assert!(stderr.contains("signer key removed"));
    assert!(stderr.contains("sprocket dev module trust all"));
    assert_eq!(fs::read(consumer.join("module-lock.json")).unwrap(), before);
    assert!(stderr.contains(&old_public_key));
}

#[test]
fn lock_update_accepts_changed_signer_key_when_confirmed() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-accept");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-signer-accept",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
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

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    let new_public_key = new_key.verifying_key().to_openssh();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut update_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = output_with_stdin(update_command, "y\n");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Accepted 1 signer change"), "{stdout}");
    assert!(stdout.contains("Signer"), "{stdout}");
    assert!(stdout.contains("signer changed"), "{stdout}");
    assert!(stdout.contains("Trusted 1 signer key"), "{stdout}");

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
    assert!(String::from_utf8_lossy(&list.stdout).contains(&new_public_key));

    let lock = read_lockfile(&consumer);
    assert_eq!(
        locked_git_selector(&lock, "tasks"),
        "version ^1.0",
        "updated lock should keep the manifest selector"
    );
}

#[test]
fn lock_update_tofu_prompts_before_accepting_changed_signer_key() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-tofu");
    set_fixture_trust_mode(&fixture, "tofu");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-signer-tofu",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
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

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut update_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = output_with_stdin(update_command, "\n");
    assert!(
        !update.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&update.stdout)
    );
    assert!(String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
}

#[test]
fn lock_update_auto_accepts_changed_signer_key_without_prompting() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-auto");
    set_fixture_trust_mode(&fixture, "auto-accept");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-signer-auto",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
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

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    let new_public_key = new_key.verifying_key().to_openssh();
    add_signed_git_version(&fixture.repo_dir, "1.1.0", &new_key);

    let mut update_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket dev module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Accepted 1 signer change"), "{stdout}");
    assert!(stdout.contains("Signer"), "{stdout}");
    assert!(stdout.contains("signer changed"), "{stdout}");
    assert!(stdout.contains("Trusted 1 signer key"), "{stdout}");

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(String::from_utf8_lossy(&list.stdout).contains(&new_public_key));
}

#[test]
fn lock_update_trust_mode_flag_auto_accepts_without_prompting() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-auto-flag");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-auto-flag",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "lock", "--trust-mode", "auto-accept"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(lock.status.success());

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "1.1.0", &new_key);

    let mut update_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "update", "--trust-mode", "auto-accept"],
    );
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket dev module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Accepted 1 signer change"), "{stdout}");
    assert!(stdout.contains("Signer"), "{stdout}");
    assert!(stdout.contains("signer changed"), "{stdout}");
    assert!(stdout.contains("Trusted 1 signer key"), "{stdout}");
}

#[test]
fn lock_update_trust_mode_flag_auto_accepts_removed_signer_without_prompting() {
    let (fixture, old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-remove-auto-flag");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-signer-remove-auto-flag",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "lock", "--trust-mode", "auto-accept"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(lock.status.success());

    add_unsigned_git_version(&fixture.repo_dir, "1.1.4");

    let mut update_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "update", "--trust-mode", "auto-accept"],
    );
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket dev module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Accepted 1 signer change"), "{stdout}");
    assert!(stdout.contains("Signer"), "{stdout}");
    assert!(stdout.contains("signer removed"), "{stdout}");

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(
        String::from_utf8_lossy(&list.stdout).contains(&old_public_key),
        "accepting a removed module signature should not remove global trust for the signer key"
    );
}

#[test]
fn lock_update_trust_mode_flag_auto_accepts_unsigned_to_signed_without_prompting() {
    let fixture = GitFixture::new();
    let home = isolated_home(
        fixture.dir.path(),
        "home-update-auto-flag-unsigned-to-signed",
    );
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-auto-flag-unsigned-to-signed",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "lock", "--trust-mode", "auto-accept"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(lock.status.success());

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    let new_public_key = new_key.verifying_key().to_openssh();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut update_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "update", "--trust-mode", "auto-accept"],
    );
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket dev module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Accepted 1 signer change"), "{stdout}");
    assert!(stdout.contains("Signer"), "{stdout}");
    assert!(stdout.contains("signer added"), "{stdout}");
    assert!(stdout.contains("Trusted 1 signer key"), "{stdout}");

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(String::from_utf8_lossy(&list.stdout).contains(&new_public_key));
}

#[test]
fn lock_update_skips_git_dependency_that_is_latest() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let latest = fixture.head_commit();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-latest",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
        ),
    );

    let lock = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );
    let before = fs::read(consumer.join("module-lock.json")).unwrap();

    let update = sprocket_with_config(fixture.config_path(), &["dev", "module", "update"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Current module lockfile is up to date"));

    let after = fs::read(consumer.join("module-lock.json")).unwrap();
    assert_eq!(after, before);

    let list = sprocket_with_config(fixture.config_path(), &["dev", "module", "list"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains(&format!(
        "(source: {repo_url}, selector: branch `{default_branch}` @{}, path: tasks)",
        &latest[..7]
    )));
}

#[test]
fn update_unknown_name_errors() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-update-unknown",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let output = sprocket_with_config(fixture.config_path(), &["dev", "module", "update", "nope"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module update");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn update_named_only() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-update-named",
        &format!(
            r#"    "tasks": {{ "git": "{repo_url}", "version": "=1.0.0", "path": "tasks" }},
    "stable": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#
        ),
    );

    let first_lock = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(
        first_lock.status.success(),
        "command failed {status}: {stderr}",
        status = first_lock.status,
        stderr = String::from_utf8_lossy(&first_lock.stderr)
    );

    let lock_before = read_lockfile(&consumer);
    assert_eq!(locked_git_selector(&lock_before, "tasks"), "version =1.0.0");
    let stable_before = locked_git_commit(&lock_before, "stable");

    fs::write(
        consumer.join("module.json"),
        format!(
            r#"{{
  "name": "consumer",
  "license": "MIT",
  "entrypoint": "index.wdl",
  "dependencies": {{
    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }},
    "stable": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}
  }}
}}
"#
        ),
    )
    .unwrap();

    let update = sprocket_with_config(fixture.config_path(), &["dev", "module", "update", "tasks"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module update tasks");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );

    let lock_after = read_lockfile(&consumer);
    assert_eq!(locked_git_selector(&lock_after, "tasks"), "version ^1.0");
    assert_eq!(locked_git_commit(&lock_after, "stable"), stable_before);
}

#[test]
fn lock_update_signer_transition_matrix_respects_trust_mode() {
    let cases = [
        (
            SignerTransition::Added,
            CliTrustMode::Confirm,
            false,
            true,
            "previously unsigned module",
        ),
        (
            SignerTransition::Added,
            CliTrustMode::Tofu,
            false,
            true,
            "previously unsigned module",
        ),
        (
            SignerTransition::Added,
            CliTrustMode::AutoAccept,
            true,
            false,
            "",
        ),
        (
            SignerTransition::Changed,
            CliTrustMode::Confirm,
            false,
            true,
            "signer key changed",
        ),
        (
            SignerTransition::Changed,
            CliTrustMode::Tofu,
            false,
            true,
            "signer key changed",
        ),
        (
            SignerTransition::Changed,
            CliTrustMode::AutoAccept,
            true,
            false,
            "",
        ),
        (
            SignerTransition::Removed,
            CliTrustMode::Confirm,
            false,
            true,
            "signer key removed",
        ),
        (
            SignerTransition::Removed,
            CliTrustMode::Tofu,
            false,
            true,
            "signer key removed",
        ),
        (
            SignerTransition::Removed,
            CliTrustMode::AutoAccept,
            true,
            false,
            "",
        ),
    ];

    for (transition, mode, expect_success, expect_prompt, expected_phrase) in cases {
        let (fixture, consumer) = stage_update_transition(transition);
        let mut command = sprocket_with_config(
            fixture.config_path(),
            &["dev", "module", "update", "--trust-mode", mode.as_arg()],
        );
        command.current_dir(&consumer);
        let output = output_with_stdin(command, "\n");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.success(),
            expect_success,
            "transition={transition:?} mode={mode:?} stderr={stderr}"
        );
        assert_eq!(
            stderr.contains("[y/N]"),
            expect_prompt,
            "transition={transition:?} mode={mode:?} stderr={stderr}"
        );
        if !expected_phrase.is_empty() {
            assert!(
                stderr.contains(expected_phrase),
                "transition={transition:?} mode={mode:?} stderr={stderr}"
            );
        }
        if expect_success {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains("Accepted 1 signer change"),
                "transition={transition:?} mode={mode:?} stdout={stdout}"
            );
            assert!(
                stdout.contains("Signer"),
                "transition={transition:?} mode={mode:?} stdout={stdout}"
            );
            match transition {
                SignerTransition::Added => assert!(
                    stdout.contains("signer added"),
                    "transition={transition:?} mode={mode:?} stdout={stdout}"
                ),
                SignerTransition::Changed => assert!(
                    stdout.contains("signer changed"),
                    "transition={transition:?} mode={mode:?} stdout={stdout}"
                ),
                SignerTransition::Removed => assert!(
                    stdout.contains("signer removed"),
                    "transition={transition:?} mode={mode:?} stdout={stdout}"
                ),
            }
        }
    }
}

#[test]
fn lock_update_mixed_signer_batch_is_all_or_nothing() {
    let batch = stage_mixed_signer_batch();

    // The lockfile the refused update must leave byte-for-byte untouched.
    let lock_path = batch.consumer.join("module-lock.json");
    let lock_before = fs::read(&lock_path).expect("baseline lockfile should exist");

    let mut command = sprocket_with_config(
        &batch.config_path,
        &["dev", "module", "update", "--trust-mode", "tofu"],
    );
    command.current_dir(&batch.consumer);
    // Decline the batched confirmation for the refused (changed-signer) half.
    let output = output_with_stdin(command, "\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The mixed batch is refused as a single unit.
    assert!(
        !output.status.success(),
        "a refused mixed batch must fail the update: stderr={stderr}"
    );
    // The refusable change routes the whole batch through one prompt...
    assert!(
        stderr.contains("[y/N]"),
        "the changed signer should drive a single batched prompt: stderr={stderr}"
    );
    // ...and declining it accepts and trusts nothing.
    assert!(
        !stdout.contains("Accepted"),
        "a refused batch must not report any accepted change: stdout={stdout}"
    );

    // All-or-nothing: the proposed lockfile is never written.
    let lock_after = fs::read(&lock_path).expect("lockfile should still exist");
    assert_eq!(
        lock_after, lock_before,
        "a refused update must not rewrite `module-lock.json`"
    );
    // The brand-new, otherwise auto-accepted dependency is not locked.
    let lock = read_lockfile(&batch.consumer);
    let alpha: wdl_modules::dependency::DependencyName = "alpha".parse().unwrap();
    assert!(
        !lock.dependencies.contains_key(&alpha),
        "the auto-acceptable `alpha` dependency must not be locked when the batch is refused"
    );

    // All-or-nothing: the otherwise auto-accepted signer key is not trusted.
    let trust_path = shared_trust_store_path();
    if trust_path.exists() {
        let trust = fs::read_to_string(&trust_path).expect("trust store should be readable");
        assert!(
            !trust.contains(&batch.auto_accept_key),
            "the auto-accepted signer key must not persist to the trust store after a refusal"
        );
    }
}
