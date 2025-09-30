<p align="center">
  <h1 align="center">
    <code>wdl</code>
  </h1>

  <p align="center">
    <a href="https://github.com/stjude-rust-labs/sprocket/actions/workflows/CI.yml" target="_blank">
      <img alt="CI: Status" src="https://github.com/stjude-rust-labs/sprocket/actions/workflows/CI.yml/badge.svg" />
    </a>
    <a href="https://crates.io/crates/wdl" target="_blank">
      <img alt="crates.io version" src="https://img.shields.io/crates/v/wdl">
    </a>
    <a href="https://rustseq.zulipchat.com/join/coxb7c7b3bbahlfx7poeqqrd/" target="_blank">
      <img alt="CI: Status" src="https://img.shields.io/badge/chat-%23workflows--lib--wdl-blue?logo=zulip&logoColor=f6f6f6" />
    </a>
    <img alt="crates.io downloads" src="https://img.shields.io/crates/d/wdl">
  </p>

  <p align="center">
    Rust crates for working with Workflow Description Language (WDL) documents.
    <br />
    <a href="https://docs.rs/wdl"><strong>Explore the docs »</strong></a>
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

## 📚 Getting Started

The `wdl` family of crates consists of (a) a number of component crates (any
crate that is not explicitly `wdl`) that are developed and versioned
independently, and (b) a convenience crate (the `wdl` crate) that exists to ease
syncing compatible component crates versions. Component crates can be enabled
using features and are generally re-exported crates without the `wdl-` (or
`wdl_`) prefix.

This repository contains crates that can be used to work with WDL within your
own Rust projects—if you're looking for a command-line tool built on top of
these crates instead, you should check out [`sprocket`].

### Convenience Crate

Most users should prefer selecting a version of the convenience crate and
enabling features as they wish. For example,

```bash
cargo add wdl --features grammar
```

and then

```rust
use wdl::grammar;
```

### Component Crate(s)

You are free to include component crates directly. For example,

```bash
cargo add wdl_grammar
```

and then

```rust
use wdl_grammar;
```

Be aware, however, that versions between component crates are explicitly not
compatible. In other words, if you choose not to use the convenience crate, it
is not simple to derive which crate versions are compatible, and you'll need to
manually sync those. We _highly_ recommend using the convenience crate if you
intend to use more than one component crate in conjunction.

### Minimum Supported Rust Version

The minimum supported Rust version is currently `1.88.0`.

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
cargo msrv --min 1.88.0
```

If the reported version is newer than the crate's current minimum supported
version, an update is required.

## 🖥️ Development

The WDL specification requires that command scripts are run with the Bash shell, and
therefore developing for the `wdl` crates will require `/bin/bash` be on your `$PATH`.
Linux and Mac users should not need to do anything special to meet this requirement,
but we recommend Windows users fulfill this criteria by installing
[`Git BASH`](https://gitforwindows.org/).

There is an additional requirement for development of the `wdl-lint` crate, which
expects the `shellcheck` binary be available on your `$PATH`. See instructions for
installing ShellCheck [here](https://github.com/koalaman/shellcheck?tab=readme-ov-file#installing).

To bootstrap a development environment, please use the following commands.

```bash
# Clone the repository
git clone git@github.com:stjude-rust-labs/sprocket.git
cd sprocket

# Init submodules
git submodule update --init --recursive

# Build the crate in release mode
cargo build --release

# List out the examples
cargo run --release --example
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
# Currently this requires nightly Rust
cargo +nightly fmt --check

# Ensure the docs build.
cargo doc
```

Note that the `wdl-engine` tests require a locally running Docker daemon.

## 🤝 Contributing

Contributions, issues, and feature requests are all welcome! Feel free to read our
[contributing guide](https://github.com/stjude-rust-labs/sprocket/blob/main/crates/CONTRIBUTING.md).

## 📝 License and Legal

This project is licensed as either [Apache 2.0][license-apache] or
[MIT][license-mit] at your discretion. Additionally, please see [the
disclaimer](https://github.com/stjude-rust-labs#disclaimer) that applies to all
crates and command line tools made available by St. Jude Rust Labs.

Copyright © 2023-Present [St. Jude Children's Research Hospital](https://github.com/stjude).

[license-apache]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-APACHE
[license-mit]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-MIT
[`sprocket`]: https://github.com/stjude-rust-labs/sprocket
