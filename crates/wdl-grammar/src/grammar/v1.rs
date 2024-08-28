//! Module for the V1 grammar functions.

use super::macros::expected;
use super::macros::expected_fn;
use crate::grammar::macros::expected_in;
use crate::lexer::v1::BraceCommandToken;
use crate::lexer::v1::DQStringToken;
use crate::lexer::v1::HeredocToken;
use crate::lexer::v1::SQStringToken;
use crate::lexer::v1::Token;
use crate::lexer::TokenSet;
use crate::parser;
use crate::parser::expected_found;
use crate::parser::expected_one_of;
use crate::parser::unmatched;
use crate::parser::unterminated_braced_command;
use crate::parser::unterminated_heredoc;
use crate::parser::unterminated_string;
use crate::parser::CompletedMarker;
use crate::parser::Event;
use crate::parser::Interpolator;
use crate::parser::Marker;
use crate::parser::ParserToken;
use crate::tree::SyntaxKind;
use crate::Diagnostic;
use crate::Span;

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
    Token::DirectoryTypeKeyword as u8,
]);

/// A set of tokens for all types.
const TYPE_EXPECTED_SET: TokenSet = PRIMITIVE_TYPE_SET.union(TokenSet::new(&[
    Token::MapTypeKeyword as u8,
    Token::ArrayTypeKeyword as u8,
    Token::PairTypeKeyword as u8,
    Token::ObjectTypeKeyword as u8,
    Token::Ident as u8,
]));

/// The recovery set for struct items.
const STRUCT_ITEM_RECOVERY_SET: TokenSet = TYPE_EXPECTED_SET.union(TokenSet::new(&[
    Token::MetaKeyword as u8,
    Token::ParameterMetaKeyword as u8,
    Token::CloseBrace as u8,
]));

/// The recovery set for input items.
const INPUT_ITEM_RECOVERY_SET: TokenSet =
    TYPE_EXPECTED_SET.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set for output items.
const OUTPUT_ITEM_RECOVERY_SET: TokenSet =
    TYPE_EXPECTED_SET.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set for runtime items.
const RUNTIME_ITEM_RECOVERY_SET: TokenSet =
    ANY_IDENT.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set for requirements items.
const REQUIREMENTS_ITEM_RECOVERY_SET: TokenSet =
    ANY_IDENT.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set for hints items.
const HINTS_ITEM_RECOVERY_SET: TokenSet =
    ANY_IDENT.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set for literal input items.
const LITERAL_INPUT_ITEM_RECOVERY_SET: TokenSet =
    ANY_IDENT.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set for literal output items.
const LITERAL_OUTPUT_ITEM_RECOVERY_SET: TokenSet =
    ANY_IDENT.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The expected names of items in a struct definition.
const STRUCT_ITEM_EXPECTED_NAMES: &[&str] = &[
    "metadata section",
    "parameter metadata section",
    "struct member declaration",
];

