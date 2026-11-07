# Codemap: epic `fidelity-measures`

Read-only survey of branch `fidelity-measures` at `ac2d7fd` (= `graded-responses`,
task 1a), worktree `/Users/miki_przygoda/Projects/GitHub-Projects/semblance-fidelity-measures`.
Line numbers are for that commit. Nothing was built.

Scope reminder: spec `2026-11-04-response-indices-design.md` §1 (indices) and §4
(gate E); spec `2026-11-04-representational-axis-design.md` §1 (similarity) and §2
(controls). Nothing here may read `data/flyvis-indices/` (fixtures-3).

---

## 1. Dependency graph and the cycle question

| Crate | `[dependencies]` today |
|---|---|
| `semblance-rng` | none |
| `semblance-connectome` | rng |
| `semblance-codec` | connectome |
| `semblance-sim` | connectome |
| `semblance-evo` | codec, rng |
| `semblance-fidelity` (`crates/fidelity/Cargo.toml:20-23`) | connectome, sim, codec |
| `semblance-flyvis` (`crates/flyvis/Cargo.toml:25-27`) | connectome, sim |
| `semblance` (app) | connectome, sim, codec, evo, fidelity, flyvis |

- **flyvis -> fidelity: no cycle.** fidelity's closure is {connectome, sim, codec,
  rng}; none depends on flyvis. The new `indices`/`similarity` modules must stay
  pure (spec: "no files, no simulator, no flyvis"), so fidelity must NOT gain a
  flyvis dependency; ground truth (`groundtruth::*`) is passed in as data.
- **flyvis -> rng is also new** (controls draw with `semblance_rng::Rng`).
  Add as `semblance-rng.workspace = true` (already in `[workspace.dependencies]`,
  root `Cargo.toml:27`). Do not add anything else: Cargo.lock must stay at exactly
  the 8 `semblance*` packages.
- **Cargo.lock changes** (flyvis's dependency list gains two entries) and must be
  committed: many acceptance tests build a shadow workspace with `--locked`
  (e.g. `crates/flyvis/tests/acceptance_parity.rs:1777-1795` `shadow()`,
  `acceptance_traces.rs`), which fails if Cargo.lock is stale.
- `app/tests/acceptance_release.rs` RELEASE-02 (`:406-433`) checks every normal
  `semblance*` dependency publishes before its dependent against the hard-coded
  `PUBLISH_ORDER` (`:336-345`: rng, connectome, codec, sim, evo, fidelity,
  flyvis, semblance). flyvis->fidelity and flyvis->rng are consistent with it; a
  fidelity->flyvis edge would fail it. RELEASE-01 (`:356-399`) pins
  `[workspace.dependencies]` to exactly the 7 libraries and Cargo.lock to the 8.
- RELEASE-07 (`:557-596`) pins the descriptions of sim, flyvis, codec and app
  byte for byte. **flyvis's description must not change** (or that test changes
  with it). fidelity's description (`crates/fidelity/Cargo.toml:13`, "...The probe
  battery is still a placeholder.") is not pinned.
- Dev profile: only `semblance-sim` and `semblance-flyvis` are opt-level 3
  (root `Cargo.toml:58-62`). `semblance-fidelity` builds at opt-level 0 in tests,
  including when flyvis tests call into it. `traces_23_...`
  (`crates/flyvis/tests/acceptance_traces.rs:1545-1556`) requires those two
  sections to contain exactly `opt-level = 3` and forbids `rustflags`,
  `target-cpu`, `fast-math`, `+fma` anywhere in settings. Adding a
  `[profile.dev.package.semblance-fidelity] opt-level = 3` is not forbidden by
  that test but is a decision to record if made. Keep fidelity unit tests small.

---

## 2. `crates/fidelity` today (one file, `src/lib.rs`, 284 lines; no `tests/` dir)

Imports (`:21-24`): `semblance_codec::{Codec, Latent}`, `semblance_connectome::Connectome`,
`semblance_sim::{Behaviour, Engine, Lif, Readout, Stimulus}`, `std::time::Instant`.
No error type exists. No module structure; new code adds `pub mod indices;`
and `pub mod similarity;` (files `src/indices.rs`, `src/similarity.rs`).

Public items, signatures as written:

