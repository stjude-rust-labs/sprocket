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
/// Three forms are represented by a single node kind, distinguished by which
/// optional children are present.
///
/// 1. `import <source> [as <alias>] (alias <Old> as <New>)*` — the existing
///    import form. User-defined types from `<source>` are brought into the
///    importing document's scope; tasks and workflows are accessible
///    through the pseudo-namespace.
/// 2. `import * from <source>` — every task, workflow, and user-defined
///    type from `<source>` is brought into the importing document's scope.
/// 3. `import { <member> [as <Name>], ... } from <source>` — only the
///    listed items are brought into scope.
///
/// `<source>` is either a quoted string URI (`uri()`) or an unquoted symbolic
/// module path (`module_path()`). Forms 2 and 3 do not accept the trailing
/// `as <alias>` or `alias` clauses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportStatement<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ImportStatement<N> {
    /// Gets the `import` keyword of the statement.
    pub fn keyword(&self) -> ImportKeyword<N::Token> {
        self.token().expect("import should have a keyword")
    }

    /// The quoted URI of the import, when the source is a string literal.
    pub fn uri(&self) -> Option<LiteralString<N>> {
        self.child()
    }

    /// The unquoted symbolic module path, when the source is a path.
    pub fn module_path(&self) -> Option<SymbolicModulePath<N>> {
        self.child()
    }

    /// The braced member-selection clause, present in form 3.
    pub fn members(&self) -> Option<SymbolicImportMembers<N>> {
        self.child()
    }

    /// Whether the statement uses the `*` wildcard form.
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

    /// The `from` keyword, present in the wildcard and member forms.
    pub fn from_keyword(&self) -> Option<FromKeyword<N::Token>> {
        self.token()
    }

    /// The explicit namespace introduced by the `as <Ident>` clause.
    ///
    /// This clause is only valid on the no-member form (form 1) and renames
    /// the pseudo-namespace through which the imported module's tasks and
    /// workflows are accessed.
    pub fn explicit_namespace(&self) -> Option<Ident<N::Token>> {
        // The final `Ident` token not associated with any child node.
        // Member aliases live inside `SymbolicImportMembersNode`, not here.
        self.0
            .children_with_tokens()
            .filter_map(|c| match c {
                NodeOrToken::Token(t) => Ident::cast(t),
                NodeOrToken::Node(_) => None,
            })
            .last()
    }

    /// The `alias <src> as <dst>` clauses on a form-1 import.
    pub fn aliases(&self) -> impl Iterator<Item = ImportAlias<N>> + use<'_, N> {
        self.children()
    }

    /// The derived namespace for tasks and workflows reached through this
    /// import, along with the span at which it is defined.
    ///
    /// Returns `None` when the namespace cannot be derived. For a quoted
    /// import with no `as <alias>`, the namespace is the file stem of the
    /// URI. For a symbolic import with no `as <alias>`, the namespace is the
    /// last component of the module path. An explicit `as <alias>` overrides
    /// both.
    pub fn namespace(&self) -> Option<(String, Span)> {
        if let Some(explicit) = self.explicit_namespace() {
            return Some((explicit.text().to_string(), explicit.span()));
        }

        if let Some(uri) = self.uri() {
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
            return Some((stem, uri.span()));
        }

        if let Some(path) = self.module_path() {
            let last = path.components().last()?;
            return Some((last.text().to_string(), last.span()));
        }

        None
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

/// Represents an `alias <src> as <dst>` clause.
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
    fn quoted_imports() {
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
        let Ast::V1(ast) = document.ast() else {
            panic!("expected a V1 AST");
        };

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
            assert!(import.uri().is_some());
            assert!(import.module_path().is_none());
            assert!(!import.is_wildcard());
            assert!(import.members().is_none());
        }

        assert_eq!(imports[0].uri().unwrap().text().unwrap().text(), "foo.wdl");
        assert!(imports[0].explicit_namespace().is_none());
        assert_eq!(
            imports[0].namespace().map(|(n, _)| n).as_deref(),
            Some("foo"),
        );
        assert_eq!(imports[0].aliases().count(), 0);

        assert_eq!(imports[1].uri().unwrap().text().unwrap().text(), "bar.wdl");
        assert_eq!(imports[1].explicit_namespace().unwrap().text(), "x");
        assert_eq!(imports[1].namespace().map(|(n, _)| n).as_deref(), Some("x"),);
        assert_eq!(imports[1].aliases().count(), 0);

        assert_eq!(imports[2].uri().unwrap().text().unwrap().text(), "baz.wdl");
        assert!(imports[2].explicit_namespace().is_none());
        assert_eq!(
            imports[2].namespace().map(|(n, _)| n).as_deref(),
            Some("baz"),
        );
        assert_aliases(imports[2].aliases());

        assert_eq!(imports[3].uri().unwrap().text().unwrap().text(), "qux.wdl");
        assert_eq!(imports[3].explicit_namespace().unwrap().text(), "x");
        assert_eq!(imports[3].namespace().map(|(n, _)| n).as_deref(), Some("x"),);
        assert_aliases(imports[3].aliases());
    }

    #[test]
    fn symbolic_imports() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.4

import openwdl/csvkit
import openwdl/csvkit as csv
import * from openwdl/csvkit
import { sort } from openwdl/csvkit
import { sort.CsvSort, sort.CsvSortStable as Stable } from "local.wdl"
import { a.b.c.deep } from openwdl/csvkit
"#,
        );
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
        let Ast::V1(ast) = document.ast() else {
            panic!("expected a V1 AST");
        };

        let imports: Vec<_> = ast.imports().collect();
        assert_eq!(imports.len(), 6);

        // Form 1, symbolic, no alias.
        assert!(imports[0].uri().is_none());
        assert_eq!(imports[0].module_path().unwrap().text(), "openwdl/csvkit");
        assert!(!imports[0].is_wildcard());
        assert!(imports[0].members().is_none());
        assert_eq!(
            imports[0].namespace().map(|(n, _)| n).as_deref(),
            Some("csvkit"),
        );

        // Form 1, symbolic, aliased.
        assert_eq!(imports[1].explicit_namespace().unwrap().text(), "csv");
        assert_eq!(
            imports[1].namespace().map(|(n, _)| n).as_deref(),
            Some("csv"),
        );

        // Form 2, wildcard, symbolic source.
        assert!(imports[2].is_wildcard());
        assert!(imports[2].members().is_none());
        assert_eq!(imports[2].module_path().unwrap().text(), "openwdl/csvkit");
        assert!(imports[2].explicit_namespace().is_none());

        // Form 3, single member, symbolic source.
        assert!(!imports[3].is_wildcard());
        let members: Vec<_> = imports[3].members().unwrap().members().collect();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name().text(), "sort");

        // Form 3 with quoted source, dotted members, per-member alias.
        assert!(imports[4].uri().is_some());
        assert!(imports[4].module_path().is_none());
        let members: Vec<_> = imports[4].members().unwrap().members().collect();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name().text(), "CsvSort");
        let ns: Vec<_> = members[0]
            .namespace()
            .map(|i| i.text().to_string())
            .collect();
        assert_eq!(ns, vec!["sort"]);
        assert_eq!(members[1].name().text(), "CsvSortStable");
        assert_eq!(members[1].alias().unwrap().text(), "Stable");

        // Deep dotted path.
        let members: Vec<_> = imports[5].members().unwrap().members().collect();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name().text(), "deep");
        let ns: Vec<_> = members[0]
            .namespace()
            .map(|i| i.text().to_string())
            .collect();
        assert_eq!(ns, vec!["a", "b", "c"]);
    }
}
