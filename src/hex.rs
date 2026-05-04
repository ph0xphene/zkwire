//! Field-element hex helpers.
//!
//! Pasta's `Fp` debug-prints as a 256-bit hex string (`0x000…0007`). `u128`
//! can't hold the full range, so we route through `BigUint`.

use lazy_static::lazy_static;
use num_bigint::BigUint;
use regex::Regex;

lazy_static! {
    static ref RE_HEX: Regex = Regex::new(r"0x[0-9a-fA-F]+").unwrap();
}

/// Convert a `0x…`-prefixed hex string into a decimal string.
///
/// Returns the input unchanged if parsing fails.
pub fn hex_to_decimal(hex: &str) -> String {
    let trimmed = hex.trim().trim_start_matches("0x").trim_start_matches("0X");
    let stripped = trimmed.trim_start_matches('0');
    let normalized = if stripped.is_empty() { "0" } else { stripped };
    BigUint::parse_bytes(normalized.as_bytes(), 16)
        .map(|n| n.to_str_radix(10))
        .unwrap_or_else(|| hex.to_string())
}

/// Replace every `0x…` substring in `s` with its decimal form, but only when
/// the decimal is short enough to be human-readable (≤ 12 digits). Larger
/// values are left as hex to avoid 60-digit field-element noise.
pub fn humanize_hex(s: &str) -> String {
    RE_HEX
        .replace_all(s, |c: &regex::Captures| {
            let dec = hex_to_decimal(&c[0]);
            if dec.len() <= 12 {
                dec
            } else {
                c[0].to_string()
            }
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_value() {
        assert_eq!(hex_to_decimal("0x3e7"), "999");
    }

    #[test]
    fn padded_field_element() {
        let padded = "0x0000000000000000000000000000000000000000000000000000000000000007";
        assert_eq!(hex_to_decimal(padded), "7");
    }

    #[test]
    fn zero() {
        assert_eq!(hex_to_decimal("0x0"), "0");
        assert_eq!(hex_to_decimal("0x0000"), "0");
    }

    #[test]
    fn humanize_keeps_large_hex() {
        let big = "value=0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        assert!(humanize_hex(big).contains("0xdeadbeef"));
    }

    #[test]
    fn humanize_replaces_small_hex() {
        assert_eq!(humanize_hex("got=0x3e7"), "got=999");
    }
}
