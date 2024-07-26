<p align="center">
  <h1 align="center">
    sprocket
  </h1>

  <p align="center">
    <a href="https://github.com/stjude-rust-labs/sprocket/actions/workflows/CI.yml" target="_blank">
      <img alt="CI: Status" src="https://github.com/stjude-rust-labs/sprocket/actions/workflows/CI.yml/badge.svg" />
    </a>
    <a href="https://crates.io/crates/sprocket" target="_blank">
      <img alt="crates.io version" src="https://img.shields.io/crates/v/sprocket">
    </a>
    <img alt="crates.io downloads" src="https://img.shields.io/crates/d/sprocket">
    <a href="https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-APACHE" target="_blank">
      <img alt="License: Apache 2.0" src="https://img.shields.io/badge/license-Apache 2.0-blue.svg" />
    </a>
    <a href="https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-MIT" target="_blank">
      <img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg" />
    </a>
  </p>

  <p align="center">
    A package manager for Workflow Description Language files.
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

* **`sprocket check`** Checks the syntactic validity of Workflow Description Language files.
* **`sprocket lint`** Lint Workflow Description Language files.
* **`sprocket explain`** Explain lint rules.
* **`sprocket analyzer`** Run Sprocket as a LSP server for IDE integration.

## Guiding Principles

* **Modern, reliable foundation for everyday bioinformatics analysis—written in Rust.** `sprocket` aims to package together a fairly comprehensive set of tools and for developing bioinformatics tasks and workflows using the [Workflow Description Language](http://openwdl.org/). It is built with modern, multi-core systems in mind and written in Rust.
* **WDL specification focused.** We aim to implement the various versions of the [OpenWDL specification](https://github.com/openwdl/wdl) to the letter. In other words, `sprocket` aims to be workflow engine independent. In the future, we plan to make `sprocket` extendable for workflow engine teams.

## 📚 Getting Started

### Installation

Before you can install `sprocket`, you'll need to install
[Rust](https://www.rust-lang.org/). We recommend using
[rustup](https://rustup.rs/) to accomplish this.

Once Rust is installed, you can install the latest version of `sprocket` by
running the following command.

```bash
cargo install sprocket
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

## 📝 License

This project is licensed as either [Apache 2.0][license-apache] or
[MIT][license-mit] at your discretion.

Copyright © 2023-Present [St. Jude Children's Research Hospital](https://github.com/stjude).

[license-apache]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-APACHE
[license-mit]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-MIT