```rust
// :32
pub fn structural_distance(a: &Connectome, b: &Connectome) -> f32   // INFINITY on length mismatch
// :47  -- chore defect 2: .min() over channels/ticks/neurons; returns 0.0 when nothing compared
pub fn compare_behaviour(a: &Behaviour, b: &Behaviour) -> f32
// :84
pub trait Probe {
    fn stimulus(&self) -> Box<dyn Stimulus>;
    fn readout(&self, num_neurons: usize) -> Box<dyn Readout>;
    fn ticks(&self) -> u64;
    fn dt(&self) -> f32 { 1.0 }
}
// :94
pub fn run_probe<P: Probe>(connectome: &Connectome, probe: &P) -> Behaviour
// :103 -- chore defect 3: `if battery.is_empty() { return 0.0; }` at :104-106
pub fn behavioural_distance<P: Probe>(original: &Connectome, decoded: &Connectome, battery: &[P]) -> f32
// :118
pub struct ProbeBattery<P: Probe> { pub probes: Vec<P> }
//   new() :124, with(self, P) -> Self :129, len :134, is_empty :138, as_slice :143,
//   Default :148, From<Vec<P>> :154
// :163
pub struct DriveProbe { pub k: usize, pub amplitude: f32, pub motor: usize, pub ticks: u64 }
//   Default :170 (k 8, amplitude 1.5, motor 4, ticks 200); impl Probe :176
// :190
#[derive(Clone, Debug)]
pub struct CurvePoint { pub latent_dim: usize, pub behavioural_distance: f32, pub encode_ns: u128, pub decode_ns: u128 }
// :207 -- calls behavioural_distance at :222
pub fn size_fidelity_curve<P: Probe>(reference: &Connectome, codecs: &[Box<dyn Codec>], battery: &[P]) -> Vec<CurvePoint>
```

`compare_behaviour` body (`:47-78`): `channel_term` = mean squared diff over
`min(a.channels.len(), b.channels.len())` channels and per-channel `min` tick
lengths, `0.0` if `count == 0`; `rate_term` = RMS diff over
`min(a.spike_counts.len(), b.spike_counts.len())`, `0.0` if `n == 0`; returns
`channel_term + 0.01 * rate_term`. `a.ticks`/`b.ticks` are never read.

`Behaviour` (`crates/sim/src/lib.rs:121-127`), all fields pub, so a test can build
one literally:

```rust
pub struct Behaviour {
    pub channels: Vec<Vec<f32>>,   // channels[c][t]
    pub spike_counts: Vec<u32>,    // per neuron
    pub ticks: u64,
}
```

Gate E's "channel count, neuron count or tick count" mismatch maps to
`channels.len()`, `spike_counts.len()`, and `ticks` / each `channels[c].len()`
(decide whether ragged channels within one recording are also an error; record it).

Existing tests (`:233-284`, `mod tests`, `use super::*; use semblance_codec::WeightCodec;`):

| Test | Line | What it asserts | Affected by gate E |
|---|---|---|---|
| `test_battery()` helper | :240 | one `DriveProbe { k 8, amp 1.5, motor 4, ticks 150 }` | no |
| `lossless_codec_has_zero_behavioural_distance` | :245 | `behavioural_distance(..) == 0.0`, `structural_distance == 0.0` on `Connectome::synthetic(80, 5, 9)` | yes: now `Result`, unwrap/`Ok(0.0)` |
| `perturbing_weights_increases_distance` | :258 | +0.5 on every weight -> `bd > 0.0` | yes: `Result` |
| `probe_battery_collects_probes_for_behavioural_distance` | :271 | **the vacuous `== 0.0` at :282** (decoded == original) | yes: spec §4 replaces it with a non-empty battery vs a different network (positive) and vs the same network (0) |

