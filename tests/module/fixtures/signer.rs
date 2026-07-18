//! Signer transition scenarios and trust configuration helpers.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use git2::Repository;
use wdl_modules::hash::hash_directory;
use wdl_modules::signing::ModuleSignature;
use wdl_modules::signing::SigningKey;

use super::command::isolated_home;
use super::command::sprocket_with_config;
use super::command::use_home;
use super::git::GitFixture;
use super::git::add_signed_git_version;
use super::git::add_unsigned_git_version;
use super::git::commit_and_tag;
use super::git::commit_without_tags;
use super::git::write_git_module;
use super::git::write_signed_git_module;

impl GitFixture {
    pub(crate) fn signed_initial_version() -> (Self, String) {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("tasks-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        let repo = Repository::init(&repo_dir).unwrap();
        let private_key = generate_openssh_ed25519_private_key();
        let signing_key = SigningKey::from_openssh(&private_key).unwrap();
        write_signed_git_module(&repo_dir, "1.0.0", &signing_key);
        let public_key = signing_key.verifying_key().to_openssh();
        commit_and_tag(&repo, "add signed tasks v1.0.0", "1.0.0");

        let cache_path = dir.path().join("module-cache");
        let cache_path = serde_json::to_string(&cache_path.to_string_lossy()).unwrap();
        let config_path = dir.path().join("sprocket.toml");
        let config = format!(
            "[modules]\ncache_path = {cache_path}\nallowed_schemes = [\"file\", \"https\", \
             \"ssh\"]\ndenied_hosts = []\n"
        );
        fs::write(&config_path, config).unwrap();

        (
            Self {
                dir,
                repo_dir,
                config_path,
            },
            public_key,
        )
    }

    pub(crate) fn signed_without_version_tags() -> (Self, String) {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("tasks-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        let repo = Repository::init(&repo_dir).unwrap();
        write_git_module(&repo_dir, "1.0.0");

        let private_key = generate_openssh_ed25519_private_key();
        let signing_key = SigningKey::from_openssh(&private_key).unwrap();
        let module_root = repo_dir.join("tasks");
        let checksum = hash_directory(&module_root).unwrap();
        // SAFETY: `None` contains no invalid signer identity fields.
        let signature = ModuleSignature::new(&signing_key, &checksum, None).unwrap();
        let mut sig_bytes = Vec::new();
        signature.write(&mut sig_bytes).unwrap();
        fs::write(module_root.join("module.sig"), sig_bytes).unwrap();
        let public_key = signing_key.verifying_key().to_openssh();

        commit_without_tags(&repo, "add signed tasks");

        let cache_path = dir.path().join("module-cache");
        let cache_path = serde_json::to_string(&cache_path.to_string_lossy()).unwrap();
        let config_path = dir.path().join("sprocket.toml");
        let config = format!(
            "[common.wdl.feature_flags]\nwdl_1_4 = true\n\n[modules]\ncache_path = \
             {cache_path}\nallowed_schemes = [\"file\", \"https\", \"ssh\"]\ndenied_hosts = []\n"
        );
        fs::write(&config_path, config).unwrap();

        (
            Self {
                dir,
                repo_dir,
                config_path,
            },
            public_key,
        )
    }

    /// Builds a signed single-version fixture whose repository carries a
    /// `.gitattributes` demanding CRLF line endings for its text files.
    ///
    /// The module is signed over the on-disk (LF) content, exactly as the
    /// other signed fixtures are. The `.gitattributes` file lives inside the
    /// `tasks/` module root so that it is materialized as part of the
    /// resolver's path-filtered (sparse) checkout and thus governs its
    /// sibling files; a repository-root `.gitattributes` would never be
    /// written to disk (only `tasks/**` is checked out) and so would not
    /// take effect. Because the `.wdl`/`.json` patterns do not match
    /// `.gitattributes` itself, the file is byte-stable and safe to include
    /// in the signed content hash.
    ///
    /// Attribute-driven end-of-line conversion is active on every platform, not
    /// just Windows, so resolving this fixture exercises the resolver's
    /// `disable_filters(true)` guarantee everywhere. Without that guarantee the
    /// checked-out `.wdl`/`.json` files would gain CRLF line endings, changing
    /// the content hash and failing signature verification.
    pub(crate) fn signed_with_crlf_attributes() -> (Self, String) {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("tasks-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        let repo = Repository::init(&repo_dir).unwrap();
        write_git_module(&repo_dir, "1.0.0");
        // Force CRLF in the working tree for the module's text files. Git
        // normalizes these blobs to LF in the object database on commit, so
        // the fixture signs over LF content but any filtered checkout would
        // materialize CRLF.
        fs::write(
            repo_dir.join("tasks").join(".gitattributes"),
            "*.wdl text eol=crlf\n*.json text eol=crlf\n",
        )
        .unwrap();

        let private_key = generate_openssh_ed25519_private_key();
        let signing_key = SigningKey::from_openssh(&private_key).unwrap();
        let module_root = repo_dir.join("tasks");
        let checksum = hash_directory(&module_root).unwrap();
        // SAFETY: `None` contains no invalid signer identity fields.
        let signature = ModuleSignature::new(&signing_key, &checksum, None).unwrap();
        let mut sig_bytes = Vec::new();
        signature.write(&mut sig_bytes).unwrap();
        fs::write(module_root.join("module.sig"), sig_bytes).unwrap();
        let public_key = signing_key.verifying_key().to_openssh();

        commit_without_tags(&repo, "add signed tasks with crlf attributes");

        let cache_path = dir.path().join("module-cache");
        let cache_path = serde_json::to_string(&cache_path.to_string_lossy()).unwrap();
        let config_path = dir.path().join("sprocket.toml");
        let config = format!(
            "[common.wdl.feature_flags]\nwdl_1_4 = true\n\n[modules]\ncache_path = \
             {cache_path}\nallowed_schemes = [\"file\", \"https\", \"ssh\"]\ndenied_hosts = []\n"
        );
        fs::write(&config_path, config).unwrap();

        (
            Self {
                dir,
                repo_dir,
                config_path,
            },
            public_key,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SignerTransition {
    Added,
    Changed,
    Removed,
}

#[derive(Debug)]
pub(crate) struct SignerScenario {
    pub(crate) fixture: GitFixture,
    pub(crate) consumer: PathBuf,
}

impl SignerScenario {
    pub(crate) fn for_update(transition: SignerTransition) -> Self {
        let (fixture, consumer) = stage_update_transition(transition);
        Self { fixture, consumer }
    }

    pub(crate) fn for_upgrade(transition: SignerTransition) -> Self {
        let (fixture, consumer) = stage_upgrade_transition(transition);
        Self { fixture, consumer }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CliTrustMode {
    Confirm,
    Tofu,
    AutoAccept,
}

impl CliTrustMode {
    pub(crate) fn as_arg(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Tofu => "tofu",
            Self::AutoAccept => "auto-accept",
        }
    }
}

fn stage_update_transition(transition: SignerTransition) -> (GitFixture, PathBuf) {
    let fixture = match transition {
        SignerTransition::Added => GitFixture::new(),
        SignerTransition::Changed | SignerTransition::Removed => {
            GitFixture::signed_initial_version().0
        }
    };
    let repo_url = fixture.repo_url();
    let case_name = match transition {
        SignerTransition::Added => "consumer-update-matrix-added",
        SignerTransition::Changed => "consumer-update-matrix-changed",
        SignerTransition::Removed => "consumer-update-matrix-removed",
    };
    let consumer = fixture.write_consumer(
        case_name,
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let lock = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "lock", "--trust-mode", "auto-accept"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run setup lock");
    assert!(
        lock.status.success(),
        "setup lock failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    match transition {
        SignerTransition::Added => {
            let new_key =
                SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
            add_signed_git_version(&fixture.repo_dir, "1.1.6", &new_key);
        }
        SignerTransition::Changed => {
            let new_key =
                SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
            add_signed_git_version(&fixture.repo_dir, "1.1.6", &new_key);
        }
        SignerTransition::Removed => add_unsigned_git_version(&fixture.repo_dir, "1.1.6"),
    }

    (fixture, consumer)
}

fn stage_upgrade_transition(transition: SignerTransition) -> (GitFixture, PathBuf) {
    let fixture = match transition {
        SignerTransition::Added => GitFixture::new(),
        SignerTransition::Changed | SignerTransition::Removed => {
            GitFixture::signed_initial_version().0
        }
    };
    let repo_url = fixture.repo_url();
    let case_name = match transition {
        SignerTransition::Added => "consumer-upgrade-matrix-added",
        SignerTransition::Changed => "consumer-upgrade-matrix-changed",
        SignerTransition::Removed => "consumer-upgrade-matrix-removed",
    };
    let consumer = fixture.write_consumer(
        case_name,
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "=1.0.0", "path": "tasks" }}"#),
    );

    let lock = sprocket_with_config(
        fixture.config_path(),
        &["dev", "module", "lock", "--trust-mode", "auto-accept"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run setup lock");
    assert!(
        lock.status.success(),
        "setup lock failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    match transition {
        SignerTransition::Added => {
            let new_key =
                SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
            add_signed_git_version(&fixture.repo_dir, "2.0.2", &new_key);
        }
        SignerTransition::Changed => {
            let new_key =
                SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
            add_signed_git_version(&fixture.repo_dir, "1.1.6", &new_key);
        }
        SignerTransition::Removed => add_unsigned_git_version(&fixture.repo_dir, "1.1.6"),
    }

    (fixture, consumer)
}

pub(crate) fn set_fixture_trust_mode(fixture: &GitFixture, trust_mode: &str) {
    let cache_path = serde_json::to_string(&fixture.cache_path().to_string_lossy()).unwrap();
    fs::write(
        fixture.config_path(),
        format!(
            "[common.wdl.feature_flags]\nwdl_1_4 = true\n\n[modules]\ncache_path = \
             {cache_path}\nallowed_schemes = [\"file\", \"https\", \"ssh\"]\ndenied_hosts = \
             []\ntrust_mode = \"{trust_mode}\"\n"
        ),
    )
    .unwrap();
}

/// A staged mixed-signer batch: the next `update` produces one auto-acceptable
/// and one refused signer transition in the same command.
///
/// `alpha` is a brand-new signed dependency (a `NewSigner`, which TOFU trusts
/// automatically) added to the manifest after the baseline lock. `beta` is an
/// already-trusted dependency whose signer key is rotated (a changed signer,
/// which TOFU refuses without an interactive yes). Because a single trust mode
/// never both auto-accepts one transition and hard-refuses another, this
/// CLI-reachable mix necessarily routes the refusable change through the batch
/// prompt; declining it must void the whole batch.
pub(crate) struct MixedSignerBatch {
    /// Keeps the fixture's temporary tree (repos, cache, consumer) alive.
    _dir: tempfile::TempDir,
    /// Path to the shared `sprocket.toml` used for every command.
    pub(crate) config_path: PathBuf,
    /// Scenario-private home directory. Every command in the scenario runs with
    /// this as `$HOME` (via [`use_home`]), so the trust store and cache live
    /// under `$HOME/.config/sprocket` instead of the process-shared config
    /// root.
    pub(crate) home: PathBuf,
    /// Path to the consumer project whose lockfile the update targets.
    pub(crate) consumer: PathBuf,
    /// OpenSSH public key of `alpha`'s signer: the key TOFU would auto-trust
    /// on its own but must not persist when the batch is refused.
    pub(crate) auto_accept_key: String,
}

impl MixedSignerBatch {
    /// Path to this scenario's private trust store.
    ///
    /// [`use_home`] points `SPROCKET_CONFIG_ROOT` at `$HOME/.config/sprocket`,
    /// so every command in the scenario reads and writes its trust store here.
    pub(crate) fn trust_store_path(&self) -> PathBuf {
        self.home
            .join(".config")
            .join("sprocket")
            .join("modules-trust.toml")
    }
}

/// Writes a consumer `module.json`/`index.wdl` with the given dependency block.
fn write_consumer_manifest(consumer: &Path, dependencies: &str) {
    fs::create_dir_all(consumer).unwrap();
    fs::write(
        consumer.join("module.json"),
        format!(
            r#"{{
  "name": "consumer",
  "license": "MIT",
  "entrypoint": "index.wdl",
  "dependencies": {{
{dependencies}
  }}
}}
"#
        ),
    )
    .unwrap();
    fs::write(consumer.join("index.wdl"), "version 1.3\n").unwrap();
}

/// Stages the [`MixedSignerBatch`] scenario described on that type.
pub(crate) fn stage_mixed_signer_batch() -> MixedSignerBatch {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    // A scenario-private home so the trust store and cache stay under
    // `$HOME/.config/sprocket` instead of the process-shared config root; every
    // command below runs with this home via `use_home`.
    let home = isolated_home(base, "home-mixed-signer-batch");

    // `alpha`: a brand-new signed dependency. As a fresh lockfile entry its
    // signer is a `NewSigner`, which TOFU trusts automatically.
    let alpha_repo = base.join("alpha-repo");
    fs::create_dir_all(&alpha_repo).unwrap();
    let alpha_git = Repository::init(&alpha_repo).unwrap();
    let alpha_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    write_signed_git_module(&alpha_repo, "1.0.0", &alpha_key);
    commit_and_tag(&alpha_git, "add signed alpha v1.0.0", "1.0.0");
    let auto_accept_key = alpha_key.verifying_key().to_openssh();
    let alpha_url = url::Url::from_file_path(&alpha_repo).unwrap().to_string();

    // `beta`: a signed dependency that is trusted at baseline and then has its
    // signer key rotated, producing a changed signer TOFU refuses to accept
    // without an interactive yes.
    let beta_repo = base.join("beta-repo");
    fs::create_dir_all(&beta_repo).unwrap();
    let beta_git = Repository::init(&beta_repo).unwrap();
    let beta_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    write_signed_git_module(&beta_repo, "1.0.0", &beta_key);
    commit_and_tag(&beta_git, "add signed beta v1.0.0", "1.0.0");
    let beta_url = url::Url::from_file_path(&beta_repo).unwrap().to_string();

    // A single config: one trust store, one cache, `file://` repos allowed.
    let cache_path = serde_json::to_string(&base.join("module-cache").to_string_lossy()).unwrap();
    let config_path = base.join("sprocket.toml");
    fs::write(
        &config_path,
        format!(
            "[modules]\ncache_path = {cache_path}\nallowed_schemes = [\"file\", \"https\", \
             \"ssh\"]\ndenied_hosts = []\n"
        ),
    )
    .unwrap();

    // Baseline: depend on `beta` only, then lock and trust its first key.
    let consumer = base.join("consumer-mixed-batch");
    write_consumer_manifest(
        &consumer,
        &format!(r#"    "beta": {{ "git": "{beta_url}", "version": "^1.0", "path": "tasks" }}"#),
    );
    let mut lock_command = sprocket_with_config(
        &config_path,
        &["dev", "module", "lock", "--trust-mode", "auto-accept"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command.output().expect("failed to run baseline lock");
    assert!(
        lock.status.success(),
        "baseline lock failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    // Introduce the brand-new signed `alpha` dependency (auto-acceptable) and
    // rotate `beta`'s signer key (refusable) so the next update batches both.
    write_consumer_manifest(
        &consumer,
        &format!(
            "    \"alpha\": {{ \"git\": \"{alpha_url}\", \"version\": \"^1.0\", \"path\": \
             \"tasks\" }},\n    \"beta\": {{ \"git\": \"{beta_url}\", \"version\": \"^1.0\", \
             \"path\": \"tasks\" }}"
        ),
    );
    let beta_rotated_key =
        SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&beta_repo, "1.1.6", &beta_rotated_key);

    MixedSignerBatch {
        _dir: dir,
        config_path,
        home,
        consumer,
        auto_accept_key,
    }
}

pub(crate) fn generate_openssh_ed25519_private_key() -> String {
    let mut rng = ssh_key::rand_core::OsRng;
    let key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
    key.to_openssh(ssh_key::LineEnding::LF).unwrap().to_string()
}

pub(crate) fn generate_openssh_ed25519_public_key() -> String {
    let mut rng = ssh_key::rand_core::OsRng;
    let key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
    key.public_key().to_openssh().unwrap().to_string()
}
