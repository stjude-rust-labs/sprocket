//! Module for the V1 grammar functions.

use miette::SourceSpan;

use super::macros::expected;
use super::macros::expected_fn;
use crate::experimental::grammar::macros::expected_with_name;
use crate::experimental::lexer::v1::DQStringToken;
use crate::experimental::lexer::v1::SQStringToken;
use crate::experimental::lexer::v1::Token;
use crate::experimental::lexer::TokenSet;
use crate::experimental::parser;
use crate::experimental::parser::CompletedMarker;
use crate::experimental::parser::Error;
use crate::experimental::parser::Event;
use crate::experimental::parser::Interpolator;
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

/// The expected names of primitive types.
const PRIMITIVE_TYPE_NAMES: &[&str] = &["Boolean", "Integer", "Float", "String", "File"];

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

/// The expected set of tokens in a task definition.
const TASK_ITEM_EXPECTED_SET: TokenSet = TYPE_EXPECTED_SET.union(TokenSet::new(&[
    Token::InputKeyword as u8,
    Token::CommandKeyword as u8,
    Token::OutputKeyword as u8,
    Token::RuntimeKeyword as u8,
    Token::MetaKeyword as u8,
    Token::ParameterMetaKeyword as u8,
]));

/// The expected names of items in a task definition.
const TASK_ITEM_EXPECTED_NAMES: &[&str] = &[
    "input section",
    "command section",
    "output section",
    "runtime section",
    "metadata section",
    "parameter metadata section",
    "private declaration",
];

/// The recovery set for task items.
const TASK_ITEM_RECOVERY_SET: TokenSet =
    TASK_ITEM_EXPECTED_SET.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The expected set of tokens in a workflow definition.
const WORKFLOW_ITEM_EXPECTED_SET: TokenSet = TYPE_EXPECTED_SET.union(TokenSet::new(&[
    Token::InputKeyword as u8,
    Token::OutputKeyword as u8,
    Token::MetaKeyword as u8,
    Token::ParameterMetaKeyword as u8,
    Token::IfKeyword as u8,
    Token::ScatterKeyword as u8,
    Token::CallKeyword as u8,
]));

/// The expected names of items in a workflow definition.
const WORKFLOW_ITEM_EXPECTED_NAMES: &[&str] = &[
    "input section",
    "output section",
    "runtime section",
    "metadata section",
    "parameter metadata section",
    "conditional statement",
    "scatter statement",
    "task call statement",
    "private declaration",
];

/// The recovery set of tokens in a workflow definition.
const WORKFLOW_ITEM_RECOVERY_SET: TokenSet =
    WORKFLOW_ITEM_EXPECTED_SET.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The expected token set for metadata values.
const METADATA_VALUE_EXPECTED_SET: TokenSet = TokenSet::new(&[
    Token::Minus as u8,
    Token::Integer as u8,
    Token::Float as u8,
    Token::SQStringStart as u8,
    Token::DQStringStart as u8,
    Token::TrueKeyword as u8,
    Token::FalseKeyword as u8,
    Token::OpenBrace as u8,
    Token::OpenBracket as u8,
]);

/// The expected names of metadata values.
const METADATA_VALUE_EXPECTED_NAMES: &[&str] = &[
    "number",
    "string",
    "boolean",
    "metadata object",
    "metadata array",
    "null",
];

/// The recovery set of tokens in a metadata section.
const METADATA_SECTION_RECOVERY_SET: TokenSet =
    METADATA_VALUE_EXPECTED_SET.union(TokenSet::new(&[
        Token::Ident as u8,
        Token::CloseBrace as u8,
    ]));

/// The recovery set of tokens in a metadata object.
const METADATA_OBJECT_RECOVERY_SET: TokenSet = METADATA_VALUE_EXPECTED_SET.union(TokenSet::new(&[
    Token::Ident as u8,
    Token::Comma as u8,
    Token::CloseBrace as u8,
]));

/// The recovery set of tokens in a metadata array.
const METADATA_ARRAY_RECOVERY_SET: TokenSet = TokenSet::new(&[
    Token::Ident as u8,
    Token::Comma as u8,
    Token::CloseBracket as u8,
]);

/// A token set for expression atoms.
const ATOM_EXPECTED_SET: TokenSet = TokenSet::new(&[
    Token::Integer as u8,
    Token::Float as u8,
    Token::TrueKeyword as u8,
    Token::FalseKeyword as u8,
    Token::DQStringStart as u8,
    Token::SQStringStart as u8,
    Token::OpenBracket as u8,
    Token::OpenBrace as u8,
    Token::OpenParen as u8,
    Token::ObjectKeyword as u8,
    Token::Ident as u8,
    Token::IfKeyword as u8,
]);

/// A token set for prefix operators.
///
/// This intentionally excludes open parenthesis for grouping expressions as it
/// is handled during parsing of atoms due to the ambiguity with pair literals.
const PREFIX_OPERATOR_EXPECTED_SET: TokenSet =
    TokenSet::new(&[Token::Exclamation as u8, Token::Minus as u8]);

