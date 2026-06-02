//! This crate is a Python extension that exposes a subset of `wdl`'s API
//! using [`pyo3`].
//!
//! This crate is not meant to be imported directly. Instead, import the
//! `sprocket_bio` Python package (located at `python/sprocket_bio`), which
//! bundles this extension.

mod diagnostics;
mod grammar;

use pyo3::prelude::*;

/// Python bindings to [Sprocket](https://sprocket.bio), a bioinformatics toolkit for Workflow
/// Description Language (WDL).
#[pymodule]
mod sprocket_bio {
    use pyo3::prelude::*;
    use pyo3::types::PyString;

    #[pymodule]
    mod diagnostics {
        #[pymodule_export]
        use crate::diagnostics::Mode;
        #[pymodule_export]
        use crate::diagnostics::emit_diagnostics;
    }

    #[pymodule]
    mod grammar {
        #[pymodule_export]
        use crate::grammar::Diagnostic;
        #[pymodule_export]
        use crate::grammar::Label;
        #[pymodule_export]
        use crate::grammar::Span;
    }

    /// Initializes the module.
    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        /// Recursively registers all submodules to
        /// [`sys.modules`](https://docs.python.org/3.9/library/sys.html#sys.modules).
        ///
        /// This is required to support importing items directly from submodules
        /// (ex. `from sprocket_py.diagnostics import Mode`). For more
        /// information, see [pyo3#759](https://github.com/PyO3/pyo3/issues/759).
        fn register_submodules(
            module: &Bound<'_, PyModule>,
            parent_name: &str,
            sys_modules: &Bound<'_, PyAny>,
        ) -> PyResult<()> {
            // Loop through the names of all items in the module.
            for item_name in module.index()? {
                // Cast from `PyAny` to `PyString`.
                let item_name: &Bound<'_, PyString> = item_name.cast()?;
                // Get the item from its name.
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
            "sprocket_py",
            // Get the `sys.modules` dictionary.
            &module.py().import("sys")?.getattr("modules")?,
        )
    }
}
