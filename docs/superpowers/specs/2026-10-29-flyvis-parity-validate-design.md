# The flyvis parity record, `semblance validate`, and the 0.3.0 release

**Date:** 2026-10-29
**Status:** draft, revised after two adversarial reviews
(`.temper/run-0.3.0/research/spec3-review-fidelity.md` and
`spec3-review-implementability.md`, local). The owner's decisions of
2026-10-29 (D1–D5) are binding and are recorded as rulings R1–R5. The owner's
answers to this spec's three questions are R8 (Q1), R9 (Q2) and R16 (Q3); the
head's ruling on NOTICE is R17. R6–R15 and R18–R22 are decided here under the
owner's delegation ("decide the small and medium items"). No question remains
open.
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
that command printed on hosted CI, a test that keeps the two the same, and it
then ships 0.3.0.

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
   holds, 1 means a gate failed, 2 means verification did not complete.
4. **The record, `docs/results/0.3.0-flyvis-parity.md`.** A hand-written
   preamble followed by `validate --markdown`'s output as produced on hosted
   CI. A fixture-gated test regenerates the output and compares it with the
   committed file.
5. **CI time.** The four reductions P1–P4, plus the step that produces the
   record (R9).
6. **The 0.3.0 release.** The version bump, packaging, licence files, docs,
   and the tag. Publishing to crates.io and yanking 0.1.0 happen only after
   the owner says go.

## Facts the design rests on

| # | Fact | Evidence |
|---|---|---|
| F1 | The CLI is one file, `app/src/main.rs`, with hand-rolled std-only parsing. No command takes flags, and every command returns `ExitCode::SUCCESS`. An unknown command returns `FAILURE`. The help text ends "Everything runs on a synthetic graph so it works with no data". The module doc says "Swap in `clap` if/when you want richer flags", which contradicts the zero-dependency rule. | `app/src/main.rs:10-11, 26-57, 86, 112, 161` |
| F2 | `app` does not depend on `semblance-flyvis`. Its description reads "simulate, round-trip, and evolve". | `app/Cargo.toml` |
| F3 | The release profile has `panic = "abort"`. A panic in a release `validate` kills the process before any summary prints. A panic exits 101 in a debug build and is killed by `SIGABRT` (shell status 134) in a release build: never 0. | `Cargo.toml` `[profile.release]` |
| F4 | All the gate machinery is `#[cfg(test)]`: `gate_support.rs` (fixture, `Precision`, `Counts`, `settle_state`, `settle`, `read`, `Diff`, `diff`, `F64_BOUND`), the settling bounds and reporters, and the trace loaders, `play`, the bounds and the reporters. Its core reports bad input with `assert!`, `expect` and `panic!`. Today `settle_state::<R>` asserts `info.precision == R::DTYPE`, with `R` fixed by the test's name. | `lib.rs:17-41`; `gate_support.rs:22, 47-65, 123-191`; `settling_checks.rs:38-116`; `trace_checks.rs:48-307` |
| F5 | `test_support::data_path` is `env!("CARGO_MANIFEST_DIR")/../../data`, a compile-time checkout path, returned as a `String` and private. `missing` panics under `SEMBLANCE_REQUIRE_FIXTURES=1`. `how_to_get` has six arms: five directories and the `_` default. | `test_support.rs:25-71` |
| F6 | The public API is already enough to settle and play a run. The acceptance tests do it independently of the gate code. | `acceptance_settling.rs:26-35`, `acceptance_traces.rs:28-38` |
| F7 | `semblance-sim` is only a dev-dependency of `semblance-flyvis`. sim depends only on connectome, so making it a normal dependency creates no cycle and no new package. | `crates/flyvis/Cargo.toml`, `crates/sim/Cargo.toml` |
| F8 | The dev profile builds `semblance-sim` and `semblance-flyvis` at `opt-level = 3`. A generic integrator is monomorphised in the crate that instantiates it, so a non-generic entry point in flyvis runs at opt-level 3 even when called from an unoptimised `app`. | `Cargo.toml`; graded-settling spec, "Running time" |
| F9 | Measured 2026-10-29 on a 16-core arm64 Mac, fixtures present: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- --nocapture --test-threads=8 settling_checks:: trace_checks::` gave 42 passed in 17.09 s. The research measured the same 32 gates serially at 111.5 s. | this session; piece3-validate.md §3 |
| F10 | The results of that run (F9): all 8 exact settling runs and all 8 exact trace runs are bit-equal. The libm maxima are in [§6](#6-the-terminal-output), example table. The worst f32 ratio is 8.605e-1 (trace run 4, sequence 3, snapshots, frame 359, Mi4: 8.801e-5 against 1.023e-4). The worst f64 ratio is 3.121e-3 (trace run 7). The mirrored-mapping control reads snapshots 2.503223408264881, centre 2.291526948382731. These numbers are from the Mac; the record's libm numbers come from CI's host (R9) and may differ. | this session's log |
| F11 | **Coverage, measured from the reference** (scratch Python over `data/flyvis-reference/`, reproduced independently by the fidelity review; by F10 these equal the values from Rust's exact traces). Movement per cell type = max over the 12 sequences and every frame of \|centre − settled centre\|, widened to f64. In every run the weakest type is Tm30, in sequence 5: runs 0/1 3.801e-3, runs 2/3 3.773e-3, runs 4/5 3.248e-2, runs 6/7 3.179e-2. The next weakest are Mi11 1.342e-2 (runs 0/1) and 1.796e-2 (runs 2/3), and L2 4.600e-2 (runs 4–7). The medians are 0.497, 0.708, 0.798 and 0.896. Against the f32 libm centre bound, the weakest type's movement is 1102×, 744×, 625× and 462× in runs 0, 2, 4 and 6. Against the f64 bound it is 3.8e7× to 3.2e8×. The f64 runs match their f32 twins to 4 significant figures. | this session; fidelity review, "Verified" |
| F12 | `PROVENANCE.json` has these top-level keys: `exported`, `export_id`, `generator`, `git`, `versions`, `machine`, `uv_lock_sha256`, `device`, `threads`, `flyvis_root`, `models`, `validation_losses`, `inputs_read`, `dts`, `settling`, `stimuli`, `f32_f64_gap`, `f32_floor_trap`, `freeze_check`, `parameter_equality`, `equivalence`, `input_rounding`, `sizes`, `wall_time_s`. `threads` is `{"default":12,"determinism_check":[1,12],"bit_equal":true,"runs_checked":[0,1]}`. The JSON reader is private and keeps object keys in file order. `ReferenceError::Missing` displays as `manifest has no {p}`. | the file; `json.rs:7-15, 351-411`; `reference.rs:118, 141` |
| F13 | The acceptance tests run the crate's gates by name in shadow workspaces and match their output. The constraining strings are listed in [§2](#2-the-gates-become-wrappers). `settling_checks::` must hold exactly 20 tests. MINORS-12 reads the doc comment of `the_gate_resolves_integer_counts` from `settling_checks.rs`. MINORS-03 requires each gate's skip notice to start `skipping: <root>/crates/flyvis/../../data/<rel>`. MINORS-05 fails any `.rs` under `crates/` that spawns a process other than cargo. MAPPING-08 requires a mapping-only run's output not to contain `settling_checks` or `trace_checks`. The shadows build `--locked`. | `acceptance_traces.rs:1095-1186`; `acceptance_minors.rs:418-447, 536-556, 641-654, 713`; `acceptance_mapping.rs:421` |
| F14 | The shadow `copy_tree`s copy directories and regular files only, so a symlink is skipped. | `acceptance_settling.rs:297-313` and its siblings |
| F15 | No package contains LICENSE or NOTICE. `include = ["../../LICENSE"]` is dropped without a warning. Tracked symlinks are dereferenced into the `.crate`. flyvis compiles in flyvis's MIT data. NOTICE omits `syn_count_torch.rs`, and lists flyvis data files but not the code ported from flyvis (`hex_eye.rs`'s fp16 mask construction, the connectome expansion). From an extracted package, the flyvis and sim `tests/` cannot run or skip silently. `exclude = ["tests/"]` works, with one warning per file. | piece3-packaging.md §1, §2, §4; fidelity review M9 |
| F16 | `[workspace.dependencies]` pins `version = "0.2.0"` for all 7 libraries, so the bump must change all 7 as well. `cargo publish --workspace` publishes in the order rng, connectome, codec, sim, evo, fidelity, flyvis, semblance. | `Cargo.toml`; piece3-packaging.md §3 |
| F17 | crates.io has six crates at 0.1.0, none yanked. evo 0.1.0 depends on `rand ^0.10`. Yanking codec 0.1.0 alone strands evo, fidelity and semblance 0.1.0. | piece3-packaging.md §6 |
| F18 | The hosted CI test job took 12m32s of its 20-minute limit. The test step took 689 s. `acceptance_minors` (flyvis) took 257 s, almost all of it MINORS-15's serial re-run of `trace_checks::` (102.8 s locally). `acceptance_traces` took 253 s, of which TRACES-06's eight one-at-a-time nested runs were about 51 s locally. SEQUENCES-08's nested cargo duplicates tests the suite already runs. The `target` cache costs a 25 s upload and never saves a rebuild. Cargo runs test binaries one after another, so a new binary adds its full wall time. | piece3-ci-time.md §1–§5 |
| F19 | Doc drift: CLAUDE.md says 332 tests and 66 skips, while the stimulus-traces PR body says 429 and 123. README says 123 and names SEQUENCES-08 as the one silent skip. README's flyvis row says "the result record (0.3.0, in progress)". `CITATION.cff` says `version: 0.2.0`. The sim description says "LIF baseline". The stimulus-traces spec's H12 says Tm30 moves 3.747e-3 in run 3; F11 measures 3.773e-3. | the files |
| F20 | `ci.yml` pins every action to a full commit SHA with a version comment, and uses GitHub-owned actions only. `actions/upload-artifact`'s `v4` tag and its `v4.6.2` tag both point to commit `ea165f8d65b6e75b540449e92b4886f43607fa02` (checked 2026-10-29 with `gh api repos/actions/upload-artifact/git/ref/tags/v4` and the `v4` tag listing). | `.github/workflows/ci.yml`; `gh api` |
| F21 | Rust's `{:.3e}` and `{:e}` print exponents without padding or a plus sign: `4.768e-7`, `2.503e0`, `1e-3`, `2.8228733750523816e-6`. | implementability review S1 |

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
- Threads are optional (`std::thread::scope`), and their results are
  bit-identical to a serial run.
- The `#[cfg(test)]` gates become thin wrappers over it, and every string the
  acceptance tests match stays byte for byte as it is.