/// A token set for infix operators.
const INFIX_OPERATOR_EXPECTED_SET: TokenSet = TokenSet::new(&[
    Token::LogicalOr as u8,
    Token::LogicalAnd as u8,
    Token::Plus as u8,
    Token::Minus as u8,
    Token::Asterisk as u8,
    Token::Slash as u8,
    Token::Percent as u8,
    Token::Equal as u8,
    Token::NotEqual as u8,
    Token::Less as u8,
    Token::LessEqual as u8,
    Token::Greater as u8,
    Token::GreaterEqual as u8,
]);

/// A token set for postfix operators.
const POSTFIX_OPERATOR_EXPECTED_SET: TokenSet = TokenSet::new(&[
    Token::OpenParen as u8,
    Token::OpenBracket as u8,
    Token::Dot as u8,
]);

/// A token set used to recover to the next expression.
const EXPR_RECOVERY_SET: TokenSet = ATOM_EXPECTED_SET.union(PREFIX_OPERATOR_EXPECTED_SET);

/// A token set for map item recovery.
///
/// As the key and value in a map are both expressions, we recover
/// only at the next comma.
const MAP_RECOVERY_SET: TokenSet = TokenSet::new(&[Token::Comma as u8, Token::CloseBrace as u8]);

/// A token set for literal struct item recovery.
///
/// As both the key and value in a literal struct may be an identifier,
/// we recover only at the next comma.
const LITERAL_OBJECT_RECOVERY_SET: TokenSet =
    TokenSet::new(&[Token::Comma as u8, Token::CloseBrace as u8]);

/// A token set used to parse a delimited set of things until a closing brace.
const UNTIL_CLOSE_BRACE: TokenSet = TokenSet::new(&[Token::CloseBrace as u8]);

/// A token set used to parse a delimited set of things until a closing bracket.
const UNTIL_CLOSE_BRACKET: TokenSet = TokenSet::new(&[Token::CloseBracket as u8]);

/// A token set used to parse a delimited set of things until a closing
/// parenthesis.
const UNTIL_CLOSE_PAREN: TokenSet = TokenSet::new(&[Token::CloseParen as u8]);

/// A helper for parsing surrounding braces.
///
/// An example would be sections in a task or workflow or metadata objects.
fn braced<F>(parser: &mut Parser<'_>, cb: F) -> Result<(), Error>
where
    F: FnOnce(&mut Parser<'_>) -> Result<(), Error>,
{
    let opening = match parser.expect(Token::OpenBrace) {
        Ok(span) => span,
        Err(e) => return Err(e),
    };

    cb(parser)?;

    match parser.next() {
        Some((Token::CloseBrace, _)) => Ok(()),
        Some((token, span)) => Err(Error::UnmatchedBrace {
            found: Some(token.into_raw()),
            span,
            describe: Token::describe,
            opening,
        }),
        None => {
            let span = parser.span();
            Err(Error::UnmatchedBrace {
                found: None,
                span,
                describe: Token::describe,
                opening,
            })
        }
    }
}

/// A helper for parsing surrounding brackets.
///
/// An example would be array literals.
fn bracketed<F>(parser: &mut Parser<'_>, cb: F) -> Result<(), Error>
where
    F: FnOnce(&mut Parser<'_>) -> Result<(), Error>,
{
    let opening = match parser.expect(Token::OpenBracket) {
        Ok(span) => span,
        Err(e) => return Err(e),
    };

    cb(parser)?;

    match parser.next() {
        Some((Token::CloseBracket, _)) => Ok(()),
        Some((token, span)) => Err(Error::UnmatchedBracket {
            found: Some(token.into_raw()),
            span,
            describe: Token::describe,
            opening,
        }),
        None => {
            let span = parser.span();
            Err(Error::UnmatchedBracket {
                found: None,
                span,
                describe: Token::describe,
                opening,
            })
        }
    }
}

/// A helper for parsing surrounding parenthesis.
///
/// An example would be a call expression.
fn paren<F>(parser: &mut Parser<'_>, cb: F) -> Result<(), Error>
where
    F: FnOnce(&mut Parser<'_>) -> Result<(), Error>,
{
    let opening = match parser.expect(Token::OpenParen) {
        Ok(span) => span,
        Err(e) => return Err(e),
    };

    cb(parser)?;

    match parser.next() {
        Some((Token::CloseParen, _)) => Ok(()),
        Some((token, span)) => Err(Error::UnmatchedParen {
            found: Some(token.into_raw()),
            span,
            describe: Token::describe,
            opening,
        }),
        None => {
            let span = parser.span();
            Err(Error::UnmatchedParen {
                found: None,
                span,
                describe: Token::describe,
                opening,
            })
        }
    }
}

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
    if let Err(e) = braced(parser, |parser| {
        parser.delimited(
            None,
            UNTIL_CLOSE_BRACE,
            STRUCT_ITEM_RECOVERY_SET,
            unbound_decl,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }
    marker.complete(parser, SyntaxKind::StructDefinitionNode);
    Ok(())
}

/// Parses a task definition.
fn task_definition(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::TaskKeyword);
    expected_fn!(parser, marker, name);
    if let Err(e) = braced(parser, |parser| {
        parser.delimited(None, UNTIL_CLOSE_BRACE, TASK_ITEM_RECOVERY_SET, task_item);
        Ok(())
    }) {
        return Err((marker, e));
    };
    marker.complete(parser, SyntaxKind::TaskDefinitionNode);
    Ok(())
}

/// Parses a workflow definition.
fn workflow_definition(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::WorkflowKeyword);
    expected_fn!(parser, marker, name);
    if let Err(e) = braced(parser, |parser| {
        parser.delimited(
            None,
            UNTIL_CLOSE_BRACE,
            WORKFLOW_ITEM_RECOVERY_SET,
            workflow_item,
        );
        Ok(())
    }) {
        return Err((marker, e));
    };
    marker.complete(parser, SyntaxKind::WorkflowDefinitionNode);
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
        Some((Token::MapTypeKeyword, _)) => map_type(parser, marker),
        Some((Token::ArrayTypeKeyword, _)) => array_type(parser, marker),
        Some((Token::PairTypeKeyword, _)) => pair_type(parser, marker),
        Some((Token::ObjectTypeKeyword, _)) => object_type(parser, marker),
        Some((Token::Ident, _)) => type_ref(parser, marker),
        Some((t, _)) if PRIMITIVE_TYPE_SET.contains(t.into_raw()) => primitive_type(parser, marker),
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
fn map_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    /// Parses the inner part of the brackets
    fn parse(parser: &mut Parser<'_>) -> Result<(), Error> {
        expected_fn!(parser, primitive_type);
        parser.expect(Token::Comma)?;
        expected_fn!(parser, ty);
        Ok(())
    }

    parser.require(Token::MapTypeKeyword);
    if let Err(e) = bracketed(parser, parse) {
        return Err((marker, e));
    }
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::MapTypeNode);
    Ok(())
}

/// Parses a array type used in a declaration.
fn array_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    /// Parses the inner part of the brackets
    fn parse(parser: &mut Parser<'_>) -> Result<(), Error> {
        expected_fn!(parser, ty);
        Ok(())
    }

    parser.require(Token::ArrayTypeKeyword);
    if let Err(e) = bracketed(parser, parse) {
        return Err((marker, e));
    }
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::ArrayTypeNode);
    Ok(())
}

/// Parses a pair type used in a declaration.
fn pair_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    /// Parses the inner part of the brackets
    fn parse(parser: &mut Parser<'_>) -> Result<(), Error> {
        expected_fn!(parser, ty);
        parser.expect(Token::Comma)?;
        expected_fn!(parser, ty);
        Ok(())
    }

    parser.require(Token::PairTypeKeyword);
    if let Err(e) = bracketed(parser, parse) {
        return Err((marker, e));
    }
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::PairTypeNode);
    Ok(())
}

