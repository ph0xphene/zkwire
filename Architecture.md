# ZkForge Architecture

A short tour of the internals. Read this if you intend to extend the parser, add support for a new ZK framework, or understand why the public API looks the way it does.

## Design goals

1. **Zero modifications to `halo2_proofs`.** The crate must work against unmodified upstream so it can be adopted by teams who can't tolerate a fork in their dependency graph.
2. **No private-field access.** `MockProver::instance()` and `VerifyFailure`'s fields are `pub(crate)`. Accessing them via `unsafe` transmute or shim crates would tie ZkForge to a specific halo2 binary layout. We refuse that path.
3. **Source-level traceback.** A diagnostic that says "permutation failed at offset 1 in region `mul`" is barely better than the raw dump. The diagnostic must point back at the *Rust line* that wrote the bad value.

These three goals constrain almost every implementation choice below.

## Module map

```
src/
├── lib.rs        — re-exports; defines the public surface
├── tracker.rs    — ZkForgeTracker, AssignSite, ZkDebug trait, zk_assign! macro
├── parser.rs    — VerifyFailure → ZkReport (regex + brace-balanced slicing)
├── report.rs    — ZkReport, ErrorType, Location data types
├── reporter.rs  — Cargo-style colorized output (forge_trace)
├── hex.rs        — BigUint-backed hex/decimal helpers
└── main.rs        — demo binary
```

## String reflection over private fields

Halo2's `VerifyFailure` derives `Debug`. The output of `format!("{:#?}", failure)` is a structural dump — `ConstraintNotSatisfied { constraint: …, location: InRegion { region: Region { index: N, name: "…" }, offset: M }, cell_values: [ … ] }`. Every field we need is in that string.

Reflecting on `Debug` output trades two kinds of risk:

| Approach                | Coupling                                    | Risk                                                  |
|-------------------------|---------------------------------------------|-------------------------------------------------------|
| Private field access    | Tight to halo2's binary layout              | Breaks on minor version bumps; requires `unsafe`       |
| String reflection       | Loose to halo2's `Debug` derivation         | Breaks if upstream changes derive (rare; visible diff) |

We took the second. The implementation is a few hundred lines of regex and one brace walker, and a `Debug` change in upstream produces an obvious test failure rather than UB.

## Double-anchored parsing

The parser's job is to extract `(error_type, region, offset, column_index, gate, cell_values)` from the failure dump. A naive regex over the whole dump produces false pairings — e.g. matching a region's `name:` against a hex literal that belongs to a different `cell_values` entry deeper in the structure.

The parser is therefore **double-anchored**: structural slicing first, regex second.

### Step A — brace-balanced isolation

`isolate_cell_values_block` walks `[` and `]` byte-by-byte starting at `cell_values:` and returns the slice covering the matching closing bracket. This is a dependency-free linear scan; the alternative (running a full Rust parser over the dump, or chaining heuristic regex) is heavier and less reliable.

```rust
// parser.rs
fn isolate_cell_values_block(raw: &str) -> Option<&str> {
    let key = raw.find("cell_values:")?;
    let after = &raw[key..];
    let bracket_rel = after.find('[')?;
    let mut depth = 0usize;
    for i in bracket_rel..after.len() {
        match after.as_bytes()[i] {
            b'[' => depth += 1,
            b']' => { depth -= 1; if depth == 0 { return Some(&after[bracket_rel..=i]); } }
            _ => {}
        }
    }
    None
}
```

### Step B — regex inside the slice

Within the isolated slice, a single regex extracts every `(name, hex)` pair:

```text
(?s) name: "([^"]+)" [^"]*? , \s* "(0x[0-9a-fA-F]+)"
```

The `[^"]*?` non-greedy class spans the `column: Column { … }` and `rotation: …` fields between the cell name and its value, but cannot cross another quoted string — making it impossible to pair a name with a hex literal that belongs to a different cell. Halo2's Debug emits column types unquoted (`column_type: Advice`), which is what makes this regex robust.

### Step C — disambiguate `Permutation`

`VerifyFailure::Permutation` covers both public-input mismatches and copy-constraint failures between advice cells. We disambiguate by scanning for `column_type: Instance`:

