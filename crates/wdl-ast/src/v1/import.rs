//! V1 AST representation for import statements.

use std::ffi::OsStr;
use std::path::Path;

use rowan::NodeOrToken;
use url::Url;
use wdl_grammar::lexer::v1::is_ident;

use super::AliasKeyword;
use super::AsKeyword;
use super::Asterisk;
use super::FromKeyword;
use super::ImportKeyword;
use super::LiteralString;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::Span;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::TreeNode;
use crate::TreeToken;

/// Represents an import statement.
///
/// An import statement has one of two shapes. A quoted import references a
/// URI as a string literal. A symbolic import references a module declared
/// in the consuming module's `module.json` manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportStatement<N: TreeNode = SyntaxNode>(N);

/// Discriminates between the two import shapes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportStatementKind<N: TreeNode = SyntaxNode> {
    /// A quoted URI import.
    Quoted(QuotedImport<N>),
    /// A symbolic module import.
    Symbolic(SymbolicImport<N>),
}

impl<N: TreeNode> ImportStatement<N> {
    /// Gets the `import` keyword of the import statement.
    pub fn keyword(&self) -> ImportKeyword<N::Token> {
        self.token().expect("import should have a keyword")
    }

    /// Returns the kind of the import statement.
    pub fn kind(&self) -> ImportStatementKind<N> {
        if let Some(q) = self.as_quoted() {
            ImportStatementKind::Quoted(q)
        } else if let Some(s) = self.as_symbolic() {
            ImportStatementKind::Symbolic(s)
        } else {
            unreachable!(
                "import statement has neither a `QuotedImportNode` nor a `SymbolicImportNode` \
                 child"
            )
        }
    }

    /// Returns the statement as a `QuotedImport`, if it is one.
    pub fn as_quoted(&self) -> Option<QuotedImport<N>> {
        self.child()
    }

    /// Returns the statement as a `SymbolicImport`, if it is one.
    pub fn as_symbolic(&self) -> Option<SymbolicImport<N>> {
        self.child()
    }
}

impl<N: TreeNode> AstNode<N> for ImportStatement<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ImportStatementNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ImportStatementNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a quoted-URI import body.
///
/// A quoted import consists of a string-literal URI, an optional `as <Ident>`
/// namespace override, and zero or more `alias <A> as <B>` clauses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotedImport<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> QuotedImport<N> {
    /// Gets the URI of the import statement.
    pub fn uri(&self) -> LiteralString<N> {
        self.child().expect("quoted import should have a URI")
    }

    /// Gets the explicit namespace (the `as <Ident>` clause) of the import.
    pub fn explicit_namespace(&self) -> Option<Ident<N::Token>> {
        self.token()
    }

    /// Gets the aliased names of the import statement.
    pub fn aliases(&self) -> impl Iterator<Item = ImportAlias<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the namespace of the import.
    ///
    /// If an explicit namespace was not present, this will determine the
    /// namespace based on the URI.
    ///
    /// Returns `None` if the namespace could not be derived; this may occur
    /// when the URI contains an interpolation or if the file stem of the
    /// URI is not a valid WDL identifier.
    ///
    /// The returned span is either the span of the explicit namespace or the
    /// span of the URI.
    pub fn namespace(&self) -> Option<(String, Span)> {
        if let Some(explicit) = self.explicit_namespace() {
            return Some((explicit.text().to_string(), explicit.span()));
        }

        let uri = self.uri();
        let text = uri.text()?;
        let stem = match Url::parse(text.text()) {
            Ok(url) => Path::new(
                urlencoding::decode(url.path_segments()?.next_back()?)
                    .ok()?
                    .as_ref(),
            )
            .file_stem()
            .and_then(OsStr::to_str)?
            .to_string(),
            Err(_) => Path::new(text.text())
                .file_stem()
                .and_then(OsStr::to_str)?
                .to_string(),
        };

        if !is_ident(&stem) {
            return None;
        }

        Some((stem.to_string(), uri.span()))
    }
}