/// Parses an object type used in a declaration.
fn object_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
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
fn primitive_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    if let Err(e) = parser.expect_in(PRIMITIVE_TYPE_SET, PRIMITIVE_TYPE_NAMES) {
        return Err((marker, e));
    }

    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::PrimitiveTypeNode);
    Ok(())
}

/// Parses an item in a task definition.
fn task_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    match parser.peek() {
        Some((Token::InputKeyword, _)) => input_section(parser, marker),
        Some((Token::CommandKeyword, _)) => command_section(parser, marker),
        Some((Token::OutputKeyword, _)) => output_section(parser, marker),
        Some((Token::RuntimeKeyword, _)) => runtime_section(parser, marker),
        Some((Token::MetaKeyword, _)) => metadata_section(parser, marker),
        Some((Token::ParameterMetaKeyword, _)) => parameter_metadata_section(parser, marker),
        Some((t, _)) if TYPE_EXPECTED_SET.contains(t.into_raw()) => bound_decl(parser, marker),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.into_raw()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                Error::ExpectedOneOf {
                    expected: TASK_ITEM_EXPECTED_NAMES,
                    found,
                    span,
                    describe: Token::describe,
                },
            ))
        }
    }
}

/// Parses an item in a workflow definition.
fn workflow_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    match parser.peek() {
        Some((Token::InputKeyword, _)) => input_section(parser, marker),
        Some((Token::OutputKeyword, _)) => output_section(parser, marker),
        Some((Token::MetaKeyword, _)) => metadata_section(parser, marker),
        Some((Token::ParameterMetaKeyword, _)) => parameter_metadata_section(parser, marker),
        Some((Token::IfKeyword, _)) => conditional_statement(parser, marker),
        Some((Token::ScatterKeyword, _)) => scatter_statement(parser, marker),
        Some((Token::CallKeyword, _)) => call_statement(parser, marker),
        Some((t, _)) if TYPE_EXPECTED_SET.contains(t.into_raw()) => bound_decl(parser, marker),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.into_raw()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                Error::ExpectedOneOf {
                    expected: WORKFLOW_ITEM_EXPECTED_NAMES,
                    found,
                    span,
                    describe: Token::describe,
                },
            ))
        }
    }
}

