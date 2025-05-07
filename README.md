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
- **`sprocket explain`** explains validation and lint rules supported by Sprocket.
- **`sprocket format`** formats a document.
- **`sprocket input`** writes template JSON for WDL input.
- **`sprocket lint`** performs static analysis on a document or directory of documents with additional linting rules enabled (effectively a shortcut for `check --lint`).
- **`sprocket run`** runs a task or workflow.
- **`sprocket validate`** validates a set of inputs read from files or on the command line against a task or workflow.
- **`sprocket completions`** generates shell completions for Sprocket.

## Guiding Principles

The following are high-level guiding principles of the Sprocket project.

- Provide a **high-performance** workflow execution engine capable of
  orchestrating massive bioinformatics workloads (the stated target is 20,000+
  concurrent jobs).
- Develop a suite of **modern development tools** that brings bioinformatics
  development on par with other modern languages (e.g.,
  [`wdl-lsp`](https://github.com/stjude-rust-labs/wdl/tree/main/wdl-lsp)).
- Maintain an **community-focused codebase** that enables a diverse set of
  contributors from academic, non-profit, and commercial organizations.
- Build on an **open, domain-tailored standard** to ensure the toolset remains
  singularly focused on unencumbered innovation within bioinformatics.
- Retain a **simple and accessible user experience** when complexity isn't warranted.

## 📚 Getting Started

### Installation

Before you can install `sprocket`, you'll need to install
[Rust](https://www.rust-lang.org/). We recommend using [rustup](https://rustup.rs/) to accomplish this. Once Rust is installed, you can install the latest version of `sprocket` by
running the following command.

```bash
cargo install sprocket
```

### Docker

Sprocket is available as a Docker [image](https://github.com/stjude-rust-labs/sprocket/pkgs/container/sprocket).

```bash
docker pull ghcr.io/stjude-rust-labs/sprocket:v0.12.0
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
cargo fmt --check

# Ensure the docs build.
cargo doc
```

## 🤝 Contributing

Contributions, issues and feature requests are welcome! Feel free to check
[issues page](https://github.com/stjude-rust-labs/sprocket/issues).

Most of the work for this binary happens over at [the `wdl` family of crates' repository](https://github.com/stjude-rust-labs/wdl).
For more information about our contributor policies, please read the `wdl` [CONTRIBUTING guide](https://github.com/stjude-rust-labs/wdl/blob/main/CONTRIBUTING.md).

## 📝 License and Legal

This project is licensed as either [Apache 2.0][license-apache] or
[MIT][license-mit] at your discretion. Additionally, please see [the
disclaimer](https://github.com/stjude-rust-labs#disclaimer) that applies to all
crates and command line tools made available by St. Jude Rust Labs.

Copyright © 2023-Present [St. Jude Children's Research Hospital](https://github.com/stjude).

[license-apache]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-APACHE
[license-mit]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-MIT
