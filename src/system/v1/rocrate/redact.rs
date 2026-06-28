//! Redaction helpers for RO-Crate log content.

use std::sync::OnceLock;

use regex::Regex;

/// Replaces secret-shaped text before it is written into RO-Crate metadata.
#[derive(Debug, Clone, Copy, Default)]
pub struct RedactionPolicy;

impl RedactionPolicy {
    /// Scrubs common secret shapes from text.
    pub fn scrub(&self, text: &str) -> Scrubbed {
        let mut redactions = 0;
        let mut scrubbed = text.to_string();

        scrubbed = secret_regex()
            .replace_all(&scrubbed, |_: &regex::Captures<'_>| {
                redactions += 1;
                "[REDACTED]".to_string()
            })
            .into_owned();

        Scrubbed {
            text: scrubbed,
            redactions,
        }
    }
}

/// Text after redaction and the number of replacements made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrubbed {
    /// The scrubbed text.
    pub text: String,
    /// The number of redactions applied.
    pub redactions: usize,
}

/// Returns the shared regex used for secret redaction.
fn secret_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();

    REGEX.get_or_init(|| {
        compile_regex(concat!(
            r"(?s:-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----)",
            r"|(?i:\bBearer\s+[-A-Za-z0-9._~+/=]+)",
            r"|\bAKIA[0-9A-Z]{16}\b",
            r#"|(?i:\b(?:password|passwd|secret|token|api[_-]?key|access[_-]?key)\b\s*(?:=|:)\s*(?:"[^"\r\n]*"|'[^'\r\n]*'|[^\s,;]+))"#,
        ))
    })
}

/// Compiles a static redaction regex.
fn compile_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => panic!("invalid redaction regex `{pattern}`: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_bearer_tokens() {
        let scrubbed = RedactionPolicy.scrub("authorization: Bearer abc.def-123");

        assert_eq!(scrubbed.text, "authorization: [REDACTED]");
        assert_eq!(scrubbed.redactions, 1);
    }

    #[test]
    fn scrubs_aws_access_keys() {
        let scrubbed = RedactionPolicy.scrub("aws key AKIA0123456789ABCDEF was logged");

        assert_eq!(scrubbed.text, "aws key [REDACTED] was logged");
        assert_eq!(scrubbed.redactions, 1);
    }

    #[test]
    fn scrubs_pem_private_key_blocks() {
        let scrubbed = RedactionPolicy.scrub(
            "before\n-----BEGIN RSA PRIVATE KEY-----\nsecret\n-----END RSA PRIVATE KEY-----\nafter",
        );

        assert_eq!(scrubbed.text, "before\n[REDACTED]\nafter");
        assert_eq!(scrubbed.redactions, 1);
    }

    #[test]
    fn scrubs_key_value_secrets() {
        let scrubbed = RedactionPolicy.scrub("password=hunter2 api_key: \"abc 123\" ok=true");

        assert_eq!(scrubbed.text, "[REDACTED] [REDACTED] ok=true");
        assert_eq!(scrubbed.redactions, 2);
    }

    #[test]
    fn leaves_clean_text_unchanged() {
        let scrubbed = RedactionPolicy.scrub("workflow completed without sensitive output");

        assert_eq!(scrubbed.text, "workflow completed without sensitive output");
        assert_eq!(scrubbed.redactions, 0);
    }
}
