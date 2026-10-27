# Contributing

semblance is a research project with one maintainer. Issues and pull requests
are welcome; for anything beyond a small fix, open an issue first so the change
can be discussed before you spend time on it. `CLAUDE.md` holds the fuller
version of the rules below.

## Rules that are easy to break by accident

- **Zero external dependencies.** `Cargo.lock` lists exactly 8 packages, all
  `semblance*`. Do not add a crates.io dependency, not even a small one or a
  dev-dependency for tests, without asking first. `tools/` is offline Python
  run by hand; nothing in the Rust workspace may depend on it at runtime.
- **The `semblance-rng` stream is frozen.** `crates/rng/tests/vectors.rs` pins
  its exact outputs. If those vectors fail, do not update them to match: find
  out what changed the stream.
- **`sim` is on the way out.** It models spiking neurons; the fly visual system
  is largely non-spiking, and the crate is to be replaced. Please do not extend
  `Lif`.
- **`ReducedCodec::encode` and `decode` are deliberately `todo!()`.** They are
  the research, with their own design to come, not missing code to fill in.
- **Rust 1.85+, edition 2024.** CI also builds on 1.85 exactly, and runs clippy
  on a pinned newer toolchain.

## Commits

Plain imperative subject lines with no `feat:` / `fix:` / `chore:` prefixes;
match `git log`.

## Verifying a change

```bash
cargo build --workspace
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --quiet -- roundtrip  # both distances must read exactly 0.000000
```

The flyvis parity tests need fixture data under `data/` (gitignored). Without
it they skip and still pass, which is why the test command above sets
`SEMBLANCE_REQUIRE_FIXTURES=1`: it turns each skip into a failure. Get the
fixtures with `ci/fetch-fixtures.sh` (needs an authenticated `gh`); see the
README. CI runs all of the above on every pull request.

A tolerance in a comparison is not to be widened to make a test pass. If a
comparison that should be exact stops being exact, that is the bug.

## Licence

By contributing you agree that your contribution is licensed under the
Apache License 2.0, as the rest of the code is (see `LICENSE`). Fixture data
derived from flyvis stays under flyvis's MIT licence; see `NOTICE`.
