//! Tests for the Git module resolver.

use std::fs;
use std::sync::Arc;

use tempfile::TempDir;
use tempfile::tempdir;

use super::*;

/// Builds a `Module` from a parsed `Manifest` and the directory it
/// lives in.
fn module(manifest: Manifest, root: &Path) -> Module {
    Module::new(Arc::new(manifest), root.to_path_buf())
}

fn checksum() -> crate::hash::ContentHash {
    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        .parse()
        .unwrap()
}

/// Builds a `module.json` at `dir` with the given name, version, and
/// Converts a path to a JSON-safe string (forward slashes on all
/// platforms).
fn json_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// optional `dependencies` map (each value is the JSON-encoded
/// dependency source).
fn write_manifest(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
    fs::create_dir_all(dir).unwrap();
    let deps_obj = if deps.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = deps.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        format!(",\"dependencies\":{{{}}}", entries.join(","))
    };
    let body =
        format!("{{\"name\":\"{name}\",\"version\":\"{version}\",\"license\":\"MIT\"{deps_obj}}}");
    fs::write(dir.join(crate::MANIFEST_FILENAME), body).unwrap();
}

fn resolver(cache: &TempDir) -> GitResolver {
    resolver_with_lockfile(cache, Lockfile::default())
}

fn resolver_with_lockfile(cache: &TempDir, lockfile: Lockfile) -> GitResolver {
    GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(lockfile)
        .build()
}

/// Resolves a consumer's tree and builds a lockfile from it.
async fn resolve_and_lock(cache: &TempDir, consumer: &Module) -> (GitResolver, Lockfile) {
    resolve_and_lock_with_config(
        cache,
        consumer,
        ResolverPolicy::default(),
        TrustStore::default(),
    )
    .await
}

async fn resolve_and_lock_with_config(
    cache: &TempDir,
    consumer: &Module,
    policy: ResolverPolicy,
    trust: TrustStore,
) -> (GitResolver, Lockfile) {
    let r = resolver(cache);
    let tree = r.resolve_tree(consumer).await.unwrap();
    let outcome =
        crate::resolver::lock::partial_relock(&consumer.manifest, &Lockfile::default(), &tree)
            .unwrap();
    let locked = GitResolver::builder()
        .cache_root(cache.path())
        .trust(trust)
        .lockfile(outcome.lockfile.clone())
        .policy(policy)
        .build();
    (locked, outcome.lockfile)
}

/// Writes a `module.sig` next to `dir`'s `module.json` over the
/// directory's content hash.
fn write_signature(dir: &Path, signer: &crate::signing::SigningKey) {
    let digest = crate::hash::hash_directory(dir).unwrap();
    // SAFETY: `None` contains no invalid signer identity fields.
    let sig = crate::signing::ModuleSignature::new(signer, &digest, None).unwrap();
    let mut buf = Vec::new();
    sig.write(&mut buf).unwrap();
    fs::write(dir.join(crate::SIGNATURE_FILENAME), buf).unwrap();
}

#[test]
fn git_resolver_facade_exposes_only_command_api() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GitResolver>();
    assert_send_sync::<GitResolverBuilder>();
    assert_send_sync::<CacheCleanStats>();
    assert_send_sync::<VerifyLockedReport>();
}

