import builtins
import typing

from typing_extensions import disjoint_base

from sprocket_bio.grammar import Span
from sprocket_bio.grammar.version import SupportedVersion

from . import AstNode, AstToken, Ident

@typing.final
class AccessExpr(AstNode):
    def is_task_access(self) -> bool: ...
    def operands(self) -> tuple[Expr, Ident]: ...

@typing.final
class AdditionExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class AfterKeyword(AstToken):
    pass

@typing.final
class AliasKeyword(AstToken):
    pass

@typing.final
class ArrayType(AstNode):
    def element_type(self) -> Type: ...
    def is_non_empty(self) -> bool: ...
    def is_optional(self) -> bool: ...

@typing.final
class ArrayTypeKeyword(AstToken):
    pass

@typing.final
class AsKeyword(AstToken):
    pass

@typing.final
class Assignment(AstToken):
    pass

@typing.final
class Ast(AstNode):
    def enums(self) -> list[EnumDefinition]: ...
    def imports(self) -> list[ImportStatement]: ...
    def items(self) -> list[DocumentItem]: ...
    def structs(self) -> list[StructDefinition]: ...
    def tasks(self) -> list[TaskDefinition]: ...
    def workflows(self) -> list[WorkflowDefinition]: ...

@typing.final
class Asterisk(AstToken):
    pass

@typing.final
class BooleanTypeKeyword(AstToken):
    pass

@typing.final
class BoundDecl(AstNode):
    def env(self) -> EnvKeyword | None: ...
    def expr(self) -> Expr: ...
    def name(self) -> Ident: ...
    def ty(self) -> Type: ...

@typing.final
class CallAfter(AstNode):
    def name(self) -> Ident: ...

@typing.final
class CallAlias(AstNode):
    def name(self) -> Ident: ...

@typing.final
class CallExpr(AstNode):
    def arguments(self) -> list[Expr]: ...
    def target(self) -> Ident: ...

@typing.final
class CallInputItem(AstNode):
    def expr(self) -> Expr | None: ...
    def is_implicit_bind(self) -> bool: ...
    def name(self) -> Ident: ...
    def parent(self) -> CallStatement: ...

@typing.final
class CallKeyword(AstToken):
    pass

@typing.final
class CallStatement(AstNode):
    def after(self) -> list[CallAfter]: ...
    def alias(self) -> CallAlias | None: ...
    def inputs(self) -> list[CallInputItem]: ...
    def keyword(self) -> CallKeyword: ...
    def target(self) -> CallTarget: ...

@typing.final
class CallTarget(AstNode):
    def names(self) -> list[Ident]: ...

@typing.final
class CloseBrace(AstToken):
    pass

@typing.final
class CloseBracket(AstToken):
    pass

@typing.final
class CloseHeredoc(AstToken):
    pass

@typing.final
class CloseParen(AstToken):
    pass

@typing.final
class Colon(AstToken):
    pass

@typing.final
class Comma(AstToken):
    pass

@typing.final
class CommandKeyword(AstToken):
    pass

@disjoint_base
class CommandPart:
    @typing.final
    class Placeholder(CommandPart):
        _0: typing.Final[Placeholder]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: Placeholder) -> CommandPart.Placeholder: ...

    @typing.final
    class Text(CommandPart):
        _0: typing.Final[CommandText]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: CommandText) -> CommandPart.Text: ...

@typing.final
class CommandSection(AstNode):
    def count_whitespace(self) -> int | None: ...
    def is_heredoc(self) -> bool: ...
    def keyword(self) -> CommandKeyword: ...
    def parent(self) -> SectionParent: ...
    def parts(self) -> list[CommandPart]: ...

@typing.final
class CommandText(AstToken):
    pass

@typing.final
class ConditionalStatement(AstNode):
    def clauses(self) -> list[ConditionalStatementClause]: ...
    def else_clause(self) -> ConditionalStatementClause | None: ...
    def else_if_clauses(self) -> list[ConditionalStatementClause]: ...
    def if_clause(self) -> ConditionalStatementClause: ...

@typing.final
class ConditionalStatementClause(AstNode):
    def expr(self) -> Expr | None: ...
    def kind(self) -> ConditionalStatementClauseKind: ...
    def statements(self) -> list[WorkflowStatement]: ...

@typing.final
class ConditionalStatementClauseKind:
    ELSE: typing.Final[ConditionalStatementClauseKind]
    ELSE_IF: typing.Final[ConditionalStatementClauseKind]
    IF: typing.Final[ConditionalStatementClauseKind]

    def __int__(self) -> int: ...
    def __repr__(self) -> str: ...

@disjoint_base
class Decl:
    def env(self) -> EnvKeyword | None: ...
    def expr(self) -> Expr | None: ...
    def name(self) -> Ident: ...
    def ty(self) -> Type: ...
    @typing.final
    class Bound(Decl):
        _0: typing.Final[BoundDecl]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: BoundDecl) -> Decl.Bound: ...

    @typing.final
    class Unbound(Decl):
        _0: typing.Final[UnboundDecl]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: UnboundDecl) -> Decl.Unbound: ...

@typing.final
class DefaultOption(AstNode):
    def value(self) -> LiteralString: ...

@typing.final
class DirectoryTypeKeyword(AstToken):
    pass

