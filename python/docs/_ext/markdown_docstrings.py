import typing

from markdown_it.renderer import RendererHTML
from myst_parser.config.main import MdParserConfig
from myst_parser.parsers.mdit import create_md_parser
from sphinx.application import Sphinx
from sphinx.util.typing import ExtensionMetadata


def docstring(
    app: Sphinx,
    what: str,
    name: str,
    obj: typing.Any,
    options: typing.Any,
    lines: list[str],
) -> None:
    # Do not go through the process of parsing if there is no docstring.
    if len(lines) == 0:
        return

    # Create a parser using MyST's config that renders HTML.
    mdit = create_md_parser(app.env.myst_config, RendererHTML)  # type: ignore[attr-defined] # `app.env.myst_config` is defined by the `myst_parser` extension.

    # Render the Markdown into HTML.
    markdown = "\n".join(lines)
    html: str = mdit.render(markdown)

    # Replace the docstring with HTML embedded in RST.
    lines.clear()

    lines += [".. raw:: html", ""]

    for line in html.splitlines():
        lines.append(f"   {line}")


def setup(app: Sphinx) -> ExtensionMetadata:
    # These extensions are needed before this can run.
    app.setup_extension("sphinx.ext.autodoc")
    app.setup_extension("myst_parser")

    # Register our docstring event listener with the greatest priority, so it runs first.
    app.connect("autodoc-process-docstring", docstring, priority=0)

    return {
        "env_version": 1,
        "parallel_read_safe": True,
        "parallel_write_safe": True,
    }