#[test]
fn builds_with_explicit_paths() {
    let cache = tempdir().unwrap();
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(Lockfile::default())
        .build();
    assert_eq!(r.cache_root(), cache.path());
    assert!(r.trust_store().keys.is_empty());
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
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(Lockfile::default())
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

fn hash_from_byte(byte: u8) -> crate::hash::ContentHash {
    format!("sha256:{}", hex::encode([byte; 32]))
        .parse()
        .unwrap()
}

#[test]
fn cycle_identity_ignores_commit_and_selector() {
    // Same repository URL and sub-path but different resolved commit
    // and selector: identical coordinates, so a self-dependency is a
    // cycle even at a different version.
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

    // A different sub-path is a different module.
    let c = ResolvedSource::Git {
        git: "https://example.com/repo".parse().unwrap(),
        sha: "a".repeat(40).parse().unwrap(),
        selector: GitSelector::Version("^1".parse().unwrap()),
        path: Some("other".parse().unwrap()),
    };
    assert_ne!(a.coordinates(), c.coordinates());
}

fn rel(s: &str) -> crate::relative_path::RelativePath {
    s.parse().unwrap()
}

#[test]
fn exclude_set_honors_gitignore_semantics() {
    let patterns = [rel("internal"), rel("scratch/*.wdl"), rel("secret/**")];
    let set = exclude_set(&patterns).unwrap();

    // A plain directory name excludes everything beneath it.
    assert!(set.is_match(Path::new("internal/private.wdl")));
    assert!(set.is_match(Path::new("internal/deep/nested.wdl")));
    // `*` matches within a single path segment only.
    assert!(set.is_match(Path::new("scratch/tmp.wdl")));
    assert!(!set.is_match(Path::new("scratch/sub/tmp.wdl")));
    // `**` crosses separators.
    assert!(set.is_match(Path::new("secret/a/b/c.wdl")));
    // Unrelated paths are not excluded.
    assert!(!set.is_match(Path::new("public.wdl")));
}

#[test]
fn resolve_normalized_subpath_matches_hyphen_variant() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("my-tasks")).unwrap();
    fs::write(dir.path().join("my-tasks/do-thing.wdl"), b"version 1.3\n").unwrap();
    let dep: DependencyName = "dep".parse().unwrap();

    // The symbolic components use underscores; the files use hyphens.
    let resolved = resolve_normalized_subpath(dir.path(), "my_tasks/do_thing", &dep).unwrap();
    assert_eq!(resolved.as_path(), Path::new("my-tasks/do-thing.wdl"));
}

#[test]
fn resolve_normalized_subpath_reports_ambiguity() {
    let dir = tempdir().unwrap();
    // Two files normalize to the same component `my_task`.
    fs::write(dir.path().join("my_task.wdl"), b"version 1.3\n").unwrap();
    fs::write(dir.path().join("my-task.wdl"), b"version 1.3\n").unwrap();
    let dep: DependencyName = "dep".parse().unwrap();

    let err = resolve_normalized_subpath(dir.path(), "my_task", &dep).unwrap_err();
    assert!(
        matches!(err, ResolverError::AmbiguousSubPath { .. }),
        "expected `AmbiguousSubPath`, got: {err}"
    );
}

#[test]
fn resolve_normalized_subpath_missing_is_not_found() {
    let dir = tempdir().unwrap();
    let dep: DependencyName = "dep".parse().unwrap();
    let err = resolve_normalized_subpath(dir.path(), "nope", &dep).unwrap_err();
    assert!(
        matches!(&err, ResolverError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound),
        "expected `NotFound` I/O error, got: {err}"
    );
}

#[tokio::test]
async fn materialize_returns_not_in_lockfile_when_dep_missing_from_lock() {
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
    let err = resolver(&cache)
        .materialize(&consumer, &"dep".parse().unwrap())
        .await
        .unwrap_err();
    assert!(
        matches!(err, ResolverError::NotInLockfile { .. }),
        "expected `NotInLockfile`, got: {err}"
    );
}

