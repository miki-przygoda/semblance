# The flyvis parity record, `semblance validate`, and the 0.3.0 release

**Date:** 2026-10-29
**Status:** draft. The owner's decisions of 2026-10-29 (D1–D5) are binding and
are recorded as rulings R1–R5. R6–R15 are decided here under the owner's
delegation ("decide the small and medium items"). Three questions remain the
owner's; they are listed at the end.
**Epic:** 0.3.0 graded dynamics, task 7 (piece 3 of 3) plus the release —
`docs/project-outline/0.3.0-graded-dynamics/` (not in git)
**Builds on:**
- [`2026-10-28-graded-settling-design.md`](2026-10-28-graded-settling-design.md):
  piece 1, the integrator, the readers, and the settling gates
- [`2026-10-28-stimulus-traces-design.md`](2026-10-28-stimulus-traces-design.md):
  piece 2, the input mapping and the trace gates

**Research:** `.temper/run-0.3.0/research/piece3-validate.md`,
`piece3-packaging.md` and `piece3-ci-time.md` (local, 2026-10-29). Every fact
below that this spec relies on was re-checked against the code at `a174645`.

## Why

Pieces 1 and 2 proved, inside `cargo test`, that this workspace reproduces
flyvis bit for bit. That proof is invisible to anyone who does not read test
output under `--nocapture`, and it is written down nowhere a reader of 0.5.0's
numbers would look. The epic's task 7 calls the record "the epic's actual
deliverable: the artifact a reader consults when asking why they should
believe 0.5.0's numbers".

This piece makes the proof runnable and readable. It adds a command that runs
the gates on any machine that has the fixtures, a committed record of what
that command printed, a test that keeps the two the same, and it then ships
0.3.0.

## Goal

1. **`semblance_flyvis::parity`.** A public library module that holds what the
   gates do: reading the fixtures from a data directory, settling, trace
   playback, comparison, the pre-registered bounds, the controls, and the
   coverage measure. It returns typed results and errors and never panics on
   input.
2. **The gates become thin wrappers** over `parity`. Their names, prints and
   panic messages stay byte for byte as they are.
3. **`semblance validate [--data DIR] [--threads N] [--markdown]`.** It runs
   every gate and control and prints one table. Exit code 0 means every gate
   holds, 1 means a gate failed, 2 means nothing was verified.
4. **The record, `docs/results/0.3.0-flyvis-parity.md`.** A hand-written
   preamble followed by `validate --markdown`'s output. A fixture-gated test
   regenerates the deterministic part and compares it with the committed file.
5. **CI time.** The four reductions P1–P4.
6. **The 0.3.0 release.** The version bump, packaging, licence files, docs,
   and the tag. Publishing to crates.io and yanking 0.1.0 happen only after
   the owner says go.

## Facts the design rests on

| # | Fact | Evidence |
|---|---|---|
| F1 | The CLI is one file, `app/src/main.rs`, with hand-rolled std-only parsing. No command takes flags, and every command returns `ExitCode::SUCCESS`. An unknown command returns `FAILURE`. The help text ends "Everything runs on a synthetic graph so it works with no data". The module doc says "Swap in `clap` if/when you want richer flags", which contradicts the zero-dependency rule. | `app/src/main.rs:10-11, 26-57, 86, 112, 161` |
| F2 | `app` does not depend on `semblance-flyvis`. Its description reads "simulate, round-trip, and evolve". | `app/Cargo.toml` |
| F3 | The release profile has `panic = "abort"`. A panic in a release `validate` kills the process before any summary prints. | `Cargo.toml` `[profile.release]` |
| F4 | All the gate machinery is `#[cfg(test)]`: `gate_support.rs` (fixture, `Precision`, `Counts`, `settle_state`, `settle`, `read`, `Diff`, `diff`, `F64_BOUND`), the settling bounds and reporters, and the trace loaders, `play`, the bounds and the reporters. Its core reports bad input with `assert!`, `expect` and `panic!`. | `lib.rs:17-41`; `gate_support.rs:22, 47-65, 123-191`; `settling_checks.rs:38-116`; `trace_checks.rs:48-307` |
| F5 | `test_support::data_path` is `env!("CARGO_MANIFEST_DIR")/../../data`, a compile-time checkout path. `missing` panics under `SEMBLANCE_REQUIRE_FIXTURES=1`. `how_to_get` holds the per-directory hints. | `test_support.rs:25-71` |
| F6 | The public API is already enough to settle and play a run. The acceptance tests do it independently of the gate code. | `acceptance_settling.rs:26-35`, `acceptance_traces.rs:28-38` |
| F7 | `semblance-sim` is only a dev-dependency of `semblance-flyvis`. sim depends only on connectome, so making it a normal dependency creates no cycle and no new package. | `crates/flyvis/Cargo.toml`, `crates/sim/Cargo.toml` |
| F8 | The dev profile builds `semblance-sim` and `semblance-flyvis` at `opt-level = 3`. A generic integrator is monomorphised in the crate that instantiates it, so a non-generic entry point in flyvis runs at opt-level 3 even when called from an unoptimised `app`. | `Cargo.toml`; graded-settling spec, "Running time" |
| F9 | Measured 2026-10-29 on a 16-core arm64 Mac, fixtures present: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- --nocapture --test-threads=8 settling_checks:: trace_checks::` gave 42 passed in 17.09 s. The research measured the same 32 gates serially at 111.5 s. | this session; piece3-validate.md §3 |
| F10 | The results of that run (F9): all 8 exact settling runs and all 8 exact trace runs are bit-equal. The libm maxima are in [§6](#6-the-terminal-output), example table. The worst f32 ratio is 8.605e-1 (trace run 4, sequence 3, snapshots, frame 359, Mi4: 8.801e-5 against 1.023e-4). The worst f64 ratio is 3.121e-3 (trace run 7). The mirrored-mapping control reads snapshots 2.503223408264881, centre 2.291526948382731. | this session's log |
| F11 | **Coverage, measured from the reference** (scratch Python over `data/flyvis-reference/`; by F10 these equal the values from Rust's exact traces). Movement per cell type = max over the 12 sequences and every frame of \|centre − settled centre\|. In every run the weakest type is Tm30, in sequence 5: runs 0/1 3.801e-3, runs 2/3 3.773e-3, runs 4/5 3.248e-2, runs 6/7 3.179e-2. The next weakest are Mi11 1.342e-2 (runs 0/1) and 1.796e-2 (runs 2/3), and L2 4.600e-2 (runs 4–7). The medians are 0.497, 0.708, 0.798 and 0.896. Against the f32 libm centre bound, the weakest type's movement is 1102×, 744×, 625× and 462× in runs 0, 2, 4 and 6. Against the f64 bound it is 3.8e7× to 3.2e8×. | this session |
| F12 | `PROVENANCE.json` has these top-level keys: `exported`, `export_id`, `generator`, `git`, `versions`, `machine`, `uv_lock_sha256`, `device`, `threads`, `flyvis_root`, `models`, `validation_losses`, `inputs_read`, `dts`, `settling`, `stimuli`, `f32_f64_gap`, `freeze_check`, `parameter_equality`, `equivalence`, `input_rounding`, `sizes`, `wall_time_s`. The JSON reader is private and keeps object keys in file order. | the file; `json.rs:7-15, 351-411` |
| F13 | The acceptance tests run the crate's gates by name in shadow workspaces and match their output. The constraining strings are listed in [§2](#2-the-gates-become-wrappers). `settling_checks::` must hold exactly 20 tests. MINORS-12 reads the doc comment of `the_gate_resolves_integer_counts` from `settling_checks.rs`. MINORS-03 requires each gate's skip notice to start `skipping: <root>/crates/flyvis/../../data/<rel>`. MINORS-05 fails any `.rs` under `crates/` that spawns a process other than cargo. | `acceptance_traces.rs:1095-1186`; `acceptance_minors.rs:418-447, 536-556, 641-654, 713` |
| F14 | The shadow `copy_tree`s copy directories and regular files only, so a symlink is skipped. | `acceptance_settling.rs:297-313` and its siblings |
| F15 | No package contains LICENSE or NOTICE. `include = ["../../LICENSE"]` is dropped without a warning. Tracked symlinks are dereferenced into the `.crate`. flyvis compiles in flyvis's MIT data. NOTICE omits `syn_count_torch.rs`. From an extracted package, the flyvis and sim `tests/` cannot run or skip silently. `exclude = ["tests/"]` works, with one warning per file. | piece3-packaging.md §1, §2, §4 |
| F16 | `[workspace.dependencies]` pins `version = "0.2.0"` for all 7 libraries, so the bump must change all 7 as well. `cargo publish --workspace` publishes in the order rng, connectome, codec, sim, evo, fidelity, flyvis, semblance. | `Cargo.toml`; piece3-packaging.md §3 |
| F17 | crates.io has six crates at 0.1.0, none yanked. evo 0.1.0 depends on `rand ^0.10`. Yanking codec 0.1.0 alone strands evo, fidelity and semblance 0.1.0. | piece3-packaging.md §6 |
| F18 | The hosted CI test job took 12m32s of its 20-minute limit. The test step took 689 s. `acceptance_minors` (flyvis) took 257 s, almost all of it MINORS-15's serial re-run of `trace_checks::` (102.8 s locally). `acceptance_traces` took 253 s, of which TRACES-06's eight one-at-a-time nested runs were about 51 s locally. SEQUENCES-08's nested cargo duplicates tests the suite already runs. The `target` cache costs a 25 s upload and never saves a rebuild. Cargo runs test binaries one after another, so a new binary adds its full wall time. | piece3-ci-time.md §1–§5 |
| F19 | Doc drift: CLAUDE.md says 332 tests and 66 skips, while the stimulus-traces PR body says 429 and 123. README says 123 and names SEQUENCES-08 as the one silent skip. README's flyvis row says "the result record (0.3.0, in progress)". `CITATION.cff` says `version: 0.2.0`. The sim description says "LIF baseline". The stimulus-traces spec's H12 says Tm30 moves 3.747e-3 in run 3; F11 measures 3.773e-3. | the files |

## Rulings

**R1 (D1): `validate` runs the gates itself, through `semblance_flyvis::parity`.**
- The module reads the fixtures from a data directory `&Path`, never a
  checkout-relative path.
- It settles, plays the traces, compares, and applies the pre-registered
  bounds as they are written. A PROVENANCE value that disagrees with a
  registered gap is a fixture error (exit 2). It is never a reason to
  recompute a bound.
- Its entry points are non-generic (F8), and it returns `Result` and never
  panics on input (F3).
- Threads are optional (`std::thread::scope`), and their results are bit-identical to a
  serial run.
- The `#[cfg(test)]` gates become thin wrappers over it, and every string the
  acceptance tests match stays byte for byte as it is.
