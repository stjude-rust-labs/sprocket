//! Runtime section.

use std::collections::BTreeMap;

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::extract_one;
use wdl_macros::unwrap_one;

use crate::v1::document::expression;
use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;
use crate::v1::document::Expression;

mod builder;
pub mod value;

pub use builder::Builder;
pub use value::Value;

/// An error related to a [`Runtime`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

    /// An expression error.
    Expression(expression::Error),

    /// An identifier error.
    Identifier(singular::Error),

    /// An runtime value error.
    Value(value::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Expression(err) => write!(f, "expression error: {err}"),
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
            Error::Value(err) => write!(f, "runtime value error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A runtime section.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Runtime {
    /// The `container` field.
    container: Option<Value>,

    /// The `cpu` field.
    cpu: Option<Value>,

    /// The `memory` field.
    memory: Option<Value>,

    /// The `gpu` field.
    gpu: Option<Value>,

    /// The `disks` field.
    disks: Option<Value>,

    /// The `maxRetries` field.
    max_retries: Option<Value>,

    /// The `returnCodes` field.
    return_codes: Option<Value>,

    /// Other included runtime hints.
    hints: Option<BTreeMap<Identifier, Expression>>,
}

impl Runtime {
    /// Gets the `container` field for this [`Runtime`] by reference (if it
    /// exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let container = Value::try_from(Expression::Literal(Literal::String(String::from(
    ///     "ubuntu:latest",
    /// ))))?;
    /// let runtime = Builder::default()
    ///     .container(container.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(runtime.container(), Some(&container));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn container(&self) -> Option<&Value> {
        self.container.as_ref()
    }

    /// Gets the `cpu` field for this [`Runtime`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let cpu = Value::try_from(Expression::Literal(Literal::Integer(4)))?;
    /// let runtime = Builder::default().cpu(cpu.clone())?.try_build()?;
    ///
    /// assert_eq!(runtime.cpu(), Some(&cpu));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn cpu(&self) -> Option<&Value> {
        self.cpu.as_ref()
    }

    /// Gets the `memory` field for this [`Runtime`] by reference (if it
    /// exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let memory = Value::try_from(Expression::Literal(Literal::String(String::from("2 GiB"))))?;
    /// let runtime = Builder::default().memory(memory.clone())?.try_build()?;
    ///
    /// assert_eq!(runtime.memory(), Some(&memory));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn memory(&self) -> Option<&Value> {
        self.memory.as_ref()
    }

    /// Gets the `gpu` field for this [`Runtime`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let gpu = Value::try_from(Expression::Literal(Literal::Boolean(false)))?;
    /// let runtime = Builder::default().gpu(gpu.clone())?.try_build()?;
    ///
    /// assert_eq!(runtime.gpu(), Some(&gpu));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn gpu(&self) -> Option<&Value> {
        self.gpu.as_ref()
    }

    /// Gets the `disks` field for this [`Runtime`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let disks = Value::try_from(Expression::Literal(Literal::String(String::from("1 GiB"))))?;
    /// let runtime = Builder::default().disks(disks.clone())?.try_build()?;
    ///
    /// assert_eq!(runtime.disks(), Some(&disks));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn disks(&self) -> Option<&Value> {
        self.disks.as_ref()
    }

    /// Gets the `maxRetries` field for this [`Runtime`] by reference (if it
    /// exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let max_retries = Value::try_from(Expression::Literal(Literal::Integer(0)))?;
    /// let runtime = Builder::default()
    ///     .max_retries(max_retries.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(runtime.max_retries(), Some(&max_retries));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn max_retries(&self) -> Option<&Value> {
        self.max_retries.as_ref()
    }

    /// Gets the `returnCodes` field for this [`Runtime`] by reference (if it
    /// exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::task::runtime::Value;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let return_codes = Value::try_from(Expression::Literal(Literal::Integer(0)))?;
    /// let runtime = Builder::default()
    ///     .return_codes(return_codes.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(runtime.return_codes(), Some(&return_codes));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn return_codes(&self) -> Option<&Value> {
        self.return_codes.as_ref()
    }

    /// Gets the hints for this [`Runtime`] (if they exist).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::task::runtime::Builder;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let runtime = Builder::default()
    ///     .insert_hint(
    ///         Identifier::try_from("hello")?,
    ///         Expression::Literal(Literal::None),
    ///     )
    ///     .try_build()?;
    ///
    /// assert_eq!(
    ///     runtime.hints().unwrap().get("hello"),
    ///     Some(&Expression::Literal(Literal::None))
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn hints(&self) -> Option<&BTreeMap<Identifier, Expression>> {
        self.hints.as_ref()
    }
}

