import argparse
import pathlib
import sys

from sprocket_bio.diagnostics import Mode, emit_diagnostics
from sprocket_bio.grammar import Severity, SupportedVersion
from sprocket_bio.grammar.grammar import document
from sprocket_bio.grammar.parser import Event
from sprocket_bio.grammar.version import V1


def parse_args() -> argparse.Namespace:
    """Parses command-line arguments and returns them."""

    parser = argparse.ArgumentParser(
        description="Parses a WDL document and prints the event stream"
    )

    parser.add_argument(
        "source_file",
        type=pathlib.Path,
        help="path to WDL document",
        metavar="<source_file>",
    )

    return parser.parse_args()


def main() -> None:
    args = parse_args()

    with open(args.source_file, "rt", encoding="utf-8") as f:
        source = f.read()

    # Parse the source string into a list of parser events.
    events, diagnostics = document(
        source, fallback_version=SupportedVersion.V1(V1.ZERO)
    )

    # Emit diagnostics, if there are any.
    emit_diagnostics(
        str(args.source_file), source, diagnostics, report_mode=Mode.FULL, colorize=True
    )

    # If any of the diagnostics are errors, exit.
    if any([d.severity is Severity.ERROR for d in diagnostics]):
        sys.exit(1)

    # Get the byte representation of the source code.
    source_bytes = source.encode("utf-8")

    indent = 0

    # Print all events in the stream.
    for event in events:
        match event:
            case Event.NodeStarted(kind, _):
                print("  " * indent + str(kind))
                indent += 1
            case Event.NodeFinished():
                indent -= 1
            case Event.Token(kind, span):
                # Get the text in the source code represented by this span. We convert the string
                # to bytes first, as `Span`s index bytes instead of Unicode code points. Not doing
                # so will result in the wrong text being displayed.
                token_text = source_bytes[span.start : span.end].decode("utf-8")
                print("  " * indent + f"{kind}@{span} {token_text!r}")


if __name__ == "__main__":
    main()
