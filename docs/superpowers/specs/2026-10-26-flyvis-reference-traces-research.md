# flyvis reference traces: research notes

**Date:** 2026-10-26
**Cited by:** `docs/superpowers/specs/2026-10-26-flyvis-reference-traces-design.md` (0.3.0 task 1)

These notes merge three research passes made for 0.3.0 task 1, the export of
flyvis reference traces: documented facts, best practices, and prior art in the
repo. Each item has a stable id (F = fact, P = practice, R = repo convention or
prior art, C = conflict or correction), and the spec cites items by id.

Path abbreviations:
- `F/` = `tools/.venv/lib/python3.12/site-packages/flyvis`.
- `DM/` = `tools/.venv/lib/python3.12/site-packages/datamate`.
- Other paths are relative to the repo root.

Versions read from the venv: flyvis 1.2.0 (`F/version.py`, commit g92b3845cc),
torch 2.14.0, numpy 2.5.3, h5py 3.16.0, datamate 1.0.0.

How the evidence was gathered:
- flyvis was never imported. Measurements used plain torch, numpy or h5py
  under `python -I`, and none of those runs wrote anything.
- **UNVERIFIED** marks a claim nobody checked in this session.
- **[GK]** marks a claim from general knowledge whose URL was not fetched.

---

## Documented facts

### Root dir and caches

- **F1.** `FLYVIS_ROOT_DIR` is read when flyvis is imported. Its default is
  `repo_dir/"data"`. For an installed package `repo_dir` is the package
  directory itself, so the default is `F/data`. That directory is created if it
  is missing, and `datamate.set_root_dir(root_dir)` is then called.
  - Source: `F/__init__.py:39-56,67-70`.
  - `F/__init__.py:18` also calls `dotenv.load_dotenv(find_dotenv(usecwd=True))`,
    so a `.env` file could set the variable. The repo has none today.
- **F2.** The datamate caches live under the root dir:
  - renderings at `root_dir/renderings` (`F/__init__.py:59`)
  - the connectome at `root_dir/connectome` (`F/connectome/connectome.py:108`)
- **F3.** Constructing a `NetworkView` creates `<model dir>/__cache__`.
  - Source: `F/network/network_view.py:109-114` uses joblib
    `Memory(location=dir/"__cache__")`, and joblib's `FileSystemStoreBackend`
    runs `mkdirp(location)` (`joblib/_store_backends.py:470`).
  - `chkpt_utils.py:206` is a separate call, `mkdir(exist_ok=True)` on the
    existing `chkpts/` dir, and does nothing. See C14.
- **F4.** With `FLYVIS_ROOT_DIR=data/flyvis-pretrained`, a run creates:
  - `data/flyvis-pretrained/connectome/`
  - `data/flyvis-pretrained/renderings/`
  - `results/flow/0000/{000,049}/__cache__/`

  Pointing the root at a scratch directory instead, and passing `NetworkView` an
  absolute model path, still creates `__cache__` inside that model dir.
  Source: `F/__init__.py:47-59`; `network_view.py:109-114,343-345`;
  `connectome.py:108`.
- **F5.** The existing exporters ran with the default root, so `F/data/` already
  holds:
  - `connectome/`
  - `renderings/RenderedFlashes_0000` (dt 0.005)
  - `RenderedOffsets_0000` (height 9, angles [0,30,90,150])
  - `responses_norm.h5`

  Source: `ls F/data`; the `_meta.yaml` files.
- **F6.** How datamate looks up its cache: for a `@root(...)` Directory it scans
  the root and reuses the first directory whose `_meta.yaml` config equals the
  full constructor config and whose status is "done". Directories marked
  "modified" are skipped. New directories are named `<Type>_<hex>`. **dtype is
  not part of the key.** Source: `DM/directory.py:1364-1408,1611-1621`.
- **F7.** The `RenderedFlashes` cache key includes dt, boxfilter,
  dynamic_range, t_stim, t_pre, radius and alternations. dt 1/200 and dt 0.02
  therefore get separate cache directories. Source: `F/datasets/flashes.py:44-53`.
- **F8.** The `RenderedOffsets` cache key has **no dt**, because resampling
  happens at runtime (`moving_bar.py:302-348`). The key does include angles,
  widths, intensities, offsets, led_width, height (in radians) and
  bar_loc_horizontal. Height 80 and a different angle subset each get a new
  cache directory. Source: `F/datasets/moving_bar.py:53-64,283-297`.

### Models and checkpoints

- **F9.** `NetworkView("flow/0000/049")` resolves through `Directory(name)`
  inside `set_root_context(flyvis.results_dir)`. An absolute path also works.
  `.name` is always the last three path components.
  Source: `network_view.py:330-349`.
- **F10.** `init_network("best")` calls `CheckpointedNetwork.init()`, which
  builds `Network(**config)` and calls `.eval()`. It then calls `recover()`,
  which does `torch.load(...).pop("network")` followed by `load_state_dict`.
  - Source: `network_view.py:85-123,290-305,428-457`;
    `F/utils/chkpt_utils.py:17-42,122-150`.
  - The checkpoint is loaded with `map_location=flyvis.device`
    (`chkpt_utils.py:141-143`).
- **F11.** "best" means `argmin` over the 0-d `validation/loss` (after the
  epe→loss fallback), which picks `chkpts/chkpt_00000`.
  - Source: `chkpt_utils.py:218-263`; default `loss_file_name="epe"` at
    `network_view.py:93-96`.
  - `best_chkpt` holds the same tensors as `chkpts/chkpt_00000`. This was
    checked with `torch.equal` for model 049.
- **F12.** On disk, `data/flyvis-pretrained/results/flow/0000/` holds 50 model
  directories (000–049) plus `umap_and_clustering/`. Each model directory holds:
  - `_meta.yaml`
  - `chkpts/chkpt_00000`
  - `validation/loss.h5`
  - `best_chkpt`
  - `validation_loss.h5`
- **F13.** Checkpoint contents:
  - Top-level keys are `network` and `decoder`.
  - `network` holds nodes_bias(65), nodes_time_const(65), edges_sign(604),
    edges_syn_count(2355, stored as ln n) and edges_syn_strength(604), all f32.
  - Source: `torch.load(..., weights_only=True)`.
