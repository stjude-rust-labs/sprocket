//! Optional workspace module metadata.
//!
//! A documented workspace may contain one or more WDL modules (directories
//! with a `module.json` manifest). This module discovers them by recursively
//! scanning the workspace, so documentation generation can label module
//! directories with their names and render a module overview, whether a module
//! sits at the workspace root or is nested within it, such as a monorepo of
//! sibling modules under a manifest-less root.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use wdl_modules::module::Module;
use wdl_modules::module::is_module_root;

use crate::error::DocResult;

/// Metadata for a single WDL module, either the workspace root or one of its
/// local-path dependencies.
#[derive(Clone, Debug)]
pub(crate) struct ModuleMetadata {
    /// The module's root directory, relative to the workspace root. The
    /// workspace root module itself uses an empty path.
    root: PathBuf,
    /// The module's display name.
    name: String,
    /// A brief description of the module.
    description: Option<String>,
    /// The path to the module's entrypoint WDL file, relative to
    /// [`root`](Self::root).
    entrypoint: PathBuf,
}

impl From<(&Module, PathBuf)> for ModuleMetadata {
    fn from((module, root): (&Module, PathBuf)) -> Self {
        let manifest = &module.manifest;
        Self {
            root,
            name: manifest.name.clone(),
            description: manifest.description.clone(),
            entrypoint: manifest.entrypoint_filename().to_path_buf(),
        }
    }
}

impl ModuleMetadata {
    /// Returns the module's root directory, relative to the workspace root.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the module's display name.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Returns the module's description, if any.
    pub(crate) fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the path to the module's entrypoint WDL file, relative to the
    /// workspace root.
    pub(crate) fn entrypoint(&self) -> PathBuf {
        self.root.join(&self.entrypoint)
    }
}

/// Metadata for a workspace that contains one or more WDL modules, discovered
/// by recursively scanning the workspace for `module.json` manifests.
#[derive(Clone, Debug)]
pub(crate) struct WorkspaceMetadata {
    /// The module rooted at the workspace root, if the workspace root itself
    /// has a `module.json` manifest. A workspace that only contains nested
    /// modules (e.g. a monorepo of sibling modules) has no root module.
    root: Option<ModuleMetadata>,
    /// All modules discovered in the workspace, keyed by their root directory
    /// relative to the workspace root. The root module, when present, is keyed
    /// by the empty path.
    modules: BTreeMap<PathBuf, ModuleMetadata>,
}

