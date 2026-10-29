# The input mapping and the stimulus-trace gate: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Place flyvis's stimulus frames onto the photoreceptors exactly as
flyvis does, and prove that the Rust integrator then reproduces flyvis's own
traces under flashes and moving edges, frame by frame, bit for bit with
flyvis's synapse counts and within fixed bounds with Rust's `exp`. Fix the
open minor items from PR #12 on the way.

**Architecture:** Two small library additions: `wiring::scatter_hexals`
(one frame onto the input nodes) and `reference::parse_sequences` (the
manifest's `sequences`). `Graded` gains `set_state`, so each sequence restarts
from the settled state, and `MovingEdge` gains `stimulus_frames`. The
settling gates' shared test machinery moves from `settling_checks.rs` into a
new `#[cfg(test)]` module, `gate_support.rs`. A new `#[cfg(test)]` module,
`trace_checks.rs`, holds 16 trace gates, their supporting tests and the
check of Rust's own stimuli against flyvis's input. The two arithmetic
crates build at `opt-level = 3` in the dev profile, so the gates run in
seconds rather than minutes.

**Tech Stack:** Rust 1.85+ (edition 2024), no external crates. One offline
Python script in `tools/` (uv; flyvis 1.2.0, torch 2.14) is rerun once.

**Spec:** `docs/superpowers/specs/2026-10-28-stimulus-traces-design.md`. The
spec is the authority; section numbers and fact ids (H1–H13, R1–R5) below
refer to it. Every piece of code in this plan was compiled and run against
the real fixtures in a scratch copy of the repo at `6dcd8d7` before the plan
was written. All 8 exact runs were bit-equal in all 12 sequences, the libm
ratios matched the spec's H13, the full suite was 373 passed and 0 failed
with `SEMBLANCE_REQUIRE_FIXTURES=1`, and 90 tests were fixture-gated.

## Execution: waves and ownership

This plan runs as parallel waves of implementers, each task in its own git
worktree of branch `stimulus-traces`. Two tasks in the same wave never touch
the same file. Each task's **Owns** line lists every file it may change; it
changes nothing else.

| Wave | Task | Owns |
|---|---|---|
| 0 | 1. Integrator minors | `crates/sim/src/graded.rs`, `crates/sim/tests/acceptance_integrator.rs` |
| 0 | 2. Undated generated header | `tools/export_syn_count.py`, `tools/README.md`, `crates/flyvis/src/syn_count_torch.rs`, `crates/flyvis/src/syn_count_torch_checks.rs` |
| 0 | 3. Moving-edge window, shared tolerance | `crates/flyvis/src/moving_edge.rs`, `crates/flyvis/src/moving_edge_checks.rs` |
| 1 | 4. Shared gate machinery, settling minors | `crates/flyvis/src/test_support.rs`, `crates/flyvis/src/gate_support.rs` (new), `crates/flyvis/src/settling_checks.rs`, `crates/flyvis/src/lib.rs`, `crates/flyvis/tests/acceptance_settling.rs`, `docs/superpowers/specs/2026-10-28-graded-settling-design.md` |
| 2 | 5. Input mapping, sequence reader | `crates/flyvis/src/wiring.rs`, `crates/flyvis/src/reference.rs` |
| 3 | 6. The stimulus-trace gates | `crates/flyvis/src/trace_checks.rs` (new), `crates/flyvis/src/lib.rs`, `Cargo.toml` |
| 4 | 7. Documentation and counts | `CLAUDE.md`, `README.md` |

`lib.rs` is owned by tasks 4 and 6 only, which are in different waves.

## Global Constraints

- **Zero external dependencies.** `Cargo.lock` lists exactly 8 packages, all
  `semblance*` (`grep -c '^name = ' Cargo.lock` prints `8`). Nothing here adds
  one, not even for tests.
- **Rust 1.85 MSRV, edition 2024.** CI also builds on 1.85, so no API newer
  than 1.85.
- **Clippy is clean:** `cargo clippy --workspace --all-targets -- -D warnings`.
- **Library code never panics on input.** Malformed input is a typed error
  (`WiringError`, `ReferenceError`, `GradedError`). `#[cfg(test)]` code may
  `expect`.
- **Never relax a comparison.** No bound, gap constant, tolerance or `==` is
  ever widened, loosened or replaced to make a test pass. The exact gates
  stay `==` on every value. The libm trace bounds are fixed by R3: **f64 ≤
  1e-10**, and **f32 ≤ 2 × flyvis's own f32/f64 gap** per (model, dt), for
  `centre` and `snapshots` separately, from PROVENANCE `f32_f64_gap` (H8):

  | model | dt | `centre` gap | `snapshots` gap |
  |---|---|---|---|
  | 000 | 1/200 | 1.7249018977594233e-06 | 2.6318956276583094e-06 |
  | 049 | 1/200 | 2.600224337356849e-05 | 5.1135401793800384e-05 |
  | 000 | 1/50 | 2.5357808688397654e-06 | 2.9820266052027478e-06 |
  | 049 | 1/50 | 3.4419031081256435e-05 | 1.4233448473766064e-04 |

  The worst measured f32 ratio is 0.861 (run 4, sequence 3, snapshots); the
  bound holds and is not changed. An f32 libm failure while the same run's
  exact gate passes is a libm portability difference, to be reported, never a
  reason to change a bound.
- **`TOLERANCE` (0.005) in `moving_edge_checks.rs` keeps its value.** Task 3
  only makes it `pub(crate)`.
- **No fixture changes.** Nothing under `data/` is written, and nothing in
  `ci/fixtures.sha256` or `ci/fixtures-tag` changes. `data/` is gitignored
  and exists only in the main checkout, `<main checkout>/data`. A worktree
  sees it through a `data` symlink at its root, which the gate scripts
  create. If it is missing, create it:
  `ln -s <main checkout>/data data`.
  The symlink is not ignored by `/data/` (that pattern only matches a
  directory), so **always `git add` explicit paths, never `git add -A` or
  `git add .`**, and never commit `data`.
- **Run the suite with the fixtures required:**
  `SEMBLANCE_REQUIRE_FIXTURES=1`. Without it a missing fixture is a silent
  pass.
- **Commit style:** plain imperative subject, no `feat:`/`fix:` prefixes.
  Each task commits on its own worktree branch; do not push, rebase or merge.
- **Stop rule.** If any gate (exact, libm, mapping, stimulus check, control)
  fails, stop and report the full printed output. Do not change a bound, a
  constant, an assertion or an `==` to get past it. The spec measured every
  one of these passing (H13).

## Review Focus

The input classes the spec implies that are most likely to bite, most likely
first. Each is pinned by a test in the task that owns the code.

1. **A fixture missing under parallel tests.** The gate fixtures are built
   once in a `OnceLock`; every test that finds one missing must print its own
   `skipping:` notice (not just the first), and fail naming the path under
   `SEMBLANCE_REQUIRE_FIXTURES=1`. **Task 4:**
   `a_missing_fixture_is_an_error_naming_its_path`, and SETTLING-11
   unchanged.
2. **A NaN reaching a comparison.** A NaN reference value must become the
   maximum error (infinite) and fail every bound, not be skipped by `>`; a
   NaN membrane potential must not be hidden by `relu`. **Task 4:**
   `a_nan_reference_value_makes_the_maximum_infinite`. **Task 1:**
   `relu_passes_nan_through_like_torch`.
3. **A malformed frame.** A frame of the wrong length, or an input node past
   the buffer, is a typed error and writes nothing. **Task 5:**
   `scatter_refuses_a_frame_of_the_wrong_length_and_writes_nothing`,
   `scatter_refuses_a_node_past_the_buffer_and_writes_nothing`.
4. **A malformed `sequences` entry.** A window or snapshot past `n_frames`,
   unordered snapshots, a missing or mistyped field are typed errors, never
   panics or out-of-range reads. **Task 5:**
   `sequences_refuse_frames_out_of_order_or_past_n_frames`,
   `sequences_refuse_a_missing_or_mistyped_field`.
5. **Restarting from a bad saved state.** `set_state` with the wrong length
   or a non-finite value is an error that leaves the state alone. **Task 1:**
   `set_state_refuses_a_wrong_length_or_a_non_finite_value_and_changes_nothing`.

## Rulings made while planning

Each resolves something the spec leaves open; none changes a spec decision.

- **"The reporters" that move to `gate_support.rs`** are `cell_type` and
  `Diff`/`diff`. The `exact`/`libm` reporting functions and `bounds` stay in
  `settling_checks.rs`: their strings are settling-specific and the
  acceptance tests match them there.
- **The gate fixture keeps the input nodes per type** (`input_nodes:
  Vec<Vec<u32>>`, flyvis's [8][721]) instead of the flattened, sorted
  `inputs`. Settling writes grey to the same set of nodes; the trace gates
  scatter onto the per-type lists.
- **`settle` is split** into `settle_state` (build, settle, return the
  integrator and the centre trace) and `settle`, which wraps it and reads the
  reference. Every assertion and message is unchanged and in the same order.
- **Spec section 2 vs the testing list.** Section 2 says the parser checks
  structure only and the content (every dt present, six snapshots) is a
  fixture test. The testing list asks for "a missing dt entry" and "5
  snapshot frames" to be rejected. Both hold: `parse_sequences` is
  structural, and a test-only `check_content` in `reference.rs` rejects those
  two, unit-tested on synthetic input and applied to the real manifest.
- **`ReferenceError` gains `FrameRange(String)`** for a window or snapshot
  out of order or past `n_frames`. `set_state` reuses `GradedError::Length`
  and `NonFinite` with `what: "state"`.
- **`relu` is written `if x > R::ZERO || x.is_nan() { x } else { R::ZERO }`**:
  the same function as the spec's `else if` form, which clippy's
  `if_same_then_else` rejects under `-D warnings`.
- **The flash window** is computed from `FlashParams` with the same
  `frame_count` that `Flashes::new` uses (`Flashes` has no window accessor,
  and the spec adds none). Every frame is then compared exactly, which pins
  the content of the window too.
- **The edge offsets' correspondence** is read from the raw manifest JSON:
  `parse_sequences` skips non-numeric args, as the spec requires, and
  `offsets` is an array.
- **R2's "the gate's helper takes either" start** is exercised by one extra
  supporting test, `a_run_from_flyviss_settled_state_is_bit_equal_too` (run
  3, sequence 8), so the `Start::Flyvis` path is tested and not dead code.
- **`snapshots_agree_with_the_centre_column` compares bits**, which is
  stricter than `==` (it also distinguishes signed zeros).
- **`stimulus_checks`** is a nested module inside `trace_checks.rs`.
- **`tools/README.md`'s provenance rule** asks every export to record its
  date. Task 2 adds the one exception (c) creates: `syn_count_torch.rs` is
  committed source, so its commit dates it.
- **The epic's local outline** (spec section 6, third bullet) is not in git
  and exists only in the main checkout. Task 7 edits it there as a final,
  uncommitted step.

---

### Task 1: Integrator minors (wave 0)

**Wave:** 0

**Owns:** `crates/sim/src/graded.rs`, `crates/sim/tests/acceptance_integrator.rs`

Spec section 5, items (e), (g) and (i) (for `acceptance_integrator.rs`), and
section 3's `Graded::set_state`.

**Files:**
- Modify: `crates/sim/src/graded.rs` (`Real`, its two impls, `GradedError`
  docs and `Display`, `relu`, `Graded::new`'s dt check, new
  `Graded::set_state`, tests)
- Modify: `crates/sim/tests/acceptance_integrator.rs`
  (`integrator_15_real_is_sealed_and_exactly_f32_and_f64`)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `semblance_sim::graded::Real::is_nan(self) -> bool` (required method; the
    trait stays sealed)
  - `semblance_sim::graded::Graded::<R>::set_state(&mut self, state: &[R]) -> Result<(), GradedError>`.
    Errors: `GradedError::Length { what: "state", expected, found }`,
    `GradedError::NonFinite { what: "state", index }`. On error the state is
    unchanged.
  - `Graded::new` returns `Err(GradedError::BadDt(dt))` for any `dt` whose
    `dt as f32` is not a positive normal f32.

- [ ] **Step 1: Write the failing tests.** In `crates/sim/src/graded.rs`, in
  `mod tests`, replace the end of `refuses_a_dt_that_is_zero_or_infinite_in_f32`
  (the existing lines below) so the subnormal f32 dt is now refused (review
  S5):

  Replace:

```rust
        // The smallest positive f32 is still a usable dt.
        let tiny = f64::from(f32::from_bits(1));
        let p = GradedParams { bias: &[0.5], time_const: &[0.0], syn_strength: &[], edge_syn_count: &[] };
        assert!(Graded::<f64>::new(&net, &p, tiny).is_ok());
    }
```

  with:

```rust
        // The smallest positive f32 is subnormal, and its reciprocal
        // overflows f32: refused in both precisions.
        let tiny = f64::from(f32::from_bits(1));
        let p = GradedParams { bias: &[0.5], time_const: &[0.0], syn_strength: &[], edge_syn_count: &[] };
        assert_eq!(Graded::<f64>::new(&net, &p, tiny).map(|_| ()), Err(GradedError::BadDt(tiny)));
        let p32 = GradedParams { bias: &[0.5], time_const: &[0.0], syn_strength: &[], edge_syn_count: &[] };
        assert_eq!(Graded::<f32>::new(&net, &p32, tiny).map(|_| ()), Err(GradedError::BadDt(tiny)));
    }

    /// Below the smallest normal f32 the floor's reciprocal overflows f32, so
    /// such a dt is refused in both precisions. The smallest normal f32 is
    /// accepted: its reciprocal, about 8.5e37, is finite.
    #[test]
    fn refuses_a_dt_that_is_subnormal_in_f32() {
        let net = network(&[0], 1, &[], &[]);
        let half = f64::from(f32::MIN_POSITIVE) / 2.0;
        let p64 = GradedParams { bias: &[0.5], time_const: &[0.0], syn_strength: &[], edge_syn_count: &[] };
        let p32 = GradedParams { bias: &[0.5], time_const: &[0.0], syn_strength: &[], edge_syn_count: &[] };
        assert_eq!(Graded::<f64>::new(&net, &p64, half).map(|_| ()), Err(GradedError::BadDt(half)));
        assert_eq!(Graded::<f32>::new(&net, &p32, half).map(|_| ()), Err(GradedError::BadDt(half)));
        let normal = f64::from(f32::MIN_POSITIVE);
        assert!(Graded::<f64>::new(&net, &p64, normal).is_ok());
        assert!(Graded::<f32>::new(&net, &p32, normal).is_ok());
    }

    #[test]
    fn is_nan_is_true_for_nan_only() {
        assert!(<f32 as Real>::is_nan(f32::NAN));
        assert!(<f64 as Real>::is_nan(f64::NAN));
        for x in [0.0f32, -0.0, 1.0, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!<f32 as Real>::is_nan(x), "{x}");
            assert!(!<f64 as Real>::is_nan(f64::from(x)), "{x}");
        }
    }

    /// `torch.relu(nan)` is nan. A NaN source must poison its targets, not
    /// vanish into a zero that hides it.
    #[test]
    fn relu_passes_nan_through_like_torch() {
        assert!(relu(f32::NAN).is_nan());
        assert!(relu(f64::NAN).is_nan());
        assert_eq!(relu(-1.5f32), 0.0);
        assert_eq!(relu(2.5f64), 2.5);
        assert_eq!(relu(f64::NEG_INFINITY), 0.0);
    }

    fn set_state_then_step<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let new = || graded::<R>(&net, &[0.5, -0.25], &[0.1, 0.1], &[1.0], &[R::ONE], 0.01);
        // Step a reference twice; capture its state after one step.
        let mut a = new();
        a.step(&[r::<R>(0.3), R::ZERO]).unwrap();
        let after_one = a.state().to_vec();
        a.step(&[R::ZERO, r::<R>(0.7)]).unwrap();
        // A fresh integrator set to that state steps to the same place.
        let mut b = new();
        b.set_state(&after_one).unwrap();
        assert_eq!(b.state(), &after_one[..]);
        b.step(&[R::ZERO, r::<R>(0.7)]).unwrap();
        assert_eq!(b.state(), a.state());
    }

    #[test]
    fn set_state_then_step_equals_stepping_from_that_state_f32() { set_state_then_step::<f32>() }
    #[test]
    fn set_state_then_step_equals_stepping_from_that_state_f64() { set_state_then_step::<f64>() }

    #[test]
    fn set_state_refuses_a_wrong_length_or_a_non_finite_value_and_changes_nothing() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let mut g = graded::<f64>(&net, &[0.5, -0.25], &[0.1, 0.1], &[1.0], &[1.0], 0.01);
        g.step(&[1.0, 0.0]).unwrap();
        let before = g.state().to_vec();
        assert_eq!(g.set_state(&[0.0]), Err(GradedError::Length { what: "state", expected: 2, found: 1 }));
        assert_eq!(g.set_state(&[0.0, 0.0, 0.0]), Err(GradedError::Length { what: "state", expected: 2, found: 3 }));
        assert_eq!(g.set_state(&[0.0, f64::NAN]), Err(GradedError::NonFinite { what: "state", index: 1 }));
        assert_eq!(g.set_state(&[f64::INFINITY, 0.0]), Err(GradedError::NonFinite { what: "state", index: 0 }));
        assert_eq!(g.state(), &before[..]);
    }
```

- [ ] **Step 2: Run them to see them fail.**

  Run: `cargo test -p semblance-sim --lib`
  Expected: does not compile: `no function or associated item named is_nan`,
  `no method named set_state`.

- [ ] **Step 3: Implement.** In `crates/sim/src/graded.rs`:

  1. In `pub trait Real`, after `fn is_finite(self) -> bool;`, add:

```rust
    fn is_nan(self) -> bool;
```

  2. In `impl Real for f32`, after its `is_finite` line, add
     `fn is_nan(self) -> bool { f32::is_nan(self) }`. In `impl Real for f64`,
     after its `is_finite` line, add
     `fn is_nan(self) -> bool { f64::is_nan(self) }`.

  3. In `pub enum GradedError`, change the `BadDt` doc comment to
     `/// \`dt\` is not finite and positive in f64, or not a positive normal f32.`
     and in `Display` change its arm to:

```rust
            GradedError::BadDt(dt) => write!(f, "dt {dt} is not finite and positive, or not a normal f32"),
```

  4. Replace `relu` with:

```rust
/// `torch.relu`: NaN passes through rather than becoming 0.
fn relu<R: Real>(x: R) -> R {
    if x > R::ZERO || x.is_nan() { x } else { R::ZERO }
}
```

  5. In `Graded::new`, replace the comment and the check:

```rust
        // The floor and the f32 step use f32(dt), so that must be finite and
        // positive too: a dt that rounds to 0 or overflows in f32 would make
        // inv_tau 1/0 or 1/inf.
        let dt32 = dt as f32;
        if !(dt.is_finite() && dt > 0.0 && dt32.is_finite() && dt32 > 0.0) {
```

  with:

```rust
        // The floor and the f32 step use f32(dt), so that must be a positive
        // normal f32: a dt that rounds to 0 or overflows in f32 would make
        // inv_tau 1/0 or 1/inf, and a subnormal one's reciprocal overflows
        // f32. `is_normal` accepts negatives, so `> 0.0` stays.
        let dt32 = dt as f32;
        if !(dt.is_finite() && dt > 0.0 && dt32.is_normal() && dt32 > 0.0) {
```

  6. After `pub fn reset(&mut self) { ... }`, add:

```rust
    /// Replaces the state, one value per node, so a run can restart from a
    /// state saved earlier (a settled state, say). A wrong length or a NaN or
    /// infinite value is an error and changes nothing.
    pub fn set_state(&mut self, state: &[R]) -> Result<(), GradedError> {
        let n = self.state.len();
        if state.len() != n {
            return Err(GradedError::Length { what: "state", expected: n, found: state.len() });
        }
        if let Some(index) = state.iter().position(|v| !v.is_finite()) {
            return Err(GradedError::NonFinite { what: "state", index });
        }
        self.state.copy_from_slice(state);
        Ok(())
    }
```

- [ ] **Step 4: Run the unit tests.**

  Run: `cargo test -p semblance-sim --lib`
  Expected: PASS, 0 failed (6 tests more than before).

- [ ] **Step 5: Item (i) and the sealed-trait check in
  `crates/sim/tests/acceptance_integrator.rs`.** In
  `integrator_15_real_is_sealed_and_exactly_f32_and_f64`, inside the `check`
  closure, replace:

```rust
        let dir = base.join(name);
        std::fs::create_dir_all(dir.join("src")).unwrap();
```

  with:

```rust
        let dir = base.join(name);
        // Start from an empty source directory, so nothing left by an earlier
        // run is compiled. `target` is a sibling and is kept.
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("removing {}: {e}", dir.display()),
        }
        std::fs::create_dir_all(dir.join("src")).unwrap();
```

  Then, in the `"impls_real"` crate body, add `is_nan` so that the seal is
  the only reason it fails to compile. Replace:

```rust
             fn is_finite(self) -> bool { self.0.is_finite() }\n\
         }\n",
```

  with:

```rust
             fn is_finite(self) -> bool { self.0.is_finite() }\n\
             fn is_nan(self) -> bool { self.0.is_nan() }\n\
         }\n",
```

- [ ] **Step 6: Run the whole crate and clippy.**

  Run: `cargo test -p semblance-sim && cargo clippy -p semblance-sim --all-targets -- -D warnings`
  Expected: every test passes (INTEGRATOR-01..15 included: INTEGRATOR-03's
  dts are all still refused, and INTEGRATOR-15 still reports the seal), and
  clippy is clean.

- [ ] **Step 7: Commit.**

```bash
git add crates/sim/src/graded.rs crates/sim/tests/acceptance_integrator.rs
git commit -m "Refuse subnormal f32 dt, pass NaN through relu, add Graded::set_state"
```

---

### Task 2: An undated generated header (wave 0)

**Wave:** 0

**Owns:** `tools/export_syn_count.py`, `tools/README.md`, `crates/flyvis/src/syn_count_torch.rs`, `crates/flyvis/src/syn_count_torch_checks.rs`

Spec section 5, item (c). Needs the worktree's `data` symlink (the exporter
reads the checkpoints under `data/flyvis-pretrained/results/`) and the main
checkout's Python environment.

**Files:**
- Modify: `tools/export_syn_count.py` (drop the date)
- Modify: `tools/README.md` (the provenance rule's one exception)
- Regenerate: `crates/flyvis/src/syn_count_torch.rs` (header line 4 only)
- Test: `crates/flyvis/src/syn_count_torch_checks.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing new. `SYN_COUNT_TORCH_F32` / `SYN_COUNT_TORCH_F64` are
  unchanged, value for value.

- [ ] **Step 1: Write the failing test.** Append to
  `crates/flyvis/src/syn_count_torch_checks.rs`:

```rust

/// The generated header records the machine and versions, not a date, so a
/// rerun on the same machine leaves the file unchanged (spec 2026-10-28
/// stimulus traces, item (c)).
#[test]
fn the_generated_header_carries_no_export_date() {
    let header: Vec<&str> =
        include_str!("syn_count_torch.rs").lines().take_while(|l| l.starts_with("//!")).collect();
    assert!(
        header.iter().any(|l| l.contains("GENERATED by `tools/export_syn_count.py`")),
        "the header is the generated one:\n{}",
        header.join("\n")
    );
    let dated: Vec<&&str> = header.iter().filter(|l| l.contains("exported")).collect();
    assert!(dated.is_empty(), "the header must not carry an export date: {dated:?}");
}
```

- [ ] **Step 2: Run it to see it fail.**

  Run: `cargo test -p semblance-flyvis --lib syn_count_torch_checks`
  Expected: FAIL in `the_generated_header_carries_no_export_date`: "the
  header must not carry an export date", listing the line ending
  `exported 2026-10-28`.

- [ ] **Step 3: Change the exporter.** In `tools/export_syn_count.py`:
  1. Delete the line `import datetime`.
  2. Replace:

```python
                f"CPU capability {torch.backends.cpu.get_cpu_capability()} · "
                f"exported {datetime.date.today().isoformat()}")
```

     with:

```python
                f"CPU capability {torch.backends.cpu.get_cpu_capability()}")
