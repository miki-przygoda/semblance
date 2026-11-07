# fidelity-measures: requirements extract

**Epic:** `fidelity-measures` (temper), 0.4.0 tasks 3 and 4, the parts that
read no fixtures-3. Branch `fidelity-measures`, from `graded-responses`
(task 1a, PR #19).

**Sources.** Every requirement cites one of:

- **RI**: `docs/superpowers/specs/2026-11-04-response-indices-design.md`
  (task 3). In scope: §1 and §4, and the Proof items for them.
- **RA**: `docs/superpowers/specs/2026-11-04-representational-axis-design.md`
  (task 4). In scope: §1 and §2, and the Proof items for them.
- **GR**: `docs/superpowers/specs/2026-11-04-graded-responses-design.md`
  (task 1a, already built on this branch). Cited for the types this epic
  consumes.
- **T0**: `task-0-facts.md`, which is local and gitignored. Build agents cannot
  read it, so every fact a requirement needs from it is copied inline below.
- **FV**: flyvis 1.2.0 source, read on 2026-11-05 at
  `tools/.venv/lib/python3.12/site-packages/flyvis/` on the owner's Mac (numpy
  2.5.3, xarray 2026.7.0, scipy 1.18.1). Build agents cannot read that either.
  Where a requirement needs flyvis's exact expression, the expression is quoted.
- **OD**: owner decisions in `docs/project-outline/0.4.0-fidelity-against-recordings/README.md`
  ("Owner's answers", the dated rows) and `overnight-2026-11-05.md`. Both are
  local.

**Out of scope here; they belong to the later epic `fidelity-gates`:** gates
A, B, C and D; `validate` / `--ensemble` wiring; Zhou & Hasler's 50-model runs
and `fade_in_state`; `IndexFixture` / `IndexBattery`; `score`; any test that
reads `data/flyvis-indices/`. Where a test here would need those, it uses
synthetic data or fixtures-2 instead (`data/flyvis-reference/`,
`data/flyvis-pretrained/ensemble.csv`, `ReferenceBattery`, the spec under
`data/flyvis-supplementary/`), all of which the gates already link.

**Standing constraints** (CLAUDE.md):

- No crates.io dependencies, including for tests. Adding an edge between
  workspace crates is allowed: `semblance-rng` may become a dependency of
  `semblance-flyvis`, and a dev-dependency of `semblance-fidelity`.
- `semblance-rng`'s stream is frozen. Use its existing methods only.
- Clippy must pass with `-D warnings`.
- `roundtrip` must print `0.000000`.
- `validate`'s 0.3.0 rows must stay byte-identical.
- No `mul_add`, no fast-math, no `target-cpu`.
- No version bump.
- Plain imperative commit subjects.

---

## Facts this epic consumes (inline, from GR / T0 / the code at `graded-responses`)

- **F1 `Responses`** (`semblance_sim::response`, built in 1a).
  - `Responses { cell_types, dt, precision: Precision, stimuli: Vec<StimulusResponse> }`.
  - `Precision { F32, F64 }`. Values are stored widened to f64, which is exact
    for an f32 run.
  - `StimulusResponse { label, n_frames, values }`, row-major `[n_frames][n_types]`.
  - **Row `i` is the state after `i + 1` steps from the settled state.** That
    is flyvis's frame `i`: 0.3.0's trace gates prove the alignment bit for bit.
  - Readers: `cell_types()`, `type_index(&str) -> Option<usize>`, `dt()`,
    `precision()`, `len()`, `is_empty()`, `stimulus(s) -> Option<&StimulusResponse>`;
    `label()`, `n_frames()`, `frame(i) -> Option<&[f64]>`,
    `trace(t) -> Option<impl Iterator<Item = f64>>`.
  - Construction is `Responses::new(cell_types, dt, precision)`, then
    `push(label, n_frames, values)`, then `finish()`.
  - `ResponseError`: `NoCellTypes`, `RepeatedCellType(String)`, `BadDt(f64)`,
    `NoFrames{stimulus}`, `Shape{stimulus, expected, found}`,
    `NonFinite{stimulus, frame, cell_type}`, `EmptyBattery`.
- **F2 `semblance_flyvis::parity`** (1a):
  - `respond(f: &Fixtures, params: &ModelParams, dt, precision: Dtype, counts: Counts, battery: &mut dyn Battery) -> Result<Responses, ParityError>`.
  - `respond_model(f, id, …)`.
  - `central_nodes(f)`.
  - `trait Battery { len; next_stimulus(&mut Vec<f64>) -> Result<Option<String>, ParityError> }`.
  - `ReferenceBattery::new(&Fixtures, &TraceInputs, dt_id)`.
  - `ParityError::NonFiniteResponse { stimulus, label, frame, node, cell_type }`,
    with `is_gate_failure()` true.
  - `Counts { Torch, Libm, IntegerNSyn }`.
  - Settling goes through private `settle_core::<R>(f, p, dt, n_settle, counts, centre, ctx)`.
    That function reads `f.net`, `f.groups` (one synapse-count group per edge,
    which indexes `SYN_COUNT_TORCH_F32/F64[2355]`) and `f.input_nodes`.
  - Consts: `F64_BOUND = 1e-10`, `CONTROL_MISS = 1e-3`.
- **F3 `ModelParams`**: `{ bias: [f32; 65], time_const: [f32; 65], syn_strength: [f32; 604] }`.
  That is 734 = 65 + 65 + 604 (`ROWS_PER_MODEL`). `Ensemble` holds 50 models
  `000`..`049` and provides `model(id)` and `ids()`. `N_MODELS = 50`. The
  released ensemble is sorted best to worst, so 000 is the best (T0).
- **F4 `TypedNetwork`** (`semblance_connectome`): built from
  `TypedNetworkParts { cell_types, node_cell_type: Vec<u16>, node_u: Vec<i16>, node_v: Vec<i16>, conn_types: Vec<(u16 source, u16 target)>, conn_sign: Vec<f32>, edge_source: Vec<u32>, edge_target: Vec<u32>, edge_conn_type: Vec<u16>, edge_n_syn: Vec<f32> }`
  and validated by `TypedNetwork::new`. An edge's weight is
  `conn_sign[ct] * n_syn * syn_strength[ct]`, so **the sign belongs to the
  connection type, not the strength**. There are 604 connection types.
- **F5 `semblance_rng::Rng`**: `seed_from_u64(u64)`, `next_u64`,
  `next_f32() -> [0,1)` (top 24 bits / 2^24), `below(n) -> [0,n)` (rejection
  sampling; panics on 0), `chance`, `gaussian`.
