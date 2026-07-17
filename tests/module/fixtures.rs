//! Shared fixtures and helpers for the `sprocket dev module` integration tests.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::OnceLock;

use git2::Repository;
use wdl_modules::Lockfile;
use wdl_modules::dependency::DependencyName;
use wdl_modules::dependency::GitSelector;
use wdl_modules::hash::hash_directory;
use wdl_modules::lockfile::ResolvedSource;
use wdl_modules::signing::ModuleSignature;
use wdl_modules::signing::SigningKey;

pub(crate) fn sprocket(args: &[&str]) -> Command {
    sprocket_with_global_args(&[], args)
}

/// A per-process temporary configuration root shared by every spawned
/// `sprocket` command.
///
/// The `sprocket` binary resolves its trust store and module cache under
/// `SPROCKET_CONFIG_ROOT` (see `sprocket::config::config_root`). Pointing that
/// at a fresh temporary directory keeps tests from touching (or racing on) the
/// developer's or CI runner's real Sprocket config directory. Under nextest
/// each test runs in its own process, so this yields full per-test isolation;
/// under plain `cargo test` it is a single fresh directory per run, which is
/// still strictly better than the real config directory. The `TempDir` is kept
/// alive for the lifetime of the test process so the directory is not deleted
/// out from under running commands.
pub(crate) static SHARED_CONFIG_ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();

pub(crate) fn sprocket_with_global_args(global_args: &[&str], args: &[&str]) -> Command {
    let config_root = SHARED_CONFIG_ROOT
        .get_or_init(|| tempfile::tempdir().expect("failed to create shared config root"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_sprocket"));
    command
        .arg("--skip-config-search")
        .args(global_args)
        .args(args)
        .env("SPROCKET_CONFIG_ROOT", config_root.path())
        .env("RUST_LOG", "none")
        .env_remove("RUST_BACKTRACE");
    command
}

pub(crate) fn sprocket_with_config(config_path: &Path, args: &[&str]) -> Command {
    let config_path = config_path.to_string_lossy().into_owned();
    sprocket_with_global_args(&["--config", &config_path], args)
}

pub(crate) fn output_with_stdin(mut command: Command, input: &str) -> std::process::Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn sprocket");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(input.as_bytes())
        .expect("failed to write prompt input");
    child
        .wait_with_output()
        .expect("failed to wait on sprocket")
}

pub(crate) fn isolated_home(base: &Path, name: &str) -> PathBuf {
    let home = base.join(name);
    fs::create_dir_all(&home).unwrap();
    home
}

pub(crate) fn use_home(command: &mut Command, home: &Path) {
    // Also override the config root so home-isolated tests keep their own trust
    // store and cache under `$HOME/.config/sprocket`. This re-sets the
    // `SPROCKET_CONFIG_ROOT` set in `sprocket_with_global_args`; a later `.env`
    // for the same key overrides the earlier one for `std::process::Command`.
    command.env("HOME", home).env("USERPROFILE", home).env(
        "SPROCKET_CONFIG_ROOT",
        home.join(".config").join("sprocket"),
    );
}

pub(crate) struct ModuleFixture {
    pub(crate) dir: tempfile::TempDir,
}

impl ModuleFixture {
    pub(crate) fn with_local_dep() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let fixture = Self { dir };

        fs::create_dir_all(fixture.consumer()).unwrap();
        fs::create_dir_all(fixture.dep()).unwrap();

        fs::write(
            fixture.consumer().join("module.json"),
            r#"{
  "name": "consumer",
  "license": "MIT",
  "entrypoint": "index.wdl"
}
"#,
        )
        .unwrap();
        fs::write(fixture.consumer().join("index.wdl"), "version 1.3\n").unwrap();

        fs::write(
            fixture.dep().join("module.json"),
            r#"{
  "name": "dep",
  "license": "MIT",
  "entrypoint": "index.wdl"
}
"#,
        )
        .unwrap();
        fs::write(fixture.dep().join("index.wdl"), "version 1.3\n").unwrap();

        fixture
    }

    pub(crate) fn with_local_dep_added() -> Self {
        let fixture = Self::with_local_dep();
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
        fixture
    }

    pub(crate) fn consumer(&self) -> std::path::PathBuf {
        self.dir.path().join("consumer")
    }

    pub(crate) fn dep(&self) -> std::path::PathBuf {
        self.dir.path().join("dep")
    }
}

pub(crate) struct GitFixture {
    pub(crate) dir: tempfile::TempDir,
    pub(crate) repo_dir: PathBuf,
    config_path: PathBuf,
}