```

  3. In the module docstring, replace:

```text
pretrained.rs's SYN_COUNT. They are one machine's torch exp; the generated
file's header records which.
```

     with:

```text
pretrained.rs's SYN_COUNT. They are one machine's torch exp; the generated
file's header records which, and no date, so rerunning the script on the same
machine and versions rewrites the file byte for byte.
```

- [ ] **Step 4: Regenerate the table**, from the worktree root, with the main
  checkout's environment (it takes under a second):

```bash
test -e data/flyvis-pretrained/results || ln -s <main checkout>/data data
<main checkout>/tools/.venv/bin/python tools/export_syn_count.py
```

  Expected output:

```text
  torch.exp differs from math.exp on 108 of 2355 float64 counts
  wrote crates/flyvis/src/syn_count_torch.rs
```

- [ ] **Step 5: Verify that only the header changed.**

```bash
git diff --numstat crates/flyvis/src/syn_count_torch.rs
git diff -U0 crates/flyvis/src/syn_count_torch.rs | grep '^[-+][^-+]'
```

  Expected: `1	1	crates/flyvis/src/syn_count_torch.rs`, and exactly
  these two changed lines:

```text
-//! flyvis 1.2.0 · torch 2.14.0 · numpy 2.5.3 · arm64 · CPU capability DEFAULT · exported 2026-10-28
+//! flyvis 1.2.0 · torch 2.14.0 · numpy 2.5.3 · arm64 · CPU capability DEFAULT
```

  If any table value changed, or the versions differ, stop and report: the
  environment is not the one the table was made with. Do not commit a
  changed table.

- [ ] **Step 6: Record the exception in `tools/README.md`.** Under
  `## Provenance rule`, after the paragraph ending "An exported file whose
  origin cannot be reconstructed is not usable as evidence.", add:

```markdown
`crates/flyvis/src/syn_count_torch.rs` is the one export whose header carries
no date: it is committed source, so its commit dates it, and a dated header
would make every rerun a diff. Its header records the flyvis, torch and numpy
versions, the machine and torch's CPU capability.
```

- [ ] **Step 7: Run the checks.**

  Run: `cargo test -p semblance-flyvis --lib syn_count_torch_checks && cargo clippy -p semblance-flyvis --all-targets -- -D warnings`
  Expected: 3 passed; clippy clean.

- [ ] **Step 8: Commit.**

```bash
git add tools/export_syn_count.py tools/README.md crates/flyvis/src/syn_count_torch.rs crates/flyvis/src/syn_count_torch_checks.rs
git commit -m "Write the synapse-count table without an export date"
```

---

### Task 3: The moving-edge window and the shared tolerance (wave 0)

**Wave:** 0

**Owns:** `crates/flyvis/src/moving_edge.rs`, `crates/flyvis/src/moving_edge_checks.rs`

Spec section 4: `MovingEdge`'s timing is private, and the stimulus check
needs its window; `TOLERANCE` is imported by the check, not copied.

**Files:**
- Modify: `crates/flyvis/src/moving_edge.rs` (new method and its test)
- Modify: `crates/flyvis/src/moving_edge_checks.rs` (visibility of
  `TOLERANCE` only)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `MovingEdge::stimulus_frames(&self, sequence: usize) -> Option<(usize, usize)>`:
    `[n_pre, n_pre + n_stim)`, `None` for a sequence out of range.
  - `crate::moving_edge_checks::TOLERANCE: f64` (`pub(crate)`, value 0.005,
    unchanged).

- [ ] **Step 1: Write the failing test.** In `crates/flyvis/src/moving_edge.rs`,
  in `mod tests`, insert immediately before the doc comment
  `/// A background hexal reads exactly 0.5 here.`:

```rust
    /// The window is `[n_pre, n_pre + n_stim)`, where the frames themselves
    /// change: background before it, the held last swept frame after it.
    #[test]
    fn stimulus_frames_is_the_sweeps_window() {
        let e = edge_for(vec![0]);
        // Speed 2.4 sweeps 678 frames, speed 13 125, speed 25 65; all after 200.
        assert_eq!(e.stimulus_frames(0), Some((200, 878)));
        assert_eq!(e.stimulus_frames(3), Some((200, 325)));
        assert_eq!(e.stimulus_frames(5), Some((200, 265)));
        assert_eq!(e.stimulus_frames(12), None, "no such sequence");
        for s in 0..12 {
            let (start, end) = e.stimulus_frames(s).unwrap();
            assert_eq!(e.first_frame_selecting(s, 0), Some(start), "sequence {s}");
            assert_eq!(e.resample_index(s, end - start - 1), Some(20), "sequence {s}: the last swept frame");
            assert_eq!(e.resample_index(s, end - start), None, "sequence {s}: past the sweep");
        }
        let (mut before, mut first) = (vec![0.0f32; 721], vec![0.0f32; 721]);
        e.frame_into(3, 199, &mut before).unwrap();
        e.frame_into(3, 200, &mut first).unwrap();
        assert!(before.iter().all(|&x| x == 0.5), "frame 199 is background");
        assert!(first.iter().any(|&x| x != 0.5), "frame 200 is swept");
    }

```

- [ ] **Step 2: Run it to see it fail.**

  Run: `cargo test -p semblance-flyvis --lib stimulus_frames`
  Expected: does not compile: `no method named stimulus_frames found for
  struct MovingEdge`.

- [ ] **Step 3: Implement.** In `impl MovingEdge`, insert immediately before
  the doc comment `/// Which offset position a swept frame selects, counting
  from the first` (the method `resample_index`):

```rust
    /// The sweep's frames, `[first, end)` counting from frame zero: the
    /// background before it is `0..first`, and the held final frame follows
    /// from `end` to [`valid_frames`](Self::valid_frames). `None` for a
    /// sequence that does not exist.
    pub fn stimulus_frames(&self, sequence: usize) -> Option<(usize, usize)> {
        self.decode(sequence).map(|(_, _, s)| {
            let t = self.timing[s];
            (t.n_pre, t.n_pre + t.n_stim)
        })
    }

```

- [ ] **Step 4: Share the tolerance.** In
  `crates/flyvis/src/moving_edge_checks.rs`, change only the visibility:

```rust
const TOLERANCE: f64 = 0.005;
```

  becomes

```rust
pub(crate) const TOLERANCE: f64 = 0.005;
```

  The value and its doc comment stay exactly as they are.

- [ ] **Step 5: Run the tests and clippy.**

  Run: `cargo test -p semblance-flyvis --lib moving_edge && cargo clippy -p semblance-flyvis --all-targets -- -D warnings`
  Expected: PASS (including `stimulus_frames_is_the_sweeps_window`); clippy
  clean.

- [ ] **Step 6: Commit.**

```bash
git add crates/flyvis/src/moving_edge.rs crates/flyvis/src/moving_edge_checks.rs
git commit -m "Expose the moving edge's stimulus window and share its tolerance"
```

---

### Task 4: Shared gate machinery and the settling minors (wave 1)

**Wave:** 1

**Owns:** `crates/flyvis/src/test_support.rs`, `crates/flyvis/src/gate_support.rs` (new), `crates/flyvis/src/settling_checks.rs`, `crates/flyvis/src/lib.rs`, `crates/flyvis/tests/acceptance_settling.rs`, `docs/superpowers/specs/2026-10-28-graded-settling-design.md`

Spec section 3 (the move, before the trace gates land) and section 5, items
(a), (b), (f), (h), and (i) for `acceptance_settling.rs`.

**The constraint that governs this whole task:**
`tests/acceptance_settling.rs` runs the settling gates in a shadow copy of
the workspace as `cargo test ... -- --exact settling_checks::<name>`, and
greps their output. So every settling test keeps its name **and its module
path** (`settling_checks::<name>`), and every panic and print string stays
byte-identical, apart from item (b)'s ratio format. In particular:
- SETTLING-05 matches `"dt 1/200"`, `"settling steps at dt 1/200"`,
  `"settling_centre shape"`, `"settled shape"`;
- SETTLING-06 matches `"node {n} ({type}) is not finite after settle step {step}"`;
- SETTLING-11 matches a line starting `skipping:` containing
  `data/<rel> not found`, and, under the require flag,
  `SEMBLANCE_REQUIRE_FIXTURES=1 but` … `data/<rel> is missing`;
- SETTLING-02 and SETTLING-04 parse the exact and libm report lines.

The test_support loaders below reproduce every existing notice character for
character.

**Files:**
- Modify (rewrite): `crates/flyvis/src/test_support.rs`
- Create: `crates/flyvis/src/gate_support.rs`
- Modify: `crates/flyvis/src/settling_checks.rs` (lines 1–228 replaced; two
  later edits)
- Modify: `crates/flyvis/src/lib.rs` (register `gate_support`)
- Modify: `crates/flyvis/tests/acceptance_settling.rs` (three edits)
- Modify: `docs/superpowers/specs/2026-10-28-graded-settling-design.md`
  (amendment appended)

