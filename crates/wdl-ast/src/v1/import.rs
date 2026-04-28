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
///    importing document's scope; tasks and workflows are accessible through
///    the pseudo-namespace.
/// 2. `import * from <source>` — every task, workflow, and user-defined type
///    from `<source>` is brought into the importing document's scope.
/// 3. `import { <member> [as <Name>], ... } from <source>` — only the listed
///    items are brought into scope.
///
/// `<source>` is either a quoted string URI or an unquoted symbolic module
/// path; the variants are reachable through `source()`. Forms 2 and 3 do not
/// accept the trailing `as <alias>` or `alias` clauses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportStatement<N: TreeNode = SyntaxNode>(N);

/// The source of an [`ImportStatement`].
///
/// The grammar guarantees that every well-formed `ImportStatementNode` has
/// exactly one of these two children, so callers always receive a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportSource<N: TreeNode = SyntaxNode> {
    /// A quoted string URI source, e.g. `"some/file.wdl"`.
    Uri(LiteralString<N>),
    /// An unquoted symbolic module path source, e.g. `wizard/spellbook`.
    ModulePath(SymbolicModulePath<N>),
}

impl<N: TreeNode> ImportSource<N> {
    /// The span of the source.
    pub fn span(&self) -> Span {
        match self {
            Self::Uri(uri) => uri.span(),
            Self::ModulePath(path) => path.span(),
        }
    }
}

/// The shape of an [`ImportStatement`].
///
/// Callers dispatch on this and then reach for `members`,
/// `explicit_namespace`, or `aliases` as the form requires.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImportForm {
    /// `import <source> [as <alias>] (alias <Old> as <New>)*`. Introduces
    /// a namespace through which the imported module's tasks and workflows
    /// are accessed; user-defined types are copied into the importing
    /// document's scope.
    ///
    /// ```wdl
    /// import "csvkit.wdl"
    /// import wizard/spellbook as book
    /// ```
    One,
    /// `import * from <source>`. Brings every task, workflow, and
    /// user-defined type from the source into the importing document's
    /// scope. No namespace.
    ///
    /// ```wdl
    /// import * from wizard/spellbook
    /// ```
    Two,
    /// `import { <member> [as <Name>], ... } from <source>`. Brings only
    /// the listed members into the importing document's scope, with an
    /// optional per-member rename. No namespace.
    ///
    /// ```wdl
    /// import { Cauldron, Wand as Staff } from wizard/spellbook
    /// ```
    Three,
}

impl<N: TreeNode> ImportStatement<N> {
    /// Gets the `import` keyword of the statement.
    pub fn keyword(&self) -> ImportKeyword<N::Token> {
        self.token().expect("import should have a keyword")
    }

    /// The shape of the import statement.
    pub fn form(&self) -> ImportForm {
        if self.wildcard().is_some() {
            ImportForm::Two
        } else if self.members().is_some() {
            ImportForm::Three
        } else {
            ImportForm::One
        }
    }

    /// The source of the import, either a quoted URI or a symbolic module
    /// path.
    pub fn source(&self) -> ImportSource<N> {
        if let Some(uri) = self.child::<LiteralString<N>>() {
            return ImportSource::Uri(uri);
        }
        if let Some(path) = self.child::<SymbolicModulePath<N>>() {
            return ImportSource::ModulePath(path);
        }
        // SAFETY: the v1 grammar's `import_statement` rule requires either a
        // string source or a `symbolic_module_path`, so a well-formed
        // `ImportStatementNode` always has one of these as a child.
        unreachable!()
    }

    /// The braced member-selection clause, present in form 3.
    pub fn members(&self) -> Option<ImportMembers<N>> {
        self.child()
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
    /// The `as <alias>` clause is only valid on form 1; the grammar rejects
    /// it on the wildcard and member-selection forms. This accessor walks
    /// the top-level token stream and returns the `Ident` immediately
    /// following an `AsKeyword`, so it returns `None` on every form except a
    /// form-1 import that explicitly aliases its source.
    pub fn explicit_namespace(&self) -> Option<Ident<N::Token>> {
        let mut tokens = self.0.children_with_tokens().filter_map(|c| c.into_token());
        while let Some(t) = tokens.next() {
            if t.kind() == SyntaxKind::AsKeyword {
                return tokens.find_map(Ident::cast);
            }
        }
        None
    }

    /// The `alias <src> as <dst>` clauses on a form-1 import.
    pub fn aliases(&self) -> impl Iterator<Item = ImportAlias<N>> + use<'_, N> {
        self.children()
    }

