use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::PyString;

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

/// Emits the given diagnostics to the terminal.
#[pyfunction]
pub fn emit_diagnostics(
    path: &str,
    source: &str,
    diagnostics: Vec<Py<PyAny>>, // TODO: correct diagnostics type
    report_mode: Bound<'_, Mode>,
    colorize: bool,
) -> PyResult<()> {
    wdl::diagnostics::emit_diagnostics(
        path,
        source,
        diagnostics.into_iter().map(|x| todo!("map diagnostics")),
        report_mode.get().0,
        colorize,
    )
    .map_err(PyErr::from)
}
