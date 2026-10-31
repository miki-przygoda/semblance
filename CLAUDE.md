# CLAUDE.md

Rust workspace for connectome compression research. Take a connectome-constrained
model that predicts real neural recordings, compress its representation, and
measure how far it can shrink before it stops behaving like the fly.

Current: **v0.3.1**, crate versions `0.3.1`. See "Version bumps" below — they
mean something specific here.

---

## Where things are

| Path | What it is | Read it when |
|---|---|---|
| `README.md` | what runs today, quickstart, crate table | orienting for the first time |
| `DESIGN.md` | the research framing and the gap being claimed | you need the *why* behind a crate |
| `RESOURCES.md` | link-checked prior art, grouped by which crate it maps to | looking for the paper behind a decision |
| `docs/project-outline/` | **gitignored.** Long-term direction, epics, version plan | deciding what to do next |
| `docs/superpowers/specs/` | design docs for individual pieces of work | implementing something already designed |
| `docs/superpowers/plans/` | step-by-step implementation plans | executing a plan task by task |
| `tools/` | offline Python (uv). **Not part of the workspace** | exporting something from flyvis |

`docs/project-outline/` is local-only and will not exist in a fresh clone. If it
is missing, the roadmap is missing — ask before inventing one.

---

## How work flows

```
docs/project-outline/README.md     what next, and why        (not in git)
        ↓  epic selected
docs/project-outline/<ver>-<epic>/ that epic broken into tasks (not in git)
        ↓  ready to build
docs/superpowers/specs/            how, exactly               (in git)
        ↓
docs/superpowers/plans/            step by step               (in git)
        ↓
commits
```

The outline answers *what next and why*; the specs and plans answer *how*. The
handoff runs one way. Git history records what was built; the outline records
why it was next.

Use the superpowers skills for this: `brainstorming` before design work,
`writing-plans` to turn a spec into a plan, `subagent-driven-development` or
`executing-plans` to run one.

---

## Version bumps

**One epic, one minor bump.** Each `0.x.0` corresponds to exactly one epic in
`docs/project-outline/`. The version is the epic's identity.

**An epic bumps only when it is finished** — every task complete, or explicitly
deferred or removed and recorded as such in that epic's README. "Mostly done"
is not a bump.

Do not bump a version because work happened. Tag intermediate milestones
instead (`v0.1.1` is one).

---

## Constraints that are easy to break by accident

**Zero external dependencies.** `Cargo.lock` contains exactly 8 packages, all
`semblance*`. This was deliberate and hard-won — do not add a crates.io
dependency without asking, not even a small one, not even for tests.

`tools/` is not an exception to this. It is offline Python, run by hand, that
exports flyvis artifacts into plain files; no crate imports it, invokes it, or
records it in `Cargo.lock`. It exists because flyvis's trained parameters ship as
PyTorch checkpoints. `crates/flyvis/src/pretrained.rs` and `syn_count_torch.rs`
are its output, compiled in: regenerate them only by rerunning their scripts.
The connectome itself needs none of it — that is read
directly in Rust. If a script there ever becomes something the Rust side needs at
*runtime*, the boundary has been broken. See `tools/README.md`.
`semblance validate` reads the exported files under `data/` and nothing
else: it never runs `tools/` or git.

**`semblance-rng`'s stream is frozen by contract.** `crates/rng/tests/vectors.rs`
pins exact outputs. Changing the algorithm, the constants, the number of draws a
method consumes, or the order of draws within a method is a breaking change
requiring a major bump. If those vectors fail, do not update them to match —
find out what moved.

One caveat already documented in the crate: `gaussian()`'s bit-exactness depends
on the host libm, so a failure confined to the gaussian values on an unusual
platform is a portability issue, not a broken stream.

**`Cargo.lock` is committed** and must stay that way — the workspace ships a
binary.

**Rust 1.85+, edition 2024.** `gen` is a reserved keyword; `rand`-style APIs that
use it will not compile.

**Commit style: plain imperative subject, no `feat:`/`fix:`/`chore:` prefixes.**
Match the existing log.

---

## Two things about the current code

**The spiking path in `crates/sim` is on the way out.** `Lif`, `Engine` and
the traits around them model spiking neurons, and their signatures encode that
assumption (`step` returns `&[bool]`, `observe` takes `&[bool]`, `Behaviour`
counts spikes). The fly visual system is largely non-spiking — graded
membrane potential, not spikes — which `semblance_sim::graded` implements,
beside the spiking path and touching none of its traits. Replacing the
spiking trait surface is planned work, so do not invest in extending `Lif`.

**Two `todo!()`s are the research, not bugs.** `ReducedCodec::encode`/`decode`
in `crates/codec` are deliberately unimplemented. Do not fill them in
opportunistically — that is an epic with its own design.

