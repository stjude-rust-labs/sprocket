//! Optional workspace module metadata.
//!
//! A documented workspace may or may not be a WDL module (i.e. it may or may
//! not have a `module.json` manifest at its root). When it is, this module
//! loads the root manifest along with the manifests of any local-path
//! dependencies so that documentation generation can use module names,
//! versions, and descriptions when building navigation.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use wdl_modules::dependency::DependencySource;
use wdl_modules::module::Module;
use wdl_modules::module::is_module_root;

use crate::error::DocResult;

/// Metadata for a single WDL module, either the workspace root or one of its
/// local-path dependencies.
// NOTE: `name`, `version`, and `description` are read by tests but not yet by
// any non-test code path; upcoming navigation work (module names and
// descriptions in the sidebar) will consume them. `#[expect(dead_code)]`
// would fire inconsistently here since the dead-code lint only sees these
// fields as read in the `#[cfg(test)]` configuration.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct ModuleMetadata {
    /// The module's root directory, relative to the workspace root. The
    /// workspace root module itself uses an empty path.
    root: PathBuf,
    /// The module's display name.
    name: String,
    /// The module version.
    version: String,
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
            version: manifest.version.to_string(),
            description: manifest.description.clone(),
            entrypoint: manifest.entrypoint_filename().to_path_buf(),
        }
    }
}

impl ModuleMetadata {
    /// Returns the module's root directory, relative to the workspace root.
    #[expect(dead_code)]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the module's display name.
    // NOTE: only called by tests today; see the struct-level `#[allow]`.
    #[allow(dead_code)]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Returns the module version.
    // NOTE: only called by tests today; see the struct-level `#[allow]`.
    #[allow(dead_code)]
    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    /// Returns the module's description, if any.
    #[expect(dead_code)]
    pub(crate) fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the path to the module's entrypoint WDL file, relative to the
    /// workspace root.
    pub(crate) fn entrypoint(&self) -> PathBuf {
        self.root.join(&self.entrypoint)
    }
}

/// Metadata for a workspace that is a WDL module, including the root module
/// and any local-path dependency modules reachable from it.
#[derive(Clone, Debug)]
pub(crate) struct WorkspaceMetadata {
    /// The workspace root module.
    // NOTE: only read by tests today (via `root()`, itself only called by
    // tests); upcoming navigation work will surface the root module's name
    // and version in the sidebar. `#[expect(dead_code)]` would fire
    // inconsistently since the dead-code lint only sees this field as read
    // in the `#[cfg(test)]` configuration.
    #[allow(dead_code)]
    root: ModuleMetadata,
    /// All modules in the workspace, keyed by their root directory relative
    /// to the workspace root. This includes the root module at the empty
    /// path.
    modules: BTreeMap<PathBuf, ModuleMetadata>,
}

impl WorkspaceMetadata {
    /// Loads workspace module metadata rooted at `workspace_root`.
    ///
    /// Returns `Ok(None)` when `workspace_root` has no `module.json`
    /// manifest (i.e. the workspace is not a WDL module). Otherwise, loads
    /// the root manifest and recursively follows `DependencySource::LocalPath`
    /// dependencies whose canonical roots remain inside the workspace,
    /// storing each module's root relative to the workspace.
    pub(crate) fn load(workspace_root: &Path) -> DocResult<Option<Self>> {
        if !is_module_root(workspace_root) {
            return Ok(None);
        }

        // Canonicalize the workspace root once so that dependency roots can
        // be checked for containment and made relative to it consistently.
        let workspace_root_canonical = workspace_root.canonicalize()?;

        let root_module = Module::load_from_path(workspace_root)?;
        let root_metadata = ModuleMetadata::from((&root_module, PathBuf::new()));

        let mut modules = BTreeMap::new();
        modules.insert(PathBuf::new(), root_metadata.clone());

        let mut queue = vec![root_module];
        while let Some(module) = queue.pop() {
            for source in module.manifest.dependencies.values() {
                let DependencySource::LocalPath { path, .. } = source else {
                    // Only local-path dependencies live on disk within
                    // reach of this workspace; Git dependencies are not
                    // resolved for documentation purposes.
                    continue;
                };

                let dependency_root = module.resolve_local_path(path);
                let dependency_root_canonical = dependency_root.canonicalize()?;

                let Ok(relative_root) =
                    dependency_root_canonical.strip_prefix(&workspace_root_canonical)
                else {
                    // The dependency resolves outside the workspace; it is
                    // not documented alongside it.
                    continue;
                };
                let relative_root = relative_root.to_path_buf();

                if modules.contains_key(&relative_root) {
                    continue;
                }

                let dependency_module = Module::load_from_path(&dependency_root_canonical)?;
                let dependency_metadata =
                    ModuleMetadata::from((&dependency_module, relative_root.clone()));
                modules.insert(relative_root, dependency_metadata);
                queue.push(dependency_module);
            }
        }

        Ok(Some(Self {
            root: root_metadata,
            modules,
        }))
    }

