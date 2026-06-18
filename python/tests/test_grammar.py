import pytest

from sprocket_bio.grammar import Diagnostic, Label, Severity, Span, SyntaxKind


def test_diagnostic_builder() -> None:
    d = (
        Diagnostic.error("an error occurred")
        .with_highlight(Span(2, 3))
        .with_label("custom label", Span(0, 2))
        .with_fix("don't do that")
        .with_help("helpful help")
        .with_rule("LintRule")
    )

    assert d.severity is Severity.ERROR
    assert d.message == "an error occurred"
    assert sorted(d.labels) == [
        Label("custom label", Span(0, 2)),
        Label("", Span(2, 3)),
    ]
    assert d.fix == "don't do that"
    assert d.help == "helpful help"
    assert d.rule == "LintRule"


def test_diagnostic_eq() -> None:
    a = Diagnostic.note("a note").with_highlight(Span(3, 5))
    b = Diagnostic.note("a note").with_highlight(Span(3, 5))
    c = Diagnostic.warning("a note").with_highlight(Span(3, 5))

    assert a == a
    assert a == b
    assert a != c


def test_label_new() -> None:
    label = Label("My message", Span(0, 10))

    assert label.message == "My message"
    assert label.span == Span(0, 10)


def test_severity_ordering() -> None:
    assert Severity.ERROR < Severity.WARNING
    assert Severity.WARNING < Severity.NOTE
    assert Severity.ERROR < Severity.NOTE


def test_span_getters() -> None:
    x = Span(3, 10)

    assert x.start == 3
    assert x.end == 13

    with pytest.raises(AttributeError, match="is not writable"):
        x.start = 0  # type: ignore # purposefully assigning to a constant

    with pytest.raises(AttributeError, match="is not writable"):
        x.end = 0  # type: ignore # purposefully assigning to a constant


def test_span_len() -> None:
    x = Span(5, 10)

    assert x.len() == 10
    assert len(x) == 10


def test_span_is_empty() -> None:
    x = Span(1, 0)
    assert x.is_empty()


def test_span_contains() -> None:
    x = Span(3, 3)

    assert not x.contains(2)
    assert x.contains(3)
    assert x.contains(5)
    assert not x.contains(6)


def test_span_intersect() -> None:
    x = Span(0, 5)
    y = Span(3, 4)

    assert x.intersect(y) == Span(3, 2)


def test_span_eq() -> None:
    a = Span(2, 3)
    b = Span(2, 3)
    c = Span(3, 0)

    assert a == a
    assert a == b
    assert a != c


def test_span_ord() -> None:
    lesser = Span(3, 2)
    greater = Span(8, 2)

    assert lesser < greater
    assert greater > lesser


def test_span_overflow() -> None:
    with pytest.raises(
        OverflowError, match="sum of `start` and `len` is greater than or equal to"
    ):
        Span(2**64 - 1, 1)


def test_syntax_kind_is_symbolic() -> None:
    assert SyntaxKind.ABANDONED.is_symbolic()
    assert SyntaxKind.UNKNOWN.is_symbolic()
    assert SyntaxKind.UNPARSED.is_symbolic()
    assert SyntaxKind.MAX.is_symbolic()

    assert not SyntaxKind.AS_KEYWORD.is_symbolic()


def test_syntax_kind_describe() -> None:
    assert SyntaxKind.FLOAT.describe() == "float"

    with pytest.raises(match="entered unreachable code"):
        SyntaxKind.UNKNOWN.describe()

    with pytest.raises(match="entered unreachable code"):
        SyntaxKind.UNPARSED.describe()

    with pytest.raises(match="entered unreachable code"):
        SyntaxKind.ABANDONED.describe()

    with pytest.raises(match="entered unreachable code"):
        SyntaxKind.MAX.describe()


def test_syntax_kind_is_trivia() -> None:
    assert SyntaxKind.WHITESPACE.is_trivia()
    assert SyntaxKind.COMMENT.is_trivia()

    assert not SyntaxKind.ABANDONED.is_trivia()
    assert not SyntaxKind.WORKFLOW_KEYWORD.is_trivia()


def test_syntax_kind_is_keyword() -> None:
    assert SyntaxKind.ELSE_KEYWORD.is_keyword()
    assert SyntaxKind.OUTPUT_KEYWORD.is_keyword()

    assert not SyntaxKind.ARRAY_TYPE_KEYWORD.is_keyword()
    assert not SyntaxKind.LITERAL_OUTPUT_ITEM_NODE.is_keyword()


def test_syntax_kind_is_type() -> None:
    assert SyntaxKind.ARRAY_TYPE_KEYWORD.is_type()
    assert SyntaxKind.MAP_TYPE_KEYWORD.is_type()

    assert not SyntaxKind.ALIAS_KEYWORD.is_type()
    assert not SyntaxKind.ELSE_KEYWORD.is_type()


def test_syntax_kind_is_operator() -> None:
    assert SyntaxKind.PLUS.is_operator()
    assert SyntaxKind.EQUAL.is_operator()
    assert SyntaxKind.DOT.is_operator()

    assert not SyntaxKind.ABANDONED.is_operator()
    assert not SyntaxKind.COMMENT.is_operator()
