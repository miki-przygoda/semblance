# The input mapping and the stimulus-trace gate

**Date:** 2026-10-28
**Status:** self-approved under the owner's delegation of 2026-10-28 (see
[Rulings](#rulings)), after one adversarial review (two lenses, 2026-10-29:
3 blocking, 8 should-fix, 12 minor; all resolved below)
**Epic:** 0.3.0 graded dynamics, tasks 5 and 6 (piece 2 of 3), plus the open
minor items from PR #12 (piece 1) — `docs/project-outline/0.3.0-graded-dynamics/`
(not in git)
**Builds on:**
- [`2026-10-28-graded-settling-design.md`](2026-10-28-graded-settling-design.md):
  piece 1 — the integrator, the readers, the settling gates
- [`2026-10-26-flyvis-reference-traces-design.md`](2026-10-26-flyvis-reference-traces-design.md):
  task 1 — the reference data, decisions O1–O4, deferred question D1

**Research:** `.temper/run-0.3.0/research/` (local, four reports dated
2026-10-28). Their findings are restated below wherever this spec relies on
them.

## Why

Piece 1 proved that the Rust integrator reproduces flyvis's settling exactly.
Settling involves no stimulus, though: every photoreceptor sees a flat 0.5.
Stage 2 adds the one thing settling never exercised — how a 721-hexal frame
reaches the 45,669 cells — and then holds the whole network to flyvis's own
output under flashes and moving edges, frame by frame.

The epic's diagnostic ladder rests on this split: stage 1 passing and stage 2
failing means the input mapping is wrong, a one-file search. Exactness keeps
that literal.

## Goal

1. **The input mapping (task 5).** `semblance_flyvis::wiring::scatter_hexals`,
   which places one hexal frame onto the photoreceptor nodes. It is gated
   exactly against flyvis's lattice: every hexal lands on the node at the same
   (u, v), in every input cell type.
2. **The stimulus-trace gates (task 6).** Every reference run (8) plays all 12
   reference sequences from its own settled state:
   - the **exact** gate uses torch's counts and must equal flyvis's centre
     column at every frame and every cell at the six snapshot frames, bit for
     bit
   - the **libm** gate uses Rust's `exp`, under bounds fixed in this spec
3. **The stimulus check.** Rust's own flash and moving-edge generators, compared
   frame by frame with the input flyvis actually fed.
4. **The PR #12 minors**, fixed (section 5).

No version bump; piece 3 does that.

## Facts the design rests on

From the research reports, which cite the flyvis 1.2.0 source, the export
script and the reference data; restated here so this spec stands alone.