@typing.final
class DivisionExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@disjoint_base
class DocumentItem:
    @typing.final
    class Enum(DocumentItem):
        _0: typing.Final[EnumDefinition]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: EnumDefinition) -> DocumentItem.Enum: ...

    @typing.final
    class Import(DocumentItem):
        _0: typing.Final[ImportStatement]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ImportStatement) -> DocumentItem.Import: ...

    @typing.final
    class Struct(DocumentItem):
        _0: typing.Final[StructDefinition]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: StructDefinition) -> DocumentItem.Struct: ...

    @typing.final
    class Task(DocumentItem):
        _0: typing.Final[TaskDefinition]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: TaskDefinition) -> DocumentItem.Task: ...

    @typing.final
    class Workflow(DocumentItem):
        _0: typing.Final[WorkflowDefinition]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: WorkflowDefinition) -> DocumentItem.Workflow: ...

@typing.final
class Dot(AstToken):
    pass

@typing.final
class DoubleQuote(AstToken):
    pass

@typing.final
class ElseKeyword(AstToken):
    pass

@typing.final
class EnumChoice(AstNode):
    def name(self) -> Ident: ...
    def value(self) -> Expr | None: ...

@typing.final
class EnumDefinition(AstNode):
    def choices(self) -> list[EnumChoice]: ...
    def keyword(self) -> EnumKeyword: ...
    def name(self) -> Ident: ...
    def type_parameter(self) -> EnumTypeParameter | None: ...

@typing.final
class EnumKeyword(AstToken):
    pass

@typing.final
class EnumTypeParameter(AstNode):
    def ty(self) -> Type: ...

@typing.final
class EnvKeyword(AstToken):
    pass

@typing.final
class Equal(AstToken):
    pass

@typing.final
class EqualityExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class Exclamation(AstToken):
    pass

@typing.final
class Exponentiation(AstToken):
    pass

@typing.final
class ExponentiationExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@disjoint_base
class Expr:
    @typing.final
    class Access(Expr):
        _0: typing.Final[AccessExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: AccessExpr) -> Expr.Access: ...

    @typing.final
    class Addition(Expr):
        _0: typing.Final[AdditionExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: AdditionExpr) -> Expr.Addition: ...

    @typing.final
    class Call(Expr):
        _0: typing.Final[CallExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: CallExpr) -> Expr.Call: ...

    @typing.final
    class Division(Expr):
        _0: typing.Final[DivisionExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: DivisionExpr) -> Expr.Division: ...

    @typing.final
    class Equality(Expr):
        _0: typing.Final[EqualityExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: EqualityExpr) -> Expr.Equality: ...

    @typing.final
    class Exponentiation(Expr):
        _0: typing.Final[ExponentiationExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ExponentiationExpr) -> Expr.Exponentiation: ...

    @typing.final
    class Greater(Expr):
        _0: typing.Final[GreaterExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: GreaterExpr) -> Expr.Greater: ...

    @typing.final
    class GreaterEqual(Expr):
        _0: typing.Final[GreaterEqualExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: GreaterEqualExpr) -> Expr.GreaterEqual: ...

    @typing.final
    class If(Expr):
        _0: typing.Final[IfExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: IfExpr) -> Expr.If: ...

    @typing.final
    class Index(Expr):
        _0: typing.Final[IndexExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: IndexExpr) -> Expr.Index: ...

    @typing.final
    class Inequality(Expr):
        _0: typing.Final[InequalityExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: InequalityExpr) -> Expr.Inequality: ...

    @typing.final
    class Less(Expr):
        _0: typing.Final[LessExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LessExpr) -> Expr.Less: ...

    @typing.final
    class LessEqual(Expr):
        _0: typing.Final[LessEqualExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LessEqualExpr) -> Expr.LessEqual: ...

    @typing.final
    class Literal(Expr):
        _0: typing.Final[LiteralExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralExpr) -> Expr.Literal: ...

    @typing.final
    class LogicalAnd(Expr):
        _0: typing.Final[LogicalAndExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LogicalAndExpr) -> Expr.LogicalAnd: ...

    @typing.final
    class LogicalNot(Expr):
        _0: typing.Final[LogicalNotExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LogicalNotExpr) -> Expr.LogicalNot: ...

    @typing.final
    class LogicalOr(Expr):
        _0: typing.Final[LogicalOrExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LogicalOrExpr) -> Expr.LogicalOr: ...

    @typing.final
    class Modulo(Expr):
        _0: typing.Final[ModuloExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ModuloExpr) -> Expr.Modulo: ...

    @typing.final
    class Multiplication(Expr):
        _0: typing.Final[MultiplicationExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: MultiplicationExpr) -> Expr.Multiplication: ...

    @typing.final
    class NameRef(Expr):
        _0: typing.Final[NameRefExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: NameRefExpr) -> Expr.NameRef: ...

    @typing.final
    class Negation(Expr):
        _0: typing.Final[NegationExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: NegationExpr) -> Expr.Negation: ...

    @typing.final
    class Parenthesized(Expr):
        _0: typing.Final[ParenthesizedExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ParenthesizedExpr) -> Expr.Parenthesized: ...

    @typing.final
    class Subtraction(Expr):
        _0: typing.Final[SubtractionExpr]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: SubtractionExpr) -> Expr.Subtraction: ...

