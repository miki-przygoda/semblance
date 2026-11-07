## What changed

This epic builds the parts of 0.4.0 tasks 3 and 4 that read no fixtures-3: the response indices, the representational axis, gate C's controls and gate E. Gates A–D, which need `data/flyvis-indices/`, belong to the follow-on `fidelity-gates` epic (see "Open items").

**`semblance-sim`**
- `semblance_sim::response`: one network's central responses to a battery of stimuli. Every 0.4.0 index reads this.

**`semblance-flyvis`**
- `parity::respond`, `respond_model` and `respond_network` settle any 734-parameter network the way the settling and trace gates do, then record each cell type's central response over a battery. `settle_core` now takes the network, so `respond` and `respond_network` share one path and `respond` stays bit-identical to flyvis's reference centre traces. `central_nodes` derives the central nodes from the network and checks them against the manifest.
- `parity::controls` (public, not re-exported) builds gate C's four controls: zeroed, shuffled, random parameters, and rewired.
  - Every draw is seeded by `control_seed = 0x5EB1_A9CE_0000_0000 ^ (control << 24) ^ (model << 8) ^ attempt`.
  - An unstable draw is redrawn up to 10 times (`MAX_REDRAWS`). The zeroed control gets one attempt.
  - Shuffling uses Durstenfeld's Fisher–Yates. Random parameters are drawn uniformly within the ensemble's range. The rewired control moves each synapse-count group with its edge.
  - Redraws and dropped counts are returned, never printed.
- New dependency on `semblance-rng`, which is in the workspace. `Cargo.lock` still has 8 `semblance*` packages.

**`semblance-fidelity`**
- `indices`: flyvis's four response indices and the paper's agreement rules, as pure functions over a `Responses`.
  - The indices are FRI, DSI, preferred direction and tuning correlation. The module also has the FRI and moving-edge windows and flyvis's to-known helpers.
  - The agreement rules are polarity, the exact `u128` binomial rule, `pd_within` (pairing T4 with ON and T5 with OFF) and `tuning_r`.
  - The functions emulate flyvis's dtypes operation by operation. For example, an F32 run's FRI uses an f32 numerator, an f64 epsilon and an f32 result. An F32 run holding a value that is not an f32 is refused, never rounded.
  - Reductions do not depend on the layout's order.
- `similarity`: debiased linear CKA over each cell type's central response time course (the representational axis). Beside it are Zhou & Hasler's peak-voltage RDMs, rank correlations and biased CKA. These are reported only, and refuse any type count other than 65.
- **Gate E:** `compare_behaviour`, `behavioural_distance` and `size_fidelity_curve` return `Result<_, FidelityError>`. A shape mismatch, an empty battery or nothing to compare is refused. Before, each of these scored as a perfect 0.
- New dev-dependency on `semblance-rng`. `semblance-fidelity` never depends on `semblance-flyvis`; ground truth arrives as arguments.
- Hardening after acceptance:
  - HSICs and biased CKA norms are scaled before dividing, so their product cannot overflow.
  - `choose` cancels the gcd, so C(n, i) stays exact up to n = 128.
  - These are refused instead of panicking or scoring: an RDM size whose square overflows, a window whose frame count overflows `usize`, a binomial `p0` that is NaN or outside [0, 1], and `tuning_r` with no known curves.

**`semblance` (app)**
- `roundtrip` handles `behavioural_distance`'s `Result`. It still prints `0.000000` for both distances.

**Docs**
- Specs: `docs/superpowers/specs/2026-11-04-*` contains the 0.4.0 specs. The response-indices and representational-axis specs now note the `Result` signatures and the widened tuning curves.
- `CLAUDE.md`:
  - The test count is now 715 and the skip count 196.
  - New paragraphs cover `respond`, `indices`, `similarity`, `controls` and gate E.
  - It names the 19 ignored acceptance-failing tests and says they are un-ignored as their gates land.
- `README.md`: the skip count is now 196, and the `fidelity` row and the intro now say that the metric's measures exist but its gates do not yet.
- `DESIGN.md`: the behavioural-fidelity section and the `flyvis`, `sim` and `fidelity` entries describe the new modules.
- `semblance-fidelity`'s package description now names the indices and CKA. This takes effect on crates.io only at the next publish, which waits for the owner.
- Temper records are under `docs/superpowers/temper/fidelity-measures/`: the plan, the plan review, the behaviour spec, the codemap and the log.

## Why