- **F14.** Validation loss:
  - It is stored as a 0-d f64 under key `"data"` and can be read without
    flyvis via `h5py.File(".../validation/loss.h5")["data"][()]`.
  - flyvis itself reads `nv.dir["validation"]["loss"][()]` (`ensemble.py:500`).
  - Over all 50 models the loss strictly increases with directory index:
    000 = 5.136557016107771 (best) and 049 = 5.677851685219341 (worst).
  - `validation_loss.h5` holds the same value.
  - Ensemble sorting is at `ensemble.py:425-448`, argsort at `:450` and
    `validation_losses` at `:487-501`.
- **F15.** `sign` (604) and `syn_count` (2355) are **identical in all 50
  checkpoints** (`torch.equal` against model 000). `bias`, `time_const` and
  `syn_strength` differ in every other model.
  - Consequence: `ensemble.csv` holds only the 734 free parameters per model
    (65 + 65 + 604), 36,700 rows in total.
  - Sign and syn_count live in `crates/flyvis/src/pretrained.rs:700-886,1186`
    as `SIGN[604]` and `SYN_COUNT[2355]`.
- **F16.** Parameters are created as f32 whatever the default dtype is.
  - Source: `F/network/initialization.py:124,157,166-169,177-178`, which use
    `.float()`.
  - Loading the f32 checkpoint into them is therefore exact.
  - The syn_count semantic value is `exp(raw)` (`initialization.py:199-202`).
    In an f64 run that is exp computed in f64 of an f32-quantised ln n.
- **F17.** `.double()` keeps the same Parameter objects. It swaps
  `param.data` in place, so the `semantic_values` that `_param_api` uses also
  become f64.
  - Measured: `h.p is m.x` is True and the dtype is float64.
  - `torch.__future__` overwrite and swap flags are both False in 2.14.
- **F18.** `forward()` calls `clamp()` on every pass (`network.py:480-507,527`).
  - The clamp mutates in place any parameter with `requires_grad=True`.
  - `steady_state` and `stimulus_response` do **not** enter `simulation()`. They
    switch grad off only through `enable_grad(False)`, so parameters keep
    `requires_grad=True` and the clamp runs.
  - Only syn_strength has a clamp mode, `non_negative`
    (`syn_strength.yaml:5`; `_meta.yaml` edge_config).
  - On the loaded values the clamp changes nothing: none are negative (F37).
    Many values do sit exactly on the clamp boundary.

### Dynamics and simulation

- **F19.** `steady_state(t_pre, dt, batch_size, value=0.5, state=None,
  grad=False, return_last=True)`:
  - It returns the state unchanged if t_pre is None or ≤ 0, or value is None.
  - Otherwise it allocates `int(t_pre/dt)` frames, calls `add_pre_stim(value)`,
    and runs under `enable_grad(grad)`.
  - With `return_last=False` it returns `int(t_pre/dt)` states.
  - Source: `network.py:548-586`.
  - `int(1.0/(1/200)) = 200` and `int(1.0/0.02) = 50`.
- **F20.** The initial state is the bias (`F/network/dynamics.py:169-183`). The
  first state returned has already taken one Euler step, because the generator
  yields only after `_next_state` (`network.py:532-542`).
- **F21.** `simulate`:
  - The input must be 4-D.
  - It warns only when `dt > 1/50`, strictly greater, so 0.02 does not warn
    (`:673`).
  - `"auto"` means `steady_state(1.0, dt, B)`.
  - The output is (B, F, n_nodes). Frame i is the state after step i+1, and the
    initial state is not included.
  - It runs `with simulation(self)` and asserts that nothing requires grad
    (`:685-687`).
  - Source: `network.py:628-696`.
- **F22.** `stimulus_response` (`network.py:712-799`) runs these steps in order:
  1. Sets `stim_dataset.dt = dt` (`:744`).
  2. Computes `initial_state = steady_state(t_pre, dt, batch_size=1, value=0.5)`
     once and reuses it for every batch (`:754`).
  3. With t_fade_in=0 the fade-in returns the state unchanged (`:588-611`).
  4. Calls `stimulus.zero(B,F)`.
  5. Calls `add_input(stim.unsqueeze(-2))`.
  6. Calls `self(stimulus(), dt, state=…)`.

  The stock analysis calls it with t_pre=1.0, t_fade_in=0.0 and **batch_size=4**
  (`F/analysis/stimulus_responses.py:283-285,298,316-318`).
- **F23.** The time-constant floor is `torch.max(tau, torch.tensor(dt).float())`
  (`dynamics.py:207-218`), so the floor is f32(dt) =
  0.019999999552965164 at dt 0.02.
  - Even under the f64 default, `torch.tensor(0.02).float()` is still f32.
    `torch.max(f64, f32)` promotes the result to f64. Both were measured.
  - The Euler step itself uses the Python-float dt (`vel * dt`,
    `network.py:406`). Only the floor sees f32(dt).
- **F24.** `target_sum` is built with `torch.zeros(...)` in the default dtype
  and then `scatter_add_` (`network.py:333-351`). A dtype mismatch raises (torch
  semantics).
- **F25.** CPU `scatter_add_` determinism was measured on torch 2.14:
  - Setup: N = 45,669 and E = 1,513,231 random targets.
  - Result: 1 thread and 8 threads are bit-equal, and both are bit-equal to a
    sequential `np.add.at` in f32.
  - **UNVERIFIED:** whether the test used flyvis's `.expand()`ed index layout
    (`network.py:343-350`). That layout may take PyTorch's sort-based
    "expanded index" CPU fast path. The ATen kernel source is not installed. See
    P11.
- **F26.** Device choice:
  - `flyvis.device` is cuda when it is available and cpu otherwise. Import runs
    `torch.set_default_device(device)` (`F/__init__.py:13-14`).
  - MPS is never chosen.
  - flyvis checks only `torch.cuda.is_available()`. Setting
    `CUDA_VISIBLE_DEVICES=""` before import forces CPU, and the script can then
    assert `flyvis.device.type == "cpu"`.
- **F27.** No randomness happens in `simulate` or `steady_state`. flyvis's
  stock pipeline asserts batch-size independence via joblib's
  `ignore=['batch_size']` (`stimulus_responses.py:148`).
  - The initial state is computed at batch 1 and broadcast. Every op is
    row-independent.
  - **UNVERIFIED:** bitwise equality between batch 4 and batch 1.

### Default dtype: what depends on it, and when