@typing.final
class FalseKeyword(AstToken):
    pass

@typing.final
class FileTypeKeyword(AstToken):
    pass

@typing.final
class Float(AstToken):
    pass

@typing.final
class FloatTypeKeyword(AstToken):
    pass

@typing.final
class FromKeyword(AstToken):
    pass

@typing.final
class Greater(AstToken):
    pass

@typing.final
class GreaterEqual(AstToken):
    pass

@typing.final
class GreaterEqualExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class GreaterExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class HintsKeyword(AstToken):
    pass

@typing.final
class IfExpr(AstNode):
    def exprs(self) -> tuple[Expr, Expr, Expr]: ...

@typing.final
class IfKeyword(AstToken):
    pass

@typing.final
class ImportAlias(AstNode):
    def alias_keyword(self) -> AliasKeyword: ...
    def as_keyword(self) -> AsKeyword: ...
    def names(self) -> tuple[Ident, Ident]: ...

@typing.final
class ImportForm:
    NAMESPACE: typing.Final[ImportForm]
    SELECTED: typing.Final[ImportForm]
    WILDCARD: typing.Final[ImportForm]

    def __int__(self) -> int: ...
    def __repr__(self) -> str: ...

@typing.final
class ImportKeyword(AstToken):
    pass

@typing.final
class ImportMember(AstNode):
    def alias(self) -> Ident | None: ...
    def name(self) -> Ident: ...

@typing.final
class ImportMembers(AstNode):
    def members(self) -> list[ImportMember]: ...

@disjoint_base
class ImportSource:
    def span(self) -> Span: ...
    @typing.final
    class ModulePath(ImportSource):
        _0: typing.Final[SymbolicModulePath]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: SymbolicModulePath) -> ImportSource.ModulePath: ...

    @typing.final
    class Uri(ImportSource):
        _0: typing.Final[LiteralString]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralString) -> ImportSource.Uri: ...

@typing.final
class ImportStatement(AstNode):
    def aliases(self) -> list[ImportAlias]: ...
    def explicit_namespace(self) -> Ident | None: ...
    def form(self) -> ImportForm: ...
    def from_keyword(self) -> FromKeyword | None: ...
    def keyword(self) -> ImportKeyword: ...
    def members(self) -> ImportMembers | None: ...
    def namespace(self) -> tuple[str, Span] | None: ...
    def source(self) -> ImportSource: ...
    def wildcard(self) -> Asterisk | None: ...

@typing.final
class InKeyword(AstToken):
    pass

@typing.final
class IndexExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class InequalityExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class InputKeyword(AstToken):
    pass

@typing.final
class InputSection(AstNode):
    def declarations(self) -> list[Decl]: ...
    def parent(self) -> SectionParent: ...

@typing.final
class IntTypeKeyword(AstToken):
    pass

@typing.final
class Integer(AstToken):
    pass

@typing.final
class Less(AstToken):
    pass

@typing.final
class LessEqual(AstToken):
    pass

@typing.final
class LessEqualExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class LessExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class LiteralArray(AstNode):
    def elements(self) -> list[Expr]: ...

@typing.final
class LiteralBoolean(AstNode):
    def value(self) -> bool: ...

@disjoint_base
class LiteralExpr:
    @typing.final
    class Array(LiteralExpr):
        _0: typing.Final[LiteralArray]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralArray) -> LiteralExpr.Array: ...

    @typing.final
    class Boolean(LiteralExpr):
        _0: typing.Final[LiteralBoolean]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralBoolean) -> LiteralExpr.Boolean: ...

    @typing.final
    class Float(LiteralExpr):
        _0: typing.Final[LiteralFloat]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralFloat) -> LiteralExpr.Float: ...

    @typing.final
    class Hints(LiteralExpr):
        _0: typing.Final[LiteralHints]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralHints) -> LiteralExpr.Hints: ...

    @typing.final
    class Input(LiteralExpr):
        _0: typing.Final[LiteralInput]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralInput) -> LiteralExpr.Input: ...

    @typing.final
    class Integer(LiteralExpr):
        _0: typing.Final[LiteralInteger]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralInteger) -> LiteralExpr.Integer: ...

    @typing.final
    class Map(LiteralExpr):
        _0: typing.Final[LiteralMap]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralMap) -> LiteralExpr.Map: ...

    @typing.final
    class None_(LiteralExpr):
        _0: typing.Final[LiteralNone]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralNone) -> LiteralExpr.None_: ...

    @typing.final
    class Object(LiteralExpr):
        _0: typing.Final[LiteralObject]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralObject) -> LiteralExpr.Object: ...

    @typing.final
    class Output(LiteralExpr):
        _0: typing.Final[LiteralOutput]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralOutput) -> LiteralExpr.Output: ...

    @typing.final
    class Pair(LiteralExpr):
        _0: typing.Final[LiteralPair]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralPair) -> LiteralExpr.Pair: ...

    @typing.final
    class String(LiteralExpr):
        _0: typing.Final[LiteralString]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralString) -> LiteralExpr.String: ...

    @typing.final
    class Struct(LiteralExpr):
        _0: typing.Final[LiteralStruct]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralStruct) -> LiteralExpr.Struct: ...

