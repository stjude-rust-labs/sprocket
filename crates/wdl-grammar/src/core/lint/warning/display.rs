//! Display of lint warnings.

/// A mode of operation when displaying a lint warning.
#[derive(Debug)]
pub enum Mode {
    /// Displays the warning by sharing as little information as possible to
    /// describe the aforementioned concern.
    ///
    /// In particular, there is not extraneous information outlining _why_ the
    /// lint warning is thrown or how to fix it.
    OneLine,

    /// Displays the warning by sharing all known information.
    Full,
}
