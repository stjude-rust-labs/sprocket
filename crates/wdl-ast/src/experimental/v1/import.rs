//! V1 AST representation for import statements.

use rowan::ast::support::child;
use rowan::ast::support::children;
use rowan::ast::AstChildren;
use rowan::ast::AstNode;

use super::LiteralString;
use crate::experimental::token;
use crate::experimental::AstToken;
use crate::experimental::Ident;
use crate::experimental::SyntaxElement;
use crate::experimental::SyntaxKind;
use crate::experimental::SyntaxNode;
use crate::experimental::WorkflowDescriptionLanguage;

/// Represents an import statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportStatement(pub(super) SyntaxNode);

impl ImportStatement {
    /// Gets the URI of the import statement.
    pub fn uri(&self) -> LiteralString {
        child(&self.0).expect("import should have a URI")
    }

    /// Gets the optional namespace of the import statement (i.e. the `as`
    /// clause).
    pub fn namespace(&self) -> Option<Ident> {
        token(&self.0)
    }

    /// Gets the aliased names of the import statement.
    pub fn aliases(&self) -> AstChildren<ImportAlias> {
        children(&self.0)
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
pub struct ImportAlias(pub(super) SyntaxNode);

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
    use crate::experimental::v1::Visitor;
    use crate::experimental::Ast;
    use crate::experimental::Document;
    use crate::experimental::VisitReason;

    #[test]
    fn import_statements() {
        let parse = Document::parse(
            r#"
version 1.1

import "foo.wdl"
import "bar.wdl" as x
import "baz.wdl" alias A as B alias C as D
import "qux.wdl" as x alias A as B alias C as D
"#,
        );
        let document = parse.into_result().expect("there should be no errors");
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
                assert!(imports[0].namespace().is_none());
                assert_eq!(imports[0].aliases().count(), 0);

                // Second import statement
                assert_eq!(imports[1].uri().text().unwrap().as_str(), "bar.wdl");
                assert_eq!(imports[1].namespace().unwrap().as_str(), "x");
                assert_eq!(imports[1].aliases().count(), 0);

                // Third import statement
                assert_eq!(imports[2].uri().text().unwrap().as_str(), "baz.wdl");
                assert!(imports[2].namespace().is_none());
                assert_aliases(imports[2].aliases());

                // Fourth import statement
                assert_eq!(imports[3].uri().text().unwrap().as_str(), "qux.wdl");
                assert_eq!(imports[3].namespace().unwrap().as_str(), "x");
                assert_aliases(imports[3].aliases());

                // Use a visitor to visit the import statements in the tree
                struct MyVisitor(usize);

                impl Visitor for MyVisitor {
                    type State = ();

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
                ast.visit(&mut (), &mut visitor);
                assert_eq!(visitor.0, 4);
            }
            _ => panic!("expected a V1 AST"),
        }
    }
}