@typing.final
class LiteralFloat(AstNode):
    def float(self) -> Float: ...
    def minus(self) -> Minus | None: ...
    def value(self) -> builtins.float | None: ...

@typing.final
class LiteralHints(AstNode):
    def items(self) -> list[LiteralHintsItem]: ...

@typing.final
class LiteralHintsItem(AstNode):
    def expr(self) -> Expr: ...
    def name(self) -> Ident: ...

@typing.final
class LiteralInput(AstNode):
    def items(self) -> list[LiteralInputItem]: ...

@typing.final
class LiteralInputItem(AstNode):
    def expr(self) -> Expr: ...
    def names(self) -> list[Ident]: ...

@typing.final
class LiteralInteger(AstNode):
    def integer(self) -> Integer: ...
    def minus(self) -> Minus | None: ...
    def negate(self) -> int | None: ...
    def value(self) -> int | None: ...

@typing.final
class LiteralMap(AstNode):
    def items(self) -> list[LiteralMapItem]: ...

@typing.final
class LiteralMapItem(AstNode):
    def key_value(self) -> tuple[Expr, Expr]: ...

@typing.final
class LiteralNone(AstNode):
    pass

@typing.final
class LiteralNull(AstNode):
    pass

@typing.final
class LiteralObject(AstNode):
    def items(self) -> list[LiteralObjectItem]: ...

@typing.final
class LiteralObjectItem(AstNode):
    def name_value(self) -> tuple[Ident, Expr]: ...

@typing.final
class LiteralOutput(AstNode):
    def items(self) -> list[LiteralOutputItem]: ...

@typing.final
class LiteralOutputItem(AstNode):
    def expr(self) -> Expr: ...
    def names(self) -> list[Ident]: ...

@typing.final
class LiteralPair(AstNode):
    def exprs(self) -> tuple[Expr, Expr]: ...

@typing.final
class LiteralString(AstNode):
    def is_empty(self) -> bool: ...
    def kind(self) -> LiteralStringKind: ...
    def parts(self) -> list[StringPart]: ...
    def text(self) -> LiteralStringText | None: ...

@typing.final
class LiteralStringKind:
    DOUBLE_QUOTED: typing.Final[LiteralStringKind]
    MULTILINE: typing.Final[LiteralStringKind]
    SINGLE_QUOTED: typing.Final[LiteralStringKind]

    def __int__(self) -> int: ...
    def __repr__(self) -> str: ...

@disjoint_base
class LiteralStringText:
    @typing.final
    class Empty(LiteralStringText):
        __match_args__ = ()

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls) -> LiteralStringText.Empty: ...

    @typing.final
    class Token(LiteralStringText):
        _0: typing.Final[StringText]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: StringText) -> LiteralStringText.Token: ...

@typing.final
class LiteralStruct(AstNode):
    def items(self) -> list[LiteralStructItem]: ...
    def name(self) -> Ident: ...

@typing.final
class LiteralStructItem(AstNode):
    def name_value(self) -> tuple[Ident, Expr]: ...

@typing.final
class LogicalAnd(AstToken):
    pass

@typing.final
class LogicalAndExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class LogicalNotExpr(AstNode):
    def operand(self) -> Expr: ...

@typing.final
class LogicalOr(AstToken):
    pass

@typing.final
class LogicalOrExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class MapType(AstNode):
    def is_optional(self) -> bool: ...
    def types(self) -> tuple[PrimitiveType, Type]: ...

@typing.final
class MapTypeKeyword(AstToken):
    pass

@typing.final
class MetaKeyword(AstToken):
    pass

@typing.final
class MetadataArray(AstNode):
    def elements(self) -> list[MetadataValue]: ...

@typing.final
class MetadataObject(AstNode):
    def items(self) -> list[MetadataObjectItem]: ...

@typing.final
class MetadataObjectItem(AstNode):
    def name(self) -> Ident: ...
    def value(self) -> MetadataValue: ...

@typing.final
class MetadataSection(AstNode):
    def items(self) -> list[MetadataObjectItem]: ...
    def keyword(self) -> MetaKeyword: ...
    def parent(self) -> SectionParent: ...

@disjoint_base
class MetadataValue:
    @typing.final
    class Array(MetadataValue):
        _0: typing.Final[MetadataArray]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: MetadataArray) -> MetadataValue.Array: ...

    @typing.final
    class Boolean(MetadataValue):
        _0: typing.Final[LiteralBoolean]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralBoolean) -> MetadataValue.Boolean: ...

    @typing.final
    class Float(MetadataValue):
        _0: typing.Final[LiteralFloat]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralFloat) -> MetadataValue.Float: ...

    @typing.final
    class Integer(MetadataValue):
        _0: typing.Final[LiteralInteger]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralInteger) -> MetadataValue.Integer: ...

    @typing.final
    class Null(MetadataValue):
        _0: typing.Final[LiteralNull]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralNull) -> MetadataValue.Null: ...

    @typing.final
    class Object(MetadataValue):
        _0: typing.Final[MetadataObject]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: MetadataObject) -> MetadataValue.Object: ...

    @typing.final
    class String(MetadataValue):
        _0: typing.Final[LiteralString]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralString) -> MetadataValue.String: ...

@typing.final
class Minus(AstToken):
    pass

