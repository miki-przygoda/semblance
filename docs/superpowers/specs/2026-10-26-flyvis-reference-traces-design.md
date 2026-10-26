# flyvis reference traces, exported for the graded-dynamics gate

**Date:** 2026-10-26
**Status:** draft — under review
**Epic:** 0.3.0 graded dynamics, task 1 —
`docs/project-outline/0.3.0-graded-dynamics/`
**Research file:**
[`2026-10-26-flyvis-reference-traces-research.md`](2026-10-26-flyvis-reference-traces-research.md).
Items are cited by id: F = fact, P = practice, R = repo convention, C =
conflict or correction.
**Decisions it implements:** the owner decisions and approved design sections
1–3 recorded for task 1 on 2026-10-26, and epic decisions 2, 5 and 6.

## Why

0.3.0 proves that a Rust port of flyvis's dynamics reproduces flyvis's own
voltage traces. That needs flyvis's traces as plain files. Every later gate in
the epic compares against them:

- task 4, the stage-1 settling gate
- task 6, the stage-2 full-trace gate
- task 7, the record

Task 2 needs `ensemble.csv` in the CI fixture set.

The tolerance comes from data, not from a guess (epic decision 2). flyvis is
run in float32 and in float64, and the gap between the two is the floor. So
the export has to store both precisions, and it has to establish three things
before writing any of them:

- that the parameters flyvis loaded are the ones Rust holds
- that the float64 run really is float64
- that the run is deterministic

If any of those is wrong, every later comparison is wrong, and nothing
downstream would notice.

## Goal

Write one offline script, `tools/export_reference.py`. It runs flyvis 1.2.0's
stock simulation path for:

- 2 models (`flow/0000/000`, the best, and `flow/0000/049`, the worst)
- 2 time steps (1/200 and 0.02)
- 2 precisions (float32 and float64)

