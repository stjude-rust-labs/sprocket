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
    Python bindings to Sprocket, a bioinformatics toolkit for Workflow Description Language (WDL).
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

## 🖥️ Development

`sprocket_bio` requires Python 3.10 or greater, which you can [install from python.org](https://www.python.org/downloads/) or from your favorite package manager. Once you have Python installed, you can set up your development environment with the following commands:

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

The Python package is located at `python/sprocket_bio` (in this folder), and the Python extension that it bundles is compiled from `crates/sprocket-py` using the [Maturin build system](https://www.maturin.rs). Dependencies and additional metadata are specified in `pyproject.toml` and `crates/sprocket-py/Cargo.toml`. Unit tests are defined in `python/tests` using the [`pytest`](https://docs.pytest.org) framework. Type and stub checking is performed by [`mypy`](https://mypy.readthedocs.io). Code formatting is performed by [`black`](https://black.readthedocs.io), and import statement sorting is done by [`isort`](https://isort.readthedocs.io).

### Code Coverage

To generate code coverage reports, first install the `llvm-tools` [Rustup component](https://rust-lang.github.io/rustup/concepts/components.html) and install [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov). For example:

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

Code coverage of Python code is generated using [`pytest-cov`](https://pytest-cov.readthedocs.io/en/latest/), which is installed as part of the `cov` dependency group. The Python report is printed when you run `pytest` with the `--cov` option. The Rust coverage data is generated when you run `pytest` as well, but the report must be printed after the fact using `cargo-llvm-cov`.