/// Parses an input section in a task or workflow.
fn input_section(parser: &mut Parser<'_>, _marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::InputKeyword);
    todo!("parse input sections")
}

/// Parses a command section in a task.
fn command_section(parser: &mut Parser<'_>, _marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::CommandKeyword);
    todo!("parse command sections")
}

/// Parses an output section in a task or workflow.
fn output_section(parser: &mut Parser<'_>, _marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::OutputKeyword);
    todo!("parse output sections")
}

/// Parses a runtime section in a task.
fn runtime_section(parser: &mut Parser<'_>, _marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::RuntimeKeyword);
    todo!("parse runtime sections")
}

/// Parses a metadata section in a task or workflow.
fn metadata_section(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::MetaKeyword);
    if let Err(e) = braced(parser, |parser| {
        parser.delimited(
            None,
            UNTIL_CLOSE_BRACE,
            METADATA_SECTION_RECOVERY_SET,
            metadata_object_item,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }
    marker.complete(parser, SyntaxKind::MetadataSectionNode);
    Ok(())
}

/// Parses an item in a metadata object.
fn metadata_object_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    expected!(parser, marker, Token::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, metadata_value);
    marker.complete(parser, SyntaxKind::MetadataObjectItemNode);
    Ok(())
}

/// Parses a metadata value.
fn metadata_value(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    match parser.peek() {
        Some((Token::Minus, _)) | Some((Token::Integer, _)) | Some((Token::Float, _)) => {
            number(parser, marker, true)?;
            Ok(())
        }
        Some((Token::SQStringStart, _)) => {
            single_quote_string(parser, marker, false)?;
            Ok(())
        }
        Some((Token::DQStringStart, _)) => {
            double_quote_string(parser, marker, false)?;
            Ok(())
        }
        Some((Token::TrueKeyword, _)) | Some((Token::FalseKeyword, _)) => {
            boolean(parser, marker)?;
            Ok(())
        }
        Some((Token::NullKeyword, _)) => null(parser, marker),
        Some((Token::OpenBrace, _)) => metadata_object(parser, marker),
        Some((Token::OpenBracket, _)) => metadata_array(parser, marker),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.into_raw()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                Error::ExpectedOneOf {
                    expected: METADATA_VALUE_EXPECTED_NAMES,
                    found,
                    span,
                    describe: Token::describe,
                },
            ))
        }
    }
}

/// Parses a number.
fn number(
    parser: &mut Parser<'_>,
    marker: Marker,
    accept_minus: bool,
) -> Result<CompletedMarker, (Marker, Error)> {
    if accept_minus {
        parser.next_if(Token::Minus);
    }
    let kind = match parser.expect_in(
        TokenSet::new(&[Token::Integer as u8, Token::Float as u8]),
        &["number"],
    ) {
        Ok((Token::Integer, _)) => SyntaxKind::LiteralIntegerNode,
        Ok((Token::Float, _)) => SyntaxKind::LiteralFloatNode,
        Ok(_) => unreachable!(),
        Err(e) => return Err((marker, e)),
    };

    Ok(marker.complete(parser, kind))
}

/// Parses a placeholder option.
fn placeholder_option(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    match parser.peek() {
        Some((Token::Ident, span)) => {
            let kind = match parser.source(span) {
                "sep" => SyntaxKind::PlaceholderSepOptionNode,
                "default" => SyntaxKind::PlaceholderDefaultOptionNode,
                _ => {
                    // Not a placeholder option
                    marker.abandon(parser);
                    return Ok(());
                }
            };

            parser.next();
            expected!(parser, marker, Token::Assignment);
            expected_fn!(parser, marker, string);
            marker.complete(parser, kind);
            Ok(())
        }
        Some((t @ Token::TrueKeyword, _)) | Some((t @ Token::FalseKeyword, _)) => {
            parser.next();
            expected!(parser, marker, Token::Assignment);
            expected_fn!(parser, marker, string);
            expected!(
                parser,
                marker,
                if t == Token::TrueKeyword {
                    Token::FalseKeyword
                } else {
                    Token::TrueKeyword
                }
            );
            expected!(parser, marker, Token::Assignment);
            expected_fn!(parser, marker, string);
            marker.complete(parser, SyntaxKind::PlaceholderTrueFalseOptionNode);
            Ok(())
        }
        _ => {
            // Not a placeholder option
            marker.abandon(parser);
            Ok(())
        }
    }
}

