from sprocket_bio.diagnostics import emit_diagnostics, Mode
from sprocket_bio.grammar.parser import Event
from sprocket_bio.grammar.version import V1
from sprocket_bio.grammar import SupportedVersion, SyntaxKind, Severity, Span
from sprocket_bio.grammar.grammar import document


def test_document_version_statement() -> None:
    source = "version 1.4"

    events, [] = document(source, None)

    expected = [
        Event.NodeStarted(SyntaxKind.ROOT_NODE, None),
        Event.NodeStarted(SyntaxKind.VERSION_STATEMENT_NODE, None),
        Event.Token(SyntaxKind.VERSION_KEYWORD, Span(0, 7)),
        Event.Token(SyntaxKind.WHITESPACE, Span(7, 1)),
        Event.Token(SyntaxKind.VERSION, Span(8, 3)),
        Event.NodeFinished(),
        Event.NodeFinished(),
    ]

    assert events == expected


def test_document_empty_call() -> None:
    source = """\
# This is a test of an empty call.

version 1.1

workflow test {
    call x { }
}"""

    events, [] = document(source, None)

    expected = [
        Event.NodeStarted(SyntaxKind.ROOT_NODE, None),
        Event.Token(SyntaxKind.COMMENT, Span(0, 34)),
        Event.Token(SyntaxKind.WHITESPACE, Span(34, 2)),
        Event.NodeStarted(SyntaxKind.VERSION_STATEMENT_NODE, None),
        Event.Token(SyntaxKind.VERSION_KEYWORD, Span(36, 7)),
        Event.Token(SyntaxKind.WHITESPACE, Span(43, 1)),
        Event.Token(SyntaxKind.VERSION, Span(44, 3)),
        Event.NodeFinished(),
        Event.Token(SyntaxKind.WHITESPACE, Span(47, 2)),
        Event.NodeStarted(SyntaxKind.WORKFLOW_DEFINITION_NODE, None),
        Event.Token(SyntaxKind.WORKFLOW_KEYWORD, Span(49, 8)),
        Event.Token(SyntaxKind.WHITESPACE, Span(57, 1)),
        Event.Token(SyntaxKind.IDENT, Span(58, 4)),
        Event.Token(SyntaxKind.WHITESPACE, Span(62, 1)),
        Event.Token(SyntaxKind.OPEN_BRACE, Span(63, 1)),
        Event.Token(SyntaxKind.WHITESPACE, Span(64, 5)),
        Event.NodeStarted(SyntaxKind.CALL_STATEMENT_NODE, None),
        Event.Token(SyntaxKind.CALL_KEYWORD, Span(69, 4)),
        Event.Token(SyntaxKind.WHITESPACE, Span(73, 1)),
        Event.NodeStarted(SyntaxKind.CALL_TARGET_NODE, None),
        Event.Token(SyntaxKind.IDENT, Span(74, 1)),
        Event.NodeFinished(),
        Event.Token(SyntaxKind.WHITESPACE, Span(75, 1)),
        Event.Token(SyntaxKind.OPEN_BRACE, Span(76, 1)),
        Event.Token(SyntaxKind.WHITESPACE, Span(77, 1)),
        Event.Token(SyntaxKind.CLOSE_BRACE, Span(78, 1)),
        Event.NodeFinished(),
        Event.Token(SyntaxKind.WHITESPACE, Span(79, 1)),
        Event.Token(SyntaxKind.CLOSE_BRACE, Span(80, 1)),
        Event.NodeFinished(),
        Event.NodeFinished(),
    ]

    assert events == expected


def test_document_error() -> None:
    events, [diagnostic] = document("", SupportedVersion.V1(V1.ZERO))

    assert events == [
        Event.NodeStarted(SyntaxKind.ROOT_NODE, None),
        Event.NodeFinished(),
    ]

    assert diagnostic.severity is Severity.ERROR
    assert "must start with a version statement" in diagnostic.message
