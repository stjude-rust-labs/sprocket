//! Module for the V1 grammar functions.

use super::macros::expected;
use super::macros::expected_fn;
use crate::experimental::grammar::macros::expected_with_name;
use crate::experimental::lexer::v1::Token;
use crate::experimental::lexer::TokenSet;
use crate::experimental::parser;
use crate::experimental::parser::Error;
use crate::experimental::parser::Marker;
use crate::experimental::parser::ParserToken;
use crate::experimental::tree::SyntaxKind;

/// The parser type for the V1 grammar.
pub type Parser<'a> = parser::Parser<'a, Token>;

/// The expected set of tokens at the top-level of a WDL document.
const TOP_EXPECTED_SET: TokenSet = TokenSet::new(&[
    Token::ImportKeyword as u8,
    Token::StructKeyword as u8,
    Token::TaskKeyword as u8,
    Token::WorkflowKeyword as u8,
]);

/// The names of the expected top-level items.
const TOP_EXPECTED_NAMES: &[&str] = &[
    "import statement",
    "struct definition",
    "task definition",
    "workflow definition",
];

/// The recovery set for top-level.
const TOP_RECOVERY_SET: TokenSet = TOP_EXPECTED_SET;

/// A set of tokens for primitive types.
const PRIMITIVE_TYPE_SET: TokenSet = TokenSet::new(&[
    Token::BooleanTypeKeyword as u8,
    Token::IntTypeKeyword as u8,
    Token::FloatTypeKeyword as u8,
    Token::StringTypeKeyword as u8,
    Token::FileTypeKeyword as u8,
]);

/// A set of tokens for all types.
const TYPE_EXPECTED_SET: TokenSet = PRIMITIVE_TYPE_SET.union(TokenSet::new(&[
    Token::MapTypeKeyword as u8,
    Token::ArrayTypeKeyword as u8,
    Token::PairTypeKeyword as u8,
    Token::ObjectTypeKeyword as u8,
    Token::Ident as u8,
]));

/// The expected set of tokens in a struct definition of a WDL document.
const STRUCT_ITEM_EXPECTED_SET: TokenSet = TYPE_EXPECTED_SET;

/// The recovery set for struct items.
const STRUCT_ITEM_RECOVERY_SET: TokenSet =
    STRUCT_ITEM_EXPECTED_SET.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The expected set of tokens in a task definition of a WDL document.
const TASK_ITEM_EXPECTED_SET: TokenSet = TYPE_EXPECTED_SET.union(TokenSet::new(&[
    Token::InputKeyword as u8,
    Token::CommandKeyword as u8,
    Token::OutputKeyword as u8,
    Token::RuntimeKeyword as u8,
    Token::MetaKeyword as u8,
    Token::ParameterMetaKeyword as u8,
]));

/// The recovery set for task items.
const TASK_ITEM_RECOVERY_SET: TokenSet =
    TASK_ITEM_EXPECTED_SET.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set for workflow items.
const WORKFLOW_ITEM_RECOVERY_SET: TokenSet = TYPE_EXPECTED_SET.union(TokenSet::new(&[
    Token::InputKeyword as u8,
    Token::OutputKeyword as u8,
    Token::MetaKeyword as u8,
    Token::ParameterMetaKeyword as u8,
    Token::IfKeyword as u8,
    Token::ScatterKeyword as u8,
    Token::CallKeyword as u8,
]));

/// A token set used to parse a delimited set of things until a closing brace.
const UNTIL_CLOSE_BRACE: TokenSet = TokenSet::new(&[Token::CloseBrace as u8]);

/// Parses the top-level items of a V1 document.
///
/// It is expected that the version statement has already been parsed.
pub fn items(parser: &mut Parser<'_>) {
    while parser.peek().is_some() {
        let marker = parser.start();
        if let Err((marker, e)) = item(parser, marker) {
            parser.error(e);
            parser.recover(TOP_RECOVERY_SET);
            marker.abandon(parser);
        }
    }
}

/// Parses a single top-level item in a WDL document.
fn item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    match parser.peek() {
        Some((Token::ImportKeyword, _)) => import_statement(parser, marker),
        Some((Token::StructKeyword, _)) => struct_definition(parser, marker),
        Some((Token::TaskKeyword, _)) => task_definition(parser, marker),
        Some((Token::WorkflowKeyword, _)) => workflow_definition(parser, marker),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.into_raw()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                Error::ExpectedOneOf {
                    expected: TOP_EXPECTED_NAMES,
                    found,
                    span,
                    describe: Token::describe,
                },
            ))
        }
    }
}

