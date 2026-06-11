import typing

@typing.final
class Mode:
    FULL: Mode
    ONE_LINE: Mode

    @staticmethod
    def default() -> Mode: ...

__all__ = ["Mode"]
