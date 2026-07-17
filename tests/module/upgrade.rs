//! Integration tests for `sprocket dev module upgrade`.

use std::fs;

use wdl_modules::signing::SigningKey;

use crate::fixtures::*;

#[test]
fn lock_upgrade_signer_transition_matrix_respects_trust_mode() {
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
        let (fixture, consumer) = stage_upgrade_transition(transition);
        let mut command = sprocket_with_config(
            fixture.config_path(),
            &["dev", "module", "upgrade", "--trust-mode", mode.as_arg()],
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
fn upgrade_raises_constraint_and_relocks() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
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
    let before = read_lockfile(&consumer);
    assert_eq!(locked_git_selector(&before, "tasks"), "version ^1.0");

    let upgrade = sprocket_with_config(fixture.config_path(), &["dev", "module", "upgrade"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module upgrade");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    let stdout = String::from_utf8_lossy(&upgrade.stdout);
    assert!(stdout.contains("Upgraded 1 dependency"));
    assert!(stdout.contains("tasks"));
    assert!(stdout.contains("v1.0 -> v2.0.0"));

    assert_eq!(
        manifest_dep_version(&consumer, "tasks").as_deref(),
        Some("^2.0.0")
    );
    let after = read_lockfile(&consumer);
    assert_eq!(locked_git_selector(&after, "tasks"), "version ^2.0.0");
}

#[test]
fn upgrade_dry_run_prints_changes_without_writing() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-dry-run",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
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
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();

    let upgrade = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "upgrade", "--dry-run"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket dev module upgrade --dry-run");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    let stdout = String::from_utf8_lossy(&upgrade.stdout);
    assert!(
        stdout.contains("Would upgrade 1 dependency")
            && stdout.contains("tasks")
            && stdout.contains("v1.0 -> v2.0.0"),
        "dry run should print the planned change, got: {stdout}"
    );
    assert!(
        stdout.contains("commit:"),
        "dry run should include the resolved lockfile change, got: {stdout}"
    );

    assert_eq!(
        fs::read(consumer.join("module.json")).unwrap(),
        manifest_before,
        "dry run must not modify `module.json`"
    );
    assert_eq!(
        fs::read(consumer.join("module-lock.json")).unwrap(),
        lock_before,
        "dry run must not modify `module-lock.json`"
    );
    assert!(!consumer.join(".sprocket").join("module-mutation").exists());
}

#[test]
fn upgrade_relocks_non_version_dependencies_too() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-relocks-all",
        &format!(
            r#"    "versioned": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }},
    "branched": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
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
    let lock_before = read_lockfile(&consumer);
    let branched_before = locked_git_commit(&lock_before, "branched");

    add_unsigned_git_version(&fixture.repo_dir, "2.0.1");
    let latest = fixture.head_commit();

    let upgrade = sprocket_with_config(fixture.config_path(), &["dev", "module", "upgrade"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module upgrade");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    let stdout = String::from_utf8_lossy(&upgrade.stdout);
    assert!(stdout.contains("branched"));
    assert!(stdout.contains("commit:"));

    let lock_after = read_lockfile(&consumer);
    assert_ne!(branched_before, latest);
    assert_eq!(locked_git_commit(&lock_after, "branched"), latest);
    assert_eq!(
        manifest_dep_version(&consumer, "versioned").as_deref(),
        Some("^2.0.1")
    );
}

#[test]
fn upgrade_prompts_before_accepting_changed_signer_key() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-upgrade-prompt");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-signer-prompt",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "=1.0.0", "path": "tasks" }}"#),
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
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "2.0.1", &new_key);

    let mut upgrade_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "upgrade"]);
    upgrade_command.current_dir(&consumer);
    use_home(&mut upgrade_command, &home);
    let upgrade = output_with_stdin(upgrade_command, "\n");
    assert!(
        !upgrade.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&upgrade.stdout)
    );
    let stderr = String::from_utf8_lossy(&upgrade.stderr);
    assert!(stderr.contains("module signer key requires trust"));
    assert!(stderr.contains("[y/N]"));
    assert!(stderr.contains("refusing to update `module-lock.json`"));
    assert_eq!(
        fs::read(consumer.join("module-lock.json")).unwrap(),
        lock_before
    );
    assert_eq!(
        fs::read(consumer.join("module.json")).unwrap(),
        manifest_before
    );
}