`semblance-codec` 0.3.1 still contains both `todo!()`s; its description says
so. The six 0.1.0 releases (semblance, -codec, -connectome, -evo, -fidelity,
-sim) are to be yanked once all eight 0.3.1 crates are live: they predate
flyvis and graded dynamics, evo 0.1.0 depends on `rand`, and yanking codec
alone would strand evo, fidelity and semblance 0.1.0. Publishing and yanking
wait for the owner's go-ahead. A real `ReducedCodec` later needs no yank of
its own.

---

## Verifying a change

```bash
cargo build --workspace
cargo test --workspace          # 568 tests, 0 failures
cargo clippy --workspace --all-targets -- -D warnings
cargo run --quiet -- roundtrip  # both distances must read exactly 0.000000
cargo run --release -- validate # exit 0: every parity gate and control holds
```

172 of those tests skip when `data/flyvis-supplementary/`, `data/flyvis-build/`,
`data/flyvis-stimuli/`, `data/flyvis-reference/` and
`data/flyvis-pretrained/ensemble.csv` are absent (all five are gitignored
real-data; the first is fetched per `RESOURCES.md`, the next two produced by
`tools/export_build.py`, and `tools/export_stimuli.py` with
`tools/export_moving_edges.py` respectively; `tools/export_reference.py`
writes `data/flyvis-reference/` and also the three height-80 `_h80`
moving-edge files; `ensemble.csv` comes from `tools/export_params.py`).
A skip prints `eprintln!("skipping: ...")`, but libtest captures `eprintln!`
for *passing* tests, so under plain `cargo test --workspace` those notices are invisible —
the suite reports the same "568 passed, 0 failed" whether the data is present
or absent. They are only visible under `cargo test --workspace -- --nocapture`,
and even then not quite one per skipped test. Fixtures loaded once and shared
-- the settling and trace gates' (`crates/flyvis/src/gate_support.rs`,
`trace_checks.rs`) and the acceptance tests' in
`crates/flyvis/tests/acceptance_settling.rs` and `acceptance_traces.rs` --
cache the miss, not the notice, so every test that finds one missing prints
its own (`acceptance_traces.rs`'s
`every_fixture_gated_acceptance_test_prints_its_own_skip_notice` holds the
acceptance files to that). A test that reads more than one missing fixture
prints more than one: 166 of the 172 print a notice, 178 notices in all
under `--nocapture --test-threads=1`. The other six -- `validate_02`, `_07`,
`_11`, `_12`, `_13` and `_20` in `app/tests/acceptance_validate.rs` -- share
`good()`'s cached run, which prints its notice only for the first test to
ask. Under `SEMBLANCE_REQUIRE_FIXTURES=1` every one of the 172 fails
instead.

Set `SEMBLANCE_REQUIRE_FIXTURES=1` to turn every missing-fixture skip into a
hard failure instead — CI sets this (`.github/workflows/ci.yml`); anywhere else
that runs the suite must too, or the acceptance gates that prove
this workspace reproduces flyvis can pass while silently verifying nothing.

To get the fixtures without installing Python, flyvis or PyTorch, run
`ci/fetch-fixtures.sh`. It needs `gh` access to this repo, fetches only what
is missing, and verifies every file against `ci/fixtures.sha256`. It never
overwrites a file that fails verification. Since `fixtures-2` it also fetches
`data/flyvis-reference/` (flyvis's reference traces, from
`tools/export_reference.py`) and `data/flyvis-pretrained/ensemble.csv`,
which the settling and trace gates read. CI runs the same script, then the
suite with `SEMBLANCE_REQUIRE_FIXTURES=1`.

`roundtrip` reporting anything other than `0.000000` means a lossless round-trip
stopped being lossless — that is a real break, not a tolerance to adjust.

**`semblance validate` is the gates, as a command.** `cargo run --release --
validate` (fixtures under `./data`, or `--data DIR`) runs every settling and
trace gate and the four controls through `semblance_flyvis::parity`, the same
code the `#[cfg(test)]` gates call, and prints one table. Exit 0: every exact
row is bit-equal and every libm row within its pre-registered bound. Exit 1: a
gate or control failed. Exit 2: verification did not complete (a fixture
missing, unreadable, malformed or mismatched, or a bad command line); nothing
counts as verified. The exact rows are bit-equal only because
`syn_count_torch.rs` compiles in torch's own `exp` of the synapse counts.

