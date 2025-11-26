//! Filesystem operations for provenance tracking in v1.

pub mod index;

use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;

pub use index::create_index_entries;
pub use index::rebuild_index;

/// Subdirectory name for workflow execution runs.
const RUNS_DIR: &str = "runs";

/// Subdirectory name for the provenance index.
const INDEX_DIR: &str = "index";

/// Input file name.
const INPUTS_FILE: &str = "inputs.json";

/// Output file name.
const OUTPUTS_FILE: &str = "outputs.json";

/// Root directory for all workflow outputs and indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDirectory(PathBuf);

impl OutputDirectory {
    /// Create a new output directory.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self(root.as_ref().to_path_buf())
    }

    /// Get the workflow execution directory for a given workflow name.
    pub fn workflow_run(&self, workflow_name: impl Into<PathBuf>) -> RunDirectory {
        RunDirectory::new(self.clone(), workflow_name)
    }

    /// Constructs a workflow directory and then ensure that it exists.
    pub fn ensure_workflow_run(
        &self,
        workflow_name: impl Into<PathBuf>,
    ) -> std::io::Result<RunDirectory> {
        let dir = self.workflow_run(workflow_name);
        std::fs::create_dir_all(dir.root())?;
        Ok(dir)
    }

    /// Get the index directory for a given index path.
    pub fn index_dir(&self, index_path: impl Into<PathBuf>) -> PathBuf {
        self.0.join(INDEX_DIR).join(index_path.into())
    }

    /// Get the index directory and ensure it exists.
    pub fn ensure_index_dir(&self, index_path: impl Into<PathBuf>) -> std::io::Result<PathBuf> {
        let path = self.index_dir(index_path);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// Get the root directory.
    pub fn root(&self) -> &Path {
        &self.0
    }

    /// Convert an absolute path to a relative path within the output directory.
    ///
    /// Returns `Some` with a path starting with `./` if the path is within the
    /// output directory, or `None` if the path is not within the output
    /// directory.
    pub fn make_relative_to(&self, path: impl AsRef<Path>) -> Option<String> {
        let path = path.as_ref();
        path.strip_prefix(&self.0)
            .ok()
            .map(|p| format!("./{}", p.display()))
    }
}

/// Run execution directory.
///
/// The first item in the tuple is a the output directory this run is contained
/// within.
///
/// The second item in the tuple is the path to the run directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDirectory(OutputDirectory, PathBuf);

impl RunDirectory {
    /// Creates a new run directory.
    pub fn new(output_dir: OutputDirectory, name: impl Into<PathBuf>) -> Self {
        let path = PathBuf::from(output_dir.root())
            .join(RUNS_DIR)
            .join(name.into());
        Self(output_dir, path)
    }

    /// Gets a reference to the output directory.
    pub fn output_directory(&self) -> &OutputDirectory {
        &self.0
    }

    /// Gets the relative path to the run directory within the output directory
    /// (e.g., `runs/workflow-name`).
    pub fn relative_path(&self) -> &Path {
        // SAFETY: because of the way `RunDirectory`s are created, we know that
        // the inner path is prefixed by the output directory.
        self.1.strip_prefix(self.0.root()).unwrap()
    }

    /// Returns the path to the run execution directory.
    pub fn root(&self) -> &Path {
        &self.1
    }

    /// Returns the path to the inputs file.
    pub fn inputs_file(&self) -> PathBuf {
        self.root().join(INPUTS_FILE)
    }

    /// Returns the path to the outputs file.
    pub fn outputs_file(&self) -> PathBuf {
        self.root().join(OUTPUTS_FILE)
    }
}

impl Deref for RunDirectory {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_relative_to_within_output_dir() {
        let output_dir = OutputDirectory::new("/tmp/output");

        // Test path within output directory
        let path = Path::new("/tmp/output/runs/workflow-123");
        assert_eq!(
            output_dir.make_relative_to(path),
            Some("./runs/workflow-123".to_string())
        );

        // Test path at root of output directory
        let path = Path::new("/tmp/output");
        assert_eq!(output_dir.make_relative_to(path), Some("./".to_string()));

        // Test nested path
        let path = Path::new("/tmp/output/index/my-workflow/output.txt");
        assert_eq!(
            output_dir.make_relative_to(path),
            Some("./index/my-workflow/output.txt".to_string())
        );
    }

    #[test]
    fn make_relative_to_outside_output_dir() {
        let output_dir = OutputDirectory::new("/tmp/output");

        // Test path outside output directory
        let path = Path::new("/tmp/other/workflow");
        assert_eq!(output_dir.make_relative_to(path), None);

        // Test path at sibling directory
        let path = Path::new("/tmp/workflows/run");
        assert_eq!(output_dir.make_relative_to(path), None);
    }

    #[test]
    fn ensure_workflow_run_creates_directory() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        let workflow_name = "my-workflow-123";
        let run_path = output_dir.ensure_workflow_run(workflow_name).unwrap();

        // Should create `runs/my-workflow-123`
        assert!(run_path.exists());
        assert!(run_path.is_dir());
        assert_eq!(
            run_path.root(),
            temp.path().join("runs").join(workflow_name)
        );
    }

    #[test]
    fn ensure_index_dir_creates_nested_path() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        let nested_index = "project/sample/results";
        let index_path = output_dir.ensure_index_dir(nested_index).unwrap();

        // Should create `index/project/sample/results`
        assert!(index_path.exists());
        assert!(index_path.is_dir());
        assert_eq!(index_path, temp.path().join("index").join(nested_index));
    }

    #[test]
    fn ensure_operations_are_idempotent() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        // Call `ensure_workflow_run` twice
        let path1 = output_dir.ensure_workflow_run("workflow-1").unwrap();
        let path2 = output_dir.ensure_workflow_run("workflow-1").unwrap();
        assert_eq!(path1, path2);

        // Call `ensure_index_dir` twice
        let path3 = output_dir.ensure_index_dir("index-1").unwrap();
        let path4 = output_dir.ensure_index_dir("index-1").unwrap();
        assert_eq!(path3, path4);
    }

    #[test]
    fn workflow_run_and_index_dir_with_special_characters() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        // Unicode emoji in workflow name
        let emoji_workflow = "🚀-workflow";
        let emoji_path = output_dir.ensure_workflow_run(emoji_workflow).unwrap();
        assert!(emoji_path.exists());

        // Spaces in index name
        let spaces_index = "my index path";
        let spaces_path = output_dir.ensure_index_dir(spaces_index).unwrap();
        assert!(spaces_path.exists());
    }

    #[test]
    fn make_relative_to_with_symlinks() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        // Create a real directory inside output dir
        let real_dir = temp.path().join("real");
        std::fs::create_dir(&real_dir).unwrap();

        // Create a symlink to it
        let symlink = temp.path().join("link");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_dir, &symlink).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real_dir, &symlink).unwrap();

        // Both should work
        assert_eq!(
            output_dir.make_relative_to(&real_dir),
            Some("./real".to_string())
        );
        assert_eq!(
            output_dir.make_relative_to(&symlink),
            Some("./link".to_string())
        );
    }
}