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

### Component Crates

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
