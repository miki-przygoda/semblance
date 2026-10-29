## What changed

Piece 2 of epic 0.3.0 (graded dynamics), covering tasks 5 and 6. It drives the
graded simulator with flyvis's own stimuli and holds the full traces under
stimulus to flyvis's output. There is no version bump; that comes with piece 3.

- **The input mapping** (`semblance_flyvis::wiring::scatter_hexals`). It
  writes a hexal frame onto the photoreceptor nodes, `input_nodes[k][h]` for
  hexal `h`. It validates the frame and the node indices before writing, so an
  error leaves the output untouched.
- **The sequence reader** (`reference::parse_sequences`). Reads the reference
  manifest's 12 sequences: family, flyvis index, arguments, and per dt the
  frame count, stimulus window and snapshot frames. A malformed entry is a
  typed `ReferenceError` (including the new `FrameRange`).
- **`Graded::set_state`**, so a trace starts from Rust's own settled state.
- **The trace gates** (`crates/flyvis/src/trace_checks.rs`). For each of the
  eight reference runs and each of the 12 sequences, the network plays
  flyvis's recorded input (`input.f64.bin`, narrowed `as f32` in f32 runs) and
  is compared on the centre column at every frame and on six full-network
  snapshots:
  - **exact:** torch's counts; every value must equal flyvis's.
  - **libm:** Rust's `exp`; held to bounds fixed in the spec before the gate
    ran: 1e-10 in f64, 2x flyvis's own f32/f64 centre and snapshot gaps in f32.
- **The stimulus check.** Rust's own flash and moving-edge generators against
  flyvis's recorded input: flashes must be exact, the 0 degree edges
  bit-equal to `x as f32`, other angles within the existing moving-edge
  `TOLERANCE` (0.005, now shared rather than copied).
- **PR #12's minor items:** typed fixture errors, `{:.3e}` ratios, an undated
  generated header, the `f32::is_normal` dt rule, NaN reported as an infinite
  error, relu passing NaN through like torch, and the shared gate machinery
  moved into `gate_support`.
- **`opt-level = 3`** for `semblance-sim` and `semblance-flyvis` in the dev
  profile: a 1/50 trace run goes from 31 s to 2.7 s. It does not change
  results (no FMA contraction or fast-math at any level), and the exact gates
  re-prove bit-equality on every run.
- **Acceptance tests** written from the behaviour spec
  (`docs/superpowers/temper/stimulus-traces/behaviour.md`), and docs.

## Results

- **Exact gate:** all 8 runs bit-equal to flyvis in all 12 sequences, centre
  and snapshots.
- **libm gate:** worst f32 ratio to bound 0.861 (run 4, snapshots, 8.80e-5,
  sequence 3, frame 359, node 15414, Mi4); other f32 runs 0.276, 0.320, 0.222.
  f64 runs at or below 3.1e-3 of bound.
- **Controls:** a mirrored input mapping moves the snapshots by 2.50 and the
  centre by 2.29 (must exceed 1e-3), so the gate sees a wrong mapping.
- **Stimulus check:** flashes exact at both dts; moving edges at most
  2.9e-8, 3.79e-3, 1.96e-3 and 3.88e-3 at 0/30/90/150 degrees, with 0 degrees
  bit-equal to flyvis's f32 input.
- **Suite:** 429 tests, 0 failures with `SEMBLANCE_REQUIRE_FIXTURES=1`; 123
  of them are fixture-gated and print a skip notice when `data/` is absent.

## Review

A four-lens sweep (security, failure paths, test strength and docs, numerical
fidelity to flyvis) found 7 issues, none refuted, all fixed:

- Acceptance tests read git history (`git show`/`git archive` of base
  commits), which fails or silently skips in CI's depth-1 clone. Tests no
  longer touch git; "unchanged since the base commit" is a review check.
- MINORS-05 ran `tools/export_syn_count.py` through the tools' Python,
  breaking the rule that no crate invokes `tools/`. It now checks only the
  tree; the byte-for-byte rerun is an owner step (`owner-steps.sh syn-count`).
- A false comment on why a subnormal f32 dt is refused (the refusal itself is
  unchanged), stale test counts, and missing per-test skip notices.

A second sweep found 4 minor items, fixed by the head: a Drop guard in
TRACES-19's shadow edit, the owner step comparing against `HEAD`, the step
count per dt in CLAUDE.md, and README's skip-notice wording.

No bound, gap constant, `==` or `TOLERANCE` changed. `Cargo.lock` still has 8
packages.

## Open items

None from this piece. Carried from piece 1 (owner): the integer-count control
covers the f64 libm bound only; the f32 libm bound misses that error class in
3 of 4 runs (the exact gate still catches it).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
