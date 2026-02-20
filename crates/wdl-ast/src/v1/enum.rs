//! V1 AST representation for enum definitions.

use std::fmt;
use std::fmt::Formatter;

use super::EnumKeyword;
use super::Expr;
use super::Type;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::TreeNode;

/// Represents an enum definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumDefinition<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> EnumDefinition<N> {
    /// Gets the name of the enum.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("enum should have a name")
    }

    /// Gets the `enum` keyword.
    pub fn keyword(&self) -> EnumKeyword<N::Token> {
        self.token().expect("enum should have a keyword")
    }

    /// Gets the optional type parameter of the enum definition.
    ///
    /// The type parameter specifies the common type that all choice values
    /// must coerce to. For example, `enum Status[String]` has a type parameter
    /// of `String`.
    ///
    /// Returns `None` if no explicit type parameter was specified. In this
    /// case, the type is inferred from the choice values, or defaults to
    /// `Union` if the enum has no values.
    pub fn type_parameter(&self) -> Option<EnumTypeParameter<N>> {
        self.children().next()
    }

    /// Gets the choices in the enum definition.
    pub fn variants(&self) -> impl Iterator<Item = EnumChoice<N>> + use<'_, N> {
        self.children()
    }

    /// Writes a Markdown formatted description of the enum.
    pub fn markdown_description(
        &self,
        f: &mut impl fmt::Write,
        computed_type: Option<&str>,
    ) -> fmt::Result {
        writeln!(f, "```wdl")?;
        write!(f, "{}", self.display(computed_type))?;
        write!(f, "```")?;

        Ok(())
    }

    /// Returns an object that implements [`Display`] for printing enums that
    /// may have a pre-computed type.
    ///
    /// The printed result will be stripped of any comments.
    ///
    /// For example:
    ///
    /// ```wdl
    /// ## An RGB24 color enum
    /// enum Color[String] {
    ///     ## Pure red
    ///     Red = "#FF0000",
    /// }
    /// ```
    ///
    /// Will produce:
    ///
    /// ```wdl
    /// enum Color[String] {
    ///     Red = "#FF0000",
    /// }
    /// ```
    pub fn display<'a>(&'a self, computed_type: Option<&'a str>) -> EnumDefinitionDisplay<'a, N> {
        EnumDefinitionDisplay {
            definition: self,
            computed_type,
        }
    }
}

/// Helper struct for printing [`EnumDefinition`]s.
#[derive(Debug)]
pub struct EnumDefinitionDisplay<'a, N: TreeNode> {
    /// The enum definition to print.
    definition: &'a EnumDefinition<N>,
    /// The computed type of the enum, if provided.
    computed_type: Option<&'a str>,
}

impl<N: TreeNode> fmt::Display for EnumDefinitionDisplay<'_, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "enum {}", self.definition.name().text())?;

        if let Some(ty_param) = self.definition.type_parameter() {
            write!(f, "[{}]", ty_param.ty().inner().text())?;
        } else if let Some(computed_ty) = self.computed_type {
            write!(f, "[{}]", computed_ty)?;
        }

        writeln!(f, " {{")?;

        for choice in self.definition.variants() {
            write!(f, "  {}", choice.name().text())?;
            if let Some(value) = choice.value() {
                write!(f, " = {}", value.inner().text())?;
            }
            writeln!(f, ",")?;
        }

        writeln!(f, "}}")?;
        Ok(())
    }
}

impl<N: TreeNode> AstNode<N> for EnumDefinition<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::EnumDefinitionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::EnumDefinitionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an enum type parameter (e.g., [String]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumTypeParameter<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> EnumTypeParameter<N> {
    /// Gets the inner type.
    pub fn ty(&self) -> Type<N> {
        self.0
            .children()
            .find_map(Type::cast)
            .expect("type parameter should have a type")
    }
}

impl<N: TreeNode> AstNode<N> for EnumTypeParameter<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::EnumTypeParameterNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::EnumTypeParameterNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an enum choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumChoice<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> EnumChoice<N> {
    /// Gets the choice name.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("choice should have a name")
    }

    /// Gets the optional value expression.
    pub fn value(&self) -> Option<Expr<N>> {
        self.children().next()
    }
}

impl<N: TreeNode> AstNode<N> for EnumChoice<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::EnumChoiceNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::EnumChoiceNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Ast;
    use crate::Document;

    #[test]
    fn enum_definitions() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.3

enum Empty {}

enum Color {
    Red,
    Green,
    Blue
}

enum Status[String] {
    Pending,
    Running,
    Complete
}

enum Priority[Int] {
    Low = 1,
    Medium = 2,
    High = 3
}

enum Mixed[Int] {
    First = 1,
    Second,
    Third = 3
}

workflow test {}
"#,
        );
        assert!(diagnostics.is_empty());

        match document.ast() {
            Ast::V1(ast) => {
                let enums: Vec<_> = ast.enums().collect();
                assert_eq!(enums.len(), 5);

                // Empty enum
                let empty = &enums[0];
                assert_eq!(empty.name().text(), "Empty");
                assert!(empty.type_parameter().is_none());
                assert_eq!(empty.variants().count(), 0);

                // Basic enum without type parameter
                let color = &enums[1];
                assert_eq!(color.name().text(), "Color");
                assert!(color.type_parameter().is_none());
                let choices: Vec<_> = color.variants().collect();
                assert_eq!(choices.len(), 3);
                assert_eq!(choices[0].name().text(), "Red");
                assert_eq!(choices[1].name().text(), "Green");
                assert_eq!(choices[2].name().text(), "Blue");
                for choice in &choices {
                    assert!(choice.value().is_none());
                }

                // Enum with String type parameter
                let status = &enums[2];
                assert_eq!(status.name().text(), "Status");
                let type_param = status.type_parameter().expect("should have type parameter");
                assert_eq!(type_param.ty().inner().text(), "String");
                assert_eq!(status.variants().count(), 3);

                // Enum with Int type parameter and values
                let priority = &enums[3];
                assert_eq!(priority.name().text(), "Priority");
                let type_param = priority
                    .type_parameter()
                    .expect("should have type parameter");
                assert_eq!(type_param.ty().inner().text(), "Int");
                let choices: Vec<_> = priority.variants().collect();
                assert_eq!(choices.len(), 3);
                for choice in &choices {
                    assert!(choice.value().is_some());
                }

                // Enum with mixed values (some with, some without)
                let mixed = &enums[4];
                assert_eq!(mixed.name().text(), "Mixed");
                let choices: Vec<_> = mixed.variants().collect();
                assert_eq!(choices.len(), 3);
                assert!(choices[0].value().is_some());
                assert!(choices[1].value().is_none());
                assert!(choices[2].value().is_some());
            }
            _ => panic!("expected V1 AST"),
        }
    }
}
