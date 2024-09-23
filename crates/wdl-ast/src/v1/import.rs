//! V1 AST representation for import statements.

use std::ffi::OsStr;
use std::path::Path;

use url::Url;
use wdl_grammar::lexer::v1::Logos;
use wdl_grammar::lexer::v1::Token;

use super::LiteralString;
use crate::AstChildren;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::Span;
use crate::SyntaxElement;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::ToSpan;
use crate::WorkflowDescriptionLanguage;
use crate::support::child;
use crate::support::children;
use crate::token;

/// Represents an import statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportStatement(pub(crate) SyntaxNode);

impl ImportStatement {
    /// Gets the URI of the import statement.
    pub fn uri(&self) -> LiteralString {
        child(&self.0).expect("import should have a URI")
    }

    /// Gets the explicit namespace of the import statement (i.e. the `as`
    /// clause).
    pub fn explicit_namespace(&self) -> Option<Ident> {
        token(&self.0)
    }

    /// Gets the aliased names of the import statement.
    pub fn aliases(&self) -> AstChildren<ImportAlias> {
        children(&self.0)
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
            return Some((explicit.as_str().to_string(), explicit.span()));
        }

        // Get just the file stem of the URI
        let uri = self.uri();
        let text = uri.text()?;
        let stem = match Url::parse(text.as_str()) {
            Ok(url) => Path::new(
                urlencoding::decode(url.path_segments()?.last()?)
                    .ok()?
                    .as_ref(),
            )
            .file_stem()
            .and_then(OsStr::to_str)?
            .to_string(),
            Err(_) => Path::new(text.as_str())
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

        Some((stem.to_string(), uri.syntax().text_range().to_span()))
    }
}

impl AstNode for ImportStatement {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::ImportStatementNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ImportStatementNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an import alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportAlias(SyntaxNode);

impl ImportAlias {
    /// Gets the source and target names of the alias.
    pub fn names(&self) -> (Ident, Ident) {
        let mut children = self.0.children_with_tokens().filter_map(|c| match c {
            SyntaxElement::Node(_) => None,
            SyntaxElement::Token(t) => Ident::cast(t),
        });

        let source = children.next().expect("expected a source identifier");
        let target = children.next().expect("expected a target identifier");
        (source, target)
    }
}

impl AstNode for ImportAlias {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::ImportAliasNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ImportAliasNode => Some(Self(syntax)),
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

    use super::*;
    use crate::Ast;
    use crate::Document;
    use crate::SupportedVersion;
    use crate::VisitReason;
    use crate::Visitor;

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
                let assert_aliases = |mut aliases: AstChildren<ImportAlias>| {
                    let alias = aliases.next().unwrap();
                    let (to, from) = alias.names();
                    assert_eq!(to.as_str(), "A");
                    assert_eq!(from.as_str(), "B");
                    let alias = aliases.next().unwrap();
                    let (to, from) = alias.names();
                    assert_eq!(to.as_str(), "C");
                    assert_eq!(from.as_str(), "D");
                    assert!(aliases.next().is_none());
                };

                let imports: Vec<_> = ast.imports().collect();
                assert_eq!(imports.len(), 4);

                // First import statement
                assert_eq!(imports[0].uri().text().unwrap().as_str(), "foo.wdl");
                assert!(imports[0].explicit_namespace().is_none());
                assert_eq!(
                    imports[0].namespace().map(|(n, _)| n).as_deref(),
                    Some("foo")
                );
                assert_eq!(imports[0].aliases().count(), 0);

                // Second import statement
                assert_eq!(imports[1].uri().text().unwrap().as_str(), "bar.wdl");
                assert_eq!(imports[1].explicit_namespace().unwrap().as_str(), "x");
                assert_eq!(imports[1].namespace().map(|(n, _)| n).as_deref(), Some("x"));
                assert_eq!(imports[1].aliases().count(), 0);

                // Third import statement
                assert_eq!(imports[2].uri().text().unwrap().as_str(), "baz.wdl");
                assert!(imports[2].explicit_namespace().is_none());
                assert_eq!(
                    imports[2].namespace().map(|(n, _)| n).as_deref(),
                    Some("baz")
                );
                assert_aliases(imports[2].aliases());

                // Fourth import statement
                assert_eq!(imports[3].uri().text().unwrap().as_str(), "qux.wdl");
                assert_eq!(imports[3].explicit_namespace().unwrap().as_str(), "x");
                assert_eq!(imports[3].namespace().map(|(n, _)| n).as_deref(), Some("x"));
                assert_aliases(imports[3].aliases());

                // Use a visitor to visit the import statements in the tree
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

                    fn import_statement(
                        &mut self,
                        _: &mut Self::State,
                        reason: VisitReason,
                        stmt: &ImportStatement,
                    ) {
                        if reason == VisitReason::Exit {
                            return;
                        }

                        let uri = stmt.uri().text().unwrap();
                        match self.0 {
                            0 => assert_eq!(uri.as_str(), "foo.wdl"),
                            1 => assert_eq!(uri.as_str(), "bar.wdl"),
                            2 => assert_eq!(uri.as_str(), "baz.wdl"),
                            3 => assert_eq!(uri.as_str(), "qux.wdl"),
                            _ => panic!("too many imports"),
                        }

                        self.0 += 1;
                    }
                }

                let mut visitor = MyVisitor(0);
                document.visit(&mut (), &mut visitor);
                assert_eq!(visitor.0, 4);
            }
            _ => panic!("expected a V1 AST"),
        }
    }
}
