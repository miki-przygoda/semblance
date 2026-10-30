# The flyvis parity record, `semblance validate`, and the 0.3.0 release: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the settling and trace gates' machinery into a public,
non-panicking library module, `semblance_flyvis::parity`, and make the gates
thin wrappers over it with every printed string unchanged. Add `semblance
validate [--data DIR] [--threads N] [--markdown]`, a CI step that produces
the parity record's generated block, the record
`docs/results/0.3.0-flyvis-parity.md` with a test that holds it to
`validate`'s output, the four CI-time cuts P1–P4, the packaging fixes, the
docs, and the 0.3.0 bump. Publishing, yanking and the tag are the head's,
after the owner says go.

**Architecture:** `crates/flyvis/src/parity/` is a directory module:
`mod.rs` (constants, `ParityError`), `bounds.rs`, `inputs.rs` (`Fixtures`,
`TraceData`, `TraceInputs`, `how_to_get`), `run.rs` (the one core:
`settle_state`, `compare_settling`, `play_from`, and the non-generic `settle`,
`play`, `run` that dispatch on the registered precision), `checks.rs` (the
fixture checks), `validate.rs` (`Options`, `Report`, the thread pool) and
`fingerprint.rs` (the libm fingerprint). `settling_checks.rs` and
`trace_checks.rs` call it and keep their format strings. `semblance-sim`
becomes a normal dependency of `semblance-flyvis`; `app` gains
`semblance-flyvis` and a new `app/src/validate.rs` that formats the report
(terminal table or Markdown). `Cargo.lock` keeps exactly 8 packages.

**Tech Stack:** Rust 1.85+ (edition 2024), std only. No Python, nothing
under `tools/` is run.

**Spec:** `docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md`.
The spec is the authority; rulings (R1–R22), facts (F1–F21) and
the C-list (C1–C18) below refer to it.

## How this plan was verified

Every task below was executed, in wave order, in a scratch copy of the
repository at `e75c860` with `data/` linked to the real fixtures, and every
code block in this plan is the code that ran there (the plan was generated
from that tree, and its instructions were re-applied in order to the files of
`e75c860` to check that they reproduce it). Measured on the owner's 16-core arm64 Mac:

- **The gates' prints are unchanged.** After task 3, the 37 result lines
  the settling, trace and stimulus gates print (`exact run`, `libm run`,
  `trace exact run`, `trace libm run`, `mirrored mapping`, `flashes at`,
  `moving edges at`) were byte-identical to the same lines before it. All
  five shadow acceptance suites passed on task 3's commit alone
  (`acceptance_settling` 13, `acceptance_traces` 24, `acceptance_minors` 9,
  `acceptance_mapping` 8, `acceptance_sequences` 9).
- **Test totals** with `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace`:
  the base is 429 passed. After wave 1: 448 passed, 0 failed. After wave 5
  (the record has no generated block yet): 477 passed, 1 failed, the one
  failure being `the_committed_record_is_what_validate_prints` with
  `the committed record: no <!-- BEGIN semblance validate --markdown -->`.
  After a generated block is committed (simulated with this Mac's own
  `validate --markdown`): **478 passed, 0 failed**, 112 s wall with a warm
  `target`. The flyvis lib unit tests are 296 (18.6 s); the app has 14.
- **Fixture-gated tests:** with `data/` absent and
  `SEMBLANCE_REQUIRE_FIXTURES=1`, **129** tests fail (the base's 123, minus
  SEQUENCES-08, plus `the_real_provenance_parses`,
  `run_equals_settle_then_play`, `play_refuses_trace_inputs_of_the_other_precision`,
  the three `validate` fixture tests, and the record test). Without the
  variable all 478 pass, and under `--nocapture --test-threads=1` every one
  of the 129 prints its own `skipping:` notice, 141 notices in all.
- **CI-time cuts, locally:** `acceptance_traces` 103.7 s to 63–64 s;
  `acceptance_minors` 116.9 s to 9.5–14.5 s.
- **`semblance validate`:** release build, 9.65 s at 16 threads, 17.1 s at 4,
  64.7 s at 1; debug `app` (flyvis and sim are at opt-level 3 in the dev
  profile) 12.3 s at 16 threads. Exit 0. Release and debug `--markdown`
  output differ only in the host section's `debug assertions` row.
  `validate | head -2` exits 0 with no panic; `--data /nonexistent` lists all
  13 files and exits 2; `--threads 0` exits 2.
- **Per-run results** (this Mac; the libm numbers are host facts, R9): every
  exact row is bit-equal, every libm row within its bound, every control
  holds. The table task 6 prints is reproduced in task 6, step 6. Worst f32
  ratio 8.605e-1 (traces, run 4, sequence 3, frame 359, Mi4); worst f64
  ratio 3.121e-3 (traces, run 7). Controls: wrong model settled max
  6.397e0, integer counts 4.963e-7, mirrored snapshots max
  2.503223408264881e0 (centre max 2.291526948382731e0), flyvis's settled
  start 0 of 282529 unequal. Coverage: the weakest type is Tm30 in every
  run; the overall weakest is **run 3** at 3.773e-3 (the spec's example says
  run 2: the two print alike to four figures, run 3's f64 value is the
  smaller); weakest / libm centre bound, runs 0–7: 1.102e3 3.801e7 7.439e2
  3.773e7 6.245e2 3.248e8 4.618e2 3.179e8. Libm fingerprint on this Mac:
  `f32:106:55fdb5ef0c7a79a5,f64:108:8970802a716917ba`.
- **Packaging:** `cargo package --workspace` lists 16 LICENSE/NOTICE entries;
  `cargo publish --workspace --dry-run` exits 0, uploading in the order rng,
  connectome, codec, sim, evo, fidelity, flyvis, semblance. `Cargo.lock`:
  8 packages, all 0.3.0 after task 7.
- `cargo clippy --workspace --all-targets -- -D warnings` clean (Rust
  1.97.1), `rustup run 1.85 cargo build --workspace --all-targets` clean
  (1.85.1), `cargo run --quiet -- roundtrip` reads `0.000000` twice.

**The record test cannot pass before the head's record step.** The record
comes from Linux CI. From task 9 until the head commits CI's `parity-record`
artifact, `the_committed_record_is_what_validate_prints` fails at its first
step on every host, with `the committed record: no <!-- BEGIN semblance
validate --markdown -->`, and it is the only failing test. After the head's
commit, on the Mac (whose libm fingerprint will differ from CI's) it passes,
printing under `--nocapture`
`record: libm sections not compared: this host's libm fingerprint
f32:106:55fdb5ef0c7a79a5,f64:108:8970802a716917ba differs from the record's
<CI's>`; on CI, with `SEMBLANCE_REQUIRE_LIBM_RECORD=1`, it compares the libm
section too. If CI's run differs in any deterministic number from the Mac's
(it must not: every such number is IEEE arithmetic on fixed inputs), the
Mac's run fails naming the first differing line; stop and report it.

## Execution: waves and ownership

Each task runs in its own git worktree of branch `parity-validate`; two
tasks in one wave never touch the same file. Each task's **Files** list is
every file it may change; it changes nothing else. `Cargo.lock` is owned by
at most one task per wave (tasks 3, 6 and 7), and that task commits it with
the manifest change that moved it (spec §3); waves 2, 5 and 6 change no
dependency and no task owns it there.

| Wave | Task (id) | Files |
|---|---|---|
| 1 | 1. CI time, P1–P4 (`ci-time`) | `crates/flyvis/tests/acceptance_minors.rs`, `crates/flyvis/tests/acceptance_traces.rs`, `crates/flyvis/tests/acceptance_sequences.rs`, `.github/workflows/ci.yml` |
| 1 | 2. Provenance (`provenance`) | `crates/flyvis/src/reference.rs` |
| 1 | 3. Extraction (`extraction`) | `crates/flyvis/src/parity/mod.rs`, `crates/flyvis/src/parity/bounds.rs`, `crates/flyvis/src/parity/inputs.rs`, `crates/flyvis/src/parity/run.rs`, `crates/flyvis/src/lib.rs`, `crates/flyvis/src/gate_support.rs`, `crates/flyvis/src/settling_checks.rs`, `crates/flyvis/src/trace_checks.rs`, `crates/flyvis/src/test_support.rs`, `crates/flyvis/Cargo.toml`, `Cargo.lock` |
| 2 | 4. Validate (`validate`) | `crates/flyvis/src/parity/mod.rs`, `crates/flyvis/src/parity/checks.rs`, `crates/flyvis/src/parity/validate.rs`, `crates/flyvis/src/parity/fingerprint.rs`, `crates/flyvis/src/settling_checks.rs`, `crates/flyvis/src/trace_checks.rs`, `crates/flyvis/src/test_support.rs` |
| 2 | 5. Packaging (`packaging`) | the 16 symlinks `crates/{rng,connectome,codec,sim,evo,fidelity,flyvis}/{LICENSE,NOTICE}` and `app/{LICENSE,NOTICE}`, `NOTICE`, `CITATION.cff`, `crates/sim/Cargo.toml`, `crates/codec/Cargo.toml`, `crates/flyvis/Cargo.toml`, `Cargo.toml` |
| 3 | 6. CLI (`cli`) | `app/src/main.rs`, `app/src/validate.rs`, `app/Cargo.toml`, `Cargo.lock`, `app/tests/validate_cli.rs` |
| 4 | 7. Bump (`bump`) | `Cargo.toml`, `Cargo.lock` |
| 4 | 8. CI record step (`record-step`) | `.github/workflows/ci.yml` |
| 5 | 9. Record test and preamble (`record-test`) | `app/tests/record.rs`, `docs/results/0.3.0-flyvis-parity.md` |
| 5 | 10. README and crate docs (`readme-docs`) | `README.md`, `crates/flyvis/src/lib.rs` |
| 6 | 11. Counts and CLAUDE.md (`docs-counts`) | `CLAUDE.md`, `README.md` |

Dependencies: task 3's shadow suites are re-run after task 1 merges (task 1
changes them). Task 4 needs tasks 2 and 3; task 5 needs task 3 (it edits
`crates/flyvis/Cargo.toml` after it). Task 6 needs task 4. Task 7 needs
tasks 5 and 6; task 8 needs task 6. Task 9 needs tasks 6 and 8; task 10
needs tasks 4 and 5. Task 11 needs the head's record step (see "Head steps").

## Global Constraints

- **Zero external dependencies.** `Cargo.lock` lists exactly 8 packages, all
  `semblance*` (`grep -c '^name = ' Cargo.lock` prints `8`). Nothing here adds
  one, not even for tests.
- **Rust 1.85 MSRV, edition 2024**: no let-chains, no std API newer than 1.85.
  Check with `rustup run 1.85 cargo build --workspace --all-targets`
  (`cargo +1.85` does not work on this machine).
- **Clippy is clean:** `cargo clippy --workspace --all-targets -- -D warnings`.
- **Never loosen a comparison.** No bound, gap constant, `TOLERANCE`,
  threshold or `==` is widened, replaced or recomputed. The gap tables move
  into `parity` with their digits unchanged; `1e-3` in the two controls
  becomes `CONTROL_MISS`, whose value is `1e-3`. A PROVENANCE value that
  disagrees with a registered gap is a fixture error (exit 2), never a reason
  to change the constant.
- **Never change a fixture.** Nothing under `data/` is written; nothing in
  `ci/fixtures.sha256` or `ci/fixtures-tag` changes.
- **Keep every C-string byte for byte** (spec §2, C1–C18). The gate
  wrappers' format strings are moved verbatim; `ParityError`'s `NonFinite`
  displays C5 and C12 exactly; the shape and dt messages keep C6 and C13 as
  substrings; the gap checks keep C7. `settling_checks::` keeps exactly its
  20 tests and `trace_checks::` its 22, with the same names and doc
  comments, and neither file may emit a compiler warning (C17).
- **Tests never read git history and never run anything under `tools/`.**
  The only process a test spawns is cargo or the `semblance` binary under
  test.
- **Every fixture-gated test prints its own skip notice** (`skipping: <path>
  not found. <how>`) through `test_support::or_skip` or the record test's own
  printer, and fails under `SEMBLANCE_REQUIRE_FIXTURES=1`.
- **Run the suite with the fixtures required:**
  `SEMBLANCE_REQUIRE_FIXTURES=1`. A worktree sees the main checkout's
  `data/` through a `data` symlink at its root; if it is missing, create it:
  `ln -s <main checkout>/data data`. That symlink is not covered by `/data/`
  in `.gitignore`, so **always `git add` explicit paths, never `git add -A`
  or `git add .`**, and never commit `data`.
- **Commits:** plain imperative subject, no `feat:`/`fix:` prefixes, then a
  blank line and the trailer `Temper-Task: parity-validate/<task id>` (the
  ids are in the table above). Each task commits on its own worktree branch;
  do not push, rebase or merge.
- **A task's gate** is, unless the task says otherwise:
  `cargo build --workspace --locked`,
  `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --locked` (0 failed),
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo run --quiet -- roundtrip` (`0.000000` twice),
  `rustup run 1.85 cargo build --workspace --all-targets --locked`, and
  `grep -c '^name = ' Cargo.lock` printing `8`. This is
  `.temper/gate-full.sh`.
- **Stop rule.** If any exact gate, libm bound, control, shadow acceptance
  test or print comparison fails, stop and report the full output. Do not
  change a bound, a constant, an assertion, an expected string or an `==` to
  get past it. A shadow acceptance test that fails after task 3 means a
  C-string moved: restore the string, never the test's expectation.

## Review Focus

The input classes most likely to bite, most likely first. Each is pinned by a
test in the task that owns the code.

1. **A C-string moving in the extraction.** The gates are now formatted from
   `parity`'s structs; one changed character fails a shadow suite on CI only.
   **Task 3:** step 1 captures the 37 printed result lines before any change
   and step 10 requires them byte-identical after; step 11 runs all five
   shadow suites.
2. **A broken fixture becoming a skip** (R21). A present but malformed file
   must fail every gate that uses it, never pass silently. **Task 3:**
   `gate_support::tests::a_malformed_fixture_fails_rather_than_skips`,
   `parity::inputs::tests::from_parts_refuses_malformed_input_without_panicking`.
3. **A pass predicate that does not fail closed** (R18): an errored, short,
   foreign or missing job counted as passed. **Task 4:**
   `a_job_passes_only_when_complete_and_within_its_bounds`,
   `controls_pass_only_when_every_one_returned_and_holds`,
   `a_report_passes_only_with_every_job_once`.
4. **A thread count changing a result.** **Task 4:**
   `validate_is_bit_identical_across_thread_counts` (runs 2 and 3 at 1 and 4
   threads, every job, control and coverage value equal).
5. **The record compared on the wrong host or not at all** (R9). **Task 9:**
   `require_libm_record_turns_a_fingerprint_mismatch_into_a_failure`,
   `a_host_fingerprint_that_disagrees_with_the_libm_marker_fails`,
   `placeholder_replacement_hides_only_the_libm_section`.
6. **A panic in release, which aborts before any summary** (F3): a closed
   stdout, a bad flag, a reference value that does not fit. **Task 6:**
   `a_broken_pipe_keeps_the_exit_code_and_does_not_panic`; **task 4:**
   `a_centre_node_outside_the_network_is_a_mismatch_not_a_panic`; **task 3:**
   `signed_zeros_in_the_reference_are_refused`.

## Rulings made while planning

Each resolves something the spec leaves open or gets wrong; none changes a
spec decision.

- **16 symlinks, not 18** (spec §9's packaging item 1 and T5's row say "the
  18 symlinks"). Seven crates and `app` are eight directories, two links
  each: 16, which is also what the spec's pre-merge grep requires.
- **Task 3 owns `Cargo.lock` but does not change it.** Moving `semblance-sim`
  from `[dev-dependencies]` to `[dependencies]` leaves the lock file
  identical (a lock file records dev-dependencies too). Task 3 checks this
  and commits the lock only if it changed. Only tasks 6 and 7 change it.
- **The preamble's one libm number** (R22 with R9). The spec asks the
  preamble to quote the worst ratio, which is a libm value and so a fact of
  CI's host, and R22 requires every preamble numeral to occur in the block,
  while task 9 writes the preamble before CI has produced any block. Task 9
  quotes this Mac's `8.605e-1` and otherwise uses only deterministic numbers
  (coverage ratios, thresholds, counts). In the head's record step, if the
  record test reports that the preamble quotes a number the block does not
  print, the head replaces that one quote with the block's own value, copied,
  and says so in the commit message. No other preamble number is edited.
- **Numeral tokens end before trailing punctuation.** R22's "maximal run of
  `0-9 . e + -`" would make `3.773e-3.` at the end of a sentence a token that
  never occurs in the block. Trailing `.`, `+` and `-` are stripped from a
  token before it is checked.
- **Ties in the terminal's `where` and the record's per-stimulus `where`:
  the last candidate wins,** as the gates' `max_by` does. With that rule the
  table names exactly what C11 prints (run 0: `centre, seq 9 frame 302 (T2)`;
  run 1: `snaps, seq 11 frame 314 (T2)`), as in the spec's example; with
  "first wins" it would name sequence 2.
- **The manifest's run list is checked in `Fixtures::from_parts`,** as
  `Malformed` (`the manifest's runs are not the eight registered: ...`), so
  the test gates refuse a wrong run list too; `validate` inherits it through
  `Fixtures::load`. The spec lists it among `validate`'s fixture checks as a
  `Mismatch`; both are exit 2.
- **`Report` gains `centre_nodes: Vec<u32>`** (the manifest's centre node
  per type), which the record's `where` column needs (`node 22236 (T2)`).
- **`ProvenanceMissing` also covers a mistyped field**, as the spec's test
  list requires (`each required field missing or mistyped ... displayed
  PROVENANCE.json has no ...`).
- **`settling_bounds`/`trace_bounds` with `Counts::IntegerNSyn` is
  `ParityError::Usage`;** an unregistered (model, dt) is `Mismatch("no
  registered bound for model {m} at dt {dt}")` for every `Counts`.
- **`Precision::SUFFIX` is derived from `DTYPE`** (a provided associated
  const), so `DTYPE` is used outside tests and the lib builds without a
  dead-code warning.
- **`mirrored_hexals` cannot panic:** the lattice is point-symmetric by
  construction, and a hexal without a mirror (impossible) would map to
  itself, which the mirrored control would then fail on rather than abort.
- **The controls run on the same pool** with `min(threads, 4)` workers; the
  pool's calling thread is a worker, so `threads == 1` spawns nothing.
  `Report::threads` is the number used, `min(threads, jobs)`.
- **Exit codes:** a job that errored for a fixture reason (anything but
  `NonFinite`) makes the run exit 2 even if a gate also failed; a control
  that errored is a gate failure, exit 1 (R14).
- **The verdict line** is `PASS — ...` (exit 0), `FAIL — <every failing row
  and control>` (exit 1), or `NOT VERIFIED — ...` (exit 2).
- **CLAUDE.md's notice sentence was stale.** It said one fixture-gated test
  prints no notice; measured, all 129 do (141 notices). Task 11 corrects it.
- **Task 9's gate expects exactly one failure,** the record test at its
  first step; task 9 also checks its preamble against a locally generated
  block before restoring the preamble-only file.

---

### Task 1: CI time, P1–P4 (wave 1, id `ci-time`)

**Wave:** 1

Spec §8, P1–P4 (R4). Every negative control stays; no comparison changes.

