# Behaviour spec: parity-validate

Derived only from `docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md`.
Each behaviour is a given/when/then statement with a stable id.

## parity: The parity library: loading fixtures, running the gates, pass rules and controls

### PARITY-01
- Given a data directory passed as a `&Path`
- When `Fixtures::load`, `TraceData::load` or `parity::validate` reads fixtures
- Then they read from that directory only, never from a checkout-relative or compile-time path, and never read `SEMBLANCE_REQUIRE_FIXTURES`

### PARITY-02
- Given malformed fixture input (an empty spec, a truncated ensemble, a manifest with one run, a settling file one byte short)
- When `Fixtures::from_parts` parses it
- Then it returns `ParityError::Malformed` and never panics; `from_parts` performs no I/O and neither constructor reads `PROVENANCE.json`

### PARITY-03
- Given any input to the parity entry points (`settle`, `play`, `run`, `validate`)
- When the input is bad (missing, unreadable, malformed or mismatched)
- Then they return a `ParityError` and never panic

### PARITY-04
- Given `Counts::Torch`, `Counts::Libm` or `Counts::IntegerNSyn` for a registered (model, dt, precision)
- When `settling_bounds` or `trace_bounds` is asked for the bound
- Then Torch gives `Bound::Exact`, Libm gives `Bound::F64` in f64 or `Bound::F32 { gap, .. }` in f32, and IntegerNSyn or an unregistered model/dt is an error (`no registered bound for model {m} at dt {dt}`)

### PARITY-05
- Given a `Bound`
- When `Bound::value()` is called
- Then `Exact` gives 0.0, `F64` gives `F64_BOUND` (1e-10), and `F32` gives `2.0 * gap` with the registered gap

### PARITY-06
- Given a parsed `Provenance` whose `f32_f64_gap` values all equal the registered `SETTLING_F32_GAPS` and `TRACE_F32_GAPS` bit for bit
- When `check_settling_gaps` and `check_trace_gaps` run
- Then they pass; any gap one ulp off is a `Mismatch` naming `{model} {dt} {quantity}`, and the bound is never recomputed from PROVENANCE

### PARITY-07
- Given a `PROVENANCE.json` with a required field missing or mistyped
- When `Provenance::parse` reads it
- Then it returns `ReferenceError::ProvenanceMissing` (or an error naming the field) displayed `PROVENANCE.json has no {p}`, with `{p}` the JSON path; unknown extra fields are ignored and a minimal valid document parses

### PARITY-08
- Given a run index and fixtures whose manifest precision for that run differs from the registered `RUNS[run].3`
- When `settle`, `play` or `run` is called
- Then it returns `Mismatch("run {run}'s precision: manifest {a}, expected {b}")`, with `b` from the registry, before any dispatch

### PARITY-09
- Given `TraceInputs` whose `precision` differs from the run's registered precision
- When `play` or `run` is called
- Then it returns `Mismatch("trace inputs are {a}, run {run} is {b}")`

### PARITY-10
- Given fixtures and trace inputs for run 3
- When `run(f, t, 3, Torch)` is compared with `settle(f, 3, "000", Torch)` plus `play(f, t, 3, Torch, Start::Rust, 0..12, false)`
- Then they are equal field for field, because all three entry points go through the same private core

### PARITY-11
- Given a flyvis reference value that is +0.0 or -0.0
- When the core reads it
- Then it returns `Mismatch("{file} {key}: reference value {i} is ±0.0; == is bit equality only without signed zeros")`; the `==` comparison itself is not changed

### PARITY-12
- Given a reference array containing NaN
- When `diff` compares it
- Then the maximum is infinite and the NaN is counted in `differ`

### PARITY-13
- Given a `GradedError` from `Graded::new` or `set_state`, or a `WiringError` from `scatter_hexals`
- When the core meets it
- Then it becomes `Mismatch("run {run}: {e}")`

### PARITY-14
- Given a state that becomes non-finite during settling or playback
- When the core scans it after the step
- Then it returns `ParityError::NonFinite`, displayed `run {run}: node {node} ({cell_type}) is not finite after settle step {k}` or `... after frame {i} of sequence {s}`, and `is_gate_failure()` is true only for this variant

