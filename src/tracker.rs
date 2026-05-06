//! Origin tracking for cell assignments + the `ZkDebug` extension trait.
//!
//! The tracker is a `HashMap`-based registry keyed by `(region, offset,
//! column_index)`. The [`zkwire_assign!`] macro records `file!()` / `line!()` at
//! every assignment, and the parser cross-references those entries when
//! building a [`crate::report::ZkReport`].
//!
//! The tracker is installed as a *thread-local* via [`ZkWireTracker::install`]
//! so chips don't need to thread a parameter through every method.

use halo2_proofs::dev::{MockProver, VerifyFailure};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::hex::humanize_hex;
use crate::reporter::forge_trace;

// ─── Records ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize)]
pub struct AssignKey {
    pub region: String,
    pub offset: usize,
    pub column_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssignSite {
    pub region: String,
    pub offset: usize,
    pub column_index: usize,
    pub column_name: String,
    pub expression: String,
    pub file: &'static str,
    pub line: u32,
}

// ─── Registry ───────────────────────────────────────────────────────────────

#[derive(Default)]
struct TrackerInner {
    sites: HashMap<AssignKey, AssignSite>,
    /// `(pretty, raw)` so the reporter can show both decimal and hex.
    public_inputs: Vec<(String, String)>,
}

pub struct ZkWireTracker {
    inner: Mutex<TrackerInner>,
}

impl ZkWireTracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(TrackerInner::default()),
        }
    }

    /// Record where a cell was assigned. Called by [`zkwire_assign!`].
    pub fn record(&self, site: AssignSite) {
        let key = AssignKey {
            region: site.region.clone(),
            offset: site.offset,
            column_index: site.column_index,
        };
        self.inner.lock().unwrap().sites.insert(key, site);
    }

    /// Capture the public inputs as the user passed them to `MockProver::run`.
    /// Generic over any `F: Debug` so it works for Pasta `Fp`, BN-254 `Fr`, etc.
    pub fn record_public_inputs<F: std::fmt::Debug>(&self, inputs: &[F]) {
        let formatted: Vec<(String, String)> = inputs
            .iter()
            .map(|f| {
                let raw = format!("{:?}", f);
                let pretty = humanize_hex(&raw);
                (pretty, raw)
            })
            .collect();
        self.inner.lock().unwrap().public_inputs = formatted;
    }

    /// Look up an assignment by `(region, offset)`. If `column_index` is
    /// known we use it to disambiguate; otherwise we return any site that
    /// matches region + offset.
    pub fn lookup(
        &self,
        region: &str,
        offset: usize,
        column_index: Option<usize>,
    ) -> Option<AssignSite> {
        let inner = self.inner.lock().unwrap();
        if let Some(ci) = column_index {
            let key = AssignKey {
                region: region.to_string(),
                offset,
                column_index: ci,
            };
            if let Some(site) = inner.sites.get(&key) {
                return Some(site.clone());
            }
        }
        inner
            .sites
            .values()
            .find(|s| s.region == region && s.offset == offset)
            .cloned()
    }

    pub fn public_inputs(&self) -> Vec<(String, String)> {
        self.inner.lock().unwrap().public_inputs.clone()
    }

    /// Install this tracker as the current thread-local. The returned guard
    /// restores the previous tracker on drop.
    pub fn install(tracker: &Arc<Self>) -> TrackerGuard {
        let prev = CURRENT.with(|cell| cell.borrow_mut().replace(Arc::clone(tracker)));
        TrackerGuard { prev }
    }
}

impl Default for ZkWireTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Thread-local plumbing ──────────────────────────────────────────────────

thread_local! {
    static CURRENT: RefCell<Option<Arc<ZkWireTracker>>> =
        const { RefCell::new(None) };
}

pub struct TrackerGuard {
    prev: Option<Arc<ZkWireTracker>>,
}

impl Drop for TrackerGuard {
    fn drop(&mut self) {
        CURRENT.with(|cell| *cell.borrow_mut() = self.prev.take());
    }
}

/// Run `f` with the currently-installed tracker, if any.
pub fn with_current<R>(f: impl FnOnce(&ZkWireTracker) -> R) -> Option<R> {
    CURRENT.with(|cell| {
        let borrow = cell.borrow();
        borrow.as_ref().map(|arc| f(arc.as_ref()))
    })
}

/// Extract a `Column`'s index by parsing its `Debug` output.
///
/// `Column::index()` is not public in halo2_proofs 0.3.x, so we stay consistent
/// with the rest of the crate and reflect via `Debug` —
/// `Column { index: 0, column_type: Advice }` → `0`.
pub fn column_index<C>(col: &halo2_proofs::plonk::Column<C>) -> usize
where
    C: halo2_proofs::plonk::ColumnType,
{
    let s = format!("{:?}", col);
    if let Some(i) = s.find("index:") {
        let rest = &s[i + "index:".len()..];
        let digits: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(n) = digits.parse() {
            return n;
        }
    }
    usize::MAX
}

// ─── The macro ──────────────────────────────────────────────────────────────

/// Wraps `region.assign_advice(...)` and records the `file!()`/`line!()` of
/// the call site if a [`ZkWireTracker`] is currently installed. Otherwise
/// it's a transparent pass-through to `assign_advice`.
///
/// ```ignore
/// zkwire_assign!(
///     region,            // &mut Region
///     "mul",             // region name (must match the assign_region label)
///     "lhs * rhs",       // assignment annotation
///     config.advice[0],  // Column<Advice>
///     1,                 // offset within the region
///     value              // Value<F>
/// )
/// ```
#[macro_export]
macro_rules! zkwire_assign {
    (
        $region:expr,
        $region_name:expr,
        $cell_name:expr,
        $column:expr,
        $offset:expr,
        $value:expr $(,)?
    ) => {{
        let __region_name: &str = $region_name;
        let __column_name: &str = $cell_name;
        let __column = $column;
        let __offset: usize = $offset;
        let __res = $region.assign_advice(|| __column_name, __column, __offset, || $value);
        $crate::tracker::with_current(|__t| {
            __t.record($crate::tracker::AssignSite {
                region: __region_name.to_string(),
                offset: __offset,
                column_index: $crate::tracker::column_index(&__column),
                column_name: __column_name.to_string(),
                expression: stringify!($value).to_string(),
                file: file!(),
                line: line!(),
            });
        });
        __res
    }};
}

// ─── ZkDebug extension trait ────────────────────────────────────────────────

/// `MockProver` extension that runs `verify()` and, on failure, emits the
/// ZkWire trace with origin tracking.
pub trait ZkDebug<F> {
    /// Verify the circuit; on failure, render the ZkWire report and return
    /// the original failures so the caller can still inspect them.
    fn verify_and_forge(&self, tracker: &ZkWireTracker) -> Result<(), Vec<VerifyFailure>>;
}

impl<F> ZkDebug<F> for MockProver<F>
where
    F: ff::PrimeField + ff::FromUniformBytes<64> + Ord,
{
    fn verify_and_forge(&self, tracker: &ZkWireTracker) -> Result<(), Vec<VerifyFailure>> {
        match self.verify() {
            Ok(()) => Ok(()),
            Err(failures) => {
                forge_trace(&failures, tracker);
                Err(failures)
            }
        }
    }
}
