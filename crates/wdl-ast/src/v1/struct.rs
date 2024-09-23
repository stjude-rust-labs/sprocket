//! V1 AST representation for struct definitions.

use super::MetadataSection;
use super::ParameterMetadataSection;
use super::UnboundDecl;
use crate::AstChildren;
use crate::AstNode;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::WorkflowDescriptionLanguage;
use crate::support::children;
use crate::token;

/// Represents a struct definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructDefinition(pub(crate) SyntaxNode);

impl StructDefinition {
    /// Gets the name of the struct.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("struct should have a name")
    }

    /// Gets the items in the struct definition.
    pub fn items(&self) -> AstChildren<StructItem> {
        children(&self.0)
    }

    /// Gets the member declarations of the struct.
    pub fn members(&self) -> AstChildren<UnboundDecl> {
        children(&self.0)
    }

    /// Gets the metadata sections of the struct.
    pub fn metadata(&self) -> AstChildren<MetadataSection> {
        children(&self.0)
    }

    /// Gets the parameter metadata sections of the struct.
    pub fn parameter_metadata(&self) -> AstChildren<ParameterMetadataSection> {
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

/// Represents an item in a struct definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructItem {
    /// The item is a member declaration.
    Member(UnboundDecl),
    /// The item is a metadata section.
    Metadata(MetadataSection),
    /// The item is a parameter meta section.
    ParameterMetadata(ParameterMetadataSection),
}

impl AstNode for StructItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::UnboundDeclNode
                | SyntaxKind::MetadataSectionNode
                | SyntaxKind::ParameterMetadataSectionNode
        )
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::UnboundDeclNode => Some(Self::Member(UnboundDecl(syntax))),
            SyntaxKind::MetadataSectionNode => Some(Self::Metadata(MetadataSection(syntax))),
            SyntaxKind::ParameterMetadataSectionNode => {
                Some(Self::ParameterMetadata(ParameterMetadataSection(syntax)))
            }
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Member(m) => &m.0,
            Self::Metadata(m) => &m.0,
            Self::ParameterMetadata(m) => &m.0,
        }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::AstToken;
    use crate::Document;
    use crate::SupportedVersion;
    use crate::VisitReason;
    use crate::Visitor;
    use crate::v1::StructDefinition;

    #[test]
    fn struct_definitions() {
        let (document, diagnostics) = Document::parse(
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
    Directory k
    Directory? l

    meta {
        ok: "good"
    }

    parameter_meta {
        a: "foo"
    }
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
    Array[Directory] k

    meta {
        ok: "good"
    }

    parameter_meta {
        a: "foo"
    }
}            
"#,
        );
        assert!(diagnostics.is_empty());
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
        assert_eq!(members.len(), 12);

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

        // Eleventh member
        assert_eq!(members[10].name().as_str(), "k");
        assert_eq!(members[10].ty().to_string(), "Directory");
        assert!(!members[10].ty().is_optional());

        // Twelfth member
        assert_eq!(members[11].name().as_str(), "l");
        assert_eq!(members[11].ty().to_string(), "Directory?");
        assert!(members[11].ty().is_optional());

        // Third struct definition
        assert_eq!(structs[2].name().as_str(), "ComplexTypes");
        let members: Vec<_> = structs[2].members().collect();
        assert_eq!(members.len(), 11);

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

        // Eleventh member
        assert_eq!(members[10].name().as_str(), "k");
        assert_eq!(members[10].ty().to_string(), "Array[Directory]");
        assert!(!members[10].ty().is_optional());

        // Use a visitor to count the number of struct definitions in the tree
        struct MyVisitor(usize);

        impl Visitor for MyVisitor {
            type State = ();

            fn document(
                &mut self,
                _: &mut Self::State,
                _: VisitReason,
                _: &Document,
                _: SupportedVersion,
            ) {
            }

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
        document.visit(&mut (), &mut visitor);
        assert_eq!(visitor.0, 3);
    }
}