/// Parses a placeholder expression.
fn placeholder_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
    opening: SourceSpan,
) -> Result<(), (Marker, Error)> {
    expected_fn!(parser, marker, placeholder_option);
    expected_fn!(parser, marker, expr);

    // Check for a closing brace; if it's missing, add an error
    // but don't consume the token; the found token will be considered
    // part of the string
    match parser.peek() {
        Some((Token::CloseBrace, _)) => {
            parser.next();
            marker.complete(parser, SyntaxKind::PlaceholderNode);
            Ok(())
        }
        found => {
            let (found, found_span) = found
                .map(|(t, s)| (Some(t.into_raw()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                Error::UnmatchedPlaceholder {
                    found,
                    span: found_span,
                    describe: Token::describe,
                    opening,
                },
            ))
        }
    }
}

/// Interpolates a single-quoted string.
fn single_quote_interpolate(
    start: SourceSpan,
    allow_interpolation: bool,
    mut interpolator: Interpolator<'_, SQStringToken>,
) -> (Parser<'_>, Result<(), Error>) {
    let mut text = None;
    let mut end = None;

    while let Some((Ok(token), span)) = interpolator.next() {
        match token {
            SQStringToken::PlaceholderStart if allow_interpolation => {
                // Add any encountered literal text
                if let Some(span) = text.take() {
                    interpolator.event(Event::Token {
                        kind: SyntaxKind::LiteralStringText,
                        span,
                    })
                }

                let marker = interpolator.start();
                interpolator.event(Event::Token {
                    kind: SyntaxKind::PlaceholderOpen,
                    span,
                });

                // Parse the placeholder expression
                let mut parser = interpolator.into_parser();
                if let Err((marker, e)) = placeholder_expr(&mut parser, marker, span) {
                    parser.error(e);
                    marker.abandon(&mut parser);
                    parser.recover(TokenSet::new(&[
                        Token::CloseBrace as u8,
                        Token::SQStringStart as u8,
                    ]));
                }

                interpolator = parser.into_interpolator();
            }
            t @ (SQStringToken::PlaceholderStart
            | SQStringToken::Escape
            | SQStringToken::Text
            | SQStringToken::DollarSign
            | SQStringToken::Tilde) => {
                // Placeholders are not be allowed at this point
                if t == SQStringToken::PlaceholderStart {
                    interpolator.error(Error::MetadataStringPlaceholder { span });
                }

                // Update the span of the text to include this token
                text = match text {
                    Some(prev) => Some(SourceSpan::new(
                        prev.offset().into(),
                        prev.len() + span.len(),
                    )),
                    None => Some(span),
                };
            }
            SQStringToken::End => {
                end = Some(span);
                break;
            }
        }
    }

    if let Some(span) = text.take() {
        interpolator.event(Event::Token {
            kind: SyntaxKind::LiteralStringText,
            span,
        })
    }

    match end {
        Some(span) => {
            // Push an end quote as we're done interpolating the string
            interpolator.event(Event::Token {
                kind: SyntaxKind::SingleQuote,
                span,
            });

            (interpolator.into_parser(), Ok(()))
        }
        None => {
            // String wasn't terminated
            (
                interpolator.into_parser(),
                Err(Error::UnterminatedString { span: start }),
            )
        }
    }
}

/// Parses either a single-quote string or a double-quote string.
fn string(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Error)> {
    match parser.peek() {
        Some((Token::SQStringStart, _)) => single_quote_string(parser, marker, true),
        Some((Token::DQStringStart, _)) => double_quote_string(parser, marker, true),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.into_raw()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                Error::Expected {
                    expected: "string",
                    found,
                    span,
                    describe: Token::describe,
                },
            ))
        }
    }
}

/// Parses a single-quoted string.
fn single_quote_string(
    parser: &mut Parser<'_>,
    marker: Marker,
    allow_interpolation: bool,
) -> Result<CompletedMarker, (Marker, Error)> {
    let start = parser.require(Token::SQStringStart);

    if let Err(e) = parser.interpolate(|i| single_quote_interpolate(start, allow_interpolation, i))
    {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralStringNode))
}

/// Interpolates a double-quoted string.
fn double_quote_interpolate(
    start: SourceSpan,
    allow_interpolation: bool,
    mut interpolator: Interpolator<'_, DQStringToken>,
) -> (Parser<'_>, Result<(), Error>) {
    let mut text = None;
    let mut end = None;

    while let Some((Ok(token), span)) = interpolator.next() {
        match token {
            DQStringToken::PlaceholderStart if allow_interpolation => {
                // Add any encountered literal text
                if let Some(span) = text.take() {
                    interpolator.event(Event::Token {
                        kind: SyntaxKind::LiteralStringText,
                        span,
                    })
                }

                let marker = interpolator.start();
                interpolator.event(Event::Token {
                    kind: SyntaxKind::PlaceholderOpen,
                    span,
                });

                // Parse the placeholder expression
                let mut parser = interpolator.into_parser();
                if let Err((marker, e)) = placeholder_expr(&mut parser, marker, span) {
                    parser.error(e);
                    marker.abandon(&mut parser);
                    parser.recover(TokenSet::new(&[
                        Token::CloseBrace as u8,
                        Token::DQStringStart as u8,
                    ]));
                }

                interpolator = parser.into_interpolator();
            }
            t @ (DQStringToken::PlaceholderStart
            | DQStringToken::Escape
            | DQStringToken::Text
            | DQStringToken::DollarSign
            | DQStringToken::Tilde) => {
                // Placeholders are not be allowed at this point
                if t == DQStringToken::PlaceholderStart {
                    interpolator.error(Error::MetadataStringPlaceholder { span });
                }

                text = match text {
                    Some(prev) => Some(SourceSpan::new(
                        prev.offset().into(),
                        prev.len() + span.len(),
                    )),
                    None => Some(span),
                };
            }
            DQStringToken::End => {
                end = Some(span);
                break;
            }
        }
    }

    // Add any encountered literal text
    if let Some(span) = text.take() {
        interpolator.event(Event::Token {
            kind: SyntaxKind::LiteralStringText,
            span,
        })
    }

    match end {
        Some(span) => {
            // Push an end quote as we're done parsing the string
            interpolator.event(Event::Token {
                kind: SyntaxKind::DoubleQuote,
                span,
            });

            (interpolator.into_parser(), Ok(()))
        }
        None => {
            // String wasn't terminated
            (
                interpolator.into_parser(),
                Err(Error::UnterminatedString { span: start }),
            )
        }
    }
}

