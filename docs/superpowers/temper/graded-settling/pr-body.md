## What changed

Piece 1 of epic 0.3.0 (graded dynamics), covering tasks 2, 3 and 4. It adds a
graded simulator and the first gate that holds it to flyvis's own output.
There is no version bump; that comes with piece 3.

- **`semblance_sim::graded`.** flyvis's passive point neurons with graded
  synapses, integrated by explicit Euler, generic over f32 and f64 through a
  sealed `Real` trait. It follows flyvis 1.2.0 operation for operation, so
  sums are associated the same way, it takes the reciprocal and then
  multiplies, it floors the time constant at `f32(dt)` and sums inputs in
  ascending edge order. It sits beside the spiking path and does not touch any
  of its traits. It refuses bad input with a typed `GradedError`: mismatched
  lengths, a non-finite parameter or count, or a `dt` that is not finite and
  positive in f64 or once rounded to f32. A non-finite state during a run
  fails the settling gate, which names the node, the cell type and the step.
- **`semblance_flyvis::ensemble`.** Parses `data/flyvis-pretrained/ensemble.csv`
  into `ModelParams` for all 50 trained models. The parse is strict and does no
  I/O. Every violation is an `EnsembleError` that names the line.
- **`semblance_flyvis::reference`.** Reads `data/flyvis-reference/manifest.json`
  and returns typed, shaped `f32`/`f64` arrays from its `.bin` blocks. It
  validates the block layout and checks it again before each read.
- **`semblance_flyvis::syn_count_torch`.** A generated table of the 2,355
  synapse counts exactly as torch's `exp` computes them, in f32 and f64. The
  new offline script `tools/export_syn_count.py` writes it.
- **Wiring.** `edge_syn_groups` returns each edge's synapse-count group and
  `input_nodes` returns the photoreceptor nodes. The sign is pinned to the
  checkpoint's.
- **Settling gates** (`crates/flyvis/src/settling_checks.rs`). Eight reference
  runs: models 000 and 049, dt 1/200 and 1/50, f32 and f64. Each run is
  checked by two gates:
  - **exact:** uses torch's counts. Every settled value and every
    centre-neuron value at every settling step must equal flyvis's.
  - **libm:** uses Rust's `exp`. Each run is held to a bound fixed in the spec
    before the gate ran: 1e-10 in f64, and 2x flyvis's own f32/f64 gap in f32.
- **Acceptance tests** for the integrator, the ensemble, the reference reader
  and the settling gates, written from the behaviour spec
  (`docs/superpowers/temper/graded-settling/behaviour.md`).
- **Docs.**
  - `CLAUDE.md`:
    - test and skip counts
    - "the spiking path" is now what is on the way out
    - a paragraph on the settling gates
    - `syn_count_torch.rs` is listed as generated
    - what reads `flyvis-reference/` and `ensemble.csv`
  - `README.md`: the skip count, and the `flyvis` and `sim` crate rows.
  - `tools/README.md`: the new exporter.

`semblance-flyvis` gains `semblance-sim` as a dev-dependency only.
`Cargo.lock` still lists 8 packages, all `semblance*`.

## Why

Everything after 0.3.0 reports numbers that this workspace's simulator
produces. If the simulator does not reproduce flyvis, a later size-vs-fidelity
curve measures the port's bugs instead. Settling (1.0 s of uniform grey from
the bias state) involves no stimulus geometry. A failure here is therefore a
failure of the dynamics, the parameters or the wiring.

The review found that the port reproduces flyvis bit for bit once it uses the
synapse counts flyvis itself computed. That is why the main gate is exact. The
libm gate exists because torch's `exp` and the C library's differ in the last
bit on 104–106 of the 2,355 counts in f32 and 108 in f64. That difference is
the only reason that gate has a bound.

## How it was tested

Measured on the final branch head, with fixtures present unless noted.

- `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace`: **332 passed, 0
  failed.**
