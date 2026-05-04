# Contributing to ZkForge

Thank you for considering a contribution. ZkForge is a small, focused crate; the bar for changes is "does this make Halo2 debugging objectively faster?" If yes, please send a PR.

## Quick start

```bash
git clone https://github.com/<you>/zk-debug
cd zk-debug
cargo test           # 5 unit tests (hex helpers)
cargo run            # demo binary; produces a colored failure trace
```

The demo in `src/main.rs` is the de facto integration test — if your change breaks the demo's output, it's load-bearing.

## What we accept

- **Parser improvements.** New regex patterns, additional `VerifyFailure` variant coverage, robustness fixes for halo2 versions we haven't tested against.
- **New framework backends.** `halo2_axiom`, `pse-halo2`, `halo2curves`-based forks. See [Architecture.md](Architecture.md#adding-a-new-framework) for the integration recipe.
- **Reporter polish.** Better suggestions, additional `= note:` context, machine-readable output modes (JSON, SARIF).
- **Reproductions.** A failing input + the raw `format!("{:#?}", failure)` dump is enough to open an issue. We will turn it into a regression test.

## What we are unlikely to merge

- Changes that introduce `unsafe` to access private halo2 fields. The whole point of the crate is that it works without forking.
- New top-level crates / sub-crates without a clear separation of concerns.
- Features behind heavy dependencies (tokio, async runtimes, GUI frameworks). ZkForge is a CLI-shaped library and intends to stay that way.
- Reformatting churn unrelated to the change at hand.

## Adding support for a new framework

Most ZK-circuit frameworks in Rust derive `Debug` on their failure type. If yours does, the playbook is:

1. **Add a feature flag** in `Cargo.toml`:
   ```toml
   [features]
   axiom = ["halo2_axiom"]
   ```
2. **Capture a real failure** by running an intentionally broken circuit and copying the raw `format!("{:#?}", failure)` output into `tests/fixtures/<framework>_<variant>.txt`.
3. **Write a parallel parser** in `src/<framework>/parser.rs`, gated on the feature. Reuse `hex.rs` and `report.rs`.
4. **Implement `ZkDebug`** for the new prover type, alongside the existing `MockProver` impl.
5. **Document the new types** in [Architecture.md](Architecture.md) and add a usage example to the README.

We prefer one PR per framework so review stays focused.

## Improving the parser

The parser is intentionally small (~150 LOC). Two principles govern changes:

1. **Brace-balanced slicing before regex.** Anything that needs to extract values from a nested structure should isolate the structure first (see `isolate_cell_values_block`). Pure regex over the whole dump is a recipe for cross-pairing bugs.
2. **One regex per concept.** Don't merge two extraction passes into one mega-pattern. The cost of two `captures` calls is negligible; the cost of debugging a 200-character regex is not.

Add a unit test in `parser.rs` for any new pattern. The existing tests in `hex.rs` are the style template.

## Style

- `cargo fmt` before pushing.
- `cargo clippy --all-targets -- -D warnings` is expected to pass.
- No emojis in source files or docs (the demo output is the one exception, and it currently has none).
- Comments explain *why*, not *what*. The code already shows what.

## Filing bugs

A useful bug report contains:

1. The halo2 version (`halo2_proofs = "0.x.y"`).
2. The Rust version (`rustc --version`).
3. The minimal circuit that reproduces the failure.
4. The raw `format!("{:#?}", failure)` dump that ZkForge mishandled.
5. What you expected the report to say.

Issues without (4) will be closed with a request for the dump — we cannot debug a parser regression we can't reproduce.

## Releases

We follow [SemVer](https://semver.org). Pre-1.0 minor bumps may include breaking changes; patch releases never do. The `Architecture.md` document is the source of truth for any internal change that affects extension authors.

## License

By contributing, you agree your contributions will be licensed under [MIT](LICENSE).