impl<N: TreeNode> AstNode<N> for QuotedImport<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::QuotedImportNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::QuotedImportNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a symbolic-module import body.
///
/// A symbolic import references a module declared in the consuming module's
/// `module.json` manifest. The body has one of three shapes, each accepting
/// an optional trailing `as <Ident>` alias.
///
/// 1. A module path. Every member is brought into scope under a namespace;
///    the default name is the last path component, and the optional alias
///    renames that namespace.
/// 2. A wildcard `*` followed by `from <path>`. Every member is brought into
///    the consuming document's top-level scope; the optional alias groups
///    them under a namespace.
/// 3. A braced member list `{ ... }` followed by `from <path>`. Only the
///    selected members are brought into top-level scope; the optional alias
///    groups them under a namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicImport<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> SymbolicImport<N> {
    /// The symbolic module path of the import.
    pub fn module_path(&self) -> SymbolicModulePath<N> {
        self.child()
            .expect("symbolic import should have a module path")
    }

    /// The selected-members clause, present in the braced form.
    pub fn members(&self) -> Option<SymbolicImportMembers<N>> {
        self.child()
    }

    /// Whether the selection is the wildcard form (`import * from ...`).
    pub fn is_wildcard(&self) -> bool {
        self.0.children_with_tokens().any(|c| match c {
            NodeOrToken::Token(t) => t.kind() == SyntaxKind::Asterisk,
            NodeOrToken::Node(_) => false,
        })
    }

    /// The `*` token, present in the wildcard form.
    pub fn wildcard(&self) -> Option<Asterisk<N::Token>> {
        self.token()
    }

    /// The `from` keyword, present in wildcard and member forms.
    pub fn from_keyword(&self) -> Option<FromKeyword<N::Token>> {
        self.token()
    }

    /// The optional module alias (the trailing `as <Ident>`).
    pub fn alias(&self) -> Option<Ident<N::Token>> {
        self.0
            .children_with_tokens()
            .filter_map(|c| match c {
                NodeOrToken::Token(t) => Ident::cast(t),
                NodeOrToken::Node(_) => None,
            })
            .last()
    }
}

impl<N: TreeNode> AstNode<N> for SymbolicImport<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SymbolicImportNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::SymbolicImportNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents the unquoted path of a symbolic import.
///
/// The path consists of one or more identifier components separated by `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicModulePath<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> SymbolicModulePath<N> {
    /// The path components, in source order.
    pub fn components(&self) -> impl Iterator<Item = Ident<N::Token>> + use<'_, N> {
        self.0.children_with_tokens().filter_map(|c| match c {
            NodeOrToken::Token(t) => Ident::cast(t),
            NodeOrToken::Node(_) => None,
        })
    }

    /// The path rendered with `/` separators.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let mut first = true;
        for c in self.components() {
            if !first {
                out.push('/');
            }
            out.push_str(c.text());
            first = false;
        }
        out
    }
}

impl<N: TreeNode> AstNode<N> for SymbolicModulePath<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SymbolicModulePathNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::SymbolicModulePathNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// The braced selected-members clause of a symbolic import.
///
/// The clause contains one or more comma-separated `SymbolicImportMember`
/// entries inside `{` and `}`. A trailing comma is permitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicImportMembers<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> SymbolicImportMembers<N> {
    /// The member entries, in source order.
    pub fn members(&self) -> impl Iterator<Item = SymbolicImportMember<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for SymbolicImportMembers<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SymbolicImportMembersNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::SymbolicImportMembersNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// One member entry inside a braced `SymbolicImportMembers` clause.