- **F6 ground truth** (`semblance_flyvis::groundtruth`; `fidelity` cannot
  depend on `flyvis`, so ground truth reaches the index code as data):
  - `POLARITY`: 65 entries, 32 of them nonzero. The 32 are R1–R8 (all ON),
    L1–L5, C3, CT1(Lo1), CT1(M10), Mi1, Mi4, Mi9, T4a–d, T5a–d, Tm1–Tm4, Tm9.
  - `PREFERRED_DIRECTIONS`: T4a 180, T4b 0, T4c 90, T4d 270, T5a 180, T5b 0,
    T5c 90, T5d 270.
  - `TUNING_CURVES: &[(&str, [f32; 12])]`, Maisak 2013, in file order T4b,
    T4a, T4c, T4d, T5b, T5a, T5c, T5d.
  - `MOTION_TUNING` = T5a, T5b, T5c, T5d, T4a, T4b, T4c, T4d.
  - `NO_MOTION_TUNING` = Tm1, Tm2, Tm4, Tm9, CT1(Lo1), Mi1, Tm3, Mi4, Mi9,
    CT1(M10).
  - `KNOWN_DSI_TYPES` = those 10 followed by the 8. That makes 18.
  - `INSUFFICIENT_DATA` and `NOISY_DATA` filter **no** metric in flyvis (T0).
- **F7 batteries** (T0):
  - **Flashes.** Radius −1 (full field) and 6, intensities 0 and 1, on grey
    0.5. `t_pre` = `t_stim` = 1 s, alternations (0,1,0), dt 1/200. That is 4
    stimuli of 600 frames. **Only radius 6 feeds the FRI.**
  - **Moving edges.** 12 angles (0..330 step 30) × intensity {0, 1} × 6
    speeds (2.4, 4.8, 9.7, 13, 19, 25 columns/s), width 80, height 80,
    offsets (−10, 11), `t_pre` = `t_post` = 1 s. That is 144 stimuli, NaN-padded
    to 1078 frames in flyvis. The valid lengths are 1078 / 739 / 567 / 525 /
    485 / 465 by speed, and the export stores valid frames only.
  - Both batteries initialise with `steady_state(t_pre=1.0, value=0.5)` and
    `t_fade_in=0`, the same as 0.3.0.
  - Every index reads the **central column only**: one node per type, 65 values.
- **F8 flyvis time axis** (FV `analysis/stimulus_responses.py`): `time = np.arange(n_frames).astype(float) * dt - t_pre`, in f64.

---

## Part 1: `semblance_fidelity::indices` (RI §1)

Pure: no files, no simulator, no `flyvis` dependency. The input is
`&Responses` plus a `BatteryLayout`, and ground truth arrives as arguments.

### Battery description

- **R1. Types.** The spec gives these signatures (RI §1):
  ```rust
  pub enum Stimulus {
      Flash { radius: i32, intensity: u8 },
      Edge { angle_deg: u16, speed: f64, intensity: u8 },
  }
  pub struct BatteryLayout { stimuli: Vec<(String, Stimulus, Window)> }
  pub struct Window { first: usize, last: usize } // inclusive frames
  ```
  The layout is matched to a `Responses` **by position**. Fields are private,
  so constructors and accessors are needed (see A1).
- **R2. `BatteryLayout::check(&Responses)`.** It refuses:
  - a different stimulus count;
  - a label mismatch, where the label at position s differs from
    `responses.stimulus(s).label()`;
  - a window past a stimulus's last frame (`window.last >= n_frames`).

  Each refusal is an `IndexError` (R20). Every index function that takes a
  layout runs this check first. (RI §1, §4)

### Windows

- **R3. `fri_window(dt)`** reproduces flyvis's own float expression instead of
  hard-coding frames. Two sources:
  - FV `analysis/flash_responses.py`:
    `time_query = f"{-dt} <= time <= {t_stim}"`, then
    `responses.query(frame=time_query)`. With dt = 1/200 that is the query
    `-0.005 <= time <= 1.0`, and the f-string round-trips `-dt` exactly.
  - F8's time axis, with `t_pre = 1.0` and `t_stim = 1.0`.

  The included frames are those with `-dt <= f*dt - t_pre && f*dt - t_pre <= t_stim`,
  evaluated in f64. Because `199*0.005 - 1.0 = -0.0050000000000000044 < -0.005`,
  **the window is frames 200..=400 (201 frames)**. That is the flash plus the
  first grey frame after it. It is computed, not hard-coded (T0, RI §1).
  The unit test pins `Window { first: 200, last: 400 }` at dt = 1/200.
- **R4. `edge_window(speed, dt, …)`** reproduces flyvis's moving-edge mask (FV
  `analysis/moving_bar_responses.py`, `peak_responses` → `get_time_masks` →
  `time_window` → `mask_between_seconds`). The defaults are offsets
  `(start, end) = (-10, 11)`.
  ```
  from_degree = start * 2.25              # -22.5
  to_degree   = (end - 1) * 2.25          #  22.5
  from_column = from_degree / 5.8
  to_column   = to_degree / 5.8
  start_in_columns = start * 2.25 / 5.8
  to_column += 2.25 / 5.8                 # "symmetric around the central column"
  t_start = (abs(start_in_columns) - abs(from_column)) / speed   # exactly 0.0
  t_end   = t_start + (to_column - from_column) / speed
  mask    = (time >= t_start) & (time <= t_end)   # time per F8
  ```
  The window is the first and last masked frame. Expected at dt = 1/200, all
  starting at frame 200:

  | speed | t_end | window |
  |---|---|---|
  | 2.4 | 3.394396551724139 | 200..=878 |
  | 4.8 | 1.6971982758620694 | 200..=539 |
  | 9.7 | 0.8398506932100962 | 200..=367 |
  | 13 | 0.6266578249336872 | 200..=325 |
  | 19 | 0.4287658802177859 | 200..=285 |
  | 25 | 0.3258620689655173 | 200..=265 |

  These were re-derived in plain Python on 2026-11-05 and agree with T0. Unit
  tests pin all six. The extra `…` parameters (offsets, `t_pre`) are the
  implementer's choice; see A2. Each window ends before the valid end of its
  speed's stimulus (F7), so no NaN padding is ever inside a window.
- **R5.** RI §1 also asks for a fixture-gated test holding the windows to task
  2's manifest. That test reads fixtures-3, so it is **deferred to
  fidelity-gates**. This epic pins the windows by unit test only.

### Dtype emulation (RI §1)

