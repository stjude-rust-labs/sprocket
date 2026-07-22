use std::fs;

use tempfile::tempdir;

use super::super::resolve::locked_selector_satisfies;
use super::json_path;
use super::locked_git_entry;
use super::module;
use super::resolve_and_lock;
use super::resolver;
use super::write_manifest;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::dependency::GitSelector;
use crate::lockfile::ResolvedSource;
use crate::resolver::DependencyScope;
use crate::resolver::ModulesConfig;
use crate::resolver::Resolver;
use crate::resolver::ResolverError;
use crate::resolver::policy::ResolverPolicy;

/// Writes a `module.sig` next to `dir`'s `module.json` over the
/// directory's content hash.
fn write_signature(dir: &std::path::Path, signer: &crate::signing::SigningKey) {
    let digest = crate::hash::hash_directory(dir).unwrap();
    let sig = crate::signing::ModuleSignature::new(signer, &digest, None).unwrap();
    let mut buf = Vec::new();
    sig.write(&mut buf).unwrap();
    fs::write(dir.join(crate::SIGNATURE_FILENAME), buf).unwrap();
}

#[tokio::test]
async fn transitive_dep_on_unallowed_host_is_rejected() {
    let cache = tempdir().unwrap();
    let r = resolver(&cache);

    let dep: DependencyName = "widget".parse().unwrap();
    let source: DependencySource = serde_json::from_str(
        r#"{"git": "https://bitbucket.org/acme/widget", "version": "^1.0.0"}"#,
    )
    .unwrap();

    let err = r
        .discover_versions(&dep, &source, DependencyScope::Transitive)
        .await
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        "`widget` git URL `https://bitbucket.org/acme/widget` targets host `bitbucket.org` which \
         is not in the configured allow list; to allow it, add `bitbucket.org` to \
         `allowed_transitive_hosts` in the `[modules]` section of your `sprocket.toml`"
    );
}

#[tokio::test]
async fn github_rejected_when_removed_from_transitive_allowlist() {
    let cache = tempdir().unwrap();
    let policy = ResolverPolicy::try_from(&ModulesConfig {
        allowed_transitive_hosts: vec!["gitlab.com".into()],
        ..ModulesConfig::default()
    })
    .unwrap();
    let r = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(crate::resolver::TrustStore::default())
        .lockfile(crate::Lockfile::default())
        .policy(policy)
        .build();

    let dep: DependencyName = "widget".parse().unwrap();
    let source: DependencySource =
        serde_json::from_str(r#"{"git": "https://github.com/acme/widget", "version": "^1.0.0"}"#)
            .unwrap();

    let err = r
        .discover_versions(&dep, &source, DependencyScope::Transitive)
        .await
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        "`widget` git URL `https://github.com/acme/widget` targets host `github.com` which is not \
         in the configured allow list; to allow it, add `github.com` to \
         `allowed_transitive_hosts` in the `[modules]` section of your `sprocket.toml`"
    );
}

#[tokio::test]
async fn resolve_tree_recurses_into_local_path_deps() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    let dep_dir = workdir.path().join("dep");

    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);

    let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
    let consumer = Manifest::parse(&bytes).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let tree = resolver(&cache).resolve_tree(&consumer).await.unwrap();
    let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
    assert!(matches!(&dep.source, ResolvedSource::Path { .. }));
    assert!(dep.dependencies.is_empty());
    assert_eq!(dep.version, None);
}

#[test]
fn cycle_identity_ignores_commit_and_selector() {
    let a = ResolvedSource::Git {
        git: "https://example.com/repo".parse().unwrap(),
        sha: "a".repeat(40).parse().unwrap(),
        selector: GitSelector::Version("^1".parse().unwrap()),
        path: Some("pkg".parse().unwrap()),
    };
    let b = ResolvedSource::Git {
        git: "https://example.com/repo".parse().unwrap(),
        sha: "b".repeat(40).parse().unwrap(),
        selector: GitSelector::Version("^2".parse().unwrap()),
        path: Some("pkg".parse().unwrap()),
    };
    assert_eq!(a.coordinates(), b.coordinates());

    let c = ResolvedSource::Git {
        git: "https://example.com/repo".parse().unwrap(),
        sha: "a".repeat(40).parse().unwrap(),
        selector: GitSelector::Version("^1".parse().unwrap()),
        path: Some("other".parse().unwrap()),
    };
    assert_ne!(a.coordinates(), c.coordinates());
}