**Interfaces:**
- Consumes: nothing from tasks 1–3.
- Produces (all `pub(crate)`, all `#[cfg(test)]`):
  - `test_support::read_fixture_bytes(rel: &str) -> Result<Vec<u8>, String>`
    and `test_support::read_fixture_text(rel: &str) -> Result<String, String>`.
    `rel` is relative to `data/` (e.g. `"flyvis-reference/input.f64.bin"`).
    `Err` is the notice `"<path> not found. <how to get it>"`, unprinted,
    without `skipping: `; under `SEMBLANCE_REQUIRE_FIXTURES=1` they panic
    `"SEMBLANCE_REQUIRE_FIXTURES=1 but <path> is missing"`.
  - `test_support::or_skip<T>(loaded: Result<T, String>) -> Option<T>`:
    prints `skipping: <notice>` on `Err`.
  - `test_support::read_real_spec_and_network() -> Result<(ConnectomeSpec, TypedNetwork), String>`.
  - Every existing loader keeps its signature and output
    (`load_real_spec`, `load_build_fixture`, `load_build_bytes`,
    `load_stimulus_fixture`, `load_stimulus_bytes`, `load_pretrained_fixture`,
    `real_spec_and_network`, `reference_dir`, `load_reference_fixture`,
    `load_reference_bytes`).
  - `gate_support::F64_BOUND: f64 = 1e-10`
  - `gate_support::Fixture { pub(crate) net: TypedNetwork, pub(crate) groups: Vec<u32>, pub(crate) input_nodes: Vec<Vec<u32>>, pub(crate) ensemble: Ensemble, pub(crate) manifest: Manifest, /* private */ files }`
  - `gate_support::fixture() -> Option<&'static Fixture>` (prints the skip
    notice in every caller when a file is missing)
  - `gate_support::cell_type(net: &TypedNetwork, node: usize) -> &str`
  - `gate_support::Precision: Real` with `const DTYPE: Dtype`,
    `const SUFFIX: &'static str`, `fn torch_count(group: usize) -> Self`,
    `fn libm_count(group: usize) -> Self`,
    `fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>`;
    implemented for `f32` and `f64`
  - `gate_support::Counts { Torch, Libm, IntegerNSyn }` (`Clone, Copy`)
  - `gate_support::settle_state<R: Precision>(f: &Fixture, run: u32, model: &str, counts: Counts) -> (Graded<R>, Vec<f64>)`:
    the settled integrator and the centre values after every settle step
  - `gate_support::Settled { rust, flyvis, rust_centre, flyvis_centre: Vec<f64> }`
    and `gate_support::settle<R: Precision>(f: &Fixture, run: u32, model: &str, counts: Counts) -> Settled`
  - `gate_support::read<R: Precision>(f: &Fixture, name: &str, key: &BlockKey) -> Array<R>`
    (a block of `settled.*` or `settling_centre.*`)
  - `gate_support::Diff { pub(crate) max: f64, pub(crate) at: usize, pub(crate) differ: usize }`
    and `gate_support::diff(rust: &[f64], flyvis: &[f64]) -> Diff` (a NaN
    error counts as `f64::INFINITY`)

- [ ] **Step 1: Write the failing test for (a).** Append to
  `crates/flyvis/src/test_support.rs`:

```rust

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing file is an `Err` carrying its notice, unprinted and without
    /// the `skipping: ` prefix; under `SEMBLANCE_REQUIRE_FIXTURES=1` it is a
    /// panic instead.
    #[test]
    fn a_missing_fixture_is_an_error_naming_its_path() {
        let rel = "flyvis-reference/no-such-file.bin";
        match std::panic::catch_unwind(|| (read_fixture_bytes(rel), read_fixture_text(rel))) {
            Ok((bytes, text)) => {
                assert!(!fixtures_required(), "under SEMBLANCE_REQUIRE_FIXTURES=1 this must panic");
                for notice in [bytes.expect_err("the file is absent"), text.expect_err("the file is absent")] {
                    assert!(
                        notice.ends_with(
                            "data/flyvis-reference/no-such-file.bin not found. Run ci/fetch-fixtures.sh, \
                             or `uv run --project tools tools/export_reference.py`."
                        ),
                        "{notice}"
                    );
                    assert!(!notice.starts_with("skipping"), "the printer adds the prefix: {notice}");
                }
            }
            Err(_) => assert!(fixtures_required(), "only SEMBLANCE_REQUIRE_FIXTURES=1 makes this panic"),
        }
    }
}
```

  Run: `cargo test -p semblance-flyvis --lib test_support`
  Expected: does not compile: `cannot find function read_fixture_bytes`.

- [ ] **Step 2: Rewrite `crates/flyvis/src/test_support.rs`** as follows (the
  test module from step 1 is included at the end). The notices are the same
  strings as before, now in one table:

```rust
//! Test-only helpers shared across this crate's test modules.
//!
//! Kept as its own module so the download URL for the real flyvis file lives
//! in exactly one place, instead of being copy-pasted into every test module
//! that needs to skip when the real data is absent.
//!
//! Two layers. `read_fixture_bytes` / `read_fixture_text` return a missing
//! file's notice as `Err` without printing it, so a fixture built once (the
//! gates' `OnceLock`s) can hand the same notice to every test that asks.
//! The `load_*` loaders wrap them for a test that loads its own file: they
//! print `skipping: <notice>` and return `None`.

use semblance_connectome::TypedNetwork;

use crate::spec::ConnectomeSpec;

/// Whether missing fixtures must fail the suite instead of skipping it.
///
/// libtest captures `eprintln!` for *passing* tests, so a skip notice is
/// invisible under plain `cargo test`: with the data absent the suite reports
/// the same pass count as a full run. Measured, not assumed.
///
/// Set `SEMBLANCE_REQUIRE_FIXTURES=1` — as CI must — to turn every skip into a
/// failure. Without it the acceptance gates can silently verify nothing.
fn fixtures_required() -> bool {
    std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1")
}

/// The real, published flyvis connectome spec, under `data/`.
const REAL_SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";

/// `rel`'s path under the workspace's `data/`, as every notice prints it.
fn data_path(rel: &str) -> String {
    format!("{}/../../data/{rel}", env!("CARGO_MANIFEST_DIR"))
}

/// How to produce a file under `data/`, by its directory.
fn how_to_get(rel: &str) -> &'static str {
    match rel.split('/').next() {
        Some("flyvis-supplementary") => {
            "Download \
             https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
             and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/, \
             or run ci/fetch-fixtures.sh"
        }
        Some("flyvis-build") => {
            "Run `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-stimuli") => {
            "Run `uv run --project tools tools/export_stimuli.py`, \
             `uv run --project tools tools/export_moving_edges.py` and, \
             for the `_h80` files, `uv run --project tools tools/export_reference.py` \
             to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-pretrained") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_params.py`."
        }
        Some("flyvis-reference") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        }
        _ => "Run ci/fetch-fixtures.sh.",
    }
}

/// A missing fixture's notice, `<path> not found. <how to get it>`, without
/// the `skipping: ` its printer adds. Under `SEMBLANCE_REQUIRE_FIXTURES=1`
/// this panics instead, naming the path.
fn missing(path: &str, rel: &str) -> String {
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    format!("{path} not found. {}", how_to_get(rel))
}

/// The bytes of `data/<rel>`, or its missing-fixture notice. Prints nothing.
pub(crate) fn read_fixture_bytes(rel: &str) -> Result<Vec<u8>, String> {
    let path = data_path(rel);
    std::fs::read(&path).map_err(|_| missing(&path, rel))
}

/// The text of `data/<rel>`, or its missing-fixture notice. Prints nothing.
pub(crate) fn read_fixture_text(rel: &str) -> Result<String, String> {
    let path = data_path(rel);
    std::fs::read_to_string(&path).map_err(|_| missing(&path, rel))
}

/// `Some` of a loaded fixture, or `None` after printing
/// `skipping: <notice>`. Every gate test that skips goes through this, so
/// each one prints its own notice (visible under `--nocapture`).
pub(crate) fn or_skip<T>(loaded: Result<T, String>) -> Option<T> {
    match loaded {
        Ok(v) => Some(v),
        Err(notice) => {
            eprintln!("skipping: {notice}");
            None
        }
    }
}

/// Loads the real, published flyvis connectome spec as a string, or `None`
/// if it hasn't been fetched. The file is gitignored real data (see
/// `RESOURCES.md` for how to get it); tests that depend on it must skip via
/// this path rather than failing outright.
pub(crate) fn load_real_spec() -> Option<String> {
    or_skip(read_fixture_text(REAL_SPEC))
}

/// Loads a fixture exported from flyvis's own build, or `None` if the export
/// has not been run. See `tools/export_build.py`.
pub(crate) fn load_build_fixture(name: &str) -> Option<String> {
    or_skip(read_fixture_text(&format!("flyvis-build/{name}")))
}

/// Loads a binary fixture exported from flyvis's own build, or `None` if the
/// export has not been run. See `tools/export_build.py`.
pub(crate) fn load_build_bytes(name: &str) -> Option<Vec<u8>> {
    or_skip(read_fixture_bytes(&format!("flyvis-build/{name}")))
}

/// Loads a stimulus fixture exported from flyvis, or `None` if the export has
/// not been run. See `tools/export_stimuli.py`, `tools/export_moving_edges.py`
/// and, for the `_h80` files, `tools/export_reference.py`.
pub(crate) fn load_stimulus_fixture(name: &str) -> Option<String> {
    or_skip(read_fixture_text(&format!("flyvis-stimuli/{name}")))
}

/// Loads a binary stimulus fixture, or `None` if the export has not been run.
pub(crate) fn load_stimulus_bytes(name: &str) -> Option<Vec<u8>> {
    or_skip(read_fixture_bytes(&format!("flyvis-stimuli/{name}")))
}

/// Loads a file from `data/flyvis-pretrained/`, or `None` if it is absent.
/// See `tools/export_params.py`.
pub(crate) fn load_pretrained_fixture(name: &str) -> Option<String> {
    or_skip(read_fixture_text(&format!("flyvis-pretrained/{name}")))
}

/// The real spec and its expansion, or the missing spec's notice. Prints
/// nothing.
pub(crate) fn read_real_spec_and_network() -> Result<(ConnectomeSpec, TypedNetwork), String> {
    let raw = read_fixture_text(REAL_SPEC)?;
    let spec = ConnectomeSpec::from_json_str(&raw).expect("real spec must load");
    let net = crate::expand::expand(&spec, crate::expand::DEFAULT_EXTENT, crate::expand::DEFAULT_N_SYN_FILL)
        .expect("real spec must expand");
    Ok((spec, net))
}

/// The real spec and its expansion, or `None` if the spec has not been
/// fetched. Every gate that needs the real network builds it here.
pub(crate) fn real_spec_and_network() -> Option<(ConnectomeSpec, TypedNetwork)> {
    or_skip(read_real_spec_and_network())
}

/// `data/flyvis-reference/`, from `tools/export_reference.py`.
pub(crate) fn reference_dir() -> String {
    data_path("flyvis-reference")
}

/// A text file from `data/flyvis-reference/`, or `None` if it is absent.
pub(crate) fn load_reference_fixture(name: &str) -> Option<String> {
    or_skip(read_fixture_text(&format!("flyvis-reference/{name}")))
}

/// A binary file from `data/flyvis-reference/`, or `None` if it is absent.
pub(crate) fn load_reference_bytes(name: &str) -> Option<Vec<u8>> {
    or_skip(read_fixture_bytes(&format!("flyvis-reference/{name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing file is an `Err` carrying its notice, unprinted and without
    /// the `skipping: ` prefix; under `SEMBLANCE_REQUIRE_FIXTURES=1` it is a
    /// panic instead.
    #[test]
    fn a_missing_fixture_is_an_error_naming_its_path() {
        let rel = "flyvis-reference/no-such-file.bin";
        match std::panic::catch_unwind(|| (read_fixture_bytes(rel), read_fixture_text(rel))) {
            Ok((bytes, text)) => {
                assert!(!fixtures_required(), "under SEMBLANCE_REQUIRE_FIXTURES=1 this must panic");
                for notice in [bytes.expect_err("the file is absent"), text.expect_err("the file is absent")] {
                    assert!(
                        notice.ends_with(
                            "data/flyvis-reference/no-such-file.bin not found. Run ci/fetch-fixtures.sh, \
                             or `uv run --project tools tools/export_reference.py`."
                        ),
                        "{notice}"
                    );
                    assert!(!notice.starts_with("skipping"), "the printer adds the prefix: {notice}");
                }
            }
            Err(_) => assert!(fixtures_required(), "only SEMBLANCE_REQUIRE_FIXTURES=1 makes this panic"),
        }
    }
}
```

  Run: `cargo test -p semblance-flyvis --lib && SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib test_support`
  Expected: both PASS (with the flag, the new test catches the panic it
  expects; its message appears in the output, which is fine).

- [ ] **Step 3: Move the shared machinery.** Create
  `crates/flyvis/src/gate_support.rs` with the code moved from
  `settling_checks.rs`. It is the old code with these changes only: items are
  `pub(crate)`; the fixture is `Result<Fixture, String>` and loads through
  `read_fixture_*` in the same order as before (ensemble, manifest, the four
  settling files, the spec), so the first missing file is still the one
  named; `inputs` becomes `input_nodes` (per type); `settle` is split into
  `settle_state` and `settle`. `diff` is still the old one; step 5 changes it.

