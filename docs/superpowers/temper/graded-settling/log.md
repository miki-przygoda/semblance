# graded-settling — temper log

Epic 0.3.0, piece 1 (epic tasks 2, 3, 4): the ensemble loader, the graded
integrator and the settling gates.

- Spec: `docs/superpowers/specs/2026-10-28-graded-settling-design.md`
- Plan: `docs/superpowers/plans/2026-10-28-graded-settling.md`
- Branch: `graded-settling`, from `main` at `ce1b079`. Host: local.
- Gates: `.temper/gate-quick.sh` (build, every test with fixtures required,
  clippy) and `.temper/gate-full.sh` (CI's steps plus the 1.85 build and the
  8-package check). Both are local scripts, not in git.

Entries below are appended by temper's stages and by the head.

## Checkpoint (MVP) — `graded-settling` at `0ab184e`

**Gate.** `.temper/gate-full.sh` passed (exit 0, `gate-full: passed`): build,
279 tests 0 failed with `SEMBLANCE_REQUIRE_FIXTURES=1`, the moving-edge gate,
clippy `-D warnings`, roundtrip `0.000000` twice, the 1.85 build, and 8
packages in `Cargo.lock`. Log: `.temper/graded-settling/logs/checkpoint-gate-full.log`
(local, not in git).

**Flows.** The spec has no section titled "user flows"; this piece adds no
command (`validate` is piece 3). The flows run are the ones the spec's Goal,
section 6, Failure handling and Done-when describe a user doing by hand.

| Flow | Result | Evidence |
|---|---|---|
| Run the settling gates and read their output | ok | `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis settling -- --nocapture --test-threads=1`: 20 passed in 38.5 s (39 s wall, debug, single-threaded; under the 120 s rule, no opt-level profile added). All 8 exact runs print `settled 0 of 45669 differ; settling centre 0 of N differ` (N = 13000 at 1/200, 3250 at 1/50). All 8 libm runs within bound: f32 worst ratios 0.169–0.327 settled, 0.042–0.228 centre (largest: run 2, settled 8.345e-7 at node 15739 Mi4, 0.327); f64 maxima 1.1e-15 to 9.4e-15, printed as `0.000 of bound`. Bit-equal settled counts 19628–33987 of 45669. The four supporting tests pass. |
| Run the suite without fixtures, plain | ok | `data` link moved aside: `cargo test --workspace` exits 0, 279 passed 0 failed, same as with data, as CLAUDE.md warns. |
| Run the suite without fixtures, `SEMBLANCE_REQUIRE_FIXTURES=1` | ok | exits 101, 227 passed, 52 failed (`--no-fail-fast`), matching the "52" in CLAUDE.md and README.md. A settling test fails naming the path: `SEMBLANCE_REQUIRE_FIXTURES=1 but .../data/flyvis-pretrained/ensemble.csv is missing`. |
| See the skips with `--nocapture`, as CLAUDE.md says | gap | `cargo test --workspace -- --nocapture --test-threads=1` without data: 18 of the 20 settling tests (the 16 runs and the two controls) print `ok` with no `skipping:` line. They share one `OnceLock` fixture, so only the test that initialises it (`reference_runs_are_the_eight_expected`) prints the notice. |
| Regenerate the torch synapse-count table by hand | ok | `tools/export_syn_count.py` run with the main checkout's `tools/.venv` python (not via `uv run`, to avoid a new environment): prints `torch.exp differs from math.exp on 108 of 2355 float64 counts`, `wrote crates/flyvis/src/syn_count_torch.rs`; `git status` clean afterwards, byte-identical. |
| Fetch and verify fixtures | ok | `sh ci/fetch-fixtures.sh`: `all 35 fixtures present and verified`. |
| Lossless roundtrip and zero-dependency checks | ok | `cargo run --quiet -- roundtrip`: structural and behavioural distance `0.000000`; `grep -c '^name = ' Cargo.lock` = 8. |
| Docs per spec section 7 | gap (minor) | CLAUDE.md: 279 tests, 52 skips, "the spiking path", the settling-gates paragraph, `syn_count_torch.rs` as generated, and what reads `flyvis-reference/` are all present. tools/README.md lists the exporter. README.md's `sim` row says settling reproduces flyvis bit for bit, but the `flyvis` row does not mention graded dynamics or the settling gate. |

**Gaps** (recorded, nothing fixed):

1. Under fixture absence, 18 settling tests skip with no notice even under
   `--nocapture`, contrary to CLAUDE.md ("A skip prints `eprintln!("skipping: ...")`")
   and the spec's failure table ("the test prints a skip notice and passes").
   Under `SEMBLANCE_REQUIRE_FIXTURES=1` they all fail correctly, so CI is not
   affected.
2. The libm gate prints the f64 ratios as `0.000 of bound` (maxima ~1e-15
   against a 1e-10 bound), which hides the ratio the docs say to read.
3. README.md's `flyvis` crate row was not updated for the settling gate, per
   spec section 7 ("the `sim` and `flyvis` rows").
4. The generated header of `syn_count_torch.rs` records `exported <date>`, so
   regenerating on a later day changes the file even when every value is the
   same.

## Head: acceptance and green branches brought in (2026-10-28)

The acceptance and to-green agents committed on side branches
(`temper/graded-settling/acceptance-*`, `green-1`) because `graded-settling`
was checked out in the merge worktree. `green-1` carried all four acceptance
commits plus the green fix, so the head fast-forwarded `graded-settling` to it
(`8ac6f4c`, no merge commit, nothing forced). Full gate after: passed, 330
tests, 0 failed, 69 s wall. CLAUDE.md's counts (279 tests, 52 skips) predate
the acceptance tests and are re-measured before the PR.
