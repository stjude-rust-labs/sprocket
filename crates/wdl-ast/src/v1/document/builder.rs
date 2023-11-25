//! A builder for a [`Document`].

use indexmap::IndexSet;

use crate::v1::document::task::Task;
use crate::v1::document::Document;
use crate::v1::document::Import;
use crate::v1::document::Struct;
use crate::v1::document::Version;
use crate::v1::document::Workflow;

/// An error that occurs when a required field is missing at build time.
#[derive(Debug)]
pub enum MissingError {
    /// A version was not provided to the [`Builder`].
    Version,
}

impl std::fmt::Display for MissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingError::Version => write!(f, "version"),
        }
    }
}

impl std::error::Error for MissingError {}

/// An error that occurs when a multiple values were provded for a field that
/// only accepts a single value.
#[derive(Debug)]
pub enum MultipleError {
    /// Attempted to set multiple values for the version field within the
    /// [`Builder`].
    Version,

    /// Attempted to set multiple values for the workflow field within the
    /// [`Builder`].
    Workflow,
}

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipleError::Version => write!(f, "version"),
            MultipleError::Workflow => write!(f, "workflow"),
        }
    }
}

impl std::error::Error for MultipleError {}

/// An error related to a [`Builder`].
#[derive(Debug)]
pub enum Error {
    /// A required field was missing at build time.
    Missing(MissingError),

    /// Multiple values were provided for a field that accepts a single value.
    Multiple(MultipleError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Missing(err) => write!(f, "missing value for field: {err}"),
            Error::Multiple(err) => {
                write!(f, "multiple values provided for single value field: {err}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A builder for a [`Document`].
#[derive(Default, Debug)]
pub struct Builder {
    /// Document imports.
    imports: Vec<Import>,

    /// Document structs.
    structs: Vec<Struct>,

    /// Document tasks.
    tasks: IndexSet<Task>,

    /// Document workflow.
    workflow: Option<Workflow>,

    /// Document version.
    version: Option<Version>,
}

impl Builder {
    /// Pushes an [`Import`] into the document [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document;
    /// use ast::v1::document::import::Builder;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let import = Builder::default()
    ///     .uri(String::from("../mapping.wdl"))?
    ///     .try_build()?;
    ///
    /// let document = document::Builder::default()
    ///     .version(document::Version::OneDotOne)?
    ///     .push_import(import)
    ///     .try_build()?;
    ///
    /// let import = document.imports().into_iter().next().unwrap();
    /// assert_eq!(import.uri(), "../mapping.wdl");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn push_import(mut self, import: Import) -> Self {
        self.imports.push(import);
        self
    }

    /// Pushes a [`Struct`] into the document [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::r#struct;
    /// use ast::v1::document::r#struct::Builder;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let r#struct = Builder::default()
    ///     .name(Identifier::try_from("a_struct").unwrap())?
    ///     .try_build()?;
    ///
    /// let document = document::Builder::default()
    ///     .version(document::Version::OneDotOne)?
    ///     .push_struct(r#struct)
    ///     .try_build()?;
    ///
    /// let r#struct = document.structs().first().unwrap();
    /// assert_eq!(r#struct.name().as_str(), "a_struct");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn push_struct(mut self, r#struct: Struct) -> Self {
        self.structs.push(r#struct);
        self
    }

    /// Inserts a task into the document [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::task::command::Contents;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let name = Identifier::try_from(String::from("name"))?;
    /// let contents = "echo 'Hello, world!'".parse::<Contents>().unwrap();
    /// let command = document::task::Command::HereDoc(contents);
    /// let task = document::task::Builder::default()
    ///     .name(name.clone())?
    ///     .command(command)?
    ///     .try_build()?;
    ///
    /// let document = document::Builder::default()
    ///     .version(document::Version::OneDotOne)?
    ///     .insert_task(task)
    ///     .try_build()?;
    /// assert_eq!(document.tasks().len(), 1);
    ///
    /// let task = document.tasks().first().unwrap();
    /// assert_eq!(task.name().as_str(), "name");
    /// assert_eq!(task.command().to_string(), "echo 'Hello, world!'");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn insert_task(mut self, task: Task) -> Self {
        self.tasks.insert(task);
        self
    }

    /// Adds a document version to the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let document = document::Builder::default()
    ///     .version(document::Version::OneDotOne)?
    ///     .try_build()?;
    ///
    /// assert_eq!(document.version(), &document::Version::OneDotOne);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn version(mut self, version: Version) -> Result<Self> {
        if self.version.is_some() {
            return Err(Error::Multiple(MultipleError::Version));
        }

        self.version = Some(version);
        Ok(self)
    }

    /// Adds a workflow to the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::workflow;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let workflow = workflow::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .try_build()?;
    ///
    /// let document = document::Builder::default()
    ///     .version(document::Version::OneDotOne)?
    ///     .workflow(workflow.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(document.workflow(), Some(&workflow));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn workflow(mut self, workflow: Workflow) -> Result<Self> {
        if self.workflow.is_some() {
            return Err(Error::Multiple(MultipleError::Workflow));
        }

        self.workflow = Some(workflow);
        Ok(self)
    }

    /// Consumes `self` to attempt to build a [`Document`] from the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// use ast::v1::document;
    ///
    /// let document = document::Builder::default()
    ///     .version(document::Version::OneDotOne)?
    ///     .try_build()?;
    ///
    /// assert_eq!(document.version(), &document::Version::OneDotOne);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn try_build(self) -> Result<Document> {
        let version = self
            .version
            .map(Ok)
            .unwrap_or(Err(Error::Missing(MissingError::Version)))?;

        Ok(Document {
            imports: self.imports,
            structs: self.structs,
            tasks: self.tasks,
            workflow: self.workflow,
            version,
        })
    }
}
