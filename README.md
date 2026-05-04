# ZkWire

A debugging layer for Halo2 circuits. Maps cryptic `VerifyFailure` dumps to `file:line` traces in your Rust source, decodes 256-bit field elements to integers, and renders the result like a Cargo error.

`gdb` is to a segfault as ZkWire is to a `Permutation` failure.

> **Status**: alpha. Public API is stable for the patterns documented below; internals may shift before 0.2.

## The pain

When a Halo2 circuit refuses to verify, you get this:

```text
[
    Permutation {
        column: Column { index: 0, column_type: Instance },
        location: InRegion {
            region: Region { index: 7, name: "expose c" },
            offset: 0,
        },
    },
    Permutation {
        column: Column { index: 0, column_type: Advice },
        location: InRegion {
            region: Region { index: 5, name: "mul" },
            offset: 1,
        },
    },
]
```

For a `ConstraintNotSatisfied`, every `cell_values` field is a 64-character hex string — `0x0000000000000000000000000000000000000000000000000000000000000007` instead of `7`. You tab between your circuit and a hex calculator while the deadline slips.

`MockProver` makes it worse: `instance()` is `pub(crate)`, `VerifyFailure`'s fields are private, and patching halo2 means forking a critical dependency. There is no supported way to extract context.

ZkWire solves all three problems:

- **Origin tracking** — the `zkwire_assign!` macro records `(file!(), line!())` at every cell assignment. When verification fails, the report points at the exact line where the offending value was written. A stack trace, but for advice columns.
- **Field-element decoding** — `BigUint`-backed conversion turns `0x00…03e7` into `999`. Works for Pasta `Fp`, BN-254 `Fr`, Goldilocks, or any `F: Debug`. No `u128` truncation.
- **Non-invasive** — everything goes through `format!("{:#?}", failure)`. No fork of `halo2_proofs`, no private fields, no `unsafe`.

## Before / After

**Before** — the raw `unwrap_err` you get today:

```text
[
    Permutation {
        column: Column { index: 0, column_type: Instance },
        location: InRegion { region: Region { index: 7, name: "expose c" }, offset: 0 },
    },
    Permutation {
        column: Column { index: 0, column_type: Advice },
        location: InRegion { region: Region { index: 5, name: "mul" }, offset: 1 },
    },
]
```

**After** — `prover.verify_and_forge(&tracker)`:

```text
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 ZkWire   2 circuit failures detected
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

 public inputs (as passed to MockProver)
   [0] 999   raw: 0x00000000000000000000000000000000000000000000000000000000000003e7

error[01]: public input mismatch
  --> Instance column
  = help: Public input passed to `MockProver::run` doesn't match what the
          circuit exposes via `expose_public`. Print both sides and confirm
          equality.

error[02]: permutation / copy mismatch
  --> region `mul`  ·  offset 1  ·  Advice column
  = note: value first assigned at src/main.rs:142  (cell `lhs * rhs`)
  = help: Two cells linked by an equality constraint hold different values.
          Trace the `copy_advice` chain that produced these cells.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 tip: rerun after addressing the suggestions above.
```

The `= note:` line is rust-compiler-style. Most modern terminals (iTerm2, WezTerm, VS Code) make `src/main.rs:142` clickable — Cmd/Ctrl-click jumps your editor to the assignment site.

## Install

```toml
[dependencies]
zkwire = "0.1"
halo2_proofs = "0.3"
```

## Quick start

Three lines wire ZkWire into an existing test:

```rust
use std::sync::Arc;
use zkwire::{ZkDebug, ZkWireTracker};

let tracker = Arc::new(ZkWireTracker::new());
let _guard = ZkWireTracker::install(&tracker);           // RAII; uninstalls on drop
tracker.record_public_inputs(&public_inputs);

let prover = MockProver::run(k, &circuit, vec![public_inputs.clone()]).unwrap();
let _ = prover.verify_and_forge(&tracker);               // prints the report on failure
```

That's it. Existing circuits get readable diagnostics without any structural change.

## Tracking origins

To map failures back to source lines, swap `region.assign_advice(...)` for `zkwire_assign!`:

```rust
use zkwire::zkwire_assign;

layouter.assign_region(
    || "mul",
    |mut region| {
        config.s_mul.enable(&mut region, 0)?;

        a.copy_advice(|| "lhs", &mut region, advice[0], 0)?;
        b.copy_advice(|| "rhs", &mut region, advice[1], 0)?;

        let value = a.value().copied() * b.value();

        zkwire_assign!(
            region,           // &mut Region
            "mul",            // region name (must match the assign_region label)
            "lhs * rhs",      // assignment annotation
            advice[0],        // Column<Advice>
            1,                // offset within the region
            value             // Value<F>
        )
    },
)
```

When verification fails on a cell linked to that assignment, the report's `= note:` points back at the `zkwire_assign!` call site. The macro is a transparent wrapper — when no tracker is installed, it is a single thread-local read followed by an early return.

## Features

- **RAII tracker installation** — `ZkWireTracker::install` returns a `TrackerGuard` keyed on a thread-local. Drop the guard, the tracker uninstalls. Safe across nested scopes and parallel test runs.
- **Near-zero overhead when uninstalled** — production builds that don't install a tracker pay one thread-local read per `zkwire_assign!`. No global state, no allocation.
- **Field-agnostic hex decoding** — `BigUint` (no `u128` truncation). Field elements up to 256 bits — Pasta, BN-254, Goldilocks — all decode the same way.
- **Double-anchored parser** — brace-balanced slicing isolates the `cell_values: [ … ]` block; regex extraction runs *inside* that slice. Prevents cross-contamination from unrelated `name:` fields elsewhere in the dump. See [Architecture.md](Architecture.md).
- **Cargo-style diagnostics** — `error[NN]`, `-->` location lines, `= note:` for source origin, `= help:` for actionable guidance. Color via the `colored` crate.
- **Generic `ZkDebug` trait** — `impl<F> ZkDebug<F> for MockProver<F> where F: PrimeField + FromUniformBytes<64> + Ord`. Works for any field MockProver supports.
- **No fork required** — works with stock `halo2_proofs` 0.3.0+ via `format!("{:#?}", failure)` reflection.

## How it works

See [Architecture.md](Architecture.md) for the parser design, the shadow-mapping strategy that links failure dumps back to source lines, and the rationale for "string reflection" over private-field access.

## Compatibility

|              | Versions                                                        |
|--------------|-----------------------------------------------------------------|
| halo2_proofs | `0.3.0`+ (tested against `0.3.2`)                               |
| Rust         | `1.85` (edition 2024)                                           |
| Fields       | any `F: PrimeField + FromUniformBytes<64> + Ord` — Pasta, BN-254, Goldilocks |

## Roadmap

- First-class support for `halo2_axiom` and `pse-halo2` failure shapes
- JSON output mode for CI integration
- Visual cell-grid renderer (a la Chrome DevTools' DOM inspector)
- Coverage map: which gates fired, which didn't

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Parser improvements, new framework backends, and reproductions of unhandled `VerifyFailure` shapes are all welcome.

## License

[MIT](LICENSE).