**The record, `docs/results/0.3.0-flyvis-parity.md`,** is a hand-written
preamble followed by `validate --markdown`'s output from hosted CI, held to it
by `app/tests/record.rs`. Only the head regenerates it, from CI's
`parity-record` artifact (the "Validate, writing the record's generated
block" step), never by hand or on a workstation; every number the preamble
quotes must occur in the generated block. CI sets
`SEMBLANCE_REQUIRE_LIBM_RECORD=1`, so there the libm section must match byte
for byte. Elsewhere a libm fingerprint that differs from the record's skips
that section, and its `record: libm sections not compared` notice is visible
only under `--nocapture`; everything else is still compared.

**The moving-edge geometry gate is not precedent for loosening a comparison
that could be exact.** `moving_edge_offsets_match_flyvis_within_tolerance`,
in `crates/flyvis/src/moving_edge_checks.rs`, is bounded rather than exact
because flyvis rasterises into `uint8` and rotates through Pillow's bilinear
resampler, while this crate rotates the sampling geometry instead — a
decision recorded in
`docs/superpowers/specs/2026-09-30-moving-edge-stimuli-design.md`. The two
renderers are not computing the same function, so that bound would still be
there on a machine with perfect arithmetic: it is not a precision artifact,
and it does not get tighter by trying harder. That gate prints its observed
distribution; read it, do not just watch it pass. The same file's
`the_ownership_map_agrees_with_flyviss_fp16_mask`, by contrast, is exact:
flyvis builds that mask in `float16`, and `crates/flyvis/src/hex_eye.rs`
emulates that construction operation by operation, so every pixel must agree.
It once bounded how many pixels could disagree, while the crate built the
mask in `f64` -- and that tolerated difference turned out to be what failed
the geometry gate at bar height 80.

**The settling and trace gates are exact where exactness exists.**
`crates/flyvis/src/settling_checks.rs` settles the graded integrator for all
eight reference runs and runs two gates over them. The exact gate uses
`crates/flyvis/src/syn_count_torch.rs` (generated by
`tools/export_syn_count.py`: torch's own `exp` of every synapse count). It
requires every settled value, and every centre-neuron value at every settling
step, to equal flyvis's. The libm gate uses Rust's `exp`, and holds each run to
a bound fixed in the spec before it ran: 1e-10 in f64, and twice flyvis's own
f32/f64 gap in f32. torch's `exp` and the C library's differ in the last bit
on 104–106 of the 2,355 counts in f32 and 108 in f64 on the author's Mac, and
on 104 in each on CI's hosted Linux. That is the only reason
the second gate has a bound. The trace gates, in
`crates/flyvis/src/trace_checks.rs`, continue each run from its settled state
through flyvis's twelve reference flashes and moving edges, fed flyvis's own
input through `wiring::scatter_hexals`, and hold every centre neuron at every
frame and every neuron at six snapshot frames to the same two rules (their
f32 bounds are twice flyvis's trace gaps). The worst f32 libm ratio, in run 4,
is the one most likely to move on another platform's `exp`, and it already
does: 0.86 on the author's Mac, 7.428e-1 in the committed record from CI's
hosted Linux. Either way that is a portability finding, not a reason to change
the bound. If an exact gate
fails, find the discrepancy in flyvis's source; do not relax it. All the
gates print their results, with ratios in `{:.3e}`; read them.
`trace_checks.rs` also checks Rust's own flash and moving-edge generators
against flyvis's input: flashes exactly, edges within the geometry gate's
`TOLERANCE`, 0° edges bit for bit.

Elsewhere, `grep '\.abs() <' crates/*/src/*.rs` turns up other non-exact
float comparisons, for reasons specific to each one. Most are round-trip
epsilons on values that are supposed to be numerically identical — a value
re-read through flyvis's own `f32` export, say, or a closed-form check
against flyvis's published formula — and those are precision artifacts, not
evidence of a different algorithm; they should get tighter over time, not
looser. A few, in `semblance-rng`, bound how closely a statistic converges to
its true value over a finite number of random draws, which cannot be driven
to zero no matter how precise the arithmetic — a different question again.
Neither kind, and nothing about the geometry gate above, is reason to widen
a comparison that could be exact instead.

Clippy is clean, and CI enforces it with `-D warnings` on a pinned toolchain
(see `.github/workflows/ci.yml`). The three `needless_range_loop` sites in `sim`
carry scoped `#[allow]`s rather than rewrites, because `sim`'s spiking path is
on the way out; they go when it does.

**`semblance-sim` and `semblance-flyvis` build at `opt-level = 3` in the dev
profile** (workspace `Cargo.toml`). The trace gates step the full network about
6,800 times per 1/200 run (about 1,700 per 1/50 run); unoptimised a 1/200
run takes about two minutes, optimised a few seconds. It does not change results: rustc never contracts into a fused
multiply-add or enables fast-math at any level, and the exact gates re-prove
bit-equality on every run. Do not add `target-cpu`, fast-math or `mul_add`.

`demo` and `evolve` print numbers that depend on the RNG stream. They change
when the stream legitimately changes; they are not regression baselines.