#[tokio::test]
async fn local_path_dep_records_no_checksum() {
    // Local path sources carry no checksum; their lockfile entry
    // records `None` and their content is read as-is.
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
async fn materialize_succeeds_with_matching_lockfile_checksum() {
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
    let (r, _) = resolve_and_lock(&cache, &consumer).await;
    let mat = r
        .materialize(&consumer, &"dep".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(mat.path, dep_dir.join("index.wdl").canonicalize().unwrap());
}

#[tokio::test]
async fn materialize_reads_local_path_content_as_is() {
    // Local path content is read as-is at materialization time, so
    // content that changed since locking does not fail the build.
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

    // Mutate the dep content after locking.
    fs::write(dep_dir.join("extra.wdl"), b"workflow extra {}").unwrap();

    let r = resolver_with_lockfile(&cache, lockfile);
    let mat = r
        .materialize(&consumer, &"dep".parse().unwrap())
        .await
        .expect("local path content is read as-is, not checksum-verified");
    assert_eq!(mat.path, dep_dir.join("index.wdl").canonicalize().unwrap());
}

#[tokio::test]
async fn materialize_rejects_changed_local_path_after_lock() {
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

    // Change the manifest to point to a different path.
    let other_dir = workdir.path().join("other");
    write_manifest(&other_dir, "dep", "1.0.0", &[]);
    fs::write(other_dir.join("index.wdl"), b"workflow w {}").unwrap();
    let other_src = format!("{{\"path\":\"{}\"}}", json_path(&other_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &other_src)]);
    let consumer2 =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer2 = module(consumer2, &consumer_dir);

    let r = resolver_with_lockfile(&cache, lockfile);
    let err = r
        .materialize(&consumer2, &"dep".parse().unwrap())
        .await
        .unwrap_err();
    assert!(
        matches!(err, ResolverError::LockfileSourceMismatch { .. }),
        "expected `LockfileSourceMismatch`, got: {err}"
    );
}

#[tokio::test]
async fn materialize_checks_transitive_git_policy_for_child_module()
-> Result<(), Box<dyn std::error::Error>> {
    let workdir = tempdir()?;
    let child_dir = workdir.path().join("child");
    let ssh_dep = r#"{"git":"ssh://git@github.com/openwdl/tasks","commit":"0000000000000000000000000000000000000001"}"#;
    write_manifest(&child_dir, "child", "1.0.0", &[("dep", ssh_dep)]);
    let child = Manifest::parse(&fs::read(child_dir.join(crate::MANIFEST_FILENAME))?)?;

    let parent_dir = workdir.path().join("parent");
    write_manifest(&parent_dir, "parent", "1.0.0", &[]);
    let parent = Manifest::parse(&fs::read(parent_dir.join(crate::MANIFEST_FILENAME))?)?;
    let parent = module(parent, &parent_dir);
    let child_name = "child".parse()?;
    let child = parent.child(child_name, Arc::new(child), child_dir);

    let cache = tempdir()?;
    let symbolic_path = "dep".parse()?;
    let err = match resolver(&cache).materialize(&child, &symbolic_path).await {
        Ok(_) => panic!("expected transitive git policy rejection"),
        Err(err) => err,
    };
    assert!(
        matches!(err, ResolverError::GitUrlPolicyViolation { .. }),
        "expected `GitUrlPolicyViolation`, got: {err}"
    );
    Ok(())
}

#[tokio::test]
async fn materialize_uses_top_level_git_policy_for_top_level_module()
-> Result<(), Box<dyn std::error::Error>> {
    let workdir = tempdir()?;
    let consumer_dir = workdir.path().join("consumer");
    let ssh_dep = r#"{"git":"ssh://git@github.com/openwdl/tasks","commit":"0000000000000000000000000000000000000001"}"#;
    write_manifest(&consumer_dir, "consumer", "1.0.0", &[("dep", ssh_dep)]);
    let consumer = Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME))?)?;
    let consumer = module(consumer, &consumer_dir);
    assert!(
        consumer.lockfile_scope.is_empty(),
        "consumer must be a top-level `Module`"
    );

    let cache = tempdir()?;
    let symbolic_path = "dep".parse()?;
    let err = match resolver(&cache)
        .materialize(&consumer, &symbolic_path)
        .await
    {
        Ok(_) => panic!("expected lockfile rejection, not git policy rejection"),
        Err(err) => err,
    };
    assert!(
        !matches!(err, ResolverError::GitUrlPolicyViolation { .. }),
        "top-level `ssh://` dep must pass `DependencyScope::TopLevel` policy; got: {err}"
    );
    Ok(())
}