```rust
//! What the settling and trace gates share: the fixture, the two precisions,
//! where synapse counts come from, settling itself, and the comparison.
//!
//! Moved here from `settling_checks.rs` (spec
//! docs/superpowers/specs/2026-10-28-stimulus-traces-design.md, section 3)
//! without changing what it does. `tests/acceptance_settling.rs` matches the
//! panic and print strings below byte for byte: do not reword them.

use std::sync::OnceLock;

use semblance_connectome::TypedNetwork;
use semblance_sim::graded::{Graded, GradedParams, Real};

use crate::ensemble::Ensemble;
use crate::pretrained::SYN_COUNT;
use crate::reference::{Array, BlockKey, Dtype, FileInfo, Manifest, ReferenceError};
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text, read_real_spec_and_network};
use crate::wiring::{edge_syn_groups, input_nodes};

/// The libm gates' float64 bound, for every quantity they compare.
pub(crate) const F64_BOUND: f64 = 1e-10;

const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

pub(crate) struct Fixture {
    pub(crate) net: TypedNetwork,
    pub(crate) groups: Vec<u32>,
    /// The photoreceptor nodes per input type, in `spec.input_units` order,
    /// each in node order: flyvis's [8, 721] `input_index`.
    pub(crate) input_nodes: Vec<Vec<u32>>,
    pub(crate) ensemble: Ensemble,
    pub(crate) manifest: Manifest,
    files: Vec<(&'static str, Vec<u8>)>,
}

static FIXTURE: OnceLock<Result<Fixture, String>> = OnceLock::new();

/// Everything the gates read, built once. Every file is loaded (or found
/// missing) before anything is built from it. A missing file makes this
/// `None`, and every test that asks prints the notice.
pub(crate) fn fixture() -> Option<&'static Fixture> {
    or_skip(FIXTURE.get_or_init(load).as_ref().map_err(String::clone))
}

fn load() -> Result<Fixture, String> {
    let ensemble = read_fixture_text("flyvis-pretrained/ensemble.csv")?;
    let manifest = read_fixture_text("flyvis-reference/manifest.json")?;
    let mut files = Vec::new();
    for name in SETTLING_FILES {
        files.push((name, read_fixture_bytes(&format!("flyvis-reference/{name}"))?));
    }
    let (spec, net) = read_real_spec_and_network()?;
    let groups = edge_syn_groups(&net).expect("2,355 synapse-count groups");
    let inputs = input_nodes(&spec, &net).expect("input units are cell types");
    Ok(Fixture {
        groups,
        input_nodes: inputs,
        ensemble: Ensemble::parse(&ensemble).expect("ensemble.csv must parse"),
        manifest: Manifest::parse(&manifest).expect("manifest.json must parse"),
        files,
        net,
    })
}

pub(crate) fn cell_type(net: &TypedNetwork, node: usize) -> &str {
    &net.cell_types()[net.node_cell_type()[node] as usize]
}

/// The two precisions, each with its count tables and reference reader.
pub(crate) trait Precision: Real {
    const DTYPE: Dtype;
    const SUFFIX: &'static str;
    fn torch_count(group: usize) -> Self;
    fn libm_count(group: usize) -> Self;
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>;
}

impl Precision for f32 {
    const DTYPE: Dtype = Dtype::F32;
    const SUFFIX: &'static str = "f32";
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F32[g] }
    fn libm_count(g: usize) -> Self { SYN_COUNT[g].exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f32(bytes, key)
    }
}

impl Precision for f64 {
    const DTYPE: Dtype = Dtype::F64;
    const SUFFIX: &'static str = "f64";
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F64[g] }
    fn libm_count(g: usize) -> Self { (SYN_COUNT[g] as f64).exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f64(bytes, key)
    }
}

/// Where the per-edge synapse counts come from.
#[derive(Clone, Copy)]
pub(crate) enum Counts {
    /// flyvis's own: torch's `exp`, from `syn_count_torch.rs`.
    Torch,
    /// Rust's `exp` of `SYN_COUNT`.
    Libm,
    /// The spec's integer counts, never exponentiated: a negative control.
    IntegerNSyn,
}

/// One settling run, Rust's and flyvis's values widened to f64.
pub(crate) struct Settled {
    pub(crate) rust: Vec<f64>,
    pub(crate) flyvis: Vec<f64>,
    pub(crate) rust_centre: Vec<f64>,
    pub(crate) flyvis_centre: Vec<f64>,
}

/// Builds `model`'s integrator at reference run `run`'s dt and precision and
/// settles it, as flyvis does: `n_settle` steps of grey on every
/// photoreceptor. Returns the settled integrator and the 65 centre values
/// recorded after every step, widened to f64.
pub(crate) fn settle_state<R: Precision>(f: &Fixture, run: u32, model: &str, counts: Counts) -> (Graded<R>, Vec<f64>) {
    let info = &f.manifest.runs[run as usize];
    assert_eq!(info.index, run, "manifest runs must be listed in index order");
    assert_eq!(info.precision, R::DTYPE, "run {run}'s precision");
    let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("the run's dt is listed");
    let want_dt = match dt.id.as_str() {
        "1/200" => 1.0 / 200.0,
        "1/50" => 0.02,
        other => panic!("unexpected dt id {other}"),
    };
    assert_eq!(dt.dt, want_dt, "dt {}", dt.id);
    assert_eq!(dt.n_settle, (f.manifest.t_pre / dt.dt) as usize, "settling steps at dt {}", dt.id);

    let p = f.ensemble.model(model).expect("the model is in the ensemble");
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
    let mut g = Graded::<R>::new(&f.net, &params, dt.dt).expect("the real network and parameters are valid");

    let mut input = vec![R::ZERO; f.net.num_nodes()];
    for &i in f.input_nodes.iter().flatten() {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let mut rust_centre = Vec::with_capacity(dt.n_settle * centre.len());
    for step in 0..dt.n_settle {
        g.step(&input).expect("one input value per node");
        if let Some(i) = g.state().iter().position(|v| !v.is_finite()) {
            panic!("run {run}: node {i} ({}) is not finite after settle step {step}", cell_type(&f.net, i));
        }
        // settle step `step` is the state after step + 1 Euler steps
        rust_centre.extend(centre.iter().map(|&i| g.state()[i].to_f64()));
    }
    (g, rust_centre)
}

/// Settles `model`'s network at reference run `run`'s dt and precision, and
/// reads that run's reference.
pub(crate) fn settle<R: Precision>(f: &Fixture, run: u32, model: &str, counts: Counts) -> Settled {
    let (g, rust_centre) = settle_state::<R>(f, run, model, counts);
    let info = &f.manifest.runs[run as usize];
    let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("the run's dt is listed");
    let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
    let settled = read::<R>(f, &format!("settled.{}.bin", R::SUFFIX), &key);
    assert_eq!(settled.shape, [f.net.num_nodes()], "settled shape");
    let flyvis_centre = read::<R>(f, &format!("settling_centre.{}.bin", R::SUFFIX), &key);
    assert_eq!(flyvis_centre.shape, [dt.n_settle, f.manifest.centre_nodes.len()], "settling_centre shape");
    Settled {
        rust: g.state().iter().map(|v| v.to_f64()).collect(),
        flyvis: settled.data.iter().map(|v| v.to_f64()).collect(),
        rust_centre,
        flyvis_centre: flyvis_centre.data.iter().map(|v| v.to_f64()).collect(),
    }
}

/// One block of a settling file (`settled.*`, `settling_centre.*`).
pub(crate) fn read<R: Precision>(f: &Fixture, name: &str, key: &BlockKey) -> Array<R> {
    let file = f.manifest.file(name).unwrap_or_else(|| panic!("{name} is not in the manifest"));
    let bytes = &f.files.iter().find(|(n, _)| *n == name).expect("loaded by fixture()").1;
    R::read(file, bytes, key).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The largest absolute error, where, and how many values are not equal.
pub(crate) struct Diff {
    pub(crate) max: f64,
    pub(crate) at: usize,
    pub(crate) differ: usize,
}

pub(crate) fn diff(rust: &[f64], flyvis: &[f64]) -> Diff {
    assert_eq!(rust.len(), flyvis.len());
    let mut d = Diff { max: 0.0, at: 0, differ: 0 };
    for (i, (a, b)) in rust.iter().zip(flyvis).enumerate() {
        let e = (a - b).abs();
        d.differ += usize::from(a != b);
        if e > d.max {
            d.max = e;
            d.at = i;
        }
    }
    d
}
```

  In `crates/flyvis/src/lib.rs`, after

```rust
#[cfg(test)]
mod expand_checks;
```

  add:

```rust
#[cfg(test)]
mod gate_support;
```

  In `crates/flyvis/src/settling_checks.rs`, replace lines 1–228 (the module
  doc, the imports, `F32_GAPS`, `F64_BOUND`, `SETTLING_FILES`, `Fixture`,
  `FIXTURE`, `fixture`, `load`, `cell_type`, `Precision` and its impls,
  `Counts`, `Settled`, `settle`, `read`, `Diff` and `diff`, ending with the
  closing brace of `fn diff`) with:

```rust
//! The settling gates: epic 0.3.0's stage 1.
//!
//! flyvis settles its network from the bias state for 1.0 s of uniform grey
//! before every stimulus. These tests run `semblance_sim::graded` through
//! the same settling for every run in task 1's reference set -- models 000
//! and 049, dt 1/200 and 0.02, float32 and float64 -- and compare the
//! settled state (45,669 values) and the 65 centre neurons at every settling
//! step against flyvis's.
//!
//! Two gates over the same eight runs:
//!
//!   exact  synapse counts as flyvis's torch computed them
//!          (`syn_count_torch.rs`). Every value must equal flyvis's.
//!   libm   synapse counts from Rust's own `exp`. Every value must lie within
//!          a bound fixed in the spec before any comparison ran.
//!
//! Neither is to be relaxed. A failure is a discrepancy to find in flyvis's
//! source, recorded as an amendment to the spec:
//! docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 6.
//!
//! Each test prints its result (visible with `--nocapture`). Read the libm
//! ratios, do not just watch them pass. All skip without fixtures;
//! `SEMBLANCE_REQUIRE_FIXTURES=1` makes a skip a failure (see `CLAUDE.md`).
//!
//! The fixture, settling itself and the comparison live in `gate_support.rs`,
//! shared with the trace gates in `trace_checks.rs`.

use crate::gate_support::{cell_type, diff, fixture, settle, Counts, Precision, F64_BOUND};
use crate::json;
use crate::pretrained::CELL_TYPES;
use crate::reference::Dtype;
use crate::test_support::load_reference_fixture;

/// flyvis's own float32-vs-float64 gaps, PROVENANCE.json `f32_f64_gap`:
/// (model, dt, settled, settling_centre). The libm gate's f32 bound is twice
/// each (two f32 computations can sit on opposite sides of the exact value).
/// `pre_registered_bounds_match_provenance` pins these to the file.
const F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.4114366875261908e-06, 1.347661143569212e-06),
    ("049", "1/200", 6.1578304686449314e-06, 1.9363140433270587e-06),
    ("000", "1/50", 1.276739533651039e-06, 2.6109457484935206e-06),
    ("049", "1/50", 6.4440782598373936e-06, 1.1488733433040466e-05),
];
```

  Everything from `fn exact<R: Precision>(run: u32) {` to the end of the file
  stays as it is.

- [ ] **Step 4: Prove the move changed nothing.**

```bash
cargo clippy -p semblance-flyvis --all-targets -- -D warnings
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib settling_checks -- --nocapture 2>&1 | tee /tmp/settling-moved.log | grep -E 'exact run|libm run|test result'
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --test acceptance_settling
```

  Expected: clippy clean; 20 `settling_checks` tests pass, with every exact
  run `0 of 45669 differ` and `0 of … differ`; all 13 SETTLING tests pass
  (about 30 s, most of it the shadow build).

- [ ] **Step 5: (f) — write the failing NaN test.** Append to
  `crates/flyvis/src/gate_support.rs`:

```rust

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nan_reference_value_makes_the_maximum_infinite() {
        let d = diff(&[1.0, 2.0, 3.0], &[1.0, f64::NAN, 3.5]);
        assert_eq!(d.max, f64::INFINITY, "a NaN must not be skipped");
        assert_eq!(d.at, 1);
        assert_eq!(d.differ, 2);
    }
}
```

  Run: `cargo test -p semblance-flyvis --lib gate_support`
  Expected: FAIL: `left: 0.5, right: inf`, "a NaN must not be skipped".

  Then fix `diff` in `gate_support.rs`. Replace:

```rust
pub(crate) fn diff(rust: &[f64], flyvis: &[f64]) -> Diff {
    assert_eq!(rust.len(), flyvis.len());
    let mut d = Diff { max: 0.0, at: 0, differ: 0 };
    for (i, (a, b)) in rust.iter().zip(flyvis).enumerate() {
        let e = (a - b).abs();
        d.differ += usize::from(a != b);
```

  with:

```rust
/// A NaN on either side counts as an infinite error, so it becomes the
/// maximum and fails every bound, instead of being skipped by `>`.
pub(crate) fn diff(rust: &[f64], flyvis: &[f64]) -> Diff {
    assert_eq!(rust.len(), flyvis.len());
    let mut d = Diff { max: 0.0, at: 0, differ: 0 };
    for (i, (a, b)) in rust.iter().zip(flyvis).enumerate() {
        let e = (a - b).abs();
        let e = if e.is_nan() { f64::INFINITY } else { e };
        d.differ += usize::from(a != b);
```

  In `crates/flyvis/tests/acceptance_settling.rs`, make the same change to
  `max_err`. Replace:

```rust
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        let e = (x - y).abs();
        m.2 += usize::from(x != y);
```

  with:

```rust
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        let e = (x - y).abs();
        // a NaN is an infinite error, not one `>` skips
        let e = if e.is_nan() { f64::INFINITY } else { e };
        m.2 += usize::from(x != y);
```

  Run: `cargo test -p semblance-flyvis --lib gate_support`
  Expected: PASS.

- [ ] **Step 6: (b) — ratios in `{:.3e}`.** In `settling_checks.rs`'s `libm`,
  replace the two `{:.3} of bound` in the format string:

```rust
        "libm run {run} ({}, dt {}, {}): settled max {:.3e} at node {} ({}), {:.3} of bound; \
         settling centre max {:.3e} at step {} ({}), {:.3} of bound; worst types {}; settled bit-equal {} of {}",
```

  with:

```rust
        "libm run {run} ({}, dt {}, {}): settled max {:.3e} at node {} ({}), {:.3e} of bound; \
         settling centre max {:.3e} at step {} ({}), {:.3e} of bound; worst types {}; settled bit-equal {} of {}",
```

  SETTLING-04 asserts the old text, so it fails now. Confirm:

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --test acceptance_settling settling_04`
  Expected: FAIL, "settled ratio".

  Update SETTLING-04's two ratio assertions (the only edit to that test). In
  `crates/flyvis/tests/acceptance_settling.rs`, replace

```rust
    assert_eq!(field(after_settled, "), ", ' '), format!("{:.3}", ms / bs), "settled ratio: {line}");
```

  with

```rust
    assert_eq!(field(after_settled, "), ", ' '), format!("{:.3e}", ms / bs), "settled ratio: {line}");
```

  and

```rust
    assert_eq!(field(after_centre, "), ", ' '), format!("{:.3}", mc / bc), "centre ratio: {line}");
```

  with

```rust
    assert_eq!(field(after_centre, "), ", ' '), format!("{:.3e}", mc / bc), "centre ratio: {line}");
```

- [ ] **Step 7: (i) — an empty shadow tree.** In `acceptance_settling.rs`'s
  `fn shadow()`, replace:

```rust
        let root = base.join("workspace");
        let ws = Path::new(WORKSPACE);
        std::fs::create_dir_all(&root).unwrap();
```

  with:

```rust
        let root = base.join("workspace");
        let ws = Path::new(WORKSPACE);
        // Copy into an empty tree, so a source file deleted from the
        // workspace is not still compiled here. The sibling target dir is
        // kept. remove_dir_all removes data/'s symlinks, never their targets.
        match std::fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("removing the old shadow {}: {e}", root.display()),
        }
        std::fs::create_dir_all(&root).unwrap();
```

  (`std::fs::remove_dir_all` does not follow symlinks, so the shadow's
  `data/` links are removed and the real `data/` is never touched.)

- [ ] **Step 8: (h) — the integer-count wording.** In `settling_checks.rs`,
  replace the doc comment of `the_gate_resolves_integer_counts`:

```rust
/// A control at the scale the libm bound exists to resolve: the spec's
/// integer counts in place of exp(ln count) must exceed the f64 bound.
/// Measured 4.8e-7 to 6.7e-6 across runs during the spec's review.
#[test]
```

  with:

```rust
/// A control at the scale the libm bound exists to resolve: the spec's
/// integer counts in place of exp(ln count) must exceed the f64 bound.
/// Measured 4.8e-7 to 6.7e-6 across runs during the spec's review.
///
/// It resolves them against the f64 bound only. The f32 libm bound does not
/// resolve integer counts in 3 of the 4 f32 runs; in f32 the exact gate is
/// the guard (the graded-settling spec's 2026-10-29 amendment).
#[test]
```

  No assertion changes. Append to
  `docs/superpowers/specs/2026-10-28-graded-settling-design.md` (after its
  last line, with one blank line before):

```markdown
## Amendment (2026-10-29): what the integer-count control resolves

A wording correction, recorded under the stimulus-traces spec
([`2026-10-28-stimulus-traces-design.md`](2026-10-28-stimulus-traces-design.md),
ruling R4, item (h)). No assertion changes.

Section 6 says `the_gate_resolves_integer_counts` shows that "the libm bound
resolves the class of error it exists to catch". That holds for the f64
bound, which is the one the control runs against (run 3, model 000, dt 0.02,
f64). It does not hold for the f32 bound: in 3 of the 4 f32 runs, integer
counts stay within twice flyvis's f32/f64 gap. In f32, the exact gate is what
guards against integer counts. The test's doc comment says the same.
```

- [ ] **Step 9: Run everything this task touches.**

```bash
cargo clippy --workspace --all-targets -- -D warnings
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- --nocapture 2>&1 | grep -E 'libm run|test result'
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --test acceptance_settling
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace
```

  Expected: clippy clean. The libm lines now read `…, 1.689e-1 of bound;`
  in f32 and `…, 1.110e-5 of bound;` (run 1) in f64, not `0.000`. All 13
  SETTLING tests pass. The workspace passes with 0 failed.

- [ ] **Step 10: Commit.**

```bash
git add crates/flyvis/src/test_support.rs crates/flyvis/src/gate_support.rs crates/flyvis/src/settling_checks.rs crates/flyvis/src/lib.rs crates/flyvis/tests/acceptance_settling.rs docs/superpowers/specs/2026-10-28-graded-settling-design.md
git commit -m "Move the shared gate machinery into gate_support and fix the settling minors"
```

---

### Task 5: The input mapping and the sequence reader (wave 2)

**Wave:** 2

**Owns:** `crates/flyvis/src/wiring.rs`, `crates/flyvis/src/reference.rs`

Spec sections 1 and 2. Both are library code: every malformed input is a
typed error, never a panic. The `lib.rs` re-export of `scatter_hexals` is
Task 6's (it owns `lib.rs` in that wave); nothing here needs it.

**Files:**
- Modify: `crates/flyvis/src/wiring.rs` (`WiringError` variants,
  `scatter_hexals`, tests)
- Modify: `crates/flyvis/src/reference.rs` (`SequenceInfo`,
  `SequenceFrames`, `ReferenceError::FrameRange`, `parse_sequences`, tests)

**Interfaces:**
- Consumes: `test_support::{load_stimulus_fixture, real_spec_and_network, load_reference_fixture}` (unchanged signatures, Task 4).
- Produces:
  - `semblance_flyvis::wiring::scatter_hexals<T: Copy>(nodes: &[Vec<u32>], frame: &[T], out: &mut [T]) -> Result<(), WiringError>`
  - `WiringError::FrameLength { expected: usize, found: usize }`,
    `WiringError::NodeOutOfRange { node: u32, len: usize }`
  - `semblance_flyvis::reference::SequenceInfo { pub index: u32, pub family: String, pub flyvis_index: u32, pub args: Vec<(String, f64)>, pub frames: Vec<SequenceFrames> }`
    with `fn arg(&self, name: &str) -> Option<f64>` and
    `fn frames(&self, dt: &str) -> Option<&SequenceFrames>`
  - `semblance_flyvis::reference::SequenceFrames { pub dt: String, pub n_frames: usize, pub stimulus_frames: (usize, usize), pub snapshot_frames: Vec<usize> }`
  - `semblance_flyvis::reference::parse_sequences(json: &str) -> Result<Vec<SequenceInfo>, ReferenceError>`
  - `ReferenceError::FrameRange(String)` (the JSON path of the offending array)

- [ ] **Step 1: Write the failing `scatter_hexals` tests.** In
  `crates/flyvis/src/wiring.rs`, change the test module's import

```rust
    use crate::test_support::real_spec_and_network;