/// Parses a double-quoted string.
fn double_quote_string(
    parser: &mut Parser<'_>,
    marker: Marker,
    allow_interpolation: bool,
) -> Result<CompletedMarker, (Marker, Error)> {
    let start = parser.require(Token::DQStringStart);

    if let Err(e) = parser.interpolate(|i| double_quote_interpolate(start, allow_interpolation, i))
    {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralStringNode))
}

/// Parses a literal boolean.
fn boolean(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Error)> {
    parser.require_in(TokenSet::new(&[
        Token::TrueKeyword as u8,
        Token::FalseKeyword as u8,
    ]));

    Ok(marker.complete(parser, SyntaxKind::LiteralBooleanNode))
}

/// Parses a literal null.
fn null(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::NullKeyword);
    marker.complete(parser, SyntaxKind::LiteralNullNode);
    Ok(())
}

/// Parses a metadata object.
fn metadata_object(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    if let Err(e) = braced(parser, |parser| {
        parser.delimited(
            Some(Token::Comma),
            UNTIL_CLOSE_BRACE,
            METADATA_OBJECT_RECOVERY_SET,
            metadata_object_item,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }
    marker.complete(parser, SyntaxKind::MetadataObjectNode);
    Ok(())
}

/// Parses a metadata array.
fn metadata_array(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    if let Err(e) = bracketed(parser, |parser| {
        parser.delimited(
            Some(Token::Comma),
            UNTIL_CLOSE_BRACKET,
            METADATA_ARRAY_RECOVERY_SET,
            metadata_value,
        );
        Ok(())
    }) {
        return Err((marker, e));
    };
    marker.complete(parser, SyntaxKind::MetadataArrayNode);
    Ok(())
}

/// Parses a parameter metadata section in a task or workflow.
fn parameter_metadata_section(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<(), (Marker, Error)> {
    parser.require(Token::ParameterMetaKeyword);
    if let Err(e) = braced(parser, |parser| {
        parser.delimited(
            None,
            UNTIL_CLOSE_BRACE,
            METADATA_SECTION_RECOVERY_SET,
            metadata_object_item,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }
    marker.complete(parser, SyntaxKind::ParameterMetadataSectionNode);
    Ok(())
}

/// Parses a bound declaration.
fn bound_decl(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    expected_fn!(parser, marker, ty);
    expected_fn!(parser, marker, name);
    expected!(parser, marker, Token::Assignment);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::BoundDeclNode);
    Ok(())
}

/// Parses a conditional statement in a workflow.
fn conditional_statement(parser: &mut Parser<'_>, _marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::IfKeyword);
    todo!("parse conditional statement")
}

/// Parses a scatter statement in a workflow.
fn scatter_statement(parser: &mut Parser<'_>, _marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::ScatterKeyword);
    todo!("parse scatter statement")
}

/// Parses a call statement in a workflow.
fn call_statement(parser: &mut Parser<'_>, _marker: Marker) -> Result<(), (Marker, Error)> {
    parser.require(Token::CallKeyword);
    todo!("parse call statement")
}

/// Parses an expression.
#[inline]
fn expr(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    expr_with_precedence(parser, marker, 0)?;
    Ok(())
}

/// Parses an expression with the given minimum precedence.
///
/// See https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html
fn expr_with_precedence(
    parser: &mut Parser<'_>,
    marker: Marker,
    min_precedence: u8,
) -> Result<CompletedMarker, (Marker, Error)> {
    // First parse an atom or a prefix operation as the left-hand side
    let mut lhs = match parser.peek() {
        Some((token, _)) if ATOM_EXPECTED_SET.contains(token.into_raw()) => {
            let lhs = parser.start();
            match atom_expr(parser, lhs, token) {
                Ok(lhs) => lhs,
                Err((lhs, e)) => {
                    lhs.abandon(parser);
                    return Err((marker, e));
                }
            }
        }
        Some((token, _)) if PREFIX_OPERATOR_EXPECTED_SET.contains(token.into_raw()) => {
            let prefix = parser.start();
            parser.next();
            let rhs = parser.start();
            let (precedence, kind, associativity) = prefix_precedence(token);
            match expr_with_precedence(
                parser,
                rhs,
                // Add one to the precedence for left-associative operators
                match associativity {
                    Associativity::Left => precedence + 1,
                    Associativity::Right => precedence,
                },
            ) {
                Ok(_) => prefix.complete(parser, kind),
                Err((rhs, e)) => {
                    prefix.abandon(parser);
                    rhs.abandon(parser);
                    return Err((marker, e));
                }
            }
        }
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.into_raw()), s))
                .unwrap_or_else(|| (None, parser.span()));
            return Err((
                marker,
                Error::Expected {
                    expected: "expression",
                    found,
                    span,
                    describe: Token::describe,
                },
            ));
        }
    };

    // Extend the parent chain of the left-hand side to the provided marker.
    lhs = lhs.extend_to(parser, marker);

    loop {
        // Check for either an infix or postix operation
        match parser.peek() {
            Some((token, _)) if INFIX_OPERATOR_EXPECTED_SET.contains(token.into_raw()) => {
                // The operation is an infix operation; check the precedence level
                let (precedence, kind, associativity) = infix_precedence(token);
                if precedence < min_precedence {
                    break;
                }

                let infix = lhs.precede(parser);
                parser.next();

                // Recuse for the right-hand side
                let rhs = parser.start();
                if let Err((rhs, e)) = expr_with_precedence(
                    parser,
                    rhs,
                    // Add one to the precedence for left-associative operators
                    match associativity {
                        Associativity::Left => precedence + 1,
                        Associativity::Right => precedence,
                    },
                ) {
                    rhs.abandon(parser);
                    return Err((infix, e));
                }

                lhs = infix.complete(parser, kind);
            }
            Some((token, _)) if POSTFIX_OPERATOR_EXPECTED_SET.contains(token.into_raw()) => {
                // The operation is a postfix operation; check the precedence level
                let precedence = postfix_precedence(token);
                if precedence < min_precedence {
                    break;
                }

                // Call the operation-specific parse function
                let postfix = lhs.precede(parser);
                let res = match token {
                    Token::OpenParen => call_expr(parser, postfix),
                    Token::OpenBracket => index_expr(parser, postfix),
                    Token::Dot => access_expr(parser, postfix),
                    _ => panic!("unexpected postfix operator"),
                };

                lhs = match res {
                    Ok(marker) => marker,
                    Err((postfix, e)) => {
                        return Err((postfix, e));
                    }
                };
            }
            _ => break,
        }
    }

    Ok(lhs)
}

