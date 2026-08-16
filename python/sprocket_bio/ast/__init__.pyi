import typing

from typing_extensions import disjoint_base

from sprocket_bio.grammar import Span, SupportedVersion

from . import v1

@disjoint_base
class Ast:
    @typing.final
    class Unsupported(Ast):
        __match_args__ = ()

    @typing.final
    class V1(Ast):
        _0: typing.Final[v1.Ast]
        __match_args__ = ("_0",)

        def __new__(cls, _0: v1.Ast) -> Ast.V1: ...

class AstNode:
    pass

class AstToken:
    pass

@typing.final
class Comment(AstToken):
    def directive(self) -> Directive | None: ...
    def is_inline_comment(self, /) -> bool: ...
    def kind(self) -> CommentKind: ...

@disjoint_base
class CommentKind:
    @typing.final
    class Directive(CommentKind):
        _0: typing.Final[DirectiveKind]
        __match_args__ = ("_0",)

        def __new__(cls, _0: DirectiveKind) -> CommentKind.Directive: ...

    @typing.final
    class Documentation(CommentKind):
        __match_args__ = ()

        def __new__(cls) -> CommentKind.Documentation: ...

    @typing.final
    class Line(CommentKind):
        __match_args__ = ()

        def __new__(cls) -> CommentKind.Line: ...

@disjoint_base
class Directive:
    @typing.final
    class Except(Directive):
        _0: typing.Final[ExceptRule]
        __match_args__ = ("_0",)

        def __new__(cls, _0: set[ExceptRule]) -> Directive.Except: ...

@typing.final
class DirectiveKind:
    EXCEPT: DirectiveKind

@typing.final
class Document(AstNode):
    @staticmethod
    def parse(source: str, fallback_version: SupportedVersion | None) -> Document: ...
    def ast(self) -> Ast: ...
    def ast_with_version_fallback(
        self, fallback_version: SupportedVersion | None
    ) -> Ast: ...
    def version_statement(self, /) -> VersionStatement | None: ...

@typing.final
class ExceptRule:
    name: str
    span: Span

@typing.final
class Ident(AstToken):
    def hashable(self, /) -> TokenText: ...

@typing.final
class TokenText(AstToken):
    pass

@typing.final
class Version(AstToken):
    pass

@typing.final
class VersionStatement(AstNode):
    def keyword(self, /) -> v1.VersionKeyword: ...
    def version(self, /) -> Version: ...

@typing.final
class Whitespace(AstToken):
    pass

__all__ = [
    "Ast",
    "AstNode",
    "AstToken",
    "Comment",
    "CommentKind",
    "Directive",
    "DirectiveKind",
    "Document",
    "ExceptRule",
    "Ident",
    "TokenText",
    "Version",
    "VersionStatement",
    "Whitespace",
    "v1",
]