    /// The derived namespace for tasks and workflows reached through this
    /// import, along with the span at which it is defined.
    ///
    /// Only form 1 introduces a namespace; the wildcard and member-selection
    /// forms bring items directly into the importing document's scope and
    /// return `None`. For a quoted form-1 import with no `as <alias>`, the
    /// namespace is the file stem of the URI. For a symbolic form-1 import
    /// with no `as <alias>`, the namespace is the last component of the
    /// module path. An explicit `as <alias>` overrides both.
    pub fn namespace(&self) -> Option<(String, Span)> {
        if self.form() != ImportForm::One {
            return None;
        }

        if let Some(explicit) = self.explicit_namespace() {
            return Some((explicit.text().to_string(), explicit.span()));
        }

        match self.source() {
            ImportSource::Uri(uri) => {
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
                Some((stem, uri.span()))
            }
            ImportSource::ModulePath(path) => {
                let last = path.components().last()?;
                Some((last.text().to_string(), last.span()))
            }
        }
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

/// The braced selected-members clause of an import.
///
/// The clause contains one or more comma-separated `ImportMember`
/// entries inside `{` and `}`. A trailing comma is permitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportMembers<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ImportMembers<N> {
    /// The member entries, in source order.
    pub fn members(&self) -> impl Iterator<Item = ImportMember<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for ImportMembers<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ImportMembersNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ImportMembersNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// One member entry inside a braced `ImportMembers` clause.
///
/// An entry is a single identifier optionally followed by `as <alias>` to
/// rename it locally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportMember<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ImportMember<N> {
    /// The name of the imported member.
    pub fn name(&self) -> Ident<N::Token> {
        self.idents()
            .next()
            .expect("member should have a name identifier")
    }

    /// The optional alias (the `as <Ident>` clause).
    pub fn alias(&self) -> Option<Ident<N::Token>> {
        self.idents().nth(1)
    }

    /// Returns every `Ident` child token in source order.
    fn idents(&self) -> impl Iterator<Item = Ident<N::Token>> + use<'_, N> {
        self.0.children_with_tokens().filter_map(|c| match c {
            NodeOrToken::Token(t) => Ident::cast(t),
            NodeOrToken::Node(_) => None,
        })
    }
}