- **F28.** Building a `NetworkView` touches only numpy, datamate and joblib, so
  nothing in it depends on the torch dtype (`network_view.py:99-123`;
  `connectome.py:235-264,1365-1374`).
- **F29.** The default dtype matters at **call time**, in these places:
  - The stimulus buffer, created lazily by the first `zero()`
    (`stimulus.py:161`; `Network` defaults to `init_buffer=False`,
    `network.py:122`).
  - The `target_sum` zeros, on every step (F24).
  - `torch.Tensor(...)` in `Flashes.get_item` (`flashes.py:214`).
  - `torch.ones` in `render_flash` (`flashes.py:107,120`).
  - `torch.linspace` in `resample` (`rendering/utils.py:481`), whose result is
    truncated to long.
- **F30.** `Stimulus.zero()` **reuses an existing buffer of the same (B,F)
  shape and keeps its dtype** (`stimulus.py:155-160`). A `Network` instance used
  under one precision therefore carries its buffer dtype into the next.
- **F31.** `Flashes.get_item` uses `torch.Tensor(ndarray)`, so it returns the
  default dtype: f32 normally and **f64 under the f64 default** (measured). The
  cached `flashes.h5` is f4. The values (0, 0.5, 1) are exact in both dtypes.
- **F32.** Cached `RenderedOffsets` data is **f8** with shape (8, 21, 721), and
  `_offsets = torch.tensor(np_array)` keeps it f64 (`moving_bar.py:299`;
  h5py on `F/data/renderings/*`).
  - In an f32 run, `add_input` downcasts the edge stimulus to f32 in the buffer
    (`stimulus.py:206`). In an f64 run it stays f64.
  - So moving-edge inputs **differ between the f32 and f64 runs by f32
    rounding**, and that rounding is part of the f32/f64 gap. Flash inputs are
    identical in both.
- **F33.** `linspace(0, n−1, 6).long()` gives the same indices in f32 and f64
  for every count used (measured on torch 2.14):

  | frames | indices |
  |---|---|
  | 600 | 0, 119, 239, 359, 479, 599 |
  | 525 | 0, 104, 209, 314, 419, 524 |
  | 150 | 0, 29, 59, 89, 119, 149 |
  | 131 | 0, 26, 52, 78, 104, 130 |
  | 200 | 0, 39, 79, 119, 159, 199 |
  | 50 | 0, 9, 19, 29, 39, 49 |

  The edge stimulus counts n = 125 and n = 31 also match across dtypes.

### Stimuli

- **F34.** Flashes:
  - Defaults: dt=1/200, t_pre=1, t_stim=1, radius=[-1,6], dynamic_range=[0,1],
    alternations=(0,1,0).
  - Each phase lasts `int(1.0/dt)` frames, so a sequence is 3 × that: 600 frames
    at 1/200 and 150 at 0.02 (`rendering/utils.py:481`).
  - Onset is at frame 200 / 50 and offset at frame 400 / 100
    (`flashes.py:119-123`).
  - `self.dt` is a plain attribute, so reassigning it does not re-render.
  - Source: `flashes.py:44-78,159-193`.
- **F35.** Flash sample order is `product(zip(baseline, intensity), radius)`:

  | index | baseline | intensity | radius | meaning |
  |---|---|---|---|---|
  | 0 | 0.5 | 0 | −1 | OFF, full field |
  | 1 | 0.5 | 0 | 6 | OFF, radius 6 |
  | 2 | 0.5 | 1 | −1 | ON, full field |
  | 3 | 0.5 | 1 | 6 | ON, radius 6 |

  - `radius == -1` is the full-field sentinel (`arange(721)`). Any other radius
    uses `HexLattice.filled_circle`.
  - Source: `flashes.py:109-117,186-191`; `data/flyvis-stimuli/flashes_args.csv`.
- **F36.** MovingEdge:
  - Defaults: widths=[80], bar_loc_horizontal=0.0, dt=1/200, t_pre=t_post=1.0,
    speeds=[2.4,4.8,9.7,13,19,25], offsets=(-10,11), post_pad_mode="continue",
    intensities=[0,1], height=9 (`moving_bar.py:662-695`).
  - The stock analysis passes height=80 (`stimulus_responses.py:300-310`).
  - dt is a property. Setting a *different* value only warns. Setting the
    *same* value re-runs `_resample` under the current default dtype
    (`moving_bar.py:244-262`).
  - `_resample` pads with NaN out to `t_stim_max + t_pre + t_post`
    (`moving_bar.py:340-346`; pad length `int(T/dt)`, `utils.py:557`).
- **F37.** Speed 13, timing:
  - t_stim = 21·rad(2.25)/(13·rad(5.8)) = 0.6266578249336869 s.
  - Total frames: 525 at 1/200 and 131 at 0.02.
  - Stimulus frames: 125 / 31. The pre-pad ends at frame 325 / 81.

  Speed 13, sequence index:
  - The index is built from `product(angles, widths, intensities,
    zip(t_stim, speeds))` (`moving_bar.py:211-224,350-395`).
  - With the class defaults (144 samples), the speed-13 indices are:

    | angle | OFF | ON |
    |---|---|---|
    | 0 | 3 | 9 |
    | 30 | 15 | 21 |
    | 90 | 39 | 45 |
    | 150 | 63 | 69 |

- **F38.** `MovingEdge(angles=[0,30,90,150], speeds=[13], height=80, dt=dt)`
  gives 8 samples, with idx = a·2 + i (angle-major, OFF before ON).
  - Because `t_stim_max = t(13)`, the pad difference is ≤ 0 and padding is
    skipped (`utils.py:557-559`). **No NaN appears.**
  - With the default speeds, the speed-13 sequence is 1078 / 269 frames. Its
    first 525 / 131 frames are finite, and they are identical to the subset
    route's frames.
  - The existing exporter already subsets by constructor
    (`tools/export_moving_edges.py:73`).
- **F39.** At height 80 the bar is 80·rad(2.25) = π high.
  `bar_height_px = int(π/rad(5.8)·25) = 775`, the whole monitor, with rows
  [0, 775).
  - Source: `moving_bar.py:205`; `F/datasets/rendering/eye.py:297-300,399-404`;
    `utils.py:328-331`; Rust `crates/flyvis/src/moving_edge.rs:143-149`.
  - Timing and offsets do not depend on height.
  - flyvis pads with `mode="edge"` before rotating (`utils.py:437-441`). Rust
    covers that case only through the source-coordinate clamp
    (`moving_edge.rs:192-215`), which no height-9 case has exercised.
