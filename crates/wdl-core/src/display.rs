//! Display modes.

/// A mode of operation when displaying a static code analysis rule.
#[derive(Debug)]
pub enum Mode {
    /// Displays the concern by sharing a summary of the information.
    ///
    /// No extraneous information outlining _why_ the concern was raised or how
    /// to fix it.
    OneLine,

    /// Displays the concern by sharing all known information.
    Full,
}
