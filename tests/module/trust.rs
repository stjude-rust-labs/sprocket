//! Integration tests for `sprocket dev module trust`.

use std::fs;

use crate::fixtures::*;

#[test]
fn trust_add_accepts_multiple_keys() {
    let dir = tempfile::tempdir().unwrap();

    let key_a = generate_openssh_ed25519_public_key();
    let key_b = generate_openssh_ed25519_public_key();

    let mut add_command = sprocket(&["dev", "module", "trust", "add", &key_a, &key_b]);
    add_command.current_dir(dir.path());
    use_home(&mut add_command, dir.path());
    let add = add_command
        .output()
        .expect("failed to run sprocket dev module trust add");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let mut list_command = sprocket(&["dev", "module", "trust", "list"]);
    list_command.current_dir(dir.path());
    use_home(&mut list_command, dir.path());
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains(&key_a));
    assert!(stdout.contains(&key_b));
}

#[test]
fn trust_commands_work_outside_a_module() {
    let dir = tempfile::tempdir().unwrap();
    let home = isolated_home(dir.path(), "home-global-trust");
    let mut command = sprocket(&["dev", "module", "trust", "list"]);
    command.current_dir(dir.path());
    use_home(&mut command, &home);
    let output = command.output().expect("failed to run sprocket");
    assert!(
        output.status.success(),
        "trust list should not require a `module.json`: {stderr}",
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("no trusted keys"));
}

#[test]
fn trust_add_then_list_shows_entry() {
    let dir = tempfile::tempdir().unwrap();

    let pub_key = generate_openssh_ed25519_public_key();

    let mut add_command = sprocket(&["dev", "module", "trust", "add", &pub_key]);
    add_command.current_dir(dir.path());
    use_home(&mut add_command, dir.path());
    let add = add_command
        .output()
        .expect("failed to run sprocket dev module trust add");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let mut list_command = sprocket(&["dev", "module", "trust", "list"]);
    list_command.current_dir(dir.path());
    use_home(&mut list_command, dir.path());
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains(&pub_key));
}

#[test]
fn trust_add_preserves_unstructured_public_key_comment() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let pub_key = format!("{} release signer", generate_openssh_ed25519_public_key());

    let mut add_command = sprocket(&["dev", "module", "trust", "add", &pub_key]);
    add_command.current_dir(dir.path());
    use_home(&mut add_command, dir.path());
    let add = add_command.output()?;
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let mut list_command = sprocket(&["dev", "module", "trust", "list"]);
    list_command.current_dir(dir.path());
    use_home(&mut list_command, dir.path());
    let list = list_command.output()?;
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains("release signer"));
    Ok(())
}

#[test]
fn trust_remove_drops_entry() {
    let dir = tempfile::tempdir().unwrap();

    let pub_key_path = dir.path().join("id_ed25519.pub");
    fs::write(&pub_key_path, generate_openssh_ed25519_public_key()).unwrap();

    let pub_key_arg = pub_key_path.to_string_lossy().into_owned();
    let mut add_command = sprocket(&["dev", "module", "trust", "add", &pub_key_arg]);
    add_command.current_dir(dir.path());
    use_home(&mut add_command, dir.path());
    let add = add_command
        .output()
        .expect("failed to run sprocket dev module trust add");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let mut remove_command = sprocket(&["dev", "module", "trust", "remove", &pub_key_arg]);
    remove_command.current_dir(dir.path());
    use_home(&mut remove_command, dir.path());
    let remove = remove_command
        .output()
        .expect("failed to run sprocket dev module trust remove");
    assert!(
        remove.status.success(),
        "command failed {status}: {stderr}",
        status = remove.status,
        stderr = String::from_utf8_lossy(&remove.stderr)
    );

    let mut list_command = sprocket(&["dev", "module", "trust", "list"]);
    list_command.current_dir(dir.path());
    use_home(&mut list_command, dir.path());
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains("no trusted keys"));
}