```rust
fn detect_error_type(raw: &str) -> ErrorType {
    match leading_ident(raw) {
        "Permutation" if column_type_matches(raw, "Instance") => ErrorType::InstanceMismatch,
        "Permutation" => ErrorType::PermutationMismatch,
        // …
    }
}
```

This is the only place where ZkForge classifies one halo2 variant into two ZkForge variants. The split is justified by the very different remediation advice each case requires.

## Shadow mapping

The `ZkForgeTracker` mirrors halo2's internal cell layout in a small `HashMap`, indexed by a key the parser can reconstruct from the failure dump:

```rust
#[derive(Hash, Eq, PartialEq)]
struct AssignKey {
    region: String,        // e.g. "mul"
    offset: usize,         // within-region row
    column_index: usize,   // halo2's Column.index
}

struct AssignSite {
    region: String,
    offset: usize,
    column_index: usize,
    cell_name: String,
    file: &'static str,
    line: u32,
}
```

The `zk_assign!` macro inserts one `AssignSite` per call. The parser, on extracting `(region, offset, column_index)` from a `VerifyFailure`, performs an `O(1)` lookup against this shadow map. If a hit is found, the report acquires an `origin: AssignSite` and the reporter renders a `= note:` line pointing at `file:line`.

The same column-index reflection trick (`format!("{:?}", column)` → parse `index:`) is used in the macro because halo2 0.3.2 demoted `Column::index()` to `pub(crate)`. Consistent with our "no private API" stance.

### Why a thread-local?

Halo2 chips often live in library crates that don't know about ZkForge. Threading a `&ZkForgeTracker` through every chip method would force tracker-awareness into chip APIs we don't own.

Instead, `ZkForgeTracker::install(&tracker)` stores an `Arc<ZkForgeTracker>` in a `thread_local!` cell and returns a `TrackerGuard`. The macro reads the thread-local; if no tracker is installed, the macro is a no-op (one thread-local read, one `Option::None` branch). The `TrackerGuard` restores the previous tracker on drop, so nested installations compose.

This is the same pattern that `tracing` uses for its current-subscriber dispatch. The contract: `MockProver::run` synthesizes on the calling thread, so the thread-local is visible to every `zk_assign!` invocation made during synthesis.

## The reporter

`forge_trace(&[VerifyFailure], &ZkForgeTracker)` is the single entry point. It:

1. Pulls public inputs from the tracker (recorded by `tracker.record_public_inputs(&inputs)`) and renders them as `decimal (raw_hex)`.
2. Calls `parse_failure` once per failure to produce a `ZkReport`.
3. Emits one `error[NN]:` block per report with `--> location`, optional `values found`, optional `= note: file:line` (when origin is found in the tracker), and `= help: <suggestion>`.

Color via the `colored` crate. The format is intentionally close to `rustc` so the output looks native to a Rust terminal session.

## Field-element decoding

`hex::hex_to_decimal` strips the `0x` prefix, drops leading zeros, and parses through `BigUint::parse_bytes(s, 16)`. `u128` was rejected up front: Pasta `Fp` is 255 bits, BN-254 `Fr` is 254 bits, Goldilocks is 64 bits. A single `u128` path would silently wrap or truncate the larger fields.

`hex::humanize_hex` runs the conversion across an arbitrary string but only swaps in the decimal form when the result is ≤ 12 digits. Beyond that, decimal is no more readable than hex, so we leave the hex in place.

## Adding a new framework

The parser pattern (`Debug` reflection + brace-balanced slicing + regex) generalizes. To add `halo2_axiom` or `pse-halo2`:

1. Add a feature flag (`axiom`, `pse`, …) in `Cargo.toml`.
2. Behind the flag, define a parallel set of `parse_*` functions that target the new failure type's `Debug` shape.
3. If the failure shape diverges, introduce a new `ErrorType` variant and a corresponding suggestion in `build_suggestion`.
4. Add a `ZkDebug` impl for the new prover type.

Reproductions of the new framework's `Debug` output are the unit of work — paste the dump into a test, write the regex, ship.
