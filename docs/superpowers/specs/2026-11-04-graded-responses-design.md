# Graded responses: what a network does over a stimulus battery

**Date:** 2026-11-04
**Status:** design approved by the owner in chat 2026-11-04; spec awaiting
the owner's review.
**Epic:** 0.4.0 fidelity against recordings, task 1a —
`docs/project-outline/0.4.0-fidelity-against-recordings/` (not in git), and
its `task-1-trait-migration.md`, "1a — additive".
**Builds on:**
[`2026-10-29-flyvis-parity-validate-design.md`](2026-10-29-flyvis-parity-validate-design.md)
(`semblance_flyvis::parity`) and
[`2026-10-28-stimulus-traces-design.md`](2026-10-28-stimulus-traces-design.md)
(the input mapping and the playback the trace gates use).

## Why

Every index 0.4.0 computes (FRI, DSI, preferred direction, tuning
correlation) and its representational measure read the same thing: each cell
type's central response, frame by frame, to each stimulus of a battery. Today
nothing produces that. `parity`'s playback runs only the eight registered
reference runs, takes its model, `dt` and precision from the run's index, and
returns differences from flyvis's traces rather than the traces themselves.

This piece adds the container for those responses and the one function that
fills it for any set of parameters. It is additive: nothing is removed, and
the spiking path, `roundtrip` and every 0.3.0 gate are untouched.

## Goal

1. **`semblance_sim::response`**: a `Responses` type holding one network's
   central responses to one battery, with its shape and finiteness enforced
   at construction.
2. **`semblance_flyvis::parity::respond`**: settles any 734-parameter network
   the way the 0.3.0 gates do, plays a battery from the settled state, and
   returns `Responses`.
3. **A proof that it is right**: on flyvis's 12 reference sequences, model
   000's responses equal flyvis's reference centre traces bit for bit.

## Facts the design rests on

Each re-checked against the code at `1b43c0e` or against task 0's facts table
(`task-0-facts.md`, local).

| Fact | Source |
|---|---|
| flyvis's index pipeline initialises with `steady_state(t_pre=1.0, value=0.5)` and `t_fade_in=0`: the same pipeline as the 0.3.0 trace gates | task-0-facts.md, flashes table; `network/network.py:754,769-774` |
| Every index reads the central column only: one neuron per cell type, 65 values per frame | task-0-facts.md; `stimulus_responses.py:41-72` |
| The moving-edge battery is 144 stimuli of 1078 frames at `dt = 1/200`; the flash battery 4 samples of 600 frames | task-0-facts.md |
| `parity`'s settling is `settle_state`: `n_settle = t_pre / dt` Euler steps with `pre_stim_value` on every input node, from a `Graded<R>` built from `GradedParams` and the chosen counts | `crates/flyvis/src/parity/run.rs:294-340` |
| `play_from` resets to the settled state before each sequence, scatters each frame's 721 hexal values through `wiring::scatter_hexals` into an input buffer zeroed once, steps, and checks for non-finite state | `run.rs:390-509` |
| The centre nodes come from the reference manifest (`centre_nodes`); the network carries `node_u`/`node_v` per node | `run.rs:192-207`; `crates/connectome/src/typed.rs:200-202` |
| `ModelParams` is one model's 734 parameters (65 + 65 + 604) | `crates/flyvis/src/ensemble.rs:19-26` |
| `fidelity` depends on `sim`, not `flyvis`; `flyvis` depends on `sim` | the crates' `Cargo.toml` |

## Design

### 1. The type: `semblance_sim::response`

A new module in `sim`, beside `graded`. A type here can be produced by
`flyvis` and read by `fidelity` without a new dependency edge, and the index
functions of task 3 never see `flyvis` (epic decision 2).

```rust
/// Which integrator precision produced a set of responses. The values are
/// stored widened to f64; widening an f32 is exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision { F32, F64 }

/// One network's central responses to one battery of stimuli.
#[derive(Clone, Debug, PartialEq)]
pub struct Responses {
    cell_types: Vec<String>,
    dt: f64,
    precision: Precision,
    stimuli: Vec<StimulusResponse>,
}

/// One stimulus's responses: `n_frames` rows of one value per cell type,
/// row-major. Row `i` is the state after `i + 1` steps from the settled
/// state.
#[derive(Clone, Debug, PartialEq)]
pub struct StimulusResponse {
    label: String,
    n_frames: usize,
    values: Vec<f64>,
}
```

Construction:

```rust
impl Responses {
    pub fn new(cell_types: Vec<String>, dt: f64, precision: Precision) -> Result<Self, ResponseError>;
    pub fn push(&mut self, label: String, n_frames: usize, values: Vec<f64>) -> Result<(), ResponseError>;
    pub fn finish(self) -> Result<Self, ResponseError>; // refuses an empty battery
}
```

`ResponseError` refuses, each as its own variant:

- no cell types, or a repeated cell-type name;
- a `dt` that is not finite and positive;
- `n_frames == 0`, or `values.len() != n_frames * cell_types.len()`;
- a non-finite value (with the stimulus index, frame and cell type);
- `finish` on a battery with no stimuli.

These are the first of gate E's error paths: a malformed response set is an
error, never a score.

Reading:

```rust
impl Responses {
    pub fn cell_types(&self) -> &[String];
    pub fn type_index(&self, name: &str) -> Option<usize>;
    pub fn dt(&self) -> f64;
    pub fn precision(&self) -> Precision;
    pub fn len(&self) -> usize;                      // stimuli
    pub fn stimulus(&self, s: usize) -> Option<&StimulusResponse>;
}
impl StimulusResponse {
    pub fn label(&self) -> &str;
    pub fn n_frames(&self) -> usize;
    pub fn frame(&self, i: usize) -> Option<&[f64]>; // one value per cell type
    pub fn trace(&self, t: usize) -> impl Iterator<Item = f64> + '_; // one cell type over time
}
```

