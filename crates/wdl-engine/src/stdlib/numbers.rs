//! Parsing of numeric values read from files by `read_int` and `read_float`.
//!
//! Starting with WDL 1.1, these functions accept surrounding whitespace, an
//! optional `+` or `-` sign, and then any form accepted for an `Int` (or, for
//! `read_float`, `Int` or `Float`) source literal.
//!
//! See <https://github.com/openwdl/wdl/pull/791>.

use wdl_grammar::lexer::Lexer;
use wdl_grammar::lexer::v1::Token;

/// Splits an optional leading `+` or `-` sign from the given string.
///
/// Returns whether the value is negative and the remainder of the string.
fn split_sign(s: &str) -> (bool, &str) {
    if let Some(rest) = s.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = s.strip_prefix('+') {
        (false, rest)
    } else {
        (false, s)
    }
}

/// Splits an unsigned `Int` source literal into its radix and digits.
///
/// Returns `None` if the string is not a valid `Int` source literal.
fn int_literal_digits(s: &str) -> Option<(u32, &str)> {
    let (radix, digits) = match s.get(..2) {
        Some("0x" | "0X") => (16, &s[2..]),
        Some("0o" | "0O") => (8, &s[2..]),
        Some("0b" | "0B") => (2, &s[2..]),
        // Leading zeros (e.g. a legacy octal literal like `0755`) are not
        // permitted in a decimal literal
        _ if s.len() > 1 && s.starts_with('0') => return None,
        _ => (10, s),
    };

    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }

    Some((radix, digits))
}

/// Determines if the given string is an unsigned `Float` source literal.
fn is_float_literal(s: &str) -> bool {
    let mut lexer = Lexer::<Token>::new(s);
    matches!(lexer.next(), Some((Ok(Token::Float), _))) && lexer.next().is_none()
}

/// Parses the contents of a file read by `read_int` in WDL 1.1 or later.
///
/// Returns `None` if the contents are not a valid integer or if the integer is
/// out of range.
pub(super) fn parse_int(s: &str) -> Option<i64> {
    let (negative, s) = split_sign(s.trim());
    let (radix, digits) = int_literal_digits(s)?;
    let magnitude = u64::from_str_radix(digits, radix).ok()?;
    if negative {
        if magnitude > i64::MIN.unsigned_abs() {
            return None;
        }

        Some((magnitude as i64).wrapping_neg())
    } else {
        i64::try_from(magnitude).ok()
    }
}

/// Parses the contents of a file read by `read_float` in WDL 1.1 or later.
///
/// Returns `None` if the contents are not a valid integer or float or if the
/// value is outside the finite range of `f64`.
pub(super) fn parse_float(s: &str) -> Option<f64> {
    let (negative, s) = split_sign(s.trim());
    let value = match int_literal_digits(s) {
        Some((10, digits)) => digits.parse::<f64>().ok()?,
        Some((radix, digits)) => u64::from_str_radix(digits, radix).ok()? as f64,
        None if is_float_literal(s) => s.parse::<f64>().ok()?,
        None => return None,
    };

    if !value.is_finite() {
        return None;
    }

    Some(if negative { -value } else { value })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn int() {
        for (s, expected) in [
            ("0", 0),
            ("-0", 0),
            ("+0", 0),
            ("12345", 12345),
            ("  \t-12345 \t ", -12345),
            ("+12345", 12345),
            ("0xcafe", 0xCAFE),
            ("0XCAFE", 0xCAFE),
            ("-0x10", -16),
            ("0o755", 0o755),
            ("+0O755", 0o755),
            ("0b101010", 42),
            ("-0B101010", -42),
            ("0x000F", 15),
            ("0o0", 0),
            ("9223372036854775807", i64::MAX),
            ("-9223372036854775808", i64::MIN),
            ("0x7FFFFFFFFFFFFFFF", i64::MAX),
            ("-0x8000000000000000", i64::MIN),
            ("-0o1000000000000000000000", i64::MIN),
            (
                "-0b1000000000000000000000000000000000000000000000000000000000000000",
                i64::MIN,
            ),
        ] {
            assert_eq!(parse_int(s), Some(expected), "`{s}` should parse");
        }

        for s in [
            "",
            "   ",
            "+",
            "-",
            "0755",
            "00",
            "-0755",
            "+-1",
            "--1",
            "- 1",
            "1 2",
            "0x",
            "0xG",
            "0x+1",
            "0x-1",
            "0o8",
            "0b2",
            "0d10",
            "1_000",
            "1.0",
            "1e3",
            "1i64",
            "abc",
            "9223372036854775808",
            "-9223372036854775809",
            "0x8000000000000000",
            "-0x8000000000000001",
            "0xFFFFFFFFFFFFFFFFF",
            "0b1000000000000000000000000000000000000000000000000000000000000000",
        ] {
            assert_eq!(parse_int(s), None, "`{s}` should not parse");
        }
    }

    #[test]
    fn float() {
        let large_decimal = format!("1{z}", z = "0".repeat(309));

        for (s, expected) in [
            ("0", 0.0),
            ("12345", 12345.0),
            ("  -12345 \n", -12345.0),
            ("0xcafe", 51966.0),
            ("-0o755", -493.0),
            ("+0b101010", 42.0),
            ("1.", 1.0),
            ("1.0", 1.0),
            ("00.5", 0.5),
            (".5", 0.5),
            ("-.5", -0.5),
            ("1e3", 1000.0),
            ("1E+3", 1000.0),
            ("1.25e-3", 0.00125),
            (".5e1", 5.0),
            ("1e-400", 0.0),
            ("1.7976931348623157e308", f64::MAX),
            ("9223372036854775808", 9223372036854775808.0),
            ("0xFFFFFFFFFFFFFFFF", 18446744073709551615.0),
            ("0x1FFFFFFFFFFFFF", 9007199254740991.0),
            // Ties round to even
            ("0x20000000000001", 9007199254740992.0),
            ("0x20000000000003", 9007199254740996.0),
            // Values above a tie round up
            ("0x200000000000011", 144115188075855904.0),
        ] {
            assert_eq!(parse_float(s), Some(expected), "`{s}` should parse");
        }

        for s in [
            "",
            "   ",
            "+",
            "-",
            ".",
            "-.",
            "0755",
            "+-1",
            "- 1",
            "1 2",
            "e3",
            ".e3",
            "1e",
            "1e+",
            "1.e",
            "1.5.5",
            "0x",
            "0x1.8",
            "0x1p3",
            "0o8",
            "0b2",
            "1_000.0",
            "1.0f",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "Infinity",
            "-Infinity",
            "1e309",
            "-1e309",
            "1.7976931348623159e308",
            &large_decimal,
            // Prefixed values must fit in a `u64`
            "0x10000000000000000",
        ] {
            assert_eq!(parse_float(s), None, "`{s}` should not parse");
        }
    }
}
