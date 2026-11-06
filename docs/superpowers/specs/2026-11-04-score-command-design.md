# `semblance score`: score any network on both axes (gate F)

**Date:** 2026-11-04
**Status:** design approved by the owner in chat 2026-11-04; spec approved
2026-11-04.
**Epic:** 0.4.0 fidelity against recordings, task 5 —
`docs/project-outline/0.4.0-fidelity-against-recordings/` (not in git):
gate F; decision 4 and Q7 as revised 2026-11-04; decisions 3, 6 and 7.
**Builds on:**
[`2026-11-04-graded-responses-design.md`](2026-11-04-graded-responses-design.md),
[`2026-11-04-flyvis-index-export-design.md`](2026-11-04-flyvis-index-export-design.md),
[`2026-11-04-response-indices-design.md`](2026-11-04-response-indices-design.md),
[`2026-11-04-representational-axis-design.md`](2026-11-04-representational-axis-design.md),
and `validate`'s conventions in
[`2026-10-29-flyvis-parity-validate-design.md`](2026-10-29-flyvis-parity-validate-design.md).

## Why

0.5.0 will compress the 734 parameters and needs to ask, of each compressed
network, how much like the fly it still is. Gates A–D prove the scoring is
right on flyvis's own models; this piece makes the same scoring a command
that takes any network, so 0.5.0 has nothing to build but the codec.

## Goal

1. `semblance score`: reads a 734-vector, runs the flash and edge batteries,
   prints both axes, through exactly the code gates A–D run.
2. Gate F: tests that pin `score` to those gates' numbers.
3. The generator finding: how far Rust's own stimulus generators would move
   each value, measured once on model 000 and recorded.

## Decisions this rests on

- **Decision 4, revised 2026-11-04:** `score` plays fixtures-3's exported
  flyvis input, the battery gates A–D use, so a score reads directly against
  gate D's scale. Rust's generators are measured, not used for scoring.
- **Q7, revised 2026-11-04:** the input is a file in `ensemble.csv`'s long
  format (`model,kind,index,name,value`), 734 rows for one model, parsed by
  the existing `Ensemble::parse` with its checks.
- **Decision 6:** a network is a 734-vector over fixed topology. The rewired
  control's topology entry point stays internal to gate C.
- **Decision 7:** every component is printed separately; no combined scalar.

## Design

### 1. The command

```
semblance score FILE [--model ID] [--parent ID] [--data DIR] [--threads N]
                     [--markdown] [--recompute-scale]
semblance score --ensemble-model ID [same options]
```

- `FILE` holds one model, or several with `--model` picking one; several
  without `--model` is a usage error naming the ids found.
- `--ensemble-model ID` scores a trained member from `ensemble.csv`, no file.
- `--parent ID`: the model the CKA compares against; default `000`.
- `--data DIR`: fixtures, default `./data`, as `validate`.
- `--threads N`: the network and its parent run in parallel; with
  `--recompute-scale`, the 50 models.
- `--markdown`: the table as markdown, as `validate --markdown`.

Dispatched from `app/src/main.rs` beside `validate`, in a new
`app/src/score.rs`. `app` parses arguments and prints; every number comes
from `semblance_flyvis::parity` and `semblance_fidelity`.

**Exit codes.** 0: scored. 2: not scored, with the reason (a missing or
malformed fixture, a malformed or ambiguous input file, a bad command line,
or a network whose state went non-finite, named by stimulus, frame, node and
cell type). There is no 1: a score is not a gate.

### 2. The code path

Exactly what the gates run, in this order:

1. `Fixtures::load` and `IndexFixture::load` (task 2).
2. `respond(f, params, 1/200, F32, Counts::Torch, IndexBattery)` for the
   flash and the edge battery (task 1a), for the network and for the parent.
3. Task 3's `fri`, `dsi`, `preferred_direction`, `tuning_r`, and their
   agreement rules against `groundtruth.rs`.
4. Task 4's `ResponseMatrix` and `cka` against the parent.

A shared function in `parity`, `score_network(...) -> Result<Score,
ParityError>`, does steps 2–4, and gates A–D call it for their per-model
values, so gate F's identity is structural, not coincidental.

### 3. The output

One table:

| Row | Content |
|---|---|
| polarity | agreements of 32, and the types missed |
| DSI | per type over the 18, and the count above 0.357, labelled "ours: a per-network reading of the paper's threshold" (the paper's rule needs a population) |
| preferred direction | count of the 8 within 45°, and each angular error |
| tuning | r per motion type, at its best speed |
| CKA vs parent | the value, and where it falls in gate D's trained distribution: below the minimum, below the 5th percentile, below the median, or at or above it |

**The scale.** Gate D's minimum, 5th percentile, median and maximum are
registered constants in `parity` (`TRAINED_CKA`), set from the 0.4.0 record
when task 6 produces it. `app/tests/record.rs` requires each to appear in
the record's generated block, so they cannot drift from it. Until the
record exists they are absent, and `score` prints the CKA with "scale not
yet recorded". `--recompute-scale` runs all 50 models and prints the scale
from this machine instead, labelled as such.

### 4. Gate F

Fixture-gated tests in `app/tests/`:

- `score` on a file holding model 000 prints the same polarity, DSI,
  preferred-direction and tuning values gates A and B computed for 000, and
  CKA exactly `1.000000` against parent 000.
- `score --ensemble-model 049` prints task 3's values for 049.
- A file holding gate C's zeroed control for model 000 scores below the
  trained minimum.
- Usage and input errors exit 2 with pinned messages: a malformed file (the
  parser's own error), several models without `--model`, an unknown
  `--model`/`--parent`, a missing fixture (`how_to_get`'s remedy).
- `--markdown` output pinned for model 000.

### 5. The generator finding (record step, not a gate)

`validate --ensemble` also scores model 000 on batteries built from Rust's
own `Flashes` and `MovingEdge` generators (radius-6 flashes; 12 angles × 6
speeds × ON/OFF at height 80, `flyvis_defaults`), and prints, per index and
for CKA against model 000 on flyvis's input, the difference from the same
values on flyvis's input. The record states it as how far a
generator-driven score would move. Nothing gates on it: the geometry gate
already bounds the generator, by design (`CLAUDE.md`).

## Out of scope

- Scoring a `Latent` or anything from `codec`: 0.5.0.
- Topology-changing networks in `score` (decision 6).
- A combined score (decision 7).

## Versioning

`semblance` (the binary) gains a subcommand; `semblance-flyvis` gains
`score_network` and `TRAINED_CKA`. No version number changes in this piece.