impl TryFrom<Pair<'_, Rule>> for Runtime {
    type Error = Error;

    fn try_from(node: Pair<'_, Rule>) -> Result<Self, Self::Error> {
        check_node!(node, task_runtime);
        let mut builder = Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::task_runtime_mapping => {
                    let key_node =
                        extract_one!(node.clone(), task_runtime_mapping_key, task_runtime_mapping)?;
                    let value_node =
                        extract_one!(node, task_runtime_mapping_value, task_runtime_mapping)?;

                    match key_node.as_str() {
                        "container" | "docker" => {
                            let container = Value::try_from(value_node).map_err(Error::Value)?;
                            builder = builder.container(container).map_err(Error::Builder)?
                        }
                        "cpu" => {
                            let cpu = Value::try_from(value_node).map_err(Error::Value)?;
                            builder = builder.cpu(cpu).map_err(Error::Builder)?;
                        }
                        "memory" => {
                            let memory = Value::try_from(value_node).map_err(Error::Value)?;
                            builder = builder.memory(memory).map_err(Error::Builder)?;
                        }
                        "gpu" => {
                            let gpu = Value::try_from(value_node).map_err(Error::Value)?;
                            builder = builder.gpu(gpu).map_err(Error::Builder)?;
                        }
                        "disks" => {
                            let disks = Value::try_from(value_node).map_err(Error::Value)?;
                            builder = builder.disks(disks).map_err(Error::Builder)?;
                        }
                        "maxRetries" => {
                            let max_retries = Value::try_from(value_node).map_err(Error::Value)?;
                            builder = builder.max_retries(max_retries).map_err(Error::Builder)?;
                        }
                        "returnCodes" => {
                            let return_codes = Value::try_from(value_node).map_err(Error::Value)?;
                            builder = builder.return_codes(return_codes).map_err(Error::Builder)?;
                        }
                        _ => {
                            let identifier_node = unwrap_one!(key_node, task_runtime_mapping_key);
                            let key =
                                Identifier::try_from(identifier_node).map_err(Error::Identifier)?;

                            let expression_node =
                                unwrap_one!(value_node, task_runtime_mapping_value);
                            let value =
                                Expression::try_from(expression_node).map_err(Error::Expression)?;

                            builder = builder.insert_hint(key, value);
                        }
                    }
                }
                Rule::WHITESPACE => {}
                Rule::COMMENT => {}
                rule => unreachable!("task runtime should not contain {:?}", rule),
            }
        }

        builder.try_build().map_err(Error::Builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::document::expression::Literal;
    use crate::v1::document::Expression;

    #[test]
    fn it_parses_a_runtime_successfully() -> Result<(), Box<dyn std::error::Error>> {
        let pt = wdl_grammar::v1::parse(
            r#"version 1.1

task foo {
    command <<<
        echo "Hello, world!"
    >>>

    runtime {
        container: "ubuntu:latest"
        cpu: 16
        memory: "2 GiB"
        gpu: true
        disks: "1 GiB"
        maxRetries: 3
        returnCodes: "*"
    }
}"#,
        )?
        .into_tree()
        .unwrap();

        let ast = crate::v1::parse(pt)?.into_tree().unwrap();
        assert_eq!(ast.tasks().len(), 1);

        let task = ast.tasks().iter().next().unwrap();
        assert!(task.runtime().is_some());

        let runtime = task.runtime().unwrap();
        assert_eq!(
            runtime.container().unwrap(),
            &Value::try_from(Expression::Literal(Literal::String(String::from(
                "ubuntu:latest"
            ))))
            .unwrap()
        );
        assert_eq!(
            runtime.cpu().unwrap(),
            &Value::try_from(Expression::Literal(Literal::Integer(16))).unwrap()
        );
        assert_eq!(
            runtime.memory().unwrap(),
            &Value::try_from(Expression::Literal(Literal::String(String::from("2 GiB")))).unwrap()
        );
        assert_eq!(
            runtime.gpu().unwrap(),
            &Value::try_from(Expression::Literal(Literal::Boolean(true))).unwrap()
        );
        assert_eq!(
            runtime.disks().unwrap(),
            &Value::try_from(Expression::Literal(Literal::String(String::from("1 GiB")))).unwrap()
        );
        assert_eq!(
            runtime.max_retries().unwrap(),
            &Value::try_from(Expression::Literal(Literal::Integer(3))).unwrap()
        );
        assert_eq!(
            runtime.return_codes().unwrap(),
            &Value::try_from(Expression::Literal(Literal::String(String::from("*")))).unwrap()
        );

        Ok(())
    }
}
