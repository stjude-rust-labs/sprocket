//! Validation of supported syntax for WDL versions.

use std::str::FromStr;

use rowan::ast::support::token;
use wdl_grammar::ToSpan;

use crate::v1::Expr;
use crate::AstNode;
use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Span;
use crate::SyntaxKind;
use crate::VisitReason;
use crate::Visitor;

/// Creates an "exponentiation requirement" diagnostic.
fn exponentiation_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("usage of the exponentiation operator requires WDL version 1.2")
        .with_highlight(span)
}

/// Represents a supported V1 WDL version.
// NOTE: it is expected that this enumeration is in increasing order of 1.x versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum V1 {
    /// The document version is 1.0.
    Zero,
    /// The document version is 1.1.
    One,
    /// The document version is 1.2.
    Two,
}

/// Represents a supported WDL version.
// NOTE: it is expected that this enumeration is in increasing order of WDL versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Version {
    /// The document version is 1.x.
    V1(V1),
}

impl FromStr for Version {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1.0" => Ok(Self::V1(V1::Zero)),
            "1.1" => Ok(Self::V1(V1::One)),
            "1.2" => Ok(Self::V1(V1::Two)),
            _ => Err(()),
        }
    }
}

/// An AST visitor that ensures the syntax present in the document matches the
/// document's declared version.
#[derive(Debug, Default)]
pub struct VersionVisitor {
    /// Stores the version of the WDL document we're visiting.
    version: Option<Version>,
}

impl Visitor for VersionVisitor {
    type State = Diagnostics;

    fn document(&mut self, _: &mut Self::State, reason: VisitReason, document: &Document) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = document
            .version_statement()
            .and_then(|s| s.version().as_str().parse().ok());
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version {
            match expr {
                Expr::Exponentiation(e) if version < Version::V1(V1::Two) => {
                    state.add(exponentiation_requirement(
                        token(e.syntax(), SyntaxKind::Exponentiation)
                            .expect("should have operator")
                            .text_range()
                            .to_span(),
                    ));
                }
                _ => {}
            }
        }
    }
}