fn locked_git_resolver(cache: &TempDir, dep: &str, entry: DependencyEntry) -> GitResolver {
    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(dep.parse().unwrap(), entry);
    resolver_with_lockfile(cache, lockfile)
}

fn locked_git_entry(selector: GitSelector) -> DependencyEntry {
    DependencyEntry {
        source: ResolvedSource::Git {
            git: "https://github.com/openwdl/tasks".parse().unwrap(),
            sha: "0000000000000000000000000000000000000001".parse().unwrap(),
            path: None,
            selector,
        },
        checksum: Some(checksum()),
        signer: None,
        dependencies: Default::default(),
    }
}

#[tokio::test]
async fn locked_git_materialization_rejects_version_selector_mismatch() {
    let cache = tempdir().unwrap();
    let r = locked_git_resolver(
        &cache,
        "dep",
        locked_git_entry(GitSelector::Version("^1".parse().unwrap())),
    );
    let dep = "dep".parse().unwrap();
    let url = "https://github.com/openwdl/tasks".parse().unwrap();
    let selector = GitSelector::Version("^2".parse().unwrap());
    let err = r
        .plan_git_materialization(
            &dep,
            &url,
            &selector,
            &None,
            DependencyScope::TopLevel,
            ResolutionMode::Locked {
                lockfile_scope: &[],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ResolverError::LockfileSourceMismatch { .. }));
}

#[tokio::test]
async fn locked_git_materialization_rejects_commit_selector_mismatch() {
    let cache = tempdir().unwrap();
    let r = locked_git_resolver(
        &cache,
        "dep",
        locked_git_entry(GitSelector::Version("^1".parse().unwrap())),
    );
    let dep = "dep".parse().unwrap();
    let url = "https://github.com/openwdl/tasks".parse().unwrap();
    let selector = GitSelector::Commit("0000000000000000000000000000000000000002".parse().unwrap());
    let err = r
        .plan_git_materialization(
            &dep,
            &url,
            &selector,
            &None,
            DependencyScope::TopLevel,
            ResolutionMode::Locked {
                lockfile_scope: &[],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ResolverError::LockfileSourceMismatch { .. }));
}

#[tokio::test]
async fn locked_git_materialization_rejects_tag_selector_mismatch() {
    let cache = tempdir().unwrap();
    let r = locked_git_resolver(
        &cache,
        "dep",
        locked_git_entry(GitSelector::Tag("v1.0.0".to_string())),
    );
    let dep = "dep".parse().unwrap();
    let url = "https://github.com/openwdl/tasks".parse().unwrap();
    let selector = GitSelector::Tag("v2.0.0".to_string());
    let err = r
        .plan_git_materialization(
            &dep,
            &url,
            &selector,
            &None,
            DependencyScope::TopLevel,
            ResolutionMode::Locked {
                lockfile_scope: &[],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ResolverError::LockfileSourceMismatch { .. }));
}

#[tokio::test]
async fn locked_git_materialization_uses_scoped_lockfile_entry()
-> Result<(), Box<dyn std::error::Error>> {
    let cache = tempdir()?;
    let parent_dir = cache.path().join("parent");
    let parent: DependencyName = "parent".parse()?;
    let dep: DependencyName = "dep".parse()?;
    let selector = GitSelector::Commit("0000000000000000000000000000000000000001".parse()?);

    let mut parent_entry = DependencyEntry {
        source: ResolvedSource::Path { path: parent_dir },
        checksum: Some(checksum()),
        signer: None,
        dependencies: Default::default(),
    };
    parent_entry
        .dependencies
        .insert(dep.clone(), locked_git_entry(selector.clone()));

    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(parent.clone(), parent_entry);
    let r = resolver_with_lockfile(&cache, lockfile);
    let url = "https://github.com/openwdl/tasks".parse()?;
    let plan = r
        .plan_git_materialization(
            &dep,
            &url,
            &selector,
            &None,
            DependencyScope::Transitive,
            ResolutionMode::Locked {
                lockfile_scope: &[parent],
            },
        )
        .await?;
    assert_eq!(
        plan.commit,
        "0000000000000000000000000000000000000001".parse()?
    );
    Ok(())
}