- **F40.** `post_pad_mode="continue"` repeats the last stimulus frame (the edge
  at offset +10) for t_post (`utils.py:571-593`).
- **F41.** Background levels:
  - `bg_intensity = 0.5` is used for the pre-pad (`moving_bar.py:194,320-326`).
  - Rendered offset frames have a 127/255 = 0.498039 background, because of
    uint8 truncation. `tools/export_moving_edges.py` asserts this.
  - So the input steps from 0.5 to 0.498 at onset.
- **F42.** Input mapping:
  - `input_index` is the R1–R8 node indices, shape (8, 721).
  - `add_input` does `buffer[:, frames, input_index] += x`, which broadcasts the
    same value to all 8 types. `add_pre_stim(v)` adds to the same indices.
  - Source: `F/network/stimulus.py:128-132,173-240`; `input_units` in
    `fib25-fib19_v2.2.json`.

### Centre column and node order

- **F43.** Output node order is the built connectome order. Nodes are added in
  JSON `nodes` order (`connectome.py:226,235-241,316-323`). Lines 196-221 build
  `layout`, which has nothing to do with node order.
- **F44.** flyvis's own definition is `central_cells_index = nonzero(u==0 &
  v==0)` (`connectome.py:261-264`). The stock analyses record exactly these
  nodes (`stimulus_responses.py:41,54-56,221-223`).
- **F45.** Measured from `data/flyvis-build/nodes.csv`:
  - 45,669 nodes and 65 types.
  - **Exactly 65 nodes sit at (0,0)**, one per type.
  - Example indices: R1 360, R2 1081, L5 9012, Lawf1 9434, Lawf2 9557, Am 9979,
    TmY18 45308.
  - Every type has 721 nodes except Lawf1 and Lawf2, which have 123 each because
    they use stride [3,2]; (0,0) is still included (`connectome.py:326-342`).

### Freeze-check inputs (all 50 checkpoints, measured)

- **F46.** time_const:
  - Range 0.018780678 to 0.543077.
  - 2,396 (model, type) pairs fall below f32(0.02): 51 in model 000 and 36 in
    model 049.
  - None lies in [f32(0.02), 0.02].
  - The largest value below the floor is 0.019996209. The smallest at or above
    it is 0.020369984.
  - None falls below f32(0.005), so nothing is floored at dt 1/200.
- **F47.** syn_strength:
  - 919 of 30,200 values are exactly 0.0, on the `non_negative` boundary. None
    are negative.
  - Every model has some: min 8, median 18, max 40 (model 000 has 31, model 049
    has 15).
  - 418 connection types are zero in at least one model, and none is zero in
    all 50.
  - Source: checkpoints and `ensemble.csv`.
- **F48.** bias ranges from −1.41182 to 2.63304 and has no clamp
  (`symmetric: []`).

### Sizes

- **F49.** Frames per run:
  - dt 1/200: 4·600 + 8·525 = 6,600
  - dt 0.02: 4·150 + 8·131 = 1,648

  One all-cell f32 frame is 45,669 × 4 = 182,676 B.
- **F50.** Estimated sizes:

  | file | f32 | f64 |
  |---|---|---|
  | centre (65 nodes) | ≈ 4.3 MB | ≈ 8.6 MB |
  | snapshots | 52.6 MB | 105.2 MB |

  Plus settled ≈ 2.2 MB and settling_centre ≈ 0.4 MB, for a total of
  ≈ **173 MB**.

  For comparison: `ensemble.csv` is 1,444,857 B, `flyvis-build` is 18 MB and
  `flyvis-stimuli` is 31 MB.

### GitHub release and Actions cache

- **F51.** A release asset must be smaller than 2 GiB, and a release can have
  up to 1,000 assets.
  https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases
  (fetched 2026-10-26)
- **F52.** Actions cache:
  - 10 GB per repository by default. Eviction goes by last access, and an entry
    unused for more than 7 days is removed. No per-entry limit is documented.
  - Keys may use `hashFiles` and are at most 512 characters. `cache-hit` is true
    only on an exact match. Caches are immutable.
  - A cache is saved only if the job succeeds.
  - A run can restore caches from its own branch or the default branch, but not
    from sibling branches. PR caches are scoped to the merge ref.
  - Source: https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching
- **F53.** The `actions/cache` README shows v6, and v5 needs runner ≥ 2.327.1.
  This repo pins `@v4` (`.github/workflows/ci.yml`). v4 supports `lookup-only`
  and `fail-on-cache-miss`, and `actions/cache/restore` and `/save` exist as
  separate actions. **UNVERIFIED:** how long v4 will stay supported.
  https://github.com/actions/cache

---

## Best practices

- **P1. Use both precisions, each for its own comparison.** Running f64 on the
  f32 parameters separates arithmetic error from parameter error.
  - Rust-f64 vs flyvis-f64 is the tightest comparison. Only summation order
    differs (Higham, *Accuracy and Stability of Numerical Algorithms*, ch. 4:
    recursive sum error ≲ (n−1)·u·Σ|xᵢ|). **[GK]**
  - flyvis-f32 vs flyvis-f64 measures the reference's own f32 noise.
  - Rust-f32 vs flyvis-f32 compares two independent f32 computations. By the
    triangle inequality their difference can reach **≈ 2× the f32/f64 gap**, so
    a floor of 1× gap produces false failures. **[GK]**
  - https://pytorch.org/docs/stable/notes/numerical_accuracy.html **[GK]**
- **P2. The f32/f64 gap is a floor for f32 comparisons only.** For the f64
  floor there are two options:
  - Measure it: run f64 a second time with the edge order permuted.
  - Or use the heuristic gap × 2⁻²⁹ (ε₆₄/ε₃₂), recorded as a heuristic.
    **[GK]**
- **P3. Use a mixed tolerance and report it per cell type.**
  - Mixed tolerance: `|a−b| ≤ atol + rtol·|b|`, or normalise by each type's
    dynamic range.
  - Graded potentials cross zero, so pure ulp or relative tests fail there.
  - Report per cell type, not as one global max.
  - Sources: https://numpy.org/doc/stable/reference/generated/numpy.testing.assert_allclose.html;
    https://randomascii.wordpress.com/2012/02/25/comparing-floating-point-numbers-2012-edition/;
    https://docs.oracle.com/cd/E19957-01/806-3568/ncg_goldberg.html. **[GK]**
- **P4. Measure the gap against frame index**, as a windowed max plus its
  trend.
  - A leaky integrator damps perturbations, but recurrent loops and the ReLU
    threshold can amplify them.
  - At dt 0.02 with the floor, dt/τ = 1 for 51/65 types. That is the edge of
    explicit-Euler stability, where perturbations are damped least.
  - A tolerance taken from early frames can fail late. **[GK]** (Belongs to
    task 4; the spec should name it.)
- **P5. Record every constant the reference quantises, and assert it in the
  reference.** F23 is the instance here.
- **P6. Sample time dense on a spatial subset and sparse on all cells.** NEST
  and Brian2 regression tests use this pattern. **[GK]** Anchor snapshots to
  stimulus events as well as an even grid. See C8.
- **P7. State the frame semantics explicitly in the manifest.**
  - Frame i is the state after step i+1 from the settled state (F21).
  - Settling frame 0 is already one step in (F20).
  - Each frame's time is t = (i+1)·dt, or the formula for it, so stage 2 does
    not re-derive it.
  - Do not copy flyvis's `time = arange(n)*dt − t_pre`
    (`stimulus_responses.py:234`), which labels frame 0 as −t_pre.
- **P8. Binary format.** Write raw LE data plus a self-describing header, in the
  style of npy or safetensors.
  - Per block, record:
    - dtype string (`"<f4"`/`"<f8"`)
    - shape
    - axis names and order
    - C order
    - byte offset and byte length
  - Write with `astype("<f4")` and pass `tobytes(order="C")` deliberately.
  - Sources: https://numpy.org/doc/stable/reference/generated/numpy.lib.format.html;
    https://github.com/huggingface/safetensors. **[GK]**
- **P9. Store size information twice so the loader can check it.**
  - The loader asserts `file_len == Σ shape·itemsize`.
  - The precision appears in both the filename and the manifest dtype, and they
    must agree.
  - This catches truncation or a swapped f32/f64 file without sha256 in Rust.
- **P10. Manifest hygiene.**
  - Include a `schema_version`.
  - Write JSON with `json.dumps(..., allow_nan=False)`
    (https://docs.python.org/3/library/json.html).
  - Write offsets as integers.
  - Write dt as both a float and an exact rational string (`"1/200"`).
  - Optionally include each `.bin`'s sha256 in the manifest.
- **P11. Measure determinism instead of assuming it.**
  - Steps: set `torch.use_deterministic_algorithms(True)`, run one sequence at
    `set_num_threads(1)` and at the default thread count, and assert the two are
    bitwise equal.
  - Record the result in provenance.
  - https://pytorch.org/docs/stable/notes/randomness.html;
    https://pytorch.org/docs/stable/generated/torch.use_deterministic_algorithms.html
    **[GK]**
- **P12. Force CPU, and assert the device of every tensor.** ARM NEON and x86
  AVX can differ in the last ulps (FMA, vector width). Record the reference as
  "flyvis on <machine>". **[GK]**
- **P13. Process-global state.** `set_default_dtype` is process-wide. Either use
  one process per precision, or reset and assert the dtype before each run.
- **P14. Fail closed and write atomically.**
  - Compute and assert everything in memory first.
  - Write into a sibling temp dir, fsync, then `os.replace`.
  - Write the manifest last, as the commit marker.
  - Refuse an existing output dir unless `--force` is given. `rename(2)` onto a
    directory needs it empty.
  - Sources: https://docs.python.org/3/library/os.html#os.replace;
    https://pubs.opengroup.org/onlinepubs/9699919799/functions/rename.html;
    https://danluu.com/file-consistency/. **[GK]**
  - The existing exporters interleave writes and asserts (for example
    `export_stimuli.py:56` writes before `:66` asserts), so this is a new
    convention.
- **P15. Reruns should be idempotent.** A rerun should produce byte-identical
  `.bin` files, which is the cheapest determinism test available. Volatile
  fields (date, wall time) belong only in PROVENANCE. PROVENANCE is pinned in
  `ci/fixtures.sha256`, so any rerun needs a new tag.
- **P16. Cross-check against existing fixtures.**
  - Node order equals `data/flyvis-build/nodes.csv`.
  - The centre set equals `central_cells_index` and the (0,0) rows.
  - Frame counts equal the timing formula and `moving_edge_timing.json`.
- **P17. Assert sanity bounds and record the values, without tuning against
  them.**
  - Assert loose bounds: for example f32/f64 gap < 1e-2, and the gap no larger
    than the signal.
  - Assert that the settled state equals `steady_state(return_last=True)` bit
    for bit.
  - Record the measured values.
  - Task-8 README `:249-255`: two earlier probe bugs gave "large, internally
    consistent, entirely plausible" numbers (0.36 and 0.43).
- **P18. Fuller provenance.**
  - Python version, `platform.platform()`/`machine()`, `torch.get_num_threads()`
    and `torch.__config__.show()`.
  - sha256 of `uv.lock`, of the exporter script and of every checkpoint read,
    plus the sha256 of `ensemble.csv`.
  - `git rev-parse HEAD` plus a dirty flag.
  - The result of the determinism check.
  - Each sequence's `arg_df` row and `dataset.config.to_dict()`.
- **P19. CI data cache.**
  - Use an exact key on `hashFiles('ci/fixtures.sha256', 'ci/fixtures-tag')`
    with **no `restore-keys`**.
  - Always verify with `shasum -c` after a restore (≈ 1 s for 250 MB).
  - Deterministic tarballs (sorted, fixed mtime/owner) are optional, since
    per-file hashes are pinned. macOS bsdtar lacks `--sort`.
    https://reproducible-builds.org/docs/archives/ **[GK]**
- **P20. Mirror flyvis's own response pipeline where it is sound:**
  - Store the input that was actually fed next to the responses
    (`compute_responses`, `stimulus_responses.py:33-108`).
  - Keep the central cells.
  - Avoid HDF5/xarray, which zero-dependency Rust cannot read.
  - Trim NaN before simulating. Integration is causal, so the valid frames
    match.

---

## Prior art and repo conventions

- **R1. Exporter shape** (`tools/export_moving_edges.py:1-39`,
  `export_stimuli.py:1-29`, `export_build.py:1-9`):
  - The module docstring says why the export exists and what each artifact
    answers, and ends with `Run by hand:  uv run --project tools tools/<script>.py`.
  - `ROOT = pathlib.Path(__file__).resolve().parent.parent` and
    `OUT = ROOT / "data" / "<dir>"`.
  - Every choice is a named constant with a reason.
  - flyvis and torch are imported inside `main()`, which returns 0 and is run by
    `sys.exit(main())`.
- **R2. Binaries are raw LE**: `astype("<f4").tobytes()`, `"<u2"` with a 65535
  sentinel, or `struct.pack("<IIf")` (`export_moving_edges.py:58-59,78-80`;
  `export_build.py:145-147`).
- **R3. Every binary has a text sidecar**, written with `indent=2` and a
  trailing `\n`.
  - Example: `moving_edge_offsets.json` = `{"shape", "groups":[{"index","angle","intensity"}], "dtype":"<f4"}`
    (`export_moving_edges.py:81-90`).
  - CSVs have an `index,...` header and floats written with `{x!r}`
    (`export_stimuli.py:57-60,76-79`).
- **R4. Order follows flyvis's `arg_df`, stated in a comment.**
  - Flashes: `[OFF-full, OFF-r6, ON-full, ON-r6]`.
  - Edges: angle-major, then intensity 0 before 1, then speed
    (`export_moving_edges.py:75-77`; `moving_edge_checks.rs:163-172`).
- **R5. Assertions and output.** Each exporter checks shapes, published figures
  and physical facts, prints one line per artifact, and ends with
  `print("  all assertions passed")` (`export_build.py:176-183`).
- **R6. Provenance.**
  - The rule (`tools/README.md:59-64`): record the flyvis version, upstream
    release and date "in its output or beside it".
  - Fields used: `"exported": date.today().isoformat()`, `flyvis`, `torch`,
    `numpy`, plus every constant that shaped the export.
  - One `PROVENANCE.json` per directory. A second exporter writing into the same
    directory uses `<thing>_provenance.json` (`export_stimuli.py:84-102`;
    `export_moving_edges.py:147-164`).
- **R7. Parameter text.** `f32()` writes the shortest f32 round-trip string
  (`export_params.py:33-51`).
  - `ensemble.csv` has rows `model,kind,index,name,value` under 3 `#` header
    lines, one of which carries the 2026-09-24 date (`export_params.py:152-167`).
  - Equality checks against it must be bit-exact.
  - `export_params.py:83` loads checkpoints by explicit path with
    `torch.load(..., map_location="cpu")`, not through `NetworkView`.
