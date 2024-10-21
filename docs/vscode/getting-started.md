# Getting Started

Sprocket provides an extension for the popular [Visual Studio
Code](https://code.visualstudio.com/) editor. You can access the published
version of the extension in the [Visual Studio
Marketplace](https://marketplace.visualstudio.com/items?itemName=stjude-rust-labs.sprocket-vscode).

This extension has the following features:

* **Syntax highlighting** using a complete and up-to-date [TextMate grammar].
* **Code snippets** for common constructs and conventions are included.
* **Document and workspace diagnostics from static analysis** courtesy of the
  language server protocol implementation provided by `sprocket analyzer`.

The source code for the extension is available to explore, modify, and build
yourself at [`stjude-rust-labs/sprocket-vscode`](https://github.com/stjude-rust-labs/sprocket-vscode).

## Installation

::: warning
The Sprocket Visual Studio Code extension is currently in very early
development. You may also experience various issues, such as needing to
manually restart the Sprocket extension if it crashes. We plan to improve all
of these things as we continue to develop the extension.
:::

You can install the extension from the **Extensions** tab in Visual Studio
Code ([guide](https://code.visualstudio.com/docs/editor/extension-marketplace))
or visit the extension's page in the Visual Studio Marketplace
([link](https://marketplace.visualstudio.com/items?itemName=stjude-rust-labs.sprocket-vscode)).

### Automatically installing `sprocket`

The Sprocket extension will automatically install the latest `sprocket` command
line tool directly from GitHub the first time it is initialized.

It will also check for new versions of the `sprocket` command line tool each
time the extension is activated.

The extension will prompt you to install or update the `sprocket` command line
tool when necessary.

### Manually installing `sprocket`

If a manual installation of `sprocket` is preferred, you can install the tool
by following these instructions:

To use the Sprocket Visual Studio Code extension, you first need to install the
`sprocket` command line tool. You can do so by executing these commands.

::: code-group

```shell
# (1) Ensure Rust is installed by following the instructions at
#     https://rustup.rs.

# (2) Install the latest version of `sprocket` available.
cargo install sprocket

# (3) Make sure `sprocket` is accessible from the command line.
sprocket --version
```

With `sprocket` on your `PATH`, you can now configure the Sprocket extension to
use it over an automatically installed version.

Set the `Sprocket > Server: Path` setting to simply `sprocket` to use the
`sprocket` from your `PATH`.

## Known Issues

See the [known issues](https://github.com/stjude-rust-labs/sprocket-vscode?tab=readme-ov-file#known-issues)
section of the `README.md` for a list of known issues.

[TextMate grammar]: https://macromates.com/manual/en/language_grammars