| # | Fact |
|---|---|
| H1 | flyvis writes stimulus frame `x[frame][h]` to node `input_index[k][h]` for each of the 8 input cell types k (R1–R8): `buffer[:, frames, input_index] += x` onto a zeroed buffer (`stimulus.py:206`). No scaling, offset, filter or rounding beyond the buffer's dtype. Every other node gets 0. |
| H2 | `input_index[k]` is the type's nodes in node order (`stimulus.py:128-130`), and the type's h-th node sits at hex_coords row h: placement and hex_coords iterate the lattice the same way (`expand.rs:58-83`), both gated against flyvis (`hex_coords_match_flyvis`, `node_ordering_matches_flyvis`). `wiring::input_nodes` already returns exactly this [8][721] structure. |
| H3 | x_t is added last inside the bracket, as in settling (`dynamics.py:207-218`). |
| H4 | Each sequence starts from the settled state. Frame i is the state after i+1 Euler steps from it, where step i+1 used input frame i (manifest `frame_semantics`). |
| H5 | `input.f64.bin` holds the input every run fed, as [n_frames, 721] per (dt, sequence), `run: null`. The f64 runs fed it as is. The f32 runs fed `float32(x)`: flashes are f32-exact; edges were rounded once, by at most 2.98e-8 (PROVENANCE `input_rounding`). Pre-stimulus frames are exactly 0.5. Post frames hold 0.5 (flashes) or repeat the last frame (edges). |
| H6 | The 12 sequences: flashes 0–3 (OFF/ON × full-field/radius 6), 600 frames at 1/200 and 150 at 1/50; moving edges 4–11 at angles 0/30/90/150, OFF then ON, speed 13, height 80, 525 and 131 frames. Stimulus windows: flashes [200,400) and [50,100); edges [200,325) and [50,81). |
| H7 | `centre.{f32,f64}.bin`: per (run, sequence), shape [n_frames, 65], the `centre_nodes` at every frame. `snapshots.{f32,f64}.bin`: shape [6, 45669] at `snapshot_frames` = trunc(linspace(0, n−1, 6)). `centre[snapshot_frames]` equals `snapshots[:, centre_nodes]` exactly. |
| H8 | flyvis's f32/f64 gaps (PROVENANCE `f32_f64_gap`, max abs, in f64), `centre` / `snapshots`: 000 1/200 1.7249018977594233e-06 / 2.6318956276583094e-06; 000 1/50 2.5357808688397654e-06 / 2.9820266052027478e-06; 049 1/200 2.600224337356849e-05 / 5.1135401793800384e-05; 049 1/50 3.4419031081256435e-05 / 1.4233448473766064e-04. They include the edges' input rounding. |
| H9 | The export's direct `forward` path equals flyvis's `stimulus_response` bit for bit (PROVENANCE `equivalence`, sequences 0 and 4). |
| H10 | Rust's generators emit f32 frames. Rust flashes equal `input.f64.bin` exactly (checked at 1/200 by task 1; at 1/50 measured during research, unasserted). Rust moving edges at 0° match within 2.9e-8. At 90° they differ by flyvis's uint8 background, 1.961e-3, and at 30°/150° by up to 3.79e-3/3.88e-3. All are inside `TOLERANCE` 0.005. Frame counts and windows match exactly at both dts. |
| H11 | The manifest's `sequences` list carries each sequence's family, flyvis index, arguments, and per-dt `n_frames`, `stimulus_frames` and `snapshot_frames`. The Rust `Manifest` does not parse it yet. |
| H13 | **The design is already measured.** Both reviewers ran it in scratch code against the fixtures. All 8 exact runs matched flyvis bit for bit in all 12 sequences, centre and snapshots, f32 and f64. The worst f64 libm error was 3.1e-13. The worst f32 libm ratio was 0.861 (run 4, sequence 3, snapshots: 8.80e-5 against 1.0227e-4); every other f32 ratio was 0.32 or less. Speed: 17 ms per step in debug, 0.9 ms at `opt-level` 3, bit-identical. |
| H12 | Every cell type's centre trace moves by at least 3.7e-3 over the 12 sequences (research; the smallest is Tm30, 3.747e-3 in run 3). Sintel's revisit trigger 1 (types left undriven) does not fire. |

## Rulings

These are decisions made under the owner's delegation of 2026-10-28 ("decide
the small and medium items, ask on research-level ones").

- **R1 — D1 decided: the gates are driven by `input.f64.bin`, not Rust's
  stimuli.**
  - The question is about the dynamics. Rust's edges differ by up to 3.9e-3
    (H10), a hundred times the largest centre gap (H8), so they would bury an
    arithmetic gate under stimulus-rendering error.
  - Rust's stimuli get their own check (section 4), against the same input,
    under the existing `TOLERANCE`. This is the split task 1's O4 was accepted
    to make possible.
  - The epic outline's task-6 sentence ("generated by 0.2.0's Rust stimuli")
    predates O4; it is updated in the outline.
