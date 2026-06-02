use pyo3::{intern, prelude::*, types::PyString};

/// The diagnostic mode to use for reporting diagnostics.
#[pyclass(module = "sprocket_bio.diagnostics", frozen, eq)]
#[derive(PartialEq)]
pub struct Mode(wdl::diagnostics::Mode);

#[pymethods]
impl Mode {
    /// Prints diagnostics as multiple lines.
    #[classattr]
    const FULL: Self = Self(wdl::diagnostics::Mode::Full);

    /// Prints diagnostics as one line.
    #[classattr]
    const ONE_LINE: Self = Self(wdl::diagnostics::Mode::OneLine);

    /// Returns the “default value” for a type.
    #[staticmethod]
    fn default() -> Self {
        Self(Default::default())
    }

    fn __str__<'py>(&self, py: Python<'py>) -> &Bound<'py, PyString> {
        match self.0 {
            wdl::diagnostics::Mode::Full => intern!(py, "Mode.FULL"),
            wdl::diagnostics::Mode::OneLine => intern!(py, "Mode.ONE_LINE"),
        }
    }

    fn __repr__<'py>(&self, py: Python<'py>) -> &Bound<'py, PyString> {
        match self.0 {
            wdl::diagnostics::Mode::Full => intern!(py, "<Mode.FULL>"),
            wdl::diagnostics::Mode::OneLine => intern!(py, "<Mode.ONE_LINE>"),
        }
    }
}