```

  to

```rust
    use crate::test_support::{load_stimulus_fixture, real_spec_and_network};
```

  and insert before `#[test] fn the_real_network_has_2355_groups_and_eight_input_types_of_721`:

```rust
    #[test]
    fn scatter_writes_every_type_and_leaves_other_nodes_alone() {
        let nodes = input_nodes(&spec(r#"["B","A"]"#), &net()).unwrap();
        let mut out = [-1i32; 4];
        assert_eq!(scatter_hexals(&nodes, &[7, 9], &mut out), Ok(()));
        assert_eq!(out, [7, 9, 7, 9], "hexal h lands on each type's h-th node");
        let mut out = [-1i32; 5];
        assert_eq!(scatter_hexals(&[vec![2, 3]], &[7, 9], &mut out), Ok(()));
        assert_eq!(out, [-1, -1, 7, 9, -1], "nodes outside `nodes` keep their values");
    }

    #[test]
    fn scatter_refuses_a_frame_of_the_wrong_length_and_writes_nothing() {
        let mut out = [-1i32; 4];
        assert_eq!(
            scatter_hexals(&[vec![0, 1], vec![2, 3]], &[7, 9, 11], &mut out),
            Err(WiringError::FrameLength { expected: 2, found: 3 })
        );
        assert_eq!(
            scatter_hexals(&[vec![0, 1], vec![2]], &[7, 9], &mut out),
            Err(WiringError::FrameLength { expected: 1, found: 2 }),
            "checked for every type before the first is written"
        );
        assert_eq!(out, [-1; 4]);
    }

    #[test]
    fn scatter_refuses_a_node_past_the_buffer_and_writes_nothing() {
        let mut out = [-1i32; 4];
        assert_eq!(
            scatter_hexals(&[vec![0, 1], vec![2, 4]], &[7, 9], &mut out),
            Err(WiringError::NodeOutOfRange { node: 4, len: 4 })
        );
        assert_eq!(out, [-1; 4], "the first type is valid, and still not written");
    }

    /// The input mapping, gated on flyvis's own lattice: a frame lit at
    /// hexal h alone lights exactly eight nodes, one in each input type, each
    /// at hex_coords.csv row h's (u, v). A transposed or reflected mapping
    /// fails here, before any dynamics run.
    #[test]
    fn input_mapping_matches_flyviss_lattice() {
        let Some(csv) = load_stimulus_fixture("hex_coords.csv") else { return };
        let Some((spec, net)) = real_spec_and_network() else { return };
        let mut lines = csv.lines();
        assert_eq!(lines.next(), Some("index,u,v"), "hex_coords.csv header");
        let rows: Vec<(i32, i32)> = lines
            .enumerate()
            .map(|(h, line)| {
                let f: Vec<i32> = line.split(',').map(|x| x.trim().parse().expect("an integer")).collect();
                assert_eq!((f.len(), f[0]), (3, h as i32), "hex_coords.csv row {h}");
                (f[1], f[2])
            })
            .collect();
        assert_eq!(rows.len(), 721);
        let nodes = input_nodes(&spec, &net).expect("input units are cell types");
        let input_types: Vec<u16> = nodes.iter().map(|k| net.node_cell_type()[k[0] as usize]).collect();
        let mut frame = vec![0u8; 721];
        let mut out = vec![0u8; net.num_nodes()];
        for (h, &(u, v)) in rows.iter().enumerate() {
            frame[h] = 1;
            out.fill(0);
            scatter_hexals(&nodes, &frame, &mut out).expect("721 hexals onto 8 x 721 nodes");
            frame[h] = 0;
            let lit: Vec<usize> = (0..out.len()).filter(|&i| out[i] == 1).collect();
            assert_eq!(lit.len(), 8, "hexal {h} lights {} nodes", lit.len());
            let mut types: Vec<u16> = lit.iter().map(|&i| net.node_cell_type()[i]).collect();
            types.sort_unstable();
            let mut want = input_types.clone();
            want.sort_unstable();
            assert_eq!(types, want, "hexal {h}: one node in each input type");
            for &i in &lit {
                assert_eq!(
                    (i32::from(net.node_u()[i]), i32::from(net.node_v()[i])),
                    (u, v),
                    "hexal {h} lands on node {i} ({})",
                    net.cell_types()[net.node_cell_type()[i] as usize]
                );
            }
        }
    }
```

  Run: `cargo test -p semblance-flyvis --lib wiring`
  Expected: does not compile: `cannot find function scatter_hexals`, `no
  variant FrameLength`.

- [ ] **Step 2: Implement.** In `wiring.rs`, replace the module doc's first
  paragraph and spec line:

```rust
//! What the graded integrator needs from flyvis's build besides the network:
//! which synapse-count group each edge belongs to, and which nodes receive
//! the visual input.
//!
//! Spec: docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 2.
```

  with:

```rust
//! What the graded integrator needs from flyvis's build besides the network:
//! which synapse-count group each edge belongs to, which nodes receive the
//! visual input, and how one stimulus frame reaches them.
//!
//! Specs: docs/superpowers/specs/2026-10-28-graded-settling-design.md,
//! section 2; docs/superpowers/specs/2026-10-28-stimulus-traces-design.md,
//! section 1 (`scatter_hexals`).
```

  Add the two variants at the end of `pub enum WiringError`:

```rust
    /// An input type's node list and the stimulus frame differ in length.
    FrameLength { expected: usize, found: usize },
    /// An input node is past the end of the buffer being written.
    NodeOutOfRange { node: u32, len: usize },
```

  and their `Display` arms after `UnknownInputUnit`'s:

```rust
            WiringError::FrameLength { expected, found } =>
                write!(f, "a frame of {found} hexals, but an input type has {expected} nodes"),
            WiringError::NodeOutOfRange { node, len } =>
                write!(f, "input node {node} is past the end of a buffer of {len}"),
```

  Then add, after `pub fn input_nodes(…) { … }` and before `#[cfg(test)]`:

```rust
/// Writes one stimulus frame onto the input nodes: `out[nodes[k][h]] =
/// frame[h]` for every input type k and hexal h. Nodes not in `nodes` are left
/// as they are; the input nodes are overwritten, so a buffer zeroed once stays
/// zero elsewhere. flyvis `stimulus.py:206`.
///
/// `nodes` is [`input_nodes`]'s [8][721]. Everything is checked before
/// anything is written, so an error leaves `out` untouched.
pub fn scatter_hexals<T: Copy>(nodes: &[Vec<u32>], frame: &[T], out: &mut [T]) -> Result<(), WiringError> {
    for k in nodes {
        if k.len() != frame.len() {
            return Err(WiringError::FrameLength { expected: k.len(), found: frame.len() });
        }
        if let Some(&node) = k.iter().find(|&&i| i as usize >= out.len()) {
            return Err(WiringError::NodeOutOfRange { node, len: out.len() });
        }
    }
    for k in nodes {
        for (&i, &x) in k.iter().zip(frame) {
            out[i as usize] = x;
        }
    }
    Ok(())
}
```

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib wiring`
  Expected: PASS, including the fixture gate
  `input_mapping_matches_flyviss_lattice` (all 721 hexals, 8 nodes each, at
  hex_coords.csv's (u, v)). If the fixture gate fails, stop and report: per
  the epic's diagnostic ladder, a mapping failure here means the input
  mapping is wrong, and nothing downstream is meaningful.

- [ ] **Step 3: Write the failing `parse_sequences` tests.** In
  `crates/flyvis/src/reference.rs`, in `mod tests`, insert before
  `#[test] fn parses_a_valid_manifest()`:

```rust
    const SEQUENCES: &str = r#"{"sequences":[
        {"index":0,"family":"flashes","flyvis_index":0,
         "args":{"baseline":0.5,"intensity":0.0,"radius":-1},
         "frames":{"1/200":{"n_frames":600,"stimulus_frames":[200,400],"snapshot_frames":[0,119,239,359,479,599]},
                   "1/50":{"n_frames":150,"stimulus_frames":[50,100],"snapshot_frames":[0,29,59,89,119,149]}}},
        {"index":1,"family":"moving_edge","flyvis_index":4,
         "args":{"angle":90,"intensity":0.0,"speed":13.0,"offsets":[-10,11],"post_pad_mode":"continue"},
         "frames":{"1/50":{"n_frames":131,"stimulus_frames":[50,81],"snapshot_frames":[0,26,52,78,104,130]}}}]}"#;

    fn sequence_error(text: &str) -> ReferenceError {
        parse_sequences(text).expect_err("must be refused")
    }

    #[test]
    fn parses_the_sequences_of_a_synthetic_manifest() {
        let s = parse_sequences(SEQUENCES).expect("valid");
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].index, s[0].family.as_str(), s[0].flyvis_index), (0, "flashes", 0));
        assert_eq!(
            s[0].args,
            vec![("baseline".to_string(), 0.5), ("intensity".to_string(), 0.0), ("radius".to_string(), -1.0)]
        );
        assert_eq!(
            s[0].frames("1/200"),
            Some(&SequenceFrames {
                dt: "1/200".into(),
                n_frames: 600,
                stimulus_frames: (200, 400),
                snapshot_frames: vec![0, 119, 239, 359, 479, 599],
            })
        );
        assert_eq!(s[0].frames.iter().map(|f| f.dt.as_str()).collect::<Vec<_>>(), ["1/200", "1/50"]);
        assert_eq!((s[1].arg("angle"), s[1].arg("speed"), s[1].arg("radius")), (Some(90.0), Some(13.0), None));
        assert_eq!(s[1].arg("offsets"), None, "a non-numeric arg is skipped");
        assert_eq!(s[1].frames("1/200"), None);
        assert_eq!(s[1].frames("1/50").map(|f| f.stimulus_frames), Some((50, 81)));
    }

    #[test]
    fn sequences_refuse_frames_out_of_order_or_past_n_frames() {
        let at = |p: &str| ReferenceError::FrameRange(p.to_string());
        let snap = "sequences[1].frames.1/50.snapshot_frames";
        let window = "sequences[1].frames.1/50.stimulus_frames";
        assert_eq!(sequence_error(&SEQUENCES.replace("104,130]", "104,131]")), at(snap), "a snapshot at n_frames");
        assert_eq!(sequence_error(&SEQUENCES.replace("[0,26,52,78,104,130]", "[0,52,26,78,104,130]")), at(snap));
        assert_eq!(sequence_error(&SEQUENCES.replace("[0,26,52,78,104,130]", "[0,26,26,78,104,130]")), at(snap));
        assert_eq!(sequence_error(&SEQUENCES.replace("[50,81]", "[50,132]")), at(window), "a window past n_frames");
        assert_eq!(sequence_error(&SEQUENCES.replace("[50,81]", "[82,81]")), at(window), "a window that ends first");
        // the window may be empty, and may end at n_frames exactly
        assert!(parse_sequences(&SEQUENCES.replace("[50,81]", "[131,131]")).is_ok());
    }

    #[test]
    fn sequences_refuse_a_missing_or_mistyped_field() {
        use ReferenceError::{Missing, WrongType};
        assert_eq!(sequence_error(r#"{"runs":[]}"#), Missing("manifest.sequences".into()));
        assert_eq!(
            sequence_error(&SEQUENCES.replacen(r#""flyvis_index":0,"#, "", 1)),
            Missing("sequences[0].flyvis_index".into())
        );
        assert_eq!(
            sequence_error(&SEQUENCES.replacen(r#""n_frames":600"#, r#""n_frames":"600""#, 1)),
            WrongType("sequences[0].frames.1/200.n_frames".into())
        );
        assert_eq!(
            sequence_error(&SEQUENCES.replace("[50,81]", "[50]")),
            WrongType("sequences[1].frames.1/50.stimulus_frames".into())
        );
        assert_eq!(
            sequence_error(&SEQUENCES.replace("[0,26,52,78,104,130]", "[0,-26,52,78,104,130]")),
            WrongType("sequences[1].frames.1/50.snapshot_frames".into())
        );
        assert_eq!(
            sequence_error(&SEQUENCES.replacen(r#""args":{"baseline":0.5,"intensity":0.0,"radius":-1}"#, r#""args":[]"#, 1)),
            WrongType("sequences[0].args".into())
        );
        assert_eq!(sequence_error(&SEQUENCES.replacen(r#""family":"flashes""#, r#""family":1"#, 1)), WrongType("sequences[0].family".into()));
    }

    /// The reference set's content, which the parser leaves to its caller:
    /// a frame layout for every dt id, with six snapshot frames each.
    fn check_content(sequences: &[SequenceInfo], dt_ids: &[&str]) -> Result<(), String> {
        for s in sequences {
            for &dt in dt_ids {
                let f = s.frames(dt).ok_or_else(|| format!("sequence {} has no frames for dt {dt}", s.index))?;
                if f.snapshot_frames.len() != 6 {
                    return Err(format!("sequence {} dt {dt}: {} snapshot frames", s.index, f.snapshot_frames.len()));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn the_content_check_refuses_a_missing_dt_entry_and_five_snapshots() {
        let s = parse_sequences(SEQUENCES).unwrap();
        assert_eq!(check_content(&s[..1], &["1/200", "1/50"]), Ok(()));
        assert_eq!(check_content(&s, &["1/200", "1/50"]), Err("sequence 1 has no frames for dt 1/200".into()));
        let five = parse_sequences(&SEQUENCES.replace("[0,26,52,78,104,130]", "[0,26,52,78,130]")).unwrap();
        assert_eq!(check_content(&five[1..], &["1/50"]), Err("sequence 1 dt 1/50: 5 snapshot frames".into()));
    }

    /// The published manifest's sequences are the twelve the trace gates
    /// play: flashes 0-3, then moving edges 4-11 at angles 0/30/90/150, OFF
    /// then ON, speed 13, height 80 (spec, fact H6).
    #[test]
    fn the_real_manifest_lists_the_twelve_reference_sequences() {
        let Some(text) = load_reference_fixture("manifest.json") else { return };
        let m = Manifest::parse(&text).expect("manifest.json must parse");
        let s = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let dt_ids: Vec<&str> = m.dts.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(dt_ids, ["1/200", "1/50"]);
        check_content(&s, &dt_ids).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.len(), 12);
        for (i, q) in s.iter().enumerate() {
            assert_eq!(q.index, i as u32, "sequences are listed in index order");
            assert_eq!(q.frames.len(), 2, "sequence {i}: one layout per dt, no others");
            let (family, flyvis_index, n_frames, window): (&str, usize, [usize; 2], [(usize, usize); 2]) = if i < 4 {
                ("flashes", i, [600, 150], [(200, 400), (50, 100)])
            } else {
                ("moving_edge", i - 4, [525, 131], [(200, 325), (50, 81)])
            };
            assert_eq!((q.family.as_str(), q.flyvis_index as usize), (family, flyvis_index), "sequence {i}");
            for (k, dt) in dt_ids.iter().enumerate() {
                let f = q.frames(dt).unwrap();
                assert_eq!((f.n_frames, f.stimulus_frames), (n_frames[k], window[k]), "sequence {i} dt {dt}");
                // trunc(linspace(0, n - 1, 6))
                let want: Vec<usize> = (0..6).map(|j| (j as f64 * (n_frames[k] - 1) as f64 / 5.0) as usize).collect();
                assert_eq!(f.snapshot_frames, want, "sequence {i} dt {dt}");
            }
            if i < 4 {
                assert_eq!(q.arg("intensity"), Some(if i < 2 { 0.0 } else { 1.0 }), "sequence {i}");
                assert_eq!(q.arg("radius"), Some(if i % 2 == 0 { -1.0 } else { 6.0 }), "sequence {i}");
            } else {
                let e = i - 4;
                assert_eq!(q.arg("angle"), Some([0.0, 30.0, 90.0, 150.0][e / 2]), "sequence {i}");
                assert_eq!(q.arg("intensity"), Some((e % 2) as f64), "sequence {i}");
                assert_eq!((q.arg("speed"), q.arg("height"), q.arg("width")), (Some(13.0), Some(80.0), Some(80.0)), "sequence {i}");
            }
        }
    }
```

  Run: `cargo test -p semblance-flyvis --lib reference`
  Expected: does not compile: `cannot find function parse_sequences`,
  `SequenceFrames`, `FrameRange`.

- [ ] **Step 4: Implement.** In `reference.rs`:

  1. Below the module doc's `//! Spec: …graded-settling-design.md, section 5.`
     line, add:

```rust
//! The manifest's `sequences`, read by [`parse_sequences`]:
//! docs/superpowers/specs/2026-10-28-stimulus-traces-design.md, section 2.
```

  2. Before `#[derive(Clone, Debug, PartialEq)] pub enum ReferenceError {`,
     add:

```rust
/// One reference stimulus sequence, from the manifest's `sequences`. Field
/// names are the manifest's own.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceInfo {
    /// `index`: the sequence's key in every per-sequence block.
    pub index: u32,
    /// `family`: `"flashes"` or `"moving_edge"`.
    pub family: String,
    /// `flyvis_index`: its index within flyvis's own stimulus dataset.
    pub flyvis_index: u32,
    /// `args`: every numeric entry, in file order. Non-numeric entries
    /// (`offsets`, `post_pad_mode`) are skipped.
    pub args: Vec<(String, f64)>,
    /// `frames`: one entry per dt id, in file order.
    pub frames: Vec<SequenceFrames>,
}

/// A sequence's frame layout at one dt.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceFrames {
    /// The key: `"1/200"` or `"1/50"`.
    pub dt: String,
    pub n_frames: usize,
    /// `[start, end)`: end is exclusive.
    pub stimulus_frames: (usize, usize),
    /// The frames `snapshots.*.bin` holds the full state at, ascending.
    pub snapshot_frames: Vec<usize>,
}

impl SequenceInfo {
    /// The numeric argument `name`, if the manifest gives one.
    pub fn arg(&self, name: &str) -> Option<f64> {
        self.args.iter().find(|(k, _)| k == name).map(|&(_, v)| v)
    }

    /// The frame layout at dt id `dt`.
    pub fn frames(&self, dt: &str) -> Option<&SequenceFrames> {
        self.frames.iter().find(|f| f.dt == dt)
    }
}
```

  3. Add the variant at the end of `pub enum ReferenceError`:

```rust
    /// A sequence's frame indices are out of order or past its `n_frames`.
    FrameRange(String),
```

     and its `Display` arm after `NoBlock`'s:

```rust
            FrameRange(p) => write!(f, "manifest {p} is out of order or past n_frames"),
```

  4. Before `fn parse_file(f: &Json, at: &str) -> Result<FileInfo, ReferenceError> {`,
     add:

```rust
/// An object's entries, in file order.
fn object<'a>(obj: &'a Json, key: &str, at: &str) -> Result<&'a [(String, Json)], ReferenceError> {
    match get(obj, key, at)? {
        Json::Object(pairs) => Ok(pairs),
        _ => Err(ReferenceError::WrongType(format!("{at}.{key}"))),
    }
}

/// A non-negative integer: a frame index or count.
fn frame_index(value: &Json, at: &str) -> Result<usize, ReferenceError> {
    value.as_i64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| ReferenceError::WrongType(at.to_string()))
}

fn parse_frames(dt: &str, f: &Json, at: &str) -> Result<SequenceFrames, ReferenceError> {
    let n_frames = count(f, "n_frames", at)?;
    let window_at = format!("{at}.stimulus_frames");
    let [start, end] = array(f, "stimulus_frames", at)? else {
        return Err(ReferenceError::WrongType(window_at));
    };
    let stimulus_frames = (frame_index(start, &window_at)?, frame_index(end, &window_at)?);
    if !(stimulus_frames.0 <= stimulus_frames.1 && stimulus_frames.1 <= n_frames) {
        return Err(ReferenceError::FrameRange(window_at));
    }
    let snapshots_at = format!("{at}.snapshot_frames");
    let snapshot_frames = array(f, "snapshot_frames", at)?
        .iter()
        .map(|v| frame_index(v, &snapshots_at))
        .collect::<Result<Vec<_>, _>>()?;
    let ascending = snapshot_frames.windows(2).all(|w| w[0] < w[1]);
    if !ascending || snapshot_frames.last().is_some_and(|&last| last >= n_frames) {
        return Err(ReferenceError::FrameRange(snapshots_at));
    }
    Ok(SequenceFrames { dt: dt.to_string(), n_frames, stimulus_frames, snapshot_frames })
}

/// Reads the manifest's `sequences`. Separate from [`Manifest::parse`], which
/// does not require them.
///
/// Checks structure only: every field present and typed, each
/// `stimulus_frames` with start <= end <= `n_frames`, each `snapshot_frames`
/// strictly ascending and inside `0..n_frames`. How many sequences there are,
/// and which dts each covers, are the caller's to check.
pub fn parse_sequences(json: &str) -> Result<Vec<SequenceInfo>, ReferenceError> {
    let root = json::parse(json).map_err(|e| ReferenceError::Json { offset: e.offset })?;
    array(&root, "sequences", "manifest")?
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let at = format!("sequences[{i}]");
            let args = object(s, "args", &at)?
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                .collect();
            let frames = object(s, "frames", &at)?
                .iter()
                .map(|(dt, f)| parse_frames(dt, f, &format!("{at}.frames.{dt}")))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SequenceInfo {
                index: index(get(s, "index", &at)?, &format!("{at}.index"))?,
                family: string(s, "family", &at)?,
                flyvis_index: index(get(s, "flyvis_index", &at)?, &format!("{at}.flyvis_index"))?,
                args,
                frames,
            })
        })
        .collect()
}
```

  `Manifest::parse` is not touched: its synthetic test manifests carry no
  `sequences` (review B2), and REFERENCE-01..10 must keep passing unchanged.

- [ ] **Step 5: Run.**

```bash
cargo clippy -p semblance-flyvis --all-targets -- -D warnings
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- reference:: wiring::
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --test acceptance_reference
```

  Expected: clippy clean; every `reference::` and `wiring::` test passes,
  including `the_real_manifest_lists_the_twelve_reference_sequences`; the
  REFERENCE acceptance tests pass unchanged.

- [ ] **Step 6: Commit.**

```bash
git add crates/flyvis/src/wiring.rs crates/flyvis/src/reference.rs
git commit -m "Add the hexal-to-photoreceptor scatter and the manifest's sequence reader"
```

---

### Task 6: The stimulus-trace gates (wave 3)

**Wave:** 3

**Owns:** `crates/flyvis/src/trace_checks.rs` (new), `crates/flyvis/src/lib.rs`, `Cargo.toml`

Spec sections 3 and 4, and R5. Everything this task uses already exists
after waves 0–2.

**Stop rule (repeat).** If any test in `trace_checks` fails, stop and report
its full `--nocapture` output. Do not change a bound, a gap constant,
`TOLERANCE`, an `==` or an assertion. For an exact failure, R2's first
diagnostic step is to rerun from flyvis's settled state (`Start::Flyvis`) and
report both results; do not go further without the owner.

**Files:**
- Create: `crates/flyvis/src/trace_checks.rs`
- Modify: `crates/flyvis/src/lib.rs` (register the module; re-export
  `scatter_hexals`)
- Modify: `Cargo.toml` (dev-profile `opt-level` for two crates)

**Interfaces:**
- Consumes:
  - Task 1: `Graded::set_state(&mut self, &[R]) -> Result<(), GradedError>`
  - Task 3: `MovingEdge::stimulus_frames(&self, usize) -> Option<(usize, usize)>`,
    `crate::moving_edge_checks::TOLERANCE`
  - Task 4: `gate_support::{cell_type, diff, fixture, read, settle_state, Counts, Diff, Fixture, Precision, F64_BOUND}`,
    `test_support::{load_reference_fixture, or_skip, read_fixture_bytes, read_fixture_text}`
  - Task 5: `wiring::scatter_hexals`, `reference::{parse_sequences, SequenceInfo}`
  - Existing: `crate::expand::{hex_coords, DEFAULT_EXTENT}`, `crate::json`,
    `crate::stimuli::{frame_count, FlashParams, Flashes, Stimulus}`,
    `crate::moving_edge::{MovingEdge, MovingEdgeParams}`,
    `crate::reference::{BlockKey, Dtype, FileInfo, Manifest}`
- Produces: `semblance_flyvis::scatter_hexals` (crate-root re-export). The
  tests listed below.

The 22 tests this task adds:
- 8 exact gates `trace_exact_run_<n>_<model>_<dt>_<p>` and 8 libm gates
  `trace_libm_run_<n>_<model>_<dt>_<p>`, for runs 0–7
- `trace_bounds_match_provenance`, `snapshots_agree_with_the_centre_column`,
  `the_trace_gate_sees_a_wrong_mapping`,
  `a_run_from_flyviss_settled_state_is_bit_equal_too`
- `stimulus_checks::rust_flashes_equal_flyviss_input`,
  `stimulus_checks::rust_moving_edges_match_flyviss_input`

- [ ] **Step 1: Write the gates.** Create `crates/flyvis/src/trace_checks.rs`:

```rust
//! The stimulus-trace gates: epic 0.3.0's stage 2.
//!
//! Every reference run (models 000 and 049, dt 1/200 and 0.02, float32 and
//! float64) settles as in stage 1, then plays all twelve reference
//! sequences -- four flashes, eight moving edges -- each from that settled
//! state. Every frame of flyvis's own input (`input.f64.bin`) is scattered
//! onto the photoreceptors (`wiring::scatter_hexals`) and stepped. The 65
//! centre neurons are compared at every frame, and all 45,669 neurons at the
//! manifest's six snapshot frames.
//!
//!   exact  torch's synapse counts. Every value must equal flyvis's.
//!   libm   Rust's `exp`. Within the spec's bounds (R3): 1e-10 in f64, twice
//!          flyvis's own f32/f64 trace gap in f32.
//!
//! Neither is to be relaxed. A failure stops the work for diagnosis; the
//! first step is to rerun from flyvis's settled state (`Start::Flyvis`).
//! Spec: docs/superpowers/specs/2026-10-28-stimulus-traces-design.md.
//!
//! The gates are driven by flyvis's input, not Rust's stimuli (ruling R1):
//! Rust's moving edges differ from flyvis's by up to 3.9e-3, which would
//! bury an arithmetic gate. `stimulus_checks`, below, holds Rust's
//! generators to that same input separately.
//!
//! The trace files (about 220 MB) are loaded lazily, each once and only by
//! these tests: the settling gates never read them. Every test prints its
//! result (visible with `--nocapture`); read the libm ratios. All skip
//! without fixtures; `SEMBLANCE_REQUIRE_FIXTURES=1` makes a skip a failure.

use std::sync::OnceLock;

use crate::expand::{hex_coords, DEFAULT_EXTENT};
use crate::gate_support::{cell_type, diff, fixture, read, settle_state, Counts, Diff, Fixture, Precision, F64_BOUND};
use crate::json;
use crate::reference::{parse_sequences, BlockKey, Dtype, FileInfo, SequenceInfo};
use crate::test_support::{load_reference_fixture, or_skip, read_fixture_bytes, read_fixture_text};
use crate::wiring::scatter_hexals;

const HEXALS: usize = 721;

/// Snapshot frames per sequence, at every dt (`snapshots.*.bin`'s first axis).
const SNAPSHOTS: usize = 6;

/// flyvis's own float32-vs-float64 gaps over the stimulus traces,
/// PROVENANCE.json `f32_f64_gap`: (model, dt, centre, snapshots). The libm
/// gate's f32 bound is twice each (spec R3). They include the moving edges'
/// one rounding of the input to f32. `trace_bounds_match_provenance` pins
/// these to the file.
const TRACE_F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.7249018977594233e-06, 2.6318956276583094e-06),
    ("049", "1/200", 2.600224337356849e-05, 5.1135401793800384e-05),
    ("000", "1/50", 2.5357808688397654e-06, 2.9820266052027478e-06),
    ("049", "1/50", 3.4419031081256435e-05, 1.4233448473766064e-04),
];

const TRACE_FILES: [&str; 5] =
    ["input.f64.bin", "centre.f32.bin", "centre.f64.bin", "snapshots.f32.bin", "snapshots.f64.bin"];

/// One trace file's bytes, loaded on first use. Prints nothing.
fn trace_file(name: &str) -> Result<&'static [u8], String> {
    static FILES: [OnceLock<Result<Vec<u8>, String>>; 5] = [const { OnceLock::new() }; 5];
    let i = TRACE_FILES.iter().position(|&n| n == name).expect("one of TRACE_FILES");
    FILES[i]
        .get_or_init(|| read_fixture_bytes(&format!("flyvis-reference/{name}")))
        .as_ref()
        .map(Vec::as_slice)
        .map_err(String::clone)
}

/// The manifest's twelve sequences, parsed once. Prints nothing.
fn sequences() -> Result<&'static [SequenceInfo], String> {
    static SEQUENCES: OnceLock<Result<Vec<SequenceInfo>, String>> = OnceLock::new();
    SEQUENCES
        .get_or_init(|| {
            let text = read_fixture_text("flyvis-reference/manifest.json")?;
            Ok(parse_sequences(&text).expect("manifest.json's sequences must parse"))
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(String::clone)
}

/// Everything one precision's trace run reads.
struct Traces {
    f: &'static Fixture,
    sequences: &'static [SequenceInfo],
    input: &'static [u8],
    centre: &'static [u8],
    snapshots: &'static [u8],
}

fn load_traces<R: Precision>(f: &'static Fixture) -> Result<Traces, String> {
    Ok(Traces {
        f,
        sequences: sequences()?,
        input: trace_file("input.f64.bin")?,
        centre: trace_file(&format!("centre.{}.bin", R::SUFFIX))?,
        snapshots: trace_file(&format!("snapshots.{}.bin", R::SUFFIX))?,
    })
}

/// The traces for precision `R`, or `None` after printing a skip notice.
fn traces<R: Precision>() -> Option<Traces> {
    or_skip(load_traces::<R>(fixture()?))
}

fn file<'a>(f: &'a Fixture, name: &str) -> &'a FileInfo {
    f.manifest.file(name).unwrap_or_else(|| panic!("{name} is not in the manifest"))
}

/// For each hexal h, the hexal at (-u, -v): a point reflection of the frame.
fn mirrored_hexals() -> Vec<usize> {
    let c: Vec<(i32, i32)> = hex_coords(DEFAULT_EXTENT).collect();
    c.iter()
        .map(|&(u, v)| c.iter().position(|&p| p == (-u, -v)).expect("the lattice is point-symmetric"))
        .collect()
}

/// Which settled state each sequence starts from.
#[derive(Clone, Copy)]
enum Start {
    /// Rust's own settling (spec R2): settling and stimulus, end to end.
    Rust,
    /// flyvis's `settled.<p>.bin`: R2's first diagnostic step, isolating the
    /// stimulus phase.
    Flyvis,
}

/// One sequence played and compared, Rust's and flyvis's values widened to
/// f64.
struct Played {
    sequence: u32,
    n_frames: usize,
    snapshot_frames: Vec<usize>,
    /// Over [n_frames, 65].
    centre: Diff,
    /// Over [6, 45,669].
    snapshots: Diff,
    /// The first unequal centre value, as an index into [n_frames, 65].
    first: Option<usize>,
}

/// Settles reference run `run`, then plays `sequences` from the settled
/// state, each with flyvis's input (mirrored hexal by hexal if `mirror`), and
/// compares every frame's centre and every snapshot with flyvis's.
fn play<R: Precision>(t: &Traces, run: u32, counts: Counts, start: Start, sequences: &[u32], mirror: bool) -> Vec<Played> {
    let f = t.f;
    let info = &f.manifest.runs[run as usize];
    let (mut g, _) = settle_state::<R>(f, run, &info.model, counts);
    if let Start::Flyvis = start {
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
        let settled = read::<R>(f, &format!("settled.{}.bin", R::SUFFIX), &key);
        g.set_state(&settled.data).expect("flyvis's settled state: one finite value per node");
    }
    let settled = g.state().to_vec();
    let n = f.net.num_nodes();
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let source: Vec<usize> = if mirror { mirrored_hexals() } else { (0..HEXALS).collect() };
    let input_file = file(f, "input.f64.bin");
    let centre_file = file(f, &format!("centre.{}.bin", R::SUFFIX));
    let snapshots_file = file(f, &format!("snapshots.{}.bin", R::SUFFIX));

    // Zeroed once per run: every frame overwrites the same input nodes, and
    // every other node stays 0, as in flyvis's zeroed buffer.
    let mut input = vec![R::ZERO; n];
    let mut frame = vec![R::ZERO; HEXALS];
    let mut played = Vec::with_capacity(sequences.len());
    for &s in sequences {
        let q = &t.sequences[s as usize];
        assert_eq!(q.index, s, "manifest sequences must be listed in index order");
        let layout = q.frames(&info.dt).unwrap_or_else(|| panic!("sequence {s} has no frames at dt {}", info.dt));
        let n_frames = layout.n_frames;
        assert_eq!(layout.snapshot_frames.len(), SNAPSHOTS, "sequence {s}'s snapshot frames");
        g.set_state(&settled).expect("the settled state: one finite value per node");

        let x = input_file
            .read_f64(t.input, &BlockKey { run: None, dt: &info.dt, sequence: Some(s) })
            .unwrap_or_else(|e| panic!("input.f64.bin: {e}"));
        assert_eq!(x.shape, [n_frames, HEXALS], "input shape, sequence {s}");
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(s) };
        let flyvis_centre = R::read(centre_file, t.centre, &key).unwrap_or_else(|e| panic!("{}: {e}", centre_file.name));
        assert_eq!(flyvis_centre.shape, [n_frames, centre.len()], "centre shape, sequence {s}");
        let flyvis_snapshots =
            R::read(snapshots_file, t.snapshots, &key).unwrap_or_else(|e| panic!("{}: {e}", snapshots_file.name));
        assert_eq!(flyvis_snapshots.shape, [SNAPSHOTS, n], "snapshots shape, sequence {s}");

        let mut rust_centre = Vec::with_capacity(n_frames * centre.len());
        let mut rust_snapshots = Vec::with_capacity(SNAPSHOTS * n);
        for i in 0..n_frames {
            let row = &x.data[i * HEXALS..(i + 1) * HEXALS];
            for (slot, &h) in frame.iter_mut().zip(&source) {
                *slot = R::from_f64(row[h]);
            }
            scatter_hexals(&f.input_nodes, &frame, &mut input).expect("721 hexals onto 8 x 721 input nodes");
            g.step(&input).expect("one input value per node");
            if let Some(k) = g.state().iter().position(|v| !v.is_finite()) {
                panic!("run {run}: node {k} ({}) is not finite after frame {i} of sequence {s}", cell_type(&f.net, k));
            }
            // frame i is the state after i + 1 steps from the settled state
            rust_centre.extend(centre.iter().map(|&k| g.state()[k].to_f64()));
            if layout.snapshot_frames.contains(&i) {
                rust_snapshots.extend(g.state().iter().map(|v| v.to_f64()));
            }
        }
        let flyvis_centre: Vec<f64> = flyvis_centre.data.iter().map(|v| v.to_f64()).collect();
        let flyvis_snapshots: Vec<f64> = flyvis_snapshots.data.iter().map(|v| v.to_f64()).collect();
        played.push(Played {
            sequence: s,
            n_frames,
            snapshot_frames: layout.snapshot_frames.clone(),
            first: rust_centre.iter().zip(&flyvis_centre).position(|(a, b)| a != b),
            centre: diff(&rust_centre, &flyvis_centre),
            snapshots: diff(&rust_snapshots, &flyvis_snapshots),
        });
    }
    played
}

fn all_sequences(t: &Traces) -> Vec<u32> {
    t.sequences.iter().map(|q| q.index).collect()
}

fn exact<R: Precision>(run: u32) {
    let Some(t) = traces::<R>() else { return };
    let f = t.f;
    let info = &f.manifest.runs[run as usize];
    let played = play::<R>(&t, run, Counts::Torch, Start::Rust, &all_sequences(&t), false);
    let (n, n_centre) = (f.net.num_nodes(), f.manifest.centre_nodes.len());
    let mut failures = Vec::new();
    for p in &played {
        if p.centre.differ + p.snapshots.differ == 0 {
            continue;
        }
        let first = p.first.map_or_else(
            || "the snapshots only".to_string(),
            |i| format!("frame {} ({})", i / n_centre, f.manifest.centre_nodes[i % n_centre].cell_type),
        );
        failures.push(format!(
            "sequence {}: {} of {} centre and {} of {} snapshot values differ; first difference at {first}; \
             largest centre error {:e}; largest snapshot error {:e} at node {} ({})",
            p.sequence,
            p.centre.differ,
            p.n_frames * n_centre,
            p.snapshots.differ,
            SNAPSHOTS * n,
            p.centre.max,
            p.snapshots.max,
            p.snapshots.at % n,
            cell_type(&f.net, p.snapshots.at % n)
        ));
    }
    eprintln!(
        "trace exact run {run} ({}, dt {}, {}): {} of {} sequences bit-equal, centre and snapshots",
        info.model, info.dt, R::SUFFIX, played.len() - failures.len(), played.len()
    );
    assert!(failures.is_empty(), "run {run} is not bit-equal to flyvis under stimulus:\n{}", failures.join("\n"));
}

/// R3's bounds for one run: (centre, snapshots).
fn trace_bounds(model: &str, dt: &str, precision: Dtype) -> (f64, f64) {
    match precision {
        Dtype::F64 => (F64_BOUND, F64_BOUND),
        Dtype::F32 => {
            let &(_, _, c, s) =
                TRACE_F32_GAPS.iter().find(|g| g.0 == model && g.1 == dt).expect("a gap for every f32 run");
            (2.0 * c, 2.0 * s)
        }
    }
}

fn libm<R: Precision>(run: u32) {
    let Some(t) = traces::<R>() else { return };
    let f = t.f;
    let info = &f.manifest.runs[run as usize];
    let (bound_centre, bound_snapshots) = trace_bounds(&info.model, &info.dt, info.precision);
    let played = play::<R>(&t, run, Counts::Libm, Start::Rust, &all_sequences(&t), false);
    let (n, n_centre) = (f.net.num_nodes(), f.manifest.centre_nodes.len());

    let ratio = |p: &Played| (p.centre.max / bound_centre).max(p.snapshots.max / bound_snapshots);
    let worst = played.iter().max_by(|a, b| ratio(a).total_cmp(&ratio(b))).expect("twelve sequences");
    let c = played.iter().max_by(|a, b| a.centre.max.total_cmp(&b.centre.max)).expect("twelve sequences");
    let s = played.iter().max_by(|a, b| a.snapshots.max.total_cmp(&b.snapshots.max)).expect("twelve sequences");
    let c_node = &f.manifest.centre_nodes[c.centre.at % n_centre];
    let s_node = s.snapshots.at % n;
    let (equal, total) = played.iter().fold((0, 0), |(equal, total), p| {
        let size = p.n_frames * n_centre + SNAPSHOTS * n;
        (equal + size - p.centre.differ - p.snapshots.differ, total + size)
    });
    eprintln!(
        "trace libm run {run} ({}, dt {}, {}): worst sequence {}, {:.3e} of bound; \
         centre max {:.3e} in sequence {} at frame {} node {} ({}), {:.3e} of bound; \
         snapshots max {:.3e} in sequence {} at frame {} node {} ({}), {:.3e} of bound; bit-equal {equal} of {total}",
        info.model, info.dt, R::SUFFIX, worst.sequence, ratio(worst),
        c.centre.max, c.sequence, c.centre.at / n_centre, c_node.node, c_node.cell_type, c.centre.max / bound_centre,
        s.snapshots.max, s.sequence, s.snapshot_frames[s.snapshots.at / n], s_node, cell_type(&f.net, s_node),
        s.snapshots.max / bound_snapshots,
    );
    let over: Vec<String> = played
        .iter()
        .filter(|p| p.centre.max > bound_centre || p.snapshots.max > bound_snapshots)
        .map(|p| format!("sequence {}: centre {:e}, snapshots {:e}", p.sequence, p.centre.max, p.snapshots.max))
        .collect();
    assert!(
        over.is_empty(),
        "run {run}: Rust's exp exceeds the spec's bounds (centre {bound_centre:e}, snapshots {bound_snapshots:e}):\n{}",
        over.join("\n")
    );
}

macro_rules! trace_runs {
    ($($exact:ident, $libm:ident, $run:expr, $t:ty;)*) => {
        $(
            #[test]
            fn $exact() { exact::<$t>($run) }
            #[test]
            fn $libm() { libm::<$t>($run) }
        )*
    };
}

trace_runs! {
    trace_exact_run_0_000_1_200_f32, trace_libm_run_0_000_1_200_f32, 0, f32;
    trace_exact_run_1_000_1_200_f64, trace_libm_run_1_000_1_200_f64, 1, f64;
    trace_exact_run_2_000_1_50_f32, trace_libm_run_2_000_1_50_f32, 2, f32;
    trace_exact_run_3_000_1_50_f64, trace_libm_run_3_000_1_50_f64, 3, f64;
    trace_exact_run_4_049_1_200_f32, trace_libm_run_4_049_1_200_f32, 4, f32;
    trace_exact_run_5_049_1_200_f64, trace_libm_run_5_049_1_200_f64, 5, f64;
    trace_exact_run_6_049_1_50_f32, trace_libm_run_6_049_1_50_f32, 6, f32;
    trace_exact_run_7_049_1_50_f64, trace_libm_run_7_049_1_50_f64, 7, f64;
}

/// The gaps above are PROVENANCE.json's, bit for bit.
#[test]
fn trace_bounds_match_provenance() {
    let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
    let root = json::parse(&text).expect("PROVENANCE.json must parse");
    let gaps = root.get("f32_f64_gap").expect("PROVENANCE.json has f32_f64_gap");
    for &(model, dt, centre, snapshots) in &TRACE_F32_GAPS {
        let key = format!("{model} {dt}");
        let g = gaps.get(&key).unwrap_or_else(|| panic!("no gap for {key}"));
        for (name, want) in [("centre", centre), ("snapshots", snapshots)] {
            let got = g.get(name).and_then(|v| v.as_f64()).unwrap_or_else(|| panic!("{key} {name}"));
            assert_eq!(got.to_bits(), want.to_bits(), "{key} {name}: PROVENANCE {got:e}, gate {want:e}");
        }
    }
}

fn snapshots_hold_the_centre_column<R: Precision>() {
    let Some(t) = traces::<R>() else { return };
    let f = t.f;
    let n = f.net.num_nodes();
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let centre_file = file(f, &format!("centre.{}.bin", R::SUFFIX));
    let snapshots_file = file(f, &format!("snapshots.{}.bin", R::SUFFIX));
    let mut checked = 0;
    for info in f.manifest.runs.iter().filter(|r| r.precision == R::DTYPE) {
        for q in t.sequences {
            let layout = q.frames(&info.dt).unwrap_or_else(|| panic!("sequence {} at dt {}", q.index, info.dt));
            let key = BlockKey { run: Some(info.index), dt: &info.dt, sequence: Some(q.index) };
            let c = R::read(centre_file, t.centre, &key).unwrap_or_else(|e| panic!("{e}"));
            let s = R::read(snapshots_file, t.snapshots, &key).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(c.shape, [layout.n_frames, centre.len()], "run {} sequence {}", info.index, q.index);
            assert_eq!(s.shape, [SNAPSHOTS, n], "run {} sequence {}", info.index, q.index);
            for (k, &frame) in layout.snapshot_frames.iter().enumerate() {
                for (j, &node) in centre.iter().enumerate() {
                    let (a, b) = (c.data[frame * centre.len() + j].to_f64(), s.data[k * n + node].to_f64());
                    assert_eq!(
                        a.to_bits(),
                        b.to_bits(),
                        "run {} sequence {} snapshot {k} (frame {frame}), node {node} ({}): centre {a:e}, snapshots {b:e}",
                        info.index, q.index, cell_type(&f.net, node)
                    );
                }
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 4 * t.sequences.len(), "four {} runs, every sequence", R::SUFFIX);
}

/// `centre[snapshot_frames]` is `snapshots[:, centre_nodes]` in flyvis's
/// own reference (fact H7). The gates index both by that assumption.
#[test]
fn snapshots_agree_with_the_centre_column() {
    snapshots_hold_the_centre_column::<f32>();
    snapshots_hold_the_centre_column::<f64>();
}

/// A negative control: run 3 (000, 0.02, f64), sequence 8 (the 90 degree
/// OFF edge, which is not point-symmetric), with every frame's hexals
/// mirrored (u, v) -> (-u, -v) before scattering. A wrong mapping must miss
/// by far more than any bound. (A full-field flash would not change.)
/// Measured during the spec's review: about 2.5.
#[test]
fn the_trace_gate_sees_a_wrong_mapping() {
    let Some(t) = traces::<f64>() else { return };
    let p = &play::<f64>(&t, 3, Counts::Torch, Start::Rust, &[8], true)[0];
    eprintln!("mirrored mapping, run 3, sequence 8: snapshots max {:e}, centre max {:e}", p.snapshots.max, p.centre.max);
    assert!(p.snapshots.max > 1e-3, "a mirrored mapping must miss by far more than any bound: snapshots max {:e}", p.snapshots.max);
}

/// R2's first diagnostic step works: restarting from flyvis's own settled
/// state, run 3, sequence 8 is still bit-equal (settling is exact, so the
/// two starts are the same state).
#[test]
fn a_run_from_flyviss_settled_state_is_bit_equal_too() {
    let Some(t) = traces::<f64>() else { return };
    let p = &play::<f64>(&t, 3, Counts::Torch, Start::Flyvis, &[8], false)[0];
    assert_eq!((p.centre.differ, p.snapshots.differ), (0, 0), "run 3, sequence 8, from settled.f64.bin");
}

/// Rust's own stimulus generators against the input flyvis fed (spec
/// section 4). Not part of the gates above, which use flyvis's input.
mod stimulus_checks {
    use super::*;
    use crate::moving_edge::{MovingEdge, MovingEdgeParams};
    use crate::moving_edge_checks::TOLERANCE;
    use crate::reference::Manifest;
    use crate::stimuli::{frame_count, FlashParams, Flashes, Stimulus};

    /// The manifest's text and `input.f64.bin`, or `None` after a notice.
    fn reference_input() -> Option<(String, &'static [u8])> {
        let text = or_skip(read_fixture_text("flyvis-reference/manifest.json"))?;
        let bytes = or_skip(trace_file("input.f64.bin"))?;
        Some((text, bytes))
    }

    /// Flashes at both dts, matched to the reference by intensity and
    /// radius, equal flyvis's input exactly: frame count, stimulus window
    /// and every value.
    #[test]
    fn rust_flashes_equal_flyviss_input() {
        let Some((text, bytes)) = reference_input() else { return };
        let m = Manifest::parse(&text).expect("manifest.json must parse");
        let sequences = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let input = m.file("input.f64.bin").expect("input.f64.bin is in the manifest");
        for dt in &m.dts {
            let params = FlashParams::flyvis_defaults(dt.dt);
            let n_pre = frame_count(params.t_pre, params.dt);
            let window = (n_pre, n_pre + frame_count(params.t_stim, params.dt));
            let flashes = Flashes::new(params);
            assert_eq!(flashes.num_sequences(), 4);
            let mut out = vec![0.0f32; HEXALS];
            for s in 0..flashes.num_sequences() {
                let (intensity, radius) = (flashes.intensity_of(s).unwrap(), flashes.radius_of(s).unwrap());
                let matching: Vec<&SequenceInfo> = sequences
                    .iter()
                    .filter(|q| {
                        q.family == "flashes"
                            && q.arg("intensity") == Some(f64::from(intensity))
                            && q.arg("radius") == Some(f64::from(radius))
                    })
                    .collect();
                assert_eq!(matching.len(), 1, "dt {}: Rust flash {s} (intensity {intensity}, radius {radius})", dt.id);
                let q = matching[0];
                let layout = q.frames(&dt.id).expect("every sequence has every dt");
                assert_eq!(flashes.num_frames(), layout.n_frames, "dt {}: sequence {}'s frame count", dt.id, q.index);
                assert_eq!(window, layout.stimulus_frames, "dt {}: sequence {}'s stimulus window", dt.id, q.index);
                let x = input
                    .read_f64(bytes, &BlockKey { run: None, dt: &dt.id, sequence: Some(q.index) })
                    .unwrap_or_else(|e| panic!("input.f64.bin: {e}"));
                assert_eq!(x.shape, [layout.n_frames, HEXALS]);
                for i in 0..layout.n_frames {
                    flashes.frame_into(s, i, &mut out).expect("a frame in range");
                    for (h, (&rust, &flyvis)) in out.iter().zip(&x.data[i * HEXALS..(i + 1) * HEXALS]).enumerate() {
                        assert!(
                            f64::from(rust) == flyvis,
                            "dt {}: sequence {} frame {i} hexal {h}: Rust {rust:e}, flyvis {flyvis:e}",
                            dt.id, q.index
                        );
                    }
                }
            }
            eprintln!("flashes at dt {}: all 4 sequences equal flyvis's input, every frame and hexal", dt.id);
        }
    }

    /// Moving edges at height 80, speed 13, angles 0/30/90/150, matched by
    /// angle, intensity and speed. Frame count and window are exact; every
    /// value is within the geometry gate's `TOLERANCE` (the two renderers
    /// are different functions); 0 degrees, where flyvis rotates nothing, is
    /// bit-equal to the f32 input flyvis's float32 runs fed.
    #[test]
    fn rust_moving_edges_match_flyviss_input() {
        let Some((text, bytes)) = reference_input() else { return };
        let m = Manifest::parse(&text).expect("manifest.json must parse");
        let sequences = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let root = json::parse(&text).expect("manifest.json must parse");
        let input = m.file("input.f64.bin").expect("input.f64.bin is in the manifest");
        let angles = [0, 30, 90, 150];
        for dt in &m.dts {
            let mut params = MovingEdgeParams::flyvis_defaults(dt.dt);
            params.height = 80;
            params.speeds = vec![13.0];
            params.angles = angles.to_vec();
            assert_eq!(params.offsets, (-10, 10), "the default offsets, inclusive");
            let edges = MovingEdge::new(params.clone());
            assert_eq!(edges.num_sequences(), 8);
            let mut out = vec![0.0f32; HEXALS];
            let mut worst = [0.0f64; 4];
            for s in 0..edges.num_sequences() {
                let (angle, intensity, speed) =
                    (edges.angle_of(s).unwrap(), edges.intensity_of(s).unwrap(), edges.speed_of(s).unwrap());
                let matching: Vec<&SequenceInfo> = sequences
                    .iter()
                    .filter(|q| {
                        q.family == "moving_edge"
                            && q.arg("angle") == Some(f64::from(angle))
                            && q.arg("intensity") == Some(f64::from(intensity))
                            && q.arg("speed") == Some(speed)
                    })
                    .collect();
                assert_eq!(matching.len(), 1, "dt {}: Rust edge {s} (angle {angle}, intensity {intensity})", dt.id);
                let q = matching[0];
                assert_eq!(
                    (q.arg("height"), q.arg("width")),
                    (Some(f64::from(params.height)), Some(f64::from(params.width))),
                    "sequence {}'s bar",
                    q.index
                );
                // flyvis's offsets are an exclusive range, [-10, 11); ours
                // are inclusive, (-10, 10)
                let offsets: Vec<f64> = root
                    .get("sequences")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(q.index as usize))
                    .and_then(|v| v.get("args"))
                    .and_then(|v| v.get("offsets"))
                    .and_then(|v| v.as_array())
                    .expect("the manifest gives each edge's offsets")
                    .iter()
                    .map(|v| v.as_f64().expect("a numeric offset"))
                    .collect();
                assert_eq!(
                    offsets,
                    [f64::from(params.offsets.0), f64::from(params.offsets.1 + 1)],
                    "sequence {}'s offsets",
                    q.index
                );
                let layout = q.frames(&dt.id).expect("every sequence has every dt");
                assert_eq!(
                    (edges.num_frames(), edges.valid_frames(s)),
                    (layout.n_frames, Some(layout.n_frames)),
                    "dt {}: sequence {}'s frame count",
                    dt.id, q.index
                );
                assert_eq!(edges.stimulus_frames(s), Some(layout.stimulus_frames), "dt {}: sequence {}'s window", dt.id, q.index);
                let x = input
                    .read_f64(bytes, &BlockKey { run: None, dt: &dt.id, sequence: Some(q.index) })
                    .unwrap_or_else(|e| panic!("input.f64.bin: {e}"));
                assert_eq!(x.shape, [layout.n_frames, HEXALS]);
                let a = angles.iter().position(|&g| g == angle).expect("one of the four angles");
                for i in 0..layout.n_frames {
                    edges.frame_into(s, i, &mut out).expect("a frame in range");
                    for (h, (&rust, &flyvis)) in out.iter().zip(&x.data[i * HEXALS..(i + 1) * HEXALS]).enumerate() {
                        let d = (f64::from(rust) - flyvis).abs();
                        assert!(
                            d <= TOLERANCE,
                            "dt {}: sequence {} (angle {angle}) frame {i} hexal {h}: Rust {rust:e}, flyvis {flyvis:e}, {d:e} apart",
                            dt.id, q.index
                        );
                        if angle == 0 {
                            assert_eq!(
                                rust.to_bits(),
                                (flyvis as f32).to_bits(),
                                "dt {}: 0 degree sequence {} frame {i} hexal {h}: Rust {rust:e}, flyvis's f32 input {:e}",
                                dt.id, q.index, flyvis as f32
                            );
                        }
                        worst[a] = worst[a].max(d);
                    }
                }
            }
            eprintln!(
                "moving edges at dt {}: max |Rust - flyvis| at 0/30/90/150 degrees: {:.3e} {:.3e} {:.3e} {:.3e} \
                 (TOLERANCE {TOLERANCE}); 0 degrees bit-equal to flyvis's f32 input",
                dt.id, worst[0], worst[1], worst[2], worst[3]
            );
        }
    }
}
```

  In `crates/flyvis/src/lib.rs`, after

```rust
#[cfg(test)]
mod syn_count_torch_checks;
```

  add

```rust
#[cfg(test)]
mod trace_checks;
```

  and change

```rust
pub use wiring::{edge_syn_groups, input_nodes, WiringError};
```

  to

```rust
pub use wiring::{edge_syn_groups, input_nodes, scatter_hexals, WiringError};
```

- [ ] **Step 2: Compile and lint.**

  Run: `cargo clippy -p semblance-flyvis --all-targets -- -D warnings`
  Expected: clean.

- [ ] **Step 3: Measure "before" (R5).** One 1/50 run, at the default
  `opt-level` 0:

```bash
time SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- --exact trace_checks::trace_exact_run_3_000_1_50_f64 --nocapture
```

  Expected: PASS, `trace exact run 3 (000, dt 1/50, f64): 12 of 12 sequences
  bit-equal, centre and snapshots`, finishing in about 31 s (libtest's
  `finished in`; measured 31.07 s on the M3 Max that wrote this plan). A 1/200
  run steps four times as often, so it would take about two minutes: R5's
  120 s rule fires. Record the time; do not run the other 1/200 tests at
  `opt-level` 0.

- [ ] **Step 4: Optimise the two arithmetic crates in dev builds (R5).**
  Append to the workspace `Cargo.toml`:

```toml

# The trace gates (crates/flyvis/src/trace_checks.rs) step the 45,669-cell,
# 1.5M-synapse network 6,800 times per 1/200 run, twice per run. Unoptimised
# that is about 17 ms a step, two minutes a test; at opt-level 3 about
# 0.9 ms, with bit-identical states. No optimisation level contracts into a
# fused multiply-add or enables fast-math, and the exact gates re-prove it on
# every run. Only the two crates doing that arithmetic are optimised; dev
# builds otherwise keep the default.
[profile.dev.package.semblance-sim]
opt-level = 3

[profile.dev.package.semblance-flyvis]
opt-level = 3
```

  `Cargo.lock` does not change (`git diff --stat Cargo.lock` is empty).

- [ ] **Step 5: Measure "after".**

```bash
time SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- --exact trace_checks::trace_exact_run_3_000_1_50_f64 --nocapture
```

  Expected: PASS, the same line, finishing in about 2.7 s (measured 2.68 s).

- [ ] **Step 6: Run every trace test and read the output.**

```bash
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- trace_checks --nocapture 2>&1 | tee /tmp/trace.log | grep -E 'trace |mirrored|flashes at|moving edges at|test result'
```

  Expected: `22 passed; 0 failed`, in about 12 s. Specifically (measured):
  - every `trace exact run` line ends `12 of 12 sequences bit-equal, centre
    and snapshots`
  - the f32 `trace libm run` lines' worst ratios are 0.276 (run 0), 0.320
    (run 2), **0.861 (run 4, sequence 3, snapshots 8.801e-5)** and 0.222
    (run 6); the f64 runs' are 1.8e-5, 3.1e-5, 2.9e-3 and 3.1e-3
  - `mirrored mapping, run 3, sequence 8: snapshots max 2.50…`
  - flashes equal flyvis's input at both dts; moving edges' per-angle
    maxima at 0/30/90/150 are `2.897e-8 3.790e-3 1.961e-3 3.878e-3` at both
    dts, with 0° bit-equal to the f32 input

  Any failure: stop (see the stop rule).

- [ ] **Step 7: Run the whole workspace under the new profile.** The
  `opt-level` change applies to every test of the two crates, so all of them
  must still pass unchanged:

```bash
cargo clippy --workspace --all-targets -- -D warnings
time SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace 2>&1 | tee /tmp/workspace.log | grep 'test result' | awk '{p+=$4; f+=$6} END {print "passed", p, "failed", f}'
cargo run --quiet -- roundtrip
grep -c '^name = ' Cargo.lock
```

  Expected: clippy clean; `passed 373 failed 0` (the measured total with all
  tasks in; 41 more than the 332 before this plan), in about 30 s wall time;
  roundtrip reads `0.000000` for both distances; `8`.

- [ ] **Step 8: Commit,** with the measured times in the body:

```bash
git add crates/flyvis/src/trace_checks.rs crates/flyvis/src/lib.rs Cargo.toml
git commit -m "Gate the full traces under stimulus against flyvis" -m "Optimise semblance-sim and semblance-flyvis at opt-level 3 in dev builds (R5). trace_exact_run_3_000_1_50_f64: <before> s at opt-level 0, <after> s at opt-level 3; the 1/200 runs would take about 2 min each unoptimised."
```

  Replace `<before>` and `<after>` with the two `finished in` times from
  steps 3 and 5.

---

### Task 7: Documentation and counts (wave 4)

**Wave:** 4

**Owns:** `CLAUDE.md`, `README.md`

Spec section 6. Uses the measured results of Task 6.

**Files:**
- Modify: `CLAUDE.md`, `README.md`
- Local, not committed: the main checkout's
  `docs/project-outline/0.3.0-graded-dynamics/README.md`

**Interfaces:**
- Consumes: the merged branch with tasks 1–6.

- [ ] **Step 1: Measure the test and skip counts.** The skip count is the
  number of failures with `data` absent and every fixture required. In the
  worktree, `data` is a symlink; move it aside and put it back:

```bash
cargo test --workspace 2>&1 | grep 'test result' | awk '{p+=$4; f+=$6} END {print "passed", p, "failed", f}'
mv data data.measure-off
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --no-fail-fast 2>&1 | grep 'test result' | awk '{f+=$6} END {print "fixture-gated", f}'
mv data.measure-off data
ls data/   # flyvis-build flyvis-pretrained flyvis-reference flyvis-stimuli flyvis-supplementary
```

  Call the totals `TOTAL` and `SKIPS`. Measured while planning: `TOTAL` =
  373, `SKIPS` = 90 (66 before, plus the mapping gate, the real-manifest
  sequence test and the 22 trace tests). If yours differ, use yours and say
  why in the report. Confirm `data` is back before continuing.

- [ ] **Step 2: Update `CLAUDE.md`.**
  - `cargo test --workspace          # 332 tests, 0 failures` → `# TOTAL tests, 0 failures`.
  - `66 of those tests skip when` → `SKIPS of those tests skip when`.
  - `the suite reports the same "332 passed, 0 failed"` → `"TOTAL passed, 0 failed"`.
  - Replace

```markdown
or absent. They are only visible under `cargo test --workspace -- --nocapture`,
and even then not one per test: the settling gates in
`crates/flyvis/src/settling_checks.rs` share one lazily loaded fixture, so only
the test that loads it prints the notice and the other skipped ones print
nothing. Under `SEMBLANCE_REQUIRE_FIXTURES=1` every one of them still fails.
```

    with

```markdown
or absent. They are only visible under `cargo test --workspace -- --nocapture`,
one per skipped test: the settling and trace gates load their fixtures once
(`crates/flyvis/src/gate_support.rs`, `trace_checks.rs`), but every test that
finds one missing prints its own notice. Under `SEMBLANCE_REQUIRE_FIXTURES=1`
every one of them fails instead.
```

  - `which the settling gates read.` (after `data/flyvis-pretrained/ensemble.csv`,)
    → `which the settling and trace gates read.`
  - Replace the paragraph that starts **The settling gates are exact where
    exactness exists.** (through "…read the maximum itself there.") with:

```markdown
**The settling and trace gates are exact where exactness exists.**
`crates/flyvis/src/settling_checks.rs` settles the graded integrator for all
eight reference runs and runs two gates over them. The exact gate uses
`crates/flyvis/src/syn_count_torch.rs` (generated by
`tools/export_syn_count.py`: torch's own `exp` of every synapse count). It
requires every settled value, and every centre-neuron value at every settling
step, to equal flyvis's. The libm gate uses Rust's `exp`, and holds each run to
a bound fixed in the spec before it ran: 1e-10 in f64, and twice flyvis's own
f32/f64 gap in f32. torch's `exp` and the C library's differ in the last bit
on 104–106 of the 2,355 counts in f32 and 108 in f64. That is the only reason
the second gate has a bound. The trace gates, in
`crates/flyvis/src/trace_checks.rs`, continue each run from its settled state
through flyvis's twelve reference flashes and moving edges, fed flyvis's own
input through `wiring::scatter_hexals`, and hold every centre neuron at every
frame and every neuron at six snapshot frames to the same two rules (their
f32 bounds are twice flyvis's trace gaps). The worst f32 libm ratio, 0.86 in
run 4, is the one most likely to move on another platform's `exp`; that would
be a portability finding, not a reason to change the bound. If an exact gate
fails, find the discrepancy in flyvis's source; do not relax it. All the
gates print their results, with ratios in `{:.3e}`; read them.
`trace_checks.rs` also checks Rust's own flash and moving-edge generators
against flyvis's input: flashes exactly, edges within the geometry gate's
`TOLERANCE`, 0° edges bit for bit.
```

  - After the paragraph that starts "Clippy is clean, and CI enforces it"
    (ending "…they go when it does."), add:

```markdown
**`semblance-sim` and `semblance-flyvis` build at `opt-level = 3` in the dev
profile** (workspace `Cargo.toml`). The trace gates step the full network about
6,800 times per run; unoptimised that is about two minutes a test, optimised
a few seconds. It does not change results: rustc never contracts into a fused
multiply-add or enables fast-math at any level, and the exact gates re-prove
bit-equality on every run. Do not add `target-cpu`, fast-math or `mul_add`.
```

- [ ] **Step 3: Update `README.md`.**
  - `**Without fixture data, 66 flyvis parity tests skip` → `SKIPS`.
  - Replace

```markdown
printing a skip notice (the settling gates share one fixture, so they print a
single notice between them); libtest hides that notice for passing tests, so plain
```

    with

```markdown
printing a skip notice; libtest hides that notice for passing tests, so plain
```

  - In the crate table's `flyvis` row, built column, after "…at both time
    steps, in f32 and f64", add: ". Its trace gates continue from that
    settled state through flyvis's flashes and moving edges, fed through the
    input mapping (`scatter_hexals`, gated on flyvis's lattice), and require
    every centre neuron at every frame, and every neuron at six snapshot
    frames, to equal flyvis's bit for bit".
  - In the same row's open column, replace "the input mapping and full-trace
    gate under stimulus (0.3.0, in progress)" with "the result record (0.3.0,
    in progress)".
  - In the `sim` row, replace "reproducing flyvis's settling bit for bit" with
    "reproducing flyvis's settling and its traces under stimulus bit for bit".

- [ ] **Step 4: Verify.**

```bash
cargo build --workspace
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace 2>&1 | grep 'test result' | awk '{p+=$4; f+=$6} END {print "passed", p, "failed", f}'
cargo clippy --workspace --all-targets -- -D warnings
cargo run --quiet -- roundtrip
grep -c '^name = ' Cargo.lock
git status --short
```

  Expected: `passed TOTAL failed 0`; clippy clean; both roundtrip distances
  `0.000000`; `8`; only `CLAUDE.md` and `README.md` modified (and no `data`,
  no `docs/project-outline/`).

- [ ] **Step 5: Commit.**

```bash
git add CLAUDE.md README.md
git commit -m "Document the input mapping and the stimulus-trace gates"
```

- [ ] **Step 6: Update the epic's local outline (not committed).** In the
  main checkout, edit
  `<main checkout>/docs/project-outline/0.3.0-graded-dynamics/README.md`
  (gitignored; it does not exist in a worktree):
  - **Task 5** and **Task 6** headings: append ` — **DONE** (2026-10-29)`,
    and under each add one line of results: task 5, "`scatter_hexals`; the
    mapping gate holds for all 721 hexals in all 8 input types"; task 6, "8
    exact runs bit-equal in all 12 sequences; 8 libm runs within R3 (worst
    f32 ratio 0.861, run 4); mirror control 2.50; Rust flashes exact, edges
    within `TOLERANCE`, 0° bit-equal".
  - **Task 6's text**: replace "(generated by 0.2.0's Rust stimuli at the
    matching `dt`)" with "(driven by flyvis's own input, `input.f64.bin`;
    Rust's stimuli are checked against that input separately — ruling R1 of
    the stimulus-traces spec)".
  - **Decision 7**: replace its paragraph with: "Task 1 kept, per run and
    sequence, the 65 centre neurons at every frame and every neuron at six
    snapshot frames (`trunc(linspace(0, n - 1, 6))`), in both f32 and f64,
    plus the full settled state. The f32/f64 gap is measured and recorded in
    `PROVENANCE.json`, not stored as a trace."
  - **Task 1's bullet** "Confirm `bias` and `syn_strength` do not freeze …":
    replace its second sentence with: "`syn_strength` does pass through a
    clamp (`max(·, 0)`, non-negative): task 1's A8 found 919 exact zeros and
    none negative across the 50 models. `bias` has no clamp."
  - **Risks**, "The input mapping may be more than a mapping.": "Task 4
    assumes" → "Task 5 assumes", and "task 4 grows" → "task 5 grows".