#[test]
fn locked_git_materialization_rejects_version_selector_mismatch() {
    let entry = locked_git_entry(GitSelector::Version("^1".parse().unwrap()));
    let ResolvedSource::Git {
        sha,
        selector: locked_selector,
        ..
    } = &entry.source
    else {
        unreachable!();
    };

    assert!(!locked_selector_satisfies(
        &GitSelector::Version("^2".parse().unwrap()),
        sha,
        locked_selector,
    ));
}

#[test]
fn locked_git_materialization_rejects_commit_selector_mismatch() {
    let entry = locked_git_entry(GitSelector::Version("^1".parse().unwrap()));
    let ResolvedSource::Git {
        sha,
        selector: locked_selector,
        ..
    } = &entry.source
    else {
        unreachable!();
    };

    assert!(!locked_selector_satisfies(
        &GitSelector::Commit("0000000000000000000000000000000000000002".parse().unwrap()),
        sha,
        locked_selector,
    ));
}

#[test]
fn locked_git_materialization_rejects_tag_selector_mismatch() {
    let entry = locked_git_entry(GitSelector::Tag("v1.0.0".to_string()));
    let ResolvedSource::Git {
        sha,
        selector: locked_selector,
        ..
    } = &entry.source
    else {
        unreachable!();
    };

    assert!(!locked_selector_satisfies(
        &GitSelector::Tag("v2.0.0".to_string()),
        sha,
        locked_selector,
    ));
}

#[tokio::test]
async fn require_signed_exempts_local_path_dep() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let r = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(crate::resolver::TrustStore::default())
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                require_signed: true,
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .lockfile(crate::Lockfile::default())
        .build();
    let tree = r.resolve_tree(&consumer).await.unwrap();
    let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
    assert_eq!(dep.signer, None);
}

#[tokio::test]
async fn resolve_tree_verifies_parent_before_transitive_dependencies() {
    let workdir = tempdir().unwrap();
    let child_dir = workdir.path().join("child");
    write_manifest(&child_dir, "child", "1.0.0", &[]);

    let parent_dir = workdir.path().join("parent");
    let child_src = format!("{{\"path\":\"{}\"}}", json_path(&child_dir));
    write_manifest(&parent_dir, "parent", "1.0.0", &[("child", &child_src)]);
    fs::write(parent_dir.join("index.wdl"), b"workflow w {}").unwrap();
    fs::write(parent_dir.join("extra.wdl"), b"workflow e {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let parent_src = format!("{{\"path\":\"{}\"}}", json_path(&parent_dir));
    write_manifest(
        &consumer_dir,
        "consumer",
        "0.1.0",
        &[("parent", &parent_src)],
    );
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let r = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(crate::resolver::TrustStore::default())
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                max_materialized_files: Some(2),
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .lockfile(crate::Lockfile::default())
        .build();

    let err = r.resolve_tree(&consumer).await.unwrap_err();
    let ResolverError::MaterializedTreeLimitExceeded { dep, .. } = err else {
        panic!("expected parent validation to run before transitive dependency traversal");
    };
    assert_eq!(dep, "parent");
}

#[tokio::test]
async fn local_path_dep_signature_is_not_verified() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    let signer = crate::signing::test_utils::signing_key_from_seed(7);
    write_signature(&dep_dir, &signer);
    fs::write(dep_dir.join("extra.wdl"), b"workflow w {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let tree = resolver(&cache).resolve_tree(&consumer).await.unwrap();
    let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
    assert_eq!(dep.signer, None, "local path deps record no signer");
}

#[tokio::test]
async fn local_path_dep_bypasses_trust_pin() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    let signer = crate::signing::test_utils::signing_key_from_seed(7);
    write_signature(&dep_dir, &signer);

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let pinned = crate::signing::test_utils::signing_key_from_seed(99).verifying_key();
    let mut trust = crate::resolver::TrustStore::default();
    trust.insert_key(pinned);
    let cache = tempdir().unwrap();
    let r = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(trust)
        .lockfile(crate::Lockfile::default())
        .build();
    let tree = r.resolve_tree(&consumer).await.unwrap();
    let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
    assert_eq!(dep.signer, None, "local path deps bypass trust pins");
}

#[tokio::test]
async fn resolve_tree_rejects_symlink_escaping_module_root() {
    let workdir = tempdir().unwrap();
    let outside = workdir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.txt"), b"sensitive").unwrap();

    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    let link = dep_dir.join("escape");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&outside, &link).unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let err = resolver(&cache).resolve_tree(&consumer).await.unwrap_err();
    assert!(
        matches!(
            err,
            ResolverError::Walk(crate::module_walk::ModuleWalkError::Symlink(_))
        ),
        "expected `Walk(Symlink)`, got: {err}"
    );
}