#[tokio::test]
async fn materialize_resolves_default_entrypoint() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
    let consumer = Manifest::parse(&bytes).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let (r, _) = resolve_and_lock(&cache, &consumer).await;
    let mat = r
        .materialize(&consumer, &"dep".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(mat.path, dep_dir.join("index.wdl").canonicalize().unwrap());
    assert!(matches!(mat.source, ResolvedSource::Path { .. }));
}

#[tokio::test]
async fn materialize_resolves_named_entrypoint() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    fs::create_dir_all(&dep_dir).unwrap();
    // Manifest declares an explicit `entrypoint` other than the
    // default `index.wdl`.
    fs::write(
        dep_dir.join(crate::MANIFEST_FILENAME),
        br#"{"name":"dep","license":"MIT","entrypoint":"main.wdl"}"#,
    )
    .unwrap();
    fs::write(dep_dir.join("main.wdl"), b"workflow w {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let (r, _) = resolve_and_lock(&cache, &consumer).await;
    let mat = r
        .materialize(&consumer, &"dep".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(mat.path, dep_dir.join("main.wdl").canonicalize().unwrap());
    assert!(matches!(mat.source, ResolvedSource::Path { .. }));
}

#[tokio::test]
async fn materialize_resolves_sub_path_to_wdl_file() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    fs::write(dep_dir.join("cut.wdl"), b"workflow cut {}").unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
    let consumer = Manifest::parse(&bytes).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let (r, _) = resolve_and_lock(&cache, &consumer).await;
    let mat = r
        .materialize(&consumer, &"dep/cut".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(mat.path, dep_dir.join("cut.wdl").canonicalize().unwrap());
    assert!(matches!(mat.source, ResolvedSource::Path { .. }));
}

#[tokio::test]
async fn manifest_parse_rejects_invalid_commit_sha() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    let bad_src = "{\"git\":\"https://example.com/repo.git\",\"commit\":\"not-a-sha\"}";
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", bad_src)]);
    let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
    let err = Manifest::parse(&bytes).unwrap_err();
    assert!(
        matches!(err, crate::manifest::ManifestError::InvalidJson(_)),
        "expected `InvalidJson` from manifest parse, got: {err}"
    );
}

#[tokio::test]
async fn materialize_blocks_excluded_glob() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    fs::create_dir_all(dep_dir.join("internal")).unwrap();
    let body = r#"{"name":"dep","license":"MIT","exclude":["internal/**"]}"#;
    fs::write(dep_dir.join(crate::MANIFEST_FILENAME), body).unwrap();
    fs::write(
        dep_dir.join("internal").join("private.wdl"),
        b"workflow w {}",
    )
    .unwrap();

    let consumer_dir = workdir.path().join("consumer");
    let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let (r, _) = resolve_and_lock(&cache, &consumer).await;
    let err = r
        .materialize(&consumer, &"dep/internal/private".parse().unwrap())
        .await
        .unwrap_err();
    let ResolverError::MissingFile { kind, .. } = err else {
        panic!("expected `MissingFile`, got: {err}");
    };
    assert_eq!(kind, MissingFileKind::Excluded);
}

