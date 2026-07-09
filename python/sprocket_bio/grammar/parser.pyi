from typing_extensions import disjoint_base
from sprocket_bio.grammar import SyntaxKind, Span
import typing

@disjoint_base
class Event:
    @typing.final
    class NodeStarted(Event):
        kind: SyntaxKind
        forward_parent: int | None
        __match_args__ = ("kind", "forward_parent")

        def __new__(
            cls, kind: SyntaxKind, forward_parent: int | None
        ) -> Event.NodeStarted: ...

    @typing.final
    class NodeFinished(Event):
        __match_args__ = ()

        def __new__(cls) -> Event.NodeFinished: ...

    @typing.final
    class Token(Event):
        kind: SyntaxKind
        span: Span
        __match_args__ = ("kind", "span")

        def __new__(cls, kind: SyntaxKind, span: Span) -> Event.Token: ...

    @classmethod
    def abandoned(cls) -> Event.NodeStarted: ...

__all__ = ["Event"]