- **R6.** Each operation runs in the dtype flyvis runs it in, taken from
  `Responses::precision()`, operation by operation, in the way `hex_eye.rs`
  emulates flyvis's fp16 mask.
  - **For an F32 run, the FRI chain** (FV `flash_responses.py`) is:
    1. `stim_response = window ∩ radius==6`.
    2. `minimum = stim_response.min(dim=['frame','sample'])`, per neuron, over
       the window **and both intensities**. This is an f32 min.
    3. `stim_response += np.abs(minimum)`, an f32 add per value. This adds
       |min|; it does not subtract min.
    4. `on_peak = max over frame` (intensity 1) and `off_peak` (intensity 0).
       These are exact f32 values.
    5. `fri = on_peak - off_peak`, an f32 subtraction.
    6. `den = on_peak + off_peak` as an f32 add, then `+ np.array([1e-16])`,
       which promotes to f64: `(den_f32 as f64) + 1e-16`.
    7. `fri /= den`, an in-place numpy divide. The ufunc loop is f64
       (`(fri_f32 as f64) / den_f64`) and the result is cast to f32 on store.

    The stored FRI is therefore
    `((num as f64) / ((on + off) as f32 as f64 + 1e-16)) as f32`.
    T0 summarises it as "f32 numerator, f64 epsilon, stored f32". For an F64
    run every step is f64.
  - **DSI, preferred direction and tuning reductions run in f64** (numpy's
    complex128 and float64), starting from the peaks. A caveat that the spec
    does not raise is in A9.
- **R7.** `fri()` returns `Vec<f64>` of 65 values. For an F32 run every value
  is f32-representable (RI §1: "f32-valued for f32 runs").

### The indices (signatures as given by RI §1)

```rust
pub fn fri_peaks(r: &Responses, b: &BatteryLayout) -> Result<FriPeaks, IndexError>; // [65][off,on]
pub fn fri(p: &FriPeaks) -> Result<Vec<f64>, IndexError>;                          // 65, f32-valued for f32 runs
pub fn edge_peaks(r: &Responses, b: &BatteryLayout) -> Result<EdgePeaks, IndexError>; // [144][65]
pub fn dsi(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError>;   // per intensity
pub fn preferred_direction(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError>;
pub fn tuning_r(p: &EdgePeaks, b: &BatteryLayout, known: &[(&str, [f32; 12])])
    -> Result<TuningR, IndexError>;                                                 // [8][2][6], plus best
```

- **R8. `fri_peaks`** returns per cell type (65, in `Responses` order) the
  pair `[off, on]`, which are the shifted window peaks of R6 steps 1–4. It
  uses the layout's radius-6 flashes only: intensity 0 is OFF and intensity
  1 is ON. Radius −1 entries are ignored. `FriPeaks` must carry the
  precision (and the type names) so that `fri()` can apply R6. There must be
  exactly one radius-6 flash per intensity; anything else is an `IndexError`
  (A3).
- **R9. `fri`**: `(on - off) / (on + off + 1e-16)` with R6's dtypes. The
  epsilon is the f64 literal `1e-16`.
- **R10. `edge_peaks`**: for each edge stimulus (in layout order, 144 for the
  real battery) and each type, `peak = max(0, max over window of raw voltage)`.
  - Flyvis does it as `responses.where(masks, other=0).clip(min=0).max('frame')`:
    frames outside the window are zeroed, then everything is clipped, then
    the max is taken.
  - There is no baseline and no norm (`ensemble_view.py:270` passes none).
  - Values are exact copies of response values, or 0. A zero peak is
    `+0.0`; see A10.
- **R11. `dsi`**, per intensity (FV `direction_selectivity_index`):
  - `θ = deg2rad(angle)`, which is `angle * (π/180)` in f64.
  - `z(angle, intensity, speed) = peak · e^{iθ}`, in complex128.
  - `vector_length = |Σ_θ z|`. The `abs` is complex magnitude (hypot).
  - `normalization = max over intensity of Σ_θ |z|`. **This denominator is
    shared by ON and OFF**, per speed.
  - `dsi = vector_length / (normalization + 1e-15)`, per (intensity, speed).
  - Then **the mean over the 6 speeds** (and width, which has one value).
  - The result is `[Vec<f64>; 2]`, indexed [intensity 0, intensity 1], each
    holding 65 values in `Responses` type order.
  - The epsilon is the f64 literal `1e-15`.
- **R12. `preferred_direction`**, per intensity (FV `preferred_direction`, with
  `average=True`):
  - `θ_pref = angle(Σ over angle, speed (and width) of peak·e^{iθ})`. This is
    an **un-normalised** vector sum over all speeds, giving radians in
    (−π, π] as `atan2(im, re)`.
  - It returns `[Vec<f64>; 2]` like R11.
- **R13. `tuning_r`** (FV `correlation_to_known_tuning_curves`, `absmax=False`):
  - For each known type (8: T4a–d and T5a–d), each intensity (both kept) and
    each speed, compute the Pearson r over the 12 angles of the peaks against
    Maisak's 12-point curve.
  - Then `fillna(0)`: a zero-variance (flat) curve gives r = 0.
  - Then **the speed with the maximum r** is kept. It is the best of 6, not a
    mean, and the first is taken on ties (numpy/xarray `argmax`).
  - `TuningR` holds `[8][2][6]` plus the best r and its speed index per
    (type, intensity).
  - Flyvis's known curve is `interp1d(gt_angles, gt, kind='cubic')` evaluated
    at the same 12 angles. T0 says that is the identity to about 1e-16, so the
    values are used directly.
  - Pearson is xarray's `corr`: `mean((a−ā)(b−b̄)) / (std_a · std_b)`, with
    population std (ddof = 0).
  - See A8 about the `[f32; 12]` argument.
- **R14.** Cell types are found by name through `Responses::type_index`. A
  missing one is `IndexError::MissingCellType` (RI §1).
