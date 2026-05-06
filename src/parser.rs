//! "String reflection" parser for `halo2_proofs::dev::VerifyFailure`.
//!
//! Halo2 doesn't expose the failure's internals as public fields, so we format
//! it with `{:#?}` and extract structure with regex + brace-balanced slicing.
//! No private API, no `Assignment` trait poking — just the public `Debug` impl.

use halo2_proofs::dev::VerifyFailure;
use lazy_static::lazy_static;
use num_bigint::BigUint;
use regex::Regex;

use crate::hex::hex_to_decimal;
use crate::report::{ErrorType, Location, ZkReport};
use crate::tracker::{AssignSite, ZkWireTracker};

lazy_static! {
    static ref RE_REGION: Regex =
        Regex::new(r#"(?s)region:\s*Region\s*\{[^}]*?name:\s*"([^"]*)""#).unwrap();
    static ref RE_OFFSET: Regex = Regex::new(r"offset:\s*(\d+)").unwrap();
    static ref RE_COLUMN_TYPE: Regex = Regex::new(r"column_type:\s*(\w+)").unwrap();
    static ref RE_COLUMN_INDEX: Regex = Regex::new(r"Column\s*\{[^}]*index:\s*(\d+)").unwrap();
    static ref RE_GATE: Regex =
        Regex::new(r#"(?s)gate:\s*Gate\s*\{[^}]*?name:\s*"([^"]*)""#).unwrap();
    static ref RE_CELL_VALUE: Regex =
        Regex::new(r#"(?s)name:\s*"([^"]+)"[^"]*?,\s*"(0x[0-9a-fA-F]+)""#).unwrap();
    static ref KNOWN_FIELD_PRIMES: Vec<BigUint> = vec![
        BigUint::parse_bytes(
            b"21888242871839275222246405745257275088548364400416034343698204186575808495617",
            10,
        )
        .unwrap(),
        BigUint::parse_bytes(
            b"28948022309329048855892746252171976963363056481941647379679742748393362948097",
            10,
        )
        .unwrap(),
    ];
}

pub fn parse_failure(index: usize, failure: &VerifyFailure, tracker: &ZkWireTracker) -> ZkReport {
    let raw = format!("{:#?}", failure);
    parse_raw_failure(index, &raw, tracker)
}

pub fn parse_raw_failure(index: usize, raw: &str, tracker: &ZkWireTracker) -> ZkReport {
    let error_type = detect_error_type(&raw);
    let location = extract_location(&raw);
    let value_found = extract_cell_values(&raw);
    let origin = lookup_origin(&location, tracker);
    let suggestion = build_suggestion(&error_type, &location);
    let warnings = build_security_warnings(&value_found);

    ZkReport {
        index,
        error_type,
        location,
        value_found,
        origin,
        suggestion,
        warnings,
        raw: raw.to_string(),
    }
}

// ─── Step C: classify the failure variant ───────────────────────────────────

fn detect_error_type(raw: &str) -> ErrorType {
    // Take the leading PascalCase identifier (everything up to the first
    // non-ident character — typically a space or `{`).
    let raw = raw.trim_start_matches(|c: char| !c.is_alphabetic());
    let head: String = raw
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    match head.as_str() {
        "ConstraintNotSatisfied" => ErrorType::ConstraintViolation,
        // Disambiguate Permutation by scanning for an Instance column ref.
        "Permutation" => {
            if column_type_matches(raw, "Instance") {
                ErrorType::InstanceMismatch
            } else {
                ErrorType::PermutationMismatch
            }
        }
        "InstanceCellNotAssigned" => ErrorType::InstanceMismatch,
        "CellNotAssigned" => ErrorType::CellNotAssigned,
        "Lookup" => ErrorType::Lookup,
        other => ErrorType::Unknown(other.to_string()),
    }
}

fn column_type_matches(raw: &str, ty: &str) -> bool {
    RE_COLUMN_TYPE.captures_iter(raw).any(|c| &c[1] == ty)
}

// ─── Location extraction ────────────────────────────────────────────────────

fn extract_location(raw: &str) -> Location {
    let region = RE_REGION.captures(raw).map(|c| c[1].to_string());
    let offset = RE_OFFSET.captures(raw).and_then(|c| c[1].parse().ok());
    let column_type = RE_COLUMN_TYPE.captures(raw).map(|c| c[1].to_string());
    let column_index = RE_COLUMN_INDEX
        .captures(raw)
        .and_then(|c| c[1].parse().ok());
    let gate = RE_GATE.captures(raw).map(|c| c[1].to_string());

    Location {
        region,
        offset,
        column_type,
        column_index,
        gate,
    }
}

// ─── Step A + B: isolate `cell_values` then regex inside it ────────────────

fn extract_cell_values(raw: &str) -> Vec<(String, String)> {
    let block = match isolate_cell_values_block(raw) {
        Some(b) => b,
        None => return Vec::new(),
    };
    RE_CELL_VALUE
        .captures_iter(block)
        .map(|c| (c[1].to_string(), hex_to_decimal(&c[2])))
        .collect()
}

/// Walk `[ … ]` brackets and return the slice covering the `cell_values: [ … ]`
/// list. This *prevents* pairing a region/gate `name:` with a hex literal
/// belonging to a different part of the dump.
fn isolate_cell_values_block(raw: &str) -> Option<&str> {
    let key = raw.find("cell_values:")?;
    let after = &raw[key..];
    let bracket_rel = after.find('[')?;
    let bytes = after.as_bytes();
    let mut depth = 0usize;
    for i in bracket_rel..bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(&after[bracket_rel..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

// ─── Cross-reference tracker for origin ─────────────────────────────────────

fn lookup_origin(loc: &Location, tracker: &ZkWireTracker) -> Option<AssignSite> {
    let region = loc.region.as_deref()?;
    let offset = loc.offset?;
    tracker.lookup(region, offset, loc.column_index)
}

// ─── Suggestions ────────────────────────────────────────────────────────────

fn build_suggestion(et: &ErrorType, loc: &Location) -> String {
    match et {
        ErrorType::InstanceMismatch => {
            "Public input passed to `MockProver::run` doesn't match what the circuit \
             exposes via `expose_public`. Print both sides and confirm equality."
                .to_string()
        }
        ErrorType::ConstraintViolation => {
            let prefix = match (&loc.gate, &loc.region, loc.offset) {
                (Some(g), Some(r), Some(o)) => {
                    format!("Gate `{}` failed in region `{}` at offset {}.", g, r, o)
                }
                (Some(g), Some(r), None) => {
                    format!("Gate `{}` failed in region `{}`.", g, r)
                }
                (Some(g), None, _) => format!("Gate `{}` failed.", g),
                (None, Some(r), _) => format!("Constraint failed in region `{}`.", r),
                _ => "Polynomial gate identity violated.".to_string(),
            };
            format!(
                "{} The witness violates the gate's algebra — recompute the gate by \
                 hand using the cell values shown.",
                prefix
            )
        }
        ErrorType::PermutationMismatch => {
            "Two cells linked by an equality constraint hold different values. \
             Trace the `copy_advice` chain that produced these cells."
                .to_string()
        }
        ErrorType::CellNotAssigned => {
            "An advice cell touched by an enabled selector was never assigned. \
             Make sure every cell the gate queries gets a `region.assign_advice(...)`."
                .to_string()
        }
        ErrorType::Lookup => "Witness value isn't a member of the lookup table. Confirm the table \
             is populated for the rows the lookup is enabled on."
            .to_string(),
        ErrorType::Unknown(s) => format!(
            "Unrecognized failure variant `{}`. See the raw debug dump below.",
            s
        ),
    }
}

fn build_security_warnings(values: &[(String, String)]) -> Vec<String> {
    let mut warnings = Vec::new();
    for (name, value) in values {
        let Some(n) = BigUint::parse_bytes(value.as_bytes(), 10) else {
            continue;
        };

        if n == BigUint::from(0u32) || n == BigUint::from(1u32) {
            warnings.push(format!(
                "Potential Boolean Constraint Weakness: `{}` is {}. Confirm this cell is explicitly boolean-constrained if it gates security logic.",
                name, value
            ));
        }

        if is_near_known_field_prime(&n) {
            warnings.push(format!(
                "Potential Underflow Detected: `{}` is within 1000 of a known field modulus. Check for subtraction underflow or missing range constraints.",
                name
            ));
        }
    }

    warnings
}

fn is_near_known_field_prime(value: &BigUint) -> bool {
    let window = BigUint::from(1000u32);
    KNOWN_FIELD_PRIMES
        .iter()
        .any(|prime| value < prime && prime - value <= window)
}
