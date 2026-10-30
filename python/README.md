<p align="center">
  <h1 align="center">
    <code>sprocket_bio</code>
  </h1>

  <p align="center">
    <a href="https://github.com/stjude-rust-labs/sprocket/actions/workflows/CI.yml" target="_blank">
      <img alt="CI: Status" src="https://github.com/stjude-rust-labs/sprocket/actions/workflows/CI.yml/badge.svg" />
    </a>
    <a href="https://pypi.org/project/sprocket_bio/" target="_blank">
      <img alt="PyPI: Version" src="https://img.shields.io/pypi/v/sprocket_bio">
    </a>
    <a href="https://join.slack.com/t/openwdl/shared_invite/zt-ctmj4mhf-cFBNxIiZYs6SY9HgM9UAVw" target="_blank">
      <img alt="Chat: Slack" src="https://badgen.net/badge/icon/%23sprocket/4A154B?icon=slack&label=slack" />
    </a>
    <img alt="PyPI: Downloads" src="https://img.shields.io/pypi/dm/sprocket_bio">
  </p>

  <p align="center">
    A Python library for parsing and analyzing <a href="https://openwdl.org/">Workflow Description Language</a> (WDL), powered by <a href="https://sprocket.bio/">Sprocket</a>.
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

## Installation

Sprocket's Python bindings are available on PyPI as
[`sprocket-bio`](https://pypi.org/project/sprocket-bio/). It requires Python 3.10 or greater, and
can be installed using [Pip](https://pip.pypa.io/):

```bash
pip install sprocket-bio
```

Sprocket publishes precompiled
[wheels](https://packaging.python.org/en/latest/specifications/binary-distribution-format/) for many
common platforms and Python versions. If you install `sprocket-bio` on a platform that does not have
a precompiled wheel, you will need the latest stable release of the
[Rust compiler](https://rust-lang.org/) in order to build from source.

Release 0.31.0 ships wheels for CPython 3.10 through 3.15 (including the free-threaded builds) and
PyPy 3.11, covering macOS on x86-64 and Apple silicon, `manylinux` and `musllinux` on x86-64 and
aarch64, and Windows on x86-64 and ARM64.

### 🐍 Minimum Supported Python Version

The minimum supported Python version is currently 3.10.

`sprocket_bio` supports the oldest Python version still receiving security updates. It will drop
support for Python versions as they reach end-of-life. For more information, please see
[Python's Version Status page](https://devguide.python.org/versions/).

## Usage

The package exposes three modules: `sprocket_bio.grammar` for parsing WDL, `sprocket_bio.ast` for
working with the resulting syntax tree, and `sprocket_bio.diagnostics` for rendering diagnostics the
way the command line tool does. The package is typed, so it ships type stubs for editors and `mypy`.

For example, parsing a document and printing any diagnostics:

```python
from sprocket_bio.diagnostics import Mode, emit_diagnostics
from sprocket_bio.grammar import SupportedVersion
from sprocket_bio.grammar.grammar import document
from sprocket_bio.grammar.version import V1

with open("example.wdl", "rt", encoding="utf-8") as f:
    source = f.read()

events, diagnostics = document(source, fallback_version=SupportedVersion.V1(V1.ZERO))

if diagnostics:
    emit_diagnostics("example.wdl", source, diagnostics, report_mode=Mode.FULL, colorize=True)
```

Complete, runnable examples — emitting diagnostics, walking the parser event stream, and building a
syntax highlighter — live in
[`python/sprocket_bio/examples`](https://github.com/stjude-rust-labs/sprocket/tree/main/python/sprocket_bio/examples)
in the Sprocket repository.

## Documentation

You can find the Python binding's API docs and several examples online at
<https://sprocket-bio.readthedocs.io/>.

## 🖥️ Development

`sprocket_bio` requires Python 3.10 or greater, which you can
[install from python.org](https://www.python.org/downloads/) or from your favorite package manager.
Once you have Python installed, you can set up your development environment with the following
commands:

```bash
# Create the Python virtual environment, installing the latest version of pip and setuptools.
python -m venv --upgrade-deps .venv

# Activate the virtual environment.
source .venv/bin/activate

# Install the build system, Maturin.
pip install maturin

# Compile and install `sprocket_bio` in the virtual environment.
maturin develop

# Run unit tests.
pytest

# Check types and type stubs.
mypy python/
python -m mypy.stubtest sprocket_bio

# Format code and sort import statements.
black python/
isort python/
```

The Python package is located at `python/sprocket_bio` (in this folder), and the Python extension
that it bundles is compiled from `crates/sprocket-py` using the
[Maturin build system](https://www.maturin.rs). Dependencies and additional metadata are specified
in `pyproject.toml` and `crates/sprocket-py/Cargo.toml`. Unit tests are defined in `python/tests`
using the [`pytest`](https://docs.pytest.org) framework. Type and stub checking is performed by
[`mypy`](https://mypy.readthedocs.io). Code formatting is performed by
[`black`](https://black.readthedocs.io), and import statement sorting is done by
[`isort`](https://isort.readthedocs.io).

### Code Coverage

To generate code coverage reports, first install the `llvm-tools`
[Rustup component](https://rust-lang.github.io/rustup/concepts/components.html) and install
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov). For example:

```bash
rustup component add llvm-tools
cargo install cargo-llvm-cov --version 0.8 --locked
```

Then, run the following commands with the virtual environment activated:

```bash
# Configure Rust to build with code coverage.
source <(cargo llvm-cov show-env --sh)

# Remove unwanted artifacts that may affect coverage results.
cargo llvm-cov clean --workspace

# Compile and install `sprocket_bio`.
maturin develop --group=cov

# Run tests and display Python code coverage.
pytest --cov=python/sprocket_bio

# Display Rust code coverage.
cargo llvm-cov report -p sprocket-py -p wdl-diagnostics -p wdl-grammar -p wdl-ast
```

Code coverage of Python code is generated using
[`pytest-cov`](https://pytest-cov.readthedocs.io/en/latest/), which is installed as part of the
`cov` dependency group. The Python report is printed when you run `pytest` with the `--cov` option.
The Rust coverage data is generated when you run `pytest` as well, but the report must be printed
after the fact using `cargo-llvm-cov`.

### API Documentation

In order to build the API docs, you must have Python 3.12 or greater installed. You can then install
the necessary dependencies and generate the website with the following commands:

```bash
# Install dependencies necessary to build API docs.
maturin develop --group docs

# Build website.
sphinx-build --fail-on-warning python/docs python/docs/_build
```

The API docs are built using [Sphinx](https://www.sphinx-doc.org/) with the
[Read the Docs theme](https://sphinx-rtd-theme.readthedocs.io/). The source is in `python/docs`, and
the output is in `python/docs/_build`. The API docs are hosted online at
<https://sprocket-bio.readthedocs.io/>.

## 📝 License and Legal

This project is licensed as either [Apache 2.0][license-apache] or [MIT][license-mit] at your
discretion. Additionally, please see
[the disclaimer](https://github.com/stjude-rust-labs#disclaimer) that applies to all crates and
command line tools made available by St. Jude Rust Labs.

Copyright © 2026-Present [St. Jude Children's Research Hospital](https://github.com/stjude).

[license-apache]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-APACHE
[license-mit]: https://github.com/stjude-rust-labs/sprocket/blob/main/LICENSE-MIT
