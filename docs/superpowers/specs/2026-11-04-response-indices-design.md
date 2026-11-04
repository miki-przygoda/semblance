# The response indices, validated: gates A, B and E

**Date:** 2026-11-04
**Status:** design approved by the owner in chat 2026-11-04; spec awaiting
the owner's review.
**Epic:** 0.4.0 fidelity against recordings, task 3 —
`docs/project-outline/0.4.0-fidelity-against-recordings/` (not in git):
gates A, B and E as rewritten 2026-11-04, owner's answers Q1, Q2, and
decisions 1, 2, 3 and 7.
**Builds on:**
[`2026-11-04-graded-responses-design.md`](2026-11-04-graded-responses-design.md)
(task 1a: `Responses`, `respond`, `Battery`) and
[`2026-11-04-flyvis-index-export-design.md`](2026-11-04-flyvis-index-export-design.md)
(task 2: `data/flyvis-indices/`, `IndexFixture`, `IndexBattery`).
**Facts:** `task-0-facts.md` (local), cited below as "task 0".

## Why

0.4.0's behavioural axis is four indices flyvis was judged by: the flash
response index (polarity), the direction selectivity index, the preferred
direction, and the tuning-curve correlation. This piece implements them,
proves they are flyvis's (gate A), proves flyvis's model then reproduces
flyvis's published claims under them (gate B), and makes scoring refuse
malformed input instead of reporting a perfect score (gate E).

Task 0 found that flyvis's code computes the indices per model but contains
none of the rules for counting a model as agreeing with the fly. Those come
from the paper and are recorded here as the paper's, or as ours where the
paper has none (Q1).

## Goal

1. `semblance_fidelity::indices`: the four indices, their windows, and the
   agreement rules, as pure functions over task 1a's `Responses`.
2. Gate A: Rust's index values equal flyvis's, exact where exactness exists,
   otherwise within bounds fixed here.
3. Gate B: flyvis's ensemble reproduces the paper's claims under the paper's
   rules.
4. Gate E: malformed input is an error, in the new path and in the two
   legacy functions chore defects 2 and 3 name.
5. `semblance validate` runs gates A and B; `--ensemble` adds the 50-model
   runs for the record step.

## Facts the design rests on

| Fact | Source |
|---|---|
| FRI: radius-6 flashes only; window frames 200..=400 (flyvis's `-0.005 <= t` query drops 199 to float rounding); per type, `min` over the window **and both intensities**, then `+= abs(min)`; raw peaks; `(on - off) / (on + off + 1e-16)`, f32 numerator, f64 epsilon, stored f32; central column | task 0, FRI |
| Edge peak: `max(0, max over the window of raw voltage)`, frames outside the window zeroed then clipped, no baseline, no norm. Windows end at 878 / 539 / 367 / 325 / 285 / 265 for speeds 2.4 / 4.8 / 9.7 / 13 / 19 / 25 | task 0, DSI |
| `DSI = abs(Σ_θ r·e^{iθ}) / (max over intensity of Σ_θ abs(r) + 1e-15)`, the denominator shared by ON and OFF, then averaged over the 6 speeds; per intensity | task 0, DSI |
| Preferred direction = `angle(Σ over angle, speed of r·e^{iθ})`, un-normalised | task 0, DSI |
| Tuning: Pearson r over the 12 angles against Maisak 2013 (`groundtruth::TUNING_CURVES`), per type, intensity and speed, `fillna(0)`, best speed kept | task 0, tuning |
| flyvis's `*_to_known` helpers: FRI Pearson r over the 32 polarised types; DSI Pearson r over 18 types, DSI maxed over intensity, against a 0/1 label; angular distance, T4 at intensity 1, T5 at intensity 0, against 180/0/90/270° | task 0 |
| Paper: model 000 ("the task-optimal model") 30 of 32 polarities; ensemble-median FRI 32 of 32, over 50 or the top 10 (ambiguous); direction selective when DSI > `d_thresh` = 0.357 across models passes a one-sided binomial test, H0 p = 0.1, P < 0.05; 45° acceptance angle for model selection | task 0, paper headlines |
| `INSUFFICIENT_DATA` and `NOISY_DATA` filter no metric in flyvis | task 0 |
| `compare_behaviour` returns 0.0 on any shape mismatch; `behavioural_distance` returns 0.0 for an empty battery, and its guarding test asserts `== 0.0` vacuously | chore-and-fixes defects 2, 3; `crates/fidelity/src/lib.rs:47,103` |
| `fidelity` depends on `connectome`, `sim`, `codec`; `flyvis` on `connectome`, `sim`. Neither on the other | the crates' `Cargo.toml` |
| `parity::Options` holds `threads` and `runs` | `crates/flyvis/src/parity/validate.rs:18-29` |

## Design

### 1. `semblance_fidelity::indices`

Pure: no files, no simulator, no `flyvis`. Ground truth arrives as data.

**Battery description**, matched to a `Responses` by position:

```rust
pub enum Stimulus {
    Flash { radius: i32, intensity: u8 },
    Edge { angle_deg: u16, speed: f64, intensity: u8 },
}
pub struct BatteryLayout { stimuli: Vec<(String, Stimulus, Window)> }
pub struct Window { first: usize, last: usize } // inclusive frames
```

`BatteryLayout::check(&Responses)` refuses a different stimulus count, a
label mismatch, or a window past a stimulus's last frame.

**Windows.** `fri_window(dt)` and `edge_window(speed, dt, …)` reproduce
flyvis's own float expressions (time axis `frame * dt - t_pre`, the queries
as written), so frame 199's exclusion is computed, not hard-coded. Unit
tests pin the tabled windows; a fixture-gated test holds them to task 2's
manifest.

