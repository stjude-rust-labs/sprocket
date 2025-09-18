//! The `container` item within the `requirements` block.

use wdl_grammar::SyntaxKind;
use wdl_grammar::SyntaxNode;

use crate::AstNode;
use crate::AstToken;
use crate::TreeNode;
use crate::v1::RequirementsItem;
use crate::v1::TASK_REQUIREMENT_CONTAINER;
use crate::v1::common::container::value;
use crate::v1::common::container::value::Value;

/// The `container` item within a `requirements` block.
#[derive(Debug)]
pub struct Container<N: TreeNode = SyntaxNode>(RequirementsItem<N>);

impl<N: TreeNode> Container<N> {
    /// Gets the [`Value`] from a [`Container`] (if it can be parsed).
    pub fn value(&self) -> value::Result<Value<N>> {
        Value::try_from(self.0.expr())
    }
}

impl<N: TreeNode> AstNode<N> for Container<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        RequirementsItem::<N>::can_cast(kind)
    }

    fn cast(inner: N) -> Option<Self> {
        RequirementsItem::cast(inner).and_then(|item| Container::try_from(item).ok())
    }

    fn inner(&self) -> &N {
        self.0.inner()
    }
}

impl<N: TreeNode> TryFrom<RequirementsItem<N>> for Container<N> {
    type Error = ();

    fn try_from(value: RequirementsItem<N>) -> Result<Self, Self::Error> {
        if value.name().text() == TASK_REQUIREMENT_CONTAINER {
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

        let section = document
            .ast()
            .as_v1()
            .expect("v1 ast")
            .tasks()
            .next()
            .expect("the 'hello' task to exist")
            .requirements()
            .expect("the 'requirements' block to exist");

        let container = section.items().filter_map(|p| p.into_container());

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

        let section = document
            .ast()
            .as_v1()
            .expect("v1 ast")
            .tasks()
            .next()
            .expect("the 'hello' task to exist")
            .requirements()
            .expect("the 'requirements' block to exist");

        let container = section.items().filter_map(|p| p.into_container());

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

        let section = document
            .ast()
            .as_v1()
            .expect("v1 ast")
            .tasks()
            .next()
            .expect("the 'hello' task to exist")
            .requirements()
            .expect("the 'requirements' block to exist");

        let container = section.items().filter_map(|p| p.into_container());

        assert_eq!(container.count(), 0);
    }
}
