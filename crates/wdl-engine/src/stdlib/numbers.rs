//! Parsing of numeric values read from files by `read_int` and `read_float`.
//!
//! Starting with WDL 1.1, these functions accept surrounding whitespace, an
//! optional `+` or `-` sign, and then any form accepted for an `Int` (or, for
//! `read_float`, `Int` or `Float`) source literal.
//!
//! See <https://github.com/openwdl/wdl/pull/791>.

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
///
/// The accepted forms are `digit+ exponent`, `digit+ . digit* exponent?`, and
/// `. digit+ exponent?`, where `exponent` is `e` or `E`, an optional `+` or
/// `-`, and one or more digits.
fn is_float_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    let count_digits = |start: usize| {
        bytes[start..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };

    let int_digits = count_digits(0);
    let mut i = int_digits;

    let has_point = bytes.get(i) == Some(&b'.');
    let mut frac_digits = 0;
    if has_point {
        i += 1;
        frac_digits = count_digits(i);
        i += frac_digits;
    }

    if int_digits == 0 && frac_digits == 0 {
        return false;
    }

    let has_exponent = matches!(bytes.get(i), Some(b'e' | b'E'));
    if has_exponent {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }

        let exp_digits = count_digits(i);
        if exp_digits == 0 {
            return false;
        }

        i += exp_digits;
    }

    (has_point || has_exponent) && i == bytes.len()
}

/// Converts the digits of a hexadecimal, octal, or binary literal to the
/// nearest `f64`, rounding ties to even.
///
/// Returns infinity if the value exceeds the range of `f64`.
fn radix_digits_to_f64(digits: &str, radix: u32) -> f64 {
    /// The number of bits in the significand of an `f64` (including the
    /// implicit bit).
    const SIGNIFICAND_BITS: u32 = f64::MANTISSA_DIGITS;

    debug_assert!(radix.is_power_of_two());
    let bits_per_digit = radix.trailing_zeros();
    let bits = digits
        .chars()
        .map(|c| c.to_digit(radix).expect("digits should be valid"))
        .flat_map(|d| (0..bits_per_digit).rev().map(move |i| (d >> i) & 1 == 1))
        .skip_while(|b| !b);

    let mut significand: u64 = 0;
    let mut len = 0;
    // The number of bits beyond the significand (i.e. the binary exponent)
    let mut exponent: u64 = 0;
    // The first bit beyond the significand
    let mut round = false;
    // Whether any bit after the round bit is set
    let mut sticky = false;
    for bit in bits {
        if len < SIGNIFICAND_BITS {
            significand = (significand << 1) | u64::from(bit);
            len += 1;
        } else {
            if exponent == 0 {
                round = bit;
            } else {
                sticky |= bit;
            }

            exponent += 1;
        }
    }

    if round && (sticky || significand & 1 == 1) {
        significand += 1;
        if significand == 1 << SIGNIFICAND_BITS {
            significand >>= 1;
            exponent += 1;
        }
    }

    let max_exponent = u64::try_from(f64::MAX_EXP - 1).expect("should fit");
    if exponent > max_exponent {
        return f64::INFINITY;
    }

    // Both the significand and the power of two are exactly representable, so
    // the multiplication is exact unless it overflows to infinity
    let scale = f64::from_bits((exponent + max_exponent) << (SIGNIFICAND_BITS - 1));
    significand as f64 * scale
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
        Some((radix, digits)) => radix_digits_to_f64(digits, radix),
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
        let max = format!("0x{f}8{z}", f = "f".repeat(13), z = "0".repeat(242));
        let below_max_tie = format!("0x{f}b{f2}", f = "f".repeat(13), f2 = "f".repeat(242));
        let max_tie = format!("0x{f}c{z}", f = "f".repeat(13), z = "0".repeat(242));
        let pow_1023_bin = format!("0b1{z}", z = "0".repeat(1023));
        let pow_1023_oct = format!("-0o1{z}", z = "0".repeat(341));
        let pow_1024_bin = format!("0b1{z}", z = "0".repeat(1024));
        let all_ones = format!("0x{f}", f = "F".repeat(256));
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
            // Values above `u128::MAX`
            ("0x1000000000000000000000000000000000", 2f64.powi(132)),
            (&max, f64::MAX),
            (&below_max_tie, f64::MAX),
            (&pow_1023_bin, 2f64.powi(1023)),
            (&pow_1023_oct, -(2f64.powi(1023))),
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
            &max_tie,
            &all_ones,
            &pow_1024_bin,
        ] {
            assert_eq!(parse_float(s), None, "`{s}` should not parse");
        }
    }
}