@typing.final
class ModuloExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class MultiplicationExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class NameRefExpr(AstNode):
    def name(self) -> Ident: ...

@typing.final
class NegationExpr(AstNode):
    def operand(self) -> Expr: ...

@typing.final
class NoneKeyword(AstToken):
    pass

@typing.final
class NotEqual(AstToken):
    pass

@typing.final
class NullKeyword(AstToken):
    pass

@typing.final
class ObjectKeyword(AstToken):
    pass

@typing.final
class ObjectType(AstNode):
    def is_optional(self) -> bool: ...

@typing.final
class ObjectTypeKeyword(AstToken):
    pass

@typing.final
class OpenBrace(AstToken):
    pass

@typing.final
class OpenBracket(AstToken):
    pass

@typing.final
class OpenHeredoc(AstToken):
    pass

@typing.final
class OpenParen(AstToken):
    pass

@typing.final
class OutputKeyword(AstToken):
    pass

@typing.final
class OutputSection(AstNode):
    def declarations(self) -> list[BoundDecl]: ...
    def parent(self) -> SectionParent: ...

@typing.final
class PairType(AstNode):
    def is_optional(self) -> bool: ...
    def types(self) -> tuple[Type, Type]: ...

@typing.final
class PairTypeKeyword(AstToken):
    pass

@typing.final
class ParameterMetaKeyword(AstToken):
    pass

@typing.final
class ParameterMetadataSection(AstNode):
    def items(self) -> list[MetadataObjectItem]: ...
    def keyword(self) -> ParameterMetaKeyword: ...
    def parent(self) -> SectionParent: ...

@typing.final
class ParenthesizedExpr(AstNode):
    def expr(self) -> Expr: ...

@typing.final
class Percent(AstToken):
    pass

@typing.final
class Placeholder(AstNode):
    def expr(self) -> Expr: ...
    def has_tilde(self) -> bool: ...
    def option(self) -> PlaceholderOption | None: ...

@typing.final
class PlaceholderOpen(AstToken):
    pass

@disjoint_base
class PlaceholderOption:
    @typing.final
    class Default(PlaceholderOption):
        _0: typing.Final[DefaultOption]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: DefaultOption) -> PlaceholderOption.Default: ...

    @typing.final
    class Sep(PlaceholderOption):
        _0: typing.Final[SepOption]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: SepOption) -> PlaceholderOption.Sep: ...

    @typing.final
    class TrueFalse(PlaceholderOption):
        _0: typing.Final[TrueFalseOption]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: TrueFalseOption) -> PlaceholderOption.TrueFalse: ...

@typing.final
class Plus(AstToken):
    pass

@typing.final
class PrimitiveType(AstNode):
    def is_optional(self) -> bool: ...
    def kind(self) -> PrimitiveTypeKind: ...

@typing.final
class PrimitiveTypeKind:
    BOOLEAN: typing.Final[PrimitiveTypeKind]
    DIRECTORY: typing.Final[PrimitiveTypeKind]
    FILE: typing.Final[PrimitiveTypeKind]
    FLOAT: typing.Final[PrimitiveTypeKind]
    INTEGER: typing.Final[PrimitiveTypeKind]
    STRING: typing.Final[PrimitiveTypeKind]

    def __int__(self) -> int: ...
    def __repr__(self) -> str: ...

@typing.final
class QuestionMark(AstToken):
    pass

@typing.final
class RequirementsItem(AstNode):
    def expr(self) -> Expr: ...
    def name(self) -> Ident: ...

@typing.final
class RequirementsKeyword(AstToken):
    pass

@typing.final
class RequirementsSection(AstNode):
    def items(self) -> list[RequirementsItem]: ...
    def keyword(self) -> RequirementsKeyword: ...
    def parent(self) -> SectionParent: ...

@typing.final
class RuntimeItem(AstNode):
    def expr(self) -> Expr: ...
    def name(self) -> Ident: ...

@typing.final
class RuntimeKeyword(AstToken):
    pass

@typing.final
class RuntimeSection(AstNode):
    def items(self) -> list[RuntimeItem]: ...
    def parent(self) -> SectionParent: ...

@typing.final
class ScatterKeyword(AstToken):
    pass

@typing.final
class ScatterStatement(AstNode):
    def expr(self) -> Expr: ...
    def keyword(self) -> ScatterKeyword: ...
    def statements(self) -> list[WorkflowStatement]: ...
    def variable(self) -> Ident: ...

@disjoint_base
class SectionParent:
    def name(self) -> Ident: ...
    @typing.final
    class Struct(SectionParent):
        _0: typing.Final[StructDefinition]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: StructDefinition) -> SectionParent.Struct: ...

    @typing.final
    class Task(SectionParent):
        _0: typing.Final[TaskDefinition]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: TaskDefinition) -> SectionParent.Task: ...

    @typing.final
    class Workflow(SectionParent):
        _0: typing.Final[WorkflowDefinition]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: WorkflowDefinition) -> SectionParent.Workflow: ...

@typing.final
class SepOption(AstNode):
    def separator(self) -> LiteralString: ...

@typing.final
class SingleQuote(AstToken):
    pass

@typing.final
class Slash(AstToken):
    pass