- **R8. No existing exporter sets `FLYVIS_ROOT_DIR`.** Their caches went to
  `F/data/` (F5).
- **R9. `tools/README.md`.**
  - The `:51-57` table needs a row for `export_reference.py`.
  - `:26-27` is the boundary rule: nothing in Rust invokes `tools/`.
- **R10. Rust fixture loaders** (`crates/flyvis/src/test_support.rs:7-17,46-139`):
  - There is one `load_<dir>_fixture`/`_bytes(name) -> Option<_>` per data
    directory.
  - When a file is missing, the loader runs
    `assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing")`,
    then prints a skip message that **names the generating script** and
    `ci/fetch-fixtures.sh`.
  - `fixtures_required()` is true only when the variable equals `"1"` exactly.
- **R11. The gate pattern** (`crates/flyvis/src/moving_edge_checks.rs`):
  1. Load with `let (Some(bytes), Some(manifest)) = (...) else { return };`
     (`:140-145`).
  2. Assert the sidecar's shape, indices and order against constants
     (`:149-172`).
  3. Print the distribution with `eprintln!` (`:211-225`).
  4. Assert with a "do not widen" message (`:227-235`).

  `TOLERANCE = 0.005`, and its doc comment records how it was measured
  (`:41-58`).
- **R12. JSON parsing in that test.**
  - The test uses ad-hoc scanners `json_field`, `json_field_all` and
    `json_array` (`:74-135`). Because `json_field_all("index")` scans the whole
    document, key names must not collide.
  - `crate::json::parse` (`json.rs:39`, `pub(crate)`) exists. Tasks 4 and 6
    should use it for `manifest.json`.