impl GitFixture {
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("tasks-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        let repo = Repository::init(&repo_dir).unwrap();
        write_git_module(&repo_dir, "1.0.0");
        commit_and_tag(&repo, "add tasks v1.0.0", "1.0.0");
        write_git_module(&repo_dir, "1.1.0");
        commit_and_tag(&repo, "add tasks v1.1.0", "1.1.0");
        write_git_module(&repo_dir, "2.0.0");
        commit_and_tag(&repo, "add tasks v2.0.0", "2.0.0");

        let cache_path = dir.path().join("module-cache");
        let cache_path = serde_json::to_string(&cache_path.to_string_lossy()).unwrap();
        let config_path = dir.path().join("sprocket.toml");
        let config = format!(
            "[modules]\ncache_path = {cache_path}\nallowed_schemes = [\"file\", \"https\", \
             \"ssh\"]\ndenied_hosts = []\n"
        );
        fs::write(&config_path, config).unwrap();

        Self {
            dir,
            repo_dir,
            config_path,
        }
    }

    pub(crate) fn without_version_tags() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("tasks-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        let repo = Repository::init(&repo_dir).unwrap();
        write_git_module(&repo_dir, "1.0.0");
        commit_without_tags(&repo, "add tasks");

        let cache_path = dir.path().join("module-cache");
        let cache_path = serde_json::to_string(&cache_path.to_string_lossy()).unwrap();
        let config_path = dir.path().join("sprocket.toml");
        let config = format!(
            "[modules]\ncache_path = {cache_path}\nallowed_schemes = [\"file\", \"https\", \
             \"ssh\"]\ndenied_hosts = []\n"
        );
        fs::write(&config_path, config).unwrap();

        Self {
            dir,
            repo_dir,
            config_path,
        }
    }

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

    pub(crate) fn repo_url(&self) -> String {
        url::Url::from_file_path(&self.repo_dir)
            .expect("repository directory should convert to a `file://` URL")
            .to_string()
    }

    pub(crate) fn write_consumer(&self, name: &str, dependencies: &str) -> PathBuf {
        let consumer = self.dir.path().join(name);
        fs::create_dir_all(&consumer).unwrap();
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
        consumer
    }

    pub(crate) fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub(crate) fn cache_path(&self) -> PathBuf {
        self.dir.path().join("module-cache")
    }

    pub(crate) fn default_branch(&self) -> String {
        let repo = Repository::open(&self.repo_dir).unwrap();
        repo.head()
            .unwrap()
            .shorthand()
            .expect("HEAD should resolve to a branch")
            .to_string()
    }

    pub(crate) fn head_commit(&self) -> String {
        let repo = Repository::open(&self.repo_dir).unwrap();
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string()
    }

    pub(crate) fn head_parent_commit(&self) -> String {
        let repo = Repository::open(&self.repo_dir).unwrap();
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .parent(0)
            .unwrap()
            .id()
            .to_string()
    }
}

pub(crate) fn write_git_module(path: &Path, version: &str) {
    let module = r#"{
  "name": "tasks",
  "license": "MIT",
  "entrypoint": "index.wdl"
}
"#;
    fs::write(path.join("module.json"), module).unwrap();
    fs::write(
        path.join("index.wdl"),
        format!("version 1.3\n# fixture version {version}\ntask t {{ command <<< echo hi >>> }}\n"),
    )
    .unwrap();

    fs::create_dir_all(path.join("tasks")).unwrap();
    fs::write(path.join("tasks").join("module.json"), module).unwrap();
    fs::write(
        path.join("tasks").join("index.wdl"),
        format!("version 1.3\n# fixture version {version}\ntask t {{ command <<< echo hi >>> }}\n"),
    )
    .unwrap();
}

pub(crate) fn write_signed_git_module(path: &Path, version: &str, signing_key: &SigningKey) {
    write_git_module(path, version);
    sign_git_module(&path.join("tasks"), signing_key);
}

pub(crate) fn sign_git_module(module_root: &Path, signing_key: &SigningKey) {
    let checksum = hash_directory(module_root).unwrap();
    // SAFETY: `None` contains no invalid signer identity fields.
    let signature = ModuleSignature::new(signing_key, &checksum, None).unwrap();
    let mut sig_bytes = Vec::new();
    signature.write(&mut sig_bytes).unwrap();
    fs::write(module_root.join("module.sig"), sig_bytes).unwrap();
}

pub(crate) fn add_signed_git_version(repo_dir: &Path, version: &str, signing_key: &SigningKey) {
    let repo = Repository::open(repo_dir).unwrap();
    write_signed_git_module(repo_dir, version, signing_key);
    commit_and_tag(&repo, &format!("add signed tasks v{version}"), version);
}

