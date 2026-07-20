//! Git repository fixtures and repository mutation helpers.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use git2::Repository;
use wdl_modules::hash::hash_directory;
use wdl_modules::signing::ModuleSignature;
use wdl_modules::signing::SigningKey;

#[derive(Debug)]
pub(crate) struct GitFixture {
    pub(crate) dir: tempfile::TempDir,
    pub(crate) repo_dir: PathBuf,
    pub(super) config_path: PathBuf,
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
