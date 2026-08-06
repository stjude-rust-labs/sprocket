import typing

from .grammar import Diagnostic

@typing.final
class Mode:
    FULL: Mode
    ONE_LINE: Mode

    @staticmethod
    def default() -> Mode: ...

def emit_diagnostics(
    path: str,
    source: str,
    diagnostics: list[Diagnostic],
    report_mode: Mode,
    colorize: bool,
) -> None: ...

__all__ = ["Mode", "emit_diagnostics"]