    /// Returns the workspace root module's metadata.
    // NOTE: only called by tests today; `#[expect(dead_code)]` would fire
    // inconsistently since it is used in the `#[cfg(test)]` configuration.
    #[allow(dead_code)]
    pub(crate) fn root(&self) -> &ModuleMetadata {
        &self.root
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
    /// module's documentation lives at, e.g., `modules/qc/` instead of
    /// `modules/qc/qc/`. The workspace root's own entrypoint is exempted
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

    /// The showcase workspace checked into the repository, reused here as a
    /// realistic fixture for module metadata tests.
    fn showcase_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wdl-doc-showcase")
    }

    /// Builds a `TempDir` containing copies of the showcase's `module.json`
    /// manifests, mirroring its on-disk module layout (root plus the local
    /// `qc` and `alignment` dependencies the root manifest declares) without
    /// copying the WDL source files.
    fn module_workspace() -> TempDir {
        let showcase = showcase_root();
        let dir = tempfile::tempdir().unwrap();

        for relative_manifest in [
            "module.json",
            "modules/qc/module.json",
            "modules/alignment/module.json",
        ] {
            let src = showcase.join(relative_manifest);
            let dst = dir.path().join(relative_manifest);
            // SAFETY: `relative_manifest` always has a parent component
            // (`modules/qc` and friends, or the workspace root itself).
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

        assert_eq!(metadata.root().name(), "genomics-showcase");
        assert_eq!(metadata.root().version().to_string(), "1.0.0");
        assert_eq!(
            metadata
                .module_for_document(Path::new("modules/qc/qc.wdl"))
                .unwrap()
                .name(),
            "qc"
        );
    }

    #[test]
    fn collapses_local_dependency_entrypoint_path() {
        let dir = module_workspace();
        let metadata = WorkspaceMetadata::load(dir.path()).unwrap().unwrap();

        assert_eq!(
            metadata.documentation_path(Path::new("modules/qc/qc.wdl")),
            PathBuf::from("modules/qc")
        );
        assert_eq!(
            metadata.documentation_path(Path::new("main.wdl")),
            PathBuf::from("main")
        );
    }

    #[test]
    fn errors_for_missing_local_dependency() {
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

        assert!(WorkspaceMetadata::load(dir.path()).is_err());
    }

    #[test]
    fn errors_for_malformed_dependency_manifest() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"{
                "name": "root",
                "version": "1.0.0",
                "license": "MIT",
                "dependencies": {
                    "broken": { "path": "broken" }
                }
            }"#,
        );
        write_manifest(&dir.path().join("broken"), "{");

        assert!(WorkspaceMetadata::load(dir.path()).is_err());
    }

    #[test]
    fn skips_local_dependency_outside_workspace() {
        let parent = tempfile::tempdir().unwrap();
        let workspace = parent.path().join("workspace");
        let dependency = parent.path().join("dependency");
        write_manifest(
            &workspace,
            r#"{
                "name": "root",
                "version": "1.0.0",
                "license": "MIT",
                "dependencies": {
                    "external": { "path": "../dependency" }
                }
            }"#,
        );
        write_manifest(
            &dependency,
            r#"{
                "name": "external",
                "version": "1.0.0",
                "license": "MIT"
            }"#,
        );

        let metadata = WorkspaceMetadata::load(&workspace).unwrap().unwrap();
        assert_eq!(metadata.modules.len(), 1);
    }
}
