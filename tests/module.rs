//! Integration tests for `sprocket module` commands.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use git2::Repository;
use wdl_modules::Lockfile;
use wdl_modules::Manifest;
use wdl_modules::dependency::DependencyName;
use wdl_modules::dependency::GitSelector;
use wdl_modules::hash::hash_directory;
use wdl_modules::lockfile::ResolvedSource;
use wdl_modules::signing::ModuleSignature;
use wdl_modules::signing::SigningKey;

fn sprocket(args: &[&str]) -> Command {
    sprocket_with_global_args(&[], args)
}

fn sprocket_with_global_args(global_args: &[&str], args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_sprocket"));
    command
        .arg("--skip-config-search")
        .args(global_args)
        .args(args)
        .env("RUST_LOG", "none")
        .env_remove("RUST_BACKTRACE");
    command
}

fn sprocket_with_config(config_path: &Path, args: &[&str]) -> Command {
    let config_path = config_path.to_string_lossy().into_owned();
    sprocket_with_global_args(&["--config", &config_path], args)
}

fn output_with_stdin(mut command: Command, input: &str) -> std::process::Output {
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

fn isolated_home(base: &Path, name: &str) -> PathBuf {
    let home = base.join(name);
    fs::create_dir_all(&home).unwrap();
    home
}

fn use_home(command: &mut Command, home: &Path) {
    command.env("HOME", home).env("USERPROFILE", home);
}

#[test]
fn init_scaffolds_a_parseable_module() {
    let dir = tempfile::tempdir().unwrap();
    let output = sprocket(&["module", "init", "--name", "demo"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run sprocket");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read(dir.path().join("module.json")).unwrap();
    Manifest::parse(&manifest).expect("scaffold parses");
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(value["description"], "The `demo` WDL module.");
    assert!(value.get("version").is_none());
    assert!(dir.path().join("index.wdl").exists());
    assert!(dir.path().join("README.md").exists());

    assert!(!dir.path().join(".gitignore").exists());
}

#[test]
fn directory_module_entrypoint_does_not_require_wdl_1_4() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("module.json"),
        r#"{"name":"demo","license":"MIT","entrypoint":"main.wdl"}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.wdl"),
        "version 1.2\nworkflow wf {\n  input {\n    String name\n  }\n}\n",
    )
    .unwrap();

    let module_arg = dir.path().to_string_lossy().into_owned();
    let output = sprocket(&["inputs", &module_arg])
        .output()
        .expect("failed to run sprocket inputs");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"wf.name\""));
}