- `semblance-sim` becomes a normal dependency of `semblance-flyvis`.
  `Cargo.lock` stays at 8 packages.

**R2 (D2): the record is `docs/results/0.3.0-flyvis-parity.md`.** It is a
hand-written preamble followed by `semblance validate --markdown`'s output. A
fixture-gated test regenerates the output and compares it byte for byte with
the committed file. Two parts are excluded from that comparison: the
host-specific section always, and the libm section on a host whose libm
fingerprint differs from the record's, where that is allowed at all (R9).

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
- CLAUDE.md: the version, the re-measured counts, `validate`, the record and
  `SEMBLANCE_REQUIRE_LIBM_RECORD`, and the yank sentence.
- The drift in F19.
- The head edits the local outline. It is not a task in this spec.

**R6: formatting lives in `app`, computation in `parity`.** The library
returns structs. The terminal table and the Markdown are `app` code
(`app/src/validate.rs`). The gate wrappers keep their own format strings,
moved unchanged. So a change to the CLI's layout cannot move an acceptance
string, and the reverse holds too.

**R7: exit 2 means "verification did not complete".** That covers missing,
unreadable, malformed or mismatched fixtures. It also covers a bad `validate`
command line: an unknown flag, a missing value, or `--threads 0`. Nothing in
such a run counts as verified, even if some jobs finished before the error;
that is the distinction a script needs. An unknown *command* keeps today's
behaviour, exit 1 (F1).

**R8 (owner, Q1): the coverage threshold is an absolute 1e-3.** A cell type
counts as driven in a run if its centre trace moves by more than 1e-3
(`movement > COVERAGE_THRESHOLD`, strictly). It is recorded, not a gate.
- 1e-3 is the threshold the existing controls already use for "misses by far
  more than any bound" (`the_gate_tells_models_apart`, `settling_checks.rs:184`;
  `the_trace_gate_sees_a_wrong_mapping`, `trace_checks.rs:398`). It is 3.51×
  the largest bound applied anywhere (2 × 1.4233448473766064e-4 = 2.847e-4,
  run 6 snapshots).
- D2's wording, "every type moves ≥ ~3.7e-3, i.e. > 1000× the applicable
  bound", holds against the f64 bound (3.8e7× to 3.2e8×) and against run 0's
  f32 bound (1102×). It does not hold against the f32 libm centre bounds of
  runs 2, 4 and 6, which give 744×, 625× and 462× (F11).
- The record therefore states the absolute threshold and prints, for every
  run, the weakest type's movement divided by that run's libm centre bound,
  so the 744×, 625× and 462× cases stay visible and no reader takes "1000×"
  on trust. For the exact gate, whose bound is equality, any non-zero
  movement is visible.

**R9 (owner, Q2): the record is generated on hosted Linux CI, and its libm
sections are held to CI's libm.**
- The generated block has three kinds of section:
  - **deterministic**, compared byte for byte everywhere
  - **host-specific**, never compared: the semblance version, the target,
    debug assertions, the thread count, and the libm fingerprint
  - **libm**, compared when the running host's libm fingerprint equals the
    one recorded in the section's marker
- Rust's `f32::exp` and `f64::exp` call the platform's libm. So the libm
  maxima are facts about one host, like `gaussian()` in `semblance-rng`.
- Everything else is IEEE arithmetic on fixed inputs and is the same on every
  host: the exact rows, the controls, coverage, the reference provenance, and
  the bounds. `crates/sim/src/graded.rs` uses only `+ − × ÷`, `max` and casts,
  and nothing on the expansion or reading path calls libm (fidelity review,
  "Verified").
- **The recording host is CI's `test` job** (`ubuntu-latest`, Rust 1.97 as
  pinned in `ci.yml`), never the owner's Mac. A CI step runs
  `semblance validate --markdown` and uploads its stdout as an artifact
  (§8, "The record step"). The head commits it (§7, "Generating the record").
  That is the only way the record is generated or regenerated.
- CI's test step sets `SEMBLANCE_REQUIRE_LIBM_RECORD=1`. Under it a libm
  fingerprint mismatch fails the record test, so on CI the libm sections must
  match byte for byte.
- Anywhere else (the owner's Mac, a contributor's machine), a mismatch skips
  the libm section with a visible notice, and everything else is still
  compared.

**R10: no rustc version, no date in the generated block.**
- Neither is available without a build script, and a build script that shells
  out is what the stimulus-traces review removed.
- The host section uses `std::env::consts::{ARCH, OS}` plus `target_env` via
  `cfg!`, and `cfg!(debug_assertions)`.
- The preamble states by hand, on its `Generated:` line, the toolchain (Rust
  1.97 on `ubuntu-latest`, as `ci.yml` pins it), the CI run it came from, and
  the date.

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
SETTLING-11, TRACES-15), and `validate`'s messages, the record test's notice
and the tests' notices cannot drift apart.

**R14: the controls in `validate` reuse the tests' thresholds**, from the same
`pub const`s:
- the wrong model: settled max > `CONTROL_MISS` (1e-3)
- integer counts: settled max > `F64_BOUND`
- the mirrored mapping: snapshots max > `CONTROL_MISS`
- the start from flyvis's settled state: bit-equal

A control that does not hold, or that returned an error, is a gate failure
(exit 1).

**R15: `app/tests/` is excluded from the `semblance` package**, as `tests/` is
for flyvis and sim (F15). The new app tests read the workspace's `docs/` and
`data/`, which no package carries.

**R16 (owner, Q3): the record test runs the full `validate` on CI.** The
estimated 65–90 s is accepted. It is not limited to the exact jobs.

**R17 (head, on the fidelity review's M9): NOTICE credits the ported code as
well as the data.** NOTICE gains a line saying that parts of
`semblance-flyvis` are ports of flyvis's algorithms (the connectome
expansion, the hex-eye fp16 mask construction, the ground-truth readers),
MIT-licensed, attribution only. No licence changes: the crate is already
`Apache-2.0 AND MIT` (§9).

**R18: every pass predicate fails closed.** A result that is absent, errored,
short or empty never counts as passed (§1, "Validating"). `validate` refuses
an empty or duplicated run list as `Usage`, and the CLI and the record test
require the runs to be exactly 0..8.

**R19: precision comes from the registry, not the manifest.** Every entry
point dispatches on `RUNS[run].3` and checks the manifest's precision against
it, so the check `run {run}'s precision: manifest {a}, expected {b}` keeps an
independent source for `b` (F4).

**R20: one core, three entry points.** `settle`, `play` and `run` are all thin
over the same private functions, so the gates exercise exactly the code
`validate` runs (§1, "Running").

**R21: a broken fixture never becomes a skip.** Only a *missing* file is a
skip. A fixture that is present but malformed fails every gate that uses it,
with or without `SEMBLANCE_REQUIRE_FIXTURES` (§2).

**R22: the preamble is held to the generated block by test.** Every measured
number the preamble quotes must appear verbatim in the generated block (§7).

## Design

### 1. `semblance_flyvis::parity`

A new directory module, `crates/flyvis/src/parity/`, declared `pub mod parity;`
in `lib.rs`, so the implementation waves can split it by file:

| File | Holds |
|---|---|
| `parity/mod.rs` | the module doc, the `mod` declarations and re-exports, the constants, `ParityError`, `After` |
| `parity/bounds.rs` | `Bound`, `settling_bounds`, `trace_bounds` |
| `parity/inputs.rs` | `Fixtures`, `TraceData`, `TraceInputs`, `required_files`, `how_to_get` |
| `parity/run.rs` | `Counts`, `Start`, `Diff`, `diff`, the private core, `settle`, `play`, `run`, and the `pub(crate)` helpers listed below |
| `parity/checks.rs` | the fixture checks, `check_settling_gaps`, `check_trace_gaps` |
| `parity/validate.rs` | `Options`, `Report`, `Job`, `Controls`, `Coverage`, `validate` |
| `parity/fingerprint.rs` | `LibmFingerprint`, `libm_fingerprint`, FNV-1a |

The paths below name items as `parity::X`; the re-exports make that true
whichever file holds them.

Every public struct and enum in `parity` derives `Clone, Debug, PartialEq`
(`LibmFingerprint`, `Counts`, `Start` and `Bound`'s fieldless parts also
`Eq` where the fields allow). `ParityError` holds `io::ErrorKind`, which is
`PartialEq`. The thread test compares reports with `assert_eq!`, which needs
both.

The code builds on Rust 1.85 (the `msrv` job builds `--all-targets`): no
let-chains and no std API stabilised after 1.85.

**Constants.** These are the only copies; the gate wrappers import them.

```rust
pub const F64_BOUND: f64 = 1e-10;
/// (model, dt id, settled, settling_centre): PROVENANCE `f32_f64_gap`, as registered.
pub const SETTLING_F32_GAPS: [(&str, &str, f64, f64); 4] = [/* moved verbatim from settling_checks.rs:38-43 */];
/// (model, dt id, centre, snapshots), as registered.
pub const TRACE_F32_GAPS: [(&str, &str, f64, f64); 4] = [/* moved verbatim from trace_checks.rs:48-53 */];
/// "Misses by far more than any bound": the model and mapping controls.
pub const CONTROL_MISS: f64 = 1e-3;
/// Sintel revisit trigger 1: a cell type is driven if its centre trace moves by more than this (R8). Recorded, not a gate.
pub const COVERAGE_THRESHOLD: f64 = 1e-3;
/// The eight registered runs: (index, model, dt id, precision). The source of every run's precision (R19).
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
    /// max |rust - flyvis| <= 2.0 * gap. `model` and `dt` are the registered
    /// table's own `&'static str`s; the PROVENANCE key is "{model} {dt}".
    /// `quantity` is one of "settled", "settling_centre", "centre", "snapshots".
    F32 { gap: f64, model: &'static str, dt: &'static str, quantity: &'static str },
}
impl Bound { pub fn value(&self) -> f64; /* 0.0, F64_BOUND, 2.0 * gap */ }

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
    /// `view(F32)` borrows centre.f32.bin and snapshots.f32.bin, `view(F64)` the
    /// f64 pair; both borrow input.f64.bin. `precision` is set to `p`.
    pub fn view(&self, p: Dtype) -> TraceInputs<'_>;
}
/// One precision's trace inputs, borrowed. The test wrappers build this from
/// their lazy `OnceLock`s (traces_13), read in today's order: sequences,
/// input, centre, snapshots (TRACES-13).
pub struct TraceInputs<'a> {
    pub precision: Dtype,
    pub sequences: &'a [SequenceInfo], pub input: &'a [u8], pub centre: &'a [u8], pub snapshots: &'a [u8],
}