### PARITY-15
- Given `Options` with `threads == 0`, an empty `runs`, a run ≥ 8, or a repeated run
- When `validate` is called
- Then it returns `ParityError::Usage`; `Options::default()` is 1 thread and runs 0..8

### PARITY-16
- Given a data directory with one or more of the 13 `required_files()` absent
- When `validate` runs
- Then it returns `ParityError::Missing` for the first absent file, displayed `{path} not found. {how_to_get(rel)}`, before any arithmetic

### PARITY-17
- Given all required files present
- When `validate` runs its fixture checks before any job
- Then it fails with a `Mismatch` if the manifest's runs are not `RUNS`, a centre node index is ≥ `n_nodes` (checked before indexing, without panicking), the centre nodes do not match the network's cell types in `CELL_TYPES` order, there are not 12 sequences 0–11 each with 6 snapshot frames, a block's shape is wrong, or a registered gap differs

### PARITY-18
- Given a reference file whose byte length differs from its manifest `bytes` or its PROVENANCE `sizes` entry
- When `validate` runs its fixture checks
- Then it returns `Mismatch` naming the file (`{file}: {n} bytes, manifest says {m}` or `{file}: {n} bytes, PROVENANCE sizes says {m}`) before any job runs; `validate` does not hash files

### PARITY-19
- Given PROVENANCE with `git.dirty` true, or `threads.bit_equal`, `parameter_equality.all_50_checkpoints_equal_ensemble_csv` or `equivalence.stimulus_response_bit_equal` false
- When `validate` runs its fixture checks
- Then it returns `Mismatch("PROVENANCE {path} is {v}: the reference is not the one registered")`

### PARITY-20
- Given `validate` with valid fixtures and runs `R`
- When it runs the jobs
- Then it runs 2 × |R| jobs (each run × [Torch, Libm], in (run, counts) order) on `min(threads, jobs)` scoped workers, one job's error is stored in its `outcome` while the others carry on, and if a worker cannot be spawned the remaining jobs run on the calling thread without panicking

### PARITY-21
- Given the same fixtures and runs [2, 3]
- When `validate` runs with 1 thread and with 4 threads
- Then the two reports' jobs, controls and coverage are equal (bit-identical to a serial run)

### PARITY-22
- Given a job slot whose `Ok` outcome has `settling.run` or `settling.counts` differing from the slot's job
- When `validate` checks the slots after the pool finishes
- Then that outcome is replaced by `Err(Mismatch("job slot {i} holds run {r} {counts:?}"))` and never counted as a pass

### PARITY-23
- Given a `Job`
- When `Job::passed` is evaluated
- Then it is true only if the outcome is `Ok`, its run and counts equal the job's, it has exactly 12 sequences 0–11 in order, and every `differ == 0` for an exact job or every `max <=` its bound's value for a libm job; an `Err` outcome, 11 sequences, or a mismatched run/counts is not passed

### PARITY-24
- Given the four controls (wrong model: run 3 settled with 049's parameters; integer counts: run 3 with edge_n_syn; mirrored mapping: run 3 sequence 8 with mirrored hexals; flyvis start: run 3 sequence 8 from flyvis's settled state)
- When `Controls::passed` is evaluated
- Then it is true only if all four are `Ok` and wrong model settled max > `CONTROL_MISS` (1e-3), integer counts settled max > `F64_BOUND`, mirrored snapshots max > `CONTROL_MISS`, and the flyvis start is bit-equal

### PARITY-25
- Given a `Report`
- When `Report::passed` and `Report::complete` are evaluated
- Then `passed` requires `jobs.len() == 2 * runs.len()`, every (run, counts) pair exactly once, every job and the controls passed, and one coverage entry per run; `complete` is true only for runs exactly [0..8]

### PARITY-26
- Given the Torch job for a run
- When coverage is computed
- Then if that job passed, `movement` holds per centre type the max over 12 sequences and every frame of |centre (widened to f64) − settled centre (widened to f64)|; otherwise `movement` is `None`; a type is driven only if `movement > COVERAGE_THRESHOLD` (1e-3, strict) and coverage is recorded, never a gate

