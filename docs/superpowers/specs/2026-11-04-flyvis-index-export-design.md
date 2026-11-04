# Exporting flyvis's index values: `tools/export_indices.py` and `fixtures-3`

**Date:** 2026-11-04
**Status:** design approved by the owner in chat 2026-11-04 (run on beast as
a jobq job, owner's call); spec approved 2026-11-04.
**Epic:** 0.4.0 fidelity against recordings, task 2 —
`docs/project-outline/0.4.0-fidelity-against-recordings/` (not in git).
Owner's answers Q2 and Q4; epic decisions 1 and 4.
**Builds on:**
[`2026-10-26-flyvis-reference-traces-design.md`](2026-10-26-flyvis-reference-traces-design.md)
(`export_reference.py`, whose checks and layout this follows) and
[`2026-11-04-graded-responses-design.md`](2026-11-04-graded-responses-design.md)
(task 1a's `Battery`, which this piece implements over the exported input).

## Why

Gate A says every index Rust computes equals the value flyvis's own analysis
code computes for the same model, fed the same input: exact where the
traces are bit-equal, otherwise within a bound fixed beforehand at twice
flyvis's own f32/f64 gap. That needs, once, offline:

- flyvis's index values, from its own code;
- the input flyvis fed, for the full battery (0.3.0's 12 reference sequences
  are a small subset), since Rust's rotated edges agree with flyvis's only
  within the geometry gate's tolerance (epic decision 4);
- each index's f32/f64 gap, for the bounds.

**Revised 2026-11-04, after the plan's smoke run** (owner's decision): the
export is about 25× slower than estimated here (about 55 min per model for
both precisions on 8 cores), so it exports **all 50 models at f32 and models
000 and 049 at f64**, as parallel shards merged at the end. Every gate's use
is covered: gate A's f64 rows and the f32/f64 gaps come from 000 and 049;
the all-50 gate A and gate B's cross-check run at f32. Where the sections
below say "50 × 2", read this. Details: the plan's amendment.

**Revised again 2026-11-04, after Task 3's verification run** (owner's
decision): beast's torch (2.14.0+cu130, x86_64, MKL) does not reproduce the
Mac's (2.14.0, arm64), the machine fixtures-2 and every exact 0.3.0 gate
came from: model 000's settled state differs by up to 4.8e-7 at f32 (from
settling step 1) and 9.4e-16 at f64 (from step 0). The export runs on beast
anyway, and check 3 and gate A's exact rows become bounded by 0.3.0's libm
rule: **1e-10 at f64, twice flyvis's own f32/f64 gap at f32**. Check 3 takes
that gap from fixtures-2's own `settled`/`centre` pair; gate A takes it from
`PROVENANCE.json`, which now records the gap for the peaks too. Every check 3
difference is printed and recorded in `PROVENANCE.json`, and the finding goes
in the epic README and the 0.4.0 record. Rust's bit-equality with flyvis on
the Mac is unchanged: 0.3.0's gates against fixtures-2 still prove it. Where
check 3 or §4 below says "bit for bit" or "rerun on the owner's Mac", read
this.

## Goal

1. `tools/export_indices.py`: flyvis 1.2.0's own index pipeline for all 50
   models at f32 and f64, `dt = 1/200`, checked in memory before anything is
   written.
2. `data/flyvis-indices/`: the input as distinct frames plus a per-stimulus
   index, the indices, their per-stimulus peaks, a manifest and provenance.
3. A Rust reader and an `IndexBattery` over that input, with fixture-gated
   structure tests. The index gates themselves are task 3.
4. `fixtures-3`: `fixtures-2` plus `flyvis-indices/`. Uploading waits for
   the owner's sign-off.

## Facts the design rests on

From task 0 (`task-0-facts.md`, local), each with its flyvis 1.2.0 source
line there, and from measurement on 2026-11-04.