- `semblance-sim` becomes a normal dependency of `semblance-flyvis`.
  `Cargo.lock` stays at 8 packages.

**R2 (D2): the record is `docs/results/0.3.0-flyvis-parity.md`.** It is a
hand-written preamble followed by `semblance validate --markdown`'s output.
A fixture-gated test regenerates the output and compares it byte for byte
with the committed file, except in the host-specific section (R9).

**R3 (D3): piece 3 ends 0.3.0.**
- Bump to 0.3.0, and apply the packaging fixes from piece3-packaging.md.
- After the merge the head pushes an annotated `v0.3.0` tag.
- `cargo publish --workspace` runs only on the owner's explicit go-ahead at
  that moment, from the tagged commit. The six 0.1.0 releases are yanked after
  that.
- There is no GitHub Release.

**R4 (D4): the CI cuts are P1–P4 exactly.** Every negative control stays. No
comparison, bound, `TOLERANCE` or `==` changes.

**R5 (D5): the docs to change.**
- README: the headline.
- CLAUDE.md: the version, the re-measured counts, `validate`, and the yank
  sentence.
- The drift in F19.
- The head edits the local outline. It is not a task in this spec.

**R6: formatting lives in `app`, computation in `parity`.** The library
returns structs. The terminal table and the Markdown are `app` code
(`app/src/validate.rs`). The gate wrappers keep their own format strings,
moved unchanged. So a change to the CLI's layout cannot move an acceptance
string, and the reverse holds too.

**R7: exit 2 means "nothing was verified".** That covers missing, unreadable,
malformed or mismatched fixtures. It also covers a bad `validate` command
line: an unknown flag, a missing value, or `--threads 0`. In none of these
cases did any gate run, which is the distinction a script needs. An unknown
*command* keeps today's behaviour, exit 1 (F1).

**R8: the coverage threshold is 1e-3, absolute.** It is recorded, not a gate.
- 1e-3 is the threshold the existing controls already use for "misses by far
  more than any bound" (`the_gate_tells_models_apart`,
  `the_trace_gate_sees_a_wrong_mapping`). It is 3.5× the largest bound applied
  anywhere (2.847e-4, run 6 snapshots).
- D2's wording, "every type moves ≥ ~3.7e-3, i.e. > 1000× the applicable
  bound", holds against the f64 bound and against run 0's f32 bound. It does
  not hold against the f32 libm centre bounds of runs 2, 4 and 6, which give
  744×, 625× and 462× (F11).
- The record therefore states the absolute threshold and prints each run's
  ratio, so no reader takes "1000×" on trust. For the exact gate, whose bound
  is equality, any non-zero movement is visible.
- Owner question Q1.

**R9: how the record stays honest.**
- The generated block has three kinds of section:
  - **deterministic**, compared byte for byte everywhere
  - **host-specific**, never compared: the semblance version, the target,
    debug assertions, the thread count, and the libm fingerprint
  - **libm**, compared only when the running host's libm fingerprint equals
    the one recorded in the section's marker
- Rust's `f32::exp` and `f64::exp` call the platform's libm. So the libm
  maxima are facts about one host, like `gaussian()` in `semblance-rng`.
- Everything else is IEEE arithmetic on fixed inputs and is the same on every
  host: the exact rows, the controls, coverage, the reference provenance, and
  the bounds.
- Owner question Q2.

**R10: no rustc version, no date in the generated block.**
- Neither is available without a build script, and a build script that shells
  out is what the stimulus-traces review removed.
- The host section uses `std::env::consts::{ARCH, OS}` plus `target_env` via
  `cfg!`, and `cfg!(debug_assertions)`.
- The preamble states by hand the toolchain and the date the record was made.

**R11: `validate` does not hash.** Hashing would need a SHA-256 implementation
in a workspace with no external dependencies (the same ruling as piece 1).
- `validate` checks each reference file's length against both the manifest and
  PROVENANCE `sizes`.
- `ci/fetch-fixtures.sh` stays the integrity check.
- The preamble names `fixtures-2` and `ci/fixtures.sha256`.

**R12: with no data, `validate` verifies nothing and says so.** It lists every
missing path with its `how_to_get` hint and points to the committed record. It
exits 2. It never prints the record as if it were a result.

**R13: `how_to_get` moves into `parity` with its text unchanged.**
`test_support` calls it, so the notices stay byte-identical (MINORS-03,
SETTLING-11, TRACES-15), and `validate`'s messages and the tests' notices
cannot drift apart.

**R14: the controls in `validate` reuse the tests' thresholds**, from the same
`pub const`s:
- the wrong model: settled max > 1e-3
- integer counts: settled max > `F64_BOUND`
- the mirrored mapping: snapshots max > 1e-3
- the start from flyvis's settled state: bit-equal

A control that does not hold is a gate failure (exit 1).

**R15: `app/tests/` is excluded from the `semblance` package**, as `tests/` is
for flyvis and sim (F15). The new app tests read the workspace's `docs/` and
`data/`, which no package carries.

## Design

### 1. `semblance_flyvis::parity`

A new file, `crates/flyvis/src/parity.rs`, declared `pub mod parity;` in
`lib.rs`. It imports `semblance_sim::graded`.

**Constants.** These are the only copies; the gate wrappers import them.

```rust
pub const F64_BOUND: f64 = 1e-10;
/// (model, dt id, settled, settling_centre): PROVENANCE `f32_f64_gap`, as registered.
pub const SETTLING_F32_GAPS: [(&str, &str, f64, f64); 4] = [/* moved verbatim from settling_checks.rs:38-43 */];
/// (model, dt id, centre, snapshots), as registered.
pub const TRACE_F32_GAPS: [(&str, &str, f64, f64); 4] = [/* moved verbatim from trace_checks.rs:48-53 */];
/// "Misses by far more than any bound": the model and mapping controls.
pub const CONTROL_MISS: f64 = 1e-3;
/// Sintel revisit trigger 1: a cell type moving less than this is "undriven". Recorded, not a gate (R8).
pub const COVERAGE_THRESHOLD: f64 = 1e-3;
/// The eight registered runs: (index, model, dt id, precision).
pub const RUNS: [(u32, &str, &str, Dtype); 8] = [/* as in reference_runs_are_the_eight_expected */];
```

**Bounds.**

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bound {
    /// rust == flyvis for every value.
    Exact,
    /// max |rust - flyvis| <= 1e-10.
    F64,
    /// max |rust - flyvis| <= 2.0 * gap; `gap` and its PROVENANCE key ride along for the record.
    F32 { gap: f64, key: &'static str /* "000 1/200" */, quantity: &'static str },
}
impl Bound { pub fn value(&self) -> f64; /* 0.0, 1e-10, 2.0 * gap */ }