pub(crate) fn add_unsigned_git_version(repo_dir: &Path, version: &str) {
    let repo = Repository::open(repo_dir).unwrap();
    write_git_module(repo_dir, version);
    let _ = fs::remove_file(repo_dir.join("tasks").join("module.sig"));
    commit_and_tag(&repo, &format!("add unsigned tasks v{version}"), version);
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SignerTransition {
    Added,
    Changed,
    Removed,
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

pub(crate) fn stage_update_transition(transition: SignerTransition) -> (GitFixture, PathBuf) {
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

pub(crate) fn stage_upgrade_transition(transition: SignerTransition) -> (GitFixture, PathBuf) {
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
    /// Path to the consumer project whose lockfile the update targets.
    pub(crate) consumer: PathBuf,
    /// OpenSSH public key of `alpha`'s signer: the key TOFU would auto-trust
    /// on its own but must not persist when the batch is refused.
    pub(crate) auto_accept_key: String,
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

/// Path to the shared trust store that spawned `sprocket` commands persist to.
///
/// Commands resolve their trust store under `SPROCKET_CONFIG_ROOT`, which
/// [`sprocket_with_global_args`] points at [`SHARED_CONFIG_ROOT`].
pub(crate) fn shared_trust_store_path() -> PathBuf {
    SHARED_CONFIG_ROOT
        .get()
        .expect("shared config root should be initialized by a spawned command")
        .path()
        .join("modules-trust.toml")
}

/// Stages the [`MixedSignerBatch`] scenario described on that type.
pub(crate) fn stage_mixed_signer_batch() -> MixedSignerBatch {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

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
    let lock = sprocket_with_config(
        &config_path,
        &["dev", "module", "lock", "--trust-mode", "auto-accept"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run baseline lock");
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
        consumer,
        auto_accept_key,
    }
}

pub(crate) fn commit_and_tag(repo: &Repository, message: &str, tag: &str) {
    commit(repo, message);

    let commit = repo.head().unwrap().peel_to_commit().unwrap();
    repo.tag_lightweight(tag, commit.as_object(), false)
        .unwrap();
    let resolver_tag = format!("v{tag}");
    repo.tag_lightweight(&resolver_tag, commit.as_object(), false)
        .unwrap();
    let prefixed_tag = format!("tasks/{resolver_tag}");
    repo.tag_lightweight(&prefixed_tag, commit.as_object(), false)
        .unwrap();
}

pub(crate) fn commit_without_tags(repo: &Repository, message: &str) {
    commit(repo, message);
}

pub(crate) fn commit(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("sprocket-tests", "sprocket-tests@example.com").unwrap();

    let head = repo.head().ok().and_then(|h| h.target());
    if let Some(head) = head {
        let parent = repo.find_commit(head).unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent],
        )
        .unwrap();
    } else {
        repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
            .unwrap();
    }
}

pub(crate) fn read_lockfile(consumer: &Path) -> Lockfile {
    let lockfile = fs::read(consumer.join("module-lock.json")).unwrap();
    Lockfile::parse(&lockfile).unwrap()
}

pub(crate) fn locked_git_commit(lock: &Lockfile, name: &str) -> String {
    let name: DependencyName = name.parse().unwrap();
    let entry = lock.dependencies.get(&name).unwrap();
    match &entry.source {
        ResolvedSource::Git { sha, .. } => sha.to_string(),
        ResolvedSource::Path { .. } => panic!("expected `{name}` to be a Git dependency"),
    }
}

pub(crate) fn locked_git_selector(lock: &Lockfile, name: &str) -> String {
    let name: DependencyName = name.parse().unwrap();
    let entry = lock.dependencies.get(&name).unwrap();
    match &entry.source {
        ResolvedSource::Git { selector, .. } => match selector {
            GitSelector::Version(requirement) => format!("version {requirement}"),
            GitSelector::Tag(tag) => format!("tag {tag}"),
            GitSelector::Branch(branch) => format!("branch {branch}"),
            GitSelector::Commit(commit) => format!("commit {commit}"),
        },
        ResolvedSource::Path { .. } => panic!("expected `{name}` to be a Git dependency"),
    }
}

pub(crate) fn set_locked_git_commit(consumer: &Path, name: &str, commit: &str) {
    let path = consumer.join("module-lock.json");
    let bytes = fs::read(&path).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["dependencies"][name]["source"]["sha"] = serde_json::Value::String(commit.to_string());
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

pub(crate) fn manifest_dep_version(consumer: &Path, name: &str) -> Option<String> {
    let manifest = fs::read(consumer.join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    value["dependencies"][name]["version"]
        .as_str()
        .map(ToString::to_string)
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

pub(crate) fn overwrite_first_file_named(root: &Path, file_name: &str, content: &str) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).unwrap();
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some(file_name) {
                fs::write(path, content).unwrap();
                return true;
            }
        }
    }
    false
}