/// Parses an atomic expression such as a literal.
///
/// Due to the WDL grammar having an ambiguity between parenthesized expressions
/// and pair literals, this function handles the former in addition to pair
/// literals.
fn atom_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
    peeked: Token,
) -> Result<CompletedMarker, (Marker, Error)> {
    match peeked {
        Token::Float | Token::Integer => number(parser, marker, false),
        Token::TrueKeyword | Token::FalseKeyword => boolean(parser, marker),
        Token::SQStringStart => single_quote_string(parser, marker, true),
        Token::DQStringStart => double_quote_string(parser, marker, true),
        Token::OpenBracket => array(parser, marker),
        Token::OpenBrace => map(parser, marker),
        Token::OpenParen => pair_or_paren_expr(parser, marker),
        Token::ObjectKeyword => object(parser, marker),
        Token::Ident => literal_struct_or_name_ref(parser, marker),
        Token::IfKeyword => if_expr(parser, marker),
        _ => unreachable!(),
    }
}

/// Parses an array literal expression.
fn array(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Error)> {
    if let Err(e) = bracketed(parser, |parser| {
        parser.delimited(
            Some(Token::Comma),
            UNTIL_CLOSE_BRACKET,
            EXPR_RECOVERY_SET,
            expr,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralArrayNode))
}

/// Parses a map literal expression.
fn map(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Error)> {
    if let Err(e) = braced(parser, |parser| {
        parser.delimited(
            Some(Token::Comma),
            UNTIL_CLOSE_BRACE,
            MAP_RECOVERY_SET,
            map_item,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralMapNode))
}

/// Parses a single item in a literal map.
fn map_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    expected_fn!(parser, marker, expr);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::LiteralMapItemNode);
    Ok(())
}

/// Parses a pair literal or parenthesized expression.
fn pair_or_paren_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Error)> {
    let opening = match parser.expect(Token::OpenParen) {
        Ok(span) => span,
        Err(e) => return Err((marker, e)),
    };

    expected_fn!(parser, marker, expr);

    if parser.next_if(Token::CloseParen) {
        // This was actually a parenthesized expression.
        return Ok(marker.complete(parser, SyntaxKind::ParenthesizedExprNode));
    }

    // At this point, it must be a pair literal
    expected!(parser, marker, Token::Comma);
    expected_fn!(parser, marker, expr);

    match parser.next() {
        Some((Token::CloseParen, _)) => Ok(marker.complete(parser, SyntaxKind::LiteralPairNode)),
        Some((token, span)) => Err((
            marker,
            Error::UnmatchedParen {
                found: Some(token.into_raw()),
                span,
                describe: Token::describe,
                opening,
            },
        )),
        None => {
            let span = parser.span();
            Err((
                marker,
                Error::UnmatchedParen {
                    found: None,
                    span,
                    describe: Token::describe,
                    opening,
                },
            ))
        }
    }
}