/// The 13 paths under the data dir that `validate` needs (R11).
pub fn required_files() -> &'static [&'static str];
/// Moved verbatim from test_support.rs:38-63, all six arms (R13).
pub fn how_to_get(rel: &str) -> &'static str;
```

`Fixtures::from_parts` replaces today's `expect`s (`gate_support.rs:55-61`)
with errors. Neither constructor reads `PROVENANCE.json`: the gate fixture does
not depend on it today (MINORS-03's `PROVENANCE_ONLY`, SETTLING-11), and that
stays so. `validate` reads it separately.

The per-run checks (dt, `n_settle`, the settled and `settling_centre` shapes,
the per-sequence input, centre and snapshot shapes) stay inside the core that
`settle` and `play` call. They are not moved into `from_parts`. SETTLING-05 and
TRACES-05 match their substrings through the gate wrappers, which never call
`validate`'s fixture checks.

**Provenance**, in `reference.rs`, keeping `json` private. Each field names the
JSON path it reads:

```rust
pub struct Provenance {
    pub exported: String,                    // exported
    pub export_id: String,                   // export_id
    pub generator_path: String,              // generator.path
    pub generator_sha256: String,            // generator.sha256 (64 hex, printed in full)
    pub git_head: String,                    // git.head (40 hex, printed in full)
    pub git_dirty: bool,                     // git.dirty
    pub versions: Vec<(String, String)>,     // versions, in file order
    pub platform: String,                    // machine.platform
    pub machine: String,                     // machine.machine
    pub device: String,                      // device
    pub uv_lock_sha256: String,              // uv_lock_sha256
    pub threads_default: u32,                // threads.default
    pub threads_checked: Vec<u32>,           // threads.determinism_check
    pub threads_bit_equal: bool,             // threads.bit_equal
    pub threads_runs_checked: Vec<u32>,      // threads.runs_checked
    pub models: Vec<ProvenanceModel>,        // models[]: id, name, rank, validation_loss, chkpt_00000_sha256
    pub all_checkpoints_equal_ensemble: bool,// parameter_equality.all_50_checkpoints_equal_ensemble_csv
    pub stimulus_response_bit_equal: bool,   // equivalence.stimulus_response_bit_equal
    pub equivalence_sequences: Vec<u32>,     // equivalence.sequences
    pub input_rounding: Vec<(String, f64)>,  // input_rounding, per dt id, in file order
    pub gaps: Vec<GapEntry>,                 // f32_f64_gap: key, settled, settling_centre, centre, snapshots
    pub sizes: Vec<(String, u64)>,           // sizes
}
impl Provenance { pub fn parse(json: &str) -> Result<Provenance, ReferenceError>; }
```

The parser is strict about these fields and ignores the rest (F12). A missing
or mistyped field is a `ReferenceError` that names it. Because
`ReferenceError::Missing` displays `manifest has no {p}` (F12), the parser uses
a new variant, `ReferenceError::ProvenanceMissing(String)`, displayed
`PROVENANCE.json has no {p}`, with `{p}` the JSON path above.

**Running.** The entry points are non-generic. Each dispatches to a private
generic over `Precision`, chosen by the run's **registered** precision,
`RUNS[run].3` (R19), so monomorphisation happens in flyvis (F8). Before it
dispatches, it checks the manifest's precision for that run against
`RUNS[run].3` and returns `Mismatch("run {run}'s precision: manifest {a},
expected {b}")` if they differ. `play` and `run` also check
`t.precision == RUNS[run].3`, else `Mismatch("trace inputs are {a}, run {run}
is {b}")`.

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
    pub per_type_max: Vec<f64>,         // settled max |error| per cell type, in the network's cell-type order (Report::cell_types)
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
    pub sequences: Vec<SequenceResult>,   // all 12, in sequence order
    pub centre_movement: Vec<f64>,        // per centre type, CELL_TYPES order; see "Coverage" below
}

pub fn settle(f: &Fixtures, run: u32, model: &str, counts: Counts) -> Result<SettlingResult, ParityError>;
pub fn play(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts, start: Start,
            sequences: &[u32], mirror: bool) -> Result<Vec<SequenceResult>, ParityError>;