- **R15. Battery shape for the edge reductions.** DSI, PD and tuning need,
  for each intensity in {0, 1} and each of the 6 speeds, exactly the 12
  angles 0, 30, …, 330, each once. Otherwise the error is "the wrong number
  of angles or speeds for a reduction" (RI §1). Sums over angle run in
  ascending angle order and means over speed in ascending speed order
  (xarray's unstack sorts). See A4.

### flyvis's helpers, recomputed (RI §1, T0)

Known values are passed in as arguments. Suggested signatures are in A5.
Pearson is the R13 definition throughout. No `fillna` applies here: a NaN in
flyvis is an `IndexError` here.

- **R16. `fri_correlation_to_known`**: the Pearson r, per model, of FRI
  against the known ±1 polarity over the **32** nonzero-polarity types (F6),
  including R1–R8. Order follows `known` (FV iterates groundtruth's dict
  order).
- **R17. `dsi_correlation_to_known`**: the Pearson r over the **18**
  `KNOWN_DSI_TYPES`. The DSI is **maxed over intensity** and compared against
  a 0/1 label: 1.0 if the type is in `MOTION_TUNING`, else 0.0. FV selects
  the types with `.where(isin, drop=True)`, so **the order is the network /
  `Responses` type order**, not the known list's.
- **R18. `angular_distance_to_known`**: it returns 8 distances in the order
  T4a, T4b, T4c, T4d, T5a, T5b, T5c, T5d.
  - **T4 is read at intensity 1 and T5 at intensity 0.**
  - The known angles are a/b/c/d = π, 0, π/2, 3π/2 (180/0/90/270°).
  - Distance per FV `simple_angle_distance`, with `upper = π`:
    ```
    a = a % (2π);  b = b % (2π)          # numpy %: result has the divisor's sign (non-negative)
    y = a - b if a >= b else b - a
    if y > π: y = 2π - y
    return y / π * upper
    ```
  - It returns distances only, with no tolerance.

### Agreement rules (the paper's, recorded as such; RI §1, T0)

Signatures as given by RI §1, with possible amendments in A6:

```rust
pub fn polarity_agreement(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Agreement; // count, of, misses
pub fn median_fri(per_model: &[Vec<f64>]) -> Result<Vec<f64>, IndexError>;
pub fn binomial_upper_tail(n: u32, k: u32, p0: f64) -> f64; // exact, summed in f64
pub fn dsi_class(per_model: &[Vec<f64>], d_thresh: f64, p0: f64, alpha: f64) -> Vec<bool>;
pub fn pd_within(pd: &[f64], known_deg: &[(&str, u16)], tolerance_deg: f64) -> Agreement;
```

- **R19a. `polarity_agreement`**: sign(FRI) against the known polarity over
  the known nonzero types (32), with **no dead zone** (the paper's statement).
  `Agreement` carries the count that agree, the total (`of`), and the names
  of the misses.
- **R19b. `median_fri`**: the per-type median over models. With an even
  model count it is the mean of the two middle values, as `np.median` does.
  Callers use it over all 50 and over the top 10 (models 000–009, the
  released order). It refuses empty input, ragged lengths and non-finite
  values.
- **R19c. `binomial_upper_tail(n, k, p0)`** = P(X ≥ k) for X ~ Binomial(n,
  p0). It is exact and summed in f64 (A7). Hand values for the tests:
  - n=1, k=1, p=0.1 → 0.1
  - n=2, k=1 → 0.19
  - n=2, k=2 → 0.01
  - n=3, k=2 → 0.028
  - n=10, k=3 → 0.0701908264
  - n=10, k=4 → 0.0127951984
  - k=0 → 1.0

  At n=50 and p0=0.1: k=9 → 0.05787 and k=10 → 0.02454, so a type needs at
  least 10 of 50 models above `d_thresh`.
- **R19d. `dsi_class`**: per type, k = #{models with DSI > `d_thresh`}
  (strict). The type is direction selective iff
  `binomial_upper_tail(n_models, k, p0) < alpha` (strict). This is the paper's
  rule: `d_thresh = 0.357` is the 99% quantile of a 100× angle-permutation
  null, and the test is a one-sided binomial with H0 p = 0.1 at P < 0.05.
  Which DSI per model (the intensity reduction) is the caller's input; A6
  recommends max over intensity.
- **R19e. `pd_within`**: a type agrees when its angular distance to the known
  direction is ≤ the tolerance, in degrees. It uses
  `angular_distance_to_known`'s pairing (T4 at intensity 1, T5 at intensity 0)
  over T4a–d and T5a–d.
- **R19f. Constants, in one place, each with its citation (RI §1):**
  - `D_THRESH = 0.357`: the paper (Lappalainen et al. 2024), Methods "DSI".
  - `BINOMIAL_P0 = 0.1`: Methods, "Determining whether a cell type with
    asymmetric inputs counts as direction selective".
  - `BINOMIAL_ALPHA = 0.05`: same section.
  - `PD_TOLERANCE_DEG = 45.0`: **borrowed** from the paper's model-selection
    rule ("45° acceptance angle, assuming 225° for TmY3", Methods "Model
    selection"). The doc comment must say it is borrowed.

### `IndexError` (RI §1, §4)

- **R20.** Every variant is a refusal and never a score. The variants are:
  - battery/response count mismatch;
  - label mismatch;
  - window past the last frame;
  - missing cell type;
  - non-finite value;
  - empty input;
  - the wrong number of angles or speeds for a reduction.

  Add more only where needed (A3, A6). It implements `Display` and
  `std::error::Error`, like `ResponseError`.

---

## Part 2: gate E (RI §4)

### The legacy functions, fixed now (task 1b deletes them later)

- **R21. `compare_behaviour(a, b) -> Result<f32, FidelityError>`.** A mismatch
  in channel count, neuron count (`spike_counts.len()`) or tick count is an
  error. Today, at `crates/fidelity/src/lib.rs:47-78`, every dimension is
  `.min()`-ed and an empty comparison returns 0.0. `FidelityError` is a new
  public enum in `semblance-fidelity`. On equal shapes the arithmetic is
  unchanged.
- **R22. `behavioural_distance(…) -> Result<f32, FidelityError>`.** An empty
  battery is an error. Today, at `lib.rs:104-106`, it returns 0.0.
- **R23. `size_fidelity_curve`** propagates both errors and returns
  `Result<Vec<CurvePoint>, FidelityError>`.
- **R24.** Each fix gets a test that **fails on today's code**, using the
  chore's own reproductions:
  - (defect 2) a busy recording versus an empty one. The chore used a busy
    4-channel/500-neuron recording against a completely empty one, and today
    it scores 0.
  - (defect 3) an empty battery versus a network with every weight shifted by
    +5.0, using `Connectome::synthetic(80, 5, 9)` as in the existing tests.
    Today it scores 0; a single probe scores 0.847.
- **R25.** Replace `probe_battery_collects_probes_for_behavioural_distance`'s
  vacuous `== 0.0`. Use a non-empty battery against a *different* network and
  assert a positive distance; then the same network, and assert 0.
- **R26. `app` handles the `Result`.** `roundtrip` still prints `0.000000` for
  both distances. Grep shows `app/src/main.rs:110` (`cmd_roundtrip`) is the
  only caller. `evolve` calls only `run_probe`, which does not change (A11).
- **Versioning (RI):** these signatures are breaking changes inside 0.4.0's
  minor bump. No version number changes.

### The new path (RI §4)

- **R27.** Every `IndexError` variant (R20) and every `ResponseError` variant
  (F1) has a test. That includes the analogues of defects 2 and 3:
  - a `Responses` whose shape disagrees with the layout, which must give an
    error and not a score;
  - an empty battery. Note that `Responses::finish` refuses one, so also cover
    an empty layout or empty per-model input.

  1a may already test the `ResponseError` variants; if it does, cite those
  tests and add only what is missing. Every `SimilarityError` variant (R31)
  also gets a test, in the same spirit.

---

## Part 3: `semblance_fidelity::similarity` (RA §1)

Pure, over task 1a's `Responses`.

- **R28. `ResponseMatrix::from_responses(&Responses, onset: &[usize])`.**
  - Rows are (stimulus, frame) for every frame from each stimulus's onset to
    its end, in battery order. Columns are the cell types (65).
  - The grey pre-stimulus frames are excluded (the owner's choice), because
    they repeat the settled state.
  - The stimulus phase is **frames 200 onward** of each edge and each flash
    (T0), so the real batteries pass onset 200.
  - It refuses an `onset.len() != responses.len()` and an onset ≥ the
    stimulus's `n_frames` (see A12).
  - Storage is **f32 when `Responses::precision()` is F32** (lossless) and f64
    otherwise.
  - Size: RA says "about 125k rows". That is the NaN-padded 1078-frame count;
    with valid frames only it is about 65k (A13).
- **R29. Debiased linear CKA:**
  ```rust
  pub fn hsic_unbiased(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError>;
  pub fn cka(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError>;
  ```
  - `hsic_unbiased` is Song et al. (2012)'s unbiased estimator with linear
    kernels, **computed in feature space**: the n × n Gram matrix is never
    formed.
  - It is built from `XᵀY` (65 × 65), the row norms and the column sums, in
    O(n·p²), accumulated in f64.
  - The formula, with K̃ = XXᵀ with its diagonal zeroed and L̃ the same for Y:
    `HSIC₁ = [ tr(K̃L̃) + (1ᵀK̃1)(1ᵀL̃1)/((n−1)(n−2)) − (2/(n−2))·1ᵀK̃L̃1 ] / (n(n−3))`.
  - In feature space:
    - `tr(K̃L̃) = ‖XᵀY‖_F² − Σᵢ‖xᵢ‖²‖yᵢ‖²`
    - `1ᵀK̃1 = ‖Σᵢxᵢ‖² − Σᵢ‖xᵢ‖²`
    - `1ᵀK̃L̃1 = Σᵢ (xᵢ·s_x − ‖xᵢ‖²)(yᵢ·s_y − ‖yᵢ‖²)`, where `s_x = Σᵢ xᵢ`.
  - `cka = H(X,Y) / sqrt(H(X,X) · H(Y,Y))`.
- **R30. Exact identity and symmetry, by construction (RA §1):**
  - `H(X,X)` is the same function as `H(X,Y)` called with the same argument
    twice. So `cka(x, x)` is `H / sqrt(H·H)`, which in binary floating point
    returns exactly `|H|`, barring overflow or underflow. So `cka(x,x) == 1.0`,
    with **no special case for identical inputs**.
  - `hsic_unbiased(x, y)` must sum in an order symmetric in its arguments, so
    that `cka(a,b) == cka(b,a)` bit for bit.
  - This constrains the code. `(YᵀX)` is bitwise `(XᵀY)ᵀ`, but a row-major
    Frobenius sum is *not* symmetric under transpose. Sum the diagonal, then
    each `(a<b)` pair as `m[a][b]² + m[b][a]²`, or use another
    transpose-invariant order. Every other term is a commutative product.
- **R31. `SimilarityError`.** The variants are:
  - row-count mismatch;
  - column-count mismatch;
  - **fewer than 4 rows** (the estimator divides by n − 3);
  - a **non-positive `H(X,X)`** (and likewise `H(Y,Y)`);
  - non-finite values.
- **R32. Zhou & Hasler's variant, reported only (RA §1):**
  ```rust
  pub fn peak_vectors(r: &Responses) -> Vec<[f64; 65]>;           // per condition
  pub fn cosine_rdm(v: &[[f64; 65]]) -> Rdm;                      // + 1e-10, as theirs
  pub fn spearman(a: &Rdm, b: &Rdm) -> f64;                       // upper triangle, ties averaged
  pub fn kendall_tau_a(a: &Rdm, b: &Rdm) -> f64;
  pub fn cka_biased(x: &[[f64; 65]], y: &[[f64; 65]]) -> f64;     // on ensemble means
  ```
  The T0 facts behind it:
  - The response is **the peak central-cell raw voltage per type over the
    whole trace**: 65 values per condition, with no rectification, no
    baseline and no time bins.
  - The RDM is cosine distance (+1e-10).
  - RDMs are compared by Spearman and Kendall τA over the upper triangle
    (diagonal excluded). Spearman ties take averaged ranks. τA =
    (concordant − discordant) / (m(m−1)/2), where ties count as neither and
    m is the number of upper-triangle entries.
  - CKA is **linear and biased**: HSIC on double-centred kernels
    (`tr(KHLH)`, normalisation cancelling), computed on **ensemble-mean**
    matrices, not per model. The caller averages.
  - **No Nili permutation test.** Their p-values are not reproduced, only
    their correlations.
  - Z&H's code (github.com/michaela10c/connectome-fidelity) has **no licence**.
    Reimplement from the description and never copy it.
  - Their published values (printed by fidelity-gates, not here) are:
    pairwise trained RDM Spearman 0.721 ± 0.150 (Exp 1) and 0.838 ± 0.059
    (Exp 2); untrained 0.006 ± 0.133; trained vs random 0.686 and 0.846.

  See A14 on `Result` and the hard-coded 65.

---

## Part 4: `semblance_flyvis::parity::controls` (RA §2)

Each control is applied to each of the 50 trained models, giving **50 draws
per control**. Draws are seeded through `semblance_rng::Rng::seed_from_u64`
with a seed **derived from (control index, model index) by a fixed formula
written in the code and the record** (formula: A15). Running the 50 draws
through the battery and scoring them is gate C, which is out of scope. This
epic builds the constructions, the seeding, the redraw rule and the
`respond_network` entry point.

- **R33. zeroed**: the model's biases and time constants, with all **604**
  strengths 0. It is deterministic and consumes no RNG.
- **R34. shuffled**: the model's 604 strengths **permuted across connection
  types**, by Fisher–Yates via `Rng::below`. Biases and time constants are
  unchanged.
  - Signs stay with the connection type (F4), so every sign is kept. These
    are Z&H's sign-preserving shuffles.
  - Variant: Durstenfeld, `for i in (1..604).rev() { j = rng.below(i+1); swap(i, j) }`
    (A16).
- **R35. random parameters**: each of the **734** parameters is drawn
  uniformly between that parameter's minimum and maximum over the 50 models,
  using `Rng::next_f32`.
  - Draw order is the 734-row order: bias[0..65], time_const[0..65],
    syn_strength[0..604] (A17).
- **R36. rewired**: the model's parameters on a new topology.
  - Each connection type keeps its target type, offsets, synapse counts, sign
    and strength, and **draws a new presynaptic type**.
  - Its edges are rebuilt from the new type's node **in the same source
    column** (same `(u, v)`).
  - An edge whose source column the new type lacks is dropped, and **the
    count of dropped edges is printed**.
  - A draw that would give two connection types the same (source, target)
    pair is redrawn.
  - The result must pass `TypedNetwork::new`.
  - It is **our addition, not Z&H's** (they do not rewire; OD Q5). The record
    and the doc comment say so.
- **R37. `respond_network(f, net: &TypedNetwork, params, …)`**, a
  **control-only** entry point (OD Q5, decision 6).
  - `respond` and `score` keep taking a 734-vector over the fixed topology.
  - **The torch synapse counts follow each edge**: a kept edge keeps its
    synapse-count group (`f.groups[e]`), so exactness is unaffected.
  - The rewired network therefore needs its per-edge group vector, filtered
    in the same way as the dropped edges (A18).
  - Nodes, input nodes and central nodes are unchanged by rewiring.
  - `settle_core` / the playback helper must take the network (and groups)
    explicitly, not via `f.net`. Their behaviour for `respond` must not
    change, and 1a's bit-exact proof must still hold.
- **R38. Unstable draws.**
  - If a draw's state goes non-finite (`ParityError::NonFiniteResponse`, or a
    settling non-finite), it is redrawn **with the next seed in its
    sequence**, **up to 10 times**, as Z&H rejection-sample.
  - Every redraw is printed and recorded.
  - **Ten in a row on one draw is a gate C failure.**
  - Build this as a reusable function the gates epic calls, testable with a
    fake responder. A19 settles the count and covers zeroed, which cannot
    redraw.
- **R39. Seeds.** The same (control, model) gives the same draw.

---

## Proof items in scope

From RI's "Proof":

- **P1.** Unit tests for every index on small hand-built `Responses` with
  known answers. They must include:
  - the shared DSI denominator (an ON-only response gives OFF a DSI over ON's
    denominator);
  - the speed average;
  - the best-speed tuning choice;
  - `fillna(0)` on a flat curve;
  - the FRI `+= |min|` shift over both intensities;
  - FRI's f32 emulation (a case where f32 and f64 arithmetic give different
    stored values).
- **P2.** `binomial_upper_tail` against exact values computed by hand for
  small n (R19c).
- **P3.** The full suite, clippy `-D warnings`, `roundtrip` at 0.000000, and
  `validate`'s 0.3.0 rows unchanged.

  RI's other proof item, "Gate A's control proves gate A can fail", belongs to
  fidelity-gates.

From RA's "Proof":

- **P4.** `hsic_unbiased` against a direct Gram-matrix implementation on small
  random matrices (test-only code, **n ≤ 50**), equal **to within 1e-12
  relative**.
- **P5.** `cka(x, x) == 1.0` and `cka(a, b) == cka(b, a)` (bitwise) on random
  matrices.
- **P6.** CKA is unchanged (to **1e-12**) under an orthogonal transform and an
  isotropic scaling of one argument, and changed by a per-column scaling.
  - Build the orthogonal matrices in-test (Givens/Householder).
  - Generate random data with `semblance-rng` as a dev-dependency. It is a
    workspace crate, not crates.io.
- **P7.** `spearman`, `kendall_tau_a` and `cosine_rdm` against hand-computed
  small cases, **ties included**.
- **P8.** Each control's construction:
  - zeroed has 604 zeros;
  - shuffled is a permutation of the original, with signs unchanged;
  - random parameters lie within the per-parameter range;
  - rewired keeps every connection type's target, offsets and counts, and
    passes `TypedNetwork::new`.

  Use a synthetic network and ensemble, plus fixture-gated tests on the real
  network and `ensemble.csv` (fixtures-2), following the skip and
  `SEMBLANCE_REQUIRE_FIXTURES` rules.
- **P9.** Seeds: the same (control, model) gives the same draw.
- **P10.** If `settle_core` or the playback path is refactored for R37, 1a's
  exact-reproduction tests (`respond_model` 000 vs `centre.f32/f64.bin`) must
  still pass. They are fixture-gated on fixtures-2.

---

## Boundary with fidelity-gates

The later epic will call the following. Keep them `pub` and keep the shapes
stated.

1. **Layout construction from data.** It builds a `BatteryLayout` from
   fixtures-3's manifest stimulus table (battery, family, angle, speed,
   intensity, radius, n_frames, window).
   - It therefore needs a public constructor taking `Vec<(String, Stimulus, Window)>`.
   - It needs `Window::new(first, last)` and accessors.
   - It needs `fri_window(dt)` and `edge_window(speed, dt, …)` to cross-check
     the manifest's windows. That is RI §1's deferred fixture test.
2. **Every index function, with intermediate values exposed for gate A's
   exact and bounded rows.** Gate A compares:
   - FRI peaks `[65][2]` (off, on) against `fri_peaks.p.bin [50,65,2]`;
   - FRI against `fri.p.bin [50,65]`;
   - edge peaks `[144][65]` against `edge_peaks.p.bin [50,144,65]`;
   - DSI and PD as `[2][65]` (intensity 0, 1) against `dsi/pd.p.bin [50,2,65]`;
   - tuning r as `[8][2][6]` against `tuning_r.p.bin [50,8,2,6]`, which is in
     `MOTION_TUNING` order (T5a–d, T4a–d). So `TuningR` must be orderable or
     indexable by type name;
   - the three `*_to_known` outputs against `to_known.p.bin`.

   `FriPeaks`, `EdgePeaks` and `TuningR` need read accessors that return
   values as f64 (and precision), so the gates can apply `==` with the
   signed-zero refusal or the bound.
3. **Agreement rules and constants for gates B and C.**
   - `polarity_agreement`, `median_fri`, `binomial_upper_tail`, `dsi_class`,
     `pd_within`, `Agreement { count, of, misses }`.
   - `D_THRESH`, `BINOMIAL_P0`, `BINOMIAL_ALPHA`, `PD_TOLERANCE_DEG`.
   - Gate B runs these rules over Rust's values *and* over flyvis's exported
     values (the cross-check), so they must take plain `&[f64]`/`&[Vec<f64>]`
     plus type names, never `Responses`.
4. **Similarity for gates C and D.**
   - `ResponseMatrix::from_responses`, `cka`, `hsic_unbiased`. Gate D
     computes 1,225 pairwise CKAs over 50 matrices held at once, so
     `ResponseMatrix` must be cheap to hold (f32 at F32, as R28 says).
   - `cka` must take `&ResponseMatrix` without consuming it.
   - Z&H's functions `peak_vectors`, `cosine_rdm`, `spearman`,
     `kendall_tau_a`, `cka_biased` and `Rdm`. Gate D averages RDMs and the
     mean peak matrices, so either expose the `Rdm` values or provide a mean.
5. **Controls for gate C.**
   - The four constructions as public functions taking `(&ModelParams, …)` or
     the ensemble, and returning `ModelParams` (or, for rewired, the network
     plus groups).
   - The seed formula as a public `fn` and documented constant.
   - The redraw runner (R38) taking a closure, so the gates epic plugs in
     `respond` / `respond_network` with its batteries.
   - `respond_network`.
   - A public enum `Control { Zeroed, Shuffled, RandomParams, Rewired }` whose
     index feeds the seed. The record prints control names and redraws.
6. **Error types.** `IndexError`, `SimilarityError` and `FidelityError` need
   `Display`, so `validate` can map them to exit 2 ("did not complete") or to a
   gate failure. `ParityError` gains nothing in this epic unless R37 needs a
   variant.
7. **Dependencies.**
   - `semblance-flyvis` gains `semblance-rng` here, for the controls.
   - It gains `semblance-fidelity` in fidelity-gates, for gate A. That is
     legal because `fidelity` does not depend on `flyvis`, and nothing here
     may make `fidelity` depend on `flyvis`.
   - `fade_in_state` (Z&H initialisation) is fidelity-gates' work.

---

## Ambiguities (with recommended resolutions)

- **A1. Constructors for `BatteryLayout` and `Window`.** The spec shows
  private fields only. Recommend:
  - `BatteryLayout::new(Vec<(String, Stimulus, Window)>) -> Result<Self, IndexError>`,
    refusing an empty layout and `first > last`;
  - `Window::new(first, last) -> Result<…>`;
  - read accessors.

  Why: the gates epic builds layouts from the manifest (Boundary 1), and an
  invalid window must be a refusal (gate E).
- **A2. `edge_window`'s elided parameters.** Recommend
  `edge_window(speed: f64, dt: f64, t_pre: f64, offsets: (i32, i32)) -> Result<Window, IndexError>`
  (or a `FLYVIS_EDGE_OFFSETS = (-10, 11)` default), computed exactly as R4's
  expression sequence, with no algebraic simplification. Why: the spec
  demands flyvis's float expressions "as written". Reordering, say to
  `22.5*2/5.8`, could move a boundary frame.
- **A3. FRI battery requirements.** Recommend refusing anything other than
  exactly one radius-6 flash at intensity 0 and one at intensity 1, and
  refusing differing windows on those two. Why: flyvis takes one min over
  both intensities in one shared frame window. Two windows have no flyvis
  meaning.
- **A4. Speed set for the edge reductions.** RI's `[8][2][6]` and "averaged
  over the 6 speeds" fix the count. Recommend requiring exactly 6 distinct
  speeds, identical for both intensities, intensities exactly {0, 1}, and
  exactly the 12 angles once each. Synthetic tests can still use tiny frame
  counts (windows within frames). Alternative: allow any number of speeds of
  at least 1 with `Vec` outputs. Rejected, because it diverges from the
  spec's typed shape for no current user.
- **A5. Helper signatures (not given by RI).** Recommend:
  - `fri_correlation_to_known(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<f64, IndexError>`;
  - `dsi_correlation_to_known(dsi: &[Vec<f64>; 2], types: &[String], known_dsi_types: &[&str], motion_tuning: &[&str]) -> Result<f64, IndexError>`;
  - `angular_distance_to_known(pd: &[Vec<f64>; 2], types: &[String], known: &[(&str, u16)]) -> Result<Vec<f64>, IndexError>`
    (8 values, radians, in the R18 order).

  A zero-variance Pearson (NaN in flyvis) is an `IndexError`, not NaN. Why:
  gate E says never return a non-number as a score.
- **A6. Agreement-rule signatures return `Agreement` and `Vec<bool>`, not
  `Result`.** That contradicts R14/R20: a missing cell type or a length
  mismatch would have to panic or be skipped silently. Recommend
  `Result<Agreement, IndexError>` and `Result<Vec<bool>, IndexError>`.
  - `pd_within` needs `types: &[String]` and per-intensity input to apply the
    T4@1/T5@0 pairing. Recommend
    `pd_within(pd: &[Vec<f64>; 2], types: &[String], known_deg: &[(&str, u16)], tolerance_deg: f64) -> Result<Agreement, IndexError>`,
    with distance per R18 and agreement at `distance ≤ tolerance`.
  - `dsi_class` takes per-model per-type DSIs already reduced over intensity.
    Recommend that the gates pass max over intensity, which is
    `dsi_correlation_to_known`'s convention, and that the doc comment says so.
  - `polarity_agreement`: an FRI exactly 0.0 counts as a miss (no sign), and
    known entries with value 0 are refused (pass only the 32).

  Why: these are the minimum changes for the functions to be total and to
  obey the "refusal, never a score" rule. Record them as a spec deviation in
  the plan.
- **A7. `binomial_upper_tail` exactness.** "Exact, summed in f64."
  Recommend:
  - compute P(X ≥ k) = Σ_{i=k..n} C(n,i) p^i (1−p)^(n−i);
  - build C(n,i) as an exact u128 integer while it fits (n ≤ 50 always fits),
    then convert to f64;
  - compute the powers by `powi`;
  - sum from i = k upward;
  - k = 0 → 1.0 exactly, k > n → 0.0.

  Test against the R19c values to 1e-15 relative.
- **A8. `tuning_r`'s `known: &[(&str, [f32; 12])]`.** Widening groundtruth's
  f32 curves (0.745f32 → 0.7450000047683716) differs from flyvis's f64
  literals by about 1e-8 relative. That moves r by about 1e-8, well past
  gate A's `F64_BOUND = 1e-10`. Recommend taking `[f64; 12]`, so the gates
  epic can pass f64 values equal to flyvis's Python literals.
  - The gates epic can get those from an f64 table added to `groundtruth.rs`
    then, or recover them here by round-tripping each f32 through its
    shortest decimal string. All literals have ≤ 4 significant digits.
  - Accept `[f32; 12]` only via an explicit, documented widening helper.

  Why: otherwise gate A's f64 tuning row is designed to fail.
- **A9. f32-run Pearson dtypes.** For an F32 run, flyvis's `xr.corr` demeans
  and takes the std of the **f32** tuning peaks (and f32 FRI) in f32 before
  multiplying by f64 ground truth. RI says these reductions run in f64.
  Recommend following RI (f64) and noting the risk in a doc comment. Gate A's
  f32 rows are bounded at twice flyvis's own f32/f64 gap, which should absorb
  differences of about 1e-7. If gate A later fails only there, emulate
  numpy's f32 mean and std then. Why: emulating numpy's pairwise f32
  summation is costly and unneeded for a bounded row.
- **A10. Signed zero in edge peaks.** Flyvis's `where(…, other=0)` writes
  +0.0 outside the window, and every window starts at frame 200, so the max
  always meets +0.0. Recommend that a zero peak is +0.0. Gate A's signed-zero
  refusal will flag it if flyvis differs.
- **A11. "`app`'s `roundtrip` and `evolve` handle the `Result`."** `evolve`
  never calls `compare_behaviour` or `behavioural_distance`; it uses only
  `run_probe`. Recommend changing `roundtrip` only and noting that `evolve`
  needed nothing. Task 1b later removes `roundtrip`'s behavioural line (OD
  Q6, revised); do not do that here.