@disjoint_base
class StringPart:
    @typing.final
    class Placeholder(StringPart):
        _0: typing.Final[Placeholder]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: Placeholder) -> StringPart.Placeholder: ...

    @typing.final
    class Text(StringPart):
        _0: typing.Final[StringText]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: StringText) -> StringPart.Text: ...

@typing.final
class StringText(AstToken):
    pass

@typing.final
class StringTypeKeyword(AstToken):
    pass

@typing.final
class StructDefinition(AstNode):
    def items(self) -> list[StructItem]: ...
    def keyword(self) -> StructKeyword: ...
    def members(self) -> list[UnboundDecl]: ...
    def metadata(self) -> list[MetadataSection]: ...
    def name(self) -> Ident: ...
    def parameter_metadata(self) -> list[ParameterMetadataSection]: ...

@disjoint_base
class StructItem:
    @typing.final
    class Member(StructItem):
        _0: typing.Final[UnboundDecl]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: UnboundDecl) -> StructItem.Member: ...

    @typing.final
    class Metadata(StructItem):
        _0: typing.Final[MetadataSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: MetadataSection) -> StructItem.Metadata: ...

    @typing.final
    class ParameterMetadata(StructItem):
        _0: typing.Final[ParameterMetadataSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(
            cls, _0: ParameterMetadataSection
        ) -> StructItem.ParameterMetadata: ...

@typing.final
class StructKeyword(AstToken):
    pass

@typing.final
class SubtractionExpr(AstNode):
    def operands(self) -> tuple[Expr, Expr]: ...

@typing.final
class SymbolicModulePath(AstNode):
    def components(self) -> list[Ident]: ...
    def text(self) -> str: ...

@typing.final
class TaskDefinition(AstNode):
    def command(self) -> CommandSection | None: ...
    def declarations(self) -> list[BoundDecl]: ...
    def hints(self) -> TaskHintsSection | None: ...
    def input(self) -> InputSection | None: ...
    def items(self) -> list[TaskItem]: ...
    def keyword(self) -> TaskKeyword: ...
    def metadata(self) -> MetadataSection | None: ...
    def name(self) -> Ident: ...
    def output(self) -> OutputSection | None: ...
    def parameter_metadata(self) -> ParameterMetadataSection | None: ...
    def requirements(self) -> RequirementsSection | None: ...
    def runtime(self) -> RuntimeSection | None: ...

@typing.final
class TaskHintsItem(AstNode):
    def expr(self) -> Expr: ...
    def name(self) -> Ident: ...

@typing.final
class TaskHintsSection(AstNode):
    def items(self) -> list[TaskHintsItem]: ...
    def parent(self) -> TaskDefinition: ...

@disjoint_base
class TaskItem:
    @typing.final
    class Command(TaskItem):
        _0: typing.Final[CommandSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: CommandSection) -> TaskItem.Command: ...

    @typing.final
    class Declaration(TaskItem):
        _0: typing.Final[BoundDecl]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: BoundDecl) -> TaskItem.Declaration: ...

    @typing.final
    class Hints(TaskItem):
        _0: typing.Final[TaskHintsSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: TaskHintsSection) -> TaskItem.Hints: ...

    @typing.final
    class Input(TaskItem):
        _0: typing.Final[InputSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: InputSection) -> TaskItem.Input: ...

    @typing.final
    class Metadata(TaskItem):
        _0: typing.Final[MetadataSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: MetadataSection) -> TaskItem.Metadata: ...

    @typing.final
    class Output(TaskItem):
        _0: typing.Final[OutputSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: OutputSection) -> TaskItem.Output: ...

    @typing.final
    class ParameterMetadata(TaskItem):
        _0: typing.Final[ParameterMetadataSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(
            cls, _0: ParameterMetadataSection
        ) -> TaskItem.ParameterMetadata: ...

    @typing.final
    class Requirements(TaskItem):
        _0: typing.Final[RequirementsSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: RequirementsSection) -> TaskItem.Requirements: ...

    @typing.final
    class Runtime(TaskItem):
        _0: typing.Final[RuntimeSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: RuntimeSection) -> TaskItem.Runtime: ...

@typing.final
class TaskKeyword(AstToken):
    pass

@typing.final
class ThenKeyword(AstToken):
    pass

@typing.final
class TrueFalseOption(AstNode):
    def values(self) -> tuple[LiteralString, LiteralString]: ...

@typing.final
class TrueKeyword(AstToken):
    pass

@disjoint_base
class Type:
    def is_optional(self) -> bool: ...
    @typing.final
    class Array(Type):
        _0: typing.Final[ArrayType]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ArrayType) -> Type.Array: ...

    @typing.final
    class Map(Type):
        _0: typing.Final[MapType]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: MapType) -> Type.Map: ...

    @typing.final
    class Object(Type):
        _0: typing.Final[ObjectType]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ObjectType) -> Type.Object: ...

    @typing.final
    class Pair(Type):
        _0: typing.Final[PairType]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: PairType) -> Type.Pair: ...

    @typing.final
    class Primitive(Type):
        _0: typing.Final[PrimitiveType]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: PrimitiveType) -> Type.Primitive: ...

    @typing.final
    class Ref(Type):
        _0: typing.Final[TypeRef]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: TypeRef) -> Type.Ref: ...

@typing.final
class TypeRef(AstNode):
    def is_optional(self) -> bool: ...
    def name(self) -> Ident: ...