///
/// An entry is a dotted path of one or more identifiers (e.g. `name`,
/// `ns.name`, `ns.inner.name`), optionally followed by `as <alias>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicImportMember<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> SymbolicImportMember<N> {
    /// The path components of the member, in source order.
    ///
    /// The iterator yields at least one identifier. For `ns.inner.name`, it
    /// yields `ns`, `inner`, `name` in order. The last component is the
    /// member's effective name; every preceding component is part of the
    /// namespace prefix.
    pub fn components(&self) -> impl Iterator<Item = Ident<N::Token>> + use<'_, N> {
        let count = self.component_count();
        self.idents().take(count)
    }

    /// The effective name of the member (the last path component).
    pub fn name(&self) -> Ident<N::Token> {
        self.components()
            .last()
            .expect("member should have at least one component")
    }

    /// The namespace components before the name.
    ///
    /// Returns an empty iterator when the member is a single identifier.
    pub fn namespace(&self) -> impl Iterator<Item = Ident<N::Token>> + use<'_, N> {
        let count = self.component_count();
        self.idents().take(count.saturating_sub(1))
    }

    /// The optional alias (the `as <Ident>` after the path).
    pub fn alias(&self) -> Option<Ident<N::Token>> {
        let count = self.component_count();
        self.idents().nth(count)
    }

    /// Returns every `Ident` child token in source order.
    fn idents(&self) -> impl Iterator<Item = Ident<N::Token>> + use<'_, N> {
        self.0.children_with_tokens().filter_map(|c| match c {
            NodeOrToken::Token(t) => Ident::cast(t),
            NodeOrToken::Node(_) => None,
        })
    }

    /// Returns the number of dotted path components (one more than the number
    /// of `.` tokens).
    fn component_count(&self) -> usize {
        1 + self
            .0
            .children_with_tokens()
            .filter(|c| match c {
                NodeOrToken::Token(t) => t.kind() == SyntaxKind::Dot,
                NodeOrToken::Node(_) => false,
            })
            .count()
    }
}

impl<N: TreeNode> AstNode<N> for SymbolicImportMember<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SymbolicImportMemberNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::SymbolicImportMemberNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an import alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportAlias<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ImportAlias<N> {
    /// Gets the source and target names of the alias.
    pub fn names(&self) -> (Ident<N::Token>, Ident<N::Token>) {
        let mut children = self.0.children_with_tokens().filter_map(|c| match c {
            NodeOrToken::Node(_) => None,
            NodeOrToken::Token(t) => Ident::cast(t),
        });

        let source = children.next().expect("expected a source identifier");
        let target = children.next().expect("expected a target identifier");
        (source, target)
    }

    /// Gets the `alias` keyword of the alias.
    pub fn alias_keyword(&self) -> AliasKeyword<N::Token> {
        self.token().expect("alias should have an `alias` keyword")
    }

    /// Gets the `as` keyword of the alias.
    pub fn as_keyword(&self) -> AsKeyword<N::Token> {
        self.token().expect("alias should have an `as` keyword")
    }
}

impl<N: TreeNode> AstNode<N> for ImportAlias<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ImportAliasNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ImportAliasNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::Ast;
    use crate::Document;

    #[test]
    fn quoted_import_statements() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

import "foo.wdl"
import "bar.wdl" as x
import "baz.wdl" alias A as B alias C as D
import "qux.wdl" as x alias A as B alias C as D
"#,
        );
        assert!(diagnostics.is_empty());
        match document.ast() {
            Ast::V1(ast) => {
                fn assert_aliases<N: TreeNode>(mut aliases: impl Iterator<Item = ImportAlias<N>>) {
                    let alias = aliases.next().unwrap();
                    let (to, from) = alias.names();
                    assert_eq!(to.text(), "A");
                    assert_eq!(from.text(), "B");
                    let alias = aliases.next().unwrap();
                    let (to, from) = alias.names();
                    assert_eq!(to.text(), "C");
                    assert_eq!(from.text(), "D");
                    assert!(aliases.next().is_none());
                }

                let imports: Vec<_> = ast.imports().collect();
                assert_eq!(imports.len(), 4);

                for import in &imports {
                    assert!(import.as_quoted().is_some());
                    assert!(import.as_symbolic().is_none());
                }

                let q = imports[0].as_quoted().unwrap();
                assert_eq!(q.uri().text().unwrap().text(), "foo.wdl");
                assert!(q.explicit_namespace().is_none());
                assert_eq!(q.namespace().map(|(n, _)| n).as_deref(), Some("foo"));
                assert_eq!(q.aliases().count(), 0);

                let q = imports[1].as_quoted().unwrap();
                assert_eq!(q.uri().text().unwrap().text(), "bar.wdl");
                assert_eq!(q.explicit_namespace().unwrap().text(), "x");
                assert_eq!(q.namespace().map(|(n, _)| n).as_deref(), Some("x"));
                assert_eq!(q.aliases().count(), 0);

                let q = imports[2].as_quoted().unwrap();
                assert_eq!(q.uri().text().unwrap().text(), "baz.wdl");
                assert!(q.explicit_namespace().is_none());
                assert_eq!(q.namespace().map(|(n, _)| n).as_deref(), Some("baz"));
                assert_aliases(q.aliases());

                let q = imports[3].as_quoted().unwrap();
                assert_eq!(q.uri().text().unwrap().text(), "qux.wdl");
                assert_eq!(q.explicit_namespace().unwrap().text(), "x");
                assert_eq!(q.namespace().map(|(n, _)| n).as_deref(), Some("x"));
                assert_aliases(q.aliases());
            }
            _ => panic!("expected a V1 AST"),
        }
    }

    #[test]
    fn symbolic_imports_cover_every_form() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.4

