//! Filesystems.

use std::collections::HashMap;
use std::path::PathBuf;

use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;

use crate::report::Reporter;

/// A filesystem error.
#[derive(Debug)]
pub enum Error {
    /// A WDL 1.x abstract syntax tree error.
    AstV1(wdl::ast::v1::Error),

    /// A WDL 1.x grammar error.
    GrammarV1(wdl::grammar::v1::Error),

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
            Error::AstV1(err) => write!(f, "ast error: {err}"),
            Error::GrammarV1(err) => write!(f, "grammar error: {err}"),
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
    handles: HashMap<String, usize>,

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

    /// Attempts to parse an existing entry into a WDL v1.x abstract syntax
    /// tree.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use sprocket::file::Repository;
    ///
    /// let mut repository = Repository::default();
    /// repository.load("test.wdl")?;
    /// let ast = repository.parse("test.wdl")?;
    ///
    /// assert!(matches!(ast.tree(), Some(_)));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse(&self, entry: impl AsRef<str>) -> Result<wdl::ast::v1::Result> {
        let entry = entry.as_ref();
        let handle = *self
            .handles
            .get(entry)
            .ok_or(Error::MissingEntry(entry.to_owned()))?;

        let file = match self.sources.get(handle) {
            Ok(result) => result,
            // SAFETY: this entry will _always_ exist in the inner
            // [`SimpleFiles`], as we just ensured it existed in the mapping
            // between entry names and handles.
            Err(_) => unreachable!(),
        };

        let mut all_concerns = wdl::core::concern::concerns::Builder::default();

        let (pt, concerns) = wdl::grammar::v1::parse(file.source())
            .map_err(Error::GrammarV1)?
            .into_parts();

        if let Some(concerns) = concerns {
            for concern in concerns.into_inner() {
                all_concerns = all_concerns.push(concern);
            }
        }

        let pt = match pt {
            Some(pt) => pt,
            None => {
                // SAFETY: because `grammar::v1::parse` returns a
                // `grammar::v1::Result`, we know that either the concerns or the
                // parse tree must be [`Some`] (else, this would have failed at
                // `grammar::v1::Result` creation time). That said, we just checked
                // that `pt` is [`None`]. In this case, it must follow that the
                // concerns are not empty. As such, this will always unwrap.
                return Ok(wdl::ast::v1::Result::try_new(None, all_concerns.build()).unwrap());
            }
        };

        let (ast, concerns) = wdl::ast::v1::parse(pt).map_err(Error::AstV1)?.into_parts();

        if let Some(concerns) = concerns {
            for concern in concerns.into_inner() {
                all_concerns = all_concerns.push(concern);
            }
        }

        match ast {
            Some(ast) => {
                // SAFETY: the ast is [`Some`], so this will always unwrap.
                Ok(wdl::ast::v1::Result::try_new(Some(ast), all_concerns.build()).unwrap())
            }
            None => {
                // SAFETY: because `ast::v1::parse` returns a
                // `ast::v1::Result`, we know that either the concerns or the
                // parse tree must be [`Some`] (else, this would have failed at
                // `ast::v1::Result` creation time). That said, we just checked
                // that `ast` is [`None`]. In this case, it must follow that the
                // concerns are not empty. As such, this will always unwrap.
                Ok(wdl::ast::v1::Result::try_new(None, all_concerns.build()).unwrap())
            }
        }
    }

    /// Reports all concerns for all documents in the [`Repository`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use codespan_reporting::term::termcolor::ColorChoice;
    /// use codespan_reporting::term::termcolor::StandardStream;
    /// use codespan_reporting::term::Config;
    /// use sprocket::file::Repository;
    ///
    /// let mut repository = Repository::default();
    /// repository.load("test.wdl")?;
    ///
    /// let config = Config::default();
    /// let writer = StandardStream::stderr(ColorChoice::Always);
    /// repository.report_concerns(config, writer);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn report_concerns(&self, config: Config, writer: StandardStream) -> Result<()> {
        let mut reporter = Reporter::new(config, writer, &self.sources);

        for (file_name, handle) in self.handles.iter() {
            let document = self.parse(file_name)?;

            if let Some(concerns) = document.into_concerns() {
                for concern in concerns.into_inner() {
                    reporter.report_concern(concern, *handle);
                }
            }
        }

        Ok(())
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

        Ok(acc)
    })
}