- **R2 — Start from Rust's own settled state.** Settling is already exact (piece
  1), so continuing from it tests settling and stimulus end to end, as flyvis
  does. If a stage-2 exact run fails, step 1 of its diagnosis is to rerun from
  `settled.<p>.bin` (the gate's helper takes either).
- **R3 — A libm stage-2 gate, with bounds fixed here:** f64 ≤ 1e-10, and f32 ≤
  2 × H8's gap per (model, dt), for centre and snapshots separately. That is the
  same rule piece 1 adopted with the owner. The bounds are hard-coded and
  pinned bit for bit to PROVENANCE. Measured before the gate exists (H13):
  the worst f32 ratio is 0.861, in run 4, sequence 3, snapshots. The bound
  holds, with the least margin of any run, and is not changed.
- **R4 — The PR #12 minors are fixed as listed in section 5.** Item (h) is a
  wording correction to piece 1's spec, not a new assertion: the f32 libm bound
  does not resolve integer counts in 3 of 4 runs, and the exact gate is what
  guards f32.
- **R5 — The debug-build 120 s rule fires in advance.** Stage 2 runs 6,800 steps
  per 1/200 run over 1.5M edges, 13,600 for the exact and libm tests
  together. At 17 ms per step in debug, that is about 115–140 s per test; at
  `opt-level` 3 it is 0.9 ms per step, 6–8 s per test, with bit-identical
  states (H13). The workspace `Cargo.toml` gains
  `opt-level = 3` for `semblance-sim` and `semblance-flyvis` in the dev profile,
  with before/after times recorded. It does not change results: the gates are
  bit-exact, and no flag here enables fast-math or FMA.

## Design

### 1. The input mapping (task 5): `wiring::scatter_hexals`

```rust
/// Writes one stimulus frame onto the input nodes: `out[nodes[k][h]] =
/// frame[h]` for every input type k and hexal h. Nodes not in `nodes` are left
/// as they are; the input nodes are overwritten, so a buffer zeroed once stays
/// zero elsewhere. flyvis `stimulus.py:206`.
pub fn scatter_hexals<T: Copy>(nodes: &[Vec<u32>], frame: &[T], out: &mut [T]) -> Result<(), WiringError>;
```

`WiringError` gains `FrameLength { expected, found }` (a type's node list and
the frame must have the same length) and `NodeOutOfRange { node, len }`.

**Gate (`input_mapping_matches_flyviss_lattice`, fixture):**
- For each hexal h, a frame that is 1 at h and 0 elsewhere puts exactly 8 ones
  into `out`.
- Each lands on a node of a distinct input type, whose (u, v) equals row h of
  `data/flyvis-stimuli/hex_coords.csv`.

This catches transposition and reflection directly, before any dynamics run.

### 2. Reading the manifest's `sequences`

```rust
pub struct SequenceInfo {
    pub index: u32,                     // "index"
    pub family: String,                 // "family": "flashes" | "moving_edge"
    pub flyvis_index: u32,              // "flyvis_index"
    pub args: Vec<(String, f64)>,       // "args": every numeric entry, in file order
    pub frames: Vec<SequenceFrames>,    // "frames": an object keyed by dt id
}
pub struct SequenceFrames {
    pub dt: String,                     // the key: "1/200" | "1/50"
    pub n_frames: usize,
    pub stimulus_frames: (usize, usize),   // [start, end), end exclusive
    pub snapshot_frames: Vec<usize>,
}
impl SequenceInfo { pub fn arg(&self, name: &str) -> Option<f64>; pub fn frames(&self, dt: &str) -> Option<&SequenceFrames>; }
```

The field names are the manifest's own, as printed above: `{"index": 0,
"family": "flashes", "flyvis_index": 0, "args": {"baseline": 0.5,
"intensity": 0.0, "radius": -1}, "frames": {"1/200": {"n_frames": 600,
"stimulus_frames": [200, 400], "snapshot_frames": [...]}, ...}}`. Non-numeric
args (`offsets`, `post_pad_mode`) are skipped. The gate needs only `intensity`,
`radius`, `angle` and `speed`.

- A **separate** function, `reference::parse_sequences(json: &str) ->
  Result<Vec<SequenceInfo>, ReferenceError>`. `Manifest::parse` and its
  existing tests stay untouched. Folding this into `Manifest::parse` would
  break `acceptance_reference.rs` and the unit tests, whose synthetic
  manifests carry no or partial `sequences` (review B2).
- Structural checks only: every listed field is present and typed, each
  `snapshot_frames` is ascending and inside `0..n_frames`, and each
  `stimulus_frames` has start ≤ end ≤ `n_frames`.
- The content checks (12 sequences, indices 0–11, every dt id present, 6
  snapshots each) are a fixture test on the real manifest, not parser rules.

### 3. The stimulus-trace gates (task 6)

`crates/flyvis/src/trace_checks.rs`, `#[cfg(test)]`. The trace files
(`centre.*.bin`, `snapshots.*.bin`, `input.f64.bin`; about 220 MB) are **not**
added to the settling fixture. Each is loaded lazily in its own `OnceLock`,
only by the trace tests, so the settling gates neither load nor depend on
them. Before it lands, the
shared machinery of `settling_checks.rs` moves into
`crates/flyvis/src/gate_support.rs` (`#[cfg(test)]`). Behaviour is unchanged
and every settling test keeps its name **and its module path,
`settling_checks::<name>`**. `tests/acceptance_settling.rs` runs them as
`--exact settling_checks::<name>` in a shadow workspace, and greps their
panic and print strings, which therefore stay byte-identical apart from item
(b). The shared machinery is:
- the fixture
- `Precision`, `Counts` and `diff`
- the reporters
- `settle`

**One stage-2 run** takes a reference run and a count source:

1. Settle as in piece 1, which gives the state S0. In debug builds this costs
   the same as the settling gate.
2. For each sequence 0–11:
   0. Zero the input buffer once per run.
   1. Reset to S0. Add `Graded::set_state(&[R]) -> Result<(), GradedError>`,
      which checks the length and finiteness.
   2. Read `input.f64.bin`'s block (`run: None`, the run's dt, the sequence),
      shape [n_frames, 721].
   3. For each frame i: scatter `R::from_f64(x[i][h])`
      onto the input nodes, and step.
   4. Record the 65 centre values after every step, and the full state at
      `snapshot_frames`.
   5. Scan the state for non-finite values after every step, as settling does.