#[tokio::test]
async fn resolve_tree_rejects_too_many_materialized_files() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    fs::write(dep_dir.join("one.wdl"), b"workflow one {}").unwrap();
    fs::write(dep_dir.join("two.wdl"), b"workflow two {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let r = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(crate::resolver::TrustStore::default())
        .lockfile(crate::Lockfile::default())
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                max_materialized_files: Some(1),
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .build();
    let err = r.resolve_tree(&consumer).await.unwrap_err();
    assert!(
        matches!(err, ResolverError::MaterializedTreeLimitExceeded { .. }),
        "expected `MaterializedTreeLimitExceeded`, got: {err}"
    );
}

#[tokio::test]
async fn resolve_tree_rejects_too_many_materialized_bytes() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    fs::write(dep_dir.join("big.wdl"), vec![b'x'; 1024]).unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let r = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(crate::resolver::TrustStore::default())
        .lockfile(crate::Lockfile::default())
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                max_materialized_bytes: Some(100),
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .build();
    let err = r.resolve_tree(&consumer).await.unwrap_err();
    assert!(
        matches!(err, ResolverError::MaterializedTreeLimitExceeded { .. }),
        "expected `MaterializedTreeLimitExceeded`, got: {err}"
    );
}

#[tokio::test]
async fn tree_limit_does_not_delete_local_path_dep() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    fs::write(dep_dir.join("one.wdl"), b"workflow one {}").unwrap();
    fs::write(dep_dir.join("two.wdl"), b"workflow two {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let r = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(crate::resolver::TrustStore::default())
        .lockfile(crate::Lockfile::default())
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                max_materialized_files: Some(1),
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .build();
    let err = r.resolve_tree(&consumer).await.unwrap_err();
    assert!(matches!(
        err,
        ResolverError::MaterializedTreeLimitExceeded { .. }
    ));
    assert!(
        dep_dir.exists(),
        "local-path dep directory must survive the limit error"
    );
    assert!(dep_dir.join("one.wdl").exists());
    assert!(dep_dir.join("two.wdl").exists());
}

#[tokio::test]
async fn local_path_dep_records_no_checksum() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;
    let dep_name = "dep".parse().unwrap();
    assert_eq!(
        lockfile.dependencies.get(&dep_name).unwrap().checksum,
        None,
        "local path deps carry no checksum"
    );
}

#[tokio::test]
async fn local_path_relock_records_no_checksum_across_content_changes() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    fs::write(dep_dir.join("index.wdl"), b"workflow original {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let (_, lockfile_v1) = resolve_and_lock(&cache, &consumer).await;
    assert_eq!(
        lockfile_v1
            .dependencies
            .get(&"dep".parse().unwrap())
            .unwrap()
            .checksum,
        None,
    );

    fs::write(dep_dir.join("index.wdl"), b"workflow changed {}").unwrap();

    let (_, lockfile_v2) = resolve_and_lock(&cache, &consumer).await;
    assert_eq!(
        lockfile_v2
            .dependencies
            .get(&"dep".parse().unwrap())
            .unwrap()
            .checksum,
        None,
    );
}

#[tokio::test]
async fn resolve_tree_detects_self_cycle() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("self-loop");

    let self_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&dep_dir, "loop", "1.0.0", &[("loop", &self_src)]);

    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("loop", &self_src)]);
    let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
    let consumer = Manifest::parse(&bytes).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let err = resolver(&cache).resolve_tree(&consumer).await.unwrap_err();
    let ResolverError::Cycle { path } = err else {
        panic!("expected `Cycle`, got: {err}");
    };
    assert_eq!(path.len(), 2, "self-loop should report a 2-element chain");
}

