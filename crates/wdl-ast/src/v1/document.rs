//! Documents.

use grammar::v1::Rule;
use indexmap::IndexSet;
use pest::iterators::Pair;
use wdl_grammar as grammar;

mod builder;
pub mod declaration;
pub mod expression;
pub mod identifier;
pub mod import;
pub mod input;
pub mod metadata;
pub mod output;
pub mod private_declarations;
pub mod r#struct;
pub mod task;
mod version;
pub mod workflow;

pub use builder::Builder;
pub use declaration::Declaration;
pub use expression::Expression;
pub use identifier::Identifier;
pub use import::Import;
pub use input::Input;
pub use metadata::Metadata;
pub use output::Output;
pub use private_declarations::PrivateDeclarations;
pub use r#struct::Struct;
pub use task::Task;
pub use version::Version;
use wdl_macros::check_node;
pub use workflow::Workflow;

/// An error related to a [`Document`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

    /// An import error.
    Import(import::Error),

    /// A struct error.
    Struct(r#struct::Error),

    /// A task error.
    Task(task::Error),

    /// A version error.
    Version(version::Error),

    /// A workflow error.
    Workflow(workflow::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Import(err) => write!(f, "import error: {err}"),
            Error::Struct(err) => write!(f, "struct error: {err}"),
            Error::Task(err) => write!(f, "task error: {err}"),
            Error::Version(err) => write!(f, "version error: {err}"),
            Error::Workflow(err) => write!(f, "workflow error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A document.
#[derive(Clone, Debug)]
pub struct Document {
    /// Document version.
    version: Version,

    /// Document imports.
    imports: Vec<Import>,

    /// Document structs.
    structs: Vec<Struct>,

    /// Document tasks.
    tasks: IndexSet<Task>,

    /// Document workflow.
    workflow: Option<Workflow>,
}

impl Document {
    /// Gets the imports for this [`Document`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use grammar::v1::Rule;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let pt = grammar::v1::parse(
    ///     r#"version 1.1
    /// import "../hello.wdl" as hello alias foo as bar"#,
    /// )
    /// .unwrap()
    /// .into_tree()
    /// .unwrap();
    /// let ast = ast::v1::parse(pt).unwrap();
    ///
    /// let tree = ast.into_tree().unwrap();
    /// assert_eq!(tree.imports().len(), 1);
    ///
    /// let import = tree.imports().first().unwrap();
    /// assert_eq!(import.uri(), "../hello.wdl");
    /// assert_eq!(import.r#as().unwrap().as_str(), "hello");
    /// assert_eq!(
    ///     import.aliases().unwrap().get("foo"),
    ///     Some(&Identifier::try_from("bar").unwrap())
    /// );
    /// ```
    pub fn imports(&self) -> &[Import] {
        self.imports.as_ref()
    }

    /// Gets the structs for this [`Document`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use grammar::v1::Rule;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let pt = grammar::v1::parse(
    ///     r#"version 1.1
    /// struct Hello {
    ///     String world
    /// }"#,
    /// )
    /// .unwrap()
    /// .into_tree()
    /// .unwrap();
    ///
    /// let ast = ast::v1::parse(pt).unwrap();
    ///
    /// let tree = ast.tree().unwrap();
    /// assert_eq!(tree.structs().len(), 1);
    ///
    /// let r#struct = tree.structs().first().unwrap();
    /// assert_eq!(r#struct.name().as_str(), "Hello");
    ///
    /// let declaration = r#struct.declarations().unwrap().into_iter().next().unwrap();
    /// assert_eq!(declaration.name().as_str(), "world");
    /// assert_eq!(declaration.r#type().kind(), &Kind::String);
    /// assert_eq!(declaration.r#type().optional(), false);
    /// ```
    pub fn structs(&self) -> &[Struct] {
        self.structs.as_ref()
    }

    /// Gets the tasks for this [`Document`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use grammar::v1::Rule;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let pt = grammar::v1::parse(
    ///     "version 1.1
    /// task say_hello {
    ///     command <<<
    ///         echo 'Hello, world!'
    ///     >>>
    /// }",
    /// )
    /// .unwrap()
    /// .into_tree()
    /// .unwrap();
    /// let ast = ast::v1::parse(pt).unwrap();
    ///
    /// let tree = ast.tree().unwrap();
    /// assert_eq!(tree.tasks().len(), 1);
    ///
    /// let task = tree.tasks().first().unwrap();
    /// assert_eq!(task.name().as_str(), "say_hello");
    /// assert_eq!(task.command().to_string(), "echo 'Hello, world!'");
    /// ```
    pub fn tasks(&self) -> &IndexSet<Task> {
        &self.tasks
    }

    /// Gets the workflow for this [`Document`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::workflow::execution::Statement;
    /// use grammar::v1::Rule;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let pt = grammar::v1::parse(
    ///     r#"version 1.1
    /// workflow hello_world {
    ///     call test
    /// }"#,
    /// )
    /// .unwrap()
    /// .into_tree()
    /// .unwrap();
    /// let ast = ast::v1::parse(pt).unwrap();
    ///
    /// let tree = ast.tree().unwrap();
    /// assert!(tree.workflow().is_some());
    ///
    /// let workflow = tree.workflow().unwrap();
    /// assert_eq!(workflow.name().as_str(), "hello_world");
    ///
    /// let statements = workflow.statements().unwrap();
    /// assert_eq!(statements.len(), 1);
    ///
    /// let call = match statements.into_iter().next().unwrap() {
    ///     Statement::Call(call) => call,
    ///     _ => unreachable!(),
    /// };
    /// assert_eq!(call.name().to_string(), "test");
    /// ```
    pub fn workflow(&self) -> Option<&Workflow> {
        self.workflow.as_ref()
    }

    /// Gets the document version of this [`Document`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::Version;
    /// use grammar::v1::Rule;
    /// use wdl_ast as ast;
    /// use wdl_grammar as grammar;
    ///
    /// let pt = grammar::v1::parse("version 1.1")
    ///     .unwrap()
    ///     .into_tree()
    ///     .unwrap();
    /// let ast = ast::v1::parse(pt).unwrap().into_tree().unwrap();
    ///
    /// let mut version = ast.version();
    /// assert_eq!(version, &Version::OneDotOne);
    /// ```
    pub fn version(&self) -> &Version {
        &self.version
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Document {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        check_node!(node, document);
        let mut builder = builder::Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::version => {
                    let version = Version::try_from(node).map_err(Error::Version)?;
                    builder = builder.version(version).map_err(Error::Builder)?;
                }
                Rule::import => {
                    let import = Import::try_from(node).map_err(Error::Import)?;
                    builder = builder.push_import(import);
                }
                Rule::r#struct => {
                    let r#struct = Struct::try_from(node).map_err(Error::Struct)?;
                    builder = builder.push_struct(r#struct);
                }
                Rule::task => {
                    let task = Task::try_from(node).map_err(Error::Task)?;
                    builder = builder.insert_task(task);
                }
                Rule::workflow => {
                    let workflow = Workflow::try_from(node).map_err(Error::Workflow)?;
                    builder = builder.workflow(workflow).map_err(Error::Builder)?;
                }
                Rule::EOI => {}
                Rule::COMMENT => {}
                Rule::WHITESPACE => {}
                rule => unreachable!("workflow should not contain {:?}", rule),
            }
        }

        builder.try_build().map_err(Error::Builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let version = wdl_macros::test::valid_node!("version 1.1", version, Version);
        assert_eq!(version, Version::OneDotOne);
    }

    wdl_macros::test::create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        version,
        Version,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
