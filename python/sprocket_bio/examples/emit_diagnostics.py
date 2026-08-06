import os

from sprocket_bio.diagnostics import Mode, emit_diagnostics
from sprocket_bio.grammar import Diagnostic, Span

diagnostics = [
    # Annotate the `command` section with an error.
    Diagnostic.error("this is an error").with_highlight(Span(77, 54)),
    # Annotate the `version` statement with a warning.
    Diagnostic.warning("this is a warning")
    .with_label("additional details on the warning", Span(0, 11))
    .with_help("this is the help message"),
]


def main() -> None:
    # Get the full path of `example.wdl` from this script's location.
    workflow_path = os.path.join(
        os.path.dirname(os.path.realpath(__file__)), "example.wdl"
    )

    with open(workflow_path, mode="rt", encoding="utf-8") as f:
        workflow_source = f.read()

    emit_diagnostics(
        workflow_path,
        workflow_source,
        diagnostics,
        report_mode=Mode.FULL,
        colorize=True,
    )


if __name__ == "__main__":
    main()