New tests gate E needs (each must fail on today's code): busy recording vs an
empty `Behaviour` -> error (chore reproduction: 4 channels / 500 neurons vs
empty); empty battery vs a +5.0-shifted network -> error (chore reproduction:
empty battery scored 0, one probe 0.847); `size_fidelity_curve` propagates both.

Callers outside the crate (`git grep`, excluding docs):

- `app/src/main.rs:24` `use semblance_fidelity::{behavioural_distance, structural_distance, DriveProbe};`
- `app/src/main.rs:97-121` `cmd_roundtrip`: `let bd = behavioural_distance(&c, &decoded, &battery);` (`:110`),
  printed `println!("behavioural distance: {bd:.6}   (0.0 == identical behaviour)");` (`:115`). Must
  handle the `Result` and still print `0.000000` for both lines.
- `app/src/main.rs:127-139` `RateTargetFitness::score` calls `semblance_fidelity::run_probe` only
  (`:137`). `cmd_evolve` (`:142-170`) never calls `compare_behaviour` or `behavioural_distance`, so
  **evolve needs no change** unless `run_probe` changes signature (spec §4 says "roundtrip and
  evolve handle the Result"; in fact only roundtrip has one to handle).
- `size_fidelity_curve` and `compare_behaviour` have no callers outside fidelity.
- Docs naming them: `DESIGN.md:75,80,114,121`, `README.md:117-118` (prose, no tests pin them).

Tests pinning roundtrip's output (must keep passing):

- `app/tests/acceptance_validate.rs:932-940` `validate_18_roundtrip_is_still_lossless`:
  a line `== "structural distance:  0.000000"` and a line starting
  `"behavioural distance: 0.000000"`.
- `app/tests/validate_cli.rs:119-125` `roundtrip_is_still_lossless`: stdout contains both strings.
- `.github/workflows/ci.yml:128-131`: `grep -qF 'structural distance:  0.000000'` and
  `'behavioural distance: 0.000000'`.

---

## 3. `crates/sim/src/response.rs` (task 1a; 260 lines) -- what indices/similarity consume

Module path: `semblance_sim::response` (`crates/sim/src/lib.rs:24`, `pub mod response;`).

```rust
// :15
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision { F32, F64 }

// :22
#[derive(Clone, Debug, PartialEq)]
pub enum ResponseError {
    NoCellTypes,
    RepeatedCellType(String),
    BadDt(f64),
    NoFrames { stimulus: usize },
    Shape { stimulus: usize, expected: usize, found: usize },
    NonFinite { stimulus: usize, frame: usize, cell_type: String },
    EmptyBattery,
}
// Display :33-49, impl std::error::Error :51

// :55  fields private: cell_types: Vec<String>, dt: f64, precision: Precision, stimuli: Vec<StimulusResponse>
#[derive(Clone, Debug, PartialEq)]
pub struct Responses { .. }
// :66  fields private: label: String, n_frames: usize, n_types: usize, values: Vec<f64> (row-major [n_frames][n_types])
#[derive(Clone, Debug, PartialEq)]
pub struct StimulusResponse { .. }

impl Responses {
    pub fn new(cell_types: Vec<String>, dt: f64, precision: Precision) -> Result<Self, ResponseError>; // :75
    pub fn push(&mut self, label: String, n_frames: usize, values: Vec<f64>) -> Result<(), ResponseError>; // :91
    pub fn finish(self) -> Result<Self, ResponseError>;   // :119, EmptyBattery if no stimuli
    pub fn cell_types(&self) -> &[String];                // :123
    pub fn type_index(&self, name: &str) -> Option<usize>; // :128
    pub fn dt(&self) -> f64;                               // :132
    pub fn precision(&self) -> Precision;                  // :136
    pub fn len(&self) -> usize;                            // :141 number of stimuli
    pub fn is_empty(&self) -> bool;                        // :145
    pub fn stimulus(&self, s: usize) -> Option<&StimulusResponse>; // :149
}
impl StimulusResponse {
    pub fn label(&self) -> &str;                           // :155
    pub fn n_frames(&self) -> usize;                       // :159
    pub fn frame(&self, i: usize) -> Option<&[f64]>;       // :164 one value per cell type
    pub fn trace(&self, t: usize) -> Option<impl Iterator<Item = f64> + '_>; // :172 column t over frames
}
```

Facts to rely on: values are stored widened to f64 (an f32 run's values are exactly
f32-representable); row `i` is the state after `i + 1` steps from the settled state;
there is **no public constructor bypassing `push`**, so `Responses` is always
non-empty, rectangular per stimulus, finite, and has ≥1 unique cell type -- many
`IndexError` variants (non-finite, empty input) can only be reached from the
layout side or from hand-built `FriPeaks`/`EdgePeaks`/`ResponseMatrix`. The spec's
"each `ResponseError` variant has a test" is already satisfied for all 7 variants
by `response.rs` tests (`:217-250`); gate E's new-path tests are the `IndexError`
ones plus the two defect analogues (layout/shape disagreement; empty battery).

Test style there (`:180-260`): small `types()` / `two()` builders; `assert_eq!` on
the exact `Err(...)`; `errors_display_as_registered` pins `to_string()` text.

`crates/sim/src/graded.rs` (580 lines) public API, for controls:

```rust
pub trait Real: Copy + PartialOrd + Debug + Add + Sub + Mul + Div + Neg + sealed::Sealed { // :35
    const ZERO: Self; const ONE: Self;
    fn from_f32(x: f32) -> Self; fn from_f64(x: f64) -> Self; fn to_f64(self) -> f64;
    fn is_finite(self) -> bool; fn is_nan(self) -> bool;
}                                                       // impl for f32 :58, f64 :68
#[derive(Clone, Copy, Debug)]
pub struct GradedParams<'a, R: Real> {                  // :81
    pub bias: &'a [f32], pub time_const: &'a [f32], pub syn_strength: &'a [f32], pub edge_syn_count: &'a [R],
}
#[derive(Clone, Debug, PartialEq)]
pub enum GradedError {                                  // :89
    Length { what: &'static str, expected: usize, found: usize },
    NonFinite { what: &'static str, index: usize },
    BadDt(f64),
}
pub struct Graded<R: Real> { .. }                       // :113
impl<R: Real> Graded<R> {
    pub fn new(net: &TypedNetwork, params: &GradedParams<R>, dt: f64) -> Result<Self, GradedError>; // :137
    pub fn step(&mut self, input: &[R]) -> Result<(), GradedError>; // :220 never checks state finiteness
    pub fn reset(&mut self);                                         // :243
    pub fn set_state(&mut self, state: &[R]) -> Result<(), GradedError>; // :250
    pub fn state(&self) -> &[R];                                     // :262
    pub fn dt(&self) -> f64;                                         // :266
}
```

`Graded::new` checks `syn_strength.len() == net.conn_types().len()` (604) and
`edge_syn_count.len() == net.num_edges()`; weights are
`conn_sign[c] * edge_syn_count[e] * max(syn_strength[c], 0)` (`:192-201`):
**negative strengths clamp to 0**, signs belong to the connection type.

---

## 4. `crates/flyvis/src/parity/` -- respond, Battery, Fixtures, errors

`mod.rs` (207 lines): submodules `bounds, checks, fingerprint, inputs, respond, run,
validate` (`:17-23`), all private, re-exported:

```rust
pub use bounds::{settling_bounds, trace_bounds, Bound};                       // :25
pub use checks::{check_settling_gaps, check_trace_gaps};
pub use fingerprint::{libm_fingerprint, LibmFingerprint};
pub use inputs::{how_to_get, required_files, Fixtures, TraceData, TraceInputs};
pub use respond::{central_nodes, respond, respond_model, Battery, ReferenceBattery}; // :29
pub use run::{diff, play, run, settle, Counts, Diff, RunResult, SequenceResult, SettlingResult, Start};
pub use validate::{validate, Controls, Coverage, Job, Options, Report};       // :31  NB `Controls` struct exists
#[cfg(test)] pub(crate) use run::{cell_type, file, Precision, HEXALS, SNAPSHOTS}; // :34
```

The new module is `mod controls;` in this list plus its `pub use`. **Name clash to
avoid:** `parity::Controls` (validate.rs:101, the 0.3.0 controls' result struct) is
already public; a module `parity::controls` can coexist (different namespace) but
do not re-export anything named `Controls` from it.

**`controls` must be a single file `parity/controls.rs`, not a directory:**
`acceptance_parity.rs:453-467` `parity_sources()` reads every entry of `src/parity`
with `read_to_string` (a subdirectory panics), strips from
`"#[cfg(test)]\nmod tests"` on, asserts ≥7 files, and PARITY-01 (`:528-532`) bans
`SEMBLANCE_REQUIRE_FIXTURES`, `CARGO_MANIFEST_DIR`, `env!(`, `env::var`,
`current_dir` in the non-test part. The tests block must start exactly with
`#[cfg(test)]\nmod tests` for that strip to work.

Constants (`mod.rs`): `F64_BOUND = 1e-10` (:41), `SETTLING_F32_GAPS` (:47),
`TRACE_F32_GAPS` (:58), `CONTROL_MISS = 1e-3` (:66), `COVERAGE_THRESHOLD` (:70),
`RUNS: [(u32, &str, &str, Dtype); 8]` (:74).

```rust
// mod.rs:86
#[derive(Clone, Debug, PartialEq)]
pub enum ParityError {
    Missing { rel: String, path: PathBuf },
    Unreadable { rel: String, path: PathBuf, kind: std::io::ErrorKind },
    Malformed { rel: String, detail: String },
    Mismatch(String),
    NonFinite { run: u32, node: usize, cell_type: String, after: After },
    NonFiniteSettle { node: usize, cell_type: String, step: usize },
    NonFiniteResponse { stimulus: usize, label: String, frame: usize, node: usize, cell_type: String },
    Usage(String),
}
pub enum After { SettleStep(usize), Frame { frame: usize, sequence: u32 } } // :110
impl ParityError { pub fn is_gate_failure(&self) -> bool } // :120, true only for the three NonFinite*
// Display :128-149; tests errors_display_as_registered :158, respond_errors_display_as_registered :184
```

Controls' "a draw's state goes non-finite -> redraw" maps onto
`ParityError::NonFiniteSettle` / `NonFiniteResponse` (both `is_gate_failure()`).

`respond.rs` (391 lines):

```rust
pub fn central_nodes(f: &Fixtures) -> Result<Vec<usize>, ParityError>;          // :19 node at (0,0) per type, checked vs manifest
pub trait Battery {                                                              // :44
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }
    fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError>; // rows of 721 hexals
}
pub fn respond(f: &Fixtures, params: &ModelParams, dt: f64, precision: Dtype, counts: Counts,
               battery: &mut dyn Battery) -> Result<Responses, ParityError>;    // :64
pub fn respond_model(f: &Fixtures, id: &str, dt: f64, precision: Dtype, counts: Counts,
                     battery: &mut dyn Battery) -> Result<Responses, ParityError>; // :79, Usage if id unknown
const MAX_SETTLE_STEPS: f64 = 1e6;                                              // :92
fn respond_as<R: Precision>(..)                                                  // :94-153
pub struct ReferenceBattery<'a> { f, t: &'a TraceInputs<'a>, dt: String, next: usize } // :157
impl<'a> ReferenceBattery<'a> { pub fn new(f: &'a Fixtures, t: &'a TraceInputs<'a>, dt_id: &str) -> Self } // :165
// labels "{index} {family}"; reads flyvis-reference/input.f64.bin blocks per sequence
```

`respond_as` flow (what `respond_network` for the rewired control must replicate
on a different `TypedNetwork`): checks `t_pre / dt` is a whole number in 1..=1e6
(`:101-107`, else `Usage`); `central_nodes(f)` (uses `f.net` and the manifest);
`settle_core::<R>(f, params, dt, steps, counts, &[], Ctx::Respond)`; builds
`Responses` with the central types; per stimulus resets to the settled state,
`step_frame` per 721-hexal row, records `g.state()[centre[k]]`; final
`s != battery.len()` is a `Mismatch`. Everything is hard-wired to `f.net`,
`f.groups`, `f.input_nodes` -- see run.rs below.

Tests in `respond.rs` (`:206-391`): a private `Fixed` battery (`:216-233`) for error
paths; `grey(frames)`; all start `let Some(f) = fixture() else { return };`;
`reference(suffix)` caches `input.f64.bin` in a `OnceLock` and uses `or_skip`;
`reproduces::<R>(run, model)` prints `eprintln!("respond run ...: {differ} values
differ ..., largest {max:.3e}")`; control test
`another_models_responses_do_not_reproduce_run_0` asserts `max > CONTROL_MISS`.

`run.rs` (798 lines), items controls will reuse:

```rust
pub(crate) const HEXALS: usize = 721;                                    // :23
pub(crate) const SNAPSHOTS: usize = 6;                                   // :26
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counts { Torch, Libm, IntegerNSyn }                             // :30
pub enum Start { Rust, Flyvis }                                          // :41
pub struct Diff { pub max: f64, pub at: usize, pub differ: usize }       // :51
pub fn diff(rust: &[f64], flyvis: &[f64]) -> Diff;                       // :60
pub(crate) trait Precision: Real {                                       // :133
    const DTYPE: Dtype; const SUFFIX: &'static str;
    fn torch_count(group: usize) -> Self; fn libm_count(group: usize) -> Self;
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>;
}
pub(crate) const fn dtype_name(p: Dtype) -> &'static str;                // :160
pub(super) fn mismatch(m: String) -> ParityError;                        // :167
pub(crate) fn cell_type(net: &TypedNetwork, node: usize) -> &str;        // :171
pub(crate) fn file<'a>(f: &'a Fixtures, name: &str) -> Result<&'a FileInfo, ParityError>; // :176
pub(super) enum Ctx { Run(u32), Respond }                                // :300; Respond maps GradedError -> Usage("network: ..")
pub(super) fn settle_core<R: Precision>(f: &Fixtures, p: &ModelParams, dt: f64, n_settle: usize,
    counts: Counts, centre: &[usize], ctx: Ctx) -> Result<(Graded<R>, Vec<f64>), ParityError>; // :340
pub(super) fn step_frame<R: Real>(f: &Fixtures, g: &mut Graded<R>, frame: &[R], input: &mut [R])
    -> Result<Option<usize>, String>;                                    // :381 returns first non-finite node
pub fn settle(f: &Fixtures, run: u32, model: &str, counts: Counts) -> Result<SettlingResult, ParityError>; // :630
pub fn play(f, t, run, counts, start, sequences: &[u32], mirror: bool) -> Result<Vec<SequenceResult>, ParityError>; // :639
pub fn run(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts) -> Result<RunResult, ParityError>; // :658
```

`settle_core` (`:340-376`) uses `f.groups[e]` to index `torch_count`/`libm_count`
per edge, `f.net` for `Graded::new`, `f.input_nodes` for the grey input, and
`f.manifest.pre_stim_value`. `step_frame` scatters via `f.input_nodes`. **For the
rewired control** the edge list changes, so (a) the per-edge synapse-count group
must be carried from each original edge to its rebuilt edge ("the torch synapse
counts follow each edge"), not recomputed: `wiring::edge_syn_groups(&rewired)`
(`wiring.rs:67`) groups by (conn type, du, dv) and errors with
`WiringError::GroupCount { expected: 2355, .. }` if the count differs, which
dropped edges will cause; (b) `settle_core`/`step_frame`/`central_nodes` take
`&Fixtures` and read `f.net`/`f.groups`; `respond_network` needs a variant that
takes `net` and per-edge groups explicitly (e.g. refactor `settle_core` to take
`(&TypedNetwork, &[u32])`, keeping `respond`'s results bit-identical -- the
0.3.0 gates and `respond_reproduces_flyviss_centre_traces_*` re-prove that).
Input nodes and central nodes are unchanged by rewiring (nodes are not touched).

`inputs.rs` (361 lines):

```rust
pub(crate) const SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json"; // :17
pub(crate) const ENSEMBLE: &str = "flyvis-pretrained/ensemble.csv";  // :18
pub(crate) const MANIFEST: &str = "flyvis-reference/manifest.json";  // :19
pub fn required_files() -> &'static [&'static str];                   // :44, the 13
pub fn how_to_get(rel: &str) -> &'static str;                          // :49, per top dir
#[derive(Clone, Debug, PartialEq)]
pub struct Fixtures {                                                 // :99  all fields pub(crate)
    pub(crate) net: TypedNetwork,
    pub(crate) groups: Vec<u32>,              // per edge: SYN_COUNT group (2355 groups)
    pub(crate) input_nodes: Vec<Vec<u32>>,    // [8][721]
    pub(crate) ensemble: Ensemble,
    pub(crate) manifest: Manifest,
    pub(crate) settling: Vec<(&'static str, Vec<u8>)>,
}
impl Fixtures {
    pub fn load(data: &Path) -> Result<Fixtures, ParityError>;                              // :113
    pub fn from_parts(spec_json: &str, ensemble_csv: &str, manifest_json: &str,
                      settling: Vec<(&'static str, Vec<u8>)>) -> Result<Fixtures, ParityError>; // :131
    pub fn manifest(&self) -> &Manifest;      // :159
    pub fn network(&self) -> &TypedNetwork;   // :163
}
pub struct TraceData { .. }  impl TraceData { pub fn load(data: &Path) -> ..; pub fn view(&self, p: Dtype) -> TraceInputs<'_> } // :170-
pub struct TraceInputs<'a> { pub precision: Dtype, pub sequences: &'a [SequenceInfo], pub input: &'a [u8],
                             pub centre: &'a [u8], pub snapshots: &'a [u8] }              // :210
```

There is **no public accessor for the ensemble** on `Fixtures` (`f.ensemble` is
`pub(crate)`); controls live inside flyvis so they can read it directly.
`#[cfg(test)] pub(crate) fn synthetic_ensemble() -> String` exists at `inputs.rs:317`.

`ensemble.rs` (356 lines; `pub use` at `lib.rs:65`):

```rust
pub const N_MODELS: usize = 50;                        // :14
pub const ROWS_PER_MODEL: usize = 65 + 65 + 604;       // :16 (= 734)
#[derive(Clone, Debug, PartialEq)]
pub struct ModelParams { pub bias: [f32; 65], pub time_const: [f32; 65], pub syn_strength: [f32; 604] } // :21
impl ModelParams { pub fn pretrained() -> Self }       // :29 model 000 from pretrained.rs
#[derive(Clone, Debug, PartialEq)]
pub struct Ensemble { models: Vec<(String, ModelParams)> } // :36
pub struct EnsembleError { pub line: usize, pub kind: EnsembleErrorKind } // :42
pub enum EnsembleErrorKind { CarriageReturn, MissingHeader, BadHeader(String), CommentAfterHeader,
    FieldCount(usize), BadModel(String), ModelOutOfOrder{..}, BadKind{..}, BadIndex{..}, BadName{..},
    BadValue(String), TooFewRows{..}, TooManyRows{..} } // :48
impl Ensemble {
    pub fn parse(csv: &str) -> Result<Self, EnsembleError>;   // :108 exactly 50 models, every row checked
    pub fn model(&self, id: &str) -> Option<&ModelParams>;    // :186
    pub fn ids(&self) -> impl Iterator<Item = &str>;          // :191 file order "000".."049"
}
```

Random-parameters control: per-parameter min/max over `ensemble.ids()` across the
734 slots (`bias`, `time_const`, `syn_strength`), drawn with `Rng::next_f32` in
[0,1). Shuffled: permute `syn_strength` (604). `pretrained.rs`:
`CELL_TYPES: [&str; 65]` (:23), `CONN_TYPES: [(&str, &str); 604]` (:92),
`SIGN: [f32; 604]` (:806), `SYN_COUNT: [f32; 2355]` (:886),
`FREE_PARAMETER_COUNT: usize = 734` (:1186).

`reference.rs`: `pub enum Dtype { F32, F64 }` (:19, `size()`); `Manifest { n_nodes,
t_pre, pre_stim_value, dts: Vec<DtInfo>, runs: Vec<RunInfo>, centre_nodes:
Vec<CentreNode>, files, .. }` (:64); `DtInfo { id, dt, floor_dt_f32, n_settle }`
(:34); `CentreNode { node: u32, cell_type: String }` (:40); `SequenceInfo { index,
family, flyvis_index, args: Vec<(String, f64)>, frames, .. }` (:77). Map `Dtype` ->
`semblance_sim::response::Precision` as `respond.rs:113-116` does.

`validate.rs`: `Options { pub threads: usize, pub runs: Vec<u32> }` (:18). Not
touched by this epic (validate wiring is `fidelity-gates`).

---

## 5. `crates/connectome/src/typed.rs` (rewired control)

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct TypedNetworkParts {               // :18
    pub cell_types: Vec<String>, pub node_cell_type: Vec<u16>, pub node_u: Vec<i16>, pub node_v: Vec<i16>,
    pub conn_types: Vec<(u16, u16)>,         // (source type, target type)
    pub conn_sign: Vec<f32>,                 // +1.0 / -1.0
    pub edge_source: Vec<u32>, pub edge_target: Vec<u32>, pub edge_conn_type: Vec<u16>, pub edge_n_syn: Vec<f32>,
}
pub enum NetworkError { NodeArrayLengthMismatch{..}, EdgeArrayLengthMismatch{..}, ConnArrayLengthMismatch{..},
    CellTypeOutOfRange{..}, ConnTypeOutOfRange{..}, NodeIndexOutOfRange{..}, ConnEndpointOutOfRange{..},
    BadSign{..}, BufferLengthMismatch{..} }  // :43
impl TypedNetwork {
    pub fn new(parts: TypedNetworkParts) -> Result<Self, NetworkError>; // :113
    // accessors :183-210: num_nodes, num_edges, num_columns, cell_types, conn_types, conn_sign,
    // node_cell_type, node_u, node_v, edge_source, edge_target, edge_conn_type, edge_n_syn
    pub fn weights_into(&self, syn_strength: &[f32], out: &mut [f32]) -> Result<(), NetworkError>; // :216
}
```

`TypedNetwork::new` validates lengths, ranges and signs only; it does **not**
check that an edge's source node has its conn type's source cell type, nor that
(source, target) pairs are unique -- the rewired control must enforce its own
invariants ("two connection types the same (source, target) pair is redrawn") and
the proof tests must check them. Note the lib doc says flyvis has "605 connection
types" (`flyvis/src/lib.rs:5`) while the network and params have 604 -- use
`net.conn_types().len()`, never a literal.

---

## 6. `semblance-rng` (`crates/rng/src/lib.rs`, 304 lines)

```rust
#[derive(Clone, Debug)]
pub struct Rng { s: [u64; 4] }                 // :48
impl Rng {
    pub fn seed_from_u64(seed: u64) -> Self;    // :54 SplitMix64 -> xoshiro256++; no unseeded ctor
    pub fn next_u64(&mut self) -> u64;          // :65
    pub fn next_f32(&mut self) -> f32;          // :87 uniform [0,1), top 24 bits / 2^24
    pub fn below(&mut self, n: usize) -> usize; // :102 uniform [0,n), rejection; panics on n == 0
    pub fn chance(&mut self, p: f32) -> bool;   // :120
    pub fn gaussian(&mut self) -> f32;          // :131 libm-dependent; avoid in controls
}
```

Stream frozen by contract (`crates/rng/tests/vectors.rs`); use only existing
methods. `next_u64`/`next_f32`/`below` are bit-frozen on every target; `gaussian`
is not -- controls should not use it. Fisher-Yates via `below(i + 1)`. The seed
formula from (control index, model index) must be a fixed expression in code
(and later in the record). `below(0)` panics, so guard any zero-length range.

---

## 7. `crates/flyvis/src/groundtruth.rs` (310 lines) -- data the indices are fed

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polarity { On, Off, Unknown }                            // :11
pub const POLARITY: &[(&str, Polarity)];                           // :21, 65 entries, 32 known
pub const PREFERRED_DIRECTIONS: &[(&str, u16)];                    // :90, T4a 180, T4b 0, T4c 90, T4d 270, T5a..d same
pub const TUNING_CURVES: &[(&str, [f32; 12])];                     // :102, 8 (Maisak 2013)
pub const MOTION_TUNING: &[&str];                                  // :154, 8
pub const NO_MOTION_TUNING: &[&str];                               // :157, 10
pub const ON_PATHWAY: &[&str]; pub const OFF_PATHWAY: &[&str];     // :162, :167, 12 each
pub const INSUFFICIENT_DATA: &[&str];                              // :172, 9 -- doc says "exclude from metrics"
pub const NOISY_DATA: &[&str];                                     // :177, 4
pub const KNOWN_DSI_TYPES: &[&str];                                // :180, 18 (union of the two motion lists)
```

fidelity cannot name `Polarity` (no flyvis dep), so the spec's
`polarity_agreement(.., known: &[(&str, i8)])` takes +1/-1 (Unknown omitted);
the conversion lives on the flyvis side (or in the later gates epic). Task 0:
`INSUFFICIENT_DATA` and `NOISY_DATA` filter no metric in flyvis, despite the doc
comment at `:171`. Tests there: `table_sizes_match_upstream` (:196) pins all sizes.

---

## 8. Patterns new code must follow

**Error enums.** `#[derive(Clone, Debug, PartialEq)] pub enum XError { .. }`,
struct-like variants carrying the indices that locate the fault, a hand-written
`impl std::fmt::Display` (one short lowercase sentence, no trailing period), and
`impl std::error::Error for XError {}`. One test named
`errors_display_as_registered` pins representative `to_string()` outputs
(`response.rs:252`, `parity/mod.rs:158`). Errors are refusals, never scores; nothing
in a library path panics on input (parity's module doc, `mod.rs:7-8`). New:
`FidelityError` (legacy, gate E), `IndexError` (indices), `SimilarityError`
(similarity). Converting into `ParityError` follows `respond.rs`:
`.map_err(|e| mismatch(e.to_string()))` or `ParityError::Usage(..)`.

**Test naming.** `snake_case` full sentences stating the behaviour:
`push_refuses_empty_ragged_and_non_finite_stimuli`,
`an_empty_battery_does_not_finish`,
`a_dt_that_does_not_divide_t_pre_is_a_usage_error`,
`another_models_responses_do_not_reproduce_run_0` (controls prove a check can
fail). Unit tests live in `#[cfg(test)]\nmod tests { use super::*; ... }` at the
bottom of each file. Doc comments on helpers say what they build.

**Fixture-gated tests (flyvis only; fidelity has none and must need none).**

- `crate::gate_support::fixture() -> Option<&'static Fixtures>`
  (`gate_support.rs:30`): `OnceLock` caching the *miss*, not the notice; every
  caller of a miss goes through `or_skip`, which prints `skipping: <notice>`. A
  present-but-malformed fixture panics (never skips).
- Pattern: `let Some(f) = fixture() else { return };` as the first line.
- Extra files: `test_support::{or_skip, read_fixture_bytes, read_fixture_text}`
  (`test_support.rs:122,129,136`); `read_fixture_*` return `Err(notice)` without
  printing; under `SEMBLANCE_REQUIRE_FIXTURES=1` a missing file panics
  (`missing()` `:40-43`). Cache big reads in a `static OnceLock<Result<Vec<u8>, String>>`
  as `respond.rs:304-317` does.
- Every fixture-gated test must print its own notice (CLAUDE.md skip paragraph;
  enforced for `tests/acceptance_traces.rs` and `tests/acceptance_settling.rs` by
  `every_fixture_gated_acceptance_test_prints_its_own_skip_notice`,
  `acceptance_traces.rs:1629`). A test that skips via two different missing fixtures
  prints two notices.
- Gates print their numbers (`eprintln!` with ratios in `{:.3e}`); read them.
- Controls' fixture needs: fixtures-2 only (`ensemble.csv`, `flyvis-reference/*`,
  the spec). Tests that play a battery should use `ReferenceBattery` (12
  sequences) or a tiny synthetic `Battery`, not fixtures-3. A full 50-model x 4
  controls run is the gates epic; this epic's tests run a handful of draws.

**Float comparisons.** Exact where exactness exists. Spec proof bounds:
`hsic_unbiased` vs a test-only direct Gram implementation (n <= 50) within 1e-12
relative; `cka(x, x) == 1.0` and `cka(a, b) == cka(b, a)` bit for bit; CKA
invariances to 1e-12. No `mul_add`, no fast-math (CLAUDE.md, TRACES-23).

**Commits:** plain imperative subject, no `feat:`-style prefix.

---

## 9. Counts and docs this epic will change

Current values in the worktree (`CLAUDE.md:133-163`, already bumped by task 1a's
`5d68886`; the main checkout's CLAUDE.md still says 570/174):

- `cargo test --workspace          # 587 tests, 0 failures` (`:133`)
- `184 of those tests skip when ...` (`:139`)
- `"587 passed, 0 failed"` (`:149`)
- `178 of the 184 print a notice, 192 notices in all` (`:159`)
- `every one of the 184 fails` (`:163`)
- `README.md:68` "Without fixture data, 184 flyvis parity tests skip ..."

Tests that hold these:

- `app/tests/acceptance_release.rs:711-716` `claude_counts()` parses the number
  before `" tests, 0 failures"` and before `" of those tests skip"`.
- RELEASE-11 (`:806-816`): `tests >= written_tests()` (count of literal `#[test]`
  lines under `app/` and `crates/`, `:723-745`), and CLAUDE.md must contain
  `"\"{tests} passed, 0 failed\""` and `"every one of the {skips} fails"`.
- RELEASE-10 (`:788-792`): README's skip count == CLAUDE.md's; `!= 123`.
- So every new `#[test]` must be followed by re-measuring and updating all five
  CLAUDE.md numbers plus README's (pure fidelity tests raise the total but not
  the skip count; new fixture-gated flyvis tests raise both, and the notice
  counts). Re-measure on the build host, not by arithmetic alone.

Other text/tests possibly touched:

- `crates/fidelity/Cargo.toml:13` description (unpinned; update to drop "placeholder"
  if desired -- record the decision).
- `crates/fidelity/src/lib.rs:1-19` module doc lists DONE/YOURS items; add the new modules.
- `crates/flyvis/src/lib.rs:13-21` parity doc paragraph is checked by RELEASE-09
  (`acceptance_release.rs:669`, `crate_docs("crates/flyvis/src/lib.rs")`): read that
  test before editing the flyvis crate doc. It requires a `# Licensing` section
  naming `pretrained`, `syn_count_torch`, `groundtruth`, and a "[`parity`]"
  paragraph containing "`semblance validate`" within 600 chars. It also pins
  `pretrained.rs` and `syn_count_torch.rs` by git blob hash: never edit them
  (read `CELL_TYPES`, `CONN_TYPES`, `SIGN` etc. from them, do not add to them).
- `semblance-flyvis` licence/NOTICE unchanged (no new flyvis data compiled in).
- `README.md:95` crate-table row for `fidelity` (RELEASE-10 checks only the
  `flyvis` and `app` rows).

---

## 10. Things to decide and record (found while mapping)

1. Where the rewired network's per-edge synapse-count groups come from (carry the
   original edge's group; do not call `edge_syn_groups` on the rewired net).
2. How `respond_network` shares code with `respond` without changing `respond`'s
   output (refactor `settle_core`/`step_frame`/`central_nodes` to take the network
   and groups explicitly; the existing exact tests re-prove it).
3. Whether ragged `channels` inside one `Behaviour`, or `ticks` disagreeing with
   `channels[c].len()`, count as gate E mismatches.
4. Whether fidelity gets `opt-level = 3` in the dev profile (CKA over ~125k rows
   is slow at opt-level 0; this epic's tests can stay small instead).
5. `polarity_agreement`'s `known` uses `i8` (+1/-1) since fidelity cannot name
   `groundtruth::Polarity`; zero FRI and `Unknown` handling per task 0.
6. Seed formula for (control, model, redraw) -- fixed in code; redraws use "the
   next seed in its sequence", up to 10.