- **R13. One CI step greps a single test's output.** It runs
  `cargo test -p semblance-flyvis -- --exact --nocapture moving_edge_checks::moving_edge_offsets_match_flyvis_within_tolerance`
  and requires `1 passed` (`ci.yml:65-73`).
  - Extending that function keeps the step valid.
  - A new test function would not appear in that log.
- **R14. `ci/fixtures.sha256`.**
  - Format: `shasum -a 256 -c`, paths relative to `data/`, 20 lines today.
  - It is generated per `docs/superpowers/plans/2026-10-01-ci.md` Task 2 Step 1:
    `cd data; { shasum -a 256 <json>; find flyvis-build flyvis-stimuli -type f ! -name '.*' | LC_ALL=C sort | xargs shasum -a 256; } > ../ci/fixtures.sha256`.
  - For fixtures-2, add `flyvis-reference` to the `find` and list
    `flyvis-pretrained/ensemble.csv` explicitly. Never run `find` over
    `flyvis-pretrained/`, which holds `results/`, two zips and
    `PROVENANCE.json`.
- **R15. Tarball and release** (`ci.md` Task 3):
  1. Build with `COPYFILE_DISABLE=1 tar -czf "$out" -C data <paths>`.
  2. Check the listing: no `._` entries, and a `diff` against the manifest
     paths.
  3. **STOP for owner sign-off.**
  4. Run `gh release create fixtures-N "$out" ... --latest=false`.
  5. Test the empty-dir and one-missing-file cases with
     `SEMBLANCE_DATA_DIR=<scratch>`.

  The asset name is fixed: `flyvis-fixtures.tar.gz` (`ci/fetch-fixtures.sh:25`).
- **R16. The fetch script never overwrites.**
  - A file that exists with the wrong hash makes it exit 1, naming the file
    (`:44-55`).
  - It fetches only missing files and verifies each in `$tmp` (`:84-94`).
  - It ends with `shasum -c` over everything (`:118-121`).
  - Its header (`:4-7,13-14`) names only build and stimuli.
- **R17. Tag and manifest change together.** `ci/fixtures-tag` = `fixtures-1`
  today, and the tag and manifest must change in one commit
  (`2026-10-01-ci-design.md:89-91`).
