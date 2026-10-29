# parity-validate: temper log

## MVP checkpoint (2026-10-29)

Run on `parity-validate` at `7e400e4`, in the worktree
`.temper/worktrees/checkpoint` (branch `temper/parity-validate/checkpoint`,
because `parity-validate` is checked out in the merge worktree). Nothing in
the product or the tests was changed.

**Gate.** `sh .temper/gate-full.sh`: **passed**, exit 0, 146 s wall. 478
tests passed, 0 failed, with `SEMBLANCE_REQUIRE_FIXTURES=1`; roundtrip
`0.000000` twice; clippy clean; the 1.85 build passes; `Cargo.lock` has 8
packages, all at 0.3.0. The log is `target/gate-full.log` in the checkpoint
worktree, and `target/moving-edge.log` holds the geometry gate's distribution.

**Flows.** The spec has no section titled "user flows". The flows run are the
ones its Goal, sections 4 to 7 and 9, Failure handling and Done when describe
a user doing by hand, against `target/release/semblance`.

| Flow | Result | Evidence |
|---|---|---|
| `semblance help` | ok | exit 0. It prints the help text of section 4 word for word, `validate` line and data paragraph included. `app/src` no longer mentions `clap`. |
| `cargo run --release -- validate`, fixtures present | ok | exit 0 in 9.3 s (16 threads). Header: reference `20261027T114358Z-63037`, flyvis 1.2.0, torch 2.14.0; models 000 (5.137) and 049 (5.678); libm `f32:106:55fdb5ef0c7a79a5,f64:108:8970802a716917ba`. All 16 exact rows `0 ==`. Every libm row is within its bound, worst `8.605e-1` (traces run 4). All four controls hold: wrong model `6.397e0`, integer counts `4.963e-7`, mirrored `2.503223408264881e0`, settled start `0 unequal`. Coverage: Tm30 `3.773e-3`, ratios `1.102e3 3.801e7 7.439e2 3.773e7 6.245e2 3.248e8 4.618e2 3.179e8`. `verdict: PASS`. Every number matches section 6's example; the `…` slots are now filled in. |
| `validate --threads 1` and `--threads 64` | ok | Both exit 0. `--threads 1` takes 64 s wall, and its table is identical to the default run's once the thread count and time are stripped. `--threads 64` is capped to `16 threads`. |
| `validate --markdown` against the committed record | ok | exit 0, nothing on stderr. Diffed against the block in `docs/results/0.3.0-flyvis-parity.md`, only the host rows (target, debug assertions, libm counts and fingerprint) and the libm section differ: the Mac's libm (106/108) is not CI's (104/104). The reference, the exact sections, the controls and the coverage section are identical. |
| Record test on the Mac | ok | `cargo test -p semblance --test record -- --nocapture`: 6 passed. It prints `record: libm sections not compared: this host's libm fingerprint f32:106:… differs from the record's f32:104:…`, as CLAUDE.md says. |
| No `data/` | ok | `validate` in an empty directory: exit 2, `validate: nothing was verified; 13 fixture files are missing:`, then the 13 `./data/...` paths, the `ci/fetch-fixtures.sh` hint and the record sentence, all on stderr. `--data` naming a directory that does not exist gives the same message with that directory's paths. |
| Exactly one fixture missing | ok | The `centre.f64.bin` link is removed from a copy of `data/` made of links: exit 2, `1 fixture file is missing`, and its `how_to_get` hint is printed in full (`Run ci/fetch-fixtures.sh, or \`uv run --project tools tools/export_reference.py\`.`). |
| Malformed fixture (truncated) | ok | `centre.f64.bin` cut to 1000 bytes: exit 2, `validate: nothing was verified: centre.f64.bin: 1000 bytes, manifest says 8577920`. |
| A reference value differs (exit 1 path) | ok | One f64 value in a copy of `centre.f64.bin` is changed by 1e-6: exit 1. Rows `traces 1 exact ... 1 1.000e-6 ... FAIL` and `traces 1 libm ... 1.000e4 FAIL`, then `coverage: run 1 not measured: exact job failed`, then `verdict: FAIL — traces run 1 exact, traces run 1 libm`. The other runs still report. |
| A reference value is ±0.0 | ok | The same value set to 0.0: exit 2. Each run 1 row reads `reference value 5000 is ±0.0; == is bit equality only without signed zeros FAIL`, and the verdict is `NOT VERIFIED — a fixture did not fit; nothing counts as verified: ...`. |
| Usage errors | gap (minor) | `--threads 0`, `--threads x`, `--bogus`, `--markdown --markdown`, `--threads 2 --threads 3`, and `--data` with no value all exit 2 with the usage line on stderr, as required. The messages are poor, though: see gap 1. |
| `validate \| head` | ok | `semblance validate 2>err \| head -1`: semblance exits 0 (its computed code), stderr is empty, no panic. |
| Suite without fixtures (CLAUDE.md counts) | ok | With the `data` link moved aside, plain `cargo test --workspace` gives exit 0, 478 passed. With `SEMBLANCE_REQUIRE_FIXTURES=1 --no-fail-fast` it gives exit 101, 349 passed, 129 failed. `-- --nocapture --test-threads=1` prints 141 `skipping` notices. All three match CLAUDE.md (478, 129, 141). |
| Packaging (section 9 pre-merge check) | ok | `cargo package --workspace --locked` exits 0. The 8 `0.3.0` crates hold 16 `LICENSE`/`NOTICE` entries, and only `semblance-rng` ships a `tests/` file. `cargo publish --workspace --dry-run --locked` exits 0 (`aborting upload due to dry run` for each crate). |
| Docs (section 10) | ok | README has the `validate` headline paragraph (exit codes), the quickstart line, and the flyvis and app rows. CLAUDE.md has `v0.3.0`, the validate and record paragraphs, and the counts. The record's `Generated:` line names Rust 1.97, `ubuntu-latest`, CI run 40967303221 and 2026-10-29. `CITATION.cff` has `version: 0.3.0` but still `date-released: "2026-10-04"`; section 9 leaves that date for the head to set at tag time. |

**Gaps** (recorded, nothing fixed):

1. Usage errors name the wrong thing. A bad `--threads` value (`0`, `x`) and a
   repeated flag (`--markdown --markdown`, `--threads 2 --threads 3`) all print
   `validate: unknown argument <every argument joined>`, as if the flag itself
   were unknown. The exit code (2) and the usage line are right.
2. The coverage line and the record's coverage sentence name the weakest type
   as `Tm30, run 3`, but section 6's example and section 7's coverage text say
   `run 2`. Runs 2 and 3 tie at `3.773e-3` (the same model and time step in f32
   and f64), and the code breaks the tie towards the later run. The record is
   consistent with the code, and the preamble quotes only the ratios, so
   nothing fails.
3. Cosmetic. When `where` is longer than its 30-character column (for example
   `centre, seq 0 frame 76 (TmY10)` on a failing row), the bound and ratio
   columns shift right. The header prints `1 threads` for `--threads 1`.
4. Pending head step, not a defect: `CITATION.cff`'s `date-released` is still
   `2026-10-04` and is set at tag time (section 9).