/// The expected set of tokens in a task definition.
const TASK_ITEM_EXPECTED_SET: TokenSet = TYPE_EXPECTED_SET.union(TokenSet::new(&[
    Token::InputKeyword as u8,
    Token::CommandKeyword as u8,
    Token::OutputKeyword as u8,
    Token::RequirementsKeyword as u8,
    Token::HintsKeyword as u8,
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
    Token::HintsKeyword as u8,
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
    "call statement",
    "private declaration",
];

/// The recovery set of tokens in a workflow definition.
const WORKFLOW_ITEM_RECOVERY_SET: TokenSet =
    WORKFLOW_ITEM_EXPECTED_SET.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set for workflow statements.
const WORKFLOW_STATEMENT_RECOVERY_SET: TokenSet = TokenSet::new(&[
    Token::IfKeyword as u8,
    Token::CallKeyword as u8,
    Token::ScatterKeyword as u8,
    Token::CloseBrace as u8,
]);

/// The recovery set for input items in a call statement.
const CALL_INPUT_ITEM_RECOVERY_SET: TokenSet = ANY_IDENT.union(TokenSet::new(&[
    Token::Comma as u8,
    Token::CloseBrace as u8,
]));

/// The expected token set for metadata values.
const METADATA_VALUE_EXPECTED_SET: TokenSet = TokenSet::new(&[
    Token::Minus as u8,
    Token::Integer as u8,
    Token::Float as u8,
    Token::SingleQuote as u8,
    Token::DoubleQuote as u8,
    Token::OpenHeredoc as u8,
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
    ANY_IDENT.union(TokenSet::new(&[Token::CloseBrace as u8]));

/// The recovery set of tokens in a metadata object.
const METADATA_OBJECT_RECOVERY_SET: TokenSet =
    METADATA_SECTION_RECOVERY_SET.union(TokenSet::new(&[
        Token::Comma as u8,
        Token::CloseBrace as u8,
    ]));

/// The recovery set of tokens in a metadata array.
const METADATA_ARRAY_RECOVERY_SET: TokenSet = METADATA_VALUE_EXPECTED_SET.union(TokenSet::new(&[
    Token::Comma as u8,
    Token::CloseBracket as u8,
]));

/// A token set for expression atoms.
const ATOM_EXPECTED_SET: TokenSet = ANY_IDENT.union(TokenSet::new(&[
    Token::Integer as u8,
    Token::Float as u8,
    Token::TrueKeyword as u8,
    Token::FalseKeyword as u8,
    Token::SingleQuote as u8,
    Token::DoubleQuote as u8,
    Token::OpenHeredoc as u8,
    Token::OpenBracket as u8,
    Token::OpenBrace as u8,
    Token::OpenParen as u8,
    Token::ObjectKeyword as u8,
    Token::IfKeyword as u8,
    Token::NoneKeyword as u8,
    Token::HintsKeyword as u8,
    Token::InputKeyword as u8,
    Token::OutputKeyword as u8,
]));

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
    Token::Exponentiation as u8,
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

/// Represents *any* identifier, including reserved keywords.
const ANY_IDENT: TokenSet = TokenSet::new(&[
    Token::Ident as u8,
    Token::ArrayTypeKeyword as u8,
    Token::BooleanTypeKeyword as u8,
    Token::DirectoryTypeKeyword as u8,
    Token::FileTypeKeyword as u8,
    Token::FloatTypeKeyword as u8,
    Token::IntTypeKeyword as u8,
    Token::MapTypeKeyword as u8,
    Token::ObjectTypeKeyword as u8,
    Token::PairTypeKeyword as u8,
    Token::StringTypeKeyword as u8,
    Token::AfterKeyword as u8,
    Token::AliasKeyword as u8,
    Token::AsKeyword as u8,
    Token::CallKeyword as u8,
    Token::CommandKeyword as u8,
    Token::ElseKeyword as u8,
    Token::FalseKeyword as u8,
    Token::HintsKeyword as u8,
    Token::IfKeyword as u8,
    Token::InKeyword as u8,
    Token::ImportKeyword as u8,
    Token::InputKeyword as u8,
    Token::MetaKeyword as u8,
    Token::NoneKeyword as u8,
    Token::NullKeyword as u8,
    Token::ObjectKeyword as u8,
    Token::OutputKeyword as u8,
    Token::ParameterMetaKeyword as u8,
    Token::RequirementsKeyword as u8,
    Token::HintsKeyword as u8,
    Token::RuntimeKeyword as u8,
    Token::ScatterKeyword as u8,
    Token::StructKeyword as u8,
    Token::TaskKeyword as u8,
    Token::ThenKeyword as u8,
    Token::TrueKeyword as u8,
    Token::VersionKeyword as u8,
    Token::WorkflowKeyword as u8,
]);

/// Parses matching braces given a callback to parse the interior delimited
/// items.
macro_rules! braced_items {
    ($parser:ident, $marker:ident, $delimiter:expr, $recovery:expr, $cb:expr) => {
        if let Err(e) = $parser.matching_delimited(
            Token::OpenBrace,
            Token::CloseBrace,
            $delimiter,
            $recovery,
            $cb,
        ) {
            return Err(($marker, e));
        }
    };
}

/// Parses matching brackets given a callback to parse the interior delimited
/// items.
macro_rules! bracketed_items {
    ($parser:ident, $marker:ident, $delimiter:expr, $recovery:expr, $cb:expr) => {
        if let Err(e) = $parser.matching_delimited(
            Token::OpenBracket,
            Token::CloseBracket,
            $delimiter,
            $recovery,
            $cb,
        ) {
            return Err(($marker, e));
        }
    };
}

/// Parses matching parens given a callback to parse the interior delimited
/// items.
macro_rules! paren_items {
    ($parser:ident, $marker:ident, $delimiter:expr, $recovery:expr, $cb:expr) => {
        if let Err(e) = $parser.matching_delimited(
            Token::OpenParen,
            Token::CloseParen,
            $delimiter,
            $recovery,
            $cb,
        ) {
            return Err(($marker, e));
        }
    };
}

/// Parses matching brackets given a callback to parse the interior.
macro_rules! bracketed {
    ($parser:ident, $marker:ident, $cb:expr) => {
        if let Err(e) = $parser.matching(Token::OpenBracket, Token::CloseBracket, $cb) {
            return Err(($marker, e));
        }
    };
}

/// Parses matching parenthesis given a callback to parse the interior.
macro_rules! paren {
    ($parser:ident, $marker:ident, $cb:expr) => {
        if let Err(e) = $parser.matching(Token::OpenParen, Token::CloseParen, $cb) {
            return Err(($marker, e));
        }
    };
}

/// Parses the top-level items of a V1 document.
///
/// It is expected that the version statement has already been parsed.
pub fn items(parser: &mut Parser<'_>) {
    parser.push_recovery_set(TOP_RECOVERY_SET);

    while parser.peek().is_some() {
        let marker = parser.start();
        if let Err((marker, e)) = item(parser, marker) {
            parser.recover(e);
            marker.abandon(parser);
        }
    }

    parser.pop_recovery_set();

    // This call to `next` is important as `next` adds any remaining buffered events
    assert!(parser.next().is_none(), "parser is not finished");
}

/// Parses a single top-level item in a WDL document.
fn item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    match parser.peek() {
        Some((Token::ImportKeyword, _)) => import_statement(parser, marker),
        Some((Token::StructKeyword, _)) => struct_definition(parser, marker),
        Some((Token::TaskKeyword, _)) => task_definition(parser, marker),
        Some((Token::WorkflowKeyword, _)) => workflow_definition(parser, marker),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((marker, expected_one_of(TOP_EXPECTED_NAMES, found, span)))
        }
    }
}

/// Parses an import statement.
fn import_statement(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::ImportKeyword);
    expected_fn!(parser, marker, string);

    if parser.next_if(Token::AsKeyword) {
        expected!(parser, marker, Token::Ident, "import namespace");
    }

    while let Some((Token::AliasKeyword, _)) = parser.peek() {
        expected_fn!(parser, marker, import_alias);
    }

    marker.complete(parser, SyntaxKind::ImportStatementNode);
    Ok(())
}

/// Parses an import alias.
fn import_alias(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::AliasKeyword);
    expected!(parser, marker, Token::Ident, "source type name");
    expected!(parser, marker, Token::AsKeyword);
    expected!(parser, marker, Token::Ident, "target type name");
    marker.complete(parser, SyntaxKind::ImportAliasNode);
    Ok(())
}

/// Parses a struct definition.
fn struct_definition(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::StructKeyword);
    expected!(parser, marker, Token::Ident, "struct name");
    braced_items!(parser, marker, None, STRUCT_ITEM_RECOVERY_SET, struct_item);
    marker.complete(parser, SyntaxKind::StructDefinitionNode);
    Ok(())
}

/// Parses an item in a struct definition.
fn struct_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    match parser.peek() {
        Some((Token::MetaKeyword, _)) => metadata_section(parser, marker),
        Some((Token::ParameterMetaKeyword, _)) => parameter_metadata_section(parser, marker),
        Some((t, _)) if TYPE_EXPECTED_SET.contains(t.into_raw()) => {
            struct_member_decl(parser, marker)
        }
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                expected_one_of(STRUCT_ITEM_EXPECTED_NAMES, found, span),
            ))
        }
    }
}

