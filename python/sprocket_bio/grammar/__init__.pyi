import typing

from collections.abc import Sequence

from . import parser, version
from .version import SupportedVersion

@typing.final
class Diagnostic:
    rule: typing.Final[str | None]
    severity: typing.Final[Severity]
    message: typing.Final[str]
    help: typing.Final[str | None]
    fix: typing.Final[str | None]
    labels: Sequence[Label]

    @staticmethod
    def error(message: str) -> Diagnostic: ...
    @staticmethod
    def warning(message: str) -> Diagnostic: ...
    @staticmethod
    def note(message: str) -> Diagnostic: ...
    def with_rule(self, rule: str) -> Diagnostic: ...
    def with_help(self, help: str) -> Diagnostic: ...
    def with_fix(self, fix: str) -> Diagnostic: ...
    def with_highlight(self, span: Span) -> Diagnostic: ...
    def with_label(self, message: str, span: Span) -> Diagnostic: ...
    def with_severity(self, severity: Severity) -> Diagnostic: ...
    def __lt__(self, other: typing.Any, /) -> bool: ...
    def __le__(self, other: typing.Any, /) -> bool: ...
    def __gt__(self, other: typing.Any, /) -> bool: ...
    def __ge__(self, other: typing.Any, /) -> bool: ...

@typing.final
class Label:
    message: typing.Final[str]
    span: typing.Final[Span]

    def __new__(cls, message: str, span: Span) -> Label: ...
    def __lt__(self, other: typing.Any, /) -> bool: ...
    def __le__(self, other: typing.Any, /) -> bool: ...
    def __gt__(self, other: typing.Any, /) -> bool: ...
    def __ge__(self, other: typing.Any, /) -> bool: ...

@typing.final
class Severity:
    ERROR: Severity
    WARNING: Severity
    NOTE: Severity

    def __lt__(self, other: typing.Any, /) -> bool: ...
    def __le__(self, other: typing.Any, /) -> bool: ...
    def __gt__(self, other: typing.Any, /) -> bool: ...
    def __ge__(self, other: typing.Any, /) -> bool: ...

@typing.final
class Span:
    start: typing.Final[int]
    end: typing.Final[int]

    def __new__(cls, start: int, len: int) -> Span: ...
    def len(self) -> int: ...
    def is_empty(self) -> bool: ...
    def contains(self, offset: int) -> bool: ...
    def intersect(self, other: Span) -> Span | None: ...
    def __len__(self) -> int: ...
    def __lt__(self, other: typing.Any, /) -> bool: ...
    def __le__(self, other: typing.Any, /) -> bool: ...
    def __gt__(self, other: typing.Any, /) -> bool: ...
    def __ge__(self, other: typing.Any, /) -> bool: ...

