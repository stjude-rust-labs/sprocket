use std::fs;
use std::path::Path;
use std::sync::Arc;

use tempfile::tempdir;

use super::super::materialize::exclude_set;
use super::super::materialize::read_manifest;
use super::super::materialize::resolve_normalized_subpath;
use super::json_path;
use super::module;
use super::resolve_and_lock;
use super::resolver;
use super::resolver_with_lockfile;
use super::write_manifest;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::manifest::ManifestError;
use crate::resolver::MissingFileKind;
use crate::resolver::Resolver;
use crate::resolver::ResolverError;

fn rel(s: &str) -> crate::relative_path::RelativePath {
    s.parse().unwrap()
}

#[test]
fn exclude_set_honors_gitignore_semantics() {
    let patterns = [rel("internal"), rel("scratch/*.wdl"), rel("secret/**")];
    let set = exclude_set(&patterns).unwrap();

    assert!(set.is_match(Path::new("internal/private.wdl")));
    assert!(set.is_match(Path::new("internal/deep/nested.wdl")));
    assert!(set.is_match(Path::new("scratch/tmp.wdl")));
    assert!(!set.is_match(Path::new("scratch/sub/tmp.wdl")));
    assert!(set.is_match(Path::new("secret/a/b/c.wdl")));
    assert!(!set.is_match(Path::new("public.wdl")));
}

#[test]
fn resolve_normalized_subpath_matches_hyphen_variant() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("my-tasks")).unwrap();
    fs::write(dir.path().join("my-tasks/do-thing.wdl"), b"version 1.3\n").unwrap();
    let dep: DependencyName = "dep".parse().unwrap();

    let resolved = resolve_normalized_subpath(dir.path(), "my_tasks/do_thing", &dep).unwrap();
    assert_eq!(resolved.as_path(), Path::new("my-tasks/do-thing.wdl"));
}

#[test]
fn resolve_normalized_subpath_reports_ambiguity() {
    let dir = tempdir().unwrap();
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
    assert!(matches!(
        mat.source,
        crate::lockfile::ResolvedSource::Path { .. }
    ));
}

#[tokio::test]
async fn materialize_resolves_named_entrypoint() {
    let workdir = tempdir().unwrap();
    let dep_dir = workdir.path().join("dep");
    fs::create_dir_all(&dep_dir).unwrap();
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
    assert!(matches!(
        mat.source,
        crate::lockfile::ResolvedSource::Path { .. }
    ));
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
    assert!(matches!(
        mat.source,
        crate::lockfile::ResolvedSource::Path { .. }
    ));
}

#[test]
fn manifest_parse_rejects_invalid_commit_sha() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    let bad_src = "{\"git\":\"https://example.com/repo.git\",\"commit\":\"not-a-sha\"}";
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", bad_src)]);

    let err = read_manifest(&consumer_dir).unwrap_err();
    assert!(
        matches!(err, ResolverError::Manifest(ManifestError::InvalidJson(_))),
        "expected `Manifest(InvalidJson)`, got: {err}"
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