/// Parses a struct member declaration.
fn struct_member_decl(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_fn!(parser, marker, ty);
    expected_in!(parser, marker, ANY_IDENT, "struct member name");
    parser.update_last_token_kind(SyntaxKind::Ident);
    marker.complete(parser, SyntaxKind::UnboundDeclNode);
    Ok(())
}

/// Parses a task definition.
fn task_definition(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::TaskKeyword);
    expected!(parser, marker, Token::Ident, "task name");
    braced_items!(parser, marker, None, TASK_ITEM_RECOVERY_SET, task_item);
    marker.complete(parser, SyntaxKind::TaskDefinitionNode);
    Ok(())
}

/// Parses a workflow definition.
fn workflow_definition(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::WorkflowKeyword);
    expected!(parser, marker, Token::Ident, "workflow name");
    braced_items!(
        parser,
        marker,
        None,
        WORKFLOW_ITEM_RECOVERY_SET,
        workflow_item
    );
    marker.complete(parser, SyntaxKind::WorkflowDefinitionNode);
    Ok(())
}

/// Parses a type used in a declaration.
fn ty(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    match parser.peek() {
        Some((Token::MapTypeKeyword, _)) => map_type(parser, marker),
        Some((Token::ArrayTypeKeyword, _)) => array_type(parser, marker),
        Some((Token::PairTypeKeyword, _)) => pair_type(parser, marker),
        Some((Token::ObjectTypeKeyword, _)) => object_type(parser, marker),
        Some((Token::Ident, _)) => type_ref(parser, marker),
        Some((t, _)) if PRIMITIVE_TYPE_SET.contains(t.into_raw()) => primitive_type(parser, marker),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((marker, expected_found("type", found, span)))
        }
    }
}

/// Parses a map type used in a declaration.
fn map_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    /// Parses the inner part of the brackets
    fn parse(parser: &mut Parser<'_>, _: Span) -> Result<(), Diagnostic> {
        expected_fn!(parser, primitive_type);
        parser.expect(Token::Comma)?;
        expected_fn!(parser, ty);
        Ok(())
    }

    parser.require(Token::MapTypeKeyword);
    bracketed!(parser, marker, parse);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::MapTypeNode);
    Ok(())
}

/// Parses a array type used in a declaration.
fn array_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    /// Parses the inner part of the brackets
    fn parse(parser: &mut Parser<'_>, _: Span) -> Result<(), Diagnostic> {
        expected_fn!(parser, ty);
        Ok(())
    }

    parser.require(Token::ArrayTypeKeyword);
    bracketed!(parser, marker, parse);
    parser.next_if(Token::Plus);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::ArrayTypeNode);
    Ok(())
}

/// Parses a pair type used in a declaration.
fn pair_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    /// Parses the inner part of the brackets
    fn parse(parser: &mut Parser<'_>, _: Span) -> Result<(), Diagnostic> {
        expected_fn!(parser, ty);
        parser.expect(Token::Comma)?;
        expected_fn!(parser, ty);
        Ok(())
    }

    parser.require(Token::PairTypeKeyword);
    bracketed!(parser, marker, parse);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::PairTypeNode);
    Ok(())
}

/// Parses an object type used in a declaration.
fn object_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::ObjectTypeKeyword);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::ObjectTypeNode);
    Ok(())
}

/// Parses a type reference used in a declaration.
fn type_ref(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::Ident);
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::TypeRefNode);
    Ok(())
}

/// Parses a primitive type used in a declaration.
fn primitive_type(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(
        parser,
        marker,
        PRIMITIVE_TYPE_SET,
        "Boolean",
        "Directory",
        "File",
        "Float",
        "Int",
        "String",
    );
    parser.next_if(Token::QuestionMark);
    marker.complete(parser, SyntaxKind::PrimitiveTypeNode);
    Ok(())
}

/// Parses an item in a task definition.
fn task_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    match parser.peek() {
        Some((Token::InputKeyword, _)) => input_section(parser, marker),
        Some((Token::CommandKeyword, _)) => command_section(parser, marker),
        Some((Token::OutputKeyword, _)) => output_section(parser, marker),
        Some((Token::RuntimeKeyword, _)) => runtime_section(parser, marker),
        Some((Token::RequirementsKeyword, _)) => requirements_section(parser, marker),
        Some((Token::HintsKeyword, _)) => hints_section(parser, marker),
        Some((Token::MetaKeyword, _)) => metadata_section(parser, marker),
        Some((Token::ParameterMetaKeyword, _)) => parameter_metadata_section(parser, marker),
        Some((t, _)) if TYPE_EXPECTED_SET.contains(t.into_raw()) => {
            bound_decl(parser, marker, false)
        }
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                expected_one_of(TASK_ITEM_EXPECTED_NAMES, found, span),
            ))
        }
    }
}

/// Parses an item in a workflow definition.
fn workflow_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    match parser.peek() {
        Some((Token::InputKeyword, _)) => input_section(parser, marker),
        Some((Token::OutputKeyword, _)) => output_section(parser, marker),
        Some((Token::MetaKeyword, _)) => metadata_section(parser, marker),
        Some((Token::ParameterMetaKeyword, _)) => parameter_metadata_section(parser, marker),
        Some((Token::HintsKeyword, _)) => hints_section(parser, marker),
        Some((Token::IfKeyword, _)) => conditional_statement(parser, marker),
        Some((Token::ScatterKeyword, _)) => scatter_statement(parser, marker),
        Some((Token::CallKeyword, _)) => call_statement(parser, marker),
        Some((t, _)) if TYPE_EXPECTED_SET.contains(t.into_raw()) => {
            bound_decl(parser, marker, false)
        }
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                expected_one_of(WORKFLOW_ITEM_EXPECTED_NAMES, found, span),
            ))
        }
    }
}