#[test]
fn upgrade_prompts_before_accepting_removed_signer_key() {
    let (fixture, old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-upgrade-remove-prompt");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-signer-remove-prompt",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "=1.0.0", "path": "tasks" }}"#),
    );
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = output_with_stdin(lock_command, "y\n");
    assert!(lock.status.success());
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();

    add_unsigned_git_version(&fixture.repo_dir, "1.1.5");

    let mut upgrade_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "upgrade"]);
    upgrade_command.current_dir(&consumer);
    use_home(&mut upgrade_command, &home);
    let upgrade = output_with_stdin(upgrade_command, "\n");
    assert!(
        !upgrade.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&upgrade.stdout)
    );
    let stderr = String::from_utf8_lossy(&upgrade.stderr);
    assert!(stderr.contains("[y/N]"));
    assert!(stderr.contains("signer key removed"));
    assert!(stderr.contains("sprocket dev module trust all"));
    assert_eq!(
        fs::read(consumer.join("module-lock.json")).unwrap(),
        lock_before
    );
    assert_eq!(
        fs::read(consumer.join("module.json")).unwrap(),
        manifest_before
    );
    assert!(stderr.contains(&old_public_key));
}

#[test]
fn upgrade_prompts_when_dependency_becomes_signed() {
    let fixture = GitFixture::new();
    let home = isolated_home(fixture.dir.path(), "home-upgrade-unsigned-to-signed");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-unsigned-to-signed",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "=1.0.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(lock.status.success());
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "2.0.1", &new_key);

    let mut upgrade_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "upgrade"]);
    upgrade_command.current_dir(&consumer);
    use_home(&mut upgrade_command, &home);
    let upgrade = output_with_stdin(upgrade_command, "\n");
    assert!(
        !upgrade.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&upgrade.stdout)
    );
    let stderr = String::from_utf8_lossy(&upgrade.stderr);
    assert!(stderr.contains("module signer key requires trust"));
    assert!(stderr.contains("[y/N]"));
    assert!(stderr.contains("refusing to update `module-lock.json`"));
    assert_eq!(
        fs::read(consumer.join("module-lock.json")).unwrap(),
        lock_before
    );
    assert_eq!(
        fs::read(consumer.join("module.json")).unwrap(),
        manifest_before
    );
}

#[test]
fn upgrade_trust_mode_flag_confirm_prompts_even_when_config_auto_accept() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    set_fixture_trust_mode(&fixture, "auto-accept");
    let home = isolated_home(fixture.dir.path(), "home-upgrade-confirm-flag");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-confirm-flag",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "=1.0.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["dev", "module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket dev module lock");
    assert!(lock.status.success());

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "1.1.0", &new_key);

    let mut upgrade_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "upgrade", "--trust-mode", "confirm"],
    );
    upgrade_command.current_dir(&consumer);
    use_home(&mut upgrade_command, &home);
    let upgrade = output_with_stdin(upgrade_command, "\n");
    assert!(
        !upgrade.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&upgrade.stdout)
    );
    assert!(String::from_utf8_lossy(&upgrade.stderr).contains("[y/N]"));
}

#[test]
fn upgrade_trust_mode_flag_auto_accepts_unsigned_to_signed_without_prompting() {
    let fixture = GitFixture::new();
    let home = isolated_home(
        fixture.dir.path(),
        "home-upgrade-auto-flag-unsigned-to-signed",
    );
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-auto-flag-unsigned-to-signed",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "=1.0.0", "path": "tasks" }}"#),
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
    add_signed_git_version(&fixture.repo_dir, "2.0.1", &new_key);

    let mut upgrade_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "upgrade", "--trust-mode", "auto-accept"],
    );
    upgrade_command.current_dir(&consumer);
    use_home(&mut upgrade_command, &home);
    let upgrade = upgrade_command
        .output()
        .expect("failed to run sprocket dev module upgrade");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    assert!(!String::from_utf8_lossy(&upgrade.stderr).contains("[y/N]"));
    let stdout = String::from_utf8_lossy(&upgrade.stdout);
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
fn upgrade_skips_non_version_dep() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-skip",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "tag": "v1.1.0" }}"#),
    );
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();

    let upgrade = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "upgrade", "--dry-run", "tasks"],
    )
    .current_dir(&consumer)
    .env("RUST_LOG", "info")
    .output()
    .expect("failed to run sprocket dev module upgrade --dry-run");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    assert!(
        String::from_utf8_lossy(&upgrade.stderr).contains("skipping `tasks`; no version selector")
    );
    assert!(!String::from_utf8_lossy(&upgrade.stdout).contains("Would update"));

    let manifest_after = fs::read(consumer.join("module.json")).unwrap();
    assert_eq!(manifest_after, manifest_before);
}

#[test]
fn upgrade_rejects_an_unknown_dependency() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-unknown",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let output = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "upgrade", "missing", "--dry-run"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket dev module upgrade");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("dependency `missing` not found in `module.json`"),
        "stderr: {stderr}"
    );
}

#[test]
fn upgrade_reports_current_version_constraints() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-current",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^2.0.0", "path": "tasks" }}"#),
    );

    let output = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "upgrade", "--dry-run"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket dev module upgrade");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("all version constraints"),
        "stdout: {stdout}"
    );
}
