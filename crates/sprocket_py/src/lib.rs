//! This crate is a Python extension that exposes a subset `wdl`'s API in Python
//! using [`pyo3`].
//!
//! This crate is not meant to be imported directly. Instead, import the
//! `sprocket_py` Python package (located at `python/sprocket_py`), which
//! bundles this extension.

use pyo3::prelude::*;

/// Python bindings to [Sprocket](https://sprocket.bio), a bioinformatics toolkit for Workflow
/// Description Language (WDL)
#[pymodule]
mod sprocket_py {}