/// Parses a workflow statement.
fn workflow_statement(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    match parser.peek() {
        Some((Token::IfKeyword, _)) => conditional_statement(parser, marker),
        Some((Token::ScatterKeyword, _)) => scatter_statement(parser, marker),
        Some((Token::CallKeyword, _)) => call_statement(parser, marker),
        Some((t, _)) if TYPE_EXPECTED_SET.contains(t.into_raw()) => {
            bound_decl(parser, marker, false)
        }
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((marker, expected_found("workflow statement", found, span)))
        }
    }
}

/// Parses an input section in a task or workflow.
fn input_section(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::InputKeyword);
    braced_items!(parser, marker, None, INPUT_ITEM_RECOVERY_SET, input_item);
    marker.complete(parser, SyntaxKind::InputSectionNode);
    Ok(())
}

/// Parses an input item.
fn input_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_fn!(parser, marker, ty);
    expected_in!(parser, marker, ANY_IDENT, "input name");
    parser.update_last_token_kind(SyntaxKind::Ident);

    let kind = if parser.next_if(Token::Assignment) {
        expected_fn!(parser, marker, expr);
        SyntaxKind::BoundDeclNode
    } else {
        SyntaxKind::UnboundDeclNode
    };

    marker.complete(parser, kind);
    Ok(())
}

/// Parses a command section in a task.
fn command_section(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::CommandKeyword);

    // Check to see if this is a "braced" command
    if let Some((Token::OpenBrace, _)) = parser.peek() {
        let start = parser.next().expect("should have token").1;
        if let Err(e) =
            parser.interpolate(|interpolator| interpolate_brace_command(start, interpolator))
        {
            return Err((marker, e));
        }
    } else {
        // Not a "braced" command, so it should be a "heredoc" command.
        let start = match parser.expect(Token::OpenHeredoc) {
            Ok(span) => span,
            Err(e) => return Err((marker, e)),
        };

        if let Err(e) = parser.interpolate(|interpolator| {
            interpolate_heredoc(start, HeredocContext::Command, interpolator)
        }) {
            return Err((marker, e));
        }
    }

    marker.complete(parser, SyntaxKind::CommandSectionNode);
    Ok(())
}

/// Interpolates a brace command.
fn interpolate_brace_command(
    start: Span,
    mut interpolator: Interpolator<'_, BraceCommandToken>,
) -> (Parser<'_>, Result<(), Diagnostic>) {
    let mut text = None;
    let mut end = None;

    while let Some((Ok(token), span)) = interpolator.next() {
        match token {
            BraceCommandToken::PlaceholderStart => {
                // Add any encountered literal text
                if let Some(span) = text.take() {
                    interpolator.event(Event::Token {
                        kind: SyntaxKind::LiteralCommandText,
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
                    marker.abandon(&mut parser);
                    parser.recover_with_set(e, TokenSet::new(&[Token::CloseBrace as u8]));
                    parser.next_if(Token::CloseBrace);
                }

                interpolator = parser.into_interpolator();
            }
            BraceCommandToken::Escape
            | BraceCommandToken::Text
            | BraceCommandToken::DollarSign
            | BraceCommandToken::Tilde => {
                // Update the span of the text to include this token
                text = match text {
                    Some(prev) => Some(Span::new(prev.start(), prev.len() + span.len())),
                    None => Some(span),
                };
            }
            BraceCommandToken::End => {
                end = Some(span);
                break;
            }
        }
    }

    if let Some(span) = text.take() {
        interpolator.event(Event::Token {
            kind: SyntaxKind::LiteralCommandText,
            span,
        })
    }

    match end {
        Some(span) => {
            // Push an end brace as we're done interpolating the command
            interpolator.event(Event::Token {
                kind: SyntaxKind::CloseBrace,
                span,
            });

            (interpolator.into_parser(), Ok(()))
        }
        None => {
            // Command wasn't terminated
            (
                interpolator.into_parser(),
                Err(unterminated_braced_command(
                    Token::OpenBrace.describe(),
                    start,
                )),
            )
        }
    }
}

/// Represents context for a heredoc interpolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeredocContext {
    /// A heredoc command is being interpolated.
    Command,
    /// A multiline string is being interpolated.
    String,
    /// A multiline string in a metadata section is being interpolated.
    MetadataString,
}

/// Interpolates a heredoc command or multi-line string.
pub(crate) fn interpolate_heredoc(
    start: Span,
    context: HeredocContext,
    mut interpolator: Interpolator<'_, HeredocToken>,
) -> (Parser<'_>, Result<(), Diagnostic>) {
    /// Helper function for parsing an interpolation
    fn interpolate<'a>(
        mut interpolator: Interpolator<'a, HeredocToken>,
        open: Span,
        text: &mut Option<Span>,
        context: HeredocContext,
    ) -> Interpolator<'a, HeredocToken> {
        // Add any encountered literal text
        if let Some(span) = text.take() {
            interpolator.event(Event::Token {
                kind: if context == HeredocContext::Command {
                    SyntaxKind::LiteralCommandText
                } else {
                    SyntaxKind::LiteralStringText
                },
                span,
            })
        }

        let marker = interpolator.start();
        interpolator.event(Event::Token {
            kind: SyntaxKind::PlaceholderOpen,
            span: open,
        });

        // Parse the placeholder expression
        let mut parser = interpolator.into_parser();
        if let Err((marker, e)) = placeholder_expr(&mut parser, marker, open) {
            marker.abandon(&mut parser);
            parser.recover_with_set(
                e,
                TokenSet::new(&[Token::CloseBrace as u8, Token::CloseHeredoc as u8]),
            );
            parser.next_if(Token::CloseBrace);
        }

        parser.into_interpolator()
    }

    let mut text = None;
    let mut end = None;

    while let Some((Ok(token), span)) = interpolator.next() {
        match token {
            HeredocToken::TildePlaceholderStart
                if matches!(context, HeredocContext::Command | HeredocContext::String) =>
            {
                interpolator = interpolate(interpolator, span, &mut text, context);
            }
            HeredocToken::DollarPlaceholderStart if context == HeredocContext::String => {
                interpolator = interpolate(interpolator, span, &mut text, context);
            }
            HeredocToken::Escape
            | HeredocToken::Text
            | HeredocToken::SingleCloseAngle
            | HeredocToken::DoubleCloseAngle
            | HeredocToken::Tilde
            | HeredocToken::DollarSign
            | HeredocToken::TildePlaceholderStart
            | HeredocToken::DollarPlaceholderStart => {
                // Update the span of the text to include this token
                text = match text {
                    Some(prev) => Some(Span::new(prev.start(), prev.len() + span.len())),
                    None => Some(span),
                };
            }
            HeredocToken::End => {
                end = Some(span);
                break;
            }
        }
    }

    if let Some(span) = text.take() {
        interpolator.event(Event::Token {
            kind: if context == HeredocContext::Command {
                SyntaxKind::LiteralCommandText
            } else {
                SyntaxKind::LiteralStringText
            },
            span,
        })
    }

    match end {
        Some(span) => {
            // Push a close heredoc as we're done interpolating
            interpolator.event(Event::Token {
                kind: SyntaxKind::CloseHeredoc,
                span,
            });

            (interpolator.into_parser(), Ok(()))
        }
        None => {
            let span = Span::new(interpolator.span().start(), 0);

            // Synthesize a close token
            interpolator.event(Event::Token {
                kind: SyntaxKind::CloseHeredoc,
                span,
            });

            // Not terminated
            (
                interpolator.into_parser(),
                Err(unterminated_heredoc(
                    Token::OpenHeredoc.describe(),
                    start,
                    context == HeredocContext::Command,
                )),
            )
        }
    }
}

