//! Filesystems.

use std::path::PathBuf;

use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;
use indexmap::IndexMap;
use wdl::lint::LintVisitor;

use crate::report::Reporter;

/// A filesystem error.
#[derive(Debug)]
pub enum Error {
    /// An invalid file name was provided.
    InvalidFileName(PathBuf),

    /// An input/output error.
    Io(std::io::Error),

    /// Attempted to parse an entry that does not exist in the [`Repository`].
    MissingEntry(String),

    /// The item located at a path was missing.
    MissingPath(PathBuf),

    /// The item located at a path was not a file.
    NonFile(PathBuf),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidFileName(path) => write!(f, "invalid file name: {}", path.display()),
            Error::Io(err) => write!(f, "i/o error: {}", err),
            Error::MissingPath(path) => write!(f, "missing path: {}", path.display()),
            Error::NonFile(path) => write!(f, "not a file: {}", path.display()),
            Error::MissingEntry(entry) => write!(f, "missing entry: {entry}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A repository of files and associated source code.
#[derive(Debug)]
pub struct Repository {
    /// The mapping of entries in the source code map to file handles.
    handles: IndexMap<String, usize>,

    /// The inner source code map.
    sources: SimpleFiles<String, String>,
}

impl Default for Repository {
    fn default() -> Self {
        Self {
            sources: SimpleFiles::new(),
            handles: Default::default(),
        }
    }
}

impl Repository {
    /// Creates a new [`Repository`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    ///
    /// use sprocket::file::Repository;
    ///
    /// let mut repository = Repository::try_new(vec![PathBuf::from(".")], vec![String::from("wdl")]);
    /// ```
    pub fn try_new(paths: Vec<PathBuf>, extensions: Vec<String>) -> Result<Self> {
        let mut repository = Self::default();

        for path in expand_paths(paths, extensions)? {
            repository.load(path)?;
        }

        Ok(repository)
    }

    /// Inserts a new entry into the [`Repository`].
    ///
    /// **Note:** typically, you won't want to do this directly except in
    /// special cases. Instead, prefer using the [`load()`](Repository::load())
    /// method.
    ///
    /// # Examples
    ///
    /// ```
    /// use sprocket::file::Repository;
    ///
    /// let mut repository = Repository::default();
    /// repository.insert("foo.txt", "bar");
    /// ```
    pub fn insert(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        let path = path.into().to_string_lossy().to_string();
        let content = content.into();

        let handle = self.sources.add(path.clone(), content);
        self.handles.insert(path, handle);
    }

    /// Attempts to load a new file and its contents into the [`Repository`].
    ///
    /// An error is thrown if any issues are encountered when reading the file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use sprocket::file::Repository;
    ///
    /// let mut repository = Repository::default();
    /// repository.load("test.wdl")?;
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = path.into();

        if !path.exists() {
            return Err(Error::MissingPath(path));
        }

        if !path.is_file() {
            return Err(Error::NonFile(path));
        }

        let content = std::fs::read_to_string(&path).map_err(Error::Io)?;
        self.insert(path, content);

        Ok(())
    }

    /// Reports all diagnostics for all documents in the [`Repository`].
    pub fn report_diagnostics(
        &self,
        config: Config,
        writer: StandardStream,
        lint: bool,
        except_rules: Vec<String>,
    ) -> anyhow::Result<(bool, bool)> {
        let mut reporter = Reporter::new(config, writer);
        let mut syntax_failure = false;
        let mut lint_failure = false;

        for (_path, handle) in self.handles.iter() {
            let file = self.sources.get(*handle).expect("Expected to find file");
            match wdl::ast::Document::parse(file.source()).into_result() {
                Ok(document) => {
                    let validator = wdl::ast::Validator::default();
                    if let Err(diagnostics) = validator.validate(&document) {
                        reporter.emit_diagnostics(file, &diagnostics)?;
                        syntax_failure = true;
                        continue;
                    }

                    if lint {
                        let mut linter = wdl::ast::Validator::empty();
                        let visitor = LintVisitor::new(
                            wdl::lint::rules()
                                .into_iter()
                                .filter_map(|rule| {
                                    if except_rules.contains(&rule.id().to_string()) {
                                        None
                                    } else {
                                        Some(rule)
                                    }
                                })
                            );
                        linter.add_visitor(visitor);
                        if let Err(diagnostics) = linter.validate(&document) {
                            reporter.emit_diagnostics(file, &diagnostics)?;
                            lint_failure = true;
                        }
                    }
                }
                Err(diagnostics) => {
                    reporter.emit_diagnostics(file, &diagnostics)?;
                    syntax_failure = true;
                }
            }
        }

        Ok((syntax_failure, lint_failure))
    }
}

/// Expands a set of [`PathBuf`]s.
///
/// This means that, for each [`PathBuf`],
///
/// * if the path exists and is a file, the file is added to the result.
/// * if the path exists and is a directory, all files underneath that directory
///   (including recursively traversed directories) that have an extension in
///   the `extensions` list are added to the result.
/// * if the path does not exist, an error is thrown.
pub fn expand_paths(paths: Vec<PathBuf>, extensions: Vec<String>) -> Result<Vec<PathBuf>> {
    paths.into_iter().try_fold(Vec::new(), |mut acc, path| {
        if !path.exists() {
            return Err(Error::MissingPath(path));
        }

        if path.is_file() {
            acc.push(path);
        } else if path.is_dir() {
            let dir_files = walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(std::result::Result::ok)
                .filter(|entry| {
                    extensions
                        .iter()
                        .any(|ext| entry.path().extension() == Some(ext.as_ref()))
                })
                .map(|entry| entry.path().to_path_buf());
            acc.extend(dir_files)
        }

        acc.sort();
        Ok(acc)
    })
}
