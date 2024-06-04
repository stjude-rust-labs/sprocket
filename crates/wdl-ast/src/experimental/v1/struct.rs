//! V1 AST representation for struct definitions.

use rowan::ast::support::children;
use rowan::ast::AstChildren;
use rowan::ast::AstNode;
use wdl_grammar::experimental::tree::SyntaxKind;
use wdl_grammar::experimental::tree::SyntaxNode;
use wdl_grammar::experimental::tree::WorkflowDescriptionLanguage;

use super::UnboundDecl;
use crate::experimental::token;
use crate::experimental::Ident;

/// Represents a struct definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructDefinition(pub(super) SyntaxNode);

impl StructDefinition {
    /// Gets the name of the struct.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("struct should have a name")
    }

    /// Gets the members of the struct.
    pub fn members(&self) -> AstChildren<UnboundDecl> {
        children(&self.0)
    }
}

impl AstNode for StructDefinition {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::StructDefinitionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::StructDefinitionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::experimental::v1::StructDefinition;
    use crate::experimental::v1::VisitReason;
    use crate::experimental::v1::Visitor;
    use crate::experimental::AstToken;
    use crate::experimental::Document;

    #[test]
    fn struct_definitions() {
        let parse = Document::parse(
            r#"
version 1.1

struct Empty {}

struct PrimitiveTypes {
    Boolean a
    Boolean? b
    Int c
    Int? d
    Float e
    Float? f
    String g
    String? h
    File i
    File? j
}

struct ComplexTypes {
    Map[Boolean, String] a
    Map[Int?, Array[String]]? b
    Array[Boolean] c
    Array[Array[Float]] d
    Pair[Boolean, Boolean] e
    Pair[Array[String], Array[String?]] f
    Object g
    Object? h
    MyType i
    MyType? j
}            
"#,
        );
        let document = parse.into_result().expect("there should be no errors");
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let structs: Vec<_> = ast.structs().collect();
        assert_eq!(structs.len(), 3);

        // First struct definition
        assert_eq!(structs[0].name().as_str(), "Empty");
        assert_eq!(structs[0].members().count(), 0);

        // Second struct definition
        assert_eq!(structs[1].name().as_str(), "PrimitiveTypes");
        let members: Vec<_> = structs[1].members().collect();
        assert_eq!(members.len(), 10);

        // First member
        assert_eq!(members[0].name().as_str(), "a");
        assert_eq!(members[0].ty().to_string(), "Boolean");
        assert!(!members[0].ty().is_optional());

        // Second member
        assert_eq!(members[1].name().as_str(), "b");
        assert_eq!(members[1].ty().to_string(), "Boolean?");
        assert!(members[1].ty().is_optional());

        // Third member
        assert_eq!(members[2].name().as_str(), "c");
        assert_eq!(members[2].ty().to_string(), "Int");
        assert!(!members[2].ty().is_optional());

        // Fourth member
        assert_eq!(members[3].name().as_str(), "d");
        assert_eq!(members[3].ty().to_string(), "Int?");
        assert!(members[3].ty().is_optional());

        // Fifth member
        assert_eq!(members[4].name().as_str(), "e");
        assert_eq!(members[4].ty().to_string(), "Float");
        assert!(!members[4].ty().is_optional());

        // Sixth member
        assert_eq!(members[5].name().as_str(), "f");
        assert_eq!(members[5].ty().to_string(), "Float?");
        assert!(members[5].ty().is_optional());

        // Seventh member
        assert_eq!(members[6].name().as_str(), "g");
        assert_eq!(members[6].ty().to_string(), "String");
        assert!(!members[6].ty().is_optional());

        // Eighth member
        assert_eq!(members[7].name().as_str(), "h");
        assert_eq!(members[7].ty().to_string(), "String?");
        assert!(members[7].ty().is_optional());

        // Ninth member
        assert_eq!(members[8].name().as_str(), "i");
        assert_eq!(members[8].ty().to_string(), "File");
        assert!(!members[8].ty().is_optional());

        // Tenth member
        assert_eq!(members[9].name().as_str(), "j");
        assert_eq!(members[9].ty().to_string(), "File?");
        assert!(members[9].ty().is_optional());

        // Third struct definition
        assert_eq!(structs[2].name().as_str(), "ComplexTypes");
        let members: Vec<_> = structs[2].members().collect();
        assert_eq!(members.len(), 10);

        // First member
        assert_eq!(members[0].name().as_str(), "a");
        assert_eq!(members[0].ty().to_string(), "Map[Boolean, String]");
        assert!(!members[0].ty().is_optional());

        // Second member
        assert_eq!(members[1].name().as_str(), "b");
        assert_eq!(members[1].ty().to_string(), "Map[Int?, Array[String]]?");
        assert!(members[1].ty().is_optional());

        // Third member
        assert_eq!(members[2].name().as_str(), "c");
        assert_eq!(members[2].ty().to_string(), "Array[Boolean]");
        assert!(!members[2].ty().is_optional());

        // Fourth member
        assert_eq!(members[3].name().as_str(), "d");
        assert_eq!(members[3].ty().to_string(), "Array[Array[Float]]");
        assert!(!members[3].ty().is_optional());

        // Fifth member
        assert_eq!(members[4].name().as_str(), "e");
        assert_eq!(members[4].ty().to_string(), "Pair[Boolean, Boolean]");
        assert!(!members[4].ty().is_optional());

        // Sixth member
        assert_eq!(members[5].name().as_str(), "f");
        assert_eq!(
            members[5].ty().to_string(),
            "Pair[Array[String], Array[String?]]"
        );
        assert!(!members[5].ty().is_optional());

        // Seventh member
        assert_eq!(members[6].name().as_str(), "g");
        assert_eq!(members[6].ty().to_string(), "Object");
        assert!(!members[6].ty().is_optional());

        // Eighth member
        assert_eq!(members[7].name().as_str(), "h");
        assert_eq!(members[7].ty().to_string(), "Object?");
        assert!(members[7].ty().is_optional());

        // Ninth member
        assert_eq!(members[8].name().as_str(), "i");
        assert_eq!(members[8].ty().to_string(), "MyType");
        assert!(!members[8].ty().is_optional());

        // Tenth member
        assert_eq!(members[9].name().as_str(), "j");
        assert_eq!(members[9].ty().to_string(), "MyType?");
        assert!(members[9].ty().is_optional());

        // Use a visitor to count the number of struct definitions in the tree
        struct MyVisitor(usize);

        impl Visitor for MyVisitor {
            type State = ();

            fn struct_definition(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &StructDefinition,
            ) {
                if reason == VisitReason::Enter {
                    self.0 += 1;
                }
            }
        }

        let mut visitor = MyVisitor(0);
        ast.visit(&mut (), &mut visitor);
        assert_eq!(visitor.0, 3);
    }
}