impl<N: TreeNode> AstNode<N> for ImportMember<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ImportMemberNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ImportMemberNode => Some(Self(inner)),
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
            assert_eq!(import.form(), ImportForm::One);
            assert!(matches!(import.source(), ImportSource::Uri(_)));
            assert!(import.wildcard().is_none());
            assert!(import.members().is_none());
        }

        assert_eq!(uri_text(&imports[0]), "foo.wdl");
        assert!(imports[0].explicit_namespace().is_none());
        assert_eq!(
            imports[0].namespace().map(|(n, _)| n).as_deref(),
            Some("foo"),
        );
        assert_eq!(imports[0].aliases().count(), 0);

        assert_eq!(uri_text(&imports[1]), "bar.wdl");
        assert_eq!(imports[1].explicit_namespace().unwrap().text(), "x");
        assert_eq!(imports[1].namespace().map(|(n, _)| n).as_deref(), Some("x"),);
        assert_eq!(imports[1].aliases().count(), 0);

        assert_eq!(uri_text(&imports[2]), "baz.wdl");
        assert!(imports[2].explicit_namespace().is_none());
        assert_eq!(
            imports[2].namespace().map(|(n, _)| n).as_deref(),
            Some("baz"),
        );
        assert_aliases(imports[2].aliases());

        assert_eq!(uri_text(&imports[3]), "qux.wdl");
        assert_eq!(imports[3].explicit_namespace().unwrap().text(), "x");
        assert_eq!(imports[3].namespace().map(|(n, _)| n).as_deref(), Some("x"),);
        assert_aliases(imports[3].aliases());
    }

    fn uri_text(import: &ImportStatement) -> String {
        match import.source() {
            ImportSource::Uri(uri) => uri.text().unwrap().text().to_string(),
            ImportSource::ModulePath(_) => panic!("expected a quoted URI source"),
        }
    }

    fn module_path_text(import: &ImportStatement) -> String {
        match import.source() {
            ImportSource::ModulePath(path) => path.text(),
            ImportSource::Uri(_) => panic!("expected a symbolic module path source"),
        }
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
import { CsvSort, CsvSortStable as Stable } from "local.wdl"
"#,
        );
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
        let Ast::V1(ast) = document.ast() else {
            panic!("expected a V1 AST");
        };

        let imports: Vec<_> = ast.imports().collect();
        assert_eq!(imports.len(), 5);

        // Form 1, symbolic, no alias: `import openwdl/csvkit`.
        assert_eq!(imports[0].form(), ImportForm::One);
        assert_eq!(imports[0].keyword().text(), "import");
        assert_eq!(module_path_text(&imports[0]), "openwdl/csvkit");
        assert!(imports[0].wildcard().is_none());
        assert!(imports[0].from_keyword().is_none());
        assert!(imports[0].members().is_none());
        assert!(imports[0].explicit_namespace().is_none());
        assert_eq!(imports[0].aliases().count(), 0);
        assert_eq!(
            imports[0].namespace().map(|(n, _)| n).as_deref(),
            Some("csvkit"),
        );

        // Form 1, symbolic, aliased: `import openwdl/csvkit as csv`.
        assert_eq!(imports[1].form(), ImportForm::One);
        assert_eq!(imports[1].keyword().text(), "import");
        assert_eq!(module_path_text(&imports[1]), "openwdl/csvkit");
        assert!(imports[1].wildcard().is_none());
        assert!(imports[1].from_keyword().is_none());
        assert!(imports[1].members().is_none());
        assert_eq!(imports[1].explicit_namespace().unwrap().text(), "csv");
        assert_eq!(imports[1].aliases().count(), 0);
        assert_eq!(
            imports[1].namespace().map(|(n, _)| n).as_deref(),
            Some("csv"),
        );

        // Form 2, wildcard, symbolic source: `import * from openwdl/csvkit`.
        assert_eq!(imports[2].form(), ImportForm::Two);
        assert_eq!(imports[2].keyword().text(), "import");
        assert_eq!(module_path_text(&imports[2]), "openwdl/csvkit");
        assert!(imports[2].wildcard().is_some());
        assert_eq!(imports[2].from_keyword().unwrap().text(), "from");
        assert!(imports[2].members().is_none());
        assert!(imports[2].explicit_namespace().is_none());
        assert_eq!(imports[2].aliases().count(), 0);
        assert!(imports[2].namespace().is_none());

        // Form 3, single member, symbolic source:
        // `import { sort } from openwdl/csvkit`.
        assert_eq!(imports[3].form(), ImportForm::Three);
        assert_eq!(imports[3].keyword().text(), "import");
        assert_eq!(module_path_text(&imports[3]), "openwdl/csvkit");
        assert!(imports[3].wildcard().is_none());
        assert_eq!(imports[3].from_keyword().unwrap().text(), "from");
        assert!(imports[3].explicit_namespace().is_none());
        assert_eq!(imports[3].aliases().count(), 0);
        assert!(imports[3].namespace().is_none());
        let members: Vec<_> = imports[3].members().unwrap().members().collect();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name().text(), "sort");
        assert!(members[0].alias().is_none());

        // Form 3, quoted source, multiple members with per-member alias:
        // `import { CsvSort, CsvSortStable as Stable } from "local.wdl"`.
        assert_eq!(imports[4].form(), ImportForm::Three);
        assert_eq!(imports[4].keyword().text(), "import");
        assert_eq!(uri_text(&imports[4]), "local.wdl");
        assert!(imports[4].wildcard().is_none());
        assert_eq!(imports[4].from_keyword().unwrap().text(), "from");
        assert!(imports[4].explicit_namespace().is_none());
        assert_eq!(imports[4].aliases().count(), 0);
        assert!(imports[4].namespace().is_none());
        let members: Vec<_> = imports[4].members().unwrap().members().collect();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name().text(), "CsvSort");
        assert!(members[0].alias().is_none());
        assert_eq!(members[1].name().text(), "CsvSortStable");
        assert_eq!(members[1].alias().unwrap().text(), "Stable");
    }
}
