from sprocket_bio.grammar.parser import Event
from sprocket_bio.grammar import SupportedVersion, Diagnostic

def document(
    source: str, fallback_version: SupportedVersion | None
) -> tuple[list[Event], list[Diagnostic]]: ...

__all__ = ["document"]
