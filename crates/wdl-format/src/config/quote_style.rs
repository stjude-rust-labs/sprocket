//! Quote style within formatting configuration.

/// The quote style to use when formatting literal strings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteStyle {
    /// Preserve the original quote style.
    #[default]
    Preserve,
    /// Force single quotes.
    Single,
    /// Force double quotes.
    Double,
}
