# fidelity-measures Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This plan is run by temper's build workflow: each task below is built from its own text, the contracts, the fixed rules and the build rule.

**Goal:** Build the parts of 0.4.0 tasks 3 and 4 that read no fixtures-3: flyvis's response indices and the paper's agreement rules (`semblance_fidelity::indices`), gate E's refusals in the legacy fidelity functions, debiased linear CKA and Zhou & Hasler's variant (`semblance_fidelity::similarity`), and gate C's four controls with their seeded, redrawn draws and the control-only `respond_network` (`semblance_flyvis::parity::controls`).

**Architecture:** `indices` and `similarity` are pure functions over task 1a's `semblance_sim::response::Responses`; `semblance-fidelity` never depends on `semblance-flyvis`, so ground truth arrives as arguments. The controls live in `semblance-flyvis` (they read the ensemble and the network) and gain `semblance-rng` as a dependency. `respond_network` shares `respond`'s code path through a `settle_core` that takes the network and its synapse-count groups explicitly, so `respond`'s results stay bit-identical. Gates A–D, `validate --ensemble`, Zhou & Hasler's 50-model runs and `score` are the later epic `fidelity-gates`.

**Tech Stack:** Rust 1.85, edition 2024, workspace crates only (`semblance-sim`, `semblance-connectome`, `semblance-rng`, `semblance-fidelity`, `semblance-flyvis`, `semblance` app).

**Spec:**
- `docs/superpowers/specs/2026-11-04-response-indices-design.md`, §1 (`semblance_fidelity::indices`) and §4 (gate E), and the Proof items for them.
- `docs/superpowers/specs/2026-11-04-representational-axis-design.md`, §1 (`semblance_fidelity::similarity`) and §2 (`semblance_flyvis::parity::controls`), and the Proof items for them.
- `docs/superpowers/temper/fidelity-measures/requirements.md` (the requirements extract R1–R39, P1–P10, A1–A20, with every task-0 and flyvis fact the tasks need quoted inline).

Out of scope here (epic `fidelity-gates`, after fixtures-3 is staged): gates A, B, C, D; `validate` and `--ensemble` wiring; Zhou & Hasler's 50-model runs and `fade_in_state`; `IndexFixture` / `IndexBattery`; `score`; any test of the windows against task 2's manifest.

## Global Constraints

- **Zero new crates.io dependencies**, including for tests. `Cargo.lock` lists exactly 8 packages, all `semblance*`. Edges between workspace crates are allowed: `semblance-flyvis` gains `semblance-rng` (task `ctl-params`), `semblance-fidelity` gains `semblance-rng` as a dev-dependency (task `sim-cka`). `semblance-fidelity` must never depend on `semblance-flyvis`.
- **Rust 1.85, edition 2024.** `gen` is a reserved keyword.
- **No fast-math, no `mul_add`, no `target-cpu`**, anywhere (settings, code, flags). No `[profile.*]` changes: `semblance-fidelity` stays at the dev profile's default opt-level, so its tests stay small.
- **Commit subjects are plain imperative sentences**, no `feat:`/`fix:`/`chore:` prefixes (plus the trailer line the build names).
- **Fixture-gated tests follow CLAUDE.md's skip-notice rule:** the first line is `let Some(f) = fixture() else { return };` (or another `or_skip` path), so each prints its own `skipping: ...` notice when a fixture is missing, and fails under `SEMBLANCE_REQUIRE_FIXTURES=1`. Only `semblance-flyvis` has fixture-gated tests; `semblance-fidelity` has none and needs none.
- **Never read `data/flyvis-indices/`** (fixtures-3). It does not exist yet. Tests use synthetic data or fixtures-2 (`data/flyvis-reference/`, `data/flyvis-pretrained/ensemble.csv`, the spec under `data/flyvis-supplementary/`), through `crate::gate_support::fixture()`.
- **`semblance-rng`'s stream is frozen.** Use only its existing methods (`seed_from_u64`, `next_u64`, `next_f32`, `below`); never `gaussian` in controls (it is libm-dependent).
- **Clippy is clean with `-D warnings`.** If clippy flags a loop (`needless_range_loop` and the like), rewrite it with iterators or `enumerate` as clippy suggests, keeping every summation in the same order; never add an `#[allow]` for it. `#[allow(clippy::too_many_arguments)]` is allowed where a contract below names more than seven parameters (precedent: `compare_settling` in `parity/run.rs`).
- **No panics on input in library code.** Every refusal is a typed error (`IndexError`, `SimilarityError`, `FidelityError`, `ParityError`), never a score.
- **Doc comments in `semblance-fidelity` and `semblance-flyvis` use ```` ```text ```` for any fenced block**, so no doctest is created (doctests change the measured test count).
- **`roundtrip` must still print `structural distance:  0.000000` and `behavioural distance: 0.000000`**; `validate`'s 0.3.0 rows stay byte-identical; no version number changes.
- **`crates/flyvis/src/pretrained.rs` and `syn_count_torch.rs` are pinned by git blob hash**: never edit them.
- **`parity/controls.rs` is one file, not a directory**, and its tests block starts exactly `#[cfg(test)]\nmod tests` (`crates/flyvis/tests/acceptance_parity.rs` reads every file in `src/parity/` and bans `SEMBLANCE_REQUIRE_FIXTURES`, `CARGO_MANIFEST_DIR`, `env!(`, `env::var`, `current_dir` outside that block).
- **Test counts (RELEASE-11, `app/tests/acceptance_release.rs`).** CLAUDE.md's stated total (`cargo test --workspace          # N tests, 0 failures` and `"N passed, 0 failed"`) must be at least the number of literal `#[test]` lines under `app/` and `crates/`. Today it states 587 against 559 such lines: 28 of slack. This epic adds about 85, so the count is carried by one **count owner** per wave (the task whose `Owns:` lists `CLAUDE.md`): it re-measures the workspace total on its own branch and writes it in both places. The other task in the wave adds at most 15 `#[test]`s. Task `docs-counts`, last, re-measures everything. Any later phase that adds `#[test]`s (acceptance tests, fixes) re-measures CLAUDE.md's total the same way before it commits.

Measuring the total (count owners and `docs-counts`): run `env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --locked` through the build rule, take the job's full log, and sum the `passed` figures of its `test result:` lines: `grep '^test result:' <log> | awk '{s += $4} END {print s}'`.

## Gates

- Quick gate (after every task and every merge): `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`
- Full gate (checkpoint and before a PR): `sh /home/miki_przygoda/work/semblance/gates/gate-full.sh`

Both are jobq jobs on beast; the quick gate runs `cargo build --workspace --locked`, `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

## Review Focus

1. **An edge layout listed in a different speed order** (flyvis's export order need not be ascending): the DSI's mean over speeds, the summed preferred direction and `tuning_r`'s best-speed index must use ascending speed order whatever order the layout lists them in. Test: `the_edge_reductions_do_not_depend_on_the_layouts_order` in task `idx-edges` (speeds reversed, and a layout with intensities 1 then 0, speeds scrambled and angles descending).
2. **A `BatteryLayout` with a NaN, infinite, zero or negative edge speed, or an angle of 360 or more**: refused at construction (`IndexError::BadStimulus`), so no reduction ever sorts a NaN or misfiles an angle. Test: `a_layout_refuses_no_stimuli_and_impossible_edges` in task `idx-layout`.
3. **Values tagged `Precision::F32` that are not f32-representable** (a hand-built `Responses`, `FriPeaks`, `EdgePeaks` or `ResponseMatrix`): refused (`NotF32`) rather than silently rounded by the f32 emulation or the f32 storage. Tests: `fri_peaks_built_by_hand_are_checked` and `fri_peaks_refuse_an_f32_run_holding_non_f32_values` (task `idx-fri`), `edge_peaks_built_by_hand_are_checked` and `edge_peaks_refuse_an_f32_run_holding_non_f32_values` (task `idx-edges`), `rows_tagged_f32_must_be_f32s`, which includes a `from_responses` case (task `sim-cka`). `Responses` itself does not check an f32 run's values, so `fri_peaks` and `edge_peaks` check every window value and `ResponseMatrix` every stored value.
4. **`respond_network` handed synapse-count groups that do not fit** (a different length, or a group index ≥ 2355): a `ParityError::Usage`, never an out-of-bounds panic inside `settle_core`. Test: `respond_network_refuses_synapse_count_groups_that_do_not_fit_without_panicking` in task `ctl-network`.
5. **A rewiring draw that cannot find a unique (source, target) pair** for some connection type: a bounded `ParityError::Usage` after `MAX_SOURCE_TRIES`, never an endless loop. Test: `a_target_with_more_connection_types_than_cell_types_is_refused_not_looped` in task `ctl-rewire`.

## Decisions and recorded deviations from the specs

Each is the requirements extract's recommendation (its A-number), adopted here; the build records them in `log.md`.

- **A1/A2.** `BatteryLayout::new`, `Window::new` and read accessors exist; `edge_window(speed, dt, t_pre, offsets)` takes `EDGE_OFFSETS = (-10, 11)` explicitly and evaluates flyvis's expressions in flyvis's order, unsimplified.
- **A3/A4.** The FRI needs exactly one radius-6 flash at intensity 0 and one at intensity 1, with the same window. The edge reductions need intensities exactly {0, 1}, exactly 6 distinct speeds shared by both, and the 12 angles 0, 30, …, 330 once each per (intensity, speed). Anything else is `IndexError::BatteryShape`.
- **A5/A6 (deviation).** The agreement rules return `Result` (`polarity_agreement`, `dsi_class`, `pd_within`), so a missing cell type or ragged input is a refusal; `pd_within` takes `(&[Vec<f64>; 2], &[String], …)` to apply the T4-at-ON, T5-at-OFF pairing. An FRI of exactly 0.0 is a polarity miss. Known values other than ±1 are refused.
- **A7.** `binomial_upper_tail` sums exact `u128` binomial coefficients (rounded once to f64) times `powi` powers, from `i = k` up.
- **A8 (deviation).** `tuning_r` takes Maisak's curves as `[f64; 12]`, not `[f32; 12]`; `widen_curves` widens `groundtruth::TUNING_CURVES`' f32 values through their shortest decimal string, so they equal flyvis's Python literals.
- **A9.** The tuning and `*_to_known` Pearson correlations run in f64, as RI says; the risk for gate A's f32 rows is noted in a doc comment.
- **A10.** A zero edge peak is +0.0.
- **A11.** `evolve` calls only `run_probe`, which does not change; only `roundtrip` handles a new `Result`. Task 1b, not this epic, removes `roundtrip`'s behavioural line.
- **A12.** `compare_behaviour` refuses a channel-count, neuron-count, `ticks` or per-channel-length mismatch, and two recordings with nothing to compare. `ResponseMatrix` refuses an onset at or past a stimulus's last frame.
- **A13.** The real flash and edge batteries give 65,416 response-matrix rows at valid frames (not the spec's 125k, which counted NaN padding); no test asserts either.
- **A14 (deviation).** Zhou & Hasler's functions return `Result<_, SimilarityError>` and refuse a type count other than 65; the cosine epsilon is placed `1 − (a·b)/(‖a‖·‖b‖ + 1e-10)` (the placement is unverified against the paper's text; fidelity-gates checks it, since only reported numbers depend on it).
- **A15.** `control_seed(control, model, attempt) = 0x5EB1_A9CE_0000_0000 ^ (control << 24) ^ (model << 8) ^ attempt`, control 0 zeroed, 1 shuffled, 2 random parameters, 3 rewired; injective for model < 65,536 and attempt < 256.
- **A16.** Shuffling is Durstenfeld's descending Fisher–Yates via `Rng::below(i + 1)`, pinned by a test.
- **A17.** A random parameter is `(lo + (hi − lo) · u)` in f64 from f32 bounds and `u = next_f32()`, rounded to f32 and clamped to `[lo, hi]`; a constant parameter still consumes its draw.
- **A18 (refinement).** Rewiring draws each connection type's new source uniformly from all cell types with `below`, in connection-type order, and redraws while the (source, target) pair is already taken by an earlier connection type; checking only already-assigned pairs is enough for the final set to be unique (the extract also listed still-unprocessed original pairs, which would forbid pairs no final connection type holds). At most `MAX_SOURCE_TRIES = 1000` draws per connection type. It returns the network, the per-edge groups (each kept edge keeps its original group), the original index of each kept edge, the new sources and the dropped count.
- **A19 (revised in plan review).** An unstable draw is redrawn up to `MAX_REDRAWS = 10` times (attempts 0..=10: the original draw and up to 10 redraws), as RA §2 and the epic brief say; the original and all 10 redraws non-finite is gate C's failure. The requirements extract's reading (10 attempts in total) would make gate C stricter than the approved spec. Zeroed is deterministic and gets one attempt. Only a non-finite state (`ParityError::is_gate_failure()`) triggers a redraw; any other error ends the draw.
- **Printing.** "Every redraw is printed" and "the dropped count is printed" are met by returning them (`Draw::redraws`, `DrawError::Unstable`, `Rewired::dropped`): the gates epic prints them in `validate`'s output and the record. Library code here does not print; the fixture-gated tests print what they observe.
- **A20 and extra error variants.** `IndexError` adds `BadWindow`, `BadStimulus`, `BadDt`, `BadParameter`, `NotF32`, `Length`, `BadKnown` to the spec's list; `SimilarityError` adds `OnsetCount`, `OnsetPastEnd`, `NotF32`, `Length`, `NotSymmetric`, `Degenerate`, and spells "fewer than 4 rows" as `TooFew { what: "rows", need: 4, .. }`.
- **`parity::controls` is a public module, not re-exported**, because `parity::Controls` (0.3.0's controls result) already exists; `respond_network` is re-exported from `parity` beside `respond`.

---

## Contracts

### `indices-layout`

In `crates/fidelity/src/indices.rs` (module `semblance_fidelity::indices`, declared `pub mod indices;` in `crates/fidelity/src/lib.rs`):

```rust
pub const T_PRE: f64 = 1.0;
pub const FLASH_T_STIM: f64 = 1.0;
pub const FRI_RADIUS: i32 = 6;
pub const EDGE_OFFSETS: (i32, i32) = (-10, 11);
pub const N_ANGLES: usize = 12;
pub const N_SPEEDS: usize = 6;