/// Parses an output section in a task or workflow.
fn output_section(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::OutputKeyword);
    braced_items!(
        parser,
        marker,
        None,
        OUTPUT_ITEM_RECOVERY_SET,
        |parser, marker| bound_decl(parser, marker, true)
    );
    marker.complete(parser, SyntaxKind::OutputSectionNode);
    Ok(())
}

/// Parses a runtime section in a task.
fn runtime_section(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::RuntimeKeyword);
    braced_items!(
        parser,
        marker,
        None,
        RUNTIME_ITEM_RECOVERY_SET,
        runtime_item
    );
    marker.complete(parser, SyntaxKind::RuntimeSectionNode);
    Ok(())
}

/// Parses an item in a runtime section.
fn runtime_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "runtime key");
    parser.update_last_token_kind(SyntaxKind::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::RuntimeItemNode);
    Ok(())
}

/// Parses a requirements section in a task.
fn requirements_section(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::RequirementsKeyword);
    braced_items!(
        parser,
        marker,
        None,
        REQUIREMENTS_ITEM_RECOVERY_SET,
        requirements_item
    );
    marker.complete(parser, SyntaxKind::RequirementsSectionNode);
    Ok(())
}

/// Parses an item in a requirements section.
fn requirements_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "requirements key");
    parser.update_last_token_kind(SyntaxKind::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::RequirementsItemNode);
    Ok(())
}

/// Parses a hints section in a task.
fn hints_section(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::HintsKeyword);
    braced_items!(parser, marker, None, HINTS_ITEM_RECOVERY_SET, hints_item);
    marker.complete(parser, SyntaxKind::HintsSectionNode);
    Ok(())
}

/// Parses an item in a hints section or hints literal.
fn hints_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "hints key");
    parser.update_last_token_kind(SyntaxKind::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::HintsItemNode);
    Ok(())
}

/// Parses a metadata section in a task or workflow.
fn metadata_section(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::MetaKeyword);
    braced_items!(
        parser,
        marker,
        None,
        METADATA_SECTION_RECOVERY_SET,
        metadata_object_item
    );
    marker.complete(parser, SyntaxKind::MetadataSectionNode);
    Ok(())
}

/// Parses an item in a metadata object.
fn metadata_object_item(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "metadata key");
    parser.update_last_token_kind(SyntaxKind::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, metadata_value);
    marker.complete(parser, SyntaxKind::MetadataObjectItemNode);
    Ok(())
}

/// Parses a metadata value.
fn metadata_value(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    match parser.peek() {
        Some((Token::Minus, _)) | Some((Token::Integer, _)) | Some((Token::Float, _)) => {
            number(parser, marker, true)?;
            Ok(())
        }
        Some((Token::SingleQuote, _)) => {
            single_quote_string(parser, marker, false)?;
            Ok(())
        }
        Some((Token::DoubleQuote, _)) => {
            double_quote_string(parser, marker, false)?;
            Ok(())
        }
        Some((Token::OpenHeredoc, _)) => {
            multiline_string(parser, marker, false)?;
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
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((
                marker,
                expected_one_of(METADATA_VALUE_EXPECTED_NAMES, found, span),
            ))
        }
    }
}

/// Parses a literal `None` value.
fn none(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    parser.require(Token::NoneKeyword);
    Ok(marker.complete(parser, SyntaxKind::LiteralNoneNode))
}

/// Parses a number.
fn number(
    parser: &mut Parser<'_>,
    marker: Marker,
    accept_minus: bool,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
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

/// Parses placeholder options.
fn placeholder_options(
    parser: &mut Parser<'_>,
    mut marker: Marker,
) -> Result<(), (Marker, Diagnostic)> {
    loop {
        if let Some(peek) = parser.peek2() {
            match (peek.first, peek.second) {
                ((Token::Ident, span), (Token::Assignment, _)) => {
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
                    marker = parser.start();
                    continue;
                }
                ((t @ Token::TrueKeyword, _), (Token::Assignment, _))
                | ((t @ Token::FalseKeyword, _), (Token::Assignment, _)) => {
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
                    marker = parser.start();
                    continue;
                }
                _ => {
                    // Not a placeholder, fallthrough to below
                }
            }
        }

        // Not a placeholder option
        marker.abandon(parser);
        return Ok(());
    }
}

/// Parses a placeholder expression.
fn placeholder_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
    open_span: Span,
) -> Result<(), (Marker, Diagnostic)> {
    expected_fn!(parser, marker, placeholder_options);
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
            let (found, span) = found
                .map(|(t, s)| (t.describe(), s))
                .unwrap_or_else(|| ("end of input", parser.span()));
            Err((
                marker,
                unmatched(
                    "placeholder start",
                    open_span,
                    Token::CloseBrace.describe(),
                    found,
                    span,
                ),
            ))
        }
    }
}

