//! Newline style within formatting configuration.


/// The default newline style.
pub const DEFAULT_NEWLINE_STYLE: NewlineStyle = NewlineStyle::Auto;


/// The default newline style.
pub const DEFAULT_NEWLINE_STYLE: NewlineStyle = NewlineStyle::Auto;

/// The newline style to use when formatting.
#[derive(Clone, Copy, Debug)]
pub enum NewlineStyle {
    /// Use the native newline style of the platform.
    Auto,
    /// Use Unix-style newlines (`\n`).
    Unix,
    /// Use Windows-style newlines (`\r\n`).
    Windows,
}

impl Default for NewlineStyle {
    fn default() -> Self {
        DEFAULT_NEWLINE_STYLE
    }
}


impl NewlineStyle {
    /// Gets the newline string for this style.
    pub fn as_str(&self) -> &str {
        match self {
            NewlineStyle::Auto => {
                if cfg!(windows) {
                    "\r\n"
                } else {
                    "\n"
                }
            }
            NewlineStyle::Unix => "\n",
            NewlineStyle::Windows => "\r\n",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unix_newline() {
        assert_eq!(NewlineStyle::Unix.as_str(), "\n");
    }

    #[test]
    fn test_windows_newline() {
        assert_eq!(NewlineStyle::Windows.as_str(), "\r\n");
    }

    #[test]
    fn test_auto_newline() {
        let newline = NewlineStyle::Auto.as_str();
        assert!(newline == "\n" || newline == "\r\n");
    }

    #[test]
    fn test_default_is_auto() {
        assert!(matches!(NewlineStyle::default(), NewlineStyle::Auto));
    }
}
