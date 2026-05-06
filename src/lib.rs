//! ZkWire — a human-readable debugging SDK for Halo2 circuits.
//!
//! The public surface:
//!   * [`ZkDebug`] — extension trait on `MockProver<F>`; call
//!     [`ZkDebug::verify_and_forge`] to verify and pretty-print on failure.
//!   * [`ZkWireTracker`] — registry of cell assignments + public inputs.
//!   * [`zkwire_assign!`] — drop-in for `region.assign_advice(...)` that records
//!     `(file!, line!)` for traceback.
//!   * [`forge_trace`] — render a report from raw `VerifyFailure`s.
//!   * [`ZkReport`] / [`ErrorType`] / [`Location`] — structured failure form.

pub mod hex;
pub mod parser;
pub mod report;
pub mod reporter;
pub mod tracker;

pub use parser::{parse_failure, parse_raw_failure};
pub use report::{ErrorType, Location, ZkReport};
pub use reporter::{forge_report, forge_reports, forge_trace};
pub use tracker::{AssignKey, AssignSite, TrackerGuard, ZkDebug, ZkWireTracker};

pub mod prelude {
    pub use crate::{
        ErrorType, Location, TrackerGuard, ZkDebug, ZkReport, ZkWireTracker, forge_report,
        forge_reports, forge_trace, parse_failure, parse_raw_failure, zkwire_assign,
    };
}