`Responses` knows nothing about stimulus metadata (angles, speeds,
intensities). Task 3's battery description pairs with it by stimulus
position, and checks the counts and labels agree. That keeps the index
functions pure over arrays (epic decision 2).

**Memory.** The 144-edge battery is 144 × 1078 × 65 × 8 bytes ≈ 81 MB per
model. Callers scoring many models hold one at a time.

### 2. The producer: `semblance_flyvis::parity::respond`

```rust
/// A battery of stimuli, handed over one at a time: the full moving-edge
/// battery's input is about 900 MB if held at once.
pub trait Battery {
    /// How many stimuli `next` will yield.
    fn len(&self) -> usize;
    /// Writes the next stimulus's frames into `out` (cleared first; n_frames
    /// rows of 721 hexal values, in `hex_coords` order) and returns its
    /// label, or `None` when the battery is done.
    fn next(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError>;
}

pub fn respond(
    f: &Fixtures,
    params: &ModelParams,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError>;

/// `respond` for an ensemble model, by id ("000" to "049").
pub fn respond_model(f: &Fixtures, id: &str, dt: f64, precision: Dtype, counts: Counts,
                     battery: &mut dyn Battery) -> Result<Responses, ParityError>;
```

It takes the parameters, not an ensemble id, so the controls of gate C and
the compressed networks of 0.5.0 go through the same function (epic decision
6: a network is a 734-vector over fixed topology). The topology is `f`'s
`TypedNetwork`.

**Settling.** Exactly `settle_state`'s: `t_pre / dt` steps with
`pre_stim_value` on every input node, at the chosen precision and counts.
`t_pre` (1.0) and `pre_stim_value` (0.5) come from the manifest, as today.
`dt` must give a whole number of settling steps (`t_pre / dt` integral to
within the existing check); otherwise `ParityError::Usage`.

**Playback.** For each stimulus: reset to the settled state, then for each
frame scatter its 721 values through `wiring::scatter_hexals`, step, check
for a non-finite state, and record the 65 central values. A frame count that
is not a whole number of 721-value rows is `ParityError::Usage`. A non-finite
state is a new variant:

```rust
ParityError::NonFiniteResponse { stimulus: usize, label: String, frame: usize, node: usize, cell_type: String }
```

displayed as `stimulus {stimulus} ({label}): node {node} ({cell_type}) is not
finite after frame {frame}`, with `is_gate_failure()` true. Existing variants
and their display strings do not change.

**The central cells.** For each cell type, in network order, the node at
`(u, v) = (0, 0)`, derived from the network. Exactly one per type, or
`ParityError::Mismatch`. When the manifest is loaded (it always is, through
`Fixtures`), its `centre_nodes` must name the same nodes in the same order,
or `Mismatch`. Deriving them means a future scoring path that does not load
the reference has nothing to change here.

**Sharing, not duplicating (epic decision 3).** `settle_state` gains explicit
parameters (the `ModelParams`, `dt`, `n_settle`) in place of reading them
from a run; its behaviour is unchanged. `play_from` and `respond` share one
private per-frame step helper (scatter, step, non-finite check). `play_from`'s
comparison logic, `SequenceResult`, `RunResult` and every gate print stay as
they are.

**A battery over flyvis's reference input.** `ReferenceBattery::new(&TraceInputs, dt_id)`
yields the manifest's sequences at that `dt`, in index order, labelled
`"{index} {family}"`. It is what the proof below plays, and the model for
task 2's battery over the exported index input.

### 3. Proof

All fixture-gated, following the existing skip / `SEMBLANCE_REQUIRE_FIXTURES`
rules.

1. **Exact reproduction.** `respond_model(f, "000", 1/200, F32, Torch,
   ReferenceBattery)` equals `centre.f32.bin` for run 0, every sequence,
   every frame, every type, bit for bit; the same at F64 against
   `centre.f64.bin` for run 1. Both through the same `==`-on-widened-values
   rule and the signed-zero refusal the trace gates use.
2. **At 1/50.** The same for runs 2 and 3.
3. **Control.** Model 049's parameters with run 0's settings miss
   `centre.f32.bin` by more than `CONTROL_MISS`, so test 1 can fail.
4. **Central nodes.** The derived nodes equal the manifest's `centre_nodes`.
5. **Unit tests, no fixtures.** Each `ResponseError` variant; `trace` and
   `frame` against a hand-built `Responses`; a `Battery` that yields a ragged
   frame count gives `Usage`; the new error's display string.
6. **No regressions.** `cargo test --workspace` (570 plus the new tests),
   clippy with `-D warnings`, `roundtrip` at 0.000000, and `semblance validate`
   exit 0 with its output byte-identical to before.

## Out of scope

- The index functions and the battery descriptions with stimulus metadata:
  task 3.
- Batteries built from Rust's own stimulus generators: task 3 (for the
  generator's measured effect) or task 5.
- Task 2's exported index input and its battery.
- Retiring the spiking path: task 1b.
- Parallelism across stimuli or models. `respond` is sequential; a caller may
  run models in parallel. Measuring the cost is task 3's.

## Versioning

Additive to `semblance-sim` (new module) and `semblance-flyvis` (new
functions, trait, struct). The new `ParityError` variant is a breaking change
to an exhaustive public enum, acceptable inside 0.4.0's minor bump; no
version changes in this piece (the epic bumps once, when finished).