#[test]
fn trust_destroy_clears_all_entries() {
    let dir = tempfile::tempdir().unwrap();

    let key_a = generate_openssh_ed25519_public_key();
    let key_b = generate_openssh_ed25519_public_key();
    for key in [&key_a, &key_b] {
        let mut add_command = sprocket(&["dev", "module", "trust", "add", key]);
        add_command.current_dir(dir.path());
        use_home(&mut add_command, dir.path());
        let add = add_command
            .output()
            .expect("failed to run sprocket dev module trust add");
        assert!(
            add.status.success(),
            "command failed {status}: {stderr}",
            status = add.status,
            stderr = String::from_utf8_lossy(&add.stderr)
        );
    }

    let mut destroy_command = sprocket(&["dev", "module", "trust", "destroy"]);
    destroy_command.current_dir(dir.path());
    use_home(&mut destroy_command, dir.path());
    let destroy = destroy_command
        .output()
        .expect("failed to run sprocket dev module trust destroy");
    assert!(
        destroy.status.success(),
        "command failed {status}: {stderr}",
        status = destroy.status,
        stderr = String::from_utf8_lossy(&destroy.stderr)
    );
    assert!(String::from_utf8_lossy(&destroy.stdout).contains("Removed all trusted keys"));

    let mut list_command = sprocket(&["dev", "module", "trust", "list"]);
    list_command.current_dir(dir.path());
    use_home(&mut list_command, dir.path());
    let list = list_command
        .output()
        .expect("failed to run sprocket dev module trust list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains("no trusted keys"));
}

#[test]
fn trust_destroy_removes_identity_metadata() {
    const KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINiRUmfYzFTjksGItM2fSm9s1eCL8NnMJGQgW724Uph1";
    let dir = tempfile::tempdir().unwrap();
    let home = isolated_home(dir.path(), "home-trust-destroy");

    let mut add = sprocket(&["dev", "module", "trust", "add", KEY, "--name", "Alice"]);
    add.current_dir(dir.path());
    use_home(&mut add, &home);
    assert!(add.output().expect("trust add").status.success());

    let mut destroy = sprocket(&["dev", "module", "trust", "destroy"]);
    destroy.current_dir(dir.path());
    use_home(&mut destroy, &home);
    assert!(destroy.output().expect("trust destroy").status.success());

    // Re-adding the bare key must not resurrect the old identity.
    let mut re_add = sprocket(&["dev", "module", "trust", "add", KEY]);
    re_add.current_dir(dir.path());
    use_home(&mut re_add, &home);
    assert!(re_add.output().expect("trust add").status.success());

    let mut list = sprocket(&["dev", "module", "trust", "list"]);
    list.current_dir(dir.path());
    use_home(&mut list, &home);
    let list = list.output().expect("trust list");
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains(KEY), "stdout: {stdout}");
    assert!(
        !stdout.contains("Alice"),
        "destroyed identity metadata must not survive: {stdout}"
    );
}

#[test]
fn trust_all_trusts_locked_signers_without_relocking() {
    let (fixture, public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-trust-all-lockfile");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-trust-all-lockfile",
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

    let mut verify_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "verify"]);
    verify_command.current_dir(&consumer);
    use_home(&mut verify_command, &home);
    let verify = verify_command
        .output()
        .expect("failed to run sprocket dev module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("sprocket dev module trust all"));
    assert_eq!(fs::read(consumer.join("module-lock.json")).unwrap(), before);

    let mut trust_all_command =
        sprocket_with_config(fixture.config_path(), &["dev", "module", "trust", "all"]);
    trust_all_command.current_dir(&consumer);
    use_home(&mut trust_all_command, &home);
    let trust_all = trust_all_command
        .output()
        .expect("failed to run sprocket dev module trust all");
    assert!(
        trust_all.status.success(),
        "command failed {status}: {stderr}",
        status = trust_all.status,
        stderr = String::from_utf8_lossy(&trust_all.stderr)
    );
    assert!(String::from_utf8_lossy(&trust_all.stdout).contains("Trusted 1 signer keys"));
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
    assert!(String::from_utf8_lossy(&list.stdout).contains(&public_key));
}
