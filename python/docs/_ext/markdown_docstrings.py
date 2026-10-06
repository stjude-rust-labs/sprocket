from typing import Any

from docutils import nodes
from myst_parser.parsers.sphinx_ import MystParser
from sphinx.application import Sphinx
from sphinx.util.docutils import SphinxDirective
from sphinx.util.typing import ExtensionMetadata


class EvalMystDirective(SphinxDirective):
    has_content = True

    def run(self) -> list[nodes.Node]:
        document = self.state.document
        # Keep Sphinx's document state while isolating the nodes produced here.
        previous_children = document.children
        previous_substitution_defs = dict(document.substitution_defs)
        previous_substitution_names = dict(document.substitution_names)
        parsed_children: list[nodes.Node] = []
        document.children = parsed_children

        try:
            MystParser().parse("\n".join(self.content), document)
        finally:
            document.children = previous_children
            document.substitution_defs = previous_substitution_defs
            document.substitution_names = previous_substitution_names

        return [
            child
            for child in parsed_children
            if not isinstance(child, nodes.substitution_definition)
        ]


def process_docstring(
    app: Sphinx,
    what: str,
    name: str,
    obj: Any,
    options: dict[str, bool],
    lines: list[str],
) -> None:
    if not lines:
        return

    lines[:] = [".. eval-myst::", ""] + [f"    {line}" for line in lines]


def setup(app: Sphinx) -> ExtensionMetadata:
    app.setup_extension("sphinx.ext.autodoc")
    app.setup_extension("myst_parser")

    app.add_directive("eval-myst", EvalMystDirective)
    app.connect("autodoc-process-docstring", process_docstring, priority=0)

    return {
        "env_version": 1,
        "parallel_read_safe": True,
        "parallel_write_safe": True,
    }