**Dtype emulation.** Each operation runs in the dtype flyvis runs it in,
taken from `Responses::precision()`, operation by operation, the way
`hex_eye.rs` emulates flyvis's fp16 mask: for an f32 run the shift, the
peaks and the FRI numerator are f32, the epsilon add and divide follow
numpy's promotion as task 0 recorded, and the stored FRI is f32. The DSI,
preferred-direction and tuning reductions run in f64 (numpy's complex128
and float64), from the peaks.

**Indices:**

```rust
pub fn fri_peaks(r: &Responses, b: &BatteryLayout) -> Result<FriPeaks, IndexError>; // [65][off,on]
pub fn fri(p: &FriPeaks) -> Result<Vec<f64>, IndexError>;                          // 65, f32-valued for f32 runs
pub fn edge_peaks(r: &Responses, b: &BatteryLayout) -> Result<EdgePeaks, IndexError>; // [144][65]
pub fn dsi(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError>;   // per intensity
pub fn preferred_direction(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError>;
pub fn tuning_r(p: &EdgePeaks, b: &BatteryLayout, known: &[(&str, [f32; 12])])
    -> Result<TuningR, IndexError>;                                                 // [8][2][6], plus best
```

Cell types are found by name through `Responses::type_index`; a missing one
is `IndexError::MissingCellType`.

**flyvis's helpers, recomputed:** `fri_correlation_to_known`,
`dsi_correlation_to_known`, `angular_distance_to_known`, each as task 0
describes, with the known values passed in.