impl WorkspaceMetadata {
    /// Loads workspace module metadata rooted at `workspace_root`.
    ///
    /// Recursively scans `workspace_root` for `module.json` manifests and
    /// records a module for each one, keyed by its directory relative to the
    /// workspace root. Symlinked directories are not followed. A `module.json`
    /// that does not parse as a WDL module is skipped (with a warning) rather
    /// than aborting discovery of the rest of the workspace.
    ///
    /// Returns `Ok(None)` when no modules are found, so plain WDL directories
    /// keep their non-module documentation layout.
    pub(crate) fn load(workspace_root: &Path) -> DocResult<Option<Self>> {
        // Canonicalize the workspace root once so discovered module roots can
        // be made relative to it consistently.
        let workspace_root_canonical = workspace_root.canonicalize()?;

        let mut modules: BTreeMap<PathBuf, ModuleMetadata> = BTreeMap::new();
        let mut stack = vec![workspace_root_canonical.clone()];
        while let Some(dir) = stack.pop() {
            if is_module_root(&dir) {
                match Module::load_from_path(&dir) {
                    Ok(module) => {
                        let relative = dir
                            .strip_prefix(&workspace_root_canonical)
                            .unwrap_or_else(|_| Path::new(""))
                            .to_path_buf();
                        modules
                            .entry(relative.clone())
                            .or_insert_with(|| ModuleMetadata::from((&module, relative)));
                    }
                    Err(error) => {
                        // A `module.json` that does not parse as a WDL module
                        // (e.g. a manifest missing a required field) is skipped
                        // with a warning rather than failing the whole run, so
                        // the rest of the workspace still documents.
                        let manifest = dir.join(wdl_modules::MANIFEST_FILENAME);
                        tracing::warn!(
                            "skipping module manifest `{}`: {error}",
                            manifest.display()
                        );
                    }
                }
            }

            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                // Descend into real subdirectories only. Symlinks are not
                // followed, so the scan cannot get stuck in a cycle.
                if entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false) {
                    stack.push(entry.path());
                }
            }
        }

        if modules.is_empty() {
            return Ok(None);
        }

        let root = modules.get(Path::new("")).cloned();
        Ok(Some(Self { root, modules }))
    }

    /// Returns the module rooted at the workspace root, if the workspace root
    /// itself has a `module.json` manifest.
    pub(crate) fn root(&self) -> Option<&ModuleMetadata> {
        self.root.as_ref()
    }

    /// Returns the metadata for the module rooted exactly at `root`, a
    /// directory path relative to the workspace root.
    ///
    /// Unlike [`module_for_document`](Self::module_for_document), this only
    /// matches a module whose root is exactly `root`, not one that merely
    /// contains it.
    pub(crate) fn module_at_root(&self, root: &Path) -> Option<&ModuleMetadata> {
        self.modules.get(root)
    }

    /// Returns an iterator over all modules in the workspace, including the
    /// root module.
    pub(crate) fn modules(&self) -> impl Iterator<Item = &ModuleMetadata> {
        self.modules.values()
    }

    /// Returns the metadata for the module that contains `doc_path`, a WDL
    /// document path relative to the workspace root.
    ///
    /// When multiple modules' roots are prefixes of `doc_path` (e.g. the
    /// workspace root and a nested dependency), the most specific (deepest)
    /// match is returned.
    pub(crate) fn module_for_document(&self, doc_path: &Path) -> Option<&ModuleMetadata> {
        self.modules
            .iter()
            .filter(|(root, _)| doc_path.starts_with(root))
            .max_by_key(|(root, _)| root.components().count())
            .map(|(_, module)| module)
    }

    /// Returns the documentation directory for `doc_path`, a WDL document
    /// path relative to the workspace root.
    ///
    /// When `doc_path` is the entrypoint of a local-path dependency module,
    /// its documentation collapses into that module's root directory rather
    /// than a subdirectory named after the entrypoint file, so that a
    /// module's documentation lives at, e.g., `modules/wards/` instead of
    /// `modules/wards/wards/`. The workspace root's own entrypoint is exempted
    /// from this collapse so that it does not collide with the site's home
    /// page, and instead keeps its usual file-stem path (e.g. `main`).
    pub(crate) fn documentation_path(&self, doc_path: &Path) -> PathBuf {
        if let Some(module) = self.module_for_document(doc_path)
            && !module.root.as_os_str().is_empty()
            && module.entrypoint() == doc_path
        {
            return module.root.clone();
        }

        doc_path.with_extension("")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    /// Writes a module manifest to `root`.
    fn write_manifest(root: &Path, manifest: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join(wdl_modules::MANIFEST_FILENAME), manifest).unwrap();
    }

    /// The minimal module-workspace fixture checked into the repository,
    /// reused here as a realistic fixture for module metadata tests. Unlike
    /// the local `wdl-doc-showcase/` demo (which is untracked and not
    /// guaranteed to exist in a fresh clone), this fixture is committed
    /// under `tests/fixtures/` specifically so these tests are
    /// self-contained.
    fn fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/module-workspace")
    }

    /// Builds a `TempDir` containing copies of the fixture's `module.json`
    /// manifests, mirroring its on-disk module layout (root plus the local
    /// `wards` and `enchantment` dependencies the root manifest declares)
    /// without copying the WDL source files.
    fn module_workspace() -> TempDir {
        let fixture = fixture_root();
        let dir = tempfile::tempdir().unwrap();

        for relative_manifest in [
            "module.json",
            "modules/wards/module.json",
            "modules/enchantment/module.json",
        ] {
            let src = fixture.join(relative_manifest);
            let dst = dir.path().join(relative_manifest);
            // SAFETY: `relative_manifest` always has a parent component
            // (`modules/wards` and friends, or the workspace root itself).
            fs::create_dir_all(dst.parent().unwrap()).unwrap();
            fs::copy(&src, &dst).unwrap();
        }

        dir
    }

    #[test]
    fn returns_none_without_manifest() {
        let dir = tempfile::tempdir().unwrap();
        assert!(WorkspaceMetadata::load(dir.path()).unwrap().is_none());
    }

    #[test]
    fn loads_root_and_local_dependency_modules() {
        let dir = module_workspace();
        let metadata = WorkspaceMetadata::load(dir.path()).unwrap().unwrap();

        assert_eq!(metadata.root().unwrap().name(), "spellcraft-showcase");
        assert_eq!(
            metadata
                .module_for_document(Path::new("modules/wards/wards.wdl"))
                .unwrap()
                .name(),
            "wards"
        );
    }

    #[test]
    fn collapses_local_dependency_entrypoint_path() {
        let dir = module_workspace();
        let metadata = WorkspaceMetadata::load(dir.path()).unwrap().unwrap();

        assert_eq!(
            metadata.documentation_path(Path::new("modules/wards/wards.wdl")),
            PathBuf::from("modules/wards")
        );
        assert_eq!(
            metadata.documentation_path(Path::new("main.wdl")),
            PathBuf::from("main")
        );
    }

    #[test]
    fn ignores_missing_declared_dependency() {
        // Discovery scans the filesystem rather than following declared
        // dependencies, so a dependency that is not present on disk is simply
        // not documented; the root module still loads.
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"{
                "name": "root",
                "version": "1.0.0",
                "license": "MIT",
                "dependencies": {
                    "missing": { "path": "missing" }
                }
            }"#,
        );

        let metadata = WorkspaceMetadata::load(dir.path()).unwrap().unwrap();
        assert_eq!(metadata.root().unwrap().name(), "root");
        assert_eq!(metadata.modules.len(), 1);
    }

    #[test]
    fn skips_unparsable_manifest() {
        // A nested `module.json` that does not parse as a WDL module is
        // skipped rather than aborting discovery; the valid root still loads.
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"{
                "name": "root",
                "version": "1.0.0",
                "license": "MIT"
            }"#,
        );
        write_manifest(&dir.path().join("broken"), "{");

        let metadata = WorkspaceMetadata::load(dir.path()).unwrap().unwrap();
        assert_eq!(metadata.root().unwrap().name(), "root");
        assert!(metadata.module_at_root(Path::new("broken")).is_none());
        assert_eq!(metadata.modules.len(), 1);
    }

    #[test]
    fn does_not_scan_outside_the_workspace() {
        // Discovery is confined to the workspace subtree, so a module in a
        // sibling directory is not documented.
        let parent = tempfile::tempdir().unwrap();
        let workspace = parent.path().join("workspace");
        let sibling = parent.path().join("sibling");
        write_manifest(
            &workspace,
            r#"{
                "name": "root",
                "version": "1.0.0",
                "license": "MIT"
            }"#,
        );
        write_manifest(
            &sibling,
            r#"{
                "name": "sibling",
                "version": "1.0.0",
                "license": "MIT"
            }"#,
        );

        let metadata = WorkspaceMetadata::load(&workspace).unwrap().unwrap();
        assert_eq!(metadata.modules.len(), 1);
    }

    #[test]
    fn discovers_nested_modules_without_a_root_manifest() {
        // A workspace root with no `module.json` of its own still documents
        // the modules nested within it (e.g. a monorepo of sibling modules).
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            &dir.path().join("fq"),
            r#"{
                "name": "fq",
                "version": "0.12.0",
                "license": "MIT"
            }"#,
        );
        write_manifest(
            &dir.path().join("samtools"),
            r#"{
                "name": "samtools",
                "version": "1.21.0",
                "license": "MIT"
            }"#,
        );

        let metadata = WorkspaceMetadata::load(dir.path()).unwrap().unwrap();
        // No manifest at the root, so there is no root module.
        assert!(metadata.root().is_none());
        assert_eq!(metadata.modules.len(), 2);
        assert_eq!(
            metadata
                .module_for_document(Path::new("fq/fq.wdl"))
                .unwrap()
                .name(),
            "fq"
        );
        assert_eq!(
            metadata
                .module_at_root(Path::new("samtools"))
                .unwrap()
                .name(),
            "samtools"
        );
    }
}