@typing.final
class SyntaxKind:
    UNKNOWN: SyntaxKind
    UNPARSED: SyntaxKind
    WHITESPACE: SyntaxKind
    COMMENT: SyntaxKind
    VERSION: SyntaxKind
    FLOAT: SyntaxKind
    INTEGER: SyntaxKind
    IDENT: SyntaxKind
    SINGLE_QUOTE: SyntaxKind
    DOUBLE_QUOTE: SyntaxKind
    OPEN_HEREDOC: SyntaxKind
    CLOSE_HEREDOC: SyntaxKind
    ARRAY_TYPE_KEYWORD: SyntaxKind
    BOOLEAN_TYPE_KEYWORD: SyntaxKind
    FILE_TYPE_KEYWORD: SyntaxKind
    FLOAT_TYPE_KEYWORD: SyntaxKind
    INT_TYPE_KEYWORD: SyntaxKind
    MAP_TYPE_KEYWORD: SyntaxKind
    OBJECT_TYPE_KEYWORD: SyntaxKind
    PAIR_TYPE_KEYWORD: SyntaxKind
    STRING_TYPE_KEYWORD: SyntaxKind
    AFTER_KEYWORD: SyntaxKind
    ALIAS_KEYWORD: SyntaxKind
    AS_KEYWORD: SyntaxKind
    CALL_KEYWORD: SyntaxKind
    COMMAND_KEYWORD: SyntaxKind
    ELSE_KEYWORD: SyntaxKind
    ENV_KEYWORD: SyntaxKind
    FALSE_KEYWORD: SyntaxKind
    FROM_KEYWORD: SyntaxKind
    IF_KEYWORD: SyntaxKind
    IN_KEYWORD: SyntaxKind
    IMPORT_KEYWORD: SyntaxKind
    INPUT_KEYWORD: SyntaxKind
    META_KEYWORD: SyntaxKind
    NONE_KEYWORD: SyntaxKind
    NULL_KEYWORD: SyntaxKind
    OBJECT_KEYWORD: SyntaxKind
    OUTPUT_KEYWORD: SyntaxKind
    PARAMETER_META_KEYWORD: SyntaxKind
    RUNTIME_KEYWORD: SyntaxKind
    SCATTER_KEYWORD: SyntaxKind
    STRUCT_KEYWORD: SyntaxKind
    ENUM_KEYWORD: SyntaxKind
    TASK_KEYWORD: SyntaxKind
    THEN_KEYWORD: SyntaxKind
    TRUE_KEYWORD: SyntaxKind
    VERSION_KEYWORD: SyntaxKind
    WORKFLOW_KEYWORD: SyntaxKind
    DIRECTORY_TYPE_KEYWORD: SyntaxKind
    HINTS_KEYWORD: SyntaxKind
    REQUIREMENTS_KEYWORD: SyntaxKind
    OPEN_BRACE: SyntaxKind
    CLOSE_BRACE: SyntaxKind
    OPEN_BRACKET: SyntaxKind
    CLOSE_BRACKET: SyntaxKind
    ASSIGNMENT: SyntaxKind
    COLON: SyntaxKind
    COMMA: SyntaxKind
    OPEN_PAREN: SyntaxKind
    CLOSE_PAREN: SyntaxKind
    QUESTION_MARK: SyntaxKind
    EXCLAMATION: SyntaxKind
    PLUS: SyntaxKind
    MINUS: SyntaxKind
    LOGICAL_OR: SyntaxKind
    LOGICAL_AND: SyntaxKind
    ASTERISK: SyntaxKind
    EXPONENTIATION: SyntaxKind
    SLASH: SyntaxKind
    PERCENT: SyntaxKind
    EQUAL: SyntaxKind
    NOT_EQUAL: SyntaxKind
    LESS_EQUAL: SyntaxKind
    GREATER_EQUAL: SyntaxKind
    LESS: SyntaxKind
    GREATER: SyntaxKind
    DOT: SyntaxKind
    LITERAL_STRING_TEXT: SyntaxKind
    LITERAL_COMMAND_TEXT: SyntaxKind
    PLACEHOLDER_OPEN: SyntaxKind
    ABANDONED: SyntaxKind
    ROOT_NODE: SyntaxKind
    VERSION_STATEMENT_NODE: SyntaxKind
    IMPORT_STATEMENT_NODE: SyntaxKind
    IMPORT_MEMBERS_NODE: SyntaxKind
    IMPORT_MEMBER_NODE: SyntaxKind
    SYMBOLIC_MODULE_PATH_NODE: SyntaxKind
    IMPORT_ALIAS_NODE: SyntaxKind
    STRUCT_DEFINITION_NODE: SyntaxKind
    ENUM_DEFINITION_NODE: SyntaxKind
    ENUM_TYPE_PARAMETER_NODE: SyntaxKind
    ENUM_CHOICE_NODE: SyntaxKind
    TASK_DEFINITION_NODE: SyntaxKind
    WORKFLOW_DEFINITION_NODE: SyntaxKind
    UNBOUND_DECL_NODE: SyntaxKind
    BOUND_DECL_NODE: SyntaxKind
    INPUT_SECTION_NODE: SyntaxKind
    OUTPUT_SECTION_NODE: SyntaxKind
    COMMAND_SECTION_NODE: SyntaxKind
    REQUIREMENTS_SECTION_NODE: SyntaxKind
    REQUIREMENTS_ITEM_NODE: SyntaxKind
    TASK_HINTS_SECTION_NODE: SyntaxKind
    WORKFLOW_HINTS_SECTION_NODE: SyntaxKind
    TASK_HINTS_ITEM_NODE: SyntaxKind
    WORKFLOW_HINTS_ITEM_NODE: SyntaxKind
    WORKFLOW_HINTS_OBJECT_NODE: SyntaxKind
    WORKFLOW_HINTS_OBJECT_ITEM_NODE: SyntaxKind
    WORKFLOW_HINTS_ARRAY_NODE: SyntaxKind
    RUNTIME_SECTION_NODE: SyntaxKind
    RUNTIME_ITEM_NODE: SyntaxKind
    PRIMITIVE_TYPE_NODE: SyntaxKind
    MAP_TYPE_NODE: SyntaxKind
    ARRAY_TYPE_NODE: SyntaxKind
    PAIR_TYPE_NODE: SyntaxKind
    OBJECT_TYPE_NODE: SyntaxKind
    TYPE_REF_NODE: SyntaxKind
    METADATA_SECTION_NODE: SyntaxKind
    PARAMETER_METADATA_SECTION_NODE: SyntaxKind
    METADATA_OBJECT_ITEM_NODE: SyntaxKind
    METADATA_OBJECT_NODE: SyntaxKind
    METADATA_ARRAY_NODE: SyntaxKind
    LITERAL_INTEGER_NODE: SyntaxKind
    LITERAL_FLOAT_NODE: SyntaxKind
    LITERAL_BOOLEAN_NODE: SyntaxKind
    LITERAL_NONE_NODE: SyntaxKind
    LITERAL_NULL_NODE: SyntaxKind
    LITERAL_STRING_NODE: SyntaxKind
    LITERAL_PAIR_NODE: SyntaxKind
    LITERAL_ARRAY_NODE: SyntaxKind
    LITERAL_MAP_NODE: SyntaxKind
    LITERAL_MAP_ITEM_NODE: SyntaxKind
    LITERAL_OBJECT_NODE: SyntaxKind
    LITERAL_OBJECT_ITEM_NODE: SyntaxKind
    LITERAL_STRUCT_NODE: SyntaxKind
    LITERAL_STRUCT_ITEM_NODE: SyntaxKind
    LITERAL_HINTS_NODE: SyntaxKind
    LITERAL_HINTS_ITEM_NODE: SyntaxKind
    LITERAL_INPUT_NODE: SyntaxKind
    LITERAL_INPUT_ITEM_NODE: SyntaxKind
    LITERAL_OUTPUT_NODE: SyntaxKind
    LITERAL_OUTPUT_ITEM_NODE: SyntaxKind
    PARENTHESIZED_EXPR_NODE: SyntaxKind
    NAME_REF_EXPR_NODE: SyntaxKind
    IF_EXPR_NODE: SyntaxKind
    LOGICAL_NOT_EXPR_NODE: SyntaxKind
    NEGATION_EXPR_NODE: SyntaxKind
    LOGICAL_OR_EXPR_NODE: SyntaxKind
    LOGICAL_AND_EXPR_NODE: SyntaxKind
    EQUALITY_EXPR_NODE: SyntaxKind
    INEQUALITY_EXPR_NODE: SyntaxKind
    LESS_EXPR_NODE: SyntaxKind
    LESS_EQUAL_EXPR_NODE: SyntaxKind
    GREATER_EXPR_NODE: SyntaxKind
    GREATER_EQUAL_EXPR_NODE: SyntaxKind
    ADDITION_EXPR_NODE: SyntaxKind
    SUBTRACTION_EXPR_NODE: SyntaxKind
    MULTIPLICATION_EXPR_NODE: SyntaxKind
    DIVISION_EXPR_NODE: SyntaxKind
    MODULO_EXPR_NODE: SyntaxKind
    EXPONENTIATION_EXPR_NODE: SyntaxKind
    CALL_EXPR_NODE: SyntaxKind
    INDEX_EXPR_NODE: SyntaxKind
    ACCESS_EXPR_NODE: SyntaxKind
    PLACEHOLDER_NODE: SyntaxKind
    PLACEHOLDER_SEP_OPTION_NODE: SyntaxKind
    PLACEHOLDER_DEFAULT_OPTION_NODE: SyntaxKind
    PLACEHOLDER_TRUE_FALSE_OPTION_NODE: SyntaxKind
    CONDITIONAL_STATEMENT_NODE: SyntaxKind
    CONDITIONAL_STATEMENT_CLAUSE_NODE: SyntaxKind
    SCATTER_STATEMENT_NODE: SyntaxKind
    CALL_STATEMENT_NODE: SyntaxKind
    CALL_TARGET_NODE: SyntaxKind
    CALL_ALIAS_NODE: SyntaxKind
    CALL_AFTER_NODE: SyntaxKind
    CALL_INPUT_ITEM_NODE: SyntaxKind
    MAX: SyntaxKind

    def is_symbolic(self) -> bool: ...
    def describe(self) -> str: ...
    def is_trivia(self) -> bool: ...
    def is_keyword(self) -> bool: ...
    def is_type(self) -> bool: ...
    def is_operator(self) -> bool: ...
    def __lt__(self, other: typing.Any, /) -> bool: ...
    def __le__(self, other: typing.Any, /) -> bool: ...
    def __gt__(self, other: typing.Any, /) -> bool: ...
    def __ge__(self, other: typing.Any, /) -> bool: ...

__all__ = [
    "Diagnostic",
    "Label",
    "Severity",
    "Span",
    "SyntaxKind",
    "SupportedVersion",
    "parser",
    "version",
]