**Agreement rules (the paper's, recorded as such):**

```rust
pub fn polarity_agreement(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Agreement; // count, of, misses
pub fn median_fri(per_model: &[Vec<f64>]) -> Result<Vec<f64>, IndexError>;
pub fn binomial_upper_tail(n: u32, k: u32, p0: f64) -> f64; // exact, summed in f64
pub fn dsi_class(per_model: &[Vec<f64>], d_thresh: f64, p0: f64, alpha: f64) -> Vec<bool>;
pub fn pd_within(pd: &[f64], known_deg: &[(&str, u16)], tolerance_deg: f64) -> Agreement;
```

Constants in one place, each with its citation: `D_THRESH = 0.357`,
`BINOMIAL_P0 = 0.1`, `BINOMIAL_ALPHA = 0.05`, `PD_TOLERANCE_DEG = 45.0`
(borrowed from the paper's model selection; the doc comment says so).

**`IndexError`**, every variant a refusal, never a score: battery/response
count mismatch, label mismatch, window past the last frame, missing cell
type, non-finite value, empty input, and the wrong number of angles or
speeds for a reduction.

### 2. Gate A: the metric is flyvis's metric

In `semblance_flyvis::parity`, which gains a dependency on
`semblance-fidelity` (no cycle: `fidelity` does not depend on `flyvis`).
For a model and precision: `respond_model` over task 2's flash and edge
`IndexBattery`, then the indices, compared with `IndexFixture`.

| Value | Rule |
|---|---|
| FRI peaks, FRI, edge peaks | `==` on every value, at f32 and f64, with the trace gates' signed-zero refusal |
| DSI, preferred direction, tuning r, the three helpers | f64: within `F64_BOUND` (1e-10). f32: within twice flyvis's own f32/f64 gap for that index, from `PROVENANCE.json` |

These bounds are fixed here, before the gate runs, by 0.3.0's rule. The
peaks are exact, so the bounded rows measure only the final reductions
(cos/sin and summation order). Preferred directions compare as angles, by
the shorter way round. Every row prints its worst difference in `{:.3e}`.

**Control:** model 049's responses scored against model 000's fixture
values miss by more than `CONTROL_MISS` on the FRI and the edge peaks.

**Where it runs:** every CI run, models 000 and 049 at both precisions
(Q2). The record step, all 50 at f32.

### 3. Gate B: flyvis's model reproduces flyvis's claims

Run in the record step (`validate --ensemble`): all 50 models at f32,
indices through Rust, rules from `indices`, ground truth from
`groundtruth.rs`.

- **Polarity:** model 000 agrees on exactly 30 of the 32 polarised types.
  The median FRI agrees on 32 of 32 over all 50 or over the top 10
  (models 000–009, the released order: task 0); the gate holds if either
  does, and the output names which.
- **DSI class:** over the 18 types, the binomial rule's classification
  equals the paper's reported classification. The plan's first step
  transcribes that classification from the paper with its citation and
  commits it as a constant; if it differs from `groundtruth::KNOWN_DSI_TYPES`'s
  labels, the difference is printed and recorded, not resolved by choosing.
- **Preferred direction:** model 000 within 45° on all 8 of T4a–d, T5a–d,
  by `angular_distance_to_known`'s pairing (T4 at intensity 1, T5 at 0).
- **Tuning:** r per type for model 000 and the ensemble median, printed, not
  gated.
- **Cross-check:** the same rules run over flyvis's own exported values for
  all 50, and must give the same counts, sets and verdicts as Rust's.

If the paper's numbers do not reproduce, the gate fails and the record says
what was found. No rule is loosened to make it pass (the epic's risk
section).

### 4. Gate E: scoring never fails toward perfect

**The legacy functions, fixed now** (task 1b deletes them later):

- `compare_behaviour(a, b) -> Result<f32, FidelityError>`: a mismatch in
  channel count, neuron count or tick count is an error.
- `behavioural_distance(…) -> Result<f32, FidelityError>`: an empty battery
  is an error.
- `size_fidelity_curve` propagates both.
- Each has a test that fails on today's code: the chore's own reproductions
  (busy recording vs empty one; empty battery vs a +5.0-shifted network).
- `probe_battery_collects_probes_for_behavioural_distance`'s vacuous
  `== 0.0` is replaced by a non-empty battery against a different network,
  asserting a positive distance, and the same network, asserting 0.
- `app`'s `roundtrip` and `evolve` handle the `Result`; `roundtrip` still
  prints `0.000000` for both distances.

**The new path:** each `IndexError` and `ResponseError` variant has a test,
including the analogues of defects 2 and 3 (a `Responses` whose shape
disagrees with the layout; an empty battery).

### 5. `semblance validate`

- New rows, in the existing table, for gate A (000 and 049, both
  precisions) and its control. Same exit codes: 0 holds, 1 fails, 2 did not
  complete (`flyvis-indices/` missing is 2, as any fixture).
- `--ensemble` adds gate A over all 50 at f32 and gate B. The record step
  runs it; plain CI does not.
- Models run in parallel under `--threads`, one `respond` per worker.
- The 0.3.0 rows and their output stay byte-identical.

**Cost.** Each model-precision run is about 158k network steps after
settling (task 0's estimate, unmeasured). This piece measures it and prints
per-model wall time. If the CI rows make the hosted job too slow, the split
goes back to the owner; nothing is dropped silently.

## Proof, beyond the gates themselves

- Unit tests for every index on small hand-built `Responses` with known
  answers, including the shared DSI denominator, the speed average, the
  best-speed tuning choice and `fillna(0)` on a flat curve.
- `binomial_upper_tail` against exact values computed by hand for small n.
- Gate A's control proves gate A can fail.
- The full suite, clippy `-D warnings`, `roundtrip` at 0.000000, and
  `validate`'s 0.3.0 rows unchanged.

## Out of scope

- The representational axis and gates C and D: task 4.
- Rust-generated batteries and the generator's measured effect on each
  index (epic decision 4): task 5, beside `score`.
- Retiring the spiking path: task 1b.
- The 0.4.0 record: task 6.

## Versioning

`semblance-fidelity`: `compare_behaviour`, `behavioural_distance` and
`size_fidelity_curve` change signature (breaking, inside 0.4.0's minor
bump). `semblance-flyvis` gains a dependency on `semblance-fidelity`. No
version number changes in this piece.