pub fn settling_bounds(model: &str, dt: &str, p: Dtype, counts: Counts) -> Result<(Bound, Bound), ParityError>; // (settled, settling_centre)
pub fn trace_bounds(model: &str, dt: &str, p: Dtype, counts: Counts) -> Result<(Bound, Bound), ParityError>;    // (centre, snapshots)
pub fn check_settling_gaps(p: &Provenance) -> Result<(), ParityError>;  // bit-for-bit, all 8 values
pub fn check_trace_gaps(p: &Provenance) -> Result<(), ParityError>;
```

`Counts::Torch` gives `Exact`. `Libm` gives `F64` or `F32`. `IntegerNSyn` is
used only by a control, which has its own threshold, so asking for its bound
is an error. The multiplication by 2.0 is the existing one (exact in binary).

**Inputs.** File reading is separate from parsing, so the test wrappers keep
`test_support`'s reads and notices, and `validate` reads from a `&Path`:

```rust
pub struct Fixtures { /* net, groups, input_nodes, ensemble, manifest, the 4 settling files' bytes */ }
impl Fixtures {
    /// Reads, in today's order: ensemble.csv, manifest.json, the 4 settling files, the spec.
    pub fn load(data: &Path) -> Result<Fixtures, ParityError>;
    /// Pure: no I/O. What the test fixture and `load` both call.
    pub fn from_parts(spec_json: &str, ensemble_csv: &str, manifest_json: &str,
                      settling: Vec<(&'static str, Vec<u8>)>) -> Result<Fixtures, ParityError>;
    pub fn manifest(&self) -> &Manifest;
    pub fn network(&self) -> &TypedNetwork;
}

/// The trace files, all of them: what `validate` reads.
pub struct TraceData { /* sequences, input.f64, centre.f32/.f64, snapshots.f32/.f64 bytes */ }
impl TraceData {
    pub fn load(data: &Path) -> Result<TraceData, ParityError>;
    pub fn view(&self, p: Dtype) -> TraceInputs<'_>;
}
/// One precision's trace inputs, borrowed: the test wrappers build this from their lazy `OnceLock`s (traces_13).
pub struct TraceInputs<'a> { pub sequences: &'a [SequenceInfo], pub input: &'a [u8], pub centre: &'a [u8], pub snapshots: &'a [u8] }

/// The sizes `validate` checks before any arithmetic (R11).
pub fn required_files() -> &'static [&'static str];   // the 13 paths under the data dir
pub fn how_to_get(rel: &str) -> &'static str;          // moved verbatim from test_support.rs:38-63 (R13)
```

`Fixtures::from_parts` replaces today's `expect`s (`gate_support.rs:55-61`)
with errors. Neither constructor reads `PROVENANCE.json`: the gate fixture does
not depend on it today (MINORS-03's `PROVENANCE_ONLY`, SETTLING-11), and that
stays so. `validate` reads it separately.

**Provenance**, in `reference.rs`, keeping `json` private:

```rust
pub struct Provenance {
    pub exported: String, pub export_id: String,
    pub generator_path: String, pub generator_sha256: String,
    pub git_head: String, pub git_dirty: bool,
    pub versions: Vec<(String, String)>,              // file order
    pub platform: String, pub machine: String, pub device: String,
    pub uv_lock_sha256: String,
    pub threads_default: u32, pub threads_checked: Vec<u32>, pub threads_bit_equal: bool,
    pub models: Vec<ProvenanceModel>,                 // id, name, rank, validation_loss, chkpt_00000_sha256
    pub all_checkpoints_equal_ensemble: bool,         // parameter_equality
    pub stimulus_response_bit_equal: bool, pub equivalence_sequences: Vec<u32>,
    pub input_rounding: Vec<(String, f64)>,           // per dt id
    pub gaps: Vec<GapEntry>,                          // f32_f64_gap: key, settled, settling_centre, centre, snapshots
    pub sizes: Vec<(String, u64)>,
}
impl Provenance { pub fn parse(json: &str) -> Result<Provenance, ReferenceError>; }
```

The parser is strict about these fields and ignores the rest (F12). A missing
or mistyped field is a `ReferenceError` that names it.

**Running.** The entry points are non-generic. Each dispatches to a private
generic over `Precision`, chosen by the run's manifest precision, so
monomorphisation happens in flyvis (F8).

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counts { Torch, Libm, IntegerNSyn }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start { Rust, Flyvis }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diff { pub max: f64, pub at: usize, pub differ: usize }   // moved from gate_support; NaN -> INFINITY kept
pub fn diff(rust: &[f64], flyvis: &[f64]) -> Diff;                  // callers check lengths; see below

#[derive(Clone, Debug, PartialEq)]
pub struct SettlingResult {
    pub run: u32, pub model: String, pub counts: Counts,
    pub settled: Diff,                  // over [45669]
    pub settling_centre: Diff,          // over [n_settle, 65]
    pub settled_len: usize, pub centre_len: usize,
    pub first_centre_difference: Option<usize>,   // index into [n_settle, 65]
    pub per_type_max: Vec<f64>,         // settled max |error| per cell type, in cell-type order
}
impl SettlingResult { pub fn worst_types(&self, n: usize) -> Vec<usize>; } // today's stable sort, settling_checks.rs:98-100

#[derive(Clone, Debug, PartialEq)]
pub struct SequenceResult {               // today's `Played`, same fields
    pub sequence: u32, pub family: String, pub n_frames: usize, pub snapshot_frames: Vec<usize>,
    pub centre: Diff, pub snapshots: Diff, pub first: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunResult {
    pub settling: SettlingResult,
    pub sequences: Vec<SequenceResult>,   // all 12
    pub centre_movement: Vec<f64>,        // per centre type: max over sequences and frames of |rust centre - rust settled centre|
}

pub fn settle(f: &Fixtures, run: u32, model: &str, counts: Counts) -> Result<SettlingResult, ParityError>;
pub fn play(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts, start: Start,
            sequences: &[u32], mirror: bool) -> Result<Vec<SequenceResult>, ParityError>;
/// Settle once, then play all 12 sequences from that state: both stages from one settle.
pub fn run(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts) -> Result<RunResult, ParityError>;
```

The steps, their order of operations, the zeroed input buffer, the
non-finite scan after every step, and the shape checks are piece 1's and
piece 2's, moved without change. `diff` keeps its length `assert_eq!` as an
internal invariant. Every caller compares only arrays whose shapes it has
already checked, so bad input reaches the shape check first and is returned as
an error.

**Validating.**

```rust
pub struct Options { pub threads: usize /* >= 1 */, pub runs: Vec<u32> /* default 0..8 */ }

pub struct Report {
    pub provenance: Provenance,
    pub jobs: Vec<Job>,                  // runs x [Torch, Libm], in (run, counts) order
    pub controls: Controls,
    pub coverage: Vec<Coverage>,         // one per run, from its Torch job
    pub libm: LibmFingerprint,
    pub threads: usize,
}
pub struct Job { pub run: u32, pub model: String, pub dt: String, pub precision: Dtype, pub counts: Counts,
                 pub settling_bounds: (Bound, Bound), pub trace_bounds: (Bound, Bound),
                 pub outcome: Result<RunResult, ParityError> }
pub struct Controls {
    pub wrong_model: Diff,               // settle run 3 (000, 1/50, f64) with 049's parameters, torch counts
    pub integer_counts: Diff,            // settle run 3 with edge_n_syn
    pub mirrored: SequenceResult,        // run 3, sequence 8, mirrored hexals
    pub from_flyvis: SequenceResult,     // run 3, sequence 8, Start::Flyvis
}
pub struct Coverage { pub run: u32, pub movement: Vec<f64> /* per centre type */ }
pub struct LibmFingerprint { pub f32_differ: usize, pub f64_differ: usize, pub f32_fnv: u64, pub f64_fnv: u64 }

impl Job { pub fn passed(&self) -> bool; }       // exact: every differ == 0; libm: every max <= bound value
impl Controls { pub fn passed(&self) -> bool; }  // R14's four thresholds
impl Report { pub fn passed(&self) -> bool; }

pub fn validate(data: &Path, opts: &Options) -> Result<Report, ParityError>;
pub fn libm_fingerprint() -> LibmFingerprint;
```

`validate` works in four steps:

1. **Presence.** It stats all 13 required files and returns
   `ParityError::Missing` for the first absent one. The app reports every
   absent one: it stats them itself through `required_files()`, so a user
   fixes them all in one go.
2. **Fixture checks.** These run before any arithmetic, and each failure is a
   fixture error:
   - the files parse
   - the manifest's runs equal `RUNS`
   - the centre nodes match the network's cell types, in `CELL_TYPES` order
   - there are 12 sequences, indices 0–11, each with 6 snapshot frames at
     every dt
   - every block the jobs will read has the shape they expect
   - each file's byte length equals both its manifest `bytes` and
     PROVENANCE `sizes`
   - `check_settling_gaps` and `check_trace_gaps` pass
3. **Jobs.**
   - The 2 × `runs.len()` jobs run in `std::thread::scope`, on
     `min(threads, jobs)` workers. Each worker takes the next job index from
     an `AtomicUsize` and writes its result into that job's slot.
   - Each job is single-threaded and reads only shared, immutable inputs, so a
     result does not depend on which thread ran it or when.
   - Workers are started with `thread::Builder::spawn_scoped`. If spawning
     fails, the remaining jobs run on the calling thread. Nothing panics.
   - A job's `NonFinite` error is stored in its `outcome`, and the other jobs
     carry on.
4. **Controls and coverage.**
   - Four small jobs, run on the same pool after the main ones: two settles at
     1/50 and two one-sequence plays at 1/50.
   - Coverage is read from the Torch jobs' `centre_movement`.

**Libm fingerprint.** For each precision it gives:
- how many of the 2,355 `SYN_COUNT` groups have Rust's `exp` differing from
  torch's table (G17 measured 104–106 in f32 and 108 in f64 on this Mac)
- a 64-bit FNV-1a over the little-endian bits of all 2,355 libm counts, in
  group order. FNV-1a is five lines, hand-written.

It identifies "the same libm" for R9. It is not a security hash.

**Errors.**

```rust
pub enum ParityError {
    Missing { rel: String, path: PathBuf },
    Unreadable { rel: String, path: PathBuf, kind: std::io::ErrorKind },
    Malformed { rel: String, detail: String },        // wraps SpecError, ExpandError, WiringError, EnsembleError, ReferenceError text
    Mismatch(String),
    NonFinite { run: u32, node: usize, cell_type: String, after: After },
    Usage(String),                                    // a bad Options value (threads 0, an unknown run)
}
pub enum After { SettleStep(usize), Frame { frame: usize, sequence: u32 } }
impl ParityError { pub fn is_gate_failure(&self) -> bool; }  // true only for NonFinite
impl std::fmt::Display for ParityError { /* table below */ }
impl std::error::Error for ParityError {}
```

| Variant | Display | Exit |
|---|---|---|
| `Missing` | `{path} not found. {how_to_get(rel)}` | 2 |
| `Unreadable` | `{path}: cannot be read ({kind:?})` | 2 |
| `Malformed` | `{rel}: {detail}` | 2 |
| `Mismatch` | the message, from the catalogue below | 2 |
| `NonFinite` | `run {run}: node {node} ({cell_type}) is not finite after settle step {k}`, or `… after frame {i} of sequence {s}` | 1 |
| `Usage` | the message | 2 |

The `Mismatch` messages each keep, as a substring, the text today's `assert!`
carried. This is because SETTLING-05 and TRACES-05 match those substrings:

- `manifest runs must be listed in index order`
- `run {run}'s precision: manifest {a}, expected {b}`
- `dt {id}: manifest {x}, expected {y}`
- `settling steps at dt {id}: manifest {n}, t_pre / dt gives {m}`
- `unexpected dt id {id}`
- `model {id} is not in the ensemble`
- `settled shape: expected [{n}], found {shape:?}`
- `settling_centre shape: expected [{n_settle}, 65], found {shape:?}`
- `sequence {s} has no frames at dt {dt}`
- `sequence {s}'s snapshot frames: expected 6, found {n}`
- `input shape, sequence {s}: …`
- `centre shape, sequence {s}: …`
- `snapshots shape, sequence {s}: …`
- `{model} {dt} {quantity}: PROVENANCE {got:e}, registered {want:e}`
- `the manifest's runs are not the eight registered: …`
- `centre node {k}: …`
- `{file}: {n} bytes, manifest says {m}` and `{file}: {n} bytes, PROVENANCE sizes says {m}`
- `no registered bound for model {m} at dt {dt}`

`parity` never reads `SEMBLANCE_REQUIRE_FIXTURES`. That variable belongs to
the test harness. A missing fixture is always `Missing` for the library and
always exit 2 for the command.

### 2. The gates become wrappers

`gate_support.rs` shrinks to the test fixture:
- a `OnceLock<Result<Fixtures, String>>` filled through `test_support`'s
  reads, in today's order
- then `Fixtures::from_parts`

`settling_checks.rs` and `trace_checks.rs` keep:
- every test name, module path and doc comment
- their print and panic format strings, moved verbatim

They call `parity::settle`, `parity::play` and the bounds functions, and
format the returned structs. On `Err(e)` they `panic!("{e}")`. `test_support`
keeps its loaders and notices, and calls `parity::how_to_get` (R13). The
trace wrappers keep their per-file `OnceLock`s and build a `TraceInputs`.
`the_gate_resolves_integer_counts` and its doc comment stay in
`settling_checks.rs` (MINORS-12). Neither module gains or loses a test
(TRACES-14 requires exactly 20 in `settling_checks::`, and MINORS-15 requires
"20 passed").

The extraction lands as its own commit. The shadow acceptance suites
(`acceptance_settling`, `acceptance_traces`, `acceptance_minors`) must pass on
that commit before `validate` exists.

**Strings the refactor must keep byte for byte.**

| # | String (format) | Matched by |
|---|---|---|
| C1 | `exact run {run} ({model}, dt {dt}, {p}): settled {d} of {n} differ; settling centre {d} of {n} differ` | TRACES-14 (`ends_with`), MINORS-15 |
| C2 | `run {run} is not bit-equal to flyvis: {a} settled and {b} settling-centre values differ; first difference at {settle step k (T) \| the settled state only}; largest settled error {e:e} at node {i} ({T})` | SETTLING-02, TRACES-14, MINORS-09 |
| C3 | `libm run {run} ({model}, dt {dt}, {p}): settled max {:.3e} at node {i} ({T}), {:.3e} of bound; settling centre max {:.3e} at step {k} ({T}), {:.3e} of bound; worst types {T v:.2e} ×3; settled bit-equal {e} of {n}` | SETTLING-04, MINORS-04, TRACES-14 |
| C4 | `run {run}: Rust's exp exceeds the pre-registered bound: settled {:e} (bound {:e}), settling centre {:e} (bound {:e})` | MINORS-09 |
| C5 | `run {run}: node {i} ({T}) is not finite after settle step {k}` | SETTLING-06 |
| C6 | substrings `dt {id}`, `settling steps at dt {id}`, `settling_centre shape`, `settled shape` | SETTLING-05 |
| C7 | substring `{model} {dt} {quantity}` in the provenance checks' failures | SETTLING-08, TRACES-10 |
| C8 | `skipping: {path} not found. {how}` and `SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing`, with `{path}` = `{CARGO_MANIFEST_DIR}/../../data/{rel}` | SETTLING-11, TRACES-13, TRACES-15, MINORS-01–03 |
| C9 | `trace exact run {run} ({model}, dt {dt}, {p}): {k} of {n} sequences bit-equal, centre and snapshots` | TRACES-06, -07, -23, MINORS-15 |
| C10 | `run {run} is not bit-equal to flyvis under stimulus:` then per sequence `sequence {s}: {a} of {N} centre and {b} of {M} snapshot values differ; first difference at {frame i (T) \| the snapshots only}; largest centre error {e:e}; largest snapshot error {e:e} at node {i} ({T})` | TRACES-07 |
| C11 | `trace libm run {run} (…): worst sequence {s}, {:.3e} of bound; centre max {:.3e} in sequence {s} at frame {i} node {n} ({T}), {:.3e} of bound; snapshots max {:.3e} in sequence {s} at frame {f} node {n} ({T}), {:.3e} of bound; bit-equal {e} of {t}` | TRACES-09 |
| C12 | `run {run}: node {k} ({T}) is not finite after frame {i} of sequence {s}` | TRACES-04 |
| C13 | substrings `centre shape, sequence {s}`, `snapshots shape, sequence {s}` | TRACES-05 |
| C14 | `mirrored mapping, run 3, sequence 8: snapshots max {:e}, centre max {:e}` (the printed value is compared bit for bit) | TRACES-12, -13 |
| C15 | `run {r} sequence {s} snapshot {k} (frame {f}), node {n}` | TRACES-11 (the test stays test code; it is not moved) |
| C16 | the 20 `settling_checks::` names and the 16 `trace_checks::trace_{exact,libm}_run_*` names | TRACES-14, MINORS-03, -15, CI's names |

`snapshots_agree_with_the_centre_column`, `trace_bounds_match_provenance`,
`pre_registered_bounds_match_provenance`, `reference_runs_are_the_eight_expected`
and `stimulus_checks` stay test code. The two provenance tests parse
PROVENANCE with `Provenance::parse` and call `check_*_gaps`, whose failure
text contains C7. That is the only change to them.

### 3. Dependencies

- `crates/flyvis/Cargo.toml`: `semblance-sim` moves from `[dev-dependencies]`
  to `[dependencies]`.
- `app/Cargo.toml`: adds `semblance-flyvis`.
- No other package and no external crate. `Cargo.lock` lists the same 8
  packages, now at 0.3.0. Publishing order stays valid (F16): flyvis after sim,
  and semblance last.

### 4. The command

`app/src/main.rs` gains `"validate" => validate::cmd(&args[2..])`. The new
`app/src/validate.rs` parses the flags and formats the output (R6).

```
semblance validate [--data DIR] [--threads N] [--markdown]

  --data DIR     the fixture directory (default: ./data)
  --threads N    worker threads, N >= 1 (default: available parallelism, at most 20)
  --markdown     print the record's generated block instead of the table
```

- Each flag may appear at most once. `--threads` takes a decimal integer ≥ 1.
- Any other argument is a usage error: the message and the usage line go to
  stderr, and the exit code is 2 (R7).
- The default thread count is `available_parallelism()` (or 1 if unknown),
  capped at the number of jobs. Results do not depend on it.
- Output goes to stdout. Diagnostics and missing-file lists go to stderr.

**Exit codes.**

| Code | Meaning |
|---|---|
| 0 | every exact job is bit-equal, every libm job is within its bounds, and every control holds |
| 1 | a gate failed: an exact value differs, a libm bound is exceeded, a control does not hold, or a state went non-finite. The full table still prints, with each failure marked. |
| 2 | nothing was verified: a fixture is missing, unreadable, malformed or mismatched, or the command line is wrong |

If any fixture-class error occurs, 2 wins over 1.

**Help text** (replaces `print_help`'s body):

```
semblance — connectome compression research: flyvis in Rust

USAGE:
  semblance <command> [options]

COMMANDS:
  demo        run the LIF simulator once on a synthetic connectome
  roundtrip   lossless encode->decode, report structural + behavioural fidelity
  evolve      seed from the connectome latent and run a toy evolution loop
  validate    check that this build reproduces flyvis bit for bit
              [--data DIR] [--threads N] [--markdown]
  help        show this message

demo, roundtrip and evolve run on a synthetic graph and need no data.
validate needs flyvis's reference fixtures under data/ (get them with
ci/fetch-fixtures.sh); the recorded result is docs/results/0.3.0-flyvis-parity.md.
```

The module doc's "swap in `clap`" sentence (F1) is replaced by "std-only by
design; the workspace has no external dependencies".

### 5. Missing data

`semblance validate` with no `data/`:

```
validate: nothing was verified; 13 fixture files are missing:
  ./data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json
  ./data/flyvis-pretrained/ensemble.csv
  ./data/flyvis-reference/manifest.json
  … (every absent file, one per line)
Get them with ci/fetch-fixtures.sh (needs gh), or see tools/README.md.
The result recorded for 0.3.0 is docs/results/0.3.0-flyvis-parity.md; it is not a check of this machine.
```

This goes to stderr with exit 2. Only the paths that are actually absent are
listed. When exactly one is missing, its `how_to_get` hint is printed in full.

### 6. The terminal output

The header is in `roundtrip`'s `label: value` style, then a fixed-width table
in `evolve`'s style, then the controls, coverage and a verdict line. The
example uses the numbers measured on 2026-10-29 (F9, F10, F11).

- `…` marks a value that the run fills in but that was not in the logs this
  spec was written from: the settling libm rows' total unequal count (settled
  plus centre) and the control maxima.
- `max err` is the maximum of whichever quantity has the worse ratio, and
  `where` names that quantity.

```
semblance validate: does this build reproduce flyvis? (epic 0.3.0)
data:        ./data
reference:   export 20261027T114358Z-63037 (2026-10-27); flyvis 1.2.0, torch 2.14.0; macOS-26.6.2-arm64-arm-64bit, cpu
models:      000 flow/0000/000 (best, validation loss 5.137), 049 (worst, validation loss 5.678)
this build:  semblance 0.3.0, aarch64-macos, debug assertions off, 16 threads
libm exp:    f32 differs from torch's on … of 2355 counts, f64 on … (fingerprint f32 …, f64 …)

stage     run  model  dt     prec  counts  compared  unequal    max err  where                          bound      ratio
settling  0    000    1/200  f32   exact      58669        0          0  -                              ==         -
settling  0    000    1/200  f32   libm       58669        …  4.768e-07  centre, step 3 (TmY4)          2.695e-06  1.769e-01
settling  1    000    1/200  f64   exact      58669        0          0  -                              ==         -
settling  1    000    1/200  f64   libm       58669        …  1.110e-15  settled, node 16054 (Mi4)      1.000e-10  1.110e-05
settling  2    000    1/50   f32   exact      48919        0          0  -                              ==         -
settling  2    000    1/50   f32   libm       48919        …  8.345e-07  settled, node 15739 (Mi4)      2.553e-06  3.268e-01
settling  3    000    1/50   f64   exact      48919        0          0  -                              ==         -
settling  3    000    1/50   f64   libm       48919        …  2.220e-15  settled, node 15993 (Mi4)      1.000e-10  2.220e-05
settling  4    049    1/200  f32   exact      58669        0          0  -                              ==         -
settling  4    049    1/200  f32   libm       58669        …  3.710e-06  settled, node 16134 (Mi9)      1.232e-05  3.013e-01
settling  5    049    1/200  f64   exact      58669        0          0  -                              ==         -
settling  5    049    1/200  f64   libm       58669        …  9.409e-15  settled, node 16134 (Mi9)      1.000e-10  9.409e-05
settling  6    049    1/50   f32   exact      48919        0          0  -                              ==         -
settling  6    049    1/50   f32   libm       48919        …  1.669e-06  settled, node 43787 (TmY14)    1.289e-05  1.295e-01
settling  7    049    1/50   f64   exact      48919        0          0  -                              ==         -
settling  7    049    1/50   f64   libm       48919        …  7.994e-15  settled, node 44066 (TmY14)    1.000e-10  7.994e-05
traces    0    000    1/200  f32   exact    3717168        0          0  -                              ==         -
traces    0    000    1/200  f32   libm     3717168  1706062  9.537e-07  centre, seq 9 frame 302 (T2)   3.450e-06  2.764e-01
traces    1    000    1/200  f64   exact    3717168        0          0  -                              ==         -
traces    1    000    1/200  f64   libm     3717168  1792079  1.776e-15  snaps, seq 11 frame 314 (T2)   1.000e-10  1.776e-05
traces    2    000    1/50   f32   exact    3395288        0          0  -                              ==         -
traces    2    000    1/50   f32   libm     3395288  1844269  1.907e-06  snaps, seq 7 frame 104 (Mi4)   5.964e-06  3.198e-01
traces    3    000    1/50   f64   exact    3395288        0          0  -                              ==         -
traces    3    000    1/50   f64   libm     3395288  1870870  3.109e-15  snaps, seq 11 frame 78 (Mi4)   1.000e-10  3.109e-05
traces    4    049    1/200  f32   exact    3717168        0          0  -                              ==         -
traces    4    049    1/200  f32   libm     3717168  1112385  8.801e-05  snaps, seq 3 frame 359 (Mi4)   1.023e-04  8.605e-01
traces    5    049    1/200  f64   exact    3717168        0          0  -                              ==         -
traces    5    049    1/200  f64   libm     3717168  1490218  2.871e-13  snaps, seq 3 frame 359 (Mi4)   1.000e-10  2.871e-03
traces    6    049    1/50   f32   exact    3395288        0          0  -                              ==         -
traces    6    049    1/50   f32   libm     3395288   898830  1.526e-05  centre, seq 7 frame 124 (T4a)  6.884e-05  2.217e-01
traces    7    049    1/50   f64   exact    3395288        0          0  -                              ==         -
traces    7    049    1/50   f64   libm     3395288  1346567  3.121e-13  snaps, seq 3 frame 89 (Mi4)    1.000e-10  3.121e-03

controls:  wrong model (049 vs run 3)       settled max …          must be > 1e-03   holds
           integer counts (run 3)           settled max …          must be > 1e-10   holds
           mirrored mapping (run 3, seq 8)  snapshots max 2.503e+00 must be > 1e-03  holds
           flyvis's settled start (run 3, seq 8)  0 unequal        must be ==        holds
coverage:  weakest cell type moves 3.773e-03 (Tm30, run 2); threshold 1e-03, recorded, not a gate:
           no type is undriven, so Sintel's revisit trigger 1 does not fire
verdict:   PASS — 16 exact rows bit-equal, 16 libm rows within bounds (worst 8.605e-01, traces run 4), 4 controls hold  [… s]
```

Columns:
- `compared` is the number of values checked:
  - settling: 45,669 plus n_settle × 65
  - traces: Σ n_frames × 65 plus 12 × 6 × 45,669
- `unequal` is how many are not `==`.
- Floats print in `{:.3e}`. An exact row prints `0`, `==` and `-` unless it
  fails, and then it prints its maximum.
- A failing row ends in `FAIL`, and the verdict line names every failing job
  and control.
- The trailing time is wall time, terminal only.

### 7. The record: `docs/results/0.3.0-flyvis-parity.md`

**Preamble, hand-written.** It covers:
- what 0.3.0 claims, and the three gates (exact, libm, controls)
- what "exact" and "libm" mean
- the never-relax rule
- why the f64 ratios read about 1e-5 to 1e-3, and the f32 ones up to 0.86
- that the libm rows are one host's facts (R9)
- the fixtures (`fixtures-2`, `ci/fixtures.sha256`)
- the toolchain and the date the record was generated (R10)
- that the exact gate needs the torch-count table `syn_count_torch.rs`, and
  what that table is
- the Sintel decision, citing the coverage section

The preamble quotes no measured number that the generated block does not also
contain.

**Generated block,** exactly what `semblance validate --markdown` prints, pasted
below the preamble:

```markdown
<!-- BEGIN semblance validate --markdown -->
## Reference (flyvis side)
| | |
|---|---|
| export | 20261027T114358Z-63037, exported 2026-10-27 |
| exporter | tools/export_reference.py, sha256 e46fb05b…b120 (in full); git c43f4b73517d… (clean); uv.lock sha256 … |
| versions | python 3.12.11, flyvis 1.2.0, torch 2.14.0, numpy 2.5.3, h5py 3.16.0, datamate 1.0.0 |
| machine | macOS-26.6.2-arm64-arm-64bit (arm64), device cpu; default 12 threads, runs 0 and 1 bit-equal at 1 and 12 |
| models | 000 flow/0000/000, best, validation loss 5.136557016107771, checkpoint sha256 d0e42857…; 049 …, worst, … |
| parameters | all 50 checkpoints equal ensemble.csv |
| equivalence | the export's forward path equals stimulus_response bit for bit (sequences 0, 4) |
| input rounding | 2.9801242251714655e-08 at 1/200 and 1/50 (the f32 runs' moving edges) |
| files | 9 reference files, each the size the manifest and PROVENANCE give |

<!-- BEGIN host-specific -->
## This run (Rust side)
| semblance | 0.3.0 | target | aarch64-macos | debug assertions | off | threads | 16 |
| libm exp | f32: … of 2355 counts differ from torch's; f64: … | fingerprint | f32 0x…, f64 0x… |
<!-- END host-specific -->

## Bounds as applied
| run | quantity | bound | derivation |
|---|---|---|---|
| all | every quantity, exact | == | torch's counts (syn_count_torch.rs); every value must equal flyvis's |
| 0 | settled, libm | 2.8228733750523816e-06 | 2 × f32_f64_gap["000 1/200"].settled = 2 × 1.4114366875261908e-06 |
| 1 | every quantity, libm | 1e-10 | the f64 bound |
| … | | | |

## Stage 1: settling, exact (torch's counts)
| run | model | dt | precision | quantity | compared | unequal | max error |
|---|---|---|---|---|---|---|---|
| 0 | 000 (best) | 1/200 | f32 | settled | 45669 | 0 | 0 |
| 0 | 000 (best) | 1/200 | f32 | settling centre | 13000 | 0 | 0 |
| … (16 rows) | | | | | | | |

## Stage 2: traces, exact (torch's counts), per stimulus
| run | model | dt | precision | stimulus | sequences | quantity | compared | unequal | max error |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 000 (best) | 1/200 | f32 | flashes | 0–3 | centre | 156000 | 0 | 0 |
| 0 | 000 (best) | 1/200 | f32 | flashes | 0–3 | snapshots | 1096056 | 0 | 0 |
| 0 | 000 (best) | 1/200 | f32 | moving edges | 4–11 | centre | 273000 | 0 | 0 |
| 0 | 000 (best) | 1/200 | f32 | moving edges | 4–11 | snapshots | 2192112 | 0 | 0 |
| … (32 rows) | | | | | | | | | |

<!-- BEGIN libm fingerprint=f32:<count>:<fnv>,f64:<count>:<fnv> -->
## Stage 1: settling, libm (Rust's exp)
| run | model | dt | precision | quantity | compared | unequal | max error | where | bound | ratio | worst types |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 000 (best) | 1/200 | f32 | settled | 45669 | 21628 | 4.768e-07 | node 15809 (Mi4) | 2.823e-06 | 1.689e-01 | Mi4 4.77e-07, T2 4.77e-07, TmY18 3.58e-07 |
| 0 | 000 (best) | 1/200 | f32 | settling centre | 13000 | … | 4.768e-07 | step 3 (TmY4) | 2.695e-06 | 1.769e-01 | |
| … (16 rows) | | | | | | | | | | | |

## Stage 2: traces, libm (Rust's exp), per stimulus
| run | model | dt | precision | stimulus | quantity | compared | unequal | max error | where | bound | ratio |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 4 | 049 (worst) | 1/200 | f32 | flashes | snapshots | 1096056 | … | 8.801e-05 | seq 3, frame 359, node 15414 (Mi4) | 1.023e-04 | 8.605e-01 |
| … (32 rows) | | | | | | | | | | | |
<!-- END libm -->

## Controls
| control | run | measured | must be | holds |
|---|---|---|---|---|
| model 049's parameters against model 000's reference | 3 | settled max … | > 1e-03 | yes |
| the spec's integer counts in place of exp(ln count) | 3 | settled max … | > 1e-10 | yes |
| hexals mirrored (u, v) → (−u, −v), sequence 8 | 3 | snapshots max 2.503e+00, centre max 2.292e+00 | > 1e-03 | yes |
| start from flyvis's settled.f64.bin, sequence 8 | 3 | 0 of … unequal | == | yes |

## Coverage: Sintel revisit trigger 1
Movement of a cell type = max over the 12 sequences and every frame of |centre trace − settled centre|, from the exact jobs (equal to flyvis's). Threshold 1e-03: recorded, not a gate.
| run | weakest type | movement | next two | median | centre bound | weakest / bound |
|---|---|---|---|---|---|---|
| 0 | Tm30 | 3.801e-03 | Mi11 1.342e-02, L4 3.932e-02 | 4.973e-01 | 3.450e-06 | 1.102e+03 |
| 2 | Tm30 | 3.773e-03 | Mi11 1.796e-02, L4 5.688e-02 | 7.081e-01 | 5.072e-06 | 7.439e+02 |
| 4 | Tm30 | 3.248e-02 | L2 4.600e-02, Tm28 4.818e-02 | 7.981e-01 | 5.200e-05 | 6.245e+02 |
| 6 | Tm30 | 3.179e-02 | L2 4.600e-02, Tm28 4.869e-02 | 8.958e-01 | 6.884e-05 | 4.618e+02 |
| … (8 rows; f64 runs read 1e-10 and 3.2e+07 to 3.2e+08) | | | | | | |
Every one of the 65 cell types moves by at least 3.773e-03, above the threshold: no type is undriven by flashes and moving edges, and trigger 1 does not fire. This covers the centre column only.
<!-- END semblance validate --markdown -->
```

The generated block is deterministic given the fixtures and, in its libm
sections, the host's libm. It contains no wall time, date, path, or thread
count outside the host-specific section. Integers print without separators.
Bounds print in `{:e}` (shortest round-trip). Measured values print in
`{:.3e}`, except the mirrored control, which prints `{:e}` as the test does
(C14).

**The honesty test, `app/tests/record.rs`:** one test,
`the_committed_record_is_what_validate_prints`.

1. **Fixtures.** If any of the 13 required files is absent under
   `{CARGO_MANIFEST_DIR}/../data`, the test prints
   `skipping: {path} not found. Run ci/fetch-fixtures.sh.` and passes. Under
   `SEMBLANCE_REQUIRE_FIXTURES=1` it panics instead with
   `SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing`.
2. **Run.** It runs `env!("CARGO_BIN_EXE_semblance") validate --markdown --data
   <workspace>/data` and requires exit 0.
3. **Extract.** It takes the text between the `BEGIN`/`END semblance validate
   --markdown` markers, both from the command's stdout and from the committed
   file. Each must contain the pair exactly once.
4. **Host section.** It deletes the host-specific section, markers included,
   from both.
5. **libm sections.** It reads the fingerprint from each libm `BEGIN` marker.
   - If the fingerprints are equal, nothing more is removed.
   - Otherwise it replaces each libm section's body with its marker in both
     texts, and prints
     `record: libm sections not compared: this host's libm fingerprint {x} differs from the record's {y}`.
6. **Compare.** It compares the two texts byte for byte. A failure names the
   first differing line of each.

The test spawns the binary under test, not `tools/` and not git. MINORS-05
scans `crates/`, not `app/`, and this test would satisfy its intent anyway. It
reads no git history.

### 8. CI time (R4)

| | Change | Assurance |
|---|---|---|
| P1 | MINORS-15 drops its `trace_checks::` re-run. Its settling, `stimuli_checks::` and `moving_edge_checks::` parts stay. | The 22 trace tests still run directly under `SEMBLANCE_REQUIRE_FIXTURES=1`. TRACES-06 asserts all 8 exact lines. TRACES-09 checks the libm line's format for runs 2 and 3. The record test checks all 16 trace jobs' numbers through `validate`. What is lost: a check that libm runs 0, 1 and 4–7 print a line starting `trace libm run `. |
| P2 | TRACES-06 runs the 8 exact trace gates in one nested invocation: `cargo test -p semblance-flyvis --lib --locked --offline -- trace_checks::trace_exact_run_ --nocapture --test-threads=4`. It asserts `running 8 tests`, success, and that the output contains each of the same 8 lines (C9), as today. | Nothing is lost: the same gates run and the same strings are required. `eprintln!` holds the stderr lock for each line, and stdout and stderr are captured separately, so lines do not interleave. The in-process replay of 8 runs stays. |
| P3 | SEQUENCES-08 drops its nested `cargo`. Its in-process `Manifest::parse` and `parse_sequences` checks stay. | `reference::tests::` and `acceptance_reference` already run in the same suite. With no fixture read, SEQUENCES-08 is no longer fixture-gated, so the skip count drops by one and README's sentence about it goes. |
| P4 | `.github/workflows/ci.yml`: the `test` job's `actions/cache` step no longer lists `target`. The `~/.cargo` paths stay (empty but harmless). The `msrv` job is unchanged, being off the critical path. | Nothing is lost. A fresh checkout makes every path dependency dirty, and every shadow re-copies its sources, so the cache never saved a build (F18). |

**Estimate, to be checked on the PR's first hosted run:**
- P1–P4 take the test step from 689 s to about 380–400 s.
- `app/tests/record.rs` adds one `validate` run with 4 threads on the 4-vCPU
  runner: about 255 CPU-s (F9's 111.5 s serial × the 2.3 serial factor), so
  about 65–90 s wall.
- The job goes from 12m32s to about 8–8.5 minutes.
- If the record test is over 150 s on CI, that is reported to the owner
  (Q3). It is not removed and no check is weakened.

### 9. The release (R3)

**Version.**
- `[workspace.package] version = "0.3.0"`, and every `version = "0.2.0"` in
  `[workspace.dependencies]` becomes `"0.3.0"` (F16).
- `Cargo.lock` is regenerated with `cargo build`: still 8 packages.
- `CITATION.cff`: `version: 0.3.0`, and `date-released` set to the tag date
  (the head sets it in the release commit, below).

**Packaging** (piece3-packaging.md's list):

1. Tracked symlinks `LICENSE -> ../../LICENSE` and `NOTICE -> ../../NOTICE` in
   each of `crates/{rng,connectome,codec,sim,evo,fidelity,flyvis}/`, and
   `-> ../LICENSE` and `-> ../NOTICE` in `app/`. The shadows skip symlinks
   (F14), which does not matter to a build.
2. `crates/flyvis/Cargo.toml`: `license = "Apache-2.0 AND MIT"` in place of
   `license.workspace = true`.
3. NOTICE lists `crates/flyvis/src/syn_count_torch.rs`: "torch's `exp` of
   `SYN_COUNT` in f32 and f64, generated by tools/export_syn_count.py". README's
   "Data and attribution" paragraph names it beside the other two.
4. `crates/flyvis/src/lib.rs` crate docs gain a "Licensing" paragraph:
   `pretrained`, `syn_count_torch` and `groundtruth` are flyvis's MIT data, see
   NOTICE. The two generated files are not edited (CLAUDE.md: rerun their
   scripts only). The same docs gain one paragraph on `parity` and
   `semblance validate`.
5. `exclude = ["tests/"]` in `crates/flyvis/Cargo.toml`, `crates/sim/Cargo.toml`
   and `app/Cargo.toml` (R15). rng keeps `tests/vectors.rs`, which is its
   stability contract and is self-contained.
6. Descriptions:
   - **sim:** "Neuron dynamics: flyvis's graded (non-spiking) integrator in
     f32 and f64, plus a legacy LIF spiking baseline and the engine that
     couples a connectome to input and readout."
   - **flyvis:** "flyvis (Lappalainen et al. 2024) in Rust: reads the
     published connectome spec, expands it bit-exactly into the realised
     network, carries flyvis's trained parameters and ground truth (MIT data;
     see NOTICE), and checks parity with flyvis's own traces. No external
     dependencies."
   - **codec:** "The compress/uncompress round-trip: a Codec trait, a lossless
     WeightCodec baseline, and a ReducedCodec whose encode and decode are not
     implemented yet: calling them panics (todo!())."
   - **app:** "CLI that wires the pipeline together: simulate, round-trip,
     evolve, and validate parity with flyvis. Std-only argument parsing."
7. Workspace `categories = ["science::neuroscience", "simulation"]`. The slug
   exists on crates.io (piece3-packaging.md §5).
8. README's `[LICENSE](LICENSE)` and `[NOTICE](NOTICE)` links become absolute
   GitHub URLs, which resolve on crates.io as well.

**Pre-merge packaging check,** in the PR:
- `cargo package --workspace` on a clean tree
- `for c in target/package/*.crate; do tar tzf "$c"; done | grep -cE '/(LICENSE|NOTICE)$'`
  prints 16
- `cargo publish --workspace --dry-run` passes

**After merge** (head steps, in order):
1. Annotated tag on the merge commit:
   `git tag -a v0.3.0 -m "0.3.0: graded dynamics, validated against flyvis"`,
   then `git push origin v0.3.0`.
2. **Stop and ask the owner.** Only on an explicit go-ahead at that moment,
   from a clean checkout of the tag on macOS or Linux: `cargo publish
   --workspace`. It publishes in F16's order and waits for the index between
   crates.
3. After all eight 0.3.0 crates are live: `cargo yank --version 0.1.0 <crate>`
   for semblance, semblance-codec, semblance-connectome, semblance-evo,
   semblance-fidelity and semblance-sim (F17).
4. A follow-up commit puts CLAUDE.md's yank paragraph in the past tense and
   sets `CITATION.cff`'s `date-released` if it was not set at tag time.

There is no GitHub Release.

### 10. Documentation

**README.md:**
- **Headline.** A paragraph after the opening one. It says that 0.3.0
  reproduces flyvis's graded dynamics bit for bit:
  - in every neuron's settled state, and in every centre neuron at every frame
    and every neuron at six frames of 12 stimuli
  - for the best and worst of the 50 trained models, at both time steps, in
    f32 and f64
  - and within pre-registered bounds when Rust computes the synapse counts
    itself
- **How to check.** `ci/fetch-fixtures.sh` then `cargo run --release --
  validate`, with exit codes 0, 1 and 2.
- **The record.** A link to `docs/results/0.3.0-flyvis-parity.md`.
- **Quickstart:** a `validate` line.
- **Crate table:** the flyvis row's open cell drops "the result record (0.3.0,
  in progress)", and the app row mentions `validate`.
- **Counts:** the skip count, re-measured, and the SEQUENCES-08 sentence
  removed (P3).
- **Attribution:** the paragraph adds `syn_count_torch.rs`, and the
  LICENSE/NOTICE links become absolute.

**CLAUDE.md:**
- `Current: **v0.3.0**, crate versions \`0.3.0\``.
- **"Verifying a change":**
  - The test and skip counts, re-measured at the end the way pieces 1 and 2
    did. That is `cargo test --workspace` with the fixtures, then with `data/`
    absent and `SEMBLANCE_REQUIRE_FIXTURES=1`, counting the failures.
  - A paragraph on `cargo run --release -- validate`: what its exit codes
    mean, that it runs the same code as the gates, and that
    `docs/results/0.3.0-flyvis-parity.md` is its committed output, held to it
    by `app/tests/record.rs`, with the libm sections compared only on the
    recording host's libm.
- **The yank sentence** becomes:

  > `semblance-codec` 0.3.0 still contains both `todo!()`s; its description
  > says so. The six 0.1.0 releases (semblance, -codec, -connectome, -evo,
  > -fidelity, -sim) are to be yanked once all eight 0.3.0 crates are live:
  > they predate flyvis and graded dynamics, evo 0.1.0 depends on `rand`, and
  > yanking codec alone would strand evo, fidelity and semblance 0.1.0.
  > Publishing and yanking wait for the owner's go-ahead. A real
  > `ReducedCodec` later needs no yank of its own.

  The head's follow-up commit (§9, step 4) changes "are to be yanked" to
  "were yanked on <date>".
- **The tools/ boundary** paragraph: `validate` reads only `data/`. It never
  runs `tools/` or git.

**Drift fixes (F19):** CITATION.cff, the sim, flyvis, codec and app
descriptions, the app help text and module doc, and README's flyvis row.

**Not edited:** the earlier specs. Their facts are historical. The H12
discrepancy is noted here (F19) and in the record's coverage section, which
is generated.

## Implementation order

1. **CI time (§8).** This lands first, so every later push gets the faster CI.
2. **Extract `parity` and turn the gates into wrappers (§1–§3).** No `validate`
   yet. Run the full suite with `SEMBLANCE_REQUIRE_FIXTURES=1`, including all
   shadow acceptance suites, before going on.
3. **`Provenance::parse`, the fixture checks, `validate()`, the threads, the
   controls, coverage, and the fingerprint.**
4. **The command (§4–§6) and `app/tests/validate_cli.rs`.**
5. **The record (§7):** generate it with `cargo run --release -- validate
   --markdown`, write the preamble, and add `app/tests/record.rs`.
6. **The release changes (§9)** and the docs (§10). Re-measure the counts last.

## Testing, test-first

**`parity` unit tests** (`parity::tests`, synthetic, no fixtures):
- `Bound::value`; `settling_bounds` and `trace_bounds` for every registered
  (model, dt, precision) and for each `Counts`. An unregistered model is an
  error, and `IntegerNSyn` is an error.
- `diff` maps NaN to infinity (moved with
  `a_nan_reference_value_makes_the_maximum_infinite`).
- `Provenance::parse`:
  - a minimal valid document parses
  - each required field missing or mistyped is an error naming it
- `check_*_gaps`: one ulp off in any gap is a `Mismatch` naming
  `{model} {dt} {quantity}`.
- `how_to_get`: the five texts are unchanged. A test pins each of them,
  because C8 depends on them.
- `Options`: `threads: 0` is `Usage`, as is a run index ≥ 8.
- `Fixtures::from_parts` on malformed input returns `Malformed` and never
  panics. The inputs: an empty spec, a truncated ensemble, a manifest with one
  run, and a settling file one byte short.
- FNV-1a against its published test vectors ("" → `0xcbf29ce484222325`, "a" →
  `0xaf63dc4c8601ec8c`).

**`parity` fixture tests.** Each skips with its own notice through
`test_support::or_skip` and fails under `SEMBLANCE_REQUIRE_FIXTURES=1`:
- `validate_is_bit_identical_across_thread_counts`: `Options { runs: vec![2,
  3] }` at 1 and at 4 threads. The two reports' jobs, controls and coverage
  are equal (`PartialEq`, NaN impossible after `diff`).
- `validate_refuses_a_provenance_gap_one_ulp_off`: a temp data dir of symlinks
  with an altered PROVENANCE returns `Mismatch` before any job runs.
- `validate_refuses_a_file_whose_size_disagrees`: a truncated `centre.f64.bin`
  returns `Mismatch` naming it.
- `the_libm_fingerprint_counts_differences_from_torch`: each count lies in
  1..=200. That is the same range `syn_count_torch_checks` uses, so a
  fingerprint built from the wrong table fails.

**Gate wrappers:** the existing suites, unchanged in name and outcome.
`acceptance_settling`, `acceptance_traces`, `acceptance_minors`,
`acceptance_sequences` and `acceptance_mapping` are the test that C1–C16
survived.

**`app/tests/validate_cli.rs`** (no fixtures; spawns the built binary):
- `--data <empty temp dir>`: exit 2, and stderr lists all 13 paths and names
  `ci/fetch-fixtures.sh`.
- `--threads 0`, `--threads x`, `--threads` with no value, `--bogus`, and
  `--data` given twice: each exits 2 with a usage message.
- `help`: lists `validate` and no longer says everything works with no data.
- `roundtrip`: still prints `structural distance:  0.000000` and
  `behavioural distance: 0.000000`.

**`app/tests/record.rs`:** §7.

## Failure handling

| Situation | Behaviour |
|---|---|
| fixtures absent, `validate` | exit 2; every missing path listed with its hint; nothing printed as a result (R12) |
| fixtures absent, a gate or the record test, `SEMBLANCE_REQUIRE_FIXTURES` unset | prints its own `skipping:` notice and passes |
| fixtures absent, `SEMBLANCE_REQUIRE_FIXTURES=1` | fails, naming the path |
| PROVENANCE gap ≠ registered constant | exit 2 (`Mismatch`). The constant is not changed, and the bound is not recomputed |
| file size ≠ manifest or PROVENANCE `sizes` | exit 2, naming the file |
| an exact value differs | exit 1, the row marked `FAIL`. Work stops for diagnosis, starting from the epic's ladder (piece 2's R2: rerun from flyvis's settled state) |
| a libm bound exceeded | exit 1. On a host whose libm fingerprint differs from the record's while the exact jobs pass, this is a libm portability finding, recorded and investigated. It is never a reason to change a bound |
| a control does not hold | exit 1: the gate can no longer see what it exists to see |
| a non-finite state | exit 1, naming the run, node, cell type and step or frame. The other jobs still report |
| a worker thread cannot be spawned | the remaining jobs run on the calling thread, with the same results |
| the record test differs | fails at the first differing line. The fix is regenerating the record from a passing `validate` and reviewing the diff, never editing numbers by hand |
| the record test on a host with another libm | compares everything except the libm sections and prints why (R9) |
| a shadow acceptance test fails after the extraction | a C-string moved. Restore the string, never the test's expectation |
| `cargo publish` fails part-way | stop. The crates already uploaded stay, and the owner decides. Never yank or bump around it unasked |

## Non-goals

- Changing any fixture (`fixtures-2`, `data/`), any bound, `TOLERANCE`, or
  any `==`.
- A `--quick`, `--run` or `--json` flag. `Options::runs` exists for tests and
  is not on the command line.
- Running `validate` as its own CI step. The record test runs it.
- Printing the committed record when the data is absent (R12).
- rustc version or build date in the binary (R10).
- A crate-local `LICENSE-MIT` file for flyvis. NOTICE carries the text.
- Per-crate READMEs.
- Dots, Sintel, other models, `sim`'s trait migration (0.4.0), and the codec's
  `todo!()`s.
- A GitHub Release.
- The local outline edit (task 7 done), which the head makes.

## Done when

- **Local full gate, on this Mac, with fixtures present and
  `SEMBLANCE_REQUIRE_FIXTURES=1`:**
  - `cargo build --workspace`
  - `cargo test --workspace` (every shadow acceptance suite included)
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo run --quiet -- roundtrip` reads `0.000000` twice
  - `cargo run --release -- validate` exits 0
  - `Cargo.lock` lists the same 8 packages at 0.3.0
- The record test compares the committed record in full here, with the libm
  sections included.
- `cargo package --workspace` shows 16 LICENSE/NOTICE entries, and `cargo
  publish --workspace --dry-run` passes.
- CLAUDE.md and README carry re-measured counts.
- Hosted CI (`ubuntu-latest`) is green on the PR. The test job's time is
  recorded in the PR description against the 12m32s baseline.
- The PR is merged.
- `v0.3.0` (annotated) is pushed.
- `cargo publish --workspace` and the six yanks have happened **only after the
  owner's explicit go-ahead**, followed by the CLAUDE.md past-tense commit.
  Until then 0.3.0 is tagged but not published, and that counts as done for
  the bump.

## Questions for the owner

- **Q1 — the coverage threshold (R8).** D2's "> 1000× the applicable bound"
  does not hold for the f32 runs 2, 4 and 6: the weakest type's movement is
  744×, 625× and 462× their libm centre bounds (F11). This spec records an
  absolute 1e-3, the controls' existing "far more than any bound", and prints
  every ratio. Keep 1e-3, or choose another number?
- **Q2 — libm rows checked on one host (R9).** The record's libm sections are
  compared only where the libm fingerprint matches the recording host (this
  Mac). Hosted CI (Linux, glibc) will compare the exact rows, controls,
  coverage and provenance, but probably not the libm rows. Accept that, or
  also commit a second libm section generated on CI's host?
- **Q3 — CI cost of the record test (§8).** It adds an estimated 65–90 s on
  the hosted runner, for a job of about 8–8.5 minutes after P1–P4. Accept, or
  limit the record test to the exact jobs, which would leave the libm tables
  unchecked by any test?
