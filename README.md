<img style="margin: 0px" alt="Repository Header Image" src="./assets/repo-header.png" />
<hr/>

<p align="center">
  <p align="center">
    <a href="https://github.com/stjude-rust-labs/sprocket/actions/workflows/CI.yml" target="_blank">
      <img alt="CI: Status" src="https://github.com/stjude-rust-labs/sprocket/actions/workflows/CI.yml/badge.svg" />
    </a>
    <a href="https://crates.io/crates/sprocket" target="_blank">
      <img alt="crates.io version" src="https://img.shields.io/crates/v/sprocket">
    </a>
    <a href="https://rustseq.zulipchat.com" target="_blank">
      <img alt="CI: Status" src="https://img.shields.io/badge/chat-%23workflows--bin--sprocket-blue?logo=zulip&logoColor=f6f6f6" />
    </a>
    <img alt="crates.io downloads" src="https://img.shields.io/crates/d/sprocket">
  </p>

  <p align="center">
    A bioinformatics workflow engine built on top of the Workflow Description Language (WDL).
    <br />
    <br />
    <a href="https://github.com/stjude-rust-labs/sprocket/issues/new?assignees=&title=Descriptive%20Title&labels=enhancement">Request Feature</a>
    ·
    <a href="https://github.com/stjude-rust-labs/sprocket/issues/new?assignees=&title=Descriptive%20Title&labels=bug">Report Bug</a>
    ·
    ⭐ Consider starring the repo! ⭐
    <br />
  </p>
</p>

## 🎨 Features

- **`sprocket analyzer`** runs Sprocket as a LSP server, which is useful for IDE integration.
- **`sprocket check`** performs static analysis on a document or directory of documents.
- **`sprocket completions`** generates shell completions for Sprocket.
- **`sprocket config`** prints configuration values.
- **`sprocket dev`** subcommand containing developmental and experimental commands:
  - **`sprocket dev doc`** generates documentation for a WDL workspace.
  - **`sprocket dev server`** runs an HTTP API server for workflow execution.
  - **`sprocket dev test`** runs unit tests for a WDL workspace.
- **`sprocket explain`** explains validation and lint rules supported by Sprocket.
- **`sprocket format`** formats a document or directory of documents.
- **`sprocket inputs`** writes template input file (JSON or YAML) for a task or workflow.
- **`sprocket lint`** performs static analysis on a document or directory of documents with additional linting rules enabled (effectively a shortcut for `check --lint`).
- **`sprocket run`** runs a task or workflow.
- **`sprocket validate`** validates a set of inputs read from files or on the command line against a task or workflow.

## Guiding Principles

The following are high-level guiding principles of the Sprocket project.

- Provide a **high-performance** workflow execution engine capable of
  orchestrating massive bioinformatics workloads (the stated target is 20,000+
  concurrent jobs).
- Develop a suite of **modern development tools** that brings bioinformatics
  development on par with other modern languages (e.g.,
  [`wdl-lsp`](https://github.com/stjude-rust-labs/sprocket/tree/main/crates/wdl-lsp)).
- Maintain an **community-focused codebase** that enables a diverse set of
  contributors from academic, non-profit, and commercial organizations.
- Build on an **open, domain-tailored standard** to ensure the toolset remains
  singularly focused on unencumbered innovation within bioinformatics.
- Retain a **simple and accessible user experience** when complexity isn't warranted.

## 📚 Getting Started

### Installation

Check the [GitHub releases page](https://github.com/stjude-rust-labs/sprocket/releases)
to see if Sprocket is available for your platform.

Note that the prebuilt Sprocket for Linux may not work on every distribution
due to library dependencies.

If Sprocket is not available for your platform or architecture, you may install
it with `cargo` from a [Rust](https://www.rust-lang.org/) toolchain.

We recommend using [rustup](https://rustup.rs/) to install a Rust toolchain.

Once Rust is installed, you can install the latest version of Sprocket by
running the following command:

```bash
cargo install sprocket --locked
```

### Homebrew

Sprocket is also available on [Homebrew](https://brew.sh) for both MacOS and Linux. Once Homebrew is installed, you can install Sprocket with the following command.

```bash
brew install sprocket
```

### Docker

Sprocket is available as a Docker [image](https://github.com/stjude-rust-labs/sprocket/pkgs/container/sprocket).

```bash
docker pull ghcr.io/stjude-rust-labs/sprocket:v0.18.0
```

## 🖥️ Development

To bootstrap a development environment, please use the following commands.

```bash
# Clone the repository
git clone git@github.com:stjude-rust-labs/sprocket.git
cd sprocket

# Build the crate in release mode
cargo build --release

# Run the `sprocket` command line tool
cargo run --release
```

### Dependencies

The WDL specification requires that command scripts are run with the Bash
shell, and therefore developing for Sprocket will require `/bin/bash`
be on your `$PATH`. Linux and Mac users should not need to do anything special
to meet this requirement, but we recommend Windows users fulfill this criteria
by installing [`Git BASH`](https://gitforwindows.org/).

Some tests require the `shellcheck` binary be available on your `$PATH`. See
instructions for installing ShellCheck
[here](https://github.com/koalaman/shellcheck?tab=readme-ov-file#installing).

Note that on an HPC or another environment where normal means of installing
software are difficult, it may be easiest to wrap an `apptainer` invocation of
`shellcheck` in a bash script, and then save it as executable in your PATH:

```bash
#!/usr/bin/env bash

apptainer -s run docker://koalaman/shellcheck:stable $@
```

## 🚧️ Tests

Before submitting any pull requests, please make sure the code passes the
following checks (from the root directory).

```bash
# Run the project's tests.
cargo test --all-features

# Run the tests for the examples.
cargo test --examples --all-features

# Ensure the project doesn't have any linting warnings.
cargo clippy --all-features

# Ensure the project passes `cargo fmt`.
cargo +nightly fmt --check

# Ensure the docs build.
cargo doc
```

## 🤝 Contributing

Contributions, issues and feature requests are welcome! Feel free to check the
[issues page](https://github.com/stjude-rust-labs/sprocket/issues).

Most of the work for this binary happens within [the `wdl` crates](https://github.com/stjude-rust-labs/sprocket/tree/main/crates).
For more information about our contributor policies, please read the [contributing guide](https://github.com/stjude-rust-labs/sprocket/blob/main/CONTRIBUTING.md).

## ⚙️ Minimum Supported Rust Version

The minimum supported Rust version is currently `1.91.1`.

There is a CI job that verifies the declared minimum supported version.

If a contributor submits a PR that uses a feature from a newer version of Rust,
the contributor is responsible for updating the minimum supported version in
the `Cargo.toml`.

Contributors may update the minimum supported version as-needed to the latest
stable release of Rust.

To facilitate the discovery of what the minimum supported version should be,
install the `cargo-msrv` tool:

```bash
cargo install cargo-msrv
```

And run the following command:

```bash
cargo msrv --min 1.91.1
```

If the reported version is newer than the crate's current minimum supported
version, an update is required.

## 📝 License and Legal

This project is licensed as either [Apache 2.0][license-apache] or
[MIT][license-mit] at your discretion. Additionally, please see [the
disclaimer](https://github.com/stjude-rust-labs#disclaimer) that applies to all
crates and command line tools made available by St. Jude Rust Labs.

Copyright © 2023-Present [St. Jude Children's Research Hospital](https://github.com/stjude).

[license-apache]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-APACHE
[license-mit]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-MIT
