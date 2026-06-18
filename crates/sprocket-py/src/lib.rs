//! This crate is a Python extension that exposes a subset of `wdl`'s API
//! using [`pyo3`].
//!
//! This crate is not meant to be imported directly. Instead, import the
//! `sprocket_bio` Python package (located at `python/sprocket_bio`), which
//! bundles this extension.

use pyo3::prelude::*;

/// Python bindings to [Sprocket](https://sprocket.bio), a bioinformatics toolkit for Workflow
/// Description Language (WDL).
#[pymodule]
mod _sprocket_bio {
    use pyo3::prelude::*;
    use pyo3::types::PyString;

    #[pymodule]
    mod diagnostics {
        #[pymodule_export]
        use wdl_diagnostics::Mode;
        #[pymodule_export]
        use wdl_diagnostics::py_emit_diagnostics;
    }

    #[pymodule]
    mod grammar {
        #[pymodule_export]
        use wdl_grammar::Diagnostic;
        #[pymodule_export]
        use wdl_grammar::Label;
        #[pymodule_export]
        use wdl_grammar::Severity;
        #[pymodule_export]
        use wdl_grammar::Span;
        #[pymodule_export]
        use wdl_grammar::SyntaxKind;
    }

    /// Initializer that runs when the `_sprocket_bio` Python extension is
    /// imported for the first time. As `sprocket_bio/__init__.py` imports
    /// this Python extension, this initializer is implicitly run the first
    /// time any `sprocket_bio` module is imported.
    ///
    /// This initializer is used to support importing items from submodules
    /// directly. For example, running `from sprocket_bio.diagnostics import
    /// Mode` will make the Python interpreter look for
    /// `sprocket_bio/diagnostics.py` or `sprocket_bio/diagnostics/__init__.py`.
    /// Neither of these files exist, however, and importing will result in
    /// a `ModuleNotFoundError`. To fix this, we patch [`sys.modules`](https://docs.python.org/3.10/library/sys.html#sys.modules)
    /// in this initializer so that the Python interpreter can import these
    /// submodules even though they aren't represented on the file system. For
    /// more information, see [pyo3#759](https://github.com/PyO3/pyo3/issues/759).
    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        /// Recursively visits every submodule in this Python extension and adds
        /// it to `sys.modules`.
        fn register_submodules(
            module: &Bound<'_, PyModule>,
            parent_name: &str,
            sys_modules: &Bound<'_, PyAny>,
        ) -> PyResult<()> {
            // Loop through the names of all items in the module.
            for item_name in module.index()? {
                // Cast name from `PyAny` to `PyString`.
                let item_name: &Bound<'_, PyString> = item_name.cast()?;
                // Get the actual item from its name.
                let item = module.getattr(item_name)?;

                // If the item is a submodule...
                if let Ok(submodule) = item.cast::<PyModule>() {
                    let submodule_name = format!("{parent_name}.{item_name}");

                    // ...add the submodule to `sys.modules`.
                    sys_modules.set_item(&submodule_name, submodule)?;

                    register_submodules(submodule, &submodule_name, sys_modules)?;
                }
            }

            Ok(())
        }

        register_submodules(
            module,
            "sprocket_bio",
            // Get the `sys.modules` dictionary.
            &module.py().import("sys")?.getattr("modules")?,
        )
    }
}
