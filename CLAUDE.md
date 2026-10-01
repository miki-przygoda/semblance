# CLAUDE.md

Rust workspace for connectome compression research. Take a connectome-constrained
model that predicts real neural recordings, compress its representation, and
measure how far it can shrink before it stops behaving like the fly.

Current: **v0.1.1**, crate versions `0.1.0`. See "Version bumps" below — they
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
PyTorch checkpoints. The connectome itself needs none of it — that is read
directly in Rust. If a script there ever becomes something the Rust side needs at
*runtime*, the boundary has been broken. See `tools/README.md`.

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

**`crates/sim` is on the way out.** It models spiking neurons, and its trait
signatures encode that assumption (`step` returns `&[bool]`, `observe` takes
`&[bool]`, `Behaviour` counts spikes). The fly visual system is largely
non-spiking — graded membrane potential, not spikes. Replacing that trait
surface is planned work, so do not invest in extending `Lif`.

**Two `todo!()`s are the research, not bugs.** `ReducedCodec::encode`/`decode`
in `crates/codec` are deliberately unimplemented. Do not fill them in
opportunistically — that is an epic with its own design.

`semblance-codec 0.1.0` is published on crates.io with those panics in it. When
a real implementation ships, `cargo yank` that version.

---

## Verifying a change

```bash
cargo build --workspace
cargo test --workspace          # 202 tests, 0 failures
cargo clippy --workspace --all-targets
cargo run --quiet -- roundtrip  # both distances must read exactly 0.000000
```

26 of those tests skip when `data/flyvis-supplementary/`, `data/flyvis-build/`
and `data/flyvis-stimuli/` are absent (all three are gitignored real-data; the
first is fetched per `RESOURCES.md`, the other two produced by
`tools/export_build.py`, and `tools/export_stimuli.py` with
`tools/export_moving_edges.py` respectively). A skip prints
`eprintln!("skipping: ...")`, but libtest captures `eprintln!` for *passing*
tests, so under plain `cargo test --workspace` those notices are invisible —
the suite reports the same "202 passed, 0 failed" whether the data is present
or absent. They are only visible under `cargo test --workspace -- --nocapture`.

Set `SEMBLANCE_REQUIRE_FIXTURES=1` to turn every missing-fixture skip into a
hard failure instead — CI must set this, or the acceptance gates that prove
this workspace reproduces flyvis can pass while silently verifying nothing.

`roundtrip` reporting anything other than `0.000000` means a lossless round-trip
stopped being lossless — that is a real break, not a tolerance to adjust.

**One comparison in this workspace is tolerance-gated on a magnitude, and only
one.** The moving-edge geometry gate
(`moving_edge_offsets_match_flyvis_within_tolerance` in
`crates/flyvis/src/moving_edge_checks.rs`) is gated against flyvis at a
measured bound rather than exact equality, because flyvis rotates a `uint8`
raster through Pillow's bilinear resampler and this crate rotates the
sampling geometry instead — a decision recorded in
`docs/superpowers/specs/2026-09-30-moving-edge-stimuli-design.md`. That gate
prints its observed distribution; read it, do not just watch it pass. One
other test in that file, `the_ownership_map_agrees_with_flyviss_fp16_mask`,
also carries slack — it bounds how many pixels may disagree, not how far a
value may be off, because flyvis's mask is `float16` and this crate's is
`f64`. Every other comparison — the connectome's 1,513,231 edges, the flash
and dot families, the lattice order — is exact, element-wise equality, and
"gated against flyvis" means something stronger everywhere else.

Clippy currently has 5 pre-existing warnings (4 `needless_range_loop` in `sim`,
1 manual char comparison in `connectome`). Do not count those as new.

`demo` and `evolve` print numbers that depend on the RNG stream. They change
when the stream legitimately changes; they are not regression baselines.