3. Compare against `centre.<p>.bin` and `snapshots.<p>.bin` for (run,
   sequence), asserting the shapes [n_frames, 65] and [6, 45669].

**Exact gate:** 8 tests, `trace_exact_run_<n>_<model>_<dt>_<p>`, using the torch
counts. Every centre value at every frame and every snapshot value must be
`==` flyvis's, in all 12 sequences. On failure the test prints, per sequence:
- the differ counts
- the first differing frame and its cell type
- the largest error

**libm gate:** 8 tests, `trace_libm_run_…`, using the libm counts. Pass means
each sequence's centre and snapshot maxima are within R3's bounds. For each
run the test prints:
- its worst sequence
- each maximum, with frame, node and cell type
- the ratio to the bound, in `{:.3e}`
- the bit-equal fraction

**Supporting tests:**
- `trace_bounds_match_provenance`: R3's gaps equal PROVENANCE's `centre` and
  `snapshots` entries, bit for bit.
- `snapshots_agree_with_the_centre_column`: `centre[snapshot_frames]` equals
  `snapshots[:, centre_nodes]` in the reference itself (H7). This guards the
  gate's indexing assumptions.
- `the_trace_gate_sees_a_wrong_mapping`: a negative control. Run 3 (000, 0.02,
  f64), sequence 8 (90° OFF edge, asymmetric), with each frame's hexals
  mirrored (u, v) → (−u, −v) before scattering. The snapshot maximum must
  exceed 1e-3, as the model control requires (measured: about 2.5). (Mirroring a full-field flash would change nothing, so
  the control uses an edge.)

### 4. The stimulus check: Rust's generators against flyvis's input

