//! Lockfile inspection and result mutation helpers.

use std::fs;
use std::path::Path;

use wdl_modules::Lockfile;
use wdl_modules::dependency::DependencyName;
use wdl_modules::dependency::GitSelector;
use wdl_modules::lockfile::ResolvedSource;

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
