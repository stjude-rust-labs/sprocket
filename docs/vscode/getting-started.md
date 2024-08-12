# Getting Started

Sprocket provides an extension for the popular [Visual Studio
Code](https://code.visualstudio.com/) editor. You can access the published
version of the extension in the [Visual Studio
Marketplace](https://marketplace.visualstudio.com/items?itemName=stjude-rust-labs.sprocket-vscode).

This extension has the following features:

* **Syntax highlighting** using a complete and up-to-date [TextMate grammar].
* **Code snippets** for common constructs and conventions are included.
* **Document and workspace diagnostics** courtesy of the language server
  protocol implementation provided by `sprocket analyzer`.

The source code for the extension is available to explore, modify, and build
yourself at
[`stjude-rust-labs/sprocket-vscode`](https://github.com/stjude-rust-labs/sprocket-vscode).

## Installation

::: warning
The Sprocket Visual Studio Code extension is currently in very early
development. As such, you currently have to download and install the latest
version of the `sprocket` command line tool manually before running the
extension. You may also experience various UX issues, such as needing to
manually restart the Sprocket extension if it crashes. We plan to improve all of
these things as we continue to iterate.
:::

To use the Sprocket Visual Studio Code extension, you first need to install the
`sprocket` command line tool. You can do so by executing these commands.

::: code-group

```shell
# (1) Ensure Rust is installed by following the instructions at
#     https://rustup.rs.

# (2) Install the latest version of `sprocket` available.
cargo install --git https://github.com/stjude-rust-labs/sprocket

# (3) Make sure `sprocket` is accessible from the command line.
sprocket --version
```

:::

Next, you can install the extension from the **Extensions** tab in Visual Studio
Code ([guide](https://code.visualstudio.com/docs/editor/extension-marketplace))
or visit the extension's page in the Visual Studio Marketplace
([link](https://marketplace.visualstudio.com/items?itemName=stjude-rust-labs.sprocket-vscode)).

## Known Issues

See the [known
issues](https://github.com/stjude-rust-labs/sprocket-vscode?tab=readme-ov-file#known-issues)
section of the `README.md` for a list of known issues.

[TextMate grammar]: https://macromates.com/manual/en/language_grammars
