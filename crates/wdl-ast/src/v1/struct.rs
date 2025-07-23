//! V1 AST representation for struct definitions.

use std::fmt;

use super::MetadataSection;
use super::ParameterMetadataSection;
use super::StructKeyword;
use super::UnboundDecl;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::TreeNode;
use crate::v1::MetadataValue;
use crate::v1::display::format_meta_value;
use crate::v1::display::get_param_meta;

/// Represents a struct definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructDefinition<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> StructDefinition<N> {
    /// Gets the name of the struct.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("struct should have a name")
    }

    /// Gets the `struct` keyword of the struct definition.
    pub fn keyword(&self) -> StructKeyword<N::Token> {
        self.token().expect("struct should have a keyword")
    }

    /// Gets the items in the struct definition.
    pub fn items(&self) -> impl Iterator<Item = StructItem<N>> + use<'_, N> {
        StructItem::children(&self.0)
    }

    /// Gets the member declarations of the struct.
    pub fn members(&self) -> impl Iterator<Item = UnboundDecl<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the metadata sections of the struct.
    pub fn metadata(&self) -> impl Iterator<Item = MetadataSection<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the parameter metadata sections of the struct.
    pub fn parameter_metadata(
        &self,
    ) -> impl Iterator<Item = ParameterMetadataSection<N>> + use<'_, N> {
        self.children()
    }

    /// Writes a Markdown formatted description of the struct.
    pub fn markdown_description(&self, f: &mut impl fmt::Write) -> fmt::Result {
        writeln!(f, "```wdl\nstruct {} {{", self.name().text())?;
        for member in self.members() {
            writeln!(
                f,
                "  {} {}",
                member.ty().inner().text(),
                member.name().text()
            )?;
        }
        writeln!(f, "}}\n```\n---")?;

        if let Some(meta) = self.metadata().next() {
            if let Some(desc) = meta.items().find(|i| i.name().text() == "description") {
                if let MetadataValue::String(s) = desc.value() {
                    if let Some(text) = s.text() {
                        writeln!(f, "{}\n", text.text())?;
                    }
                }
            }
        }

        let members: Vec<_> = self.members().collect();
        if !members.is_empty() {
            writeln!(f, "\n**Members**")?;
            for member in members {
                let name = member.name();
                write!(f, "- **{}**: `{}`", name.text(), member.ty().inner().text())?;
                if let Some(meta_val) =
                    get_param_meta(name.text(), self.parameter_metadata().next().as_ref())
                {
                    writeln!(f)?;
                    format_meta_value(f, &meta_val, 2)?;
                }
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

impl<N: TreeNode> AstNode<N> for StructDefinition<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StructDefinitionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::StructDefinitionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an item in a struct definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructItem<N: TreeNode = SyntaxNode> {
    /// The item is a member declaration.
    Member(UnboundDecl<N>),
    /// The item is a metadata section.
    Metadata(MetadataSection<N>),
    /// The item is a parameter meta section.
    ParameterMetadata(ParameterMetadataSection<N>),
}

impl<N: TreeNode> StructItem<N> {
    /// Returns whether or not the given syntax kind can be cast to
    /// [`StructItem`].
    pub fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::UnboundDeclNode
                | SyntaxKind::MetadataSectionNode
                | SyntaxKind::ParameterMetadataSectionNode
        )
    }

    /// Casts the given node to [`StructItem`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::UnboundDeclNode => Some(Self::Member(
                UnboundDecl::cast(inner).expect("unbound decl to cast"),
            )),
            SyntaxKind::MetadataSectionNode => Some(Self::Metadata(
                MetadataSection::cast(inner).expect("metadata section to cast"),
            )),
            SyntaxKind::ParameterMetadataSectionNode => Some(Self::ParameterMetadata(
                ParameterMetadataSection::cast(inner).expect("parameter metadata section to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Member(element) => element.inner(),
            Self::Metadata(element) => element.inner(),
            Self::ParameterMetadata(element) => element.inner(),
        }
    }

    /// Attempts to get a reference to the inner [`UnboundDecl`].
    ///
    /// * If `self` is a [`StructItem::Member`], then a reference to the inner
    ///   [`UnboundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_unbound_decl(&self) -> Option<&UnboundDecl<N>> {
        match self {
            Self::Member(d) => Some(d),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`UnboundDecl`].
    ///
    /// * If `self` is a [`StructItem::Member`], then the inner [`UnboundDecl`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_unbound_decl(self) -> Option<UnboundDecl<N>> {
        match self {
            Self::Member(d) => Some(d),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`StructItem::Metadata`], then a reference to the inner
    ///   [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_metadata_section(&self) -> Option<&MetadataSection<N>> {
        match self {
            Self::Metadata(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`StructItem::Metadata`], then the inner
    ///   [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_metadata_section(self) -> Option<MetadataSection<N>> {
        match self {
            Self::Metadata(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`StructItem::ParameterMetadata`], then a reference to
    ///   the inner [`ParameterMetadataSection`] is returned wrapped in
    ///   [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_parameter_metadata_section(&self) -> Option<&ParameterMetadataSection<N>> {
        match self {
            Self::ParameterMetadata(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`StructItem::ParameterMetadata`], then the inner
    ///   [`ParameterMetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_parameter_metadata_section(self) -> Option<ParameterMetadataSection<N>> {
        match self {
            Self::ParameterMetadata(s) => Some(s),
            _ => None,
        }
    }

    /// Finds the first child that can be cast to a [`StructItem`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`StructItem`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::AstToken;
    use crate::Document;

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
        assert_eq!(structs[0].name().text(), "Empty");
        assert_eq!(structs[0].members().count(), 0);

        // Second struct definition
        assert_eq!(structs[1].name().text(), "PrimitiveTypes");
        let members: Vec<_> = structs[1].members().collect();
        assert_eq!(members.len(), 12);

        // First member
        assert_eq!(members[0].name().text(), "a");
        assert_eq!(members[0].ty().to_string(), "Boolean");
        assert!(!members[0].ty().is_optional());

        // Second member
        assert_eq!(members[1].name().text(), "b");
        assert_eq!(members[1].ty().to_string(), "Boolean?");
        assert!(members[1].ty().is_optional());

        // Third member
        assert_eq!(members[2].name().text(), "c");
        assert_eq!(members[2].ty().to_string(), "Int");
        assert!(!members[2].ty().is_optional());

        // Fourth member
        assert_eq!(members[3].name().text(), "d");
        assert_eq!(members[3].ty().to_string(), "Int?");
        assert!(members[3].ty().is_optional());

        // Fifth member
        assert_eq!(members[4].name().text(), "e");
        assert_eq!(members[4].ty().to_string(), "Float");
        assert!(!members[4].ty().is_optional());

        // Sixth member
        assert_eq!(members[5].name().text(), "f");
        assert_eq!(members[5].ty().to_string(), "Float?");
        assert!(members[5].ty().is_optional());

        // Seventh member
        assert_eq!(members[6].name().text(), "g");
        assert_eq!(members[6].ty().to_string(), "String");
        assert!(!members[6].ty().is_optional());

        // Eighth member
        assert_eq!(members[7].name().text(), "h");
        assert_eq!(members[7].ty().to_string(), "String?");
        assert!(members[7].ty().is_optional());

        // Ninth member
        assert_eq!(members[8].name().text(), "i");
        assert_eq!(members[8].ty().to_string(), "File");
        assert!(!members[8].ty().is_optional());

        // Tenth member
        assert_eq!(members[9].name().text(), "j");
        assert_eq!(members[9].ty().to_string(), "File?");
        assert!(members[9].ty().is_optional());

        // Eleventh member
        assert_eq!(members[10].name().text(), "k");
        assert_eq!(members[10].ty().to_string(), "Directory");
        assert!(!members[10].ty().is_optional());

        // Twelfth member
        assert_eq!(members[11].name().text(), "l");
        assert_eq!(members[11].ty().to_string(), "Directory?");
        assert!(members[11].ty().is_optional());

        // Third struct definition
        assert_eq!(structs[2].name().text(), "ComplexTypes");
        let members: Vec<_> = structs[2].members().collect();
        assert_eq!(members.len(), 11);

        // First member
        assert_eq!(members[0].name().text(), "a");
        assert_eq!(members[0].ty().to_string(), "Map[Boolean, String]");
        assert!(!members[0].ty().is_optional());

        // Second member
        assert_eq!(members[1].name().text(), "b");
        assert_eq!(members[1].ty().to_string(), "Map[Int?, Array[String]]?");
        assert!(members[1].ty().is_optional());

        // Third member
        assert_eq!(members[2].name().text(), "c");
        assert_eq!(members[2].ty().to_string(), "Array[Boolean]");
        assert!(!members[2].ty().is_optional());

        // Fourth member
        assert_eq!(members[3].name().text(), "d");
        assert_eq!(members[3].ty().to_string(), "Array[Array[Float]]");
        assert!(!members[3].ty().is_optional());

        // Fifth member
        assert_eq!(members[4].name().text(), "e");
        assert_eq!(members[4].ty().to_string(), "Pair[Boolean, Boolean]");
        assert!(!members[4].ty().is_optional());

        // Sixth member
        assert_eq!(members[5].name().text(), "f");
        assert_eq!(
            members[5].ty().to_string(),
            "Pair[Array[String], Array[String?]]"
        );
        assert!(!members[5].ty().is_optional());

        // Seventh member
        assert_eq!(members[6].name().text(), "g");
        assert_eq!(members[6].ty().to_string(), "Object");
        assert!(!members[6].ty().is_optional());

        // Eighth member
        assert_eq!(members[7].name().text(), "h");
        assert_eq!(members[7].ty().to_string(), "Object?");
        assert!(members[7].ty().is_optional());

        // Ninth member
        assert_eq!(members[8].name().text(), "i");
        assert_eq!(members[8].ty().to_string(), "MyType");
        assert!(!members[8].ty().is_optional());

        // Tenth member
        assert_eq!(members[9].name().text(), "j");
        assert_eq!(members[9].ty().to_string(), "MyType?");
        assert!(members[9].ty().is_optional());

        // Eleventh member
        assert_eq!(members[10].name().text(), "k");
        assert_eq!(members[10].ty().to_string(), "Array[Directory]");
        assert!(!members[10].ty().is_optional());
    }
}