/// Interpolates a single-quoted string.
pub(crate) fn interpolate_sq_string(
    start: Span,
    allow_placeholders: bool,
    mut interpolator: Interpolator<'_, SQStringToken>,
) -> (Parser<'_>, Result<(), Diagnostic>) {
    let mut text = None;
    let mut end = None;

    while let Some((Ok(token), span)) = interpolator.next() {
        match token {
            SQStringToken::PlaceholderStart if allow_placeholders => {
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
                    marker.abandon(&mut parser);
                    parser.recover_with_set(
                        e,
                        TokenSet::new(&[Token::CloseBrace as u8, Token::SingleQuote as u8]),
                    );
                    parser.next_if(Token::CloseBrace);
                }

                interpolator = parser.into_interpolator();
            }
            SQStringToken::PlaceholderStart
            | SQStringToken::Escape
            | SQStringToken::Text
            | SQStringToken::DollarSign
            | SQStringToken::Tilde => {
                // Update the span of the text to include this token
                text = match text {
                    Some(prev) => Some(Span::new(prev.start(), prev.len() + span.len())),
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
            let span = Span::new(interpolator.span().start(), 0);

            // Synthesize a close token
            interpolator.event(Event::Token {
                kind: SyntaxKind::SingleQuote,
                span,
            });

            // String wasn't terminated
            (interpolator.into_parser(), Err(unterminated_string(start)))
        }
    }
}

/// Parses either a single-quote string or a double-quote string.
fn string(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    match parser.peek() {
        Some((Token::SingleQuote, _)) => single_quote_string(parser, marker, true),
        Some((Token::DoubleQuote, _)) => double_quote_string(parser, marker, true),
        Some((Token::OpenHeredoc, _)) => multiline_string(parser, marker, true),
        found => {
            let (found, span) = found
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            Err((marker, expected_found("string", found, span)))
        }
    }
}

/// Parses a single-quoted string.
fn single_quote_string(
    parser: &mut Parser<'_>,
    marker: Marker,
    allow_placeholders: bool,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    let start = parser.require(Token::SingleQuote);

    if let Err(e) = parser.interpolate(|i| interpolate_sq_string(start, allow_placeholders, i)) {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralStringNode))
}

/// Interpolates a double-quoted string.
pub(crate) fn interpolate_dq_string(
    start: Span,
    allow_placeholders: bool,
    mut interpolator: Interpolator<'_, DQStringToken>,
) -> (Parser<'_>, Result<(), Diagnostic>) {
    let mut text = None;
    let mut end = None;

    while let Some((Ok(token), span)) = interpolator.next() {
        match token {
            DQStringToken::PlaceholderStart if allow_placeholders => {
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
                    marker.abandon(&mut parser);
                    parser.recover_with_set(
                        e,
                        TokenSet::new(&[Token::CloseBrace as u8, Token::DoubleQuote as u8]),
                    );
                    parser.next_if(Token::CloseBrace);
                }

                interpolator = parser.into_interpolator();
            }
            DQStringToken::PlaceholderStart
            | DQStringToken::Escape
            | DQStringToken::Text
            | DQStringToken::DollarSign
            | DQStringToken::Tilde => {
                text = match text {
                    Some(prev) => Some(Span::new(prev.start(), prev.len() + span.len())),
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
            let span = Span::new(interpolator.span().start(), 0);

            // Synthesize a close token
            interpolator.event(Event::Token {
                kind: SyntaxKind::DoubleQuote,
                span,
            });

            // String wasn't terminated
            (interpolator.into_parser(), Err(unterminated_string(start)))
        }
    }
}

/// Parses a double-quoted string.
fn double_quote_string(
    parser: &mut Parser<'_>,
    marker: Marker,
    allow_placeholders: bool,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    let start = parser.require(Token::DoubleQuote);

    if let Err(e) = parser.interpolate(|i| interpolate_dq_string(start, allow_placeholders, i)) {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralStringNode))
}

/// Parses a multi-line string.
fn multiline_string(
    parser: &mut Parser<'_>,
    marker: Marker,
    allow_placeholders: bool,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    let start = parser.require(Token::OpenHeredoc);

    if let Err(e) = parser.interpolate(|i| {
        interpolate_heredoc(
            start,
            if allow_placeholders {
                HeredocContext::String
            } else {
                HeredocContext::MetadataString
            },
            i,
        )
    }) {
        return Err((marker, e));
    }

    Ok(marker.complete(parser, SyntaxKind::LiteralStringNode))
}

/// Parses a literal boolean.
fn boolean(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    parser.require_in(TokenSet::new(&[
        Token::TrueKeyword as u8,
        Token::FalseKeyword as u8,
    ]));

    Ok(marker.complete(parser, SyntaxKind::LiteralBooleanNode))
}

/// Parses a literal null.
fn null(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::NullKeyword);
    marker.complete(parser, SyntaxKind::LiteralNullNode);
    Ok(())
}

/// Parses a metadata object.
fn metadata_object(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    braced_items!(
        parser,
        marker,
        Some(Token::Comma),
        METADATA_OBJECT_RECOVERY_SET,
        metadata_object_item
    );
    marker.complete(parser, SyntaxKind::MetadataObjectNode);
    Ok(())
}

/// Parses a metadata array.
fn metadata_array(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    bracketed_items!(
        parser,
        marker,
        Some(Token::Comma),
        METADATA_ARRAY_RECOVERY_SET,
        metadata_value
    );
    marker.complete(parser, SyntaxKind::MetadataArrayNode);
    Ok(())
}

/// Parses a parameter metadata section in a task or workflow.
fn parameter_metadata_section(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::ParameterMetaKeyword);
    braced_items!(
        parser,
        marker,
        None,
        METADATA_SECTION_RECOVERY_SET,
        metadata_object_item
    );
    marker.complete(parser, SyntaxKind::ParameterMetadataSectionNode);
    Ok(())
}