#[derive(Clone, Debug, PartialEq)]
pub enum IndexError {
    CountMismatch { layout: usize, responses: usize },
    LabelMismatch { stimulus: usize, layout: String, responses: String },
    WindowPastEnd { stimulus: usize, last: usize, n_frames: usize },
    BadWindow { first: usize, last: usize },
    BadStimulus { stimulus: usize, reason: String },
    BadDt(f64),
    BadParameter { what: &'static str, value: f64 },
    MissingCellType(String),
    NonFinite(String),
    NotF32(String),
    Empty(&'static str),
    BatteryShape(String),
    Length { what: &'static str, expected: usize, found: usize },
    BadKnown { cell_type: String, reason: String },
}
// impl Display (strings fixed in task idx-layout) and std::error::Error.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stimulus {
    Flash { radius: i32, intensity: u8 },
    Edge { angle_deg: u16, speed: f64, intensity: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window { /* first, last: usize, inclusive */ }
impl Window {
    pub fn new(first: usize, last: usize) -> Result<Window, IndexError>; // BadWindow if first > last
    pub fn first(&self) -> usize;
    pub fn last(&self) -> usize;
    pub fn n_frames(&self) -> usize; // last - first + 1
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatteryLayout { /* stimuli: Vec<(String, Stimulus, Window)> */ }
impl BatteryLayout {
    pub fn new(stimuli: Vec<(String, Stimulus, Window)>) -> Result<Self, IndexError>; // Empty("battery layout"), BadStimulus
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn stimuli(&self) -> &[(String, Stimulus, Window)];
    pub fn check(&self, r: &Responses) -> Result<(), IndexError>; // CountMismatch, LabelMismatch, WindowPastEnd
}

pub fn fri_window(dt: f64) -> Result<Window, IndexError>;
pub fn edge_window(speed: f64, dt: f64, t_pre: f64, offsets: (i32, i32)) -> Result<Window, IndexError>;
```

Test helpers inside `indices.rs`'s `#[cfg(test)] mod tests`, for later tasks:
`fn responses(types: &[&str], precision: Precision, stimuli: &[(&str, usize)], fill: impl Fn(usize, usize, usize) -> f64) -> Responses` (dt 1/200; `fill(stimulus, frame, type)`), `fn window(first: usize, last: usize) -> Window`, `fn entry(label: &str, stimulus: Stimulus, w: Window) -> (String, Stimulus, Window)`.

### `indices-fri`

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct FriPeaks { /* cell_types, precision, peaks: Vec<[f64; 2]> ([off, on]) */ }
impl FriPeaks {
    pub fn new(cell_types: Vec<String>, precision: Precision, peaks: Vec<[f64; 2]>) -> Result<Self, IndexError>;
    pub fn cell_types(&self) -> &[String];
    pub fn precision(&self) -> Precision;
    pub fn peaks(&self) -> &[[f64; 2]];
}
pub fn fri_peaks(r: &Responses, b: &BatteryLayout) -> Result<FriPeaks, IndexError>;
pub fn fri(p: &FriPeaks) -> Result<Vec<f64>, IndexError>;
```

### `indices-edges`

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct EdgePeaks { /* cell_types, precision, peaks: Vec<Vec<f64>> ([edge in layout order][type]) */ }
impl EdgePeaks {
    pub fn new(cell_types: Vec<String>, precision: Precision, peaks: Vec<Vec<f64>>) -> Result<Self, IndexError>;
    pub fn cell_types(&self) -> &[String];
    pub fn precision(&self) -> Precision;
    pub fn peaks(&self) -> &[Vec<f64>];
}
pub fn edge_peaks(r: &Responses, b: &BatteryLayout) -> Result<EdgePeaks, IndexError>;
pub fn dsi(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError>;               // [intensity 0, 1][type]
pub fn preferred_direction(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError>; // radians in (-pi, pi]
#[derive(Clone, Debug, PartialEq)]
pub struct TuningR { /* cell_types (known order), speeds: [f64; N_SPEEDS] ascending, r: Vec<[[f64; N_SPEEDS]; 2]> */ }
impl TuningR {
    pub fn cell_types(&self) -> &[String];
    pub fn speeds(&self) -> [f64; N_SPEEDS];
    pub fn r(&self, cell_type: &str) -> Option<[[f64; N_SPEEDS]; 2]>;      // [intensity][speed]
    pub fn best(&self, cell_type: &str) -> Option<[(f64, usize); 2]>;      // per intensity: (r, speed index), first on ties
}
pub fn tuning_r(p: &EdgePeaks, b: &BatteryLayout, known: &[(&str, [f64; 12])]) -> Result<TuningR, IndexError>;
pub fn widen_curves<'a>(curves: &[(&'a str, [f32; 12])]) -> Vec<(&'a str, [f64; 12])>;
// private, for task idx-known:
fn column(types: &[String], name: &str) -> Result<usize, IndexError>; // MissingCellType
fn pearson(a: &[f64], b: &[f64]) -> f64;                               // NaN when an input is constant
const DEG: f64 = std::f64::consts::PI / 180.0;
```

Test helpers inside `indices.rs`'s `mod tests`, for task `idx-known`: `fn close(a: f64, b: f64, tol: f64) -> bool` (`(a - b).abs() <= tol * b.abs().max(1.0)`), plus `edge_layout(speeds)`, `edge_layout_in(intensities, speeds, angles)`, `edge_peaks_for(b, types, f)` and `const SPEEDS: [f64; 6]`.

### `indices-known`

```rust
pub const D_THRESH: f64 = 0.357;
pub const BINOMIAL_P0: f64 = 0.1;
pub const BINOMIAL_ALPHA: f64 = 0.05;
pub const PD_TOLERANCE_DEG: f64 = 45.0;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agreement { pub count: usize, pub of: usize, pub misses: Vec<String> }
pub fn fri_correlation_to_known(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<f64, IndexError>;
pub fn dsi_correlation_to_known(dsi: &[Vec<f64>; 2], types: &[String], known_dsi_types: &[&str], motion_tuning: &[&str]) -> Result<f64, IndexError>;
pub fn angular_distance_to_known(pd: &[Vec<f64>; 2], types: &[String], known: &[(&str, u16)]) -> Result<Vec<f64>, IndexError>; // radians, in `known` order
pub fn polarity_agreement(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<Agreement, IndexError>;
pub fn median_fri(per_model: &[Vec<f64>]) -> Result<Vec<f64>, IndexError>;
pub fn binomial_upper_tail(n: u32, k: u32, p0: f64) -> f64;
pub fn dsi_class(per_model: &[Vec<f64>], d_thresh: f64, p0: f64, alpha: f64) -> Result<Vec<bool>, IndexError>;
pub fn pd_within(pd: &[Vec<f64>; 2], types: &[String], known_deg: &[(&str, u16)], tolerance_deg: f64) -> Result<Agreement, IndexError>;
```

### `similarity-cka`

In `crates/fidelity/src/similarity.rs` (module `semblance_fidelity::similarity`):

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum SimilarityError {
    RowMismatch { x: usize, y: usize },
    ColumnMismatch { x: usize, y: usize },
    TooFew { what: &'static str, need: usize, found: usize },
    NonPositiveHsic { which: &'static str, value: f64 },
    NonFinite(String),
    OnsetCount { onsets: usize, stimuli: usize },
    OnsetPastEnd { stimulus: usize, onset: usize, n_frames: usize },
    NotF32 { row: usize, column: usize },
    Length { what: &'static str, expected: usize, found: usize },
    NotSymmetric { i: usize, j: usize },
    Degenerate(String),
}
// impl Display (strings fixed in task sim-cka) and std::error::Error.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseMatrix { /* cell_types, n_rows, f32 storage at Precision::F32, f64 otherwise */ }
impl ResponseMatrix {
    pub fn from_responses(r: &Responses, onset: &[usize]) -> Result<Self, SimilarityError>;
    pub fn from_rows(cell_types: Vec<String>, precision: Precision, rows: &[Vec<f64>]) -> Result<Self, SimilarityError>;
    pub fn n_rows(&self) -> usize;
    pub fn n_cols(&self) -> usize;
    pub fn cell_types(&self) -> &[String];
    pub fn precision(&self) -> Precision;
    pub fn row(&self, i: usize) -> Option<Vec<f64>>;
}
pub fn hsic_unbiased(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError>;
pub fn cka(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError>;
// private, for task sim-zh:
fn dot(a: &[f64], b: &[f64]) -> f64;            // sequential
fn frobenius_sq_symmetric(m: &[f64], p: usize) -> f64; // diagonal, then each a<b pair as m[a][b]^2 + m[b][a]^2
```

### `similarity-zh`

```rust
pub const ZH_TYPES: usize = 65;
#[derive(Clone, Debug, PartialEq)]
pub struct Rdm { /* n, d: Vec<f64> (n x n, row-major) */ }
impl Rdm {
    pub fn new(n: usize, d: Vec<f64>) -> Result<Rdm, SimilarityError>;
    pub fn n(&self) -> usize;
    pub fn get(&self, i: usize, j: usize) -> Option<f64>;
    pub fn upper(&self) -> Vec<f64>;                  // i < j, row-major
    pub fn mean(rdms: &[Rdm]) -> Result<Rdm, SimilarityError>;
}
pub fn peak_vectors(r: &Responses) -> Result<Vec<[f64; ZH_TYPES]>, SimilarityError>;
pub fn mean_peaks(per_model: &[Vec<[f64; ZH_TYPES]>]) -> Result<Vec<[f64; ZH_TYPES]>, SimilarityError>;
pub fn cosine_rdm(v: &[[f64; ZH_TYPES]]) -> Result<Rdm, SimilarityError>;
pub fn spearman(a: &Rdm, b: &Rdm) -> Result<f64, SimilarityError>;
pub fn kendall_tau_a(a: &Rdm, b: &Rdm) -> Result<f64, SimilarityError>;
pub fn cka_biased(x: &[[f64; ZH_TYPES]], y: &[[f64; ZH_TYPES]]) -> Result<f64, SimilarityError>;
```

### `respond-network`

In `crates/flyvis/src/parity/run.rs` and `respond.rs`; `respond_network` re-exported as `semblance_flyvis::parity::respond_network`:

```rust
// run.rs (private to parity):
#[allow(clippy::too_many_arguments)]
pub(super) fn settle_core<R: Precision>(f: &Fixtures, net: &TypedNetwork, groups: &[u32], p: &ModelParams,
    dt: f64, n_settle: usize, counts: Counts, centre: &[usize], ctx: Ctx) -> Result<(Graded<R>, Vec<f64>), ParityError>;
// respond.rs:
#[allow(clippy::too_many_arguments)]
pub fn respond_network(f: &Fixtures, net: &TypedNetwork, groups: &[u32], params: &ModelParams, dt: f64,
    precision: Dtype, counts: Counts, battery: &mut dyn Battery) -> Result<Responses, ParityError>;
// Usage if net's cell types or node arrays differ from f's network, if groups.len() != net.num_edges(),
// or if any group >= 2355. Otherwise exactly respond's code path on (net, groups).
```

### `controls-draws`

`crates/flyvis/src/parity/controls.rs`, declared `pub mod controls;` in `parity/mod.rs` (not re-exported):

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control { Zeroed, Shuffled, RandomParams, Rewired }
impl Control {
    pub const ALL: [Control; 4];
    pub const fn index(self) -> u64;          // 0, 1, 2, 3
    pub const fn name(self) -> &'static str;  // "zeroed", "shuffled", "random parameters", "rewired"
    pub const fn deterministic(self) -> bool; // true only for Zeroed
}
pub const SEED_BASE: u64 = 0x5EB1_A9CE_0000_0000;
pub const MAX_REDRAWS: u32 = 10; // attempts 0..=MAX_REDRAWS
pub fn control_seed(control: Control, model: usize, attempt: u32) -> u64;
pub fn zeroed(p: &ModelParams) -> ModelParams;
pub fn shuffled(p: &ModelParams, rng: &mut Rng) -> ModelParams;
#[derive(Clone, Debug, PartialEq)]
pub struct ParamRange { /* min, max: ModelParams */ }
impl ParamRange {
    pub fn over<'a>(models: impl IntoIterator<Item = &'a ModelParams>) -> Result<ParamRange, ParityError>;
    pub fn of_ensemble(e: &Ensemble) -> Result<ParamRange, ParityError>;
    pub fn min(&self) -> &ModelParams;
    pub fn max(&self) -> &ModelParams;
}
pub fn random_params(range: &ParamRange, rng: &mut Rng) -> ModelParams;
#[derive(Clone, Debug, PartialEq)]
pub struct Redraw { pub attempt: u32, pub seed: u64, pub error: ParityError }
#[derive(Clone, Debug, PartialEq)]
pub struct Draw<T> { pub value: T, pub attempt: u32, pub seed: u64, pub redraws: Vec<Redraw> }
#[derive(Clone, Debug, PartialEq)]
pub enum DrawError {
    Unstable { control: Control, model: usize, redraws: Vec<Redraw> },
    Failed { control: Control, model: usize, attempt: u32, error: ParityError },
}
impl DrawError { pub fn is_gate_failure(&self) -> bool; } // Unstable only
// impl Display and std::error::Error for DrawError.
pub fn draw_stable<T>(control: Control, model: usize, run: impl FnMut(u64) -> Result<T, ParityError>) -> Result<Draw<T>, DrawError>;
// private, used by ctl-rewire's tests:
fn slots(p: &ModelParams) -> impl Iterator<Item = &f32> + '_;
fn slots_mut(p: &mut ModelParams) -> impl Iterator<Item = &mut f32> + '_;
```

### `controls-rewire`

Appended to `parity/controls.rs`:

```rust
pub const MAX_SOURCE_TRIES: usize = 1000;
#[derive(Clone, Debug, PartialEq)]
pub struct Rewired { pub net: TypedNetwork, pub groups: Vec<u32>, pub kept: Vec<u32>, pub sources: Vec<u16>, pub dropped: usize }
pub fn rewire(net: &TypedNetwork, groups: &[u32], rng: &mut Rng) -> Result<Rewired, ParityError>;
```

### `fidelity-error`

In `crates/fidelity/src/lib.rs`:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum FidelityError {
    ChannelCount { a: usize, b: usize },
    NeuronCount { a: usize, b: usize },
    TickCount { a: u64, b: u64 },
    ChannelLength { channel: usize, a: usize, b: usize },
    NothingCompared,
    EmptyBattery,
}
// impl Display and std::error::Error.
pub fn compare_behaviour(a: &Behaviour, b: &Behaviour) -> Result<f32, FidelityError>;
pub fn behavioural_distance<P: Probe>(original: &Connectome, decoded: &Connectome, battery: &[P]) -> Result<f32, FidelityError>;
pub fn size_fidelity_curve<P: Probe>(reference: &Connectome, codecs: &[Box<dyn Codec>], battery: &[P]) -> Result<Vec<CurvePoint>, FidelityError>;
```

---

## Waves

| Wave | Tasks | Why |
|---|---|---|
| 0 | `idx-layout`, `ctl-network` | the shared contracts: the index module's layout, windows and error type; `settle_core` taking the network so `respond_network` exists. Separate crates. `idx-layout` is the count owner |
| 1 | `idx-fri`, `ctl-params` | the FRI on `indices-layout`; the controls module (no dependency on wave 0's flyvis change, but `parity/mod.rs` is wave 0's). Split for gate_slots: 2. `ctl-params` is the count owner |
| 2 | `idx-edges`, `ctl-rewire` | the edge indices on `indices.rs` after the FRI; rewiring appended to `controls.rs`, using `respond-network` and `controls-draws`. Split for gate_slots: 2. `idx-edges` is the count owner |
| 3 | `idx-known`, `sim-cka` | the helpers and agreement rules need `pearson` and `column` from `idx-edges`; CKA is independent and owns `lib.rs`, `Cargo.toml` and `Cargo.lock` this wave. Split for gate_slots: 2. `idx-known` is the count owner |
| 4 | `sim-zh`, `gate-e-legacy` | Zhou & Hasler's variant needs `SimilarityError` and the shared sums from `sim-cka`; gate E's legacy fixes own `lib.rs` after `sim-cka` has released it. Split for gate_slots: 2. `sim-zh` is the count owner |
| 5 | `docs-counts` | re-measures every count and documents the new modules, after every test exists |

## Behaviour groups

- `indices`: flyvis's response indices from a battery's responses (FRI, edge peaks, DSI, preferred direction, tuning correlation, the `*_to_known` helpers, and their windows)
- `agreement`: the paper's agreement rules and constants (polarity, ensemble-median FRI, binomial DSI class, preferred direction within 45°)
- `refusals`: scoring never fails toward perfect (the legacy distances refuse shape mismatches and empty batteries; every index and similarity function refuses malformed input)
- `similarity`: the representational axis (debiased linear CKA with exact identity and symmetry; Zhou & Hasler's peak-voltage RDMs, Spearman, Kendall τA and biased CKA)
- `controls`: gate C's four controls (zeroed, shuffled, random parameters, rewired), their seeds and redraws, and `respond_network` on a rewired topology

---

## Tasks

### Task idx-layout: the index module's battery layout, windows and errors

Owns: crates/fidelity/src/indices.rs, crates/fidelity/src/lib.rs, CLAUDE.md
Uses: none
Provides: indices-layout
Wave: 0

**What this builds.** A new module `semblance_fidelity::indices` (file `crates/fidelity/src/indices.rs`, created here) holding the battery description (`Stimulus`, `Window`, `BatteryLayout`), its check against a `Responses`, flyvis's two time windows (`fri_window`, `edge_window`), the module's constants and its error type `IndexError`. Later tasks append the indices to this same file. This task is also wave 0's **count owner**: it re-measures the workspace's test total and writes it into CLAUDE.md.

**Facts this task needs (task 0 and flyvis 1.2.0, quoted).**
- `semblance_sim::response::Responses` (built in task 1a, on this branch): `cell_types() -> &[String]`, `type_index(&str) -> Option<usize>`, `dt() -> f64`, `precision() -> Precision` (`Precision::{F32, F64}`), `len()` (stimulus count), `is_empty()`, `stimulus(s) -> Option<&StimulusResponse>`; `StimulusResponse::{label() -> &str, n_frames() -> usize, frame(i) -> Option<&[f64]>, trace(t) -> Option<impl Iterator<Item = f64>>}`. Construction: `Responses::new(cell_types, dt, precision)?`, then `push(label, n_frames, values)?` (values row-major `[n_frames][n_types]`), then `finish()?`. A `Responses` is never empty, every value is finite, and row `i` is flyvis's frame `i`.
- flyvis's time axis (`analysis/stimulus_responses.py`): `time = np.arange(n_frames).astype(float) * dt - t_pre`, in f64; Rust's `f as f64 * dt - t_pre` (multiply, then subtract, two roundings; rustc never fuses) is the same computation.
- The FRI window (`analysis/flash_responses.py`): the query `f"{-dt} <= time <= {t_stim}"`, with `t_pre = t_stim = 1.0`. The f-string round-trips `-dt` exactly. At dt = 1/200, `199*0.005 - 1.0 = -0.0050000000000000044 < -0.005`, so the window is **frames 200..=400** (201 frames); at dt = 1/50 it is **frames 50..=100**.
- The moving-edge window (`analysis/moving_bar_responses.py`, `peak_responses` → `get_time_masks` → `time_window` → `mask_between_seconds`), offsets `(start, end) = (-10, 11)`, evaluated in exactly this order:
  ```text
  from_degree = start * 2.25
  to_degree   = (end - 1) * 2.25
  from_column = from_degree / 5.8
  to_column   = to_degree / 5.8
  start_in_columns = start * 2.25 / 5.8
  to_column += 2.25 / 5.8
  t_start = (abs(start_in_columns) - abs(from_column)) / speed     # exactly 0.0
  t_end   = t_start + (to_column - from_column) / speed
  mask    = (time >= t_start) & (time <= t_end)
  ```
  Windows at dt = 1/200, all from frame 200: speed 2.4 → 200..=878; 4.8 → 200..=539; 9.7 → 200..=367; 13 → 200..=325; 19 → 200..=285; 25 → 200..=265. At dt = 1/50, all from frame 50: 2.4 → 50..=219; 4.8 → 50..=134; 9.7 → 50..=91; 13 → 50..=81; 19 → 50..=71; 25 → 50..=66. Speeds outside the battery, dt 1/200: 5.0 → 200..=525; 7.0 → 200..=432. `edge_window` evaluates the expressions; it never looks speeds up in a table.

**Files.**
- Create: `crates/fidelity/src/indices.rs`
- Modify: `crates/fidelity/src/lib.rs` (declare the module, one doc bullet)
- Modify: `CLAUDE.md` (the two test-total figures only)

- [ ] **Step 1: Write the failing tests.** Create `crates/fidelity/src/indices.rs` with only this tests block for now (it will not compile), and add `pub mod indices;` to `lib.rs` (Step 3 shows where).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use semblance_sim::response::{Precision, Responses};

    /// Responses over `types` at dt 1/200: one stimulus per `(label,
    /// n_frames)`, each value `fill(stimulus, frame, type)`.
    fn responses(
        types: &[&str],
        precision: Precision,
        stimuli: &[(&str, usize)],
        fill: impl Fn(usize, usize, usize) -> f64,
    ) -> Responses {
        let names = types.iter().map(|t| t.to_string()).collect();
        let mut r = Responses::new(names, 1.0 / 200.0, precision).unwrap();
        for (s, &(label, n)) in stimuli.iter().enumerate() {
            let mut values = Vec::with_capacity(n * types.len());
            for f in 0..n {
                for t in 0..types.len() {
                    values.push(fill(s, f, t));
                }
            }
            r.push(label.to_string(), n, values).unwrap();
        }
        r.finish().unwrap()
    }

    fn window(first: usize, last: usize) -> Window {
        Window::new(first, last).unwrap()
    }

    fn entry(label: &str, stimulus: Stimulus, w: Window) -> (String, Stimulus, Window) {
        (label.to_string(), stimulus, w)
    }

    #[test]
    fn the_fri_window_is_flyviss_query_frames_200_to_400_at_1_200() {
        assert_eq!(fri_window(1.0 / 200.0), Ok(window(200, 400)));
        assert_eq!(fri_window(1.0 / 50.0), Ok(window(50, 100)));
    }

    #[test]
    fn the_edge_windows_are_flyviss_masks() {
        let table = [(2.4, 878), (4.8, 539), (9.7, 367), (13.0, 325), (19.0, 285), (25.0, 265)];
        for (speed, last) in table {
            assert_eq!(edge_window(speed, 1.0 / 200.0, T_PRE, EDGE_OFFSETS), Ok(window(200, last)), "speed {speed}");
        }
        // evaluated from flyvis's expressions, not a table: dt 1/50, and
        // speeds the battery does not use
        let table = [(2.4, 219), (4.8, 134), (9.7, 91), (13.0, 81), (19.0, 71), (25.0, 66)];
        for (speed, last) in table {
            assert_eq!(edge_window(speed, 1.0 / 50.0, T_PRE, EDGE_OFFSETS), Ok(window(50, last)), "speed {speed} at 1/50");
        }
        assert_eq!(edge_window(5.0, 1.0 / 200.0, T_PRE, EDGE_OFFSETS), Ok(window(200, 525)));
        assert_eq!(edge_window(7.0, 1.0 / 200.0, T_PRE, EDGE_OFFSETS), Ok(window(200, 432)));
    }

    #[test]
    fn windows_refuse_a_bad_dt_speed_or_t_pre() {
        for dt in [0.0, -0.005, f64::NAN, f64::INFINITY, 1e-12] {
            assert!(matches!(fri_window(dt), Err(IndexError::BadDt(_))), "dt {dt}");
            assert!(matches!(edge_window(19.0, dt, T_PRE, EDGE_OFFSETS), Err(IndexError::BadDt(_))), "dt {dt}");
        }
        for speed in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let e = edge_window(speed, 0.005, T_PRE, EDGE_OFFSETS);
            assert!(matches!(e, Err(IndexError::BadParameter { what: "edge speed", .. })), "speed {speed}: {e:?}");
        }
        let e = edge_window(19.0, 0.005, f64::NAN, EDGE_OFFSETS);
        assert!(matches!(e, Err(IndexError::BadParameter { what: "t_pre", .. })), "{e:?}");
    }

    #[test]
    fn a_window_that_ends_before_it_starts_is_refused() {
        assert_eq!(Window::new(5, 4), Err(IndexError::BadWindow { first: 5, last: 4 }));
        let w = window(3, 3);
        assert_eq!((w.first(), w.last(), w.n_frames()), (3, 3, 1));
    }

    #[test]
    fn a_layout_refuses_no_stimuli_and_impossible_edges() {
        assert_eq!(BatteryLayout::new(vec![]), Err(IndexError::Empty("battery layout")));
        for (speed, angle) in [(f64::NAN, 0), (0.0, 0), (-2.4, 0), (f64::INFINITY, 0), (2.4, 360)] {
            let e = BatteryLayout::new(vec![entry("e", Stimulus::Edge { angle_deg: angle, speed, intensity: 1 }, window(0, 0))]);
            assert!(matches!(e, Err(IndexError::BadStimulus { stimulus: 0, .. })), "speed {speed}, angle {angle}: {e:?}");
        }
    }

    #[test]
    fn check_accepts_a_matching_layout_and_refuses_every_mismatch() {
        let r = responses(&["A", "B"], Precision::F64, &[("off", 4), ("on", 4)], |_, f, _| f as f64);
        let flash = |intensity| Stimulus::Flash { radius: 6, intensity };
        let good = BatteryLayout::new(vec![entry("off", flash(0), window(1, 3)), entry("on", flash(1), window(1, 3))]).unwrap();
        assert_eq!(good.check(&r), Ok(()));
        assert_eq!((good.len(), good.is_empty(), good.stimuli()[1].0.as_str()), (2, false, "on"));

        // Chore defect 2's analogue: a battery whose shape disagrees with the
        // responses is an error, never a score.
        let short = BatteryLayout::new(vec![entry("off", flash(0), window(1, 3))]).unwrap();
        assert_eq!(short.check(&r), Err(IndexError::CountMismatch { layout: 1, responses: 2 }));
        let renamed = BatteryLayout::new(vec![entry("off", flash(0), window(1, 3)), entry("ON", flash(1), window(1, 3))]).unwrap();
        assert_eq!(
            renamed.check(&r),
            Err(IndexError::LabelMismatch { stimulus: 1, layout: "ON".into(), responses: "on".into() })
        );
        let long = BatteryLayout::new(vec![entry("off", flash(0), window(1, 4)), entry("on", flash(1), window(1, 3))]).unwrap();
        assert_eq!(long.check(&r), Err(IndexError::WindowPastEnd { stimulus: 0, last: 4, n_frames: 4 }));
    }

    #[test]
    fn errors_display_as_registered() {
        let cases = [
            (IndexError::CountMismatch { layout: 148, responses: 147 }, "the layout has 148 stimuli, the responses 147"),
            (
                IndexError::LabelMismatch { stimulus: 2, layout: "a".into(), responses: "b".into() },
                "stimulus 2: the layout's label is \"a\", the responses' \"b\"",
            ),
            (
                IndexError::WindowPastEnd { stimulus: 3, last: 878, n_frames: 739 },
                "stimulus 3: its window ends at frame 878, past its 739 frames",
            ),
            (IndexError::BadWindow { first: 5, last: 4 }, "a window from frame 5 to frame 4 ends before it starts"),
            (IndexError::BadDt(0.0), "dt 0 is not finite and positive"),
            (IndexError::BadParameter { what: "edge speed", value: -1.0 }, "edge speed -1 is out of range"),
            (IndexError::MissingCellType("T4a".into()), "cell type T4a is not in the responses"),
            (IndexError::NonFinite("the FRI of Mi1".into()), "the FRI of Mi1 is not finite"),
            (IndexError::NotF32("the FRI peak of Mi1".into()), "the FRI peak of Mi1 is not an f32, in an f32 run"),
            (IndexError::Empty("battery layout"), "battery layout is empty"),
            (IndexError::BatteryShape("no edge at intensity 1".into()), "battery shape: no edge at intensity 1"),
            (IndexError::Length { what: "FRI", expected: 65, found: 64 }, "FRI: expected 65, found 64"),
            (
                IndexError::BadKnown { cell_type: "Mi1".into(), reason: "polarity 0 is not +1 or -1".into() },
                "known value for Mi1: polarity 0 is not +1 or -1",
            ),
        ];
        for (e, want) in cases {
            assert_eq!(e.to_string(), want);
        }
        let e = IndexError::BadStimulus { stimulus: 7, reason: "edge angle 360 is not below 360".into() };
        assert_eq!(e.to_string(), "stimulus 7: edge angle 360 is not below 360");
    }
}
```

- [ ] **Step 2: Run the tests to see them fail.** Run (through the build rule): `cargo test -p semblance-fidelity indices::`. Expected: compile errors (`cannot find function fri_window`, `cannot find type Window`, …).

- [ ] **Step 3: Write the implementation.** Put this above the tests block in `crates/fidelity/src/indices.rs`:

```rust
//! flyvis's four response indices, their time windows and the paper's
//! agreement rules, as pure functions over task 1a's [`Responses`].
//!
//! No files, no simulator, no `semblance-flyvis`: ground truth arrives as
//! arguments. A battery is described by a [`BatteryLayout`], matched to a
//! `Responses` by position. Every malformed input is an [`IndexError`],
//! never a score.
//!
//! Spec: docs/superpowers/specs/2026-11-04-response-indices-design.md,
//! sections 1 and 4.

use std::fmt;

use semblance_sim::response::Responses;

/// flyvis's `t_pre` for the flash and moving-edge batteries, in seconds.
pub const T_PRE: f64 = 1.0;
/// The flash battery's `t_stim`, in seconds: the FRI window's end.
pub const FLASH_T_STIM: f64 = 1.0;
/// The only flash radius the FRI reads (task 0: radius -1 never feeds it).
pub const FRI_RADIUS: i32 = 6;
/// flyvis's moving-edge offsets `(start, end)`, `peak_responses`' defaults.
pub const EDGE_OFFSETS: (i32, i32) = (-10, 11);
/// Angles per (intensity, speed) in the edge reductions: 0, 30, ..., 330.
pub const N_ANGLES: usize = 12;
/// Speeds in the edge reductions.
pub const N_SPEEDS: usize = 6;
/// The most frames a window search will scan.
const MAX_WINDOW_FRAMES: f64 = 1e7;

/// Why an index could not be computed. Each is a refusal, never a score.
#[derive(Clone, Debug, PartialEq)]
pub enum IndexError {
    /// The layout and the responses hold different numbers of stimuli.
    CountMismatch { layout: usize, responses: usize },
    /// The layout's label at a position is not the responses' label there.
    LabelMismatch { stimulus: usize, layout: String, responses: String },
    /// A window ends at or past its stimulus's last frame.
    WindowPastEnd { stimulus: usize, last: usize, n_frames: usize },
    /// A window whose first frame is after its last.
    BadWindow { first: usize, last: usize },
    /// A stimulus no flyvis battery could hold (a NaN speed, say).
    BadStimulus { stimulus: usize, reason: String },
    /// A dt that is not finite and positive, or too small to scan.
    BadDt(f64),
    /// A numeric argument out of its range.
    BadParameter { what: &'static str, value: f64 },
    /// A cell type the computation needs is not in its input.
    MissingCellType(String),
    /// A value, input or result, that is NaN or infinite.
    NonFinite(String),
    /// A value tagged as from an f32 run that is not an f32.
    NotF32(String),
    /// An input with nothing in it.
    Empty(&'static str),
    /// The wrong stimuli for a reduction: angles, speeds, intensities or the
    /// FRI's flashes.
    BatteryShape(String),
    /// An input of the wrong length.
    Length { what: &'static str, expected: usize, found: usize },
    /// A known (ground-truth) value the rule cannot use.
    BadKnown { cell_type: String, reason: String },
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IndexError::CountMismatch { layout, responses } => {
                write!(f, "the layout has {layout} stimuli, the responses {responses}")
            }
            IndexError::LabelMismatch { stimulus, layout, responses } => {
                write!(f, "stimulus {stimulus}: the layout's label is {layout:?}, the responses' {responses:?}")
            }
            IndexError::WindowPastEnd { stimulus, last, n_frames } => {
                write!(f, "stimulus {stimulus}: its window ends at frame {last}, past its {n_frames} frames")
            }
            IndexError::BadWindow { first, last } => {
                write!(f, "a window from frame {first} to frame {last} ends before it starts")
            }
            IndexError::BadStimulus { stimulus, reason } => write!(f, "stimulus {stimulus}: {reason}"),
            IndexError::BadDt(dt) => write!(f, "dt {dt} is not finite and positive"),
            IndexError::BadParameter { what, value } => write!(f, "{what} {value} is out of range"),
            IndexError::MissingCellType(t) => write!(f, "cell type {t} is not in the responses"),
            IndexError::NonFinite(what) => write!(f, "{what} is not finite"),
            IndexError::NotF32(what) => write!(f, "{what} is not an f32, in an f32 run"),
            IndexError::Empty(what) => write!(f, "{what} is empty"),
            IndexError::BatteryShape(m) => write!(f, "battery shape: {m}"),
            IndexError::Length { what, expected, found } => write!(f, "{what}: expected {expected}, found {found}"),
            IndexError::BadKnown { cell_type, reason } => write!(f, "known value for {cell_type}: {reason}"),
        }
    }
}

impl std::error::Error for IndexError {}

/// One stimulus of a battery, as flyvis's datasets describe it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stimulus {
    /// A flash: radius -1 is full field; intensity 0 is OFF, 1 is ON.
    Flash { radius: i32, intensity: u8 },
    /// A moving edge: direction in degrees, speed in columns per second;
    /// intensity 0 is OFF, 1 is ON.
    Edge { angle_deg: u16, speed: f64, intensity: u8 },
}

/// An inclusive frame window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    first: usize,
    last: usize,
}

impl Window {
    /// The frames `first..=last`; refused if `first > last`.
    pub fn new(first: usize, last: usize) -> Result<Window, IndexError> {
        if first > last {
            return Err(IndexError::BadWindow { first, last });
        }
        Ok(Window { first, last })
    }

    pub fn first(&self) -> usize {
        self.first
    }

    pub fn last(&self) -> usize {
        self.last
    }

    /// How many frames the window holds.
    pub fn n_frames(&self) -> usize {
        self.last - self.first + 1
    }
}

/// A battery's stimuli, in battery order: each one's label, what it is, and
/// the frames its index reads. Matched to a [`Responses`] by position.
#[derive(Clone, Debug, PartialEq)]
pub struct BatteryLayout {
    stimuli: Vec<(String, Stimulus, Window)>,
}

impl BatteryLayout {
    /// Refuses an empty layout, and an edge whose speed is not finite and
    /// positive or whose angle is 360 or more.
    pub fn new(stimuli: Vec<(String, Stimulus, Window)>) -> Result<Self, IndexError> {
        if stimuli.is_empty() {
            return Err(IndexError::Empty("battery layout"));
        }
        for (s, (_, stimulus, _)) in stimuli.iter().enumerate() {
            if let Stimulus::Edge { angle_deg, speed, .. } = *stimulus {
                if !(speed.is_finite() && speed > 0.0) {
                    let reason = format!("edge speed {speed} is not finite and positive");
                    return Err(IndexError::BadStimulus { stimulus: s, reason });
                }
                if angle_deg >= 360 {
                    let reason = format!("edge angle {angle_deg} is not below 360");
                    return Err(IndexError::BadStimulus { stimulus: s, reason });
                }
            }
        }
        Ok(BatteryLayout { stimuli })
    }

    /// The number of stimuli.
    pub fn len(&self) -> usize {
        self.stimuli.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stimuli.is_empty()
    }

    pub fn stimuli(&self) -> &[(String, Stimulus, Window)] {
        &self.stimuli
    }

    /// Whether this layout describes `r`: the same number of stimuli, the
    /// same label at every position, and every window inside its stimulus's
    /// frames. Every index that reads responses runs this first.
    pub fn check(&self, r: &Responses) -> Result<(), IndexError> {
        let mismatch = IndexError::CountMismatch { layout: self.stimuli.len(), responses: r.len() };
        if self.stimuli.len() != r.len() {
            return Err(mismatch);
        }
        for (s, (label, _, w)) in self.stimuli.iter().enumerate() {
            let got = r.stimulus(s).ok_or_else(|| mismatch.clone())?;
            if got.label() != label {
                return Err(IndexError::LabelMismatch {
                    stimulus: s,
                    layout: label.clone(),
                    responses: got.label().to_string(),
                });
            }
            if w.last >= got.n_frames() {
                return Err(IndexError::WindowPastEnd { stimulus: s, last: w.last, n_frames: got.n_frames() });
            }
        }
        Ok(())
    }
}

/// The FRI's frames: flyvis's query `-dt <= time <= t_stim` on its time
/// axis `frame * dt - t_pre`, evaluated as flyvis evaluates it. At dt 1/200
/// float rounding drops frame 199, so the window is frames 200..=400.
pub fn fri_window(dt: f64) -> Result<Window, IndexError> {
    mask_window(dt, T_PRE, FLASH_T_STIM, |t| -dt <= t && t <= FLASH_T_STIM)
}

/// A moving edge's frames: flyvis's `time_window` mask for `speed` (columns
/// per second) and `offsets`, its float expressions in its order, unsimplified.
pub fn edge_window(speed: f64, dt: f64, t_pre: f64, offsets: (i32, i32)) -> Result<Window, IndexError> {
    if !(speed.is_finite() && speed > 0.0) {
        return Err(IndexError::BadParameter { what: "edge speed", value: speed });
    }
    if !(t_pre.is_finite() && t_pre >= 0.0) {
        return Err(IndexError::BadParameter { what: "t_pre", value: t_pre });
    }
    let (start, end) = offsets;
    let from_degree = start as f64 * 2.25;
    let to_degree = (end as i64 - 1) as f64 * 2.25;
    let from_column = from_degree / 5.8;
    let mut to_column = to_degree / 5.8;
    let start_in_columns = start as f64 * 2.25 / 5.8;
    to_column += 2.25 / 5.8;
    let t_start = (start_in_columns.abs() - from_column.abs()) / speed;
    let t_end = t_start + (to_column - from_column) / speed;
    mask_window(dt, t_pre, t_end, |t| t >= t_start && t <= t_end)
}

/// The first and last frame whose flyvis time `f * dt - t_pre` satisfies
/// `keep`, scanning up to the frame just past `t_last`. Both masks are
/// intervals of a monotone time axis, so the frames between are kept too.
fn mask_window(dt: f64, t_pre: f64, t_last: f64, keep: impl Fn(f64) -> bool) -> Result<Window, IndexError> {
    if !(dt.is_finite() && dt > 0.0) {
        return Err(IndexError::BadDt(dt));
    }
    let frames = ((t_last + t_pre) / dt).ceil() + 2.0;
    if !(frames.is_finite() && (0.0..=MAX_WINDOW_FRAMES).contains(&frames)) {
        return Err(IndexError::BadDt(dt));
    }
    let (mut first, mut last) = (None, None);
    for f in 0..=frames as usize {
        if keep(f as f64 * dt - t_pre) {
            first.get_or_insert(f);
            last = Some(f);
        }
    }
    match (first, last) {
        (Some(a), Some(b)) => Window::new(a, b),
        _ => Err(IndexError::Empty("time window")),
    }
}
```

In `crates/fidelity/src/lib.rs`, add one bullet to the "What is DONE here" list of the module doc, after the `size_fidelity_curve` bullet:

```rust
//! - [`indices`]: flyvis's response indices (FRI, DSI, preferred direction,
//!   tuning correlation), their time windows and the paper's agreement rules,
//!   as pure functions over `semblance_sim::response::Responses`
```

and declare the module right after the `use std::time::Instant;` line:

```rust
pub mod indices;
```

- [ ] **Step 4: Run the tests to see them pass.** Run: `cargo test -p semblance-fidelity indices::`. Expected: 7 passed. Then `cargo clippy -p semblance-fidelity --all-targets -- -D warnings`: clean.

- [ ] **Step 5: Re-measure the test total (count owner).** Run the whole suite as Global Constraints' "Measuring the total" says and write the measured number N into CLAUDE.md in both places: the line `cargo test --workspace          # 587 tests, 0 failures` becomes `# N tests, 0 failures`, and `"587 passed, 0 failed"` becomes `"N passed, 0 failed"`. Change nothing else in CLAUDE.md.

- [ ] **Step 6: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed`.

- [ ] **Step 7: Commit.**

```bash
git add crates/fidelity/src/indices.rs crates/fidelity/src/lib.rs CLAUDE.md
git commit -m "Describe index batteries and flyvis's FRI and moving-edge windows"
```

---

### Task ctl-network: settle any topology, and respond_network for the controls

Owns: crates/flyvis/src/parity/run.rs, crates/flyvis/src/parity/respond.rs, crates/flyvis/src/parity/mod.rs
Uses: none
Provides: respond-network
Wave: 0

**What this builds.** Gate C's rewired control changes the topology, so it needs a control-only entry point that plays a battery through a *different* `TypedNetwork` (owner's answer Q5, decision 6: `respond` and the later `score` keep taking a 734-vector over the fixed topology). Today `settle_core` (in `parity/run.rs`) reads the network and its per-edge synapse-count groups from the fixtures (`f.net`, `f.groups`). This task makes `settle_core` take them explicitly, routes `respond` through a shared private `respond_on(f, &f.net, &f.groups, …)`, and adds `respond_network(f, net, groups, …)`. `respond`'s results must not change by a bit: its existing exact tests (`respond_reproduces_flyviss_centre_traces_at_1_200` and `_at_1_50`, which compare model 000 with flyvis's `centre.f32/f64.bin` for every value) and the 0.3.0 settling and trace gates re-prove it.

**Facts this task needs.**
- `settle_core<R: Precision>(f, p, dt, n_settle, counts, centre, ctx)` (run.rs) builds per-edge synapse counts from `f.groups[e]` (an index into the 2,355-entry tables `SYN_COUNT_TORCH_F32/F64` for `Counts::Torch`, `SYN_COUNT[g].exp()` for `Counts::Libm`) or `f.net.edge_n_syn()[e]` (`Counts::IntegerNSyn`), builds `Graded::<R>::new(&f.net, &params, dt)`, fills grey input on every node of `f.input_nodes` with `f.manifest.pre_stim_value`, and steps `n_settle` times, refusing a non-finite state via `ctx.settle_non_finite(i, cell_type(&f.net, i).to_string(), step)`. Its only callers are `settle_state` (run.rs) and `respond_as` (respond.rs).
- `step_frame(f, g, frame, input)` uses only `f.input_nodes`; it needs no change, since rewiring never changes nodes or input nodes.
- "The torch synapse counts follow each edge": a rewired edge keeps its original edge's group, so exactness is unaffected. A group index must therefore be checked (< 2,355) before `settle_core` indexes the tables with it, or a bad group would panic.
- `central_nodes(f)` derives central nodes from `f.net`'s nodes; nodes are unchanged by rewiring, so it applies as is.
- `TypedNetworkParts` and `TypedNetwork` are `semblance_connectome::{TypedNetwork, TypedNetworkParts}`; `TypedNetwork` has accessors `cell_types`, `node_cell_type`, `node_u`, `node_v`, `conn_types`, `conn_sign`, `edge_source`, `edge_target`, `edge_conn_type`, `edge_n_syn`, `num_nodes`, `num_edges`.

**Files.**
- Modify: `crates/flyvis/src/parity/run.rs` (`settle_core` and `settle_state`)
- Modify: `crates/flyvis/src/parity/respond.rs` (`respond`, new `respond_network`, `respond_on`, `respond_as`; three tests)
- Modify: `crates/flyvis/src/parity/mod.rs` (re-export `respond_network`)

- [ ] **Step 1: Write the failing tests.** Append inside `respond.rs`'s `#[cfg(test)] mod tests` (it already has `Fixed`, `grey`, `fixture`, `Dtype`, `Counts`), and add these imports at the top of that tests module:

```rust
    use semblance_connectome::TypedNetworkParts;
    use crate::syn_count_torch::SYN_COUNT_TORCH_F32;
```

```rust
    /// A network's parts, for building a variant of it.
    fn parts(net: &TypedNetwork) -> TypedNetworkParts {
        TypedNetworkParts {
            cell_types: net.cell_types().to_vec(),
            node_cell_type: net.node_cell_type().to_vec(),
            node_u: net.node_u().to_vec(),
            node_v: net.node_v().to_vec(),
            conn_types: net.conn_types().to_vec(),
            conn_sign: net.conn_sign().to_vec(),
            edge_source: net.edge_source().to_vec(),
            edge_target: net.edge_target().to_vec(),
            edge_conn_type: net.edge_conn_type().to_vec(),
            edge_n_syn: net.edge_n_syn().to_vec(),
        }
    }

    /// `respond_network` on the fixtures' own network and groups is
    /// `respond`, bit for bit, at both precisions.
    #[test]
    fn respond_network_on_the_fixtures_own_network_is_respond() {
        let Some(f) = fixture() else { return };
        let p = f.ensemble.model("000").unwrap();
        let flash: Vec<f64> = (0..2 * HEXALS).map(|i| (i % 2) as f64).collect();
        for precision in [Dtype::F32, Dtype::F64] {
            let battery = || Fixed { stimuli: vec![grey(3), flash.clone()], claimed: 2, next: 0 };
            let a = respond(f, p, 1.0 / 200.0, precision, Counts::Torch, &mut battery()).unwrap_or_else(|e| panic!("{e}"));
            let b = respond_network(f, &f.net, &f.groups, p, 1.0 / 200.0, precision, Counts::Torch, &mut battery())
                .unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(a.len(), b.len());
            for s in 0..a.len() {
                let (x, y) = (a.stimulus(s).unwrap(), b.stimulus(s).unwrap());
                assert_eq!((x.label(), x.n_frames()), (y.label(), y.n_frames()));
                for i in 0..x.n_frames() {
                    let bits = |row: &[f64]| row.iter().map(|v| v.to_bits()).collect::<Vec<u64>>();
                    assert_eq!(bits(x.frame(i).unwrap()), bits(y.frame(i).unwrap()), "{precision:?}, stimulus {s}, frame {i}");
                }
            }
        }
    }

    #[test]
    fn respond_network_refuses_a_network_whose_nodes_differ() {
        let Some(f) = fixture() else { return };
        let mut q = parts(&f.net);
        q.node_u[0] += 1;
        let moved = TypedNetwork::new(q).unwrap();
        let mut b = Fixed { stimuli: vec![grey(1)], claimed: 1, next: 0 };
        let p = ModelParams::pretrained();
        let e = respond_network(f, &moved, &f.groups, &p, 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.contains("nodes differ")), "{e}");
    }

    #[test]
    fn respond_network_refuses_synapse_count_groups_that_do_not_fit_without_panicking() {
        let Some(f) = fixture() else { return };
        let p = ModelParams::pretrained();
        let mut short = f.groups.clone();
        short.pop();
        let mut past = f.groups.clone();
        past[0] = SYN_COUNT_TORCH_F32.len() as u32;
        for (groups, expect) in [(short, "groups for"), (past, "names synapse-count group")] {
            let mut b = Fixed { stimuli: vec![grey(1)], claimed: 1, next: 0 };
            let e = respond_network(f, &f.net, &groups, &p, 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
            assert!(matches!(e, ParityError::Usage(ref m) if m.contains(expect)), "{e}");
        }
    }
```

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p semblance-flyvis parity::respond::tests::respond_network`. Expected: compile error (`cannot find function respond_network`).

- [ ] **Step 3: Make `settle_core` take the network and groups.** In `run.rs`, replace `settle_core`'s signature and the four places it reads `f.net`/`f.groups`; keep every other line as it is:

```rust
/// `settle_state` for any parameters on any network over the fixtures'
/// nodes: builds the integrator for `p` on `net` at `dt`, with `groups[e]`
/// naming edge `e`'s synapse-count group, and settles it for `n_settle`
/// steps of `pre_stim_value` on every input node, recording the centre
/// values after every step. `groups` must have one entry per edge of `net`,
/// each below 2355; `respond_network` checks that before calling.
#[allow(clippy::too_many_arguments)]
pub(super) fn settle_core<R: Precision>(
    f: &Fixtures,
    net: &TypedNetwork,
    groups: &[u32],
    p: &ModelParams,
    dt: f64,
    n_settle: usize,
    counts: Counts,
    centre: &[usize],
    ctx: Ctx,
) -> Result<(Graded<R>, Vec<f64>), ParityError> {
    let count: Vec<R> = groups
        .iter()
        .enumerate()
        .map(|(e, &g)| match counts {
            Counts::Torch => R::torch_count(g as usize),
            Counts::Libm => R::libm_count(g as usize),
            Counts::IntegerNSyn => R::from_f32(net.edge_n_syn()[e]),
        })
        .collect();
    let params = GradedParams { bias: &p.bias, time_const: &p.time_const, syn_strength: &p.syn_strength, edge_syn_count: &count };
    let mut g = Graded::<R>::new(net, &params, dt).map_err(|e| ctx.graded(e))?;

    let mut input = vec![R::ZERO; net.num_nodes()];
    for &i in f.input_nodes.iter().flatten() {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    let mut rust_centre = Vec::new();
    for step in 0..n_settle {
        g.step(&input).map_err(|e| ctx.graded(e))?;
        if let Some(i) = g.state().iter().position(|v| !v.is_finite()) {
            return Err(ctx.settle_non_finite(i, cell_type(net, i).to_string(), step));
        }
        // settle step `step` is the state after step + 1 Euler steps
        rust_centre.extend(centre.iter().map(|&i| g.state()[i].to_f64()));
    }
    Ok((g, rust_centre))
}
```

and in `settle_state`:

```rust
    settle_core::<R>(f, &f.net, &f.groups, p, dt.dt, dt.n_settle, counts, centre, Ctx::Run(run))
```

- [ ] **Step 4: Route `respond` through `respond_on` and add `respond_network`.** In `respond.rs`, add `use semblance_connectome::TypedNetwork;` and `use crate::syn_count_torch::SYN_COUNT_TORCH_F32;` to the imports, then replace `respond` and `respond_as` with:

```rust
/// Settles `params` on the fixtures' network at `dt` and `precision`, as the
/// 0.3.0 gates settle, then plays every stimulus of `battery` from the
/// settled state and records each cell type's central value at every frame.
/// The battery's frames must have been rendered at this `dt`; nothing checks
/// it.
pub fn respond(
    f: &Fixtures,
    params: &ModelParams,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    respond_on(f, &f.net, &f.groups, params, dt, precision, counts, battery)
}

/// `respond` on another topology over the fixtures' nodes: gate C's rewired
/// control, and nothing else (owner's answer Q5; decision 6 keeps `respond`
/// on the fixed topology). `groups[e]` is edge `e`'s synapse-count group,
/// carried from the edge it was rebuilt from, so torch's counts follow each
/// edge and exactness is unaffected. Refuses, as `Usage`, a network whose
/// cell types or nodes differ from the fixtures' network, groups of the
/// wrong length, and a group past the 2355 tabled.
#[allow(clippy::too_many_arguments)]
pub fn respond_network(
    f: &Fixtures,
    net: &TypedNetwork,
    groups: &[u32],
    params: &ModelParams,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    let same_nodes = net.cell_types() == f.net.cell_types()
        && net.node_cell_type() == f.net.node_cell_type()
        && net.node_u() == f.net.node_u()
        && net.node_v() == f.net.node_v();
    if !same_nodes {
        return Err(ParityError::Usage("respond_network: the network's nodes differ from the fixtures' network".into()));
    }
    if groups.len() != net.num_edges() {
        return Err(ParityError::Usage(format!(
            "respond_network: {} synapse-count groups for {} edges",
            groups.len(),
            net.num_edges()
        )));
    }
    if let Some((e, &g)) = groups.iter().enumerate().find(|&(_, &g)| g as usize >= SYN_COUNT_TORCH_F32.len()) {
        return Err(ParityError::Usage(format!(
            "respond_network: edge {e} names synapse-count group {g}, but only {} exist",
            SYN_COUNT_TORCH_F32.len()
        )));
    }
    respond_on(f, net, groups, params, dt, precision, counts, battery)
}

#[allow(clippy::too_many_arguments)]
fn respond_on(
    f: &Fixtures,
    net: &TypedNetwork,
    groups: &[u32],
    params: &ModelParams,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    match precision {
        Dtype::F32 => respond_as::<f32>(f, net, groups, params, dt, counts, battery),
        Dtype::F64 => respond_as::<f64>(f, net, groups, params, dt, counts, battery),
    }
}
```

In `respond_as`, add the parameters `net: &TypedNetwork, groups: &[u32]` after `f`, and change exactly these lines (everything else stays): the settle call becomes `settle_core::<R>(f, net, groups, params, dt, steps.round() as usize, counts, &[], Ctx::Respond)?`; `cell_type(&f.net, k)` becomes `cell_type(net, k)` (both places); `vec![R::ZERO; f.net.num_nodes()]` becomes `vec![R::ZERO; net.num_nodes()]`. `central_nodes(f)` stays.

In `parity/mod.rs`, extend the re-export:

```rust
pub use respond::{central_nodes, respond, respond_model, respond_network, Battery, ReferenceBattery};
```

- [ ] **Step 5: Run the tests to see them pass.** Run: `env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis parity::respond::`. Expected: every test in the module passes, including the three new ones and `respond_reproduces_flyviss_centre_traces_at_1_200` / `_at_1_50` (P10: 1a's exact reproduction still holds).

- [ ] **Step 6: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed` (the 0.3.0 settling and trace gates re-prove `settle_state`'s results are unchanged).

- [ ] **Step 7: Commit.**

```bash
git add crates/flyvis/src/parity/run.rs crates/flyvis/src/parity/respond.rs crates/flyvis/src/parity/mod.rs
git commit -m "Let settle_core take the network and add respond_network for the controls"
```

---

### Task idx-fri: the flash response index, in flyvis's dtypes

Owns: crates/fidelity/src/indices.rs
Uses: indices-layout
Provides: indices-fri
Wave: 1

**What this builds.** In `crates/fidelity/src/indices.rs` (module `semblance_fidelity::indices`, which already holds `Stimulus`, `Window`, `BatteryLayout` with `check`, `IndexError`, `FRI_RADIUS = 6` and the tests helpers `responses(types, precision, stimuli, fill)`, `window(first, last)`, `entry(label, stimulus, window)`): `FriPeaks`, `fri_peaks` and `fri`, emulating flyvis's dtypes operation by operation. This task adds at most 15 `#[test]`s (it adds 8) and does not touch CLAUDE.md.

**Facts this task needs (flyvis `analysis/flash_responses.py`, task 0).**
- The FRI reads the **radius-6 flashes only** (radius -1 never feeds it); intensity 0 is OFF, intensity 1 is ON; one shared window (frames 200..=400 at dt 1/200).
- For an **F32** run, per cell type (central column), operation by operation:
  1. `stim_response` = the window's frames of both radius-6 flashes;
  2. `minimum = stim_response.min(dim=['frame','sample'])`: one f32 min over the window **and both intensities**;
  3. `stim_response += np.abs(minimum)`: an f32 add per value (this adds |min|; it does not subtract min);
  4. `on_peak`, `off_peak` = the f32 max over frames of the shifted ON and OFF traces;
  5. `fri = on_peak - off_peak`: f32 subtraction;
  6. `den = on_peak + off_peak` (f32 add), then `+ np.array([1e-16])`, which promotes to f64: `(den_f32 as f64) + 1e-16`;
  7. `fri /= den`: numpy's in-place divide loops in f64, `(fri_f32 as f64) / den_f64`, and stores f32.
  So the stored FRI is `((num as f64) / ((on + off) as f32 as f64 + 1e-16)) as f32`. For an **F64** run every step is f64: `(on - off) / (on + off + 1e-16)`.
- `Responses` stores an f32 run's values widened to f64 (exact); the stored FRI of an F32 run is f32-valued.

**Files.** Modify: `crates/fidelity/src/indices.rs`.

- [ ] **Step 1: Write the failing tests.** Append inside `mod tests`:

```rust
    /// The FRI battery used below: radius -1 and radius 6 flashes, both
    /// intensities, 5 frames each, the window frames 1..=3.
    fn fri_layout() -> BatteryLayout {
        let flash = |radius, intensity| Stimulus::Flash { radius, intensity };
        BatteryLayout::new(vec![
            entry("full off", flash(-1, 0), window(1, 3)),
            entry("off", flash(6, 0), window(1, 3)),
            entry("on", flash(6, 1), window(1, 3)),
            entry("full on", flash(-1, 1), window(1, 3)),
        ])
        .unwrap()
    }

    const FRI_STIMULI: [(&str, usize); 4] = [("full off", 5), ("off", 5), ("on", 5), ("full on", 5)];

    #[test]
    fn fri_peaks_shift_by_the_minimum_over_both_intensities() {
        // A: OFF window -2, 1, 0 and ON window -1, 3, 2, so min -2, shift +2.
        // B: OFF window 1, 2, 3 and ON window 2, 4, 1, so min 1, shift +1:
        // flyvis adds |min|, it does not subtract min.
        let a = [[9.0, -2.0, 1.0, 0.0, 9.0], [9.0, -1.0, 3.0, 2.0, 9.0]];
        let b = [[0.0, 1.0, 2.0, 3.0, 0.0], [0.0, 2.0, 4.0, 1.0, 0.0]];
        let r = responses(&["A", "B"], Precision::F64, &FRI_STIMULI, |s, f, t| match s {
            1 | 2 => [a, b][t][s - 1][f],
            _ => -100.0, // the radius -1 flashes never feed the FRI
        });
        let p = fri_peaks(&r, &fri_layout()).unwrap();
        assert_eq!(p.peaks(), &[[3.0, 5.0], [4.0, 5.0]]);
        assert_eq!((p.precision(), p.cell_types()), (Precision::F64, &["A".to_string(), "B".to_string()][..]));
        let fri = fri(&p).unwrap();
        assert_eq!(fri, vec![(5.0 - 3.0) / (5.0 + 3.0 + 1e-16), (5.0 - 4.0) / (5.0 + 4.0 + 1e-16)]);
    }

    #[test]
    fn an_f32_runs_fri_follows_numpys_dtypes_step_by_step() {
        // OFF peak v = 1e-8 (an f32), ON peak 1. In f32, 1 - v and 1 + v are
        // both 1.0, and 1.0 / (1.0 + 1e-16) is 1.0; in f64 they are not.
        let v = 1e-8f32 as f64;
        let fill = |s: usize, f: usize, _t: usize| match (s, f) {
            (1, 2) => v,
            (2, 2) => 1.0,
            _ => 0.0,
        };
        let f32_run = responses(&["A"], Precision::F32, &FRI_STIMULI, fill);
        let f64_run = responses(&["A"], Precision::F64, &FRI_STIMULI, fill);
        let x = fri(&fri_peaks(&f32_run, &fri_layout()).unwrap()).unwrap();
        let y = fri(&fri_peaks(&f64_run, &fri_layout()).unwrap()).unwrap();
        assert_eq!(x, vec![1.0]);
        assert!(y[0] < 1.0, "{}", y[0]);
        assert_eq!(y, vec![(1.0 - v) / (1.0 + v + 1e-16)]);
        // every stored f32-run FRI is an f32
        assert_eq!(x[0] as f32 as f64, x[0]);
    }

    #[test]
    fn an_f32_runs_epsilon_is_added_in_f64_and_the_shift_in_f32() {
        // Peaks [off 0, on 2^-54]: only numpy's promotion gives this value.
        // An f32 epsilon gives 0.3569593131542206, no epsilon 1.0, and all
        // f64 without the final f32 rounding 0.35695929707772656 (the f64
        // run's value).
        let tiny = 2f64.powi(-54);
        let p = FriPeaks::new(vec!["A".into()], Precision::F32, vec![[0.0, tiny]]).unwrap();
        assert_eq!(fri(&p).unwrap(), vec![0.3569592833518982]);
        let p = FriPeaks::new(vec!["A".into()], Precision::F64, vec![[0.0, tiny]]).unwrap();
        assert_eq!(fri(&p).unwrap(), vec![0.35695929707772656]);
        // A nonzero shift: OFF window -1, 0.25, 0 and ON window 0, 0.5, 0, so
        // min -1, shift +1 in f32: peaks [1.25, 1.5].
        let fill = |s: usize, f: usize, _t: usize| match (s, f) {
            (1, 1) => -1.0,
            (1, 2) => 0.25,
            (2, 2) => 0.5,
            _ => 0.0,
        };
        let r = responses(&["A"], Precision::F32, &FRI_STIMULI, fill);
        let p = fri_peaks(&r, &fri_layout()).unwrap();
        assert_eq!(p.peaks(), &[[1.25, 1.5]]);
        assert_eq!(fri(&p).unwrap(), vec![((0.25f32 as f64) / (2.75f32 as f64 + 1e-16)) as f32 as f64]);
    }

    #[test]
    fn fri_peaks_refuse_an_f32_run_holding_non_f32_values() {
        // Responses does not check an f32 run's values; fri_peaks must not
        // round them silently.
        let r = responses(&["A"], Precision::F32, &FRI_STIMULI, |_, _, _| 0.1);
        assert!(matches!(fri_peaks(&r, &fri_layout()), Err(IndexError::NotF32(_))));
    }

    #[test]
    fn the_fri_needs_exactly_one_radius_6_flash_per_intensity_in_one_window() {
        let r = responses(&["A"], Precision::F64, &FRI_STIMULI, |_, _, _| 0.5);
        let flash = |radius, intensity| Stimulus::Flash { radius, intensity };
        let with = |second: Stimulus, w: Window| {
            BatteryLayout::new(vec![
                entry("full off", flash(-1, 0), window(1, 3)),
                entry("off", flash(6, 0), window(1, 3)),
                entry("on", second, w),
                entry("full on", flash(-1, 1), window(1, 3)),
            ])
            .unwrap()
        };
        for (second, w) in [
            (flash(-1, 1), window(1, 3)), // no ON radius-6 flash
            (flash(6, 0), window(1, 3)),  // two OFF ones
            (flash(6, 2), window(1, 3)),  // an intensity that is neither
            (flash(6, 1), window(1, 2)),  // two windows
        ] {
            let e = fri_peaks(&r, &with(second, w));
            assert!(matches!(e, Err(IndexError::BatteryShape(_))), "{second:?} {w:?}: {e:?}");
        }
    }

    #[test]
    fn fri_peaks_refuse_responses_the_layout_does_not_describe() {
        // chore defect 2's analogue on the new path
        let r = responses(&["A"], Precision::F64, &FRI_STIMULI[..3], |_, _, _| 0.5);
        assert_eq!(fri_peaks(&r, &fri_layout()), Err(IndexError::CountMismatch { layout: 4, responses: 3 }));
    }

    #[test]
    fn fri_peaks_built_by_hand_are_checked() {
        let names = || vec!["A".to_string(), "B".to_string()];
        assert_eq!(FriPeaks::new(vec![], Precision::F64, vec![]), Err(IndexError::Empty("FRI peaks")));
        assert_eq!(
            FriPeaks::new(names(), Precision::F64, vec![[0.0, 1.0]]),
            Err(IndexError::Length { what: "FRI peaks", expected: 2, found: 1 })
        );
        assert!(matches!(
            FriPeaks::new(names(), Precision::F64, vec![[0.0, 1.0], [f64::NAN, 1.0]]),
            Err(IndexError::NonFinite(_))
        ));
        assert!(matches!(
            FriPeaks::new(names(), Precision::F32, vec![[0.0, 1.0], [0.1, 1.0]]),
            Err(IndexError::NotF32(_))
        ));
        assert!(FriPeaks::new(names(), Precision::F32, vec![[0.0, 1.0], [0.5, 1.0]]).is_ok());
    }

    #[test]
    fn an_fri_that_is_not_finite_is_refused() {
        // off = -1e-16, on = 0: the numerator is 1e-16
        let p = FriPeaks::new(vec!["A".into()], Precision::F64, vec![[-1e-16, 0.0]]).unwrap();
        // on + off + 1e-16 is 0, so the quotient is not finite
        assert!(matches!(fri(&p), Err(IndexError::NonFinite(_))), "{:?}", fri(&p));
    }
```

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p semblance-fidelity indices::tests::fri`. Expected: compile errors (`cannot find function fri_peaks`, `FriPeaks`).

- [ ] **Step 3: Write the implementation.** Change the top import to `use semblance_sim::response::{Precision, Responses, StimulusResponse};`, and add above `#[cfg(test)]`:

```rust
/// Per cell type, in `Responses` order, the FRI's two shifted window peaks
/// `[off, on]`, with the precision `fri` emulates.
#[derive(Clone, Debug, PartialEq)]
pub struct FriPeaks {
    cell_types: Vec<String>,
    precision: Precision,
    peaks: Vec<[f64; 2]>,
}

impl FriPeaks {
    /// Refuses no peaks, a count that is not one pair per cell type, a
    /// non-finite peak, and (for an f32 run) a peak that is not an f32.
    pub fn new(cell_types: Vec<String>, precision: Precision, peaks: Vec<[f64; 2]>) -> Result<Self, IndexError> {
        if peaks.is_empty() {
            return Err(IndexError::Empty("FRI peaks"));
        }
        if peaks.len() != cell_types.len() {
            return Err(IndexError::Length { what: "FRI peaks", expected: cell_types.len(), found: peaks.len() });
        }
        for (t, pair) in peaks.iter().enumerate() {
            for &v in pair {
                if !v.is_finite() {
                    return Err(IndexError::NonFinite(format!("the FRI peak of {}", cell_types[t])));
                }
                if precision == Precision::F32 && v as f32 as f64 != v {
                    return Err(IndexError::NotF32(format!("the FRI peak {v} of {}", cell_types[t])));
                }
            }
        }
        Ok(FriPeaks { cell_types, precision, peaks })
    }

    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// `[off, on]` per cell type.
    pub fn peaks(&self) -> &[[f64; 2]] {
        &self.peaks
    }
}

/// flyvis's FRI peaks (`flash_responses.py`): over the radius-6 flashes'
/// shared window, each cell type's minimum over the window and both
/// intensities, `+= abs(minimum)`, then each intensity's maximum. An f32
/// run does it in f32, an f64 run in f64. An f32 run's window value that is
/// not an f32 is refused (`NotF32`).
pub fn fri_peaks(r: &Responses, b: &BatteryLayout) -> Result<FriPeaks, IndexError> {
    b.check(r)?;
    let (off, on, w) = fri_pair(b)?;
    let missing = || IndexError::CountMismatch { layout: b.len(), responses: r.len() };
    let (s_off, s_on) = (r.stimulus(off).ok_or_else(missing)?, r.stimulus(on).ok_or_else(missing)?);
    let mut peaks = Vec::with_capacity(r.cell_types().len());
    let f32_run = r.precision() == Precision::F32;
    for t in 0..r.cell_types().len() {
        let (x, y) = (window_trace(s_off, t, w), window_trace(s_on, t, w));
        // Responses does not check an f32 run's values: refuse one that is
        // not an f32 rather than round it silently.
        if let Some(&v) = x.iter().chain(&y).find(|&&v| f32_run && v as f32 as f64 != v) {
            return Err(IndexError::NotF32(format!("{}'s value {v} in the FRI window", r.cell_types()[t])));
        }
        peaks.push(match r.precision() {
            Precision::F32 => shifted_peaks_f32(&x, &y),
            Precision::F64 => shifted_peaks_f64(&x, &y),
        });
    }
    FriPeaks::new(r.cell_types().to_vec(), r.precision(), peaks)
}

/// The flash response index per cell type, `(on - off) / (on + off + 1e-16)`,
/// in numpy's dtypes: for an f32 run an f32 numerator and sum, the f64
/// epsilon add and divide, stored f32; for an f64 run all f64.
pub fn fri(p: &FriPeaks) -> Result<Vec<f64>, IndexError> {
    let mut out = Vec::with_capacity(p.peaks.len());
    for (t, &[off, on]) in p.peaks.iter().enumerate() {
        let v = match p.precision {
            Precision::F32 => {
                let (on, off) = (on as f32, off as f32);
                let num = on - off;
                let den = (on + off) as f64 + 1e-16;
                ((num as f64) / den) as f32 as f64
            }
            Precision::F64 => (on - off) / (on + off + 1e-16),
        };
        if !v.is_finite() {
            return Err(IndexError::NonFinite(format!("the FRI of {}", p.cell_types[t])));
        }
        out.push(v);
    }
    Ok(out)
}

/// The OFF and ON radius-6 flashes' positions and their shared window.
fn fri_pair(b: &BatteryLayout) -> Result<(usize, usize, Window), IndexError> {
    let mut found: [Option<(usize, Window)>; 2] = [None, None];
    for (s, (_, stimulus, w)) in b.stimuli().iter().enumerate() {
        if let Stimulus::Flash { radius: FRI_RADIUS, intensity } = *stimulus {
            let slot = found.get_mut(intensity as usize).ok_or_else(|| {
                IndexError::BatteryShape(format!("stimulus {s}: a radius-{FRI_RADIUS} flash of intensity {intensity}, not 0 or 1"))
            })?;
            if slot.is_some() {
                return Err(IndexError::BatteryShape(format!(
                    "stimulus {s}: a second radius-{FRI_RADIUS} flash of intensity {intensity}"
                )));
            }
            *slot = Some((s, *w));
        }
    }
    match found {
        [Some((off, a)), Some((on, b))] if a == b => Ok((off, on, a)),
        [Some(_), Some(_)] => Err(IndexError::BatteryShape(format!("the radius-{FRI_RADIUS} flashes have different windows"))),
        _ => Err(IndexError::BatteryShape(format!(
            "the FRI needs one radius-{FRI_RADIUS} flash of intensity 0 and one of intensity 1"
        ))),
    }
}

/// Cell type `t`'s values over window `w` (checked against the stimulus by
/// `BatteryLayout::check`).
fn window_trace(s: &StimulusResponse, t: usize, w: Window) -> Vec<f64> {
    s.trace(t).map(|tr| tr.skip(w.first()).take(w.n_frames()).collect()).unwrap_or_default()
}

fn shifted_peaks_f32(off: &[f64], on: &[f64]) -> [f64; 2] {
    let shift = off.iter().chain(on).map(|&v| v as f32).fold(f32::INFINITY, f32::min).abs();
    let peak = |vals: &[f64]| vals.iter().map(|&v| v as f32 + shift).fold(f32::NEG_INFINITY, f32::max) as f64;
    [peak(off), peak(on)]
}

fn shifted_peaks_f64(off: &[f64], on: &[f64]) -> [f64; 2] {
    let shift = off.iter().chain(on).copied().fold(f64::INFINITY, f64::min).abs();
    let peak = |vals: &[f64]| vals.iter().map(|&v| v + shift).fold(f64::NEG_INFINITY, f64::max);
    [peak(off), peak(on)]
}
```

- [ ] **Step 4: Run them to see them pass.** Run: `cargo test -p semblance-fidelity indices::`. Expected: all pass (15). `cargo clippy -p semblance-fidelity --all-targets -- -D warnings`: clean.

- [ ] **Step 5: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed`.

- [ ] **Step 6: Commit.**

```bash
git add crates/fidelity/src/indices.rs
git commit -m "Compute the flash response index in flyvis's dtypes"
```

---

### Task ctl-params: the controls module: seeds, zeroed, shuffled, random parameters, redraws

Owns: crates/flyvis/src/parity/controls.rs, crates/flyvis/src/parity/mod.rs, crates/flyvis/Cargo.toml, Cargo.lock, app/tests/acceptance_release.rs, CLAUDE.md
Uses: none
Provides: controls-draws
Wave: 1

**What this builds.** A new public module `semblance_flyvis::parity::controls` (one file, `crates/flyvis/src/parity/controls.rs`) holding gate C's control identities, the seed formula, three of the four constructions (zeroed, shuffled, random parameters) and the redraw runner. `semblance-flyvis` gains its first dependency on `semblance-rng`. Running the draws through a battery and scoring them is gate C, in the later epic; this task builds the parts it calls. This task is wave 1's **count owner** (Step 7). `app/tests/acceptance_release.rs` is owned only because its RELEASE-01/02 tests assert the dependency graph and `Cargo.lock`; they must pass unchanged (`semblance-rng` publishes before `semblance-flyvis`, and the lock still lists exactly the 8 `semblance*` packages).

**Facts this task needs (spec RA §2, task 0, the owner's answers).**
- Each control is applied to each of the 50 trained models: **50 draws per control**, each seeded through `semblance_rng::Rng::seed_from_u64` with a seed derived from (control index, model index) by a fixed formula written in the code and (later) the record. The formula, fixed here: `control_seed(c, m, a) = 0x5EB1_A9CE_0000_0000 ^ (c << 24) ^ (m << 8) ^ a`, with `c` = 0 zeroed, 1 shuffled, 2 random parameters, 3 rewired, `m` the model's ensemble position (0..49), `a` the attempt (0 for the first draw, +1 per redraw). Injective for m < 65,536 and a < 256.
- **zeroed**: the model's biases and time constants, all **604** strengths 0. Deterministic; consumes no draws.
- **shuffled**: the model's 604 strengths permuted across connection types by Durstenfeld's Fisher–Yates, `for i in (1..604).rev() { j = rng.below(i + 1); swap(i, j) }`. Biases and time constants unchanged. Signs belong to the connection type (an edge's weight is `conn_sign[ct] * n_syn * syn_strength[ct]`, and `Graded` clamps negative strengths to 0), so every sign is kept: Zhou & Hasler's sign-preserving shuffle. Model 000's strengths are all ≥ 0.
- **random parameters**: each of the **734** parameters drawn uniformly between that parameter's minimum and maximum over the 50 models with `Rng::next_f32` (uniform in [0, 1), top 24 bits / 2^24), in the 734-row order: `bias[0..65]`, `time_const[0..65]`, `syn_strength[0..604]`. The value is `(lo + (hi - lo) * u)` in f64 from the f32 bounds, rounded to f32 and clamped to `[lo, hi]`; a constant parameter still consumes its draw, so the stream position never depends on the data.
- **Unstable draws**: if a draw's state goes non-finite (`ParityError::is_gate_failure()`: `NonFinite`, `NonFiniteSettle`, `NonFiniteResponse`), it is redrawn with the next seed in its sequence (the next attempt), as Zhou & Hasler rejection-sample, **up to 10 times** (`MAX_REDRAWS = 10`: attempts 0..=10, the original draw plus 10 redraws); the original and all 10 redraws non-finite is gate C's failure. Zeroed is deterministic, so it gets one attempt. Any other error ends the draw. Every redraw is recorded in the result (the gates epic prints them).
- `ModelParams { pub bias: [f32; 65], pub time_const: [f32; 65], pub syn_strength: [f32; 604] }` (`crate::ensemble`), `ModelParams::pretrained()` is model 000. `Ensemble::{ids() -> impl Iterator<Item = &str>, model(id) -> Option<&ModelParams>}`; inside the crate a `Fixtures`' ensemble is the `pub(crate)` field `f.ensemble`, its network `f.net`.
- `semblance_rng::Rng`: `seed_from_u64(u64)`, `next_u64()`, `next_f32()`, `below(n)` (panics on 0). The stream is frozen; use nothing else.
- The pinned shuffle (computed from `semblance-rng`'s algorithm): for strengths `0.0, 1.0, …, 603.0` and `Rng::seed_from_u64(control_seed(Shuffled, 0, 0))` (= `0x5EB1_A9CE_0100_0000`), the result begins `272, 155, 332, 262, 416, 459` and ends `9, 447, 44`.

**Files.**
- Create: `crates/flyvis/src/parity/controls.rs`
- Modify: `crates/flyvis/src/parity/mod.rs` (`pub mod controls;`)
- Modify: `crates/flyvis/Cargo.toml` (`semblance-rng.workspace = true` under `[dependencies]`; the `description` line must not change: RELEASE-07 pins it)
- Modify: `Cargo.lock` (the `semblance-flyvis` package's dependencies become `"semblance-connectome", "semblance-rng", "semblance-sim"`, in that order)
- Modify: `CLAUDE.md` (the two test-total figures only)

- [ ] **Step 1: Add the dependency.** In `crates/flyvis/Cargo.toml`'s `[dependencies]`, add `semblance-rng.workspace = true` after `semblance-connectome.workspace = true`. In `Cargo.lock`, make the `semblance-flyvis` entry:

```toml
[[package]]
name = "semblance-flyvis"
version = "0.3.1"
dependencies = [
 "semblance-connectome",
 "semblance-rng",
 "semblance-sim",
]
```

In `parity/mod.rs`, add `pub mod controls;` above `mod inputs;` (the list is alphabetical: `bounds, checks, controls, fingerprint, …`).

- [ ] **Step 2: Write the failing tests.** Create `controls.rs` with this tests block at its end (the block must start exactly `#[cfg(test)]\nmod tests`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_support::fixture;

    /// Model 000 scaled by `1 + k / 100` everywhere: a synthetic ensemble member.
    fn scaled(k: u32) -> ModelParams {
        let mut p = ModelParams::pretrained();
        for v in slots_mut(&mut p) {
            *v *= 1.0 + k as f32 / 100.0;
        }
        p
    }

    fn unstable() -> ParityError {
        ParityError::NonFiniteSettle { node: 7, cell_type: "T4a".into(), step: 3 }
    }

    #[test]
    fn the_seed_formula_is_pinned_and_injective() {
        assert_eq!(SEED_BASE, 0x5EB1_A9CE_0000_0000);
        assert_eq!(control_seed(Control::Shuffled, 0, 0), 0x5EB1_A9CE_0100_0000);
        assert_eq!(control_seed(Control::Shuffled, 7, 2), 0x5EB1_A9CE_0100_0702);
        assert_eq!(control_seed(Control::Rewired, 49, 9), 0x5EB1_A9CE_0300_3109);
        assert_eq!(control_seed(Control::Rewired, 49, MAX_REDRAWS), 0x5EB1_A9CE_0300_310A);
        let mut seen = std::collections::HashSet::new();
        for c in Control::ALL {
            for m in 0..50 {
                for a in 0..=MAX_REDRAWS {
                    assert!(seen.insert(control_seed(c, m, a)), "{c:?} {m} {a}");
                }
            }
        }
        let names: Vec<&str> = Control::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names, ["zeroed", "shuffled", "random parameters", "rewired"]);
        assert_eq!(Control::ALL.map(|c| c.index()), [0, 1, 2, 3]);
    }

    #[test]
    fn zeroed_keeps_biases_and_time_constants_and_zeroes_all_604_strengths() {
        let p = scaled(3);
        let z = zeroed(&p);
        assert_eq!(z.syn_strength.len(), 604);
        assert!(z.syn_strength.iter().all(|s| s.to_bits() == 0), "every strength is +0.0");
        assert_eq!((z.bias, z.time_const), (p.bias, p.time_const));
    }

    #[test]
    fn shuffled_is_a_durstenfeld_permutation_of_the_strengths() {
        let mut p = ModelParams::pretrained();
        for (i, s) in p.syn_strength.iter_mut().enumerate() {
            *s = i as f32;
        }
        let seed = control_seed(Control::Shuffled, 0, 0);
        let q = shuffled(&p, &mut Rng::seed_from_u64(seed));
        assert_eq!(&q.syn_strength[..6], &[272.0, 155.0, 332.0, 262.0, 416.0, 459.0]);
        assert_eq!(&q.syn_strength[601..], &[9.0, 447.0, 44.0]);
        let mut sorted = q.syn_strength.to_vec();
        sorted.sort_by(f32::total_cmp);
        assert_eq!(sorted, p.syn_strength.to_vec());
        assert_eq!((q.bias, q.time_const), (p.bias, p.time_const));
        assert_eq!(shuffled(&p, &mut Rng::seed_from_u64(seed)), q, "the same seed, the same draw");
    }

    #[test]
    fn random_parameters_lie_within_each_parameters_range_and_repeat_by_seed() {
        let mut models = [scaled(0), scaled(5), scaled(10)];
        for m in models.iter_mut() {
            m.bias[0] = 0.25;
        }
        let range = ParamRange::over(&models).unwrap();
        let mut seen = std::collections::HashSet::new();
        for seed in 0..20u64 {
            let p = random_params(&range, &mut Rng::seed_from_u64(seed));
            for ((&v, &lo), &hi) in slots(&p).zip(slots(range.min())).zip(slots(range.max())) {
                assert!(lo <= v && v <= hi, "seed {seed}: {v} outside [{lo}, {hi}]");
            }
            assert_eq!(p.bias[0], 0.25, "a constant parameter stays constant");
            assert_eq!(random_params(&range, &mut Rng::seed_from_u64(seed)), p, "the same seed, the same draw");
            seen.insert(p.bias[1].to_bits());
        }
        assert!(seen.len() >= 2, "a drawn value depends on its draw");
        // slot 1 (bias[1]) takes the second draw: lo + (hi - lo) u in f64
        let mut g = Rng::seed_from_u64(0);
        g.next_f32();
        let u = g.next_f32();
        let (lo, hi) = (range.min().bias[1], range.max().bias[1]);
        let want = ((lo as f64 + (hi as f64 - lo as f64) * u as f64) as f32).max(lo).min(hi);
        assert_eq!(random_params(&range, &mut Rng::seed_from_u64(0)).bias[1], want);
        // exactly 734 draws, whatever the data
        let mut a = Rng::seed_from_u64(1);
        random_params(&range, &mut a);
        let mut b = Rng::seed_from_u64(1);
        for _ in 0..734 {
            b.next_f32();
        }
        assert_eq!(a.next_u64(), b.next_u64());
        assert!(matches!(ParamRange::over(std::iter::empty()), Err(ParityError::Usage(_))));
    }

    #[test]
    fn an_unstable_draw_is_redrawn_with_the_next_seed() {
        let mut seen = Vec::new();
        let d = draw_stable(Control::Shuffled, 4, |seed| {
            seen.push(seed);
            if seen.len() < 4 { Err(unstable()) } else { Ok(seed) }
        })
        .unwrap();
        let want: Vec<u64> = (0..4).map(|a| control_seed(Control::Shuffled, 4, a)).collect();
        assert_eq!(seen, want);
        assert_eq!((d.value, d.attempt, d.seed), (want[3], 3, want[3]));
        let redraws: Vec<(u32, u64)> = d.redraws.iter().map(|r| (r.attempt, r.seed)).collect();
        assert_eq!(redraws, vec![(0, want[0]), (1, want[1]), (2, want[2])]);
    }

    #[test]
    fn an_original_and_ten_unstable_redraws_are_a_gate_failure_and_zeroed_gets_one() {
        let mut seeds = Vec::new();
        let e = draw_stable(Control::RandomParams, 0, |seed| {
            seeds.push(seed);
            Err::<(), _>(unstable())
        })
        .unwrap_err();
        assert!(e.is_gate_failure());
        // the original draw plus 10 redraws: 11 attempts
        assert_eq!(MAX_REDRAWS, 10);
        assert!(matches!(e, DrawError::Unstable { ref redraws, .. } if redraws.len() == 11), "{e}");
        let want: Vec<u64> = (0..=MAX_REDRAWS).map(|a| control_seed(Control::RandomParams, 0, a)).collect();
        assert_eq!(seeds, want);
        // a draw that needs its 10th redraw still counts
        let mut calls = 0;
        let d = draw_stable(Control::RandomParams, 0, |seed| {
            calls += 1;
            if calls <= 10 { Err(unstable()) } else { Ok(seed) }
        })
        .unwrap();
        assert_eq!((d.attempt, d.redraws.len()), (10, 10));
        let e = draw_stable(Control::Zeroed, 0, |_| Err::<(), _>(unstable())).unwrap_err();
        assert!(matches!(e, DrawError::Unstable { ref redraws, .. } if redraws.len() == 1), "{e}");
    }

    #[test]
    fn any_other_error_ends_the_draw_without_a_redraw() {
        let mut calls = 0;
        let e = draw_stable(Control::Shuffled, 0, |_| {
            calls += 1;
            Err::<(), _>(ParityError::Usage("bad".into()))
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(!e.is_gate_failure());
        assert_eq!(e, DrawError::Failed { control: Control::Shuffled, model: 0, attempt: 0, error: ParityError::Usage("bad".into()) });
    }

    #[test]
    fn errors_display_as_registered() {
        let e = DrawError::Failed { control: Control::Rewired, model: 3, attempt: 2, error: ParityError::Usage("bad".into()) };
        assert_eq!(e.to_string(), "rewired control, model 3, attempt 2: bad");
        let redraws = vec![Redraw { attempt: 0, seed: 0x10, error: unstable() }, Redraw { attempt: 1, seed: 0x11, error: unstable() }];
        let e = DrawError::Unstable { control: Control::RandomParams, model: 5, redraws };
        assert_eq!(e.to_string(), "random parameters control, model 5: 2 draws in a row went non-finite (seeds 0x10, 0x11)");
    }

    /// Fixture-gated: the range over the real 50 models holds every model.
    #[test]
    fn the_real_ensembles_range_holds_every_model() {
        let Some(f) = fixture() else { return };
        let range = ParamRange::of_ensemble(&f.ensemble).unwrap_or_else(|e| panic!("{e}"));
        for id in f.ensemble.ids() {
            let m = f.ensemble.model(id).unwrap();
            for ((&v, &lo), &hi) in slots(m).zip(slots(range.min())).zip(slots(range.max())) {
                assert!(lo <= v && v <= hi, "model {id}");
            }
        }
        let constant = slots(range.min()).zip(slots(range.max())).filter(|(a, b)| a == b).count();
        eprintln!("random-parameter range over the 50 models: {constant} of 734 parameters are the same in every model");
        assert!(constant < 734);
    }

    /// Fixture-gated: on the real network a shuffled model 000 keeps every
    /// edge's sign, because the sign is the connection type's.
    #[test]
    fn shuffled_strengths_keep_every_connections_sign() {
        let Some(f) = fixture() else { return };
        let p = f.ensemble.model("000").unwrap();
        assert!(p.syn_strength.iter().all(|&s| s >= 0.0), "model 000's strengths are non-negative");
        let q = shuffled(p, &mut Rng::seed_from_u64(control_seed(Control::Shuffled, 0, 0)));
        let mut w = vec![0.0f32; f.net.num_edges()];
        f.net.weights_into(&q.syn_strength, &mut w).unwrap();
        for (e, (&x, &ct)) in w.iter().zip(f.net.edge_conn_type()).enumerate() {
            assert!(x == 0.0 || x.signum() == f.net.conn_sign()[ct as usize], "edge {e}");
        }
    }
}
```

- [ ] **Step 3: Run them to see them fail.** Run: `cargo test -p semblance-flyvis parity::controls::`. Expected: compile errors (`cannot find function control_seed`, …).

- [ ] **Step 4: Write the implementation.** Put this above the tests block in `controls.rs`:

```rust
//! Gate C's controls: networks that should not look like the fly.
//!
//! Each control is applied to each of the 50 trained models, giving 50
//! draws per control, each seeded through `semblance_rng` by
//! [`control_seed`]. A draw whose state goes non-finite is redrawn with the
//! next seed in its sequence ([`draw_stable`]), as Zhou & Hasler
//! rejection-sample. Running the draws through a battery and scoring them is
//! gate C, in `validate --ensemble`.
//!
//! Spec: docs/superpowers/specs/2026-11-04-representational-axis-design.md,
//! section 2.

use std::fmt;

use semblance_rng::Rng;

use super::ParityError;
use crate::ensemble::{Ensemble, ModelParams};

/// The four controls (owner's answer Q5), in seed order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control {
    /// The model's biases and time constants; every strength 0.
    Zeroed,
    /// The model's strengths permuted across connection types.
    Shuffled,
    /// Every parameter uniform within the ensemble's range for it.
    RandomParams,
    /// The model's parameters on a randomly rewired topology (ours, not
    /// Zhou & Hasler's).
    Rewired,
}

impl Control {
    pub const ALL: [Control; 4] = [Control::Zeroed, Control::Shuffled, Control::RandomParams, Control::Rewired];

    /// The control's index in [`control_seed`].
    pub const fn index(self) -> u64 {
        match self {
            Control::Zeroed => 0,
            Control::Shuffled => 1,
            Control::RandomParams => 2,
            Control::Rewired => 3,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Control::Zeroed => "zeroed",
            Control::Shuffled => "shuffled",
            Control::RandomParams => "random parameters",
            Control::Rewired => "rewired",
        }
    }

    /// Whether every draw is the same, so redrawing cannot help.
    pub const fn deterministic(self) -> bool {
        matches!(self, Control::Zeroed)
    }
}

/// The constant every control seed starts from.
pub const SEED_BASE: u64 = 0x5EB1_A9CE_0000_0000;

/// The most redraws one draw gets after its original attempt (RA §2:
/// "redrawn ... up to 10 times"): attempts 0..=MAX_REDRAWS. The original and
/// all 10 redraws non-finite is gate C's failure.
pub const MAX_REDRAWS: u32 = 10;

/// The seed of `control`'s draw for the model at ensemble position `model`,
/// attempt `attempt` (0 for the first draw, +1 per redraw):
///
/// ```text
/// SEED_BASE ^ (control.index() << 24) ^ (model << 8) ^ attempt
/// ```
///
/// Distinct for every model below 65,536 and attempt below 256.
pub fn control_seed(control: Control, model: usize, attempt: u32) -> u64 {
    SEED_BASE ^ (control.index() << 24) ^ ((model as u64) << 8) ^ attempt as u64
}

/// The zeroed control: `p`'s biases and time constants, all 604 strengths 0.
pub fn zeroed(p: &ModelParams) -> ModelParams {
    ModelParams { syn_strength: [0.0; 604], ..p.clone() }
}

/// The shuffled control: `p`'s 604 strengths permuted across connection
/// types by Durstenfeld's Fisher–Yates. A connection type's sign is the
/// network's, not the strength's, so every sign is kept.
pub fn shuffled(p: &ModelParams, rng: &mut Rng) -> ModelParams {
    let mut s = p.syn_strength;
    for i in (1..s.len()).rev() {
        let j = rng.below(i + 1);
        s.swap(i, j);
    }
    ModelParams { syn_strength: s, ..p.clone() }
}

/// Each of the 734 parameters' minimum and maximum over a set of models.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamRange {
    min: ModelParams,
    max: ModelParams,
}

impl ParamRange {
    /// The range over `models`; refused (`Usage`) if there are none.
    pub fn over<'a>(models: impl IntoIterator<Item = &'a ModelParams>) -> Result<ParamRange, ParityError> {
        let mut it = models.into_iter();
        let first = it.next().ok_or_else(|| ParityError::Usage("a parameter range needs at least one model".into()))?;
        let (mut min, mut max) = (first.clone(), first.clone());
        for m in it {
            for ((lo, hi), &v) in slots_mut(&mut min).zip(slots_mut(&mut max)).zip(slots(m)) {
                *lo = lo.min(v);
                *hi = hi.max(v);
            }
        }
        Ok(ParamRange { min, max })
    }

    /// The range over every model of the ensemble.
    pub fn of_ensemble(e: &Ensemble) -> Result<ParamRange, ParityError> {
        Self::over(e.ids().filter_map(|id| e.model(id)))
    }

    pub fn min(&self) -> &ModelParams {
        &self.min
    }

    pub fn max(&self) -> &ModelParams {
        &self.max
    }
}

/// The random-parameters control: each parameter `lo + (hi - lo) * u` in
/// f64, `u = rng.next_f32()`, rounded to f32 and clamped to `[lo, hi]`, in
/// the 734-row order (biases, time constants, strengths). Every parameter
/// consumes one draw, constant or not.
pub fn random_params(range: &ParamRange, rng: &mut Rng) -> ModelParams {
    let mut out = range.min.clone();
    for ((v, &lo), &hi) in slots_mut(&mut out).zip(slots(&range.min)).zip(slots(&range.max)) {
        let u = rng.next_f32();
        let x = (lo as f64 + (hi as f64 - lo as f64) * u as f64) as f32;
        *v = x.max(lo).min(hi);
    }
    out
}

/// One non-finite attempt that was redrawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Redraw {
    pub attempt: u32,
    pub seed: u64,
    pub error: ParityError,
}

/// A stable draw: its value, the attempt and seed that gave it, and the
/// attempts before it that went non-finite.
#[derive(Clone, Debug, PartialEq)]
pub struct Draw<T> {
    pub value: T,
    pub attempt: u32,
    pub seed: u64,
    pub redraws: Vec<Redraw>,
}

/// Why a draw has no value.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawError {
    /// Every attempt went non-finite: gate C's failure.
    Unstable { control: Control, model: usize, redraws: Vec<Redraw> },
    /// An attempt failed for another reason: verification did not complete.
    Failed { control: Control, model: usize, attempt: u32, error: ParityError },
}

impl DrawError {
    /// Whether this is a gate failure (exit 1) rather than verification
    /// not completing (exit 2).
    pub fn is_gate_failure(&self) -> bool {
        matches!(self, DrawError::Unstable { .. })
    }
}

impl fmt::Display for DrawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DrawError::Unstable { control, model, redraws } => {
                let seeds: Vec<String> = redraws.iter().map(|r| format!("{:#x}", r.seed)).collect();
                write!(
                    f,
                    "{} control, model {model}: {} draws in a row went non-finite (seeds {})",
                    control.name(),
                    redraws.len(),
                    seeds.join(", ")
                )
            }
            DrawError::Failed { control, model, attempt, error } => {
                write!(f, "{} control, model {model}, attempt {attempt}: {error}", control.name())
            }
        }
    }
}

impl std::error::Error for DrawError {}

/// Draws `control` for the model at position `model`: calls `run` with
/// attempt 0's seed, and while it returns a non-finite state
/// (`ParityError::is_gate_failure`) calls it again with the next attempt's
/// seed, up to [`MAX_REDRAWS`] redraws (attempts 0..=MAX_REDRAWS; one
/// attempt for the deterministic zeroed control). Any other error ends the draw. The redraws are returned for the
/// caller to print and record.
pub fn draw_stable<T>(
    control: Control,
    model: usize,
    mut run: impl FnMut(u64) -> Result<T, ParityError>,
) -> Result<Draw<T>, DrawError> {
    let last = if control.deterministic() { 0 } else { MAX_REDRAWS };
    let mut redraws = Vec::new();
    for attempt in 0..=last {
        let seed = control_seed(control, model, attempt);
        match run(seed) {
            Ok(value) => return Ok(Draw { value, attempt, seed, redraws }),
            Err(error) if error.is_gate_failure() => redraws.push(Redraw { attempt, seed, error }),
            Err(error) => return Err(DrawError::Failed { control, model, attempt, error }),
        }
    }
    Err(DrawError::Unstable { control, model, redraws })
}

/// The 734 parameters in row order: biases, time constants, strengths.
fn slots(p: &ModelParams) -> impl Iterator<Item = &f32> + '_ {
    p.bias.iter().chain(&p.time_const).chain(&p.syn_strength)
}

fn slots_mut(p: &mut ModelParams) -> impl Iterator<Item = &mut f32> + '_ {
    p.bias.iter_mut().chain(p.time_const.iter_mut()).chain(p.syn_strength.iter_mut())
}
```

- [ ] **Step 5: Run them to see them pass.** Run: `env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis parity::controls::`. Expected: 10 passed; the fixture tests print the constant-parameter count. Then `cargo test -p semblance --test acceptance_release` (RELEASE-01, -02, -07 still pass) and `cargo test -p semblance-flyvis --test acceptance_parity parity_01` (the parity-sources check still passes).

- [ ] **Step 6: Run clippy.** `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.