/// Settle once, then play all 12 sequences from that state: both stages from one settle.
pub fn run(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts) -> Result<RunResult, ParityError>;
```

**One core (R20).** `run.rs` has three private generic functions:
`settle_state` (build the integrator and settle it), `compare_settling` (the
settling comparison and `SettlingResult`), and `play_from(state, …)` (play
sequences from a given settled state and compare). `settle` is
`settle_state` then `compare_settling`. `play` is `settle_state` (or flyvis's
settled state, for `Start::Flyvis`) then `play_from`. `run` is
`settle_state`, `compare_settling`, then `play_from` on the same state. No
entry point has arithmetic of its own. A fixture-gated test requires `run(f,
t, 3, Torch)` to equal `settle(f, 3, "000", Torch)` together with `play(f, t,
3, Torch, Start::Rust, &[0..12], false)`, field for field.

The steps, their order of operations, the zeroed input buffer, the
non-finite scan after every step, and the shape checks are piece 1's and
piece 2's, moved without change. `diff` keeps its length `assert_eq!` as an
internal invariant. Every caller compares only arrays whose shapes it has
already checked, so bad input reaches the shape check first and is returned as
an error. `parse_sequences` already refuses non-ascending or out-of-range
snapshot frames (`reference.rs:228-229`), so the assert cannot be reached
through them.

**Signed zeros.** The comparison is `==` on f64-widened values (R4). That is
bit equality only when no reference value is ±0.0 (NaN already shows up
through `differ` and the infinite maximum). Today's reference has none. The
core therefore checks every flyvis value it reads for ±0.0 and returns
`Mismatch("{file} {key}: reference value {i} is ±0.0; == is bit equality only
without signed zeros")`, so "bit for bit" stays literally true for any
fixture that passes. The `==` itself is unchanged.

**Errors from construction.** A `GradedError` from `Graded::new` or
`set_state`, or a `WiringError` from `scatter_hexals`, is fixture data that
does not fit the network: it becomes `Mismatch(format!("run {run}: {e}"))`.

**Helpers the test code keeps using**, `pub(crate)` in `parity/run.rs`:
`Precision` (with `DTYPE`, `SUFFIX`, `read`, `torch_count`, `libm_count`),
`cell_type`, `file`, `HEXALS`, `SNAPSHOTS` and `mirrored_hexals`.
`snapshots_hold_the_centre_column`, `stimulus_checks` and the wrappers'
C2/C3/C10/C11 formatting use them.

**Validating.**

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Options { pub threads: usize /* >= 1 */, pub runs: Vec<u32> /* non-empty, distinct, each < 8 */ }
impl Default for Options { /* threads: 1, runs: 0..8 */ }

pub struct Report {
    pub provenance: Provenance,
    pub runs: Vec<u32>,                  // as requested
    pub cell_types: Vec<String>,         // the network's cell types, in its order
    pub node_cell_type: Vec<u32>,        // per node, an index into cell_types
    pub jobs: Vec<Job>,                  // runs x [Torch, Libm], in (run, counts) order
    pub controls: Controls,
    pub coverage: Vec<Coverage>,         // one per run, from its Torch job
    pub libm: LibmFingerprint,
    pub threads: usize,
}
impl Report { pub fn cell_type(&self, node: usize) -> &str; }

pub struct Job { pub run: u32, pub model: String, pub dt: String, pub precision: Dtype, pub counts: Counts,
                 pub settling_bounds: (Bound, Bound), pub trace_bounds: (Bound, Bound),
                 pub outcome: Result<RunResult, ParityError> }
pub struct Controls {
    pub wrong_model: Result<Diff, ParityError>,                // settle run 3 (000, 1/50, f64) with 049's parameters, torch counts
    pub integer_counts: Result<Diff, ParityError>,             // settle run 3 with edge_n_syn
    pub mirrored: Result<SequenceResult, ParityError>,         // run 3, sequence 8, mirrored hexals
    pub from_flyvis: Result<SequenceResult, ParityError>,      // run 3, sequence 8, Start::Flyvis
}
pub struct Coverage { pub run: u32, pub movement: Option<Vec<f64>> /* per centre type; None if the Torch job did not pass */ }
pub struct LibmFingerprint { pub f32_differ: usize, pub f64_differ: usize, pub f32_fnv: u64, pub f64_fnv: u64 }

pub fn validate(data: &Path, opts: &Options) -> Result<Report, ParityError>;
pub fn libm_fingerprint() -> LibmFingerprint;
```

**The pass predicates (R18).** Each is false unless every part of it is
present and holds:
- `Job::passed`: `outcome` is `Ok`, its `settling.run` and `settling.counts`
  equal the job's, `sequences.len() == 12` with sequences 0–11 in order, and
  then for an exact job every `differ == 0` (settled, settling centre, and
  each sequence's centre and snapshots), or for a libm job every `max <=` its
  bound's `value()`.
- `Controls::passed`: all four fields are `Ok`, and each meets R14's
  threshold.
- `Report::passed`: `jobs.len() == 2 * runs.len()`; every (run, counts) pair
  for `runs` × [Torch, Libm] appears exactly once; every job passed; the
  controls passed; and `coverage` has one entry per run.
- `Report::complete`: `runs == [0, 1, …, 7]`. The CLI exits 0 only if
  `passed() && complete()`; the record test requires both.

`Options` with `threads == 0`, an empty `runs`, a run ≥ 8, or a repeated run
is `ParityError::Usage`.

`validate` works in four steps:

1. **Presence.** It stats all 13 required files and returns
   `ParityError::Missing` for the first absent one. The app reports every
   absent one: it stats them itself through `required_files()`, so a user
   fixes them all in one go.
2. **Fixture checks** (`checks.rs`). These run before any arithmetic, and each
   failure is a fixture error:
   - the files parse
   - the manifest's runs equal `RUNS`
   - every centre node index is `< n_nodes`, checked before it is used to
     index `node_cell_type()`; otherwise `Mismatch("centre node {k}: node {n}
     is outside the network's {n_nodes} nodes")`
   - the centre nodes match the network's cell types, in `CELL_TYPES` order
   - there are 12 sequences, indices 0–11, each with 6 snapshot frames at
     every dt
   - every block the jobs will read has the shape they expect
   - each file's byte length equals both its manifest `bytes` and
     PROVENANCE `sizes`
   - `check_settling_gaps` and `check_trace_gaps` pass
   - PROVENANCE's four booleans read as registered: `git.dirty` is false,
     `threads.bit_equal`, `parameter_equality.all_50_checkpoints_equal_ensemble_csv`
     and `equivalence.stimulus_response_bit_equal` are true. Any other value
     is `Mismatch("PROVENANCE {path} is {v}: the reference is not the one
     registered")`. So the record's "clean", "bit-equal" and "equal" are
     checked, never fixed text.
3. **Jobs.**
   - The 2 × `runs.len()` jobs run in `std::thread::scope`, on
     `min(threads, jobs)` workers. Each worker takes the next job index from
     an `AtomicUsize` and writes its result into that job's slot.
   - Each job is single-threaded and reads only shared, immutable inputs, so a
     result does not depend on which thread ran it or when.
   - After the pool finishes, `validate` checks every slot: an `Ok` outcome
     whose `settling.run` or `settling.counts` differs from the slot's job is
     replaced by `Err(Mismatch("job slot {i} holds run {r} {counts:?}"))`,
     never counted as a pass.
   - Workers are started with `thread::Builder::spawn_scoped`. If spawning
     fails, the remaining jobs run on the calling thread. Nothing panics.
   - A job's error is stored in its `outcome`, and the other jobs carry on.
4. **Controls and coverage.**
   - Four small jobs, run on the same pool after the main ones: two settles at
     1/50 and two one-sequence plays at 1/50. Each stores its own `Result`.
   - Coverage is read from the Torch jobs' `centre_movement`, for runs whose
     Torch job passed. For any other run it is `None`.

**Coverage, exactly.** For centre type `k` (in `CELL_TYPES` order) with centre
node `n_k`, `centre_movement[k]` is the maximum over the 12 sequences and every
frame `i` of `|(centre_rust[s][i][k] as f64) − (settled_rust[n_k] as f64)|`:
widened to f64 first, then subtracted, combined with `f64::max`. The state is
already scanned for non-finite values after every step, so every term is
finite. Formatting (`app`) then uses:
- weakest, next two: the three smallest by `f64::total_cmp`; an exact tie goes
  to the lower cell-type index
- the overall weakest across runs: the smallest by `total_cmp`; an exact tie
  goes to the lowest run index, then the lowest cell-type index
- median: the 65 values sorted by `total_cmp`, element 32
- driven: `movement > COVERAGE_THRESHOLD` (R8)

**Libm fingerprint** (`fingerprint.rs`). It is computed directly, not through
`Precision::libm_count` or any gate code: for every group `g` in order,
`f32::exp(black_box(SYN_COUNT[g]))` and `f64::exp(black_box(SYN_COUNT[g] as
f64))`. (`std::hint::black_box` keeps the compiler from folding the call at
build time, so the value is the running host's libm.) For each precision it
gives:
- how many of the 2,355 groups have that value's bits differing from torch's
  table (`SYN_COUNT_TORCH_F32` / `_F64`). G17 measured 104–106 in f32 and 108
  in f64 on the Mac.
- a 64-bit FNV-1a over the little-endian bits of all 2,355 values, in group
  order. FNV-1a is five lines, hand-written.

A unit test requires `<f32 as Precision>::libm_count(g).to_bits()` and
`<f64 as Precision>::libm_count(g).to_bits()` to equal the fingerprint's own
values for every `g`. So if the gate's count formula changes, that test fails;
the fingerprint does not silently follow it, and the libm sections cannot
switch themselves off.

It identifies "the same libm" for R9. It is not a security hash. It prints as
`f32:{f32_differ}:{f32_fnv:016x},f64:{f64_differ}:{f64_fnv:016x}` wherever it
appears.

**Errors.**

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum ParityError {
    Missing { rel: String, path: PathBuf },
    Unreadable { rel: String, path: PathBuf, kind: std::io::ErrorKind },
    Malformed { rel: String, detail: String },        // wraps SpecError, ExpandError, WiringError, EnsembleError, ReferenceError text
    Mismatch(String),
    NonFinite { run: u32, node: usize, cell_type: String, after: After },
    Usage(String),                                    // a bad Options value (threads 0, no runs, a repeated run, a run >= 8)
}
#[derive(Clone, Debug, PartialEq)]
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
- `run {run}'s precision: manifest {a}, expected {b}` (`b` from `RUNS`, R19)
- `trace inputs are {a}, run {run} is {b}`
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
- `{file} {key}: reference value {i} is ±0.0; …`
- `PROVENANCE {path} is {v}: the reference is not the one registered`
- `job slot {i} holds run {r} {counts:?}`
- `run {run}: {GradedError or WiringError}`
- `no registered bound for model {m} at dt {dt}`

`parity` never reads `SEMBLANCE_REQUIRE_FIXTURES`. That variable belongs to
the test harness. A missing fixture is always `Missing` for the library and
always exit 2 for the command.

### 2. The gates become wrappers

`gate_support.rs` shrinks to the test fixture:
- a `OnceLock<Result<Fixtures, String>>` filled through `test_support`'s
  reads, in today's order
- then `Fixtures::from_parts`

**The `String` is only ever `test_support`'s missing-file notice (R21).** The
initialiser is a pure function over the reads' results,
`fn build(reads: Result<Parts, String>) -> Result<Fixtures, String>`: a read
that reports a missing file passes its notice through as `Err`, and the gates
skip with it as today. A `from_parts` error is never converted to a `String`:
`build` panics with `"{e}"`, which leaves the `OnceLock` empty, so every gate
test that uses the fixture fails with it, with or without
`SEMBLANCE_REQUIRE_FIXTURES`. A truncated `ensemble.csv` or an unparsable
manifest therefore fails 35+ gates instead of letting them pass while
verifying nothing. A unit test calls `build` with an empty ensemble and
requires the panic (`#[should_panic]`).

`settling_checks.rs` and `trace_checks.rs` keep:
- every test name, module path and doc comment
- their print and panic format strings, moved verbatim
- their macros' `$t:ty` parameter. Each generated test asserts
  `<$t as Precision>::DTYPE == parity::RUNS[$run].3` before calling `parity`.
  That keeps the name-to-precision cross-check F4 describes, and keeps `$t`
  used (clippy's `extra_unused_type_parameters` under `-D warnings`).

They call `parity::settle`, `parity::play` and the bounds functions, and
format the returned structs. On `Err(e)` they `panic!("{e}")`. `test_support`
keeps its loaders and notices, and calls `parity::how_to_get` (R13). The
trace wrappers keep their per-file `OnceLock`s, read in today's order
(sequences, input, centre, snapshots), and build a `TraceInputs` with its
`precision`. `the_gate_resolves_integer_counts` and its doc comment stay in
`settling_checks.rs` (MINORS-12). Neither module gains or loses a test
(TRACES-14 requires exactly 20 in `settling_checks::`, and MINORS-15 requires
"20 passed"). Neither file may produce a compiler warning (C17).

The extraction lands as its own commit. The shadow acceptance suites
(`acceptance_settling`, `acceptance_traces`, `acceptance_minors`,
`acceptance_mapping`) must pass on that commit before `validate` exists.

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
| C17 | the absence of `settling_checks` and `trace_checks` in a mapping-only run's output: neither file may emit a compiler warning | MAPPING-08 |
| C18 | libtest's counts, matched literally: `8 passed` (MINORS-04), `test result: ok. 20 passed; 0 failed` (MINORS-15), `running 1 test` (every shadow), `running 8 tests` (TRACES-06 after P2) | the shadows |

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
- Each task that edits a manifest's dependencies commits the regenerated
  `Cargo.lock` in the same commit, because the shadows build `--locked` (F13).

### 4. The command

`app/src/main.rs` gains `"validate" => validate::cmd(&args[2..])`. The new
`app/src/validate.rs` parses the flags and formats the output (R6).

```
semblance validate [--data DIR] [--threads N] [--markdown]

  --data DIR     the fixture directory (default: ./data)
  --threads N    worker threads, N >= 1 (default: available parallelism, at most 16)
  --markdown     print the record's generated block instead of the table
```

- Each flag may appear at most once. `--threads` takes a decimal integer ≥ 1.
- Any other argument is a usage error: the message and the usage line go to
  stderr, and the exit code is 2 (R7).
- The default thread count is `available_parallelism()` (or 1 if unknown),
  capped at 16, the number of main jobs. An explicit `--threads` above 16 is
  accepted and capped the same way inside `validate` (`min(threads, jobs)`).
  Results do not depend on it.
- `cmd` always runs all eight runs (`Options::default().runs`); there is no
  flag to change that, and it requires `Report::complete()` for exit 0.
- Output goes to stdout. Diagnostics and missing-file lists go to stderr.
- stdout is written with `writeln!` on a locked handle, never `println!`.
  An `ErrorKind::BrokenPipe` (`validate | head`) stops the output and the
  command exits with the code it had computed; it never panics, which in a
  release build would abort (F3). Any other write error is exit 2.

**Exit codes.**

| Code | Meaning |
|---|---|
| 0 | all eight runs ran; every exact job is bit-equal, every libm job is within its bounds, and every control holds |
| 1 | a gate failed: an exact value differs, a libm bound is exceeded, a control does not hold or errored, or a state went non-finite. The full table still prints, with each failure marked. |
| 2 | verification did not complete: a fixture is missing, unreadable, malformed or mismatched, or the command line is wrong. Nothing counts as verified. |

If any fixture-class error occurs, 2 wins over 1. A panic, which would be a bug,
exits 101 (debug) or 134 (release abort): never 0 (F3).

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
example uses the numbers measured on the Mac on 2026-10-29 (F9, F10, F11).

- Every float prints with Rust's native `{:.3e}` (F21), except the
  thresholds, which print with `{:e}` of their `pub const` (`1e-3`, `1e-10`),
  and the models' validation losses, which print `{:.3}`.
- `…` marks a value that the run fills in but that was not in the logs this
  spec was written from: the settling libm rows' total unequal count (settled
  plus centre), the control maxima, and the libm fingerprint.
- `max err` is the maximum of whichever quantity has the worse ratio, and
  `where` names that quantity.

```
semblance validate: does this build reproduce flyvis? (epic 0.3.0)
data:        ./data
reference:   export 20261027T114358Z-63037 (2026-10-27); flyvis 1.2.0, torch 2.14.0; macOS-26.6.2-arm64-arm-64bit, cpu
models:      000 flow/0000/000 (best, validation loss 5.137), 049 (worst, validation loss 5.678)
this build:  semblance 0.3.0, aarch64-macos, debug assertions off, 16 threads
libm exp:    f32 differs from torch's on … of 2355 counts, f64 on … (fingerprint f32:…:…,f64:…:…)

stage     run  model  dt     prec  counts  compared  unequal    max err  where                          bound      ratio
settling  0    000    1/200  f32   exact      58669        0          0  -                              ==         -
settling  0    000    1/200  f32   libm       58669        …   4.768e-7  centre, step 3 (TmY4)          2.695e-6   1.769e-1
settling  1    000    1/200  f64   exact      58669        0          0  -                              ==         -
settling  1    000    1/200  f64   libm       58669        …  1.110e-15  settled, node 16054 (Mi4)      1.000e-10  1.110e-5
settling  2    000    1/50   f32   exact      48919        0          0  -                              ==         -
settling  2    000    1/50   f32   libm       48919        …   8.345e-7  settled, node 15739 (Mi4)      2.553e-6   3.268e-1
settling  3    000    1/50   f64   exact      48919        0          0  -                              ==         -
settling  3    000    1/50   f64   libm       48919        …  2.220e-15  settled, node 15993 (Mi4)      1.000e-10  2.220e-5
settling  4    049    1/200  f32   exact      58669        0          0  -                              ==         -
settling  4    049    1/200  f32   libm       58669        …   3.710e-6  settled, node 16134 (Mi9)      1.232e-5   3.013e-1
settling  5    049    1/200  f64   exact      58669        0          0  -                              ==         -
settling  5    049    1/200  f64   libm       58669        …  9.409e-15  settled, node 16134 (Mi9)      1.000e-10  9.409e-5
settling  6    049    1/50   f32   exact      48919        0          0  -                              ==         -
settling  6    049    1/50   f32   libm       48919        …   1.669e-6  settled, node 43787 (TmY14)    1.289e-5   1.295e-1
settling  7    049    1/50   f64   exact      48919        0          0  -                              ==         -
settling  7    049    1/50   f64   libm       48919        …  7.994e-15  settled, node 44066 (TmY14)    1.000e-10  7.994e-5
traces    0    000    1/200  f32   exact    3717168        0          0  -                              ==         -
traces    0    000    1/200  f32   libm     3717168  1706062   9.537e-7  centre, seq 9 frame 302 (T2)   3.450e-6   2.764e-1
traces    1    000    1/200  f64   exact    3717168        0          0  -                              ==         -
traces    1    000    1/200  f64   libm     3717168  1792079  1.776e-15  snaps, seq 11 frame 314 (T2)   1.000e-10  1.776e-5
traces    2    000    1/50   f32   exact    3395288        0          0  -                              ==         -
traces    2    000    1/50   f32   libm     3395288  1844269   1.907e-6  snaps, seq 7 frame 104 (Mi4)   5.964e-6   3.198e-1
traces    3    000    1/50   f64   exact    3395288        0          0  -                              ==         -
traces    3    000    1/50   f64   libm     3395288  1870870  3.109e-15  snaps, seq 11 frame 78 (Mi4)   1.000e-10  3.109e-5
traces    4    049    1/200  f32   exact    3717168        0          0  -                              ==         -
traces    4    049    1/200  f32   libm     3717168  1112385   8.801e-5  snaps, seq 3 frame 359 (Mi4)   1.023e-4   8.605e-1
traces    5    049    1/200  f64   exact    3717168        0          0  -                              ==         -
traces    5    049    1/200  f64   libm     3717168  1490218  2.871e-13  snaps, seq 3 frame 359 (Mi4)   1.000e-10  2.871e-3
traces    6    049    1/50   f32   exact    3395288        0          0  -                              ==         -
traces    6    049    1/50   f32   libm     3395288   898830   1.526e-5  centre, seq 7 frame 124 (T4a)  6.884e-5   2.217e-1
traces    7    049    1/50   f64   exact    3395288        0          0  -                              ==         -
traces    7    049    1/50   f64   libm     3395288  1346567  3.121e-13  snaps, seq 3 frame 89 (Mi4)    1.000e-10  3.121e-3

controls:  wrong model (049 vs run 3)             settled max …                     must be > 1e-3   holds
           integer counts (run 3)                 settled max …                     must be > 1e-10  holds
           mirrored mapping (run 3, seq 8)        snapshots max 2.503223408264881e0 must be > 1e-3   holds
           flyvis's settled start (run 3, seq 8)  0 unequal                         must be ==       holds
coverage:  weakest cell type moves 3.773e-3 (Tm30, run 2); threshold 1e-3 (driven if above), recorded, not a gate:
           every type is driven in every run, so Sintel's revisit trigger 1 does not fire;
           weakest / libm centre bound, runs 0-7: 1.102e3 3.801e7 7.439e2 3.773e7 6.245e2 3.248e8 4.618e2 3.179e8
verdict:   PASS — 16 exact rows bit-equal, 16 libm rows within bounds (worst 8.605e-1, traces run 4), 4 controls hold  [… s]
```

Columns:
- `compared` is the number of values checked:
  - settling: 45,669 plus n_settle × 65
  - traces: Σ n_frames × 65 plus 12 × 6 × 45,669
- `unequal` is how many are not `==`.
- An exact row prints `0`, `==` and `-` unless it fails, and then it prints
  its maximum.
- A failing row ends in `FAIL`, and the verdict line names every failing job
  and control. A control that errored prints its error in place of its
  measurement and `FAIL`.
- A run whose exact job did not pass prints `coverage: run {r} not measured:
  exact job failed`.
- The coverage line prints each run's weakest-type ratio to its libm centre
  bound, `{:.3e}`, in run order.
- The trailing time is wall time, terminal only.

### 7. The record: `docs/results/0.3.0-flyvis-parity.md`

**Preamble, hand-written.** It covers:
- what 0.3.0 claims, and the three gates (exact, libm, controls)
- what "exact" and "libm" mean, and that "bit for bit" is `==` on values that
  the core has checked hold no ±0.0 (§1, "Signed zeros")
- the never-relax rule
- why the f64 ratios are about 1e-5 to 1e-3 and the f32 ones larger, quoting
  the worst ratio from the generated block
- that the libm rows are the recording host's facts: hosted CI's
  `ubuntu-latest` (R9), and that elsewhere they are bound-checked, not
  compared
- the fixtures (`fixtures-2`, `ci/fixtures.sha256`)
- one `Generated:` line, filled in by the head: the toolchain (Rust 1.97 on
  `ubuntu-latest`, as `ci.yml` pins it), the CI run URL, and the date (R10)
- that the exact gate needs the torch-count table `syn_count_torch.rs`, and
  what that table is
- the Sintel decision, citing the coverage section, with the threshold rule of
  R8 and the runs 2, 4 and 6 ratios quoted as the generated block prints
  them (`7.439e2`, `6.245e2`, `4.618e2`)

**The preamble numeral rule (R22).** A *numeral token* is a maximal run of the
characters `0-9 . e + -` that starts with a digit and either contains a `.`
or an `e` or has three or more digits. Tokens with two or more `.`
(versions such as `0.3.0`, `1.2.0`) are exempt, and so is every token on the
`Generated:` line. Every other numeral token in the preamble must occur
verbatim in the committed generated block (host section included, so
`2355` is allowed).
So the preamble quotes `8.605e-1`, never "0.86", and `7.439e2`, never
"744×".

**Generated block,** exactly what `semblance validate --markdown` prints on
CI, pasted below the preamble by the head (see "Generating the record"):

```markdown
<!-- BEGIN semblance validate --markdown -->
## Reference (flyvis side)
| | |
|---|---|
| export | 20261027T114358Z-63037, exported 2026-10-27 |
| exporter | tools/export_reference.py, sha256 e46fb05b37d9a4502fac3795bfde34e2d2f4d021e2b60cc5beea2ce9dd76b120; git c43f4b73517da4cf8208fd9b86f20e213355b825 (clean); uv.lock sha256 … |
| versions | python 3.12.11, flyvis 1.2.0, torch 2.14.0, numpy 2.5.3, h5py 3.16.0, datamate 1.0.0 |
| machine | macOS-26.6.2-arm64-arm-64bit (arm64), device cpu; default 12 threads, runs 0 and 1 bit-equal at 1 and 12 |
| models | 000 flow/0000/000, best, validation loss 5.136557016107771, checkpoint sha256 …; 049 …, worst, … |
| parameters | all 50 checkpoints equal ensemble.csv |
| equivalence | the export's forward path equals stimulus_response bit for bit (sequences 0, 4) |
| input rounding | 2.9801242251714655e-8 at 1/200 and 1/50 (the f32 runs' moving edges) |
| files | 9 reference files, each the size the manifest and PROVENANCE give |

<!-- BEGIN host-specific -->
## This run (Rust side)
| | |
|---|---|
| semblance | 0.3.0 |
| target | x86_64-linux-gnu |
| debug assertions | on |
| threads | 4 |
| libm exp | f32: … of 2355 counts differ from torch's; f64: … |
| libm fingerprint | f32:…:…,f64:…:… |
<!-- END host-specific -->

## Bounds as applied
| run | quantity | bound | derivation |
|---|---|---|---|
| all | every quantity, exact | == | torch's counts (syn_count_torch.rs); every value must equal flyvis's |
| 0 | settled, libm | 2.8228733750523816e-6 | 2 × f32_f64_gap["000 1/200"].settled = 2 × 1.4114366875261908e-6 |
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

<!-- BEGIN libm fingerprint=f32:…:…,f64:…:… -->
## Stage 1: settling, libm (Rust's exp)
| run | model | dt | precision | quantity | compared | unequal | max error | where | bound | ratio | worst types |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 000 (best) | 1/200 | f32 | settled | 45669 | 21628 | 4.768e-7 | node 15809 (Mi4) | 2.823e-6 | 1.689e-1 | Mi4 4.77e-7, T2 4.77e-7, TmY18 3.58e-7 |
| 0 | 000 (best) | 1/200 | f32 | settling centre | 13000 | … | 4.768e-7 | step 3 (TmY4) | 2.695e-6 | 1.769e-1 | |
| … (16 rows) | | | | | | | | | | | |

## Stage 2: traces, libm (Rust's exp), per stimulus
| run | model | dt | precision | stimulus | quantity | compared | unequal | max error | where | bound | ratio |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 4 | 049 (worst) | 1/200 | f32 | flashes | snapshots | 1096056 | … | 8.801e-5 | seq 3, frame 359, node 15414 (Mi4) | 1.023e-4 | 8.605e-1 |
| … (32 rows) | | | | | | | | | | | |
<!-- END libm -->

## Controls
| control | run | measured | must be | holds |
|---|---|---|---|---|
| model 049's parameters against model 000's reference | 3 | settled max … | > 1e-3 | yes |
| the spec's integer counts in place of exp(ln count) | 3 | settled max … | > 1e-10 | yes |
| hexals mirrored (u, v) → (−u, −v), sequence 8 | 3 | snapshots max 2.503223408264881e0, centre max 2.291526948382731e0 | > 1e-3 | yes |
| start from flyvis's settled.f64.bin, sequence 8 | 3 | 0 of … unequal | == | yes |

## Coverage: Sintel revisit trigger 1
Movement of a cell type = max over the 12 sequences and every frame of |centre trace − settled centre|, widened to f64, from the exact jobs (equal to flyvis's where the exact job passed). A type is driven if its movement exceeds 1e-3: recorded, not a gate.
| run | weakest type | movement | next two | median | libm centre bound | weakest / bound |
|---|---|---|---|---|---|---|
| 0 | Tm30 | 3.801e-3 | Mi11 1.342e-2, L4 3.932e-2 | 4.973e-1 | 3.450e-6 | 1.102e3 |
| 1 | Tm30 | 3.801e-3 | … | … | 1.000e-10 | 3.801e7 |
| 2 | Tm30 | 3.773e-3 | Mi11 1.796e-2, L4 5.688e-2 | 7.081e-1 | 5.072e-6 | 7.439e2 |
| 3 | Tm30 | 3.773e-3 | … | … | 1.000e-10 | 3.773e7 |
| 4 | Tm30 | 3.248e-2 | L2 4.600e-2, Tm28 4.818e-2 | 7.981e-1 | 5.200e-5 | 6.245e2 |
| 5 | Tm30 | 3.248e-2 | … | … | 1.000e-10 | 3.248e8 |
| 6 | Tm30 | 3.179e-2 | L2 4.600e-2, Tm28 4.869e-2 | 8.958e-1 | 6.884e-5 | 4.618e2 |
| 7 | Tm30 | 3.179e-2 | … | … | 1.000e-10 | 3.179e8 |

Every one of the 65 cell types moves by more than 1e-3 in every run (the weakest, Tm30 in run 2, by 3.773e-3): no type is undriven by flashes and moving edges, and trigger 1 does not fire. This covers the centre column only.
<!-- END semblance validate --markdown -->
```

(The f64 rows' movements above are their f32 twins' values to 4 significant
figures, F11; the record prints the f64 runs' own.)

The generated block is deterministic given the fixtures and, in its libm
section, the host's libm. It contains no wall time, date, path, or thread
count outside the host-specific section. Integers print without separators.
Bounds in "Bounds as applied" print with `{:e}` (shortest round-trip), and
so do the thresholds and the input rounding. Measured values, bounds and
ratios in the other tables print with `{:.3e}`; worst-type values with
`{:.2e}`, as C3 does; the mirrored control with `{:e}` in full, as the test
does (C14); validation losses with `{}` (shortest round-trip). All use Rust's
native exponent form (F21). The exporter and git SHAs print in full. The
input-rounding row prints one value "at 1/200 and 1/50" when the two are
equal, and otherwise `{a:e} at 1/200; {b:e} at 1/50`. A run whose exact job
did not pass prints `not measured: exact job failed` across its coverage row,
and the closing sentence then says which runs were not measured instead of
claiming every type is driven.

**Generating the record (R9).** The record is produced only on hosted CI:
1. `ci.yml`'s test job runs `target/debug/semblance validate --markdown`
   and uploads its stdout as the artifact `parity-record` (§8, "The record
   step").
2. The head takes the artifact from a run whose validate step exited 0:
   `gh run download <run-id> --name parity-record`.
3. The head replaces everything in `docs/results/0.3.0-flyvis-parity.md`
   from the `BEGIN semblance validate --markdown` marker to the `END` marker,
   inclusive, with the artifact's content (or appends it below the preamble
   if the file has no block yet). It fills the preamble's `Generated:` line
   with the run URL, the toolchain and the date. It edits nothing else and no
   number by hand.
4. The head commits that file alone and pushes. CI runs again; the record
   test must then pass under `SEMBLANCE_REQUIRE_LIBM_RECORD=1`.

**Bootstrap.** When `app/tests/record.rs` first lands, the record file holds
only the preamble (or, later, a stale block), so the record test fails on CI,
and locally, until the head has run steps 2–4 on that push's artifact. That
failure is expected and is the only failing test at that point. The head then
commits the artifact's output and re-runs CI, which must be green. Any later
change that moves a generated number goes through the same loop; nobody
regenerates the record on a workstation.

**The honesty test, `app/tests/record.rs`:** one integration test,
`the_committed_record_is_what_validate_prints`, plus unit tests of its text
functions on synthetic strings.

1. **The committed file, no fixtures needed.** These checks run first, on
   every host, and each is a failure, never a skip:
   - the file exists and contains the `BEGIN`/`END semblance validate
     --markdown` pair exactly once, the host-specific pair exactly once, and
     exactly one libm section (`BEGIN libm fingerprint=…` … `END libm`)
   - the libm marker's fingerprint parses
   - the libm marker's fingerprint equals the `libm fingerprint` row of the
     committed host section, so the two cannot be edited apart
   - the preamble numeral rule (R22)
2. **Fixtures.** If any of `semblance_flyvis::parity::required_files()` is
   absent under `{CARGO_MANIFEST_DIR}/../data`, the test prints
   `skipping: {path} not found. {how_to_get(rel)}` (R13) and passes. Under
   `SEMBLANCE_REQUIRE_FIXTURES=1` it panics instead with
   `SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing`.
3. **Run.** It runs `env!("CARGO_BIN_EXE_semblance") validate --markdown
   --data <workspace>/data` and requires exit 0.
4. **Extract.** It takes the text between the `BEGIN`/`END semblance validate
   --markdown` markers from the command's stdout, with the same
   exactly-once checks as step 1. The committed side is step 1's.
5. **Host section.** It reads the fingerprint from each libm `BEGIN` marker
   first, then deletes the host-specific section, markers included, from both
   texts.
6. **libm section.**
   - If the two fingerprints are equal, nothing more is removed.
   - If they differ and `SEMBLANCE_REQUIRE_LIBM_RECORD=1` is set, the test
     fails: `SEMBLANCE_REQUIRE_LIBM_RECORD=1 but this host's libm fingerprint
     {x} differs from the record's {y}`.
   - Otherwise it replaces the whole libm section, both markers included,
     with the one fixed line `<!-- libm sections not compared -->` in both
     texts, and prints with `eprintln!`
     `record: libm sections not compared: this host's libm fingerprint {x}
     differs from the record's {y}`. Like every skip notice, it is visible
     only under `--nocapture` (CLAUDE.md says so).
7. **Compare.** It compares the two texts byte for byte. A failure names the
   first differing line of each.

The test spawns the binary under test, not `tools/` and not git. MINORS-05
scans `crates/`, not `app/`, and this test would satisfy its intent anyway. It
reads no git history.

### 8. CI time (R4), and the record step (R9)

| | Change | Assurance |
|---|---|---|
| P1 | MINORS-15 drops its `trace_checks::` re-run. Its settling, `stimuli_checks::` and `moving_edge_checks::` parts stay. | The 22 trace tests still run directly under `SEMBLANCE_REQUIRE_FIXTURES=1`. TRACES-06 asserts all 8 exact lines, in the `12 of 12` form. TRACES-09 checks the libm line's format for runs 2 and 3. On CI the record test compares every number the record prints for all 16 trace jobs byte for byte, the libm ones included, because CI is the recording host (R9). Elsewhere the libm trace numbers are bound-checked (exit 0 requires every bound to hold, and every job to have 12 sequences, R18), not compared. What is lost: MINORS-15's check that the `trace libm run ` line prints for libm runs 0, 1 and 4–7, and that every exact line reports `total > 0`; the record's 12-sequence requirement and its compared row counts stand in for both. |
| P2 | TRACES-06 runs the 8 exact trace gates in one nested invocation: `cargo test -p semblance-flyvis --lib --locked --offline -- trace_checks::trace_exact_run_ --nocapture --test-threads=4`, with `SEMBLANCE_REQUIRE_FIXTURES=1` set as today's `sh.gate(name, true)` sets it. It asserts `running 8 tests`, success, and that the output contains each of the same 8 lines (C9), as today. | Nothing is lost: the same gates run, under the same flag, and the same strings are required. `eprintln!` holds the stderr lock for each line, and stdout and stderr are captured separately, so lines do not interleave. The in-process replay of 8 runs stays. |
| P3 | SEQUENCES-08 drops its nested `cargo`. Its in-process `Manifest::parse` and `parse_sequences` checks stay. It is renamed `sequences_08_the_manifest_and_sequences_parse_in_process` and its doc comment, and the module doc's SEQUENCES-08 sentence, say that `reference::tests` and `acceptance_reference` run directly in the same suite rather than nested. | `reference::tests::` and `acceptance_reference` already run in the same suite. With no fixture read, SEQUENCES-08 is no longer fixture-gated, so the skip count drops by one and README's sentence about it goes. |
| P4 | `.github/workflows/ci.yml`: the `test` job's `actions/cache` step no longer lists `target`. The `~/.cargo` paths stay (empty but harmless). The `msrv` job is unchanged, being off the critical path. | Nothing is lost. A fresh checkout makes every path dependency dirty, and every shadow re-copies its sources, so the cache never saved a build (F18). |

**The record step.** In the `test` job, after `Build` and before `Test, with
every fixture required`:

```yaml
      # The record's generated block, produced on this host (spec R9). The head
      # commits it from this artifact; nothing else generates the record.
      # A gate failure here fails the job; the upload still runs so the
      # output can be read.
      - name: Validate, writing the record's generated block
        run: target/debug/semblance validate --markdown > parity-record.md

      - name: Upload the record's generated block
        if: always()
        uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2
        with:
          name: parity-record
          path: parity-record.md
          if-no-files-found: error
```

and the test step's `env` gains `SEMBLANCE_REQUIRE_LIBM_RECORD: "1"`. The
action is GitHub-owned and pinned to the commit SHA that both `v4` and
`v4.6.2` resolve to (F20). The record test then compares the libm section on
CI byte for byte. A fingerprint mismatch on CI fails the job.

**Estimate, to be checked on the PR's first hosted run:**
- P1–P4 take the test step from 689 s to about 380–400 s.
- `app/tests/record.rs` adds one `validate` run with 4 threads on the 4-vCPU
  runner: about 255 CPU-s (F9's 111.5 s serial × the 2.3 serial factor), so
  about 65–90 s wall (R16).
- The record step runs `validate` once more, outside the test step: another
  65–90 s.
- The parity fixture tests add two `validate` runs of runs 2 and 3 (the two
  cheaper 1/50 runs, four jobs each, at 1 and at 4 threads) and two loads of
  all 13 files: about 60–80 s of CPU, partly hidden behind the other tests in
  the flyvis lib binary.
- The job goes from 12m32s to about 10.5–11.5 minutes, under its 20-minute
  limit.
- If the record test alone is over 150 s on CI, that is reported to the owner.
  It is not removed and no check is weakened.

### 9. The release (R3)

**Version.**
- `[workspace.package] version = "0.3.0"`, and every `version = "0.2.0"` in
  `[workspace.dependencies]` becomes `"0.3.0"` (F16).
- `Cargo.lock` is regenerated with `cargo build`: still 8 packages.
- `CITATION.cff`: `version: 0.3.0`, and `date-released` set to the tag date
  (the head sets it at tag time, below).

**Packaging** (piece3-packaging.md's list):

1. Tracked symlinks `LICENSE -> ../../LICENSE` and `NOTICE -> ../../NOTICE` in
   each of `crates/{rng,connectome,codec,sim,evo,fidelity,flyvis}/`, and
   `-> ../LICENSE` and `-> ../NOTICE` in `app/`. The shadows skip symlinks
   (F14), which does not matter to a build.
2. `crates/flyvis/Cargo.toml`: `license = "Apache-2.0 AND MIT"` in place of
   `license.workspace = true`.
3. NOTICE lists `crates/flyvis/src/syn_count_torch.rs`: "torch's `exp` of
   `SYN_COUNT` in f32 and f64, generated by tools/export_syn_count.py". It
   also gains the line R17 rules: parts of `semblance-flyvis` are ports of
   flyvis's algorithms (the connectome expansion, the hex-eye fp16 mask
   construction in `hex_eye.rs`, the ground-truth readers), under flyvis's
   MIT licence, whose text NOTICE already carries; attribution only, no
   licence change. README's "Data and attribution" paragraph names
   `syn_count_torch.rs` beside the other two and says the same about the
   ported code.
4. `crates/flyvis/src/lib.rs` crate docs gain a "Licensing" paragraph:
   `pretrained`, `syn_count_torch` and `groundtruth` are flyvis's MIT data,
   and parts of the crate port flyvis's MIT code; see NOTICE. The two
   generated files are not edited (CLAUDE.md: rerun their scripts only). The
   same docs gain one paragraph on `parity` and `semblance validate`.
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
   exists on crates.io (piece3-packaging.md §5); the dry run does not check
   it, crates.io does.
8. README's `[LICENSE](LICENSE)` and `[NOTICE](NOTICE)` links, and the new
   link to the record, become absolute GitHub URLs, which resolve on
   crates.io as well.

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
  reproduces flyvis 1.2.0's CPU reference (`fixtures-2`) bit for bit, with
  torch's own `exp` of the synapse counts compiled in (`syn_count_torch.rs`,
  generated by `tools/export_syn_count.py`):
  - in every neuron's settled state, and in every centre neuron at every frame
    and every neuron at six frames of 12 stimuli
  - for the best and worst of the 50 trained models, at both time steps, in
    f32 and f64
  - and within pre-registered bounds when Rust computes the synapse counts
    itself with its own `exp`
- **How to check.** `ci/fetch-fixtures.sh` then `cargo run --release --
  validate`, with exit codes 0, 1 and 2.
- **The record.** An absolute link to `docs/results/0.3.0-flyvis-parity.md`,
  noting it was generated on hosted CI.
- **Quickstart:** a `validate` line.
- **Crate table:** the flyvis row's open cell drops "the result record (0.3.0,
  in progress)", and the app row mentions `validate`.
- **Counts:** the skip count, re-measured, and the SEQUENCES-08 sentence
  removed (P3).
- **Attribution:** the paragraph adds `syn_count_torch.rs` and the ported
  code (R17), and the LICENSE/NOTICE links become absolute.

**CLAUDE.md:**
- `Current: **v0.3.0**, crate versions \`0.3.0\``.
- **"Verifying a change":**
  - The test and skip counts, re-measured at the end the way pieces 1 and 2
    did. That is `cargo test --workspace` with the fixtures, then with `data/`
    absent and `SEMBLANCE_REQUIRE_FIXTURES=1`, counting the failures.
  - A paragraph on `cargo run --release -- validate`: what its exit codes
    mean, that it runs the same code as the gates (R20), and that its exact
    rows are bit-equal because of `syn_count_torch.rs` (torch's own `exp`).
  - A paragraph on the record: `docs/results/0.3.0-flyvis-parity.md` is
    `validate --markdown`'s output from hosted CI, held to it by
    `app/tests/record.rs`; it is regenerated only by the head from CI's
    `parity-record` artifact, never by hand or on a workstation. CI sets
    `SEMBLANCE_REQUIRE_LIBM_RECORD=1`, so there the libm section must match;
    elsewhere a libm fingerprint mismatch skips that section, and its
    `record: libm sections not compared` notice is visible only under
    `--nocapture`.
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

## Implementation waves

Each task owns the files listed; no file is owned by two tasks in one wave.
`Cargo.lock` is owned by at most one task per wave, and that task commits it
with the manifest change that moved it (§3). A task's gate is the full
`SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace`, clippy with
`-D warnings`, and the 1.85 build, unless stated otherwise.

**Wave 1**

| Task | Owns | Depends on |
|---|---|---|
| T1 CI time (P1–P4) | `crates/flyvis/tests/acceptance_minors.rs`, `crates/flyvis/tests/acceptance_traces.rs`, `crates/flyvis/tests/acceptance_sequences.rs`, `.github/workflows/ci.yml` | — |
| T2 Provenance | `crates/flyvis/src/reference.rs` (`Provenance`, `ProvenanceModel`, `GapEntry`, `parse`, `ReferenceError::ProvenanceMissing`, their unit tests) | — |
| T3 Extraction (§1 running, §2, §3's flyvis half) | `crates/flyvis/src/parity/{mod,bounds,inputs,run}.rs`, `crates/flyvis/src/lib.rs`, `gate_support.rs`, `settling_checks.rs`, `trace_checks.rs`, `test_support.rs`, `crates/flyvis/Cargo.toml` (sim → `[dependencies]`), **`Cargo.lock`** | — ; its shadow gate re-runs after T1 merges, since T1 changes the shadows it must pass |

T3 holds: the constants, `Bound` and the bounds functions, `Counts`, `Start`,
`Diff`, `diff`, the one core (R20) with the registered-precision dispatch
(R19) and the ±0.0 check, `Fixtures`, `TraceData`, `TraceInputs`,
`how_to_get`, `required_files`, `ParityError`, the `pub(crate)` helpers, the
gate fixture's `build` (R21), and the `run == settle + play` fixture test. The
provenance tests are not touched in this wave.

**Wave 2**

| Task | Owns | Depends on |
|---|---|---|
| T4 Validate | `crates/flyvis/src/parity/{mod,checks,validate,fingerprint}.rs`, `settling_checks.rs` and `trace_checks.rs` (the two provenance tests switch to `Provenance::parse`), `test_support.rs` (`data_dir`, `require_present`) | T2, T3 |
| T5 Packaging | the 18 symlinks, `NOTICE` (incl. R17), `CITATION.cff` (version only), `crates/{sim,codec,flyvis}/Cargo.toml` (descriptions, licence, `exclude`), workspace `Cargo.toml` (`categories` only) | T3 (`crates/flyvis/Cargo.toml`) |

None of T5's manifest edits changes `Cargo.lock`, so no task owns it in this
wave. T5's gate adds `cargo package --workspace` and the 16-entry grep.

**Wave 3**

| Task | Owns | Depends on |
|---|---|---|
| T6 CLI | `app/src/main.rs`, `app/src/validate.rs`, `app/Cargo.toml` (+flyvis, description, `exclude`), **`Cargo.lock`**, `app/tests/validate_cli.rs` | T4 |

**Wave 4**

| Task | Owns | Depends on |
|---|---|---|
| T7 Bump | workspace `Cargo.toml` (`version`, `[workspace.dependencies]`), **`Cargo.lock`** | T5, T6 |
| T8 CI record step | `.github/workflows/ci.yml` (the record step, the upload, `SEMBLANCE_REQUIRE_LIBM_RECORD`) | T6 |

T7's gate: 8 packages at 0.3.0, `cargo package --workspace`, `cargo publish
--workspace --dry-run`. T8 is checked on the next hosted run.

**Wave 5**

| Task | Owns | Depends on |
|---|---|---|
| T9 Record test and preamble | `app/tests/record.rs`, `docs/results/0.3.0-flyvis-parity.md` (preamble only; no generated block) | T6, T8 |
| T10 Docs (non-count parts) | `README.md`, `crates/flyvis/src/lib.rs` (crate docs only) | T4, T5 |

T9's gate is the full suite with exactly one expected failure,
`the_committed_record_is_what_validate_prints`, failing in step 1 because the
file has no generated block (Bootstrap, §7). Its unit tests on synthetic text
(placeholder replacement, marker parsing, the host/libm fingerprint check,
the numeral rule, `REQUIRE_LIBM_RECORD` handling) must pass. The preamble's
numbers are written as the R22 rule requires and checked once the block
exists.

**Head step: the record.** Push; CI runs the record step; the head commits the
`parity-record` artifact into the record (§7, "Generating the record"), pushes
again, and requires CI green. This is not a wave task: no agent generates the
record.

**Wave 6**

| Task | Owns | Depends on |
|---|---|---|
| T11 Counts and CLAUDE.md | `CLAUDE.md`, `README.md` (the count sentence) | the head's record step |

Re-measure with fixtures, then with `data/` absent under
`SEMBLANCE_REQUIRE_FIXTURES=1`. The head then merges, tags, and publishes and
yanks only on the owner's go-ahead (§9).

## Testing, test-first

**`parity` unit tests** (`parity::tests`, synthetic, no fixtures):
- `Bound::value`; `settling_bounds` and `trace_bounds` for every registered
  (model, dt, precision) and for each `Counts`. An unregistered model is an
  error, and `IntegerNSyn` is an error.
- `diff` maps NaN to infinity (moved with
  `a_nan_reference_value_makes_the_maximum_infinite`).
- `Provenance::parse`:
  - a minimal valid document parses
  - each required field missing or mistyped is an error naming its JSON path,
    displayed `PROVENANCE.json has no …`
- `check_*_gaps`: one ulp off in any gap is a `Mismatch` naming
  `{model} {dt} {quantity}`.
- The PROVENANCE boolean check: each of the four read the other way is a
  `Mismatch` naming its path.
- `how_to_get`: all six arms (five directories and the default) are pinned to
  their exact text, because C8 depends on them.
- `Options`: `threads: 0`, an empty `runs`, a run ≥ 8 and a repeated run are
  each `Usage`.
- The pass predicates (R18): a `Job` whose `outcome` is `Err` is not passed; a
  `Job` with 11 sequences is not passed; a `Job` whose result's run or counts
  differ from its own is not passed; a `Controls` with any `Err` is not
  passed; a `Report` with a missing or duplicated (run, counts) pair is not
  passed; `complete()` is false for runs other than 0..8.
- `Fixtures::from_parts` on malformed input returns `Malformed` and never
  panics. The inputs: an empty spec, a truncated ensemble, a manifest with one
  run, and a settling file one byte short.
- The fixture check refuses a centre node index ≥ `n_nodes` as `Mismatch`,
  without panicking.
- The core refuses a ±0.0 reference value as `Mismatch`, and `play` refuses
  `TraceInputs` whose precision differs from the run's.
- The gate fixture's `build` panics, not skips, on an empty ensemble (R21).
- FNV-1a against its published test vectors ("" → `0xcbf29ce484222325`, "a" →
  `0xaf63dc4c8601ec8c`).
- `the_libm_fingerprint_is_independent_of_the_gate`: for every group,
  `Precision::libm_count(g).to_bits()` equals the fingerprint's own value, in
  both precisions.
- `the_libm_fingerprint_counts_differences_from_torch`: each count is ≤ 200,
  as `syn_count_torch_checks` bounds it, and equals an independent recount
  against `SYN_COUNT_TORCH_F32`/`_F64` in the test. (No lower bound: a host
  whose libm agrees with torch on every count is a legitimate host, not a
  defect.) This test reads only compiled-in tables, so it is synthetic and
  never skips.

**`parity` fixture tests.** Each finds the data through two new
`test_support` helpers: `pub(crate) fn data_dir() -> PathBuf` (the same
`{CARGO_MANIFEST_DIR}/../../data` as `data_path`) and `pub(crate) fn
require_present(rels: &[&str]) -> Result<(), String>`, which checks metadata
only, returns the usual `{path} not found. {how}` notice for the first absent
file, and panics through `missing()` under `SEMBLANCE_REQUIRE_FIXTURES=1`. A
test that gets `Err` skips with its own `skipping:` notice through
`test_support::or_skip`.
- `validate_is_bit_identical_across_thread_counts`: `Options { runs: vec![2,
  3], .. }` at 1 and at 4 threads. The two reports' jobs, controls and
  coverage are equal (`assert_eq!`; NaN is impossible after `diff`).
- `run_equals_settle_then_play` (T3): R20's test, for run 3.
- `validate_refuses_a_provenance_gap_one_ulp_off` and
  `validate_refuses_a_file_whose_size_disagrees` (`#[cfg(unix)]`): each builds
  a fresh data dir from `test_support::scratch_dir` (under the build's own
  target dir, never the shared `std::env::temp_dir()`, which another local
  account can pre-populate; named from the test name and the process id), of
  symlinks to the real files except one altered
  copy (PROVENANCE with one gap one ulp off; a `centre.f64.bin` one byte
  short). Each requires `Mismatch` before any job runs, naming the gap or the
  file, and removes the dir at the end. (`CARGO_TARGET_TMPDIR` is not set for
  lib unit tests.)

**Gate wrappers:** the existing suites, unchanged in name and outcome.
`acceptance_settling`, `acceptance_traces`, `acceptance_minors`,
`acceptance_sequences` and `acceptance_mapping` are the test that C1–C18
survived.

**`app/src/validate.rs` unit tests:** the output writer, given a `Write` that
returns `BrokenPipe`, returns the computed exit code and does not panic.

**`app/tests/validate_cli.rs`** (no fixtures; spawns the built binary):
- `--data <empty temp dir>`: exit 2, and stderr lists all 13 paths and names
  `ci/fetch-fixtures.sh`.
- `--threads 0`, `--threads x`, `--threads` with no value, `--bogus`, and
  `--data` given twice: each exits 2 with a usage message.
- `help`: lists `validate` and no longer says everything works with no data.
- `roundtrip`: still prints `structural distance:  0.000000` and
  `behavioural distance: 0.000000`.

**`app/tests/record.rs`:** §7, including its unit tests on synthetic text:
two texts that differ only inside the libm section compare equal after the
placeholder replacement and unequal without it; a missing, duplicated or
unparsable marker fails; a host fingerprint that disagrees with the libm
marker fails; a preamble numeral absent from the block fails, and a version
or a `Generated:` line token does not; under `SEMBLANCE_REQUIRE_LIBM_RECORD=1`
a fingerprint mismatch fails (the decision is a pure function of the two
fingerprints and the flag, so it is tested without setting the variable).

## Failure handling

| Situation | Behaviour |
|---|---|
| fixtures absent, `validate` | exit 2; every missing path listed with its hint; nothing printed as a result (R12) |
| fixtures absent, a gate or the record test, `SEMBLANCE_REQUIRE_FIXTURES` unset | prints its own `skipping:` notice and passes (the record test's step 1 still runs and can fail) |
| fixtures absent, `SEMBLANCE_REQUIRE_FIXTURES=1` | fails, naming the path |
| a fixture present but malformed, in a gate | every gate that uses it fails with the error, with or without `SEMBLANCE_REQUIRE_FIXTURES` (R21) |
| PROVENANCE gap ≠ registered constant | exit 2 (`Mismatch`). The constant is not changed, and the bound is not recomputed |
| a PROVENANCE boolean not as registered | exit 2 (`Mismatch`), naming the path |
| file size ≠ manifest or PROVENANCE `sizes` | exit 2, naming the file |
| a reference value is ±0.0 | exit 2 (`Mismatch`); `==` is not changed |
| an exact value differs | exit 1, the row marked `FAIL`; coverage for that run reads "not measured". Work stops for diagnosis, starting from the epic's ladder (piece 2's R2: rerun from flyvis's settled state) |
| a libm bound exceeded | exit 1. On a host whose libm fingerprint differs from the record's while the exact jobs pass, this is a libm portability finding, recorded and investigated. It is never a reason to change a bound |
| a control does not hold, or errors | exit 1: the gate can no longer see what it exists to see |
| a non-finite state | exit 1, naming the run, node, cell type and step or frame. The other jobs still report |
| a job slot holds another job's result | that job is `Mismatch`, not passed; exit 2 |
| a worker thread cannot be spawned | the remaining jobs run on the calling thread, with the same results |
| stdout closed early (`validate \| head`) | output stops; the computed exit code is returned; no panic |
| the record test differs | fails at the first differing line. The fix is the head regenerating the record from a passing CI run's artifact and reviewing the diff, never editing numbers by hand |
| the record file has no block, a duplicated marker, or a libm marker that disagrees with its host section | fails on every host, fixtures or not |
| the record test on CI, libm fingerprint differs from the record's | fails (`SEMBLANCE_REQUIRE_LIBM_RECORD=1`). Usually `ubuntu-latest` changed its libm: the head checks that the exact rows did not move and every bound still holds, then regenerates the record through the artifact loop and says so in the PR. Never a bound change |
| the record test elsewhere, libm fingerprint differs | compares everything except the libm section, which is replaced in both texts by one placeholder line, and prints why on stderr (R9) |
| a shadow acceptance test fails after the extraction | a C-string moved. Restore the string, never the test's expectation |
| `cargo publish` fails part-way | stop. The crates already uploaded stay, and the owner decides. Never yank or bump around it unasked |

## Non-goals

- Changing any fixture (`fixtures-2`, `data/`), any bound, `TOLERANCE`, or
  any `==`.
- A `--quick`, `--run` or `--json` flag. `Options::runs` exists for tests and
  is not on the command line.
- Generating or regenerating the record anywhere but hosted CI (R9).
- Printing the committed record when the data is absent (R12).
- rustc version or build date in the binary (R10).
- A crate-local `LICENSE-MIT` file for flyvis. NOTICE carries the text.
- Per-crate READMEs.
- Dots, Sintel, other models, `sim`'s trait migration (0.4.0), and the codec's
  `todo!()`s.
- A GitHub Release.
- The local outline edit (task 7 done), which the head makes.

## Done when

- **Local full gate, on the Mac, with fixtures present and
  `SEMBLANCE_REQUIRE_FIXTURES=1`:**
  - `cargo build --workspace`
  - `cargo test --workspace` (every shadow acceptance suite and the record
    test included; the record test compares everything but the libm section,
    unless the Mac's fingerprint happens to equal CI's)
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo run --quiet -- roundtrip` reads `0.000000` twice
  - `cargo run --release -- validate` exits 0
  - `Cargo.lock` lists the same 8 packages at 0.3.0
- The committed record was generated by CI's record step and committed by the
  head (§7), and hosted CI's record test compares it in full, libm section
  included, under `SEMBLANCE_REQUIRE_LIBM_RECORD=1`.
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

## Owner decisions recorded

The three questions this spec raised were answered by the owner on
2026-10-29, and the head ruled on NOTICE:

- **Q1 → R8.** The coverage threshold is an absolute 1e-3 (driven if the
  centre trace moves by more than 1e-3), and every run's ratio to its
  applicable bound is printed, so the 744×, 625× and 462× f32 cases stay
  visible.
- **Q2 → R9.** The committed record is generated on hosted Linux CI, not on
  the Mac, through the record step and the head's artifact commit. CI sets
  `SEMBLANCE_REQUIRE_LIBM_RECORD=1`, so the libm section is compared there
  byte for byte; elsewhere a fingerprint mismatch skips it with a notice.
- **Q3 → R16.** The record test runs the full `validate` on CI.
- **NOTICE (fidelity review M9) → R17.** NOTICE credits the ported algorithms
  as well as the data; no licence change.
