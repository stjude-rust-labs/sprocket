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
    <a href="https://rustseq.zulipchat.com/join/coxb7c7b3bbahlfx7poeqqrd/" target="_blank">
      <img alt="Chat: Zulip" src="https://img.shields.io/badge/chat-%23workflows--lib--wdl-blue?logo=zulip&logoColor=f6f6f6" />
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
```

The Python package is located at `python/sprocket_bio` (in this folder), and the Python extension that it bundles is compiled from `crates/sprocket-py`. Dependencies and additional metadata are specified in `pyproject.toml` and `crates/sprocket-py/Cargo.toml`.