- **A12. Equal-shape but empty `Behaviour`s, and `ResponseMatrix` onset
  edges.**
  - For `compare_behaviour`: two recordings with zero channels **and** zero
    neurons compare nothing. Recommend `FidelityError::Empty` rather than
    `Ok(0.0)`, in the spirit of "never fails toward perfect".
  - A tick mismatch means either `a.ticks != b.ticks` or any channel's length
    differing; recommend checking both.
  - For `ResponseMatrix`: refuse `onset >= n_frames`, so every stimulus
    contributes at least one row. Let `cka` refuse n < 4 as R31 says.
- **A13. RA's "about 125k rows" / "1.6 GB".** That is the 1078-frame padded
  count (144 × 878 + 4 × 400). The export and `Responses` hold valid frames
  only (F7), giving 24 × Σ_speeds(valid − 200) + 1600 = 24 × 2659 + 1600 =
  65,416 rows, about 0.85 GB for 50 models at f32. No action. Tests must not
  assert 125k.
- **A14. Z&H functions with no `Result` and a hard-coded 65.** Recommend:
  - `peak_vectors(&Responses) -> Result<Vec<[f64; 65]>, SimilarityError>`,
    refusing a type count other than 65;
  - `cosine_rdm`, `spearman`, `kendall_tau_a` and `cka_biased` return
    `Result`, refusing mismatched or too-small inputs (an RDM needs 2 or more
    conditions; Spearman/τA need 2 or more upper-triangle entries and nonzero
    rank variance).
  - The cosine epsilon is placed as `d = 1 − (a·b)/(‖a‖·‖b‖ + 1e-10)`.

  Verify that placement against Z&H's paper text in fidelity-gates, since it
  is reported only. Why: same refusal rule. The epsilon's placement is not
  stated anywhere readable, the code is unlicensed, and only reported numbers
  depend on it.
