# ZkWire

**From cryptic field elements to source-linked Halo2 diagnostics.**

ZkWire is a unified CLI and Rust SDK for Halo2 circuit auditing. It turns raw `MockProver` / `VerifyFailure` dumps into readable reports with decoded field values, source snippets, semantic labels, and security-focused warnings.

Built for ZK researchers who need to move fast without losing audit-grade context.

## Demo

Run the built-in demo:

```bash
zkwire demo
```

The demo renders a pre-baked Halo2 constraint failure as a Cargo-style diagnostic:

```text
error[01]: gate constraint not satisfied
  --> region `broken fibonacci`  ·  offset 8  ·  Advice column #2  ·  gate `fibonacci step`
  = layout: broken fibonacci around offset 8
    ┌─────┬──────────┬──────────┬──────────┬──────────┬──────────┐
    │ row │ Advice 0 │ Advice 1 │ Advice 2 │ Advice 3 │ Advice 4 │
    ├─────┼──────────┼──────────┼──────────┼──────────┼──────────┤
    │  6  │    ·     │    ·     │    ·     │    ·     │    ·     │
    │  7  │    ·     │    ·     │    ·     │    ·     │    ·     │
    │  8  │    1     │    1     │    3     │    ·     │    ·     │
    │  9  │    ·     │    ·     │    ·     │    ·     │    ·     │
    │ 10  │    ·     │    ·     │    ·     │    ·     │    ·     │
    └─────┴──────────┴──────────┴──────────┴──────────┴──────────┘
  values found
    │ fib(a) = 1
    │ fib(b) = 1
    │ fib(c) = 3 (from expression: a + b + 1)
  = note: value first assigned at ./examples/broken_fibonacci.rs:74:1  (annotation `fib(c) intentionally wrong`)
  source context
    --> ./examples/broken_fibonacci.rs:74:1
    │
       72 │                     Value::known(Fp::from(1)),
       73 │                 )?;
    >  74 │                 zkwire_assign!(
       75 │                     region,
       76 │                     "broken fibonacci",
  = warning: Potential Boolean Constraint Weakness: `fib(a)` is 1. Confirm this cell is explicitly boolean-constrained if it gates security logic.
  = warning: Potential Boolean Constraint Weakness: `fib(b)` is 1. Confirm this cell is explicitly boolean-constrained if it gates security logic.
  = help: Gate `fibonacci step` failed in region `broken fibonacci` at offset 8. The witness violates the gate's algebra — recompute the gate by hand using the cell values shown.
```

What this gives you:

- A 5x5 local view of the Halo2 table around the failing offset.
- Field elements decoded from 256-bit hex into decimal using `BigUint`.
- A clickable `file:line:column` source link.
- The surrounding Rust source code injected into the terminal.
- Semantic context from the Rust expression that assigned the value.
- Early security heuristics for underflows and boolean-like values.

## Features

### Source-To-Terminal Injection

ZkWire reads the tracked source file and prints the failing line plus two lines above and below it.

No detached debugger. No guessing which assignment produced the witness value. The source appears inside the diagnostic stream.

### Semantic Context

Use `zkwire_assign!` where you assign advice cells:

```rust
zkwire_assign!(
    region,
    "broken fibonacci",
    "fib(c)",
    config.advice[2],
    0,
    Value::known(a + b + Fp::one()),
)?;
```

The macro records:

- Region label.
- Advice column index, reflected from Halo2 `Debug` output.
- Offset.
- Source `file!()` and `line!()`.
- Human annotation.
- Rust expression via `stringify!`.

Reports can then explain not just where a value was assigned, but what expression produced it.

### Security Auditing

ZkWire includes alpha-stage heuristics for common ZK footguns:

- **Potential Underflow Detected** when a decoded value is close to a known field modulus.
- **Potential Boolean Constraint Weakness** when a value is `0` or `1` and may require explicit boolean constraints.

These warnings are intentionally conservative. They are audit prompts, not proofs of vulnerability.

### Stream Processing

ZkWire is designed for log streams.

```bash
cargo test | zkwire explain
```

`explain` reads stdin line-by-line, buffers only the current brace-balanced `VerifyFailure`, and emits diagnostics as soon as each failure block is complete. This keeps memory usage stable on large logs.

For CI and auditing scripts:

```bash
cargo test | zkwire explain --json > zkwire-report.json
```

JSON mode emits a machine-readable array of detected failures and suppresses unrelated log lines.

## Installation

```bash
cargo install zkwire
```

Requires Rust 1.85+.

## Workflow

Run the demo:

```bash
zkwire demo
```

Explain test output:

```bash
cargo test | zkwire explain
```

Export CI-readable JSON:

```bash
cargo test | zkwire explain --json > audit.json
```

Use as a Rust SDK inside tests:

```rust
use std::sync::Arc;
use zkwire::{ZkDebug, ZkWireTracker};

let tracker = Arc::new(ZkWireTracker::new());
let _guard = ZkWireTracker::install(&tracker);

let prover = MockProver::run(k, &circuit, public_inputs).unwrap();
let _ = prover.verify_and_forge(&tracker);
```

## Technical Stack

- **Rust**: 1.85+, Edition 2024.
- **Circuit framework**: Halo2 `MockProver` diagnostics.
- **Parsing**: Regex-based string reflection over public `Debug` output. No private Halo2 APIs.
- **Field decoding**: `num-bigint::BigUint` for 256-bit field elements without truncation.
- **CLI**: `clap` derive API.
- **Terminal output**: `colored` with Cargo-style `error`, `note`, `warning`, and `help` lines.
- **CI output**: `serde` / `serde_json`.

## Why String Reflection?

Halo2 does not expose all `VerifyFailure` internals as stable public fields. ZkWire deliberately avoids private APIs, unsafe layout assumptions, and Halo2 forks.

Instead, it parses the public `Debug` representation:

```rust
format!("{:#?}", failure)
```

This is not as clean as a first-class Halo2 diagnostics API, but it is portable, auditable, and works with stock `halo2_proofs`.

## Status

ZkWire is early-stage security tooling.

The current focus is high-signal debugging for Halo2 circuits:

- Better failure localization.
- Better source context.
- Better field-element readability.
- Better audit prompts.

Expect the heuristics and report schema to evolve as more real-world failure shapes are collected.

## License

MIT.
