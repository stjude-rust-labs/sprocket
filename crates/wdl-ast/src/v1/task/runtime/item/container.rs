//! The `container` item within the `runtime` block.

use rowan::ast::AstNode;
use wdl_grammar::WorkflowDescriptionLanguage;

use crate::AstToken;
use crate::v1::RuntimeItem;
use crate::v1::common::container::value;
use crate::v1::common::container::value::Value;

/// The key name for a container runtime item.
const CONTAINER_KEYS: &[&str] = &["container", "docker"];

/// The `container` item within a `runtime` block.
#[derive(Debug)]
pub struct Container(RuntimeItem);

impl Container {
    /// Gets the [`Value`] from a [`Container`] (if it can be parsed).
    pub fn value(&self) -> value::Result<Value> {
        Value::try_from(self.0.expr())
    }
}

impl AstNode for Container {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool
    where
        Self: Sized,
    {
        RuntimeItem::can_cast(kind)
    }

    fn cast(node: rowan::SyntaxNode<Self::Language>) -> Option<Self>
    where
        Self: Sized,
    {
        RuntimeItem::cast(node).and_then(|item| Container::try_from(item).ok())
    }

    fn syntax(&self) -> &rowan::SyntaxNode<Self::Language> {
        self.0.syntax()
    }
}

impl TryFrom<RuntimeItem> for Container {
    type Error = ();

    fn try_from(value: RuntimeItem) -> Result<Self, Self::Error> {
        if CONTAINER_KEYS
            .iter()
            .any(|key| value.name().as_str() == *key)
        {
            return Ok(Self(value));
        }

        Err(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Document;

    #[test]
    fn simple_example() {
        let (document, diagnostics) = Document::parse(
            r#"version 1.2

task hello {
    runtime {
        container: "ubuntu"
    }
}"#,
        );

        assert!(diagnostics.is_empty());

        let container = document
            .ast()
            .as_v1()
            .expect("v1 ast")
            .tasks()
            .next()
            .expect("the 'hello' task to exist")
            .runtime()
            .expect("the 'runtime' block to exist")
            .items()
            .filter_map(|p| p.into_container());

        assert!(container.count() == 1);
    }

    #[test]
    fn missing_container_item() {
        let (document, diagnostics) = Document::parse(
            r#"version 1.2

task hello {
    runtime {
        foo: "ubuntu"
    }
}"#,
        );

        assert!(diagnostics.is_empty());

        let container = document
            .ast()
            .as_v1()
            .expect("v1 ast")
            .tasks()
            .next()
            .expect("the 'hello' task to exist")
            .runtime()
            .expect("the 'runtime' block to exist")
            .items()
            .filter_map(|p| p.into_container());

        assert!(container.count() == 0);
    }

    #[test]
    fn docker_alias() {
        let (document, diagnostics) = Document::parse(
            r#"version 1.2

task hello {
    runtime {
        docker: "ubuntu"
    }
}"#,
        );

        assert!(diagnostics.is_empty());

        let container = document
            .ast()
            .as_v1()
            .expect("v1 ast")
            .tasks()
            .next()
            .expect("the 'hello' task to exist")
            .runtime()
            .expect("the 'runtime' block to exist")
            .items()
            .filter_map(|p| p.into_container());

        assert!(container.count() == 1);
    }
}