#[tokio::test]
async fn resolve_tree_detects_relative_local_path_cycle() {
    let workdir = tempdir().expect("failed to create temporary directory");
    let consumer_dir = workdir.path().join("consumer");
    let dep_a_dir = workdir.path().join("a");
    let dep_b_dir = workdir.path().join("b");

    write_manifest(&dep_a_dir, "a", "1.0.0", &[("b", r#"{"path":"../b"}"#)]);
    write_manifest(&dep_b_dir, "b", "1.0.0", &[("a", r#"{"path":"../a"}"#)]);
    write_manifest(
        &consumer_dir,
        "consumer",
        "0.1.0",
        &[("a", r#"{"path":"../a"}"#)],
    );

    let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME))
        .expect("failed to read consumer manifest");
    let consumer = Manifest::parse(&bytes).expect("failed to parse consumer manifest");
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().expect("failed to create cache directory");
    let err = resolver(&cache)
        .resolve_tree(&consumer)
        .await
        .expect_err("relative local path cycle should be rejected");
    let ResolverError::Cycle { path } = err else {
        panic!("expected `Cycle`, got: {err}");
    };
    assert_eq!(path, ["a", "b", "a"]);
}

#[tokio::test]
async fn discover_versions_returns_matching_tags() {
    let upstream = tempdir().unwrap();
    let repo = git2::Repository::init(upstream.path()).unwrap();
    let sig = git2::Signature::now("test", "test@example.com").unwrap();

    write_manifest(upstream.path(), "dep", "1.0.0", &[]);
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, "v1.0.0", &tree, &[])
        .unwrap();
    repo.tag_lightweight("v1.0.0", &repo.find_object(oid, None).unwrap(), false)
        .unwrap();

    let source = DependencySource::Git {
        url: url::Url::from_file_path(upstream.path()).unwrap(),
        selector: GitSelector::Version("^1".parse().unwrap()),
        path: None,
        extra: Default::default(),
    };

    let cache = tempdir().unwrap();
    let r = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(crate::resolver::TrustStore::default())
        .lockfile(crate::Lockfile::default())
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                allowed_schemes: vec!["https".into(), "ssh".into(), "file".into()],
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .build();
    let dep = "tasks".parse().unwrap();
    let versions = r
        .discover_versions(&dep, &source, DependencyScope::TopLevel)
        .await
        .unwrap();
    assert_eq!(
        versions,
        vec![semver::Version::parse("1.0.0").unwrap()],
        "should discover `v1.0.0` tag"
    );
}

#[tokio::test]
async fn discovers_all_matching_path_scoped_tags() -> Result<(), Box<dyn std::error::Error>> {
    let upstream = tempdir()?;
    let repo = git2::Repository::init(upstream.path())?;
    let sig = git2::Signature::now("test", "test@example.com")?;

    write_manifest(upstream.path(), "dep", "1.0.0", &[]);
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let tree = repo.find_tree(index.write_tree()?)?;
    let oid = repo.commit(Some("HEAD"), &sig, &sig, "versions", &tree, &[])?;
    let object = repo.find_object(oid, None)?;
    for tag in [
        "v9.0.0",
        "modules/other/v2.0.0",
        "modules/tasks/v1.0.0",
        "modules/tasks/v1.2.0",
    ] {
        repo.tag_lightweight(tag, &object, false)?;
    }

    let source = DependencySource::Git {
        // SAFETY: temporary directory paths always convert to file URLs.
        url: url::Url::from_file_path(upstream.path()).unwrap(),
        selector: GitSelector::Version("^1".parse()?),
        path: Some("modules/tasks".parse()?),
        extra: Default::default(),
    };
    let cache = tempdir()?;
    let resolver = crate::resolver::GitResolver::builder()
        .cache_root(cache.path())
        .trust(crate::resolver::TrustStore::default())
        .lockfile(crate::Lockfile::default())
        .policy(ResolverPolicy::try_from(&ModulesConfig {
            allowed_schemes: vec!["https".into(), "ssh".into(), "file".into()],
            ..ModulesConfig::default()
        })?)
        .build();
    let dependency = "tasks".parse()?;

    assert_eq!(
        resolver
            .discover_versions(&dependency, &source, DependencyScope::TopLevel)
            .await?,
        [
            semver::Version::parse("1.2.0")?,
            semver::Version::parse("1.0.0")?,
        ]
    );
    Ok(())
}