- **A15. Seed formula (RA: "a fixed formula written in the code and the
  record").** Not given. Recommend:
  - `seed(control, model, attempt) = 0x5EB1_A9CE_0000_0000 ^ ((control as u64) << 24) ^ ((model as u64) << 8) ^ attempt as u64`;
  - control = 0 zeroed, 1 shuffled, 2 random parameters, 3 rewired;
  - model = 0..49 by ensemble position;
  - attempt = 0 for the first draw, +1 per redraw ("the next seed in its
    sequence").

  Any injective formula will do. Pin it with a test and a doc comment, and
  print it in the record.
- **A16. Fisher–Yates variant.** Recommend Durstenfeld, descending as in R34,
  since it uses `below` only. Pin the first few output indices for one seed
  in a test, so a change is caught. That pins our use of the stream, which is
  itself frozen.
- **A17. Random-parameter arithmetic.** Recommend
  `v = min + (max − min) * u` computed in f64 from f32 min/max and
  `u = next_f32()`, rounded to f32, then clamped to `[min, max]`. Draw order
  is R35's. When min == max (a parameter constant across models) the value
  is that constant, but a draw is still consumed so the stream position does
  not depend on the data. Why: f32 rounding could otherwise step outside the
  range that P8 asserts.
- **A18. Rewiring details the spec leaves open.**
  1. The candidate presynaptic types are drawn uniformly from all 65 via
     `below(65)`, and the original type may recur. Uniform and simplest; the
     doc says so.
  2. Connection types are processed in index order. A candidate whose
     (source, target) duplicates an already-assigned *or* still-unprocessed
     original pair is redrawn, so the final set has unique pairs. A bound,
     say 1,000 tries per type, then gives an error instead of looping forever.
  3. A kept edge's source node is the new type's node at the old source
     node's `(u, v)`. Target, `edge_conn_type`, `n_syn` and group are
     unchanged. Edge order is the original order minus dropped edges.
  4. It returns `(TypedNetwork, Vec<u32> groups, dropped: usize)`.
  5. `respond_network(f, net, groups, params, dt, precision, counts, battery)`
     reuses `f`'s input nodes, central nodes and manifest. It refuses a `net`
     whose node arrays differ from `f.net`'s.

  Why: this keeps exactness (groups follow edges) and makes "the count is
  printed" a returned value the gates epic prints.
- **A19. Redraw count off-by-one.** "Redrawn … up to 10 times" (that is, 11
  attempts) and "ten in a row on one draw is a gate C failure" (10 attempts)
  disagree by one. Recommend at most 10 attempts per draw (attempt 0..=9):
  the 10th consecutive non-finite attempt is the failure, and a draw needing
  9 redraws still counts.
  - Why: "ten in a row … is a failure" is the operative rule and the
    stricter reading. Record the choice.
  - Zeroed is deterministic: a non-finite zeroed draw fails at once, with no
    redraws, since redrawing cannot change it.
  - Only non-finite *state* triggers a redraw. Any other `ParityError`
    propagates.
- **A20. Where `IndexError`'s non-finite check applies.** `Responses` already
  refuses non-finite values at construction. Recommend that the index
  functions still check their own inputs (`FriPeaks`/`EdgePeaks` built by
  hand, the per-model slices of agreement rules, and outputs such as a NaN
  Pearson), so the variant is reachable and tested.