/// Parses an object literal expression.
fn object(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Error)> {
    parser.require(Token::ObjectKeyword);

    if let Err(e) = braced(parser, |parser| {
        parser.delimited(
            Some(Token::Comma),
            UNTIL_CLOSE_BRACE,
            LITERAL_OBJECT_RECOVERY_SET,
            object_item,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralObjectNode))
}

/// Parses a single item in a literal object.
fn object_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    expected!(parser, marker, Token::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::LiteralObjectItemNode);
    Ok(())
}

/// Parses a literal struct or a name reference.
fn literal_struct_or_name_ref(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Error)> {
    parser.require(Token::Ident);

    if !matches!(parser.peek(), Some((Token::OpenBrace, _)) | None) {
        // This is actually a name reference.
        return Ok(marker.complete(parser, SyntaxKind::NameReferenceNode));
    }

    if let Err(e) = braced(parser, |parser| {
        parser.delimited(
            Some(Token::Comma),
            UNTIL_CLOSE_BRACE,
            LITERAL_OBJECT_RECOVERY_SET, // same as literal objects
            literal_struct_item,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralStructNode))
}

/// Parses a single item in a literal struct.
fn literal_struct_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Error)> {
    expected!(parser, marker, Token::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::LiteralStructItemNode);
    Ok(())
}

/// Parses an `if` expression.
fn if_expr(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Error)> {
    parser.require(Token::IfKeyword);
    expected_fn!(parser, marker, expr);
    expected!(parser, marker, Token::ThenKeyword);
    expected_fn!(parser, marker, expr);
    expected!(parser, marker, Token::ElseKeyword);
    expected_fn!(parser, marker, expr);
    Ok(marker.complete(parser, SyntaxKind::IfExprNode))
}

/// Parses a call expression.
fn call_expr(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Error)> {
    if let Err(e) = paren(parser, |parser| {
        parser.delimited(
            Some(Token::Comma),
            UNTIL_CLOSE_PAREN,
            EXPR_RECOVERY_SET,
            expr,
        );
        Ok(())
    }) {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::CallExprNode))
}

/// Parses an index expression.
fn index_expr(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Error)> {
    if let Err(e) = bracketed(parser, |parser| {
        expected_fn!(parser, expr);
        Ok(())
    }) {
        return Err((marker, e));
    }
    Ok(marker.complete(parser, SyntaxKind::IndexExprNode))
}

/// Parses an access expression.
fn access_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Error)> {
    parser.require(Token::Dot);
    expected!(parser, marker, Token::Ident);
    Ok(marker.complete(parser, SyntaxKind::AccessExprNode))
}

/// An operator associativity.
enum Associativity {
    /// The operator has left-associativity.
    Left,
    /// The operator has right-associativity.
    Right,
}

/// Determines the precedence of a prefix operator.
///
/// See: https://github.com/openwdl/wdl/blob/wdl-1.1/SPEC.md#operator-precedence-table
fn prefix_precedence(token: Token) -> (u8, SyntaxKind, Associativity) {
    use Associativity::*;
    use SyntaxKind::*;
    match token {
        Token::Exclamation => (7, LogicalNotExprNode, Right),
        Token::Minus => (7, NegationExprNode, Right),
        // As paren expression is ambiguous with a pair literal expression,
        // this is handled in `atom_expr`
        // Token::OpenParen => 11,
        _ => panic!("unknown prefix operator token"),
    }
}

/// Determines the precedence of an infix operator.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.1/SPEC.md#operator-precedence-table
fn infix_precedence(token: Token) -> (u8, SyntaxKind, Associativity) {
    use Associativity::*;
    use SyntaxKind::*;
    match token {
        Token::LogicalOr => (1, LogicalOrExprNode, Left),
        Token::LogicalAnd => (2, LogicalAndExprNode, Left),
        Token::Equal => (3, EqualityExprNode, Left),
        Token::NotEqual => (3, InequalityExprNode, Left),
        Token::Less => (4, LessExprNode, Left),
        Token::LessEqual => (4, LessEqualExprNode, Left),
        Token::Greater => (4, GreaterExprNode, Left),
        Token::GreaterEqual => (4, GreaterEqualExprNode, Left),
        Token::Plus => (5, AdditionExprNode, Left),
        Token::Minus => (5, SubtractionExprNode, Left),
        Token::Asterisk => (6, MultiplicationExprNode, Left),
        Token::Slash => (6, DivisionExprNode, Left),
        Token::Percent => (6, ModuloExprNode, Left),
        _ => panic!("unknown infix operator token"),
    }
}

/// Determines the precedence of a postfix operator.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.1/SPEC.md#operator-precedence-table
fn postfix_precedence(token: Token) -> u8 {
    // All postfix operators are left-associative
    match token {
        Token::OpenParen => 8,
        Token::OpenBracket => 9,
        Token::Dot => 10,
        _ => panic!("unknown postfix operator token"),
    }
}