#[test]
fn init_preserves_existing_scaffold_files() {
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.wdl");
    let readme = dir.path().join("README.md");
    let gitignore = dir.path().join(".gitignore");
    fs::write(&index, "version 1.0\n").unwrap();
    fs::write(&readme, "# custom\n").unwrap();
    fs::write(&gitignore, "target/\n").unwrap();

    let output = sprocket(&["module", "init", "--name", "demo"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run sprocket");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(fs::read_to_string(&index).unwrap(), "version 1.0\n");
    assert_eq!(fs::read_to_string(&readme).unwrap(), "# custom\n");
    assert_eq!(fs::read_to_string(&gitignore).unwrap(), "target/\n");

    assert!(String::from_utf8_lossy(&output.stdout).contains("Created module `demo`"));
}

struct ModuleFixture {
    dir: tempfile::TempDir,
}

impl ModuleFixture {
    fn with_local_dep() -> Self {
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
        fs::write(fixture.consumer().join("index.wdl"), "version 1.2\n").unwrap();

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
        fs::write(fixture.dep().join("index.wdl"), "version 1.2\n").unwrap();

        fixture
    }

    fn with_local_dep_added() -> Self {
        let fixture = Self::with_local_dep();
        let output = sprocket(&["module", "add", "utils", "../dep"])
            .current_dir(fixture.consumer())
            .output()
            .expect("failed to run sprocket module add");
        assert!(
            output.status.success(),
            "command failed {status}: {stderr}",
            status = output.status,
            stderr = String::from_utf8_lossy(&output.stderr)
        );
        fixture
    }

    fn consumer(&self) -> std::path::PathBuf {
        self.dir.path().join("consumer")
    }

    fn dep(&self) -> std::path::PathBuf {
        self.dir.path().join("dep")
    }
}

struct GitFixture {
    dir: tempfile::TempDir,
    repo_dir: PathBuf,
    config_path: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
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

    fn without_version_tags() -> Self {
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

    fn signed_initial_version() -> (Self, String) {
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

    fn signed_without_version_tags() -> (Self, String) {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("tasks-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        let repo = Repository::init(&repo_dir).unwrap();
        write_git_module(&repo_dir, "1.0.0");

        let private_key = generate_openssh_ed25519_private_key();
        let signing_key = SigningKey::from_openssh(&private_key).unwrap();
        let module_root = repo_dir.join("tasks");
        let checksum = hash_directory(&module_root).unwrap();
        let signature = ModuleSignature {
            public_key: signing_key.verifying_key(),
            identity: None,
            signature: signing_key.sign(&checksum),
        };
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

    fn repo_url(&self) -> String {
        format!("file://{}", self.repo_dir.display())
    }

    fn write_consumer(&self, name: &str, dependencies: &str) -> PathBuf {
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
        fs::write(consumer.join("index.wdl"), "version 1.2\n").unwrap();
        consumer
    }

    fn config_path(&self) -> &Path {
        &self.config_path
    }

    fn cache_path(&self) -> PathBuf {
        self.dir.path().join("module-cache")
    }

    fn default_branch(&self) -> String {
        let repo = Repository::open(&self.repo_dir).unwrap();
        repo.head()
            .unwrap()
            .shorthand()
            .expect("HEAD should resolve to a branch")
            .to_string()
    }

    fn head_commit(&self) -> String {
        let repo = Repository::open(&self.repo_dir).unwrap();
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string()
    }

    fn head_parent_commit(&self) -> String {
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

fn write_git_module(path: &Path, version: &str) {
    let module = r#"{
  "name": "tasks",
  "license": "MIT",
  "entrypoint": "index.wdl"
}
"#;
    fs::write(path.join("module.json"), module).unwrap();
    fs::write(
        path.join("index.wdl"),
        format!("version 1.2\n# fixture version {version}\ntask t {{ command <<< echo hi >>> }}\n"),
    )
    .unwrap();

    fs::create_dir_all(path.join("tasks")).unwrap();
    fs::write(path.join("tasks").join("module.json"), module).unwrap();
    fs::write(
        path.join("tasks").join("index.wdl"),
        format!("version 1.2\n# fixture version {version}\ntask t {{ command <<< echo hi >>> }}\n"),
    )
    .unwrap();
}

fn write_signed_git_module(path: &Path, version: &str, signing_key: &SigningKey) {
    write_git_module(path, version);
    sign_git_module(&path.join("tasks"), signing_key);
}

fn sign_git_module(module_root: &Path, signing_key: &SigningKey) {
    let checksum = hash_directory(module_root).unwrap();
    let signature = ModuleSignature {
        public_key: signing_key.verifying_key(),
        identity: None,
        signature: signing_key.sign(&checksum),
    };
    let mut sig_bytes = Vec::new();
    signature.write(&mut sig_bytes).unwrap();
    fs::write(module_root.join("module.sig"), sig_bytes).unwrap();
}

fn add_signed_git_version(repo_dir: &Path, version: &str, signing_key: &SigningKey) {
    let repo = Repository::open(repo_dir).unwrap();
    write_signed_git_module(repo_dir, version, signing_key);
    commit_and_tag(&repo, &format!("add signed tasks v{version}"), version);
}

fn add_unsigned_git_version(repo_dir: &Path, version: &str) {
    let repo = Repository::open(repo_dir).unwrap();
    write_git_module(repo_dir, version);
    let _ = fs::remove_file(repo_dir.join("tasks").join("module.sig"));
    commit_and_tag(&repo, &format!("add unsigned tasks v{version}"), version);
}

#[derive(Clone, Copy, Debug)]
enum SignerTransition {
    Added,
    Changed,
    Removed,
}

#[derive(Clone, Copy, Debug)]
enum CliTrustMode {
    Confirm,
    Tofu,
    Auto,
}

impl CliTrustMode {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Tofu => "tofu",
            Self::Auto => "auto",
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
        &["module", "lock", "--trust-mode", "auto"],
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
        &["module", "lock", "--trust-mode", "auto"],
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

fn set_fixture_trust_mode(fixture: &GitFixture, trust_mode: &str) {
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

fn commit_and_tag(repo: &Repository, message: &str, tag: &str) {
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

fn commit_without_tags(repo: &Repository, message: &str) {
    commit(repo, message);
}

fn commit(repo: &Repository, message: &str) {
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

fn read_lockfile(consumer: &Path) -> Lockfile {
    let lockfile = fs::read(consumer.join("module-lock.json")).unwrap();
    Lockfile::parse(&lockfile).unwrap()
}

fn locked_git_commit(lock: &Lockfile, name: &str) -> String {
    let name: DependencyName = name.parse().unwrap();
    let entry = lock.dependencies.get(&name).unwrap();
    match &entry.source {
        ResolvedSource::Git { sha, .. } => sha.to_string(),
        ResolvedSource::Path { .. } => panic!("expected `{name}` to be a Git dependency"),
    }
}

fn locked_git_selector(lock: &Lockfile, name: &str) -> String {
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

fn set_locked_git_commit(consumer: &Path, name: &str, commit: &str) {
    let path = consumer.join("module-lock.json");
    let bytes = fs::read(&path).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["dependencies"][name]["source"]["sha"] = serde_json::Value::String(commit.to_string());
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn manifest_dep_version(consumer: &Path, name: &str) -> Option<String> {
    let manifest = fs::read(consumer.join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    value["dependencies"][name]["version"]
        .as_str()
        .map(ToString::to_string)
}

fn generate_openssh_ed25519_private_key() -> String {
    let mut rng = ssh_key::rand_core::OsRng;
    let key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
    key.to_openssh(ssh_key::LineEnding::LF).unwrap().to_string()
}

fn generate_openssh_ed25519_public_key() -> String {
    let mut rng = ssh_key::rand_core::OsRng;
    let key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
    key.public_key().to_openssh().unwrap().to_string()
}

fn overwrite_first_file_named(root: &Path, file_name: &str, content: &str) -> bool {
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

#[test]
fn add_local_path_dep_edits_manifest_and_locks() {
    let fixture = ModuleFixture::with_local_dep();
    let output = sprocket(&["module", "add", "utils", "../dep"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module add");

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
    fs::write(module.join("index.wdl"), "version 1.2\n").unwrap();

    let collection_arg = collection.to_string_lossy().into_owned();
    let output = sprocket(&[
        "module",
        "add",
        &collection_arg,
        "--path",
        "modules/alchemy",
    ])
    .current_dir(fixture.consumer())
    .output()
    .expect("failed to run sprocket module add");

    assert!(
        output.status.success(),
        "command failed {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read(fixture.consumer().join("module.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(
        value["dependencies"]["alchemy"]["path"].as_str(),
        Some(module.to_string_lossy().as_ref())
    );

    let lockfile = fs::read(fixture.consumer().join("module-lock.json")).unwrap();
    Lockfile::parse(&lockfile).unwrap();
}

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

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
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
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
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

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
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
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
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

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
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
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
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

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
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
        &["module", "lock", "--trust-mode", "auto"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );
    assert!(!String::from_utf8_lossy(&lock.stderr).contains("[y/N]"));
}

#[test]
fn add_git_dep_without_tags_tracks_default_branch_and_locks() {
    let fixture = GitFixture::without_version_tags();
    let repo_url = fixture.repo_url();
    let default_branch = fixture.default_branch();
    let consumer = fixture.write_consumer("consumer-add-default-branch", "");

    let output = sprocket_with_config(
        fixture.config_path(),
        &["module", "add", "dep", &repo_url, "--path", "tasks"],
    )
    .current_dir(&consumer)
    .env("RUST_LOG", "info")
    .output()
    .expect("failed to run sprocket module add");

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
        &["module", "add", &repo_url, "--path", "tasks"],
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
        .expect("failed to run sprocket module add");
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
        "module",
        "add",
        "stjudecloud/workflows",
        "--branch",
        "main",
        "--no-lock",
    ])
    .current_dir(fixture.consumer())
    .output()
    .expect("failed to run sprocket module add");

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
    .expect("failed to run sprocket module add");

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
    .expect("failed to run sprocket module add");

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
    .expect("failed to run sprocket module add");

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
    .expect("failed to run sprocket module add");

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
    let output = sprocket(&["module", "add", "1bad", "../dep"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module add");

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
    let output = sprocket(&["module", "add", "utils", "../dep"])
        .current_dir(fixture.consumer())
        .env("RUST_LOG", "info")
        .output()
        .expect("failed to run sprocket module add");

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
    let output = sprocket(&["module", "remove", "utils"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module remove");

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
fn lock_writes_lockfile() {
    let fixture = ModuleFixture::with_local_dep();
    let add = sprocket(&["module", "add", "utils", "../dep", "--no-lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module add --no-lock");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    assert!(!fixture.consumer().join("module-lock.json").exists());

    let output = sprocket(&["module", "lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module lock");
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
        let output = sprocket(&["module", "lock", removed])
            .output()
            .expect("failed to run sprocket module lock");
        assert!(
            !output.status.success(),
            "`sprocket module lock {removed}` unexpectedly succeeded"
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
    let add = sprocket(&["module", "add", "utils", "../dep", "--no-lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module add --no-lock");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let output = sprocket(&["module", "lock", "--locked"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module lock --locked");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn lock_idempotent_reports_up_to_date() {
    let fixture = ModuleFixture::with_local_dep();
    let first = sprocket(&["module", "lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run first sprocket module lock");
    assert!(
        first.status.success(),
        "command failed {status}: {stderr}",
        status = first.status,
        stderr = String::from_utf8_lossy(&first.stderr)
    );

    let second = sprocket(&["module", "lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run second sprocket module lock");
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
    let first = sprocket(&["module", "lock"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        first.status.success(),
        "command failed {status}: {stderr}",
        status = first.status,
        stderr = String::from_utf8_lossy(&first.stderr)
    );

    let locked = sprocket(&["module", "lock", "--locked"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module lock --locked");
    assert!(
        locked.status.success(),
        "command failed {status}: {stderr}",
        status = locked.status,
        stderr = String::from_utf8_lossy(&locked.stderr)
    );
}

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
fn verify_succeeds_after_lock() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-verify",
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
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("Verified"));
    assert!(stdout.contains("Skipped signature verification for current module (no `module.sig`)"));
    assert!(stdout.contains("Skipped signature verification for 1 dependency without a signature"));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(!stderr.contains("cryptographic signature"));

    let config_path = fixture.config_path().to_string_lossy().into_owned();
    let colored = sprocket_with_global_args(
        &["--config", &config_path, "--color", "always"],
        &["module", "verify"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run colored sprocket module verify");
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

    let verify = sprocket_with_config(fixture.config_path(), &["module", "verify", "--strict"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module verify");
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
        &["module", "verify", "--strict"],
    )
    .current_dir(&consumer)
    .output()
    .expect("failed to run colored sprocket module verify");
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
        &["module", "lock", "--trust-mode", "auto"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
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
        &["module", "trust", "remove", &public_key],
    );
    trust_remove.current_dir(&consumer);
    use_home(&mut trust_remove, &home);
    let remove = trust_remove
        .output()
        .expect("failed to run sprocket module trust remove");
    assert!(remove.status.success());

    let mut verify_command =
        sprocket_with_config(fixture.config_path(), &["module", "verify", "lockfile"]);
    verify_command.current_dir(&consumer);
    use_home(&mut verify_command, &home);
    let verify = verify_command
        .output()
        .expect("failed to run sprocket module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("2 modules are untrusted"));
    assert!(stderr.contains("`task_a` signer is untrusted"));
    assert!(stderr.contains("`task_b` signer is untrusted"));
    assert!(stderr.contains("sprocket module trust all"));
}

#[test]
fn trust_add_accepts_multiple_keys() {
    let dir = tempfile::tempdir().unwrap();

    let key_a = generate_openssh_ed25519_public_key();
    let key_b = generate_openssh_ed25519_public_key();

    let add = sprocket(&["module", "trust", "add", &key_a, &key_b])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust add");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let list = sprocket(&["module", "trust", "list"])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust list");
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
    let mut command = sprocket(&["module", "trust", "list"]);
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

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
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

    let mut trust_command = sprocket_with_config(
        fixture.config_path(),
        &["module", "trust", "add", &public_key],
    );
    trust_command.current_dir(&consumer);
    use_home(&mut trust_command, &home);
    let trust = trust_command
        .output()
        .expect("failed to run sprocket module trust add");
    assert!(
        trust.status.success(),
        "command failed {status}: {stderr}",
        status = trust.status,
        stderr = String::from_utf8_lossy(&trust.stderr)
    );

    let mut run_command = sprocket_with_config(fixture.config_path(), &["run", "."]);
    run_command.current_dir(&consumer);
    use_home(&mut run_command, &home);
    let run = run_command.output().expect("failed to run sprocket run");
    assert!(
        run.status.success(),
        "command failed {status}: {stderr}",
        status = run.status,
        stderr = String::from_utf8_lossy(&run.stderr)
    );

    let mut remove_command = sprocket_with_config(
        fixture.config_path(),
        &["module", "trust", "remove", &public_key],
    );
    remove_command.current_dir(&consumer);
    use_home(&mut remove_command, &home);
    let remove = remove_command
        .output()
        .expect("failed to run sprocket module trust remove");
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
    assert!(stderr.contains("sprocket module trust all"));
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
    .expect("failed to run sprocket module add");
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

#[test]
fn verify_fails_on_tampered_cache() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-verify-tamper",
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

    assert!(
        overwrite_first_file_named(&fixture.cache_path(), "index.wdl", "version 1.0\n"),
        "expected to find cached index.wdl to tamper with"
    );

    let verify = sprocket_with_config(fixture.config_path(), &["module", "verify", "lockfile"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module verify");
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

    let verify = sprocket_with_config(fixture.config_path(), &["module", "verify", "lockfile"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    assert!(String::from_utf8_lossy(&verify.stderr).contains("sprocket module lock"));
}

#[test]
fn verify_reports_fetch_when_cache_missing() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-verify-no-cache",
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
fn sign_writes_verifiable_signature() {
    let fixture = ModuleFixture::with_local_dep();
    let key_path = fixture.dir.path().join("id_ed25519");
    fs::write(&key_path, generate_openssh_ed25519_private_key()).unwrap();

    let key_path_arg = key_path.to_string_lossy().into_owned();
    let sign = sprocket(&["module", "sign", "--key", &key_path_arg])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module sign");
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

    let verify = sprocket(&["module", "verify", "signature"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module verify signature");
    assert!(
        verify.status.success(),
        "command failed {status}: {stderr}",
        status = verify.status,
        stderr = String::from_utf8_lossy(&verify.stderr)
    );
    assert!(String::from_utf8_lossy(&verify.stdout).contains("Verified signature"));

    let verify_all = sprocket(&["module", "verify"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module verify");
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

    let verify = sprocket(&["module", "verify", "signature"])
        .current_dir(fixture.consumer())
        .output()
        .expect("failed to run sprocket module verify signature");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("no `module.sig`"));
    assert!(stderr.contains("sprocket module sign"));
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

    let sign = sprocket(&["module", "sign"])
        .current_dir(fixture.consumer())
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .output()
        .expect("failed to run sprocket module sign");
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

    let sign = sprocket(&["module", "sign"])
        .current_dir(fixture.consumer())
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .output()
        .expect("failed to run sprocket module sign");
    assert!(
        !sign.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&sign.stdout)
    );
    let stderr = String::from_utf8_lossy(&sign.stderr);
    assert!(stderr.contains("no ed25519 signing key found"));
    assert!(stderr.contains("specify `--key`"));
}

#[test]
fn trust_add_then_list_shows_entry() {
    let dir = tempfile::tempdir().unwrap();

    let pub_key = generate_openssh_ed25519_public_key();

    let add = sprocket(&["module", "trust", "add", &pub_key])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust add");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let list = sprocket(&["module", "trust", "list"])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust list");
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
fn trust_remove_drops_entry() {
    let dir = tempfile::tempdir().unwrap();

    let pub_key_path = dir.path().join("id_ed25519.pub");
    fs::write(&pub_key_path, generate_openssh_ed25519_public_key()).unwrap();

    let pub_key_arg = pub_key_path.to_string_lossy().into_owned();
    let add = sprocket(&["module", "trust", "add", &pub_key_arg])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust add");
    assert!(
        add.status.success(),
        "command failed {status}: {stderr}",
        status = add.status,
        stderr = String::from_utf8_lossy(&add.stderr)
    );

    let remove = sprocket(&["module", "trust", "remove", &pub_key_arg])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust remove");
    assert!(
        remove.status.success(),
        "command failed {status}: {stderr}",
        status = remove.status,
        stderr = String::from_utf8_lossy(&remove.stderr)
    );

    let list = sprocket(&["module", "trust", "list"])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust list");
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
        let add = sprocket(&["module", "trust", "add", key])
            .current_dir(dir.path())
            .env("HOME", dir.path())
            .output()
            .expect("failed to run sprocket module trust add");
        assert!(
            add.status.success(),
            "command failed {status}: {stderr}",
            status = add.status,
            stderr = String::from_utf8_lossy(&add.stderr)
        );
    }

    let destroy = sprocket(&["module", "trust", "destroy"])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust destroy");
    assert!(
        destroy.status.success(),
        "command failed {status}: {stderr}",
        status = destroy.status,
        stderr = String::from_utf8_lossy(&destroy.stderr)
    );
    assert!(String::from_utf8_lossy(&destroy.stdout).contains("Removed all trusted keys"));

    let list = sprocket(&["module", "trust", "list"])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .output()
        .expect("failed to run sprocket module trust list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains("no trusted keys"));
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

#[test]
fn lock_resolves_file_git_dependency_with_config() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-file-lock",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let output = sprocket_with_config(fixture.config_path(), &["module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module lock");
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

    let output = sprocket_with_config(fixture.config_path(), &["module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not a WDL module"));
}

#[test]
fn update_moves_pin_to_newest_satisfying_version_and_is_idempotent() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-update",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let first_lock = sprocket_with_config(fixture.config_path(), &["module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        first_lock.status.success(),
        "command failed {status}: {stderr}",
        status = first_lock.status,
        stderr = String::from_utf8_lossy(&first_lock.stderr)
    );

    let first_update = sprocket_with_config(fixture.config_path(), &["module", "update"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run first sprocket module update");
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
    let first_bytes = fs::read(consumer.join("module-lock.json")).unwrap();

    let second_update = sprocket_with_config(fixture.config_path(), &["module", "update"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run second sprocket module update");
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
    set_locked_git_commit(&consumer, "tasks", &stale);

    let update = sprocket_with_config(fixture.config_path(), &["module", "update"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Updated tasks"));
    assert!(stdout.contains("selector: branch"));
    assert!(stdout.contains(&format!("commit: `{}` -> `{}`", &stale[..7], &latest[..7])));

    let lock = read_lockfile(&consumer);
    assert_eq!(locked_git_commit(&lock, "tasks"), latest);
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
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
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

    let mut update_command = sprocket_with_config(fixture.config_path(), &["module", "update"]);
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
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
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
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
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
        &["module", "trust", "add", &new_public_key],
    );
    trust_command.current_dir(&consumer);
    use_home(&mut trust_command, &home);
    let trust = trust_command
        .output()
        .expect("failed to run sprocket module trust add");
    assert!(
        trust.status.success(),
        "command failed {status}: {stderr}",
        status = trust.status,
        stderr = String::from_utf8_lossy(&trust.stderr)
    );

    let mut update_command = sprocket_with_config(fixture.config_path(), &["module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket module update");
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
fn trust_all_trusts_locked_signers_without_relocking() {
    let (fixture, public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-trust-all-lockfile");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-trust-all-lockfile",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
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
        &["module", "trust", "remove", &public_key],
    );
    remove_command.current_dir(&consumer);
    use_home(&mut remove_command, &home);
    let remove = remove_command
        .output()
        .expect("failed to run sprocket module trust remove");
    assert!(
        remove.status.success(),
        "command failed {status}: {stderr}",
        status = remove.status,
        stderr = String::from_utf8_lossy(&remove.stderr)
    );

    let mut verify_command = sprocket_with_config(fixture.config_path(), &["module", "verify"]);
    verify_command.current_dir(&consumer);
    use_home(&mut verify_command, &home);
    let verify = verify_command
        .output()
        .expect("failed to run sprocket module verify");
    assert!(
        !verify.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("sprocket module trust all"));
    assert_eq!(fs::read(consumer.join("module-lock.json")).unwrap(), before);

    let mut trust_all_command =
        sprocket_with_config(fixture.config_path(), &["module", "trust", "all"]);
    trust_all_command.current_dir(&consumer);
    use_home(&mut trust_all_command, &home);
    let trust_all = trust_all_command
        .output()
        .expect("failed to run sprocket module trust all");
    assert!(
        trust_all.status.success(),
        "command failed {status}: {stderr}",
        status = trust_all.status,
        stderr = String::from_utf8_lossy(&trust_all.stderr)
    );
    assert!(String::from_utf8_lossy(&trust_all.stdout).contains("Trusted 1 signer keys"));
    assert_eq!(fs::read(consumer.join("module-lock.json")).unwrap(), before);

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
    assert!(
        list.status.success(),
        "command failed {status}: {stderr}",
        status = list.status,
        stderr = String::from_utf8_lossy(&list.stderr)
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains(&public_key));
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

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(lock.status.success());
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut update_command = sprocket_with_config(fixture.config_path(), &["module", "update"]);
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
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = output_with_stdin(lock_command, "y\n");
    assert!(lock.status.success());
    let before = fs::read(consumer.join("module-lock.json")).unwrap();

    add_unsigned_git_version(&fixture.repo_dir, "1.1.3");

    let mut update_command = sprocket_with_config(fixture.config_path(), &["module", "update"]);
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
    assert!(stderr.contains("sprocket module trust all"));
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
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
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

    let mut update_command = sprocket_with_config(fixture.config_path(), &["module", "update"]);
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
    assert!(String::from_utf8_lossy(&update.stdout).contains("Trusted 1 signer keys"));

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
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

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut update_command = sprocket_with_config(fixture.config_path(), &["module", "update"]);
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
fn lock_update_auto_trusts_changed_signer_key_without_prompting() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-auto");
    set_fixture_trust_mode(&fixture, "auto");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-signer-auto",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(
        lock.status.success(),
        "command failed {status}: {stderr}",
        status = lock.status,
        stderr = String::from_utf8_lossy(&lock.stderr)
    );

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    let new_public_key = new_key.verifying_key().to_openssh();
    add_signed_git_version(&fixture.repo_dir, "1.1.0", &new_key);

    let mut update_command = sprocket_with_config(fixture.config_path(), &["module", "update"]);
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
    assert!(String::from_utf8_lossy(&update.stdout).contains("Trusted 1 signer keys"));

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
    assert!(String::from_utf8_lossy(&list.stdout).contains(&new_public_key));
}

#[test]
fn lock_update_trust_mode_flag_auto_trusts_without_prompting() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    let home = isolated_home(fixture.dir.path(), "home-update-auto-flag");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-lock-update-auto-flag",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "^1.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(
        fixture.config_path(),
        &["module", "lock", "--trust-mode", "auto"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(lock.status.success());

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "1.1.0", &new_key);

    let mut update_command = sprocket_with_config(
        fixture.config_path(),
        &["module", "update", "--trust-mode", "auto"],
    );
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
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
        &["module", "lock", "--trust-mode", "auto"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(lock.status.success());

    add_unsigned_git_version(&fixture.repo_dir, "1.1.4");

    let mut update_command = sprocket_with_config(
        fixture.config_path(),
        &["module", "update", "--trust-mode", "auto"],
    );
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));
    assert!(String::from_utf8_lossy(&update.stdout).contains("Accepted signer trust changes"));

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
    assert!(
        String::from_utf8_lossy(&list.stdout).contains(&old_public_key),
        "accepting a removed module signature should not remove global trust for the signer key"
    );
}

#[test]
fn lock_update_trust_mode_flag_auto_trusts_unsigned_to_signed_without_prompting() {
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
        &["module", "lock", "--trust-mode", "auto"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(lock.status.success());

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    let new_public_key = new_key.verifying_key().to_openssh();
    add_signed_git_version(&fixture.repo_dir, "1.1.2", &new_key);

    let mut update_command = sprocket_with_config(
        fixture.config_path(),
        &["module", "update", "--trust-mode", "auto"],
    );
    update_command.current_dir(&consumer);
    use_home(&mut update_command, &home);
    let update = update_command
        .output()
        .expect("failed to run sprocket module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("[y/N]"));

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
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
    let before = fs::read(consumer.join("module-lock.json")).unwrap();

    let update = sprocket_with_config(fixture.config_path(), &["module", "update"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module update");
    assert!(
        update.status.success(),
        "command failed {status}: {stderr}",
        status = update.status,
        stderr = String::from_utf8_lossy(&update.stderr)
    );
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Locking 0 packages based on `module.json`"));

    let after = fs::read(consumer.join("module-lock.json")).unwrap();
    assert_eq!(after, before);

    let list = sprocket_with_config(fixture.config_path(), &["module", "list"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module list");
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

    let output = sprocket_with_config(fixture.config_path(), &["module", "update", "nope"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module update");
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

    let first_lock = sprocket_with_config(fixture.config_path(), &["module", "lock"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module lock");
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

    let update = sprocket_with_config(fixture.config_path(), &["module", "update", "tasks"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module update tasks");
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
        (SignerTransition::Added, CliTrustMode::Auto, true, false, ""),
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
            CliTrustMode::Auto,
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
            CliTrustMode::Auto,
            true,
            false,
            "",
        ),
    ];

    for (transition, mode, expect_success, expect_prompt, expected_phrase) in cases {
        let (fixture, consumer) = stage_update_transition(transition);
        let mut command = sprocket_with_config(
            fixture.config_path(),
            &["module", "update", "--trust-mode", mode.as_arg()],
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
    }
}

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
        (SignerTransition::Added, CliTrustMode::Auto, true, false, ""),
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
            CliTrustMode::Auto,
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
            CliTrustMode::Auto,
            true,
            false,
            "",
        ),
    ];

    for (transition, mode, expect_success, expect_prompt, expected_phrase) in cases {
        let (fixture, consumer) = stage_upgrade_transition(transition);
        let mut command = sprocket_with_config(
            fixture.config_path(),
            &["module", "upgrade", "--trust-mode", mode.as_arg()],
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
    }
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
fn upgrade_raises_constraint_and_relocks() {
    let fixture = GitFixture::new();
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade",
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
    let before = read_lockfile(&consumer);
    assert_eq!(locked_git_selector(&before, "tasks"), "version ^1.0");

    let upgrade = sprocket_with_config(fixture.config_path(), &["module", "upgrade"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module upgrade");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    let stdout = String::from_utf8_lossy(&upgrade.stdout);
    assert!(stdout.contains("Upgrading 1 packages to latest version"));
    assert!(stdout.contains("Upgraded tasks v1.0 -> v2.0.0"));
    assert!(stdout.contains("Locking 1 packages based on `module.json`"));
    assert!(stdout.contains("Updated tasks"));

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
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();

    let upgrade = sprocket_with_config(fixture.config_path(), &["module", "upgrade", "--dry-run"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module upgrade --dry-run");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    let stdout = String::from_utf8_lossy(&upgrade.stdout);
    assert!(
        stdout.contains("Upgrade tasks v1.0 -> v2.0.0 (dry-run)"),
        "dry run should print the planned change, got: {stdout}"
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
    let lock_before = read_lockfile(&consumer);
    let branched_before = locked_git_commit(&lock_before, "branched");

    add_unsigned_git_version(&fixture.repo_dir, "2.0.1");
    let latest = fixture.head_commit();

    let upgrade = sprocket_with_config(fixture.config_path(), &["module", "upgrade"])
        .current_dir(&consumer)
        .output()
        .expect("failed to run sprocket module upgrade");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    assert!(String::from_utf8_lossy(&upgrade.stdout).contains("Updated branched"));

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
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
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

    let mut upgrade_command = sprocket_with_config(fixture.config_path(), &["module", "upgrade"]);
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
    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = output_with_stdin(lock_command, "y\n");
    assert!(lock.status.success());
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();

    add_unsigned_git_version(&fixture.repo_dir, "1.1.5");

    let mut upgrade_command = sprocket_with_config(fixture.config_path(), &["module", "upgrade"]);
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
    assert!(stderr.contains("sprocket module trust all"));
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

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(lock.status.success());
    let lock_before = fs::read(consumer.join("module-lock.json")).unwrap();
    let manifest_before = fs::read(consumer.join("module.json")).unwrap();

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "2.0.1", &new_key);

    let mut upgrade_command = sprocket_with_config(fixture.config_path(), &["module", "upgrade"]);
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
fn upgrade_trust_mode_flag_confirm_prompts_even_when_config_auto() {
    let (fixture, _old_public_key) = GitFixture::signed_initial_version();
    set_fixture_trust_mode(&fixture, "auto");
    let home = isolated_home(fixture.dir.path(), "home-upgrade-confirm-flag");
    let repo_url = fixture.repo_url();
    let consumer = fixture.write_consumer(
        "consumer-upgrade-confirm-flag",
        &format!(r#"    "tasks": {{ "git": "{repo_url}", "version": "=1.0.0", "path": "tasks" }}"#),
    );

    let mut lock_command = sprocket_with_config(fixture.config_path(), &["module", "lock"]);
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(lock.status.success());

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    add_signed_git_version(&fixture.repo_dir, "1.1.0", &new_key);

    let mut upgrade_command = sprocket_with_config(
        fixture.config_path(),
        &["module", "upgrade", "--trust-mode", "confirm"],
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
fn upgrade_trust_mode_flag_auto_trusts_unsigned_to_signed_without_prompting() {
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
        &["module", "lock", "--trust-mode", "auto"],
    );
    lock_command.current_dir(&consumer);
    use_home(&mut lock_command, &home);
    let lock = lock_command
        .output()
        .expect("failed to run sprocket module lock");
    assert!(lock.status.success());

    let new_key = SigningKey::from_openssh(&generate_openssh_ed25519_private_key()).unwrap();
    let new_public_key = new_key.verifying_key().to_openssh();
    add_signed_git_version(&fixture.repo_dir, "2.0.1", &new_key);

    let mut upgrade_command = sprocket_with_config(
        fixture.config_path(),
        &["module", "upgrade", "--trust-mode", "auto"],
    );
    upgrade_command.current_dir(&consumer);
    use_home(&mut upgrade_command, &home);
    let upgrade = upgrade_command
        .output()
        .expect("failed to run sprocket module upgrade");
    assert!(
        upgrade.status.success(),
        "command failed {status}: {stderr}",
        status = upgrade.status,
        stderr = String::from_utf8_lossy(&upgrade.stderr)
    );
    assert!(!String::from_utf8_lossy(&upgrade.stderr).contains("[y/N]"));
    assert!(String::from_utf8_lossy(&upgrade.stdout).contains("Trusted 1 signer keys"));

    let mut list_command =
        sprocket_with_config(fixture.config_path(), &["module", "trust", "list"]);
    list_command.current_dir(&consumer);
    use_home(&mut list_command, &home);
    let list = list_command
        .output()
        .expect("failed to run sprocket module trust list");
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
        &["module", "upgrade", "--dry-run", "tasks"],
    )
    .current_dir(&consumer)
    .env("RUST_LOG", "info")
    .output()
    .expect("failed to run sprocket module upgrade --dry-run");
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