| Fact | Source |
|---|---|
| Flash battery: radius −1 and 6, intensities 0 and 1, on grey 0.5, `t_pre` = `t_stim` = 1 s: 4 samples of 600 frames. Only radius 6 feeds the FRI | task 0, FRI |
| FRI window frames 200..=400 (the `-0.005` query's float rounding drops 199); shift by `abs(min)` over window and both intensities; raw peaks; `(on-off)/(on+off+1e-16)` stored f32 | task 0, FRI |
| Edge battery: 12 angles × ON/OFF × 6 speeds, height 80: 144 stimuli, padded with NaN to 1078 frames; valid lengths 1078 / 739 / 567 / 525 / 485 / 465 by speed (found while planning; the export stores valid frames only). Windows per speed end at 878 / 539 / 367 / 325 / 285 / 265 | task 0, DSI |
| DSI's denominator is shared by intensity and the DSI is averaged over speeds; preferred direction is the angle of the un-normalised vector sum over speeds; tuning r is per speed, best kept | task 0, DSI and tuning |
| The index pipeline initialises exactly as 0.3.0's reference does (`steady_state`, 1 s of 0.5, `t_fade_in = 0`) | task 0, FRI |
| A speed-13 moving edge in `flyvis-reference/input.f64.bin` has 22 distinct frames of 525; a flash 2 of 600. The edge values are not all f32-representable; the flash values are | measured on `fixtures-2`, 2026-11-04 |
| beast: `~/.local/bin/uv`, PyPI reachable, Python 3.14 system (flyvis needs <3.13; `uv` fetches 3.12 per `.python-version`), checkpoints at `/srv/bulk/Projects/semblance/data/flyvis-pretrained/results/flow/0000/000`..`049`, 201 GB free on `/srv/fast`. Linux `torch 2.14.0` from `uv.lock` pulls about 43 `nvidia-*` packages | checked 2026-11-04 |

## Design

### 1. The script

`tools/export_indices.py`, run by hand, never by a crate or CI. It imports
`export_reference.py`'s helpers rather than copying them: `preflight`,
`acquire_lock`, `parse_pretrained`, `read_ensemble`, `load_checkpoint`,
`check_checkpoints`, `copy_models`, the scratch-root handling, `_write_synced`
and the manifest validator's rules. Where a helper is hard-wired to
`flyvis-reference` (paths, the eight runs), it gains a parameter;
`test_export_reference.py` must still pass unchanged.

Per precision (all f32 work, then the default-dtype switch, then all f64, as
`export_reference.py` does), per model `000`..`049`:

1. Fresh `NetworkView` and network from the copied checkpoint; its
   parameters equal `ensemble.csv` bit for bit.
2. Through flyvis's own `stimulus_responses` path: the flash battery and the
   moving-edge battery, from `steady_state(t_pre=1.0, value=0.5)`.
3. Central-column responses reduced as flyvis does; then flyvis's
   `flash_response_index(radius=6)`, the moving-edge peaks,
   `direction_selectivity_index`, the preferred direction, the tuning
   correlations, and `fri_correlation_to_known`,
   `dsi_correlation_to_known`, `angular_distance_to_known`.
4. The intermediate peaks are captured from the same objects flyvis's index
   functions consume, not recomputed by the script.

### 2. Checks, all before anything is written

Each a `require(...)` that names the model, precision and stimulus.

1. **Parameters**: every model's network parameters equal `ensemble.csv`.
2. **Input**: the frames flyvis fed each stimulus equal the frames rebuilt
   from `frames.f64.bin` and `frame_index.u32.bin`, bit for bit.
3. **The link to 0.3.0**: model 000's settled state, and its central traces
   on every stimulus shared with `flyvis-reference` (the flashes it holds and
   the speed-13 edges), equal `fixtures-2`'s `settled.*` and `centre.*` bit
   for bit, at both precisions. This proves the index pipeline is the
   pipeline 0.3.0's gates already reproduce, **and that this machine's torch
   computes what the Mac's did**.
4. **Windows**: the FRI window is frames 200..=400 and each speed's edge
   window is as tabled above, read from flyvis's own time axis and masks.
5. **Threads**: model 000 at each precision gives bit-identical indices and
   peaks at 1 thread and at the job's thread count.
6. **Shape and finiteness** of every array, and the stimulus table agreeing
   with flyvis's dataset (angles, speeds, intensities, frame counts).

### 3. Output: `data/flyvis-indices/`

Raw little-endian, C order, no header; `manifest.json` lists each file's
blocks, as in `flyvis-reference`. `p` is `f32` or `f64`; models in file
order `000`..`049`; types in network order (65).

| File | Shape | Content |
|---|---|---|
| `frames.f64.bin` | [distinct, 721] | every distinct input frame across both batteries, once, in first-use order |
| `frame_index.u32.bin` | one block per stimulus, [n_frames] | row of `frames` for each frame |
| `fri_peaks.p.bin` | [50, 65, 2] | radius-6 shifted peaks, (off, on) |
| `fri.p.bin` | [50, 65] | FRI |
| `edge_peaks.p.bin` | [50, 144, 65] | windowed, clipped peak per edge stimulus |
| `dsi.p.bin` | [50, 2, 65] | DSI per intensity (0, 1) |
| `pd.p.bin` | [50, 2, 65] | preferred direction, radians, per intensity |
| `tuning_r.p.bin` | [50, 8, 2, 6] | r per motion type (`MOTION_TUNING` order), intensity, speed |
| `to_known.p.bin` | blocks per helper | the three `*_to_known` outputs per model |
| `manifest.json` | | stimulus table (battery, family, angle, speed, intensity, radius, n_frames, window), central nodes, files and blocks with sha256; deterministic |
| `PROVENANCE.json` | | host, flyvis/torch/numpy versions, `uv.lock` sha256, run time, thread counts, and per index the max f32-vs-f64 gap over all models and entries |

Expected size: about 18 MB of input, a few MB of indices.

### 4. Where it runs: beast, as a jobq job

The owner's call: beast is idle.

- In a worktree under `~/work/semblance/` on the branch carrying the
  script, `uv sync --project tools` (fetches Python 3.12 and the locked
  wheels; uv's cache stays in `~/.cache/uv`). The sync is itself a jobq job.
- One job, without the target pool (no cargo): `jobq submit --project
  semblance --cores 8 --mem 8G --timeout 4h --cwd <worktree> -- uv run
  --project tools tools/export_indices.py --out
  /srv/bulk/Projects/semblance/staging/fixtures-3`. Threads follow the
  claimed cores.
- `--out` is a staging directory, **never** the shared, sha-verified
  `/srv/bulk/Projects/semblance/data`. Inputs are read from the worktree's
  `data/` link as usual.
- If check 3 fails on beast (its x86 torch rounds differently from the
  Mac's), nothing is written; the export is rerun on the owner's Mac and the
  failure is recorded in the epic README as a portability finding.

### 5. The Rust side

`crates/flyvis/src/indices_ref.rs`:

- `IndexFixture::load(data: &Path) -> Result<IndexFixture, ParityError>`
  reads and validates the manifest and every file (sha256 not recomputed
  here; `fetch-fixtures.sh` does that), refusing malformed input with the
  existing `ParityError` variants.
- Accessors for each array by model, precision, stimulus and type, and the
  stimulus table.
- `IndexBattery` implements task 1a's `Battery` for the flash or the edge
  battery, expanding the frame index into `out` one stimulus at a time.
- Fixture-gated tests: shapes agree with the manifest; the flash battery's
  4 stimuli and the edge battery's 144 have the tabled frame counts; the
  speed-13 edges' and the flashes' expanded frames equal
  `flyvis-reference/input.f64.bin`'s.
- `required_files` and `how_to_get` learn the new files, so a missing one is
  `ParityError::Missing` with the right remedy.

### 6. The release

- Staging is copied to the owner's Mac, its sha256s checked against
  `manifest.json`, and placed under `data/flyvis-indices/`.
- `ci/fixtures-tag` becomes `fixtures-3` and `ci/fixtures.sha256` gains the
  new files in the same commit. `fetch-fixtures.sh` needs no change.
- The release upload waits for the owner's sign-off.
- `CLAUDE.md`'s fixture list and skip counts, and `tools/README.md`'s table,
  are updated.

## Out of scope

- Any index computed in Rust, and gates A and B: task 3.
- Zhou & Hasler's runs (fade-in initialisation): task 4, which decides
  whether they need an export at all.
- Rust-generated batteries.

## Versioning

No version change in this piece. `semblance-flyvis` gains a module.
