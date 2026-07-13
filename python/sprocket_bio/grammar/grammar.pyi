from sprocket_bio.grammar import Diagnostic, SupportedVersion
from sprocket_bio.grammar.parser import Event

def document(
    source: str, fallback_version: SupportedVersion | None
) -> tuple[list[Event], list[Diagnostic]]: ...

__all__ = ["document"]
