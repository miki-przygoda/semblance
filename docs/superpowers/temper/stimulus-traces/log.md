# stimulus-traces — temper log

Epic 0.3.0, piece 2 (epic tasks 5 and 6, plus PR #12's open minors): the
hexal-to-cell input mapping and the stimulus-trace gates.

- Spec: `docs/superpowers/specs/2026-10-28-stimulus-traces-design.md`
- Plan: `docs/superpowers/plans/2026-10-29-stimulus-traces.md`
- Branch: `stimulus-traces`, from `main` at `ee4626c`. Host: local.
- Gates: `.temper/gate-quick.sh` and `.temper/gate-full.sh` (local, not in git).
- Run under the owner's delegation of 2026-10-28 (self-approved spec and plan;
  merge after local and self-hosted CI are green).

Entries below are appended by temper's stages and by the head.

## Checkpoint (MVP), 2026-10-29

Branch `stimulus-traces` at `6902af1`, worktree
`.temper/worktrees/merge-stimulus-traces`. No product code or tests changed.

### Full gate

`sh .temper/gate-full.sh`: **passed** (exit 0, `gate-full: passed`). Log:
`.temper/logs/stimulus-traces-checkpoint-gate-full.log` (local). 373 tests
passed, 0 failed, under `SEMBLANCE_REQUIRE_FIXTURES=1`; roundtrip read
`0.000000` twice; clippy clean; the 1.85 build passed; `Cargo.lock` has 8
packages.

### Flows

The spec has no section titled "user flows", so the flows below come from its
Goal and "Done when" sections, run by hand.

| # | Flow | Result | Evidence |
|---|---|---|---|
| 1 | Input-mapping gate on flyvis's lattice | ok | `cargo test -p semblance-flyvis -- --nocapture --test-threads=1 trace_checks:: wiring::tests::input_mapping` (fixtures required): `wiring::tests::input_mapping_matches_flyviss_lattice ... ok`; 23 passed, 0 failed, 102 s |
| 2 | Exact trace gates, 8 runs x 12 sequences | ok | each of `trace_exact_run_0..7` printed "12 of 12 sequences bit-equal, centre and snapshots" (000 and 049, dt 1/200 and 1/50, f32 and f64) |
| 3 | libm trace gates within R3 | ok | all 8 passed. Worst ratios to bound: f32 runs 0/2/4/6 at 2.764e-1, 3.198e-1, **8.605e-1** (run 4, snapshots 8.801e-5, sequence 3, frame 359, node 15414, Mi4), 2.217e-1; f64 runs at or below 3.121e-3 |
| 4 | Supporting tests | ok | `trace_bounds_match_provenance ok`; `snapshots_agree_with_the_centre_column ok`; `the_trace_gate_sees_a_wrong_mapping`: "snapshots max 2.503223408264881e0, centre max 2.291526948382731e0" (> 1e-3, matches the spec's ~2.5); `a_run_from_flyviss_settled_state_is_bit_equal_too ok` |
| 5 | Stimulus check: Rust generators vs flyvis's input | ok | flashes at dt 1/200 and 1/50: "all 4 sequences equal flyvis's input, every frame and hexal". Moving edges at both dts: max 2.897e-8 / 3.790e-3 / 1.961e-3 / 3.878e-3 at 0/30/90/150 degrees (TOLERANCE 0.005), "0 degrees bit-equal to flyvis's f32 input" |
| 6 | Piece 1 tests and PR #12 minors | ok | in the gate log: `acceptance_settling` 13 passed (incl. `settling_04_..._bit_equal_count`, `settling_08_...`); `graded::tests::refuses_a_dt_that_is_subnormal_in_f32`, `relu_passes_nan_through_like_torch`, `set_state_*`, `gate_support::tests::a_nan_reference_value_makes_the_maximum_infinite`, `syn_count_torch_checks::the_generated_header_carries_no_export_date` all ok |
| 7 | Documented counts, fresh clone without `data/` | gap | new detached worktree `.temper/worktrees/checkpoint-nofixtures` (no `data`). Plain `cargo test --workspace`: 373 passed, 0 failed (matches CLAUDE.md). `SEMBLANCE_REQUIRE_FIXTURES=1 ... --no-fail-fast`: 283 passed, 90 failed (matches "90 of those tests skip"). But under `-- --nocapture --test-threads=1`, only 79 of those 90 tests print a `skipping:` notice (91 notices in all, since some tests print one per missing file). See gap 1 |
| 8 | Fixture provenance for the trace files | ok | `ci/fixtures.sha256` lists `centre.*`, `snapshots.*`, `input.f64.bin`; `shasum -a 256 -c` from `data/`: all 11 `flyvis-reference/` entries OK |
| 9 | The program's commands | ok | `cargo run -- help` lists demo/roundtrip/evolve/help, unchanged (no CLI is in scope; `validate` is piece 3); `roundtrip`: structural and behavioural distance `0.000000` |

### Gaps

1. **The skip-notice docs are wrong for `tests/acceptance_settling.rs`.**
   CLAUDE.md now says the notices are visible under `--nocapture` "one per
   skipped test ... every test that finds one missing prints its own notice",
   and README says each skipped test passes "printing a skip notice". Measured
   without `data/`: 11 of the 90 fixture-gated tests print nothing.
   `acceptance_settling.rs` loads one shared `OnceLock` fixture (`fx()`) and
   prints its notice once, from `settling_01_...`, and
   `settling_02`-`settling_11` and `settling_13` return silently. The test
   count and skip count (373, 90) are correct. Either the docs or that file's
   skip reporting needs to change. Recorded only; not fixed here.

Not checked at this checkpoint: the "Done when" items that come after it
(self-hosted CI green, PR merged, branches cleaned up).