**Files:**
- Modify: `crates/flyvis/tests/acceptance_minors.rs` (MINORS-15 drops its `trace_checks::` re-run: P1)
- Modify: `crates/flyvis/tests/acceptance_traces.rs` (TRACES-06's eight nested runs become one: P2)
- Modify: `crates/flyvis/tests/acceptance_sequences.rs` (SEQUENCES-08 drops its nested cargo and is renamed: P3)
- Modify: `.github/workflows/ci.yml` (the `test` job stops caching `target`: P4)

**Interfaces:**
- Consumes: nothing new. Task 3 runs in parallel; these tests pass against
  the gates before and after it, because the gates' strings do not change.
- Produces: the test `sequences_08_the_manifest_and_sequences_parse_in_process`
  in place of
  `sequences_08_manifest_parse_its_tests_and_acceptance_reference_are_unchanged_and_pass`;
  TRACES-06 asserts `running 8 tests` and each of the eight
  `test trace_checks::trace_exact_run_* ... ok` lines.

These change tests, not code, so there is no failing test to write first:
each step changes one test and runs it.

- [ ] **Step 1: P1, MINORS-15.** At the end of
  `minors_15_settling_and_stimulus_gate_results_are_unchanged`:

In `crates/flyvis/tests/acceptance_minors.rs`, replace:

```rust
    // The trace gates (with the stimulus checks beside them) are new in this
    // epic, so they have no earlier result: with the fixes they must pass,
    // and every exact trace gate must find nothing differing.
    let t = now.all("trace_checks::", true);
    assert!(t.passed && t.output.contains("test result: ok."), "the trace gates pass:\n{}", t.output);
    let exact: Vec<&str> = t.output.lines().filter(|l| l.starts_with("trace exact run ")).collect();
    assert_eq!(exact.len(), 8, "the eight exact trace gates print their results:\n{}", t.output);
    for l in exact {
        let (equal, total) = field(l, "): ", ' ').parse::<usize>().ok().zip(field(l, " of ", ' ').parse::<usize>().ok()).expect("counts");
        assert!(equal == total && total > 0, "every sequence bit-equal: {l}");
    }
    assert_eq!(t.output.lines().filter(|l| l.starts_with("trace libm run ")).count(), 8, "{}", t.output);
}
```

with:

```rust
    // The trace gates are not re-run here (spec P1, docs/superpowers/specs/
    // 2026-10-29-flyvis-parity-validate-design.md, section 8): all 22 run
    // directly in the same suite under SEMBLANCE_REQUIRE_FIXTURES=1, TRACES-06
    // requires every exact line in its `12 of 12` form, and the parity record
    // test compares every trace number `semblance validate` prints.
}
```

  (The `field` helper stays: MINORS-04 and the settling part of MINORS-15
  still use it.)

- [ ] **Step 2: P2, TRACES-06.** At the end of
  `traces_06_torch_counts_trace_bit_equal_to_flyvis_in_all_eight_runs`:

In `crates/flyvis/tests/acceptance_traces.rs`, replace:

```rust
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    for (run, name) in EXACT_GATES.iter().enumerate() {
        let out = sh.gate(&tc(name), true);
        assert!(out.passed, "{name} must pass:\n{}", out.output);
        let info = &f.manifest.runs[run];
        let want = format!(
            "trace exact run {run} ({}, dt {}, {}): 12 of 12 sequences bit-equal, centre and snapshots",
            info.model, info.dt, if info.precision == Dtype::F32 { "f32" } else { "f64" }
        );
        assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);
    }
}
```

with:

```rust
    // The eight exact gates in one nested run (spec P2), under the same flag
    // as `sh.gate(name, true)`. Each prints its line with one `eprintln!`, and
    // stdout and stderr are captured apart, so lines do not interleave.
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh
        .cargo()
        .args(["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--"])
        .args(["trace_checks::trace_exact_run_", "--nocapture", "--test-threads=4"])
        .env("SEMBLANCE_REQUIRE_FIXTURES", "1")
        .output()
        .expect("cargo runs");
    let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(output.contains("running 8 tests"), "the eight exact trace gates, in one run:\n{output}");
    assert!(out.status.success(), "the eight exact trace gates must pass:\n{output}");
    for (run, name) in EXACT_GATES.iter().enumerate() {
        assert!(output.contains(&format!("test {} ... ok", tc(name))), "{name} must pass:\n{output}");
        let info = &f.manifest.runs[run];
        let want = format!(
            "trace exact run {run} ({}, dt {}, {}): 12 of 12 sequences bit-equal, centre and snapshots",
            info.model, info.dt, if info.precision == Dtype::F32 { "f32" } else { "f64" }
        );
        assert!(output.contains(&want), "expected {want:?}:\n{output}");
    }
}
```

- [ ] **Step 3: P3, SEQUENCES-08.** The module doc:

In `crates/flyvis/tests/acceptance_sequences.rs`, replace:

```rust
//! SEQUENCES-07 and SEQUENCES-08 also run the crate's own tests through
//! cargo, in a target directory under `CARGO_TARGET_TMPDIR`: SEQUENCES-07
//! runs the real-manifest fixture test in a copy of the workspace's sources
//! whose `data/flyvis-reference/manifest.json` gives one sequence five
//! snapshot frames (the real `data/` is never written); SEQUENCES-08 runs
//! `reference::tests` and `acceptance_reference.rs` as they stand. Tests that
//! read the real manifest skip without it unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip into a failure.
```

with:

```rust
//! SEQUENCES-07 also runs the crate's own tests through cargo, in a target
//! directory under `CARGO_TARGET_TMPDIR`: it runs the real-manifest fixture
//! test in a copy of the workspace's sources whose
//! `data/flyvis-reference/manifest.json` gives one sequence five snapshot
//! frames (the real `data/` is never written). SEQUENCES-08 checks
//! `Manifest::parse` in process; `reference::tests` and
//! `acceptance_reference.rs` run directly in the same suite rather than
//! nested. Tests that read the real manifest skip without it unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip into a failure.
```

Then the test itself:

In `crates/flyvis/tests/acceptance_sequences.rs`, replace everything from the line
`/// SEQUENCES-08: `Manifest::parse` still accepts manifests with no or partial` up to, but not including, the line `// ---------------------------------------------------------------------------`, with:

```rust
/// SEQUENCES-08: `Manifest::parse` still accepts manifests with no or partial
/// `sequences`, and `parse_sequences` refuses them separately. Checked in
/// process: `Manifest::parse`'s own tests (`reference::tests`) and
/// `acceptance_reference.rs` run directly in the same suite, so this test no
/// longer runs them again in a nested cargo. That they are unchanged since
/// the epic's base is a review-time check: no test here reads git history.
#[test]
fn sequences_08_the_manifest_and_sequences_parse_in_process() {
    // No or partial sequences: Manifest::parse does not read them, even
    // where parse_sequences refuses them.
    for (what, seqs) in [
        ("no sequences", None),
        ("an empty list", Some("[]")),
        ("a partial entry", Some(r#"[{"index":0,"family":"flashes","frames":{"1/50":{"n_frames":3}}}]"#)),
        ("a mistyped list", Some(r#"{"x":1}"#)),
    ] {
        let text = manifest_with(seqs);
        let m = Manifest::parse(&text).unwrap_or_else(|e| panic!("{what}: Manifest::parse must accept it: {e}"));
        assert_eq!((m.n_nodes, m.runs.len(), m.files.len()), (7, 1, 1), "{what}");
        if seqs != Some("[]") {
            assert!(parse_sequences(&text).is_err(), "{what}: parse_sequences refuses it, separately");
        }
    }
}

```

  `cargo` (the helper) stays: SEQUENCES-07 still uses it.

- [ ] **Step 4: P4, the `target` cache.** In the `test` job only (the `msrv`
  job is unchanged):

In `.github/workflows/ci.yml`, replace:

```yaml
      - uses: actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830 # v4.3.0
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: cargo-1.97-
```

with:

```yaml
      # `target` is not cached (spec P4): a fresh checkout makes every path
      # dependency dirty, and every shadow test re-copies its sources, so a
      # restored target never saved a build and its upload cost 25 s.
      - uses: actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830 # v4.3.0
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
          key: cargo-1.97-
```

- [ ] **Step 5: Run the three changed suites.**

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --test acceptance_minors --test acceptance_traces --test acceptance_sequences`
  Expected: `acceptance_minors` 9 passed, `acceptance_sequences` 9 passed,
  `acceptance_traces` 24 passed, 0 failed. Measured: minors about 14 s
  (was 117 s), traces about 64 s (was 104 s).

- [ ] **Step 6: Clippy and the gate.**

  Run: `cargo clippy --workspace --all-targets -- -D warnings`, then
  `.temper/gate-full.sh` (or the gate commands in Global Constraints).
  Expected: clean; the suite passes, 0 failed.

- [ ] **Step 7: Commit.**

```bash
git add crates/flyvis/tests/acceptance_minors.rs \
  crates/flyvis/tests/acceptance_traces.rs \
  crates/flyvis/tests/acceptance_sequences.rs \
  .github/workflows/ci.yml
git commit -m "Cut the CI test job's time without dropping a check" \
  -m "Temper-Task: parity-validate/ci-time"
```

### Task 2: Provenance (wave 1, id `provenance`)

**Wave:** 1

Spec §1, "Provenance", and F12. `json` stays private: the parser lives in
`reference.rs`.

**Files:**
- Modify: `crates/flyvis/src/reference.rs` (`ReferenceError::ProvenanceMissing`,
  `Provenance`, `ProvenanceModel`, `GapEntry`, `Provenance::parse`, tests)

**Interfaces:**
- Consumes: `crate::json::{parse, Json}` (`get`, `as_str`, `as_f64`,
  `as_i64`, `as_bool`, `as_array`, `Json::Object`).
- Produces:
  - `pub struct Provenance { exported, export_id, generator_path, generator_sha256, git_head: String, git_dirty: bool, versions: Vec<(String, String)>, platform, machine, device, uv_lock_sha256: String, threads_default: u32, threads_checked: Vec<u32>, threads_bit_equal: bool, threads_runs_checked: Vec<u32>, models: Vec<ProvenanceModel>, all_checkpoints_equal_ensemble: bool, stimulus_response_bit_equal: bool, equivalence_sequences: Vec<u32>, input_rounding: Vec<(String, f64)>, gaps: Vec<GapEntry>, sizes: Vec<(String, u64)> }`
    (all `pub`, `Clone, Debug, PartialEq`)
  - `pub struct ProvenanceModel { pub id: String, pub name: String, pub rank: String, pub validation_loss: f64, pub chkpt_00000_sha256: String }`
  - `pub struct GapEntry { pub key: String, pub settled: f64, pub settling_centre: f64, pub centre: f64, pub snapshots: f64 }`
  - `impl Provenance { pub fn parse(json: &str) -> Result<Provenance, ReferenceError> }`
  - `ReferenceError::ProvenanceMissing(String)`, displayed
    `PROVENANCE.json has no {path}`, for a field missing or of the wrong type.

- [ ] **Step 1: Write the failing tests.** At the end of
  `crates/flyvis/src/reference.rs`'s `mod tests`, after
  `every_real_settled_block_reads_at_its_shape`:

```rust

    /// A minimal PROVENANCE.json with every field `Provenance::parse` reads.
    const PROVENANCE: &str = r#"{"exported":"2026-10-27","export_id":"x-1",
        "generator":{"path":"tools/export_reference.py","sha256":"ab"},
        "git":{"head":"cd","dirty":false},
        "versions":{"python":"3.12.11","flyvis":"1.2.0"},
        "machine":{"platform":"p-1","machine":"arm64","torch_config":"ignored"},
        "uv_lock_sha256":"ef","device":"cpu",
        "threads":{"default":12,"determinism_check":[1,12],"bit_equal":true,"runs_checked":[0,1]},
        "models":[{"id":"000","name":"flow/0000/000","validation_loss":5.25,"rank":"best","chkpt_00000_sha256":"01"}],
        "parameter_equality":{"all_50_checkpoints_equal_ensemble_csv":true},
        "equivalence":{"stimulus_response_bit_equal":true,"sequences":[0,4]},
        "input_rounding":{"1/200":2.9801242251714655e-08,"1/50":1e-8},
        "f32_f64_gap":{"000 1/200":{"settled":1.5,"settling_centre":2.5,"centre":3.5,"snapshots":4.5}},
        "sizes":{"settled.f32.bin":730704},
        "wall_time_s":248.9}"#;

    #[test]
    fn provenance_parses_a_minimal_document() {
        let p = Provenance::parse(PROVENANCE).expect("valid");
        assert_eq!((p.exported.as_str(), p.export_id.as_str()), ("2026-10-27", "x-1"));
        assert_eq!((p.generator_path.as_str(), p.generator_sha256.as_str()), ("tools/export_reference.py", "ab"));
        assert_eq!((p.git_head.as_str(), p.git_dirty), ("cd", false));
        assert_eq!(p.versions, [("python".to_string(), "3.12.11".to_string()), ("flyvis".into(), "1.2.0".into())]);
        assert_eq!((p.platform.as_str(), p.machine.as_str(), p.device.as_str()), ("p-1", "arm64", "cpu"));
        assert_eq!(p.uv_lock_sha256, "ef");
        assert_eq!((p.threads_default, p.threads_bit_equal), (12, true));
        assert_eq!((p.threads_checked, p.threads_runs_checked), (vec![1, 12], vec![0, 1]));
        assert_eq!(
            p.models,
            [ProvenanceModel {
                id: "000".into(),
                name: "flow/0000/000".into(),
                rank: "best".into(),
                validation_loss: 5.25,
                chkpt_00000_sha256: "01".into(),
            }]
        );
        assert!(p.all_checkpoints_equal_ensemble && p.stimulus_response_bit_equal);
        assert_eq!(p.equivalence_sequences, [0, 4]);
        assert_eq!(p.input_rounding, [("1/200".to_string(), 2.9801242251714655e-08), ("1/50".into(), 1e-8)]);
        assert_eq!(
            p.gaps,
            [GapEntry { key: "000 1/200".into(), settled: 1.5, settling_centre: 2.5, centre: 3.5, snapshots: 4.5 }]
        );
        assert_eq!(p.sizes, [("settled.f32.bin".to_string(), 730704)]);
    }

    /// Every field read, missing or of the wrong type, is an error naming
    /// its JSON path.
    #[test]
    fn provenance_refuses_a_missing_or_mistyped_field_naming_its_path() {
        let cases: [(&str, &str, &str); 30] = [
            (r#""exported":"2026-10-27","#, "", "exported"),
            (r#""exported":"2026-10-27""#, r#""exported":1"#, "exported"),
            (r#""export_id":"x-1","#, "", "export_id"),
            (r#""path":"tools/export_reference.py","#, "", "generator.path"),
            (r#""sha256":"ab""#, r#""sha256":null"#, "generator.sha256"),
            (r#""head":"cd","#, "", "git.head"),
            (r#""dirty":false"#, r#""dirty":"no""#, "git.dirty"),
            (r#""versions":{"python":"3.12.11","#, r#""versions":{"python":3,"#, "versions.python"),
            (r#""versions":{"python":"3.12.11","flyvis":"1.2.0"},"#, r#""versions":[],"#, "versions"),
            (r#""platform":"p-1","#, "", "machine.platform"),
            (r#""machine":"arm64","#, "", "machine.machine"),
            (r#""device":"cpu","#, "", "device"),
            (r#""uv_lock_sha256":"ef","#, "", "uv_lock_sha256"),
            (r#""default":12,"#, r#""default":-1,"#, "threads.default"),
            (r#""determinism_check":[1,12]"#, r#""determinism_check":[1,"12"]"#, "threads.determinism_check"),
            (r#""bit_equal":true"#, r#""bit_equal":1"#, "threads.bit_equal"),
            (r#","runs_checked":[0,1]"#, "", "threads.runs_checked"),
            (r#""models":["#, r#""models":"none","x":["#, "models"),
            (r#""id":"000","#, "", "models[0].id"),
            (r#""name":"flow/0000/000","#, "", "models[0].name"),
            (r#""validation_loss":5.25"#, r#""validation_loss":"5.25""#, "models[0].validation_loss"),
            (r#""rank":"best","#, "", "models[0].rank"),
            (r#","chkpt_00000_sha256":"01""#, "", "models[0].chkpt_00000_sha256"),
            (
                r#""all_50_checkpoints_equal_ensemble_csv":true"#,
                r#""x":true"#,
                "parameter_equality.all_50_checkpoints_equal_ensemble_csv",
            ),
            (r#""stimulus_response_bit_equal":true,"#, "", "equivalence.stimulus_response_bit_equal"),
            (r#","sequences":[0,4]"#, "", "equivalence.sequences"),
            (r#""1/50":1e-8"#, r#""1/50":"1e-8""#, "input_rounding.1/50"),
            (r#""settled":1.5,"#, "", "f32_f64_gap.000 1/200.settled"),
            (r#""snapshots":4.5"#, r#""snapshots":null"#, "f32_f64_gap.000 1/200.snapshots"),
            (r#""settled.f32.bin":730704"#, r#""settled.f32.bin":-4"#, "sizes.settled.f32.bin"),
        ];
        for (from, to, want) in cases {
            let doc = PROVENANCE.replacen(from, to, 1);
            assert_ne!(doc, PROVENANCE, "{from} must occur in the document");
            let e = Provenance::parse(&doc).expect_err(want);
            assert_eq!(e, ReferenceError::ProvenanceMissing(want.to_string()), "{from} -> {to}");
            assert_eq!(e.to_string(), format!("PROVENANCE.json has no {want}"));
        }
    }

    /// The real PROVENANCE.json parses, and holds what the registry expects.
    #[test]
    fn the_real_provenance_parses() {
        let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
        let p = Provenance::parse(&text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["000", "049"]);
        assert_eq!(p.gaps.len(), 4);
        assert_eq!(p.sizes.len(), 9);
        assert!(!p.git_dirty && p.threads_bit_equal && p.all_checkpoints_equal_ensemble && p.stimulus_response_bit_equal);
    }
```

- [ ] **Step 2: Run them to see them fail.**

  Run: `cargo test -p semblance-flyvis --lib reference::tests::`
  Expected: does not compile: `cannot find type Provenance`,
  `no variant ... named ProvenanceMissing`.

- [ ] **Step 3: Implement.** The new variant:

In `crates/flyvis/src/reference.rs`, replace:

```rust
    /// A sequence's frame indices are out of order or past its `n_frames`.
    FrameRange(String),
}
```

with:

```rust
    /// A sequence's frame indices are out of order or past its `n_frames`.
    FrameRange(String),
    /// PROVENANCE.json lacks this field, or has it with the wrong type.
    /// Named by its JSON path (`git.dirty`, `models[1].id`).
    ProvenanceMissing(String),
}
```

Its message:

In `crates/flyvis/src/reference.rs`, replace:

```rust
            FrameRange(p) => write!(f, "manifest {p} is out of order or past n_frames"),
```

with:

```rust
            FrameRange(p) => write!(f, "manifest {p} is out of order or past n_frames"),
            ProvenanceMissing(p) => write!(f, "PROVENANCE.json has no {p}"),
```

Then the parser:

In `crates/flyvis/src/reference.rs`, immediately before `#[cfg(test)]\nmod tests {`, insert:

```rust
/// What `PROVENANCE.json` records about the export the reference came from:
/// the fields `semblance validate` checks or prints. Each field names the
/// JSON path it is read from.
#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    /// `exported`
    pub exported: String,
    /// `export_id`
    pub export_id: String,
    /// `generator.path`
    pub generator_path: String,
    /// `generator.sha256`
    pub generator_sha256: String,
    /// `git.head`
    pub git_head: String,
    /// `git.dirty`
    pub git_dirty: bool,
    /// `versions`, in file order
    pub versions: Vec<(String, String)>,
    /// `machine.platform`
    pub platform: String,
    /// `machine.machine`
    pub machine: String,
    /// `device`
    pub device: String,
    /// `uv_lock_sha256`
    pub uv_lock_sha256: String,
    /// `threads.default`
    pub threads_default: u32,
    /// `threads.determinism_check`
    pub threads_checked: Vec<u32>,
    /// `threads.bit_equal`
    pub threads_bit_equal: bool,
    /// `threads.runs_checked`
    pub threads_runs_checked: Vec<u32>,
    /// `models[]`
    pub models: Vec<ProvenanceModel>,
    /// `parameter_equality.all_50_checkpoints_equal_ensemble_csv`
    pub all_checkpoints_equal_ensemble: bool,
    /// `equivalence.stimulus_response_bit_equal`
    pub stimulus_response_bit_equal: bool,
    /// `equivalence.sequences`
    pub equivalence_sequences: Vec<u32>,
    /// `input_rounding`, per dt id, in file order
    pub input_rounding: Vec<(String, f64)>,
    /// `f32_f64_gap`, per "{model} {dt}" key, in file order
    pub gaps: Vec<GapEntry>,
    /// `sizes`, per file name, in file order
    pub sizes: Vec<(String, u64)>,
}

/// One of PROVENANCE.json's `models`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProvenanceModel {
    pub id: String,
    pub name: String,
    pub rank: String,
    pub validation_loss: f64,
    pub chkpt_00000_sha256: String,
}

/// One `f32_f64_gap` entry: flyvis's own float32-vs-float64 gaps for one
/// (model, dt).
#[derive(Clone, Debug, PartialEq)]
pub struct GapEntry {
    /// "{model} {dt}", e.g. "000 1/200".
    pub key: String,
    pub settled: f64,
    pub settling_centre: f64,
    pub centre: f64,
    pub snapshots: f64,
}

fn path(at: &str, key: &str) -> String {
    if at.is_empty() { key.to_string() } else { format!("{at}.{key}") }
}

fn field<'a>(obj: &'a Json, at: &str, key: &str) -> Result<&'a Json, ReferenceError> {
    obj.get(key).ok_or_else(|| ReferenceError::ProvenanceMissing(path(at, key)))
}

fn typed<T>(obj: &Json, at: &str, key: &str, f: impl Fn(&Json) -> Option<T>) -> Result<T, ReferenceError> {
    f(field(obj, at, key)?).ok_or_else(|| ReferenceError::ProvenanceMissing(path(at, key)))
}

fn text(obj: &Json, at: &str, key: &str) -> Result<String, ReferenceError> {
    typed(obj, at, key, |v| v.as_str().map(str::to_string))
}

fn flag(obj: &Json, at: &str, key: &str) -> Result<bool, ReferenceError> {
    typed(obj, at, key, Json::as_bool)
}

fn small(v: &Json) -> Option<u32> {
    v.as_i64().and_then(|n| u32::try_from(n).ok())
}

fn smalls(obj: &Json, at: &str, key: &str) -> Result<Vec<u32>, ReferenceError> {
    typed(obj, at, key, |v| v.as_array()?.iter().map(small).collect())
}

/// An object's entries, in file order, each converted by `f`.
fn entries<T>(obj: &Json, at: &str, key: &str, f: impl Fn(&Json) -> Option<T>) -> Result<Vec<(String, T)>, ReferenceError> {
    let here = path(at, key);
    let Json::Object(pairs) = field(obj, at, key)? else {
        return Err(ReferenceError::ProvenanceMissing(here));
    };
    pairs
        .iter()
        .map(|(k, v)| f(v).map(|x| (k.clone(), x)).ok_or_else(|| ReferenceError::ProvenanceMissing(path(&here, k))))
        .collect()
}

impl Provenance {
    /// Parses the fields above, strictly; every other field is ignored. A
    /// missing or mistyped field is `ProvenanceMissing`, naming its path.
    pub fn parse(json: &str) -> Result<Provenance, ReferenceError> {
        let root = json::parse(json).map_err(|e| ReferenceError::Json { offset: e.offset })?;
        let r = &root;
        let generator = field(r, "", "generator")?;
        let git = field(r, "", "git")?;
        let machine = field(r, "", "machine")?;
        let threads = field(r, "", "threads")?;
        let models = typed(r, "", "models", |v| v.as_array().map(<[Json]>::to_vec))?
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let at = format!("models[{i}]");
                Ok(ProvenanceModel {
                    id: text(m, &at, "id")?,
                    name: text(m, &at, "name")?,
                    rank: text(m, &at, "rank")?,
                    validation_loss: typed(m, &at, "validation_loss", Json::as_f64)?,
                    chkpt_00000_sha256: text(m, &at, "chkpt_00000_sha256")?,
                })
            })
            .collect::<Result<Vec<_>, ReferenceError>>()?;
        let parameters = field(r, "", "parameter_equality")?;
        let equivalence = field(r, "", "equivalence")?;
        let Json::Object(gap_pairs) = field(r, "", "f32_f64_gap")? else {
            return Err(ReferenceError::ProvenanceMissing("f32_f64_gap".into()));
        };
        let gaps = gap_pairs
            .iter()
            .map(|(key, g)| {
                let at = format!("f32_f64_gap.{key}");
                let n = |q: &str| typed(g, &at, q, Json::as_f64);
                Ok(GapEntry {
                    key: key.clone(),
                    settled: n("settled")?,
                    settling_centre: n("settling_centre")?,
                    centre: n("centre")?,
                    snapshots: n("snapshots")?,
                })
            })
            .collect::<Result<Vec<_>, ReferenceError>>()?;
        Ok(Provenance {
            exported: text(r, "", "exported")?,
            export_id: text(r, "", "export_id")?,
            generator_path: text(generator, "generator", "path")?,
            generator_sha256: text(generator, "generator", "sha256")?,
            git_head: text(git, "git", "head")?,
            git_dirty: flag(git, "git", "dirty")?,
            versions: entries(r, "", "versions", |v| v.as_str().map(str::to_string))?,
            platform: text(machine, "machine", "platform")?,
            machine: text(machine, "machine", "machine")?,
            device: text(r, "", "device")?,
            uv_lock_sha256: text(r, "", "uv_lock_sha256")?,
            threads_default: typed(threads, "threads", "default", small)?,
            threads_checked: smalls(threads, "threads", "determinism_check")?,
            threads_bit_equal: flag(threads, "threads", "bit_equal")?,
            threads_runs_checked: smalls(threads, "threads", "runs_checked")?,
            models,
            all_checkpoints_equal_ensemble: flag(parameters, "parameter_equality", "all_50_checkpoints_equal_ensemble_csv")?,
            stimulus_response_bit_equal: flag(equivalence, "equivalence", "stimulus_response_bit_equal")?,
            equivalence_sequences: smalls(equivalence, "equivalence", "sequences")?,
            input_rounding: entries(r, "", "input_rounding", Json::as_f64)?,
            gaps,
            sizes: entries(r, "", "sizes", |v| v.as_i64().and_then(|n| u64::try_from(n).ok()))?,
        })
    }
}

```

- [ ] **Step 4: Run the tests.**

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib reference::tests::`
  Expected: PASS, 23 passed (3 new: `provenance_parses_a_minimal_document`,
  `provenance_refuses_a_missing_or_mistyped_field_naming_its_path` with its
  30 cases, and `the_real_provenance_parses` against the real file).

- [ ] **Step 5: Clippy and the gate.**

  Run: `cargo clippy -p semblance-flyvis --all-targets -- -D warnings`, then
  the gate. Expected: clean; 0 failed (432 passed on this task's worktree
  alone).

- [ ] **Step 6: Commit.**

```bash
git add crates/flyvis/src/reference.rs
git commit -m "Read PROVENANCE.json into a typed Provenance" \
  -m "Temper-Task: parity-validate/provenance"
```

### Task 3: Extraction into `semblance_flyvis::parity` (wave 1, id `extraction`)

**Wave:** 1

Spec §1 "Running", §2, §3's flyvis half, R13, R19, R20, R21. The gates'
arithmetic, order of operations, zeroed input buffer, non-finite scan and
shape checks move unchanged into `parity/run.rs`; the gates keep their
names, doc comments and format strings, and format `parity`'s structs. The
two provenance tests are not touched in this task (task 4 switches them).

**Files:**
- Create: `crates/flyvis/src/parity/mod.rs` (constants, `ParityError`, `After`)
- Create: `crates/flyvis/src/parity/bounds.rs` (`Bound`, `settling_bounds`, `trace_bounds`)
- Create: `crates/flyvis/src/parity/inputs.rs` (`Fixtures`, `TraceData`, `TraceInputs`, `required_files`, `how_to_get`)
- Create: `crates/flyvis/src/parity/run.rs` (`Counts`, `Start`, `Diff`, `diff`, the core, `settle`, `play`, `run`, the `pub(crate)` helpers)
- Modify: `crates/flyvis/src/lib.rs` (`pub mod parity;`)
- Modify: `crates/flyvis/src/gate_support.rs` (now only the fixture: `fixture()`, `build`)
- Modify: `crates/flyvis/src/settling_checks.rs`, `crates/flyvis/src/trace_checks.rs` (wrappers)
- Modify: `crates/flyvis/src/test_support.rs` (`how_to_get` moves out; `REAL_SPEC` becomes `pub(crate)`)
- Modify: `crates/flyvis/Cargo.toml` (`semblance-sim` to `[dependencies]`)
- Check: `Cargo.lock` (unchanged; commit it only if it changed)

**Interfaces:**
- Consumes: `semblance_sim::graded::{Graded, GradedParams, Real}` (`Graded::new`,
  `step`, `set_state`, `state`), `crate::wiring::{edge_syn_groups, input_nodes,
  scatter_hexals}`, `crate::reference::{Manifest, parse_sequences, FileInfo,
  BlockKey, Array, Dtype}`, `crate::expand::{expand, hex_coords}`.
- Produces (all in `semblance_flyvis::parity`):
  - `pub const F64_BOUND: f64 = 1e-10; pub const CONTROL_MISS: f64 = 1e-3; pub const COVERAGE_THRESHOLD: f64 = 1e-3;`
  - `pub const SETTLING_F32_GAPS: [(&str, &str, f64, f64); 4]`, `pub const TRACE_F32_GAPS: [(&str, &str, f64, f64); 4]` (moved verbatim)
  - `pub const RUNS: [(u32, &str, &str, Dtype); 8]`
  - `pub enum ParityError { Missing { rel: String, path: PathBuf }, Unreadable { rel: String, path: PathBuf, kind: std::io::ErrorKind }, Malformed { rel: String, detail: String }, Mismatch(String), NonFinite { run: u32, node: usize, cell_type: String, after: After }, Usage(String) }` with `is_gate_failure(&self) -> bool`, `Display`, `Error`
  - `pub enum After { SettleStep(usize), Frame { frame: usize, sequence: u32 } }`
  - `pub enum Bound { Exact, F64, F32 { gap: f64, model: &'static str, dt: &'static str, quantity: &'static str } }` with `value(&self) -> f64`
  - `pub fn settling_bounds(model: &str, dt: &str, p: Dtype, counts: Counts) -> Result<(Bound, Bound), ParityError>` (settled, settling_centre); `pub fn trace_bounds(...)` (centre, snapshots)
  - `pub struct Fixtures` with `load(data: &Path) -> Result<Fixtures, ParityError>`, `from_parts(spec_json: &str, ensemble_csv: &str, manifest_json: &str, settling: Vec<(&'static str, Vec<u8>)>) -> Result<Fixtures, ParityError>`, `manifest(&self) -> &Manifest`, `network(&self) -> &TypedNetwork`
  - `pub struct TraceData` with `load(data: &Path) -> Result<TraceData, ParityError>`, `view(&self, p: Dtype) -> TraceInputs<'_>`
  - `pub struct TraceInputs<'a> { pub precision: Dtype, pub sequences: &'a [SequenceInfo], pub input: &'a [u8], pub centre: &'a [u8], pub snapshots: &'a [u8] }`
  - `pub fn required_files() -> &'static [&'static str]` (13 paths), `pub fn how_to_get(rel: &str) -> &'static str`
  - `pub enum Counts { Torch, Libm, IntegerNSyn }`, `pub enum Start { Rust, Flyvis }`, `pub struct Diff { pub max: f64, pub at: usize, pub differ: usize }`, `pub fn diff(rust: &[f64], flyvis: &[f64]) -> Diff`
  - `pub struct SettlingResult { run, model, counts, settled: Diff, settling_centre: Diff, settled_len, centre_len, first_centre_difference: Option<usize>, per_type_max: Vec<f64> }` with `worst_types(&self, n: usize) -> Vec<usize>`
  - `pub struct SequenceResult { sequence: u32, family: String, n_frames: usize, snapshot_frames: Vec<usize>, centre: Diff, snapshots: Diff, first: Option<usize> }`
  - `pub struct RunResult { settling: SettlingResult, sequences: Vec<SequenceResult>, centre_movement: Vec<f64> }`
  - `pub fn settle(f: &Fixtures, run: u32, model: &str, counts: Counts) -> Result<SettlingResult, ParityError>`
  - `pub fn play(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts, start: Start, sequences: &[u32], mirror: bool) -> Result<Vec<SequenceResult>, ParityError>`
  - `pub fn run(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts) -> Result<RunResult, ParityError>`
  - `pub(crate)` for task 4 and the tests: `trait Precision` (`DTYPE`, `SUFFIX`, `torch_count`, `libm_count`, `read`), `cell_type`, `file` (now `-> Result<&FileInfo, ParityError>`), `mirrored_hexals`, `centre_nodes`, `registered`, `checked_run`, `HEXALS`, `SNAPSHOTS`, and in `inputs.rs` `SPEC`, `ENSEMBLE`, `MANIFEST`, `PROVENANCE`, `SETTLING_FILES`, `read_bytes`, `read_text`; `Fixtures`' and `TraceData`'s fields are `pub(crate)`.
  - `test_support::REAL_SPEC` becomes `pub(crate)`; `test_support` calls `parity::how_to_get`.

- [ ] **Step 1: Capture the gates' printed results before changing anything.**

  Run:
```bash
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- --nocapture settling_checks:: trace_checks:: \
  2>&1 >/dev/null | grep -E '^(exact run|libm run|trace exact|trace libm|mirrored|flashes at|moving edges at)' | sort > target/parity-before.txt
wc -l target/parity-before.txt
```
  Expected: `37 target/parity-before.txt` (8 exact and 8 libm settling
  lines, 8 and 8 trace lines, the mirrored control, two flash and two
  moving-edge lines). The redirection keeps stderr only, so libtest's own
  lines do not interleave with them.

- [ ] **Step 2: `semblance-sim` becomes a normal dependency.**

In `crates/flyvis/Cargo.toml`, replace:

```toml
[dependencies]
semblance-connectome.workspace = true

[dev-dependencies]
semblance-sim.workspace = true
```

with:

```toml
[dependencies]
semblance-connectome.workspace = true
semblance-sim.workspace = true
```

  Run: `cargo build -p semblance-flyvis && git diff --exit-code Cargo.lock`
  Expected: builds; `Cargo.lock` unchanged (a lock file already records
  dev-dependencies). If it did change, it is committed in step 13.

- [ ] **Step 3: The module root.**

Create `crates/flyvis/src/parity/mod.rs` with:

```rust
//! Parity with flyvis, as a library: what the settling and trace gates do,
//! callable from outside `cargo test`.
//!
//! It reads the fixtures (from a data directory, or from bytes the caller
//! already holds), settles a reference run, plays flyvis's twelve stimulus
//! sequences from the settled state, and compares every value with flyvis's
//! reference. It applies the bounds pre-registered in the specs as they are
//! written. Results and errors are typed; nothing here panics on input.
//!
//! The `#[cfg(test)]` gates in `settling_checks.rs` and `trace_checks.rs` are
//! thin wrappers over this module, so they run exactly the code `semblance
//! validate` runs.
//!
//! Spec: docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md,
//! section 1.

mod bounds;
mod inputs;
mod run;

pub use bounds::{settling_bounds, trace_bounds, Bound};
pub use inputs::{how_to_get, required_files, Fixtures, TraceData, TraceInputs};
pub use run::{diff, play, run, settle, Counts, Diff, RunResult, SequenceResult, SettlingResult, Start};

#[cfg(test)]
pub(crate) use run::{cell_type, file, Precision, HEXALS, SNAPSHOTS};

use std::path::PathBuf;

use crate::reference::Dtype;

/// The libm gates' float64 bound, for every quantity they compare.
pub const F64_BOUND: f64 = 1e-10;

/// flyvis's own float32-vs-float64 gaps, PROVENANCE.json `f32_f64_gap`:
/// (model, dt id, settled, settling_centre), as registered. The libm gate's
/// f32 bound is twice each (two f32 computations can sit on opposite sides
/// of the exact value).
pub const SETTLING_F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.4114366875261908e-06, 1.347661143569212e-06),
    ("049", "1/200", 6.1578304686449314e-06, 1.9363140433270587e-06),
    ("000", "1/50", 1.276739533651039e-06, 2.6109457484935206e-06),
    ("049", "1/50", 6.4440782598373936e-06, 1.1488733433040466e-05),
];

/// flyvis's own float32-vs-float64 gaps over the stimulus traces,
/// PROVENANCE.json `f32_f64_gap`: (model, dt id, centre, snapshots), as
/// registered. They include the moving edges' one rounding of the input to
/// f32.
pub const TRACE_F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.7249018977594233e-06, 2.6318956276583094e-06),
    ("049", "1/200", 2.600224337356849e-05, 5.1135401793800384e-05),
    ("000", "1/50", 2.5357808688397654e-06, 2.9820266052027478e-06),
    ("049", "1/50", 3.4419031081256435e-05, 1.4233448473766064e-04),
];

/// "Misses by far more than any bound": the model and mapping controls.
pub const CONTROL_MISS: f64 = 1e-3;

/// Sintel revisit trigger 1: a cell type is driven if its centre trace moves
/// by more than this (spec R8). Recorded, not a gate.
pub const COVERAGE_THRESHOLD: f64 = 1e-3;

/// The eight registered runs: (index, model, dt id, precision). The source
/// of every run's precision (spec R19).
pub const RUNS: [(u32, &str, &str, Dtype); 8] = [
    (0, "000", "1/200", Dtype::F32),
    (1, "000", "1/200", Dtype::F64),
    (2, "000", "1/50", Dtype::F32),
    (3, "000", "1/50", Dtype::F64),
    (4, "049", "1/200", Dtype::F32),
    (5, "049", "1/200", Dtype::F64),
    (6, "049", "1/50", Dtype::F32),
    (7, "049", "1/50", Dtype::F64),
];

/// Why a parity check could not run, or a state that went non-finite.
#[derive(Clone, Debug, PartialEq)]
pub enum ParityError {
    /// A required file is absent.
    Missing { rel: String, path: PathBuf },
    /// A required file is present but could not be read.
    Unreadable { rel: String, path: PathBuf, kind: std::io::ErrorKind },
    /// A file is present but does not parse, or does not fit the network.
    Malformed { rel: String, detail: String },
    /// The fixtures parse but disagree with each other or with what is
    /// registered.
    Mismatch(String),
    /// The integrator's state went NaN or infinite: a gate failure.
    NonFinite { run: u32, node: usize, cell_type: String, after: After },
    /// A bad `Options` value, or a question with no answer (an unregistered
    /// run, a bound for a control).
    Usage(String),
}

/// When a state went non-finite.
#[derive(Clone, Debug, PartialEq)]
pub enum After {
    /// After settle step `k` (the state after k + 1 Euler steps).
    SettleStep(usize),
    /// After frame `frame` of sequence `sequence`.
    Frame { frame: usize, sequence: u32 },
}

impl ParityError {
    /// Whether this error is a gate failure (exit 1) rather than
    /// verification not completing (exit 2). Only a non-finite state is.
    pub fn is_gate_failure(&self) -> bool {
        matches!(self, ParityError::NonFinite { .. })
    }
}

impl std::fmt::Display for ParityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParityError::Missing { rel, path } => write!(f, "{} not found. {}", path.display(), how_to_get(rel)),
            ParityError::Unreadable { path, kind, .. } => write!(f, "{}: cannot be read ({kind:?})", path.display()),
            ParityError::Malformed { rel, detail } => write!(f, "{rel}: {detail}"),
            ParityError::Mismatch(m) | ParityError::Usage(m) => f.write_str(m),
            ParityError::NonFinite { run, node, cell_type, after: After::SettleStep(k) } => {
                write!(f, "run {run}: node {node} ({cell_type}) is not finite after settle step {k}")
            }
            ParityError::NonFinite { run, node, cell_type, after: After::Frame { frame, sequence } } => {
                write!(f, "run {run}: node {node} ({cell_type}) is not finite after frame {frame} of sequence {sequence}")
            }
        }
    }
}

impl std::error::Error for ParityError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_as_registered() {
        let e = ParityError::NonFinite { run: 3, node: 7, cell_type: "Mi4".into(), after: After::SettleStep(2) };
        assert_eq!(e.to_string(), "run 3: node 7 (Mi4) is not finite after settle step 2");
        assert!(e.is_gate_failure());
        let e = ParityError::NonFinite { run: 1, node: 9, cell_type: "T4a".into(), after: After::Frame { frame: 4, sequence: 8 } };
        assert_eq!(e.to_string(), "run 1: node 9 (T4a) is not finite after frame 4 of sequence 8");
        let e = ParityError::Missing { rel: "flyvis-reference/x.bin".into(), path: PathBuf::from("d/flyvis-reference/x.bin") };
        assert_eq!(
            e.to_string(),
            "d/flyvis-reference/x.bin not found. Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        );
        assert!(!e.is_gate_failure());
        let e = ParityError::Unreadable {
            rel: "a".into(),
            path: PathBuf::from("d/a"),
            kind: std::io::ErrorKind::PermissionDenied,
        };
        assert_eq!(e.to_string(), "d/a: cannot be read (PermissionDenied)");
        let e = ParityError::Malformed { rel: "flyvis-pretrained/ensemble.csv".into(), detail: "empty".into() };
        assert_eq!(e.to_string(), "flyvis-pretrained/ensemble.csv: empty");
        for e in [ParityError::Mismatch("m".into()), ParityError::Usage("u".into())] {
            assert!(!e.is_gate_failure());
        }
    }

    /// The registry is the manifest's eight runs, in index order.
    #[test]
    fn the_registry_lists_the_eight_runs_in_index_order() {
        for (i, r) in RUNS.iter().enumerate() {
            assert_eq!(r.0 as usize, i);
            assert_eq!(r.3, if i % 2 == 0 { Dtype::F32 } else { Dtype::F64 });
        }
    }
}
```

- [ ] **Step 4: The bounds.**

Create `crates/flyvis/src/parity/bounds.rs` with:

```rust
//! The pre-registered bounds, as written in the specs: equality for torch's
//! counts, 1e-10 in f64 and twice flyvis's own f32/f64 gap in f32 for
//! Rust's `exp`.

use super::run::Counts;
use super::{ParityError, F64_BOUND, SETTLING_F32_GAPS, TRACE_F32_GAPS};
use crate::reference::Dtype;

/// One quantity's bound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bound {
    /// rust == flyvis for every value.
    Exact,
    /// max |rust - flyvis| <= 1e-10.
    F64,
    /// max |rust - flyvis| <= 2.0 * gap. `model` and `dt` are the registered
    /// table's own strings; the PROVENANCE key is "{model} {dt}".
    /// `quantity` is one of "settled", "settling_centre", "centre",
    /// "snapshots".
    F32 { gap: f64, model: &'static str, dt: &'static str, quantity: &'static str },
}

impl Bound {
    /// The largest |rust - flyvis| allowed: 0 for `Exact`.
    pub fn value(&self) -> f64 {
        match self {
            Bound::Exact => 0.0,
            Bound::F64 => F64_BOUND,
            Bound::F32 { gap, .. } => 2.0 * gap,
        }
    }
}

type Gaps = [(&'static str, &'static str, f64, f64); 4];

fn bounds(
    table: &Gaps,
    quantities: [&'static str; 2],
    model: &str,
    dt: &str,
    p: Dtype,
    counts: Counts,
) -> Result<(Bound, Bound), ParityError> {
    let &(model, dt, a, b) = table
        .iter()
        .find(|g| g.0 == model && g.1 == dt)
        .ok_or_else(|| ParityError::Mismatch(format!("no registered bound for model {model} at dt {dt}")))?;
    match (counts, p) {
        (Counts::Torch, _) => Ok((Bound::Exact, Bound::Exact)),
        (Counts::Libm, Dtype::F64) => Ok((Bound::F64, Bound::F64)),
        (Counts::Libm, Dtype::F32) => Ok((
            Bound::F32 { gap: a, model, dt, quantity: quantities[0] },
            Bound::F32 { gap: b, model, dt, quantity: quantities[1] },
        )),
        (Counts::IntegerNSyn, _) => Err(ParityError::Usage(
            "integer counts are a control with its own threshold, not a gate with a bound".into(),
        )),
    }
}

/// The settling bounds for `model` at dt id `dt` in precision `p`:
/// (settled, settling_centre).
pub fn settling_bounds(model: &str, dt: &str, p: Dtype, counts: Counts) -> Result<(Bound, Bound), ParityError> {
    bounds(&SETTLING_F32_GAPS, ["settled", "settling_centre"], model, dt, p, counts)
}

/// The trace bounds for `model` at dt id `dt` in precision `p`:
/// (centre, snapshots).
pub fn trace_bounds(model: &str, dt: &str, p: Dtype, counts: Counts) -> Result<(Bound, Bound), ParityError> {
    bounds(&TRACE_F32_GAPS, ["centre", "snapshots"], model, dt, p, counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::RUNS;

    #[test]
    fn a_bound_is_its_registered_value() {
        assert_eq!(Bound::Exact.value(), 0.0);
        assert_eq!(Bound::F64.value(), 1e-10);
        let b = Bound::F32 { gap: 1.4114366875261908e-06, model: "000", dt: "1/200", quantity: "settled" };
        assert_eq!(b.value().to_bits(), (2.0 * 1.4114366875261908e-06f64).to_bits());
    }

    #[test]
    fn every_registered_run_has_its_bounds() {
        for &(_, model, dt, p) in &RUNS {
            for (f, table, names) in [
                (settling_bounds as fn(&str, &str, Dtype, Counts) -> _, &SETTLING_F32_GAPS, ["settled", "settling_centre"]),
                (trace_bounds, &TRACE_F32_GAPS, ["centre", "snapshots"]),
            ] {
                assert_eq!(f(model, dt, p, Counts::Torch), Ok((Bound::Exact, Bound::Exact)));
                let libm = f(model, dt, p, Counts::Libm).unwrap();
                match p {
                    Dtype::F64 => assert_eq!(libm, (Bound::F64, Bound::F64)),
                    Dtype::F32 => {
                        let row = table.iter().find(|g| g.0 == model && g.1 == dt).unwrap();
                        assert_eq!(libm.0, Bound::F32 { gap: row.2, model: row.0, dt: row.1, quantity: names[0] });
                        assert_eq!(libm.1, Bound::F32 { gap: row.3, model: row.0, dt: row.1, quantity: names[1] });
                        assert_eq!(libm.0.value().to_bits(), (2.0 * row.2).to_bits());
                        assert_eq!(libm.1.value().to_bits(), (2.0 * row.3).to_bits());
                    }
                }
                assert!(matches!(f(model, dt, p, Counts::IntegerNSyn), Err(ParityError::Usage(_))));
            }
        }
    }

    #[test]
    fn an_unregistered_model_or_dt_has_no_bound() {
        for counts in [Counts::Torch, Counts::Libm] {
            assert_eq!(
                settling_bounds("001", "1/50", Dtype::F32, counts),
                Err(ParityError::Mismatch("no registered bound for model 001 at dt 1/50".into()))
            );
            assert_eq!(
                trace_bounds("000", "1/100", Dtype::F64, counts),
                Err(ParityError::Mismatch("no registered bound for model 000 at dt 1/100".into()))
            );
        }
    }
}
```

- [ ] **Step 5: The inputs.** `how_to_get` is moved verbatim from
  `test_support.rs` (all six arms; C8 depends on them).

Create `crates/flyvis/src/parity/inputs.rs` with:

```rust
//! What the parity checks read: the fixture (spec, ensemble, manifest, the
//! four settling files) and the trace files. Reading is separate from
//! parsing, so the test gates keep their own reads and skip notices while
//! `validate` reads from a data directory.

use std::path::Path;

use semblance_connectome::TypedNetwork;

use super::{ParityError, RUNS};
use crate::ensemble::Ensemble;
use crate::expand::{expand, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL};
use crate::reference::{parse_sequences, Dtype, Manifest, SequenceInfo};
use crate::spec::ConnectomeSpec;
use crate::wiring::{edge_syn_groups, input_nodes};

pub(crate) const SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";
pub(crate) const ENSEMBLE: &str = "flyvis-pretrained/ensemble.csv";
pub(crate) const MANIFEST: &str = "flyvis-reference/manifest.json";
pub(crate) const PROVENANCE: &str = "flyvis-reference/PROVENANCE.json";

/// The settling files, by their names in the manifest.
pub(crate) const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

/// The 13 files under the data directory that `validate` reads.
const REQUIRED: [&str; 13] = [
    SPEC,
    ENSEMBLE,
    MANIFEST,
    PROVENANCE,
    "flyvis-reference/settled.f32.bin",
    "flyvis-reference/settled.f64.bin",
    "flyvis-reference/settling_centre.f32.bin",
    "flyvis-reference/settling_centre.f64.bin",
    "flyvis-reference/input.f64.bin",
    "flyvis-reference/centre.f32.bin",
    "flyvis-reference/centre.f64.bin",
    "flyvis-reference/snapshots.f32.bin",
    "flyvis-reference/snapshots.f64.bin",
];

/// The 13 paths, relative to the data directory, that `validate` needs.
pub fn required_files() -> &'static [&'static str] {
    &REQUIRED
}

/// How to produce a file under `data/`, by its directory.
pub fn how_to_get(rel: &str) -> &'static str {
    match rel.split('/').next() {
        Some("flyvis-supplementary") => {
            "Download \
             https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
             and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/, \
             or run ci/fetch-fixtures.sh"
        }
        Some("flyvis-build") => {
            "Run `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-stimuli") => {
            "Run `uv run --project tools tools/export_stimuli.py`, \
             `uv run --project tools tools/export_moving_edges.py` and, \
             for the `_h80` files, `uv run --project tools tools/export_reference.py` \
             to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-pretrained") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_params.py`."
        }
        Some("flyvis-reference") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        }
        _ => "Run ci/fetch-fixtures.sh.",
    }
}

/// The bytes of `data/<rel>`: `Missing` if absent, `Unreadable` otherwise.
pub(crate) fn read_bytes(data: &Path, rel: &str) -> Result<Vec<u8>, ParityError> {
    let path = data.join(rel);
    std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ParityError::Missing { rel: rel.to_string(), path },
        kind => ParityError::Unreadable { rel: rel.to_string(), path, kind },
    })
}

/// The text of `data/<rel>`.
pub(crate) fn read_text(data: &Path, rel: &str) -> Result<String, ParityError> {
    String::from_utf8(read_bytes(data, rel)?)
        .map_err(|e| ParityError::Malformed { rel: rel.to_string(), detail: format!("not UTF-8 ({e})") })
}

fn malformed(rel: &str, detail: impl std::fmt::Display) -> ParityError {
    ParityError::Malformed { rel: rel.to_string(), detail: detail.to_string() }
}

/// Everything the settling gates read, parsed and built: the expanded
/// network, its synapse-count groups and input nodes, the ensemble, the
/// manifest, and the four settling files' bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixtures {
    pub(crate) net: TypedNetwork,
    pub(crate) groups: Vec<u32>,
    /// The photoreceptor nodes per input type, in `spec.input_units` order,
    /// each in node order: flyvis's [8, 721] `input_index`.
    pub(crate) input_nodes: Vec<Vec<u32>>,
    pub(crate) ensemble: Ensemble,
    pub(crate) manifest: Manifest,
    pub(crate) settling: Vec<(&'static str, Vec<u8>)>,
}

impl Fixtures {
    /// Reads, in the gates' order: ensemble.csv, manifest.json, the four
    /// settling files, the spec. Then [`Fixtures::from_parts`].
    pub fn load(data: &Path) -> Result<Fixtures, ParityError> {
        let ensemble = read_text(data, ENSEMBLE)?;
        let manifest = read_text(data, MANIFEST)?;
        let mut settling = Vec::new();
        for name in SETTLING_FILES {
            settling.push((name, read_bytes(data, &format!("flyvis-reference/{name}"))?));
        }
        let spec = read_text(data, SPEC)?;
        Fixtures::from_parts(&spec, &ensemble, &manifest, settling)
    }

    /// Pure: no I/O. What the test fixture and `load` both call.
    ///
    /// Refuses, as `Malformed`: a file that does not parse, a spec that does
    /// not expand to a network with flyvis's synapse-count groups and input
    /// units, a manifest whose runs are not the eight registered ones, and a
    /// settling file that is missing from `settling` or the manifest or is
    /// not the length the manifest gives.
    pub fn from_parts(
        spec_json: &str,
        ensemble_csv: &str,
        manifest_json: &str,
        settling: Vec<(&'static str, Vec<u8>)>,
    ) -> Result<Fixtures, ParityError> {
        let ensemble = Ensemble::parse(ensemble_csv).map_err(|e| malformed(ENSEMBLE, e))?;
        let manifest = Manifest::parse(manifest_json).map_err(|e| malformed(MANIFEST, e))?;
        let runs: Vec<(u32, &str, &str, Dtype)> =
            manifest.runs.iter().map(|r| (r.index, r.model.as_str(), r.dt.as_str(), r.precision)).collect();
        if runs != RUNS {
            return Err(malformed(MANIFEST, format!("the manifest's runs are not the eight registered: {runs:?}")));
        }
        for name in SETTLING_FILES {
            let rel = format!("flyvis-reference/{name}");
            let info = manifest.file(name).ok_or_else(|| malformed(MANIFEST, format!("{name} is not in the manifest")))?;
            let bytes = &settling.iter().find(|(n, _)| *n == name).ok_or_else(|| malformed(&rel, "not loaded"))?.1;
            if bytes.len() != info.bytes {
                return Err(malformed(&rel, format!("{name}: {} bytes, manifest says {}", bytes.len(), info.bytes)));
            }
        }
        let spec = ConnectomeSpec::from_json_str(spec_json).map_err(|e| malformed(SPEC, e))?;
        let net = expand(&spec, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL).map_err(|e| malformed(SPEC, e))?;
        let groups = edge_syn_groups(&net).map_err(|e| malformed(SPEC, e))?;
        let input_nodes = input_nodes(&spec, &net).map_err(|e| malformed(SPEC, e))?;
        Ok(Fixtures { net, groups, input_nodes, ensemble, manifest, settling })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn network(&self) -> &TypedNetwork {
        &self.net
    }
}

/// The trace files, all of them: what `validate` reads.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceData {
    pub(crate) sequences: Vec<SequenceInfo>,
    pub(crate) input: Vec<u8>,
    pub(crate) centre_f32: Vec<u8>,
    pub(crate) centre_f64: Vec<u8>,
    pub(crate) snapshots_f32: Vec<u8>,
    pub(crate) snapshots_f64: Vec<u8>,
}

impl TraceData {
    /// Reads the manifest's sequences, then input, centre and snapshots.
    pub fn load(data: &Path) -> Result<TraceData, ParityError> {
        let manifest = read_text(data, MANIFEST)?;
        let sequences = parse_sequences(&manifest).map_err(|e| malformed(MANIFEST, e))?;
        let file = |name: &str| read_bytes(data, &format!("flyvis-reference/{name}"));
        Ok(TraceData {
            sequences,
            input: file("input.f64.bin")?,
            centre_f32: file("centre.f32.bin")?,
            centre_f64: file("centre.f64.bin")?,
            snapshots_f32: file("snapshots.f32.bin")?,
            snapshots_f64: file("snapshots.f64.bin")?,
        })
    }

    /// `view(F32)` borrows centre.f32.bin and snapshots.f32.bin, `view(F64)`
    /// the f64 pair; both borrow input.f64.bin. `precision` is set to `p`.
    pub fn view(&self, p: Dtype) -> TraceInputs<'_> {
        let (centre, snapshots) = match p {
            Dtype::F32 => (&self.centre_f32, &self.snapshots_f32),
            Dtype::F64 => (&self.centre_f64, &self.snapshots_f64),
        };
        TraceInputs { precision: p, sequences: &self.sequences, input: &self.input, centre, snapshots }
    }
}

/// One precision's trace inputs, borrowed. The test gates build this from
/// their lazily loaded files, read in the order sequences, input, centre,
/// snapshots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceInputs<'a> {
    pub precision: Dtype,
    pub sequences: &'a [SequenceInfo],
    pub input: &'a [u8],
    pub centre: &'a [u8],
    pub snapshots: &'a [u8],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C8 depends on these: the gates' skip notices print them.
    #[test]
    fn how_to_get_names_the_way_to_each_directory() {
        assert_eq!(
            how_to_get("flyvis-supplementary/x.json"),
            "Download https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/\
             41586_2024_7939_MOESM3_ESM.zip and extract Lappalainen_et_al_connectome_avg_filters.json into \
             data/flyvis-supplementary/, or run ci/fetch-fixtures.sh"
        );
        assert_eq!(
            how_to_get("flyvis-build/nodes.csv"),
            "Run `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
        );
        assert_eq!(
            how_to_get("flyvis-stimuli/flashes.bin"),
            "Run `uv run --project tools tools/export_stimuli.py`, `uv run --project tools \
             tools/export_moving_edges.py` and, for the `_h80` files, `uv run --project tools \
             tools/export_reference.py` to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
        );
        assert_eq!(
            how_to_get("flyvis-pretrained/ensemble.csv"),
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_params.py`."
        );
        assert_eq!(
            how_to_get("flyvis-reference/manifest.json"),
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        );
        assert_eq!(how_to_get("elsewhere/x"), "Run ci/fetch-fixtures.sh.");
    }

    #[test]
    fn the_required_files_are_the_thirteen_the_gates_read() {
        let r = required_files();
        assert_eq!(r.len(), 13);
        for (i, a) in r.iter().enumerate() {
            assert!(!r[..i].contains(a), "{a} listed twice");
        }
        for name in SETTLING_FILES {
            assert!(r.contains(&format!("flyvis-reference/{name}").as_str()), "{name}");
        }
    }

    #[test]
    fn a_missing_file_is_missing_and_names_its_path() {
        let dir = std::env::temp_dir().join(format!("semblance-parity-{}-missing", std::process::id()));
        match read_bytes(&dir, "flyvis-reference/manifest.json") {
            Err(ParityError::Missing { rel, path }) => {
                assert_eq!(rel, "flyvis-reference/manifest.json");
                assert_eq!(path, dir.join("flyvis-reference/manifest.json"));
            }
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    const MANIFEST_RUNS: &str = r#""runs":[
        {"index":0,"model":"000","dt":"1/200","precision":"f32"},{"index":1,"model":"000","dt":"1/200","precision":"f64"},
        {"index":2,"model":"000","dt":"1/50","precision":"f32"},{"index":3,"model":"000","dt":"1/50","precision":"f64"},
        {"index":4,"model":"049","dt":"1/200","precision":"f32"},{"index":5,"model":"049","dt":"1/200","precision":"f64"},
        {"index":6,"model":"049","dt":"1/50","precision":"f32"},{"index":7,"model":"049","dt":"1/50","precision":"f64"}]"#;

    /// A manifest that parses, with the eight runs and four empty settling
    /// files of 4 or 8 bytes each.
    fn manifest(runs: &str) -> String {
        let file = |name: &str, dtype: &str, bytes: usize| {
            format!(
                r#"{{"name":"{name}","dtype":"{dtype}","axes":["n"],"bytes":{bytes},
                     "blocks":[{{"run":0,"dt":"1/50","sequence":null,"offset":0,"bytes":{bytes},"shape":[1]}}]}}"#
            )
        };
        format!(
            r#"{{"schema_version":1,"byte_order":"little","array_order":"C","n_nodes":1,"t_pre":1.0,"pre_stim_value":0.5,
              "dts":[{{"id":"1/50","dt":0.02,"floor_dt_f32":0.019999999552965164,"n_settle":50}}],
              {runs},"centre_nodes":[],"files":[{},{},{},{}]}}"#,
            file("settled.f32.bin", "<f4", 4),
            file("settled.f64.bin", "<f8", 8),
            file("settling_centre.f32.bin", "<f4", 4),
            file("settling_centre.f64.bin", "<f8", 8)
        )
    }

    type Settling = Vec<(&'static str, Vec<u8>)>;

    fn settling(short: Option<&str>) -> Settling {
        SETTLING_FILES
            .iter()
            .map(|&n| {
                let len = if n.contains("f32") { 4 } else { 8 };
                (n, vec![0u8; if Some(n) == short { len - 1 } else { len }])
            })
            .collect()
    }

    /// A valid ensemble: 50 models, each with model 000's parameters.
    pub(crate) fn synthetic_ensemble() -> String {
        use crate::ensemble::{ModelParams, N_MODELS};
        use crate::pretrained::{CELL_TYPES, CONN_TYPES};
        let p = ModelParams::pretrained();
        let mut out = String::from("model,kind,index,name,value\n");
        for m in 0..N_MODELS {
            for (i, t) in CELL_TYPES.iter().enumerate() {
                out.push_str(&format!("{m:03},bias,{i},{t},{}\n", p.bias[i]));
            }
            for (i, t) in CELL_TYPES.iter().enumerate() {
                out.push_str(&format!("{m:03},time_const,{i},{t},{}\n", p.time_const[i]));
            }
            for (i, (s, t)) in CONN_TYPES.iter().enumerate() {
                out.push_str(&format!("{m:03},syn_strength,{i},{s}>{t},{}\n", p.syn_strength[i]));
            }
        }
        out
    }

    /// Malformed input is an error naming the file, never a panic.
    #[test]
    fn from_parts_refuses_malformed_input_without_panicking() {
        let ensemble = synthetic_ensemble();
        assert!(Ensemble::parse(&ensemble).is_ok(), "the synthetic ensemble is valid");
        let one_run = MANIFEST_RUNS[..MANIFEST_RUNS.find("},").unwrap() + 1].to_string() + "]";
        let cases: [(&str, &str, String, Settling, &str); 4] = [
            ("an empty spec", &ensemble, manifest(MANIFEST_RUNS), settling(None), SPEC),
            ("a truncated ensemble", &ensemble[..ensemble.len() / 2], manifest(MANIFEST_RUNS), settling(None), ENSEMBLE),
            ("a manifest with one run", &ensemble, manifest(&one_run), settling(None), MANIFEST),
            (
                "a settling file one byte short",
                &ensemble,
                manifest(MANIFEST_RUNS),
                settling(Some("settled.f64.bin")),
                "flyvis-reference/settled.f64.bin",
            ),
        ];
        for (what, ensemble_csv, manifest_json, files, rel) in cases {
            match Fixtures::from_parts("", ensemble_csv, &manifest_json, files) {
                Err(ParityError::Malformed { rel: got, detail }) => assert_eq!(got, rel, "{what}: {detail}"),
                other => panic!("{what}: expected Malformed naming {rel}, got {:?}", other.map(|_| ())),
            }
        }
    }
}
```

- [ ] **Step 6: The core.** `settle_state`, `compare_settling` and
  `play_from` are piece 1's and piece 2's code with each `assert!`/`expect`/
  `panic!` on input turned into a `ParityError` carrying the same text
  (C5, C6, C12, C13). `diff` keeps its length `assert_eq!` as an internal
  invariant: every caller has checked the shapes first.

Create `crates/flyvis/src/parity/run.rs` with:

```rust
//! Settling, trace playback and comparison: piece 1's and piece 2's gate
//! machinery, moved here without changing what it computes.
//!
//! One core (spec R20): `settle_state` builds and settles the integrator,
//! `compare_settling` compares the settled state, and `play_from` plays
//! sequences from a settled state. `settle`, `play` and `run` are thin over
//! them, so the gates exercise exactly the code `validate` runs. Each entry
//! point is non-generic and dispatches on the run's registered precision
//! (`RUNS`, R19), so the integrator is monomorphised in this crate.

use semblance_connectome::TypedNetwork;
use semblance_sim::graded::{Graded, GradedParams, Real};

use super::{After, Fixtures, ParityError, TraceInputs, RUNS};
use crate::expand::{hex_coords, DEFAULT_EXTENT};
use crate::pretrained::SYN_COUNT;
use crate::reference::{Array, BlockKey, DtInfo, Dtype, FileInfo, ReferenceError, RunInfo};
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use crate::wiring::scatter_hexals;

pub(crate) const HEXALS: usize = 721;

/// Snapshot frames per sequence, at every dt (`snapshots.*.bin`'s first axis).
pub(crate) const SNAPSHOTS: usize = 6;

/// Where the per-edge synapse counts come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counts {
    /// flyvis's own: torch's `exp`, from `syn_count_torch.rs`.
    Torch,
    /// Rust's `exp` of `SYN_COUNT`.
    Libm,
    /// The spec's integer counts, never exponentiated: a negative control.
    IntegerNSyn,
}

/// Which settled state each sequence starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Rust's own settling: settling and stimulus, end to end.
    Rust,
    /// flyvis's `settled.<p>.bin`: the first diagnostic step, isolating the
    /// stimulus phase.
    Flyvis,
}

/// The largest absolute error, where, and how many values are not equal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diff {
    pub max: f64,
    pub at: usize,
    pub differ: usize,
}

/// A NaN on either side counts as an infinite error, so it becomes the
/// maximum and fails every bound, instead of being skipped by `>`. The two
/// slices must be the same length: every caller has checked the shapes.
pub fn diff(rust: &[f64], flyvis: &[f64]) -> Diff {
    assert_eq!(rust.len(), flyvis.len());
    let mut d = Diff { max: 0.0, at: 0, differ: 0 };
    for (i, (a, b)) in rust.iter().zip(flyvis).enumerate() {
        let e = (a - b).abs();
        let e = if e.is_nan() { f64::INFINITY } else { e };
        d.differ += usize::from(a != b);
        if e > d.max {
            d.max = e;
            d.at = i;
        }
    }
    d
}

/// One run's settling, compared with flyvis's.
#[derive(Clone, Debug, PartialEq)]
pub struct SettlingResult {
    pub run: u32,
    pub model: String,
    pub counts: Counts,
    /// Over `[n_nodes]`.
    pub settled: Diff,
    /// Over `[n_settle, 65]`.
    pub settling_centre: Diff,
    pub settled_len: usize,
    pub centre_len: usize,
    /// The first unequal settling-centre value, as an index into [n_settle, 65].
    pub first_centre_difference: Option<usize>,
    /// The settled state's largest |error| per cell type, in the network's
    /// cell-type order.
    pub per_type_max: Vec<f64>,
}

impl SettlingResult {
    /// The `n` cell types with the largest settled error, largest first; a
    /// tie keeps cell-type order.
    pub fn worst_types(&self, n: usize) -> Vec<usize> {
        let mut worst: Vec<usize> = (0..self.per_type_max.len()).collect();
        worst.sort_by(|&a, &b| self.per_type_max[b].total_cmp(&self.per_type_max[a]));
        worst.truncate(n);
        worst
    }
}

/// One sequence played and compared.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceResult {
    pub sequence: u32,
    pub family: String,
    pub n_frames: usize,
    pub snapshot_frames: Vec<usize>,
    /// Over `[n_frames, 65]`.
    pub centre: Diff,
    /// Over `[6, n_nodes]`.
    pub snapshots: Diff,
    /// The first unequal centre value, as an index into [n_frames, 65].
    pub first: Option<usize>,
}

/// One run, settled once and played through every sequence from that state.
#[derive(Clone, Debug, PartialEq)]
pub struct RunResult {
    pub settling: SettlingResult,
    /// All 12, in sequence order.
    pub sequences: Vec<SequenceResult>,
    /// Per centre type, in `CELL_TYPES` order: the largest |centre value -
    /// settled centre value| over every sequence and frame, both widened to
    /// f64 before subtracting.
    pub centre_movement: Vec<f64>,
}

/// The two precisions, each with its count tables and reference reader.
pub(crate) trait Precision: Real {
    const DTYPE: Dtype;
    /// "f32" or "f64": the files' suffix and the gates' label.
    const SUFFIX: &'static str = dtype_name(Self::DTYPE);
    fn torch_count(group: usize) -> Self;
    fn libm_count(group: usize) -> Self;
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>;
}

impl Precision for f32 {
    const DTYPE: Dtype = Dtype::F32;
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F32[g] }
    fn libm_count(g: usize) -> Self { SYN_COUNT[g].exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f32(bytes, key)
    }
}

impl Precision for f64 {
    const DTYPE: Dtype = Dtype::F64;
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F64[g] }
    fn libm_count(g: usize) -> Self { (SYN_COUNT[g] as f64).exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f64(bytes, key)
    }
}

pub(crate) const fn dtype_name(p: Dtype) -> &'static str {
    match p {
        Dtype::F32 => "f32",
        Dtype::F64 => "f64",
    }
}

fn mismatch(m: String) -> ParityError {
    ParityError::Mismatch(m)
}

pub(crate) fn cell_type(net: &TypedNetwork, node: usize) -> &str {
    &net.cell_types()[net.node_cell_type()[node] as usize]
}

/// A file the manifest lists.
pub(crate) fn file<'a>(f: &'a Fixtures, name: &str) -> Result<&'a FileInfo, ParityError> {
    f.manifest.file(name).ok_or_else(|| ParityError::Malformed {
        rel: "flyvis-reference/manifest.json".into(),
        detail: format!("{name} is not in the manifest"),
    })
}

/// For each hexal h, the hexal at (-u, -v): a point reflection of the frame.
/// The lattice is point-symmetric by construction, so every hexal has one.
pub(crate) fn mirrored_hexals() -> Vec<usize> {
    let c: Vec<(i32, i32)> = hex_coords(DEFAULT_EXTENT).collect();
    c.iter()
        .enumerate()
        .map(|(h, &(u, v))| c.iter().position(|&p| p == (-u, -v)).unwrap_or(h))
        .collect()
}

/// The centre nodes' indices, each checked to be inside the network.
pub(crate) fn centre_nodes(f: &Fixtures) -> Result<Vec<usize>, ParityError> {
    let n = f.net.num_nodes();
    f.manifest
        .centre_nodes
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let node = c.node as usize;
            if node < n {
                Ok(node)
            } else {
                Err(mismatch(format!("centre node {k}: node {node} is outside the network's {n} nodes")))
            }
        })
        .collect()
}

/// Run `run`'s registered precision (R19).
pub(crate) fn registered(run: u32) -> Result<Dtype, ParityError> {
    RUNS.get(run as usize)
        .map(|r| r.3)
        .ok_or_else(|| ParityError::Usage(format!("run {run} is not one of the {} registered runs", RUNS.len())))
}

/// Run `run`'s manifest entry and dt, with piece 1's checks in their order:
/// the run's index, its precision against the registry, the dt and the
/// number of settling steps. Returns the registered precision too.
pub(crate) fn checked_run(f: &Fixtures, run: u32) -> Result<(&RunInfo, &DtInfo, Dtype), ParityError> {
    let want = registered(run)?;
    let info = f.manifest.runs.get(run as usize).ok_or_else(|| mismatch(format!("run {run} is not in the manifest")))?;
    if info.index != run {
        return Err(mismatch(format!("manifest runs must be listed in index order: entry {run} is run {}", info.index)));
    }
    if info.precision != want {
        return Err(mismatch(format!(
            "run {run}'s precision: manifest {}, expected {}",
            dtype_name(info.precision),
            dtype_name(want)
        )));
    }
    let dt = f
        .manifest
        .dts
        .iter()
        .find(|d| d.id == info.dt)
        .ok_or_else(|| mismatch(format!("run {run}'s dt {} is not listed in the manifest", info.dt)))?;
    let want_dt = match dt.id.as_str() {
        "1/200" => 1.0 / 200.0,
        "1/50" => 0.02,
        other => return Err(mismatch(format!("unexpected dt id {other}"))),
    };
    if dt.dt != want_dt {
        return Err(mismatch(format!("dt {}: manifest {}, expected {want_dt}", dt.id, dt.dt)));
    }
    let n_settle = (f.manifest.t_pre / dt.dt) as usize;
    if dt.n_settle != n_settle {
        return Err(mismatch(format!(
            "settling steps at dt {}: manifest {}, t_pre / dt gives {n_settle}",
            dt.id, dt.n_settle
        )));
    }
    Ok((info, dt, want))
}

/// One block of a settling file (`settled.*`, `settling_centre.*`).
fn read<R: Precision>(f: &Fixtures, name: &str, key: &BlockKey) -> Result<Array<R>, ParityError> {
    let rel = format!("flyvis-reference/{name}");
    let info = file(f, name)?;
    let bytes = &f
        .settling
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| ParityError::Malformed { rel: rel.clone(), detail: "not loaded".into() })?
        .1;
    R::read(info, bytes, key).map_err(|e| ParityError::Malformed { rel, detail: e.to_string() })
}

/// "Bit for bit" is `==` on f64-widened values, which is bit equality only
/// when no reference value is +0.0 or -0.0. The reference has none; a
/// fixture that does is refused rather than compared.
fn no_signed_zero(name: &str, key: &BlockKey, values: &[f64]) -> Result<(), ParityError> {
    match values.iter().position(|&v| v == 0.0) {
        None => Ok(()),
        Some(i) => {
            let mut at = Vec::new();
            if let Some(r) = key.run {
                at.push(format!("run {r}"));
            }
            at.push(format!("dt {}", key.dt));
            if let Some(s) = key.sequence {
                at.push(format!("sequence {s}"));
            }
            Err(mismatch(format!(
                "{name} ({}): reference value {i} is ±0.0; == is bit equality only without signed zeros",
                at.join(", ")
            )))
        }
    }
}

fn widen<R: Real>(values: &[R]) -> Vec<f64> {
    values.iter().map(|v| v.to_f64()).collect()
}

/// Builds `model`'s integrator at reference run `run`'s dt and precision and
/// settles it, as flyvis does: `n_settle` steps of grey on every
/// photoreceptor. Returns the settled integrator and the centre values
/// recorded after every step, widened to f64.
fn settle_state<R: Precision>(
    f: &Fixtures,
    run: u32,
    dt: &DtInfo,
    model: &str,
    counts: Counts,
    centre: &[usize],
) -> Result<(Graded<R>, Vec<f64>), ParityError> {
    let p = f.ensemble.model(model).ok_or_else(|| mismatch(format!("model {model} is not in the ensemble")))?;
    let count: Vec<R> = f
        .groups
        .iter()
        .enumerate()
        .map(|(e, &g)| match counts {
            Counts::Torch => R::torch_count(g as usize),
            Counts::Libm => R::libm_count(g as usize),
            Counts::IntegerNSyn => R::from_f32(f.net.edge_n_syn()[e]),
        })
        .collect();
    let params = GradedParams { bias: &p.bias, time_const: &p.time_const, syn_strength: &p.syn_strength, edge_syn_count: &count };
    let mut g = Graded::<R>::new(&f.net, &params, dt.dt).map_err(|e| mismatch(format!("run {run}: {e}")))?;

    let mut input = vec![R::ZERO; f.net.num_nodes()];
    for &i in f.input_nodes.iter().flatten() {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    let mut rust_centre = Vec::new();
    for step in 0..dt.n_settle {
        g.step(&input).map_err(|e| mismatch(format!("run {run}: {e}")))?;
        if let Some(i) = g.state().iter().position(|v| !v.is_finite()) {
            let cell_type = cell_type(&f.net, i).to_string();
            return Err(ParityError::NonFinite { run, node: i, cell_type, after: After::SettleStep(step) });
        }
        // settle step `step` is the state after step + 1 Euler steps
        rust_centre.extend(centre.iter().map(|&i| g.state()[i].to_f64()));
    }
    Ok((g, rust_centre))
}

/// Reads run `run`'s settling reference and compares the settled state and
/// the centre trace with it.
#[allow(clippy::too_many_arguments)]
fn compare_settling<R: Precision>(
    f: &Fixtures,
    run: u32,
    info: &RunInfo,
    dt: &DtInfo,
    model: &str,
    counts: Counts,
    state: &[R],
    rust_centre: Vec<f64>,
) -> Result<SettlingResult, ParityError> {
    let n = f.net.num_nodes();
    let n_centre = f.manifest.centre_nodes.len();
    let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
    let settled_name = format!("settled.{}.bin", R::SUFFIX);
    let settled = read::<R>(f, &settled_name, &key)?;
    if settled.shape != [n] {
        return Err(mismatch(format!("settled shape: expected [{n}], found {:?}", settled.shape)));
    }
    let centre_name = format!("settling_centre.{}.bin", R::SUFFIX);
    let flyvis_centre = read::<R>(f, &centre_name, &key)?;
    if flyvis_centre.shape != [dt.n_settle, n_centre] {
        return Err(mismatch(format!(
            "settling_centre shape: expected [{}, {n_centre}], found {:?}",
            dt.n_settle, flyvis_centre.shape
        )));
    }
    let rust = widen(state);
    let flyvis = widen(&settled.data);
    let flyvis_centre = widen(&flyvis_centre.data);
    no_signed_zero(&settled_name, &key, &flyvis)?;
    no_signed_zero(&centre_name, &key, &flyvis_centre)?;

    let mut per_type_max = vec![0.0f64; f.net.cell_types().len()];
    for (i, (a, b)) in rust.iter().zip(&flyvis).enumerate() {
        let t = f.net.node_cell_type()[i] as usize;
        per_type_max[t] = per_type_max[t].max((a - b).abs());
    }
    Ok(SettlingResult {
        run,
        model: model.to_string(),
        counts,
        settled: diff(&rust, &flyvis),
        settling_centre: diff(&rust_centre, &flyvis_centre),
        settled_len: rust.len(),
        centre_len: rust_centre.len(),
        first_centre_difference: rust_centre.iter().zip(&flyvis_centre).position(|(a, b)| a != b),
        per_type_max,
    })
}

/// Plays `sequences` from `settled`, each with flyvis's input (mirrored
/// hexal by hexal if `mirror`), and compares every frame's centre and every
/// snapshot with flyvis's. Also returns each centre type's largest movement
/// away from `settled`, over everything played.
#[allow(clippy::too_many_arguments)]
fn play_from<R: Precision>(
    f: &Fixtures,
    t: &TraceInputs,
    run: u32,
    info: &RunInfo,
    centre: &[usize],
    g: &mut Graded<R>,
    settled: &[R],
    sequences: &[u32],
    mirror: bool,
) -> Result<(Vec<SequenceResult>, Vec<f64>), ParityError> {
    let n = f.net.num_nodes();
    let source: Vec<usize> = if mirror { mirrored_hexals() } else { (0..HEXALS).collect() };
    let input_file = file(f, "input.f64.bin")?;
    let centre_name = format!("centre.{}.bin", R::SUFFIX);
    let snapshots_name = format!("snapshots.{}.bin", R::SUFFIX);
    let centre_file = file(f, &centre_name)?;
    let snapshots_file = file(f, &snapshots_name)?;
    let malformed = |name: &str, e: ReferenceError| ParityError::Malformed {
        rel: format!("flyvis-reference/{name}"),
        detail: e.to_string(),
    };
    let settled_centre: Vec<f64> = centre.iter().map(|&k| settled[k].to_f64()).collect();
    let mut movement = vec![0.0f64; centre.len()];

    // Zeroed once per run: every frame overwrites the same input nodes, and
    // every other node stays 0, as in flyvis's zeroed buffer.
    let mut input = vec![R::ZERO; n];
    let mut frame = vec![R::ZERO; HEXALS];
    let mut played = Vec::with_capacity(sequences.len());
    for &s in sequences {
        let q = t
            .sequences
            .get(s as usize)
            .ok_or_else(|| mismatch(format!("sequence {s} is not in the manifest's sequences")))?;
        if q.index != s {
            return Err(mismatch(format!(
                "manifest sequences must be listed in index order: entry {s} is sequence {}",
                q.index
            )));
        }
        let layout = q.frames(&info.dt).ok_or_else(|| mismatch(format!("sequence {s} has no frames at dt {}", info.dt)))?;
        let n_frames = layout.n_frames;
        if layout.snapshot_frames.len() != SNAPSHOTS {
            return Err(mismatch(format!(
                "sequence {s}'s snapshot frames: expected {SNAPSHOTS}, found {}",
                layout.snapshot_frames.len()
            )));
        }
        g.set_state(settled).map_err(|e| mismatch(format!("run {run}: {e}")))?;

        let x = input_file
            .read_f64(t.input, &BlockKey { run: None, dt: &info.dt, sequence: Some(s) })
            .map_err(|e| malformed("input.f64.bin", e))?;
        if x.shape != [n_frames, HEXALS] {
            return Err(mismatch(format!("input shape, sequence {s}: expected [{n_frames}, {HEXALS}], found {:?}", x.shape)));
        }
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(s) };
        let flyvis_centre = R::read(centre_file, t.centre, &key).map_err(|e| malformed(&centre_name, e))?;
        if flyvis_centre.shape != [n_frames, centre.len()] {
            return Err(mismatch(format!(
                "centre shape, sequence {s}: expected [{n_frames}, {}], found {:?}",
                centre.len(),
                flyvis_centre.shape
            )));
        }
        let flyvis_snapshots = R::read(snapshots_file, t.snapshots, &key).map_err(|e| malformed(&snapshots_name, e))?;
        if flyvis_snapshots.shape != [SNAPSHOTS, n] {
            return Err(mismatch(format!(
                "snapshots shape, sequence {s}: expected [{SNAPSHOTS}, {n}], found {:?}",
                flyvis_snapshots.shape
            )));
        }
        let flyvis_centre = widen(&flyvis_centre.data);
        let flyvis_snapshots = widen(&flyvis_snapshots.data);
        no_signed_zero(&centre_name, &key, &flyvis_centre)?;
        no_signed_zero(&snapshots_name, &key, &flyvis_snapshots)?;

        let mut rust_centre = Vec::with_capacity(n_frames * centre.len());
        let mut rust_snapshots = Vec::with_capacity(SNAPSHOTS * n);
        for i in 0..n_frames {
            let row = &x.data[i * HEXALS..(i + 1) * HEXALS];
            for (slot, &h) in frame.iter_mut().zip(&source) {
                *slot = R::from_f64(row[h]);
            }
            scatter_hexals(&f.input_nodes, &frame, &mut input).map_err(|e| mismatch(format!("run {run}: {e}")))?;
            g.step(&input).map_err(|e| mismatch(format!("run {run}: {e}")))?;
            if let Some(k) = g.state().iter().position(|v| !v.is_finite()) {
                let cell_type = cell_type(&f.net, k).to_string();
                return Err(ParityError::NonFinite { run, node: k, cell_type, after: After::Frame { frame: i, sequence: s } });
            }
            // frame i is the state after i + 1 steps from the settled state
            for (j, &k) in centre.iter().enumerate() {
                let v = g.state()[k].to_f64();
                rust_centre.push(v);
                movement[j] = movement[j].max((v - settled_centre[j]).abs());
            }
            if layout.snapshot_frames.contains(&i) {
                rust_snapshots.extend(g.state().iter().map(|v| v.to_f64()));
            }
        }
        played.push(SequenceResult {
            sequence: s,
            family: q.family.clone(),
            n_frames,
            snapshot_frames: layout.snapshot_frames.clone(),
            first: rust_centre.iter().zip(&flyvis_centre).position(|(a, b)| a != b),
            centre: diff(&rust_centre, &flyvis_centre),
            snapshots: diff(&rust_snapshots, &flyvis_snapshots),
        });
    }
    Ok((played, movement))
}

fn check_inputs(t: &TraceInputs, run: u32, want: Dtype) -> Result<(), ParityError> {
    if t.precision != want {
        return Err(mismatch(format!(
            "trace inputs are {}, run {run} is {}",
            dtype_name(t.precision),
            dtype_name(want)
        )));
    }
    Ok(())
}

fn settle_as<R: Precision>(f: &Fixtures, run: u32, model: &str, counts: Counts) -> Result<SettlingResult, ParityError> {
    let (info, dt, _) = checked_run(f, run)?;
    let centre = centre_nodes(f)?;
    let (g, rust_centre) = settle_state::<R>(f, run, dt, model, counts, &centre)?;
    compare_settling::<R>(f, run, info, dt, model, counts, g.state(), rust_centre)
}

#[allow(clippy::too_many_arguments)]
fn play_as<R: Precision>(
    f: &Fixtures,
    t: &TraceInputs,
    run: u32,
    counts: Counts,
    start: Start,
    sequences: &[u32],
    mirror: bool,
) -> Result<Vec<SequenceResult>, ParityError> {
    let (info, dt, _) = checked_run(f, run)?;
    let centre = centre_nodes(f)?;
    let (mut g, _) = settle_state::<R>(f, run, dt, &info.model, counts, &centre)?;
    if let Start::Flyvis = start {
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
        let settled = read::<R>(f, &format!("settled.{}.bin", R::SUFFIX), &key)?;
        g.set_state(&settled.data).map_err(|e| mismatch(format!("run {run}: {e}")))?;
    }
    let settled = g.state().to_vec();
    Ok(play_from::<R>(f, t, run, info, &centre, &mut g, &settled, sequences, mirror)?.0)
}

fn run_as<R: Precision>(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts) -> Result<RunResult, ParityError> {
    let (info, dt, _) = checked_run(f, run)?;
    let centre = centre_nodes(f)?;
    let (mut g, rust_centre) = settle_state::<R>(f, run, dt, &info.model, counts, &centre)?;
    let settled = g.state().to_vec();
    let settling = compare_settling::<R>(f, run, info, dt, &info.model, counts, &settled, rust_centre)?;
    let all: Vec<u32> = t.sequences.iter().map(|q| q.index).collect();
    let (sequences, centre_movement) = play_from::<R>(f, t, run, info, &centre, &mut g, &settled, &all, false)?;
    Ok(RunResult { settling, sequences, centre_movement })
}

/// Settles `model`'s network at reference run `run`'s dt and registered
/// precision, and compares it with that run's reference.
pub fn settle(f: &Fixtures, run: u32, model: &str, counts: Counts) -> Result<SettlingResult, ParityError> {
    match registered(run)? {
        Dtype::F32 => settle_as::<f32>(f, run, model, counts),
        Dtype::F64 => settle_as::<f64>(f, run, model, counts),
    }
}

/// Settles run `run` with its own model (or starts from flyvis's settled
/// state), then plays `sequences` from that state and compares them.
pub fn play(
    f: &Fixtures,
    t: &TraceInputs,
    run: u32,
    counts: Counts,
    start: Start,
    sequences: &[u32],
    mirror: bool,
) -> Result<Vec<SequenceResult>, ParityError> {
    let p = registered(run)?;
    check_inputs(t, run, p)?;
    match p {
        Dtype::F32 => play_as::<f32>(f, t, run, counts, start, sequences, mirror),
        Dtype::F64 => play_as::<f64>(f, t, run, counts, start, sequences, mirror),
    }
}

/// Settle once, then play all of the manifest's sequences from that state:
/// both stages from one settle.
pub fn run(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts) -> Result<RunResult, ParityError> {
    let p = registered(run)?;
    check_inputs(t, run, p)?;
    match p {
        Dtype::F32 => run_as::<f32>(f, t, run, counts),
        Dtype::F64 => run_as::<f64>(f, t, run, counts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_support::fixture;
    use crate::reference::parse_sequences;
    use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text};

    #[test]
    fn a_nan_reference_value_makes_the_maximum_infinite() {
        let d = diff(&[1.0, 2.0, 3.0], &[1.0, f64::NAN, 3.5]);
        assert_eq!(d.max, f64::INFINITY, "a NaN must not be skipped");
        assert_eq!(d.at, 1);
        assert_eq!(d.differ, 2);
    }

    #[test]
    fn signed_zeros_in_the_reference_are_refused() {
        let key = BlockKey { run: Some(3), dt: "1/50", sequence: Some(8) };
        assert_eq!(no_signed_zero("centre.f64.bin", &key, &[1.0, -2.0]), Ok(()));
        for z in [0.0, -0.0] {
            assert_eq!(
                no_signed_zero("centre.f64.bin", &key, &[1.0, z]),
                Err(ParityError::Mismatch(
                    "centre.f64.bin (run 3, dt 1/50, sequence 8): reference value 1 is ±0.0; \
                     == is bit equality only without signed zeros"
                        .into()
                ))
            );
        }
    }

    #[test]
    fn worst_types_are_largest_first_and_keep_order_on_ties() {
        let r = SettlingResult {
            run: 0,
            model: "000".into(),
            counts: Counts::Libm,
            settled: Diff { max: 0.0, at: 0, differ: 0 },
            settling_centre: Diff { max: 0.0, at: 0, differ: 0 },
            settled_len: 4,
            centre_len: 0,
            first_centre_difference: None,
            per_type_max: vec![1.0, 3.0, 1.0, 2.0],
        };
        assert_eq!(r.worst_types(3), [1, 3, 0]);
        assert_eq!(r.worst_types(9), [1, 3, 0, 2]);
    }

    #[test]
    fn the_mirror_is_a_point_reflection_of_the_lattice() {
        let m = mirrored_hexals();
        let c: Vec<(i32, i32)> = hex_coords(DEFAULT_EXTENT).collect();
        assert_eq!(m.len(), HEXALS);
        for (h, &k) in m.iter().enumerate() {
            assert_eq!(c[k], (-c[h].0, -c[h].1));
            assert_eq!(m[k], h);
        }
    }

    /// The f64 trace files, read by the test itself.
    struct F64Traces {
        sequences: Vec<crate::reference::SequenceInfo>,
        input: Vec<u8>,
        centre: Vec<u8>,
        snapshots: Vec<u8>,
    }

    fn f64_traces() -> Option<F64Traces> {
        let text = or_skip(read_fixture_text("flyvis-reference/manifest.json"))?;
        let sequences = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let input = or_skip(read_fixture_bytes("flyvis-reference/input.f64.bin"))?;
        let centre = or_skip(read_fixture_bytes("flyvis-reference/centre.f64.bin"))?;
        let snapshots = or_skip(read_fixture_bytes("flyvis-reference/snapshots.f64.bin"))?;
        Some(F64Traces { sequences, input, centre, snapshots })
    }

    /// R20: `run` is `settle` then `play` from the same state, field for
    /// field, because all three are the same core.
    #[test]
    fn run_equals_settle_then_play() {
        let Some(f) = fixture() else { return };
        let Some(x) = f64_traces() else { return };
        let t = TraceInputs { precision: Dtype::F64, sequences: &x.sequences, input: &x.input, centre: &x.centre, snapshots: &x.snapshots };
        let all: Vec<u32> = (0..12).collect();
        let r = run(f, &t, 3, Counts::Torch).unwrap_or_else(|e| panic!("{e}"));
        let s = settle(f, 3, "000", Counts::Torch).unwrap_or_else(|e| panic!("{e}"));
        let p = play(f, &t, 3, Counts::Torch, Start::Rust, &all, false).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.settling, s);
        assert_eq!(r.sequences, p);
        assert_eq!(r.centre_movement.len(), 65);
        assert!(r.centre_movement.iter().all(|m| m.is_finite() && *m > 0.0), "{:?}", r.centre_movement);
    }

    /// R19: the trace inputs' precision must be the run's.
    #[test]
    fn play_refuses_trace_inputs_of_the_other_precision() {
        let Some(f) = fixture() else { return };
        let t = TraceInputs { precision: Dtype::F32, sequences: &[], input: &[], centre: &[], snapshots: &[] };
        for result in [
            play(f, &t, 3, Counts::Torch, Start::Rust, &[8], false).map(|_| ()),
            run(f, &t, 3, Counts::Torch).map(|_| ()),
        ] {
            assert_eq!(result, Err(ParityError::Mismatch("trace inputs are f32, run 3 is f64".into())));
        }
        assert!(matches!(settle(f, 8, "000", Counts::Torch), Err(ParityError::Usage(_))));
    }
}
```

- [ ] **Step 7: Declare the module.**

In `crates/flyvis/src/lib.rs`, replace:

```rust
#[cfg(test)]
mod moving_edge_checks;
pub mod pretrained;
#[cfg(test)]
```

with:

```rust
#[cfg(test)]
mod moving_edge_checks;
pub mod parity;
pub mod pretrained;
#[cfg(test)]
```

- [ ] **Step 8: `test_support` uses `parity::how_to_get`** (R13), and exposes
  the spec's path to the gate fixture.

In `crates/flyvis/src/test_support.rs`, replace:

```rust

/// The real, published flyvis connectome spec, under `data/`.
const REAL_SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";

/// `rel`'s path under the workspace's `data/`, as every notice prints it.
fn data_path(rel: &str) -> String {
    format!("{}/../../data/{rel}", env!("CARGO_MANIFEST_DIR"))
}

/// How to produce a file under `data/`, by its directory.
fn how_to_get(rel: &str) -> &'static str {
    match rel.split('/').next() {
        Some("flyvis-supplementary") => {
            "Download \
             https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
             and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/, \
             or run ci/fetch-fixtures.sh"
        }
        Some("flyvis-build") => {
            "Run `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-stimuli") => {
            "Run `uv run --project tools tools/export_stimuli.py`, \
             `uv run --project tools tools/export_moving_edges.py` and, \
             for the `_h80` files, `uv run --project tools tools/export_reference.py` \
             to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-pretrained") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_params.py`."
        }
        Some("flyvis-reference") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        }
        _ => "Run ci/fetch-fixtures.sh.",
    }
}

```

with:

```rust

/// The real, published flyvis connectome spec, under `data/`.
pub(crate) const REAL_SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";

/// `rel`'s path under the workspace's `data/`, as every notice prints it.
fn data_path(rel: &str) -> String {
    format!("{}/../../data/{rel}", env!("CARGO_MANIFEST_DIR"))
}

```

In `crates/flyvis/src/test_support.rs`, replace:

```rust
fn missing(path: &str, rel: &str) -> String {
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    format!("{path} not found. {}", how_to_get(rel))
}

```

with:

```rust
fn missing(path: &str, rel: &str) -> String {
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    format!("{path} not found. {}", crate::parity::how_to_get(rel))
}

```

- [ ] **Step 9: The gate fixture, and the wrappers.** `gate_support.rs`
  shrinks to the fixture (R21: only a missing file is a skip):

Replace the whole of `crates/flyvis/src/gate_support.rs` with:

```rust
//! The gates' fixture: read once through `test_support`, in the order the
//! gates have always read it, then built by `parity::Fixtures::from_parts`.
//!
//! What the gates compute lives in `crate::parity`, which `semblance
//! validate` runs too (spec
//! docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md,
//! section 2).

use std::sync::OnceLock;

use crate::parity::Fixtures;
use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text, REAL_SPEC};

const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

/// The fixture's files, read but not parsed.
struct Parts {
    ensemble: String,
    manifest: String,
    settling: Vec<(&'static str, Vec<u8>)>,
    spec: String,
}

static FIXTURE: OnceLock<Result<Fixtures, String>> = OnceLock::new();

/// Everything the gates read, built once. Every file is loaded (or found
/// missing) before anything is built from it. A missing file makes this
/// `None`, and every test that asks prints the notice.
pub(crate) fn fixture() -> Option<&'static Fixtures> {
    or_skip(FIXTURE.get_or_init(|| build(read())).as_ref().map_err(String::clone))
}

/// Reads ensemble.csv, manifest.json, the four settling files and the spec,
/// in that order. A missing file is its notice, unprinted.
fn read() -> Result<Parts, String> {
    let ensemble = read_fixture_text("flyvis-pretrained/ensemble.csv")?;
    let manifest = read_fixture_text("flyvis-reference/manifest.json")?;
    let mut settling = Vec::new();
    for name in SETTLING_FILES {
        settling.push((name, read_fixture_bytes(&format!("flyvis-reference/{name}"))?));
    }
    let spec = read_fixture_text(REAL_SPEC)?;
    Ok(Parts { ensemble, manifest, settling, spec })
}

/// The `Err` is only ever a missing file's notice, so a gate skips only
/// when a file is missing (spec R21). A file that is present but malformed
/// panics here: the `OnceLock` stays empty, and every gate that asks fails
/// with the error, with or without `SEMBLANCE_REQUIRE_FIXTURES`.
fn build(reads: Result<Parts, String>) -> Result<Fixtures, String> {
    let p = reads?;
    match Fixtures::from_parts(&p.spec, &p.ensemble, &p.manifest, p.settling) {
        Ok(f) => Ok(f),
        Err(e) => panic!("{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_a_skip_notice() {
        assert_eq!(build(Err("x not found. y".into())), Err("x not found. y".to_string()));
    }

    #[test]
    #[should_panic(expected = "flyvis-pretrained/ensemble.csv: ensemble.csv line 1: no header line")]
    fn a_malformed_fixture_fails_rather_than_skips() {
        let empty = Parts { ensemble: String::new(), manifest: String::new(), settling: Vec::new(), spec: String::new() };
        let _ = build(Ok(empty));
    }
}
```

The settling wrappers:

In `crates/flyvis/src/settling_checks.rs`, replace everything from the line
`//! The fixture, settling itself and the comparison live in `gate_support.rs`,` up to, but not including, the line `macro_rules! settling_runs {`, with:

```rust
//! What the gates compute -- settling, the comparison, the bounds -- lives
//! in `crate::parity`, which `semblance validate` runs too. These tests are
//! thin wrappers over it, and keep their names and their printed strings.

use crate::gate_support::fixture;
use crate::json;
use crate::parity::{self, cell_type, settle, settling_bounds, Counts, Precision, CONTROL_MISS, F64_BOUND};
use crate::pretrained::CELL_TYPES;
use crate::reference::Dtype;
use crate::test_support::load_reference_fixture;

// The registered gaps, `parity::SETTLING_F32_GAPS`: (model, dt, settled,
// settling_centre). `pre_registered_bounds_match_provenance` pins them to
// the file.
use crate::parity::SETTLING_F32_GAPS as F32_GAPS;

/// The test's name says which precision it runs; the registry must agree.
fn assert_registered<R: Precision>(run: u32) {
    assert_eq!(R::DTYPE, parity::RUNS[run as usize].3, "run {run}'s registered precision");
}

fn exact<R: Precision>(run: u32) {
    assert_registered::<R>(run);
    let Some(f) = fixture() else { return };
    let info = &f.manifest.runs[run as usize];
    let s = settle(f, run, &info.model, Counts::Torch).unwrap_or_else(|e| panic!("{e}"));
    let (settled, centre) = (s.settled, s.settling_centre);
    let n_centre = f.manifest.centre_nodes.len();
    eprintln!(
        "exact run {run} ({}, dt {}, {}): settled {} of {} differ; settling centre {} of {} differ",
        info.model, info.dt, R::SUFFIX, settled.differ, s.settled_len, centre.differ, s.centre_len
    );
    if settled.differ + centre.differ > 0 {
        let first = s
            .first_centre_difference
            .map(|i| format!("settle step {} ({})", i / n_centre, f.manifest.centre_nodes[i % n_centre].cell_type));
        panic!(
            "run {run} is not bit-equal to flyvis: {} settled and {} settling-centre values differ; \
             first difference at {}; largest settled error {:e} at node {} ({})",
            settled.differ,
            centre.differ,
            first.unwrap_or_else(|| "the settled state only".into()),
            settled.max,
            settled.at,
            cell_type(&f.net, settled.at)
        );
    }
}

fn libm<R: Precision>(run: u32) {
    assert_registered::<R>(run);
    let Some(f) = fixture() else { return };
    let info = &f.manifest.runs[run as usize];
    let (bound_settled, bound_centre) =
        settling_bounds(&info.model, &info.dt, info.precision, Counts::Libm).unwrap_or_else(|e| panic!("{e}"));
    let (bound_settled, bound_centre) = (bound_settled.value(), bound_centre.value());
    let s = settle(f, run, &info.model, Counts::Libm).unwrap_or_else(|e| panic!("{e}"));
    let (settled, centre) = (s.settled, s.settling_centre);
    let n_centre = f.manifest.centre_nodes.len();
    let worst: Vec<String> =
        s.worst_types(3).iter().map(|&t| format!("{} {:.2e}", f.net.cell_types()[t], s.per_type_max[t])).collect();

    eprintln!(
        "libm run {run} ({}, dt {}, {}): settled max {:.3e} at node {} ({}), {:.3e} of bound; \
         settling centre max {:.3e} at step {} ({}), {:.3e} of bound; worst types {}; settled bit-equal {} of {}",
        info.model, info.dt, R::SUFFIX,
        settled.max, settled.at, cell_type(&f.net, settled.at), settled.max / bound_settled,
        centre.max, centre.at / n_centre, f.manifest.centre_nodes[centre.at % n_centre].cell_type, centre.max / bound_centre,
        worst.join(", "), s.settled_len - settled.differ, s.settled_len
    );
    assert!(
        settled.max <= bound_settled && centre.max <= bound_centre,
        "run {run}: Rust's exp exceeds the pre-registered bound: settled {:e} (bound {:e}), \
         settling centre {:e} (bound {:e})",
        settled.max, bound_settled, centre.max, bound_centre
    );
}

```

and the two controls, which now read `parity`'s results:

In `crates/flyvis/src/settling_checks.rs`, replace:

```rust
fn the_gate_tells_models_apart() {
    let Some(f) = fixture() else { return };
    let s = settle::<f64>(f, 3, "049", Counts::Torch);
    let d = diff(&s.rust, &s.flyvis);
    assert!(d.max > 1e-3, "model 049 against model 000's reference: max {:e}", d.max);
}

```

with:

```rust
fn the_gate_tells_models_apart() {
    let Some(f) = fixture() else { return };
    let d = settle(f, 3, "049", Counts::Torch).unwrap_or_else(|e| panic!("{e}")).settled;
    assert!(d.max > CONTROL_MISS, "model 049 against model 000's reference: max {:e}", d.max);
}

```

In `crates/flyvis/src/settling_checks.rs`, replace:

```rust
fn the_gate_resolves_integer_counts() {
    let Some(f) = fixture() else { return };
    let s = settle::<f64>(f, 3, "000", Counts::IntegerNSyn);
    let d = diff(&s.rust, &s.flyvis);
    assert!(d.max > F64_BOUND, "integer counts: settled max {:e}", d.max);
}
```

with:

```rust
fn the_gate_resolves_integer_counts() {
    let Some(f) = fixture() else { return };
    let d = settle(f, 3, "000", Counts::IntegerNSyn).unwrap_or_else(|e| panic!("{e}")).settled;
    assert!(d.max > F64_BOUND, "integer counts: settled max {:e}", d.max);
}
```

The trace wrappers:

In `crates/flyvis/src/trace_checks.rs`, replace everything from the line
`use std::sync::OnceLock;` up to, but not including, the line `macro_rules! trace_runs {`, with:

```rust
use std::sync::OnceLock;

use crate::gate_support::fixture;
use crate::json;
use crate::parity::{
    self, cell_type, file, play, trace_bounds, Counts, Fixtures, Precision, SequenceResult, Start, TraceInputs,
    CONTROL_MISS, HEXALS, SNAPSHOTS,
};
use crate::reference::{parse_sequences, BlockKey, SequenceInfo};
use crate::test_support::{load_reference_fixture, or_skip, read_fixture_bytes, read_fixture_text};

// The registered gaps, `parity::TRACE_F32_GAPS`: (model, dt, centre,
// snapshots). `trace_bounds_match_provenance` pins them to the file.
use crate::parity::TRACE_F32_GAPS;

const TRACE_FILES: [&str; 5] =
    ["input.f64.bin", "centre.f32.bin", "centre.f64.bin", "snapshots.f32.bin", "snapshots.f64.bin"];

/// One trace file's bytes, loaded on first use. Prints nothing.
fn trace_file(name: &str) -> Result<&'static [u8], String> {
    static FILES: [OnceLock<Result<Vec<u8>, String>>; 5] = [const { OnceLock::new() }; 5];
    let i = TRACE_FILES.iter().position(|&n| n == name).expect("one of TRACE_FILES");
    FILES[i]
        .get_or_init(|| read_fixture_bytes(&format!("flyvis-reference/{name}")))
        .as_ref()
        .map(Vec::as_slice)
        .map_err(String::clone)
}

/// The manifest's twelve sequences, parsed once. Prints nothing.
fn sequences() -> Result<&'static [SequenceInfo], String> {
    static SEQUENCES: OnceLock<Result<Vec<SequenceInfo>, String>> = OnceLock::new();
    SEQUENCES
        .get_or_init(|| {
            let text = read_fixture_text("flyvis-reference/manifest.json")?;
            Ok(parse_sequences(&text).expect("manifest.json's sequences must parse"))
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(String::clone)
}

/// Everything one precision's trace run reads.
struct Traces {
    f: &'static Fixtures,
    inputs: TraceInputs<'static>,
}

/// Read in the gates' order: sequences, input, centre, snapshots.
fn load_traces<R: Precision>(f: &'static Fixtures) -> Result<Traces, String> {
    let sequences = sequences()?;
    let input = trace_file("input.f64.bin")?;
    let centre = trace_file(&format!("centre.{}.bin", R::SUFFIX))?;
    let snapshots = trace_file(&format!("snapshots.{}.bin", R::SUFFIX))?;
    Ok(Traces { f, inputs: TraceInputs { precision: R::DTYPE, sequences, input, centre, snapshots } })
}

/// The traces for precision `R`, or `None` after printing a skip notice.
fn traces<R: Precision>() -> Option<Traces> {
    or_skip(load_traces::<R>(fixture()?))
}

/// The test's name says which precision it runs; the registry must agree.
fn assert_registered<R: Precision>(run: u32) {
    assert_eq!(R::DTYPE, parity::RUNS[run as usize].3, "run {run}'s registered precision");
}

fn all_sequences(t: &Traces) -> Vec<u32> {
    t.inputs.sequences.iter().map(|q| q.index).collect()
}

fn exact<R: Precision>(run: u32) {
    assert_registered::<R>(run);
    let Some(t) = traces::<R>() else { return };
    let f = t.f;
    let info = &f.manifest.runs[run as usize];
    let played = play(f, &t.inputs, run, Counts::Torch, Start::Rust, &all_sequences(&t), false)
        .unwrap_or_else(|e| panic!("{e}"));
    let (n, n_centre) = (f.net.num_nodes(), f.manifest.centre_nodes.len());
    let mut failures = Vec::new();
    for p in &played {
        if p.centre.differ + p.snapshots.differ == 0 {
            continue;
        }
        let first = p.first.map_or_else(
            || "the snapshots only".to_string(),
            |i| format!("frame {} ({})", i / n_centre, f.manifest.centre_nodes[i % n_centre].cell_type),
        );
        failures.push(format!(
            "sequence {}: {} of {} centre and {} of {} snapshot values differ; first difference at {first}; \
             largest centre error {:e}; largest snapshot error {:e} at node {} ({})",
            p.sequence,
            p.centre.differ,
            p.n_frames * n_centre,
            p.snapshots.differ,
            SNAPSHOTS * n,
            p.centre.max,
            p.snapshots.max,
            p.snapshots.at % n,
            cell_type(&f.net, p.snapshots.at % n)
        ));
    }
    eprintln!(
        "trace exact run {run} ({}, dt {}, {}): {} of {} sequences bit-equal, centre and snapshots",
        info.model, info.dt, R::SUFFIX, played.len() - failures.len(), played.len()
    );
    assert!(failures.is_empty(), "run {run} is not bit-equal to flyvis under stimulus:\n{}", failures.join("\n"));
}

fn libm<R: Precision>(run: u32) {
    assert_registered::<R>(run);
    let Some(t) = traces::<R>() else { return };
    let f = t.f;
    let info = &f.manifest.runs[run as usize];
    let (bound_centre, bound_snapshots) =
        trace_bounds(&info.model, &info.dt, info.precision, Counts::Libm).unwrap_or_else(|e| panic!("{e}"));
    let (bound_centre, bound_snapshots) = (bound_centre.value(), bound_snapshots.value());
    let played = play(f, &t.inputs, run, Counts::Libm, Start::Rust, &all_sequences(&t), false)
        .unwrap_or_else(|e| panic!("{e}"));
    let (n, n_centre) = (f.net.num_nodes(), f.manifest.centre_nodes.len());

    let ratio = |p: &SequenceResult| (p.centre.max / bound_centre).max(p.snapshots.max / bound_snapshots);
    let worst = played.iter().max_by(|a, b| ratio(a).total_cmp(&ratio(b))).expect("twelve sequences");
    let c = played.iter().max_by(|a, b| a.centre.max.total_cmp(&b.centre.max)).expect("twelve sequences");
    let s = played.iter().max_by(|a, b| a.snapshots.max.total_cmp(&b.snapshots.max)).expect("twelve sequences");
    let c_node = &f.manifest.centre_nodes[c.centre.at % n_centre];
    let s_node = s.snapshots.at % n;
    let (equal, total) = played.iter().fold((0, 0), |(equal, total), p| {
        let size = p.n_frames * n_centre + SNAPSHOTS * n;
        (equal + size - p.centre.differ - p.snapshots.differ, total + size)
    });
    eprintln!(
        "trace libm run {run} ({}, dt {}, {}): worst sequence {}, {:.3e} of bound; \
         centre max {:.3e} in sequence {} at frame {} node {} ({}), {:.3e} of bound; \
         snapshots max {:.3e} in sequence {} at frame {} node {} ({}), {:.3e} of bound; bit-equal {equal} of {total}",
        info.model, info.dt, R::SUFFIX, worst.sequence, ratio(worst),
        c.centre.max, c.sequence, c.centre.at / n_centre, c_node.node, c_node.cell_type, c.centre.max / bound_centre,
        s.snapshots.max, s.sequence, s.snapshot_frames[s.snapshots.at / n], s_node, cell_type(&f.net, s_node),
        s.snapshots.max / bound_snapshots,
    );
    let over: Vec<String> = played
        .iter()
        .filter(|p| p.centre.max > bound_centre || p.snapshots.max > bound_snapshots)
        .map(|p| format!("sequence {}: centre {:e}, snapshots {:e}", p.sequence, p.centre.max, p.snapshots.max))
        .collect();
    assert!(
        over.is_empty(),
        "run {run}: Rust's exp exceeds the spec's bounds (centre {bound_centre:e}, snapshots {bound_snapshots:e}):\n{}",
        over.join("\n")
    );
}

```

and the tests below the macro:

In `crates/flyvis/src/trace_checks.rs`, replace:

```rust
    let n = f.net.num_nodes();
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let centre_file = file(f, &format!("centre.{}.bin", R::SUFFIX));
    let snapshots_file = file(f, &format!("snapshots.{}.bin", R::SUFFIX));
    let mut checked = 0;
    for info in f.manifest.runs.iter().filter(|r| r.precision == R::DTYPE) {
        for q in t.sequences {
            let layout = q.frames(&info.dt).unwrap_or_else(|| panic!("sequence {} at dt {}", q.index, info.dt));
            let key = BlockKey { run: Some(info.index), dt: &info.dt, sequence: Some(q.index) };
            let c = R::read(centre_file, t.centre, &key).unwrap_or_else(|e| panic!("{e}"));
            let s = R::read(snapshots_file, t.snapshots, &key).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(c.shape, [layout.n_frames, centre.len()], "run {} sequence {}", info.index, q.index);
            assert_eq!(s.shape, [SNAPSHOTS, n], "run {} sequence {}", info.index, q.index);
```

with:

```rust
    let n = f.net.num_nodes();
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let centre_file = file(f, &format!("centre.{}.bin", R::SUFFIX)).unwrap_or_else(|e| panic!("{e}"));
    let snapshots_file = file(f, &format!("snapshots.{}.bin", R::SUFFIX)).unwrap_or_else(|e| panic!("{e}"));
    let mut checked = 0;
    for info in f.manifest.runs.iter().filter(|r| r.precision == R::DTYPE) {
        for q in t.inputs.sequences {
            let layout = q.frames(&info.dt).unwrap_or_else(|| panic!("sequence {} at dt {}", q.index, info.dt));
            let key = BlockKey { run: Some(info.index), dt: &info.dt, sequence: Some(q.index) };
            let c = R::read(centre_file, t.inputs.centre, &key).unwrap_or_else(|e| panic!("{e}"));
            let s = R::read(snapshots_file, t.inputs.snapshots, &key).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(c.shape, [layout.n_frames, centre.len()], "run {} sequence {}", info.index, q.index);
            assert_eq!(s.shape, [SNAPSHOTS, n], "run {} sequence {}", info.index, q.index);
```

In `crates/flyvis/src/trace_checks.rs`, replace:

```rust
        }
    }
    assert_eq!(checked, 4 * t.sequences.len(), "four {} runs, every sequence", R::SUFFIX);
}

```

with:

```rust
        }
    }
    assert_eq!(checked, 4 * t.inputs.sequences.len(), "four {} runs, every sequence", R::SUFFIX);
}

```

In `crates/flyvis/src/trace_checks.rs`, replace:

```rust
fn the_trace_gate_sees_a_wrong_mapping() {
    let Some(t) = traces::<f64>() else { return };
    let p = &play::<f64>(&t, 3, Counts::Torch, Start::Rust, &[8], true)[0];
    eprintln!("mirrored mapping, run 3, sequence 8: snapshots max {:e}, centre max {:e}", p.snapshots.max, p.centre.max);
    assert!(p.snapshots.max > 1e-3, "a mirrored mapping must miss by far more than any bound: snapshots max {:e}", p.snapshots.max);
}

```

with:

```rust
fn the_trace_gate_sees_a_wrong_mapping() {
    let Some(t) = traces::<f64>() else { return };
    let p = &play(t.f, &t.inputs, 3, Counts::Torch, Start::Rust, &[8], true).unwrap_or_else(|e| panic!("{e}"))[0];
    eprintln!("mirrored mapping, run 3, sequence 8: snapshots max {:e}, centre max {:e}", p.snapshots.max, p.centre.max);
    assert!(p.snapshots.max > CONTROL_MISS, "a mirrored mapping must miss by far more than any bound: snapshots max {:e}", p.snapshots.max);
}

```

In `crates/flyvis/src/trace_checks.rs`, replace:

```rust
fn a_run_from_flyviss_settled_state_is_bit_equal_too() {
    let Some(t) = traces::<f64>() else { return };
    let p = &play::<f64>(&t, 3, Counts::Torch, Start::Flyvis, &[8], false)[0];
    assert_eq!((p.centre.differ, p.snapshots.differ), (0, 0), "run 3, sequence 8, from settled.f64.bin");
}
```

with:

```rust
fn a_run_from_flyviss_settled_state_is_bit_equal_too() {
    let Some(t) = traces::<f64>() else { return };
    let p = &play(t.f, &t.inputs, 3, Counts::Torch, Start::Flyvis, &[8], false).unwrap_or_else(|e| panic!("{e}"))[0];
    assert_eq!((p.centre.differ, p.snapshots.differ), (0, 0), "run 3, sequence 8, from settled.f64.bin");
}
```

- [ ] **Step 10: Run the new unit tests, then compare the prints.**

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- parity:: gate_support::`
  Expected: 17 passed (`parity::tests` 2, `parity::bounds::tests` 3,
  `parity::inputs::tests` 4, `parity::run::tests` 6, `gate_support::tests` 2).

  Run:
```bash
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- --nocapture settling_checks:: trace_checks:: \
  2>&1 >/dev/null | grep -E '^(exact run|libm run|trace exact|trace libm|mirrored|flashes at|moving edges at)' | sort > target/parity-after.txt
diff target/parity-before.txt target/parity-after.txt && echo IDENTICAL
```
  Expected: `IDENTICAL`. Any difference is a moved C-string (C1, C3, C9,
  C11, C14): restore the string. Then the whole lib:
  `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib` passes
  (277 on this worktree: the base's 261 and the 16 net new).

- [ ] **Step 11: The shadow acceptance suites, on this commit alone.**

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --test acceptance_settling --test acceptance_traces --test acceptance_minors --test acceptance_mapping --test acceptance_sequences`
  Expected: 13, 24, 9, 8 and 9 passed, 0 failed (measured: traces 104 s,
  minors 117 s on this commit, before task 1's cuts). After task 1 merges
  into the wave, the head re-runs them.

- [ ] **Step 12: Clippy, MSRV and the gate.**

  Run: `cargo clippy --workspace --all-targets -- -D warnings`,
  `rustup run 1.85 cargo build --workspace --all-targets`, then the gate.
  Expected: clean, no warning from `settling_checks.rs` or
  `trace_checks.rs` (C17); 0 failed (445 passed on this worktree alone).

- [ ] **Step 13: Commit.**

```bash
git add crates/flyvis/src/parity/mod.rs \
  crates/flyvis/src/parity/bounds.rs \
  crates/flyvis/src/parity/inputs.rs \
  crates/flyvis/src/parity/run.rs \
  crates/flyvis/src/lib.rs \
  crates/flyvis/src/gate_support.rs \
  crates/flyvis/src/settling_checks.rs \
  crates/flyvis/src/trace_checks.rs \
  crates/flyvis/src/test_support.rs \
  crates/flyvis/Cargo.toml
git commit -m "Move the gate machinery into semblance_flyvis::parity" \
  -m "Temper-Task: parity-validate/extraction"
```
  (Add `Cargo.lock` to the `git add` only if step 2 showed it changed.)

### Task 4: `parity::validate`, the fixture checks and the libm fingerprint (wave 2, id `validate`)

**Wave:** 2

Spec §1 "Validating", "Coverage, exactly", "Libm fingerprint", R11, R14,
R18; §2's provenance tests. Depends on tasks 2 and 3.

**Files:**
- Create: `crates/flyvis/src/parity/fingerprint.rs`
- Create: `crates/flyvis/src/parity/checks.rs`
- Create: `crates/flyvis/src/parity/validate.rs`
- Modify: `crates/flyvis/src/parity/mod.rs` (declare and re-export them)
- Modify: `crates/flyvis/src/settling_checks.rs`, `crates/flyvis/src/trace_checks.rs` (the two provenance tests use `Provenance::parse` and `check_*_gaps`)
- Modify: `crates/flyvis/src/test_support.rs` (`data_dir`, `require_present`)

**Interfaces:**
- Consumes: task 2's `Provenance`, `GapEntry`, `ReferenceError`; task 3's
  `parity` items (`Fixtures`, `TraceData`, `run`, `settle`, `play`,
  `settling_bounds`, `trace_bounds`, `checked_run`, `centre_nodes`, `file`,
  `Precision`, `read_text`, `PROVENANCE`, `SETTLING_FILES`).
- Produces (in `semblance_flyvis::parity`):
  - `pub fn check_settling_gaps(p: &Provenance) -> Result<(), ParityError>`, `pub fn check_trace_gaps(p: &Provenance) -> Result<(), ParityError>` (bit for bit; failure text `{model} {dt} {quantity}: PROVENANCE {got:e}, registered {want:e}`, C7)
  - `pub struct LibmFingerprint { pub f32_differ: usize, pub f64_differ: usize, pub f32_fnv: u64, pub f64_fnv: u64 }` (`Copy, Eq`), `Display` as `f32:{n}:{016x},f64:{n}:{016x}`; `pub fn libm_fingerprint() -> LibmFingerprint`
  - `pub struct Options { pub threads: usize, pub runs: Vec<u32> }` (`Default`: 1 thread, runs 0..8)
  - `pub struct Job { pub run, pub model, pub dt, pub precision, pub counts, pub settling_bounds: (Bound, Bound), pub trace_bounds: (Bound, Bound), pub outcome: Result<RunResult, ParityError> }` with `passed(&self) -> bool`
  - `pub struct Controls { pub wrong_model: Result<Diff, ParityError>, pub integer_counts: Result<Diff, ParityError>, pub mirrored: Result<SequenceResult, ParityError>, pub from_flyvis: Result<SequenceResult, ParityError> }` with `passed(&self) -> bool`
  - `pub struct Coverage { pub run: u32, pub movement: Option<Vec<f64>> }`
  - `pub struct Report { pub provenance: Provenance, pub runs: Vec<u32>, pub cell_types: Vec<String>, pub node_cell_type: Vec<u32>, pub centre_nodes: Vec<u32>, pub jobs: Vec<Job>, pub controls: Controls, pub coverage: Vec<Coverage>, pub libm: LibmFingerprint, pub threads: usize }` with `cell_type(&self, node: usize) -> &str`, `passed(&self) -> bool`, `complete(&self) -> bool`
  - `pub fn validate(data: &Path, opts: &Options) -> Result<Report, ParityError>`
  - `test_support::data_dir() -> PathBuf`, `test_support::require_present(rels: &[&str]) -> Result<(), String>`

- [ ] **Step 1: The test helpers, and the provenance tests as the spec
  wants them (failing first).** In `test_support.rs`:

In `crates/flyvis/src/test_support.rs`, replace:

```rust
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    format!("{path} not found. {}", crate::parity::how_to_get(rel))
}

```

with:

```rust
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    format!("{path} not found. {}", crate::parity::how_to_get(rel))
}

/// The workspace's `data/` directory, the same one `data_path` names.
pub(crate) fn data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}/../../data", env!("CARGO_MANIFEST_DIR")))
}

/// Whether every file in `rels` is present under `data/`, by metadata only:
/// `Err` of the first absent one's notice, unprinted. Under
/// `SEMBLANCE_REQUIRE_FIXTURES=1` an absent file panics instead.
pub(crate) fn require_present(rels: &[&str]) -> Result<(), String> {
    for rel in rels {
        let path = data_path(rel);
        if std::fs::metadata(&path).is_err() {
            return Err(missing(&path, rel));
        }
    }
    Ok(())
}

```

In `settling_checks.rs`:

In `crates/flyvis/src/settling_checks.rs`, replace:

```rust

use crate::gate_support::fixture;
use crate::json;
use crate::parity::{self, cell_type, settle, settling_bounds, Counts, Precision, CONTROL_MISS, F64_BOUND};
use crate::pretrained::CELL_TYPES;
use crate::reference::Dtype;
use crate::test_support::load_reference_fixture;

// The registered gaps, `parity::SETTLING_F32_GAPS`: (model, dt, settled,
// settling_centre). `pre_registered_bounds_match_provenance` pins them to
// the file.
use crate::parity::SETTLING_F32_GAPS as F32_GAPS;

/// The test's name says which precision it runs; the registry must agree.
```

with:

```rust

use crate::gate_support::fixture;
use crate::parity::{
    self, cell_type, check_settling_gaps, settle, settling_bounds, Counts, Precision, CONTROL_MISS, F64_BOUND,
};
use crate::pretrained::CELL_TYPES;
use crate::reference::{Dtype, Provenance};
use crate::test_support::load_reference_fixture;

/// The test's name says which precision it runs; the registry must agree.
```

In `crates/flyvis/src/settling_checks.rs`, replace:

```rust
fn pre_registered_bounds_match_provenance() {
    let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
    let root = json::parse(&text).expect("PROVENANCE.json must parse");
    let gaps = root.get("f32_f64_gap").expect("PROVENANCE.json has f32_f64_gap");
    for &(model, dt, settled, centre) in &F32_GAPS {
        let key = format!("{model} {dt}");
        let g = gaps.get(&key).unwrap_or_else(|| panic!("no gap for {key}"));
        for (name, want) in [("settled", settled), ("settling_centre", centre)] {
            let got = g.get(name).and_then(|v| v.as_f64()).unwrap_or_else(|| panic!("{key} {name}"));
            assert_eq!(got.to_bits(), want.to_bits(), "{key} {name}: PROVENANCE {got:e}, gate {want:e}");
        }
    }
}

```

with:

```rust
fn pre_registered_bounds_match_provenance() {
    let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
    let p = Provenance::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    check_settling_gaps(&p).unwrap_or_else(|e| panic!("{e}"));
}

```

In `trace_checks.rs`:

In `crates/flyvis/src/trace_checks.rs`, replace:

```rust
use crate::json;
use crate::parity::{
    self, cell_type, file, play, trace_bounds, Counts, Fixtures, Precision, SequenceResult, Start, TraceInputs,
    CONTROL_MISS, HEXALS, SNAPSHOTS,
};
use crate::reference::{parse_sequences, BlockKey, SequenceInfo};
use crate::test_support::{load_reference_fixture, or_skip, read_fixture_bytes, read_fixture_text};

// The registered gaps, `parity::TRACE_F32_GAPS`: (model, dt, centre,
// snapshots). `trace_bounds_match_provenance` pins them to the file.
use crate::parity::TRACE_F32_GAPS;

const TRACE_FILES: [&str; 5] =
```

with:

```rust
use crate::json;
use crate::parity::{
    self, cell_type, check_trace_gaps, file, play, trace_bounds, Counts, Fixtures, Precision, SequenceResult, Start,
    TraceInputs, CONTROL_MISS, HEXALS, SNAPSHOTS,
};
use crate::reference::{parse_sequences, BlockKey, Provenance, SequenceInfo};
use crate::test_support::{load_reference_fixture, or_skip, read_fixture_bytes, read_fixture_text};

const TRACE_FILES: [&str; 5] =
```

In `crates/flyvis/src/trace_checks.rs`, replace:

```rust
fn trace_bounds_match_provenance() {
    let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
    let root = json::parse(&text).expect("PROVENANCE.json must parse");
    let gaps = root.get("f32_f64_gap").expect("PROVENANCE.json has f32_f64_gap");
    for &(model, dt, centre, snapshots) in &TRACE_F32_GAPS {
        let key = format!("{model} {dt}");
        let g = gaps.get(&key).unwrap_or_else(|| panic!("no gap for {key}"));
        for (name, want) in [("centre", centre), ("snapshots", snapshots)] {
            let got = g.get(name).and_then(|v| v.as_f64()).unwrap_or_else(|| panic!("{key} {name}"));
            assert_eq!(got.to_bits(), want.to_bits(), "{key} {name}: PROVENANCE {got:e}, gate {want:e}");
        }
    }
}

```

with:

```rust
fn trace_bounds_match_provenance() {
    let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
    let p = Provenance::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    check_trace_gaps(&p).unwrap_or_else(|e| panic!("{e}"));
}

```

  The doc comments above the two tests stay as they are (C16).

- [ ] **Step 2: Run them to see them fail.**

  Run: `cargo test -p semblance-flyvis --lib provenance`
  Expected: does not compile: `unresolved imports crate::parity::check_settling_gaps`,
  `crate::parity::check_trace_gaps`.

- [ ] **Step 3: The libm fingerprint.** Computed here, not through the gate
  code, and tested against it (spec §1, "Libm fingerprint"):

Create `crates/flyvis/src/parity/fingerprint.rs` with:

```rust
//! Which `exp` this host's libm computes, as a fingerprint.
//!
//! The libm gates use Rust's `f32::exp` and `f64::exp`, which call the
//! platform's libm, so their maxima are facts about one host. The record
//! compares its libm section only on a host whose fingerprint is the
//! record's (spec R9). This is an identifier, not a security hash.

use std::hint::black_box;

use crate::pretrained::SYN_COUNT;
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};

/// This host's `exp` of every synapse count, summarised: how many of the
/// 2,355 groups differ in their bits from torch's table, and a 64-bit FNV-1a
/// over the little-endian bits of all 2,355 values, in group order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibmFingerprint {
    pub f32_differ: usize,
    pub f64_differ: usize,
    pub f32_fnv: u64,
    pub f64_fnv: u64,
}

impl std::fmt::Display for LibmFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "f32:{}:{:016x},f64:{}:{:016x}", self.f32_differ, self.f32_fnv, self.f64_differ, self.f64_fnv)
    }
}

/// 64-bit FNV-1a.
fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// This host's `f32::exp` of every synapse count, computed here and not
/// through the gate code. `black_box` keeps the compiler from folding the
/// call at build time, so the value is the running host's libm.
fn exp_f32() -> Vec<f32> {
    SYN_COUNT.iter().map(|&c| f32::exp(black_box(c))).collect()
}

/// As [`exp_f32`], in f64.
fn exp_f64() -> Vec<f64> {
    SYN_COUNT.iter().map(|&c| f64::exp(black_box(c as f64))).collect()
}

/// The fingerprint of this host's libm.
pub fn libm_fingerprint() -> LibmFingerprint {
    let (a, b) = (exp_f32(), exp_f64());
    LibmFingerprint {
        f32_differ: a.iter().zip(&SYN_COUNT_TORCH_F32).filter(|(x, y)| x.to_bits() != y.to_bits()).count(),
        f64_differ: b.iter().zip(&SYN_COUNT_TORCH_F64).filter(|(x, y)| x.to_bits() != y.to_bits()).count(),
        f32_fnv: fnv1a(a.iter().flat_map(|v| v.to_le_bytes())),
        f64_fnv: fnv1a(b.iter().flat_map(|v| v.to_le_bytes())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::Precision;

    #[test]
    fn fnv1a_matches_its_published_test_vectors() {
        assert_eq!(fnv1a(*b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(*b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a(*b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn the_fingerprint_prints_as_registered() {
        let f = LibmFingerprint { f32_differ: 104, f64_differ: 108, f32_fnv: 0xab, f64_fnv: 0x1234_5678_9abc_def0 };
        assert_eq!(f.to_string(), "f32:104:00000000000000ab,f64:108:123456789abcdef0");
    }

    /// If the gate's count formula changes, this fails: the fingerprint does
    /// not silently follow it, so the libm sections cannot switch themselves
    /// off.
    #[test]
    fn the_libm_fingerprint_is_independent_of_the_gate() {
        let (a, b) = (exp_f32(), exp_f64());
        assert_eq!(a.len(), SYN_COUNT.len());
        for g in 0..SYN_COUNT.len() {
            assert_eq!(<f32 as Precision>::libm_count(g).to_bits(), a[g].to_bits(), "group {g}, f32");
            assert_eq!(<f64 as Precision>::libm_count(g).to_bits(), b[g].to_bits(), "group {g}, f64");
        }
    }

    /// Each count is at most 200, as `syn_count_torch_checks` bounds it, and
    /// equals an independent recount. No lower bound: a host whose libm
    /// agrees with torch everywhere is a legitimate host.
    #[test]
    fn the_libm_fingerprint_counts_differences_from_torch() {
        let f = libm_fingerprint();
        let mut f32_differ = 0;
        let mut f64_differ = 0;
        for g in 0..SYN_COUNT.len() {
            if SYN_COUNT[g].exp().to_bits() != SYN_COUNT_TORCH_F32[g].to_bits() {
                f32_differ += 1;
            }
            if (SYN_COUNT[g] as f64).exp().to_bits() != SYN_COUNT_TORCH_F64[g].to_bits() {
                f64_differ += 1;
            }
        }
        assert_eq!((f.f32_differ, f.f64_differ), (f32_differ, f64_differ));
        assert!(f.f32_differ <= 200 && f.f64_differ <= 200, "{f}");
        eprintln!("libm fingerprint on this host: {f}");
    }
}
```

- [ ] **Step 4: The fixture checks.** Run before any arithmetic; every
  failure is `Mismatch` with the catalogue's text.

Create `crates/flyvis/src/parity/checks.rs` with:

```rust
//! The fixture checks `validate` runs before any arithmetic: the files agree
//! with each other, with PROVENANCE.json, and with what the specs
//! registered. Every failure is a fixture error, never a recomputed bound.

use semblance_connectome::TypedNetwork;

use super::inputs::SETTLING_FILES;
use super::run::{centre_nodes, checked_run, file, HEXALS, SNAPSHOTS};
use super::{Fixtures, ParityError, TraceData, RUNS, SETTLING_F32_GAPS, TRACE_F32_GAPS};
use crate::pretrained::CELL_TYPES;
use crate::reference::{BlockKey, CentreNode, Dtype, Provenance};

fn mismatch(m: String) -> ParityError {
    ParityError::Mismatch(m)
}

fn check_gaps(p: &Provenance, table: &[(&str, &str, f64, f64); 4], trace: bool) -> Result<(), ParityError> {
    for &(model, dt, a, b) in table {
        let key = format!("{model} {dt}");
        let g = p
            .gaps
            .iter()
            .find(|g| g.key == key)
            .ok_or_else(|| mismatch(format!("{key}: PROVENANCE f32_f64_gap has no such entry")))?;
        let pairs = if trace {
            [("centre", g.centre, a), ("snapshots", g.snapshots, b)]
        } else {
            [("settled", g.settled, a), ("settling_centre", g.settling_centre, b)]
        };
        for (quantity, got, want) in pairs {
            if got.to_bits() != want.to_bits() {
                return Err(mismatch(format!("{model} {dt} {quantity}: PROVENANCE {got:e}, registered {want:e}")));
            }
        }
    }
    Ok(())
}

/// The settling gaps in PROVENANCE.json are the registered ones, bit for bit.
pub fn check_settling_gaps(p: &Provenance) -> Result<(), ParityError> {
    check_gaps(p, &SETTLING_F32_GAPS, false)
}

/// The trace gaps in PROVENANCE.json are the registered ones, bit for bit.
pub fn check_trace_gaps(p: &Provenance) -> Result<(), ParityError> {
    check_gaps(p, &TRACE_F32_GAPS, true)
}

/// PROVENANCE's four booleans read as registered: a clean export, threads
/// bit-equal, every checkpoint equal to ensemble.csv, and the forward path
/// equal to flyvis's `stimulus_response`.
pub(crate) fn check_booleans(p: &Provenance) -> Result<(), ParityError> {
    for (path, value, want) in [
        ("git.dirty", p.git_dirty, false),
        ("threads.bit_equal", p.threads_bit_equal, true),
        ("parameter_equality.all_50_checkpoints_equal_ensemble_csv", p.all_checkpoints_equal_ensemble, true),
        ("equivalence.stimulus_response_bit_equal", p.stimulus_response_bit_equal, true),
    ] {
        if value != want {
            return Err(mismatch(format!("PROVENANCE {path} is {value}: the reference is not the one registered")));
        }
    }
    Ok(())
}

/// The centre nodes are the network's, one per cell type, in `CELL_TYPES`
/// order. Each index is checked before it is used.
pub(crate) fn check_centre_nodes(centre: &[CentreNode], net: &TypedNetwork) -> Result<(), ParityError> {
    let n = net.num_nodes();
    if centre.len() != CELL_TYPES.len() {
        return Err(mismatch(format!("centre nodes: {} listed, expected {}", centre.len(), CELL_TYPES.len())));
    }
    for (k, c) in centre.iter().enumerate() {
        let node = c.node as usize;
        if node >= n {
            return Err(mismatch(format!("centre node {k}: node {node} is outside the network's {n} nodes")));
        }
        let t = &net.cell_types()[net.node_cell_type()[node] as usize];
        if *t != c.cell_type || c.cell_type != CELL_TYPES[k] {
            return Err(mismatch(format!(
                "centre node {k}: node {node} is {t}, manifest says {}, CELL_TYPES says {}",
                c.cell_type, CELL_TYPES[k]
            )));
        }
    }
    Ok(())
}

/// A block's shape, from the manifest alone.
fn shape(f: &Fixtures, name: &str, key: &BlockKey) -> Result<Vec<usize>, ParityError> {
    let info = file(f, name)?;
    info.blocks
        .iter()
        .find(|b| b.run == key.run && b.dt == key.dt && b.sequence == key.sequence)
        .map(|b| b.shape.clone())
        .ok_or_else(|| mismatch(format!("{name}: no block for run {:?}, dt {}, sequence {:?}", key.run, key.dt, key.sequence)))
}

/// Everything `validate` checks before any job runs.
pub(crate) fn check_fixtures(f: &Fixtures, t: &TraceData, p: &Provenance) -> Result<(), ParityError> {
    let n = f.net.num_nodes();
    centre_nodes(f)?;
    check_centre_nodes(&f.manifest.centre_nodes, &f.net)?;

    // the sequences: 12, in index order, six snapshot frames at every dt
    if t.sequences.len() != 12 {
        return Err(mismatch(format!("the manifest lists {} sequences, expected 12", t.sequences.len())));
    }
    for (i, q) in t.sequences.iter().enumerate() {
        if q.index as usize != i {
            return Err(mismatch(format!("manifest sequences must be listed in index order: entry {i} is sequence {}", q.index)));
        }
        for dt in &f.manifest.dts {
            let layout = q.frames(&dt.id).ok_or_else(|| mismatch(format!("sequence {i} has no frames at dt {}", dt.id)))?;
            if layout.snapshot_frames.len() != SNAPSHOTS {
                return Err(mismatch(format!(
                    "sequence {i}'s snapshot frames: expected {SNAPSHOTS}, found {}",
                    layout.snapshot_frames.len()
                )));
            }
        }
    }

    // every block the jobs read has the shape they expect
    for &(run, _, _, p) in &RUNS {
        let (info, dt, _) = checked_run(f, run)?;
        let sfx = match p {
            Dtype::F32 => "f32",
            Dtype::F64 => "f64",
        };
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
        let s = shape(f, &format!("settled.{sfx}.bin"), &key)?;
        if s != [n] {
            return Err(mismatch(format!("run {run}: settled shape: expected [{n}], found {s:?}")));
        }
        let s = shape(f, &format!("settling_centre.{sfx}.bin"), &key)?;
        if s != [dt.n_settle, CELL_TYPES.len()] {
            return Err(mismatch(format!(
                "run {run}: settling_centre shape: expected [{}, {}], found {s:?}",
                dt.n_settle,
                CELL_TYPES.len()
            )));
        }
        for q in &t.sequences {
            let seq = q.index;
            let frames = q.frames(&info.dt).map_or(0, |l| l.n_frames);
            let input = shape(f, "input.f64.bin", &BlockKey { run: None, dt: &info.dt, sequence: Some(seq) })?;
            if input != [frames, HEXALS] {
                return Err(mismatch(format!("input shape, sequence {seq}: expected [{frames}, {HEXALS}], found {input:?}")));
            }
            let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(seq) };
            let c = shape(f, &format!("centre.{sfx}.bin"), &key)?;
            if c != [frames, CELL_TYPES.len()] {
                return Err(mismatch(format!(
                    "run {run}: centre shape, sequence {seq}: expected [{frames}, {}], found {c:?}",
                    CELL_TYPES.len()
                )));
            }
            let s = shape(f, &format!("snapshots.{sfx}.bin"), &key)?;
            if s != [SNAPSHOTS, n] {
                return Err(mismatch(format!("run {run}: snapshots shape, sequence {seq}: expected [{SNAPSHOTS}, {n}], found {s:?}")));
            }
        }
    }

    // each file is the size the manifest and PROVENANCE give
    let lengths = SETTLING_FILES
        .iter()
        .map(|&name| (name, f.settling.iter().find(|(n, _)| *n == name).map_or(0, |(_, b)| b.len())))
        .chain([
            ("input.f64.bin", t.input.len()),
            ("centre.f32.bin", t.centre_f32.len()),
            ("centre.f64.bin", t.centre_f64.len()),
            ("snapshots.f32.bin", t.snapshots_f32.len()),
            ("snapshots.f64.bin", t.snapshots_f64.len()),
        ]);
    for (name, len) in lengths {
        let manifest = file(f, name)?.bytes;
        if len != manifest {
            return Err(mismatch(format!("{name}: {len} bytes, manifest says {manifest}")));
        }
        let size = p
            .sizes
            .iter()
            .find(|(k, _)| k == name)
            .map(|&(_, v)| v)
            .ok_or_else(|| mismatch(format!("{name}: PROVENANCE sizes has no entry")))?;
        if len as u64 != size {
            return Err(mismatch(format!("{name}: {len} bytes, PROVENANCE sizes says {size}")));
        }
    }

    check_settling_gaps(p)?;
    check_trace_gaps(p)?;
    check_booleans(p)
}

/// A Provenance carrying the registered gaps and the registered booleans.
#[cfg(test)]
pub(crate) fn registered_provenance() -> Provenance {
    use crate::reference::GapEntry;
    {
        let gaps = SETTLING_F32_GAPS
            .iter()
            .zip(&TRACE_F32_GAPS)
            .map(|(s, t)| GapEntry {
                key: format!("{} {}", s.0, s.1),
                settled: s.2,
                settling_centre: s.3,
                centre: t.2,
                snapshots: t.3,
            })
            .collect();
        Provenance {
            exported: "2026-10-27".into(),
            export_id: "x".into(),
            generator_path: "tools/export_reference.py".into(),
            generator_sha256: "0".into(),
            git_head: "0".into(),
            git_dirty: false,
            versions: vec![],
            platform: "p".into(),
            machine: "m".into(),
            device: "cpu".into(),
            uv_lock_sha256: "0".into(),
            threads_default: 12,
            threads_checked: vec![1, 12],
            threads_bit_equal: true,
            threads_runs_checked: vec![0, 1],
            models: vec![],
            all_checkpoints_equal_ensemble: true,
            stimulus_response_bit_equal: true,
            equivalence_sequences: vec![0, 4],
            input_rounding: vec![],
            gaps,
            sizes: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered() -> Provenance {
        registered_provenance()
    }

    #[test]
    fn the_registered_gaps_pass() {
        let p = registered();
        assert_eq!(check_settling_gaps(&p), Ok(()));
        assert_eq!(check_trace_gaps(&p), Ok(()));
        assert_eq!(check_booleans(&p), Ok(()));
    }

    /// One ulp off in any of the 16 gaps is a Mismatch naming it.
    #[test]
    fn a_gap_one_ulp_off_is_a_mismatch_naming_it() {
        for i in 0..4 {
            for q in ["settled", "settling_centre", "centre", "snapshots"] {
                let mut p = registered();
                let g = &mut p.gaps[i];
                let v = match q {
                    "settled" => &mut g.settled,
                    "settling_centre" => &mut g.settling_centre,
                    "centre" => &mut g.centre,
                    _ => &mut g.snapshots,
                };
                *v = f64::from_bits(v.to_bits() + 1);
                let key = g.key.clone();
                let e = if matches!(q, "settled" | "settling_centre") { check_settling_gaps(&p) } else { check_trace_gaps(&p) };
                match e {
                    Err(ParityError::Mismatch(m)) => assert!(m.starts_with(&format!("{key} {q}: PROVENANCE ")), "{m}"),
                    other => panic!("{key} {q}: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn a_provenance_boolean_read_the_other_way_is_a_mismatch_naming_its_path() {
        type Flip = fn(&mut Provenance);
        let cases: [(&str, Flip); 4] = [
            ("git.dirty is true", |p| p.git_dirty = true),
            ("threads.bit_equal is false", |p| p.threads_bit_equal = false),
            (
                "parameter_equality.all_50_checkpoints_equal_ensemble_csv is false",
                |p| p.all_checkpoints_equal_ensemble = false,
            ),
            ("equivalence.stimulus_response_bit_equal is false", |p| p.stimulus_response_bit_equal = false),
        ];
        for (what, flip) in cases {
            let mut p = registered();
            flip(&mut p);
            assert_eq!(
                check_booleans(&p),
                Err(ParityError::Mismatch(format!("PROVENANCE {what}: the reference is not the one registered")))
            );
        }
    }

    /// A centre node past the network is refused before it indexes anything.
    #[test]
    fn a_centre_node_outside_the_network_is_a_mismatch_not_a_panic() {
        let net = TypedNetwork::new(semblance_connectome::TypedNetworkParts {
            cell_types: CELL_TYPES.iter().map(|t| t.to_string()).collect(),
            conn_types: vec![],
            conn_sign: vec![],
            node_cell_type: (0..65).collect(),
            node_u: vec![0; 65],
            node_v: vec![0; 65],
            edge_source: vec![],
            edge_target: vec![],
            edge_conn_type: vec![],
            edge_n_syn: vec![],
        })
        .expect("a valid network");
        let mut centre: Vec<CentreNode> =
            CELL_TYPES.iter().enumerate().map(|(k, t)| CentreNode { node: k as u32, cell_type: t.to_string() }).collect();
        assert_eq!(check_centre_nodes(&centre, &net), Ok(()));
        centre[7].node = 65;
        assert_eq!(
            check_centre_nodes(&centre, &net),
            Err(ParityError::Mismatch("centre node 7: node 65 is outside the network's 65 nodes".into()))
        );
        centre[7].node = 8;
        assert!(matches!(check_centre_nodes(&centre, &net), Err(ParityError::Mismatch(m)) if m.starts_with("centre node 7: node 8 is")));
    }
}
```

- [ ] **Step 5: `validate`.** Presence, fixture checks, the 2 x runs jobs on
  a scoped pool (each job's error kept in its slot, a foreign result in a
  slot refused), the four controls on the same pool, and coverage from the
  exact jobs that passed. The pass predicates fail closed (R18).

Create `crates/flyvis/src/parity/validate.rs` with:

```rust
//! `validate`: every gate and control, on a pool of threads, into one
//! report. What `semblance validate` prints is formatted from this in `app`.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::bounds::{settling_bounds, trace_bounds, Bound};
use super::checks::check_fixtures;
use super::fingerprint::{libm_fingerprint, LibmFingerprint};
use super::inputs::{read_text, required_files, PROVENANCE};
use super::run::{play, run, settle, Counts, Diff, RunResult, SequenceResult, Start};
use super::{Fixtures, ParityError, TraceData, CONTROL_MISS, F64_BOUND, RUNS};
use crate::reference::{Dtype, Provenance};

/// What to run. `runs` exists for tests; the command always runs all eight.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Worker threads, at least 1. More than there are jobs is capped.
    pub threads: usize,
    /// Non-empty, distinct, each below 8.
    pub runs: Vec<u32>,
}

impl Default for Options {
    fn default() -> Self {
        Options { threads: 1, runs: (0..RUNS.len() as u32).collect() }
    }
}

impl Options {
    fn check(&self) -> Result<(), ParityError> {
        if self.threads == 0 {
            return Err(ParityError::Usage("threads must be at least 1".into()));
        }
        if self.runs.is_empty() {
            return Err(ParityError::Usage("no runs to validate".into()));
        }
        for (i, &r) in self.runs.iter().enumerate() {
            if r as usize >= RUNS.len() {
                return Err(ParityError::Usage(format!("run {r} is not one of the {} registered runs", RUNS.len())));
            }
            if self.runs[..i].contains(&r) {
                return Err(ParityError::Usage(format!("run {r} is listed twice")));
            }
        }
        Ok(())
    }
}

/// One run with one source of synapse counts: settling, then all twelve
/// sequences from the settled state.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub run: u32,
    pub model: String,
    pub dt: String,
    pub precision: Dtype,
    pub counts: Counts,
    /// (settled, settling_centre)
    pub settling_bounds: (Bound, Bound),
    /// (centre, snapshots)
    pub trace_bounds: (Bound, Bound),
    pub outcome: Result<RunResult, ParityError>,
}

impl Job {
    /// Fails closed (spec R18): only a complete result for this job, with
    /// every exact value equal or every libm maximum within its bound.
    pub fn passed(&self) -> bool {
        let Ok(r) = &self.outcome else { return false };
        let s = &r.settling;
        if s.run != self.run || s.counts != self.counts || s.settled_len == 0 || s.centre_len == 0 {
            return false;
        }
        let complete = r.sequences.len() == 12
            && r.sequences.iter().enumerate().all(|(i, q)| q.sequence as usize == i && q.n_frames > 0 && q.snapshot_frames.len() == 6);
        if !complete {
            return false;
        }
        match self.counts {
            Counts::Torch => {
                s.settled.differ == 0
                    && s.settling_centre.differ == 0
                    && r.sequences.iter().all(|q| q.centre.differ == 0 && q.snapshots.differ == 0)
            }
            Counts::Libm => {
                let (bs, bc) = (self.settling_bounds.0.value(), self.settling_bounds.1.value());
                let (tc, ts) = (self.trace_bounds.0.value(), self.trace_bounds.1.value());
                s.settled.max <= bs
                    && s.settling_centre.max <= bc
                    && r.sequences.iter().all(|q| q.centre.max <= tc && q.snapshots.max <= ts)
            }
            Counts::IntegerNSyn => false,
        }
    }
}

/// The four controls, each on run 3 (model 000, dt 1/50, f64).
#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    /// Settled with model 049's parameters, torch's counts.
    pub wrong_model: Result<Diff, ParityError>,
    /// Settled with the spec's integer counts.
    pub integer_counts: Result<Diff, ParityError>,
    /// Sequence 8 with every frame's hexals mirrored.
    pub mirrored: Result<SequenceResult, ParityError>,
    /// Sequence 8 from flyvis's own settled state.
    pub from_flyvis: Result<SequenceResult, ParityError>,
}

impl Controls {
    /// Every control returned and holds (spec R14).
    pub fn passed(&self) -> bool {
        matches!(&self.wrong_model, Ok(d) if d.max > CONTROL_MISS)
            && matches!(&self.integer_counts, Ok(d) if d.max > F64_BOUND)
            && matches!(&self.mirrored, Ok(s) if s.snapshots.max > CONTROL_MISS)
            && matches!(&self.from_flyvis, Ok(s) if s.n_frames > 0 && s.centre.differ == 0 && s.snapshots.differ == 0)
    }
}

/// Sintel revisit trigger 1, for one run: each centre type's movement, from
/// the run's exact job. `None` if that job did not pass.
#[derive(Clone, Debug, PartialEq)]
pub struct Coverage {
    pub run: u32,
    pub movement: Option<Vec<f64>>,
}

/// Everything `validate` found.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub provenance: Provenance,
    /// As requested.
    pub runs: Vec<u32>,
    /// The network's cell types, in its order.
    pub cell_types: Vec<String>,
    /// Per node, an index into `cell_types`.
    pub node_cell_type: Vec<u32>,
    /// The manifest's centre nodes, one per cell type in `CELL_TYPES` order.
    pub centre_nodes: Vec<u32>,
    /// runs x [Torch, Libm], in (run, counts) order.
    pub jobs: Vec<Job>,
    pub controls: Controls,
    /// One per run, in run order.
    pub coverage: Vec<Coverage>,
    pub libm: LibmFingerprint,
    /// The worker threads used.
    pub threads: usize,
}

impl Report {
    pub fn cell_type(&self, node: usize) -> &str {
        &self.cell_types[self.node_cell_type[node] as usize]
    }

    /// Fails closed (spec R18): one job per (run, counts), each passed, the
    /// controls passed, and one coverage entry per run.
    pub fn passed(&self) -> bool {
        self.jobs.len() == 2 * self.runs.len()
            && self.runs.iter().all(|&r| {
                [Counts::Torch, Counts::Libm]
                    .iter()
                    .all(|&c| self.jobs.iter().filter(|j| j.run == r && j.counts == c).count() == 1)
            })
            && self.jobs.iter().all(Job::passed)
            && self.controls.passed()
            && self.coverage.len() == self.runs.len()
            && self.coverage.iter().zip(&self.runs).all(|(c, &r)| c.run == r)
    }

    /// All eight registered runs, in order.
    pub fn complete(&self) -> bool {
        self.runs.iter().copied().eq(0..RUNS.len() as u32)
    }
}

/// Runs `work(i)` for every i in 0..n on up to `threads` threads, the
/// calling thread included, and returns the results in index order. A
/// thread that cannot be spawned leaves its share to the others.
fn pool<T: Send>(threads: usize, n: usize, work: impl Fn(usize) -> T + Sync) -> Vec<Option<T>> {
    let slots: Vec<Mutex<Option<T>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let worker = || loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        if i >= n {
            break;
        }
        let r = work(i);
        *slots[i].lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    };
    std::thread::scope(|s| {
        for _ in 1..threads.min(n) {
            if std::thread::Builder::new().spawn_scoped(s, worker).is_err() {
                break;
            }
        }
        worker();
    });
    slots.into_iter().map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner())).collect()
}

enum Control {
    Settled(Result<Diff, ParityError>),
    Played(Result<SequenceResult, ParityError>),
}

fn unfilled(i: usize) -> ParityError {
    ParityError::Mismatch(format!("job slot {i} was never filled"))
}

/// Runs every gate and control on the fixtures under `data`.
///
/// 1. Every required file is present (else `Missing`, for the first absent).
/// 2. The fixture checks pass (else a fixture error), before any arithmetic.
/// 3. The 2 x runs jobs run on `min(threads, jobs)` threads; each job's
///    error stays in its outcome.
/// 4. The four controls run on the same pool, and coverage is read from the
///    exact jobs that passed.
pub fn validate(data: &Path, opts: &Options) -> Result<Report, ParityError> {
    opts.check()?;
    for rel in required_files() {
        let path = data.join(rel);
        if let Err(e) = std::fs::metadata(&path) {
            return Err(match e.kind() {
                std::io::ErrorKind::NotFound => ParityError::Missing { rel: rel.to_string(), path },
                kind => ParityError::Unreadable { rel: rel.to_string(), path, kind },
            });
        }
    }
    let provenance = Provenance::parse(&read_text(data, PROVENANCE)?)
        .map_err(|e| ParityError::Malformed { rel: PROVENANCE.into(), detail: e.to_string() })?;
    let f = Fixtures::load(data)?;
    let t = TraceData::load(data)?;
    check_fixtures(&f, &t, &provenance)?;

    let mut jobs = Vec::new();
    for &r in &opts.runs {
        let (_, model, dt, p) = RUNS[r as usize];
        for counts in [Counts::Torch, Counts::Libm] {
            jobs.push(Job {
                run: r,
                model: model.to_string(),
                dt: dt.to_string(),
                precision: p,
                counts,
                settling_bounds: settling_bounds(model, dt, p, counts)?,
                trace_bounds: trace_bounds(model, dt, p, counts)?,
                outcome: Err(unfilled(jobs.len())),
            });
        }
    }
    let threads = opts.threads.min(jobs.len());
    let outcomes = pool(threads, jobs.len(), |i| run(&f, &t.view(jobs[i].precision), jobs[i].run, jobs[i].counts));
    for (i, (job, outcome)) in jobs.iter_mut().zip(outcomes).enumerate() {
        job.outcome = match outcome {
            None => Err(unfilled(i)),
            Some(Ok(r)) if r.settling.run != job.run || r.settling.counts != job.counts => Err(ParityError::Mismatch(
                format!("job slot {i} holds run {} {:?}", r.settling.run, r.settling.counts),
            )),
            Some(o) => o,
        };
    }

    let t64 = t.view(Dtype::F64);
    let mut controls = pool(threads.min(4), 4, |i| match i {
        0 => Control::Settled(settle(&f, 3, "049", Counts::Torch).map(|s| s.settled)),
        1 => Control::Settled(settle(&f, 3, "000", Counts::IntegerNSyn).map(|s| s.settled)),
        2 => Control::Played(first(play(&f, &t64, 3, Counts::Torch, Start::Rust, &[8], true))),
        _ => Control::Played(first(play(&f, &t64, 3, Counts::Torch, Start::Flyvis, &[8], false))),
    })
    .into_iter();
    let mut settled = |i: usize| match controls.next() {
        Some(Some(Control::Settled(r))) => r,
        _ => Err(unfilled(i)),
    };
    let wrong_model = settled(0);
    let integer_counts = settled(1);
    let mut played = |i: usize| match controls.next() {
        Some(Some(Control::Played(r))) => r,
        _ => Err(unfilled(i)),
    };
    let mirrored = played(2);
    let from_flyvis = played(3);

    let coverage = opts
        .runs
        .iter()
        .map(|&r| Coverage {
            run: r,
            movement: jobs
                .iter()
                .find(|j| j.run == r && j.counts == Counts::Torch && j.passed())
                .and_then(|j| j.outcome.as_ref().ok())
                .map(|o| o.centre_movement.clone()),
        })
        .collect();
    Ok(Report {
        provenance,
        runs: opts.runs.clone(),
        cell_types: f.net.cell_types().to_vec(),
        node_cell_type: f.net.node_cell_type().iter().map(|&t| u32::from(t)).collect(),
        centre_nodes: f.manifest.centre_nodes.iter().map(|c| c.node).collect(),
        jobs,
        controls: Controls { wrong_model, integer_counts, mirrored, from_flyvis },
        coverage,
        libm: libm_fingerprint(),
        threads,
    })
}

fn first(r: Result<Vec<SequenceResult>, ParityError>) -> Result<SequenceResult, ParityError> {
    r?.into_iter().next().ok_or_else(|| ParityError::Mismatch("a control played no sequence".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::SettlingResult;
    use crate::test_support::{data_dir, or_skip, require_present};

    fn diff0() -> Diff {
        Diff { max: 0.0, at: 0, differ: 0 }
    }

    fn sequence(s: u32) -> SequenceResult {
        SequenceResult {
            sequence: s,
            family: "flashes".into(),
            n_frames: 150,
            snapshot_frames: vec![0, 29, 59, 89, 119, 149],
            centre: diff0(),
            snapshots: diff0(),
            first: None,
        }
    }

    fn job(run: u32, counts: Counts) -> Job {
        let (_, model, dt, p) = RUNS[run as usize];
        Job {
            run,
            model: model.into(),
            dt: dt.into(),
            precision: p,
            counts,
            settling_bounds: settling_bounds(model, dt, p, counts).unwrap(),
            trace_bounds: trace_bounds(model, dt, p, counts).unwrap(),
            outcome: Ok(RunResult {
                settling: SettlingResult {
                    run,
                    model: model.into(),
                    counts,
                    settled: diff0(),
                    settling_centre: diff0(),
                    settled_len: 45669,
                    centre_len: 3250,
                    first_centre_difference: None,
                    per_type_max: vec![0.0; 65],
                },
                sequences: (0..12).map(sequence).collect(),
                centre_movement: vec![1.0; 65],
            }),
        }
    }

    fn controls() -> Controls {
        let mut mirrored = sequence(8);
        mirrored.snapshots.max = 2.5;
        Controls {
            wrong_model: Ok(Diff { max: 1.0, at: 0, differ: 1 }),
            integer_counts: Ok(Diff { max: 1e-6, at: 0, differ: 1 }),
            mirrored: Ok(mirrored),
            from_flyvis: Ok(sequence(8)),
        }
    }

    fn report(runs: Vec<u32>) -> Report {
        Report {
            provenance: crate::parity::checks::registered_provenance(),
            jobs: runs.iter().flat_map(|&r| [job(r, Counts::Torch), job(r, Counts::Libm)]).collect(),
            coverage: runs.iter().map(|&r| Coverage { run: r, movement: Some(vec![1.0; 65]) }).collect(),
            runs,
            cell_types: vec![],
            node_cell_type: vec![],
            centre_nodes: vec![],
            controls: controls(),
            libm: libm_fingerprint(),
            threads: 1,
        }
    }

    #[test]
    fn options_refuse_zero_threads_no_runs_an_unknown_run_and_a_repeat() {
        assert_eq!(Options::default(), Options { threads: 1, runs: (0..8).collect() });
        assert_eq!(Options::default().check(), Ok(()));
        for opts in [
            Options { threads: 0, ..Options::default() },
            Options { runs: vec![], ..Options::default() },
            Options { runs: vec![8], ..Options::default() },
            Options { runs: vec![2, 3, 2], ..Options::default() },
        ] {
            assert!(matches!(opts.check(), Err(ParityError::Usage(_))), "{opts:?}");
            assert!(matches!(validate(Path::new("/nonexistent"), &opts), Err(ParityError::Usage(_))), "{opts:?}");
        }
    }

    #[test]
    fn a_job_passes_only_when_complete_and_within_its_bounds() {
        assert!(job(2, Counts::Torch).passed() && job(2, Counts::Libm).passed());
        let mut j = job(2, Counts::Torch);
        j.outcome = Err(ParityError::Mismatch("x".into()));
        assert!(!j.passed(), "an error");
        let mut j = job(2, Counts::Torch);
        j.outcome.as_mut().unwrap().sequences.pop();
        assert!(!j.passed(), "11 sequences");
        let mut j = job(2, Counts::Torch);
        j.outcome.as_mut().unwrap().settling.run = 3;
        assert!(!j.passed(), "another run's result");
        let mut j = job(2, Counts::Libm);
        j.outcome.as_mut().unwrap().settling.counts = Counts::Torch;
        assert!(!j.passed(), "other counts' result");
        let mut j = job(2, Counts::Torch);
        j.outcome.as_mut().unwrap().sequences[5].snapshots.differ = 1;
        assert!(!j.passed(), "an exact value differs");
        let mut j = job(2, Counts::Libm);
        let bound = j.trace_bounds.1.value();
        j.outcome.as_mut().unwrap().sequences[5].snapshots = Diff { max: bound, at: 0, differ: 9 };
        assert!(j.passed(), "a libm maximum at its bound holds");
        j.outcome.as_mut().unwrap().sequences[5].snapshots.max = f64::from_bits(bound.to_bits() + 1);
        assert!(!j.passed(), "a libm maximum past its bound");
        let mut j = job(2, Counts::Libm);
        j.outcome.as_mut().unwrap().settling.settled.max = f64::INFINITY;
        assert!(!j.passed(), "an infinite maximum");
    }

    #[test]
    fn controls_pass_only_when_every_one_returned_and_holds() {
        assert!(controls().passed());
        let e = || ParityError::Mismatch("x".into());
        let mut c = controls();
        c.wrong_model = Err(e());
        assert!(!c.passed());
        let mut c = controls();
        c.integer_counts = Ok(Diff { max: 1e-10, at: 0, differ: 1 });
        assert!(!c.passed(), "not above the f64 bound");
        let mut c = controls();
        c.mirrored = Err(e());
        assert!(!c.passed());
        let mut c = controls();
        c.from_flyvis.as_mut().unwrap().centre.differ = 1;
        assert!(!c.passed());
    }

    #[test]
    fn a_report_passes_only_with_every_job_once() {
        assert!(report(vec![2, 3]).passed());
        assert!(!report(vec![2, 3]).complete());
        assert!(report((0..8).collect()).complete());
        let mut r = report(vec![2, 3]);
        r.jobs.pop();
        assert!(!r.passed(), "a missing job");
        let mut r = report(vec![2, 3]);
        r.jobs[3] = job(3, Counts::Torch);
        assert!(!r.passed(), "a duplicated (run, counts)");
        let mut r = report(vec![2, 3]);
        r.coverage.pop();
        assert!(!r.passed(), "a missing coverage entry");
        let mut r = report(vec![2, 3]);
        r.controls.from_flyvis = Err(ParityError::Mismatch("x".into()));
        assert!(!r.passed(), "a control errored");
        assert!(!report(vec![0, 1, 2, 3, 4, 5, 7, 6]).complete(), "out of order");
    }

    #[test]
    fn the_pool_returns_every_result_in_order_at_any_thread_count() {
        for threads in [1, 3, 16] {
            let r = pool(threads, 10, |i| i * i);
            assert_eq!(r, (0..10).map(|i| Some(i * i)).collect::<Vec<_>>(), "{threads} threads");
        }
    }

    /// The thread count changes nothing: every job, control and coverage
    /// value is equal at 1 and at 4 threads.
    #[test]
    fn validate_is_bit_identical_across_thread_counts() {
        let Some(()) = or_skip(require_present(required_files())) else { return };
        let one = validate(&data_dir(), &Options { threads: 1, runs: vec![2, 3] }).unwrap_or_else(|e| panic!("{e}"));
        let four = validate(&data_dir(), &Options { threads: 4, runs: vec![2, 3] }).unwrap_or_else(|e| panic!("{e}"));
        assert!(one.passed(), "runs 2 and 3 pass");
        assert_eq!((one.threads, four.threads), (1, 4));
        assert_eq!(one.jobs, four.jobs);
        assert_eq!(one.controls, four.controls);
        assert_eq!(one.coverage, four.coverage);
    }

    /// A temporary data dir of links to the real files, with one replaced.
    #[cfg(unix)]
    fn altered_data(test: &str, rel: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("semblance-parity-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for r in required_files() {
            let to = dir.join(r);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            if *r == rel {
                std::fs::write(&to, bytes).unwrap();
            } else {
                std::os::unix::fs::symlink(data_dir().join(r), &to).unwrap();
            }
        }
        dir
    }

    #[cfg(unix)]
    #[test]
    fn validate_refuses_a_provenance_gap_one_ulp_off() {
        let Some(()) = or_skip(require_present(required_files())) else { return };
        let text = std::fs::read_to_string(data_dir().join(PROVENANCE)).unwrap();
        let (model, dt, gap, _) = crate::parity::TRACE_F32_GAPS[3];
        let printed = format!("{gap:e}");
        let k = text.find(&format!("\"{model} {dt}\"")).expect("the gap's key");
        let at = k + text[k..].find("\"centre\": ").expect("its centre gap") + "\"centre\": ".len();
        let len = text[at..].find([',', '}']).unwrap();
        let value: f64 = text[at..at + len].trim().parse().unwrap();
        assert_eq!(value.to_bits(), gap.to_bits(), "{printed}");
        let altered = format!("{}{:e}{}", &text[..at], f64::from_bits(gap.to_bits() + 1), &text[at + len..]);
        let dir = altered_data("gap", PROVENANCE, altered.as_bytes());
        let r = validate(&dir, &Options::default());
        std::fs::remove_dir_all(&dir).unwrap();
        match r {
            Err(ParityError::Mismatch(m)) => assert!(m.starts_with(&format!("{model} {dt} centre: PROVENANCE ")), "{m}"),
            other => panic!("expected the gap's Mismatch, got {:?}", other.map(|_| ())),
        }
    }

    #[cfg(unix)]
    #[test]
    fn validate_refuses_a_file_whose_size_disagrees() {
        let Some(()) = or_skip(require_present(required_files())) else { return };
        let rel = "flyvis-reference/centre.f64.bin";
        let mut bytes = std::fs::read(data_dir().join(rel)).unwrap();
        bytes.pop();
        let dir = altered_data("size", rel, &bytes);
        let r = validate(&dir, &Options::default());
        std::fs::remove_dir_all(&dir).unwrap();
        match r {
            Err(ParityError::Mismatch(m)) => {
                assert_eq!(m, format!("centre.f64.bin: {} bytes, manifest says {}", bytes.len(), bytes.len() + 1))
            }
            other => panic!("expected the size Mismatch, got {:?}", other.map(|_| ())),
        }
    }
}
```

- [ ] **Step 6: Declare and re-export them.**

In `crates/flyvis/src/parity/mod.rs`, replace:

```rust

mod bounds;
mod inputs;
mod run;

pub use bounds::{settling_bounds, trace_bounds, Bound};
pub use inputs::{how_to_get, required_files, Fixtures, TraceData, TraceInputs};
pub use run::{diff, play, run, settle, Counts, Diff, RunResult, SequenceResult, SettlingResult, Start};

#[cfg(test)]
```

with:

```rust

mod bounds;
mod checks;
mod fingerprint;
mod inputs;
mod run;
mod validate;

pub use bounds::{settling_bounds, trace_bounds, Bound};
pub use checks::{check_settling_gaps, check_trace_gaps};
pub use fingerprint::{libm_fingerprint, LibmFingerprint};
pub use inputs::{how_to_get, required_files, Fixtures, TraceData, TraceInputs};
pub use run::{diff, play, run, settle, Counts, Diff, RunResult, SequenceResult, SettlingResult, Start};
pub use validate::{validate, Controls, Coverage, Job, Options, Report};

#[cfg(test)]
```

- [ ] **Step 7: Run the tests.**

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib -- parity:: provenance gate_support`
  Expected: 38 passed (16 new: `fingerprint` 4, `checks` 4, `validate` 8),
  about 15 s. `validate_is_bit_identical_across_thread_counts` runs runs 2
  and 3 twice. Then `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --lib`:
  296 passed with task 2 merged.

- [ ] **Step 8: The shadow suites that run the provenance tests.**

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --test acceptance_settling --test acceptance_traces --test acceptance_minors`
  Expected: 13, 24 and 9 passed. SETTLING-08 and TRACES-10 put one gap one
  ulp off in PROVENANCE.json and require the failure to contain
  `{model} {dt} {quantity}` (C7); MINORS-03 requires both provenance tests
  to pass with the manifest missing (they read only PROVENANCE.json).

- [ ] **Step 9: Clippy, MSRV and the gate.** As in Global Constraints.
  Expected: clean; 0 failed (464 passed after wave 1 plus this task).

- [ ] **Step 10: Commit.**

```bash
git add crates/flyvis/src/parity/mod.rs \
  crates/flyvis/src/parity/checks.rs \
  crates/flyvis/src/parity/validate.rs \
  crates/flyvis/src/parity/fingerprint.rs \
  crates/flyvis/src/settling_checks.rs \
  crates/flyvis/src/trace_checks.rs \
  crates/flyvis/src/test_support.rs
git commit -m "Add parity::validate, the fixture checks and the libm fingerprint" \
  -m "Temper-Task: parity-validate/validate"
```

### Task 5: Packaging (wave 2, id `packaging`)

**Wave:** 2

Spec §9 "Packaging", items 1–3, 5 and 6 (sim, codec, flyvis), 7; R17; F15.
`app/Cargo.toml` is task 6's; the version is task 7's.

**Files:**
- Create: the 16 symlinks `crates/{rng,connectome,codec,sim,evo,fidelity,flyvis}/LICENSE` and `.../NOTICE` (`-> ../../LICENSE`, `-> ../../NOTICE`), and `app/LICENSE`, `app/NOTICE` (`-> ../LICENSE`, `-> ../NOTICE`)
- Modify: `NOTICE` (`syn_count_torch.rs`; the ported code, R17)
- Modify: `CITATION.cff` (version only)
- Modify: `crates/sim/Cargo.toml`, `crates/codec/Cargo.toml`, `crates/flyvis/Cargo.toml` (descriptions, flyvis's licence, `exclude = ["tests/"]` for sim and flyvis)
- Modify: `Cargo.toml` (`categories` only)

**Interfaces:**
- Consumes: task 3's `crates/flyvis/Cargo.toml` (sim under `[dependencies]`).
- Produces: every `.crate` carries LICENSE and NOTICE; `semblance-flyvis` is
  `Apache-2.0 AND MIT`. No dependency changes, so `Cargo.lock` does not move.

- [ ] **Step 1: See the packages lack the licence files.**

  Run: `rm -rf target/package && cargo package --workspace --allow-dirty 2>/dev/null; for c in target/package/*.crate; do tar tzf "$c"; done | grep -cE '/(LICENSE|NOTICE)$'`
  Expected: `0`.

- [ ] **Step 2: The symlinks.** They are tracked as symlinks and dereferenced
  into each `.crate` (F15). The shadow acceptance tests copy regular files
  only, so they skip them (F14), which no build needs.

```bash
for c in rng connectome codec sim evo fidelity flyvis; do
  ln -s ../../LICENSE crates/$c/LICENSE
  ln -s ../../NOTICE crates/$c/NOTICE
done
ln -s ../LICENSE app/LICENSE
ln -s ../NOTICE app/NOTICE
```

- [ ] **Step 3: NOTICE.**

In `NOTICE`, replace:

```text
                                      flyvis/utils/groundtruth_utils.py,
                                      commit 9a51668
  - release assets, in flyvis-fixtures.tar.gz:
      flyvis-build/                   (fixtures-1, fixtures-2)
```

with:

```text
                                      flyvis/utils/groundtruth_utils.py,
                                      commit 9a51668
  - crates/flyvis/src/syn_count_torch.rs
                                      torch's `exp` of `SYN_COUNT` in f32
                                      and f64, generated by
                                      tools/export_syn_count.py
  - release assets, in flyvis-fixtures.tar.gz:
      flyvis-build/                   (fixtures-1, fixtures-2)
```

In `NOTICE`, replace:

```text

  (Text from https://github.com/TuragaLab/flyvis/blob/main/license.)

They were exported from flyvis and its published pretrained models and
```

with:

```text

  (Text from https://github.com/TuragaLab/flyvis/blob/main/license.)

Parts of semblance-flyvis are also ports of flyvis's algorithms: the
connectome expansion (crates/flyvis/src/expand.rs), the hex-eye fp16 mask
construction (crates/flyvis/src/hex_eye.rs), and the readers of its ground
truth and reference exports. flyvis's code is under the same MIT licence
above, and this notice is its attribution; the ported code changes no
licence. The semblance-flyvis package is licensed "Apache-2.0 AND MIT".

They were exported from flyvis and its published pretrained models and
```

- [ ] **Step 4: CITATION.cff** (`date-released` is set by the head at tag time).

In `CITATION.cff`, replace:

```yaml
license: Apache-2.0
repository-code: "https://github.com/miki-przygoda/semblance"
version: 0.2.0
date-released: "2026-10-04"
keywords:
```

with:

```yaml
license: Apache-2.0
repository-code: "https://github.com/miki-przygoda/semblance"
version: 0.3.0
date-released: "2026-10-04"
keywords:
```

- [ ] **Step 5: The manifests.** The workspace's categories (the slug
  `science::neuroscience` exists on crates.io; the dry run does not check it):

In `Cargo.toml`, replace:

```toml
homepage = "https://github.com/miki-przygoda/semblance"
keywords = ["connectome", "neuroevolution", "compression", "neuroscience"]
categories = ["science", "simulation"]

# Shared dependency versions. Individual crates opt in under [dependencies].
```

with:

```toml
homepage = "https://github.com/miki-przygoda/semblance"
keywords = ["connectome", "neuroevolution", "compression", "neuroscience"]
categories = ["science::neuroscience", "simulation"]

# Shared dependency versions. Individual crates opt in under [dependencies].
```

`semblance-sim`:

In `crates/sim/Cargo.toml`, replace:

```toml
categories.workspace = true
readme = "../../README.md"
description = "Neuron dynamics (LIF baseline) and the simulation engine that couples a connectome to sensory input and motor readout."

[lib]
```

with:

```toml
categories.workspace = true
readme = "../../README.md"
description = "Neuron dynamics: flyvis's graded (non-spiking) integrator in f32 and f64, plus a legacy LIF spiking baseline and the engine that couples a connectome to input and readout."
# The acceptance tests read the workspace's fixtures and sources, which no
# package carries.
exclude = ["tests/"]

[lib]
```

`semblance-codec`:

In `crates/codec/Cargo.toml`, replace:

```toml
categories.workspace = true
readme = "../../README.md"
description = "The compress/uncompress round-trip: a Codec trait, a lossless WeightCodec baseline, and a ReducedCodec whose encode and decode are not implemented yet."

[lib]
```

with:

```toml
categories.workspace = true
readme = "../../README.md"
description = "The compress/uncompress round-trip: a Codec trait, a lossless WeightCodec baseline, and a ReducedCodec whose encode and decode are not implemented yet: calling them panics (todo!())."

[lib]
```

`semblance-flyvis`:

In `crates/flyvis/Cargo.toml`, replace:

```toml
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true
authors.workspace = true
```

with:

```toml
version.workspace = true
edition.workspace = true
# flyvis's MIT data is compiled in (pretrained.rs, syn_count_torch.rs,
# groundtruth.rs); see NOTICE.
license = "Apache-2.0 AND MIT"
rust-version.workspace = true
authors.workspace = true
```

In `crates/flyvis/Cargo.toml`, replace:

```toml
categories.workspace = true
readme = "../../README.md"
description = "Reads the published flyvis connectome spec and carries its curated ground truth, with no external dependencies."

[lib]
```

with:

```toml
categories.workspace = true
readme = "../../README.md"
description = "flyvis (Lappalainen et al. 2024) in Rust: reads the published connectome spec, expands it bit-exactly into the realised network, carries flyvis's trained parameters and ground truth (MIT data; see NOTICE), and checks parity with flyvis's own traces. No external dependencies."
# The acceptance tests read the workspace's fixtures and sources, which no
# package carries.
exclude = ["tests/"]

[lib]
```

- [ ] **Step 6: Commit first, then package from the clean tree.**

```bash
git add crates/rng/LICENSE \
  crates/rng/NOTICE \
  crates/connectome/LICENSE \
  crates/connectome/NOTICE \
  crates/codec/LICENSE \
  crates/codec/NOTICE \
  crates/sim/LICENSE \
  crates/sim/NOTICE \
  crates/evo/LICENSE \
  crates/evo/NOTICE \
  crates/fidelity/LICENSE \
  crates/fidelity/NOTICE \
  crates/flyvis/LICENSE \
  crates/flyvis/NOTICE \
  app/LICENSE \
  app/NOTICE \
  NOTICE \
  CITATION.cff \
  Cargo.toml \
  crates/sim/Cargo.toml \
  crates/codec/Cargo.toml \
  crates/flyvis/Cargo.toml
git commit -m "Ship LICENSE and NOTICE in every package and credit flyvis's ported code" \
  -m "Temper-Task: parity-validate/packaging"
```

  Run: `git status --short` (only untracked files outside the packages, if
  any), then
  `rm -rf target/package && cargo package --workspace && for c in target/package/*.crate; do tar tzf "$c"; done | grep -cE '/(LICENSE|NOTICE)$'`
  Expected: eight `Verifying` lines, one `warning: ignoring test ...` per
  excluded test file (sim 2, flyvis 7), and `16`. `git diff --exit-code
  Cargo.lock` is clean.

- [ ] **Step 7: The gate.** As in Global Constraints. Expected: 0 failed.
  If anything in steps 6–7 needs a fix, amend the commit with explicit paths.

### Task 6: `semblance validate` (wave 3, id `cli`)

**Wave:** 3

Spec §4, §5, §6, §7 "Generated block", R6, R7, R10, R12, F1–F3, F21.
Formatting lives here; computation in `parity`. Depends on task 4.

**Files:**
- Create: `app/src/validate.rs` (flags, exit codes, the terminal table, the Markdown block)
- Modify: `app/src/main.rs` (`"validate" => validate::cmd(&args[2..])`, the help text, the module doc)
- Modify: `app/Cargo.toml` (`semblance-flyvis`, description, `exclude = ["tests/"]`, R15)
- Modify: `Cargo.lock` (app gains `semblance-flyvis`; still 8 packages)
- Create: `app/tests/validate_cli.rs`

**Interfaces:**
- Consumes: `semblance_flyvis::parity::{validate, Options, Report, Job,
  Bound, Counts, Diff, RunResult, SequenceResult, ParityError,
  required_files, how_to_get, CONTROL_MISS, COVERAGE_THRESHOLD, F64_BOUND}`,
  `semblance_flyvis::pretrained::SYN_COUNT`, `semblance_flyvis::reference::Dtype`.
- Produces: `semblance validate [--data DIR] [--threads N] [--markdown]`
  with exit codes 0/1/2; `pub fn cmd(args: &[String]) -> ExitCode` in
  `app/src/validate.rs`. The Markdown block's exact layout (markers,
  sections, tables) is what task 9's record test parses:
  `<!-- BEGIN semblance validate --markdown -->` ... `<!-- BEGIN host-specific -->`
  ... `| libm fingerprint | {fp} |` ... `<!-- END host-specific -->` ...
  `<!-- BEGIN libm fingerprint={fp} -->` ... `<!-- END libm -->` ...
  `<!-- END semblance validate --markdown -->`.

- [ ] **Step 1: Write the failing command-line tests.**

Create `app/tests/validate_cli.rs` with:

```rust
//! The `semblance` binary's command line, without fixtures: `validate`'s
//! usage errors and its report of missing data, `help`, and `roundtrip`.
//! Each test spawns the built binary.

use std::path::PathBuf;
use std::process::{Command, Output};

fn semblance(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_semblance")).args(args).output().expect("the binary runs")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// An empty directory under the target's temp dir, unique to `name`.
fn empty_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("validate-cli-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn validate_with_no_data_lists_all_thirteen_files_and_exits_2() {
    let dir = empty_dir("no-data");
    let out = semblance(&["validate", "--data", dir.to_str().unwrap()]);
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(out.stdout.is_empty(), "nothing is printed as a result: {}", text(&out.stdout));
    assert!(err.starts_with("validate: nothing was verified; 13 fixture files are missing:\n"), "{err}");
    for rel in semblance_flyvis::parity::required_files() {
        assert!(err.contains(&format!("  {}\n", dir.join(rel).display())), "{rel} not listed:\n{err}");
    }
    assert!(err.contains("ci/fetch-fixtures.sh"), "{err}");
    assert!(err.contains("docs/results/0.3.0-flyvis-parity.md; it is not a check of this machine."), "{err}");
}

#[test]
fn validate_refuses_a_bad_command_line_with_exit_2() {
    for args in [
        &["validate", "--threads", "0"][..],
        &["validate", "--threads", "x"],
        &["validate", "--threads"],
        &["validate", "--bogus"],
        &["validate", "--data", "a", "--data", "b"],
    ] {
        let out = semblance(args);
        let err = text(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {err}");
        assert!(err.starts_with("validate: "), "{args:?}: {err}");
        assert!(err.contains("usage: semblance validate [--data DIR] [--threads N] [--markdown]"), "{args:?}: {err}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn an_unknown_command_still_exits_1() {
    let out = semblance(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("unknown command: frobnicate"));
}

#[test]
fn help_lists_validate_and_no_longer_says_everything_works_with_no_data() {
    let out = semblance(&["help"]);
    assert!(out.status.success());
    let help = text(&out.stdout);
    assert!(help.contains("  validate    check that this build reproduces flyvis bit for bit\n"), "{help}");
    assert!(help.contains("[--data DIR] [--threads N] [--markdown]"), "{help}");
    assert!(help.contains("demo, roundtrip and evolve run on a synthetic graph and need no data."), "{help}");
    assert!(!help.contains("Everything runs on a synthetic graph so it works with no data"), "{help}");
}

#[test]
fn roundtrip_is_still_lossless() {
    let out = semblance(&["roundtrip"]);
    assert!(out.status.success());
    let s = text(&out.stdout);
    assert!(s.contains("structural distance:  0.000000"), "{s}");
    assert!(s.contains("behavioural distance: 0.000000"), "{s}");
}
```

- [ ] **Step 2: Run them to see them fail.**

  Run: `cargo test -p semblance --test validate_cli`
  Expected: does not compile: `semblance_flyvis` is not yet a dependency of
  the app (`use of unresolved module or unlinked crate`).

- [ ] **Step 3: The dependency, description and exclusion.**

In `app/Cargo.toml`, replace:

```toml
categories.workspace = true
readme = "../README.md"
description = "CLI that wires the pipeline together: simulate, round-trip, and evolve. Dependency-light (std arg parsing) on purpose."

[[bin]]
```

with:

```toml
categories.workspace = true
readme = "../README.md"
description = "CLI that wires the pipeline together: simulate, round-trip, evolve, and validate parity with flyvis. Std-only argument parsing."
# The tests read the workspace's docs/ and data/, which no package carries.
exclude = ["tests/"]

[[bin]]
```

In `app/Cargo.toml`, replace:

```toml
semblance-evo.workspace = true
semblance-fidelity.workspace = true
```

with:

```toml
semblance-evo.workspace = true
semblance-fidelity.workspace = true
semblance-flyvis.workspace = true
```

  Run: `cargo build -p semblance && git diff Cargo.lock`
  Expected: the lock's `semblance` package gains `"semblance-flyvis"` in its
  dependencies, one line; `grep -c '^name = ' Cargo.lock` prints `8`.
  Then `cargo test -p semblance --test validate_cli`: compiles; 3 of 5 fail
  (no `validate` command yet: exit 1 and `unknown command: validate`; the
  help text is the old one).

- [ ] **Step 4: The command.**

Create `app/src/validate.rs` with:

```rust
//! `semblance validate [--data DIR] [--threads N] [--markdown]`: does this
//! build reproduce flyvis?
//!
//! The computation is `semblance_flyvis::parity`, the same code the crate's
//! gates run; this module parses the flags and formats the report, as a
//! terminal table or as the record's generated block (`--markdown`).
//!
//! Exit codes: 0, every gate holds; 1, a gate failed; 2, verification did not
//! complete (a fixture missing, unreadable, malformed or mismatched, or a bad
//! command line). Spec: docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md.

use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use semblance_flyvis::parity::{
    self, Bound, Counts, Diff, Job, Options, ParityError, Report, RunResult, SequenceResult, CONTROL_MISS,
    COVERAGE_THRESHOLD, F64_BOUND,
};
use semblance_flyvis::pretrained::SYN_COUNT;
use semblance_flyvis::reference::Dtype;

const USAGE: &str = "usage: semblance validate [--data DIR] [--threads N] [--markdown]";

/// The number of main jobs: the default thread count's cap.
const MAX_DEFAULT_THREADS: usize = 16;

const BEGIN: &str = "<!-- BEGIN semblance validate --markdown -->";
const END: &str = "<!-- END semblance validate --markdown -->";

#[derive(Debug, PartialEq)]
struct Args {
    data: PathBuf,
    threads: Option<usize>,
    markdown: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let (mut data, mut threads, mut markdown) = (None, None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data" => {
                if data.is_some() {
                    return Err("--data given twice".into());
                }
                data = Some(PathBuf::from(it.next().ok_or("--data needs a directory")?));
            }
            "--threads" => {
                if threads.is_some() {
                    return Err("--threads given twice".into());
                }
                let v = it.next().ok_or("--threads needs a number")?;
                let n = v
                    .parse::<usize>()
                    .ok()
                    .filter(|_| v.bytes().all(|b| b.is_ascii_digit()))
                    .ok_or_else(|| format!("--threads {v}: not a whole number"))?;
                if n == 0 {
                    return Err("--threads must be at least 1".into());
                }
                threads = Some(n);
            }
            "--markdown" => {
                if markdown {
                    return Err("--markdown given twice".into());
                }
                markdown = true;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args { data: data.unwrap_or_else(|| PathBuf::from("./data")), threads, markdown })
}

fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_DEFAULT_THREADS)
}

/// The stderr message when fixture files are absent: nothing was verified.
fn missing_message(data: &Path, missing: &[&str]) -> String {
    let mut s = if missing.len() == 1 {
        "validate: nothing was verified; 1 fixture file is missing:\n".to_string()
    } else {
        format!("validate: nothing was verified; {} fixture files are missing:\n", missing.len())
    };
    for rel in missing {
        let _ = writeln!(s, "  {}", data.join(rel).display());
    }
    if let [rel] = missing {
        let _ = writeln!(s, "{}", parity::how_to_get(rel));
    }
    s.push_str("Get them with ci/fetch-fixtures.sh (needs gh), or see tools/README.md.\n");
    s.push_str(
        "The result recorded for 0.3.0 is docs/results/0.3.0-flyvis-parity.md; it is not a check of this machine.\n",
    );
    s
}

/// 2 if any job could not complete for a fixture reason, else 0 if every
/// gate and control holds for all eight runs, else 1.
fn exit_code(r: &Report) -> u8 {
    if r.jobs.iter().any(|j| matches!(&j.outcome, Err(e) if !e.is_gate_failure())) {
        2
    } else if r.passed() && r.complete() {
        0
    } else {
        1
    }
}

/// Writes `text` line by line. A closed pipe stops the output and keeps
/// `code`; any other write error is 2. Never panics.
fn emit<W: Write>(out: &mut W, text: &str, code: u8) -> u8 {
    for line in text.lines() {
        match writeln!(out, "{line}") {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => return code,
            Err(_) => return 2,
        }
    }
    match out.flush() {
        Ok(()) => code,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => code,
        Err(_) => 2,
    }
}

pub fn cmd(args: &[String]) -> ExitCode {
    let a = match parse(args) {
        Ok(a) => a,
        Err(m) => {
            eprintln!("validate: {m}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let missing: Vec<&str> =
        parity::required_files().iter().copied().filter(|rel| std::fs::metadata(a.data.join(rel)).is_err()).collect();
    if !missing.is_empty() {
        eprint!("{}", missing_message(&a.data, &missing));
        return ExitCode::from(2);
    }
    let start = Instant::now();
    let opts = Options { threads: a.threads.unwrap_or_else(default_threads), ..Options::default() };
    let report = match parity::validate(&a.data, &opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("validate: nothing was verified: {e}");
            return ExitCode::from(if e.is_gate_failure() { 1 } else { 2 });
        }
    };
    let code = exit_code(&report);
    let text =
        if a.markdown { markdown(&report) } else { terminal(&report, &a.data, start.elapsed().as_secs_f64(), code) };
    ExitCode::from(emit(&mut io::stdout().lock(), &text, code))
}

// ---------------------------------------------------------------------------
// What both formats need
// ---------------------------------------------------------------------------

fn precision(p: Dtype) -> &'static str {
    match p {
        Dtype::F32 => "f32",
        Dtype::F64 => "f64",
    }
}

fn counts(c: Counts) -> &'static str {
    match c {
        Counts::Torch => "exact",
        Counts::Libm => "libm",
        Counts::IntegerNSyn => "integer",
    }
}

fn version<'a>(r: &'a Report, name: &str) -> &'a str {
    r.provenance.versions.iter().find(|(k, _)| k == name).map_or("?", |(_, v)| v.as_str())
}

/// "000 (best)": the model and its rank in PROVENANCE.
fn model(r: &Report, id: &str) -> String {
    match r.provenance.models.iter().find(|m| m.id == id) {
        Some(m) => format!("{id} ({})", m.rank),
        None => id.to_string(),
    }
}

/// The target this binary was built for: arch-os, plus the environment if any.
fn target() -> String {
    let env = if cfg!(target_env = "gnu") {
        "-gnu"
    } else if cfg!(target_env = "musl") {
        "-musl"
    } else if cfg!(target_env = "msvc") {
        "-msvc"
    } else {
        ""
    };
    format!("{}-{}{env}", std::env::consts::ARCH, std::env::consts::OS)
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// The centre type `k`'s name: the cell type of the manifest's centre node.
fn centre_type(r: &Report, k: usize) -> &str {
    r.cell_type(r.centre_nodes[k] as usize)
}

/// A maximum: `0` when nothing differs, otherwise `{:.3e}`.
fn max_text(d: f64) -> String {
    if d == 0.0 { "0".to_string() } else { format!("{d:.3e}") }
}

fn ratio(max: f64, bound: &Bound) -> f64 {
    max / bound.value()
}

/// One stage of one job, summarised: what the terminal row prints.
struct Row {
    compared: usize,
    unequal: usize,
    max: f64,
    /// For a libm row, max / bound; NaN for an exact row.
    ratio: f64,
    bound: f64,
    place: String,
    holds: bool,
}

/// Picks the worse of several (max, bound, where) candidates: by ratio for a
/// libm job, by maximum for an exact one. The last wins a tie, as the gates'
/// `max_by` does, so the terminal names what the gates print.
fn worst(job: &Job, candidates: Vec<(Diff, Bound, String)>) -> (f64, f64, f64, String) {
    let exact = job.counts == Counts::Torch;
    let mut best: Option<(f64, f64, f64, String)> = None;
    for (d, b, place) in candidates {
        let key = if exact { d.max } else { ratio(d.max, &b) };
        if best.as_ref().is_none_or(|(_, _, k, _)| key.total_cmp(k).is_ge()) {
            best = Some((d.max, b.value(), key, place));
        }
    }
    best.unwrap_or((0.0, 0.0, 0.0, "-".into()))
}

fn settling_row(r: &Report, job: &Job, o: &RunResult) -> Row {
    let s = &o.settling;
    let n_centre = r.centre_nodes.len();
    let candidates = vec![
        (s.settled, job.settling_bounds.0, format!("settled, node {} ({})", s.settled.at, r.cell_type(s.settled.at))),
        (
            s.settling_centre,
            job.settling_bounds.1,
            format!("centre, step {} ({})", s.settling_centre.at / n_centre, centre_type(r, s.settling_centre.at % n_centre)),
        ),
    ];
    let (max, bound, key, place) = worst(job, candidates);
    let holds = match job.counts {
        Counts::Torch => s.settled.differ + s.settling_centre.differ == 0,
        _ => s.settled.max <= job.settling_bounds.0.value() && s.settling_centre.max <= job.settling_bounds.1.value(),
    };
    Row {
        compared: s.settled_len + s.centre_len,
        unequal: s.settled.differ + s.settling_centre.differ,
        max,
        ratio: if job.counts == Counts::Torch { f64::NAN } else { key },
        bound,
        place,
        holds,
    }
}

fn centre_place(r: &Report, q: &SequenceResult, long: bool) -> String {
    let n_centre = r.centre_nodes.len();
    let k = q.centre.at % n_centre;
    if long {
        format!("seq {}, frame {}, node {} ({})", q.sequence, q.centre.at / n_centre, r.centre_nodes[k], centre_type(r, k))
    } else {
        format!("centre, seq {} frame {} ({})", q.sequence, q.centre.at / n_centre, centre_type(r, k))
    }
}

fn snapshot_place(r: &Report, q: &SequenceResult, long: bool) -> String {
    let n = r.node_cell_type.len();
    let node = q.snapshots.at % n;
    let frame = q.snapshot_frames.get(q.snapshots.at / n).copied().unwrap_or(0);
    if long {
        format!("seq {}, frame {frame}, node {node} ({})", q.sequence, r.cell_type(node))
    } else {
        format!("snaps, seq {} frame {frame} ({})", q.sequence, r.cell_type(node))
    }
}

fn trace_row(r: &Report, job: &Job, sequences: &[&SequenceResult]) -> Row {
    let n = r.node_cell_type.len();
    let n_centre = r.centre_nodes.len();
    let mut candidates = Vec::new();
    for q in sequences {
        candidates.push((q.centre, job.trace_bounds.0, centre_place(r, q, false)));
        candidates.push((q.snapshots, job.trace_bounds.1, snapshot_place(r, q, false)));
    }
    let (max, bound, key, place) = worst(job, candidates);
    let holds = sequences.iter().all(|q| match job.counts {
        Counts::Torch => q.centre.differ + q.snapshots.differ == 0,
        _ => q.centre.max <= job.trace_bounds.0.value() && q.snapshots.max <= job.trace_bounds.1.value(),
    });
    Row {
        compared: sequences.iter().map(|q| q.n_frames * n_centre + q.snapshot_frames.len() * n).sum(),
        unequal: sequences.iter().map(|q| q.centre.differ + q.snapshots.differ).sum(),
        max,
        ratio: if job.counts == Counts::Torch { f64::NAN } else { key },
        bound,
        place,
        holds,
    }
}

/// The sequences of each stimulus family, in order of first appearance.
fn families(o: &RunResult) -> Vec<(String, Vec<&SequenceResult>)> {
    let mut out: Vec<(String, Vec<&SequenceResult>)> = Vec::new();
    for q in &o.sequences {
        match out.iter_mut().find(|(f, _)| *f == q.family) {
            Some((_, v)) => v.push(q),
            None => out.push((q.family.clone(), vec![q])),
        }
    }
    out
}

fn family_name(f: &str) -> &str {
    match f {
        "moving_edge" => "moving edges",
        other => other,
    }
}

/// "0–3", or "0, 2, 5" if the indices are not consecutive.
fn sequence_range(v: &[&SequenceResult]) -> String {
    let ids: Vec<u32> = v.iter().map(|q| q.sequence).collect();
    match (ids.first(), ids.last()) {
        (Some(a), Some(b)) if ids.windows(2).all(|w| w[1] == w[0] + 1) && ids.len() > 1 => format!("{a}–{b}"),
        _ => ids.iter().map(u32::to_string).collect::<Vec<_>>().join(", "),
    }
}

/// One run's coverage, from its exact job: (weakest type, its movement),
/// the next two, the median, and the run's libm centre bound.
struct RunCoverage {
    run: u32,
    order: Vec<usize>,
    movement: Vec<f64>,
    bound: Option<f64>,
}

impl RunCoverage {
    fn weakest(&self) -> f64 {
        self.movement[self.order[0]]
    }
    fn median(&self) -> f64 {
        self.movement[self.order[self.order.len() / 2]]
    }
}

/// Per run: `Some` if measured. Ordered by `total_cmp`; a tie goes to the
/// lower cell-type index.
fn coverage(r: &Report) -> Vec<(u32, Option<RunCoverage>)> {
    r.coverage
        .iter()
        .map(|c| {
            let measured = c.movement.as_ref().filter(|m| !m.is_empty()).map(|m| {
                let mut order: Vec<usize> = (0..m.len()).collect();
                order.sort_by(|&a, &b| m[a].total_cmp(&m[b]).then(a.cmp(&b)));
                let bound =
                    r.jobs.iter().find(|j| j.run == c.run && j.counts == Counts::Libm).map(|j| j.trace_bounds.0.value());
                RunCoverage { run: c.run, order, movement: m.clone(), bound }
            });
            (c.run, measured)
        })
        .collect()
}

/// The weakest movement over every measured run: (run, type, movement). A
/// tie goes to the lowest run, then the lowest cell-type index.
fn overall_weakest(cov: &[(u32, Option<RunCoverage>)]) -> Option<(u32, usize, f64)> {
    let mut best: Option<(u32, usize, f64)> = None;
    for c in cov.iter().filter_map(|(_, c)| c.as_ref()) {
        let k = c.order[0];
        let m = c.movement[k];
        if best.is_none_or(|(_, _, b)| m.total_cmp(&b).is_lt()) {
            best = Some((c.run, k, m));
        }
    }
    best
}

/// Every (run, type) that is not driven: movement not above the threshold.
fn undriven(r: &Report, cov: &[(u32, Option<RunCoverage>)]) -> Vec<String> {
    let mut out = Vec::new();
    for c in cov.iter().filter_map(|(_, c)| c.as_ref()) {
        for (k, &m) in c.movement.iter().enumerate() {
            if m.partial_cmp(&COVERAGE_THRESHOLD) != Some(std::cmp::Ordering::Greater) {
                out.push(format!("{} in run {} ({m:.3e})", centre_type(r, k), c.run));
            }
        }
    }
    out
}

struct ControlRow {
    terminal_name: &'static str,
    record_name: &'static str,
    terminal: String,
    record: String,
    must: String,
    holds: bool,
}

fn controls(r: &Report) -> [ControlRow; 4] {
    let c = &r.controls;
    let settled = |d: &Result<Diff, ParityError>, above: f64| match d {
        Ok(d) => (format!("settled max {:.3e}", d.max), d.max > above),
        Err(e) => (e.to_string(), false),
    };
    let (wrong, wrong_holds) = settled(&c.wrong_model, CONTROL_MISS);
    let (integer, integer_holds) = settled(&c.integer_counts, F64_BOUND);
    let n = r.node_cell_type.len();
    let n_centre = r.centre_nodes.len();
    let (mirrored_t, mirrored_r, mirrored_holds) = match &c.mirrored {
        Ok(s) => (
            format!("snapshots max {:e}", s.snapshots.max),
            format!("snapshots max {:e}, centre max {:e}", s.snapshots.max, s.centre.max),
            s.snapshots.max > CONTROL_MISS,
        ),
        Err(e) => (e.to_string(), e.to_string(), false),
    };
    let (flyvis_t, flyvis_r, flyvis_holds) = match &c.from_flyvis {
        Ok(s) => {
            let unequal = s.centre.differ + s.snapshots.differ;
            let compared = s.n_frames * n_centre + s.snapshot_frames.len() * n;
            (format!("{unequal} unequal"), format!("{unequal} of {compared} unequal"), unequal == 0 && s.n_frames > 0)
        }
        Err(e) => (e.to_string(), e.to_string(), false),
    };
    [
        ControlRow {
            terminal_name: "wrong model (049 vs run 3)",
            record_name: "model 049's parameters against model 000's reference",
            record: wrong.clone(),
            terminal: wrong,
            must: format!("> {CONTROL_MISS:e}"),
            holds: wrong_holds,
        },
        ControlRow {
            terminal_name: "integer counts (run 3)",
            record_name: "the spec's integer counts in place of exp(ln count)",
            record: integer.clone(),
            terminal: integer,
            must: format!("> {F64_BOUND:e}"),
            holds: integer_holds,
        },
        ControlRow {
            terminal_name: "mirrored mapping (run 3, seq 8)",
            record_name: "hexals mirrored (u, v) → (−u, −v), sequence 8",
            terminal: mirrored_t,
            record: mirrored_r,
            must: format!("> {CONTROL_MISS:e}"),
            holds: mirrored_holds,
        },
        ControlRow {
            terminal_name: "flyvis's settled start (run 3, seq 8)",
            record_name: "start from flyvis's settled.f64.bin, sequence 8",
            terminal: flyvis_t,
            record: flyvis_r,
            must: "==".into(),
            holds: flyvis_holds,
        },
    ]
}

// ---------------------------------------------------------------------------
// The terminal table
// ---------------------------------------------------------------------------

fn terminal(r: &Report, data: &Path, seconds: f64, code: u8) -> String {
    let p = &r.provenance;
    let mut s = String::new();
    let _ = writeln!(s, "semblance validate: does this build reproduce flyvis? (epic 0.3.0)");
    let _ = writeln!(s, "data:        {}", data.display());
    let _ = writeln!(
        s,
        "reference:   export {} ({}); flyvis {}, torch {}; {}, {}",
        p.export_id,
        p.exported,
        version(r, "flyvis"),
        version(r, "torch"),
        p.platform,
        p.device
    );
    let models: Vec<String> = p
        .models
        .iter()
        .map(|m| format!("{} {} ({}, validation loss {:.3})", m.id, m.name, m.rank, m.validation_loss))
        .collect();
    let _ = writeln!(s, "models:      {}", models.join(", "));
    let _ = writeln!(
        s,
        "this build:  semblance {}, {}, debug assertions {}, {} threads",
        env!("CARGO_PKG_VERSION"),
        target(),
        on_off(cfg!(debug_assertions)),
        r.threads
    );
    let _ = writeln!(
        s,
        "libm exp:    f32 differs from torch's on {} of {} counts, f64 on {} (fingerprint {})",
        r.libm.f32_differ,
        SYN_COUNT.len(),
        r.libm.f64_differ,
        r.libm
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "{:<10}{:<5}{:<7}{:<7}{:<6}{:<6}{:>10}{:>9}{:>11}  {:<31}{:<11}ratio",
        "stage", "run", "model", "dt", "prec", "counts", "compared", "unequal", "max err", "where", "bound"
    );
    let mut failures = Vec::new();
    let mut worst: Option<(f64, &str, u32)> = None;
    let (mut exact_rows, mut libm_rows) = (0, 0);
    for stage in ["settling", "traces"] {
        for job in &r.jobs {
            let lead = format!(
                "{:<10}{:<5}{:<7}{:<7}{:<6}{:<6}",
                stage,
                job.run,
                job.model,
                job.dt,
                precision(job.precision),
                counts(job.counts)
            );
            let row = match &job.outcome {
                Ok(o) if stage == "settling" => settling_row(r, job, o),
                Ok(o) => trace_row(r, job, &o.sequences.iter().collect::<Vec<_>>()),
                Err(e) => {
                    let _ = writeln!(s, "{lead}  {e}  FAIL");
                    failures.push(format!("{stage} run {} {}", job.run, counts(job.counts)));
                    continue;
                }
            };
            let exact = job.counts == Counts::Torch;
            let (max, place, bound, ratio) = if exact && row.holds {
                ("0".to_string(), "-".to_string(), "==".to_string(), "-".to_string())
            } else if exact {
                (format!("{:.3e}", row.max), row.place.clone(), "==".to_string(), "-".to_string())
            } else {
                (format!("{:.3e}", row.max), row.place.clone(), format!("{:.3e}", row.bound), format!("{:.3e}", row.ratio))
            };
            let fail = if row.holds { "" } else { "  FAIL" };
            let _ = writeln!(
                s,
                "{lead}{:>10}{:>9}{:>11}  {:<31}{:<11}{ratio}{fail}",
                row.compared, row.unequal, max, place, bound
            );
            if exact {
                exact_rows += 1;
            } else {
                libm_rows += 1;
                if worst.is_none_or(|(w, _, _)| row.ratio > w) {
                    worst = Some((row.ratio, stage, job.run));
                }
            }
            if !row.holds {
                failures.push(format!("{stage} run {} {}", job.run, counts(job.counts)));
            }
        }
    }

    let _ = writeln!(s);
    for (i, c) in controls(r).iter().enumerate() {
        let label = if i == 0 { "controls:" } else { "" };
        let must = format!("must be {}", c.must);
        let holds = if c.holds { "holds" } else { "FAIL" };
        let _ = writeln!(s, "{label:<11}{:<39}{:<34}{:<17}{holds}", c.terminal_name, c.terminal, must);
        if !c.holds {
            failures.push(format!("control {}", c.terminal_name));
        }
    }

    let cov = coverage(r);
    let mut lines = Vec::new();
    for (run, c) in &cov {
        if c.is_none() {
            lines.push(format!("run {run} not measured: exact job failed"));
        }
    }
    if let Some((run, k, m)) = overall_weakest(&cov) {
        lines.push(format!(
            "weakest cell type moves {m:.3e} ({}, run {run}); threshold {COVERAGE_THRESHOLD:e} (driven if above), recorded, not a gate:",
            centre_type(r, k)
        ));
        let undriven = undriven(r, &cov);
        if undriven.is_empty() && cov.iter().all(|(_, c)| c.is_some()) {
            lines.push("every type is driven in every run, so Sintel's revisit trigger 1 does not fire;".into());
        } else if undriven.is_empty() {
            lines.push("every type is driven in every run measured;".into());
        } else {
            lines.push(format!("not driven: {}, so Sintel's revisit trigger 1 fires;", undriven.join(", ")));
        }
        let ratios: Vec<String> = cov
            .iter()
            .map(|(_, c)| match c.as_ref().and_then(|c| c.bound.map(|b| c.weakest() / b)) {
                Some(x) => format!("{x:.3e}"),
                None => "-".into(),
            })
            .collect();
        let runs = if r.complete() {
            "0-7".to_string()
        } else {
            r.runs.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
        };
        lines.push(format!("weakest / libm centre bound, runs {runs}: {}", ratios.join(" ")));
    }
    for (i, l) in lines.iter().enumerate() {
        let label = if i == 0 { "coverage:" } else { "" };
        let _ = writeln!(s, "{label:<11}{l}");
    }

    let verdict = match code {
        0 => {
            let (w, stage, run) = worst.unwrap_or((0.0, "-", 0));
            format!(
                "PASS — {exact_rows} exact rows bit-equal, {libm_rows} libm rows within bounds (worst {w:.3e}, {stage} run {run}), 4 controls hold"
            )
        }
        1 => format!("FAIL — {}", if failures.is_empty() { "not every run was checked".into() } else { failures.join(", ") }),
        _ => format!("NOT VERIFIED — a fixture did not fit; nothing counts as verified: {}", failures.join(", ")),
    };
    let _ = writeln!(s, "verdict:   {verdict}  [{seconds:.1} s]");
    s
}

// ---------------------------------------------------------------------------
// The record's generated block
// ---------------------------------------------------------------------------

fn markdown(r: &Report) -> String {
    let p = &r.provenance;
    let mut s = String::new();
    let _ = writeln!(s, "{BEGIN}");
    let _ = writeln!(s, "## Reference (flyvis side)");
    let _ = writeln!(s, "| | |\n|---|---|");
    let _ = writeln!(s, "| export | {}, exported {} |", p.export_id, p.exported);
    let _ = writeln!(
        s,
        "| exporter | {}, sha256 {}; git {} ({}); uv.lock sha256 {} |",
        p.generator_path,
        p.generator_sha256,
        p.git_head,
        if p.git_dirty { "dirty" } else { "clean" },
        p.uv_lock_sha256
    );
    let versions: Vec<String> = p.versions.iter().map(|(k, v)| format!("{k} {v}")).collect();
    let _ = writeln!(s, "| versions | {} |", versions.join(", "));
    let join = |v: &[u32]| v.iter().map(u32::to_string).collect::<Vec<_>>().join(" and ");
    let _ = writeln!(
        s,
        "| machine | {} ({}), device {}; default {} threads, runs {} bit-equal at {} |",
        p.platform,
        p.machine,
        p.device,
        p.threads_default,
        join(&p.threads_runs_checked),
        join(&p.threads_checked)
    );
    let models: Vec<String> = p
        .models
        .iter()
        .map(|m| {
            format!(
                "{} {}, {}, validation loss {}, checkpoint sha256 {}",
                m.id, m.name, m.rank, m.validation_loss, m.chkpt_00000_sha256
            )
        })
        .collect();
    let _ = writeln!(s, "| models | {} |", models.join("; "));
    let _ = writeln!(s, "| parameters | all 50 checkpoints equal ensemble.csv |");
    let seqs: Vec<String> = p.equivalence_sequences.iter().map(u32::to_string).collect();
    let _ = writeln!(
        s,
        "| equivalence | the export's forward path equals stimulus_response bit for bit (sequences {}) |",
        seqs.join(", ")
    );
    let rounding = match p.input_rounding.as_slice() {
        [first, rest @ ..] if rest.iter().all(|x| x.1.to_bits() == first.1.to_bits()) => {
            let ids: Vec<&str> = p.input_rounding.iter().map(|x| x.0.as_str()).collect();
            format!("{:e} at {}", first.1, ids.join(" and "))
        }
        all => all.iter().map(|(dt, v)| format!("{v:e} at {dt}")).collect::<Vec<_>>().join("; "),
    };
    let _ = writeln!(s, "| input rounding | {rounding} (the f32 runs' moving edges) |");
    let _ = writeln!(
        s,
        "| files | {} reference files, each the size the manifest and PROVENANCE give |",
        p.sizes.len()
    );
    let _ = writeln!(s);

    let _ = writeln!(s, "<!-- BEGIN host-specific -->");
    let _ = writeln!(s, "## This run (Rust side)");
    let _ = writeln!(s, "| | |\n|---|---|");
    let _ = writeln!(s, "| semblance | {} |", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(s, "| target | {} |", target());
    let _ = writeln!(s, "| debug assertions | {} |", on_off(cfg!(debug_assertions)));
    let _ = writeln!(s, "| threads | {} |", r.threads);
    let _ = writeln!(
        s,
        "| libm exp | f32: {} of {} counts differ from torch's; f64: {} |",
        r.libm.f32_differ,
        SYN_COUNT.len(),
        r.libm.f64_differ
    );
    let _ = writeln!(s, "| libm fingerprint | {} |", r.libm);
    let _ = writeln!(s, "<!-- END host-specific -->");
    let _ = writeln!(s);

    let _ = writeln!(s, "## Bounds as applied");
    let _ = writeln!(s, "| run | quantity | bound | derivation |\n|---|---|---|---|");
    let _ = writeln!(
        s,
        "| all | every quantity, exact | == | torch's counts (syn_count_torch.rs); every value must equal flyvis's |"
    );
    for job in r.jobs.iter().filter(|j| j.counts == Counts::Libm) {
        let bounds = [job.settling_bounds.0, job.settling_bounds.1, job.trace_bounds.0, job.trace_bounds.1];
        if bounds.iter().all(|b| *b == Bound::F64) {
            let _ = writeln!(s, "| {} | every quantity, libm | {:e} | the f64 bound |", job.run, F64_BOUND);
            continue;
        }
        for b in bounds {
            match b {
                Bound::F32 { gap, model, dt, quantity } => {
                    let _ = writeln!(
                        s,
                        "| {} | {}, libm | {:e} | 2 × f32_f64_gap[\"{model} {dt}\"].{quantity} = 2 × {gap:e} |",
                        job.run,
                        quantity.replace('_', " "),
                        b.value()
                    );
                }
                other => {
                    let _ = writeln!(s, "| {} | libm | {:e} | {other:?} |", job.run, other.value());
                }
            }
        }
    }
    let _ = writeln!(s);

    let exact: Vec<&Job> = r.jobs.iter().filter(|j| j.counts == Counts::Torch).collect();
    let libm: Vec<&Job> = r.jobs.iter().filter(|j| j.counts == Counts::Libm).collect();
    let lead = |j: &Job| format!("| {} | {} | {} | {} |", j.run, model(r, &j.model), j.dt, precision(j.precision));

    let _ = writeln!(s, "## Stage 1: settling, exact (torch's counts)");
    let _ = writeln!(
        s,
        "| run | model | dt | precision | quantity | compared | unequal | max error |\n|---|---|---|---|---|---|---|---|"
    );
    for j in &exact {
        match &j.outcome {
            Ok(o) => {
                let st = &o.settling;
                for (q, d, n) in [("settled", st.settled, st.settled_len), ("settling centre", st.settling_centre, st.centre_len)] {
                    let _ = writeln!(s, "{} {q} | {n} | {} | {} |", lead(j), d.differ, max_text(d.max));
                }
            }
            Err(e) => {
                let _ = writeln!(s, "{} not run: {e} | | | |", lead(j));
            }
        }
    }
    let _ = writeln!(s);

    let n = r.node_cell_type.len();
    let n_centre = r.centre_nodes.len();
    let _ = writeln!(s, "## Stage 2: traces, exact (torch's counts), per stimulus");
    let _ = writeln!(
        s,
        "| run | model | dt | precision | stimulus | sequences | quantity | compared | unequal | max error |\n\
         |---|---|---|---|---|---|---|---|---|---|"
    );
    for j in &exact {
        match &j.outcome {
            Ok(o) => {
                for (family, qs) in families(o) {
                    let range = sequence_range(&qs);
                    for (q, compared, d) in group(&qs, n, n_centre) {
                        let _ = writeln!(
                            s,
                            "{} {} | {range} | {q} | {compared} | {} | {} |",
                            lead(j),
                            family_name(&family),
                            d.differ,
                            max_text(d.max)
                        );
                    }
                }
            }
            Err(e) => {
                let _ = writeln!(s, "{} not run: {e} | | | | | |", lead(j));
            }
        }
    }
    let _ = writeln!(s);

    let _ = writeln!(s, "<!-- BEGIN libm fingerprint={} -->", r.libm);
    let _ = writeln!(s, "## Stage 1: settling, libm (Rust's exp)");
    let _ = writeln!(
        s,
        "| run | model | dt | precision | quantity | compared | unequal | max error | where | bound | ratio | worst types |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for j in &libm {
        match &j.outcome {
            Ok(o) => {
                let st = &o.settling;
                let worst: Vec<String> = st
                    .worst_types(3)
                    .iter()
                    .map(|&t| format!("{} {:.2e}", r.cell_types[t], st.per_type_max[t]))
                    .collect();
                let (sb, cb) = j.settling_bounds;
                let _ = writeln!(
                    s,
                    "{} settled | {} | {} | {:.3e} | node {} ({}) | {:.3e} | {:.3e} | {} |",
                    lead(j),
                    st.settled_len,
                    st.settled.differ,
                    st.settled.max,
                    st.settled.at,
                    r.cell_type(st.settled.at),
                    sb.value(),
                    ratio(st.settled.max, &sb),
                    worst.join(", ")
                );
                let c = st.settling_centre;
                let _ = writeln!(
                    s,
                    "{} settling centre | {} | {} | {:.3e} | step {} ({}) | {:.3e} | {:.3e} | |",
                    lead(j),
                    st.centre_len,
                    c.differ,
                    c.max,
                    c.at / n_centre,
                    centre_type(r, c.at % n_centre),
                    cb.value(),
                    ratio(c.max, &cb)
                );
            }
            Err(e) => {
                let _ = writeln!(s, "{} not run: {e} | | | | | | | |", lead(j));
            }
        }
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "## Stage 2: traces, libm (Rust's exp), per stimulus");
    let _ = writeln!(
        s,
        "| run | model | dt | precision | stimulus | quantity | compared | unequal | max error | where | bound | ratio |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for j in &libm {
        match &j.outcome {
            Ok(o) => {
                for (family, qs) in families(o) {
                    for (quantity, bound) in [("centre", j.trace_bounds.0), ("snapshots", j.trace_bounds.1)] {
                        let pick = |q: &SequenceResult| if quantity == "centre" { q.centre } else { q.snapshots };
                        let compared: usize = qs
                            .iter()
                            .map(|q| if quantity == "centre" { q.n_frames * n_centre } else { q.snapshot_frames.len() * n })
                            .sum();
                        let unequal: usize = qs.iter().map(|q| pick(q).differ).sum();
                        let mut at = qs[0];
                        for q in &qs {
                            if pick(q).max.total_cmp(&pick(at).max).is_ge() {
                                at = q;
                            }
                        }
                        let max = pick(at).max;
                        let place =
                            if quantity == "centre" { centre_place(r, at, true) } else { snapshot_place(r, at, true) };
                        let _ = writeln!(
                            s,
                            "{} {} | {quantity} | {compared} | {unequal} | {max:.3e} | {place} | {:.3e} | {:.3e} |",
                            lead(j),
                            family_name(&family),
                            bound.value(),
                            ratio(max, &bound)
                        );
                    }
                }
            }
            Err(e) => {
                let _ = writeln!(s, "{} not run: {e} | | | | | | | |", lead(j));
            }
        }
    }
    let _ = writeln!(s, "<!-- END libm -->");
    let _ = writeln!(s);

    let _ = writeln!(s, "## Controls");
    let _ = writeln!(s, "| control | run | measured | must be | holds |\n|---|---|---|---|---|");
    for c in controls(r) {
        let _ = writeln!(s, "| {} | 3 | {} | {} | {} |", c.record_name, c.record, c.must, if c.holds { "yes" } else { "no" });
    }
    let _ = writeln!(s);

    let cov = coverage(r);
    let _ = writeln!(s, "## Coverage: Sintel revisit trigger 1");
    let _ = writeln!(
        s,
        "Movement of a cell type = max over the 12 sequences and every frame of |centre trace − settled centre|, \
         widened to f64, from the exact jobs (equal to flyvis's where the exact job passed). A type is driven if its \
         movement exceeds {COVERAGE_THRESHOLD:e}: recorded, not a gate."
    );
    let _ = writeln!(
        s,
        "| run | weakest type | movement | next two | median | libm centre bound | weakest / bound |\n\
         |---|---|---|---|---|---|---|"
    );
    for (run, c) in &cov {
        match c {
            Some(c) => {
                let k = c.order[0];
                let next: Vec<String> =
                    c.order[1..3.min(c.order.len())].iter().map(|&t| format!("{} {:.3e}", centre_type(r, t), c.movement[t])).collect();
                let (bound, ratio) = match c.bound {
                    Some(b) => (format!("{b:.3e}"), format!("{:.3e}", c.weakest() / b)),
                    None => ("-".into(), "-".into()),
                };
                let _ = writeln!(
                    s,
                    "| {run} | {} | {:.3e} | {} | {:.3e} | {bound} | {ratio} |",
                    centre_type(r, k),
                    c.weakest(),
                    next.join(", "),
                    c.median()
                );
            }
            None => {
                let _ = writeln!(s, "| {run} | not measured: exact job failed | | | | | |");
            }
        }
    }
    let _ = writeln!(s);
    let unmeasured: Vec<String> = cov.iter().filter(|(_, c)| c.is_none()).map(|(r, _)| r.to_string()).collect();
    let undriven = undriven(r, &cov);
    let sentence = match overall_weakest(&cov) {
        _ if !unmeasured.is_empty() => format!(
            "Runs {} were not measured, because their exact jobs failed, so this does not show that every type is \
             driven. This covers the centre column only.",
            unmeasured.join(", ")
        ),
        Some((run, k, m)) if undriven.is_empty() => format!(
            "Every one of the {} cell types moves by more than {COVERAGE_THRESHOLD:e} in every run (the weakest, {} in \
             run {run}, by {m:.3e}): no type is undriven by flashes and moving edges, and trigger 1 does not fire. \
             This covers the centre column only.",
            r.centre_nodes.len(),
            centre_type(r, k)
        ),
        Some(_) => format!(
            "Not driven (moving by no more than {COVERAGE_THRESHOLD:e}): {}. Trigger 1 fires. This covers the centre \
             column only.",
            undriven.join(", ")
        ),
        None => "No run was measured. This covers the centre column only.".to_string(),
    };
    let _ = writeln!(s, "{sentence}");
    let _ = writeln!(s, "{END}");
    s
}

/// A family's centre and snapshot totals: (quantity, compared, combined Diff).
fn group(qs: &[&SequenceResult], n: usize, n_centre: usize) -> [(&'static str, usize, Diff); 2] {
    let fold = |pick: fn(&SequenceResult) -> Diff| {
        qs.iter().fold(Diff { max: 0.0, at: 0, differ: 0 }, |acc, q| {
            let d = pick(q);
            Diff { max: if d.max > acc.max { d.max } else { acc.max }, at: 0, differ: acc.differ + d.differ }
        })
    };
    [
        ("centre", qs.iter().map(|q| q.n_frames * n_centre).sum(), fold(|q| q.centre)),
        ("snapshots", qs.iter().map(|q| q.snapshot_frames.len() * n).sum(), fold(|q| q.snapshots)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flags_parse_once_each() {
        assert_eq!(parse(&args(&[])), Ok(Args { data: PathBuf::from("./data"), threads: None, markdown: false }));
        assert_eq!(
            parse(&args(&["--markdown", "--threads", "4", "--data", "d"])),
            Ok(Args { data: PathBuf::from("d"), threads: Some(4), markdown: true })
        );
        for bad in [
            &["--threads", "0"][..],
            &["--threads", "x"],
            &["--threads", "+3"],
            &["--threads"],
            &["--data"],
            &["--data", "a", "--data", "b"],
            &["--markdown", "--markdown"],
            &["--bogus"],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
    }

    struct Pipe {
        writes: usize,
        kind: io::ErrorKind,
    }

    impl Write for Pipe {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.writes == 0 {
                return Err(io::Error::new(self.kind, "closed"));
            }
            self.writes -= 1;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// `validate | head`: a closed pipe stops the output and keeps the code.
    #[test]
    fn a_broken_pipe_keeps_the_exit_code_and_does_not_panic() {
        let text = "a\nb\nc\n";
        for code in [0, 1] {
            assert_eq!(emit(&mut Pipe { writes: 1, kind: io::ErrorKind::BrokenPipe }, text, code), code);
        }
        assert_eq!(emit(&mut Pipe { writes: 1, kind: io::ErrorKind::Other }, text, 0), 2);
        assert_eq!(emit(&mut Pipe { writes: 100, kind: io::ErrorKind::Other }, text, 1), 1);
    }

    #[test]
    fn the_missing_message_lists_every_absent_file() {
        let m = missing_message(Path::new("./data"), &["flyvis-pretrained/ensemble.csv", "flyvis-reference/manifest.json"]);
        assert_eq!(
            m,
            "validate: nothing was verified; 2 fixture files are missing:\n  \
             ./data/flyvis-pretrained/ensemble.csv\n  ./data/flyvis-reference/manifest.json\n\
             Get them with ci/fetch-fixtures.sh (needs gh), or see tools/README.md.\n\
             The result recorded for 0.3.0 is docs/results/0.3.0-flyvis-parity.md; it is not a check of this machine.\n"
        );
        let one = missing_message(Path::new("d"), &["flyvis-pretrained/ensemble.csv"]);
        assert!(one.starts_with("validate: nothing was verified; 1 fixture file is missing:\n  d/flyvis-pretrained/ensemble.csv\n"));
        assert!(one.contains(parity::how_to_get("flyvis-pretrained/ensemble.csv")));
    }
}
```

- [ ] **Step 5: Wire it in, with the new help text and module doc.**

In `app/src/main.rs`, replace:

```rust
//! confirm the whole thing works before touching the research logic.
//!
//! Subcommands (all run on a synthetic connectome unless you point them at data):
//!   demo       run the simulator once, print a behaviour summary
//!   roundtrip  encode -> decode with the lossless WeightCodec, print fidelity
//!   evolve     seed from the connectome's latent, run N generations of a toy
//!              fitness, print best-fitness per generation
//!
//! Arg parsing is hand-rolled (std only) to keep the dependency tree tiny. Swap in
//! `clap` if/when you want richer flags.

use std::env;
```

with:

```rust
//! confirm the whole thing works before touching the research logic.
//!
//! Subcommands:
//!   demo       run the simulator once, print a behaviour summary
//!   roundtrip  encode -> decode with the lossless WeightCodec, print fidelity
//!   evolve     seed from the connectome's latent, run N generations of a toy
//!              fitness, print best-fitness per generation
//!   validate   check that this build reproduces flyvis bit for bit, from the
//!              reference fixtures under data/ (see `validate.rs`)
//!
//! demo, roundtrip and evolve run on a synthetic connectome. Arg parsing is
//! hand-rolled: std-only by design; the workspace has no external
//! dependencies.

mod validate;

use std::env;
```

In `app/src/main.rs`, replace:

```rust
        "roundtrip" => cmd_roundtrip(),
        "evolve" => cmd_evolve(),
        "help" | "-h" | "--help" => {
            print_help();
```

with:

```rust
        "roundtrip" => cmd_roundtrip(),
        "evolve" => cmd_evolve(),
        "validate" => validate::cmd(&args[2..]),
        "help" | "-h" | "--help" => {
            print_help();
```

In `app/src/main.rs`, replace:

```rust
fn print_help() {
    println!(
        "semblance — connectome compress / evolve scaffold\n\n\
         USAGE:\n  semblance <command>\n\n\
         COMMANDS:\n  \
         demo        run the LIF simulator once on a synthetic connectome\n  \
         roundtrip   lossless encode->decode, report structural + behavioural fidelity\n  \
         evolve      seed from the connectome latent and run a toy evolution loop\n  \
         help        show this message\n\n\
         Everything runs on a synthetic graph so it works with no data. See\n\
         RESOURCES.md for how to load a real FlyWire / MaleCNS export instead."
    );
}
```

with:

```rust
fn print_help() {
    println!(
        "semblance — connectome compression research: flyvis in Rust\n\n\
         USAGE:\n  semblance <command> [options]\n\n\
         COMMANDS:\n  \
         demo        run the LIF simulator once on a synthetic connectome\n  \
         roundtrip   lossless encode->decode, report structural + behavioural fidelity\n  \
         evolve      seed from the connectome latent and run a toy evolution loop\n  \
         validate    check that this build reproduces flyvis bit for bit\n              \
         [--data DIR] [--threads N] [--markdown]\n  \
         help        show this message\n\n\
         demo, roundtrip and evolve run on a synthetic graph and need no data.\n\
         validate needs flyvis's reference fixtures under data/ (get them with\n\
         ci/fetch-fixtures.sh); the recorded result is docs/results/0.3.0-flyvis-parity.md."
    );
}
```

- [ ] **Step 6: Run the tests, then the command.**

  Run: `cargo test -p semblance`
  Expected: 8 passed (3 unit tests in `validate.rs`, 5 in `validate_cli`).

  Run: `time cargo run --release -- validate; echo "exit $?"`
  Expected: exit 0, about 10 s on 16 threads, printing the following (the
  libm numbers, the thread count and the time are this Mac's; the version
  reads `semblance 0.2.0` until task 7):

```text
semblance validate: does this build reproduce flyvis? (epic 0.3.0)
data:        ./data
reference:   export 20261027T114358Z-63037 (2026-10-27); flyvis 1.2.0, torch 2.14.0; macOS-26.6.2-arm64-arm-64bit, cpu
models:      000 flow/0000/000 (best, validation loss 5.137), 049 flow/0000/049 (worst, validation loss 5.678)
this build:  semblance 0.2.0, aarch64-macos, debug assertions off, 16 threads
libm exp:    f32 differs from torch's on 106 of 2355 counts, f64 on 108 (fingerprint f32:106:55fdb5ef0c7a79a5,f64:108:8970802a716917ba)

stage     run  model  dt     prec  counts  compared  unequal    max err  where                          bound      ratio
settling  0    000    1/200  f32   exact      58669        0          0  -                              ==         -
settling  0    000    1/200  f32   libm       58669    27442   4.768e-7  centre, step 3 (TmY4)          2.695e-6   1.769e-1
settling  1    000    1/200  f64   exact      58669        0          0  -                              ==         -
settling  1    000    1/200  f64   libm       58669    28858  1.110e-15  settled, node 16054 (Mi4)      1.000e-10  1.110e-5
settling  2    000    1/50   f32   exact      48919        0          0  -                              ==         -
settling  2    000    1/50   f32   libm       48919    27212   8.345e-7  settled, node 15739 (Mi4)      2.553e-6   3.268e-1
settling  3    000    1/50   f64   exact      48919        0          0  -                              ==         -
settling  3    000    1/50   f64   libm       48919    27785  2.220e-15  settled, node 15993 (Mi4)      1.000e-10  2.220e-5
settling  4    049    1/200  f32   exact      58669        0          0  -                              ==         -
settling  4    049    1/200  f32   libm       58669    14981   3.710e-6  settled, node 16134 (Mi9)      1.232e-5   3.013e-1
settling  5    049    1/200  f64   exact      58669        0          0  -                              ==         -
settling  5    049    1/200  f64   libm       58669    19070  9.409e-15  settled, node 16134 (Mi9)      1.000e-10  9.409e-5
settling  6    049    1/50   f32   exact      48919        0          0  -                              ==         -
settling  6    049    1/50   f32   libm       48919    12232   1.669e-6  settled, node 43787 (TmY14)    1.289e-5   1.295e-1
settling  7    049    1/50   f64   exact      48919        0          0  -                              ==         -
settling  7    049    1/50   f64   libm       48919    16980  7.994e-15  settled, node 44066 (TmY14)    1.000e-10  7.994e-5
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

controls:  wrong model (049 vs run 3)             settled max 6.397e0               must be > 1e-3   holds
           integer counts (run 3)                 settled max 4.963e-7              must be > 1e-10  holds
           mirrored mapping (run 3, seq 8)        snapshots max 2.503223408264881e0 must be > 1e-3   holds
           flyvis's settled start (run 3, seq 8)  0 unequal                         must be ==       holds
coverage:  weakest cell type moves 3.773e-3 (Tm30, run 3); threshold 1e-3 (driven if above), recorded, not a gate:
           every type is driven in every run, so Sintel's revisit trigger 1 does not fire;
           weakest / libm centre bound, runs 0-7: 1.102e3 3.801e7 7.439e2 3.773e7 6.245e2 3.248e8 4.618e2 3.179e8
verdict:   PASS — 16 exact rows bit-equal, 16 libm rows within bounds (worst 8.605e-1, traces run 4), 4 controls hold  [9.4 s]
```

  Run: `cargo run --release -- validate --markdown > target/validate.md; echo "exit $?"`
  Expected: exit 0; the block starts with
  `<!-- BEGIN semblance validate --markdown -->`, ends with
  `<!-- END semblance validate --markdown -->`, its coverage table ends with
  the row `| 7 | Tm30 | 3.179e-2 | L2 4.600e-2, Tm28 4.869e-2 | 8.958e-1 | 1.000e-10 | 3.179e8 |`,
  and its controls table includes
  `| hexals mirrored (u, v) → (−u, −v), sequence 8 | 3 | snapshots max 2.503223408264881e0, centre max 2.291526948382731e0 | > 1e-3 | yes |`.

  Run: `bash -c 'cargo run --quiet --release -- validate | head -2; echo "exit ${PIPESTATUS[0]}"'`
  Expected: two lines, then `exit 0` (no panic on the closed pipe).

- [ ] **Step 7: Clippy, MSRV and the gate.** As in Global Constraints.
  Expected: clean; 0 failed (472 passed after waves 1 and 2 plus this task).

- [ ] **Step 8: Commit.**

```bash
git add app/src/main.rs \
  app/src/validate.rs \
  app/Cargo.toml \
  Cargo.lock \
  app/tests/validate_cli.rs
git commit -m "Add semblance validate" \
  -m "Temper-Task: parity-validate/cli"
```

### Task 7: Bump to 0.3.0 (wave 4, id `bump`)

**Wave:** 4

Spec §9 "Version", F16. Depends on tasks 5 and 6.

**Files:**
- Modify: `Cargo.toml` (`[workspace.package] version`, the seven `[workspace.dependencies]` versions)
- Modify: `Cargo.lock` (regenerated by `cargo build`)

**Interfaces:**
- Consumes: nothing.
- Produces: every crate at `0.3.0`; `env!("CARGO_PKG_VERSION")` in `app` reads
  `0.3.0` (the terminal's `this build:` line and the record's host section).

- [ ] **Step 1: See the old version.**

  Run: `grep -A1 '^name = ' Cargo.lock | grep -c 'version = "0.2.0"'`
  Expected: `8`.

- [ ] **Step 2: Bump.**

In `Cargo.toml`, replace:

```toml

[workspace.package]
version = "0.2.0"
edition = "2024"
license = "Apache-2.0"
```

with:

```toml

[workspace.package]
version = "0.3.0"
edition = "2024"
license = "Apache-2.0"
```

In `Cargo.toml`, replace:

```toml
# Shared dependency versions. Individual crates opt in under [dependencies].
[workspace.dependencies]
semblance-rng = { path = "crates/rng", version = "0.2.0" }
semblance-flyvis = { path = "crates/flyvis", version = "0.2.0" }
semblance-connectome = { path = "crates/connectome", version = "0.2.0" }
semblance-sim = { path = "crates/sim", version = "0.2.0" }
semblance-codec = { path = "crates/codec", version = "0.2.0" }
semblance-evo = { path = "crates/evo", version = "0.2.0" }
semblance-fidelity = { path = "crates/fidelity", version = "0.2.0" }

# A release profile tuned for the tick loop / evolution inner loops.
```

with:

```toml
# Shared dependency versions. Individual crates opt in under [dependencies].
[workspace.dependencies]
semblance-rng = { path = "crates/rng", version = "0.3.0" }
semblance-flyvis = { path = "crates/flyvis", version = "0.3.0" }
semblance-connectome = { path = "crates/connectome", version = "0.3.0" }
semblance-sim = { path = "crates/sim", version = "0.3.0" }
semblance-codec = { path = "crates/codec", version = "0.3.0" }
semblance-evo = { path = "crates/evo", version = "0.3.0" }
semblance-fidelity = { path = "crates/fidelity", version = "0.3.0" }

# A release profile tuned for the tick loop / evolution inner loops.
```

  Run: `cargo build --workspace`, then
  `grep -c '^name = ' Cargo.lock; grep -A1 '^name = ' Cargo.lock | grep -c 'version = "0.3.0"'`
  Expected: `8` and `8`. `git diff Cargo.lock` changes exactly the eight
  `version` lines, nothing else.

- [ ] **Step 3: Commit, then the release checks on the clean tree.**

```bash
git add Cargo.toml \
  Cargo.lock
git commit -m "Bump the workspace to 0.3.0" \
  -m "Temper-Task: parity-validate/bump"
```

  Run: `rm -rf target/package && cargo package --workspace && for c in target/package/*.crate; do tar tzf "$c"; done | grep -cE '/(LICENSE|NOTICE)$'`
  Expected: eight `semblance*-0.3.0.crate` files and `16`. (The `rm -rf`
  matters: stale 0.2.0 packages would double the count.)

  Run: `cargo publish --workspace --dry-run; echo "exit $?"`
  Expected: `Uploading` for rng, connectome, codec, sim, evo, fidelity,
  flyvis, semblance (in that order), each followed by
  `warning: aborting upload due to dry run`; `exit 0`.

- [ ] **Step 4: The gate.** As in Global Constraints. Expected: 0 failed;
  `cargo run --quiet -- validate | sed -n 5p` begins
  `this build:  semblance 0.3.0,`.

### Task 8: The CI record step (wave 4, id `record-step`)

**Wave:** 4

Spec §8 "The record step", R9, F20. Depends on task 6.

**Files:**
- Modify: `.github/workflows/ci.yml` (the validate step, the upload, `SEMBLANCE_REQUIRE_LIBM_RECORD`)

**Interfaces:**
- Consumes: `target/debug/semblance validate --markdown` (task 6).
- Produces: the artifact `parity-record` (`parity-record.md`) on every
  hosted run of the `test` job; the test step runs with
  `SEMBLANCE_REQUIRE_LIBM_RECORD=1`.

`actions/upload-artifact` is GitHub-owned and pinned to the commit both its
`v4` and `v4.6.2` tags resolve to (F20). No other action is added.

- [ ] **Step 1: The steps, after `Build` and before the test step.**

In `.github/workflows/ci.yml`, replace:

```yaml
        run: cargo build --workspace

      - name: Test, with every fixture required
        env:
          SEMBLANCE_REQUIRE_FIXTURES: "1"
        run: cargo test --workspace

```

with:

```yaml
        run: cargo build --workspace

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

      # SEMBLANCE_REQUIRE_LIBM_RECORD: this host recorded the record, so its
      # libm section must match byte for byte, not be skipped.
      - name: Test, with every fixture required
        env:
          SEMBLANCE_REQUIRE_FIXTURES: "1"
          SEMBLANCE_REQUIRE_LIBM_RECORD: "1"
        run: cargo test --workspace

```

- [ ] **Step 2: Check the workflow locally as far as possible.**

  Run: `grep -n 'parity-record\|SEMBLANCE_REQUIRE_LIBM_RECORD\|upload-artifact@' .github/workflows/ci.yml`
  Expected: the step name, `name: parity-record`, `path: parity-record.md`,
  the env line, and `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2`.
  Then `cargo build --workspace && target/debug/semblance validate --markdown > target/parity-record.md; echo "exit $?"`
  prints `exit 0` (what the step runs). The step itself is checked on the
  next hosted run.

- [ ] **Step 3: Commit.**

```bash
git add .github/workflows/ci.yml
git commit -m "Produce the parity record's generated block on CI" \
  -m "Temper-Task: parity-validate/record-step"
```

### Task 9: The record test and the preamble (wave 5, id `record-test`)

**Wave:** 5

Spec §7 (the preamble, the numeral rule R22, the honesty test), R2, R9,
R16. Depends on tasks 6 and 8. **No agent writes a generated block into the
record**: that is the head's step, from CI's artifact.

**Files:**
- Create: `app/tests/record.rs`
- Create: `docs/results/0.3.0-flyvis-parity.md` (the preamble only)

**Interfaces:**
- Consumes: `env!("CARGO_BIN_EXE_semblance")`, task 6's Markdown markers,
  `semblance_flyvis::parity::{required_files, how_to_get}`.
- Produces: `the_committed_record_is_what_validate_prints`, and five unit
  tests of its text functions on synthetic blocks.

- [ ] **Step 1: Write the test.**

Create `app/tests/record.rs` with:

```rust
//! The parity record, docs/results/0.3.0-flyvis-parity.md, is a hand-written
//! preamble followed by what `semblance validate --markdown` printed on
//! hosted CI. This test regenerates that output and compares it with the
//! committed file (spec
//! docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md,
//! section 7, R9 and R22).
//!
//! Never compared: the host-specific section. Compared only on a host whose
//! libm fingerprint is the record's: the libm section. Under
//! `SEMBLANCE_REQUIRE_LIBM_RECORD=1` (CI, the recording host) a different
//! fingerprint is a failure; elsewhere it replaces the libm section with one
//! placeholder line in both texts and prints why, visible under
//! `--nocapture`.
//!
//! The record is regenerated only from CI's `parity-record` artifact, never
//! by editing numbers here or on a workstation. This test spawns the binary
//! under test and nothing else; it reads no git history.

use std::collections::HashSet;
use std::process::Command;

const RECORD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/results/0.3.0-flyvis-parity.md");
const BEGIN: &str = "<!-- BEGIN semblance validate --markdown -->";
const END: &str = "<!-- END semblance validate --markdown -->";
const HOST_BEGIN: &str = "<!-- BEGIN host-specific -->";
const HOST_END: &str = "<!-- END host-specific -->";
const LIBM_BEGIN: &str = "<!-- BEGIN libm fingerprint=";
const LIBM_END: &str = "<!-- END libm -->";
const PLACEHOLDER: &str = "<!-- libm sections not compared -->";

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1")
}

/// The byte offset of `marker`'s one occurrence in `text`.
fn once(text: &str, marker: &str, what: &str) -> Result<usize, String> {
    let mut found = text.match_indices(marker).map(|(i, _)| i);
    match (found.next(), found.next()) {
        (Some(i), None) => Ok(i),
        (None, _) => Err(format!("{what}: no {marker}")),
        (Some(_), Some(_)) => Err(format!("{what}: {marker} more than once")),
    }
}

/// (the preamble, the generated block from BEGIN to END inclusive).
fn split<'a>(text: &'a str, what: &str) -> Result<(&'a str, &'a str), String> {
    let (b, e) = (once(text, BEGIN, what)?, once(text, END, what)?);
    if e < b {
        return Err(format!("{what}: {END} comes before {BEGIN}"));
    }
    Ok((&text[..b], &text[b..e + END.len()]))
}

/// `f32:{n}:{16 hex digits},f64:{n}:{16 hex digits}`.
fn parse_fingerprint(s: &str) -> Result<(), String> {
    let part = |p: &str, name: &str| -> bool {
        let mut f = p.splitn(3, ':');
        f.next() == Some(name)
            && f.next().is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            && f.next().is_some_and(|h| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    match s.split_once(',') {
        Some((a, b)) if part(a, "f32") && part(b, "f64") => Ok(()),
        _ => Err(format!("the libm fingerprint {s:?} does not parse")),
    }
}

/// Checks a generated block's markers and returns its libm fingerprint:
/// one host-specific section, one libm section whose fingerprint parses and
/// equals the host section's `libm fingerprint` row.
fn check_block(block: &str, what: &str) -> Result<String, String> {
    let (hb, he) = (once(block, HOST_BEGIN, what)?, once(block, HOST_END, what)?);
    let (lb, le) = (once(block, LIBM_BEGIN, what)?, once(block, LIBM_END, what)?);
    if he < hb || le < lb {
        return Err(format!("{what}: a section ends before it begins"));
    }
    let start = lb + LIBM_BEGIN.len();
    let len = block[start..].find(" -->").ok_or_else(|| format!("{what}: the libm marker does not close"))?;
    let fingerprint = &block[start..start + len];
    parse_fingerprint(fingerprint).map_err(|e| format!("{what}: {e}"))?;
    let host = block[hb..he]
        .lines()
        .find_map(|l| l.strip_prefix("| libm fingerprint | ").and_then(|r| r.strip_suffix(" |")))
        .ok_or_else(|| format!("{what}: the host section has no libm fingerprint row"))?;
    if host != fingerprint {
        return Err(format!("{what}: the libm marker's fingerprint {fingerprint} differs from the host section's {host}"));
    }
    Ok(fingerprint.to_string())
}

/// `block` without its host-specific section, markers and line included.
fn remove_host(block: &str) -> String {
    match (block.find(HOST_BEGIN), block.find(HOST_END)) {
        (Some(b), Some(e)) if b < e => {
            let end = e + HOST_END.len() + usize::from(block[e + HOST_END.len()..].starts_with('\n'));
            format!("{}{}", &block[..b], &block[end..])
        }
        _ => block.to_string(),
    }
}

/// `block` with its libm section, markers included, replaced by one line.
fn replace_libm(block: &str) -> String {
    match (block.find(LIBM_BEGIN), block.find(LIBM_END)) {
        (Some(b), Some(e)) if b < e => format!("{}{PLACEHOLDER}{}", &block[..b], &block[e + LIBM_END.len()..]),
        _ => block.to_string(),
    }
}

#[derive(Debug, PartialEq)]
enum Libm {
    Compare,
    Skip,
    Fail(String),
}

/// What to do with the libm sections: a pure function of the two
/// fingerprints and `SEMBLANCE_REQUIRE_LIBM_RECORD`.
fn libm_decision(here: &str, record: &str, require: bool) -> Libm {
    if here == record {
        Libm::Compare
    } else if require {
        Libm::Fail(format!(
            "SEMBLANCE_REQUIRE_LIBM_RECORD=1 but this host's libm fingerprint {here} differs from the record's {record}"
        ))
    } else {
        Libm::Skip
    }
}

/// R22's numeral tokens: maximal runs of `0-9 . e + -` that start with a
/// digit and contain a `.` or an `e` or three or more digits. Trailing `.`,
/// `+` and `-` are punctuation, not part of the number. Tokens with two or
/// more `.` (versions) are exempt.
fn numerals(text: &str) -> Vec<String> {
    let in_class = |c: char| c.is_ascii_digit() || matches!(c, '.' | 'e' | '+' | '-');
    let mut out = Vec::new();
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if in_class(c) {
            run.push(c);
            continue;
        }
        let token = run.trim_end_matches(['.', '+', '-']);
        if token.starts_with(|c: char| c.is_ascii_digit())
            && (token.contains(['.', 'e']) || token.bytes().filter(u8::is_ascii_digit).count() >= 3)
            && token.matches('.').count() < 2
        {
            out.push(token.to_string());
        }
        run.clear();
    }
    out
}

/// Every numeral the preamble quotes, outside its `Generated:` line, occurs
/// in the generated block (R22).
fn check_preamble(preamble: &str, block: &str) -> Result<(), String> {
    let known: HashSet<String> = numerals(block).into_iter().collect();
    let missing: Vec<String> = preamble
        .lines()
        .filter(|l| !l.contains("Generated:"))
        .flat_map(numerals)
        .filter(|t| !known.contains(t))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("the preamble quotes numbers the generated block does not print: {}", missing.join(", ")))
    }
}

/// The first line where `a` and `b` differ, both sides shown.
fn first_difference(a: &str, b: &str) -> Option<String> {
    let (mut la, mut lb) = (a.lines(), b.lines());
    for n in 1.. {
        match (la.next(), lb.next()) {
            (None, None) => return None,
            (x, y) if x == y => continue,
            (x, y) => {
                return Some(format!(
                    "line {n} of the generated block differs:\n  committed: {}\n  validate:  {}",
                    x.unwrap_or("(end)"),
                    y.unwrap_or("(end)")
                ))
            }
        }
    }
    None
}

#[test]
fn the_committed_record_is_what_validate_prints() {
    // 1. The committed file, on every host, fixtures or not.
    let committed = std::fs::read_to_string(RECORD).unwrap_or_else(|e| panic!("{RECORD}: {e}"));
    let (preamble, block) = split(&committed, "the committed record").unwrap_or_else(|e| panic!("{e}"));
    let recorded = check_block(block, "the committed record").unwrap_or_else(|e| panic!("{e}"));
    check_preamble(preamble, block).unwrap_or_else(|e| panic!("{e}"));

    // 2. The fixtures, or a skip notice naming the first absent one.
    let data = format!("{}/../data", env!("CARGO_MANIFEST_DIR"));
    for rel in semblance_flyvis::parity::required_files() {
        let path = format!("{data}/{rel}");
        if std::fs::metadata(&path).is_err() {
            assert!(!flag("SEMBLANCE_REQUIRE_FIXTURES"), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
            eprintln!("skipping: {path} not found. {}", semblance_flyvis::parity::how_to_get(rel));
            return;
        }
    }

    // 3. Run validate, as CI's record step does.
    let out = Command::new(env!("CARGO_BIN_EXE_semblance"))
        .args(["validate", "--markdown", "--data", &data])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "validate must pass:\n{stdout}{}", String::from_utf8_lossy(&out.stderr));

    // 4. Its block, held to the same marker checks.
    let (_, fresh) = split(&stdout, "validate's output").unwrap_or_else(|e| panic!("{e}"));
    let here = check_block(fresh, "validate's output").unwrap_or_else(|e| panic!("{e}"));

    // 5. The host-specific sections are never compared.
    let (mut a, mut b) = (remove_host(block), remove_host(fresh));

    // 6. The libm sections, only on the record's libm.
    match libm_decision(&here, &recorded, flag("SEMBLANCE_REQUIRE_LIBM_RECORD")) {
        Libm::Compare => {}
        Libm::Fail(m) => panic!("{m}"),
        Libm::Skip => {
            a = replace_libm(&a);
            b = replace_libm(&b);
            eprintln!("record: libm sections not compared: this host's libm fingerprint {here} differs from the record's {recorded}");
        }
    }

    // 7. Byte for byte.
    if let Some(d) = first_difference(&a, &b) {
        panic!("{d}");
    }
    assert_eq!(a, b, "the generated blocks differ in their line endings");
}

// ---------------------------------------------------------------------------
// The text functions, on synthetic blocks
// ---------------------------------------------------------------------------

const FP: &str = "f32:104:0123456789abcdef,f64:108:fedcba9876543210";

fn block(fp_marker: &str, fp_host: &str, libm_body: &str, rest: &str) -> String {
    format!(
        "{BEGIN}\n## Reference\n| a | 1.5e-3 |\n\n{HOST_BEGIN}\n| threads | 4 |\n| libm fingerprint | {fp_host} |\n{HOST_END}\n\n\
         {LIBM_BEGIN}{fp_marker} -->\n{libm_body}\n{LIBM_END}\n\n{rest}\n{END}"
    )
}

#[test]
fn placeholder_replacement_hides_only_the_libm_section() {
    let x = block(FP, FP, "| 4 | 8.605e-1 |", "| control | 2.5e0 |");
    let y = block(FP, FP, "| 4 | 8.601e-1 |", "| control | 2.5e0 |");
    assert_ne!(remove_host(&x), remove_host(&y));
    assert_eq!(replace_libm(&remove_host(&x)), replace_libm(&remove_host(&y)));
    assert!(replace_libm(&x).contains(PLACEHOLDER) && !replace_libm(&x).contains("8.605e-1"));
    // outside the libm section a difference still shows
    let z = block(FP, FP, "| 4 | 8.605e-1 |", "| control | 2.4e0 |");
    let d = first_difference(&replace_libm(&x), &replace_libm(&z)).expect("a difference");
    assert!(d.contains("| control | 2.5e0 |") && d.contains("| control | 2.4e0 |"), "{d}");
    // the host section, whatever it holds, is removed
    assert_eq!(remove_host(&x), remove_host(&x.replace("| threads | 4 |", "| threads | 16 |")));
    assert!(!remove_host(&x).contains("threads"));
}

#[test]
fn a_missing_duplicated_or_unparsable_marker_fails() {
    let good = block(FP, FP, "x", "y");
    assert_eq!(check_block(&good, "t"), Ok(FP.to_string()));
    assert!(split(&format!("preamble\n{good}\n"), "t").is_ok());
    assert!(split("preamble only\n", "t").is_err(), "no block");
    assert!(split(&format!("{good}\n{good}"), "t").is_err(), "a block twice");
    assert!(check_block(&good.replace(HOST_END, ""), "t").is_err(), "no host end");
    assert!(check_block(&good.replace(LIBM_END, &format!("{LIBM_END}\n{LIBM_END}")), "t").is_err(), "libm end twice");
    for bad in ["f32:104:0123,f64:108:fedcba9876543210", "f64:108:fedcba9876543210,f32:104:0123456789abcdef", "x"] {
        assert!(check_block(&block(bad, bad, "x", "y"), "t").is_err(), "{bad}");
    }
}

#[test]
fn a_host_fingerprint_that_disagrees_with_the_libm_marker_fails() {
    let other = "f32:104:0123456789abcdef,f64:109:fedcba9876543210";
    let e = check_block(&block(FP, other, "x", "y"), "the committed record").unwrap_err();
    assert!(e.contains("differs from the host section's"), "{e}");
}

#[test]
fn the_preamble_may_quote_only_numbers_the_block_prints() {
    let b = block(FP, FP, "| 4 | 8.605e-1 |", "| run 2 | 7.439e2 | 45669 |");
    assert_eq!(check_preamble("The worst ratio is 8.605e-1, and 7.439e2 in run 2.\n", &b), Ok(()));
    assert_eq!(check_preamble("45669 values, f32 and f64, 12 sequences, 65 types.\n", &b), Ok(()));
    let e = check_preamble("The worst ratio is about 0.86.\n", &b).unwrap_err();
    assert!(e.ends_with("0.86"), "{e}");
    assert!(check_preamble("744 times the bound\n", &b).is_err(), "three digits");
    assert_eq!(check_preamble("semblance 0.3.0 reproduces flyvis 1.2.0.\n", &b), Ok(()), "versions are exempt");
    assert_eq!(check_preamble("**Generated:** Rust 1.97, run 123456, 2026-10-30.\n", &b), Ok(()), "the Generated: line");
    assert_eq!(numerals("by 3.773e-3. Then 1e-10, f32, 1/200 and fixtures-2"), ["3.773e-3", "1e-10", "200"]);
}

#[test]
fn require_libm_record_turns_a_fingerprint_mismatch_into_a_failure() {
    let other = "f32:0:0000000000000000,f64:0:0000000000000000";
    assert_eq!(libm_decision(FP, FP, true), Libm::Compare);
    assert_eq!(libm_decision(FP, FP, false), Libm::Compare);
    assert_eq!(libm_decision(other, FP, false), Libm::Skip);
    assert_eq!(
        libm_decision(other, FP, true),
        Libm::Fail(format!(
            "SEMBLANCE_REQUIRE_LIBM_RECORD=1 but this host's libm fingerprint {other} differs from the record's {FP}"
        ))
    );
}
```

- [ ] **Step 2: Run it to see it fail.**

  Run: `cargo test -p semblance --test record`
  Expected: the five synthetic tests pass;
  `the_committed_record_is_what_validate_prints` fails:
  `.../app/../docs/results/0.3.0-flyvis-parity.md: No such file or directory`.

- [ ] **Step 3: Write the preamble.** Every numeral in it, outside the
  `Generated:` line, must occur in the generated block (R22): it quotes
  deterministic numbers (`45669`, `2355`, `1e-10`, `1e-3`, the coverage
  ratios `7.439e2`, `6.245e2`, `4.618e2`) and one libm number, the worst
  ratio `8.605e-1`, measured on the Mac (see "Rulings" for what the head does
  if CI's differs). Versions (`0.3.0`) are exempt; write no year and no
  other bare number of three or more digits.

Create `docs/results/0.3.0-flyvis-parity.md` with:

```markdown
# semblance 0.3.0 reproduces flyvis: the parity record

**Generated:** (filled in by the head from hosted CI: the toolchain, Rust 1.97 on `ubuntu-latest` as `ci.yml` pins it; the CI run's URL; the date)

This is the record of epic 0.3.0, graded dynamics. It is the artifact to
consult when asking why semblance's later numbers should be believed: it
shows that this workspace's Rust reimplementation of flyvis (Lappalainen et
al., Nature) computes what flyvis computes.

Everything below the `BEGIN semblance validate --markdown` marker is the
output of `semblance validate --markdown`, pasted unedited from the
`parity-record` artifact of a hosted CI run. `app/tests/record.rs`
regenerates that output and compares it with this file, so the record cannot
drift from the code. Nobody edits its numbers by hand, and nobody regenerates
it on a workstation.

## What 0.3.0 claims

For the best and the worst of flyvis's 50 trained models (000 and 049), at
both of flyvis's time steps (1/200 and 1/50), in f32 and in f64, the Rust
integrator reproduces flyvis's own reference traces in three stages:

- **settling**: every one of the 45669 neurons' settled state after flyvis's
  one second of grey, and the 65 centre neurons at every settling step
- **traces**: from that settled state, through flyvis's twelve reference
  stimuli (four flashes, eight moving edges), every centre neuron at every
  frame and every neuron at six snapshot frames per stimulus
- **controls**: four negative and positive controls, which show the gates can
  see what they exist to see

Three gates hold the claim:

- **exact.** With torch's own `exp` of the synapse counts compiled in
  (`crates/flyvis/src/syn_count_torch.rs`, generated by
  `tools/export_syn_count.py`: flyvis computes each synapse count as
  `exp(ln count)` in torch, and that table is torch's result for every one of
  the 2355 counts, in f32 and f64), every Rust value equals flyvis's. "Bit for
  bit" here is `==` on values widened to f64, and the comparison first checks
  that no reference value is +0.0 or -0.0, so `==` is bit equality for every
  value compared.
- **libm.** With Rust's own `exp` computing the synapse counts, every value
  lies within a bound fixed in the specs before any comparison ran: 1e-10 in
  f64, and twice flyvis's own f32-against-f64 gap in f32. "Bounds as applied"
  below lists every bound and where it comes from.
- **controls.** The wrong model's parameters miss by more than 1e-3; the
  spec's integer synapse counts miss by more than 1e-10; mirrored input
  misses by more than 1e-3; and a run restarted from flyvis's own settled
  state is still bit-equal.

No bound, tolerance or comparison is ever relaxed to make a gate pass. A
failure is a discrepancy to find in flyvis's source.

## Reading the libm rows

The libm bounds exist because torch's `exp` and the platform's differ in the
last bit for a few of the synapse counts; the f32 bound is twice flyvis's own
f32-against-f64 gap, so the f32 ratios measure Rust's `exp` against flyvis's
own precision limit and sit well below 1, while the f64 ratios are many
orders of magnitude smaller, against the fixed f64 bound. The worst ratio in
the record is 8.605e-1: within its bound.

The libm rows are facts about one host, because Rust's `f32::exp` and
`f64::exp` call the platform's libm. This record's libm section was produced
on hosted CI (`ubuntu-latest`), and the libm fingerprint in its marker names
that host's `exp`. On any other host, `semblance validate` still requires
every libm value to be within its bound, but the record test compares the
libm section only where the fingerprint matches. Everything else in the
record is IEEE arithmetic on fixed inputs and is compared on every host.

## The fixtures

The reference is flyvis's own CPU export, release `fixtures-2`; every file is
verified against `ci/fixtures.sha256` by `ci/fetch-fixtures.sh`.
`semblance validate` checks each file's length against the manifest and
against PROVENANCE, the gaps and the four PROVENANCE flags against what the
specs registered, and refuses anything else as a fixture error.

## Coverage and the Sintel decision

Sintel's revisit trigger 1 asks whether some cell type is not driven by the
reference stimuli, which would make the trace gates blind to it. The
"Coverage" section answers it from the exact jobs. A type is driven if its
centre trace moves by more than 1e-3, an absolute threshold, recorded and not
a gate. Every type is driven in every run, so trigger 1 does not fire.

The weakest type's movement is many times the libm centre bound in every
run, but not a thousand times in every f32 run: the ratios printed for runs 2, 4
and 6 are 7.439e2, 6.245e2 and 4.618e2. The record states the absolute
threshold and prints every run's ratio, rather than claim one multiple for
all.

```

- [ ] **Step 4: Check the preamble against a real block, then restore it.**

  Run:
```bash
cp docs/results/0.3.0-flyvis-parity.md target/preamble.md
cargo run --quiet -- validate --markdown >> docs/results/0.3.0-flyvis-parity.md
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance --test record
cp target/preamble.md docs/results/0.3.0-flyvis-parity.md
git diff --exit-code docs/results/0.3.0-flyvis-parity.md || git status --short
```
  Expected: `6 passed` (the block came from this host, so its libm section
  is compared too), and after the restore the file holds the preamble only.
  This block is never committed.

- [ ] **Step 5: Run the test as it stands until the head's record step.**

  Run: `cargo test -p semblance --test record`
  Expected: 5 passed, 1 failed:
  `the_committed_record_is_what_validate_prints` panics with
  `the committed record: no <!-- BEGIN semblance validate --markdown -->`.

- [ ] **Step 6: The gate, with exactly one expected failure.**

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --no-fail-fast 2>&1 | grep -E '^test .* FAILED$|^test result'`
  Expected: one `FAILED` line, `test the_committed_record_is_what_validate_prints ... FAILED`;
  477 passed in all. Then clippy, roundtrip, the 1.85 build and the
  `Cargo.lock` count as in Global Constraints: all clean. (`.temper/gate-full.sh`
  stops at the failing test; run its later commands by hand.)

- [ ] **Step 7: Commit.**

```bash
git add app/tests/record.rs \
  docs/results/0.3.0-flyvis-parity.md
git commit -m "Add the parity record's preamble and the test that holds it to validate" \
  -m "Temper-Task: parity-validate/record-test"
```

### Task 10: README and the crate docs (wave 5, id `readme-docs`)

**Wave:** 5

Spec §10 (README, except the count sentence), §9 packaging items 3 (README's
attribution), 4 and 8, R17. Depends on tasks 4 and 5.

**Files:**
- Modify: `README.md` (headline, how to check, the record link, quickstart, crate table, attribution, absolute LICENSE/NOTICE links)
- Modify: `crates/flyvis/src/lib.rs` (crate docs only: "Parity with flyvis" and "Licensing")

**Interfaces:**
- Consumes: nothing.
- Produces: docs only. The count sentence ("Without fixture data, ...") is
  task 11's.

- [ ] **Step 1: The crate docs.** The generated files `pretrained.rs` and
  `syn_count_torch.rs` are not edited (CLAUDE.md: rerun their scripts only).

In `crates/flyvis/src/lib.rs`, replace:

```rust
//! here: the crates that compress and score a network should not have to
//! depend on the importer to name the thing they operate on.

pub mod groundtruth;
```

with:

```rust
//! here: the crates that compress and score a network should not have to
//! depend on the importer to name the thing they operate on.
//!
//! # Parity with flyvis
//!
//! [`parity`] runs `semblance-sim`'s graded integrator through flyvis's
//! settling and its twelve reference stimuli, and compares every value with
//! flyvis's own reference export: exactly, with torch's `exp` of the synapse
//! counts ([`syn_count_torch`]), and within bounds fixed in advance, with
//! Rust's. The crate's `#[cfg(test)]` gates and the `semblance validate`
//! command both run it. The result is recorded in
//! `docs/results/0.3.0-flyvis-parity.md`.
//!
//! # Licensing
//!
//! [`pretrained`], [`syn_count_torch`] and [`groundtruth`] are flyvis's data,
//! under flyvis's MIT licence, and parts of this crate port flyvis's
//! MIT-licensed code (the connectome expansion, the hex-eye fp16 mask
//! construction, the ground-truth readers). The package is licensed
//! `Apache-2.0 AND MIT`; NOTICE lists the files and carries flyvis's licence
//! text.

pub mod groundtruth;
```

  Run: `cargo doc -p semblance-flyvis --no-deps 2>&1 | grep -c 'unresolved link'`
  Expected: `1` (the existing one in `wiring.rs`; none from `lib.rs` or
  `parity`).

- [ ] **Step 2: README.** The links that crates.io also renders are
  absolute GitHub URLs.

In `README.md`, replace:

```markdown
and **the behavioural-fidelity metric**, are not built yet. There is no
compression result yet.

## The thesis in one line
```

with:

```markdown
and **the behavioural-fidelity metric**, are not built yet. There is no
compression result yet.

**0.3.0 reproduces flyvis 1.2.0's CPU reference (`fixtures-2`) bit for
bit.** With torch's own `exp` of the synapse counts compiled in
(`syn_count_torch.rs`, generated by `tools/export_syn_count.py`), the Rust
integrator's values equal flyvis's:

- in every neuron's settled state, and in every centre neuron at every frame
  and every neuron at six frames of 12 stimuli
- for the best and worst of the 50 trained models, at both time steps, in f32
  and f64
- and within pre-registered bounds when Rust computes the synapse counts
  itself, with its own `exp`

To check it on your machine, fetch the fixtures with `ci/fetch-fixtures.sh`
and run `cargo run --release -- validate`. It exits 0 if every gate holds, 1
if a gate failed, and 2 if verification did not complete (a fixture missing
or malformed, or a bad command line). The result as recorded on hosted CI is
[docs/results/0.3.0-flyvis-parity.md](https://github.com/miki-przygoda/semblance/blob/main/docs/results/0.3.0-flyvis-parity.md).

## The thesis in one line
```

In `README.md`, replace:

````markdown
cargo run -- roundtrip # lossless encode->decode, report structural + behavioural fidelity
cargo run -- evolve    # seed from the connectome latent, run a toy evolution loop
```

````

with:

````markdown
cargo run -- roundtrip # lossless encode->decode, report structural + behavioural fidelity
cargo run -- evolve    # seed from the connectome latent, run a toy evolution loop
cargo run --release -- validate  # check this build reproduces flyvis (needs the fixtures)
```

````

In `README.md`, replace:

```markdown
| `rng` | `semblance-rng` — SplitMix64-seeded xoshiro256++, frozen stream pinned by test vectors | — |
| `connectome` | graph type, CSR view, edge-list loader, synthetic generator; `TypedNetwork` — a realised network that keeps flyvis's parameter sharing (cell type per neuron, connection type per edge) instead of flattening it | load a real FlyWire/MaleCNS export |
| `flyvis` | `semblance-flyvis` — reads the published flyvis connectome spec and expands it exactly as flyvis builds it (45,669 neurons, 1,513,231 synapses, 604 connection types); carries the trained parameters, the curated ground truth, the three stimulus families (flashes, dots, moving edges), the 50-model trained ensemble, and flyvis's reference traces, all gated against flyvis's own output. Its settling gates run `sim`'s graded dynamics through flyvis's 1 s of grey and require every neuron's settled state to equal flyvis's bit for bit, for the best and worst trained models (000 and 049), at both time steps, in f32 and f64. Its trace gates continue from that settled state through flyvis's flashes and moving edges, fed through the input mapping (`scatter_hexals`, gated on flyvis's lattice), and require every centre neuron at every frame, and every neuron at six snapshot frames, to equal flyvis's bit for bit | the result record (0.3.0, in progress) |
| `sim` | graded dynamics (`semblance_sim::graded`): flyvis's passive point neurons, explicit Euler, f32 and f64, reproducing flyvis's settling and its traces under stimulus bit for bit. Also an LIF spiking path, a placeholder to be replaced | sensory encoding + motor decoding |
| `codec` | `WeightCodec` — lossless baseline, directly evolvable | `ReducedCodec` — the actual compression (`todo!()`) |
| `evo` | gaussian mutation + truncation selection + elitism, seedable | a real `Fitness` (environment + reward) |
| `fidelity` | structural + behavioural distance, size-vs-fidelity curve harness | the **probe battery** design |
| `app` | CLI wiring the three flows together | — |

The open work is `ReducedCodec` in `crates/codec/src/lib.rs`, whose `encode`
```

with:

```markdown
| `rng` | `semblance-rng` — SplitMix64-seeded xoshiro256++, frozen stream pinned by test vectors | — |
| `connectome` | graph type, CSR view, edge-list loader, synthetic generator; `TypedNetwork` — a realised network that keeps flyvis's parameter sharing (cell type per neuron, connection type per edge) instead of flattening it | load a real FlyWire/MaleCNS export |
| `flyvis` | `semblance-flyvis` — reads the published flyvis connectome spec and expands it exactly as flyvis builds it (45,669 neurons, 1,513,231 synapses, 604 connection types); carries the trained parameters, the curated ground truth, the three stimulus families (flashes, dots, moving edges), the 50-model trained ensemble, and flyvis's reference traces, all gated against flyvis's own output. Its settling gates run `sim`'s graded dynamics through flyvis's 1 s of grey and require every neuron's settled state to equal flyvis's bit for bit, for the best and worst trained models (000 and 049), at both time steps, in f32 and f64. Its trace gates continue from that settled state through flyvis's flashes and moving edges, fed through the input mapping (`scatter_hexals`, gated on flyvis's lattice), and require every centre neuron at every frame, and every neuron at six snapshot frames, to equal flyvis's bit for bit. `parity` runs those gates as a library, for the crate's tests and for `semblance validate` | — |
| `sim` | graded dynamics (`semblance_sim::graded`): flyvis's passive point neurons, explicit Euler, f32 and f64, reproducing flyvis's settling and its traces under stimulus bit for bit. Also an LIF spiking path, a placeholder to be replaced | sensory encoding + motor decoding |
| `codec` | `WeightCodec` — lossless baseline, directly evolvable | `ReducedCodec` — the actual compression (`todo!()`) |
| `evo` | gaussian mutation + truncation selection + elitism, seedable | a real `Fitness` (environment + reward) |
| `fidelity` | structural + behavioural distance, size-vs-fidelity curve harness | the **probe battery** design |
| `app` | CLI wiring the three flows together, and `validate`, which runs every parity gate and control and prints the record's table | — |

The open work is `ReducedCodec` in `crates/codec/src/lib.rs`, whose `encode`
```

In `README.md`, replace:

```markdown

The fixture data — and the trained parameters in
`crates/flyvis/src/pretrained.rs` and the ground truth in
`crates/flyvis/src/groundtruth.rs` — come from flyvis
(https://github.com/TuragaLab/flyvis). They are flyvis's MIT-licensed data, not
Apache-2.0. `NOTICE` lists exactly which files, carries flyvis's licence text,
and cites the upstream connectome sources.

```

with:

```markdown

The fixture data — and the trained parameters in
`crates/flyvis/src/pretrained.rs`, the ground truth in
`crates/flyvis/src/groundtruth.rs` and torch's synapse counts in
`crates/flyvis/src/syn_count_torch.rs` — come from flyvis
(https://github.com/TuragaLab/flyvis). They are flyvis's MIT-licensed data, not
Apache-2.0. Parts of `semblance-flyvis` also port flyvis's MIT-licensed code
(the connectome expansion, the hex-eye fp16 mask construction, the
ground-truth readers); that package is licensed `Apache-2.0 AND MIT`.
`NOTICE` lists exactly which files, carries flyvis's licence text,
and cites the upstream connectome sources.

```

In `README.md`, replace:

```markdown
## License

Apache-2.0 — see [LICENSE](LICENSE). Third-party data is excepted; see
[NOTICE](NOTICE).

Copyright 2026 Mikolaj Mikuliszyn.
```

with:

```markdown
## License

Apache-2.0 — see [LICENSE](https://github.com/miki-przygoda/semblance/blob/main/LICENSE).
Third-party data and ported code are excepted; see
[NOTICE](https://github.com/miki-przygoda/semblance/blob/main/NOTICE).

Copyright 2026 Mikolaj Mikuliszyn.
```

- [ ] **Step 3: The gate.** As in Global Constraints. Expected: 0 failed on
  this task's worktree (it does not contain task 9).

- [ ] **Step 4: Commit.**

```bash
git add README.md \
  crates/flyvis/src/lib.rs
git commit -m "Document flyvis parity, validate and the licensing in README and the crate docs" \
  -m "Temper-Task: parity-validate/readme-docs"
```

### Task 11: Counts and CLAUDE.md (wave 6, id `docs-counts`)

**Wave:** 6

Spec §10 "CLAUDE.md", README's count sentence, R5. Runs after the head has
committed CI's generated block into the record (see "Head steps"), so the
suite is green.

**Files:**
- Modify: `CLAUDE.md` (version, counts, the notice sentence, `validate`, the record, `SEMBLANCE_REQUIRE_LIBM_RECORD`, the yank paragraph, the tools/ boundary)
- Modify: `README.md` (the count sentence)

**Interfaces:**
- Consumes: the merged branch with the record's generated block.
- Produces: docs only.

- [ ] **Step 1: Re-measure, with the fixtures.**

  Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --no-fail-fast 2>&1 | grep -E '^test result' | awk '{p+=$4; f+=$6} END {print p, f}'`
  Expected: `478 0`. (The record test passes here, skipping the libm section
  unless this host's fingerprint is CI's.)

- [ ] **Step 2: Re-measure, with `data/` absent.** In this worktree `data`
  is a symlink; remove it and restore it afterwards:
```bash
rm data
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --no-fail-fast 2>&1 | grep -cE '^test .* FAILED$'
cargo test --workspace --no-fail-fast -- --nocapture --test-threads=1 2>&1 | grep -c 'skipping:'
cargo test --workspace 2>&1 | grep -E '^test result' | awk '{p+=$4; f+=$6} END {print p, f}'
ln -s <main checkout>/data data
```
  Expected: `129`, `141`, `478 0`. If a number differs, use the measured
  one below and say so in the commit message.

- [ ] **Step 3: CLAUDE.md.**

In `CLAUDE.md`, replace:

```markdown
measure how far it can shrink before it stops behaving like the fly.

Current: **v0.2.0**, crate versions `0.2.0`. See "Version bumps" below — they
mean something specific here.

```

with:

```markdown
measure how far it can shrink before it stops behaving like the fly.

Current: **v0.3.0**, crate versions `0.3.0`. See "Version bumps" below — they
mean something specific here.

```

In `CLAUDE.md`, replace:

```markdown
directly in Rust. If a script there ever becomes something the Rust side needs at
*runtime*, the boundary has been broken. See `tools/README.md`.

**`semblance-rng`'s stream is frozen by contract.** `crates/rng/tests/vectors.rs`
```

with:

```markdown
directly in Rust. If a script there ever becomes something the Rust side needs at
*runtime*, the boundary has been broken. See `tools/README.md`.
`semblance validate` reads the exported files under `data/` and nothing
else: it never runs `tools/` or git.

**`semblance-rng`'s stream is frozen by contract.** `crates/rng/tests/vectors.rs`
```

In `CLAUDE.md`, replace:

```markdown
opportunistically — that is an epic with its own design.

`semblance-codec 0.1.0` is published on crates.io with those panics in it. When
a real implementation ships, `cargo yank` that version.

---
```

with:

```markdown
opportunistically — that is an epic with its own design.

`semblance-codec` 0.3.0 still contains both `todo!()`s; its description says
so. The six 0.1.0 releases (semblance, -codec, -connectome, -evo, -fidelity,
-sim) are to be yanked once all eight 0.3.0 crates are live: they predate
flyvis and graded dynamics, evo 0.1.0 depends on `rand`, and yanking codec
alone would strand evo, fidelity and semblance 0.1.0. Publishing and yanking
wait for the owner's go-ahead. A real `ReducedCodec` later needs no yank of
its own.

---
```

In `CLAUDE.md`, replace:

````markdown
```bash
cargo build --workspace
cargo test --workspace          # 429 tests, 0 failures
cargo clippy --workspace --all-targets -- -D warnings
cargo run --quiet -- roundtrip  # both distances must read exactly 0.000000
```

123 of those tests skip when `data/flyvis-supplementary/`, `data/flyvis-build/`,
`data/flyvis-stimuli/`, `data/flyvis-reference/` and
`data/flyvis-pretrained/ensemble.csv` are absent (all five are gitignored
````

with:

````markdown
```bash
cargo build --workspace
cargo test --workspace          # 478 tests, 0 failures
cargo clippy --workspace --all-targets -- -D warnings
cargo run --quiet -- roundtrip  # both distances must read exactly 0.000000
cargo run --release -- validate # exit 0: every parity gate and control holds
```

129 of those tests skip when `data/flyvis-supplementary/`, `data/flyvis-build/`,
`data/flyvis-stimuli/`, `data/flyvis-reference/` and
`data/flyvis-pretrained/ensemble.csv` are absent (all five are gitignored
````

In `CLAUDE.md`, replace:

```markdown
A skip prints `eprintln!("skipping: ...")`, but libtest captures `eprintln!`
for *passing* tests, so under plain `cargo test --workspace` those notices are invisible —
the suite reports the same "429 passed, 0 failed" whether the data is present
or absent. They are only visible under `cargo test --workspace -- --nocapture`,
and even then not quite one per skipped test. Fixtures loaded once and shared
```

with:

```markdown
A skip prints `eprintln!("skipping: ...")`, but libtest captures `eprintln!`
for *passing* tests, so under plain `cargo test --workspace` those notices are invisible —
the suite reports the same "478 passed, 0 failed" whether the data is present
or absent. They are only visible under `cargo test --workspace -- --nocapture`,
and even then not quite one per skipped test. Fixtures loaded once and shared
```

In `CLAUDE.md`, replace:

```markdown
`every_fixture_gated_acceptance_test_prints_its_own_skip_notice` holds the
acceptance files to that). A test that reads more than one missing fixture
prints more than one. The exception is a test that runs other tests in a child
`cargo test`
(`sequences_08_manifest_parse_its_tests_and_acceptance_reference_are_unchanged_and_pass`),
which prints none of its own: 122 of the 123 print a notice, 134 notices in
all under `--nocapture --test-threads=1`. Under
`SEMBLANCE_REQUIRE_FIXTURES=1` every one of the 123 fails instead.

Set `SEMBLANCE_REQUIRE_FIXTURES=1` to turn every missing-fixture skip into a
```

with:

```markdown
`every_fixture_gated_acceptance_test_prints_its_own_skip_notice` holds the
acceptance files to that). A test that reads more than one missing fixture
prints more than one: all 129 print a notice, 141 notices in all under
`--nocapture --test-threads=1`. Under `SEMBLANCE_REQUIRE_FIXTURES=1` every
one of the 129 fails instead.

Set `SEMBLANCE_REQUIRE_FIXTURES=1` to turn every missing-fixture skip into a
```

In `CLAUDE.md`, replace:

```markdown
`roundtrip` reporting anything other than `0.000000` means a lossless round-trip
stopped being lossless — that is a real break, not a tolerance to adjust.

**The moving-edge geometry gate is not precedent for loosening a comparison
```

with:

```markdown
`roundtrip` reporting anything other than `0.000000` means a lossless round-trip
stopped being lossless — that is a real break, not a tolerance to adjust.

**`semblance validate` is the gates, as a command.** `cargo run --release --
validate` (fixtures under `./data`, or `--data DIR`) runs every settling and
trace gate and the four controls through `semblance_flyvis::parity`, the same
code the `#[cfg(test)]` gates call, and prints one table. Exit 0: every exact
row is bit-equal and every libm row within its pre-registered bound. Exit 1: a
gate or control failed. Exit 2: verification did not complete (a fixture
missing, unreadable, malformed or mismatched, or a bad command line); nothing
counts as verified. The exact rows are bit-equal only because
`syn_count_torch.rs` compiles in torch's own `exp` of the synapse counts.

**The record, `docs/results/0.3.0-flyvis-parity.md`,** is a hand-written
preamble followed by `validate --markdown`'s output from hosted CI, held to it
by `app/tests/record.rs`. Only the head regenerates it, from CI's
`parity-record` artifact (the "Validate, writing the record's generated
block" step), never by hand or on a workstation; every number the preamble
quotes must occur in the generated block. CI sets
`SEMBLANCE_REQUIRE_LIBM_RECORD=1`, so there the libm section must match byte
for byte. Elsewhere a libm fingerprint that differs from the record's skips
that section, and its `record: libm sections not compared` notice is visible
only under `--nocapture`; everything else is still compared.

**The moving-edge geometry gate is not precedent for loosening a comparison
```

- [ ] **Step 4: README's count sentence.** SEQUENCES-08 no longer runs a
  nested cargo (task 1), so every fixture-gated test prints its notice.

In `README.md`, replace:

```markdown
(`RESOURCES.md` → "Getting the data").

**Without fixture data, 123 flyvis parity tests skip and still report
green.** They are the tests that prove this workspace reproduces flyvis. They
read gitignored files under `data/`, and when those are absent each one passes,
all but one printing a skip notice (SEQUENCES-08 runs a nested
`cargo test` and prints none of its own); libtest hides that notice for passing tests, so plain
`cargo test` looks the same either way. To see the skips, run
`cargo test --workspace -- --nocapture`. To make a missing fixture a failure
```

with:

```markdown
(`RESOURCES.md` → "Getting the data").

**Without fixture data, 129 flyvis parity tests skip and still report
green.** They are the tests that prove this workspace reproduces flyvis. They
read gitignored files under `data/`, and when those are absent each one passes,
printing a skip notice; libtest hides that notice for passing tests, so plain
`cargo test` looks the same either way. To see the skips, run
`cargo test --workspace -- --nocapture`. To make a missing fixture a failure
```

- [ ] **Step 5: The gate.** As in Global Constraints, plus
  `cargo run --release -- validate` (exit 0). Expected: 0 failed.

- [ ] **Step 6: Commit.**

```bash
git add CLAUDE.md \
  README.md
git commit -m "Record 0.3.0's version, test counts, validate and the record in CLAUDE.md" \
  -m "Temper-Task: parity-validate/docs-counts"
```

---

## Head steps

These are not tasks: no agent does them.

**After wave 5 merges: the record.** (Spec §7, "Generating the record".)

1. Push `parity-validate`. CI's `test` job builds, runs `Validate, writing the
   record's generated block` and uploads the `parity-record` artifact; its
   test step then fails on `the_committed_record_is_what_validate_prints`
   only (bootstrap). If the validate step itself failed, stop: a gate failed
   on CI's host.
2. `gh run download <run-id> --name parity-record --dir target/parity-record`.
3. Append the artifact's content below the preamble of
   `docs/results/0.3.0-flyvis-parity.md` (or replace everything from the
   `<!-- BEGIN semblance validate --markdown -->` line to the
   `<!-- END semblance validate --markdown -->` line, inclusive, if a block
   is there). Fill the preamble's `Generated:` line with the toolchain (Rust
   1.97 on `ubuntu-latest`), the run URL and the date. Edit no number by
   hand, with one exception: run
   `cargo test -p semblance --test record`; if it fails with `the preamble
   quotes numbers the generated block does not print: 8.605e-1`, CI's worst
   ratio differs from the Mac's, and the preamble's one quote of it is
   replaced by the block's value (the `ratio` of the `traces` row with the
   largest ratio), copied, and the commit message says so.
4. Locally, with fixtures: `cargo test -p semblance --test record --
   --nocapture` must pass, printing the `record: libm sections not compared`
   notice (the Mac's fingerprint differs from CI's). A failure naming a
   differing line outside the libm section is a real discrepancy: stop.
5. `git add docs/results/0.3.0-flyvis-parity.md` and commit
   `Commit the parity record's generated block from CI run <id>` with the
   trailer `Temper-Task: parity-validate/record`. Push; CI must be green,
   with the record test comparing everything under
   `SEMBLANCE_REQUIRE_LIBM_RECORD=1`. Record the test job's time in the PR
   description against the 12m32s baseline; if the record test alone takes
   more than 150 s on CI, report it to the owner (nothing is removed).

Then wave 6 (task 11) runs.

**After the PR merges** (spec §9):

1. `git tag -a v0.3.0 -m "0.3.0: graded dynamics, validated against flyvis"`
   on the merge commit; `git push origin v0.3.0`. Set `CITATION.cff`'s
   `date-released` to the tag date in a follow-up commit if it was not set
   before tagging. Then, from a clean checkout of the tag, with network
   access and cargo 1.90 or later (`--workspace` is unstable on `publish`
   before that): `cargo publish --workspace --dry-run --locked; echo "exit $?"`.
   Expected: `Uploading` for rng, connectome, codec, sim, evo, fidelity,
   flyvis, semblance (in that order), each followed by `warning: aborting
   upload due to dry run`; `exit 0`. This is RELEASE-03's dry run and the
   upload order RELEASE-02 names; they need the crates.io index, so they are
   checked here and not by `app/tests/acceptance_release.rs`, which stays
   offline. If it fails, stop: nothing is published.
2. **Stop and ask the owner.** Only on an explicit go-ahead at that moment,
   from a clean checkout of the tag: `cargo publish --workspace` (it
   publishes rng, connectome, codec, sim, evo, fidelity, flyvis, semblance in
   that order, waiting for the index between crates). If it fails part-way,
   stop; the owner decides.
3. After all eight 0.3.0 crates are live: `cargo yank --version 0.1.0 <crate>`
   for `semblance`, `semblance-codec`, `semblance-connectome`,
   `semblance-evo`, `semblance-fidelity` and `semblance-sim`.
4. A follow-up commit changes CLAUDE.md's "are to be yanked once all eight
   0.3.0 crates are live" to "were yanked on <date>".
5. Edit the local outline (`docs/project-outline/`, not in git): epic 0.3.0's
   task 7 done.

There is no GitHub Release.
