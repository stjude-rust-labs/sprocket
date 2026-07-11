//! Integration tests for `sprocket dev module sign` and `sprocket dev module
//! verify`.

use std::fs;

use wdl_modules::dependency::DependencyName;
use wdl_modules::hash::hash_directory;
use wdl_modules::signing::ModuleSignature;

use crate::fixtures::*;

#[test]
fn verify_succeeds_after_lock() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-verify",
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

    let verify = sprocket_with_config(fixture.config_path(), &["dev", "module", "verify"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module verify");
    assert!(
        verify.status.success(),
        "command failed {status}: {stderr}",
        status = verify.status,
        stderr = String::from_utf8_lossy(&verify.stderr)
    );
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("Verified"));
    assert!(stdout.contains("Skipped signature verification for current module (no `module.sig`)"));
    assert!(stdout.contains("Skipped signature verification for 1 dependency without a signature"));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(!stderr.contains("cryptographic signature"));

    let config_path = fixture.config_path().to_string_lossy().into_owned();
    let colored = sprocket_with_global_args(
        &["--config", &config_path, "--color", "always"],
        &["dev", "module", "verify"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run colored sprocket dev module verify");
    assert!(
        colored.status.success(),
        "command failed {status}: {stderr}",
        status = colored.status,
        stderr = String::from_utf8_lossy(&colored.stderr)
    );
    assert!(String::from_utf8_lossy(&colored.stdout).contains("\u{1b}[1;36mSkipped\u{1b}[0m"));
}

#[test]
fn verify_strict_requires_all_packages_to_be_signed() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-verify-strict",
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

    let verify = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "verify", "--strict"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket dev module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("Failed signature verification for current module (no `module.sig`)"));
    assert!(stdout.contains("Failed signature verification for 1 dependency without a signature"));
    assert!(!stdout.contains("Failed strict signature verification"));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("strict verification requires signatures for every package"));
    assert!(stderr.contains("`consumer` (current module) has no `module.sig`"));
    assert!(stderr.contains("dependency `tasks` has no `module.sig`"));

    let config_path = fixture.config_path().to_string_lossy().into_owned();
    let colored = sprocket_with_global_args(
        &["--config", &config_path, "--color", "always"],
        &["dev", "module", "verify", "--strict"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run colored sprocket dev module verify");
    assert!(
        !colored.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&colored.stdout)
    );
    assert!(String::from_utf8_lossy(&colored.stdout).contains("\u{1b}[1;31mFailed\u{1b}[0m"));
}

#[test]
fn verify_reports_all_untrusted_modules_at_once() {
    let (fixture, _public_key) = GitFixture::signed_without_version_tags();
    let home = isolated_home(fixture.dir.path(), "home-verify-all-untrusted");
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer(
        "consumer-verify-all-untrusted",
        &format!(
            r#"    "task_a": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }},
    "task_b": {{ "git": "{repo_url}", "branch": "{default_branch}", "path": "tasks" }}"#
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
    assert!(lock.status.success());

    let lockfile = read_lockfile(&consumer);
    let dep_name: DependencyName = "task_a".parse().unwrap();
    let public_key = lockfile
        .dependencies
        .get(&dep_name)
        .and_then(|entry| entry.signer)
        .expect("locked dependency should record a signer")
        .to_openssh();

    let mut trust_remove = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "trust", "remove", &public_key],
    );
    trust_remove.current_dir(&consumer);
    use_home(&mut trust_remove, &home);
    let remove = trust_remove
        .output()
        .expect("failed to run sprocket dev module trust remove");
    assert!(remove.status.success());

    let mut verify_command = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "verify", "lockfile"],
    );
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
    assert!(stderr.contains("2 modules are untrusted"));
    assert!(stderr.contains("`task_a` signer is untrusted"));
    assert!(stderr.contains("`task_b` signer is untrusted"));
    assert!(stderr.contains("sprocket dev module trust all"));
}

#[test]
fn verify_fails_on_tampered_cache() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-verify-tamper",
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

    assert!(
        overwrite_first_file_named(&fixture.cache_path(), "index.wdl", "version 1.0\n"),
        "expected to find cached index.wdl to tamper with"
    );

    let verify = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "verify", "lockfile"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket dev module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
}

