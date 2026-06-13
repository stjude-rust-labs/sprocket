from sprocket_bio.grammar import Diagnostic, Label, Span

import pytest


def test_diagnostic_builder():
    d = (
        Diagnostic.error("an error occurred")
        .with_highlight(Span(2, 3))
        .with_fix("don't do that")
    )

    assert d.message == "an error occurred"
    assert d.labels == [Label("", Span(2, 3))]
    assert d.fix == "don't do that"


def test_label_new():
    label = Label("My message", Span(0, 10))

    assert label.message == "My message"
    assert label.span == Span(0, 10)


def test_span_getters():
    x = Span(3, 10)

    assert x.start == 3
    assert x.end == 13

    with pytest.raises(AttributeError, match="is not writable"):
        x.start = 0

    with pytest.raises(AttributeError, match="is not writable"):
        x.end = 0


def test_span_len():
    x = Span(5, 10)

    assert x.len() == 10
    assert len(x) == 10


def test_span_is_empty():
    x = Span(1, 0)
    assert x.is_empty()


def test_span_contains():
    x = Span(3, 3)

    assert not x.contains(2)
    assert x.contains(3)
    assert x.contains(5)
    assert not x.contains(6)


def test_span_intersect():
    x = Span(0, 5)
    y = Span(3, 4)

    assert x.intersect(y) == Span(3, 2)


def test_span_eq():
    a = Span(2, 3)
    b = Span(2, 3)
    c = Span(3, 0)

    assert a == a
    assert a == b
    assert a != c


def test_span_ord():
    lesser = Span(3, 2)
    greater = Span(8, 2)

    assert lesser < greater
    assert greater > lesser
