# Retiring the spiking path

**Date:** 2026-11-04
**Status:** design approved by the owner in chat 2026-11-04; spec awaiting
the owner's review.
**Epic:** 0.4.0 fidelity against recordings, task 1b —
`docs/project-outline/0.4.0-fidelity-against-recordings/` (not in git):
decision 5, `task-1-trait-migration.md` (its inventory and stop rule), and
Q6 as revised 2026-11-04.
**Runs after:** task 5
([`2026-11-04-score-command-design.md`](2026-11-04-score-command-design.md)).
Decision 5 orders it last among the code tasks.

## Why

`CLAUDE.md` has said since 0.3.0 that the spiking path is on the way out: the
fly's visual system is graded, and `semblance_sim::graded` replaced it for
everything that matters. After tasks 1a–5, nothing in the research path uses
it. Leaving it costs a misleading `roundtrip` line, a toy `evolve`, two
fidelity functions that score toward perfect, and a `CLAUDE.md` caveat.

## Goal

Remove the spiking surface and everything built only on it, resolve `demo`,
`roundtrip` and `evolve` per Q6 as revised, and update the docs, without
touching anything the stop rule protects.

## Facts the design rests on

Re-checked at `1b43c0e`; the plan's first step re-checks them at its own
base, since tasks 1a–5 land first.

| Fact | Source |
|---|---|
| The spiking surface: `NeuronModel`, `Lif`, `Engine`, `Stimulus`, `Readout`, `Behaviour`, `ConstantDrive`, `SpikeReadout` | `crates/sim/src/lib.rs`; `task-1-trait-migration.md` |
| Three scoped `#[allow(clippy::needless_range_loop)]` on the spiking path | `crates/sim/src/lib.rs:75,158,241` |
| `fidelity` imports `Behaviour, Engine, Lif, Readout, Stimulus` from `sim` and `Codec, Latent` from `codec`; `structural_distance` needs only `Connectome` | `crates/fidelity/src/lib.rs:21-23,32` |
| `roundtrip` prints a structural and a behavioural distance; the behavioural one runs the spiking `Engine` on a network and its lossless decode | `app/src/main.rs:98-121` |
| `evolve` uses `RateTargetFitness` over `run_probe`; `evo` depends only on `codec` and `rng` | `app/src/main.rs:123-140`; `crates/evo/Cargo.toml` |
| No acceptance test references the spiking names | `task-1-trait-migration.md` (grep under `crates/*/tests/`) |

## Design

### 1. `semblance-sim`

Remove the eight items above, their tests, and the three `needless_range_loop`
allows. The crate is then `graded` (0.3.0) and `response` (task 1a). Its
description in `Cargo.toml` and its crate docs say so.

### 2. `semblance-fidelity`

Remove `Probe`, `run_probe`, `compare_behaviour`, `behavioural_distance`,
`ProbeBattery`, `DriveProbe`, `size_fidelity_curve`, `CurvePoint`, and their
tests, including task 3's legacy gate E fixes and the tests that pinned them
(the functions they guarded are gone; the new path's refusal tests, which
cover the same failure modes, stay). Keep `structural_distance` (used by
`roundtrip`), `indices` (task 3) and `similarity` (task 4).

`semblance-codec` leaves `fidelity`'s dependencies if nothing left uses it;
`Cargo.lock` must still list exactly the 8 `semblance*` packages.

### 3. `app`

- **`roundtrip`** (Q6, revised): encodes and decodes with `WeightCodec` and
  prints the latent dimension and the structural distance, which must read
  exactly `0.000000`. In place of the behavioural line, one line: behaviour
  is measured once 0.5.0 maps a `Latent` to a 734-vector (decision 6). Runs
  without fixtures, as today.
- **`demo`** (Q6): loads the fixtures, settles model 000 at `dt = 1/200`,
  f32, torch counts, and plays one ON and one OFF radius-6 flash from
  fixtures-3's flash battery through `parity::respond`. Prints the central
  responses of a handful of types (R1, L1, Mi1, Tm3, T4a, T5a) at a few
  frames, and their FRI. Missing fixtures: exit 2 with `how_to_get`'s
  remedy. Its output is pinned by a fixture-gated test.
- **`evolve`**: removed, with `RateTargetFitness`. The `evo` crate is
  untouched; evolution returns to `app` after 0.6.0.
- The usage text lists the commands that remain.

### 4. Docs

- `CLAUDE.md`: "Verifying a change" says `roundtrip`'s structural distance
  must read exactly `0.000000`; "Two things about the current code" loses
  the spiking paragraph and the `needless_range_loop` note; the clippy
  paragraph loses its mention of the allows; the test and skip counts are
  updated.
- `README.md` and `DESIGN.md`: mentions of the spiking path, `Lif`,
  `evolve` and the behavioural line are updated or removed.
- `crates/*/Cargo.toml` descriptions for `sim` and `fidelity`.

### 5. The stop rule

Unchanged from `task-1-trait-migration.md`. Stop, record the deferral in
the epic README, and move the remainder to its own epic or
`chore-and-fixes/` if this piece needs any of:

- a change to `codec`'s `Codec` trait or `Latent`;
- a change to `connectome` or `flyvis` public items;
- a change to any 0.3.0 gate, bound or acceptance string;
- a substitute for `roundtrip`'s lossless check that cannot run in CI.

The revised `roundtrip` keeps the lossless check and needs no fixtures, so
the last point does not trip.

## Proof

- `grep -rn` for each removed name across `crates/`, `app/` and the docs
  finds only historical mentions (specs, plans, the 0.3.0 record).
- `cargo test --workspace` passes with `SEMBLANCE_REQUIRE_FIXTURES=1`; every
  acceptance test passes unchanged.
- Clippy `-D warnings` is clean with no `allow`s added.
- `roundtrip`'s structural distance reads `0.000000`; `validate` exits 0 with
  its output unchanged.
- `Cargo.lock` lists exactly 8 packages.

## Versioning

Breaking for `semblance-sim` and `semblance-fidelity`, inside 0.4.0's minor
bump. No version number changes in this piece; task 6 bumps.