- **R18. An existing cache overlaps.** `ci.yml:43-46` already caches
  `data/flyvis-supplementary/...` under a key derived from the connectome
  JSON's sha.
- **R19. flyvis's stock response pipeline**
  (`F/analysis/stimulus_responses.py`):
  - It stores the stimulus as fed plus responses, for central cells only
    (`:56,223`).
  - It is joblib-cached per `NetworkView` with `ignore=['batch_size']` (`:148`),
    keyed on the dataset config.
  - It uses dt 1/200 and batch_size=4.
  - The edges use height 80, offsets (-10,11), all 6 speeds,
    post_pad_mode "continue", t_pre 1 and t_post 1 (`:292-320`).
- **R20. Earlier lesson: count-preserving lattice permutations.**
  `chore-and-fixes/README.md:326-331` records that this failure class was hit
  four times, and that the dot-subset centres are fixed points of the
  v-reversal those centres were supposed to catch.

---

## Conflicts and corrections

Places where a researcher found the brief wrong, or in conflict with a repo
convention or another source. The spec must settle each one.

### Brief facts that are wrong or incomplete

- **C1. The brief's claim that `Flashes.get_item` returns f32 is wrong under
  the f64 default.** It returns the default dtype, so f64 in the f64 runs (F31).
  This is harmless because the values are exact, but the "f64 throughout"
  assertion must expect it. The practices report also repeated the f32 claim,
  which is wrong for the same reason.
- **C2. Moving-edge inputs are not identical across precisions.** The rendered
  offsets are f64 (F32). The f32 run downcasts them in the buffer and the f64
  run does not.
  - So the f32/f64 gap includes input rounding.
  - The "f64 input == upcast f32 input, bitwise" assert (P13 and prior art)
    holds at the dataset-tensor level only, not at the buffer level. The spec
    must say which level it asserts, and record the effect in provenance.
- **C3. The brief says setting `stim_dataset.dt = dt` is "ignored by these
  datasets". That is false for MovingEdge**: setting an equal dt re-runs
  `_resample` under the current default dtype (F36).
  - The indices are measured identical across dtypes (F33), so the effect is
    benign here.
  - Avoid it anyway by calling `forward` directly, or by asserting that the
    inputs match.
- **C4. The brief says to set the default dtype before building, but every
  dtype-sensitive buffer is created lazily and reused** (F29, F30). Requirements:
  - Build a **fresh Network for each precision run**. `Stimulus.zero()` keeps an
    existing buffer's dtype.
  - Use one process per precision, or reset and assert the dtype before each
    run (P13).
- **C5. Do not call `nv.flash_responses` or `nv.moving_edge_responses`.** Their
  joblib cache key ignores the torch dtype (`stimulus_responses.py:139-183`;
  `network_view.py:352-365`), so an f64 run could silently get cached f32
  results. Call `network.stimulus_response` or `forward` directly.
- **C6. The brief does not cover sign and syn_count in the 049 parameter
  check.** They are not in `ensemble.csv`, and they are identical across all 50
  models (F15). The 049 check must also compare them against `pretrained.rs`
  `SIGN`/`SYN_COUNT`.
  - Check equality **before** `.double()`, or after an exact widen (F16).
  - Check it **again after the runs**, because `clamp()` mutates parameters in
    place (F18).
  - For f64, assert on the `_param_api()` tensors, not only on `parameters()`
    (F17).
- **C7. The brief's NaN-trimming step is moot when the edge dataset is subset
  in the constructor.** `MovingEdge(speeds=[13], angles=[...])` produces no NaN
  padding at all (F38). Keep the no-NaN assertion, and say in the spec why no
  trim happens.
- **C8. Snapshots miss every flash transient and land on pre-stimulus frames.**
  - The even grid puts the snapshots 195 ms after onset (frame 239 vs onset 200)
    and 395 ms after offset (frame 479 vs offset 400) at dt 1/200, and 180 ms
    after onset (frame 59 vs onset 50) at dt 0.02. The time constants pile up
    just under 20 ms, so the all-cell snapshots never see a transient.
  - 2 of the 6 snapshots sit in the pre-stimulus grey, near the fixed point:
    - flashes: frames 0 and 119 at dt 1/200, frames 0 and 29 at dt 0.02
    - edges: frames 0 and 104 at dt 1/200

    This repeats the epic's risk that "stage 1 may pass for the wrong reason"
    (epic README `:380-383`).
  - Settling is stored only for the centre column, so no all-cell frame from
    mid-settling exists. One would cost 182 KB.
  - Options: add event-anchored frames (onset+1, onset+k, offset+1, ...) next to
    the 6 even ones, or state that transients are checked only on the centre
    column. "6 evenly spaced" is an owner decision, so changing it needs owner
    sign-off.
- **C9. The centre column cannot see a reflection in the input mapping.** Dense
  traces at (0,0) sit on the lattice's symmetry fixed point (R20), so they are
  blind to a reflection in task 5's input mapping. Only the 6 snapshots can see
  one. Consider one off-axis dense column, or record why there is none.
- **C10. The brief's output-dir proposal conflicts with run constraints and
  repo practice.** Setting `FLYVIS_ROOT_DIR=data/flyvis-pretrained` writes
  `connectome/`, `renderings/` and `__cache__/` into the directory that
  `ensemble.csv` is published from (F4).
  - This conflicts with the constraint that flyvis runs only under a scratch
    root under `target/`, and with practice: no exporter sets the variable (R8).
  - A stale cache could also be reused silently (F6).
  - Alternative: point `FLYVIS_ROOT_DIR` at scratch and call
    `NetworkView(<absolute model path>)`, which keeps flyvis's stock load path.
    `__cache__` still appears in the model dir and must stay out of the fixture
    set (R14).
- **C11. The 1× gap floor and the missing f64 floor.**
  - Rust-f32 vs flyvis-f32 can legitimately reach ≈ 2× the f32/f64 gap (P1).
  - Nothing measures the f64-vs-f64 floor (P2).
  - The spec should record both for task 4.
- **C12. Thread-count determinism is claimed but not measured on flyvis's
  layout.** flyvis uses an expanded index (F25), and the brief's claim rests on
  the source alone. Add the 1-thread vs N-thread bitwise check (P11).
- **C13. Batch size.** The brief uses 1 and the stock analysis uses 4 (F22,
  F27). Equivalence is UNVERIFIED. Keep 1 and record it, or check the two
  bitwise.
