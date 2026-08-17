# Configuration file for the Sphinx documentation builder.

# Project information

project = "Sprocket"
author = "Sprocket Contributors"
copyright = "%Y, Sprocket Contributors"

# General configuration

extensions = [
    # Track how long it takes to render pages.
    "sphinx.ext.duration",
    # Markdown support.
    "myst_parser",
    # Sphinx Read the Docs theme.
    "sphinx_rtd_theme",
]
nitpicky = True
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]
source_suffix = {
    ".md": "markdown",
    ".txt": "markdown",
}

# Options for HTML output

html_theme = "sphinx_rtd_theme"

# Options for Markdown

myst_enable_extension = []
