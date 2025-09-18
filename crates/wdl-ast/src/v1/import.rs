//! V1 AST representation for import statements.

use std::ffi::OsStr;
use std::path::Path;

use rowan::NodeOrToken;
use url::Url;
use wdl_grammar::lexer::v1::Logos;
use wdl_grammar::lexer::v1::Token;

use super::AliasKeyword;
use super::AsKeyword;
use super::ImportKeyword;
use super::LiteralString;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::Span;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::TreeNode;

/// Represents an import statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportStatement<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ImportStatement<N> {
    /// Gets the URI of the import statement.
    pub fn uri(&self) -> LiteralString<N> {
        self.child().expect("import should have a URI")
    }

    /// Gets the `import` keyword of the import statement.
    pub fn keyword(&self) -> ImportKeyword<N::Token> {
        self.token().expect("import should have a keyword")
    }

    /// Gets the explicit namespace of the import statement (i.e. the `as`
    /// clause).
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

        // Get just the file stem of the URI
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

        // Check to see if the stem is a valid WDL identifier
        let mut lexer = Token::lexer(&stem);
        match lexer.next()?.ok()? {
            Token::Ident if lexer.next().is_none() => {}
            _ => return None,
        }

        Some((stem.to_string(), uri.span()))
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
    fn import_statements() {
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

                // First import statement
                assert_eq!(imports[0].uri().text().unwrap().text(), "foo.wdl");
                assert!(imports[0].explicit_namespace().is_none());
                assert_eq!(
                    imports[0].namespace().map(|(n, _)| n).as_deref(),
                    Some("foo")
                );
                assert_eq!(imports[0].aliases().count(), 0);

                // Second import statement
                assert_eq!(imports[1].uri().text().unwrap().text(), "bar.wdl");
                assert_eq!(imports[1].explicit_namespace().unwrap().text(), "x");
                assert_eq!(imports[1].namespace().map(|(n, _)| n).as_deref(), Some("x"));
                assert_eq!(imports[1].aliases().count(), 0);

                // Third import statement
                assert_eq!(imports[2].uri().text().unwrap().text(), "baz.wdl");
                assert!(imports[2].explicit_namespace().is_none());
                assert_eq!(
                    imports[2].namespace().map(|(n, _)| n).as_deref(),
                    Some("baz")
                );
                assert_aliases(imports[2].aliases());

                // Fourth import statement
                assert_eq!(imports[3].uri().text().unwrap().text(), "qux.wdl");
                assert_eq!(imports[3].explicit_namespace().unwrap().text(), "x");
                assert_eq!(imports[3].namespace().map(|(n, _)| n).as_deref(), Some("x"));
                assert_aliases(imports[3].aliases());
            }
            _ => panic!("expected a V1 AST"),
        }
    }
}
