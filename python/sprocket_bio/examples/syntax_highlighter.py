import argparse
import html
import os
import pathlib
import sys
import webbrowser

from sprocket_bio.diagnostics import Mode, emit_diagnostics
from sprocket_bio.grammar import Severity, SupportedVersion, SyntaxKind
from sprocket_bio.grammar.grammar import document
from sprocket_bio.grammar.parser import Event
from sprocket_bio.grammar.version import V1


def parse_args() -> argparse.Namespace:
    """Parses command-line arguments and returns them."""

    parser = argparse.ArgumentParser(
        description="Syntax highlights a WDL document, outputting HTML that can be viewed in your browser"
    )

    parser.add_argument(
        "source_file",
        type=pathlib.Path,
        help="path to WDL document",
        metavar="<source_file>",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=pathlib.Path,
        help="the path to write to, defaults to <source_file>.html",
        metavar="<OUTPUT_FILE>",
    )
    parser.add_argument(
        "--open", action="store_true", help="opens the output in a web browser"
    )

    args = parser.parse_args()

    # If the output is not specified, set it to the source_file with an ".html" suffix.
    if args.output is None:
        args.output = args.source_file.with_suffix(args.source_file.suffix + ".html")

    return args


def syntax_kind_color(kind: SyntaxKind) -> str | None:
    """Returns the color a token of a specific `kind` should be highlighted, or `None` if it shouldn't."""

    match kind:
        # Keywords
        case _ if kind.is_keyword():
            return "#99A7F1"
        # Types
        case _ if kind.is_type():
            return "#BA9CFF"
        # Operators
        case _ if kind.is_operator():
            return "#9CB2FF"
        # Literals
        case (
            SyntaxKind.INTEGER
            | SyntaxKind.FLOAT
            | SyntaxKind.DOUBLE_QUOTE
            | SyntaxKind.SINGLE_QUOTE
            | SyntaxKind.LITERAL_STRING_TEXT
            | SyntaxKind.LITERAL_COMMAND_TEXT
            | SyntaxKind.TRUE_KEYWORD
            | SyntaxKind.FALSE_KEYWORD
            | SyntaxKind.NONE_KEYWORD
            | SyntaxKind.VERSION
        ):
            return "#E59CFF"
        # Comments
        case SyntaxKind.COMMENT:
            return "#7780A3"
        # Punctuation
        case (
            SyntaxKind.OPEN_BRACE
            | SyntaxKind.CLOSE_BRACE
            | SyntaxKind.OPEN_HEREDOC
            | SyntaxKind.CLOSE_HEREDOC
            | SyntaxKind.OPEN_BRACKET
            | SyntaxKind.CLOSE_BRACKET
            | SyntaxKind.OPEN_PAREN
            | SyntaxKind.CLOSE_PAREN
            | SyntaxKind.PLACEHOLDER_OPEN
            | SyntaxKind.COLON
        ):
            return "#BBBBBB"
        # Everything else
        case _:
            return None


def main() -> None:
    args = parse_args()

    with open(args.source_file, mode="rt", encoding="utf-8") as f:
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

    # Do not overwrite the source file.
    if os.path.samefile(args.source_file, args.output):
        sys.exit(
            f"source and output files are the same, refusing to overwrite it: {args.output}"
        )

    with open(args.output, mode="wt", encoding="utf-8") as f:
        # Write the prelude of the HTML document and configure the base style.
        f.write(
            '<!DOCTYPE html><html><body style="background: #070A19; color: white"><pre>'
        )

        # Get the byte representation of the source code.
        source_bytes = source.encode("utf-8")

        for event in events:
            # Filter the parser events for tokens, which represent individual pieces of syntax.
            if isinstance(event, Event.Token):
                # Get the text in the source code represented by this span. `Span`s index by bytes,
                # not Unicode code points, which is why we slice the byte representation of the
                # source. We also escape the text, so it is safe to include in HTML.
                token_text = html.escape(
                    source_bytes[event.span.start : event.span.end].decode("utf-8")
                )

                # If this token should be colored then write it inside of a styled `<span>`, else
                # just write the plain text.
                if color := syntax_kind_color(event.kind):
                    f.write(f'<span style="color: {color}">{token_text}</span>')
                else:
                    f.write(token_text)

        # Wrap up the HTML document.
        f.write("</pre></body></html>")

    # Open the HTML document in the default browser if `--open` if specified.
    if args.open:
        webbrowser.open(args.output.absolute().as_uri())


if __name__ == "__main__":
    main()