### PARITY-27
- Given the compiled-in synapse-count table
- When `libm_fingerprint()` is computed
- Then it evaluates `f32::exp` and `f64::exp` directly per group (through `black_box`), counts groups whose bits differ from torch's table (each count ≤ 200, equal to an independent recount), FNV-1a-hashes the little-endian bits in group order, and prints as `f32:{n}:{fnv:016x},f64:{n}:{fnv:016x}`

### PARITY-28
- Given every synapse-count group
- When `Precision::libm_count(g).to_bits()` is compared with the fingerprint's own value in f32 and f64
- Then they are equal, so the fingerprint cannot silently diverge from the gate's count formula

### PARITY-29
- Given the FNV-1a implementation
- When it hashes "" and "a"
- Then it returns `0xcbf29ce484222325` and `0xaf63dc4c8601ec8c`

### PARITY-30
- Given a relative fixture path
- When `parity::how_to_get` is called
- Then each of its six arms (five directories and the default) returns its exact, unchanged text, which `test_support`'s notices use

### PARITY-31
- Given the `#[cfg(test)]` settling and trace gates after the extraction
- When they run
- Then they call `parity` and keep every test name, module path, doc comment, print and panic string (C1–C18) byte for byte, `settling_checks::` holds exactly 20 tests, each generated test asserts `<$t as Precision>::DTYPE == parity::RUNS[$run].3`, and neither file emits a compiler warning

### PARITY-32
- Given a gate fixture that is present but malformed (e.g. an empty or truncated `ensemble.csv`)
- When any gate that uses the shared fixture runs
- Then it fails with the error, with or without `SEMBLANCE_REQUIRE_FIXTURES`; only a missing file becomes a skip

### PARITY-33
- Given a fixture missing under the test harness
- When a gate or parity fixture test runs
- Then it prints `skipping: {path} not found. {how}` (path `{CARGO_MANIFEST_DIR}/../../data/{rel}`) and passes, or under `SEMBLANCE_REQUIRE_FIXTURES=1` fails with `SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing`

### PARITY-34
- Given a temp data dir that is the real fixtures except PROVENANCE with one gap one ulp off, or `centre.f64.bin` one byte short
- When `validate` runs on it
- Then it returns a `Mismatch` naming the gap or the file before any job runs

## validate: The semblance validate command: output, exit codes, threads and missing data

### VALIDATE-01
- Given the `semblance` binary
- When it is run as `semblance validate [--data DIR] [--threads N] [--markdown]`
- Then it runs all eight runs' gates and the four controls and prints one table to stdout; `--data` defaults to `./data`

### VALIDATE-02
- Given fixtures in which every exact job is bit-equal, every libm job is within its bound, every control holds, and all eight runs ran
- When `semblance validate` finishes
- Then it exits 0

### VALIDATE-03
- Given an exact value that differs, a libm bound exceeded, a control that does not hold or errored, or a non-finite state
- When `semblance validate` finishes
- Then it exits 1, the full table still prints with each failing row ending in `FAIL`, and the verdict line names every failing job and control