- **C14. Minor citation errors in the brief's facts:**
  - The `__cache__` creation is at `network_view.py:109-114` via joblib. The
    cited `chkpt_utils.py:206` creates `chkpts/` (F3).
  - Node order is at `connectome.py:226,235-241,316-323`, not `:197-226` (F43).
  - The default root is `F/data`, the package dir itself, not
    "<pkg parent>/data" (F1).
- **C15. Sizes.** centre is ≈ 12.9 MB, not ~10 MB, and the total is
  ≈ 173 MB (F50).
- **C16. Don't assume which model is worst.** Recompute the argmin and argmax
  of the validation loss over all 50 at export time, assert them, and record
  each checkpoint's sha256 (F14, P18).

### Brief versus the fixture pipeline

- **C17. Never-overwrite versus re-running old exporters.**
  - Re-running `export_moving_edges.py` rewrites five fixtures-1-pinned files
    (`export_moving_edges.py:59-147`):
    - `hex_eye_owner.bin`
    - `hex_eye_kernel_sum.json`
    - `moving_edge_timing.json`
    - `moving_edges_provenance.json` (dated)
    - `moving_edge_offsets.*`
  - Re-running `export_stimuli.py` rewrites `PROVENANCE.json`.
  - `fetch-fixtures.sh` then hard-fails on every existing copy (R16). That
    breaks the brief's premise that "fixtures-1 files still verify".
  - So the h80 path must write only new `*_h80.*` files. For the
    height-independent files, assert byte equality instead of rewriting them.
  - Never regenerate `ensemble.csv`, whose header is dated.
  - Build the tarball from local files that pass `ci/fetch-fixtures.sh`.
- **C18. h80 provenance placement.**
  - Recording it only in `flyvis-reference/PROVENANCE.json` breaks the "beside
    it" rule (R6).
  - Touching `moving_edges_provenance.json` breaks C17.
  - Use a sibling `moving_edges_h80_provenance.json`.
  - The `test_support` skip message must name whichever script actually writes
    the h80 files (R10).
- **C19. The brief's single `data/` cache causes three problems.**
  - It overlaps the existing connectome-JSON cache (R18).
  - It would capture local or flyvis cache directories.
  - With `restore-keys`, it would restore stale files, which the fetch script
    then refuses to overwrite, turning CI red.

  The fix:
  - Use an exact key covering `ci/fixtures.sha256` **and** `ci/fixtures-tag`,
    with no `restore-keys`.
  - Scope the paths to the release-backed directories: `flyvis-build`,
    `flyvis-stimuli`, `flyvis-reference` and `flyvis-pretrained/ensemble.csv`.
  - Fold in the JSON cache, or replace it.
  - The cache is saved only if the job succeeds (F52). Use
    `actions/cache/restore` and `/save` around the fetch to avoid depending on
    that.
  - PR caches hit only on reruns. Main seeds the cache for later PRs (F52).
- **C20. The extended gate's two halves can mask each other.** With
  `let (Some, Some) else return`, a missing h80 file silently skips the h9 half,
  or the reverse.
  - Load the two halves independently.
  - Assert `"height": 80` in the h80 sidecar so the files cannot be swapped
    silently.
  - Label each printed table by height.
- **C21. The h80 gate exercises untested Rust code.** It runs the
  source-coordinate clamp, which no h9 case has hit (F39). Consequences:
  - Expect a different error distribution, and do not widen 0.005.
  - These comments go stale and need updating:
    - `moving_edge.rs:240` ("88 of the monitor's 775 rows")
    - `moving_edge_checks.rs:29-31` ("debug suite runs in about a second").
      The band grows to all owned pixels, about 50M comparisons in a debug
      build. Measure it.
  - Taller bars would make a negative numpy slice start wrap around, which Rust
    does not model. Height 80 is exactly at the limit.

### Brief versus epic and spec documents (they need edits)

- **C22. Epic decision 7 and the task-1 bullet disagree with the approved
  design.**
  - Epic decision 7 (`0.3.0 README:192-198`) says every cell, every k-th frame,
    f32 only, with k in PROVENANCE.
  - The task-1 bullet (`:268-271`, especially `:269`) says to store "the
    float64-vs-float32 gap".
  - The approved design stores a dense centre, 6 snapshots and both precisions,
    and stores no gap.
  - The epic README needs rewriting. Only `docs/project-outline/` holds it,
    which this task must not modify, so the owner edits it.
- **C23. The `syn_strength` freeze premise is wrong.** The finding's §5
  action 4 says "Neither passes through a max()". But `syn_strength` is clamped
  non-negative and has 919 exact zeros (F47). The brief's "relative to any
  boundary" wording covers this. The finding and the epic's expectation ("should
  not freeze") should record the measured result, not the premise.
- **C24. Task 6's use of Rust's own stimuli mixes stimulus error into the
  stage-2 gate.**
  - Task 6 (`:316`) drives stage 2 with Rust's own stimuli.
  - Rust's edge renderer agrees with flyvis only to ≤ 0.005, and its background
    is 0.5 vs flyvis's 0.498 on rotated frames (F41).
  - Edge timing and flashes are fixtured only at dt 1/200.
  - So edge traces at 30/90/150 would mix stimulus error into a gate whose
    tolerance is the arithmetic gap.
  - Option A: also export flyvis's exact input movies. They are shared across
    models and precisions: ≈ 24 MB f32 at 1/200 plus ≈ 4.7 MB at 0.02, and
    match P20.
  - Option B: gate only angle 0 element-wise.
  - Neither is in the brief: **owner decision needed.**
- **C25. Fixture documentation needs edits.**
  - The CI design table (`2026-10-01-ci-design.md:48`) says
    `data/flyvis-pretrained/` holds data no test reads.
  - The tarball paths, the fetch-script header (R16) and the fixture paragraph
    in `CLAUDE.md` ("26 of those tests skip when … three dirs") all change with
    fixtures-2.
- **C26. Flyvis order is intensity 0 (OFF) before 1 (ON).** The brief writes
  "ON/OFF", but flyvis's order, and the existing sidecar's, is OFF first,
  angle-major (R4). The manifest must use flyvis's order.
- **C27. Minor: wrong task number in the epic.** The epic's Risks section
  (`:385-388`) says "Task 4 assumes Stimulus is a scatter". That is task 5.