- [ ] **Step 7: Re-measure the test total (count owner).** Run the whole suite as Global Constraints' "Measuring the total" says and write the measured number into CLAUDE.md's two test-total figures. Change nothing else in CLAUDE.md.

- [ ] **Step 8: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed`.

- [ ] **Step 9: Commit.**

```bash
git add crates/flyvis/src/parity/controls.rs crates/flyvis/src/parity/mod.rs crates/flyvis/Cargo.toml Cargo.lock CLAUDE.md
git commit -m "Add gate C's seeded zeroed, shuffled and random-parameter controls"
```

---

### Task idx-edges: edge peaks, DSI, preferred direction and tuning correlation

Owns: crates/fidelity/src/indices.rs, CLAUDE.md
Uses: indices-layout
Provides: indices-edges
Wave: 2

**What this builds.** In `crates/fidelity/src/indices.rs` (module `semblance_fidelity::indices`; it already holds `Stimulus`, `Window`, `BatteryLayout`, `IndexError`, `N_ANGLES = 12`, `N_SPEEDS = 6`, the FRI, and the test helpers `responses(types, precision, stimuli, fill)`, `window(first, last)`, `entry(label, stimulus, window)`): `EdgePeaks`, `edge_peaks`, `dsi`, `preferred_direction`, `TuningR`, `tuning_r`, `widen_curves`, and the private helpers `column`, `pearson` and `DEG` that task `idx-known` uses. This task is wave 2's **count owner** (Step 5).

**Facts this task needs (flyvis `analysis/moving_bar_responses.py`, `analysis/direction_selectivity.py`, task 0).**
- **Edge peak** (flyvis `peak_responses`, `ensemble_view.py:270` passes no baseline and no norm): `responses.where(masks, other=0).clip(min=0).max('frame')`: frames outside the window are zeroed, then everything is clipped at 0, then the max is taken. So per edge stimulus and cell type, `peak = max(0, max over the window of the raw central voltage)`; a zero peak is +0.0. Peaks are exact copies of response values, or +0.0.
- The real battery: 12 angles (0, 30, …, 330) × intensity {0, 1} × 6 speeds (2.4, 4.8, 9.7, 13, 19, 25 columns/s): 144 edges. Flyvis's reductions see intensities, speeds and angles sorted ascending (xarray's unstack sorts): sums over angle run in ascending angle order, means over speed in ascending speed order.
- **DSI**, per intensity (`direction_selectivity_index`): `θ = deg2rad(angle)` = `angle * (π / 180)` in f64 (numpy computes `x * (NPY_PI / 180.0)`); `z = peak · e^{iθ}` in complex128, i.e. `(peak·cos θ, peak·sin θ)`; `vector_length = |Σ_θ z|` (complex magnitude: `hypot`); `normalization = max over intensity of Σ_θ |z|` (|z| is again `hypot`), **one denominator shared by ON and OFF**, per speed; `dsi = vector_length / (normalization + 1e-15)` per (intensity, speed); then **the mean over the 6 speeds**. Result `[Vec<f64>; 2]`, `[intensity 0, intensity 1]`, each 65 values in `Responses` type order.
- **Preferred direction**, per intensity (`preferred_direction`, `average=True`): `θ_pref = angle(Σ over angle and speed of peak·e^{iθ})`, an **un-normalised** sum over all speeds, as `atan2(im, re)` in (−π, π].
- **Tuning correlation** (`correlation_to_known_tuning_curves`, `absmax=False`): for each known type (T4a–d, T5a–d), each intensity (both kept) and each speed, the Pearson r over the 12 angles of the peaks against Maisak 2013's 12-point curve; then `fillna(0)` (a constant input gives r = 0); then **the speed with the maximum r** is kept (best of 6, not a mean; numpy/xarray `argmax`, the first on ties). Flyvis's curve is `interp1d(gt_angles, gt, kind='cubic')` at the same 12 angles, the identity to about 1e-16, so the values are used directly. Pearson is xarray's `corr`: `mean((a−ā)(b−b̄)) / (std_a · std_b)` with population std (ddof 0).
- Maisak's curves are stored in `semblance_flyvis::groundtruth::TUNING_CURVES` as `[f32; 12]`; widening an f32 such as 0.745 gives 0.7450000047683716, about 1e-8 from flyvis's f64 literal, which would move r past gate A's 1e-10 bound. So `tuning_r` takes `[f64; 12]` (a recorded deviation from the spec's `[f32; 12]`), and `widen_curves` widens through each f32's shortest decimal string (every literal has at most 4 significant digits), which recovers flyvis's literal.
- The f32 caveat (A9): flyvis's `xr.corr` demeans an f32 run's peaks in f32; here the reductions run in f64, as the spec says. Gate A's f32 rows are bounded, which should absorb it; the doc comment says so.

**Files.** Modify: `crates/fidelity/src/indices.rs`; `CLAUDE.md` (the two test-total figures).

- [ ] **Step 1: Write the failing tests.** Append inside `mod tests`:

```rust
    const SPEEDS: [f64; 6] = [2.4, 4.8, 9.7, 13.0, 19.0, 25.0];

    /// The real edge battery's shape over `speeds` (in that order): intensity,
    /// then speed, then angle. Windows are frame 0; the reductions never read
    /// them.
    fn edge_layout(speeds: &[f64]) -> BatteryLayout {
        let angles: Vec<u16> = (0..12u16).map(|a| a * 30).collect();
        edge_layout_in(&[0, 1], speeds, &angles)
    }

    /// The edge battery listed intensity by intensity, speed by speed and
    /// angle by angle, each in the order given.
    fn edge_layout_in(intensities: &[u8], speeds: &[f64], angles: &[u16]) -> BatteryLayout {
        let mut v = Vec::new();
        for &intensity in intensities {
            for &speed in speeds {
                for &angle_deg in angles {
                    let label = format!("edge {intensity} {speed} {angle_deg}");
                    v.push(entry(&label, Stimulus::Edge { angle_deg, speed, intensity }, window(0, 0)));
                }
            }
        }
        BatteryLayout::new(v).unwrap()
    }

    /// Edge peaks over `b`'s edges: `f(intensity, speed, angle, type)`.
    fn edge_peaks_for(b: &BatteryLayout, types: &[&str], f: impl Fn(u8, f64, u16, usize) -> f64) -> EdgePeaks {
        let rows = b
            .stimuli()
            .iter()
            .filter_map(|(_, s, _)| match *s {
                Stimulus::Edge { angle_deg, speed, intensity } => {
                    Some((0..types.len()).map(|t| f(intensity, speed, angle_deg, t)).collect::<Vec<f64>>())
                }
                Stimulus::Flash { .. } => None,
            })
            .collect();
        EdgePeaks::new(types.iter().map(|t| t.to_string()).collect(), Precision::F64, rows).unwrap()
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * b.abs().max(1.0)
    }

    #[test]
    fn edge_peaks_are_the_clipped_window_maximum() {
        let b = BatteryLayout::new(vec![
            entry("flash", Stimulus::Flash { radius: 6, intensity: 1 }, window(0, 3)),
            entry("e0", Stimulus::Edge { angle_deg: 0, speed: 19.0, intensity: 1 }, window(1, 2)),
            entry("e1", Stimulus::Edge { angle_deg: 30, speed: 19.0, intensity: 1 }, window(1, 3)),
        ])
        .unwrap();
        // A: a big value outside the window is ignored. B: all negative, so +0.0.
        let r = responses(&["A", "B"], Precision::F64, &[("flash", 4), ("e0", 4), ("e1", 4)], |s, f, t| match (s, t) {
            (1, 0) => [9.0, 0.5, 0.25, 9.0][f],
            (2, 0) => [9.0, 0.1, 0.7, 0.3][f],
            (_, _) => -1.0 - f as f64,
        });
        let p = edge_peaks(&r, &b).unwrap();
        assert_eq!(p.peaks(), &[vec![0.5, 0.0], vec![0.7, 0.0]]);
        assert!(p.peaks().iter().all(|row| row[1].to_bits() == 0), "a zero peak is +0.0");
        let renamed = BatteryLayout::new(vec![
            entry("flash", Stimulus::Flash { radius: 6, intensity: 1 }, window(0, 3)),
            entry("e0", Stimulus::Edge { angle_deg: 0, speed: 19.0, intensity: 1 }, window(1, 2)),
            entry("E1", Stimulus::Edge { angle_deg: 30, speed: 19.0, intensity: 1 }, window(1, 3)),
        ])
        .unwrap();
        assert!(matches!(edge_peaks(&r, &renamed), Err(IndexError::LabelMismatch { stimulus: 2, .. })));
        let flashes_only = BatteryLayout::new(vec![entry("flash", Stimulus::Flash { radius: 6, intensity: 1 }, window(0, 3))]).unwrap();
        let r1 = responses(&["A"], Precision::F64, &[("flash", 4)], |_, _, _| 0.0);
        assert_eq!(edge_peaks(&r1, &flashes_only), Err(IndexError::Empty("edge stimuli")));
    }

    #[test]
    fn the_dsi_shares_one_denominator_between_on_and_off() {
        // ON: peak 1 at 0 and 180 degrees (vector ~0, sum of |z| 2). OFF:
        // peak 0.5 at 0 degrees (vector 0.5). OFF's DSI is 0.5 over ON's 2.
        let b = edge_layout(&SPEEDS);
        let p = edge_peaks_for(&b, &["A"], |i, _, angle, _| match (i, angle) {
            (1, 0) | (1, 180) => 1.0,
            (0, 0) => 0.5,
            _ => 0.0,
        });
        let d = dsi(&p, &b).unwrap();
        assert!(close(d[0][0], 0.5 / (2.0 + 1e-15), 1e-15), "{}", d[0][0]);
        assert!(d[1][0] < 1e-15, "{}", d[1][0]);
        // Mirrored, so OFF holds the larger sum of |z|: the denominator is the
        // max over intensity, not always ON's.
        let p = edge_peaks_for(&b, &["A"], |i, _, angle, _| match (i, angle) {
            (0, 0) | (0, 180) => 1.0,
            (1, 0) => 0.5,
            _ => 0.0,
        });
        let d = dsi(&p, &b).unwrap();
        assert!(close(d[1][0], 0.5 / (2.0 + 1e-15), 1e-15), "{}", d[1][0]);
        assert!(d[0][0] < 1e-15, "{}", d[0][0]);
        // A peak so large that hypot overflows: the DSI is not finite, refused.
        let huge = edge_peaks_for(&b, &["A"], |_, _, _, _| 1e308);
        assert!(matches!(dsi(&huge, &b), Err(IndexError::NonFinite(_))));
    }

    #[test]
    fn the_dsi_is_the_mean_over_the_six_speeds() {
        // ON only: peak 1 at 0 degrees and a at 180, so each speed's DSI is
        // (1 - a) / (1 + a + 1e-15).
        let a = [0.0, 1.0 / 3.0, 0.5, 0.0, 0.0, 0.0];
        let b = edge_layout(&SPEEDS);
        let p = edge_peaks_for(&b, &["A"], |i, speed, angle, _| {
            let s = SPEEDS.iter().position(|&v| v == speed).unwrap();
            match (i, angle) {
                (1, 0) => 1.0,
                (1, 180) => a[s],
                _ => 0.0,
            }
        });
        let want = a.iter().map(|x| (1.0 - x) / (1.0 + x + 1e-15)).sum::<f64>() / 6.0;
        assert!(close(dsi(&p, &b).unwrap()[1][0], want, 1e-12));
    }

    #[test]
    fn the_preferred_direction_sums_over_speeds_unnormalised() {
        // speed 2.4: peak 3 at 0 degrees; speed 4.8: peak 1 at 90 degrees.
        // Unnormalised: atan2(1, 3); normalised per speed it would be pi/4.
        let b = edge_layout(&SPEEDS);
        let p = edge_peaks_for(&b, &["A"], |i, speed, angle, _| match (i, speed, angle) {
            (0, 2.4, 0) => 3.0,
            (0, 4.8, 90) => 1.0,
            _ => 0.0,
        });
        let pd = preferred_direction(&p, &b).unwrap();
        assert!(close(pd[0][0], 0.3217505543966422, 1e-12), "{}", pd[0][0]);
        assert_eq!(pd[1][0], 0.0, "an all-zero response points at angle 0");
    }

    #[test]
    fn tuning_keeps_the_best_speed_first_on_ties_and_a_flat_curve_scores_0() {
        let curve: [f64; 12] = std::array::from_fn(|a| a as f64);
        let b = edge_layout(&SPEEDS);
        // ON: speeds 9.7 and 19 match the curve exactly (identical r); the
        // others are scrambled. OFF: flat everywhere, so r is NaN, filled 0.
        let p = edge_peaks_for(&b, &["T4a", "Mi1"], |i, speed, angle, _| {
            let a = (angle / 30) as f64;
            match i {
                1 if speed == 9.7 || speed == 19.0 => 2.0 * a + 1.0,
                1 => ((angle / 30) * 7 % 12) as f64,
                _ => 0.5,
            }
        });
        let t = tuning_r(&p, &b, &[("T4a", curve)]).unwrap();
        assert_eq!(t.cell_types(), &["T4a".to_string()]);
        assert_eq!(t.speeds(), SPEEDS);
        let r = t.r("T4a").unwrap();
        assert_eq!(r[0], [0.0; 6], "fillna(0) on a flat curve");
        assert!(close(r[1][2], 1.0, 1e-12) && r[1][2] == r[1][4]);
        let best = t.best("T4a").unwrap();
        assert_eq!(best[0], (0.0, 0));
        assert_eq!(best[1], (r[1][2], 2), "the first of two equal speeds");
        assert_eq!(t.r("Mi1"), None);
        assert_eq!(tuning_r(&p, &b, &[("T5a", curve)]), Err(IndexError::MissingCellType("T5a".into())));
        let mut broken = curve;
        broken[3] = f64::NAN;
        assert!(matches!(tuning_r(&p, &b, &[("T4a", broken)]), Err(IndexError::BadKnown { .. })));
    }

    #[test]
    fn the_edge_reductions_do_not_depend_on_the_layouts_order() {
        let up = edge_layout(&SPEEDS);
        let mut reversed = SPEEDS;
        reversed.reverse();
        let down = edge_layout(&reversed);
        let f = |i: u8, speed: f64, angle: u16, t: usize| (speed * 0.1 + angle as f64 / 360.0 + i as f64 + t as f64).sin().abs();
        let (pu, pd) = (edge_peaks_for(&up, &["T4a"], f), edge_peaks_for(&down, &["T4a"], f));
        assert_eq!(dsi(&pu, &up), dsi(&pd, &down));
        assert_eq!(preferred_direction(&pu, &up), preferred_direction(&pd, &down));
        let curve: [f64; 12] = std::array::from_fn(|a| (a as f64).cos());
        assert_eq!(tuning_r(&pu, &up, &[("T4a", curve)]), tuning_r(&pd, &down, &[("T4a", curve)]));
        // Intensities 1 then 0, speeds scrambled, angles descending within
        // each (intensity, speed) group, and a response far from symmetric
        // in the angle: an angle taken from a row's position, or an
        // assumed intensity order, moves the DSI and PD by far more than
        // rounding.
        let descending: Vec<u16> = (0..12u16).rev().map(|a| a * 30).collect();
        let scrambled = edge_layout_in(&[1, 0], &[13.0, 2.4, 25.0, 4.8, 19.0, 9.7], &descending);
        let g = |i: u8, speed: f64, angle: u16, _t: usize| (angle as f64 / 30.0 + 1.0) * (1.0 + speed / 100.0) + i as f64;
        let (gu, gs) = (edge_peaks_for(&up, &["T4a"], g), edge_peaks_for(&scrambled, &["T4a"], g));
        assert_eq!(dsi(&gu, &up), dsi(&gs, &scrambled));
        assert_eq!(preferred_direction(&gu, &up), preferred_direction(&gs, &scrambled));
        assert_eq!(tuning_r(&gu, &up, &[("T4a", curve)]), tuning_r(&gs, &scrambled, &[("T4a", curve)]));
    }

    #[test]
    fn the_edge_reductions_refuse_the_wrong_battery_shape() {
        let p_for = |b: &BatteryLayout| edge_peaks_for(b, &["A"], |_, _, _, _| 1.0);
        let five = edge_layout(&SPEEDS[..5]);
        assert!(matches!(dsi(&p_for(&five), &five), Err(IndexError::BatteryShape(_))));
        let full = edge_layout(&SPEEDS);
        // drop one angle: 143 edges
        let short = BatteryLayout::new(full.stimuli()[1..].to_vec()).unwrap();
        assert!(matches!(dsi(&p_for(&short), &short), Err(IndexError::BatteryShape(_))));
        // repeat one angle in place of another
        let mut twice = full.stimuli().to_vec();
        twice[1].1 = Stimulus::Edge { angle_deg: 0, speed: 2.4, intensity: 0 };
        let twice = BatteryLayout::new(twice).unwrap();
        assert!(matches!(preferred_direction(&p_for(&twice), &twice), Err(IndexError::BatteryShape(_))));
        // an intensity that is neither 0 nor 1
        let mut bright = full.stimuli().to_vec();
        bright[0].1 = Stimulus::Edge { angle_deg: 0, speed: 2.4, intensity: 2 };
        let bright = BatteryLayout::new(bright).unwrap();
        assert!(matches!(tuning_r(&p_for(&bright), &bright, &[]), Err(IndexError::BatteryShape(_))));
        // peaks that do not match the layout's edge count
        let p = p_for(&full);
        assert_eq!(dsi(&p, &short), Err(IndexError::Length { what: "edge peaks", expected: 143, found: 144 }));
    }

    #[test]
    fn edge_peaks_built_by_hand_are_checked() {
        let names = || vec!["A".to_string(), "B".to_string()];
        assert_eq!(EdgePeaks::new(names(), Precision::F64, vec![]), Err(IndexError::Empty("edge peaks")));
        assert_eq!(
            EdgePeaks::new(names(), Precision::F64, vec![vec![1.0]]),
            Err(IndexError::Length { what: "edge peak row", expected: 2, found: 1 })
        );
        assert!(matches!(EdgePeaks::new(names(), Precision::F64, vec![vec![1.0, f64::NAN]]), Err(IndexError::NonFinite(_))));
        assert!(matches!(EdgePeaks::new(names(), Precision::F32, vec![vec![1.0, 0.1]]), Err(IndexError::NotF32(_))));
    }

    #[test]
    fn edge_peaks_refuse_an_f32_run_holding_non_f32_values() {
        // 0.1 is below the window's maximum 0.5, so only a check of every
        // window value catches it.
        let b = BatteryLayout::new(vec![entry("e0", Stimulus::Edge { angle_deg: 0, speed: 19.0, intensity: 1 }, window(1, 2))]).unwrap();
        let r = responses(&["A"], Precision::F32, &[("e0", 4)], |_, f, _| if f == 1 { 0.1 } else { 0.5 });
        assert!(matches!(edge_peaks(&r, &b), Err(IndexError::NotF32(_))));
        let ok = responses(&["A"], Precision::F32, &[("e0", 4)], |_, f, _| if f == 1 { 0.25 } else { 0.5 });
        assert_eq!(edge_peaks(&ok, &b).unwrap().peaks(), &[vec![0.5]]);
    }

    #[test]
    fn widened_curves_are_flyviss_decimal_literals() {
        let w = widen_curves(&[("T4a", [0.745f32; 12])]);
        assert_eq!(w[0].0, "T4a");
        assert_eq!(w[0].1, [0.745f64; 12]);
        assert_ne!(0.745f32 as f64, 0.745f64, "a plain widening would not be");
    }