/// Parses a bound declaration.
fn bound_decl(
    parser: &mut Parser<'_>,
    marker: Marker,
    output: bool,
) -> Result<(), (Marker, Diagnostic)> {
    expected_fn!(parser, marker, ty);

    if output {
        expected_in!(parser, marker, ANY_IDENT, "output name");
        parser.update_last_token_kind(SyntaxKind::Ident);
    } else {
        expected!(parser, marker, Token::Ident, "declaration name");
    }

    expected!(parser, marker, Token::Assignment);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::BoundDeclNode);
    Ok(())
}

/// Parses a conditional statement in a workflow.
fn conditional_statement(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::IfKeyword);
    paren!(parser, marker, |parser, _| {
        expected_fn!(parser, expr);
        Ok(())
    });
    braced_items!(
        parser,
        marker,
        None,
        WORKFLOW_STATEMENT_RECOVERY_SET,
        workflow_statement
    );
    marker.complete(parser, SyntaxKind::ConditionalStatementNode);
    Ok(())
}

/// Parses a scatter statement in a workflow.
fn scatter_statement(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::ScatterKeyword);
    paren!(parser, marker, |parser, _| {
        parser.expect_with_name(Token::Ident, "scatter variable name")?;
        parser.expect(Token::InKeyword)?;
        expected_fn!(parser, expr);
        Ok(())
    });
    braced_items!(
        parser,
        marker,
        None,
        WORKFLOW_STATEMENT_RECOVERY_SET,
        workflow_statement
    );
    marker.complete(parser, SyntaxKind::ScatterStatementNode);
    Ok(())
}

/// Parses a call statement in a workflow.
fn call_statement(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::CallKeyword);
    expected_fn!(parser, marker, call_target);

    if let Some((Token::AsKeyword, _)) = parser.peek() {
        expected_fn!(parser, marker, call_alias);
    }

    while let Some((Token::AfterKeyword, _)) = parser.peek() {
        expected_fn!(parser, marker, call_after_clause);
    }

    if let Some((Token::OpenBrace, _)) = parser.peek() {
        // Given the optional `input:` that we need to parse after the open brace, we
        // unfortunately can't use `Parser::matching_delimited` here
        let open_span = parser.require(Token::OpenBrace);

        if parser.next_if(Token::InputKeyword) {
            expected!(parser, marker, Token::Colon);
        }

        parser.delimited(
            Token::CloseBrace,
            Some(Token::Comma),
            CALL_INPUT_ITEM_RECOVERY_SET,
            call_input_item,
        );

        parser.consume_close_token(Token::OpenBrace, open_span, Token::CloseBrace);
    }

    marker.complete(parser, SyntaxKind::CallStatementNode);
    Ok(())
}

/// Parses a call target (i.e. a qualified name) in a call statement.
fn call_target(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected!(parser, marker, Token::Ident, "call target name");

    while parser.next_if(Token::Dot) {
        expected!(parser, marker, Token::Ident, "call target name");
    }

    marker.complete(parser, SyntaxKind::CallTargetNode);
    Ok(())
}

/// Parses an alias (i.e. `as` clause) in a call statement.
fn call_alias(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::AsKeyword);
    expected!(parser, marker, Token::Ident, "call output name");
    marker.complete(parser, SyntaxKind::CallAliasNode);
    Ok(())
}

/// Parses an `after` clause in a call statement.
fn call_after_clause(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    parser.require(Token::AfterKeyword);
    expected!(parser, marker, Token::Ident, "task name");
    marker.complete(parser, SyntaxKind::CallAfterNode);
    Ok(())
}

/// Parses a call input item.
fn call_input_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "call input key");
    parser.update_last_token_kind(SyntaxKind::Ident);

    if parser.next_if(Token::Assignment) {
        expected_fn!(parser, marker, expr);
    }

    marker.complete(parser, SyntaxKind::CallInputItemNode);
    Ok(())
}

/// Parses an expression.
#[inline]
fn expr(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
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
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
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
                .map(|(t, s)| (Some(t.describe()), s))
                .unwrap_or_else(|| (None, parser.span()));
            return Err((marker, expected_found("expression", found, span)));
        }
    };

    // Extend the parent chain of the left-hand side to the provided marker.
    lhs = lhs.extend_to(parser, marker);

    loop {
        // Check for either an infix or postfix operation
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
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    match peeked {
        Token::NoneKeyword => none(parser, marker),
        Token::Float | Token::Integer => number(parser, marker, false),
        Token::TrueKeyword | Token::FalseKeyword => boolean(parser, marker),
        Token::SingleQuote => single_quote_string(parser, marker, true),
        Token::DoubleQuote => double_quote_string(parser, marker, true),
        Token::OpenHeredoc => multiline_string(parser, marker, true),
        Token::OpenBracket => array(parser, marker),
        Token::OpenBrace => map(parser, marker),
        Token::OpenParen => pair_or_paren_expr(parser, marker),
        Token::ObjectKeyword => object(parser, marker),
        Token::IfKeyword => if_expr(parser, marker),
        Token::HintsKeyword => literal_hints(parser, marker),
        Token::InputKeyword => literal_input(parser, marker),
        Token::OutputKeyword => literal_output(parser, marker),
        t if ANY_IDENT.contains(t.into_raw()) => literal_struct_or_name_ref(parser, marker),
        _ => unreachable!(),
    }
}

/// Parses an array literal expression.
fn array(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    bracketed_items!(parser, marker, Some(Token::Comma), EXPR_RECOVERY_SET, expr);
    Ok(marker.complete(parser, SyntaxKind::LiteralArrayNode))
}

/// Parses a map literal expression.
fn map(parser: &mut Parser<'_>, marker: Marker) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    braced_items!(
        parser,
        marker,
        Some(Token::Comma),
        MAP_RECOVERY_SET,
        map_item
    );
    Ok(marker.complete(parser, SyntaxKind::LiteralMapNode))
}