#[tokio::test]
async fn materialize_rejects_entrypoint_symlink_escaping_dep_root() {
    let workdir = tempdir().unwrap();
    let outside = workdir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("evil.wdl"), b"workflow evil {}").unwrap();

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

    // Replace the entrypoint with a symlink after locking.
    fs::remove_file(dep_dir.join("index.wdl")).unwrap();
    let target = outside.join("evil.wdl");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, dep_dir.join("index.wdl")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target, dep_dir.join("index.wdl")).unwrap();

    let r = resolver_with_lockfile(&cache, lockfile);
    let err = r
        .materialize(&consumer, &"dep".parse().unwrap())
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            ResolverError::MaterializedSymlink { .. }
                | ResolverError::Walk(crate::module_walk::ModuleWalkError::Symlink(_))
        ),
        "got: {err}"
    );
}

#[tokio::test]
async fn materialize_errors_on_undeclared_dependency() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
    let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
    let consumer = Manifest::parse(&bytes).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let err = resolver(&cache)
        .materialize(&consumer, &"missing".parse().unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, ResolverError::NotADependency { .. }));
}

#[tokio::test]
async fn require_signed_exempts_local_path_dep() {
    // `require_signed` gates signature verification, which local
    // path sources are exempt from, so an unsigned local path dep
    // resolves even when the policy requires signatures.
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
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                require_signed: true,
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .lockfile(Lockfile::default())
        .build();
    let tree = r.resolve_tree(&consumer).await.unwrap();
    let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
    assert_eq!(dep.signer, None);
}

#[tokio::test]
async fn resolve_tree_verifies_parent_before_transitive_dependencies() {
    // Structural validation (here, the file-count limit) runs on a
    // parent before its transitive dependencies are traversed. The
    // parent exceeds the limit and the child does not, so the error
    // must name the parent.
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
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                max_materialized_files: Some(2),
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .lockfile(Lockfile::default())
        .build();

    let err = r.resolve_tree(&consumer).await.unwrap_err();
    let ResolverError::MaterializedTreeLimitExceeded { dep, .. } = err else {
        panic!("expected parent validation to run before transitive dependency traversal");
    };
    assert_eq!(dep, "parent");
}

#[tokio::test]
async fn local_path_dep_signature_is_not_verified() {
    // Local path sources are read as-is and are not subject to
    // signature verification, so a signature that no longer matches
    // the (tampered) content does not fail resolution.
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    write_manifest(&dep_dir, "dep", "1.0.0", &[]);
    let signer = crate::signing::test_utils::signing_key_from_seed(7);
    write_signature(&dep_dir, &signer);
    // Modify a file after signing — this would invalidate a Git
    // dependency's signature, but a local path dep is not checked.
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
    // Trust pins apply to signature verification, which local path
    // sources are exempt from, so a mismatched pin does not fail
    // resolution of a local path dependency.
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
    let mut trust = TrustStore::default();
    trust.insert_key(pinned);
    let cache = tempdir().unwrap();
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(trust)
        .lockfile(Lockfile::default())
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
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(Lockfile::default())
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
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(Lockfile::default())
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
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(Lockfile::default())
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
async fn local_path_relock_records_no_checksum_across_content_changes() {
    // A local path dependency carries no checksum, so its lockfile
    // entry stays checksum-free even as its content changes between
    // relocks; the content is read as-is at execution time.
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

    // The dep declares itself as one of its own dependencies.
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
async fn materialize_rejects_entrypoint_symlink_to_nested_metadata() {
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

    // Replace the entrypoint with a symlink to nested metadata
    // after locking.
    fs::remove_file(dep_dir.join("index.wdl")).unwrap();
    fs::create_dir_all(dep_dir.join("nested").join(".git")).unwrap();
    fs::write(
        dep_dir.join("nested").join(".git").join("config"),
        b"private",
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        dep_dir.join("nested").join(".git").join("config"),
        dep_dir.join("index.wdl"),
    )
    .unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(
        dep_dir.join("nested").join(".git").join("config"),
        dep_dir.join("index.wdl"),
    )
    .unwrap();

    let r = resolver_with_lockfile(&cache, lockfile);
    let err = r
        .materialize(&consumer, &"dep".parse().unwrap())
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            ResolverError::MaterializedSymlink { .. }
                | ResolverError::Walk(crate::module_walk::ModuleWalkError::Symlink(_))
        ),
        "expected a symlink rejection, got: {err}"
    );
}