```

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p semblance-fidelity indices::tests`. Expected: compile errors (`cannot find function edge_peaks`, …).

- [ ] **Step 3: Write the implementation.** Add above `#[cfg(test)]`:

```rust
/// Degrees to radians as numpy's `deg2rad` multiplies: `x * (pi / 180)`.
const DEG: f64 = std::f64::consts::PI / 180.0;

/// Per edge stimulus (in layout order) and cell type (in `Responses`
/// order), flyvis's edge peak: `max(0, max over the window of the raw
/// central voltage)`, with the precision of the run it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgePeaks {
    cell_types: Vec<String>,
    precision: Precision,
    peaks: Vec<Vec<f64>>,
}

impl EdgePeaks {
    /// Refuses no rows, a row that is not one value per cell type, a
    /// non-finite value, and (for an f32 run) a value that is not an f32.
    pub fn new(cell_types: Vec<String>, precision: Precision, peaks: Vec<Vec<f64>>) -> Result<Self, IndexError> {
        if peaks.is_empty() {
            return Err(IndexError::Empty("edge peaks"));
        }
        for (e, row) in peaks.iter().enumerate() {
            if row.len() != cell_types.len() {
                return Err(IndexError::Length { what: "edge peak row", expected: cell_types.len(), found: row.len() });
            }
            for (t, &v) in row.iter().enumerate() {
                if !v.is_finite() {
                    return Err(IndexError::NonFinite(format!("edge {e}'s peak of {}", cell_types[t])));
                }
                if precision == Precision::F32 && v as f32 as f64 != v {
                    return Err(IndexError::NotF32(format!("edge {e}'s peak {v} of {}", cell_types[t])));
                }
            }
        }
        Ok(EdgePeaks { cell_types, precision, peaks })
    }

    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// `[edge][cell type]`, the layout's edges in layout order.
    pub fn peaks(&self) -> &[Vec<f64>] {
        &self.peaks
    }
}

