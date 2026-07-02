import typing

@typing.final
class V1:
    ZERO: V1
    ONE: V1
    TWO: V1
    THREE: V1
    FOUR: V1

    def __lt__(self, other: typing.Any, /) -> bool: ...
    def __le__(self, other: typing.Any, /) -> bool: ...
    def __gt__(self, other: typing.Any, /) -> bool: ...
    def __ge__(self, other: typing.Any, /) -> bool: ...

__all__ = ["V1"]