`stimulus_checks` in `crates/flyvis/src/trace_checks.rs`:

- **Flashes,** `Flashes::new(FlashParams::flyvis_defaults(dt))` at both dts.
  The Rust sequence is matched to the reference sequence by intensity and
  radius from the manifest arguments. `num_frames` and the stimulus window
  `[n_pre, n_pre + n_stim)` equal the manifest's, and every frame equals the
  reference input exactly, after widening Rust's f32 to f64.
- **Moving edges,** `MovingEdge::new(params)` with `flyvis_defaults(dt)`
  modified to height 80, speed 13 only and angles 0/30/90/150. Offsets stay at the default `(-10, 10)`, which is inclusive and so equals the manifest's exclusive `[-10, 11)`; the check asserts that correspondence. `TOLERANCE` in `moving_edge_checks.rs` becomes `pub(crate)` and is imported, not copied. `MovingEdge` gains `pub fn stimulus_frames(&self, sequence) -> Option<(usize, usize)>`, because its timing is private today. Rust's sequences
  are matched by angle, intensity and speed.
  - `n_frames` and the stimulus window equal the manifest's.
  - Every frame lies within `TOLERANCE` 0.005 of the reference input. This is
    the existing moving-edge bound, for the same reason: the renderers are not
    the same function. It is not a new tolerance.
  - The 0° sequences are **exact**: each Rust f32 frame equals `x as f32` bit for
    bit, which is what flyvis's f32 runs fed. The 2.9e-8 the research reported is
    the reference's own f32 rounding, not a renderer difference (review B1).
  - The test prints the per-angle maximum.

### 5. The PR #12 minors

| Item | Fix |
|---|---|
| (a) silent skips | `test_support` gains `read_fixture_bytes(path)` / `read_fixture_text(path) -> Result<_, String>`. They panic under `SEMBLANCE_REQUIRE_FIXTURES=1` exactly as the existing loaders do; otherwise they return `Err(notice)` without printing. The existing loaders become wrappers that print the notice, so their behaviour is unchanged. The gate fixtures hold `Result<Fixture, String>`, and every gate test that gets `Err` prints `skipping: {notice}`, so each skip is visible under `--nocapture`. |
| (b) f64 ratio prints `0.000` | Ratios print with `{:.3e}`. `tests/acceptance_settling.rs` SETTLING-04 asserts the old `{:.3}` text (lines 331 and 344); it is updated in the same commit. |
| (c) dated generated header | `export_syn_count.py` writes no date. The table is regenerated, and only its header changes; `git diff` shows no value change. |
| (e) subnormal f32 dt | `Graded::new` refuses (`BadDt`) any dt whose `dt as f32` is not a *normal* f32 (`f32::is_normal`). The smallest normal f32 is about 1.18e-38, and its reciprocal, about 8.5e37, is finite in both precisions, so the floor's reciprocal can no longer overflow. Unit-tested with `f32::MIN_POSITIVE as f64 / 2.0` and with `f32::MIN_POSITIVE as f64` (accepted). The existing `dt > 0` and finiteness checks stay, because `is_normal` accepts a negative dt. The existing test `refuses_a_dt_that_is_zero_or_infinite_in_f32` asserts that the subnormal `f32::from_bits(1)` is *accepted*; that assertion is inverted, which is the point of the fix (review S5). |
| (f) NaN reference ignored | `diff` maps a NaN error to `f64::INFINITY` before comparing (`let e = if e.is_nan() { f64::INFINITY } else { e };`), so the maximum becomes infinite rather than NaN. The same applies to `max_err` in `tests/acceptance_settling.rs`. A unit test feeds a NaN reference value and asserts that the maximum is infinite. |
| (g) `relu(NaN)` | `Real` gains `fn is_nan(self) -> bool` (the trait stays sealed; `x != x` and `!(x <= ZERO)` trip clippy). `relu` returns NaN for NaN input, as `torch.relu` does: `if x > ZERO { x } else if x.is_nan() { x } else { ZERO }`. Unit-tested. Settling and stimulus results do not change: no NaN arises, which the exact gates re-prove. |
| (h) integer-count wording | Piece 1's spec gets an amendment: the control resolves integer counts against the f64 bound. In f32 the exact gate is the guard. The test's doc comment says the same. No assertion changes. |
| (i) shadow workspace | `acceptance_settling.rs` removes `…/acceptance-settling/workspace` (the copied source; its sibling target dir is kept, to avoid a rebuild). `acceptance_integrator.rs` removes the copied source under `integrator_15_sealed` but not its `target`. Both use `remove_dir_all`, ignoring NotFound, before copying. |