/// flyvis's edge peaks: frames outside each edge's window zeroed, every
/// value clipped at 0, the maximum taken; no baseline, no norm. A zero peak
/// is +0.0. An f32 run's window value that is not an f32 is refused.
pub fn edge_peaks(r: &Responses, b: &BatteryLayout) -> Result<EdgePeaks, IndexError> {
    b.check(r)?;
    let n = r.cell_types().len();
    let f32_run = r.precision() == Precision::F32;
    let mut rows = Vec::new();
    for (s, (_, stimulus, w)) in b.stimuli().iter().enumerate() {
        if !matches!(stimulus, Stimulus::Edge { .. }) {
            continue;
        }
        let sr = r.stimulus(s).ok_or(IndexError::CountMismatch { layout: b.len(), responses: r.len() })?;
        let mut row = vec![0.0f64; n];
        for f in w.first()..=w.last() {
            let frame = sr.frame(f).ok_or(IndexError::WindowPastEnd { stimulus: s, last: w.last(), n_frames: sr.n_frames() })?;
            for (t, (peak, &v)) in row.iter_mut().zip(frame).enumerate() {
                // every window value, not only the maximum
                if f32_run && v as f32 as f64 != v {
                    return Err(IndexError::NotF32(format!("{}'s value {v} in stimulus {s}'s window", r.cell_types()[t])));
                }
                if v > *peak {
                    *peak = v;
                }
            }
        }
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(IndexError::Empty("edge stimuli"));
    }
    EdgePeaks::new(r.cell_types().to_vec(), r.precision(), rows)
}

/// The edge reductions' view of a layout: for each intensity (0, 1), each
/// speed (ascending) and each angle (0, 30, ..., 330), the row of
/// `EdgePeaks` holding it.
struct EdgeGrid {
    speeds: [f64; N_SPEEDS],
    rows: [[[usize; N_ANGLES]; N_SPEEDS]; 2],
}

fn edge_grid(p: &EdgePeaks, b: &BatteryLayout) -> Result<EdgeGrid, IndexError> {
    let edges: Vec<(u16, f64, u8)> = b
        .stimuli()
        .iter()
        .filter_map(|(_, s, _)| match *s {
            Stimulus::Edge { angle_deg, speed, intensity } => Some((angle_deg, speed, intensity)),
            Stimulus::Flash { .. } => None,
        })
        .collect();
    if edges.is_empty() {
        return Err(IndexError::Empty("edge stimuli"));
    }
    if edges.len() != p.peaks.len() {
        return Err(IndexError::Length { what: "edge peaks", expected: edges.len(), found: p.peaks.len() });
    }
    let mut speeds: Vec<f64> = edges.iter().map(|e| e.1).collect();
    speeds.sort_by(f64::total_cmp);
    speeds.dedup();
    if speeds.len() != N_SPEEDS {
        return Err(IndexError::BatteryShape(format!("the edge reductions need {N_SPEEDS} speeds, found {}", speeds.len())));
    }
    let mut rows = [[[usize::MAX; N_ANGLES]; N_SPEEDS]; 2];
    for (k, &(angle, speed, intensity)) in edges.iter().enumerate() {
        if intensity > 1 {
            return Err(IndexError::BatteryShape(format!("edge {k}: intensity {intensity} is not 0 or 1")));
        }
        if angle % 30 != 0 {
            return Err(IndexError::BatteryShape(format!("edge {k}: angle {angle} is not one of 0, 30, ..., 330")));
        }
        let s = speeds
            .iter()
            .position(|&v| v == speed)
            .ok_or_else(|| IndexError::BatteryShape(format!("edge {k}: speed {speed} is not listed")))?;
        let slot = &mut rows[intensity as usize][s][(angle / 30) as usize];
        if *slot != usize::MAX {
            return Err(IndexError::BatteryShape(format!(
                "edge {k} repeats intensity {intensity}, speed {speed}, angle {angle}"
            )));
        }
        *slot = k;
    }
    for (i, by_speed) in rows.iter().enumerate() {
        for (s, by_angle) in by_speed.iter().enumerate() {
            if let Some(a) = by_angle.iter().position(|&k| k == usize::MAX) {
                return Err(IndexError::BatteryShape(format!(
                    "no edge at intensity {i}, speed {}, angle {}",
                    speeds[s],
                    a * 30
                )));
            }
        }
    }
    let mut sorted = [0.0; N_SPEEDS];
    sorted.copy_from_slice(&speeds);
    Ok(EdgeGrid { speeds: sorted, rows })
}

/// `peak * e^{i theta}` for angle index `a` (angle `30 a` degrees), as
/// numpy's complex128 product with a real peak.
fn polar(peak: f64, a: usize) -> (f64, f64) {
    let theta = (a * 30) as f64 * DEG;
    (peak * theta.cos(), peak * theta.sin())
}

fn mean_in_order(v: &[f64]) -> f64 {
    let mut s = 0.0;
    for &x in v {
        s += x;
    }
    s / v.len() as f64
}

fn finite_pair(what: &str, types: &[String], out: &[Vec<f64>; 2]) -> Result<(), IndexError> {
    for (i, v) in out.iter().enumerate() {
        if let Some(t) = v.iter().position(|x| !x.is_finite()) {
            return Err(IndexError::NonFinite(format!("the {what} of {} at intensity {i}", types[t])));
        }
    }
    Ok(())
}

/// flyvis's direction selectivity index per intensity: per speed,
/// `|sum_theta z| / (max over intensity of sum_theta |z| + 1e-15)` with
/// `z = peak e^{i theta}` (one denominator shared by ON and OFF), then the
/// mean over the six speeds. Sums over angle ascend in angle, the mean
/// ascends in speed, whatever the layout's order.
pub fn dsi(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError> {
    let g = edge_grid(p, b)?;
    let n = p.cell_types.len();
    let mut out = [vec![0.0; n], vec![0.0; n]];
    for t in 0..n {
        let mut per_speed = [[0.0f64; N_SPEEDS]; 2];
        for s in 0..N_SPEEDS {
            let mut length = [0.0f64; 2];
            let mut total = [0.0f64; 2];
            for (i, (len, tot)) in length.iter_mut().zip(total.iter_mut()).enumerate() {
                let (mut re, mut im, mut sum_abs) = (0.0f64, 0.0f64, 0.0f64);
                for (a, &row) in g.rows[i][s].iter().enumerate() {
                    let (zr, zi) = polar(p.peaks[row][t], a);
                    re += zr;
                    im += zi;
                    sum_abs += zr.hypot(zi);
                }
                *len = re.hypot(im);
                *tot = sum_abs;
            }
            let norm = total[0].max(total[1]);
            for (ps, &len) in per_speed.iter_mut().zip(&length) {
                ps[s] = len / (norm + 1e-15);
            }
        }
        for (o, ps) in out.iter_mut().zip(&per_speed) {
            o[t] = mean_in_order(ps);
        }
    }
    finite_pair("DSI", &p.cell_types, &out)?;
    Ok(out)
}

/// flyvis's preferred direction per intensity: the angle (`atan2(im, re)`,
/// in (-pi, pi]) of the un-normalised sum of `peak e^{i theta}` over every
/// speed (ascending) and angle (ascending).
pub fn preferred_direction(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError> {
    let g = edge_grid(p, b)?;
    let n = p.cell_types.len();
    let mut out = [vec![0.0; n], vec![0.0; n]];
    for (i, o) in out.iter_mut().enumerate() {
        for (t, slot) in o.iter_mut().enumerate() {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for by_angle in &g.rows[i] {
                for (a, &row) in by_angle.iter().enumerate() {
                    let (zr, zi) = polar(p.peaks[row][t], a);
                    re += zr;
                    im += zi;
                }
            }
            *slot = im.atan2(re);
        }
    }
    finite_pair("preferred direction", &p.cell_types, &out)?;
    Ok(out)
}

/// Pearson r per known type, intensity and speed, against Maisak's curves.
#[derive(Clone, Debug, PartialEq)]
pub struct TuningR {
    cell_types: Vec<String>,
    speeds: [f64; N_SPEEDS],
    r: Vec<[[f64; N_SPEEDS]; 2]>,
}

impl TuningR {
    /// The known types, in the order `tuning_r` was given them.
    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    /// The speeds, ascending: the order of every `[f64; N_SPEEDS]`.
    pub fn speeds(&self) -> [f64; N_SPEEDS] {
        self.speeds
    }

    /// `[intensity][speed]` for one known type.
    pub fn r(&self, cell_type: &str) -> Option<[[f64; N_SPEEDS]; 2]> {
        self.cell_types.iter().position(|t| t == cell_type).map(|k| self.r[k])
    }

    /// Per intensity, the best r over the speeds and its speed index: the
    /// first on ties, as numpy's argmax.
    pub fn best(&self, cell_type: &str) -> Option<[(f64, usize); 2]> {
        let r = self.r(cell_type)?;
        Some(r.map(|by_speed| {
            let mut best = (by_speed[0], 0);
            for (s, &v) in by_speed.iter().enumerate().skip(1) {
                if v > best.0 {
                    best = (v, s);
                }
            }
            best
        }))
    }
}

/// flyvis's correlation to Maisak 2013's tuning curves (`absmax=False`):
/// per known type, intensity and speed, the Pearson r over the 12 angles of
/// the peaks against the curve, a constant input scoring 0 (`fillna(0)`).
/// `known` holds f64 curves equal to flyvis's literals (see
/// [`widen_curves`]). Flyvis demeans an f32 run's peaks in f32; this runs in
/// f64, as the spec says, and gate A's f32 rows are bounded.
pub fn tuning_r(p: &EdgePeaks, b: &BatteryLayout, known: &[(&str, [f64; 12])]) -> Result<TuningR, IndexError> {
    let g = edge_grid(p, b)?;
    let mut cell_types = Vec::with_capacity(known.len());
    let mut r = Vec::with_capacity(known.len());
    for &(name, curve) in known {
        let t = column(&p.cell_types, name)?;
        if curve.iter().any(|v| !v.is_finite()) {
            return Err(IndexError::BadKnown { cell_type: name.to_string(), reason: "the tuning curve is not finite".into() });
        }
        let mut by = [[0.0f64; N_SPEEDS]; 2];
        for (i, by_speed) in by.iter_mut().enumerate() {
            for (s, slot) in by_speed.iter_mut().enumerate() {
                let peaks: Vec<f64> = g.rows[i][s].iter().map(|&row| p.peaks[row][t]).collect();
                let v = pearson(&peaks, &curve);
                *slot = if v.is_nan() { 0.0 } else { v };
            }
        }
        cell_types.push(name.to_string());
        r.push(by);
    }
    Ok(TuningR { cell_types, speeds: g.speeds, r })
}

/// Maisak's f32 curves (`groundtruth::TUNING_CURVES`) as flyvis's f64
/// literals: each f32 through its shortest decimal string, then parsed as
/// f64 (0.745f32 becomes 0.745, not 0.7450000047683716).
pub fn widen_curves<'a>(curves: &[(&'a str, [f32; 12])]) -> Vec<(&'a str, [f64; 12])> {
    curves.iter().map(|&(name, c)| (name, c.map(|x| format!("{x}").parse().unwrap_or(x as f64)))).collect()
}

/// The position of cell type `name` in `types`.
fn column(types: &[String], name: &str) -> Result<usize, IndexError> {
    types.iter().position(|t| t == name).ok_or_else(|| IndexError::MissingCellType(name.to_string()))
}

/// xarray's `corr` over one dimension: the population covariance over the
/// product of the population standard deviations, each sum in order. NaN
/// when either input is constant.
fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (mean_in_order(a), mean_in_order(b));
    let (mut cov, mut va, mut vb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b) {
        let (dx, dy) = (x - ma, y - mb);
        cov += dx * dy;
        va += dx * dx;
        vb += dy * dy;
    }
    (cov / n) / ((va / n).sqrt() * (vb / n).sqrt())
}
```

- [ ] **Step 4: Run them to see them pass.** Run: `cargo test -p semblance-fidelity indices::`. Expected: all pass. `cargo clippy -p semblance-fidelity --all-targets -- -D warnings`: clean (rewrite any flagged loop with iterators, keeping the order of every sum).

- [ ] **Step 5: Re-measure the test total (count owner).** Run the whole suite as Global Constraints' "Measuring the total" says and write the measured number into CLAUDE.md's two test-total figures. Change nothing else in CLAUDE.md.

- [ ] **Step 6: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed`.

- [ ] **Step 7: Commit.**

```bash
git add crates/fidelity/src/indices.rs CLAUDE.md
git commit -m "Compute flyvis's edge peaks, DSI, preferred direction and tuning correlation"
```

---

### Task ctl-rewire: the rewired control

Owns: crates/flyvis/src/parity/controls.rs
Uses: respond-network, controls-draws
Provides: controls-rewire
Wave: 2

**What this builds.** The fourth control, appended to `crates/flyvis/src/parity/controls.rs` (module `semblance_flyvis::parity::controls`, which already holds `Control`, `control_seed`, `MAX_REDRAWS`, `zeroed`, `shuffled`, `ParamRange`, `random_params`, `draw_stable` and the private `slots`/`slots_mut`). `rewire` gives each connection type a new presynaptic cell type and rebuilds its edges; the result runs through `semblance_flyvis::parity::respond_network(f, &net, &groups, params, dt, precision, counts, battery)` (task `ctl-network`). It is **our addition, not Zhou & Hasler's** (they do not rewire; owner's answer Q5): the doc comment says so. This task adds at most 15 `#[test]`s (it adds 4) and does not touch CLAUDE.md.

**Facts this task needs (spec RA §2, owner's answer Q5, the network types).**
- Rewired: the model's parameters on a new topology. Each connection type keeps its target type, offsets, synapse counts, sign and strength, and **draws a new presynaptic type**. Its edges are rebuilt from the new type's node **in the same source column** (same `(u, v)`); an edge whose source column the new type lacks is dropped, and the count is reported. A draw that would give two connection types the same (source, target) pair is redrawn. The result must pass `TypedNetwork::new`.
- Decisions fixed here: new sources are drawn uniformly from all cell types with `rng.below(n_types)` (the original type may recur), connection types in index order, redrawing while the (source, target) pair is already taken by an earlier connection type; at most `MAX_SOURCE_TRIES = 1000` draws per connection type, then a `ParityError::Usage`. A kept edge keeps its target, connection type, `n_syn` and its synapse-count group (so torch's counts follow each edge); edge order is the original order minus the dropped edges. Nodes, input nodes and central nodes are unchanged.
- `semblance_connectome::{TypedNetwork, TypedNetworkParts}`: `TypedNetworkParts { cell_types: Vec<String>, node_cell_type: Vec<u16>, node_u: Vec<i16>, node_v: Vec<i16>, conn_types: Vec<(u16, u16)> /* (source type, target type) */, conn_sign: Vec<f32>, edge_source: Vec<u32>, edge_target: Vec<u32>, edge_conn_type: Vec<u16>, edge_n_syn: Vec<f32> }`; `TypedNetwork::new(parts) -> Result<TypedNetwork, NetworkError>` validates lengths, ranges and signs only (not that an edge's source has its connection type's source type, nor unique pairs: `rewire` keeps those itself); accessors of the same names, plus `num_nodes`, `num_edges`.
- Inside the crate, a `Fixtures`' network is `f.net`, its per-edge groups `f.groups` (one per edge, each < 2355), its ensemble `f.ensemble`; `crate::gate_support::fixture()` returns `Option<&'static Fixtures>` and prints the skip notice.
- Pinned (computed from `semblance-rng`'s algorithm, checked against its frozen vectors) for the small test network below (four connection types, types 0 and 3 sharing target B) with seeds `control_seed(Rewired, m, 0)`, m = 0..5: the new sources are `[2,1,1,0]`, `[0,2,0,2]`, `[2,1,0,1]`, `[2,0,0,0]`, `[2,1,1,1]`, `[2,2,2,1]` and the dropped counts `1, 1, 1, 1, 1, 2`. Models 0 and 4 exercise the redraw on a taken pair (type 3's first draw is 2, already type 0's: 3 redraws for model 0, 1 for model 4).

**Files.** Modify: `crates/flyvis/src/parity/controls.rs`.

- [ ] **Step 1: Write the failing tests.** Append inside `mod tests` (add `use semblance_connectome::TypedNetworkParts;`, `use crate::parity::{respond_network, Battery, Counts};` and `use crate::reference::Dtype;` to the tests module's imports):

```rust
    /// Three cell types on two columns: A and B at (0, 0) and (1, 0), C at
    /// (0, 0) only. Connection types A>B, B>C, C>A, C>B (types 0 and 3 share
    /// target B, so their draws can collide); five edges, groups 10..14.
    fn small() -> (TypedNetwork, Vec<u32>) {
        let net = TypedNetwork::new(TypedNetworkParts {
            cell_types: vec!["A".into(), "B".into(), "C".into()],
            node_cell_type: vec![0, 0, 1, 1, 2],
            node_u: vec![0, 1, 0, 1, 0],
            node_v: vec![0, 0, 0, 0, 0],
            conn_types: vec![(0, 1), (1, 2), (2, 0), (2, 1)],
            conn_sign: vec![1.0, -1.0, 1.0, 1.0],
            edge_source: vec![0, 1, 3, 4, 4],
            edge_target: vec![2, 2, 4, 1, 2],
            edge_conn_type: vec![0, 0, 1, 2, 3],
            edge_n_syn: vec![2.0, 3.0, 1.0, 4.0, 5.0],
        })
        .unwrap();
        (net, vec![10, 11, 12, 13, 14])
    }

    /// What every rewired network keeps, checked edge by edge.
    fn assert_rewired(net: &TypedNetwork, groups: &[u32], r: &Rewired) {
        assert_eq!(r.kept.len() + r.dropped, net.num_edges());
        assert_eq!((r.net.num_edges(), r.groups.len()), (r.kept.len(), r.kept.len()));
        assert_eq!(r.net.conn_sign(), net.conn_sign());
        assert_eq!(r.net.cell_types(), net.cell_types());
        assert_eq!((r.net.node_cell_type(), r.net.node_u(), r.net.node_v()), (net.node_cell_type(), net.node_u(), net.node_v()));
        let mut pairs = std::collections::HashSet::new();
        for (c, (&(s, t), &(_, t0))) in r.net.conn_types().iter().zip(net.conn_types()).enumerate() {
            assert_eq!(t, t0, "connection type {c} keeps its target");
            assert_eq!(s, r.sources[c]);
            assert!(pairs.insert((s, t)), "connection type {c} repeats a (source, target) pair");
        }
        for (k, &e) in r.kept.iter().enumerate() {
            let e = e as usize;
            let (old, new) = (net.edge_source()[e] as usize, r.net.edge_source()[k] as usize);
            assert_eq!(r.net.edge_target()[k], net.edge_target()[e]);
            assert_eq!(r.net.edge_conn_type()[k], net.edge_conn_type()[e]);
            assert_eq!(r.net.edge_n_syn()[k].to_bits(), net.edge_n_syn()[e].to_bits());
            assert_eq!(r.groups[k], groups[e]);
            assert_eq!((r.net.node_u()[new], r.net.node_v()[new]), (net.node_u()[old], net.node_v()[old]), "edge {e} keeps its source column");
            assert_eq!(r.net.node_cell_type()[new], r.sources[net.edge_conn_type()[e] as usize]);
        }
        assert!(r.kept.windows(2).all(|w| w[0] < w[1]), "kept edges stay in order");
    }

    #[test]
    fn rewiring_keeps_targets_columns_counts_and_signs_and_passes_validation() {
        let (net, groups) = small();
        let (mut dropped, mut sources) = (Vec::new(), Vec::new());
        for model in 0..20 {
            let seed = control_seed(Control::Rewired, model, 0);
            let r = rewire(&net, &groups, &mut Rng::seed_from_u64(seed)).unwrap();
            assert_rewired(&net, &groups, &r);
            assert_ne!(r.sources[0], r.sources[3], "model {model}: types 0 and 3 share target B");
            assert_eq!(rewire(&net, &groups, &mut Rng::seed_from_u64(seed)).unwrap(), r, "the same seed, the same draw");
            dropped.push(r.dropped);
            sources.push(r.sources.clone());
        }
        // Models 0 and 4 collide: type 3's first draw is type 0's source
        // (2, C), so it is redrawn from the stream (3 redraws for model 0,
        // 1 for model 4), not skipped forward; types not yet processed do
        // not block a pair (model 0's type 0 takes C>B, type 3's original).
        assert_eq!(
            sources[..6],
            [vec![2u16, 1, 1, 0], vec![0, 2, 0, 2], vec![2, 1, 0, 1], vec![2, 0, 0, 0], vec![2, 1, 1, 1], vec![2, 2, 2, 1]],
            "pins rewire's use of the frozen stream"
        );
        assert_eq!(dropped[..6], [1, 1, 1, 1, 1, 2], "pins rewire's use of the frozen stream");
    }

    #[test]
    fn a_target_with_more_connection_types_than_cell_types_is_refused_not_looped() {
        let crowded = TypedNetwork::new(TypedNetworkParts {
            cell_types: vec!["A".into()],
            node_cell_type: vec![0],
            node_u: vec![0],
            node_v: vec![0],
            conn_types: vec![(0, 0), (0, 0)],
            conn_sign: vec![1.0, 1.0],
            edge_source: vec![],
            edge_target: vec![],
            edge_conn_type: vec![],
            edge_n_syn: vec![],
        })
        .unwrap();
        let e = rewire(&crowded, &[], &mut Rng::seed_from_u64(1)).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.contains("no unused source type")), "{e}");
    }

    #[test]
    fn rewiring_refuses_groups_that_are_not_one_per_edge() {
        let (net, _) = small();
        let e = rewire(&net, &[0], &mut Rng::seed_from_u64(1)).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.contains("groups for")), "{e}");
    }

    /// One grey stimulus of `frames` frames.
    struct Grey {
        frames: usize,
        done: bool,
    }

    impl Battery for Grey {
        fn len(&self) -> usize {
            1
        }
        fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError> {
            if self.done {
                return Ok(None);
            }
            self.done = true;
            out.clear();
            out.resize(self.frames * 721, 0.5);
            Ok(Some("grey".into()))
        }
    }

    /// Fixture-gated: the real network rewires validly, and the rewired
    /// network either responds through `respond_network` or goes non-finite
    /// (a redraw, for gate C), never another error.
    #[test]
    fn the_real_network_rewires_and_responds_or_goes_non_finite() {
        let Some(f) = fixture() else { return };
        let seed = control_seed(Control::Rewired, 0, 0);
        let r = rewire(&f.net, &f.groups, &mut Rng::seed_from_u64(seed)).unwrap_or_else(|e| panic!("{e}"));
        assert_rewired(&f.net, &f.groups, &r);
        eprintln!("rewired control, model 000, attempt 0: {} of {} edges dropped", r.dropped, f.net.num_edges());
        let p = f.ensemble.model("000").unwrap();
        let mut b = Grey { frames: 2, done: false };
        match respond_network(f, &r.net, &r.groups, p, 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b) {
            Ok(resp) => eprintln!("rewired model 000 responds: {} frames of {} types", resp.stimulus(0).unwrap().n_frames(), resp.cell_types().len()),
            Err(e) => {
                assert!(e.is_gate_failure(), "{e}");
                eprintln!("rewired model 000 went non-finite: {e}");
            }
        }
    }
```

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p semblance-flyvis parity::controls::`. Expected: compile errors (`cannot find function rewire`, `Rewired`).

- [ ] **Step 3: Write the implementation.** Extend the imports at the top of `controls.rs`:

```rust
use std::collections::{HashMap, HashSet};

use semblance_connectome::{TypedNetwork, TypedNetworkParts};
```

and append above the tests block (after `draw_stable`, before `slots`):

```rust
/// The most draws one connection type gets for an unused (source, target)
/// pair before `rewire` gives up.
pub const MAX_SOURCE_TRIES: usize = 1000;

/// The rewired control's network. Our addition, not Zhou & Hasler's (they
/// do not rewire; owner's answer Q5): it tests the topology, where their
/// shuffles test the strengths.
#[derive(Clone, Debug, PartialEq)]
pub struct Rewired {
    /// The same cell types and nodes, the connection types with new
    /// sources, the surviving edges in their original order.
    pub net: TypedNetwork,
    /// Per surviving edge, its original edge's synapse-count group.
    pub groups: Vec<u32>,
    /// Per surviving edge, the index of the original edge it was rebuilt from.
    pub kept: Vec<u32>,
    /// Per connection type, its new source cell type.
    pub sources: Vec<u16>,
    /// Edges dropped because the new source type has no node in their
    /// source column; the caller prints it.
    pub dropped: usize,
}

/// The rewired control: each connection type, in index order, draws a new
/// presynaptic cell type uniformly from all of them (`rng.below`),
/// redrawing while an earlier connection type already holds that (source,
/// target) pair, at most [`MAX_SOURCE_TRIES`] times. Each edge is rebuilt
/// from the new type's node in its old source's column `(u, v)`, keeping its
/// target, connection type (so its sign and strength), synapse count and
/// synapse-count group; an edge whose column the new type lacks is dropped.
/// Refused (`Usage`): `groups` not one per edge, a connection type that
/// finds no unused pair, a result `TypedNetwork::new` refuses.
pub fn rewire(net: &TypedNetwork, groups: &[u32], rng: &mut Rng) -> Result<Rewired, ParityError> {
    if groups.len() != net.num_edges() {
        return Err(ParityError::Usage(format!("rewire: {} synapse-count groups for {} edges", groups.len(), net.num_edges())));
    }
    let n_types = net.cell_types().len();
    if n_types == 0 && !net.conn_types().is_empty() {
        return Err(ParityError::Usage("rewire: connection types but no cell types".into()));
    }
    let mut taken: HashSet<(u16, u16)> = HashSet::new();
    let mut sources = Vec::with_capacity(net.conn_types().len());
    for (c, &(_, target)) in net.conn_types().iter().enumerate() {
        let mut found = None;
        for _ in 0..MAX_SOURCE_TRIES {
            let s = rng.below(n_types) as u16;
            if taken.insert((s, target)) {
                found = Some(s);
                break;
            }
        }
        let s = found.ok_or_else(|| {
            ParityError::Usage(format!("rewire: connection type {c} found no unused source type in {MAX_SOURCE_TRIES} draws"))
        })?;
        sources.push(s);
    }

    let mut at: HashMap<(u16, i16, i16), u32> = HashMap::with_capacity(net.num_nodes());
    for (node, ((&t, &u), &v)) in net.node_cell_type().iter().zip(net.node_u()).zip(net.node_v()).enumerate() {
        at.insert((t, u, v), node as u32);
    }
    let (mut edge_source, mut edge_target, mut edge_conn_type, mut edge_n_syn) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut kept_groups, mut kept) = (Vec::new(), Vec::new());
    let mut dropped = 0usize;
    for (e, &g) in groups.iter().enumerate() {
        let c = net.edge_conn_type()[e];
        let old = net.edge_source()[e] as usize;
        match at.get(&(sources[c as usize], net.node_u()[old], net.node_v()[old])) {
            Some(&new) => {
                edge_source.push(new);
                edge_target.push(net.edge_target()[e]);
                edge_conn_type.push(c);
                edge_n_syn.push(net.edge_n_syn()[e]);
                kept_groups.push(g);
                kept.push(e as u32);
            }
            None => dropped += 1,
        }
    }
    let parts = TypedNetworkParts {
        cell_types: net.cell_types().to_vec(),
        node_cell_type: net.node_cell_type().to_vec(),
        node_u: net.node_u().to_vec(),
        node_v: net.node_v().to_vec(),
        conn_types: sources.iter().zip(net.conn_types()).map(|(&s, &(_, t))| (s, t)).collect(),
        conn_sign: net.conn_sign().to_vec(),
        edge_source,
        edge_target,
        edge_conn_type,
        edge_n_syn,
    };
    let rewired = TypedNetwork::new(parts).map_err(|e| ParityError::Usage(format!("rewire: {e}")))?;
    Ok(Rewired { net: rewired, groups: kept_groups, kept, sources, dropped })
}
```

Also add one line to the module doc comment's first paragraph: `//! The rewired control ([`rewire`]) is ours, not Zhou & Hasler's.`

- [ ] **Step 4: Run them to see them pass.** Run: `env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis parity::controls:: -- --nocapture`. Expected: all pass; read the printed dropped count and the response line. `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.