- Without `data/`:
  - plain `cargo test --workspace`: 332 passed, 0 failed.
  - with `SEMBLANCE_REQUIRE_FIXTURES=1 --no-fail-fast`: 266 passed, 66 failed.
    All 66 are in `semblance-flyvis`: 52 in its unit tests, plus 1 in the
    ensemble acceptance tests, 1 in the reference acceptance tests and 12 in
    the settling acceptance tests. These are the 66 skips that `CLAUDE.md` and
    `README.md` now state.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo run --quiet -- roundtrip`: structural and behavioural distance both
  `0.000000`.
- `grep -c '^name = ' Cargo.lock`: 8.
- At the MVP checkpoint, `.temper/gate-full.sh` passed: CI's steps, plus the
  1.85 build and the 8-package check.
- At the checkpoint, `tools/export_syn_count.py` was rerun by hand. Its output
  was byte-identical, apart from the date issue in open item 3.
- At the checkpoint, `ci/fetch-fixtures.sh` reported: all 35 fixtures present
  and verified.

### Settling gate output

`SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib settling -- --nocapture --test-threads=1`
ran 20 tests: 20 passed in 37.3 s (debug build).

**Exact gate.** No run has a single differing value:

| Run | Model | dt | Precision | Settled differ | Settling centre differ |
|---|---|---|---|---|---|
| 0 | 000 | 1/200 | f32 | 0 of 45669 | 0 of 13000 |
| 1 | 000 | 1/200 | f64 | 0 of 45669 | 0 of 13000 |
| 2 | 000 | 1/50 | f32 | 0 of 45669 | 0 of 3250 |
| 3 | 000 | 1/50 | f64 | 0 of 45669 | 0 of 3250 |
| 4 | 049 | 1/200 | f32 | 0 of 45669 | 0 of 13000 |
| 5 | 049 | 1/200 | f64 | 0 of 45669 | 0 of 13000 |
| 6 | 049 | 1/50 | f32 | 0 of 45669 | 0 of 3250 |
| 7 | 049 | 1/50 | f64 | 0 of 45669 | 0 of 3250 |

**Libm gate.** Each run is shown against its bound. The f64 ratios print as
`0.000`, so this table gives the maxima for those runs.

| Run | Precision | Settled max (node, type) | Ratio | Centre max (step, type) | Ratio | Settled bit-equal |
|---|---|---|---|---|---|---|
| 0 | f32 | 4.768e-7 (15809, Mi4) | 0.169 | 4.768e-7 (3, TmY4) | 0.177 | 24041 of 45669 |
| 1 | f64 | 1.110e-15 (16054, Mi4) | 0.000 | 8.882e-16 (10, TmY14) | 0.000 | 23114 of 45669 |
| 2 | f32 | 8.345e-7 (15739, Mi4) | 0.327 | 1.192e-6 (8, TmY14) | 0.228 | 20211 of 45669 |
| 3 | f64 | 2.220e-15 (15993, Mi4) | 0.000 | 1.776e-15 (2, T5d) | 0.000 | 19628 of 45669 |
| 4 | f32 | 3.710e-6 (16134, Mi9) | 0.301 | 4.768e-7 (23, T5d) | 0.123 | 33295 of 45669 |
| 5 | f64 | 9.409e-15 (16134, Mi9) | 0.000 | 1.776e-15 (168, T4a) | 0.000 | 29371 of 45669 |
| 6 | f32 | 1.669e-6 (43787, TmY14) | 0.129 | 9.537e-7 (17, T5d) | 0.042 | 33987 of 45669 |
| 7 | f64 | 7.994e-15 (44066, TmY14) | 0.000 | 3.553e-15 (41, T4a) | 0.000 | 29426 of 45669 |

The four supporting tests and the two controls also pass.

## Open items

These come from `docs/superpowers/temper/graded-settling/log.md` and the docs
pass.

1. **Most settling skips print nothing.** When fixtures are absent, 18 of the
   20 settling tests skip without printing a notice, even under `--nocapture`.
   They share one `OnceLock` fixture, so only the test that loads it prints
   the notice. The spec's failure table expects each test to print one.
   `SEMBLANCE_REQUIRE_FIXTURES=1` still fails every one of them, so CI is not
   affected. This branch documents the behaviour in `CLAUDE.md` and
   `README.md`, but the code is unchanged.
2. **f64 libm ratios print as `0.000 of bound`.** The maxima are about 1e-15
   against a 1e-10 bound, so the ratio the docs say to read is hidden.
   `CLAUDE.md` now says to read the maximum for the f64 runs. The print format
   is unchanged.
3. **The generated table's header is dated.** `syn_count_torch.rs` records
   `exported <date>` in its header. Rerunning the exporter on a later day
   therefore changes the file even when every value is the same.
4. **The epic's local README and task files are not updated.** These are the
   files under `docs/project-outline/0.3.0-graded-dynamics/`, which are not in
   git. Spec section 7 asks for tasks 2, 3 and 4 to be marked done there, with
   the results above and F25's question answered (G16). The owner needs to do
   this locally.
5. **CI on this PR.** The self-hosted runner has not run on this PR yet. Merge
   waits for it and for the owner.

The crate-row gap from the checkpoint log (README's `flyvis` row not
mentioning the settling gate) is fixed in this branch.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
