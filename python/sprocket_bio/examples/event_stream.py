import pathlib
import argparse
from sprocket_bio.grammar.parser import Event
import sys
from sprocket_bio.diagnostics import Mode, emit_diagnostics
from sprocket_bio.grammar.version import V1
from sprocket_bio.grammar import SupportedVersion, Severity
from sprocket_bio.grammar.grammar import document


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
                print(
                    "  " * indent + f"{kind}@{span} {source[span.start : span.end]!r}"
                )


if __name__ == "__main__":
    main()