- [ ] **Step 5: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed`.

- [ ] **Step 6: Commit.**

```bash
git add crates/flyvis/src/parity/controls.rs
git commit -m "Add the rewired control, its synapse-count groups following each edge"
```

---

### Task idx-known: flyvis's to-known helpers and the paper's agreement rules

Owns: crates/fidelity/src/indices.rs, CLAUDE.md
Uses: indices-layout, indices-edges
Provides: indices-known
Wave: 3

**What this builds.** In `crates/fidelity/src/indices.rs` (module `semblance_fidelity::indices`; it already holds `IndexError`, the indices, and the private helpers `column(types: &[String], name: &str) -> Result<usize, IndexError>` (a missing name is `MissingCellType`), `pearson(a: &[f64], b: &[f64]) -> f64` (xarray's `corr`: population covariance over the product of population standard deviations, sums in order; NaN when an input is constant) and `DEG = PI / 180`, and in `mod tests` the helper `close(a: f64, b: f64, tol: f64) -> bool` (`(a - b).abs() <= tol * b.abs().max(1.0)`; do not define another)): flyvis's three `*_to_known` helpers recomputed, the paper's agreement rules, their constants with citations, and `Agreement`. All take plain slices and type names, never `Responses`, so the later gates can run them over flyvis's exported values too. This task is wave 3's **count owner** (Step 5).

**Facts this task needs (task 0, flyvis 1.2.0, the paper: Lappalainen et al., Nature 2024).**
- `fri_correlation_to_known`: the Pearson r of the FRI against the known ±1 polarity over the 32 nonzero-polarity types (R1–R8, L1–L5, C3, CT1(Lo1), CT1(M10), Mi1, Mi4, Mi9, T4a–d, T5a–d, Tm1–Tm4, Tm9), in `known`'s order (flyvis iterates groundtruth's dict order). A NaN (constant input) is an error here, not NaN.
- `dsi_correlation_to_known`: the Pearson r over the 18 `KNOWN_DSI_TYPES` of the DSI **maxed over intensity** against a 0/1 label (1.0 if the type is in `MOTION_TUNING` = T5a–d, T4a–d; else 0.0). flyvis selects with `.where(isin, drop=True)`, so the order is the network's (`types`) order.
- `angular_distance_to_known`: 8 distances, in `known`'s order (callers pass `PREFERRED_DIRECTIONS`: T4a 180, T4b 0, T4c 90, T4d 270, T5a 180, T5b 0, T5c 90, T5d 270). **T4 is read at intensity 1 and T5 at intensity 0.** The known angles in radians are flyvis's constants `0`, `np.pi/2`, `np.pi`, `3*np.pi/2`. The distance is flyvis's `simple_angle_distance` with `upper = π`:
  ```text
  a = a % (2*pi); b = b % (2*pi)      # numpy %: the result takes the divisor's sign (non-negative)
  y = a - b if a >= b else b - a
  if y > pi: y = 2*pi - y
  return y / pi * upper
  ```
  Distances only, no tolerance.
- Agreement rules (the paper's, recorded as such): polarity is sign(FRI) against the known polarity over the 32, **no dead zone**; an FRI of exactly 0.0 counts as a miss. The ensemble-median FRI per type is `np.median` (an even count averages the two middle values: `(a + b) / 2`). Direction selectivity: a type is direction selective when k = #{models with DSI > `d_thresh`} (strict) passes a one-sided binomial test, `P(X >= k) < alpha` (strict) for X ~ Binomial(n_models, p0). `d_thresh = 0.357` is the 99% quantile of a 100× angle-permutation null (paper, Methods "DSI"); p0 = 0.1 and alpha = 0.05 (Methods, "Determining whether a cell type with asymmetric inputs counts as direction selective"). At n = 50: k = 9 gives 0.057867205718094276, k = 10 gives 0.024537935704591455, so a type needs at least 10 of 50. Preferred direction agrees when the angular distance is ≤ 45°; the 45° is **borrowed** from the paper's model-selection rule ("45° acceptance angle, assuming 225° for TmY3", Methods "Model selection").
- Binomial hand values: n=1,k=1,p=0.1 → 0.1; n=2,k=1 → 0.19; n=2,k=2 → 0.01; n=3,k=2 → 0.028; n=10,k=3 → 0.0701908264; n=10,k=4 → 0.0127951984; k=0 → 1.0; k > n → 0.0.
- The spec's signatures returned bare values; here they return `Result` (a recorded deviation, A6), since a missing type or ragged input must be a refusal.

**Files.** Modify: `crates/fidelity/src/indices.rs`; `CLAUDE.md` (the two test-total figures).

- [ ] **Step 1: Write the failing tests.** Append inside `mod tests`:

```rust
    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn the_fri_correlation_to_known_is_pearson_over_the_known_types() {
        let types = names(&["A", "B", "C", "D"]);
        let r = fri_correlation_to_known(&[0.5, -0.5, 0.25, 9.0], &types, &[("A", 1), ("B", -1), ("C", 1)]).unwrap();
        assert!(close(r, 0.970725343394151, 1e-12), "{r}");
        assert!(matches!(fri_correlation_to_known(&[0.5, -0.5, 0.25, 9.0], &types, &[("A", 1), ("C", 1)]), Err(IndexError::NonFinite(_))));
        assert_eq!(fri_correlation_to_known(&[0.5, -0.5, 0.25, 9.0], &types, &[("E", 1), ("A", -1)]), Err(IndexError::MissingCellType("E".into())));
    }

    #[test]
    fn the_dsi_correlation_to_known_maxes_over_intensity_against_a_motion_label() {
        let types = names(&["X", "T4a", "Mi1", "T5a"]);
        let dsi = [vec![9.0, 0.2, 0.9, 0.1], vec![9.0, 0.8, 0.1, 0.7]];
        let r = dsi_correlation_to_known(&dsi, &types, &["T5a", "Mi1", "T4a"], &["T4a", "T5a"]).unwrap();
        assert!(close(r, -0.8660254037844385, 1e-12), "{r}");
        let e = dsi_correlation_to_known(&dsi, &types, &["T4a", "Mi1"], &["T5a"]);
        assert!(matches!(e, Err(IndexError::BadKnown { .. })), "{e:?}");
        assert_eq!(dsi_correlation_to_known(&dsi, &types, &["T4b"], &[]), Err(IndexError::MissingCellType("T4b".into())));
        assert_eq!(dsi_correlation_to_known(&dsi, &types, &[], &[]), Err(IndexError::Empty("known DSI types")));
        let ragged = [vec![9.0, 0.2, 0.9], vec![9.0, 0.8, 0.1, 0.7]];
        assert_eq!(
            dsi_correlation_to_known(&ragged, &types, &["T4a"], &[]),
            Err(IndexError::Length { what: "DSI", expected: 4, found: 3 })
        );
        let flat = [vec![0.5; 4], vec![0.5; 4]];
        let e = dsi_correlation_to_known(&flat, &types, &["T5a", "Mi1", "T4a"], &["T4a", "T5a"]);
        assert!(matches!(e, Err(IndexError::NonFinite(_))), "a constant DSI has no Pearson r: {e:?}");
    }

    #[test]
    fn angular_distances_read_t4_at_on_and_t5_at_off_and_wrap() {
        use std::f64::consts::PI;
        let types = names(&["T4a", "T5b", "T4d"]);
        // T4a at ON: -pi + 0.1 against 180 degrees. T5b at OFF: -0.05 against
        // 0. T4d at ON: 3 pi / 2 - 0.2 against 270. The other intensity's
        // values would be far off.
        let pd = [vec![0.0, -0.05, 0.0], vec![-PI + 0.1, 2.0, 3.0 * PI / 2.0 - 0.2]];
        let d = angular_distance_to_known(&pd, &types, &[("T4a", 180), ("T5b", 0), ("T4d", 270)]).unwrap();
        assert!(close(d[0], 0.1, 1e-12) && close(d[1], 0.05, 1e-12) && close(d[2], 0.2, 1e-12), "{d:?}");
        let e = angular_distance_to_known(&pd, &types, &[("Mi1", 0)]);
        assert!(matches!(e, Err(IndexError::BadKnown { .. })), "{e:?}");
        assert_eq!(angular_distance_to_known(&pd, &types, &[("T4c", 0)]), Err(IndexError::MissingCellType("T4c".into())));
        assert_eq!(angular_distance_to_known(&pd, &types, &[]), Err(IndexError::Empty("known preferred directions")));
        let ragged = [vec![0.0; 3], vec![0.0; 2]];
        assert_eq!(
            angular_distance_to_known(&ragged, &types, &[("T4a", 180)]),
            Err(IndexError::Length { what: "preferred directions", expected: 3, found: 2 })
        );
        let nan = [vec![0.0; 3], vec![f64::NAN, 0.0, 0.0]];
        assert!(matches!(angular_distance_to_known(&nan, &types, &[("T4a", 180)]), Err(IndexError::NonFinite(_))));
    }

    #[test]
    fn polarity_agreement_has_no_dead_zone_and_zero_is_a_miss() {
        let types = names(&["A", "B", "C", "D"]);
        let a = polarity_agreement(&[0.5, -0.2, 0.0, 1e-9], &types, &[("A", 1), ("B", 1), ("C", -1), ("D", 1)]).unwrap();
        assert_eq!(a, Agreement { count: 2, of: 4, misses: vec!["B".into(), "C".into()] });
        assert!(matches!(polarity_agreement(&[0.5; 4], &types, &[("A", 0)]), Err(IndexError::BadKnown { .. })));
        assert_eq!(polarity_agreement(&[0.5; 4], &types, &[]), Err(IndexError::Empty("known polarities")));
        assert_eq!(
            polarity_agreement(&[0.5; 3], &types, &[("A", 1)]),
            Err(IndexError::Length { what: "FRI", expected: 4, found: 3 })
        );
        assert!(matches!(polarity_agreement(&[f64::NAN, 0.0, 0.0, 0.0], &types, &[("A", 1)]), Err(IndexError::NonFinite(_))));
    }

    #[test]
    fn the_median_fri_is_numpys_median_per_type() {
        assert_eq!(median_fri(&[vec![1.0, 4.0], vec![3.0, 2.0], vec![2.0, 8.0]]), Ok(vec![2.0, 4.0]));
        assert_eq!(median_fri(&[vec![1.0], vec![2.0], vec![3.0], vec![10.0]]), Ok(vec![2.5]));
        // the empty battery's analogue: no models is a refusal, never a score
        assert_eq!(median_fri(&[]), Err(IndexError::Empty("per-model values")));
        assert_eq!(median_fri(&[vec![]]), Err(IndexError::Empty("a model's values")));
        assert_eq!(median_fri(&[vec![1.0, 2.0], vec![1.0]]), Err(IndexError::Length { what: "a model's values", expected: 2, found: 1 }));
        assert!(matches!(median_fri(&[vec![1.0], vec![f64::INFINITY]]), Err(IndexError::NonFinite(_))));
    }

    #[test]
    fn binomial_upper_tail_matches_hand_values() {
        assert_eq!(binomial_upper_tail(1, 1, 0.1), 0.1);
        assert_eq!(binomial_upper_tail(7, 0, 0.1), 1.0);
        assert_eq!(binomial_upper_tail(3, 4, 0.1), 0.0);
        for (n, k, want) in [(2, 1, 0.19), (2, 2, 0.01), (3, 2, 0.028), (10, 3, 0.0701908264), (10, 4, 0.0127951984)] {
            let got = binomial_upper_tail(n, k, 0.1);
            assert!(close(got, want, 1e-14), "n {n}, k {k}: {got}");
        }
        assert!(close(binomial_upper_tail(50, 9, 0.1), 0.057867205718094276, 1e-13));
        assert!(close(binomial_upper_tail(50, 10, 0.1), 0.024537935704591455, 1e-13));
    }

    #[test]
    fn dsi_class_is_the_papers_binomial_rule() {
        // type 0: 10 of 50 above d_thresh (selective); type 1: 9 (not);
        // type 2: exactly d_thresh everywhere (not: the test is strict).
        let per_model: Vec<Vec<f64>> = (0..50)
            .map(|m| vec![if m < 10 { 0.5 } else { 0.1 }, if m < 9 { 0.5 } else { 0.1 }, D_THRESH])
            .collect();
        assert_eq!(dsi_class(&per_model, D_THRESH, BINOMIAL_P0, BINOMIAL_ALPHA), Ok(vec![true, false, false]));
        assert!(matches!(dsi_class(&per_model, D_THRESH, 1.5, BINOMIAL_ALPHA), Err(IndexError::BadParameter { what: "p0", .. })));
        assert!(matches!(dsi_class(&per_model, D_THRESH, BINOMIAL_P0, f64::NAN), Err(IndexError::BadParameter { what: "alpha", .. })));
        assert_eq!(dsi_class(&[], D_THRESH, BINOMIAL_P0, BINOMIAL_ALPHA), Err(IndexError::Empty("per-model values")));
    }

    #[test]
    fn pd_within_uses_the_t4_on_t5_off_pairing_and_the_tolerance() {
        use std::f64::consts::PI;
        let types = names(&["T4a", "T5a"]);
        // T4a at ON is 0.1 rad from 180 degrees; T5a at OFF is 180 degrees off.
        let pd = [vec![0.0, 0.0], vec![PI - 0.1, PI]];
        let a = pd_within(&pd, &types, &[("T4a", 180), ("T5a", 180)], PD_TOLERANCE_DEG).unwrap();
        assert_eq!(a, Agreement { count: 1, of: 2, misses: vec!["T5a".into()] });
        assert!(matches!(pd_within(&pd, &types, &[("T4a", 180)], -1.0), Err(IndexError::BadParameter { .. })));
        assert!(matches!(pd_within(&pd, &types, &[("T4a", 180)], f64::NAN), Err(IndexError::BadParameter { .. })));
    }

    #[test]
    fn the_rule_constants_are_the_papers() {
        assert_eq!((D_THRESH, BINOMIAL_P0, BINOMIAL_ALPHA, PD_TOLERANCE_DEG), (0.357, 0.1, 0.05, 45.0));
    }
```

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p semblance-fidelity indices::tests`. Expected: compile errors (`cannot find function polarity_agreement`, …).

- [ ] **Step 3: Write the implementation.** Add above `#[cfg(test)]`:

```rust
/// The paper's DSI threshold: the 99% quantile of a 100x angle-permutation
/// null (Lappalainen et al. 2024, Methods, "DSI").
pub const D_THRESH: f64 = 0.357;
/// The binomial test's null rate (Methods, "Determining whether a cell type
/// with asymmetric inputs counts as direction selective").
pub const BINOMIAL_P0: f64 = 0.1;
/// The binomial test's level (the same section): P < 0.05.
pub const BINOMIAL_ALPHA: f64 = 0.05;
/// The preferred-direction tolerance, in degrees. Borrowed: the paper uses a
/// 45° acceptance angle in model selection (Methods, "Model selection"), not
/// as a rule for preferred direction; this epic applies it there and the
/// record says so.
pub const PD_TOLERANCE_DEG: f64 = 45.0;

/// How many of a known set agree, out of how many, and which did not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agreement {
    pub count: usize,
    pub of: usize,
    pub misses: Vec<String>,
}

/// flyvis's `fri_correlation_to_known`: the Pearson r of the FRI against
/// the known polarity (+1 or -1) over the known types, in `known`'s order.
pub fn fri_correlation_to_known(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<f64, IndexError> {
    let (x, y) = known_polarities(fri, types, known)?;
    pearson_defined(&x, &y, "the FRI's correlation to the known polarities")
}

/// flyvis's `dsi_correlation_to_known`: the Pearson r, over the known DSI
/// types in `types`' order, of the DSI maxed over intensity against 1.0 for
/// a motion-tuned type and 0.0 otherwise.
pub fn dsi_correlation_to_known(
    dsi: &[Vec<f64>; 2],
    types: &[String],
    known_dsi_types: &[&str],
    motion_tuning: &[&str],
) -> Result<f64, IndexError> {
    for d in dsi {
        if d.len() != types.len() {
            return Err(IndexError::Length { what: "DSI", expected: types.len(), found: d.len() });
        }
    }
    if known_dsi_types.is_empty() {
        return Err(IndexError::Empty("known DSI types"));
    }
    for name in known_dsi_types {
        column(types, name)?;
    }
    for name in motion_tuning {
        if !known_dsi_types.contains(name) {
            let reason = "a motion-tuned type is not among the known DSI types".to_string();
            return Err(IndexError::BadKnown { cell_type: name.to_string(), reason });
        }
    }
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for (t, name) in types.iter().enumerate() {
        if !known_dsi_types.contains(&name.as_str()) {
            continue;
        }
        let v = dsi[0][t].max(dsi[1][t]);
        if !v.is_finite() {
            return Err(IndexError::NonFinite(format!("the DSI of {name}")));
        }
        x.push(v);
        y.push(if motion_tuning.contains(&name.as_str()) { 1.0 } else { 0.0 });
    }
    pearson_defined(&x, &y, "the DSI's correlation to the known motion tuning")
}

/// flyvis's `angular_distance_to_known`: per known type, in `known`'s order,
/// the angular distance in radians from its preferred direction (T4 types
/// at intensity 1, T5 types at intensity 0) to the known direction.
pub fn angular_distance_to_known(pd: &[Vec<f64>; 2], types: &[String], known: &[(&str, u16)]) -> Result<Vec<f64>, IndexError> {
    for d in pd {
        if d.len() != types.len() {
            return Err(IndexError::Length { what: "preferred directions", expected: types.len(), found: d.len() });
        }
    }
    if known.is_empty() {
        return Err(IndexError::Empty("known preferred directions"));
    }
    known
        .iter()
        .map(|&(name, deg)| {
            let v = pd[motion_intensity(name)?][column(types, name)?];
            if !v.is_finite() {
                return Err(IndexError::NonFinite(format!("the preferred direction of {name}")));
            }
            Ok(simple_angle_distance(v, known_radians(deg)))
        })
        .collect()
}

/// The paper's polarity rule: sign(FRI) against the known polarity over the
/// known types, with no dead zone. An FRI of exactly 0 has no sign: a miss.
pub fn polarity_agreement(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<Agreement, IndexError> {
    let (x, y) = known_polarities(fri, types, known)?;
    let misses: Vec<String> = known
        .iter()
        .zip(x.iter().zip(&y))
        .filter(|&(_, (&v, &pol))| !((v > 0.0 && pol > 0.0) || (v < 0.0 && pol < 0.0)))
        .map(|(&(name, _), _)| name.to_string())
        .collect();
    Ok(Agreement { count: known.len() - misses.len(), of: known.len(), misses })
}

/// The per-type median over models, as `np.median`: an even count averages
/// the two middle values. Callers use all 50 models and the top 10
/// (000 to 009, the released order).
pub fn median_fri(per_model: &[Vec<f64>]) -> Result<Vec<f64>, IndexError> {
    let n_types = per_model_shape(per_model)?;
    let n = per_model.len();
    let mut out = Vec::with_capacity(n_types);
    for t in 0..n_types {
        let mut col: Vec<f64> = per_model.iter().map(|v| v[t]).collect();
        col.sort_by(f64::total_cmp);
        out.push(if n % 2 == 1 { col[n / 2] } else { (col[n / 2 - 1] + col[n / 2]) / 2.0 });
    }
    Ok(out)
}

/// P(X >= k) for X ~ Binomial(n, p0), summed in f64 from i = k up, each
/// term `C(n, i) p0^i (1 - p0)^(n - i)` with C(n, i) an exact integer
/// rounded once.
pub fn binomial_upper_tail(n: u32, k: u32, p0: f64) -> f64 {
    if k == 0 {
        return 1.0;
    }
    if k > n {
        return 0.0;
    }
    let q = 1.0 - p0;
    let mut sum = 0.0f64;
    for i in k..=n {
        sum += choose(n, i) * p0.powi(i as i32) * q.powi((n - i) as i32);
    }
    sum
}

/// The paper's direction-selectivity rule, per type: k = how many models'
/// DSI exceed `d_thresh` (strictly); selective iff
/// `binomial_upper_tail(n_models, k, p0) < alpha` (strictly).
/// `per_model[m][t]` is model m's DSI for type t, already reduced over
/// intensity by the caller (the gates pass the max over intensity, as
/// `dsi_correlation_to_known` does).
pub fn dsi_class(per_model: &[Vec<f64>], d_thresh: f64, p0: f64, alpha: f64) -> Result<Vec<bool>, IndexError> {
    if !d_thresh.is_finite() {
        return Err(IndexError::BadParameter { what: "d_thresh", value: d_thresh });
    }
    if !(0.0..=1.0).contains(&p0) {
        return Err(IndexError::BadParameter { what: "p0", value: p0 });
    }
    if alpha.is_nan() || alpha <= 0.0 || alpha > 1.0 {
        return Err(IndexError::BadParameter { what: "alpha", value: alpha });
    }
    let n_types = per_model_shape(per_model)?;
    let n = per_model.len() as u32;
    Ok((0..n_types)
        .map(|t| {
            let k = per_model.iter().filter(|m| m[t] > d_thresh).count() as u32;
            binomial_upper_tail(n, k, p0) < alpha
        })
        .collect())
}

/// The preferred-direction rule: a known type agrees when its angular
/// distance (`angular_distance_to_known`'s pairing) is at most
/// `tolerance_deg` degrees.
pub fn pd_within(pd: &[Vec<f64>; 2], types: &[String], known_deg: &[(&str, u16)], tolerance_deg: f64) -> Result<Agreement, IndexError> {
    if !(tolerance_deg.is_finite() && tolerance_deg >= 0.0) {
        return Err(IndexError::BadParameter { what: "tolerance in degrees", value: tolerance_deg });
    }
    let d = angular_distance_to_known(pd, types, known_deg)?;
    let misses: Vec<String> = known_deg
        .iter()
        .zip(&d)
        .filter(|&(_, &dist)| dist * (180.0 / std::f64::consts::PI) > tolerance_deg)
        .map(|(&(name, _), _)| name.to_string())
        .collect();
    Ok(Agreement { count: d.len() - misses.len(), of: d.len(), misses })
}

/// Per known type, its FRI and its polarity as f64, after the checks.
fn known_polarities(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<(Vec<f64>, Vec<f64>), IndexError> {
    if fri.len() != types.len() {
        return Err(IndexError::Length { what: "FRI", expected: types.len(), found: fri.len() });
    }
    if known.is_empty() {
        return Err(IndexError::Empty("known polarities"));
    }
    let (mut x, mut y) = (Vec::with_capacity(known.len()), Vec::with_capacity(known.len()));
    for &(name, pol) in known {
        if pol != 1 && pol != -1 {
            return Err(IndexError::BadKnown { cell_type: name.to_string(), reason: format!("polarity {pol} is not +1 or -1") });
        }
        let v = fri[column(types, name)?];
        if !v.is_finite() {
            return Err(IndexError::NonFinite(format!("the FRI of {name}")));
        }
        x.push(v);
        y.push(pol as f64);
    }
    Ok((x, y))
}

/// `pearson`, refusing the NaN of a constant input.
fn pearson_defined(a: &[f64], b: &[f64], what: &str) -> Result<f64, IndexError> {
    let r = pearson(a, b);
    if r.is_finite() { Ok(r) } else { Err(IndexError::NonFinite(format!("{what} (a constant input has no Pearson r)"))) }
}

/// The number of types per model, after refusing no models, an empty
/// model, ragged models and non-finite values.
fn per_model_shape(per_model: &[Vec<f64>]) -> Result<usize, IndexError> {
    let first = per_model.first().ok_or(IndexError::Empty("per-model values"))?;
    if first.is_empty() {
        return Err(IndexError::Empty("a model's values"));
    }
    for (m, v) in per_model.iter().enumerate() {
        if v.len() != first.len() {
            return Err(IndexError::Length { what: "a model's values", expected: first.len(), found: v.len() });
        }
        if let Some(t) = v.iter().position(|x| !x.is_finite()) {
            return Err(IndexError::NonFinite(format!("model {m}'s value for type {t}")));
        }
    }
    Ok(first.len())
}

/// T4 types are read at intensity 1 (ON), T5 types at intensity 0 (OFF).
fn motion_intensity(name: &str) -> Result<usize, IndexError> {
    if name.starts_with("T4") {
        Ok(1)
    } else if name.starts_with("T5") {
        Ok(0)
    } else {
        Err(IndexError::BadKnown { cell_type: name.to_string(), reason: "only T4 and T5 types have a known preferred direction".into() })
    }
}

/// flyvis's known directions in radians: 0, pi/2, pi, 3 pi/2.
fn known_radians(deg: u16) -> f64 {
    use std::f64::consts::PI;
    match deg {
        0 => 0.0,
        90 => PI / 2.0,
        180 => PI,
        270 => 3.0 * PI / 2.0,
        d => d as f64 * DEG,
    }
}

/// flyvis's `simple_angle_distance` with `upper = pi`.
fn simple_angle_distance(a: f64, b: f64) -> f64 {
    use std::f64::consts::PI;
    let tau = 2.0 * PI;
    let (a, b) = (py_mod(a, tau), py_mod(b, tau));
    let mut y = if a >= b { a - b } else { b - a };
    if y > PI {
        y = tau - y;
    }
    y / PI * PI
}

/// numpy's `%` on floats: C's fmod, moved into the divisor's sign; a zero
/// takes the divisor's sign.
fn py_mod(a: f64, m: f64) -> f64 {
    let r = a % m;
    if r == 0.0 {
        0.0f64.copysign(m)
    } else if (r < 0.0) != (m < 0.0) {
        r + m
    } else {
        r
    }
}

/// C(n, i), exact as a u128 while it fits (always for n <= 128), rounded
/// once to f64; past that, a running f64 product.
fn choose(n: u32, i: u32) -> f64 {
    let i = i.min(n - i);
    let mut c: u128 = 1;
    for j in 0..i {
        match c.checked_mul((n - j) as u128) {
            Some(v) => c = v / (j as u128 + 1),
            None => {
                let mut f = 1.0f64;
                for j in 0..i {
                    f = f * (n - j) as f64 / (j + 1) as f64;
                }
                return f;
            }
        }
    }
    c as f64
}
```

- [ ] **Step 4: Run them to see them pass.** Run: `cargo test -p semblance-fidelity indices::`. Expected: all pass. `cargo clippy -p semblance-fidelity --all-targets -- -D warnings`: clean.

- [ ] **Step 5: Re-measure the test total (count owner).** Run the whole suite as Global Constraints' "Measuring the total" says and write the measured number into CLAUDE.md's two test-total figures. Change nothing else in CLAUDE.md.

- [ ] **Step 6: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed`.

- [ ] **Step 7: Commit.**

```bash
git add crates/fidelity/src/indices.rs CLAUDE.md
git commit -m "Add flyvis's to-known helpers and the paper's agreement rules"
```

---

### Task sim-cka: debiased linear CKA over response matrices

Owns: crates/fidelity/src/similarity.rs, crates/fidelity/src/lib.rs, crates/fidelity/Cargo.toml, Cargo.lock, app/tests/acceptance_release.rs
Uses: none
Provides: similarity-cka
Wave: 3

**What this builds.** A new module `semblance_fidelity::similarity` (file `crates/fidelity/src/similarity.rs`, created here): `ResponseMatrix`, `SimilarityError`, `hsic_unbiased` and `cka`, the representational axis gates C and D will gate on (owner's answer Q3, revised: debiased linear CKA over each cell type's central full time course). `semblance-fidelity` gains `semblance-rng` as a **dev-dependency** only, for random test data. This task adds at most 15 `#[test]`s (it adds 9) and does not touch CLAUDE.md.

**Facts this task needs (spec RA §1, task 0).**
- `semblance_sim::response::Responses` (task 1a): `cell_types() -> &[String]`, `precision() -> Precision` (`F32` or `F64`), `len()`, `stimulus(s) -> Option<&StimulusResponse>`; `StimulusResponse::{n_frames(), frame(i) -> Option<&[f64]>}`; built by `Responses::new(types, dt, precision)?`, `push(label, n_frames, values)?`, `finish()?`. Values are stored widened to f64, exact for an f32 run.
- The response matrix: rows are (stimulus, frame) for every frame from each stimulus's onset to its end, in battery order; columns the cell types. The grey pre-stimulus frames are excluded (the owner's choice: they repeat the settled state); the real batteries' stimulus phase is frames 200 onward, so they pass onset 200. Stored as f32 when the precision is F32 (lossless) and f64 otherwise. The real flash and edge batteries give 65,416 rows at valid frames; 50 such matrices at f32 are about 0.85 GB (gate D holds 50 at once). No test asserts a size.
- `hsic_unbiased` is Song et al. (2012)'s unbiased estimator with linear kernels, **computed in feature space** (the n × n Gram matrix is never formed), from `XᵀY` (p × p), the row norms and the column sums, in O(n·p²), accumulated in f64. With K̃ = XXᵀ with its diagonal zeroed and L̃ the same for Y:
  ```text
  HSIC1 = [ tr(K~L~) + (1'K~1)(1'L~1)/((n-1)(n-2)) - (2/(n-2)) 1'K~L~1 ] / (n(n-3))
  tr(K~L~) = ||X'Y||_F^2 - sum_i ||x_i||^2 ||y_i||^2
  1'K~1    = ||sum_i x_i||^2 - sum_i ||x_i||^2
  1'K~L~1  = sum_i (x_i . s_x - ||x_i||^2)(y_i . s_y - ||y_i||^2),   s_x = sum_i x_i
  ```
  `cka = H(X,Y) / sqrt(H(X,X) · H(Y,Y))`.
- **Exact identity, by construction:** `H(X,X)` is the same function called with the same argument twice, so `cka(x, x)` is `H / sqrt(H · H)`, which in binary floating point is exactly 1 (barring overflow or underflow). No special case for identical inputs.
- **Symmetry:** `hsic_unbiased(x, y)` must sum in an order symmetric in its arguments, so `cka(a, b) == cka(b, a)` bit for bit. `(YᵀX)` is bitwise `(XᵀY)ᵀ`, but a row-major Frobenius sum is not transpose-invariant: sum the diagonal, then each `a < b` pair as `m[a][b]² + m[b][a]²`. Every other term is a commutative product.
- Errors: a row-count or column-count mismatch, fewer than 4 rows (the estimator divides by n − 3), a non-positive `H(X,X)` or `H(Y,Y)`, non-finite values.
- Proof: `hsic_unbiased` against a direct Gram-matrix implementation (test-only, n ≤ 50) to 1e-12 relative; `cka(x, x) == 1.0` and `cka(a, b) == cka(b, a)` bitwise on random matrices; CKA unchanged (to 1e-12) under an orthogonal transform and an isotropic scaling of one argument, and changed by a per-column scaling. Orthogonal matrices are built in-test from Givens rotations; random data comes from `semblance-rng` (`Rng::seed_from_u64(seed)`, `next_f32()` uniform in [0, 1)).

