//! Structured representation of a circuit failure.

use crate::tracker::AssignSite;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ErrorType {
    /// A polynomial gate identity was violated.
    ConstraintViolation,
    /// Two cells linked by an equality constraint hold different values.
    PermutationMismatch,
    /// A public input doesn't match what the circuit exposes.
    InstanceMismatch,
    /// A queried cell was never assigned.
    CellNotAssigned,
    /// A witness value isn't a member of a lookup table.
    Lookup,
    /// Variant we couldn't classify; raw header is preserved.
    Unknown(String),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Location {
    pub region: Option<String>,
    pub offset: Option<usize>,
    pub column_type: Option<String>,
    pub column_index: Option<usize>,
    pub gate: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkReport {
    pub index: usize,
    pub error_type: ErrorType,
    pub location: Location,
    /// `(virtual_cell_name, decimal_value)` pairs reflected from `cell_values`.
    pub value_found: Vec<(String, String)>,
    /// Origin from the [`crate::tracker::ZkWireTracker`], if a matching
    /// `(region, offset, column)` was recorded via `zkwire_assign!`.
    pub origin: Option<AssignSite>,
    pub suggestion: String,
    pub warnings: Vec<String>,
    /// The original `{:#?}`-formatted failure, kept for fall-through inspection.
    pub raw: String,
}