That makes 8 runs. Each run settles the network, then plays 4 flash sequences
and 8 moving-edge sequences. The script asserts everything listed under
[Assertions](#assertions) and then writes the results under
`data/flyvis-reference/`, in raw little-endian binaries described by one
`manifest.json`. It also writes the moving-edge geometry at height 80 under
`data/flyvis-stimuli/`.

The existing geometry gate is extended to height 80. The fixture set is
published as release `fixtures-2`, and CI caches it. No Rust in this task
reads the traces. That is tasks 4 and 6.

## Facts the design rests on

Read from flyvis 1.2.0's source or measured on 2026-10-26, never imported
into a live process. The ids point at the research file.

| fact | id |
|---|---|
| `FLYVIS_ROOT_DIR` is read at import. Its default is `F/data`. A `.env` in the cwd can set it, but does not override a variable that is already set | F1 |
| datamate reuses a cache directory whose config matches. **dtype is not part of the key** | F6 |
| `NetworkView` creates `<model dir>/__cache__`. `results_dir` is `root_dir/results` | F3, F4, F9 |
| `init_network("best")` builds `Network(**config)`, calls `.eval()`, then `load_state_dict` from `chkpts/chkpt_00000` | F10, F11 |
| sign (604) and syn_count (2355) are identical in all 50 checkpoints. `ensemble.csv` holds the other 734 parameters per model | F15 |
| parameters are created as f32 whatever the default dtype is. `.double()` swaps `param.data` in place | F16, F17 |
| `forward()` calls `clamp()` every pass and mutates `syn_strength` in place. On the loaded values this changes nothing | F18 |
| `steady_state(return_last=False)` returns `int(t_pre/dt)` states: 200 at 1/200 and 50 at 0.02. The first state is already one Euler step in | F19, F20 |
| output frame i is the state after step i+1, driven by input frame i. The initial state is not included | F21 |
| `stimulus_response` = settle once at batch 1, then `forward` from the settled state for each batch | F22 |
| the time-constant floor is `max(tau, f32(dt))` in both precisions: f32(dt) is 0.019999999552965164 at 0.02 and 0.004999999888241291 at 1/200. The Euler step's `vel * dt` uses dt in the run's precision: f32(dt) in f32 runs, because PyTorch casts the Python scalar to the tensor dtype, and the f64 dt in f64 runs | F23 |
| the default dtype matters **at call time**: in the stimulus buffer, `target_sum`, `Flashes.get_item` and `render_flash` | F29 |
| `Stimulus.zero()` reuses an existing buffer of the same shape **and keeps its dtype** | F30 |
| `Flashes.get_item` returns the default dtype. Its values 0, 0.5 and 1 are exact in both | F31 |
| rendered edge offsets are **f64**. An f32 run downcasts them in the buffer | F32 |
| `linspace(0, n−1, 6).long()` gives the same indices in f32 and f64 for every n used here | F33 |
| flash order: OFF-full, OFF-r6, ON-full, ON-r6. 600 frames at 1/200 and 150 at 0.02. Onset is frame 200/50 and offset frame 400/100 | F34, F35 |
| `MovingEdge(angles=[0,30,90,150], speeds=[13], height=80, dt=dt)` gives 8 sequences, angle-major with OFF before ON, and **no NaN padding** | F36, F38, C26 |
| speed 13 runs 525 frames at 1/200 and 131 at 0.02. The stimulus occupies frames [200, 325) and [50, 81) | F37 |
| at height 80 the bar spans the whole 775-row monitor, which exercises a Rust clamp path that no height-9 case has hit | F39, C21 |
| exactly 65 nodes sit at (u, v) = (0, 0), one per cell type, and this is flyvis's own `central_cells_index` | F44, F45 |
| CPU `scatter_add_` was bit-equal at 1 and 8 threads on a random layout. **Not measured on flyvis's expanded-index layout** | F25, C12 |
| freeze-check inputs, all 50 models: 2,396 time constants sit below f32(0.02) (51 in model 000, 36 in model 049) and none below f32(1/200). syn_strength has 919 exact zeros on its `non_negative` clamp and none negative. bias has no clamp | F46–F48 |
| validation loss rises strictly with model index. 000 = 5.136557016107771 is the best and 049 = 5.677851685219341 the worst | F14 |

## Where this spec departs from the brief, and why

The brief records the owner's decisions. Where the source shows one of the
brief's *facts* is wrong, this spec follows the source:

- **Flash inputs are not always f32** (C1). `Flashes.get_item` returns the
  default dtype, so it returns f64 in the f64 runs. The f64 check expects
  that.
- **Moving-edge inputs differ between the precisions** (C2). The f32 run sees
  `f32(x)` and the f64 run sees `x`. So the f32/f64 gap includes input
  rounding. The spec asserts this at two levels and records it in
  PROVENANCE.json (see [Assertions](#assertions)).
- **Setting `stim_dataset.dt` is not ignored by MovingEdge** (C3). It re-runs
  `_resample` under the current dtype. The export never relies on that call.
  It drives `forward` directly and checks that path against
  `stimulus_response` bit for bit.
- **Setting the dtype "before building" is not enough** (C4). Buffers are
  created lazily and then reused. Every run therefore builds a fresh
  `NetworkView`, `Network` and dataset objects, and asserts the default dtype
  before it starts.
- **No NaN trim happens** (C7). Building the edge dataset with `speeds=[13]`
  produces no padding at all. The no-NaN assertion and the frame-count
  assertion stay. Either one fires if padding ever appears.
- **The centre column has exactly 65 nodes**, not "about one per type" (F45).
  The script asserts exactly 65.
- **Edge order is OFF before ON** (C26), as flyvis and the existing fixture
  have it.
- **Sizes**: the centre file is ≈ 12.9 MB, not ~10 MB (C15). Exact figures are
  under [Sizes](#sizes).
- **Citations**: three of the brief's line references were wrong (C14). This
  spec uses the corrected ones.

Where the research found a gap the brief did not cover, this spec decides:

- `nv.flash_responses` and `nv.moving_edge_responses` are not used. Their
  joblib cache ignores dtype (C5).
- The 049 parameter check also covers sign and syn_count, against
  `pretrained.rs`. It runs again after the runs, because `clamp()` mutates in
  place (C6).
- Thread-count determinism is measured, not assumed (C12).
- Batch size stays 1, and that is recorded (C13).
- The best and worst models are recomputed from all 50 validation losses (C16).
- No existing exporter is re-run. The height-80 files are new files with an
  `_h80` suffix, and they get their own sibling provenance (C17, C18).
- The CI cache key covers the tag, has no `restore-keys`, and is scoped to
  the release-backed paths (C19).
- The two halves of the extended gate load independently (C20).

Four further changes would alter what the owner approved. Each one is listed
under [Owner decision needed](#owner-decision-needed-before-the-export-is-run).
None of them is silently applied.

## Owner decision needed (before the export is run)

None of these blocks writing the plan or the script. Each one blocks *running*
the export, because each changes what lands in `fixtures-2`. Changing it
afterwards would cost a re-export and a `fixtures-3`. The spec is written in
full with the default shown for each, so a "yes" or a "no" is all that is
needed.

**O1. Event-anchored all-cell snapshots (C8).** *Default: no. The approved 6
evenly spaced frames stay as they are.*

The even grid misses every flash transient. The nearest snapshot lands
195 ms after onset at 1/200, and the time constants sit near 20 ms. Two of
the six snapshots land in pre-stimulus grey, near the fixed point. That
repeats the epic's risk that stage 1 may pass for the wrong reason.

The proposal is to add, per sequence, all-cell frames at:

- `stim_start`
- `stim_start + k`
- `stim_end`
- `stim_end + k`

where k = 4 at 1/200 (20 ms) and k = 1 at 0.02. The proposal also adds one
all-cell settling frame per run at settling index `n_settle/2 − 1`.

| | cost |
|---|---|
| each extra frame per sequence | 26.3 MB over both precisions |
| the four extra frames | 105.2 MB |
| the settling frames | 2.2 MB |

Without O1, transients are visible only on the dense centre column. The
manifest says so.

**O2. One off-axis dense column (C9).** *Default: no.*

The centre column sits on the lattice's symmetry fixed point, so it cannot
see a reflection error in task 5's input mapping (R20). Only the six
snapshots can.

The proposal is a second dense column at one (u, v), chosen at export as the
column nearest the centre that lies off every lattice symmetry axis and
contains all 65 cell types. If no such column exists, it contains the 63
types other than Lawf1 and Lawf2, and the manifest records the choice.

Cost: ≈ 12.9 MB.

**O3. flyvis runs under a scratch root, not `FLYVIS_ROOT_DIR=data/flyvis-pretrained`
(C10).** *Default in this spec: the scratch root.* This is a departure from
approved section 1 and needs a yes.

With the brief's root, flyvis writes these into the directory that
`ensemble.csv` is published from:

- `connectome/`
- `renderings/`
- the per-model `__cache__/`

A stale cache could also be reused silently (F6).

The spec instead does three things:

1. It points `FLYVIS_ROOT_DIR` at `target/flyvis-reference-root/`, which is
   wiped and recreated at every run.
2. It copies the two model directories into that root.
3. It loads them through the stock `NetworkView("flow/0000/<id>")` path.

The checkpoint is byte-checked against the original. Nothing under `data/` is
written except the outputs.

On a "no", section 1 reverts to the brief's root. The model copy and the
sha256 check are then dropped, and the `weights_only=True` load in step 3 of
Process setup runs on the two original `chkpts/chkpt_00000` files instead.
Every other assertion is unchanged.

**O4. Store the input movies flyvis actually fed (C24, P20).** *Default in
this spec: stored, as `input.f64.bin`, +47.6 MB.* This is an addition to the
approved storage table and needs a yes.

Without it, task 6 can drive stage 2 only with Rust's own stimuli. Those
agree with flyvis's edges only to ≤ 0.005, so stimulus error mixes into a gate
whose tolerance is the arithmetic gap. Storing the inputs keeps both options
open for task 6 (deferred question D1).

On a "no", the file, its manifest blocks and the matching assertion are
dropped.

## Design

### 1. What gets run

`tools/export_reference.py`, run by hand:

```bash
uv run --project tools tools/export_reference.py [--force] [--out DIR]
```

- `--out DIR` sets where the outputs go. The default is `<repo>/data`. The
  script writes `DIR/flyvis-reference/` and three files in
  `DIR/flyvis-stimuli/`, and creates `DIR/flyvis-stimuli/` if it is missing.
- `--force` allows replacing outputs that already exist (see
  [Failure handling](#failure-handling)).

It follows the exporter pattern (R1–R6):

- a module docstring that says why and ends with the run line
- `ROOT`/`OUT` constants
- every choice as a named constant with its reason
- flyvis and torch imported inside `main()`
- one line printed per artifact, ending with `  all assertions passed`

#### Process setup, before `import flyvis`

1. If `FLYVIS_ROOT_DIR` is already set in the environment, abort and name the
   variable. A caller's root would bypass O3, so the script takes no root
   from outside.
2. Delete `ROOT/target/flyvis-reference-root/` and recreate it empty. Set
   `FLYVIS_ROOT_DIR` to it. Set `CUDA_VISIBLE_DEVICES=""` (F26).
3. Copy `data/flyvis-pretrained/results/flow/0000/000/` and `.../049/` into
   `<scratch>/results/flow/0000/`. Copy only these entries:
   - `_meta.yaml`
   - `chkpts/`
   - `validation/`
   - `best_chkpt`
   - `validation_loss.h5`

   Assert that each copied file's sha256 equals its source's. Then load each
   scratch copy of `chkpts/chkpt_00000` with
   `torch.load(path, map_location="cpu", weights_only=True)` and assert it
   succeeds. These are the exact files that every later `nv.init_network`
   unpickles with `weights_only=False` (see [Security notes](#security-notes)),
   so this safe load runs before the first `init_network`.
4. `import flyvis`, then assert:
   - `flyvis.device.type == "cpu"`
   - `flyvis.root_dir` resolves to the scratch root
5. Do **not** call `torch.use_deterministic_algorithms(True)`. It changes
   which kernels run, so the reference would no longer be flyvis as it
   ships. Determinism is measured instead (below).

#### Model selection (C16)

1. Read `validation/loss.h5["data"]` for all 50 directories under
   `data/flyvis-pretrained/results/flow/0000/`, with h5py.
2. Assert that argmin is `000` and argmax is `049`.
3. Assert the two values equal 5.136557016107771 and 5.677851685219341
   exactly.

#### The runs

There are 8 runs, in a fixed order that every file and the manifest share:

| run | model | dt | precision |
|---|---|---|---|
| 0 | 000 | 1/200 | f32 |
| 1 | 000 | 1/200 | f64 |
| 2 | 000 | 0.02 | f32 |
| 3 | 000 | 0.02 | f64 |
| 4 | 049 | 1/200 | f32 |
| 5 | 049 | 1/200 | f64 |
| 6 | 049 | 0.02 | f32 |
| 7 | 049 | 0.02 | f64 |

Runs are *executed* in a different order: the f32 runs first (0, 2, 4, 6),
then `torch.set_default_dtype(torch.float64)`, then the f64 runs (1, 3, 5, 7).
All of them run in one process. The default dtype is process-wide, so each
run starts by asserting `torch.get_default_dtype()` is the run's dtype (P13).

dt is passed as the Python float `1/200` (0.005) or `0.02`.

Each run builds everything fresh (C4):

1. `nv = NetworkView("flow/0000/<id>")`, then
   `network = nv.init_network("best")`.
2. **f64 runs only:** call `network.double()` after the load, and only after
   the first parameter check (below).
3. **Settle.** Call
   `states = network.steady_state(t_pre=1.0, dt=dt, batch_size=1, value=0.5, return_last=False)`.
   Assert `len(states) == int(1.0/dt)`, which is 200 or 50. The settling
   column is `states[i].nodes.activity[0, centre]` for every i. The settled
   state is `states[-1]`.
4. **Settled-state check (P17).** A second call,
   `network.steady_state(1.0, dt, 1, 0.5, return_last=True)`, must give a
   `.nodes.activity` bit-equal to `states[-1].nodes.activity`.
5. **Datasets.**
   - `Flashes(dt=dt)` with flyvis's defaults. Its 4 sequences are dataset
     indices 0–3.
   - `MovingEdge(dt=dt, angles=[0, 30, 90, 150], speeds=[13], height=80)`
     with every other argument left at its class default: widths [80],
     intensities [0, 1], offsets (−10, 11), t_pre 1.0, t_post 1.0,
     post_pad_mode "continue", bar_loc_horizontal 0.0. Its 8 sequences are
     dataset indices 0–7.
6. **Each of the 12 sequences** (order under
   [Sequences](#sequences-in-the-manifest)) runs as `stimulus_response` would
   at batch 1 (F22, C3):

   ```python
   x = dataset[k]                            # as the dataset returns it
   network.stimulus.zero(1, n_frames)
   network.stimulus.add_input(x[None, :, None, :])
   with torch.no_grad():
       trace = network(network.stimulus(), dt, state=settled)   # (1, n_frames, 45669)
   ```

   The script keeps:
   - `trace[0, :, centre]` (every frame)
   - `trace[0, snapshot_frames, :]`
   - `x`, from the f64 runs of model 000, as the stored input

   The full trace is then dropped.
7. **Equivalence with flyvis's own path.**
   - Sequence 0 (flashes, index 0) and sequence 4 (edges, index 0) are also
     run through
     `network.stimulus_response(dataset, dt, indices=[0], t_pre=1.0, t_fade_in=0.0, batch_size=1)`.
   - The response must be bit-equal to `trace` for that sequence.
   - This runs after step 6 for that run, because it sets `dataset.dt`.
8. **Settled state untouched.** After all sequences, `settled.nodes.activity`
   is bit-equal to its value right after step 3.

Batch size is 1 throughout. flyvis's stock analysis uses 4. Nothing here
depends on the two agreeing, and the choice is recorded (C13, D4).

#### Assertions

Every one of these runs before anything is written. A failure aborts the
whole export (see [Failure handling](#failure-handling)). Comparisons are
bitwise: values are compared as `uint32`/`uint64` views, not with `==`, so a
`-0.0` against `0.0` cannot pass.

**A1. Parameters equal Rust's copy (C6).** The script reads
`crates/flyvis/src/pretrained.rs` as text. It parses the statics `BIAS[65]`,
`TIME_CONST[65]`, `SYN_STRENGTH[604]`, `SIGN[604]` and `SYN_COUNT[2355]`, and
the name tables `CELL_TYPES[65]` and `CONN_TYPES[604]`. It asserts each
array's length. It reads `ensemble.csv` (3 `#` lines, a header, 36,700 rows).

- **All 50 checkpoints equal `ensemble.csv`.** Load each with
  `torch.load(path, map_location="cpu", weights_only=True)["network"]`. Check
  `nodes_bias`, `nodes_time_const` and `edges_syn_strength` against the rows
  for that model. Check `edges_sign` and `edges_syn_count` against `SIGN` and
  `SYN_COUNT`, for every model (F15).
- **Names.** The `name` column of the bias and time_const rows equals
  `CELL_TYPES`. The syn_strength rows equal `"src>tar"` from `CONN_TYPES`.
- **What the Network holds, run by run.** Each run checks the raw values
  `network.node_params.{bias,time_const}.raw_values` and
  `network.edge_params.{sign,syn_count,syn_strength}.raw_values`:
  - model 000 against `pretrained.rs`
  - model 049 against its `ensemble.csv` rows, with sign and syn_count against
    `pretrained.rs`

  `node_params.bias.keys` must equal `CELL_TYPES`, and
  `edge_params.syn_strength.keys` must equal `CONN_TYPES`, after decoding
  bytes to str.
- **When.** The check runs at three points:
  1. right after `init_network`
  2. after `.double()`, in the f64 runs. Each value must equal the exact
     widening `float64(float32 value)`.
  3. after the run's last sequence, to catch an in-place clamp (F18)

  In f64 runs, every floating-point tensor reachable from
  `network._param_api()` must also have dtype float64. That means the
  `.values` of every `RefTensor` in nodes/edges/sources/targets, plus every
  plain tensor that `write_derived_params` adds (`edges.weight`). The integer
  `RefTensor.indices` tensors are excluded and must stay int64. That covers
  the values the dynamics actually read (F17).
- `syn_count` stays log-space throughout. Its semantic value is `exp(raw)` (F16).

**A2. Precision is what it claims, throughout (C1, C2).** For each run, with
`want` = float32 or float64:

- `torch.get_default_dtype() == want` at run start.
- Every parameter, and every floating-point `_param_api()` tensor as A1
  defines it, has dtype `want`. The `RefTensor.indices` stay int64.
- `network.stimulus.buffer` has dtype `want`, checked after each `zero()` and
  `add_input`.
- Every settling state and every trace has dtype `want`.
- `x` has dtype `want` for flashes, because it follows the default (F31), and
  float64 for edges in both precisions (F32). If the edge check fires in an
  f32 run, F32 is wrong for some frames. Record that and correct this spec.
  Do not add a cast to make it pass.
- Every tensor is on the CPU.

**A3. Inputs, at the two levels (C2).** For each dt and sequence:

- The model-000 f64 run's `x`, as float64, is stored.
- The f32 run's `x` must be bit-equal to `stored`. Edges are float64 in both
  precisions (A2, F32). Flashes follow the default dtype (F31) and are
  compared after widening the f32 run's `x`, which is exact. So `x` does not
  depend on the default dtype. That is the *dataset* level.
- The f32 run's `buffer[0, :, input_index[j]]` must be bit-equal to
  `float32(stored)` for every one of the 8 photoreceptor rows j. That is the
  *buffer* level.
- The f64 run's buffer must be bit-equal to `stored`.
- Model 049's `x` must be bit-equal to model 000's in the same precision.

PROVENANCE.json records `max |stored − float64(float32(stored))|` over the
edge inputs. It is the input-rounding part of the f32/f64 gap.

**A4. Shapes, counts, NaN.**

- Every trace is `(1, n_frames, 45669)`. Every settling state's activity is
  `(1, 45669)`.
- No NaN or Inf appears in any input, settling state, trace or stored array.
- Frame counts:
  - flashes: 600 at 1/200 and 150 at 0.02
  - edges: 525 at 1/200 and 131 at 0.02
- The edge dataset has 8 sequences and the flash dataset 4.
- The arg rows, in index order, are:
  - flashes: (0.5, 0, −1), (0.5, 0, 6), (0.5, 1, −1), (0.5, 1, 6), as
    (baseline, intensity, radius)
  - edges: (0, 0), (0, 1), (30, 0), (30, 1), (90, 0), (90, 1), (150, 0),
    (150, 1), as (angle, intensity), all at speed 13 and width 80

**A5. Cross-checks against existing fixtures (P16).**

- The network's node table (cell type, u, v) equals
  `data/flyvis-build/nodes.csv`, row for row.
- The centre set is flyvis's own `central_cells_index`
  (`connectome.py:261-264`, `nonzero(u==0 & v==0)`), and it equals the
  `nodes.csv` rows at (0, 0). It has exactly 65 entries, one per cell type,
  ascending by node index. It starts with R1 at 360.
- The flash inputs at 1/200 equal `data/flyvis-stimuli/flashes.bin`
  (`<f4`, (4, 600, 721)) bit for bit, after `float32`.
- The edge resample indices at 1/200 (`dataset.indices[13]`) equal the
  `indices` array of the speed-13 block in
  `data/flyvis-stimuli/moving_edge_timing.json`. Its `n_stim` is 125.
- The stimulus frame spans are recomputed from flyvis's formula and must
  equal the constants:
  - flashes [200, 400) and [50, 100)
  - edges [200, 325) and [50, 81)
- Snapshot indices equal F33's table:

  | n | indices |
  |---|---|
  | 600 | 0, 119, 239, 359, 479, 599 |
  | 525 | 0, 104, 209, 314, 419, 524 |
  | 150 | 0, 29, 59, 89, 119, 149 |
  | 131 | 0, 26, 52, 78, 104, 130 |

  They are computed as `torch.linspace(0, n−1, 6).long()`, flyvis's own
  truncation.

**A6. Determinism, measured (C12, P11).** Run 0 runs sequences 0 and 4 twice:

- once at `torch.set_num_threads(1)`
- once at `torch.set_num_threads(max(2, default))`

The two traces must be bit-equal. The default thread count is restored
afterwards, and both counts are recorded.

**A7. The f32/f64 gap is sane, not tuned (P17).** For each (model, dt) pair,
the script computes the max absolute difference between the f32 and f64 runs
in three places:

- the settled state
- the settling column
- the centre traces and snapshots

It records each one and asserts it is ≤ 1e-2. If this fires, stop and report
it. Do not raise the bound. It would mean epic decision 2's premise, that f32
rounding is small in this network, does not hold. That is the epic's "f64
probe" risk materialising, and it goes to the owner.

**A8. The freeze check (finding §5 action 4, C23).** Over all 50
checkpoints, the script computes and records the following.

- **time_const:**
  - the count below `f32(0.02)`, in total and per model (min, median, max)
  - the counts for models 000 and 049
  - the count in `[f32(0.02), 0.02]`
  - the largest value below the floor and the smallest at or above it
  - the count below `f32(1/200)`
- **syn_strength:** clamp mode `non_negative`.
  - the count of exact `0.0` and the count negative
  - the zero count per model (min, median, max), and for 000 and 049
  - how many connection types are zero in at least one model, and how many
    in all 50
  - the smallest positive value
  - the count of positive values at or below 1e-6, 1e-4 and 1e-2
- **bias:**
  - min and max
  - the configured clamp, read from the model's `_meta.yaml`, which must be
    none

It asserts the figures F46–F48 already measured from the same files.
"Median" here is `statistics.median` over the 50 per-model counts: the mean
of the 25th and 26th sorted values, so a half-integer is the exact expected
value.

| figure | value |
|---|---|
| time_const below the floor, total | 2,396 |
| per model (min / median / max) | 36 / 48.5 / 55 |
| models 000 / 049 | 51 / 36 |
| in `[f32(0.02), 0.02]` | 0 |
| below `f32(1/200)` | 0 |
| syn_strength exact zeros | 919 |
| syn_strength negative | 0 |
| zeros per model (min / median / max) | 8 / 17.5 / 40 |
| zeros for models 000 / 049 | 31 / 15 |
| connection types zero in some model | 418 |
| connection types zero in all 50 | 0 |

A mismatch means the script read the checkpoints differently from the
research pass. That is a bug to find, not a number to update.

The check only records. It does not interpret. Whether the exact zeros are
"frozen" in the time-constant sense is the owner's reading, and it goes into
the epic README (see [Done when](#done-when)).

**A9. Height-80 geometry.**

1. Build `MovingEdge(dt=1/200, angles=[0, 30, 90, 150], height=80)` with
   default speeds, so the build is the existing height-9 exporter's plus
   `height`.
2. Take `offsets = wrap.offsets[:]` as float64 and assert its shape is
   (8, 21, 721).
3. Assert the background is exactly 0.5 at angle 0, intensity 1, within
   1e-6, as the height-9 exporter does. Assert it is 127/255 at angle 30,
   within 1e-6.
4. Assert that the trace runs' `MovingEdge(..., speeds=[13], height=80)`
   exposes bit-identical `wrap.offsets`. The renderings cache has no dt or
   speed in its key (F8).

#### Recorded for the port, not handled here

The floor is `max(tau, f32(dt))` in both precisions. The Euler step's
`vel * dt` uses dt in the run's precision: f32(dt) in f32 runs, because
PyTorch casts the Python scalar to the tensor dtype, and the f64 dt in f64
runs (F23). PROVENANCE.json records both values for each dt and the source
lines. Task 3's spec must reproduce it.

### 2. What gets stored

Everything in `data/flyvis-reference/`. Precision is in the filename, and
each file holds the four runs of that precision, in run order:

- `.f32.bin`: runs 0, 2, 4, 6
- `.f64.bin`: runs 1, 3, 5, 7

| file | contents | per run, or per dt for `input` |
|---|---|---|
| `settled.<p>.bin` | the full state after 1.0 s of grey (settling step n−1) | 45,669 |
| `settling_centre.<p>.bin` | the centre column at every settling step | n_settle × 65 |
| `snapshots.<p>.bin` | all cells at 6 evenly spaced frames per sequence | 12 × 6 × 45,669 |
| `centre.<p>.bin` | the centre column at every frame of every sequence | Σ n_frames × 65 |
| `input.f64.bin` (O4) | the hexal input fed, per dt, shared by both models and precisions | Σ n_frames × 721 |
| `manifest.json` | the index for all of the above. It is deterministic and has no dates | — |
| `PROVENANCE.json` | how, when and on what this was made | — |

`data/flyvis-stimuli/` gains three new files. No existing file there is
opened for writing (C17):

| file | contents |
|---|---|
| `moving_edge_offsets_h80.bin` | `<f4`, (8, 21, 721), C order, the same group order as the height-9 file |
| `moving_edge_offsets_h80.json` | `{"shape", "groups", "dtype", "height"}` |
| `moving_edges_h80_provenance.json` | the sibling provenance (C18) |

Not stored: the gap itself. Tasks 4 and 6 derive it in Rust from the paired
files, at whatever granularity they need.

#### Sizes

These are exact for the default design, with O4 included and O1 and O2 left
out:

| file | f32 bytes | f64 bytes |
|---|---|---|
| settled | 730,704 | 1,461,408 |
| settling_centre | 130,000 | 260,000 |
| snapshots | 52,610,688 | 105,221,376 |
| centre | 4,288,960 | 8,577,920 |
| input | — | 47,574,464 |

The binaries total **220,855,520 B (≈ 221 MB)**: 173,281,056 B without
`input`. `moving_edge_offsets_h80.bin` is 484,512 B. With `ensemble.csv`
(1,444,857 B) and the existing ≈ 49 MB, the uncompressed fixture set is
about 272 MB. That is far inside a release asset's 2 GiB limit and the
10 GB cache quota (F51, F52).

### 3. Publishing and CI

**The fixture set.** One release, `fixtures-2`, replaces `fixtures-1` as what
CI fetches. Its single asset is still named `flyvis-fixtures.tar.gz`.

It contains:

- `flyvis-build/` (unchanged)
- `flyvis-stimuli/` (unchanged, plus the three `_h80` files)
- `flyvis-reference/` (new)
- `flyvis-pretrained/ensemble.csv` (new, never regenerated, C17)

`fixtures-1` is left in place, because old commits still name it.

The tarball is built from local files that already pass `ci/fetch-fixtures.sh`
against the *old* manifest. The 20 fixtures-1 files are therefore
byte-identical, and the fetch script's upgrade path holds: on a machine that
has fixtures-1, those files verify and only the 15 new ones are fetched.

**The manifest and the tag** change together in one commit (R17).
`ci/fixtures.sha256` grows from 20 lines to 35 (exact contract below), and
`ci/fixtures-tag` becomes `fixtures-2`. The release must exist before that
commit is pushed.

**Upload pauses for owner sign-off,** as fixtures-1's did.

**CI caching.** One cache covers the whole fixture set, with these properties:

- its key is exact: the hash of `ci/fixtures.sha256` and `ci/fixtures-tag`
- it has no `restore-keys`
- its paths are scoped to the release-backed paths and the connectome JSON

It replaces today's connectome-JSON cache step (R18), whose file it now
covers.

It is split into `actions/cache/restore@v4` before the fetch and
`actions/cache/save@v4` after it, so a run whose *tests* fail still saves a
good fixture set. Fixture correctness never depends on the cache:
`ci/fetch-fixtures.sh` re-verifies every file's sha256 after a restore. A
corrupt or stale restore fails CI and names the file. It is never silently
used.

Because there are no `restore-keys`, a key change always starts clean. That
avoids the trap where stale restored files are refused by the
never-overwrite rule (C19).

**Documentation that changes with fixtures-2:**

- `ci/fetch-fixtures.sh`: the header comment only. Its logic already handles
  new directories and `flyvis-pretrained/ensemble.csv`.
- `tools/README.md`: a row for `export_reference.py`, and a note that it sets
  its own scratch `FLYVIS_ROOT_DIR`.
- `CLAUDE.md`'s fixture paragraph: `export_reference.py` also produces the
  `_h80` stimulus files, and `ci/fetch-fixtures.sh` now also fetches
  `data/flyvis-reference/` and `ensemble.csv`, which no test reads yet. The
  test counts stay 203 and 26, because no test is added.

The 2026-10-01 CI spec is a dated record and is not edited (C25).

### 4. The Rust change

`moving_edge_offsets_match_flyvis_within_tolerance` is extended to cover
height 80. Its name stays, so CI's `--exact` step still matches it and still
requires `1 passed` (R13). See [The Rust test](#the-rust-test) for the
contract.

Three other edits:

- `test_support.rs`: the two stimulus loaders' skip messages also name
  `tools/export_reference.py`, which writes the `_h80` files (C18).
- `moving_edge.rs:240`: the comment says the band is "88 of the monitor's 775
  rows". It becomes height-dependent wording: 88 rows at height 9, all 775 at
  height 80 (C21).
- `moving_edge_checks.rs`'s module doc: its claim that the debug suite runs
  "in about a second" is replaced with the debug time measured after the
  change (C21).

## Contracts between parts

### Files under `data/flyvis-reference/`

General rules for every `.bin` file:

- raw little-endian, C order, no header and no padding
- `.f32.bin` is IEEE-754 binary32 (`<f4`), and `.f64.bin` is binary64 (`<f8`)
- a file is a concatenation of **blocks**, listed in the manifest in storage
  order
- the first block starts at offset 0, each next block starts where the
  previous one ends, and the file length equals the sum of block bytes
- a block's bytes equal ∏shape × itemsize

| file | block order | block shape | axes |
|---|---|---|---|
| `settled.<p>.bin` | per run | [45669] | `node` |
| `settling_centre.<p>.bin` | per run | [n_settle(dt), 65] | `settle_step`, `centre_node` |
| `snapshots.<p>.bin` | run-major, then sequence 0–11 | [6, 45669] | `snapshot`, `node` |
| `centre.<p>.bin` | run-major, then sequence 0–11 | [n_frames(seq, dt), 65] | `frame`, `centre_node` |
| `input.f64.bin` | dt-major (1/200, then 0.02), then sequence 0–11 | [n_frames(seq, dt), 721] | `frame`, `hexal` |

Axis meanings:

- `node`: flyvis's built node order, identical to `data/flyvis-build/nodes.csv`
  row index (A5).
- `centre_node`: position in `manifest.centre_nodes`, which is ascending node
  index and therefore cell-type order.
- `hexal`: flyvis's hexal order, identical to `data/flyvis-stimuli/hex_coords.csv`.
- `settle_step` i: the state after i+1 Euler steps from the bias initial
  state, with grey 0.5 on every photoreceptor. Its time is (i+1)·dt.
  `settled` equals settle step n_settle−1, in every node.
- `frame` i: the state after i+1 Euler steps from the settled state, where
  step i+1 used input frame i. Its time is (i+1)·dt after the sequence
  starts. Frame 0 is **not** the settled state (F21, P7). flyvis's own
  analysis labels frame 0 as −t_pre. Do not copy that.
- `snapshot` j: frame `snapshot_frames[j]` of that sequence, at that dt.
- input `frame` i: the 721-hexal input that drove frame i. In the f32 runs
  the network saw `float32` of this value (A3).

### `manifest.json`, field by field

Written with `json.dumps(..., indent=2, allow_nan=False, sort_keys=False)`
plus a trailing `\n`. Integers are written as integers. Every float is
written as Python's `repr`, which round-trips exactly. The manifest is a
deterministic function of the inputs, so a rerun produces the same bytes
(P15). Tasks 4 and 6 parse it with `crate::json::parse`, not the ad-hoc
scanners (R12).

| field | type | value |
|---|---|---|
| `schema_version` | int | `1` |
| `generator` | str | `"tools/export_reference.py"` |
| `flyvis_version` | str | `"1.2.0"` |
| `byte_order` | str | `"little"` |
| `array_order` | str | `"C"` |
| `n_nodes` | int | `45669` |
| `n_hexals` | int | `721` |
| `t_pre` | float | `1.0` |
| `pre_stim_value` | float | `0.5` |
| `batch_size` | int | `1` |
| `dts` | array | two objects, in this order: 1/200, then 0.02 |
| `dts[].id` | str | `"1/200"` or `"1/50"`, the exact rational, used as the dt key everywhere below |
| `dts[].dt` | float | `0.005` or `0.02`, the Python float flyvis was given |
| `dts[].floor_dt_f32` | float | `0.004999999888241291` or `0.019999999552965164`, which is f32(dt) widened (F23) |
| `dts[].n_settle` | int | `200` or `50` |
| `models` | array | two objects: 000, then 049 |
| `models[].id` | str | `"000"` or `"049"` |
| `models[].name` | str | `"flow/0000/000"` or `"flow/0000/049"` |
| `models[].validation_loss` | float | 5.136557016107771 or 5.677851685219341 |
| `models[].rank` | str | `"best"` or `"worst"`, of 50, by validation loss |
| `runs` | array | 8 objects, in the run order of §1 |
| `runs[].index` | int | 0–7 |
| `runs[].model` | str | a `models[].id` |
| `runs[].dt` | str | a `dts[].id` |
| `runs[].precision` | str | `"f32"` or `"f64"` |
| `frame_semantics` | object | `{"settle_step": "...", "frame": "...", "time_s": "(i + 1) * dt"}`, the sentences of the axis list above, verbatim |
| `centre_nodes` | array | 65 objects `{"node": int, "cell_type": str}`, ascending `node`. The first is `{"node": 360, "cell_type": "R1"}` |
| `transients_visible_on` | str | `"centre"`, or `"centre+event_snapshots"` if O1 is accepted |
| `sequences` | array | 12 objects, in sequence order (below) |
| `files` | array | one object per `.bin` file, in this order: settled, settling_centre, snapshots and centre (f32 before f64 within each), then input |
| `files[].name` | str | for example `"settled.f32.bin"` |
| `files[].dtype` | str | `"<f4"` or `"<f8"`. It must agree with the name's `.f32`/`.f64` (P9) |
| `files[].axes` | array of str | per the table above |
| `files[].bytes` | int | the file length |
| `files[].sha256` | str | lowercase hex of the file's bytes |
| `files[].blocks` | array | storage order |
| `files[].blocks[].run` | int or null | a `runs[].index`. null in `input.f64.bin` |
| `files[].blocks[].dt` | str | a `dts[].id`, always present |
| `files[].blocks[].sequence` | int or null | 0–11. null in `settled` and `settling_centre` |
| `files[].blocks[].offset` | int | byte offset |
| `files[].blocks[].bytes` | int | byte length |
| `files[].blocks[].shape` | array of int | as in the file table |

#### Sequences in the manifest

Each `sequences[]` object has these fields:

- `index` (0–11)
- `family` (`"flashes"` or `"moving_edge"`)
- `flyvis_index`, the index in the dataset as constructed in §1
- `args`
- `frames`, an object keyed by dt id. Each value is
  `{"n_frames": int, "stimulus_frames": [start, end), "snapshot_frames": [6 ints]}`.

| index | family | flyvis_index | args |
|---|---|---|---|
| 0 | flashes | 0 | `{"baseline": 0.5, "intensity": 0.0, "radius": -1}` (OFF, full field) |
| 1 | flashes | 1 | `{"baseline": 0.5, "intensity": 0.0, "radius": 6}` |
| 2 | flashes | 2 | `{"baseline": 0.5, "intensity": 1.0, "radius": -1}` |
| 3 | flashes | 3 | `{"baseline": 0.5, "intensity": 1.0, "radius": 6}` |
| 4–11 | moving_edge | 0–7 | `{"angle": a, "intensity": i, "speed": 13.0, "width": 80, "height": 80, "offsets": [-10, 11], "t_pre": 1.0, "t_post": 1.0, "post_pad_mode": "continue", "bar_loc_horizontal": 0.0}`, with (a, i) = (0,0), (0,1), (30,0), (30,1), (90,0), (90,1), (150,0), (150,1) |

`radius: -1` is flyvis's full-field sentinel (F35). Frame counts, stimulus
spans and snapshot frames are the A4 and A5 constants.

### `PROVENANCE.json`

Written with `indent=2` and a trailing `\n`. It is volatile: dates, wall time
and machine. It is pinned by sha256, so any re-export needs a new fixtures tag
(P15).

| field | contents |
|---|---|
| `exported` | ISO date |
| `export_id` | `<UTC timestamp>-<pid>`, the same value as in `moving_edges_h80_provenance.json`, so a mismatched pair is detectable |
| `generator` | `{"path": "tools/export_reference.py", "sha256": ...}` |
| `git` | `{"head": <rev-parse HEAD>, "dirty": bool}` |
| `versions` | python, flyvis, torch, numpy, h5py, datamate |
| `machine` | `platform.platform()`, `platform.machine()`, `torch.__config__.show()` |
| `uv_lock_sha256` | the sha256 of `tools/uv.lock` |
| `device` | `"cpu"` |
| `threads` | `{"default": int, "determinism_check": [1, int], "bit_equal": true}` (A6) |
| `flyvis_root` | the scratch path, and the fact that the models were copied there (O3) |
| `models` | for each of 000 and 049: name, validation loss, rank, and the sha256 of `chkpts/chkpt_00000` and of the scratch copy |
| `validation_losses` | all 50, by id |
| `inputs_read` | sha256 of `ensemble.csv`, `pretrained.rs`, `nodes.csv`, `flashes.bin` and `moving_edge_timing.json` |
| `dts` | per dt: the float, the rational, n_settle, `floor_dt_f32` |
| `settling` | `{"t_pre": 1.0, "value": 0.5, "t_fade_in": 0.0, "batch_size": 1}` |
| `stimuli` | each dataset's `config.to_dict()` and its arg rows |
| `f32_floor_trap` | for each dt: `{"dt", "f32_dt", "source": "flyvis/network/dynamics.py:207-218", "euler_step_dt": {"f32": f32_dt, "f64": dt, "source": "network.py:406"}, "note": "task 3 must reproduce max(tau, f32(dt))"}` |
| `parameter_equality` | per model and parameter: count, the reference it was checked against, and true for each phase (loaded, widened, after_runs). Also `"all_50_checkpoints_equal_ensemble_csv": true` |
| `equivalence` | `{"stimulus_response_bit_equal": true, "sequences": [0, 4]}` |
| `input_rounding` | max \|x − f64(f32(x))\| over the edge inputs, per dt (A3) |
| `f32_f64_gap` | per (model, dt): the max abs gap in settled, settling_centre, centre and snapshots (A7) |
| `freeze_check` | every figure in A8 |
| `sizes` | bytes per file |
| `wall_time_s` | float |

### The height-80 geometry files

- `moving_edge_offsets_h80.bin`:
  - `<f4`, shape (8, 21, 721), C order
  - group g = 2·angle_index + intensity_index, with angles [0, 30, 90, 150]
    and intensity 0 before 1
  - the same layout as `moving_edge_offsets.bin`
- `moving_edge_offsets_h80.json`:

  ```json
  {"shape": [8, 21, 721], "groups": [{"index": 0, "angle": 0, "intensity": 0.0}, ...], "dtype": "<f4", "height": 80}
  ```

  This is the height-9 sidecar's format plus `"height": 80`. The new key
  collides with none of the scanner keys `index`, `angle`, `intensity` or
  `shape` (R12).
- `moving_edges_h80_provenance.json`, with these fields:
  - `exported` and `export_id`
  - `generator: "tools/export_reference.py"`
  - `flyvis`, `torch` and `numpy` versions
  - `dt: 0.005`
  - `gate_angles`
  - `height: 80`
  - `n_ommatidia: 721`, `ppo: 25`, `monitor_px: 775`
  - `offsets_shape`

### `ensemble.csv` in the fixture set

`data/flyvis-pretrained/ensemble.csv`, byte-for-byte as it is today (R7):

- 3 `#` lines, then the header `model,kind,index,name,value`, then 36,700
  rows
- row order: model `000`–`049`, then kind in the order `bias`, `time_const`,
  `syn_strength`, then index
- values are shortest-round-trip f32 strings
- `name` is a cell type, or `src>tar` for a connection type

A1 proves all 50 models equal their checkpoints bit for bit. sign and
syn_count are not in the file. They are the same for every model and live in
`pretrained.rs` (F15).

### The fixture manifest, tag and tarball

`ci/fixtures.sha256` is generated from `data/`:

```bash
cd data
{ shasum -a 256 flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json
  find flyvis-build flyvis-stimuli flyvis-reference -type f ! -name '.*' | LC_ALL=C sort | xargs shasum -a 256
  shasum -a 256 flyvis-pretrained/ensemble.csv
} > ../ci/fixtures.sha256
```

- It has 35 lines. The 20 fixtures-1 lines are unchanged, so
  `git diff --numstat ci/fixtures.sha256` reads `15 0`.
- `find` is never run over `flyvis-pretrained/` (R14).

`ci/fixtures-tag` holds `fixtures-2`. Both files change in **one** commit.

The tarball is built and checked like this:

```bash
COPYFILE_DISABLE=1 tar -czf "$out" -C data flyvis-build flyvis-stimuli flyvis-reference flyvis-pretrained/ensemble.csv
tar -tzf "$out" | grep -v '/$' | LC_ALL=C sort > listing
diff <(sed -n '2,$p' ci/fixtures.sha256 | awk '{print $2}' | LC_ALL=C sort) listing   # empty
grep -c '\._' listing                                                                # 0
```

The listing has 34 entries. The release is created like this:

```bash
gh release create fixtures-2 "$out" --latest=false \
  --title "fixtures-2: flyvis 1.2.0 exports and reference traces for CI" \
  --notes "Test fixtures for the flyvis acceptance gates and the 0.3.0 reference traces: data/flyvis-build, data/flyvis-stimuli (now with height-80 edge offsets), data/flyvis-reference and data/flyvis-pretrained/ensemble.csv, exported by tools/ from flyvis 1.2.0 (see each directory's provenance). Verified file-by-file against ci/fixtures.sha256 by ci/fetch-fixtures.sh. Not a version release."
```

### The CI cache

In `.github/workflows/ci.yml`, job `test`, the existing step
`actions/cache@v4` with key `connectome-json-…` is replaced by:

```yaml
      - name: Restore fixtures
        id: fixtures-cache
        uses: actions/cache/restore@v4
        with:
          path: |
            data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json
            data/flyvis-build
            data/flyvis-stimuli
            data/flyvis-reference
            data/flyvis-pretrained/ensemble.csv
          key: fixtures-${{ hashFiles('ci/fixtures.sha256', 'ci/fixtures-tag') }}

      - name: Fetch and verify fixtures      # unchanged
        env:
          GH_TOKEN: ${{ github.token }}
        run: ci/fetch-fixtures.sh

      - name: Save fixtures
        if: steps.fixtures-cache.outputs.cache-hit != 'true'
        continue-on-error: true
        uses: actions/cache/save@v4
        with:
          path: |
            (the same five paths)
          key: ${{ steps.fixtures-cache.outputs.cache-primary-key }}
```

- The key has no `restore-keys`.
- `continue-on-error` on the save is deliberate. A failed save, such as a
  concurrent run that already reserved the key, only costs a download on the
  next run. Correctness rests on the fetch step's verification, never on the
  cache.
- The cargo caches and every other step are unchanged.

### The Rust test

`crates/flyvis/src/moving_edge_checks.rs::moving_edge_offsets_match_flyvis_within_tolerance`:

- **Name unchanged.** CI's step runs it with `--exact` and requires `1 passed`.
- **The two halves load independently** (C20). The body calls a private
  helper `geometry_report(height, bin, json) -> Option<Report>` twice:
  - with `(9, "moving_edge_offsets.bin", "moving_edge_offsets.json")`
  - with `(80, "moving_edge_offsets_h80.bin", "moving_edge_offsets_h80.json")`

  Each call loads its own two files through `load_stimulus_bytes` and
  `load_stimulus_fixture`. A missing pair returns `None` for that half only:
  it skips that half without `SEMBLANCE_REQUIRE_FIXTURES=1` and fails with
  it. The other half still runs.
- **Sidecar checks.** Each half runs the existing checks: shape
  `[8, 21, 721]`, group indices 0–7, angle-major order and intensity order.
  The height-80 half also requires `json_field(json, "height") == "80"`. The
  height-9 sidecar has no `height` field and stays untouched.
- **Parameters.** Rust renders with `MovingEdgeParams::flyvis_defaults(DT)`,
  with `angles = GATE_ANGLES` and `height` set to 9 or 80.
- **Output.** Both tables print **before** any assertion. Each is headed
  `moving edge geometry, height <h>, against flyvis 1.2.0 (30282 values per angle):`
  and has the existing columns.
- **Assertions.** Each half, independently: `max <= TOLERANCE` and zero
  hexals over it, per angle, with the existing "do not widen" message plus
  the height.
- `TOLERANCE` stays `0.005` for both heights. If height 80 exceeds it, stop
  and report. The new Rust clamp path is the first suspect (F39, C21). The
  bound is never widened, per `CLAUDE.md`.

### What tasks 2, 4, 6 and 7 consume

- **Task 2 (ensemble loader):** `data/flyvis-pretrained/ensemble.csv`, in the
  format above, present in CI via fixtures-2. A1 already proves every row
  equals its checkpoint. Task 2's own pin, model 000's rows against
  `pretrained.rs`, is therefore a check of its parser, not of the data.
- **Task 4 (stage 1):** `settled.<p>.bin`, `settling_centre.<p>.bin`,
  `manifest.json` (`runs`, `dts`, `centre_nodes`, `frame_semantics`) and
  PROVENANCE's `f32_floor_trap`. It reads:
  - Rust f64 against `settled.f64` and Rust f32 against `settled.f32`
  - the floor, from the f32/f64 pair
  - mid-settling agreement from `settling_centre` (the epic's "wrong reason"
    risk)

  It must take the f32-comparison floor as ≈ 2× the measured gap, not 1×
  (C11, P1). It must report per cell type with a mixed tolerance (P3), and
  look at the error against step index (P4).
- **Task 6 (stage 2):** `centre.<p>.bin`, `snapshots.<p>.bin`, `input.f64.bin`
  and `sequences`. Which input drives the Rust run is D1.
- **Task 7 (record):** `PROVENANCE.json`, both manifests, and the
  freeze-check figures.
- **Loaders**, whichever task adds them first: add `load_reference_bytes(name)`
  and `load_reference_fixture(name)` to `test_support.rs`. They follow R10.
  Their skip message names `tools/export_reference.py` and
  `ci/fetch-fixtures.sh`. Every loader asserts `file length ==
  manifest.files[].bytes == Σ blocks[].bytes` and `dtype` against the file
  name before reading a value (P9).

## User flows

Each flow runs end to end on the real program.

1. **Export, first time.** Set-up: on branch `reference-traces`,
   `data/flyvis-pretrained/results/` present (from `download_pretrained.py`),
   no `data/flyvis-reference/`, and `FLYVIS_ROOT_DIR` unset. Run
   `uv run --project tools tools/export_reference.py`.

   It prints, in order:
   - one line per setup step
   - the model ranking
   - per run: the settle count and per-sequence frame counts
   - the A6 and A7 figures
   - the freeze-check summary
   - one line per written file with its bytes
   - `  all assertions passed`

   It exits 0. `data/flyvis-reference/` holds the 11 files of §2, and
   `data/flyvis-stimuli/` holds the three `_h80` files. Nothing else under
   `data/` changed: `ci/fetch-fixtures.sh` still reports the 20 fixtures-1
   files verified, and `git status` shows no change made by the export.
2. **Rerun without `--force`.** It exits 1 before importing flyvis, naming
   `data/flyvis-reference` or the first existing `_h80` file. Nothing is
   touched.
3. **Idempotency.** Run
   `uv run --project tools tools/export_reference.py --out target/reref`, then
   `cmp` each `.bin`, `manifest.json` and the two `_h80` data files against
   the copies in `data/`. All are identical. Only the two provenance files
   differ.
4. **The height-80 gate.** Run
   `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis -- --exact --nocapture moving_edge_checks::moving_edge_offsets_match_flyvis_within_tolerance`.

   It prints the height-9 table, which is unchanged from today's figures,
   then the height-80 table. It ends `1 passed`. The height-80 table is
   **read**: its per-angle max and mean are copied into the PR description
   and compared with the height-9 regimes. Deleting the two `_h80` files and
   rerunning with `SEMBLANCE_REQUIRE_FIXTURES=1` fails, naming the missing
   path. Without the variable, the height-9 half still prints its table.
5. **Publish fixtures-2.**
   1. Regenerate `ci/fixtures.sha256` and check `15 0`.
   2. Build and check the tarball.
   3. **Stop for owner sign-off**, quoting the asset size and the file count.
   4. Run `gh release create fixtures-2 …`.
   5. Set `ci/fixtures-tag` to `fixtures-2` and commit both `ci/` files in one
      commit.
6. **Fresh clone.** In a clean checkout with `gh` authenticated, run
   `SEMBLANCE_DATA_DIR=$(mktemp -d) /bin/bash ci/fetch-fixtures.sh`. It
   prints both fetch messages, then `fetched 35 of 35 fixtures; all 35
   verified`.
7. **Upgrade from fixtures-1.** Use a scratch data dir holding only the
   fixtures-1 set. The script fetches only the 15 new files, `fetched 15 of
   35`, and leaves the other 20 untouched (their sha256 is the same before
   and after).
8. **CI, cache miss then hit.**
   - The first PR run after the commit logs `Cache not found` for
     `fixtures-<hash>`, fetches, saves, and passes. The moving-edge step's
     log shows both height tables.
   - "Re-run all jobs" on the same PR logs `Cache restored from key:
     fixtures-<hash>`, and the fetch step prints `all 35 fixtures present and
     verified`. No release download happens.
9. **A corrupt cache is caught.** On a throwaway branch, flipping one byte of
   a restored fixture before the fetch step fails that step, naming the file.
   It is not merged.

## Failure handling

**Fail closed, all in memory first (P14).** The script computes every array
and runs every assertion (A1–A9) before it creates any output path. Any
failed assertion or exception:

- raises, exits 1 with a message that names the check, the run and the
  sequence
- removes its staging directory in a `finally`
- leaves `data/` exactly as it was

`KeyboardInterrupt` takes the same path. The scratch root under `target/` is
left in place for inspection, and the next run wipes it.

**Preflight, before importing flyvis.** These checks run first:

- If `<OUT>/flyvis-reference` exists, or any of the three `_h80` files
  exists, the script exits 1 naming it, unless `--force` is given.
- If any `<OUT>/.flyvis-reference.staging-*` or `.flyvis-reference.old-*`
  exists, the script exits 1 naming it, even with `--force`. That is a
  previous interrupted run, and a human removes it.
- If `FLYVIS_ROOT_DIR` is already set, the script exits 1.

**Writing the set atomically.**

1. Create `<OUT>/.flyvis-reference.staging-<pid>/`, a sibling of the output
   on the same filesystem. Inside it, create `flyvis-reference/` and
   `flyvis-stimuli/`.
2. Write each `.bin` with `astype("<f4"|"<f8").tobytes(order="C")`, then
   `PROVENANCE.json`, then `manifest.json` **last** as the commit marker.
   Write the three `_h80` files the same way, provenance last. `fsync` every
   file and both directories.
3. Re-read every staged `.bin` and check its sha256 and length against the
   manifest.
4. If `<OUT>/flyvis-reference` exists (`--force`), rename it to
   `<OUT>/.flyvis-reference.old-<pid>`. Rename the staged
   `flyvis-reference/` into place, then `fsync` `<OUT>`.
5. `os.replace` each staged `_h80` file into `<OUT>/flyvis-stimuli/`, in this
   order: `.bin`, `.json`, then provenance. `fsync` the directory.
6. Remove the `.old-<pid>` directory and the staging directory, then print
   `  all assertions passed`.

An interruption inside steps 4–6 can leave a new `flyvis-reference/` beside
old or missing `_h80` files. The `export_id` in the two provenance files
then differs, and `.flyvis-reference.old-*` or `.staging-*` remains, so the
next run's preflight refuses until a human looks. Recovery is to remove the
leftovers and rerun with `--force`.

**Reruns.** A rerun is the only way to change the output, and it needs
`--force`. Its binaries and manifest are byte-identical (flow 3), but its two
provenance files are not. Once fixtures-2 is published, a rerun therefore
means a new tag, `fixtures-3`, with the manifest and tag committed together.
The fetch script will refuse the re-exported files locally until that
happens, by design (R16).

**Assertions that are findings, not bugs.** For A7 (gap > 1e-2), A8 (a
freeze figure differs) and the Rust height-80 gate exceeding 0.005: stop and
report to the owner. Never loosen the bound or update the expected figure to
match.

## Security notes

- **Pickle.** Checkpoints are read with `torch.load(..., weights_only=True)`
  in A1. flyvis 1.2.0's recovery path (`recover_network` ->
  `get_from_state_dict`, `flyvis/utils/chkpt_utils.py:139-143`) calls
  `torch.load(..., weights_only=False)` explicitly, so every `init_network`
  unpickles the checkpoint with full pickle semantics. The protection on that
  path is that only the local, sha256-verified copies are loaded, and that
  each of those copies has first loaded successfully with
  `weights_only=True` (Process setup, step 3).
- **No network.** The exporter makes no network requests. flyvis needs none
  when the models are on disk. The copied checkpoints are checked against
  their originals.
- **Environment.** flyvis loads `.env` from the cwd (F1). The script sets
  `FLYVIS_ROOT_DIR` itself and refuses an inherited one, and `load_dotenv`
  does not override a variable that is already set. So a stray `.env` cannot
  redirect where flyvis writes.
- **Fixture integrity.** Every fixture is pinned by sha256 in a committed
  file. A restored CI cache is untrusted until `ci/fetch-fixtures.sh`
  re-verifies it. Cache scope is the branch plus the default branch (F52),
  and a poisoned entry fails loudly rather than being used.
- **Publishing.** The release is on the private repo, `--latest=false`, and
  created only after explicit owner sign-off. The tarball is built with
  `COPYFILE_DISABLE=1` and its listing diffed against the manifest. That
  keeps out AppleDouble metadata, `.DS_Store`, the scratch root and every
  `__cache__`.
- **CI permissions** stay `contents: read`, which is enough for
  `gh release download` and for the cache actions. No new secret is added.
- **Boundary.** Nothing in Rust invokes `tools/` or the fetch script, and
  `Cargo.lock` keeps exactly 8 packages.

## Non-goals

- Reading the traces in Rust, including the loaders (tasks 4 and 6), the
  integrator (task 3) and the hexal-to-cell mapping (task 5).
- Computing or storing the f32/f64 gap as data. PROVENANCE holds only the A7
  sanity maxima.
- Reproducing the f32-floor trap. Task 1 only records it.
- Other stimuli (dots, Sintel), other models, other dts, batch sizes other
  than 1, and CUDA or MPS.
- Re-running `export_build.py`, `export_stimuli.py`, `export_moving_edges.py`
  or `export_params.py`, or rewriting any file they produced.
- Changing the height-9 gate, its sidecar, or `TOLERANCE`.
- Editing the dated 2026-10-01 CI spec or plan.
- Deleting or replacing the `fixtures-1` release.
- Bumping any version. The 0.3.0 bump waits for tasks 0–7.

## Deferred questions

- **D1. Which input drives task 6's stage-2 run: flyvis's `input.f64.bin`, or
  Rust's own stimuli? (C24)** *Can wait* because both are possible with this
  data, provided O4 is accepted. If O4 is declined, this becomes "Rust's
  stimuli, or gate only angle 0 element-wise", which is also task 6's
  decision.
- **D2. The f64 floor (C11, P2).** Measure Rust-f64 against itself under a
  permuted edge summation order, or use gap × 2⁻²⁹ as a recorded heuristic.
  *Can wait* because both happen in Rust in task 4, and neither needs flyvis
  re-run.
- **D3. Where the reference loaders live.** The integrator is in
  `semblance-sim`, and the fixture helpers are `pub(crate)` in
  `semblance-flyvis`. *Can wait* because no task-1 code reads the traces. It
  is task 4's first design question.
- **D4. Batch-4 equivalence (C13).** *Can wait* because nothing in 0.3.0 runs
  at batch 4. 0.4.0 can check it if it reproduces flyvis's stock analysis.
- **D5. `actions/cache` v4's support horizon (F53).** *Can wait* because v4
  works today. Bump all cache steps together when it is deprecated.
- **D6. A second machine's reference (P12).** The reference is "flyvis 1.2.0
  on this Mac's CPU" and is recorded as such. *Can wait* because the gate's
  tolerance is measured, not bit-exact. A cross-machine check is only worth
  doing if task 4's margins come out tight.
- **D7. Epic README corrections (C22, C23, C27).** *Can wait* until task 1
  closes, and they are the owner's edits to a local file:
  - decision 7 and the task-1 bullet still describe "every k-th frame, f32
    only" and "store the gap"
  - the freeze expectation for syn_strength rests on a wrong premise
  - the Risks section says "task 4" where it means task 5

## Done when

- [ ] O1–O4 are answered, and the spec is updated to match before the export
      runs.
- [ ] `uv run --project tools tools/export_reference.py` exits 0 with
      `  all assertions passed`. All of A1–A9 ran and none was skipped.
- [ ] `data/flyvis-reference/` holds exactly the files in §2, and their
      lengths equal the [Sizes](#sizes) table (or its O1/O2/O4-adjusted
      version).
- [ ] `manifest.json` validates against the field table. Every
      `files[].sha256` matches, and every file length equals the sum of its
      blocks.
- [ ] Flows 2 and 3 behave as written: refusal without `--force`, and
      byte-identical binaries and manifest on a rerun.
- [ ] The height-80 gate passes with `TOLERANCE` unchanged. Its table is
      read, and its figures are recorded in the PR description beside the
      unchanged height-9 figures (0.000794 / 0.002823 / 0.002692 / 0.003057).
- [ ] `cargo build --workspace`, `cargo test --workspace` (203 passed),
      `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo run --quiet -- roundtrip` (both distances `0.000000`) all pass.
      `Cargo.lock` still has 8 packages.
- [ ] fixtures-2 is published after owner sign-off. `ci/fixtures.sha256`
      (35 lines, `15 0` diff) and `ci/fixtures-tag` are committed in one
      commit.
- [ ] Flows 6 and 7 pass: 35 of 35 from empty, and 15 of 35 from fixtures-1.
- [ ] CI is green on the PR with fixtures-2. A rerun shows the cache hit
      and `all 35 fixtures present and verified`.
- [ ] `tools/README.md`, `CLAUDE.md`'s fixture paragraph, the fetch script's
      header, the `moving_edge.rs:240` comment and the module doc's debug
      timing are updated.
- [ ] The 0.3.0 epic README (local) records the freeze-check result and the
      exact sizes. The owner makes the D7 corrections.