**Files.**
- Create: `crates/fidelity/src/similarity.rs`
- Modify: `crates/fidelity/src/lib.rs` (declare the module, one doc bullet)
- Modify: `crates/fidelity/Cargo.toml` (add `[dev-dependencies]` with `semblance-rng.workspace = true`; the `[dependencies]` stay as they are)
- Modify: `Cargo.lock` (the `semblance-fidelity` package's dependencies become `"semblance-codec", "semblance-connectome", "semblance-rng", "semblance-sim"`, in that order)

- [ ] **Step 1: Add the dev-dependency and declare the module.** Append to `crates/fidelity/Cargo.toml`:

```toml

[dev-dependencies]
semblance-rng.workspace = true
```

In `Cargo.lock`:

```toml
[[package]]
name = "semblance-fidelity"
version = "0.3.1"
dependencies = [
 "semblance-codec",
 "semblance-connectome",
 "semblance-rng",
 "semblance-sim",
]
```

In `crates/fidelity/src/lib.rs`, declare `pub mod similarity;` after `pub mod indices;`, and add a bullet to the module doc's "What is DONE here" list after the `indices` bullet:

```rust
//! - [`similarity`]: debiased linear CKA over each cell type's central
//!   response time course (the representational axis), and Zhou & Hasler's
//!   peak-voltage RDM variant, reported beside it
```

- [ ] **Step 2: Write the failing tests.** Create `similarity.rs` with this tests block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use semblance_rng::Rng;
    use semblance_sim::response::Responses;

    fn names(p: usize) -> Vec<String> {
        (0..p).map(|t| format!("t{t}")).collect()
    }

    /// `n` rows of `p` values uniform in [-1, 1), from `seed`.
    fn random_rows(seed: u64, n: usize, p: usize) -> Vec<Vec<f64>> {
        let mut rng = Rng::seed_from_u64(seed);
        let mut rows = Vec::with_capacity(n);
        for _ in 0..n {
            let mut row = Vec::with_capacity(p);
            for _ in 0..p {
                row.push(rng.next_f32() as f64 * 2.0 - 1.0);
            }
            rows.push(row);
        }
        rows
    }

    /// `x` plus `scale` times independent noise: something in common with `x`.
    fn related(x: &[Vec<f64>], seed: u64, scale: f64) -> Vec<Vec<f64>> {
        let noise = random_rows(seed, x.len(), x[0].len());
        x.iter().zip(&noise).map(|(a, b)| a.iter().zip(b).map(|(u, v)| u + scale * v).collect()).collect()
    }

    fn matrix(rows: &[Vec<f64>]) -> ResponseMatrix {
        ResponseMatrix::from_rows(names(rows[0].len()), Precision::F64, rows).unwrap()
    }

    /// The estimator straight from its definition, with the n x n Gram
    /// matrices: test-only, for small n.
    fn hsic_gram(x: &[Vec<f64>], y: &[Vec<f64>]) -> f64 {
        let n = x.len();
        let gram = |m: &[Vec<f64>]| -> Vec<Vec<f64>> {
            (0..n)
                .map(|i| (0..n).map(|j| if i == j { 0.0 } else { m[i].iter().zip(&m[j]).map(|(a, b)| a * b).sum() }).collect())
                .collect()
        };
        let (k, l) = (gram(x), gram(y));
        let nf = n as f64;
        let tr: f64 = (0..n).map(|i| (0..n).map(|j| k[i][j] * l[j][i]).sum::<f64>()).sum();
        let (sk, sl): (f64, f64) = (k.iter().flatten().sum(), l.iter().flatten().sum());
        let kl: f64 = (0..n).map(|i| k[i].iter().sum::<f64>() * l[i].iter().sum::<f64>()).sum();
        (tr + sk * sl / ((nf - 1.0) * (nf - 2.0)) - 2.0 / (nf - 2.0) * kl) / (nf * (nf - 3.0))
    }

    /// `rows` times a product of Givens rotations of every adjacent column pair.
    fn rotate(rows: &[Vec<f64>], seed: u64) -> Vec<Vec<f64>> {
        let p = rows[0].len();
        let mut rng = Rng::seed_from_u64(seed);
        let mut out = rows.to_vec();
        for a in 0..p - 1 {
            let theta = rng.next_f32() as f64 * std::f64::consts::TAU;
            let (c, s) = (theta.cos(), theta.sin());
            for row in out.iter_mut() {
                let (u, v) = (row[a], row[a + 1]);
                row[a] = c * u - s * v;
                row[a + 1] = s * u + c * v;
            }
        }
        out
    }

    #[test]
    fn hsic_matches_the_gram_matrix_definition() {
        for (k, n) in [4usize, 7, 20, 50].into_iter().enumerate() {
            let x = random_rows(10 + k as u64, n, 6);
            let y = related(&x, 20 + k as u64, 0.3);
            for (a, b) in [(&x, &y), (&x, &x), (&y, &y)] {
                let (got, want) = (hsic_unbiased(&matrix(a), &matrix(b)).unwrap(), hsic_gram(a, b));
                assert!((got - want).abs() <= 1e-12 * want.abs(), "n {n}: {got} vs {want}");
            }
        }
    }

    #[test]
    fn cka_is_exactly_1_on_itself_and_exactly_symmetric() {
        for seed in 0..10u64 {
            let x = random_rows(seed, 30, 8);
            let y = related(&x, seed + 100, 0.7);
            let (mx, my) = (matrix(&x), matrix(&y));
            assert_eq!(cka(&mx, &mx), Ok(1.0));
            assert_eq!(cka(&mx, &my).unwrap().to_bits(), cka(&my, &mx).unwrap().to_bits());
        }
    }

    #[test]
    fn cka_ignores_rotation_and_isotropic_scaling_but_not_column_scaling() {
        let x = random_rows(3, 40, 6);
        let y = related(&x, 4, 0.5);
        let base = cka(&matrix(&x), &matrix(&y)).unwrap();
        let rotated = cka(&matrix(&x), &matrix(&rotate(&y, 5))).unwrap();
        let scaled: Vec<Vec<f64>> = y.iter().map(|r| r.iter().map(|v| v * 2.5).collect()).collect();
        let column: Vec<Vec<f64>> = y.iter().map(|r| r.iter().enumerate().map(|(j, v)| if j == 0 { v * 10.0 } else { *v }).collect()).collect();
        assert!((rotated - base).abs() <= 1e-12, "{rotated} vs {base}");
        assert!((cka(&matrix(&x), &matrix(&scaled)).unwrap() - base).abs() <= 1e-12);
        assert!((cka(&matrix(&x), &matrix(&column)).unwrap() - base).abs() > 1e-3);
    }

    #[test]
    fn a_response_matrix_is_every_frame_from_each_onset_in_battery_order() {
        let mut r = Responses::new(names(2), 0.005, Precision::F32).unwrap();
        r.push("a".into(), 3, vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5]).unwrap();
        r.push("b".into(), 4, vec![3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5]).unwrap();
        let r = r.finish().unwrap();
        let m = ResponseMatrix::from_responses(&r, &[1, 2]).unwrap();
        assert_eq!((m.n_rows(), m.n_cols(), m.precision()), (4, 2, Precision::F32));
        assert_eq!(m.cell_types(), r.cell_types());
        let rows: Vec<Vec<f64>> = (0..4).map(|i| m.row(i).unwrap()).collect();
        assert_eq!(rows, vec![vec![1.0, 1.5], vec![2.0, 2.5], vec![5.0, 5.5], vec![6.0, 6.5]]);
        assert_eq!(m.row(4), None);
        assert_eq!(ResponseMatrix::from_responses(&r, &[1]), Err(SimilarityError::OnsetCount { onsets: 1, stimuli: 2 }));
        assert_eq!(
            ResponseMatrix::from_responses(&r, &[1, 4]),
            Err(SimilarityError::OnsetPastEnd { stimulus: 1, onset: 4, n_frames: 4 })
        );
    }

    #[test]
    fn rows_tagged_f32_must_be_f32s() {
        assert_eq!(
            ResponseMatrix::from_rows(names(2), Precision::F32, &[vec![0.5, 0.1]]),
            Err(SimilarityError::NotF32 { row: 0, column: 1 })
        );
        let m = ResponseMatrix::from_rows(names(2), Precision::F32, &[vec![0.5, 0.25]]).unwrap();
        assert_eq!(m.row(0), Some(vec![0.5, 0.25]));
        // Responses does not check an f32 run's values; from_responses refuses them
        let mut r = Responses::new(names(2), 0.005, Precision::F32).unwrap();
        r.push("a".into(), 2, vec![0.5, 0.25, 0.5, 0.1]).unwrap();
        let r = r.finish().unwrap();
        assert_eq!(ResponseMatrix::from_responses(&r, &[0]), Err(SimilarityError::NotF32 { row: 1, column: 1 }));
    }

    #[test]
    fn matrices_built_by_hand_are_checked() {
        assert_eq!(ResponseMatrix::from_rows(vec![], Precision::F64, &[]), Err(SimilarityError::TooFew { what: "columns", need: 1, found: 0 }));
        assert_eq!(ResponseMatrix::from_rows(names(2), Precision::F64, &[]), Err(SimilarityError::TooFew { what: "rows", need: 1, found: 0 }));
        assert_eq!(
            ResponseMatrix::from_rows(names(2), Precision::F64, &[vec![1.0]]),
            Err(SimilarityError::Length { what: "row", expected: 2, found: 1 })
        );
        assert!(matches!(ResponseMatrix::from_rows(names(1), Precision::F64, &[vec![f64::NAN]]), Err(SimilarityError::NonFinite(_))));
    }

    #[test]
    fn cka_refuses_what_it_cannot_score() {
        let x = matrix(&random_rows(1, 6, 3));
        assert_eq!(cka(&x, &matrix(&random_rows(2, 5, 3))), Err(SimilarityError::RowMismatch { x: 6, y: 5 }));
        assert_eq!(cka(&x, &matrix(&random_rows(2, 6, 4))), Err(SimilarityError::ColumnMismatch { x: 3, y: 4 }));
        let three = matrix(&random_rows(1, 3, 3));
        assert_eq!(cka(&three, &three), Err(SimilarityError::TooFew { what: "rows", need: 4, found: 3 }));
        // a constant matrix has HSIC(X, X) = 0: no scale to divide by
        let flat = matrix(&vec![vec![1.0, 2.0, 3.0]; 6]);
        assert!(matches!(cka(&flat, &x), Err(SimilarityError::NonPositiveHsic { which: "x", .. })), "{:?}", cka(&flat, &x));
        assert!(matches!(cka(&x, &flat), Err(SimilarityError::NonPositiveHsic { which: "y", .. })));
    }

    #[test]
    fn errors_display_as_registered() {
        let cases = [
            (SimilarityError::RowMismatch { x: 6, y: 5 }, "the matrices have 6 and 5 rows"),
            (SimilarityError::ColumnMismatch { x: 65, y: 64 }, "the matrices have 65 and 64 columns"),
            (SimilarityError::TooFew { what: "rows", need: 4, found: 3 }, "rows: need at least 4, found 3"),
            (SimilarityError::NonPositiveHsic { which: "x", value: 0.0 }, "HSIC(x, x) is 0, not positive"),
            (SimilarityError::NonFinite("row 2, column 3".into()), "row 2, column 3 is not finite"),
            (SimilarityError::OnsetCount { onsets: 1, stimuli: 2 }, "1 onsets for 2 stimuli"),
            (SimilarityError::OnsetPastEnd { stimulus: 1, onset: 4, n_frames: 4 }, "stimulus 1: onset 4 is past its 4 frames"),
            (SimilarityError::NotF32 { row: 0, column: 1 }, "row 0, column 1: an f32 run's value is not an f32"),
            (SimilarityError::Length { what: "row", expected: 2, found: 1 }, "row: expected 2, found 1"),
            (SimilarityError::NotSymmetric { i: 0, j: 2 }, "the RDM differs at (0, 2) and (2, 0)"),
            (SimilarityError::Degenerate("an RDM's upper triangle is constant".into()), "an RDM's upper triangle is constant"),
        ];
        for (e, want) in cases {
            assert_eq!(e.to_string(), want);
        }
    }
}
```

- [ ] **Step 3: Run them to see them fail.** Run: `cargo test -p semblance-fidelity similarity::`. Expected: compile errors (`cannot find type ResponseMatrix`, …).

- [ ] **Step 4: Write the implementation.** Put this above the tests block in `similarity.rs`:

```rust
//! The representational axis: how alike two networks represent a battery,
//! as debiased linear CKA over each cell type's central response time
//! course, plus Zhou & Hasler's own variant, computed exactly and reported.
//!
//! Pure, over task 1a's [`Responses`]. Every malformed input is a
//! [`SimilarityError`], never a score.
//!
//! Spec: docs/superpowers/specs/2026-11-04-representational-axis-design.md,
//! section 1.

use std::fmt;

use semblance_sim::response::{Precision, Responses};

/// Why a similarity could not be computed. Each is a refusal, never a score.
#[derive(Clone, Debug, PartialEq)]
pub enum SimilarityError {
    RowMismatch { x: usize, y: usize },
    ColumnMismatch { x: usize, y: usize },
    /// Too few rows, columns, conditions or entries (the unbiased HSIC
    /// divides by n - 3, so it needs 4 rows).
    TooFew { what: &'static str, need: usize, found: usize },
    /// `HSIC(X, X)` or `HSIC(Y, Y)` is not positive: no scale to divide by.
    NonPositiveHsic { which: &'static str, value: f64 },
    NonFinite(String),
    OnsetCount { onsets: usize, stimuli: usize },
    OnsetPastEnd { stimulus: usize, onset: usize, n_frames: usize },
    /// A value tagged as from an f32 run that is not an f32.
    NotF32 { row: usize, column: usize },
    Length { what: &'static str, expected: usize, found: usize },
    NotSymmetric { i: usize, j: usize },
    /// An input with no variation where the measure needs some.
    Degenerate(String),
}

impl fmt::Display for SimilarityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimilarityError::RowMismatch { x, y } => write!(f, "the matrices have {x} and {y} rows"),
            SimilarityError::ColumnMismatch { x, y } => write!(f, "the matrices have {x} and {y} columns"),
            SimilarityError::TooFew { what, need, found } => write!(f, "{what}: need at least {need}, found {found}"),
            SimilarityError::NonPositiveHsic { which, value } => write!(f, "HSIC({which}, {which}) is {value}, not positive"),
            SimilarityError::NonFinite(what) => write!(f, "{what} is not finite"),
            SimilarityError::OnsetCount { onsets, stimuli } => write!(f, "{onsets} onsets for {stimuli} stimuli"),
            SimilarityError::OnsetPastEnd { stimulus, onset, n_frames } => {
                write!(f, "stimulus {stimulus}: onset {onset} is past its {n_frames} frames")
            }
            SimilarityError::NotF32 { row, column } => write!(f, "row {row}, column {column}: an f32 run's value is not an f32"),
            SimilarityError::Length { what, expected, found } => write!(f, "{what}: expected {expected}, found {found}"),
            SimilarityError::NotSymmetric { i, j } => write!(f, "the RDM differs at ({i}, {j}) and ({j}, {i})"),
            SimilarityError::Degenerate(what) => f.write_str(what),
        }
    }
}

impl std::error::Error for SimilarityError {}

/// One network's responses as a matrix: a row per (stimulus, frame) from
/// each stimulus's onset to its end, in battery order; a column per cell
/// type. Held as f32 for an f32 run (every value is an f32, so nothing is
/// lost) and f64 otherwise: gate D holds 50 at once. The real flash and
/// edge batteries at onset 200 give 65,416 rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseMatrix {
    cell_types: Vec<String>,
    n_rows: usize,
    data: Store,
}

#[derive(Clone, Debug, PartialEq)]
enum Store {
    F32(Vec<f32>),
    F64(Vec<f64>),
}

impl ResponseMatrix {
    /// Every frame from `onset[s]` to the end of stimulus `s`. Refuses an
    /// onset count that is not one per stimulus, and an onset at or past
    /// its stimulus's last frame, so every stimulus gives at least one row.
    pub fn from_responses(r: &Responses, onset: &[usize]) -> Result<Self, SimilarityError> {
        if onset.len() != r.len() {
            return Err(SimilarityError::OnsetCount { onsets: onset.len(), stimuli: r.len() });
        }
        let mut values = Vec::new();
        for (s, &on) in onset.iter().enumerate() {
            let st = r.stimulus(s).ok_or(SimilarityError::OnsetCount { onsets: onset.len(), stimuli: r.len() })?;
            if on >= st.n_frames() {
                return Err(SimilarityError::OnsetPastEnd { stimulus: s, onset: on, n_frames: st.n_frames() });
            }
            for f in on..st.n_frames() {
                if let Some(row) = st.frame(f) {
                    values.extend_from_slice(row);
                }
            }
        }
        Self::build(r.cell_types().to_vec(), r.precision(), values)
    }

    /// A matrix from rows of one value per cell type, checked as
    /// `from_responses`' are.
    pub fn from_rows(cell_types: Vec<String>, precision: Precision, rows: &[Vec<f64>]) -> Result<Self, SimilarityError> {
        let p = cell_types.len();
        if p == 0 {
            return Err(SimilarityError::TooFew { what: "columns", need: 1, found: 0 });
        }
        for row in rows {
            if row.len() != p {
                return Err(SimilarityError::Length { what: "row", expected: p, found: row.len() });
            }
        }
        Self::build(cell_types, precision, rows.concat())
    }

    fn build(cell_types: Vec<String>, precision: Precision, values: Vec<f64>) -> Result<Self, SimilarityError> {
        let p = cell_types.len();
        if p == 0 {
            return Err(SimilarityError::TooFew { what: "columns", need: 1, found: 0 });
        }
        if values.is_empty() {
            return Err(SimilarityError::TooFew { what: "rows", need: 1, found: 0 });
        }
        if let Some(i) = values.iter().position(|v| !v.is_finite()) {
            return Err(SimilarityError::NonFinite(format!("row {}, column {}", i / p, i % p)));
        }
        let n_rows = values.len() / p;
        let data = match precision {
            Precision::F32 => {
                let mut out = Vec::with_capacity(values.len());
                for (i, &v) in values.iter().enumerate() {
                    let w = v as f32;
                    if w as f64 != v {
                        return Err(SimilarityError::NotF32 { row: i / p, column: i % p });
                    }
                    out.push(w);
                }
                Store::F32(out)
            }
            Precision::F64 => Store::F64(values),
        };
        Ok(ResponseMatrix { cell_types, n_rows, data })
    }

    pub fn n_rows(&self) -> usize {
        self.n_rows
    }

    pub fn n_cols(&self) -> usize {
        self.cell_types.len()
    }

    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    pub fn precision(&self) -> Precision {
        match self.data {
            Store::F32(_) => Precision::F32,
            Store::F64(_) => Precision::F64,
        }
    }

    /// Row `i`, widened to f64.
    pub fn row(&self, i: usize) -> Option<Vec<f64>> {
        if i >= self.n_rows {
            return None;
        }
        let mut out = vec![0.0; self.n_cols()];
        self.row_into(i, &mut out);
        Some(out)
    }

    fn row_into(&self, i: usize, out: &mut [f64]) {
        let p = self.n_cols();
        match &self.data {
            Store::F32(v) => {
                for (o, &x) in out.iter_mut().zip(&v[i * p..(i + 1) * p]) {
                    *o = x as f64;
                }
            }
            Store::F64(v) => out.copy_from_slice(&v[i * p..(i + 1) * p]),
        }
    }
}

/// Song et al. (2012)'s unbiased HSIC with linear kernels, in feature
/// space: from `X'Y`, the row norms and the column sums, never the n x n
/// Gram matrix, accumulated in f64. Every sum runs in an order symmetric in
/// `x` and `y`, so `hsic_unbiased(x, y)` and `hsic_unbiased(y, x)` are the
/// same bits.
pub fn hsic_unbiased(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError> {
    if x.n_rows != y.n_rows {
        return Err(SimilarityError::RowMismatch { x: x.n_rows, y: y.n_rows });
    }
    if x.n_cols() != y.n_cols() {
        return Err(SimilarityError::ColumnMismatch { x: x.n_cols(), y: y.n_cols() });
    }
    let (n, p) = (x.n_rows, x.n_cols());
    if n < 4 {
        return Err(SimilarityError::TooFew { what: "rows", need: 4, found: n });
    }
    let mut m = vec![0.0f64; p * p];
    let (mut sx, mut sy) = (vec![0.0f64; p], vec![0.0f64; p]);
    let (mut rx, mut ry) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut sum_rx, mut sum_ry, mut sum_rxry) = (0.0f64, 0.0f64, 0.0f64);
    let (mut xi, mut yi) = (vec![0.0f64; p], vec![0.0f64; p]);
    for i in 0..n {
        x.row_into(i, &mut xi);
        y.row_into(i, &mut yi);
        for (row, &xa) in m.chunks_exact_mut(p).zip(&xi) {
            for (cell, &yb) in row.iter_mut().zip(&yi) {
                *cell += xa * yb;
            }
        }
        for ((a, b), (&u, &v)) in sx.iter_mut().zip(sy.iter_mut()).zip(xi.iter().zip(&yi)) {
            *a += u;
            *b += v;
        }
        let (nx, ny) = (dot(&xi, &xi), dot(&yi, &yi));
        rx.push(nx);
        ry.push(ny);
        sum_rx += nx;
        sum_ry += ny;
        sum_rxry += nx * ny;
    }
    let mut kl = 0.0f64;
    for i in 0..n {
        x.row_into(i, &mut xi);
        y.row_into(i, &mut yi);
        kl += (dot(&xi, &sx) - rx[i]) * (dot(&yi, &sy) - ry[i]);
    }
    let tr = frobenius_sq_symmetric(&m, p) - sum_rxry;
    let k1 = dot(&sx, &sx) - sum_rx;
    let l1 = dot(&sy, &sy) - sum_ry;
    let nf = n as f64;
    let h = (tr + k1 * l1 / ((nf - 1.0) * (nf - 2.0)) - 2.0 / (nf - 2.0) * kl) / (nf * (nf - 3.0));
    if !h.is_finite() {
        return Err(SimilarityError::NonFinite("HSIC".into()));
    }
    Ok(h)
}

/// Debiased linear CKA: `H(X,Y) / sqrt(H(X,X) H(Y,Y))`. `cka(x, x)` is
/// exactly 1 (`H / sqrt(H H)` is exact in binary floating point) and
/// `cka(a, b) == cka(b, a)` bit for bit, both by construction.
pub fn cka(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError> {
    let hxy = hsic_unbiased(x, y)?;
    let hxx = hsic_unbiased(x, x)?;
    if !(hxx.is_finite() && hxx > 0.0) {
        return Err(SimilarityError::NonPositiveHsic { which: "x", value: hxx });
    }
    let hyy = hsic_unbiased(y, y)?;
    if !(hyy.is_finite() && hyy > 0.0) {
        return Err(SimilarityError::NonPositiveHsic { which: "y", value: hyy });
    }
    let c = hxy / (hxx * hyy).sqrt();
    if !c.is_finite() {
        return Err(SimilarityError::NonFinite("CKA".into()));
    }
    Ok(c)
}

/// The dot product, summed in order.
fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut s = 0.0f64;
    for (&u, &v) in a.iter().zip(b) {
        s += u * v;
    }
    s
}

/// The squared Frobenius norm of the p x p row-major `m`, summed so that
/// `m` and its transpose give the same bits: the diagonal, then each
/// `a < b` pair as `m[a][b]^2 + m[b][a]^2`.
fn frobenius_sq_symmetric(m: &[f64], p: usize) -> f64 {
    let mut s = 0.0f64;
    for a in 0..p {
        let d = m[a * p + a];
        s += d * d;
        for b in a + 1..p {
            let (u, v) = (m[a * p + b], m[b * p + a]);
            s += u * u + v * v;
        }
    }
    s
}
```

- [ ] **Step 5: Run them to see them pass.** Run: `cargo test -p semblance-fidelity similarity::`. Expected: 8 passed. `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean (rewrite any flagged loop with iterators, keeping every sum's order and its symmetry in `x` and `y`). `cargo build --workspace --locked` succeeds (the lock is current).

- [ ] **Step 6: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed` (RELEASE-01 still sees 8 packages; RELEASE-02 skips dev-dependencies, so the new `semblance-rng` edge needs no change to `app/tests/acceptance_release.rs`, which this task owns only in case it does).

- [ ] **Step 7: Commit.**

```bash
git add crates/fidelity/src/similarity.rs crates/fidelity/src/lib.rs crates/fidelity/Cargo.toml Cargo.lock
git commit -m "Add debiased linear CKA over central response time courses"
```

---

### Task sim-zh: Zhou & Hasler's representational variant

Owns: crates/fidelity/src/similarity.rs, CLAUDE.md
Uses: similarity-cka
Provides: similarity-zh
Wave: 4

**What this builds.** Appended to `crates/fidelity/src/similarity.rs` (module `semblance_fidelity::similarity`, which already holds `SimilarityError` with variants `RowMismatch {x, y}`, `ColumnMismatch {x, y}`, `TooFew {what, need, found}`, `NonPositiveHsic {which, value}`, `NonFinite(String)`, `OnsetCount`, `OnsetPastEnd`, `NotF32 {row, column}`, `Length {what, expected, found}`, `NotSymmetric {i, j}`, `Degenerate(String)`; `ResponseMatrix`, `hsic_unbiased`, `cka`; the private `dot(a, b)` (sequential) and `frobenius_sq_symmetric(m, p)` (diagonal, then each a<b pair as `m[a][b]² + m[b][a]²`); and test helpers `names(p)`, `random_rows(seed, n, p)`, `related(x, seed, scale)`): Zhou & Hasler's variant, computed exactly and **reported only, never gated** (owner's answer Q3, revised: it barely separates trained from random models). This task is wave 4's **count owner** (Step 5).

**Facts this task needs (task 0, "Zhou & Hasler's representational axis").**
- The response is **the peak central-cell raw voltage per type over the whole trace**: 65 values per condition (stimulus), no rectification, no baseline, no time bins.
- The RDM is cosine distance with their +1e-10, placed here as `d = 1 − (a·b) / (‖a‖·‖b‖ + 1e-10)` (the placement is unverified against their text; only reported numbers depend on it). Only the upper triangle is read; the diagonal is stored as 0.
- RDMs are compared by Spearman and by Kendall τA over the upper triangle (diagonal excluded): Spearman ties take averaged ranks (Pearson of the ranks); τA = (concordant − discordant) / (m(m−1)/2), where a pair tied in either RDM counts as neither and m is the number of upper-triangle entries.
- Their CKA is **linear and biased**: HSIC on double-centred kernels (`tr(KHLH)`; the normalisation cancels), computed on **ensemble-mean** peak matrices, not per model (the caller averages; `mean_peaks` and `Rdm::mean` help). In feature space: centre each column, then `‖XcᵀYc‖²_F / sqrt(‖XcᵀXc‖²_F · ‖YcᵀYc‖²_F)`.
- No Nili permutation test: only correlations, no p-values. Their code (github.com/michaela10c/connectome-fidelity) has **no licence**: this is reimplemented from the description and never copied.
- The spec's signatures returned bare values and hard-coded 65; here every function returns `Result` and refuses a type count other than 65 (a recorded deviation, A14).
- Hand values: for upper triangles a = [1, 2, 2], b = [1, 2, 3]: Spearman = √3/2 = 0.8660254037844386, τA = 2/3. Biased CKA of the centred rows x = [[1,0],[0,1],[−1,−1]] and y = [[1,0],[0,2],[−1,−2]] (other columns 0) is 0.9068453126375146.

**Files.** Modify: `crates/fidelity/src/similarity.rs`; `CLAUDE.md` (the two test-total figures).

- [ ] **Step 1: Write the failing tests.** Append inside `mod tests`:

```rust
    /// A 65-wide vector with `first` in its leading columns, zeros elsewhere.
    fn wide(first: &[f64]) -> [f64; ZH_TYPES] {
        let mut v = [0.0; ZH_TYPES];
        v[..first.len()].copy_from_slice(first);
        v
    }

    fn rdm3(upper: [f64; 3]) -> Rdm {
        let [a, b, c] = upper;
        Rdm::new(3, vec![0.0, a, b, a, 0.0, c, b, c, 0.0]).unwrap()
    }

    #[test]
    fn peak_vectors_are_each_conditions_raw_maximum() {
        let mut r = Responses::new(names(ZH_TYPES), 0.005, Precision::F64).unwrap();
        let frames = |k: f64| (0..3).flat_map(|f| (0..ZH_TYPES).map(move |t| -(t as f64) + f as f64 * k)).collect::<Vec<f64>>();
        r.push("a".into(), 3, frames(1.0)).unwrap();
        r.push("b".into(), 3, frames(-1.0)).unwrap();
        let r = r.finish().unwrap();
        let v = peak_vectors(&r).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!((v[0][0], v[0][64]), (2.0, -62.0), "the last frame's, raw");
        assert_eq!((v[1][0], v[1][64]), (0.0, -64.0), "the first frame's, not rectified");
        let few = Responses::new(names(3), 0.005, Precision::F64).unwrap();
        let mut few = few;
        few.push("a".into(), 1, vec![0.0; 3]).unwrap();
        assert_eq!(peak_vectors(&few.finish().unwrap()), Err(SimilarityError::Length { what: "cell types", expected: 65, found: 3 }));
    }

    #[test]
    fn the_cosine_rdm_adds_their_epsilon_to_the_norms() {
        let v = [wide(&[1.0, 0.0]), wide(&[0.0, 1.0]), wide(&[1.0, 1.0])];
        let d = cosine_rdm(&v).unwrap();
        assert_eq!(d.n(), 3);
        assert_eq!(d.get(0, 1), Some(1.0));
        assert_eq!(d.get(0, 2), Some(1.0 - 1.0 / (2f64.sqrt() + 1e-10)));
        assert_eq!(d.get(2, 0), d.get(0, 2));
        assert_eq!(d.get(1, 1), Some(0.0));
        assert_eq!(d.get(3, 0), None);
        assert_eq!(d.upper(), vec![1.0, 1.0 - 1.0 / (2f64.sqrt() + 1e-10), 1.0 - 1.0 / (2f64.sqrt() + 1e-10)]);
        assert_eq!(cosine_rdm(&v[..1]), Err(SimilarityError::TooFew { what: "conditions", need: 2, found: 1 }));
        assert!(matches!(cosine_rdm(&[wide(&[f64::NAN]), wide(&[1.0])]), Err(SimilarityError::NonFinite(_))));
    }

    #[test]
    fn spearman_and_kendall_tau_a_handle_ties_by_hand() {
        let (a, b) = (rdm3([1.0, 2.0, 2.0]), rdm3([1.0, 2.0, 3.0]));
        assert!((spearman(&a, &b).unwrap() - 0.8660254037844386).abs() <= 1e-15);
        assert_eq!(kendall_tau_a(&a, &b), Ok(2.0 / 3.0));
        assert_eq!(kendall_tau_a(&b, &b), Ok(1.0));
        assert_eq!(kendall_tau_a(&b, &rdm3([3.0, 2.0, 1.0])), Ok(-1.0));
        assert!(matches!(spearman(&rdm3([1.0, 1.0, 1.0]), &b), Err(SimilarityError::Degenerate(_))));
        let two = Rdm::new(2, vec![0.0, 1.0, 1.0, 0.0]).unwrap();
        assert_eq!(kendall_tau_a(&two, &two), Err(SimilarityError::TooFew { what: "upper-triangle entries", need: 2, found: 1 }));
        let four = Rdm::new(4, vec![0.0; 16]).unwrap();
        assert_eq!(spearman(&b, &four), Err(SimilarityError::Length { what: "RDM conditions", expected: 3, found: 4 }));
    }

    #[test]
    fn an_rdm_is_square_symmetric_and_finite() {
        assert_eq!(Rdm::new(2, vec![0.0; 3]), Err(SimilarityError::Length { what: "RDM entries", expected: 4, found: 3 }));
        assert_eq!(Rdm::new(3, vec![0.0, 1.0, 2.0, 1.0, 0.0, 3.0, 2.5, 3.0, 0.0]), Err(SimilarityError::NotSymmetric { i: 0, j: 2 }));
        assert!(matches!(Rdm::new(2, vec![0.0, f64::NAN, f64::NAN, 0.0]), Err(SimilarityError::NonFinite(_))));
        let m = Rdm::mean(&[rdm3([1.0, 2.0, 3.0]), rdm3([3.0, 2.0, 1.0])]).unwrap();
        assert_eq!(m.upper(), vec![2.0, 2.0, 2.0]);
        assert_eq!(Rdm::mean(&[]), Err(SimilarityError::TooFew { what: "RDMs", need: 1, found: 0 }));
    }

    #[test]
    fn biased_cka_matches_a_hand_computation_and_is_1_on_itself() {
        let x = [wide(&[1.0, 0.0]), wide(&[0.0, 1.0]), wide(&[-1.0, -1.0])];
        let y = [wide(&[1.0, 0.0]), wide(&[0.0, 2.0]), wide(&[-1.0, -2.0])];
        assert!((cka_biased(&x, &y).unwrap() - 0.9068453126375146).abs() <= 1e-15);
        assert_eq!(cka_biased(&x, &x), Ok(1.0));
        assert_eq!(cka_biased(&x, &y[..2]), Err(SimilarityError::RowMismatch { x: 3, y: 2 }));
        assert_eq!(cka_biased(&x[..1], &y[..1]), Err(SimilarityError::TooFew { what: "conditions", need: 2, found: 1 }));
        assert!(matches!(cka_biased(&[wide(&[f64::NAN]), wide(&[1.0])], &y[..2]), Err(SimilarityError::NonFinite(_))));
        let flat = [wide(&[1.0]), wide(&[1.0]), wide(&[1.0])];
        assert!(matches!(cka_biased(&flat, &y), Err(SimilarityError::Degenerate(_))));
    }

    #[test]
    fn mean_peaks_average_the_models_elementwise() {
        let a = vec![wide(&[1.0, 2.0]), wide(&[3.0])];
        let b = vec![wide(&[3.0, 4.0]), wide(&[5.0])];
        let m = mean_peaks(&[a.clone(), b]).unwrap();
        assert_eq!((m[0][0], m[0][1], m[1][0]), (2.0, 3.0, 4.0));
        assert_eq!(mean_peaks(&[]), Err(SimilarityError::TooFew { what: "models", need: 1, found: 0 }));
        assert_eq!(mean_peaks(&[a, vec![wide(&[0.0])]]), Err(SimilarityError::Length { what: "a model's conditions", expected: 2, found: 1 }));
    }
```

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p semblance-fidelity similarity::`. Expected: compile errors (`cannot find function cosine_rdm`, …).

- [ ] **Step 3: Write the implementation.** Add above `#[cfg(test)]`:

```rust
/// The cell types in Zhou & Hasler's peak vectors: flyvis's 65.
pub const ZH_TYPES: usize = 65;

/// A representational dissimilarity matrix over `n` conditions, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Rdm {
    n: usize,
    d: Vec<f64>,
}

impl Rdm {
    /// Refuses fewer than 2 conditions, a length other than n * n, a
    /// non-finite entry, and an asymmetric matrix.
    pub fn new(n: usize, d: Vec<f64>) -> Result<Rdm, SimilarityError> {
        if n < 2 {
            return Err(SimilarityError::TooFew { what: "conditions", need: 2, found: n });
        }
        if d.len() != n * n {
            return Err(SimilarityError::Length { what: "RDM entries", expected: n * n, found: d.len() });
        }
        if let Some(k) = d.iter().position(|v| !v.is_finite()) {
            return Err(SimilarityError::NonFinite(format!("RDM entry ({}, {})", k / n, k % n)));
        }
        for i in 0..n {
            for j in i + 1..n {
                if d[i * n + j] != d[j * n + i] {
                    return Err(SimilarityError::NotSymmetric { i, j });
                }
            }
        }
        Ok(Rdm { n, d })
    }

    pub fn n(&self) -> usize {
        self.n
    }

    pub fn get(&self, i: usize, j: usize) -> Option<f64> {
        if i < self.n && j < self.n { Some(self.d[i * self.n + j]) } else { None }
    }

    /// The entries above the diagonal, row by row.
    pub fn upper(&self) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.n * (self.n - 1) / 2);
        for i in 0..self.n {
            out.extend_from_slice(&self.d[i * self.n + i + 1..(i + 1) * self.n]);
        }
        out
    }

    /// The elementwise mean of RDMs over the same conditions, summed in order.
    pub fn mean(rdms: &[Rdm]) -> Result<Rdm, SimilarityError> {
        let first = rdms.first().ok_or(SimilarityError::TooFew { what: "RDMs", need: 1, found: 0 })?;
        let mut sum = vec![0.0f64; first.d.len()];
        for r in rdms {
            if r.n != first.n {
                return Err(SimilarityError::Length { what: "RDM conditions", expected: first.n, found: r.n });
            }
            for (s, &v) in sum.iter_mut().zip(&r.d) {
                *s += v;
            }
        }
        let k = rdms.len() as f64;
        Rdm::new(first.n, sum.into_iter().map(|s| s / k).collect())
    }
}

/// Zhou & Hasler's response: per condition (stimulus), each cell type's
/// peak raw central voltage over the whole trace. No rectification, no
/// baseline, no time bins.
pub fn peak_vectors(r: &Responses) -> Result<Vec<[f64; ZH_TYPES]>, SimilarityError> {
    if r.cell_types().len() != ZH_TYPES {
        return Err(SimilarityError::Length { what: "cell types", expected: ZH_TYPES, found: r.cell_types().len() });
    }
    let mut out = Vec::with_capacity(r.len());
    for s in 0..r.len() {
        let st = r.stimulus(s).ok_or(SimilarityError::TooFew { what: "stimuli", need: s + 1, found: r.len() })?;
        let mut v = [f64::NEG_INFINITY; ZH_TYPES];
        for f in 0..st.n_frames() {
            if let Some(row) = st.frame(f) {
                for (peak, &x) in v.iter_mut().zip(row) {
                    if x > *peak {
                        *peak = x;
                    }
                }
            }
        }
        out.push(v);
    }
    Ok(out)
}

/// The elementwise mean over models of their peak vectors: the ensemble
/// means Zhou & Hasler's biased CKA is computed on.
pub fn mean_peaks(per_model: &[Vec<[f64; ZH_TYPES]>]) -> Result<Vec<[f64; ZH_TYPES]>, SimilarityError> {
    let first = per_model.first().ok_or(SimilarityError::TooFew { what: "models", need: 1, found: 0 })?;
    let mut sum = vec![[0.0f64; ZH_TYPES]; first.len()];
    for m in per_model {
        if m.len() != first.len() {
            return Err(SimilarityError::Length { what: "a model's conditions", expected: first.len(), found: m.len() });
        }
        for (s, v) in sum.iter_mut().zip(m) {
            for (a, &b) in s.iter_mut().zip(v) {
                *a += b;
            }
        }
    }
    let k = per_model.len() as f64;
    Ok(sum.into_iter().map(|s| s.map(|a| a / k)).collect())
}

/// Zhou & Hasler's cosine RDM: `1 - (a.b) / (|a| |b| + 1e-10)` for every
/// pair of conditions; the diagonal is 0 (only the upper triangle is read).
pub fn cosine_rdm(v: &[[f64; ZH_TYPES]]) -> Result<Rdm, SimilarityError> {
    let n = v.len();
    if n < 2 {
        return Err(SimilarityError::TooFew { what: "conditions", need: 2, found: n });
    }
    if let Some(k) = v.iter().position(|x| x.iter().any(|y| !y.is_finite())) {
        return Err(SimilarityError::NonFinite(format!("condition {k}'s peak vector")));
    }
    let norms: Vec<f64> = v.iter().map(|x| dot(x, x).sqrt()).collect();
    let mut d = vec![0.0f64; n * n];
    for i in 0..n {
        for j in i + 1..n {
            let x = 1.0 - dot(&v[i], &v[j]) / (norms[i] * norms[j] + 1e-10);
            d[i * n + j] = x;
            d[j * n + i] = x;
        }
    }
    Rdm::new(n, d)
}

/// Spearman's rho over the two RDMs' upper triangles: the Pearson r of their
/// ranks, ties taking their average rank.
pub fn spearman(a: &Rdm, b: &Rdm) -> Result<f64, SimilarityError> {
    let (ua, ub) = upper_pair(a, b)?;
    let r = pearson_population(&ranks(&ua), &ranks(&ub));
    if r.is_finite() { Ok(r) } else { Err(SimilarityError::Degenerate("an RDM's upper triangle is constant".into())) }
}

/// Kendall's tau-a over the two RDMs' upper triangles: (concordant -
/// discordant) / (m (m - 1) / 2); a pair tied in either counts as neither.
pub fn kendall_tau_a(a: &Rdm, b: &Rdm) -> Result<f64, SimilarityError> {
    let (ua, ub) = upper_pair(a, b)?;
    let m = ua.len();
    let (mut concordant, mut discordant) = (0i64, 0i64);
    for i in 0..m {
        for j in i + 1..m {
            let sa = (ua[i] > ua[j]) as i64 - (ua[i] < ua[j]) as i64;
            let sb = (ub[i] > ub[j]) as i64 - (ub[i] < ub[j]) as i64;
            match sa * sb {
                1 => concordant += 1,
                -1 => discordant += 1,
                _ => {}
            }
        }
    }
    Ok((concordant - discordant) as f64 / (m * (m - 1) / 2) as f64)
}

/// Zhou & Hasler's linear, biased CKA, on (ensemble-mean) peak matrices:
/// columns centred, then `|Xc'Yc|_F^2 / sqrt(|Xc'Xc|_F^2 |Yc'Yc|_F^2)`.
pub fn cka_biased(x: &[[f64; ZH_TYPES]], y: &[[f64; ZH_TYPES]]) -> Result<f64, SimilarityError> {
    if x.len() != y.len() {
        return Err(SimilarityError::RowMismatch { x: x.len(), y: y.len() });
    }
    if x.len() < 2 {
        return Err(SimilarityError::TooFew { what: "conditions", need: 2, found: x.len() });
    }
    if x.iter().chain(y).any(|r| r.iter().any(|v| !v.is_finite())) {
        return Err(SimilarityError::NonFinite("a peak matrix".into()));
    }
    let (xc, yc) = (centred(x), centred(y));
    let (xy, xx, yy) = (cross(&xc, &yc), cross(&xc, &xc), cross(&yc, &yc));
    let den = (frobenius_sq_symmetric(&xx, ZH_TYPES) * frobenius_sq_symmetric(&yy, ZH_TYPES)).sqrt();
    if !(den.is_finite() && den > 0.0) {
        return Err(SimilarityError::Degenerate("a peak matrix has no variance across conditions".into()));
    }
    let c = frobenius_sq_symmetric(&xy, ZH_TYPES) / den;
    if c.is_finite() { Ok(c) } else { Err(SimilarityError::NonFinite("biased CKA".into())) }
}

fn upper_pair(a: &Rdm, b: &Rdm) -> Result<(Vec<f64>, Vec<f64>), SimilarityError> {
    if a.n != b.n {
        return Err(SimilarityError::Length { what: "RDM conditions", expected: a.n, found: b.n });
    }
    let (ua, ub) = (a.upper(), b.upper());
    if ua.len() < 2 {
        return Err(SimilarityError::TooFew { what: "upper-triangle entries", need: 2, found: ua.len() });
    }
    Ok((ua, ub))
}

/// 1-based ranks, ties averaged.
fn ranks(v: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..v.len()).collect();
    order.sort_by(|&i, &j| v[i].total_cmp(&v[j]));
    let mut out = vec![0.0; v.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && v[order[end]] == v[order[start]] {
            end += 1;
        }
        let rank = (start + 1 + end) as f64 / 2.0;
        for &k in &order[start..end] {
            out[k] = rank;
        }
        start = end;
    }
    out
}