### 6. Documentation

- **`CLAUDE.md`:**
  - the test and skip counts, re-measured as before
  - a sentence adding the stimulus-trace gates to the settling-gates
    paragraph, which become "the trace gates"
  - `opt-level` in the dev profile, and why
- **`README.md`:** the skip count, and the `flyvis` row: the stimulus-trace
  gate exists. The `0.3.0, in progress` cell becomes "the result record
  (0.3.0, in progress)".
- **The epic's local outline:**
  - tasks 5 and 6 are marked done, with results
  - task 6's text gets R1
  - the stale D7 items are corrected: decision 7's sampling text, the
    syn_strength freeze premise, and "task 4" → "task 5" in Risks

## Testing, test-first

- **`scatter_hexals`:**
  - on a hand-built 2-type network: the right nodes are written and others
    are left alone
  - a wrong frame length is an error, as is a node index past `out`
  - the fixture gate (section 1)
- **`Graded::set_state`:** a wrong length or a non-finite value is an error that
  leaves the state alone. `set_state` followed by `step` equals stepping from
  that state.
- **`Manifest::sequences`:**
  - a synthetic manifest parses
  - rejected: a missing dt entry, 5 snapshot frames, a snapshot frame past
    `n_frames`, or a window past `n_frames`
  - the real manifest yields the 12 sequences in H6
- **The minors:** one failing test first for each of (a) and (d)–(i) where
  code changes. (b) is pinned by SETTLING-04's updated assertion. (c) is pinned
  by a Rust test that the generated header of `syn_count_torch.rs` contains no
  `exported`.
- **`scatter_hexals`** validates everything before writing, so a failed call
  leaves `out` untouched.
- **The gates:** section 3. Time is measured before and after R5 and recorded.

## Failure handling

The same table as piece 1 applies. In addition:

| Situation | Behaviour |
|---|---|
| a stage-2 exact value differs | the test fails with the per-sequence report; work stops for diagnosis (R2's first step) |
| a Rust stimulus frame outside `TOLERANCE`, or a frame count mismatch | the test fails, naming the sequence, frame and hexal |
| an f32 libm miss while the exact run of the same run passes | a libm portability difference (another platform's `exp`), recorded and investigated; never a reason to change the bound (R3) |
| a large fixture absent (`snapshots.*.bin`, ~158 MB) | it skips with a notice, or fails under `SEMBLANCE_REQUIRE_FIXTURES=1` |

## Non-goals

- Dots, Sintel, other models, other speeds and heights.
- Event-anchored snapshots (O1, declined in task 1).
- Changing any fixture or `TOLERANCE`.
- The result record, `validate` and the bump (piece 3).
- 0.4.0's fidelity metrics.

## Done when

- `cargo test --workspace` passes with `SEMBLANCE_REQUIRE_FIXTURES=1`,
  including:
  - all 8 exact trace runs bit-equal, in all 12 sequences each
  - all 8 libm trace runs within R3
  - the mapping gate, the stimulus check and the supporting tests
  - all of piece 1's tests, unchanged in outcome
- clippy is clean, roundtrip reads `0.000000` twice, and `Cargo.lock` lists 8
  packages.
- The local full gate and the self-hosted CI are green, the PR is merged, and
  branches are cleaned up.
