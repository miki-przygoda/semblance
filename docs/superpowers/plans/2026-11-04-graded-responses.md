# Graded Responses Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `semblance_sim::response::Responses` and `semblance_flyvis::parity::respond`, which settles any 734-parameter network as the 0.3.0 gates do, plays a battery of stimuli, and records each cell type's central response at every frame.

**Architecture:** A pure container in `sim` (no new dependency edges). The producer lives in `flyvis::parity` and shares the 0.3.0 settling code (`settle_state`, made to take explicit parameters) and one per-frame step helper with `play_from`, whose comparison logic and prints do not change. Proof is bit-equality with flyvis's 12 reference centre traces.

**Tech Stack:** Rust 1.85+, edition 2024, zero external dependencies.

**Spec:** `docs/superpowers/specs/2026-11-04-graded-responses-design.md`

## Global Constraints

- Zero external dependencies: `Cargo.lock` lists exactly 8 packages, all `semblance*`.
- Rust 1.85+, edition 2024 (`gen` is reserved).
- Commit subjects: plain imperative, no `feat:`/`fix:` prefixes.
- Every 0.3.0 gate's name, print and panic message stays byte for byte; `semblance validate`'s output is unchanged.
- `roundtrip`'s two distances read exactly `0.000000`.
- Do not add `target-cpu`, fast-math or `mul_add`.
- **Every cargo command runs on beast as a jobq job**, never on the Mac and never bare over ssh. From a worktree under `~/work/semblance/` on beast:
  `sh ~/work/semblance/gates/run.sh -- <command>` (e.g. `-- cargo test -p semblance-sim response`), or `sh ~/work/semblance/gates/gate-quick.sh` / `gate-full.sh`. Set `JOBQ_SUBMITTER=<your session>`. At most 4 agents building at once.
- Code reaches beast by `git push ssh://beast/srv/bulk/Projects/semblance/src HEAD:refs/heads/incoming-graded-responses` from the Mac, then a worktree on beast: `git -C /srv/bulk/Projects/semblance/src worktree add ~/work/semblance/graded-responses incoming-graded-responses`. Later pushes: push the same ref, then `git -C ~/work/semblance/graded-responses merge --ff-only incoming-graded-responses`.
- Fixture-gated tests skip through `test_support::or_skip` when a file is missing and fail under `SEMBLANCE_REQUIRE_FIXTURES=1`, as every existing gate does.

## Review Focus

1. A battery whose frame data is not a whole number of 721-value rows (or is empty): expect `ParityError::Usage`, never a truncated stimulus. Pinned in Task 4.
2. A `dt` that does not divide `t_pre` into whole steps (e.g. `0.003`), or a non-positive / non-finite `dt`: expect `Usage` before anything is settled. Pinned in Task 4.
3. A battery whose `len()` disagrees with what it yields: expect `Mismatch`, not a silently shorter `Responses`. Pinned in Task 4.
4. A network whose parameters make `Graded::new` refuse (e.g. a non-finite time constant): expect `Usage` naming the network, not a panic. Pinned in Task 4.
5. `StimulusResponse::trace` with a cell-type index past the last type: expect `None`, not a stride over the wrong values. Pinned in Task 1.

---

### Task 1: `semblance_sim::response`

**Files:**
- Create: `crates/sim/src/response.rs`
- Modify: `crates/sim/src/lib.rs:12-21` (crate doc line and `pub mod response;`)