/// Pearson's r with population moments, sums in order; NaN when an input
/// is constant.
fn pearson_population(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let (mut cov, mut va, mut vb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b) {
        let (dx, dy) = (x - ma, y - mb);
        cov += dx * dy;
        va += dx * dx;
        vb += dy * dy;
    }
    (cov / n) / ((va / n).sqrt() * (vb / n).sqrt())
}

/// Each column minus its mean (summed in order, over the rows).
fn centred(m: &[[f64; ZH_TYPES]]) -> Vec<[f64; ZH_TYPES]> {
    let mut mean = [0.0f64; ZH_TYPES];
    for row in m {
        for (s, &v) in mean.iter_mut().zip(row) {
            *s += v;
        }
    }
    let n = m.len() as f64;
    for s in mean.iter_mut() {
        *s /= n;
    }
    m.iter().map(|row| std::array::from_fn(|t| row[t] - mean[t])).collect()
}

/// `A'B` for row-major condition-by-type matrices, as a ZH_TYPES x ZH_TYPES
/// row-major vector, each entry summed over the rows in order.
fn cross(a: &[[f64; ZH_TYPES]], b: &[[f64; ZH_TYPES]]) -> Vec<f64> {
    let mut m = vec![0.0f64; ZH_TYPES * ZH_TYPES];
    for (ra, rb) in a.iter().zip(b) {
        for (row, &x) in m.chunks_exact_mut(ZH_TYPES).zip(ra) {
            for (cell, &y) in row.iter_mut().zip(rb) {
                *cell += x * y;
            }
        }
    }
    m
}
```

Also add to the module doc comment, after its first paragraph: `//! Zhou & Hasler's variant ([`peak_vectors`], [`cosine_rdm`], [`spearman`], [`kendall_tau_a`], [`cka_biased`]) is reimplemented from their paper's description (their code has no licence) and is reported, never gated.`

- [ ] **Step 4: Run them to see them pass.** Run: `cargo test -p semblance-fidelity similarity::`. Expected: all pass. `cargo clippy -p semblance-fidelity --all-targets -- -D warnings`: clean.

- [ ] **Step 5: Re-measure the test total (count owner).** Run the whole suite as Global Constraints' "Measuring the total" says and write the measured number into CLAUDE.md's two test-total figures. Change nothing else in CLAUDE.md.

- [ ] **Step 6: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed`.

- [ ] **Step 7: Commit.**

```bash
git add crates/fidelity/src/similarity.rs CLAUDE.md
git commit -m "Add Zhou and Hasler's peak-voltage RDMs, rank correlations and biased CKA"
```

---

### Task gate-e-legacy: the legacy distances refuse malformed input

Owns: crates/fidelity/src/lib.rs, app/src/main.rs, app/tests/acceptance_validate.rs, app/tests/validate_cli.rs
Uses: similarity-cka
Provides: fidelity-error
Wave: 4

**What this builds.** Gate E for the legacy path (spec RI §4): today `compare_behaviour` `.min()`s every dimension and scores 0.0 when nothing is compared (chore defect 2), and `behavioural_distance` scores an empty battery 0.0 (chore defect 3), so both fail toward *perfect*. This task adds `FidelityError` and makes `compare_behaviour`, `behavioural_distance` and `size_fidelity_curve` return `Result`, with a test for each defect that fails on today's code, and replaces `probe_battery_collects_probes_for_behavioural_distance`'s vacuous `== 0.0`. On equal shapes the arithmetic is unchanged. `app`'s `roundtrip` handles the `Result` and still prints `0.000000` for both distances; `evolve` calls only `run_probe`, which does not change, so it needs nothing (A11; task 1b later removes `roundtrip`'s behavioural line: not here). These signatures are breaking changes inside 0.4.0's minor bump; no version number changes. `app/tests/acceptance_validate.rs` (`validate_18_roundtrip_is_still_lossless`) and `app/tests/validate_cli.rs` (`roundtrip_is_still_lossless`) pin `roundtrip`'s output and must pass **unchanged**; they are owned only so nothing else touches them this wave. This task adds at most 15 `#[test]`s (it adds 5) and does not touch CLAUDE.md.

**Facts this task needs.**
- `semblance_sim::Behaviour { pub channels: Vec<Vec<f32>> /* channels[c][t] */, pub spike_counts: Vec<u32> /* per neuron */, pub ticks: u64 }`, `Default` is the empty recording. `Engine::run` records one sample per tick per channel, so equal-shaped recordings have equal `ticks` and equal per-channel lengths.
- The chore's reproductions: a busy recording (4 channels, 500 neurons) against a completely empty one scores 0 today; an empty battery against `Connectome::synthetic(80, 5, 9)` with every weight shifted by +5.0 scores 0 today (one probe scores about 0.847). `Connectome::{synthetic(n, degree, seed), weights() -> Vec<f32>, with_weights(&[f32]) -> Connectome}`; `semblance_codec::{Codec, WeightCodec}` (`WeightCodec::from_connectome(&c)`, lossless).
- The mismatch rule fixed here (A12): a channel-count, neuron-count (`spike_counts.len()`) or `ticks` mismatch, or any channel whose length differs between the two, is an error; two recordings with no channel samples and no neurons compare nothing, which is an error too.
- `cmd_roundtrip` in `app/src/main.rs` is the only caller outside the crate (`let bd = behavioural_distance(&c, &decoded, &battery);`, printed `behavioural distance: {bd:.6}   (0.0 == identical behaviour)`).

**Files.** Modify: `crates/fidelity/src/lib.rs`; `app/src/main.rs`.

- [ ] **Step 1: Write the failing tests.** In `lib.rs`'s `mod tests`, replace the three existing tests with these versions and add the new ones (keep `test_battery()`):

```rust
    #[test]
    fn lossless_codec_has_zero_behavioural_distance() {
        let c = Connectome::synthetic(80, 5, 9);
        let codec = WeightCodec::from_connectome(&c);
        let z = codec.encode(&c);
        let decoded = codec.decode(&z);

        let battery = test_battery();
        assert_eq!(behavioural_distance(&c, &decoded, &battery), Ok(0.0), "a lossless round-trip must behave identically");
        assert_eq!(structural_distance(&c, &decoded), 0.0);
    }

    #[test]
    fn perturbing_weights_increases_distance() {
        let c = Connectome::synthetic(80, 5, 9);
        let mut w = c.weights();
        for x in w.iter_mut() {
            *x += 0.5;
        }
        let perturbed = c.with_weights(&w);
        let bd = behavioural_distance(&c, &perturbed, &test_battery()).unwrap();
        assert!(bd > 0.0, "changed dynamics should change behaviour");
    }

    #[test]
    fn probe_battery_collects_probes_for_behavioural_distance() {
        let c = Connectome::synthetic(80, 5, 9);
        let mut w = c.weights();
        for x in w.iter_mut() {
            *x += 0.5;
        }
        let different = c.with_weights(&w);
        let battery = ProbeBattery::new()
            .with(DriveProbe { k: 8, amplitude: 1.5, motor: 4, ticks: 150 })
            .with(DriveProbe { k: 4, amplitude: 2.0, motor: 4, ticks: 150 });

        assert_eq!(battery.len(), 2);
        assert!(!battery.is_empty());
        let bd = behavioural_distance(&c, &different, battery.as_slice()).unwrap();
        assert!(bd > 0.0, "a different network behaves differently: {bd}");
        assert_eq!(behavioural_distance(&c, &c, battery.as_slice()), Ok(0.0));
    }

    /// Chore defect 2: a busy recording against an empty one scored 0.
    #[test]
    fn a_busy_recording_against_an_empty_one_is_an_error() {
        let busy = Behaviour { channels: vec![vec![1.0; 50]; 4], spike_counts: vec![7; 500], ticks: 50 };
        assert_eq!(compare_behaviour(&busy, &Behaviour::default()), Err(FidelityError::ChannelCount { a: 4, b: 0 }));
        let mut fewer = busy.clone();
        fewer.spike_counts.pop();
        assert_eq!(compare_behaviour(&busy, &fewer), Err(FidelityError::NeuronCount { a: 500, b: 499 }));
        let mut shorter = busy.clone();
        shorter.ticks = 49;
        assert_eq!(compare_behaviour(&busy, &shorter), Err(FidelityError::TickCount { a: 50, b: 49 }));
        let mut ragged = busy.clone();
        ragged.channels[2].pop();
        assert_eq!(compare_behaviour(&busy, &ragged), Err(FidelityError::ChannelLength { channel: 2, a: 50, b: 49 }));
        assert_eq!(compare_behaviour(&Behaviour::default(), &Behaviour::default()), Err(FidelityError::NothingCompared));
    }

    /// Equal shapes: the arithmetic is the old one.
    #[test]
    fn equal_shapes_score_as_before() {
        let a = Behaviour { channels: vec![vec![1.0, 0.0]], spike_counts: vec![2], ticks: 2 };
        let b = Behaviour { channels: vec![vec![0.0, 0.0]], spike_counts: vec![0], ticks: 2 };
        assert_eq!(compare_behaviour(&a, &b), Ok(1.0f32 / 2.0 + 0.01 * 2.0));
    }

    /// Chore defect 3: an empty battery scored 0 against a very different network.
    #[test]
    fn an_empty_battery_is_an_error_not_a_perfect_score() {
        let c = Connectome::synthetic(80, 5, 9);
        let w: Vec<f32> = c.weights().iter().map(|x| x + 5.0).collect();
        let shifted = c.with_weights(&w);
        let none: Vec<DriveProbe> = Vec::new();
        assert_eq!(behavioural_distance(&c, &shifted, &none), Err(FidelityError::EmptyBattery));
        assert!(behavioural_distance(&c, &shifted, &test_battery()).unwrap() > 0.0);
        let codecs: Vec<Box<dyn Codec>> = vec![Box::new(WeightCodec::from_connectome(&c))];
        assert_eq!(size_fidelity_curve(&c, &codecs, &none).map(|v| v.len()), Err(FidelityError::EmptyBattery));
        assert_eq!(size_fidelity_curve(&c, &codecs, &test_battery()).map(|v| v[0].behavioural_distance), Ok(0.0));
    }

    #[test]
    fn errors_display_as_registered() {
        let cases = [
            (FidelityError::ChannelCount { a: 4, b: 0 }, "the recordings have 4 and 0 motor channels"),
            (FidelityError::NeuronCount { a: 500, b: 499 }, "the recordings have 500 and 499 neurons"),
            (FidelityError::TickCount { a: 50, b: 49 }, "the recordings ran 50 and 49 ticks"),
            (FidelityError::ChannelLength { channel: 2, a: 50, b: 49 }, "motor channel 2 has 50 and 49 samples"),
            (FidelityError::NothingCompared, "the recordings have no channel samples and no neurons to compare"),
            (FidelityError::EmptyBattery, "a battery with no probes measures nothing"),
        ];
        for (e, want) in cases {
            assert_eq!(e.to_string(), want);
        }
    }
```

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p semblance-fidelity --lib tests::`. Expected: compile errors (`cannot find type FidelityError`, mismatched types: `f32` vs `Result`) — on today's code each of defects 2 and 3 cannot be expressed as anything but a score.

- [ ] **Step 3: Write the implementation.** In `lib.rs`, add after the `pub mod similarity;` line:

```rust
/// Why two behaviours could not be compared. Each is a refusal: before
/// 0.4.0 every one of these scored as a perfect 0 (chore defects 2 and 3).
#[derive(Clone, Debug, PartialEq)]
pub enum FidelityError {
    ChannelCount { a: usize, b: usize },
    NeuronCount { a: usize, b: usize },
    TickCount { a: u64, b: u64 },
    ChannelLength { channel: usize, a: usize, b: usize },
    /// Neither recording has a channel sample or a neuron.
    NothingCompared,
    /// A battery with no probes.
    EmptyBattery,
}

impl std::fmt::Display for FidelityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FidelityError::ChannelCount { a, b } => write!(f, "the recordings have {a} and {b} motor channels"),
            FidelityError::NeuronCount { a, b } => write!(f, "the recordings have {a} and {b} neurons"),
            FidelityError::TickCount { a, b } => write!(f, "the recordings ran {a} and {b} ticks"),
            FidelityError::ChannelLength { channel, a, b } => write!(f, "motor channel {channel} has {a} and {b} samples"),
            FidelityError::NothingCompared => f.write_str("the recordings have no channel samples and no neurons to compare"),
            FidelityError::EmptyBattery => f.write_str("a battery with no probes measures nothing"),
        }
    }
}

impl std::error::Error for FidelityError {}
```

Replace `compare_behaviour` with (the arithmetic is the old one, on shapes now known equal):

```rust
/// Distance between two behaviour recordings. Combines a motor-channel term
/// (mean squared difference over channels and ticks) and a rate term
/// (difference in per-neuron spike counts). Lower = more faithful.
///
/// The recordings must have the same shape: a different channel count,
/// neuron count, tick count or channel length is a [`FidelityError`], as is
/// a pair with nothing to compare. Never a score.
pub fn compare_behaviour(a: &Behaviour, b: &Behaviour) -> Result<f32, FidelityError> {
    if a.channels.len() != b.channels.len() {
        return Err(FidelityError::ChannelCount { a: a.channels.len(), b: b.channels.len() });
    }
    if a.spike_counts.len() != b.spike_counts.len() {
        return Err(FidelityError::NeuronCount { a: a.spike_counts.len(), b: b.spike_counts.len() });
    }
    if a.ticks != b.ticks {
        return Err(FidelityError::TickCount { a: a.ticks, b: b.ticks });
    }
    for (c, (x, y)) in a.channels.iter().zip(&b.channels).enumerate() {
        if x.len() != y.len() {
            return Err(FidelityError::ChannelLength { channel: c, a: x.len(), b: y.len() });
        }
    }

    let (channel_term, count) = {
        let mut acc = 0.0f32;
        let mut count = 0usize;
        for (x, y) in a.channels.iter().zip(&b.channels) {
            for (p, q) in x.iter().zip(y) {
                let d = p - q;
                acc += d * d;
                count += 1;
            }
        }
        (if count == 0 { 0.0 } else { acc / count as f32 }, count)
    };
    if count == 0 && a.spike_counts.is_empty() {
        return Err(FidelityError::NothingCompared);
    }

    let rate_term = {
        let n = a.spike_counts.len();
        if n == 0 {
            0.0
        } else {
            let mut acc = 0.0f32;
            for (&p, &q) in a.spike_counts.iter().zip(&b.spike_counts) {
                let d = p as f32 - q as f32;
                acc += d * d;
            }
            (acc / n as f32).sqrt()
        }
    };

    Ok(channel_term + 0.01 * rate_term)
}
```

Replace `behavioural_distance`:

```rust
/// Behavioural fidelity of a decoded network vs the original, averaged over a
/// battery of probes: the mean [`compare_behaviour`] distance (0 = perfect).
/// An empty battery measures nothing, so it is an error, never 0.
pub fn behavioural_distance<P: Probe>(original: &Connectome, decoded: &Connectome, battery: &[P]) -> Result<f32, FidelityError> {
    if battery.is_empty() {
        return Err(FidelityError::EmptyBattery);
    }
    let mut total = 0.0f32;
    for probe in battery {
        let ob = run_probe(original, probe);
        let db = run_probe(decoded, probe);
        total += compare_behaviour(&ob, &db)?;
    }
    Ok(total / battery.len() as f32)
}
```

In `size_fidelity_curve`, change the return type to `Result<Vec<CurvePoint>, FidelityError>`, the call to `let bd = behavioural_distance(reference, &decoded, battery)?;`, and the last line to `Ok(points)`; add to its doc comment: `/// A shape mismatch or an empty battery is a [`FidelityError`].`

In the module doc's "What is DONE here" list, change the `compare_behaviour` bullet to:

```rust
//! - [`compare_behaviour`]: distance between two [`Behaviour`] recordings; a
//!   shape mismatch or an empty battery is a [`FidelityError`], never a score
```

In `app/src/main.rs`'s `cmd_roundtrip`, replace `let bd = behavioural_distance(&c, &decoded, &battery);` with:

```rust
    let bd = match behavioural_distance(&c, &decoded, &battery) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("roundtrip: {e}");
            return ExitCode::FAILURE;
        }
    };
```

(the `println!` lines stay as they are, so the output is byte-identical).

- [ ] **Step 4: Run them to see them pass.** Run: `cargo test -p semblance-fidelity`, then `cargo test -p semblance --test acceptance_validate validate_18` and `cargo test -p semblance --test validate_cli roundtrip`. Expected: all pass. Run `cargo run --quiet -- roundtrip`: both distances read exactly `0.000000`.

- [ ] **Step 5: Run the quick gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-quick.sh`. Expected: `gate-quick: passed`.

- [ ] **Step 6: Commit.**

```bash
git add crates/fidelity/src/lib.rs app/src/main.rs
git commit -m "Refuse shape mismatches and empty batteries in the legacy distances"
```

---

### Task docs-counts: re-measure every count and document the new modules

Owns: CLAUDE.md, README.md, app/tests/acceptance_release.rs
Uses: indices-known, similarity-zh, controls-rewire, fidelity-error, respond-network
Provides: none
Wave: 5

**What this builds.** The last task, after every `#[test]` of the waves exists: it re-measures the five counts CLAUDE.md states and the one README states, and documents the new modules. `app/tests/acceptance_release.rs` holds RELEASE-10 and RELEASE-11, which read those counts (`claude_counts()` parses the number before `" tests, 0 failures"` and before `" of those tests skip"`; RELEASE-11 requires the total ≥ the literal `#[test]` lines under `app/` and `crates/`, and the strings `"N passed, 0 failed"` and `every one of the S fails`; RELEASE-10 requires README's `S flyvis parity tests skip` to equal CLAUDE.md's S and not 123). It must pass unchanged; it is owned so its counts and the docs move together.

**Facts this task needs.** CLAUDE.md states, in "Verifying a change": the total T (`cargo test --workspace          # T tests, 0 failures`, and `"T passed, 0 failed"`); the skip count S (`S of those tests skip when …`, `every one of the S fails`); and the notice counts (`P of the S print a notice, N notices in all under --nocapture --test-threads=1`). The six tests that share `good()`'s cached run (`validate_02`, `_07`, `_11`, `_12`, `_13`, `_20` in `app/tests/acceptance_validate.rs`) print one notice between them; every other skipping test prints its own, so P = S − 6. This epic's new fixture-gated tests are in `parity/respond.rs` (3), `parity/controls.rs` (3); `semblance-fidelity` has none.

**Files.** Modify: `CLAUDE.md`, `README.md`.

- [ ] **Step 1: Measure T.** Run `env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --locked` through the build rule; from the job's full log: `grep '^test result:' <log> | awk '{s += $4} END {print s}'`.

- [ ] **Step 2: Measure S and N, with the fixtures absent.** In the worktree, replace the data link with an empty directory so nothing is linked: `mv data data.real && mkdir data`. Then, through the build rule:
  - `env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --locked --no-fail-fast`; S is the sum of the `failed` figures: `grep '^test result:' <log> | awk '{s += $6} END {print s}'`;
  - `cargo test --workspace --locked -- --nocapture --test-threads=1`; N is `grep -c 'skipping: ' <log>`, and every test still passes.
  Restore: `rmdir data && mv data.real data`. Check P = S − 6 by confirming the six `validate_*` tests above are unchanged (`git diff` of `app/tests/acceptance_validate.rs` from the epic's base is empty).

- [ ] **Step 3: Write the counts.** In CLAUDE.md, set T in both places, S in `S of those tests skip` and `every one of the S fails`, and `P of the S print a notice, N notices in all`. In README.md, set `**Without fixture data, S flyvis parity tests skip`.

- [ ] **Step 4: Document the new modules.** In CLAUDE.md, after the paragraph that begins "`semblance_flyvis::parity::respond` settles any 734-parameter network", add:

```markdown
`semblance_fidelity::indices` computes flyvis's four response indices (FRI,
DSI, preferred direction, tuning correlation) and the paper's agreement
rules from a `Responses`, emulating flyvis's dtypes operation by operation;
`semblance_fidelity::similarity` holds debiased linear CKA and Zhou &
Hasler's variant (reported, never gated); `semblance_flyvis::parity::controls`
builds gate C's four controls, seeded by `control_seed`, with
`respond_network` for the rewired one. Gates A–D, which hold these to
flyvis's exported index values, are not built yet.
```

In README.md's crate table, replace the `fidelity` row with:

```markdown
| `fidelity` | flyvis's response indices and the paper's agreement rules (`indices`); debiased linear CKA and Zhou & Hasler's RDM variant (`similarity`); structural + behavioural distance and the size-vs-fidelity harness, which refuse malformed input instead of scoring it | the **probe battery** design |
```

- [ ] **Step 5: Check the count tests.** Run `cargo test -p semblance --test acceptance_release`. Expected: RELEASE-10 and RELEASE-11 pass.

- [ ] **Step 6: Run the full gate.** `sh /home/miki_przygoda/work/semblance/gates/gate-full.sh`. Expected: passed.

- [ ] **Step 7: Commit.**

```bash
git add CLAUDE.md README.md
git commit -m "Bring the test and skip counts up to date for the fidelity measures"
```

---

## Self-review notes

- **Spec coverage.** RI §1: battery description, `check` (idx-layout); windows (idx-layout; the fixture-gated manifest cross-check is deferred to fidelity-gates, R5); dtype emulation and FRI (idx-fri); edge peaks, DSI, PD, tuning (idx-edges); flyvis's helpers, agreement rules, `binomial_upper_tail`, constants (idx-known); `IndexError` (idx-layout). RI §4: legacy fixes and roundtrip (gate-e-legacy); every `IndexError` variant is reached by a test across the idx-* tasks; every `ResponseError` variant is already tested in `crates/sim/src/response.rs` (`construction_refuses_bad_cell_types_and_dt`, `push_refuses_empty_ragged_and_non_finite_stimuli`, `an_empty_battery_does_not_finish`, `errors_display_as_registered`); the defect analogues are `check_accepts_a_matching_layout_and_refuses_every_mismatch`, `fri_peaks_refuse_responses_the_layout_does_not_describe`, `a_layout_refuses_no_stimuli_and_impossible_edges` and `the_median_fri_is_numpys_median_per_type`. RA §1: `ResponseMatrix`, HSIC, CKA (sim-cka); Zhou & Hasler (sim-zh); every `SimilarityError` variant is reached by a test in sim-cka or sim-zh. RA §2: controls (ctl-params, ctl-rewire), `respond_network` (ctl-network), seeds and redraws (ctl-params). Proof P1–P10 are each a named test above; P3 is the quick and full gates; P10 is ctl-network's Step 5.
- **Not placed:** RI §1's fixture-gated test holding the windows to task 2's manifest reads fixtures-3, so it belongs to fidelity-gates (R5).
- **Plan review (2026-11-05).** Sixteen review findings were checked against the specs, the code on `graded-responses` and an emulation of `semblance-rng`; every one was applied (see `plan-review.md`). The draw count now follows RA §2 (`MAX_REDRAWS = 10`, attempts 0..=10), two closures are fixed for edition 2024, one clippy `redundant_guards` and one wrong FRI test input are fixed, `fri_peaks` and `edge_peaks` refuse an f32 run's non-f32 window values, and the tests gained the f32 epsilon's dtype, a scrambled layout, the mirrored DSI denominator, a pinned random parameter, a colliding rewire, windows off the table and the untested refusal paths.