/// Parses a single item in a literal map.
fn map_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
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
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    let open_span = match parser.expect(Token::OpenParen) {
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
        found => {
            let (found, span) = found
                .map(|(t, s)| (t.describe(), s))
                .unwrap_or_else(|| ("end of input", parser.span()));

            Err((
                marker,
                unmatched(
                    Token::OpenParen.describe(),
                    open_span,
                    Token::CloseParen.describe(),
                    found,
                    span,
                ),
            ))
        }
    }
}

/// Parses an object literal expression.
fn object(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    parser.require(Token::ObjectKeyword);
    braced_items!(
        parser,
        marker,
        Some(Token::Comma),
        LITERAL_OBJECT_RECOVERY_SET,
        object_item
    );
    Ok(marker.complete(parser, SyntaxKind::LiteralObjectNode))
}

/// Parses a single item in a literal object.
fn object_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "object key");
    parser.update_last_token_kind(SyntaxKind::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::LiteralObjectItemNode);
    Ok(())
}

/// Parses a literal struct or a name reference.
fn literal_struct_or_name_ref(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "identifier");
    parser.update_last_token_kind(SyntaxKind::Ident);

    // To disambiguate between a name reference and a struct literal,
    // peek ahead for `{`.
    if let Some((Token::OpenBrace, _)) = parser.peek() {
        braced_items!(
            parser,
            marker,
            Some(Token::Comma),
            LITERAL_OBJECT_RECOVERY_SET, // same as literal objects
            literal_struct_item
        );
        return Ok(marker.complete(parser, SyntaxKind::LiteralStructNode));
    }

    // This is a name reference.
    Ok(marker.complete(parser, SyntaxKind::NameRefNode))
}

/// Parses a single item in a literal struct.
fn literal_struct_item(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "struct member name");
    parser.update_last_token_kind(SyntaxKind::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::LiteralStructItemNode);
    Ok(())
}

/// Parses an `if` expression.
fn if_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    parser.require(Token::IfKeyword);
    expected_fn!(parser, marker, expr);
    expected!(parser, marker, Token::ThenKeyword);
    expected_fn!(parser, marker, expr);
    expected!(parser, marker, Token::ElseKeyword);
    expected_fn!(parser, marker, expr);
    Ok(marker.complete(parser, SyntaxKind::IfExprNode))
}

/// Parses a literal hints.
fn literal_hints(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    parser.require(Token::HintsKeyword);
    braced_items!(
        parser,
        marker,
        Some(Token::Comma),
        HINTS_ITEM_RECOVERY_SET,
        literal_hints_item
    );
    Ok(marker.complete(parser, SyntaxKind::LiteralHintsNode))
}

/// Parses a literal hints item.
fn literal_hints_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "hint key");
    parser.update_last_token_kind(SyntaxKind::Ident);
    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::LiteralHintsItemNode);
    Ok(())
}

/// Parses a literal input.
fn literal_input(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    parser.require(Token::InputKeyword);
    braced_items!(
        parser,
        marker,
        Some(Token::Comma),
        LITERAL_INPUT_ITEM_RECOVERY_SET,
        literal_input_item
    );
    Ok(marker.complete(parser, SyntaxKind::LiteralInputNode))
}

/// Parses a literal input item.
fn literal_input_item(parser: &mut Parser<'_>, marker: Marker) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "input key");
    parser.update_last_token_kind(SyntaxKind::Ident);

    while parser.next_if(Token::Dot) {
        expected_in!(parser, marker, ANY_IDENT, "struct member name");
        parser.update_last_token_kind(SyntaxKind::Ident);
    }

    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::LiteralInputItemNode);
    Ok(())
}

/// Parses a literal output.
fn literal_output(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    parser.require(Token::OutputKeyword);
    braced_items!(
        parser,
        marker,
        Some(Token::Comma),
        LITERAL_OUTPUT_ITEM_RECOVERY_SET,
        literal_output_item
    );
    Ok(marker.complete(parser, SyntaxKind::LiteralOutputNode))
}

/// Parses a literal output item.
fn literal_output_item(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<(), (Marker, Diagnostic)> {
    expected_in!(parser, marker, ANY_IDENT, "output key");
    parser.update_last_token_kind(SyntaxKind::Ident);

    while parser.next_if(Token::Dot) {
        expected_in!(parser, marker, ANY_IDENT, "struct member name");
        parser.update_last_token_kind(SyntaxKind::Ident);
    }

    expected!(parser, marker, Token::Colon);
    expected_fn!(parser, marker, expr);
    marker.complete(parser, SyntaxKind::LiteralOutputItemNode);
    Ok(())
}

/// Parses a call expression.
fn call_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    paren_items!(parser, marker, Some(Token::Comma), EXPR_RECOVERY_SET, expr);
    Ok(marker.complete(parser, SyntaxKind::CallExprNode))
}

/// Parses an index expression.
fn index_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    bracketed!(parser, marker, |parser, _| {
        expected_fn!(parser, expr);
        Ok(())
    });
    Ok(marker.complete(parser, SyntaxKind::IndexExprNode))
}

/// Parses an access expression.
fn access_expr(
    parser: &mut Parser<'_>,
    marker: Marker,
) -> Result<CompletedMarker, (Marker, Diagnostic)> {
    parser.require(Token::Dot);
    expected_in!(parser, marker, ANY_IDENT, "name");
    parser.update_last_token_kind(SyntaxKind::Ident);
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
        Token::Exclamation => (8, LogicalNotExprNode, Right),
        Token::Minus => (8, NegationExprNode, Right),
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
        Token::Exponentiation => (7, ExponentiationExprNode, Left),
        _ => panic!("unknown infix operator token"),
    }
}

/// Determines the precedence of a postfix operator.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.1/SPEC.md#operator-precedence-table
fn postfix_precedence(token: Token) -> u8 {
    // All postfix operators are left-associative
    match token {
        Token::OpenParen => 9,
        Token::OpenBracket => 10,
        Token::Dot => 11,
        _ => panic!("unknown postfix operator token"),
    }
}