@typing.final
class UnboundDecl(AstNode):
    def env(self) -> EnvKeyword | None: ...
    def name(self) -> Ident: ...
    def ty(self) -> Type: ...

@typing.final
class Unknown(AstToken):
    pass

@typing.final
class VersionKeyword(AstToken):
    pass

@typing.final
class WorkflowDefinition(AstNode):
    def allows_nested_inputs(self, version: SupportedVersion) -> bool: ...
    def declarations(self) -> list[BoundDecl]: ...
    def hints(self) -> WorkflowHintsSection | None: ...
    def input(self) -> InputSection | None: ...
    def items(self) -> list[WorkflowItem]: ...
    def keyword(self) -> WorkflowKeyword: ...
    def metadata(self) -> MetadataSection | None: ...
    def name(self) -> Ident: ...
    def output(self) -> OutputSection | None: ...
    def parameter_metadata(self) -> ParameterMetadataSection | None: ...
    def statements(self) -> list[WorkflowStatement]: ...

@typing.final
class WorkflowHintsArray(AstNode):
    def elements(self) -> list[WorkflowHintsItemValue]: ...

@typing.final
class WorkflowHintsItem(AstNode):
    def name(self) -> Ident: ...
    def value(self) -> WorkflowHintsItemValue: ...

@disjoint_base
class WorkflowHintsItemValue:
    @typing.final
    class Array(WorkflowHintsItemValue):
        _0: typing.Final[WorkflowHintsArray]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: WorkflowHintsArray) -> WorkflowHintsItemValue.Array: ...

    @typing.final
    class Boolean(WorkflowHintsItemValue):
        _0: typing.Final[LiteralBoolean]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralBoolean) -> WorkflowHintsItemValue.Boolean: ...

    @typing.final
    class Float(WorkflowHintsItemValue):
        _0: typing.Final[LiteralFloat]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralFloat) -> WorkflowHintsItemValue.Float: ...

    @typing.final
    class Integer(WorkflowHintsItemValue):
        _0: typing.Final[LiteralInteger]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralInteger) -> WorkflowHintsItemValue.Integer: ...

    @typing.final
    class Object(WorkflowHintsItemValue):
        _0: typing.Final[WorkflowHintsObject]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: WorkflowHintsObject) -> WorkflowHintsItemValue.Object: ...

    @typing.final
    class String(WorkflowHintsItemValue):
        _0: typing.Final[LiteralString]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: LiteralString) -> WorkflowHintsItemValue.String: ...

@typing.final
class WorkflowHintsObject(AstNode):
    def items(self) -> list[WorkflowHintsObjectItem]: ...

@typing.final
class WorkflowHintsObjectItem(AstNode):
    def name(self) -> Ident: ...
    def value(self) -> WorkflowHintsItemValue: ...

@typing.final
class WorkflowHintsSection(AstNode):
    def items(self) -> list[WorkflowHintsItem]: ...
    def parent(self) -> WorkflowDefinition: ...

@disjoint_base
class WorkflowItem:
    @typing.final
    class Call(WorkflowItem):
        _0: typing.Final[CallStatement]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: CallStatement) -> WorkflowItem.Call: ...

    @typing.final
    class Conditional(WorkflowItem):
        _0: typing.Final[ConditionalStatement]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ConditionalStatement) -> WorkflowItem.Conditional: ...

    @typing.final
    class Declaration(WorkflowItem):
        _0: typing.Final[BoundDecl]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: BoundDecl) -> WorkflowItem.Declaration: ...

    @typing.final
    class Hints(WorkflowItem):
        _0: typing.Final[WorkflowHintsSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: WorkflowHintsSection) -> WorkflowItem.Hints: ...

    @typing.final
    class Input(WorkflowItem):
        _0: typing.Final[InputSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: InputSection) -> WorkflowItem.Input: ...

    @typing.final
    class Metadata(WorkflowItem):
        _0: typing.Final[MetadataSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: MetadataSection) -> WorkflowItem.Metadata: ...

    @typing.final
    class Output(WorkflowItem):
        _0: typing.Final[OutputSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: OutputSection) -> WorkflowItem.Output: ...

    @typing.final
    class ParameterMetadata(WorkflowItem):
        _0: typing.Final[ParameterMetadataSection]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(
            cls, _0: ParameterMetadataSection
        ) -> WorkflowItem.ParameterMetadata: ...

    @typing.final
    class Scatter(WorkflowItem):
        _0: typing.Final[ScatterStatement]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ScatterStatement) -> WorkflowItem.Scatter: ...

@typing.final
class WorkflowKeyword(AstToken):
    pass

@disjoint_base
class WorkflowStatement:
    @typing.final
    class Call(WorkflowStatement):
        _0: typing.Final[CallStatement]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: CallStatement) -> WorkflowStatement.Call: ...

    @typing.final
    class Conditional(WorkflowStatement):
        _0: typing.Final[ConditionalStatement]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ConditionalStatement) -> WorkflowStatement.Conditional: ...

    @typing.final
    class Declaration(WorkflowStatement):
        _0: typing.Final[BoundDecl]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: BoundDecl) -> WorkflowStatement.Declaration: ...

    @typing.final
    class Scatter(WorkflowStatement):
        _0: typing.Final[ScatterStatement]
        __match_args__ = ("_0",)

        def __getitem__(self, key: int, /) -> typing.Any: ...
        def __len__(self) -> int: ...
        def __new__(cls, _0: ScatterStatement) -> WorkflowStatement.Scatter: ...