/// Parses an import statement.
fn import_statement(parser: &mut Parser<'_>, _marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::ImportKeyword);
    todo!("parse import statements")
}

/// Parses a name (i.e. identifier) for a struct, task, or workflow definition.
fn name(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    expected_with_name!(parser, marker, Token::Ident, "name");
    marker.complete(parser, SyntaxKind::NameNode);
    Ok(())
}

/// Parses a struct definition.
fn struct_definition(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::StructKeyword);
    expected_fn!(parser, marker, name);
    expected!(parser, marker, Token::OpenBrace);
    parser.delimited(
        None,
        UNTIL_CLOSE_BRACE,
        STRUCT_ITEM_RECOVERY_SET,
        |parser, marker| {
            unbound_decl(parser, marker)?;
            Ok(true)
        },
    );
    expected!(parser, marker, Token::CloseBrace);
    marker.complete(parser, SyntaxKind::StructDefinitionNode);
    Ok(())
}

/// Parses a task definition.
fn task_definition(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::TaskKeyword);
    expected_fn!(parser, marker, name);
    expected!(parser, marker, Token::OpenBrace);
    parser.delimited(
        None,
        UNTIL_CLOSE_BRACE,
        TASK_ITEM_RECOVERY_SET,
        |_parser, _marker| todo!("parse task items"),
    );
    expected!(parser, marker, Token::CloseBrace);
    marker.complete(parser, SyntaxKind::TaskDefinitionNode);
    Ok(())
}

/// Parses a workflow definition.
fn workflow_definition(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::WorkflowKeyword);
    expected_fn!(parser, marker, name);
    expected!(parser, marker, Token::OpenBrace);
    parser.delimited(
        None,
        UNTIL_CLOSE_BRACE,
        WORKFLOW_ITEM_RECOVERY_SET,
        |_parser, _marker| todo!("parse workflow items"),
    );
    expected!(parser, marker, Token::CloseBrace);
    Ok(())
}

/// Parses an unbound declaration.
fn unbound_decl(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    expected_fn!(parser, marker, ty);
    expected!(parser, marker, Token::Ident);
    marker.complete(parser, SyntaxKind::UnboundDeclNode);
    Ok(())
}

/// Parses a type used in a declaration.
fn ty(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    match parser.peek() {
        Some((Token::MapTypeKeyword, _)) => map(parser, marker),
        Some((Token::ArrayTypeKeyword, _)) => array(parser, marker),
        Some((Token::PairTypeKeyword, _)) => pair(parser, marker),
        Some((Token::ObjectTypeKeyword, _)) => object(parser, marker),
        Some((Token::Ident, _)) => type_ref(parser, marker),
        Some((t, _)) if PRIMITIVE_TYPE_SET.contains(t.into_raw()) => primitive(parser, marker),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.into_raw()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                Error::Expected {
                    expected: "type",
                    found,
                    span,
                    describe: Token::describe,
                },
            ))
        }
    }
}

/// Parses a map type used in a declaration.
fn map(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::MapTypeKeyword);
    expected!(parser, marker, Token::OpenBracket);
    expected_fn!(parser, marker, primitive);
    expected!(parser, marker, Token::Comma);
    expected_fn!(parser, marker, ty);
    expected!(parser, marker, Token::CloseBracket);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::MapTypeNode);
    Ok(())
}

/// Parses a array type used in a declaration.
fn array(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::ArrayTypeKeyword);
    expected!(parser, marker, Token::OpenBracket);
    expected_fn!(parser, marker, ty);
    expected!(parser, marker, Token::CloseBracket);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::ArrayTypeNode);
    Ok(())
}

/// Parses a pair type used in a declaration.
fn pair(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::PairTypeKeyword);
    expected!(parser, marker, Token::OpenBracket);
    expected_fn!(parser, marker, ty);
    expected!(parser, marker, Token::Comma);
    expected_fn!(parser, marker, ty);
    expected!(parser, marker, Token::CloseBracket);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::PairTypeNode);
    Ok(())
}

/// Parses an object type used in a declaration.
fn object(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::ObjectTypeKeyword);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::ObjectTypeNode);
    Ok(())
}

/// Parses a type reference used in a declaration.
fn type_ref(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::Ident);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::TypeRefNode);
    Ok(())
}

/// Parses a primitive type used in a declaration.
fn primitive(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require_in(PRIMITIVE_TYPE_SET);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::PrimitiveTypeNode);
    Ok(())
}
