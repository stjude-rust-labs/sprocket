# Configuration file for the Sphinx documentation builder.

import sys

from pathlib import Path

# Add our custom extensions to the path so Sphinx can import them.
sys.path.append(str(Path("_ext").resolve()))

# Project information

project = "Sprocket"
author = "Sprocket Contributors"
copyright = "%Y, Sprocket Contributors"

# General configuration

extensions = [
    # Convert Markdown docstrings into HTML. (This is a custom extension defined in
    # `_ext/markdown_docstrings.py`.)
    "markdown_docstrings",
    # Markdown parsing.
    "myst_parser",
    # Automatically generate API docs.
    "sphinx.ext.autodoc",
    # Track how long it takes to render pages.
    "sphinx.ext.duration",
    # Sphinx Read the Docs theme.
    "sphinx_rtd_theme",
]
nitpicky = True
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]

# Options for HTML output

html_theme = "sphinx_rtd_theme"

# Extension configuration

autodoc_member_order = "groupwise"
autodoc_default_options = {
    "members": True,
    "undoc-members": True,
    "show-inheritance": True,
}
autodoc_use_type_comments = False