**Interfaces:**
- Produces: `Precision { F32, F64 }`, `Responses::{new, push, finish, cell_types, type_index, dt, precision, len, is_empty, stimulus}`, `StimulusResponse::{label, n_frames, frame, trace}`, `ResponseError` (all used by Task 5 and by task 3's index code).

- [ ] **Step 1: Write the module with its tests first (tests fail to compile until the types exist; write both, then build).**

Create `crates/sim/src/response.rs`:

```rust
//! One network's central responses to a battery of stimuli: each cell
//! type's central-column value at every frame of every stimulus.
//!
//! Every response index 0.4.0 computes reads this, and nothing else. It
//! knows nothing about the stimuli beyond their labels and frame counts:
//! the code that describes a battery pairs with it by position.
//!
//! Spec: docs/superpowers/specs/2026-11-04-graded-responses-design.md,
//! section 1.

use std::fmt;

/// Which integrator precision produced a set of responses. The values are
/// stored widened to f64; widening an f32 is exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    F32,
    F64,
}

/// Why a set of responses was refused. Each is an error, never a score.
#[derive(Clone, Debug, PartialEq)]
pub enum ResponseError {
    NoCellTypes,
    RepeatedCellType(String),
    BadDt(f64),
    NoFrames { stimulus: usize },
    Shape { stimulus: usize, expected: usize, found: usize },
    NonFinite { stimulus: usize, frame: usize, cell_type: String },
    EmptyBattery,
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResponseError::NoCellTypes => f.write_str("responses need at least one cell type"),
            ResponseError::RepeatedCellType(t) => write!(f, "cell type {t} is listed twice"),
            ResponseError::BadDt(dt) => write!(f, "dt {dt} is not finite and positive"),
            ResponseError::NoFrames { stimulus } => write!(f, "stimulus {stimulus} has no frames"),
            ResponseError::Shape { stimulus, expected, found } => {
                write!(f, "stimulus {stimulus}: expected {expected} values, found {found}")
            }
            ResponseError::NonFinite { stimulus, frame, cell_type } => {
                write!(f, "stimulus {stimulus}: {cell_type} is not finite at frame {frame}")
            }
            ResponseError::EmptyBattery => f.write_str("a battery with no stimuli has no responses"),
        }
    }
}

impl std::error::Error for ResponseError {}

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
    n_types: usize,
    values: Vec<f64>,
}

impl Responses {
    /// An empty set for `cell_types`, in the order every frame lists them.
    pub fn new(cell_types: Vec<String>, dt: f64, precision: Precision) -> Result<Self, ResponseError> {
        if cell_types.is_empty() {
            return Err(ResponseError::NoCellTypes);
        }
        for (i, t) in cell_types.iter().enumerate() {
            if cell_types[..i].contains(t) {
                return Err(ResponseError::RepeatedCellType(t.clone()));
            }
        }
        if !(dt.is_finite() && dt > 0.0) {
            return Err(ResponseError::BadDt(dt));
        }
        Ok(Responses { cell_types, dt, precision, stimuli: Vec::new() })
    }

    /// Appends one stimulus: `n_frames` rows of `cell_types().len()` values.
    pub fn push(&mut self, label: String, n_frames: usize, values: Vec<f64>) -> Result<(), ResponseError> {
        let stimulus = self.stimuli.len();
        if n_frames == 0 {
            return Err(ResponseError::NoFrames { stimulus });
        }
        let n_types = self.cell_types.len();
        match n_frames.checked_mul(n_types) {
            Some(expected) if expected == values.len() => {}
            expected => {
                return Err(ResponseError::Shape {
                    stimulus,
                    expected: expected.unwrap_or(usize::MAX),
                    found: values.len(),
                })
            }
        }
        if let Some(i) = values.iter().position(|v| !v.is_finite()) {
            return Err(ResponseError::NonFinite {
                stimulus,
                frame: i / n_types,
                cell_type: self.cell_types[i % n_types].clone(),
            });
        }
        self.stimuli.push(StimulusResponse { label, n_frames, n_types, values });
        Ok(())
    }

    /// The finished set; a battery with no stimuli is refused.
    pub fn finish(self) -> Result<Self, ResponseError> {
        if self.stimuli.is_empty() { Err(ResponseError::EmptyBattery) } else { Ok(self) }
    }

    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    /// The column of cell type `name`, or `None`.
    pub fn type_index(&self, name: &str) -> Option<usize> {
        self.cell_types.iter().position(|t| t == name)
    }

    pub fn dt(&self) -> f64 {
        self.dt
    }

    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// The number of stimuli.
    pub fn len(&self) -> usize {
        self.stimuli.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stimuli.is_empty()
    }

    pub fn stimulus(&self, s: usize) -> Option<&StimulusResponse> {
        self.stimuli.get(s)
    }
}

impl StimulusResponse {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn n_frames(&self) -> usize {
        self.n_frames
    }

    /// Frame `i`: one value per cell type, in `Responses::cell_types` order.
    pub fn frame(&self, i: usize) -> Option<&[f64]> {
        if i >= self.n_frames {
            return None;
        }
        Some(&self.values[i * self.n_types..(i + 1) * self.n_types])
    }

    /// Cell type `t` over every frame, or `None` if there is no column `t`.
    pub fn trace(&self, t: usize) -> Option<impl Iterator<Item = f64> + '_> {
        if t >= self.n_types {
            return None;
        }
        Some(self.values.iter().skip(t).step_by(self.n_types).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types() -> Vec<String> {
        vec!["R1".into(), "T4a".into()]
    }

    /// Two stimuli: 3 frames and 1 frame, two types.
    fn two() -> Responses {
        let mut r = Responses::new(types(), 0.005, Precision::F32).unwrap();
        r.push("a".into(), 3, vec![1.0, 10.0, 2.0, 20.0, 3.0, 30.0]).unwrap();
        r.push("b".into(), 1, vec![-1.0, -2.0]).unwrap();
        r.finish().unwrap()
    }

    #[test]
    fn frames_and_traces_read_back_what_was_pushed() {
        let r = two();
        assert_eq!(r.len(), 2);
        assert_eq!((r.dt(), r.precision()), (0.005, Precision::F32));
        assert_eq!(r.type_index("T4a"), Some(1));
        assert_eq!(r.type_index("T5a"), None);
        let a = r.stimulus(0).unwrap();
        assert_eq!((a.label(), a.n_frames()), ("a", 3));
        assert_eq!(a.frame(1), Some(&[2.0, 20.0][..]));
        assert_eq!(a.frame(3), None);
        assert_eq!(a.trace(1).unwrap().collect::<Vec<_>>(), vec![10.0, 20.0, 30.0]);
        assert_eq!(r.stimulus(1).unwrap().trace(0).unwrap().collect::<Vec<_>>(), vec![-1.0]);
        assert!(r.stimulus(2).is_none());
    }

    #[test]
    fn a_trace_past_the_last_type_is_none() {
        assert!(two().stimulus(0).unwrap().trace(2).is_none());
    }

    #[test]
    fn construction_refuses_bad_cell_types_and_dt() {
        assert_eq!(Responses::new(vec![], 0.005, Precision::F64), Err(ResponseError::NoCellTypes));
        let twice = vec!["R1".to_string(), "R1".to_string()];
        assert_eq!(Responses::new(twice, 0.005, Precision::F64), Err(ResponseError::RepeatedCellType("R1".into())));
        for dt in [0.0, -0.005, f64::NAN, f64::INFINITY] {
            assert!(matches!(Responses::new(types(), dt, Precision::F64), Err(ResponseError::BadDt(_))));
        }
    }

    #[test]
    fn push_refuses_empty_ragged_and_non_finite_stimuli() {
        let mut r = Responses::new(types(), 0.005, Precision::F32).unwrap();
        assert_eq!(r.push("x".into(), 0, vec![]), Err(ResponseError::NoFrames { stimulus: 0 }));
        assert_eq!(
            r.push("x".into(), 2, vec![1.0, 2.0, 3.0]),
            Err(ResponseError::Shape { stimulus: 0, expected: 4, found: 3 })
        );
        assert_eq!(
            r.push("x".into(), usize::MAX, vec![1.0]),
            Err(ResponseError::Shape { stimulus: 0, expected: usize::MAX, found: 1 })
        );
        assert_eq!(
            r.push("x".into(), 2, vec![1.0, 2.0, 3.0, f64::NAN]),
            Err(ResponseError::NonFinite { stimulus: 0, frame: 1, cell_type: "T4a".into() })
        );
        assert!(r.is_empty(), "a refused stimulus is not kept");
    }

    #[test]
    fn an_empty_battery_does_not_finish() {
        let r = Responses::new(types(), 0.005, Precision::F32).unwrap();
        assert_eq!(r.finish(), Err(ResponseError::EmptyBattery));
    }

    #[test]
    fn errors_display_as_registered() {
        let e = ResponseError::NonFinite { stimulus: 4, frame: 9, cell_type: "Mi1".into() };
        assert_eq!(e.to_string(), "stimulus 4: Mi1 is not finite at frame 9");
        let e = ResponseError::Shape { stimulus: 1, expected: 130, found: 129 };
        assert_eq!(e.to_string(), "stimulus 1: expected 130 values, found 129");
        assert_eq!(ResponseError::EmptyBattery.to_string(), "a battery with no stimuli has no responses");
    }
}
```

In `crates/sim/src/lib.rs`, after the `graded` bullet of the crate doc (line 12-13) add:

```rust
//! - [`response`]: one network's central responses to a battery of
//!   stimuli, the input to every 0.4.0 response index
```

and after `pub mod graded;` (line 21) add `pub mod response;`.

- [ ] **Step 2: Run the tests**

Run (beast, from the worktree): `sh ~/work/semblance/gates/run.sh -- cargo test -p semblance-sim response`
Expected: 6 passed.

- [ ] **Step 3: Clippy**

Run: `sh ~/work/semblance/gates/run.sh -- cargo clippy -p semblance-sim --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/sim/src/response.rs crates/sim/src/lib.rs
git commit -m "Add semblance_sim::response, one network's central responses"
```

---

### Task 2: Share settling and the per-frame step in `parity`

A pure refactor of `crates/flyvis/src/parity/run.rs`. Nothing it computes or prints changes; the existing gates prove it.

**Files:**
- Modify: `crates/flyvis/src/parity/run.rs:294-340` (`settle_state`), `:390-509` (`play_from`'s frame loop)
- Modify: `crates/flyvis/src/parity/mod.rs` (two new `ParityError` variants)

**Interfaces:**
- Produces (crate-private, used by Task 4):
  - `pub(super) enum Ctx { Run(u32), Respond }`
  - `pub(super) fn settle_core<R: Precision>(f: &Fixtures, p: &ModelParams, dt: f64, n_settle: usize, counts: Counts, centre: &[usize], ctx: Ctx) -> Result<(Graded<R>, Vec<f64>), ParityError>`
  - `pub(super) fn step_frame<R: Real>(f: &Fixtures, g: &mut Graded<R>, frame: &[R], input: &mut [R]) -> Result<Option<usize>, String>`
  - `ParityError::NonFiniteSettle { node: usize, cell_type: String, step: usize }` and `ParityError::NonFiniteResponse { stimulus: usize, label: String, frame: usize, node: usize, cell_type: String }`, both gate failures.

`NonFiniteSettle` is not named in the spec: the spec defines the playback error, and `respond` settles too, with no run index to put in `NonFinite`. It follows the same pattern and the same exit-1 classification.

- [ ] **Step 1: Write the failing test for the new variants** in `crates/flyvis/src/parity/mod.rs`'s `tests` module, after `errors_display_as_registered`:

```rust
    #[test]
    fn respond_errors_display_as_registered() {
        let e = ParityError::NonFiniteResponse {
            stimulus: 3,
            label: "3 moving_edge".into(),
            frame: 40,
            node: 7,
            cell_type: "T4a".into(),
        };
        assert_eq!(e.to_string(), "stimulus 3 (3 moving_edge): node 7 (T4a) is not finite after frame 40");
        assert!(e.is_gate_failure());
        let e = ParityError::NonFiniteSettle { node: 2, cell_type: "Mi4".into(), step: 11 };
        assert_eq!(e.to_string(), "settling: node 2 (Mi4) is not finite after settle step 11");
        assert!(e.is_gate_failure());
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `sh ~/work/semblance/gates/run.sh -- cargo test -p semblance-flyvis respond_errors_display`
Expected: compile error, no variant `NonFiniteResponse`.

- [ ] **Step 3: Add the variants.** In `ParityError` (after `NonFinite`):

```rust
    /// `respond`'s settling went NaN or infinite: a gate failure.
    NonFiniteSettle { node: usize, cell_type: String, step: usize },
    /// `respond`'s playback went NaN or infinite: a gate failure.
    NonFiniteResponse { stimulus: usize, label: String, frame: usize, node: usize, cell_type: String },
```

`is_gate_failure` becomes:

```rust
        matches!(
            self,
            ParityError::NonFinite { .. } | ParityError::NonFiniteSettle { .. } | ParityError::NonFiniteResponse { .. }
        )
```

and its doc comment: "Only a non-finite state is." stays true. Add to `Display` before the closing brace of the match:

```rust
            ParityError::NonFiniteSettle { node, cell_type, step } => {
                write!(f, "settling: node {node} ({cell_type}) is not finite after settle step {step}")
            }
            ParityError::NonFiniteResponse { stimulus, label, frame, node, cell_type } => {
                write!(f, "stimulus {stimulus} ({label}): node {node} ({cell_type}) is not finite after frame {frame}")
            }
```

- [ ] **Step 4: Refactor `settle_state` into `settle_core`.** In `run.rs`, add the imports `use semblance_sim::graded::GradedError;` and `use crate::ensemble::ModelParams;`, then replace `settle_state` (lines 294-340) with:

```rust
/// Who is settling, for the errors: a registered 0.3.0 run, or `respond`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Ctx {
    Run(u32),
    Respond,
}

impl Ctx {
    fn graded(self, e: GradedError) -> ParityError {
        match self {
            Ctx::Run(run) => mismatch(format!("run {run}: {e}")),
            Ctx::Respond => ParityError::Usage(format!("network: {e}")),
        }
    }

    fn settle_non_finite(self, node: usize, cell_type: String, step: usize) -> ParityError {
        match self {
            Ctx::Run(run) => ParityError::NonFinite { run, node, cell_type, after: After::SettleStep(step) },
            Ctx::Respond => ParityError::NonFiniteSettle { node, cell_type, step },
        }
    }
}

/// Builds `model`'s integrator at reference run `run`'s dt and precision and
/// settles it, as flyvis does: `n_settle` steps of grey on every
/// photoreceptor. Returns the settled integrator and the centre values
/// recorded after every step, widened to f64.
fn settle_state<R: Precision>(
    f: &Fixtures,
    run: u32,
    dt: &DtInfo,
    model: &str,
    counts: Counts,
    centre: &[usize],
) -> Result<(Graded<R>, Vec<f64>), ParityError> {
    let p = f.ensemble.model(model).ok_or_else(|| mismatch(format!("model {model} is not in the ensemble")))?;
    settle_core::<R>(f, p, dt.dt, dt.n_settle, counts, centre, Ctx::Run(run))
}

/// `settle_state` for any parameters: builds the integrator for `p` at `dt`
/// and settles it for `n_settle` steps of `pre_stim_value` on every input
/// node, recording the centre values after every step.
pub(super) fn settle_core<R: Precision>(
    f: &Fixtures,
    p: &ModelParams,
    dt: f64,
    n_settle: usize,
    counts: Counts,
    centre: &[usize],
    ctx: Ctx,
) -> Result<(Graded<R>, Vec<f64>), ParityError> {
    let count: Vec<R> = f
        .groups
        .iter()
        .enumerate()
        .map(|(e, &g)| match counts {
            Counts::Torch => R::torch_count(g as usize),
            Counts::Libm => R::libm_count(g as usize),
            Counts::IntegerNSyn => R::from_f32(f.net.edge_n_syn()[e]),
        })
        .collect();
    let params = GradedParams { bias: &p.bias, time_const: &p.time_const, syn_strength: &p.syn_strength, edge_syn_count: &count };
    let mut g = Graded::<R>::new(&f.net, &params, dt).map_err(|e| ctx.graded(e))?;

    let mut input = vec![R::ZERO; f.net.num_nodes()];
    for &i in f.input_nodes.iter().flatten() {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    let mut rust_centre = Vec::new();
    for step in 0..n_settle {
        g.step(&input).map_err(|e| ctx.graded(e))?;
        if let Some(i) = g.state().iter().position(|v| !v.is_finite()) {
            return Err(ctx.settle_non_finite(i, cell_type(&f.net, i).to_string(), step));
        }
        // settle step `step` is the state after step + 1 Euler steps
        rust_centre.extend(centre.iter().map(|&i| g.state()[i].to_f64()));
    }
    Ok((g, rust_centre))
}
```

(The body is the old one with `dt.dt` → `dt`, `dt.n_settle` → `n_settle`, and the two error sites through `ctx`; for `Ctx::Run` they produce exactly the old errors.)

- [ ] **Step 5: Extract the per-frame step.** Add after `settle_core`:

```rust
/// One playback frame: scatters `frame`'s hexal values into `input`, steps
/// `g`, and returns the first node whose state is not finite. The error is
/// the wiring's or the integrator's, for the caller to place.
pub(super) fn step_frame<R: Real>(
    f: &Fixtures,
    g: &mut Graded<R>,
    frame: &[R],
    input: &mut [R],
) -> Result<Option<usize>, String> {
    scatter_hexals(&f.input_nodes, frame, input).map_err(|e| e.to_string())?;
    g.step(input).map_err(|e| e.to_string())?;
    Ok(g.state().iter().position(|v| !v.is_finite()))
}
```

and in `play_from`, replace

```rust
            scatter_hexals(&f.input_nodes, &frame, &mut input).map_err(|e| mismatch(format!("run {run}: {e}")))?;
            g.step(&input).map_err(|e| mismatch(format!("run {run}: {e}")))?;
            if let Some(k) = g.state().iter().position(|v| !v.is_finite()) {
```

with

```rust
            if let Some(k) = step_frame(f, g, &frame, &mut input).map_err(|e| mismatch(format!("run {run}: {e}")))? {
```

(the body of the `if` is unchanged).

- [ ] **Step 6: Run the new test and the whole flyvis suite with fixtures required**

Run: `sh ~/work/semblance/gates/run.sh -- env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis`
Expected: all pass, including `respond_errors_display_as_registered` and every settling and trace gate (exact and libm), which is the proof the refactor computes what it did.

- [ ] **Step 7: `validate` unchanged**

Run on main's worktree and this one, and compare:
`cd ~/work/semblance/main && sh ~/work/semblance/gates/run.sh -- sh -c 'cargo run --release --quiet -- validate > /srv/bulk/Projects/semblance/logs/validate-main.txt'`
`cd ~/work/semblance/graded-responses && sh ~/work/semblance/gates/run.sh -- sh -c 'cargo run --release --quiet -- validate > /srv/bulk/Projects/semblance/logs/validate-graded.txt'`
`diff /srv/bulk/Projects/semblance/logs/validate-main.txt /srv/bulk/Projects/semblance/logs/validate-graded.txt`
Expected: both exit 0; `diff` prints nothing except, if present, a timing line. Any other difference is a failed refactor: stop and find it.

- [ ] **Step 8: Commit**

```bash
git add crates/flyvis/src/parity/run.rs crates/flyvis/src/parity/mod.rs
git commit -m "Let parity settle any parameters and share the playback step"
```

---

### Task 3: The central nodes, derived from the network

**Files:**
- Create: `crates/flyvis/src/parity/respond.rs` (this task adds `central_nodes`; Task 4 adds the rest)
- Modify: `crates/flyvis/src/parity/mod.rs` (`mod respond;` and its `pub use`)

**Interfaces:**
- Produces: `pub fn central_nodes(f: &Fixtures) -> Result<Vec<usize>, ParityError>`: for each cell type in network order, the node at `(u, v) = (0, 0)`; checked against the manifest's `centre_nodes`.

- [ ] **Step 1: Write the failing fixture-gated test.** Create `crates/flyvis/src/parity/respond.rs`:

```rust
//! Responses for any network: settle it as the 0.3.0 gates do, play a
//! battery from the settled state, and record each cell type's central
//! value at every frame.
//!
//! Spec: docs/superpowers/specs/2026-11-04-graded-responses-design.md,
//! section 2.

use super::run::mismatch;
use super::{Fixtures, ParityError};

/// For each cell type, in network order, its node at `(u, v) = (0, 0)`:
/// the central column flyvis's indices read. Derived from the network, so
/// scoring needs no reference traces, and checked against the manifest's
/// `centre_nodes`, which must name the same nodes in the same order.
pub fn central_nodes(f: &Fixtures) -> Result<Vec<usize>, ParityError> {
    let net = &f.net;
    let mut found: Vec<Option<usize>> = vec![None; net.cell_types().len()];
    let at_origin = net.node_u().iter().zip(net.node_v()).zip(net.node_cell_type()).enumerate();
    for (node, ((&u, &v), &t)) in at_origin {
        if u == 0 && v == 0 && found[t as usize].replace(node).is_some() {
            return Err(mismatch(format!("cell type {} has more than one node at (0, 0)", net.cell_types()[t as usize])));
        }
    }
    let nodes = found
        .iter()
        .enumerate()
        .map(|(t, n)| n.ok_or_else(|| mismatch(format!("cell type {} has no node at (0, 0)", net.cell_types()[t]))))
        .collect::<Result<Vec<usize>, ParityError>>()?;
    let manifest: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    if manifest != nodes {
        return Err(mismatch(format!(
            "the network's central nodes {nodes:?} differ from the manifest's centre_nodes {manifest:?}"
        )));
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_support::fixture;

    #[test]
    fn the_central_nodes_are_the_manifests_centre_nodes() {
        let Some(f) = fixture() else { return };
        let nodes = central_nodes(f).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(nodes.len(), f.network().cell_types().len());
        for (k, c) in f.manifest().centre_nodes.iter().enumerate() {
            assert_eq!(f.network().cell_types()[f.network().node_cell_type()[nodes[k]] as usize], c.cell_type);
        }
    }
}
```

In `run.rs`, make `mismatch` visible to the module's siblings: change `fn mismatch(m: String) -> ParityError {` (line 165) to `pub(super) fn mismatch(m: String) -> ParityError {`.

In `parity/mod.rs`, add `mod respond;` after `mod run;` and `pub use respond::central_nodes;` after the `pub use run::...` line.

- [ ] **Step 2: Run it**

Run: `sh ~/work/semblance/gates/run.sh -- env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis central_nodes`
Expected: PASS. If it fails with "differ from the manifest's centre_nodes", the manifest lists the central column in another order or set: stop and report the two lists to the owner. The spec requires them to agree; do not reorder to make it pass.

- [ ] **Step 3: Commit**

```bash
git add crates/flyvis/src/parity/respond.rs crates/flyvis/src/parity/mod.rs crates/flyvis/src/parity/run.rs
git commit -m "Derive the central nodes from the network, checked against the manifest"
```

---

### Task 4: `Battery`, `respond`, `respond_model` and `ReferenceBattery`

**Files:**
- Modify: `crates/flyvis/src/parity/respond.rs`
- Modify: `crates/flyvis/src/parity/mod.rs` (exports)

**Interfaces:**
- Consumes: `settle_core`, `step_frame`, `Ctx`, `mismatch`, `cell_type`, `file`, `Precision`, `HEXALS` from `run.rs` (Task 2); `central_nodes` (Task 3); `semblance_sim::response::{Precision, Responses}` (Task 1).
- Produces:
  - `pub trait Battery { fn len(&self) -> usize; fn is_empty(&self) -> bool; fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError>; }`
  - `pub fn respond(f: &Fixtures, params: &ModelParams, dt: f64, precision: Dtype, counts: Counts, battery: &mut dyn Battery) -> Result<Responses, ParityError>`
  - `pub fn respond_model(f: &Fixtures, id: &str, dt: f64, precision: Dtype, counts: Counts, battery: &mut dyn Battery) -> Result<Responses, ParityError>`
  - `pub struct ReferenceBattery<'a>` with `pub fn new(f: &'a Fixtures, t: &'a TraceInputs<'a>, dt_id: &str) -> Self`.

The spec names the battery method `next`; it is `next_stimulus` here so it never reads as `Iterator::next`.

- [ ] **Step 1: Write the failing tests** in `respond.rs`'s `tests` module (the fixture-gated ones load the trace files the way `trace_checks.rs` does; that helper is private there, so this module reads them itself):

```rust
    use crate::parity::{Counts, TraceInputs};
    use crate::reference::{parse_sequences, Dtype, SequenceInfo};
    use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text};
    use std::sync::OnceLock;

    /// A battery from a fixed list of frame buffers, for the error paths.
    struct Fixed {
        stimuli: Vec<Vec<f64>>,
        claimed: usize,
        next: usize,
    }

    impl Battery for Fixed {
        fn len(&self) -> usize {
            self.claimed
        }
        fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError> {
            let Some(s) = self.stimuli.get(self.next) else { return Ok(None) };
            self.next += 1;
            out.clear();
            out.extend_from_slice(s);
            Ok(Some(format!("fixed {}", self.next - 1)))
        }
    }

    fn grey(frames: usize) -> Vec<f64> {
        vec![0.5; frames * HEXALS]
    }

    #[test]
    fn a_stimulus_that_is_not_whole_frames_is_a_usage_error() {
        let Some(f) = fixture() else { return };
        for bad in [Vec::new(), vec![0.5; HEXALS + 1]] {
            let mut b = Fixed { stimuli: vec![bad], claimed: 1, next: 0 };
            let e = respond_model(f, "000", 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
            assert!(matches!(e, ParityError::Usage(ref m) if m.contains("whole number")), "{e}");
        }
    }

    #[test]
    fn a_dt_that_does_not_divide_t_pre_is_a_usage_error() {
        let Some(f) = fixture() else { return };
        for dt in [0.003, 0.0, -0.005, f64::NAN] {
            let mut b = Fixed { stimuli: vec![grey(2)], claimed: 1, next: 0 };
            let e = respond_model(f, "000", dt, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
            assert!(matches!(e, ParityError::Usage(_)), "dt {dt}: {e}");
        }
    }

    #[test]
    fn a_battery_that_yields_fewer_than_it_claims_is_a_mismatch() {
        let Some(f) = fixture() else { return };
        let mut b = Fixed { stimuli: vec![grey(2)], claimed: 2, next: 0 };
        let e = respond_model(f, "000", 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
        assert!(matches!(e, ParityError::Mismatch(ref m) if m.contains("said 2")), "{e}");
    }

    #[test]
    fn an_unknown_model_and_bad_parameters_are_usage_errors() {
        let Some(f) = fixture() else { return };
        let mut b = Fixed { stimuli: vec![grey(2)], claimed: 1, next: 0 };
        let e = respond_model(f, "050", 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.contains("050")), "{e}");
        let mut p = f.ensemble.model("000").unwrap().clone();
        p.time_const[0] = f32::NAN;
        let mut b = Fixed { stimuli: vec![grey(2)], claimed: 1, next: 0 };
        let e = respond(f, &p, 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.starts_with("network: ")), "{e}");
    }

    #[test]
    fn grey_responses_have_the_batterys_shape() {
        let Some(f) = fixture() else { return };
        let mut b = Fixed { stimuli: vec![grey(3), grey(1)], claimed: 2, next: 0 };
        let r = respond_model(f, "000", 1.0 / 200.0, Dtype::F64, Counts::Torch, &mut b).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.len(), 2);
        assert_eq!(r.cell_types().len(), f.network().cell_types().len());
        assert_eq!(r.precision(), semblance_sim::response::Precision::F64);
        assert_eq!((r.stimulus(0).unwrap().n_frames(), r.stimulus(1).unwrap().n_frames()), (3, 1));
        assert_eq!(r.stimulus(1).unwrap().label(), "fixed 1");
    }

    /// The reference's sequences, input and centre traces for one precision.
    fn reference(suffix: &str) -> Option<(Vec<SequenceInfo>, &'static [u8], Vec<u8>)> {
        static INPUT: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();
        let input = or_skip(
            INPUT
                .get_or_init(|| read_fixture_bytes("flyvis-reference/input.f64.bin"))
                .as_ref()
                .map(Vec::as_slice)
                .map_err(String::clone),
        )?;
        let text = or_skip(read_fixture_text("flyvis-reference/manifest.json"))?;
        let sequences = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let centre = or_skip(read_fixture_bytes(&format!("flyvis-reference/centre.{suffix}.bin")))?;
        Some((sequences, input, centre))
    }

    /// `respond` reproduces run `run`'s reference centre traces: model
    /// `model` at the run's dt and precision, torch counts, from the
    /// Rust-settled state, every sequence, frame and type. Returns how many
    /// values differ and the largest difference.
    fn reproduces<R: crate::parity::Precision>(run: u32, model: &str) -> Option<(usize, f64)> {
        let f = fixture()?;
        let (sequences, input, centre) = reference(R::SUFFIX)?;
        let info = &f.manifest.runs[run as usize];
        assert_eq!(info.precision, R::DTYPE, "run {run}'s registered precision");
        let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("the run's dt is listed").dt;
        let t = TraceInputs { precision: R::DTYPE, sequences: &sequences, input, centre: &centre, snapshots: &[] };
        let mut battery = ReferenceBattery::new(f, &t, &info.dt);
        let r = respond_model(f, model, dt, R::DTYPE, Counts::Torch, &mut battery).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.len(), sequences.len());
        let n_types = r.cell_types().len();
        let centre_file = file(f, &format!("centre.{}.bin", R::SUFFIX)).unwrap();
        let (mut differ, mut max) = (0usize, 0.0f64);
        for (s, q) in sequences.iter().enumerate() {
            let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(q.index) };
            let flyvis = R::read(centre_file, &centre, &key).unwrap();
            let got = r.stimulus(s).unwrap();
            assert_eq!(flyvis.shape, [got.n_frames(), n_types], "sequence {}", q.index);
            for (i, want) in flyvis.data.iter().enumerate() {
                let want = want.to_f64();
                assert_ne!(want, 0.0, "a ±0.0 reference value makes == more than bit equality");
                let d = (got.frame(i / n_types).unwrap()[i % n_types] - want).abs();
                if d != 0.0 {
                    differ += 1;
                    max = max.max(d);
                }
            }
        }
        eprintln!(
            "respond run {run} ({model}, dt {}, {}): {differ} values differ from flyvis's centre traces, largest {max:.3e}",
            info.dt,
            R::SUFFIX
        );
        Some((differ, max))
    }

    #[test]
    fn respond_reproduces_flyviss_centre_traces_at_1_200() {
        for (differ, _) in [reproduces::<f32>(0, "000"), reproduces::<f64>(1, "000")].into_iter().flatten() {
            assert_eq!(differ, 0);
        }
    }

    #[test]
    fn respond_reproduces_flyviss_centre_traces_at_1_50() {
        for (differ, _) in [reproduces::<f32>(2, "000"), reproduces::<f64>(3, "000")].into_iter().flatten() {
            assert_eq!(differ, 0);
        }
    }

    /// The control: model 049 played with run 0's settings misses run 0's
    /// traces by more than `CONTROL_MISS`, so the reproduction above can fail.
    #[test]
    fn another_models_responses_do_not_reproduce_run_0() {
        if let Some((_, max)) = reproduces::<f32>(0, "049") {
            assert!(max > crate::parity::CONTROL_MISS, "model 049 came within {max:e} of model 000's traces");
        }
    }
```

Also add `use crate::reference::BlockKey;` and `use super::run::{file, HEXALS};` to the tests' imports.

- [ ] **Step 2: Run them to see them fail**

Run: `sh ~/work/semblance/gates/run.sh -- cargo test -p semblance-flyvis respond`
Expected: compile errors (no `Battery`, `respond`, `respond_model`, `ReferenceBattery`).

- [ ] **Step 3: Implement.** Add to `respond.rs` above the tests (and extend its `use` lines):

```rust
use semblance_sim::response::{Precision as Prec, Responses};

use super::run::{cell_type, file, settle_core, step_frame, Ctx, Precision, HEXALS};
use super::{Counts, TraceInputs};
use crate::ensemble::ModelParams;
use crate::reference::{BlockKey, Dtype};

/// A battery of stimuli, handed over one at a time: the full moving-edge
/// battery's input is about 900 MB if held at once.
pub trait Battery {
    /// How many stimuli `next_stimulus` will yield.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Writes the next stimulus's frames into `out` (cleared first; rows of
    /// 721 hexal values in `hex_coords` order) and returns its label, or
    /// `None` when the battery is done.
    fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError>;
}

/// Settles `params` on the fixtures' network at `dt` and `precision`, as the
/// 0.3.0 gates settle, then plays every stimulus of `battery` from the
/// settled state and records each cell type's central value at every frame.
pub fn respond(
    f: &Fixtures,
    params: &ModelParams,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    match precision {
        Dtype::F32 => respond_as::<f32>(f, params, dt, counts, battery),
        Dtype::F64 => respond_as::<f64>(f, params, dt, counts, battery),
    }
}

/// `respond` for an ensemble model, by id (`"000"` to `"049"`).
pub fn respond_model(
    f: &Fixtures,
    id: &str,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    let p = f.ensemble.model(id).ok_or_else(|| ParityError::Usage(format!("model {id} is not in the ensemble")))?;
    respond(f, p, dt, precision, counts, battery)
}

fn respond_as<R: Precision>(
    f: &Fixtures,
    params: &ModelParams,
    dt: f64,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    let t_pre = f.manifest.t_pre;
    let steps = t_pre / dt;
    if !(dt.is_finite() && dt > 0.0 && steps.is_finite() && steps >= 1.0 && (steps - steps.round()).abs() <= 1e-9) {
        return Err(ParityError::Usage(format!("dt {dt} does not divide t_pre {t_pre} into whole settling steps")));
    }
    let centre = central_nodes(f)?;
    let (mut g, _) = settle_core::<R>(f, params, dt, steps.round() as usize, counts, &centre, Ctx::Respond)?;
    let settled = g.state().to_vec();

    let types: Vec<String> = centre.iter().map(|&k| cell_type(&f.net, k).to_string()).collect();
    let tag = match R::DTYPE {
        Dtype::F32 => Prec::F32,
        Dtype::F64 => Prec::F64,
    };
    let mut out = Responses::new(types, dt, tag).map_err(|e| mismatch(e.to_string()))?;

    // Zeroed once, as `play_from`: every frame overwrites the same input nodes.
    let mut input = vec![R::ZERO; f.net.num_nodes()];
    let mut frame = vec![R::ZERO; HEXALS];
    let mut frames = Vec::new();
    let mut s = 0usize;
    while let Some(label) = battery.next_stimulus(&mut frames)? {
        if frames.is_empty() || frames.len() % HEXALS != 0 {
            return Err(ParityError::Usage(format!(
                "stimulus {s} ({label}): {} values is not a whole number of {HEXALS}-hexal frames",
                frames.len()
            )));
        }
        g.set_state(&settled).map_err(|e| mismatch(e.to_string()))?;
        let n_frames = frames.len() / HEXALS;
        let mut values = Vec::with_capacity(n_frames * centre.len());
        for (i, row) in frames.chunks_exact(HEXALS).enumerate() {
            for (slot, &v) in frame.iter_mut().zip(row) {
                *slot = R::from_f64(v);
            }
            if let Some(k) = step_frame(f, &mut g, &frame, &mut input).map_err(mismatch)? {
                let cell_type = cell_type(&f.net, k).to_string();
                return Err(ParityError::NonFiniteResponse { stimulus: s, label, frame: i, node: k, cell_type });
            }
            values.extend(centre.iter().map(|&k| g.state()[k].to_f64()));
        }
        out.push(label, n_frames, values).map_err(|e| mismatch(e.to_string()))?;
        s += 1;
    }
    if s != battery.len() {
        return Err(mismatch(format!("the battery said {} stimuli and yielded {s}", battery.len())));
    }
    out.finish().map_err(|e| ParityError::Usage(e.to_string()))
}

/// flyvis's reference input as a battery: the manifest's sequences at one
/// dt, in index order, labelled `"{index} {family}"`.
pub struct ReferenceBattery<'a> {
    f: &'a Fixtures,
    t: &'a TraceInputs<'a>,
    dt: String,
    next: usize,
}

impl<'a> ReferenceBattery<'a> {
    pub fn new(f: &'a Fixtures, t: &'a TraceInputs<'a>, dt_id: &str) -> Self {
        ReferenceBattery { f, t, dt: dt_id.to_string(), next: 0 }
    }
}

impl Battery for ReferenceBattery<'_> {
    fn len(&self) -> usize {
        self.t.sequences.len()
    }

    fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError> {
        let Some(q) = self.t.sequences.get(self.next) else { return Ok(None) };
        self.next += 1;
        let key = BlockKey { run: None, dt: &self.dt, sequence: Some(q.index) };
        let x = file(self.f, "input.f64.bin")?.read_f64(self.t.input, &key).map_err(|e| ParityError::Malformed {
            rel: "flyvis-reference/input.f64.bin".into(),
            detail: e.to_string(),
        })?;
        out.clear();
        out.extend_from_slice(&x.data);
        Ok(Some(format!("{} {}", q.index, q.family)))
    }
}
```

In `run.rs`, make `cell_type`, `file`, `HEXALS` and `Precision` `pub(super)`-reachable: they are already `pub(crate)`, so nothing changes there. `settle_core`, `step_frame` and `Ctx` are `pub(super)` from Task 2.

In `parity/mod.rs`, extend the export: `pub use respond::{central_nodes, respond, respond_model, Battery, ReferenceBattery};`

Note on the `Ctx::Respond` arm of `step_frame`'s errors: `step_frame` returns the wiring's or integrator's message, and `respond` maps it through `mismatch` (a frame of the wrong length cannot reach it: the check above guarantees 721-value rows).

- [ ] **Step 4: Run the tests**

Run: `sh ~/work/semblance/gates/run.sh -- env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis respond -- --nocapture`
Expected: all pass; the reproduction lines print `0 values differ` for runs 0–3; the control prints a non-zero count with a max above `1e-3`.

- [ ] **Step 5: Clippy, full gate**

Run: `sh ~/work/semblance/gates/run.sh -- cargo clippy --workspace --all-targets -- -D warnings`, then `sh ~/work/semblance/gates/gate-full.sh`.
Expected: clean; gate-full passes (roundtrip at `0.000000`, MSRV 1.85 build, 8-package `Cargo.lock`).

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/parity/respond.rs crates/flyvis/src/parity/mod.rs
git commit -m "Add parity::respond: any network's central responses to a battery"
```

---

### Task 5: Counts and docs

**Files:**
- Modify: `CLAUDE.md` (test and skip counts, "Verifying a change"), `README.md:68` (skip count)

- [ ] **Step 1: Count.** Tests: from gate-full's log, the `passed` total. Skips: run the suite with `data/` absent (the worktree's `data` is a symlink; removing a symlink removes only the link):