__all__ = [
    "AccessExpr",
    "AdditionExpr",
    "AfterKeyword",
    "AliasKeyword",
    "ArrayType",
    "ArrayTypeKeyword",
    "AsKeyword",
    "Assignment",
    "Ast",
    "Asterisk",
    "BooleanTypeKeyword",
    "BoundDecl",
    "CallAfter",
    "CallAlias",
    "CallExpr",
    "CallInputItem",
    "CallKeyword",
    "CallStatement",
    "CallTarget",
    "CloseBrace",
    "CloseBracket",
    "CloseHeredoc",
    "CloseParen",
    "Colon",
    "Comma",
    "CommandKeyword",
    "CommandPart",
    "CommandSection",
    "CommandText",
    "ConditionalStatement",
    "ConditionalStatementClause",
    "ConditionalStatementClauseKind",
    "Decl",
    "DefaultOption",
    "DirectoryTypeKeyword",
    "DivisionExpr",
    "DocumentItem",
    "Dot",
    "DoubleQuote",
    "ElseKeyword",
    "EnumChoice",
    "EnumDefinition",
    "EnumKeyword",
    "EnumTypeParameter",
    "EnvKeyword",
    "Equal",
    "EqualityExpr",
    "Exclamation",
    "Exponentiation",
    "ExponentiationExpr",
    "Expr",
    "FalseKeyword",
    "FileTypeKeyword",
    "Float",
    "FloatTypeKeyword",
    "FromKeyword",
    "Greater",
    "GreaterEqual",
    "GreaterEqualExpr",
    "GreaterExpr",
    "HintsKeyword",
    "IfExpr",
    "IfKeyword",
    "ImportAlias",
    "ImportForm",
    "ImportKeyword",
    "ImportMember",
    "ImportMembers",
    "ImportSource",
    "ImportStatement",
    "InKeyword",
    "IndexExpr",
    "InequalityExpr",
    "InputKeyword",
    "InputSection",
    "IntTypeKeyword",
    "Integer",
    "Less",
    "LessEqual",
    "LessEqualExpr",
    "LessExpr",
    "LiteralArray",
    "LiteralBoolean",
    "LiteralExpr",
    "LiteralFloat",
    "LiteralHints",
    "LiteralHintsItem",
    "LiteralInput",
    "LiteralInputItem",
    "LiteralInteger",
    "LiteralMap",
    "LiteralMapItem",
    "LiteralNone",
    "LiteralNull",
    "LiteralObject",
    "LiteralObjectItem",
    "LiteralOutput",
    "LiteralOutputItem",
    "LiteralPair",
    "LiteralString",
    "LiteralStringKind",
    "LiteralStringText",
    "LiteralStruct",
    "LiteralStructItem",
    "LogicalAnd",
    "LogicalAndExpr",
    "LogicalNotExpr",
    "LogicalOr",
    "LogicalOrExpr",
    "MapType",
    "MapTypeKeyword",
    "MetaKeyword",
    "MetadataArray",
    "MetadataObject",
    "MetadataObjectItem",
    "MetadataSection",
    "MetadataValue",
    "Minus",
    "ModuloExpr",
    "MultiplicationExpr",
    "NameRefExpr",
    "NegationExpr",
    "NoneKeyword",
    "NotEqual",
    "NullKeyword",
    "ObjectKeyword",
    "ObjectType",
    "ObjectTypeKeyword",
    "OpenBrace",
    "OpenBracket",
    "OpenHeredoc",
    "OpenParen",
    "OutputKeyword",
    "OutputSection",
    "PairType",
    "PairTypeKeyword",
    "ParameterMetaKeyword",
    "ParameterMetadataSection",
    "ParenthesizedExpr",
    "Percent",
    "Placeholder",
    "PlaceholderOpen",
    "PlaceholderOption",
    "Plus",
    "PrimitiveType",
    "PrimitiveTypeKind",
    "QuestionMark",
    "RequirementsItem",
    "RequirementsKeyword",
    "RequirementsSection",
    "RuntimeItem",
    "RuntimeKeyword",
    "RuntimeSection",
    "ScatterKeyword",
    "ScatterStatement",
    "SectionParent",
    "SepOption",
    "SingleQuote",
    "Slash",
    "StringPart",
    "StringText",
    "StringTypeKeyword",
    "StructDefinition",
    "StructItem",
    "StructKeyword",
    "SubtractionExpr",
    "SymbolicModulePath",
    "TaskDefinition",
    "TaskHintsItem",
    "TaskHintsSection",
    "TaskItem",
    "TaskKeyword",
    "ThenKeyword",
    "TrueFalseOption",
    "TrueKeyword",
    "Type",
    "TypeRef",
    "UnboundDecl",
    "Unknown",
    "VersionKeyword",
    "WorkflowDefinition",
    "WorkflowHintsArray",
    "WorkflowHintsItem",
    "WorkflowHintsItemValue",
    "WorkflowHintsObject",
    "WorkflowHintsObjectItem",
    "WorkflowHintsSection",
    "WorkflowItem",
    "WorkflowKeyword",
    "WorkflowStatement",
]
