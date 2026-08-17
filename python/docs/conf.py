# Configuration file for the Sphinx documentation builder.

# Project information

project = "Sprocket"
author = "Sprocket Contributors"
copyright = "%Y, Sprocket Contributors"

# General configuration

extensions = [
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