`sh ~/work/semblance/gates/run.sh -- sh -c 'rm data && cargo test --workspace -- --nocapture --test-threads=1 2>&1 | grep -c "^skipping: "; ln -s /srv/bulk/Projects/semblance/data data'`

and the skipped-test count from the same run with `SEMBLANCE_REQUIRE_FIXTURES=1` (every skip becomes a failure: the `failed` total is the number of fixture-gated tests):

`sh ~/work/semblance/gates/run.sh -- sh -c 'rm data && SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --no-fail-fast 2>&1 | grep "test result"; ln -s /srv/bulk/Projects/semblance/data data'`

Expected, to be confirmed by the measurement (use the measured numbers): 570 + 6 sim tests + 10 flyvis tests (1 display, 1 central nodes, 5 error and shape, 3 reproduction and control) = 586 tests; 174 + 9 fixture-gated = 183 skips.

- [ ] **Step 2: Update the numbers** in `CLAUDE.md` ("570 tests", "174 of those tests skip", "168 of the 174 print a notice, 180 notices in all", "every one of the 174 fails") and `README.md:68`, with the measured values. Add to `CLAUDE.md`'s "Two things about the current code" nothing; add one sentence after the settling/trace gates paragraph:

```markdown
`semblance_flyvis::parity::respond` settles any 734-parameter network the way
those gates do and records each cell type's central response over a battery
(`semblance_sim::response`); its own test holds it to flyvis's reference
centre traces bit for bit.
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md README.md
git commit -m "Bring the test and skip counts up to date for parity::respond"
```

## Self-review notes

- Spec §1 (type, errors, accessors): Task 1. Spec §2 (`Battery`, `respond`, `respond_model`, settling, playback, central nodes, sharing, `ReferenceBattery`): Tasks 2, 3, 5. Spec §3 proofs 1–6: Task 5 (1, 2, 3, 5), Task 3 (4), Tasks 2 and 5 (6).
- Deviations from the spec, each named where it occurs: `next` → `next_stimulus`; `trace` returns `Option`; `ReferenceBattery::new` takes `&Fixtures` (it needs the manifest's file table); the added `NonFiniteSettle` variant.
