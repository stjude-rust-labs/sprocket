//! The `container` item within the `requirements` block.

use rowan::ast::AstNode;
use wdl_grammar::WorkflowDescriptionLanguage;

use crate::v1::common::container::value;
use crate::v1::common::container::value::Value;
use crate::v1::RequirementsItem;
use crate::AstToken;

/// The key name for a container requirements item.
const CONTAINER_KEY: &str = "container";

/// The `container` item within a `requirements` block.
#[derive(Debug)]
pub struct Container(RequirementsItem);

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
        RequirementsItem::can_cast(kind)
    }

    fn cast(node: rowan::SyntaxNode<Self::Language>) -> Option<Self>
    where
        Self: Sized,
    {
        RequirementsItem::cast(node).and_then(|item| Container::try_from(item).ok())
    }

    fn syntax(&self) -> &rowan::SyntaxNode<Self::Language> {
        self.0.syntax()
    }
}

impl TryFrom<RequirementsItem> for Container {
    type Error = ();

    fn try_from(value: RequirementsItem) -> Result<Self, Self::Error> {
        if value.name().as_str() == CONTAINER_KEY {
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
    requirements {
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
            .requirements()
            .next()
            .expect("the 'requirements' block to exist")
            .items()
            .filter_map(|p| p.into_container());

        assert!(container.count() == 1);
    }

    #[test]
    fn missing_container_item() {
        let (document, diagnostics) = Document::parse(
            r#"version 1.2

task hello {
    requirements {
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
            .requirements()
            .next()
            .expect("the 'requirements' block to exist")
            .items()
            .filter_map(|p| p.into_container());

        assert_eq!(container.count(), 0);
    }

    #[test]
    fn docker_alias() {
        // NOTE: the `docker` key is only an alias of the `container` key within
        // `runtime` blocks—not `requirements` blocks.
        let (document, diagnostics) = Document::parse(
            r#"version 1.2

task hello {
    requirements {
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
            .requirements()
            .next()
            .expect("the 'requirements' block to exist")
            .items()
            .filter_map(|p| p.into_container());

        assert_eq!(container.count(), 0);
    }
}