#[tokio::test]
async fn materialize_accepts_unsigned_when_lockfile_has_no_signer() {
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
    let (r, _) = resolve_and_lock(&cache, &consumer).await;
    let mat = r
        .materialize(&consumer, &"dep".parse().unwrap())
        .await
        .unwrap();
    assert!(mat.path.exists());
}

#[test]
fn verify_locked_verifies_matching_cache_leaf() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let dep: DependencyName = "dep".parse().unwrap();
    let git = url::Url::parse("https://github.com/openwdl/tasks").unwrap();
    let commit: GitCommit = "0000000000000000000000000000000000000001".parse().unwrap();
    let leaf = CacheKey::from_git_url(&git, &commit).absolute_path(cache.path());
    fs::create_dir_all(&leaf).unwrap();
    write_manifest(&leaf, "dep", "1.0.0", &[]);
    fs::write(leaf.join("index.wdl"), b"workflow w {}").unwrap();
    let checksum = crate::hash::hash_directory(&leaf).unwrap();

    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(
        dep,
        DependencyEntry {
            source: ResolvedSource::Git {
                git,
                sha: commit.clone(),
                selector: GitSelector::Commit(commit.as_str().parse().unwrap()),
                path: None,
            },
            checksum: Some(checksum),
            signer: None,
            dependencies: Default::default(),
        },
    );
    let r = resolver_with_lockfile(&cache, lockfile);
    assert_eq!(r.verify_locked(&consumer).unwrap(), 1);
}

#[test]
fn verify_locked_rejects_tampered_cache_leaf() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let dep: DependencyName = "dep".parse().unwrap();
    let git = url::Url::parse("https://github.com/openwdl/tasks").unwrap();
    let commit: GitCommit = "0000000000000000000000000000000000000001".parse().unwrap();
    let leaf = CacheKey::from_git_url(&git, &commit).absolute_path(cache.path());
    fs::create_dir_all(&leaf).unwrap();
    write_manifest(&leaf, "dep", "1.0.0", &[]);
    fs::write(leaf.join("index.wdl"), b"workflow w {}").unwrap();
    let checksum = crate::hash::hash_directory(&leaf).unwrap();
    fs::write(leaf.join("index.wdl"), b"workflow tampered {}").unwrap();

    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(
        dep,
        DependencyEntry {
            source: ResolvedSource::Git {
                git,
                sha: commit.clone(),
                selector: GitSelector::Commit(commit.as_str().parse().unwrap()),
                path: None,
            },
            checksum: Some(checksum),
            signer: None,
            dependencies: Default::default(),
        },
    );
    let r = resolver_with_lockfile(&cache, lockfile);
    let err = r.verify_locked(&consumer).unwrap_err();
    assert!(matches!(err, ResolverError::ChecksumMismatch { .. }));
}

#[test]
fn verify_locked_returns_not_fetched_when_cache_leaf_missing() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let dep: DependencyName = "dep".parse().unwrap();
    let git = url::Url::parse("https://github.com/openwdl/tasks").unwrap();
    let commit: GitCommit = "0000000000000000000000000000000000000001".parse().unwrap();
    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(
        dep,
        DependencyEntry {
            source: ResolvedSource::Git {
                git,
                sha: commit.clone(),
                selector: GitSelector::Commit(commit.as_str().parse().unwrap()),
                path: None,
            },
            checksum: Some(hash_from_byte(1)),
            signer: None,
            dependencies: Default::default(),
        },
    );
    let r = resolver_with_lockfile(&cache, lockfile);
    let err = r.verify_locked(&consumer).unwrap_err();
    assert!(matches!(err, ResolverError::NotFetched { .. }));
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
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(Lockfile::default())
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
