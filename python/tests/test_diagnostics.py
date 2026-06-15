from pytest import CaptureFixture
from sprocket_bio.grammar import Diagnostic, Span
from sprocket_bio.diagnostics import Mode, emit_diagnostics


def test_mode_eq():
    assert Mode.FULL == Mode.FULL
    assert Mode.ONE_LINE == Mode.ONE_LINE
    assert Mode.FULL != Mode.ONE_LINE

    assert Mode.FULL is Mode.FULL
    assert Mode.ONE_LINE is Mode.ONE_LINE
    assert Mode.FULL is not Mode.ONE_LINE


class TestEmitDiagnostics:
    path = "tests/validation/empty-struct/source.wdl"
    source = """\
# This is a test of a having an empty struct

version 1.1

struct Test {
}"""
    diagnostic = Diagnostic.error(
        "struct `Test` must have at least one declared member"
    ).with_label("this struct cannot be empty", Span(66, 4))

    def test_full(self, capfd: CaptureFixture[str]):
        # Emit the diagnostic to stderr.
        emit_diagnostics(self.path, self.source, [self.diagnostic], Mode.FULL, False)

        # Read captured text sent to stdout and stderr.
        captured = capfd.readouterr()

        expected = """\
error: struct `Test` must have at least one declared member
  ┌─ tests/validation/empty-struct/source.wdl:5:8
  │
5 │ struct Test {
  │        ^^^^ this struct cannot be empty

"""

        assert captured.err == expected

    def test_one_line(self, capfd: CaptureFixture[str]):
        # Emit the diagnostic to stderr.
        emit_diagnostics(
            self.path, self.source, [self.diagnostic], Mode.ONE_LINE, False
        )

        # Read captured text sent to stdout and stderr.
        captured = capfd.readouterr()

        expected = "tests/validation/empty-struct/source.wdl:5:8: error: struct `Test` must have at least one declared member\n"

        assert captured.err == expected