### VALIDATE-04
- Given a fixture that is missing, unreadable, malformed or mismatched (including a job slot holding another job's result)
- When `semblance validate` runs
- Then it exits 2 and nothing counts as verified, even if some jobs finished; 2 wins over 1 when both occur

### VALIDATE-05
- Given an unknown flag, a flag given twice, a flag missing its value, `--threads 0`, or a non-integer `--threads`
- When `semblance validate` parses its arguments
- Then it writes a message and the usage line to stderr and exits 2

### VALIDATE-06
- Given an unknown command (not a flag of validate)
- When `semblance` is run with it
- Then it exits 1, as before

### VALIDATE-07
- Given no `--threads`
- When `semblance validate` runs
- Then it uses `available_parallelism()` (or 1 if unknown) capped at 16; an explicit value above 16 is accepted and capped inside `validate`; results do not depend on the thread count

### VALIDATE-08
- Given a data directory with fixture files absent
- When `semblance validate` runs
- Then it writes to stderr `validate: nothing was verified; {n} fixture files are missing:`, each absent path (only those) one per line, a pointer to `ci/fetch-fixtures.sh` and tools/README.md, and that the recorded result `docs/results/0.3.0-flyvis-parity.md` is not a check of this machine; it exits 2 and never prints the record as a result

### VALIDATE-09
- Given an empty data directory
- When `semblance validate --data <dir>` runs
- Then stderr lists all 13 required paths and names `ci/fetch-fixtures.sh`, with exit 2

### VALIDATE-10
- Given exactly one fixture file missing
- When `semblance validate` runs
- Then that file's `how_to_get` hint is printed in full

### VALIDATE-11
- Given a successful run without `--markdown`
- When output is printed
- Then stdout has a `label: value` header (data, reference, models, this build, libm exp with the fingerprint), a fixed-width table with one settling and one traces row per (run, counts) — columns stage, run, model, dt, prec, counts, compared, unequal, max err, where, bound, ratio — then the controls, coverage with each run's weakest-type / libm-centre-bound ratio in run order, and a verdict line ending in wall time

### VALIDATE-12
- Given an exact row that passes
- When it is printed in the table
- Then it shows `0` unequal, `==` as bound and `-` for where/ratio; a failing exact row prints its maximum

### VALIDATE-13
- Given floats in the terminal output
- When they are printed
- Then values use Rust's native `{:.3e}` (unpadded exponent), thresholds use `{:e}` of their constants (`1e-3`, `1e-10`), and validation losses use `{:.3}`

### VALIDATE-14
- Given a run whose exact job did not pass
- When coverage is printed
- Then it reads `coverage: run {r} not measured: exact job failed`

### VALIDATE-15
- Given a control that returned an error
- When the controls are printed
- Then its error prints in place of its measurement, followed by `FAIL`, and the exit code is 1

### VALIDATE-16
- Given stdout closed early (e.g. `semblance validate | head`)
- When writing output hits `BrokenPipe`
- Then output stops and the command exits with the code it had computed, never panicking; any other write error is exit 2

### VALIDATE-17
- Given `semblance help`
- When it is printed
- Then it lists `validate` with `[--data DIR] [--threads N] [--markdown]`, says demo, roundtrip and evolve need no data while validate needs the fixtures under data/ (via `ci/fetch-fixtures.sh`) and names the record, and no longer says everything works with no data

### VALIDATE-18
- Given the change
- When `semblance roundtrip` runs
- Then it still prints `structural distance:  0.000000` and `behavioural distance: 0.000000`

### VALIDATE-19
- Given the CLI source
- When its module doc is read
- Then it says std-only by design, with no suggestion to swap in `clap`

### VALIDATE-20
- Given `semblance validate`
- When it runs
- Then it reads only the data directory: it never runs `tools/` or git

## record: The committed parity record and its check against validate's output

### RECORD-01
- Given `semblance validate --markdown`
- When it runs on valid fixtures
- Then stdout is the record's generated block between `<!-- BEGIN semblance validate --markdown -->` and `<!-- END semblance validate --markdown -->`, containing the reference section, a host-specific section, bounds as applied, the exact settling and trace stages, a libm section, controls and coverage

### RECORD-02
- Given the generated block
- When it is produced
- Then outside the host-specific section it has no wall time, date, path or thread count, and is deterministic given the fixtures and (in its libm section) the host's libm

### RECORD-03
- Given the host-specific section
- When it is printed
- Then it lies between `BEGIN host-specific`/`END host-specific` markers and holds the semblance version, the target (from `ARCH`, `OS` and `target_env`), debug assertions, the thread count, the libm exp counts and the libm fingerprint; no rustc version or build date appears

### RECORD-04
- Given the libm section
- When it is printed
- Then it is wrapped in `<!-- BEGIN libm fingerprint={fp} -->` ... `<!-- END libm -->` with `{fp}` the running host's fingerprint, and holds the libm settling and trace stages

### RECORD-05
- Given numbers in the generated block
- When they are printed
- Then integers have no separators; bounds in "Bounds as applied", thresholds and input rounding use `{:e}`; other measured values, bounds and ratios use `{:.3e}`; worst-type values `{:.2e}`; the mirrored control `{:e}` in full; validation losses `{}`; SHAs in full

### RECORD-06
- Given equal input rounding at 1/200 and 1/50
- When the input-rounding row is printed
- Then it prints one value "at 1/200 and 1/50"; otherwise `{a:e} at 1/200; {b:e} at 1/50`

### RECORD-07
- Given a run whose exact job did not pass
- When the coverage table is printed
- Then that row reads `not measured: exact job failed` and the closing sentence names the unmeasured runs instead of claiming every type is driven

### RECORD-08
- Given the coverage section
- When it is printed
- Then it states the absolute 1e-3 threshold (driven if above, recorded not a gate) and, per run, the weakest type, its movement, the next two, the median (element 32 of 65 sorted by `total_cmp`), the libm centre bound and weakest / bound; ties go to the lower cell-type index, then lower run index

### RECORD-09
- Given the committed `docs/results/0.3.0-flyvis-parity.md`
- When the record test runs, on any host and with or without fixtures
- Then it fails (never skips) unless the file exists with the BEGIN/END validate pair exactly once, the host-specific pair exactly once, exactly one libm section, a parsable libm marker fingerprint equal to the committed host section's `libm fingerprint` row, and a preamble that obeys the numeral rule

### RECORD-10
- Given a preamble numeral token (a maximal run of `0-9 . e + -` starting with a digit that contains `.` or `e` or has three or more digits)
- When the numeral rule is checked
- Then every such token must occur verbatim in the committed generated block, except tokens with two or more `.` (versions) and every token on the `Generated:` line

### RECORD-11
- Given any of `parity::required_files()` absent under the workspace `data/`
- When the record test reaches the fixture step
- Then it prints `skipping: {path} not found. {how_to_get(rel)}` and passes, or under `SEMBLANCE_REQUIRE_FIXTURES=1` panics with `SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing`

### RECORD-12
- Given fixtures present
- When the record test runs `semblance validate --markdown --data <workspace>/data` (the built binary, not `tools/` or git)
- Then it requires exit 0, extracts the block with the same exactly-once checks, removes the host-specific section (markers included) from both texts, and compares the rest byte for byte, naming the first differing line of each on failure

### RECORD-13
- Given the running host's libm fingerprint equals the record's
- When the record test compares
- Then the libm section is compared byte for byte too

### RECORD-14
- Given the fingerprints differ and `SEMBLANCE_REQUIRE_LIBM_RECORD=1`
- When the record test compares
- Then it fails with `SEMBLANCE_REQUIRE_LIBM_RECORD=1 but this host's libm fingerprint {x} differs from the record's {y}`

### RECORD-15
- Given the fingerprints differ and `SEMBLANCE_REQUIRE_LIBM_RECORD` unset
- When the record test compares
- Then it replaces the whole libm section (markers included) in both texts with `<!-- libm sections not compared -->`, prints `record: libm sections not compared: this host's libm fingerprint {x} differs from the record's {y}` to stderr, and compares everything else

### RECORD-16
- Given the record test's text functions on synthetic strings
- When they are unit-tested
- Then texts differing only inside the libm section compare equal after placeholder replacement and unequal without it; a missing, duplicated or unparsable marker fails; a disagreeing host fingerprint fails; a preamble numeral absent from the block fails while a version or `Generated:` token does not; and the libm decision is a pure function of the two fingerprints and the flag

### RECORD-17
- Given the hand-written preamble
- When the record is read
- Then it covers what 0.3.0 claims and its three gates, what exact (== with no ±0.0) and libm mean, the never-relax rule, the f64/f32 ratio explanation quoting the worst ratio from the block, that libm rows are the recording host's (hosted `ubuntu-latest`) facts, the fixtures (`fixtures-2`, `ci/fixtures.sha256`), `syn_count_torch.rs`, the Sintel decision with the runs 2, 4, 6 ratios as printed, and one `Generated:` line with toolchain, CI run URL and date

### RECORD-18
- Given CI's `test` job
- When it runs
- Then after Build and before the test step it runs `target/debug/semblance validate --markdown > parity-record.md`, uploads it as artifact `parity-record` with `if: always()` using `actions/upload-artifact` pinned to `ea165f8d65b6e75b540449e92b4886f43607fa02`, fails the job on a gate failure, and the test step's env sets `SEMBLANCE_REQUIRE_LIBM_RECORD: "1"`

### RECORD-19
- Given the CI time cuts
- When the suite runs
- Then MINORS-15 no longer re-runs `trace_checks::`; TRACES-06 runs the 8 exact trace gates in one nested invocation asserting `running 8 tests` and the same 8 lines; SEQUENCES-08 has no nested cargo and is renamed `sequences_08_the_manifest_and_sequences_parse_in_process`; the `test` job's cache no longer lists `target`; no control, comparison, bound, `TOLERANCE` or `==` changes

## release: Packaging and the 0.3.0 release metadata

### RELEASE-01
- Given the workspace
- When the version is bumped
- Then `[workspace.package] version` and every `[workspace.dependencies]` pin are `0.3.0`, and `Cargo.lock` lists the same 8 `semblance*` packages, at 0.3.0, with no external crate

### RELEASE-02
- Given the dependency changes
- When the manifests are read
- Then `semblance-sim` is a normal dependency of `semblance-flyvis`, `app` depends on `semblance-flyvis`, and publishing order stays valid (flyvis after sim, semblance last)

### RELEASE-03
- Given each of the seven library crates and `app`
- When `cargo package --workspace` runs on a clean tree
- Then every `.crate` contains LICENSE and NOTICE (via tracked symlinks), 16 entries in all, and `cargo publish --workspace --dry-run` passes

### RELEASE-04
- Given `crates/flyvis/Cargo.toml`
- When its licence is read
- Then it is `Apache-2.0 AND MIT`

### RELEASE-05
- Given NOTICE
- When it is read
- Then it lists `crates/flyvis/src/syn_count_torch.rs` (torch's `exp` of `SYN_COUNT` in f32 and f64, generated by tools/export_syn_count.py) and states that parts of `semblance-flyvis` port flyvis's MIT algorithms (connectome expansion, hex-eye fp16 mask construction, ground-truth readers), attribution only, no licence change

### RELEASE-06
- Given the flyvis, sim and app packages
- When they are packaged
- Then `tests/` is excluded from each; rng keeps `tests/vectors.rs`

### RELEASE-07
- Given the crate descriptions
- When the manifests are read
- Then sim, flyvis, codec and app carry the spec's new descriptions (sim no longer "LIF baseline"; codec says encode/decode panic with `todo!()`; app mentions validate and std-only parsing), and the workspace `categories` are `["science::neuroscience", "simulation"]`

### RELEASE-08
- Given `CITATION.cff`
- When it is read for the release
- Then `version` is `0.3.0` and `date-released` is the tag date

### RELEASE-09
- Given `crates/flyvis/src/lib.rs` crate docs
- When they are read
- Then they have a Licensing paragraph (`pretrained`, `syn_count_torch` and `groundtruth` are flyvis's MIT data, parts port flyvis's MIT code, see NOTICE) and a paragraph on `parity` and `semblance validate`; the generated files are not edited

### RELEASE-10
- Given README.md
- When it is read
- Then it has the 0.3.0 headline (bit-for-bit reproduction of flyvis 1.2.0's CPU reference with `syn_count_torch.rs`, within pre-registered bounds with Rust's own `exp`), how to check (`ci/fetch-fixtures.sh`, `cargo run --release -- validate`, exit codes 0/1/2), an absolute link to the record noting it was generated on hosted CI, a quickstart `validate` line, the flyvis row without "(0.3.0, in progress)", the app row mentioning validate, re-measured counts without the SEQUENCES-08 sentence, and absolute LICENSE/NOTICE links

### RELEASE-11
- Given CLAUDE.md
- When it is read
- Then it says v0.3.0, carries re-measured test and skip counts, describes `validate`'s exit codes and the record (regenerated only from CI's `parity-record` artifact, `SEMBLANCE_REQUIRE_LIBM_RECORD`), says validate never runs `tools/` or git, and has the new yank sentence ("are to be yanked" until the yanks happen)

### RELEASE-12
- Given the merged release
- When it is shipped
- Then an annotated `v0.3.0` tag is pushed, there is no GitHub Release, and `cargo publish --workspace` and the yanks of the six 0.1.0 crates happen only on the owner's explicit go-ahead