#[test]
fn verify_without_lockfile_errors() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-verify-no-lock",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let verify = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "verify", "lockfile"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run sprocket dev module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    assert!(String::from_utf8_lossy(&verify.stderr).contains("sprocket dev module lock"));
}

#[test]
fn verify_reports_fetch_when_cache_missing() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-verify-no-cache",
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

    fs::remove_dir_all(fixture.cache_path()).unwrap();

    let verify = sprocket_with_config(fixture.config_path(), &["dev", "module", "verify"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket dev module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    assert!(String::from_utf8_lossy(&verify.stderr).contains("sprocket dev module fetch"));
}

#[test]
fn sign_writes_verifiable_signature() {
    let fixture = ModuleFixture::with_local_dep();
    let key_path = fixture.dir.path().join("id_ed25519");
    fs::write(&key_path, generate_openssh_ed25519_private_key()).unwrap();

    let key_path_arg = key_path.to_string_lossy().into_owned();
    let sign = sprocket(&["dev", "module", "sign", "--key", &key_path_arg])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module sign");
    assert!(
        sign.status.success(),
        "command failed {status}: {stderr}",
        status = sign.status,
        stderr = String::from_utf8_lossy(&sign.stderr)
    );

    let sig_bytes = fs::read(fixture.consumer().join("module.sig")).unwrap();
    let signature = ModuleSignature::parse(&sig_bytes).unwrap();
    let digest = hash_directory(fixture.consumer()).unwrap();
    assert!(signature.verify(&digest).is_ok());

    let verify = sprocket(&["dev", "module", "verify", "signature"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module verify signature");
    assert!(
        verify.status.success(),
        "command failed {status}: {stderr}",
        status = verify.status,
        stderr = String::from_utf8_lossy(&verify.stderr)
    );
    assert!(String::from_utf8_lossy(&verify.stdout).contains("Verified signature"));

    let verify_all = sprocket(&["dev", "module", "verify"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module verify");
    assert!(
        verify_all.status.success(),
        "command failed {status}: {stderr}",
        status = verify_all.status,
        stderr = String::from_utf8_lossy(&verify_all.stderr)
    );
    assert!(String::from_utf8_lossy(&verify_all.stdout).contains("Verified signature"));
}

#[test]
fn verify_signature_without_signature_errors_with_guidance() {
    let fixture = ModuleFixture::with_local_dep();

    let verify = sprocket(&["dev", "module", "verify", "signature"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket dev module verify signature");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("no `module.sig`"));
    assert!(stderr.contains("sprocket dev module sign"));
}

#[test]
fn sign_uses_default_ed25519_key_when_key_is_omitted() {
    let fixture = ModuleFixture::with_local_dep();
    let home = fixture.dir.path().join("home");
    let ssh = home.join(".ssh");
    fs::create_dir_all(&ssh).unwrap();
    fs::write(
        ssh.join("id_ed25519"),
        generate_openssh_ed25519_private_key(),
    )
    .unwrap();
    fs::write(home.join(".gitconfig"), "").unwrap();

    let sign = sprocket(&["dev", "module", "sign"])
        .current_dir(fixture.consumer())
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .output()
        .expect("failed to run sprocket dev module sign");
    assert!(
        sign.status.success(),
        "command failed {status}: {stderr}",
        status = sign.status,
        stderr = String::from_utf8_lossy(&sign.stderr)
    );

    let sig_bytes = fs::read(fixture.consumer().join("module.sig")).unwrap();
    ModuleSignature::parse(&sig_bytes).unwrap();
}

#[test]
fn sign_without_default_key_errors_with_guidance() {
    let fixture = ModuleFixture::with_local_dep();
    let home = fixture.dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join(".gitconfig"), "").unwrap();

    let sign = sprocket(&["dev", "module", "sign"])
        .current_dir(fixture.consumer())
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .output()
        .expect("failed to run sprocket dev module sign");
    assert!(
        !sign.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&sign.stdout)
    );
    let stderr = String::from_utf8_lossy(&sign.stderr);
    assert!(stderr.contains("no ed25519 signing key found"));
    assert!(stderr.contains("specify `--key`"));
}