import openwdl/csvkit
import openwdl/csvkit as csv
import * from openwdl/csvkit
import * from openwdl/csvkit as csv
import { sort } from openwdl/csvkit
import { sort as sorter, sort.CsvSort, sort.CsvSort as MySort, a.b.c.deep } from openwdl/csvkit as csv
"#,
        );
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
        let Ast::V1(ast) = document.ast() else {
            panic!("expected a V1 AST");
        };

        let imports: Vec<_> = ast.imports().collect();
        assert_eq!(imports.len(), 6);

        for import in &imports {
            assert!(import.as_quoted().is_none());
            assert!(import.as_symbolic().is_some());
        }

        // Bare
        let s = imports[0].as_symbolic().unwrap();
        assert_eq!(s.module_path().text(), "openwdl/csvkit");
        assert!(s.members().is_none());
        assert!(!s.is_wildcard());
        assert!(s.alias().is_none());

        // Bare aliased
        let s = imports[1].as_symbolic().unwrap();
        assert_eq!(s.module_path().text(), "openwdl/csvkit");
        assert_eq!(s.alias().unwrap().text(), "csv");

        // Wildcard
        let s = imports[2].as_symbolic().unwrap();
        assert!(s.is_wildcard());
        assert!(s.members().is_none());
        assert_eq!(s.module_path().text(), "openwdl/csvkit");
        assert!(s.alias().is_none());

        // Wildcard aliased
        let s = imports[3].as_symbolic().unwrap();
        assert!(s.is_wildcard());
        assert_eq!(s.alias().unwrap().text(), "csv");

        // Simple member
        let s = imports[4].as_symbolic().unwrap();
        assert!(!s.is_wildcard());
        let members: Vec<_> = s.members().unwrap().members().collect();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name().text(), "sort");
        assert!(members[0].namespace().next().is_none());
        assert!(members[0].alias().is_none());

        // Mixed members, including a deep dotted path.
        let s = imports[5].as_symbolic().unwrap();
        let members: Vec<_> = s.members().unwrap().members().collect();
        assert_eq!(members.len(), 4);

        // `sort as sorter`
        assert_eq!(members[0].name().text(), "sort");
        assert!(members[0].namespace().next().is_none());
        assert_eq!(members[0].alias().unwrap().text(), "sorter");

        // `sort.CsvSort`
        assert_eq!(members[1].name().text(), "CsvSort");
        let ns: Vec<_> = members[1]
            .namespace()
            .map(|i| i.text().to_string())
            .collect();
        assert_eq!(ns, vec!["sort"]);
        assert!(members[1].alias().is_none());

        // `sort.CsvSort as MySort`
        assert_eq!(members[2].name().text(), "CsvSort");
        let ns: Vec<_> = members[2]
            .namespace()
            .map(|i| i.text().to_string())
            .collect();
        assert_eq!(ns, vec!["sort"]);
        assert_eq!(members[2].alias().unwrap().text(), "MySort");

        // `a.b.c.deep` (four-component path).
        assert_eq!(members[3].name().text(), "deep");
        let ns: Vec<_> = members[3]
            .namespace()
            .map(|i| i.text().to_string())
            .collect();
        assert_eq!(ns, vec!["a", "b", "c"]);
        assert!(members[3].alias().is_none());
        let components: Vec<_> = members[3]
            .components()
            .map(|i| i.text().to_string())
            .collect();
        assert_eq!(components, vec!["a", "b", "c", "deep"]);

        assert_eq!(s.alias().unwrap().text(), "csv");
    }
}