0.4.0 asks whether a network still behaves like the fly, measured against flyvis's own recordings and indices rather than a structural diff. These are the measures that question needs:
- indices to compare with the paper's known biology;
- CKA for the representational axis;
- controls that a score must separate from trained models before it means anything;
- gate E, so malformed input can no longer pass as perfect fidelity.

The fixture-free parts are split from the gates so this epic can land without fixtures-3. Without that split, any test reading `data/flyvis-indices/` would fail under `SEMBLANCE_REQUIRE_FIXTURES=1`.

## How it was tested

- **MVP checkpoint**: `gate-full` at `9d912fa` (jobq `jq-01M6QBAZDBYHBBDHH3VKTCKB12`) passed.
  - 656 tests passed, 0 failed, with no skip notices.
  - `roundtrip` printed `0.000000` twice.
  - Release-binary flows (`jq-01M6QCK6Z0DPP8TSEKPTPYZ4G0`): `validate`'s 0.3.0 rows are unchanged. The 16 exact rows show `unequal 0` and the 16 libm rows are within bounds (worst `7.428e-1`, the record's value). All 4 controls hold, `verdict: PASS`, exit 0.
- **Acceptance tests**: 66 behaviours in `behaviour.md`: 19 for indices, 13 for agreement, 9 for refusals, 13 for similarity and 12 for controls. They live in `crates/fidelity/tests/acceptance_*.rs`, `crates/flyvis/tests/acceptance_controls.rs` and `app/tests/acceptance_{indices,agreement,similarity,controls,refusals}.rs`. Fixes driven by those tests followed, ending at 715 tests.
- **This docs commit**: `gate-full` was run on the final tree. See the result below.

**Final tree: `gate-full` passed.** It ran as jobq job `jq-01M6VFWJMA4HTJT3FYCMV32ADF` on `248eed8` plus this commit's doc edits. Its log on beast is `/srv/bulk/Projects/semblance/logs/gate-full-20261107T171121-3290368.log`.
- 715 tests passed, 0 failed, and 19 were ignored (the acceptance-failing tests listed below).
- No skip notices appeared.
- `roundtrip` printed `structural distance:  0.000000` and `behavioural distance: 0.000000`.
- The gate ended `gate-full: passed`.

## Open items

From `docs/superpowers/temper/fidelity-measures/log.md`, plus what writing the docs turned up:

1. **Gate A is not in `validate`.** It has no rows and no control. The scope split moves it to `fidelity-gates`, which waits for the owner to sign off fixtures-3 (`data/flyvis-indices/`).
2. **`validate --ensemble` does not exist.** This covers gate A over all 50 models at f32, gate B, and gates C and D with their timing. The same split moved it to `fidelity-gates`.
3. **Exit 2 for a missing `flyvis-indices/` cannot be checked yet.** That waits until `validate` reads that directory (`fidelity-gates`).
4. **The spec has no user flows section.** The checkpoint's flows came from §5 and "Proof".
5. **19 acceptance tests are ignored as `acceptance-failing`.** They are INDICES-03 and -16–19, AGREEMENT-07–13, SIMILARITY-11–13 and CONTROLS-09–12. They describe gates A–D and `--ensemble`, and `fidelity-gates` should un-ignore them as it builds each gate.
6. **Zhou & Hasler's cosine epsilon** is placed per deviation A14 but not yet checked against their numbers. That is planned for `fidelity-gates`.
7. **Nobody reviewed the plan before the build.** It ran overnight under the owner's hand-over on 2026-11-05 (`overnight_without_plan_review: yes`). Decisions A1–A20 in `plan.md` and the 16 applied review findings in `plan-review.md` are recorded for checking afterwards. The deviations to check are A5/A6, A8 and A14 (`Result` returns, f64 tuning curves, the 65-type refusal), and the redraw count, which the review revised to `MAX_REDRAWS = 10`.
8. **Licensing, for the owner to decide.** `semblance-fidelity` is licensed `Apache-2.0` only, but `indices` ports flyvis's MIT-licensed index code: the FRI and edge windows, the peaks, DSI, preferred direction and the to-known helpers. In `semblance-flyvis` the same kind of port is attributed in `NOTICE` and the package is `Apache-2.0 AND MIT`. Neither `NOTICE` nor this crate's licence was changed here. It is a licensing decision, flagged for the owner before the next publish.
9. **`RESOURCES.md` has no entries for the CKA or Zhou & Hasler papers.** That file is link-checked, so entries were not added without checking the links.
