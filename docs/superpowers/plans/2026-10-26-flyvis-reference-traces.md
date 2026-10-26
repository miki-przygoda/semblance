# flyvis Reference-Trace Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Export flyvis 1.2.0's own voltage traces (2 models × 2 dts × 2 precisions, 4 flashes and 8 moving edges each) as plain little-endian files under `data/flyvis-reference/` with a deterministic `manifest.json`, after proving in memory that the parameters, the precision and the determinism are what they claim. Then extend the moving-edge geometry gate to height 80, publish the set as `fixtures-2`, and cache it in CI.

**Architecture:** One offline script, `tools/export_reference.py`, split into pure helpers (CLI, preflight, parameter readers, storage layout, manifest, atomic writer, checkpoint and freeze checks) that a plain-assert self-test file, `tools/test_export_reference.py`, exercises without flyvis, and one flyvis section (`run_one`, `export`, `main`) that runs flyvis's stock `NetworkView` → `init_network("best")` → `steady_state` → `forward` path under its own scratch `FLYVIS_ROOT_DIR`. Every check A1–A9 runs before any output path is created. The writer stages beside the output, verifies, swaps with a marker, and restores the previous set if interrupted between its two renames. The Rust gate gains a second, independently loaded half at height 80. CI swaps its connectome-only cache for a restore/save pair over the whole fixture set.

**Tech Stack:** Python 3.12 under `uv` (`tools/pyproject.toml`: flyvis 1.2.0, which brings torch 2.14.0, numpy 2.5.3, h5py, PyYAML, datamate), plain asserts (no pytest); Rust 1.85+, edition 2024, no new crates; bash 3.2, `shasum`, `tar`, `gh`; GitHub Actions `actions/cache/restore@v4` and `actions/cache/save@v4`.

**Spec:** `docs/superpowers/specs/2026-10-26-flyvis-reference-traces-design.md` (research: `docs/superpowers/specs/2026-10-26-flyvis-reference-traces-research.md`). Owner answers: O1 no, O2 no, O3 yes, O4 yes. So the scratch root applies, `input.f64.bin` is stored, and every "with O4" count applies (11 files in `data/flyvis-reference/`, 35 manifest lines, `15 0`, 34 tarball entries, 35/35 and 15/35).

## Global Constraints

- **Zero external dependencies.** `Cargo.lock` contains exactly 8 packages, all `semblance*`. Add no crates.io dependency, not even for tests. Check: `grep -c '^\[\[package\]\]' Cargo.lock` prints `8`.
- **`tools/` is offline Python, never invoked by Rust.** Nothing in any crate imports, runs or records anything under `tools/`, and nothing in Rust invokes `ci/fetch-fixtures.sh`.
- **Rust 1.85+, edition 2024.** `gen` is a reserved keyword.
- **Clippy is clean under `cargo clippy --workspace --all-targets -- -D warnings`.**
- **Do not run `cargo fmt`.** The repo is not rustfmt-clean.
- **Commit style: plain imperative subject, no `feat:`/`fix:`/`chore:` prefixes.** Match the existing log.
- **`data/` and `docs/project-outline/` are never committed, and this plan never edits `docs/project-outline/`.** `git add` only the explicit paths each commit step names.
- **Never `--force`** a git push, and never delete or replace the `fixtures-1` release.
- **Creating or uploading the `fixtures-2` release needs the owner's explicit sign-off at the point it happens** (Task 9, Step 7), even though this plan is approved. STOP and ask; never do it silently.
- **Running the export needs the owner's go-ahead** (Task 6, Step 3). It runs flyvis for several minutes and writes under `data/`.
- **Pushing the branch and merging the PR happen only on the owner's go-ahead** (Task 10).
- **Never loosen an existing comparison.** `TOLERANCE` stays `0.005`; the A7 bound stays `1e-2`; the A9 background tolerance stays `1e-6`; the A8 expected figures are never edited to match. If one fires: stop and report to the owner.
- **Never work around a check that measures flyvis or torch.** A bit-equality check against flyvis (A1, A3, A5, A6, A9, P17, `equivalence`) is never turned into a tolerance; A6's two runs (0 and 1) are never removed or skipped; `torch.set_num_threads` is never called outside A6's `thread_check`, which restores the default, and `torch.use_deterministic_algorithms` is never called at all (spec item 24). If one of these checks fires: stop and report to the owner.
- **`semblance-rng`'s test vectors are untouched**, and no version is bumped.
- **No existing exporter is re-run** (`export_build.py`, `export_stimuli.py`, `export_moving_edges.py`, `export_params.py`), and no file they produced is rewritten.
- **The 2026-10-01 CI spec and plan are not edited.**
- **Inputs are always read from `<repo>/data`**, with `<repo>` taken from the script's own location; only outputs follow `--out`.
- Exact values (spec): runs 8; sequences per run 12; `n_settle` 200 / 50; flash frames 600 / 150, stimulus [200, 400) / [50, 100); edge frames 525 / 131, stimulus [200, 325) / [50, 81), `n_stim` 125; nodes 45,669; hexals 721; centre 65, first `(360, "R1")`; `f32(dt)` 0.004999999888241291 / 0.019999999552965164; losses 5.136557016107771 (000) / 5.677851685219341 (049); binaries 220,855,520 B; `moving_edge_offsets_h80.bin` 484,512 B; test counts 203 total, 26 fixture-skipping, no new Rust test.

## Review Focus

1. **Two exports at once** (flow 3 started while flow 1 still runs): the second must refuse at once rather than wipe the first's scratch root mid-run. Pinned by `test_a_second_export_cannot_take_the_lock` (Task 1).
2. **An interruption between the two renames of write step 4 under `--force`**: the previous `flyvis-reference/` must be whole again, the leftovers kept, and the next run refused with the recovery text. Pinned by `test_a_crash_between_the_two_renames_restores_the_previous_set` (Task 3).
3. **An f32 run's array reaching an f64 file, or `-0.0` passing for `0.0`**: nothing may widen silently on the way to disk, and bit comparisons must see the sign. Pinned by `test_pack_file_keeps_bits_and_refuses_a_silent_cast` (Task 2) and `test_bits_equal_is_bitwise` (Task 4).
4. **A previous set that is half there** (an `_h80` file without `flyvis-reference/`, from an interrupted step 5 or a hand edit): without `--force` the script must refuse naming that file. Pinned by `test_preflight_names_a_lone_h80_file_without_the_reference_dir` (Task 1).
5. **A numpy scalar in the manifest** (`np.int64` in a shape, `np.float32` in an arg): `json` would crash or write a different number, breaking the byte-identical rerun. Pinned by `test_validate_manifest_refuses_numpy_scalars` (Task 2).

---

## File Structure

| File | Responsibility |
|---|---|
| `tools/export_reference.py` | create: the exporter. Sections, in file order: constants and `require`; CLI, preflight, lock (Task 1); parameter readers (Task 1); storage layout and manifest (Task 2); atomic write (Task 3); model ranking, checkpoints, freeze check, model copy (Task 4); the flyvis runs, cross-checks, provenance, `main` (Task 5) |
| `tools/test_export_reference.py` | create: plain-assert self-tests, one block per task, and `--validate DIR` for a real export |
| `tools/README.md` | modify: a row for `export_reference.py`, the scratch-root note, the "do not re-export a pinned fixture by hand" note |
| `crates/flyvis/src/moving_edge_checks.rs:1-31,137-236` | modify: module doc timing; the gate becomes two independently loaded halves via `geometry_report` |
| `crates/flyvis/src/test_support.rs:106-111,130-135` | modify: the stimulus loaders' skip messages also name `tools/export_reference.py` |
| `crates/flyvis/src/moving_edge.rs:236-240` | modify: the "88 of the monitor's 775 rows" comment becomes height-dependent |
| `.github/workflows/ci.yml:43-51` | modify: the connectome-only cache step becomes `Restore fixtures` / fetch / `Save fixtures` |
| `ci/fixtures.sha256` | regenerate: 20 → 35 lines |
| `ci/fixtures-tag` | modify: `fixtures-2` |
| `ci/fetch-fixtures.sh:2-16` | modify: header comment only |
| `CLAUDE.md:127-131,141-145` | modify: the fixture paragraphs |
| `data/flyvis-reference/*`, `data/flyvis-stimuli/*_h80*` | produced by the export, gitignored, published in `fixtures-2` |

---

### Task 1: The exporter's command line, preflight, lock and parameter readers

**Files:**
- Create: `tools/export_reference.py`
- Create: `tools/test_export_reference.py`
- Modify: `tools/README.md:49-64`

**Interfaces:**
- Consumes: `crates/flyvis/src/pretrained.rs` (text), `data/flyvis-pretrained/ensemble.csv`.
- Produces (module `export_reference`, imported by the tests as `er`):
  - constants `ROOT`, `OUT`, `DATA`, `RESULTS`, `ENSEMBLE_CSV`, `PRETRAINED_RS`, `NODES_CSV`, `FLASHES_BIN`, `TIMING_JSON`, `UV_LOCK`, `SCRATCH`, `SCRATCH_REL`, `MPLCONFIG`, `LOCK`, `GENERATOR`, `FLYVIS_VERSION`, `MODELS`, `N_MODELS`, `MODEL_ENTRIES`, `CHECKPOINT`, `DTS`, `DT_VALUE`, `N_SETTLE`, `FLOOR_DT_F32`, `RUNS`, `EXECUTION_ORDER`, `T_PRE`, `PRE_STIM_VALUE`, `T_FADE_IN`, `BATCH_SIZE`, `N_NODES`, `N_HEXALS`, `N_CENTRE`, `N_PHOTORECEPTOR_TYPES`, `N_SNAPSHOTS`, `FIRST_CENTRE`, `GATE_ANGLES`, `EDGE_SPEED`, `EDGE_HEIGHT`, `N_OMMATIDIA`, `PPO`, `MONITOR`, `FLASH_ARGS`, `EDGE_ARGS`, `N_FRAMES`, `STIMULUS_FRAMES`, `EDGE_N_STIM_1_200`, `SNAPSHOT_FRAMES`, `GAP_BOUND`, `BACKGROUND_TOL`, `PRETRAINED_LENGTHS`, `ENSEMBLE_KINDS`, `RECOVERY`, `H80_FILES`, `LEFTOVER_PREFIXES`
  - `class ExportError(Exception)`
  - `require(ok, check: str, detail: str, run=None, sequence=None) -> None`
  - `parse_args(argv) -> argparse.Namespace` with `.out: pathlib.Path` (absolute) and `.force: bool`
  - `preflight(out: pathlib.Path, force: bool, environ) -> None`
  - `acquire_lock(path: pathlib.Path) -> file` (keep it referenced; `.close()` releases)
  - `parse_pretrained(text: str) -> dict` (`CELL_TYPES: list[str]`, `CONN_TYPES: list[tuple[str, str]]`, `BIAS`/`TIME_CONST`/`SYN_STRENGTH`/`SIGN`/`SYN_COUNT`: `np.float32` arrays)
  - `read_ensemble(text: str, kinds=ENSEMBLE_KINDS, n_models=N_MODELS) -> dict[str, dict[str, tuple[list[str], np.ndarray]]]`
  - test helpers in `tools/test_export_reference.py`: `Skip`, `needs(*paths)`, `raises(fn, *needles) -> str`, `tmpdir() -> pathlib.Path`, `main(argv) -> int`, `validate_dir(directory) -> int`

- [ ] **Step 1: Write the failing tests**

Create `tools/test_export_reference.py` with exactly this content. Later tasks insert their test blocks above the `# -- runner (keep last)` line, so it must stay last.

```python
"""Self-tests for tools/export_reference.py. No flyvis import, no pytest.

Each test_* function is plain asserts. Tests that read real data under data/
skip when it is absent, unless SEMBLANCE_REQUIRE_FIXTURES=1.

Run by hand:  uv run --project tools tools/test_export_reference.py [test_name ...]
Validate an export:  uv run --project tools tools/test_export_reference.py --validate data/flyvis-reference
"""

import json
import os
import pathlib
import sys
import tempfile
import traceback

import numpy as np

import export_reference as er


class Skip(Exception):
    pass


def needs(*paths):
    for p in paths:
        if not p.exists():
            if os.environ.get("SEMBLANCE_REQUIRE_FIXTURES") == "1":
                raise AssertionError(f"SEMBLANCE_REQUIRE_FIXTURES=1 but {p} is missing")
            raise Skip(f"{p} not found")


def raises(fn, *needles):
    """fn() raises ExportError whose message contains every needle."""
    try:
        fn()
    except er.ExportError as e:
        for n in needles:
            assert n in str(e), f"{n!r} not in {str(e)!r}"
        return str(e)
    raise AssertionError("no ExportError raised")


def tmpdir() -> pathlib.Path:
    return pathlib.Path(tempfile.mkdtemp(prefix="export-reference-test-"))


# -- Task 1: command line, preflight, lock, parameter readers --------------------

def test_out_defaults_to_repo_data_and_inputs_never_follow_it():
    args = er.parse_args([])
    assert args.out == er.ROOT / "data" and args.force is False
    args = er.parse_args(["--out", "target/reref", "--force"])
    assert args.out == pathlib.Path.cwd() / "target" / "reref" and args.force is True
    for p in (er.RESULTS, er.ENSEMBLE_CSV, er.NODES_CSV, er.FLASHES_BIN, er.TIMING_JSON, er.PRETRAINED_RS):
        assert p.is_relative_to(er.ROOT), p
    assert er.DATA == er.ROOT / "data"


def test_preflight_passes_on_a_missing_or_empty_out():
    er.preflight(tmpdir() / "not-yet", False, {})
    er.preflight(tmpdir(), False, {})


def test_preflight_refuses_an_inherited_flyvis_root():
    raises(lambda: er.preflight(tmpdir(), False, {"FLYVIS_ROOT_DIR": "/elsewhere"}), "FLYVIS_ROOT_DIR")
    raises(lambda: er.preflight(tmpdir(), True, {"FLYVIS_ROOT_DIR": ""}), "FLYVIS_ROOT_DIR")


def test_preflight_refuses_existing_outputs_unless_forced():
    out = tmpdir()
    (out / "flyvis-reference").mkdir()
    raises(lambda: er.preflight(out, False, {}), str(out / "flyvis-reference"), "--force")
    er.preflight(out, True, {})


def test_preflight_names_a_lone_h80_file_without_the_reference_dir():
    # Review focus 4: a half-present previous set must still be refused.
    for name in er.H80_FILES:
        out = tmpdir()
        (out / "flyvis-stimuli").mkdir()
        (out / "flyvis-stimuli" / name).write_text("x")
        raises(lambda: er.preflight(out, False, {}), name)
        er.preflight(out, True, {})


def test_preflight_refuses_leftovers_even_when_forced():
    for prefix in er.LEFTOVER_PREFIXES:
        out = tmpdir()
        (out / f"{prefix}123").mkdir()
        msg = raises(lambda: er.preflight(out, True, {}), f"{prefix}123", "interrupted")
        assert "rename that .old-* back" in msg, msg


def test_a_second_export_cannot_take_the_lock():
    # Review focus 1: a second run must not wipe a running run's scratch root.
    path = tmpdir() / "sub" / "x.lock"
    first = er.acquire_lock(path)
    try:
        raises(lambda: er.acquire_lock(path), "another export is running")
    finally:
        first.close()
    er.acquire_lock(path).close()


SMALL_RS = '''
pub static CELL_TYPES: [&str; 65] = [
''' + "".join(f'    "T{i}",\n' for i in range(65)) + '''];

pub static CONN_TYPES: [(&str, &str); 604] = [
''' + "".join(f'    ("T{i % 65}", "T{(i + 1) % 65}"),\n' for i in range(604)) + '''];
'''


def _small_rs(counts):
    text = SMALL_RS
    for name, n in counts.items():
        vals = ", ".join(["0.5", "-1.25e-3", "0.0", "3.0"] * (n // 4) + ["1.0"] * (n % 4))
        text += f"\npub static {name}: [f32; {n}] = [\n    {vals},\n];\n"
    return text


def test_parse_pretrained_reads_exact_f32_bits():
    counts = {k: v for k, v in er.PRETRAINED_LENGTHS.items() if k not in ("CELL_TYPES", "CONN_TYPES")}
    p = er.parse_pretrained(_small_rs(counts))
    assert p["CELL_TYPES"][0] == "T0" and len(p["CELL_TYPES"]) == 65
    assert p["CONN_TYPES"][0] == ("T0", "T1") and len(p["CONN_TYPES"]) == 604
    assert p["BIAS"].dtype == np.float32 and len(p["BIAS"]) == 65
    assert p["BIAS"][1] == np.float32(-1.25e-3) and p["BIAS"][0] == np.float32(0.5)


def test_parse_pretrained_refuses_a_wrong_length():
    counts = {k: v for k, v in er.PRETRAINED_LENGTHS.items() if k not in ("CELL_TYPES", "CONN_TYPES")}
    counts["SIGN"] = 603
    raises(lambda: er.parse_pretrained(_small_rs(counts)), "SIGN[603]")


def test_parse_pretrained_real_file():
    p = er.parse_pretrained(er.PRETRAINED_RS.read_text())
    for name, n in er.PRETRAINED_LENGTHS.items():
        assert len(p[name]) == n, name
    assert p["CELL_TYPES"][0] == "R1" and p["CONN_TYPES"][0] == ("R1", "L1")
    assert p["BIAS"][0] == np.float32(0.44422865)
    assert p["SYN_COUNT"][-1] == np.float32(0.0) and p["SYN_COUNT"].dtype == np.float32


def _small_csv(kinds, n_models):
    lines = ["# a", "# b", "# c", "model,kind,index,name,value"]
    for m in range(n_models):
        for kind, n in kinds:
            lines += [f"{m:03d},{kind},{k},N{k},{0.1 * (m + 1):.7g}" for k in range(n)]
    return "\n".join(lines) + "\n"


def test_read_ensemble_checks_the_row_order():
    kinds = [("bias", 2), ("time_const", 2), ("syn_strength", 3)]
    e = er.read_ensemble(_small_csv(kinds, 2), kinds, 2)
    names, values = e["001"]["syn_strength"]
    assert names == ["N0", "N1", "N2"] and values.dtype == np.float32
    assert values[0] == np.float32(0.2)
    swapped = _small_csv(kinds, 2).replace("000,bias,1,", "000,bias,9,")
    raises(lambda: er.read_ensemble(swapped, kinds, 2), "expected 000,bias,1")
    raises(lambda: er.read_ensemble("# a\n# b\nmodel,kind,index,name,value\n", kinds, 2), "3 '#' lines")


def test_read_ensemble_real_file():
    needs(er.ENSEMBLE_CSV)
    text = er.ENSEMBLE_CSV.read_text()
    assert len(text.encode()) == 1_444_857
    e = er.read_ensemble(text)
    assert sorted(e) == [f"{m:03d}" for m in range(50)]
    p = er.parse_pretrained(er.PRETRAINED_RS.read_text())
    assert np.array_equal(e["000"]["bias"][1].view(np.uint32), p["BIAS"].view(np.uint32))
    assert e["000"]["bias"][0] == list(p["CELL_TYPES"])


# -- runner (keep last) ---------------------------------------------------------

def main(argv) -> int:
    if argv[:1] == ["--validate"]:
        return validate_dir(pathlib.Path(argv[1]))
    names = argv or sorted(n for n in globals() if n.startswith("test_"))
    failed = 0
    for name in names:
        try:
            globals()[name]()
            print(f"ok    {name}")
        except Skip as e:
            print(f"skip  {name}: {e}")
        except Exception:
            failed += 1
            print(f"FAIL  {name}")
            traceback.print_exc()
    print(f"{len(names) - failed} passed, {failed} failed")
    return 1 if failed else 0


def validate_dir(directory: pathlib.Path) -> int:
    """The manifest field table and every file's length and sha256, on a
    real export directory."""
    m = json.loads((directory / "manifest.json").read_text())
    er.validate_manifest(m)
    er.verify_files(m["files"], directory)
    names = sorted(p.name for p in directory.iterdir())
    want = sorted([f["name"] for f in m["files"]] + ["manifest.json", "PROVENANCE.json"])
    assert names == want, names
    print(f"{directory}: manifest valid, {len(m['files'])} files verified, {len(names)} files in all")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: FAIL at import, with `ModuleNotFoundError: No module named 'export_reference'`.

- [ ] **Step 3: Create the module with its constants, preflight, lock and parameter readers**

Create `tools/export_reference.py` with exactly this content. Every later task appends a section to the end of this file, separated by two blank lines.

```python
"""Export flyvis's own voltage traces as the reference for epic 0.3.0's gates.

0.3.0 proves that a Rust port of flyvis's dynamics reproduces flyvis's own
traces. Every later gate compares against the files written here, so the
export first proves three things, all in memory, before it writes anything:

  - the parameters flyvis loaded are bit-for-bit the ones Rust holds
    (crates/flyvis/src/pretrained.rs, data/flyvis-pretrained/ensemble.csv)
  - the float64 run really is float64, end to end
  - the run is deterministic across thread counts

It runs flyvis 1.2.0's stock path -- NetworkView, init_network("best"),
steady_state, forward -- for 2 models (000 best, 049 worst) x 2 dts (1/200,
0.02) x 2 precisions (f32, f64) = 8 runs. Each run settles for 1 s of grey and
then plays 4 flashes and 8 moving edges (speed 13, height 80) from the settled
state. The f32/f64 pair is stored so the later gates can take their tolerance
from the measured gap rather than from a guess.

Outputs, under --out (default <repo>/data):

  flyvis-reference/      settled, settling_centre, snapshots and centre for
                         each precision, the input movies flyvis fed
                         (input.f64.bin), manifest.json (deterministic) and
                         PROVENANCE.json (volatile)
  flyvis-stimuli/        moving_edge_offsets_h80.{bin,json} and
                         moving_edges_h80_provenance.json -- the height-80
                         geometry for the Rust gate. No existing file there is
                         opened for writing.

Inputs are always read from <repo>/data, whatever --out says.

flyvis runs under its own scratch root, target/flyvis-reference-root/, which
this script wipes and recreates every run: the two model directories are
copied there and sha256-checked, so nothing under data/ is written except the
outputs. An inherited FLYVIS_ROOT_DIR is refused.

Every `.bin` is raw little-endian, C order, no header; manifest.json lists
each file's blocks in storage order. stimulus_frames in the manifest is a
two-integer JSON array [start, end] with end exclusive. After `end` a flash
returns to the grey baseline, while a moving edge holds its last rendered
frame (post_pad_mode "continue").

Spec: docs/superpowers/specs/2026-10-26-flyvis-reference-traces-design.md

Run by hand:  uv run --project tools tools/export_reference.py [--force] [--out DIR]
"""

import argparse
import contextlib
import datetime
import fcntl
import hashlib
import json
import os
import pathlib
import re
import shutil
import statistics
import sys
import time

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "data"                # default --out; outputs only
DATA = ROOT / "data"               # inputs, always -- never follows --out

RESULTS = DATA / "flyvis-pretrained" / "results" / "flow" / "0000"
ENSEMBLE_CSV = DATA / "flyvis-pretrained" / "ensemble.csv"
PRETRAINED_RS = ROOT / "crates" / "flyvis" / "src" / "pretrained.rs"
NODES_CSV = DATA / "flyvis-build" / "nodes.csv"
FLASHES_BIN = DATA / "flyvis-stimuli" / "flashes.bin"
TIMING_JSON = DATA / "flyvis-stimuli" / "moving_edge_timing.json"
UV_LOCK = ROOT / "tools" / "uv.lock"

# flyvis writes connectome/, renderings/ and each model's __cache__/ under its
# root. Pointing it at scratch keeps all of that out of data/ (owner decision O3).
SCRATCH = ROOT / "target" / "flyvis-reference-root"
SCRATCH_REL = "target/flyvis-reference-root"     # what PROVENANCE records
# matplotlib (imported by flyvis) writes a font cache; keep it out of ~.
MPLCONFIG = ROOT / "target" / "flyvis-reference-mplconfig"
# Held for the whole run, so a second export cannot wipe this one's scratch root.
LOCK = ROOT / "target" / "flyvis-reference.lock"

GENERATOR = "tools/export_reference.py"
FLYVIS_VERSION = "1.2.0"

# Best and worst of the 50 by validation loss, recomputed and asserted (C16).
MODELS = [
    ("000", "flow/0000/000", 5.136557016107771, "best"),
    ("049", "flow/0000/049", 5.677851685219341, "worst"),
]
N_MODELS = 50
# The entries of a model directory that flyvis's load path reads. __cache__
# and anything else is deliberately not copied.
MODEL_ENTRIES = ["_meta.yaml", "chkpts", "validation", "best_chkpt", "validation_loss.h5"]
CHECKPOINT = pathlib.Path("chkpts") / "chkpt_00000"   # the file init_network("best") loads

# dt id -> the Python float flyvis is given, and int(t_pre / dt) settling steps.
DTS = [("1/200", 1.0 / 200.0, 200), ("1/50", 0.02, 50)]
DT_VALUE = {i: dt for i, dt, _ in DTS}
N_SETTLE = {i: n for i, _, n in DTS}
# f32(dt) widened: flyvis floors tau at torch.tensor(dt).float() in both precisions.
FLOOR_DT_F32 = {"1/200": 0.004999999888241291, "1/50": 0.019999999552965164}

# (index, model, dt id, precision), in storage order. Execution order differs.
RUNS = [
    (0, "000", "1/200", "f32"),
    (1, "000", "1/200", "f64"),
    (2, "000", "1/50", "f32"),
    (3, "000", "1/50", "f64"),
    (4, "049", "1/200", "f32"),
    (5, "049", "1/200", "f64"),
    (6, "049", "1/50", "f32"),
    (7, "049", "1/50", "f64"),
]
# The default dtype is process-wide: all f32 runs, then the switch, then f64.
EXECUTION_ORDER = [0, 2, 4, 6, 1, 3, 5, 7]

T_PRE = 1.0              # 1 s of grey before every sequence, as flyvis's analyses use
PRE_STIM_VALUE = 0.5     # the grey level
T_FADE_IN = 0.0          # no fade-in: stimulus_response's default
BATCH_SIZE = 1           # flyvis's stock analysis uses 4; nothing here needs it (C13)

N_NODES = 45669
N_HEXALS = 721
N_CENTRE = 65            # exactly one node per cell type at (u, v) = (0, 0)
N_PHOTORECEPTOR_TYPES = 8
N_SNAPSHOTS = 6
FIRST_CENTRE = (360, "R1")

GATE_ANGLES = [0, 30, 90, 150]   # no rotation / bilinear / axis-aligned / bilinear
EDGE_SPEED = 13                  # one speed: 525 frames at 1/200, no NaN padding
EDGE_HEIGHT = 80                 # flyvis's own analyses use 80, not the class default 9
N_OMMATIDIA = 721
PPO = 25
MONITOR = 775

FLASH_ARGS = [(0.5, 0.0, -1), (0.5, 0.0, 6), (0.5, 1.0, -1), (0.5, 1.0, 6)]  # baseline, intensity, radius
EDGE_ARGS = [(a, i) for a in GATE_ANGLES for i in (0.0, 1.0)]               # angle-major, OFF first

N_FRAMES = {
    "flashes": {"1/200": 600, "1/50": 150},
    "moving_edge": {"1/200": 525, "1/50": 131},
}
STIMULUS_FRAMES = {      # [start, end), end exclusive
    "flashes": {"1/200": [200, 400], "1/50": [50, 100]},
    "moving_edge": {"1/200": [200, 325], "1/50": [50, 81]},
}
EDGE_N_STIM_1_200 = 125
# torch.linspace(0, n - 1, 6).long(), flyvis's own truncation (F33).
SNAPSHOT_FRAMES = {
    600: [0, 119, 239, 359, 479, 599],
    525: [0, 104, 209, 314, 419, 524],
    150: [0, 29, 59, 89, 119, 149],
    131: [0, 26, 52, 78, 104, 130],
}

GAP_BOUND = 1e-2         # A7: a sanity bound on the f32/f64 gap. Never raise it.
BACKGROUND_TOL = 1e-6    # A9: the same tolerance the height-9 exporter uses

PRETRAINED_LENGTHS = {
    "CELL_TYPES": 65, "CONN_TYPES": 604, "BIAS": 65, "TIME_CONST": 65,
    "SYN_STRENGTH": 604, "SIGN": 604, "SYN_COUNT": 2355,
}
ENSEMBLE_KINDS = [("bias", 65), ("time_const", 65), ("syn_strength", 604)]

RECOVERY = """\
Recovering from an interrupted export:
  1. If <OUT>/flyvis-reference is missing and a .flyvis-reference.old-* exists,
     the stop fell between the two renames: rename that .old-* back to
     flyvis-reference. The previous set is then whole.
  2. Otherwise remove the .flyvis-reference.staging-*, .old-* and
     .incomplete-* leftovers and rerun with --force (or, once fixtures-2 is
     published, delete flyvis-reference/ and the three _h80 files and run
     ci/fetch-fixtures.sh)."""


class ExportError(Exception):
    """A failed check or refused precondition. main() prints it and exits 1."""


def require(ok, check: str, detail: str, run=None, sequence=None) -> None:
    """Raises ExportError naming the check, the run and the sequence.

    Used instead of `assert`, which `python -O` strips.
    """
    if not ok:
        where = []
        if run is not None:
            where.append(f"run {run}")
        if sequence is not None:
            where.append(f"sequence {sequence}")
        suffix = f" ({', '.join(where)})" if where else ""
        raise ExportError(f"{check} failed{suffix}: {detail}")


# -- command line, preflight and lock ----------------------------------------

def parse_args(argv):
    """`--out DIR` (relative to the cwd) and `--force`."""
    p = argparse.ArgumentParser(
        prog="export_reference.py",
        description="Export flyvis reference traces (see the module docstring).",
    )
    p.add_argument("--out", type=pathlib.Path, default=OUT,
                   help="where flyvis-reference/ and the _h80 files go (default: <repo>/data)")
    p.add_argument("--force", action="store_true",
                   help="replace outputs that already exist")
    args = p.parse_args(argv)
    args.out = args.out.expanduser().absolute()
    return args


H80_FILES = [
    "moving_edge_offsets_h80.bin",
    "moving_edge_offsets_h80.json",
    "moving_edges_h80_provenance.json",
]
LEFTOVER_PREFIXES = (
    ".flyvis-reference.staging-",
    ".flyvis-reference.old-",
    ".flyvis-reference.incomplete-",
)


def preflight(out: pathlib.Path, force: bool, environ) -> None:
    """Refuses to start, before flyvis is imported, when a run would clobber or
    be redirected. Leftovers of an interrupted run are refused even with
    --force: a human recovers from those."""
    if out.is_dir():
        for entry in sorted(out.iterdir()):
            if entry.name.startswith(LEFTOVER_PREFIXES):
                raise ExportError(
                    f"{entry} is left over from an interrupted export.\n{RECOVERY}"
                )
    if not force:
        existing = [out / "flyvis-reference"] + [out / "flyvis-stimuli" / f for f in H80_FILES]
        for path in existing:
            if path.exists():
                raise ExportError(f"{path} exists; rerun with --force to replace it")
    if "FLYVIS_ROOT_DIR" in environ:
        raise ExportError(
            "FLYVIS_ROOT_DIR is set in the environment; unset it. This script "
            f"runs flyvis under its own scratch root, {SCRATCH_REL}/"
        )


def acquire_lock(path: pathlib.Path):
    """Takes an exclusive, non-blocking flock on `path` and returns the open
    file, which must stay referenced for the lock to hold. The OS releases it
    when the process ends, however it ends."""
    path.parent.mkdir(parents=True, exist_ok=True)
    f = open(path, "w")
    try:
        fcntl.flock(f.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        f.close()
        raise ExportError(f"another export is running (it holds {path})") from None
    return f


# -- the parameters Rust holds -------------------------------------------------

_FLOAT = r"-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?"


def _static_body(text: str, name: str):
    m = re.search(
        rf"pub static {name}: \[(?P<ty>[^;]+); (?P<n>\d+)\] = \[(?P<body>.*?)\n\];",
        text,
        re.S,
    )
    if m is None:
        raise ExportError(f"A1 failed: no `pub static {name}` in pretrained.rs")
    return m.group("ty"), int(m.group("n")), m.group("body")


def parse_pretrained(text: str) -> dict:
    """The statics of crates/flyvis/src/pretrained.rs: name tables as lists of
    str (CONN_TYPES as (src, tar) tuples), values as float32 arrays.

    Every value was written as the shortest string that round-trips through
    float32, so np.float32(float(s)) recovers the exact bits."""
    out = {}
    for name, want in PRETRAINED_LENGTHS.items():
        ty, n, body = _static_body(text, name)
        if name == "CELL_TYPES":
            values = re.findall(r'"([^"]*)"', body)
        elif name == "CONN_TYPES":
            values = re.findall(r'\("([^"]*)", "([^"]*)"\)', body)
        else:
            require(ty == "f32", "A1", f"{name} is [{ty}; {n}], not f32")
            values = np.array([np.float32(float(s)) for s in re.findall(_FLOAT, body)],
                              dtype=np.float32)
        require(n == want, "A1", f"pretrained.rs declares {name}[{n}], expected {want}")
        require(len(values) == want, "A1", f"pretrained.rs {name} has {len(values)} values, expected {want}")
        out[name] = values
    return out


def read_ensemble(text: str, kinds=ENSEMBLE_KINDS, n_models: int = N_MODELS) -> dict:
    """ensemble.csv as {model id: {kind: (names, float32 values)}}.

    Asserts the layout the fixture contract states: 3 `#` lines, the header,
    then model 000..049, kind bias / time_const / syn_strength, index 0..n-1."""
    lines = text.splitlines()
    require(all(line.startswith("#") for line in lines[:3]), "A1", "ensemble.csv: expected 3 '#' lines")
    require(not lines[3].startswith("#"), "A1", "ensemble.csv: expected exactly 3 '#' lines")
    require(lines[3] == "model,kind,index,name,value", "A1", f"ensemble.csv header is {lines[3]!r}")
    rows = [line.split(",") for line in lines[4:] if line]
    per_model = sum(n for _, n in kinds)
    require(len(rows) == n_models * per_model, "A1",
            f"ensemble.csv has {len(rows)} rows, expected {n_models * per_model}")
    out = {}
    i = 0
    for m in range(n_models):
        mid = f"{m:03d}"
        out[mid] = {}
        for kind, n in kinds:
            names, values = [], []
            for k in range(n):
                model, got_kind, index, name, value = rows[i]
                require((model, got_kind, int(index)) == (mid, kind, k), "A1",
                        f"ensemble.csv row {i + 5} is {model},{got_kind},{index}; "
                        f"expected {mid},{kind},{k}")
                names.append(name)
                values.append(np.float32(float(value)))
                i += 1
            out[mid][kind] = (names, np.array(values, dtype=np.float32))
    return out
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: 12 `ok` lines and `12 passed, 0 failed`. (`test_read_ensemble_real_file` prints `skip` instead of `ok` only if `data/flyvis-pretrained/ensemble.csv` is absent; it is present on the owner's machine, and a skip here is not acceptable for this task.)

- [ ] **Step 5: Document the script in `tools/README.md`**

In the `## Scripts` table, add this row after the `export_moving_edges.py` row:

```markdown
| `export_reference.py` | flyvis's own voltage traces for 0.3.0's gates — models 000 and 049, dt 1/200 and 0.02, float32 and float64: the settled state, the centre column at every settling step and frame, all cells at 6 frames per sequence, and the input movies flyvis fed — plus the moving-edge geometry at height 80. Every check runs before anything is written | `data/flyvis-reference/`, and the three `*_h80*` files in `data/flyvis-stimuli/` |
```

Then add these two paragraphs after the table, before `## Provenance rule`:

```markdown
`export_reference.py` sets `FLYVIS_ROOT_DIR` itself, to a scratch root under
`target/` that it wipes and recreates on every run, and refuses to start if the
variable is already set: flyvis writes `connectome/`, `renderings/` and each
model's `__cache__/` under its root, and those caches' keys ignore the torch
dtype. It writes nothing under `data/` except its outputs, refuses to replace
them without `--force`, and holds a lock so two runs cannot share the scratch
root. Its self-tests need no flyvis:
`uv run --project tools tools/test_export_reference.py`.

**Do not re-export a pinned fixture by hand.** `ci/fetch-fixtures.sh` checks
every file in `ci/fixtures.sha256` byte for byte, and every export is dated —
in its provenance file, or for `ensemble.csv` in its header. Re-running
`export_params.py`, or any exporter above, therefore makes the fetch script
refuse the new file. To return to the published set, delete the re-exported
files and run `ci/fetch-fixtures.sh`; a deliberate re-export needs a new
`fixtures-N`, with `ci/fixtures.sha256` and `ci/fixtures-tag` changed in one
commit.
```

- [ ] **Step 6: Commit**

```bash
git add tools/export_reference.py tools/test_export_reference.py tools/README.md
git commit -m "Add the reference exporter's command line, preflight and parameter readers"
```

---

### Task 2: The storage layout and the manifest

**Files:**
- Modify: `tools/export_reference.py` (append a section)
- Test: `tools/test_export_reference.py` (insert a block above `# -- runner (keep last)`)

**Interfaces:**
- Consumes: Task 1's constants and `require`, `ExportError`.
- Produces:
  - constants `FRAME_SEMANTICS` (the exact three ASCII strings), `FILE_STEMS`, `INPUT_FILE`, `DTYPE`, `ITEMSIZE`, `NUMPY_DTYPE`, `MANIFEST_KEYS`
  - `sequence_table() -> list[dict]` (12 entries: `index`, `family`, `flyvis_index`, `args`, `frames[dt id] = {n_frames, stimulus_frames: [start, end], snapshot_frames}`)
  - `_file_entry(name, dtype, axes, blocks: list[(run, dt_id, sequence, shape)]) -> dict`
  - `file_layout() -> list[dict]` (9 entries in manifest order, `sha256` None)
  - `pack_file(entry: dict, arrays: dict[(run, dt_id, sequence) -> np.ndarray]) -> bytes`
  - `build_manifest(centre_nodes: list[(int, str)], files: list[dict]) -> dict`
  - `manifest_bytes(manifest: dict) -> bytes`
  - `validate_manifest(m: dict) -> None` (raises `ExportError`)
  - `sha256_file(path) -> str`, `verify_files(files: list[dict], directory) -> None`
  - `h80_sidecar() -> dict`, `json_bytes(obj) -> bytes`

`stimulus_frames` is written as a two-integer JSON array `[start, end]` with `end` exclusive, exactly the spec's `[start, end)`. After `end` a flash is back at the grey baseline and a moving edge holds its last rendered frame (`post_pad_mode "continue"`); the module docstring says so.

- [ ] **Step 1: Write the failing tests**

Insert this block above the `# -- runner (keep last)` line of `tools/test_export_reference.py`, followed by two blank lines:

```python
# -- Task 2: storage layout and manifest -------------------------------------------

SIZES = {   # the spec's Sizes table
    "settled.f32.bin": 730_704, "settled.f64.bin": 1_461_408,
    "settling_centre.f32.bin": 130_000, "settling_centre.f64.bin": 260_000,
    "snapshots.f32.bin": 52_610_688, "snapshots.f64.bin": 105_221_376,
    "centre.f32.bin": 4_288_960, "centre.f64.bin": 8_577_920,
    "input.f64.bin": 47_574_464,
}


def _centre_nodes():
    return [(360 + 721 * i, f"T{i}") if i else (360, "R1") for i in range(65)]


def _manifest():
    files = er.file_layout()
    for f in files:
        f["sha256"] = "0" * 64
    return er.build_manifest(_centre_nodes(), files)


def test_file_layout_matches_the_sizes_table():
    files = er.file_layout()
    assert [f["name"] for f in files] == list(SIZES)
    assert {f["name"]: f["bytes"] for f in files} == SIZES
    assert sum(SIZES.values()) == 220_855_520
    assert sum(v for k, v in SIZES.items() if not k.startswith("input")) == 173_281_056


def test_blocks_are_contiguous_and_in_storage_order():
    for f in er.file_layout():
        offset = 0
        for b in f["blocks"]:
            assert b["offset"] == offset, (f["name"], b)
            assert b["bytes"] == int(np.prod(b["shape"])) * er.ITEMSIZE[f["dtype"]]
            offset += b["bytes"]
        assert offset == f["bytes"]
    centre = {f["name"]: f for f in er.file_layout()}["centre.f32.bin"]
    assert [(b["run"], b["sequence"]) for b in centre["blocks"][:13]] == [(0, k) for k in range(12)] + [(2, 0)]
    assert centre["blocks"][4]["shape"] == [525, 65] and centre["blocks"][12]["shape"] == [150, 65]
    inp = {f["name"]: f for f in er.file_layout()}["input.f64.bin"]
    assert [(b["run"], b["dt"], b["sequence"]) for b in inp["blocks"][11:13]] == [(None, "1/200", 11), (None, "1/50", 0)]


def test_sequence_table_matches_the_spec():
    seqs = er.sequence_table()
    assert len(seqs) == 12
    assert seqs[0]["args"] == {"baseline": 0.5, "intensity": 0.0, "radius": -1}
    assert seqs[3]["args"] == {"baseline": 0.5, "intensity": 1.0, "radius": 6}
    assert [(s["args"]["angle"], s["args"]["intensity"]) for s in seqs[4:]] == \
        [(0, 0.0), (0, 1.0), (30, 0.0), (30, 1.0), (90, 0.0), (90, 1.0), (150, 0.0), (150, 1.0)]
    assert seqs[4]["flyvis_index"] == 0 and seqs[11]["flyvis_index"] == 7
    assert seqs[5]["frames"]["1/50"] == {"n_frames": 131, "stimulus_frames": [50, 81],
                                         "snapshot_frames": [0, 26, 52, 78, 104, 130]}
    assert seqs[2]["frames"]["1/200"]["stimulus_frames"] == [200, 400]


def test_manifest_is_deterministic_and_valid():
    a, b = er.manifest_bytes(_manifest()), er.manifest_bytes(_manifest())
    assert a == b and a.endswith(b"}\n")
    m = json.loads(a)
    er.validate_manifest(m)
    assert list(m) == er.MANIFEST_KEYS
    assert m["dts"][0]["floor_dt_f32"] == 0.004999999888241291
    assert b'"floor_dt_f32": 0.019999999552965164' in a
    assert m["centre_nodes"][0] == {"node": 360, "cell_type": "R1"}
    assert "`" not in json.dumps(m["frame_semantics"]) and "**" not in json.dumps(m["frame_semantics"])


def test_validate_manifest_rejects_what_the_field_table_forbids():
    cases = [
        (lambda m: m["files"][0].update(dtype="<f8"), "dtype"),
        (lambda m: m["centre_nodes"].pop(), "64 centre nodes"),
        (lambda m: m["files"][2]["blocks"][1].update(offset=1), "block at 1"),
        (lambda m: m.update(extra=1), "top-level keys"),
        (lambda m: m["files"][0].update(sha256="ABC"), "sha256"),
        (lambda m: m["runs"].reverse(), "runs"),
        (lambda m: m.update(t_pre=1), "t_pre"),
    ]
    for mutate, needle in cases:
        m = json.loads(er.manifest_bytes(_manifest()))
        mutate(m)
        raises(lambda: er.validate_manifest(m), needle)


def test_validate_manifest_refuses_numpy_scalars():
    # Review focus 5: a numpy int or float would change the JSON or crash it.
    m = _manifest()
    m["files"][0]["blocks"][0]["shape"] = [np.int64(45669)]
    raises(lambda: er.validate_manifest(m), "int64")
    m = _manifest()
    m["sequences"][0]["args"]["intensity"] = np.float32(0.0)
    raises(lambda: er.validate_manifest(m), "float32")


def test_pack_file_keeps_bits_and_refuses_a_silent_cast():
    # Review focus 3: an f32 run's array must never be widened into the f64 file.
    entry = er._file_entry("x.f32.bin", "<f4", ["a"], [(0, "1/200", None, [2]), (2, "1/50", None, [1])])
    arrays = {(0, "1/200", None): np.array([1.0, -0.0], dtype=np.float32),
              (2, "1/50", None): np.array([0.5], dtype=np.float32)}
    data = er.pack_file(entry, arrays)
    assert data == bytes.fromhex("0000803f" "00000080" "0000003f")
    bad = dict(arrays)
    bad[(2, "1/50", None)] = np.array([0.5], dtype=np.float64)
    raises(lambda: er.pack_file(entry, bad), "float64", "<f4")
    bad[(2, "1/50", None)] = np.array([0.5, 0.5], dtype=np.float32)
    raises(lambda: er.pack_file(entry, bad), "shape")
    # The widening direction: a float32 array into an <f8 file is refused too.
    e64 = er._file_entry("x.f64.bin", "<f8", ["a"], [(1, "1/200", None, [1])])
    assert er.pack_file(e64, {(1, "1/200", None): np.array([0.5], dtype=np.float64)}) == bytes.fromhex("000000000000e03f")
    raises(lambda: er.pack_file(e64, {(1, "1/200", None): np.array([0.5], dtype=np.float32)}), "float32", "<f8")


def test_verify_files_catches_length_and_sha256():
    d = tmpdir()
    (d / "a.f32.bin").write_bytes(b"\0" * 8)
    entry = er._file_entry("a.f32.bin", "<f4", ["a"], [(0, "1/200", None, [2])])
    entry["sha256"] = er.sha256_file(d / "a.f32.bin")
    er.verify_files([entry], d)
    (d / "a.f32.bin").write_bytes(b"\0" * 7 + b"\1")
    raises(lambda: er.verify_files([entry], d), "sha256")
    (d / "a.f32.bin").write_bytes(b"\0" * 4)
    raises(lambda: er.verify_files([entry], d), "4 bytes")


def test_h80_sidecar_is_the_h9_format_plus_height():
    side = er.h80_sidecar()
    text = er.json_bytes(side).decode()
    assert '\n  "height": 80\n' in text, text[-40:]
    assert side["shape"] == [8, 21, 721] and side["dtype"] == "<f4"
    assert [(g["index"], g["angle"], g["intensity"]) for g in side["groups"][:3]] == [(0, 0, 0.0), (1, 0, 1.0), (2, 30, 0.0)]
    h9 = er.DATA / "flyvis-stimuli" / "moving_edge_offsets.json"
    needs(h9)
    assert json.loads(h9.read_text()) == {k: v for k, v in side.items() if k != "height"}
```

- [ ] **Step 2: Run the tests to see the new ones fail**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: the 9 new tests print `FAIL`, each with `AttributeError: module 'export_reference' has no attribute ...` (`file_layout`, `sequence_table`, `build_manifest`, `pack_file`, `_file_entry`, `h80_sidecar`); the summary reads `12 passed, 9 failed`.

- [ ] **Step 3: Append the layout and manifest section**

Append to the end of `tools/export_reference.py`, after two blank lines:

```python
# -- storage layout and manifest -----------------------------------------------

FRAME_SEMANTICS = {
    "settle_step": (
        "settle_step i: the state after i+1 Euler steps from the bias initial "
        "state, with grey 0.5 on every photoreceptor. Its time is (i+1)*dt. "
        "settled equals settle step n_settle-1, in every node."
    ),
    "frame": (
        "frame i: the state after i+1 Euler steps from the settled state, where "
        "step i+1 used input frame i. Its time is (i+1)*dt after the sequence "
        "starts. Frame 0 is not the settled state. flyvis's own analysis labels "
        "frame 0 as -t_pre. Do not copy that."
    ),
    "time_s": "(i + 1) * dt",
}

FILE_STEMS = [
    ("settled", ["node"]),
    ("settling_centre", ["settle_step", "centre_node"]),
    ("snapshots", ["snapshot", "node"]),
    ("centre", ["frame", "centre_node"]),
]
INPUT_FILE = ("input", ["frame", "hexal"])
DTYPE = {"f32": "<f4", "f64": "<f8"}
ITEMSIZE = {"<f4": 4, "<f8": 8}
NUMPY_DTYPE = {"<f4": np.float32, "<f8": np.float64}

MANIFEST_KEYS = [
    "schema_version", "generator", "flyvis_version", "byte_order", "array_order",
    "n_nodes", "n_hexals", "t_pre", "pre_stim_value", "batch_size", "dts",
    "models", "runs", "frame_semantics", "centre_nodes", "transients_visible_on",
    "sequences", "files",
]


def sequence_table() -> list:
    """The 12 sequences: 4 flashes then 8 moving edges, in flyvis's arg_df order."""
    def frames(family):
        out = {}
        for dt_id, _, _ in DTS:
            n = N_FRAMES[family][dt_id]
            out[dt_id] = {
                "n_frames": n,
                "stimulus_frames": list(STIMULUS_FRAMES[family][dt_id]),
                "snapshot_frames": list(SNAPSHOT_FRAMES[n]),
            }
        return out

    seqs = []
    for k, (baseline, intensity, radius) in enumerate(FLASH_ARGS):
        seqs.append({
            "index": k,
            "family": "flashes",
            "flyvis_index": k,
            "args": {"baseline": baseline, "intensity": intensity, "radius": radius},
            "frames": frames("flashes"),
        })
    for k, (angle, intensity) in enumerate(EDGE_ARGS):
        seqs.append({
            "index": len(FLASH_ARGS) + k,
            "family": "moving_edge",
            "flyvis_index": k,
            "args": {
                "angle": angle, "intensity": intensity, "speed": float(EDGE_SPEED),
                "width": 80, "height": EDGE_HEIGHT, "offsets": [-10, 11],
                "t_pre": 1.0, "t_post": 1.0, "post_pad_mode": "continue",
                "bar_loc_horizontal": 0.0,
            },
            "frames": frames("moving_edge"),
        })
    return seqs


def _file_entry(name: str, dtype: str, axes: list, blocks: list) -> dict:
    itemsize = ITEMSIZE[dtype]
    out_blocks, offset = [], 0
    for run, dt_id, sequence, shape in blocks:
        nbytes = int(np.prod(shape)) * itemsize
        out_blocks.append({"run": run, "dt": dt_id, "sequence": sequence,
                           "offset": offset, "bytes": nbytes, "shape": list(shape)})
        offset += nbytes
    return {"name": name, "dtype": dtype, "axes": list(axes), "bytes": offset,
            "sha256": None, "blocks": out_blocks}


def file_layout() -> list:
    """Every .bin file's entry in manifest order, sha256 still None."""
    seqs = sequence_table()
    files = []
    for stem, axes in FILE_STEMS:
        for prec in ("f32", "f64"):
            blocks = []
            for run, _, dt_id, p in RUNS:
                if p != prec:
                    continue
                if stem == "settled":
                    blocks.append((run, dt_id, None, [N_NODES]))
                elif stem == "settling_centre":
                    blocks.append((run, dt_id, None, [N_SETTLE[dt_id], N_CENTRE]))
                else:
                    for s in seqs:
                        n = s["frames"][dt_id]["n_frames"]
                        shape = [N_SNAPSHOTS, N_NODES] if stem == "snapshots" else [n, N_CENTRE]
                        blocks.append((run, dt_id, s["index"], shape))
            files.append(_file_entry(f"{stem}.{prec}.bin", DTYPE[prec], axes, blocks))
    stem, axes = INPUT_FILE
    blocks = [(None, dt_id, s["index"], [s["frames"][dt_id]["n_frames"], N_HEXALS])
              for dt_id, _, _ in DTS for s in seqs]
    files.append(_file_entry(f"{stem}.f64.bin", "<f8", axes, blocks))
    return files


def pack_file(entry: dict, arrays: dict) -> bytes:
    """Concatenates the blocks of one file. `arrays` maps (run, dt id,
    sequence) to an array whose shape is the block's and whose dtype is
    already the file's: nothing is widened or narrowed here, only byte order."""
    dtype = entry["dtype"]
    parts = []
    for b in entry["blocks"]:
        key = (b["run"], b["dt"], b["sequence"])
        require(key in arrays, "write", f"{entry['name']}: no array for block {key}")
        a = np.asarray(arrays[key])
        require(a.dtype == NUMPY_DTYPE[dtype], "write",
                f"{entry['name']} block {key} is {a.dtype}, the file is {dtype}")
        require(list(a.shape) == b["shape"], "write",
                f"{entry['name']} block {key} has shape {list(a.shape)}, expected {b['shape']}")
        data = a.astype(dtype).tobytes(order="C")
        require(len(data) == b["bytes"], "write", f"{entry['name']} block {key}: byte count")
        parts.append(data)
    out = b"".join(parts)
    require(len(out) == entry["bytes"], "write", f"{entry['name']}: total byte count")
    return out


def build_manifest(centre_nodes: list, files: list) -> dict:
    """manifest.json's content, key order as the spec's field table."""
    return {
        "schema_version": 1,
        "generator": GENERATOR,
        "flyvis_version": FLYVIS_VERSION,
        "byte_order": "little",
        "array_order": "C",
        "n_nodes": N_NODES,
        "n_hexals": N_HEXALS,
        "t_pre": T_PRE,
        "pre_stim_value": PRE_STIM_VALUE,
        "batch_size": BATCH_SIZE,
        "dts": [{"id": i, "dt": dt, "floor_dt_f32": FLOOR_DT_F32[i], "n_settle": n}
                for i, dt, n in DTS],
        "models": [{"id": m, "name": name, "validation_loss": loss, "rank": rank}
                   for m, name, loss, rank in MODELS],
        "runs": [{"index": r, "model": m, "dt": d, "precision": p} for r, m, d, p in RUNS],
        "frame_semantics": dict(FRAME_SEMANTICS),
        "centre_nodes": [{"node": int(n), "cell_type": str(t)} for n, t in centre_nodes],
        "transients_visible_on": "centre",
        "sequences": sequence_table(),
        "files": files,
    }


def manifest_bytes(manifest: dict) -> bytes:
    return (json.dumps(manifest, indent=2, allow_nan=False, sort_keys=False) + "\n").encode()


def _only_builtin(obj, where="manifest"):
    """No numpy scalars, no bools posing as ints: json would either refuse
    them or write them differently from the spec's types."""
    if isinstance(obj, dict):
        for k, v in obj.items():
            require(type(k) is str, "manifest", f"{where}: key {k!r} is not str")
            _only_builtin(v, f"{where}.{k}")
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            _only_builtin(v, f"{where}[{i}]")
    else:
        require(obj is None or type(obj) in (int, float, str), "manifest",
                f"{where} is a {type(obj).__name__}")


def validate_manifest(m: dict) -> None:
    """Checks manifest.json against the spec's field table. Raises ExportError."""
    def need(ok, detail):
        require(ok, "manifest", detail)

    _only_builtin(m)
    need(list(m) == MANIFEST_KEYS, f"top-level keys {list(m)}")
    fixed = {"schema_version": 1, "generator": GENERATOR, "flyvis_version": FLYVIS_VERSION,
             "byte_order": "little", "array_order": "C", "n_nodes": N_NODES,
             "n_hexals": N_HEXALS, "t_pre": 1.0, "pre_stim_value": 0.5, "batch_size": 1,
             "transients_visible_on": "centre", "frame_semantics": FRAME_SEMANTICS}
    for k, v in fixed.items():
        need(m[k] == v and type(m[k]) is type(v), f"{k} is {m[k]!r}, expected {v!r}")
    need(m["dts"] == [{"id": "1/200", "dt": 0.005, "floor_dt_f32": 0.004999999888241291, "n_settle": 200},
                      {"id": "1/50", "dt": 0.02, "floor_dt_f32": 0.019999999552965164, "n_settle": 50}],
         f"dts {m['dts']}")
    need(m["models"] == [{"id": "000", "name": "flow/0000/000", "validation_loss": 5.136557016107771, "rank": "best"},
                         {"id": "049", "name": "flow/0000/049", "validation_loss": 5.677851685219341, "rank": "worst"}],
         f"models {m['models']}")
    need(all(list(r) == ["index", "model", "dt", "precision"] for r in m["runs"]), "runs key order")
    need([(r["index"], r["model"], r["dt"], r["precision"]) for r in m["runs"]] == RUNS, "runs")
    c = m["centre_nodes"]
    need(len(c) == N_CENTRE, f"{len(c)} centre nodes, expected {N_CENTRE}")
    need(all(list(e) == ["node", "cell_type"] for e in c), "centre_nodes keys")
    need([e["node"] for e in c] == sorted({e["node"] for e in c}), "centre_nodes not strictly ascending")
    need(len({e["cell_type"] for e in c}) == N_CENTRE, "centre_nodes cell types not one per type")
    need((c[0]["node"], c[0]["cell_type"]) == FIRST_CENTRE, f"first centre node {c[0]}")
    need(m["sequences"] == sequence_table(), "sequences differ from the spec's table")
    seq_ids = {s["index"] for s in m["sequences"]}
    run_ids = {r["index"]: r for r in m["runs"]}
    dt_ids = [d["id"] for d in m["dts"]]
    want_files = [f"{stem}.{p}.bin" for stem, _ in FILE_STEMS for p in ("f32", "f64")] + ["input.f64.bin"]
    need([f["name"] for f in m["files"]] == want_files, f"files {[f['name'] for f in m['files']]}")
    layout = {f["name"]: f for f in file_layout()}
    for f in m["files"]:
        name = f["name"]
        need(list(f) == ["name", "dtype", "axes", "bytes", "sha256", "blocks"], f"{name}: keys {list(f)}")
        need(f["dtype"] == ("<f4" if ".f32." in name else "<f8"), f"{name}: dtype {f['dtype']} vs name")
        need(f["axes"] == layout[name]["axes"], f"{name}: axes {f['axes']}")
        need(isinstance(f["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", f["sha256"]) is not None,
             f"{name}: sha256 {f['sha256']!r}")
        offset = 0
        for b in f["blocks"]:
            need(list(b) == ["run", "dt", "sequence", "offset", "bytes", "shape"], f"{name}: block keys {list(b)}")
            need(b["offset"] == offset, f"{name}: block at {b['offset']}, expected {offset}")
            need(b["bytes"] == int(np.prod(b["shape"])) * ITEMSIZE[f["dtype"]], f"{name}: block bytes vs shape")
            need(b["dt"] in dt_ids, f"{name}: block dt {b['dt']}")
            if name.startswith("input"):
                need(b["run"] is None, f"{name}: input block has a run")
            else:
                need(b["run"] in run_ids and run_ids[b["run"]]["dt"] == b["dt"]
                     and run_ids[b["run"]]["precision"] == name.split(".")[1],
                     f"{name}: block run {b['run']}")
            if name.startswith(("settled", "settling_centre")):
                need(b["sequence"] is None, f"{name}: block has a sequence")
            else:
                need(b["sequence"] in seq_ids, f"{name}: block sequence {b['sequence']}")
            offset += b["bytes"]
        need(f["bytes"] == offset, f"{name}: bytes {f['bytes']} != sum of blocks {offset}")
        stripped = {k: v for k, v in f.items() if k != "sha256"}
        need(stripped == {k: v for k, v in layout[name].items() if k != "sha256"},
             f"{name}: differs from the storage layout")


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def verify_files(files: list, directory: pathlib.Path) -> None:
    """Every listed file exists in `directory` with the manifest's length,
    sha256 and block total."""
    for f in files:
        path = directory / f["name"]
        require(path.is_file(), "verify", f"{path} is missing")
        size = path.stat().st_size
        require(size == f["bytes"], "verify", f"{path} is {size} bytes, manifest says {f['bytes']}")
        require(sum(b["bytes"] for b in f["blocks"]) == size, "verify", f"{path}: blocks do not sum to the length")
        require(sha256_file(path) == f["sha256"], "verify", f"{path}: sha256 differs from the manifest")


def h80_sidecar() -> dict:
    """moving_edge_offsets_h80.json: the height-9 sidecar's format plus height."""
    groups = [
        {"index": i, "angle": int(a), "intensity": float(v)}
        for i, (a, v) in enumerate((a, v) for a in GATE_ANGLES for v in (0.0, 1.0))
    ]
    return {"shape": [len(GATE_ANGLES) * 2, 21, N_OMMATIDIA], "groups": groups,
            "dtype": "<f4", "height": EDGE_HEIGHT}


def json_bytes(obj) -> bytes:
    return (json.dumps(obj, indent=2, allow_nan=False) + "\n").encode()
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: `21 passed, 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add tools/export_reference.py tools/test_export_reference.py
git commit -m "Lay out the reference-trace files and validate their manifest"
```

---

### Task 3: Writing the set atomically

**Files:**
- Modify: `tools/export_reference.py` (append a section)
- Test: `tools/test_export_reference.py` (insert a block above `# -- runner (keep last)`)

**Interfaces:**
- Consumes: `require`, `ExportError`, `RECOVERY`, `H80_FILES` (Task 1); `manifest_bytes`, `verify_files`, `json_bytes`, `_file_entry`, `sha256_file` (Task 2).
- Produces:
  - `check_pair(out: pathlib.Path) -> None`
  - `write_outputs(out: pathlib.Path, ref_bins: list[(str, bytes)], provenance: bytes, manifest: dict, h80: list[(str, bytes)], export_id: str, hook=lambda point: None) -> None`. `hook` is called at `"staged"`, `"verified"`, `"old_moved"`, `"reference_placed"`, `"h80_placed"`; production passes nothing.

The writer implements write steps 1–6 exactly: `<OUT>` created if missing (and removed again if this run created it and fails before step 4); `.flyvis-reference.staging-<pid>/` holding `flyvis-reference/` and `flyvis-stimuli/`; binaries, then `PROVENANCE.json`, then `manifest.json` last, each `fsync`ed, then the `_h80` files with provenance last; every staged `.bin` re-read against the manifest; the `.incomplete-<pid>` marker holding `export_id`; `flyvis-reference` → `.old-<pid>` under `--force`; the staged set into place; `flyvis-stimuli/` created only now, the `_h80` files `os.replace`d `.bin`, `.json`, provenance; the pair check; `.old`, staging, then the marker removed last. From step 4 the `finally` deletes nothing, and renames `.old-<pid>` back only when `flyvis-reference` is missing. An exception from step 4 on is re-raised as an `ExportError` carrying `RECOVERY`.

- [ ] **Step 1: Write the failing tests**

Insert this block above the `# -- runner (keep last)` line, followed by two blank lines:

```python
# -- Task 3: the atomic write --------------------------------------------------------

def _tiny_set(export_id="E1", h80_id=None, payload=b"\1\2\3\4"):
    entry = er._file_entry("a.f32.bin", "<f4", ["a"], [(0, "1/200", None, [1])])
    entry["sha256"] = __import__("hashlib").sha256(payload).hexdigest()
    manifest = {"files": [entry]}
    prov = er.json_bytes({"export_id": export_id})
    h80 = [("moving_edge_offsets_h80.bin", b"\0" * 4),
           ("moving_edge_offsets_h80.json", b"{}\n"),
           ("moving_edges_h80_provenance.json", er.json_bytes({"export_id": h80_id or export_id}))]
    return [("a.f32.bin", payload)], prov, manifest, h80


def _listing(root: pathlib.Path):
    return sorted((str(p.relative_to(root)), p.read_bytes() if p.is_file() else None)
                  for p in root.rglob("*"))


def _crash_at(point):
    def hook(p):
        if p == point:
            raise RuntimeError(f"crash at {p}")
    return hook


def test_write_into_a_missing_out_leaves_only_the_outputs():
    out = tmpdir() / "fresh"
    er.write_outputs(out, *_tiny_set(), "E1")
    names = sorted(str(p.relative_to(out)) for p in out.rglob("*"))
    assert names == ["flyvis-reference", "flyvis-reference/PROVENANCE.json", "flyvis-reference/a.f32.bin",
                     "flyvis-reference/manifest.json", "flyvis-stimuli",
                     "flyvis-stimuli/moving_edge_offsets_h80.bin", "flyvis-stimuli/moving_edge_offsets_h80.json",
                     "flyvis-stimuli/moving_edges_h80_provenance.json"], names
    assert (out / "flyvis-reference" / "manifest.json").read_bytes().endswith(b"\n")


def test_a_failure_before_the_swap_leaves_out_exactly_as_it_was():
    for point in ("staged", "verified"):
        out = tmpdir()
        (out / "flyvis-stimuli").mkdir()
        (out / "flyvis-stimuli" / "flashes.bin").write_bytes(b"keep")
        before = _listing(out)
        try:
            er.write_outputs(out, *_tiny_set(), "E1", hook=_crash_at(point))
        except RuntimeError:
            pass
        else:
            raise AssertionError("no crash")
        assert _listing(out) == before, point
    out = tmpdir() / "created-by-this-run"
    try:
        er.write_outputs(out, *_tiny_set(), "E1", hook=_crash_at("verified"))
    except RuntimeError:
        pass
    assert not out.exists()


def test_a_staged_file_that_differs_from_the_manifest_is_never_published():
    out = tmpdir()

    def corrupt(point):
        if point == "staged":
            staged = next(out.glob(".flyvis-reference.staging-*")) / "flyvis-reference" / "a.f32.bin"
            staged.write_bytes(b"\x09\x09\x09\x09")
    raises(lambda: er.write_outputs(out, *_tiny_set(), "E1", hook=corrupt), "sha256")
    assert _listing(out) == []


def test_force_replaces_the_previous_set_and_cleans_up():
    out = tmpdir()
    er.write_outputs(out, *_tiny_set("OLD", payload=b"\0\0\0\0"), "OLD")
    er.write_outputs(out, *_tiny_set("NEW"), "NEW")
    assert (out / "flyvis-reference" / "a.f32.bin").read_bytes() == b"\1\2\3\4"
    assert json.loads((out / "flyvis-stimuli" / "moving_edges_h80_provenance.json").read_text())["export_id"] == "NEW"
    assert sorted(p.name for p in out.iterdir()) == ["flyvis-reference", "flyvis-stimuli"]


def test_a_crash_between_the_two_renames_restores_the_previous_set():
    # Review focus 2.
    out = tmpdir()
    er.write_outputs(out, *_tiny_set("OLD", payload=b"\0\0\0\0"), "OLD")
    msg = raises(lambda: er.write_outputs(out, *_tiny_set("NEW"), "NEW", hook=_crash_at("old_moved")),
                 "crash at old_moved", "rename that .old-* back")
    assert (out / "flyvis-reference" / "a.f32.bin").read_bytes() == b"\0\0\0\0", msg
    left = sorted(p.name.rsplit("-", 1)[0] for p in out.iterdir() if p.name.startswith("."))
    assert left == [".flyvis-reference.incomplete", ".flyvis-reference.staging"], left
    raises(lambda: er.preflight(out, True, {}), "interrupted")


def test_a_crash_after_the_new_set_is_placed_deletes_nothing():
    out = tmpdir()
    er.write_outputs(out, *_tiny_set("OLD", payload=b"\0\0\0\0"), "OLD")
    raises(lambda: er.write_outputs(out, *_tiny_set("NEW"), "NEW", hook=_crash_at("reference_placed")),
           "crash at reference_placed")
    assert (out / "flyvis-reference" / "a.f32.bin").read_bytes() == b"\1\2\3\4"
    left = sorted(p.name.rsplit("-", 1)[0] for p in out.iterdir() if p.name.startswith("."))
    assert left == [".flyvis-reference.incomplete", ".flyvis-reference.old", ".flyvis-reference.staging"], left


def test_a_mismatched_export_id_pair_fails_the_final_check():
    out = tmpdir()
    raises(lambda: er.write_outputs(out, *_tiny_set("A", h80_id="B"), "A"), "export_id mismatch")
    assert any(p.name.startswith(".flyvis-reference.incomplete-") for p in out.iterdir())


def test_a_missing_stimuli_dir_is_created_only_at_step_5():
    out = tmpdir()
    seen = {}

    def look(point):
        seen[point] = (out / "flyvis-stimuli").exists()
    er.write_outputs(out, *_tiny_set(), "E1", hook=look)
    assert seen == {"staged": False, "verified": False, "old_moved": False,
                    "reference_placed": False, "h80_placed": True}, seen
```

- [ ] **Step 2: Run the tests to see the new ones fail**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: the 8 new tests print `FAIL` with `AttributeError: module 'export_reference' has no attribute 'write_outputs'`; `21 passed, 8 failed`.

- [ ] **Step 3: Append the writer**

Append to the end of `tools/export_reference.py`, after two blank lines:

```python
# -- writing the set atomically ------------------------------------------------

def _fsync_dir(path: pathlib.Path) -> None:
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def _write_synced(path: pathlib.Path, data: bytes) -> None:
    with open(path, "xb") as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())


def check_pair(out: pathlib.Path) -> None:
    """All three _h80 files exist and the two export_ids agree."""
    for name in H80_FILES:
        require((out / "flyvis-stimuli" / name).is_file(), "pair", f"{out / 'flyvis-stimuli' / name} is missing")
    a = json.loads((out / "flyvis-reference" / "PROVENANCE.json").read_text())["export_id"]
    b = json.loads((out / "flyvis-stimuli" / "moving_edges_h80_provenance.json").read_text())["export_id"]
    require(a == b, "pair", f"export_id mismatch: {a} vs {b}")


def write_outputs(out: pathlib.Path, ref_bins: list, provenance: bytes, manifest: dict,
                  h80: list, export_id: str, hook=lambda point: None) -> None:
    """Write steps 1-6 of the spec's Failure handling.

    ref_bins: [(name, bytes)] in manifest order. h80: [(name, bytes)] for the
    .bin, the .json and the provenance, in that order. `hook(point)` is called
    at each named point; tests raise from it to simulate a crash there."""
    pid = os.getpid()
    staging = out / f".flyvis-reference.staging-{pid}"
    marker = out / f".flyvis-reference.incomplete-{pid}"
    old = out / f".flyvis-reference.old-{pid}"
    final = out / "flyvis-reference"
    stimuli = out / "flyvis-stimuli"
    created_out = False
    committed = False          # True from write step 4 on: the finally then deletes nothing
    try:
        # 1. <OUT>, then the staging directory beside the outputs.
        if not out.exists():
            out.mkdir(parents=True)
            created_out = True
        staging.mkdir()
        sref = staging / "flyvis-reference"
        sstim = staging / "flyvis-stimuli"
        sref.mkdir()
        sstim.mkdir()
        # 2. Binaries, then PROVENANCE, then the manifest last; the _h80 files
        #    the same way, provenance last.
        for name, data in ref_bins:
            _write_synced(sref / name, data)
        _write_synced(sref / "PROVENANCE.json", provenance)
        _write_synced(sref / "manifest.json", manifest_bytes(manifest))
        for name, data in h80:
            _write_synced(sstim / name, data)
        for d in (sref, sstim, staging):
            _fsync_dir(d)
        hook("staged")
        # 3. Re-read every staged .bin against the manifest.
        verify_files(manifest["files"], sref)
        hook("verified")
        # 4. The marker, then the swap.
        committed = True
        _write_synced(marker, (export_id + "\n").encode())
        _fsync_dir(out)
        if final.exists():
            os.rename(final, old)
        hook("old_moved")
        os.rename(sref, final)
        _fsync_dir(out)
        hook("reference_placed")
        # 5. The _h80 files, .bin first and provenance last.
        stimuli.mkdir(exist_ok=True)
        for name, _ in h80:
            os.replace(sstim / name, stimuli / name)
        _fsync_dir(stimuli)
        hook("h80_placed")
        # 6. The pair check, then clean up, marker last.
        check_pair(out)
        if old.exists():
            shutil.rmtree(old)
        shutil.rmtree(staging)
        marker.unlink()
        _fsync_dir(out)
    except Exception as e:
        if committed:
            raise ExportError(f"write failed after the swap began: {e}\n{RECOVERY}") from e
        raise
    finally:
        if not committed:
            shutil.rmtree(staging, ignore_errors=True)
            if created_out:
                with contextlib.suppress(OSError):
                    out.rmdir()
        elif old.exists() and not final.exists():
            os.rename(old, final)
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: `29 passed, 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add tools/export_reference.py tools/test_export_reference.py
git commit -m "Write the reference set atomically, restoring the previous one if interrupted"
```

---

### Task 4: Model ranking, all 50 checkpoints, and the freeze check

**Files:**
- Modify: `tools/export_reference.py` (append a section)
- Test: `tools/test_export_reference.py` (insert a block above `# -- runner (keep last)`)

**Interfaces:**
- Consumes: Task 1's constants, `parse_pretrained`, `read_ensemble`; Task 2's `sha256_file`.
- Produces:
  - `read_validation_losses(results: pathlib.Path) -> dict[str, float]` (h5py, `validation/loss.h5["data"]`, the 50 digit-named directories)
  - `rank_models(losses: dict) -> None`
  - `PARAM_KEYS` (`bias`→`nodes_bias`, `time_const`→`nodes_time_const`, `syn_strength`→`edges_syn_strength`, `sign`→`edges_sign`, `syn_count`→`edges_syn_count`)
  - `load_checkpoint(path) -> dict[str, np.ndarray]` (`torch.load(path, map_location="cpu", weights_only=True)["network"]`)
  - `bits_equal(a, b) -> bool` (numpy arrays or tensors; same dtype, shape and bits)
  - `reference_params(model: str, pretrained: dict, ensemble: dict) -> dict[str, np.ndarray]`
  - `check_checkpoints(ckpts, ensemble, pretrained) -> None`
  - `read_clamps(results, ids) -> dict[str, dict[str, str | None]]` (PyYAML `safe_load` of each `_meta.yaml`)
  - `freeze_figures(ckpts, clamps) -> dict` (groups `time_const`, `syn_strength`, `bias`)
  - `FREEZE_EXPECTED`, `check_freeze(figures, expected=FREEZE_EXPECTED) -> None`
  - `copy_models(src, dst, ids) -> dict[str, {"original_sha256", "scratch_sha256"}]`
  - `max_abs_gap(a, b) -> float`

A1 loads `chkpts/chkpt_00000` (`CHECKPOINT`), the file `init_network("best")` loads. `best_chkpt` holds bit-identical network tensors in all 50 models (research F11, re-measured while planning), so this is the same data `export_params.py` read. torch is imported inside the functions that need it; flyvis is never imported here, so these tests run real checkpoints without flyvis.

- [ ] **Step 1: Write the failing tests**

Insert this block above the `# -- runner (keep last)` line, followed by two blank lines:

```python
# -- Task 4: models, checkpoints, freeze check -------------------------------------

def test_bits_equal_is_bitwise():
    a = np.array([0.0, 1.0], dtype=np.float32)
    assert er.bits_equal(a, a.copy())
    assert not er.bits_equal(a, np.array([-0.0, 1.0], dtype=np.float32))
    assert not er.bits_equal(a, a.astype(np.float64))
    assert not er.bits_equal(a, a[:1])
    nan = np.array([np.nan], dtype=np.float64)
    assert er.bits_equal(nan, nan.copy())


def test_max_abs_gap_widens_exactly():
    a = np.array([0.1], dtype=np.float32)
    assert er.max_abs_gap(a, a.astype(np.float64)) == 0.0
    assert er.max_abs_gap(np.array([1.0]), np.array([1.5])) == 0.5
    raises(lambda: er.max_abs_gap(np.zeros(2), np.zeros(3)), "shapes")


def _fake_ckpts():
    f = np.float32
    return {
        "000": {"time_const": np.array([0.019, 0.05, 0.02], f), "syn_strength": np.array([0, 0, 1e-5], f),
                "bias": np.array([-1, 2], f)},
        "001": {"time_const": np.array([0.019, 0.019, 0.5], f), "syn_strength": np.array([0, 0.1, 0.2], f),
                "bias": np.array([0, 0], f)},
        "049": {"time_const": np.array([0.03, 0.03, 0.03], f), "syn_strength": np.array([0, 1e-3, 0], f),
                "bias": np.array([0.5, 0.5], f)},
    }


def test_freeze_figures_counts_what_the_spec_names():
    clamps = {m: {"bias": None, "time_const": None, "syn_strength": "non_negative"} for m in ("000", "001", "049")}
    fig = er.freeze_figures(_fake_ckpts(), clamps)
    tc, ss, bias = fig["time_const"], fig["syn_strength"], fig["bias"]
    assert tc["below_f32_0.02_total"] == 3 and tc["below_f32_0.02_000"] == 1 and tc["below_f32_0.02_049"] == 0
    assert tc["below_f32_0.02_per_model"] == {"min": 0, "median": 1, "max": 2}
    assert tc["in_f32_0.02_to_0.02"] == 1          # f32(0.02) itself sits in the gap
    assert tc["largest_below_floor"] == float(np.float32(0.019))
    assert ss["exact_zeros"] == 5 and ss["zeros_000"] == 2 and ss["zeros_049"] == 2
    assert ss["conn_types_zero_in_some_model"] == 3 and ss["conn_types_zero_in_all_models"] == 1
    assert ss["smallest_positive"] == float(np.float32(1e-5))
    assert ss["positive_at_or_below_1e-4"] == 1 and ss["positive_at_or_below_1e-2"] == 2
    assert bias == {"min": -1.0, "max": 2.0, "clamp": ["None"]}
    assert ss["clamp"] == ["non_negative"]


def test_check_freeze_reports_the_first_mismatch():
    clamps = {m: {"bias": None, "time_const": None, "syn_strength": "non_negative"} for m in ("000", "001", "049")}
    fig = er.freeze_figures(_fake_ckpts(), clamps)
    raises(lambda: er.check_freeze(fig), "time_const.below_f32_0.02_total is 3, expected 2396")
    er.check_freeze(fig, {("syn_strength", "exact_zeros"): 5})


def test_copy_models_copies_only_the_load_path_and_checks_it():
    src, dst = tmpdir(), tmpdir()
    for mid in ("000", "049"):
        d = src / mid
        (d / "chkpts").mkdir(parents=True)
        (d / "validation").mkdir()
        (d / "__cache__").mkdir()
        (d / "chkpts" / "chkpt_00000").write_bytes(mid.encode())
        (d / "validation" / "loss.h5").write_bytes(b"l")
        for f in ("_meta.yaml", "best_chkpt", "validation_loss.h5"):
            (d / f).write_bytes(f.encode())
        (d / "__cache__" / "junk").write_bytes(b"j")
    shas = er.copy_models(src, dst, ["000", "049"])
    assert sorted(p.name for p in (dst / "000").iterdir()) == sorted(er.MODEL_ENTRIES)
    assert shas["049"]["original_sha256"] == shas["049"]["scratch_sha256"] == er.sha256_file(src / "049" / er.CHECKPOINT)


def test_models_rank_as_recorded():
    needs(er.RESULTS)
    losses = er.read_validation_losses(er.RESULTS)
    assert len(losses) == 50
    er.rank_models(losses)
    bad = dict(losses)
    bad["020"] = 0.0
    raises(lambda: er.rank_models(bad), "best 020")


def test_all_50_checkpoints_and_the_freeze_figures():
    needs(er.RESULTS, er.ENSEMBLE_CSV)
    pre = er.parse_pretrained(er.PRETRAINED_RS.read_text())
    ens = er.read_ensemble(er.ENSEMBLE_CSV.read_text())
    ids = sorted(er.read_validation_losses(er.RESULTS))
    ckpts = {m: er.load_checkpoint(er.RESULTS / m / er.CHECKPOINT) for m in ids}
    er.check_checkpoints(ckpts, ens, pre)
    fig = er.freeze_figures(ckpts, er.read_clamps(er.RESULTS, ids))
    er.check_freeze(fig)
    assert fig["time_const"]["largest_below_floor"] == float(np.float32(0.019996209))
    assert fig["time_const"]["smallest_at_or_above_floor"] == float(np.float32(0.020369984))
    ref = er.reference_params("049", pre, ens)
    assert er.bits_equal(ckpts["049"]["syn_strength"], ref["syn_strength"])
    ckpts["007"]["syn_strength"] = ckpts["007"]["syn_strength"].copy()
    ckpts["007"]["syn_strength"][0] = -0.0 if ckpts["007"]["syn_strength"][0] == 0 else 0.0
    raises(lambda: er.check_checkpoints(ckpts, ens, pre), "checkpoint 007 syn_strength")
```

- [ ] **Step 2: Run the tests to see the new ones fail**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: the 7 new tests print `FAIL` with `AttributeError` (`bits_equal`, `max_abs_gap`, `freeze_figures`, `copy_models`, `read_validation_losses`); `29 passed, 7 failed`.

- [ ] **Step 3: Append the checkpoint section**

Append to the end of `tools/export_reference.py`, after two blank lines:

```python
# -- model ranking, checkpoints and the freeze check -----------------------------

def read_validation_losses(results: pathlib.Path) -> dict:
    """validation/loss.h5["data"] for every model directory, by id."""
    import h5py

    dirs = sorted(d for d in results.iterdir() if d.is_dir() and d.name.isdigit())
    require(len(dirs) == N_MODELS, "models", f"{len(dirs)} model directories in {results}, expected {N_MODELS}")
    losses = {}
    for d in dirs:
        with h5py.File(d / "validation" / "loss.h5", "r") as f:
            losses[d.name] = float(f["data"][()])
    return losses


def rank_models(losses: dict) -> None:
    """Asserts that 000 is the best and 049 the worst, at the recorded losses."""
    best = min(losses, key=losses.get)
    worst = max(losses, key=losses.get)
    (b_id, _, b_loss, _), (w_id, _, w_loss, _) = MODELS
    require((best, worst) == (b_id, w_id), "models", f"best {best}, worst {worst}; expected {b_id}, {w_id}")
    require(losses[best] == b_loss and losses[worst] == w_loss, "models",
            f"losses {losses[best]!r} / {losses[worst]!r}; expected {b_loss!r} / {w_loss!r}")


PARAM_KEYS = {
    "bias": "nodes_bias",
    "time_const": "nodes_time_const",
    "syn_strength": "edges_syn_strength",
    "sign": "edges_sign",
    "syn_count": "edges_syn_count",
}


def load_checkpoint(path: pathlib.Path) -> dict:
    """The five network tensors of one checkpoint as float32 numpy arrays,
    loaded with weights_only=True: no pickle code runs."""
    import torch

    net = torch.load(path, map_location="cpu", weights_only=True)["network"]
    out = {}
    for name, key in PARAM_KEYS.items():
        t = net[key]
        require(t.dtype == torch.float32, "A1", f"{path}: {key} is {t.dtype}")
        out[name] = t.detach().cpu().numpy().copy()
    return out


def bits_equal(a, b) -> bool:
    """Bitwise equality of two arrays (or tensors): same dtype, same shape,
    same bits. -0.0 and 0.0 differ; NaN equals only the same NaN."""
    if hasattr(a, "detach"):
        a = a.detach().cpu().numpy()
    if hasattr(b, "detach"):
        b = b.detach().cpu().numpy()
    a, b = np.ascontiguousarray(a), np.ascontiguousarray(b)
    if a.dtype != b.dtype or a.shape != b.shape:
        return False
    view = {2: np.uint16, 4: np.uint32, 8: np.uint64}[a.dtype.itemsize]
    return bool(np.array_equal(a.view(view), b.view(view)))


def reference_params(model: str, pretrained: dict, ensemble: dict) -> dict:
    """What a model's five parameters must equal, as float32: 000 from
    pretrained.rs, any other from its ensemble.csv rows. sign and syn_count
    are the same in every model and come from pretrained.rs (F15)."""
    if model == "000":
        ref = {"bias": pretrained["BIAS"], "time_const": pretrained["TIME_CONST"],
               "syn_strength": pretrained["SYN_STRENGTH"]}
    else:
        ref = {k: ensemble[model][k][1] for k in ("bias", "time_const", "syn_strength")}
    ref["sign"] = pretrained["SIGN"]
    ref["syn_count"] = pretrained["SYN_COUNT"]
    return ref


def check_checkpoints(ckpts: dict, ensemble: dict, pretrained: dict) -> None:
    """A1 over every checkpoint: the free parameters equal ensemble.csv and
    sign/syn_count equal pretrained.rs, bit for bit; the csv's names equal
    pretrained.rs's tables; model 000's csv rows equal pretrained.rs."""
    cells = list(pretrained["CELL_TYPES"])
    conns = [f"{s}>{t}" for s, t in pretrained["CONN_TYPES"]]
    for mid in sorted(ckpts):
        for kind in ("bias", "time_const", "syn_strength"):
            names, values = ensemble[mid][kind]
            require(names == (conns if kind == "syn_strength" else cells), "A1",
                    f"ensemble.csv {mid} {kind} names differ from pretrained.rs")
            require(bits_equal(ckpts[mid][kind], values), "A1", f"checkpoint {mid} {kind} != ensemble.csv")
        require(bits_equal(ckpts[mid]["sign"], pretrained["SIGN"]), "A1", f"checkpoint {mid} sign != pretrained.rs")
        require(bits_equal(ckpts[mid]["syn_count"], pretrained["SYN_COUNT"]), "A1",
                f"checkpoint {mid} syn_count != pretrained.rs")
    for kind, rs in (("bias", "BIAS"), ("time_const", "TIME_CONST"), ("syn_strength", "SYN_STRENGTH")):
        require(bits_equal(ensemble["000"][kind][1], pretrained[rs]), "A1", f"ensemble.csv 000 {kind} != pretrained.rs")


def read_clamps(results: pathlib.Path, ids) -> dict:
    """Each model's configured clamp per parameter, from its _meta.yaml."""
    import yaml

    out = {}
    for mid in ids:
        meta = yaml.safe_load((results / mid / "_meta.yaml").read_text())
        net = meta["config"]["network"]
        out[mid] = {
            "bias": net["node_config"]["bias"].get("clamp"),
            "time_const": net["node_config"]["time_const"].get("clamp"),
            "syn_strength": net["edge_config"]["syn_strength"].get("clamp"),
        }
    return out


def _spread(counts: dict) -> dict:
    v = [counts[k] for k in sorted(counts)]
    return {"min": min(v), "median": statistics.median(v), "max": max(v)}


def freeze_figures(ckpts: dict, clamps: dict) -> dict:
    """A8: records how many values sit at or under a boundary. Interprets nothing."""
    ids = sorted(ckpts)
    floor = np.float32(0.02)
    tc = {m: ckpts[m]["time_const"] for m in ids}
    ss = {m: ckpts[m]["syn_strength"] for m in ids}
    bias = np.concatenate([ckpts[m]["bias"] for m in ids])
    all_tc = np.concatenate([tc[m] for m in ids])
    all_ss = np.concatenate([ss[m] for m in ids])
    below = {m: int((tc[m] < floor).sum()) for m in ids}
    zeros = {m: int((ss[m] == 0).sum()) for m in ids}
    zero_matrix = np.stack([ss[m] == 0 for m in ids])
    positive = all_ss[all_ss > 0].astype(np.float64)
    return {
        "time_const": {
            "below_f32_0.02_total": int(sum(below.values())),
            "below_f32_0.02_per_model": _spread(below),
            "below_f32_0.02_000": below.get("000"),
            "below_f32_0.02_049": below.get("049"),
            "in_f32_0.02_to_0.02": int(((all_tc >= floor) & (all_tc.astype(np.float64) <= 0.02)).sum()),
            "largest_below_floor": float(all_tc[all_tc < floor].max()) if (all_tc < floor).any() else None,
            "smallest_at_or_above_floor": float(all_tc[all_tc >= floor].min()),
            "below_f32_1/200": int((all_tc < np.float32(1.0 / 200.0)).sum()),
        },
        "syn_strength": {
            "clamp": sorted({str(clamps[m]["syn_strength"]) for m in ids}),
            "exact_zeros": int((all_ss == 0).sum()),
            "negative": int((all_ss < 0).sum()),
            "zeros_per_model": _spread(zeros),
            "zeros_000": zeros.get("000"),
            "zeros_049": zeros.get("049"),
            "conn_types_zero_in_some_model": int(zero_matrix.any(axis=0).sum()),
            "conn_types_zero_in_all_models": int(zero_matrix.all(axis=0).sum()),
            "smallest_positive": float(positive.min()),
            "positive_at_or_below_1e-6": int((positive <= 1e-6).sum()),
            "positive_at_or_below_1e-4": int((positive <= 1e-4).sum()),
            "positive_at_or_below_1e-2": int((positive <= 1e-2).sum()),
        },
        "bias": {
            "min": float(bias.min()),
            "max": float(bias.max()),
            "clamp": sorted({str(clamps[m]["bias"]) for m in ids}),
        },
    }


# A8's asserted figures (F46-F48). A mismatch is a bug in how the checkpoints
# were read, not a number to update.
FREEZE_EXPECTED = {
    ("time_const", "below_f32_0.02_total"): 2396,
    ("time_const", "below_f32_0.02_per_model"): {"min": 36, "median": 48.5, "max": 55},
    ("time_const", "below_f32_0.02_000"): 51,
    ("time_const", "below_f32_0.02_049"): 36,
    ("time_const", "in_f32_0.02_to_0.02"): 0,
    ("time_const", "below_f32_1/200"): 0,
    ("syn_strength", "clamp"): ["non_negative"],
    ("syn_strength", "exact_zeros"): 919,
    ("syn_strength", "negative"): 0,
    ("syn_strength", "zeros_per_model"): {"min": 8, "median": 17.5, "max": 40},
    ("syn_strength", "zeros_000"): 31,
    ("syn_strength", "zeros_049"): 15,
    ("syn_strength", "conn_types_zero_in_some_model"): 418,
    ("syn_strength", "conn_types_zero_in_all_models"): 0,
    ("bias", "clamp"): ["None"],
}


def check_freeze(figures: dict, expected: dict = FREEZE_EXPECTED) -> None:
    for (group, key), want in expected.items():
        got = figures[group][key]
        require(got == want, "A8", f"{group}.{key} is {got!r}, expected {want!r}")


def copy_models(src: pathlib.Path, dst: pathlib.Path, ids) -> dict:
    """Copies MODEL_ENTRIES of each model into dst/<id>/ and checks every
    copied file's sha256 against its source. Returns each model's
    chkpt_00000 sha256, original and copy."""
    out = {}
    for mid in ids:
        for entry in MODEL_ENTRIES:
            s, d = src / mid / entry, dst / mid / entry
            d.parent.mkdir(parents=True, exist_ok=True)
            if s.is_dir():
                shutil.copytree(s, d)
            else:
                shutil.copy2(s, d)
        for s in sorted(p for p in (src / mid).rglob("*") if p.is_file()):
            rel = s.relative_to(src / mid)
            if rel.parts[0] not in MODEL_ENTRIES:
                continue
            require(sha256_file(dst / mid / rel) == sha256_file(s), "setup", f"copy of {s} differs")
        out[mid] = {"original_sha256": sha256_file(src / mid / CHECKPOINT),
                    "scratch_sha256": sha256_file(dst / mid / CHECKPOINT)}
    return out


def max_abs_gap(a, b) -> float:
    """max |a - b| in float64, after the exact widening of either side."""
    a = np.asarray(a, dtype=np.float64)
    b = np.asarray(b, dtype=np.float64)
    require(a.shape == b.shape, "A7", f"shapes {a.shape} vs {b.shape}")
    return float(np.max(np.abs(a - b)))
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: `36 passed, 0 failed`. `test_all_50_checkpoints_and_the_freeze_figures` is A1 and A8 on the real checkpoints: it must print `ok`, not `skip`. If it fails on an A8 figure, **stop and report**: a mismatch means the checkpoints were read differently from the research pass, and the expected figure is never edited to match.

- [ ] **Step 5: Commit**

```bash
git add tools/export_reference.py tools/test_export_reference.py
git commit -m "Rank the models and check all 50 checkpoints and the freeze figures"
```

---

### Task 5: The flyvis runs, the cross-checks and `main`

**Files:**
- Modify: `tools/export_reference.py` (append the last section)
- Test: `tools/test_export_reference.py` (insert a block above `# -- runner (keep last)`)

**Interfaces:**
- Consumes: everything above.
- Produces:
  - `to_np(t) -> np.ndarray` (detached CPU copy), `as_str(k) -> str`, `jsonable(obj)`, `read_nodes(path) -> (list[str], np.ndarray, np.ndarray)`
  - `check_network_params(network, refs, cells, conns, phase, run)`, `check_param_dtypes(network, want, run)`, `check_tensor(t, shape, want, check, run, sequence=None, what="tensor")`
  - `play(network, x, dt, settled, want, run, sequence) -> (trace tensor, buffer row 0 as np.ndarray)`
  - `run_one(run, refs, pre, nodes, offsets80_raw, record) -> dict` with `settled` `(45669,)`, `settling_centre` `(n_settle, 65)`, `seqs` (12 × `{x, buffer_row, centre, snapshots}`), `threads`, `centre_nodes`
  - `thread_check(network, xs, dt, settled, want, run) -> {"default", "determinism_check": [1, n], "bit_equal": True}`
  - `cross_checks(results) -> {"input_rounding": {dt id: float}, "f32_f64_gap": {"<model> <dt id>": {settled, settling_centre, centre, snapshots}}}`
  - `block_arrays(results) -> dict[(stem, run, dt id, sequence) -> np.ndarray]`, `git_state() -> {"head", "dirty"}`
  - `export(args, started) -> int`, `main(argv=None) -> int`

How this section settles the spec's open implementation details:

- **What each run keeps (run step 6).** Per sequence: a copy of `x`, a `.clone()` of photoreceptor row 0 of the buffer (after asserting all 8 rows are bit-equal; `zero()` reuses the buffer, so a view would alias the next sequence), the centre trace and the 6 snapshots. The full traces of sequences 0 and 4 are kept until step 7 has compared them, then dropped. A3 and A7 run in `cross_checks` once every run has finished, because `stored` exists only after the f64 runs.
- **Step 7.** `stimulus_response` is a generator yielding numpy `(stim, response)`; the code consumes it with `list(...)`, asserts one batch, and compares `response` with `to_np(trace)` through `bits_equal` (same dtype required, `uint32`/`uint64` views).
- **A6** runs on run 0 (f32), as the spec asks, and also on run 1 (f64), whose `scatter_add_` instantiation the spec's "the run is deterministic" also covers; PROVENANCE records `threads.runs_checked: [0, 1]`.
- **Assertion ids are printed**, one line each for A1–A9, so the log shows that every check ran.
- **Process hygiene.** `MPLCONFIGDIR` points at `target/flyvis-reference-mplconfig/` so matplotlib's font cache stays out of `~`; PROVENANCE records `flyvis_root` as the repo-relative `target/flyvis-reference-root`; a `flock` on `target/flyvis-reference.lock` is held for the whole run.
- **Print order** follows flow 1: setup lines, the model ranking, per run the settle count and per-sequence frame counts, then A6 and A7, then the freeze summary, then one line per written file, then `  all assertions passed`. Lines for A1, A9 and A2–A5 sit between, so every id appears.

- [ ] **Step 1: Write the failing tests**

Insert this block above the `# -- runner (keep last)` line, followed by two blank lines:

```python
# -- Task 5: helpers of the run code that need torch but not flyvis ------------------

def test_snapshot_frames_are_torchs_truncation_in_both_precisions():
    import torch

    for dtype in (torch.float32, torch.float64):
        for n, want in er.SNAPSHOT_FRAMES.items():
            got = torch.linspace(0, n - 1, er.N_SNAPSHOTS, dtype=dtype).long().tolist()
            assert got == want, (dtype, n, got)


def test_floor_dt_constants_are_f32_of_dt():
    import torch

    for dt_id, dt, _ in er.DTS:
        assert float(torch.tensor(dt).float()) == er.FLOOR_DT_F32[dt_id]
        assert float(np.float32(dt)) == er.FLOOR_DT_F32[dt_id]


def test_jsonable_and_as_str():
    obj = {"a": np.int64(3), "b": (np.float32(0.5), np.array([1, 2])), "c": np.bool_(True), 4: None}
    assert er.jsonable(obj) == {"a": 3, "b": [0.5, [1, 2]], "c": True, "4": None}
    assert type(er.jsonable(np.int64(3))) is int
    assert er.as_str(b"R1") == "R1" and er.as_str(np.bytes_(b"L1")) == "L1" and er.as_str("Am") == "Am"


def test_read_nodes_real_file():
    needs(er.NODES_CSV)
    types, u, v = er.read_nodes(er.NODES_CSV)
    assert len(types) == er.N_NODES
    centre = np.nonzero((u == 0) & (v == 0))[0]
    assert len(centre) == er.N_CENTRE and (int(centre[0]), types[centre[0]]) == er.FIRST_CENTRE
    assert [types[i] for i in centre] == list(er.parse_pretrained(er.PRETRAINED_RS.read_text())["CELL_TYPES"])


def test_flashes_fixture_has_the_shape_a5_reads():
    needs(er.FLASHES_BIN)
    assert er.FLASHES_BIN.stat().st_size == 4 * 600 * 721 * 4


def test_the_edge_timing_fixture_has_the_speed_13_block():
    needs(er.TIMING_JSON)
    blocks = [b for b in json.loads(er.TIMING_JSON.read_text())["speeds"] if b["speed"] == 13.0]
    assert len(blocks) == 1 and blocks[0]["n_stim"] == er.EDGE_N_STIM_1_200 == len(blocks[0]["indices"])
```

- [ ] **Step 2: Run the tests to see the new ones fail**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: `test_jsonable_and_as_str` and `test_read_nodes_real_file` print `FAIL` with `AttributeError` (`jsonable`, `read_nodes`); the four tests that check constants and fixtures print `ok`; `40 passed, 2 failed`.

- [ ] **Step 3: Append the runs and `main`**

Append to the end of `tools/export_reference.py`, after two blank lines:

```python
# -- the runs (flyvis) -----------------------------------------------------------

def to_np(t) -> np.ndarray:
    """A detached CPU copy, so later in-place work on the tensor cannot reach it."""
    return t.detach().cpu().numpy().copy()


def as_str(k) -> str:
    return k.decode() if isinstance(k, (bytes, np.bytes_)) else str(k)


def jsonable(obj):
    """numpy scalars and arrays, tuples and nested dicts, as plain JSON types."""
    if isinstance(obj, dict):
        return {str(k): jsonable(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [jsonable(v) for v in obj]
    if isinstance(obj, np.ndarray):
        return jsonable(obj.tolist())
    if isinstance(obj, np.bool_):
        return bool(obj)
    if isinstance(obj, np.integer):
        return int(obj)
    if isinstance(obj, np.floating):
        return float(obj)
    if obj is None or isinstance(obj, (bool, int, float, str)):
        return obj
    return repr(obj)


def read_nodes(path: pathlib.Path):
    """data/flyvis-build/nodes.csv as (cell types, u, v), row = node index."""
    lines = path.read_text().splitlines()
    require(lines[0] == "index,cell_type,u,v", "A5", f"nodes.csv header {lines[0]!r}")
    types, us, vs = [], [], []
    for i, line in enumerate(lines[1:]):
        index, cell_type, u, v = line.split(",")
        require(int(index) == i, "A5", f"nodes.csv row {i} has index {index}")
        types.append(cell_type)
        us.append(int(u))
        vs.append(int(v))
    return types, np.array(us, dtype=np.int64), np.array(vs, dtype=np.int64)


def check_network_params(network, refs: dict, cells: list, conns: list, phase: str, run: int) -> None:
    """A1, per run: the raw values flyvis holds equal the reference bit for
    bit -- as float32 when loaded, and as the exact widening float64(f32)
    after .double()."""
    for name in ("bias", "time_const"):
        _check_param(getattr(network.node_params, name).raw_values, refs[name], phase, run, name)
    for name in ("sign", "syn_count", "syn_strength"):
        _check_param(getattr(network.edge_params, name).raw_values, refs[name], phase, run, name)
    keys = [as_str(k) for k in network.node_params.bias.keys]
    require(keys == cells, "A1", f"node_params.bias.keys differ from CELL_TYPES ({phase})", run)
    keys = [tuple(as_str(p) for p in k) for k in network.edge_params.syn_strength.keys]
    require(keys == conns, "A1", f"edge_params.syn_strength.keys differ from CONN_TYPES ({phase})", run)


def _check_param(raw, ref: np.ndarray, phase: str, run: int, name: str) -> None:
    got = to_np(raw)
    want = ref if got.dtype == np.float32 else ref.astype(np.float64)
    require(bits_equal(got, want), "A1", f"{name} differs from its reference ({phase}, {got.dtype})", run)


def check_param_dtypes(network, want, run: int) -> None:
    """A2: every parameter and every floating-point tensor the dynamics read
    through _param_api() has dtype `want`, on the CPU; RefTensor indices stay
    int64. Walks the raw dicts: AutoDeref's own accessors would dereference."""
    import torch
    from flyvis.utils.tensor_utils import RefTensor

    for name, p in network.named_parameters():
        require(p.dtype == want and p.device.type == "cpu", "A2", f"parameter {name} is {p.dtype} on {p.device}", run)
    with torch.no_grad():
        api = network._param_api()
    seen = set()
    for route in ("nodes", "edges", "sources", "targets"):
        for name, v in dict.items(dict.__getitem__(api, route)):
            seen.add((route, name))
            if isinstance(v, RefTensor):
                require(v.values.dtype == want and v.values.device.type == "cpu", "A2",
                        f"_param_api {route}.{name} values are {v.values.dtype}", run)
                require(v.indices.dtype == torch.int64, "A2", f"_param_api {route}.{name} indices are {v.indices.dtype}", run)
            else:
                require(isinstance(v, torch.Tensor) and v.dtype == want and v.device.type == "cpu", "A2",
                        f"_param_api {route}.{name} is {getattr(v, 'dtype', type(v))}", run)
    require(("edges", "weight") in seen, "A2", "_param_api has no edges.weight", run)


def check_tensor(t, shape, want, check: str, run, sequence=None, what="tensor") -> None:
    import torch

    require(tuple(t.shape) == tuple(shape), check, f"{what} has shape {tuple(t.shape)}, expected {tuple(shape)}", run, sequence)
    require(t.dtype == want, check, f"{what} is {t.dtype}, expected {want}", run, sequence)
    require(t.device.type == "cpu", check, f"{what} is on {t.device}", run, sequence)
    require(bool(torch.isfinite(t).all()), "A4", f"{what} has NaN or Inf", run, sequence)


def play(network, x, dt: float, settled, want, run: int, sequence: int):
    """One sequence as stimulus_response runs it at batch 1, from `settled`.
    Returns the trace and a copy of photoreceptor row 0 of the buffer, after
    checking that all 8 photoreceptor rows hold the same values."""
    import torch

    stim = network.stimulus
    n = x.shape[0]
    stim.zero(1, n)
    require(stim.buffer.dtype == want, "A2", f"buffer is {stim.buffer.dtype} after zero()", run, sequence)
    stim.add_input(x[None, :, None, :])
    require(stim.buffer.dtype == want, "A2", f"buffer is {stim.buffer.dtype} after add_input()", run, sequence)
    require(stim.buffer.device.type == "cpu", "A2", f"buffer is on {stim.buffer.device}", run, sequence)
    require(stim.input_index.shape == (N_PHOTORECEPTOR_TYPES, N_HEXALS), "A3",
            f"input_index has shape {stim.input_index.shape}", run, sequence)
    # Copies, not views: zero() reuses this buffer for the next sequence (F30).
    rows = [stim.buffer[0, :, torch.as_tensor(stim.input_index[j])].detach().clone()
            for j in range(N_PHOTORECEPTOR_TYPES)]
    for j in range(1, N_PHOTORECEPTOR_TYPES):
        require(bits_equal(rows[j], rows[0]), "A3", f"photoreceptor row {j} differs from row 0", run, sequence)
    with torch.no_grad():
        trace = network(stim(), dt, state=settled)
    check_tensor(trace, (1, n, N_NODES), want, "A4", run, sequence, "trace")
    return trace, to_np(rows[0])


def run_one(run: int, refs: dict, pre: dict, nodes, offsets80_raw: np.ndarray, record: dict) -> dict:
    """One run: build fresh, check, settle, play the 12 sequences, check again."""
    import torch
    from flyvis.datasets.flashes import Flashes
    from flyvis.datasets.moving_bar import MovingEdge
    from flyvis.network import NetworkView

    _, mid, dt_id, prec = RUNS[run]
    dt = DT_VALUE[dt_id]
    want = torch.float32 if prec == "f32" else torch.float64
    cells = list(pre["CELL_TYPES"])
    conns = [tuple(c) for c in pre["CONN_TYPES"]]
    require(torch.get_default_dtype() == want, "A2", f"default dtype is {torch.get_default_dtype()}", run)

    # 1. Fresh view and network (C4); "best" must be the copied chkpt_00000.
    nv = NetworkView(f"flow/0000/{mid}")
    best = pathlib.Path(nv.get_checkpoint("best")).resolve()
    require(best == (SCRATCH / "results" / "flow" / "0000" / mid / CHECKPOINT).resolve(), "setup",
            f"best checkpoint is {best}", run)
    network = nv.init_network("best")
    check_network_params(network, refs[mid], cells, conns, "loaded", run)
    # 2. f64 runs only: widen, after the first check.
    if prec == "f64":
        network.double()
        check_network_params(network, refs[mid], cells, conns, "widened", run)
    check_param_dtypes(network, want, run)

    # A5: node table and centre column.
    c = network.connectome
    types = [as_str(t) for t in c.nodes.type[:]]
    require(types == nodes[0], "A5", "node cell types differ from nodes.csv", run)
    require(bits_equal(np.asarray(c.nodes.u[:], dtype=np.int64), nodes[1]), "A5", "node u differs from nodes.csv", run)
    require(bits_equal(np.asarray(c.nodes.v[:], dtype=np.int64), nodes[2]), "A5", "node v differs from nodes.csv", run)
    centre = np.asarray(c.central_cells_index[:], dtype=np.int64)
    expected_centre = np.nonzero((nodes[1] == 0) & (nodes[2] == 0))[0].astype(np.int64)
    require(bits_equal(centre, expected_centre), "A5", "central_cells_index differs from the (0,0) rows of nodes.csv", run)
    require(len(centre) == N_CENTRE and bool(np.all(np.diff(centre) > 0)), "A5", f"{len(centre)} centre nodes", run)
    centre_types = [nodes[0][i] for i in centre]
    require(centre_types == cells, "A5", "centre nodes are not one per cell type in CELL_TYPES order", run)
    require((int(centre[0]), centre_types[0]) == FIRST_CENTRE, "A5", f"first centre node {centre[0]}", run)
    centre_t = torch.as_tensor(centre)

    # 3. Settle. 4. The settled state equals return_last=True.
    states = network.steady_state(t_pre=T_PRE, dt=dt, batch_size=BATCH_SIZE, value=PRE_STIM_VALUE, return_last=False)
    require(len(states) == N_SETTLE[dt_id], "A4", f"{len(states)} settling states", run)
    for i, s in enumerate(states):
        check_tensor(s.nodes.activity, (1, N_NODES), want, "A2", run, None, f"settling state {i}")
    settling_centre = to_np(torch.stack([s.nodes.activity[0, centre_t] for s in states]))
    settled = states[-1]
    settled_before = to_np(settled.nodes.activity)
    again = network.steady_state(T_PRE, dt, BATCH_SIZE, PRE_STIM_VALUE, return_last=True)
    require(bits_equal(again.nodes.activity, settled.nodes.activity), "P17",
            "steady_state(return_last=True) differs from the last settling state", run)
    del states, again

    # 5. Datasets.
    flashes = Flashes(dt=dt)
    edges = MovingEdge(dt=dt, angles=GATE_ANGLES, speeds=[EDGE_SPEED], height=EDGE_HEIGHT)
    require(len(flashes) == len(FLASH_ARGS) and len(edges) == len(EDGE_ARGS), "A4",
            f"{len(flashes)} flashes, {len(edges)} edges", run)
    rows = [(float(r.baseline), float(r.intensity), int(r.radius)) for r in flashes.arg_df.itertuples(index=False)]
    require(rows == FLASH_ARGS, "A4", f"flash arg rows {rows}", run)
    rows = [(int(r.angle), float(r.intensity)) for r in edges.arg_df.itertuples(index=False)]
    require(rows == EDGE_ARGS, "A4", f"edge arg rows {rows}", run)
    require(all(float(r.speed) == EDGE_SPEED and int(r.width) == 80 for r in edges.arg_df.itertuples(index=False)),
            "A4", "edge speed or width", run)
    require(bits_equal(np.asarray(edges.wrap.offsets[:]), offsets80_raw), "A9",
            "the trace run's height-80 offsets differ from the geometry export", run)
    # A5: spans and snapshot frames, from flyvis's own formulas.
    f0 = int(flashes.t_pre / dt)
    spans = {"flashes": [f0, f0 + int(flashes.t_stim / dt)]}
    n_stim = int(edges.indices[EDGE_SPEED].shape[0])
    e1 = int((float(edges.t_stim[0]) + edges.t_pre) / dt)
    spans["moving_edge"] = [e1 - n_stim, e1]
    for family, span in spans.items():
        require(span == STIMULUS_FRAMES[family][dt_id], "A5", f"{family} stimulus frames {span}", run)
    if dt_id == "1/200":
        timing = json.loads(TIMING_JSON.read_text())
        block = [b for b in timing["speeds"] if b["speed"] == float(EDGE_SPEED)]
        require(len(block) == 1 and block[0]["n_stim"] == EDGE_N_STIM_1_200 == n_stim, "A5", "speed-13 n_stim", run)
        got = [int(i) for i in np.asarray(edges.indices[EDGE_SPEED]).tolist()]
        require(got == block[0]["indices"], "A5", "speed-13 resample indices differ from moving_edge_timing.json", run)
    for n in {N_FRAMES["flashes"][dt_id], N_FRAMES["moving_edge"][dt_id]}:
        got = torch.linspace(0, n - 1, N_SNAPSHOTS).long().tolist()
        require(got == SNAPSHOT_FRAMES[n], "A5", f"snapshot frames for n={n}: {got}", run)
    if prec == "f32" and mid == "000":
        record["stimuli"][dt_id] = {
            "flashes": {"config": jsonable(flashes.config.to_dict()),
                        "arg_rows": jsonable(flashes.arg_df.to_dict(orient="records"))},
            "moving_edge": {"config": jsonable(edges.config.to_dict()),
                            "arg_rows": jsonable(edges.arg_df.to_dict(orient="records"))},
        }

    # 6. The 12 sequences.
    seqs = sequence_table()
    out_seqs, kept = [], {}
    for s in seqs:
        k = s["index"]
        dataset = flashes if s["family"] == "flashes" else edges
        n = s["frames"][dt_id]["n_frames"]
        x = dataset[s["flyvis_index"]]
        x_want = want if s["family"] == "flashes" else torch.float64
        check_tensor(x, (n, N_HEXALS), x_want, "A2", run, k, "x")
        trace, row = play(network, x, dt, settled, want, run, k)
        snap = torch.as_tensor(s["frames"][dt_id]["snapshot_frames"])
        out_seqs.append({
            "x": to_np(x),
            "buffer_row": row,
            "centre": to_np(trace[0, :, centre_t]),
            "snapshots": to_np(trace[0, snap, :]),
        })
        if k in (0, len(FLASH_ARGS)):
            kept[k] = trace          # step 7 compares against it
        del trace
        print(f"    sequence {k:>2} ({s['family']}, flyvis index {s['flyvis_index']}): {n} frames")

    # A6: threads, on run 0 (f32) as the spec asks, and on run 1 (f64).
    threads = None
    if run in (0, 1):
        threads = thread_check(network, [out_seqs[0]["x"], out_seqs[len(FLASH_ARGS)]["x"]], dt, settled, want, run)

    # 7. flyvis's own path, bit for bit. After step 6: it sets dataset.dt.
    for k, dataset in ((0, flashes), (len(FLASH_ARGS), edges)):
        pairs = list(network.stimulus_response(dataset, dt, indices=[0], t_pre=T_PRE,
                                               t_fade_in=T_FADE_IN, batch_size=BATCH_SIZE))
        require(len(pairs) == 1, "equivalence", f"stimulus_response yielded {len(pairs)} batches", run, k)
        _, response = pairs[0]
        require(bits_equal(response, to_np(kept[k])), "equivalence",
                "stimulus_response differs from forward from the settled state", run, k)
    del kept

    # 8. The settled state is untouched; the parameters too (clamp runs in place).
    require(bits_equal(to_np(settled.nodes.activity), settled_before), "P17", "settled state changed", run)
    check_network_params(network, refs[mid], cells, conns, "after_runs", run)
    check_param_dtypes(network, want, run)
    return {
        "settled": settled_before[0],
        "settling_centre": settling_centre,
        "seqs": out_seqs,
        "threads": threads,
        "centre_nodes": [(int(i), nodes[0][i]) for i in centre],
    }


def thread_check(network, xs, dt: float, settled, want, run: int) -> dict:
    """A6: sequences 0 and 4 at 1 thread and at max(2, default), bit-equal."""
    import torch

    default = torch.get_num_threads()
    many = max(2, default)
    traces = {}
    try:
        for n in (1, many):
            torch.set_num_threads(n)
            traces[n] = [to_np(play(network, torch.from_numpy(x), dt, settled, want, run, k)[0])
                         for k, x in zip((0, len(FLASH_ARGS)), xs)]
    finally:
        torch.set_num_threads(default)
    for k, a, b in zip((0, len(FLASH_ARGS)), traces[1], traces[many]):
        require(bits_equal(a, b), "A6", f"1 thread and {many} threads differ", run, k)
    return {"default": default, "determinism_check": [1, many], "bit_equal": True}


def cross_checks(results: dict) -> dict:
    """A3 (inputs at both levels), A5 (flashes.bin) and A7 (the gap), after
    every run has finished. Returns input_rounding and the gaps."""
    by = {(m, d, p): r for r, m, d, p in RUNS}
    rounding, gaps = {}, {}
    for dt_id, _, _ in DTS:
        stored = [s["x"] for s in results[by[("000", dt_id, "f64")]]["seqs"]]
        for k, x in enumerate(stored):
            require(x.dtype == np.float64, "A3", f"stored input is {x.dtype}", by[("000", dt_id, "f64")], k)
            for mid in ("000", "049"):
                r32, r64 = by[(mid, dt_id, "f32")], by[(mid, dt_id, "f64")]
                x32 = results[r32]["seqs"][k]["x"]
                if k < len(FLASH_ARGS):
                    require(x32.dtype == np.float32 and bits_equal(x32.astype(np.float64), x), "A3",
                            "f32 flash input differs from the stored input", r32, k)
                else:
                    require(bits_equal(x32, x), "A3", "f32 edge input differs from the stored input", r32, k)
                require(bits_equal(results[r64]["seqs"][k]["x"], x), "A3", "f64 input differs from the stored input", r64, k)
                require(bits_equal(results[r32]["seqs"][k]["buffer_row"], x.astype(np.float32)), "A3",
                        "f32 buffer differs from float32(stored)", r32, k)
                require(bits_equal(results[r64]["seqs"][k]["buffer_row"], x), "A3",
                        "f64 buffer differs from stored", r64, k)
        edge_inputs = stored[len(FLASH_ARGS):]
        rounding[dt_id] = max(max_abs_gap(x, x.astype(np.float32)) for x in edge_inputs)
        for mid in ("000", "049"):
            a, b = results[by[(mid, dt_id, "f32")]], results[by[(mid, dt_id, "f64")]]
            gap = {
                "settled": max_abs_gap(a["settled"], b["settled"]),
                "settling_centre": max_abs_gap(a["settling_centre"], b["settling_centre"]),
                "centre": max(max_abs_gap(p["centre"], q["centre"]) for p, q in zip(a["seqs"], b["seqs"])),
                "snapshots": max(max_abs_gap(p["snapshots"], q["snapshots"]) for p, q in zip(a["seqs"], b["seqs"])),
            }
            for where, g in gap.items():
                require(np.isfinite(g) and g <= GAP_BOUND, "A7",
                        f"{mid} {dt_id} {where} gap {g!r} exceeds {GAP_BOUND}. Stop and report it to the owner; "
                        "do not raise the bound.")
            gaps[f"{mid} {dt_id}"] = gap
    flash_stored = np.stack([s["x"] for s in results[by[("000", "1/200", "f64")]]["seqs"][:len(FLASH_ARGS)]])
    want = np.fromfile(FLASHES_BIN, dtype="<f4").reshape(len(FLASH_ARGS), N_FRAMES["flashes"]["1/200"], N_HEXALS)
    require(bits_equal(flash_stored.astype(np.float32), want.astype(np.float32)), "A5",
            "flash inputs at 1/200 differ from flashes.bin")
    return {"input_rounding": rounding, "f32_f64_gap": gaps}


def block_arrays(results: dict) -> dict:
    """Every block's array, keyed by (run, dt id, sequence), for pack_file."""
    arrays = {}
    for r, mid, dt_id, prec in RUNS:
        res = results[r]
        arrays[("settled", r, dt_id, None)] = res["settled"]
        arrays[("settling_centre", r, dt_id, None)] = res["settling_centre"]
        for k, s in enumerate(res["seqs"]):
            arrays[("snapshots", r, dt_id, k)] = s["snapshots"]
            arrays[("centre", r, dt_id, k)] = s["centre"]
            if mid == "000" and prec == "f64":
                arrays[("input", None, dt_id, k)] = s["x"]
    return arrays


def git_state() -> dict:
    import subprocess

    head = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"],
                          capture_output=True, text=True, check=True).stdout.strip()
    status = subprocess.run(["git", "-C", str(ROOT), "status", "--porcelain"],
                            capture_output=True, text=True, check=True).stdout
    return {"head": head, "dirty": bool(status.strip())}


def export(args, started: float) -> int:
    # Process setup 2: a fresh scratch root, flyvis pointed at it, CPU only.
    for d in (SCRATCH, MPLCONFIG):
        shutil.rmtree(d, ignore_errors=True)
        d.mkdir(parents=True)
    os.environ["FLYVIS_ROOT_DIR"] = str(SCRATCH)
    os.environ["CUDA_VISIBLE_DEVICES"] = ""
    os.environ["MPLCONFIGDIR"] = str(MPLCONFIG)
    print(f"  setup: scratch root {SCRATCH_REL}/, CUDA hidden, MPLCONFIGDIR in target/")
    # 3. Copy the two models, check every byte, and load each checkpoint safely
    #    before flyvis ever unpickles it with weights_only=False.
    copies = copy_models(RESULTS, SCRATCH / "results" / "flow" / "0000", [m for m, *_ in MODELS])
    print(f"  setup: models {', '.join(copies)} copied, sha256 verified")
    import torch

    for mid in copies:
        torch.load(SCRATCH / "results" / "flow" / "0000" / mid / CHECKPOINT, map_location="cpu", weights_only=True)
    print("  setup: scratch chkpt_00000 loads with weights_only=True")
    # 4. Import flyvis. 5. No use_deterministic_algorithms: determinism is measured.
    import flyvis
    from flyvis.datasets.moving_bar import MovingEdge

    require(flyvis.device.type == "cpu", "setup", f"flyvis device is {flyvis.device}")
    require(pathlib.Path(flyvis.root_dir).resolve() == SCRATCH.resolve(), "setup", f"flyvis root is {flyvis.root_dir}")
    print(f"  setup: flyvis {flyvis.__version__} on cpu, root {SCRATCH_REL}/")

    # Model selection (C16).
    losses = read_validation_losses(RESULTS)
    rank_models(losses)
    print(f"  models: best 000 ({losses['000']!r}), worst 049 ({losses['049']!r}) of {len(losses)}")

    # A1 over all 50 checkpoints, and A8.
    pre = parse_pretrained(PRETRAINED_RS.read_text())
    ensemble = read_ensemble(ENSEMBLE_CSV.read_text())
    ckpts = {mid: load_checkpoint(RESULTS / mid / CHECKPOINT) for mid in sorted(losses)}
    check_checkpoints(ckpts, ensemble, pre)
    print(f"  A1 checkpoints: all {len(ckpts)} equal ensemble.csv; sign and syn_count equal pretrained.rs")
    freeze = freeze_figures(ckpts, read_clamps(RESULTS, sorted(losses)))
    check_freeze(freeze)                 # printed after A7, in the spec's order
    refs = {mid: reference_params(mid, pre, ensemble) for mid, *_ in MODELS}
    nodes = read_nodes(NODES_CSV)

    # A9: the height-80 geometry, built as the height-9 exporter builds it.
    require(torch.get_default_dtype() == torch.float32, "A2", "default dtype before the f32 runs")
    edge80 = MovingEdge(dt=1.0 / 200.0, angles=GATE_ANGLES, height=EDGE_HEIGHT)
    offsets80_raw = np.asarray(edge80.wrap.offsets[:])
    offsets80 = np.asarray(offsets80_raw, dtype=np.float64)
    require(offsets80.shape == (len(GATE_ANGLES) * 2, 21, N_OMMATIDIA), "A9", f"offsets shape {offsets80.shape}")
    require(bool(np.isfinite(offsets80).all()), "A4", "height-80 offsets have NaN or Inf")
    bg0, bg30 = float(offsets80[1].min()), float(offsets80[3].min())
    require(abs(bg0 - 0.5) <= BACKGROUND_TOL, "A9", f"background at angle 0 is {bg0}")
    require(abs(bg30 - 127 / 255) <= BACKGROUND_TOL, "A9", f"background at angle 30 is {bg30}")
    print(f"  A9 height-80 geometry: {offsets80.shape}, background {bg0:.6f} at angle 0, {bg30:.6f} at angle 30")
    del edge80

    # The runs: all f32 first, then the switch, then all f64 (P13).
    record = {"stimuli": {}}
    results = {}
    for r in EXECUTION_ORDER:
        if r == 1:
            torch.set_default_dtype(torch.float64)
        _, mid, dt_id, prec = RUNS[r]
        print(f"  run {r} ({mid}, dt {dt_id}, {prec}): settling {N_SETTLE[dt_id]} steps")
        results[r] = run_one(r, refs, pre, nodes, offsets80_raw, record)
    torch.set_default_dtype(torch.float32)
    t0, t1 = results[0]["threads"], results[1]["threads"]
    require(t0 == t1, "A6", f"thread counts differ between runs 0 and 1: {t0} vs {t1}")
    print(f"  A6 threads: 1 vs {t0['determinism_check'][1]} bit-equal on sequences 0 and 4, runs 0 (f32) and 1 (f64)")
    centre_nodes = results[0]["centre_nodes"]
    require(all(results[r]["centre_nodes"] == centre_nodes for r in results), "A5", "centre nodes differ between runs")
    print("  A1 per run: loaded, widened (f64) and after the runs, bit-equal to the references")
    print("  A2 precision: default dtype, parameters, _param_api, buffers, states and traces as claimed")
    checks = cross_checks(results)
    print("  A3 inputs: dataset and buffer level bit-equal; input rounding "
          + ", ".join(f"{d} {v:.3g}" for d, v in checks["input_rounding"].items()))
    print("  A4 shapes, frame counts, arg rows and finiteness: ok")
    print(f"  A5 fixtures: nodes.csv, centre ({N_CENTRE}, R1 at 360), flashes.bin, moving_edge_timing.json, spans, snapshots")
    for key, gap in checks["f32_f64_gap"].items():
        print(f"  A7 gap {key}: " + ", ".join(f"{k} {v:.3e}" for k, v in gap.items()))
    tcf, ssf = freeze["time_const"], freeze["syn_strength"]
    print(f"  A8 freeze: time_const below f32(0.02): {tcf['below_f32_0.02_total']} "
          f"(per model {tcf['below_f32_0.02_per_model']}, 000 {tcf['below_f32_0.02_000']}, "
          f"049 {tcf['below_f32_0.02_049']}); syn_strength zeros {ssf['exact_zeros']} "
          f"(per model {ssf['zeros_per_model']}, types {ssf['conn_types_zero_in_some_model']} in some, "
          f"{ssf['conn_types_zero_in_all_models']} in all)")

    # Every array is computed and every check has passed: build the files.
    files = file_layout()
    arrays = block_arrays(results)
    ref_bins = []
    for f in files:
        stem = f["name"].split(".")[0]
        data = pack_file(f, {k[1:]: v for k, v in arrays.items() if k[0] == stem})
        f["sha256"] = hashlib.sha256(data).hexdigest()
        ref_bins.append((f["name"], data))
    del arrays, results
    manifest = build_manifest(centre_nodes, files)
    validate_manifest(manifest)

    now = datetime.datetime.now(datetime.timezone.utc)
    export_id = f"{now.strftime('%Y%m%dT%H%M%SZ')}-{os.getpid()}"
    import importlib.metadata
    import platform

    import h5py

    script = pathlib.Path(__file__).resolve()
    provenance = {
        "exported": now.date().isoformat(),
        "export_id": export_id,
        "generator": {"path": GENERATOR, "sha256": sha256_file(script)},
        "git": git_state(),
        "versions": {"python": platform.python_version(), "flyvis": flyvis.__version__,
                     "torch": torch.__version__, "numpy": np.__version__, "h5py": h5py.__version__,
                     "datamate": importlib.metadata.version("datamate")},
        "machine": {"platform": platform.platform(), "machine": platform.machine(),
                    "torch_config": torch.__config__.show()},
        "uv_lock_sha256": sha256_file(UV_LOCK),
        "device": "cpu",
        "threads": {**t0, "runs_checked": [0, 1]},
        "flyvis_root": {"path": SCRATCH_REL, "models_copied": True,
                        "entries_copied": MODEL_ENTRIES},
        "models": [{"id": m, "name": name, "validation_loss": loss, "rank": rank,
                    "chkpt_00000_sha256": copies[m]["original_sha256"],
                    "scratch_chkpt_00000_sha256": copies[m]["scratch_sha256"]}
                   for m, name, loss, rank in MODELS],
        "validation_losses": losses,
        "inputs_read": {str(p.relative_to(ROOT)): sha256_file(p)
                        for p in (ENSEMBLE_CSV, PRETRAINED_RS, NODES_CSV, FLASHES_BIN, TIMING_JSON)},
        "dts": [{"id": i, "dt": dt, "n_settle": n, "floor_dt_f32": FLOOR_DT_F32[i]} for i, dt, n in DTS],
        "settling": {"t_pre": T_PRE, "value": PRE_STIM_VALUE, "t_fade_in": T_FADE_IN, "batch_size": BATCH_SIZE},
        "stimuli": record["stimuli"],
        "f32_floor_trap": {
            i: {"dt": dt, "f32_dt": FLOOR_DT_F32[i], "source": "flyvis/network/dynamics.py:207-218",
                "euler_step_dt": {"f32": FLOOR_DT_F32[i], "f64": dt, "source": "network.py:406"},
                "note": "task 3 must reproduce max(tau, f32(dt))"}
            for i, dt, _ in DTS
        },
        "parameter_equality": {
            "all_50_checkpoints_equal_ensemble_csv": True,
            "checkpoint_file": str(CHECKPOINT),
            "models": {
                mid: {name: {"count": int(refs[mid][name].shape[0]),
                             "reference": ("pretrained.rs" if mid == "000" or name in ("sign", "syn_count")
                                           else "ensemble.csv"),
                             "loaded": True, "widened": True, "after_runs": True}
                      for name in PARAM_KEYS}
                for mid, *_ in MODELS
            },
        },
        "equivalence": {"stimulus_response_bit_equal": True, "sequences": [0, len(FLASH_ARGS)]},
        "input_rounding": checks["input_rounding"],
        "f32_f64_gap": checks["f32_f64_gap"],
        "freeze_check": freeze,
        "sizes": {name: len(data) for name, data in ref_bins},
        "wall_time_s": round(time.monotonic() - started, 1),
    }
    h80_provenance = {
        "exported": now.date().isoformat(),
        "export_id": export_id,
        "generator": GENERATOR,
        "flyvis": flyvis.__version__,
        "torch": torch.__version__,
        "numpy": np.__version__,
        "dt": 1.0 / 200.0,
        "gate_angles": GATE_ANGLES,
        "height": EDGE_HEIGHT,
        "n_ommatidia": N_OMMATIDIA,
        "ppo": PPO,
        "monitor_px": MONITOR,
        "offsets_shape": list(offsets80.shape),
    }
    h80 = [
        ("moving_edge_offsets_h80.bin", offsets80.astype("<f4").tobytes(order="C")),
        ("moving_edge_offsets_h80.json", json_bytes(h80_sidecar())),
        ("moving_edges_h80_provenance.json", json_bytes(h80_provenance)),
    ]
    write_outputs(args.out, ref_bins, json_bytes(jsonable(provenance)), manifest, h80, export_id)
    for name, data in ref_bins:
        print(f"  flyvis-reference/{name}: {len(data)} bytes")
    print(f"  flyvis-reference/PROVENANCE.json, manifest.json ({len(manifest_bytes(manifest))} bytes)")
    for name, data in h80:
        print(f"  flyvis-stimuli/{name}: {len(data)} bytes")
    print("  all assertions passed")
    return 0


def main(argv=None) -> int:
    started = time.monotonic()
    try:
        args = parse_args(sys.argv[1:] if argv is None else argv)
        preflight(args.out, args.force, os.environ)
        lock = acquire_lock(LOCK)
        try:
            return export(args, started)
        finally:
            lock.close()
    except ExportError as e:
        print(f"export_reference: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `uv run --project tools tools/test_export_reference.py`
Expected: `42 passed, 0 failed`, no `skip`.

- [ ] **Step 5: Check the command line, without importing flyvis**

Every one of these exits in preflight, before the lock and before `import flyvis`:

```bash
uv run --project tools python -I -m py_compile tools/export_reference.py && echo compiled
uv run --project tools tools/export_reference.py --help | head -1
FLYVIS_ROOT_DIR=x uv run --project tools tools/export_reference.py --out target/preflight-check; echo rc=$?
mkdir -p target/preflight-check/flyvis-reference
uv run --project tools tools/export_reference.py --out target/preflight-check; echo rc=$?
mkdir target/preflight-check/.flyvis-reference.old-1
uv run --project tools tools/export_reference.py --out target/preflight-check --force; echo rc=$?
rm -rf target/preflight-check tools/__pycache__
ls target/flyvis-reference-root 2>/dev/null | head -1; echo "scratch-root-untouched"
```

Expected, in order: `compiled`; `usage: export_reference.py [-h] [--out OUT] [--force]`; `export_reference: FLYVIS_ROOT_DIR is set in the environment; unset it. This script runs flyvis under its own scratch root, target/flyvis-reference-root/` then `rc=1`; `export_reference: <repo>/target/preflight-check/flyvis-reference exists; rerun with --force to replace it` then `rc=1`; `export_reference: <repo>/target/preflight-check/.flyvis-reference.old-1 is left over from an interrupted export.` followed by the `Recovering from an interrupted export:` text (its two numbered steps), then `rc=1`; and finally `scratch-root-untouched` with nothing listed before it (no run reached process setup).

- [ ] **Step 6: Commit**

```bash
git add tools/export_reference.py tools/test_export_reference.py
git commit -m "Run flyvis's eight reference runs and check them before writing anything"
```

---

### Task 6: Run the export (owner go-ahead)

**Files:** none in git. Produces `data/flyvis-reference/` (11 files) and three `_h80` files in `data/flyvis-stimuli/`, all gitignored.

**Interfaces:**
- Consumes: `tools/export_reference.py` and `tools/test_export_reference.py` from Tasks 1–5, committed.
- Produces: the files Tasks 7 and 9 read, and `target/reference-export.log`, the run's output, quoted in the PR (Task 10).

- [ ] **Step 1: Check the preconditions**

```bash
git status --short; git rev-parse --abbrev-ref HEAD
test -d data/flyvis-pretrained/results/flow/0000/000 && echo results-present
test -e data/flyvis-reference && echo REFERENCE-EXISTS
env | grep -c '^FLYVIS_ROOT_DIR=' 
/bin/bash ci/fetch-fixtures.sh
```

Expected: no `git status` output (so PROVENANCE records `"dirty": false`), `reference-traces`, `results-present`, no `REFERENCE-EXISTS`, `0`, and `fetch-fixtures: all 20 fixtures present and verified in <repo>/data`.

- [ ] **Step 2: Take the before-listing of `data/`**

```bash
mkdir -p target
/bin/bash -c '( cd data && find . \( -path ./flyvis-reference \
      -o -path ./flyvis-stimuli/moving_edge_offsets_h80.bin \
      -o -path ./flyvis-stimuli/moving_edge_offsets_h80.json \
      -o -path ./flyvis-stimuli/moving_edges_h80_provenance.json \) -prune \
    -o \( -type f -exec shasum -a 256 {} + \) \
    -o -print | LC_ALL=C sort )' > target/data-before.txt
wc -l < target/data-before.txt
```

Expected: a line count (it is recorded, not checked).

- [ ] **Step 3: STOP and get the owner's go-ahead**

Tell the owner: the export runs flyvis 1.2.0 on the CPU for an estimated 5–15 minutes, with a peak of a few GB of memory (the float64 runs hold full traces of two sequences, twice over during A6); it wipes and recreates `target/flyvis-reference-root/` and `target/flyvis-reference-mplconfig/`; it writes `data/flyvis-reference/` (11 files, 220,855,520 B of binaries) and three `_h80` files in `data/flyvis-stimuli/`; and Step 9 runs it a second time into `target/reref/`. Do not run it without a yes.

- [ ] **Step 4: Run the export (flow 1)**

```bash
uv run --project tools tools/export_reference.py > target/reference-export.log 2>&1; echo rc=$?
grep -v -e 'Batch' -e '^\s*$' target/reference-export.log | grep '^  '
```

Expected: `rc=0`, and the indented lines, in this order (flyvis's own log lines, its `epe`→`loss` fallback warning and tqdm bars are interleaved on stderr and filtered out here):

```text
  setup: scratch root target/flyvis-reference-root/, CUDA hidden, MPLCONFIGDIR in target/
  setup: models 000, 049 copied, sha256 verified
  setup: scratch chkpt_00000 loads with weights_only=True
  setup: flyvis 1.2.0 on cpu, root target/flyvis-reference-root/
  models: best 000 (5.136557016107771), worst 049 (5.677851685219341) of 50
  A1 checkpoints: all 50 equal ensemble.csv; sign and syn_count equal pretrained.rs
  A9 height-80 geometry: (8, 21, 721), background 0.500000 at angle 0, 0.498039 at angle 30
  run 0 (000, dt 1/200, f32): settling 200 steps
    sequence  0 (flashes, flyvis index 0): 600 frames
    sequence  1 (flashes, flyvis index 1): 600 frames
    sequence  2 (flashes, flyvis index 2): 600 frames
    sequence  3 (flashes, flyvis index 3): 600 frames
    sequence  4 (moving_edge, flyvis index 0): 525 frames
    ... through sequence 11 (moving_edge, flyvis index 7): 525 frames
  run 2 (000, dt 1/50, f32): settling 50 steps
    (12 sequence lines: 150 frames for 0-3, 131 for 4-11)
  run 4 (049, dt 1/200, f32): settling 200 steps
  run 6 (049, dt 1/50, f32): settling 50 steps
  run 1 (000, dt 1/200, f64): settling 200 steps
  run 3 (000, dt 1/50, f64): settling 50 steps
  run 5 (049, dt 1/200, f64): settling 200 steps
  run 7 (049, dt 1/50, f64): settling 50 steps
  A6 threads: 1 vs N bit-equal on sequences 0 and 4, runs 0 (f32) and 1 (f64)
  A1 per run: loaded, widened (f64) and after the runs, bit-equal to the references
  A2 precision: default dtype, parameters, _param_api, buffers, states and traces as claimed
  A3 inputs: dataset and buffer level bit-equal; input rounding 1/200 R1, 1/50 R2
  A4 shapes, frame counts, arg rows and finiteness: ok
  A5 fixtures: nodes.csv, centre (65, R1 at 360), flashes.bin, moving_edge_timing.json, spans, snapshots
  A7 gap 000 1/200: settled G, settling_centre G, centre G, snapshots G
  A7 gap 049 1/200: ...
  A7 gap 000 1/50: ...
  A7 gap 049 1/50: ...
  A8 freeze: time_const below f32(0.02): 2396 (per model {'min': 36, 'median': 48.5, 'max': 55}, 000 51, 049 36); syn_strength zeros 919 (per model {'min': 8, 'median': 17.5, 'max': 40}, types 418 in some, 0 in all)
  flyvis-reference/settled.f32.bin: 730704 bytes
  flyvis-reference/settled.f64.bin: 1461408 bytes
  flyvis-reference/settling_centre.f32.bin: 130000 bytes
  flyvis-reference/settling_centre.f64.bin: 260000 bytes
  flyvis-reference/snapshots.f32.bin: 52610688 bytes
  flyvis-reference/snapshots.f64.bin: 105221376 bytes
  flyvis-reference/centre.f32.bin: 4288960 bytes
  flyvis-reference/centre.f64.bin: 8577920 bytes
  flyvis-reference/input.f64.bin: 47574464 bytes
  flyvis-reference/PROVENANCE.json, manifest.json (M bytes)
  flyvis-stimuli/moving_edge_offsets_h80.bin: 484512 bytes
  flyvis-stimuli/moving_edge_offsets_h80.json: J bytes
  flyvis-stimuli/moving_edges_h80_provenance.json: P bytes
  all assertions passed
```

`N` (the thread count, at least 2), `R1`, `R2`, every `G` (each at most `0.01`), and `M`, `J`, `P` are measured, not predicted. If the run exits 1, read the `export_reference: <check> failed (run r, sequence s): ...` line. **If it names A7 or A8, stop and report it to the owner; do not change a bound or an expected figure.** If it names A2's edge-`x` dtype check in an f32 run, record it and report: the spec says to correct the spec, never to add a cast. **If it names A1, A6 (on either run, 0 or 1), A9, P17, `equivalence` (the `stimulus_response` comparison), or any other bit-equality check against flyvis's own output or state (A3, A5), stop and report it to the owner** with the full line; do not pin the thread count, drop a run, add a tolerance or change what is compared (see Global Constraints). Only a Python exception from the script's own code, or a failed check of the script's own bookkeeping (A4's shapes, frame counts and arg rows; the manifest and write-step verification), is a bug to find in the script; when unsure which kind a failure is, report it. Such a bug is fixed with its own test in the task that owns the code, then rerun from Step 4 (a rerun needs no new go-ahead; a failed run leaves `data/` untouched, which Step 5 confirms).

- [ ] **Step 5: Check that nothing else under `data/` changed**

```bash
/bin/bash -c '( cd data && find . \( -path ./flyvis-reference \
      -o -path ./flyvis-stimuli/moving_edge_offsets_h80.bin \
      -o -path ./flyvis-stimuli/moving_edge_offsets_h80.json \
      -o -path ./flyvis-stimuli/moving_edges_h80_provenance.json \) -prune \
    -o \( -type f -exec shasum -a 256 {} + \) \
    -o -print | LC_ALL=C sort )' > target/data-after.txt
diff target/data-before.txt target/data-after.txt && echo data-unchanged
grep -c -e '\./flyvis-pretrained/connectome' -e '\./flyvis-pretrained/renderings' \
  -e '\./flyvis-pretrained/results/flow/0000/000/__cache__' \
  -e '\./flyvis-pretrained/results/flow/0000/049/__cache__' \
  -e '\./\.flyvis-reference\.' target/data-after.txt
/bin/bash ci/fetch-fixtures.sh
```

Expected: `data-unchanged`, then `0` (the patterns are anchored to the paths the spec's O3 item 2 and flow 1 name, so the two pinned `flyvis-supplementary/Lappalainen_et_al_connectome_*` files do not count; `grep -c` exits 1 on a `0`, which is expected), then `fetch-fixtures: all 20 fixtures present and verified in <repo>/data`.

- [ ] **Step 6: Validate the set against the manifest field table and the Sizes table**

```bash
uv run --project tools tools/test_export_reference.py --validate data/flyvis-reference
ls data/flyvis-reference | wc -l
wc -c data/flyvis-stimuli/moving_edge_offsets_h80.bin
python3 -I -c 'import json; a=json.load(open("data/flyvis-reference/PROVENANCE.json")); b=json.load(open("data/flyvis-stimuli/moving_edges_h80_provenance.json")); print(a["export_id"] == b["export_id"], a["git"]["dirty"], a["threads"])'
```

Expected: `<repo>/data/flyvis-reference: manifest valid, 9 files verified, 11 files in all`; `11`; `484512 data/flyvis-stimuli/moving_edge_offsets_h80.bin`; `True False {'default': ..., 'determinism_check': [1, ...], 'bit_equal': True, 'runs_checked': [0, 1]}`. `--validate` checks every top-level field, every `files[]` entry, dtype against name, contiguous blocks, `bytes == Σ blocks`, each block against the storage layout (so every length equals the Sizes table), and each file's length and sha256.

- [ ] **Step 7: Rerun without `--force` (flow 2)**

```bash
time uv run --project tools tools/export_reference.py; echo rc=$?
/bin/bash -c '( cd data && find . \( -path ./flyvis-reference \
      -o -path ./flyvis-stimuli/moving_edge_offsets_h80.bin \
      -o -path ./flyvis-stimuli/moving_edge_offsets_h80.json \
      -o -path ./flyvis-stimuli/moving_edges_h80_provenance.json \) -prune \
    -o \( -type f -exec shasum -a 256 {} + \) \
    -o -print | LC_ALL=C sort )' | diff target/data-after.txt - && echo untouched
shasum -a 256 data/flyvis-reference/* data/flyvis-stimuli/*_h80* > target/reference-sums.txt
```

Expected: `export_reference: <repo>/data/flyvis-reference exists; rerun with --force to replace it`, `rc=1`, a wall time of a few seconds (it never imported flyvis), then `untouched`.

- [ ] **Step 8: Check that the published set is unchanged by flow 2**

```bash
shasum -a 256 -c --quiet target/reference-sums.txt && echo outputs-unchanged
```

Expected: `outputs-unchanged`.

- [ ] **Step 9: Rerun into a scratch output and compare (flow 3)**

```bash
uv run --project tools tools/export_reference.py --out target/reref > target/reref.log 2>&1; echo rc=$?
tail -1 target/reref.log
for f in settled.f32.bin settled.f64.bin settling_centre.f32.bin settling_centre.f64.bin \
         snapshots.f32.bin snapshots.f64.bin centre.f32.bin centre.f64.bin input.f64.bin manifest.json; do
  cmp data/flyvis-reference/$f target/reref/flyvis-reference/$f && echo "same $f"
done
for f in moving_edge_offsets_h80.bin moving_edge_offsets_h80.json; do
  cmp data/flyvis-stimuli/$f target/reref/flyvis-stimuli/$f && echo "same $f"
done
cmp -s data/flyvis-reference/PROVENANCE.json target/reref/flyvis-reference/PROVENANCE.json || echo "differs PROVENANCE.json"
cmp -s data/flyvis-stimuli/moving_edges_h80_provenance.json target/reref/flyvis-stimuli/moving_edges_h80_provenance.json || echo "differs moving_edges_h80_provenance.json"
ls target/reref/flyvis-stimuli
shasum -a 256 -c --quiet target/reference-sums.txt && echo outputs-unchanged
rm -rf target/reref
```

Expected: `rc=0`; `  all assertions passed`; twelve `same ...` lines; `differs PROVENANCE.json` and `differs moving_edges_h80_provenance.json`; exactly the three `_h80` file names; `outputs-unchanged`. Any other difference means the export is not deterministic: stop and report.

No commit: nothing in git changed.

---

### Task 7: The geometry gate at height 80

**Files:**
- Modify: `crates/flyvis/src/moving_edge_checks.rs:29-31` (module doc) and `:137-236` (the gate)
- Modify: `crates/flyvis/src/test_support.rs:106-111,130-135`
- Modify: `crates/flyvis/src/moving_edge.rs:236-240`

**Interfaces:**
- Consumes: `data/flyvis-stimuli/moving_edge_offsets_h80.{bin,json}` from Task 6; the existing helpers `f32s`, `json_array`, `json_field`, `json_field_all`, `json_field_all_f64`, `load_stimulus_bytes`, `load_stimulus_fixture`, `MovingEdgeParams::flyvis_defaults`, `MovingEdge::{new, speed_count, first_frame_selecting, frame_into}`, constants `DT`, `GATE_ANGLES`, `TOLERANCE`.
- Produces: private `struct Report { height: i32, rows: Vec<(i32, f64, f64, usize, usize)>, failures: Vec<String> }` with `print(&self)` and `tolerance_failures(&self) -> Vec<String>`; private `fn geometry_report(height: i32, bin: &str, json: &str) -> Option<Report>`; private `fn panic_message(panic: &(dyn std::any::Any + Send)) -> String`. The test keeps its name, `moving_edge_offsets_match_flyvis_within_tolerance`.

Each half prints its table as soon as it is computed. Sidecar problems and tolerance breaches are collected as messages, never asserted mid-way, and one `assert!` at the end reports them all. With `SEMBLANCE_REQUIRE_FIXTURES=1` a missing file still panics inside the loader, as R10 requires; the caller catches that panic with `std::panic::catch_unwind`, records its message (which names the path), and runs the other half, so the height-9 table prints even when the height-80 pair is missing.

- [ ] **Step 1: Confirm the gate has no height-80 half yet**

```bash
ls data/flyvis-stimuli/moving_edge_offsets_h80.bin data/flyvis-stimuli/moving_edge_offsets_h80.json
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis -- --exact --nocapture \
  moving_edge_checks::moving_edge_offsets_match_flyvis_within_tolerance 2>&1 | grep -c 'height 80'
```

Expected: both paths listed, then `0`: the h80 fixtures exist and nothing reads them.

- [ ] **Step 2: Replace the gate with two independently loaded halves**

In `crates/flyvis/src/moving_edge_checks.rs`, replace everything from the line `/// The geometry gate. Tolerance-gated, and it prints what it observed.` (line 137) through the closing `}` of `fn moving_edge_offsets_match_flyvis_within_tolerance` (line 236, the line before the blank line that precedes `/// The ownership map, against flyvis's own.`) with:

```rust
/// One height's comparison: the per-angle rows -- angle, max, mean, hexals
/// over `TOLERANCE`, hexals over 0.01 -- plus every sidecar check that failed.
/// Failures are collected rather than asserted, so both heights' tables print
/// before anything fails.
struct Report {
    height: i32,
    rows: Vec<(i32, f64, f64, usize, usize)>,
    failures: Vec<String>,
}

impl Report {
    /// The distribution, per angle. A failing gate must still say what it saw.
    fn print(&self) {
        eprintln!(
            "moving edge geometry, height {}, against flyvis 1.2.0 ({} values per angle):",
            self.height,
            2 * 21 * 721
        );
        eprintln!("  angle   max abs      mean   > {TOLERANCE}   > 0.01   regime");
        for &(angle, max, mean, over_tol, over_hundredth) in &self.rows {
            let regime = match angle {
                0 => "no rotation",
                90 | 180 | 270 => "axis-aligned",
                _ => "bilinear",
            };
            eprintln!("  {angle:>5}   {max:.6}  {mean:.6}   {over_tol:>7}   {over_hundredth:>6}   {regime}");
        }
        eprintln!(
            "  expected floor: 0.5 - 127/255 = {:.6}, flyvis's background after its uint8 cast",
            0.5 - 127.0 / 255.0
        );
    }

    /// `TOLERANCE`, per angle, as failure messages.
    fn tolerance_failures(&self) -> Vec<String> {
        let h = self.height;
        // At height 80 the bar fills the monitor and every owned pixel goes
        // through the source-row clamp in `inverse_rotate`, which no height-9
        // case reaches.
        let suspect = if h == 9 {
            "The likeliest cause is the rotation centre."
        } else {
            "The likeliest cause is the rotation centre; at this height the source-row clamp is the first suspect."
        };
        let mut out = Vec::new();
        for &(angle, max, _, over_tol, _) in &self.rows {
            if max.is_nan() || max > TOLERANCE {
                out.push(format!(
                    "height {h}, angle {angle}: max abs difference {max:.6} exceeds {TOLERANCE}. \
                     Do not widen this bound -- see the TOLERANCE doc comment. {suspect}"
                ));
            }
            if over_tol != 0 {
                out.push(format!("height {h}, angle {angle}: {over_tol} hexals over tolerance"));
            }
        }
        out
    }
}

/// Loads one height's fixture pair and compares Rust's rendering against it.
///
/// `None` when the pair is absent and fixtures are not required: this half is
/// skipped and the other still runs. With `SEMBLANCE_REQUIRE_FIXTURES=1` the
/// loaders panic on a missing file instead; the caller catches that, so the
/// other half still prints.
fn geometry_report(height: i32, bin: &str, json: &str) -> Option<Report> {
    let (Some(bytes), Some(manifest)) = (load_stimulus_bytes(bin), load_stimulus_fixture(json)) else {
        return None;
    };
    let mut failures = Vec::new();
    let want = f32s(&bytes);
    if want.len() != 8 * 21 * 721 {
        failures.push(format!(
            "height {height}: fixture value count {} != {}",
            want.len(),
            8 * 21 * 721
        ));
        return Some(Report { height, rows: Vec::new(), failures });
    }

    // The fixture's own account of what it holds, against what this gate
    // assumes. Without this, reordering GATE_ANGLES without regenerating the
    // fixtures would surface only as a tolerance failure blaming the rotation
    // centre.
    let shape = json_array(&manifest, "shape");
    if shape != vec![(GATE_ANGLES.len() * 2) as i64, 21, 721] {
        failures.push(format!("height {height}: fixture shape {shape:?}"));
    }
    let indices = json_field_all(&manifest, "index");
    if indices != (0..(GATE_ANGLES.len() * 2) as i64).collect::<Vec<_>>() {
        failures.push(format!("height {height}: fixture group indices {indices:?}"));
    }
    let angles = json_field_all(&manifest, "angle");
    if angles != GATE_ANGLES.iter().flat_map(|&a| [a as i64, a as i64]).collect::<Vec<_>>() {
        failures.push(format!(
            "height {height}: fixture group order must be angle-major, intensity-minor (angles {angles:?})"
        ));
    }
    let intensities = json_field_all_f64(&manifest, "intensity");
    if intensities != GATE_ANGLES.iter().flat_map(|_| [0.0, 1.0]).collect::<Vec<_>>() {
        failures.push(format!(
            "height {height}: fixture group order must be angle-major, intensity-minor (intensities {intensities:?})"
        ));
    }
    // The height-9 sidecar predates the field; every other height must name
    // itself, so the two pairs cannot be swapped silently.
    if height != 9 {
        let got = if manifest.contains("\"height\":") { json_field(&manifest, "height") } else { "<missing>" };
        if got != height.to_string() {
            failures.push(format!("height {height}: sidecar height is {got}"));
        }
    }

    let mut params = MovingEdgeParams::flyvis_defaults(DT);
    params.angles = GATE_ANGLES.to_vec();
    params.height = height;
    let e = MovingEdge::new(params);

    // Per regime, so the three numerical footings stay distinguishable.
    let mut rows: Vec<(i32, f64, f64, usize, usize)> = Vec::new();
    let mut out = vec![0.0f32; 721];

    for (ai, &angle) in GATE_ANGLES.iter().enumerate() {
        let (mut max, mut sum, mut over_tol, mut over_hundredth) = (0.0f64, 0.0f64, 0, 0);
        for ii in 0..2 {
            // Sequence index is angle-major, then intensity, then speed. The
            // slowest speed (index 0) sweeps all 21 offsets over 678 frames; we
            // compare the offsets directly by stepping to the frame that
            // selects each one.
            let sequence = (ai * 2 + ii) * e.speed_count();
            for offset in 0..21 {
                let frame = e.first_frame_selecting(sequence, offset).unwrap();
                e.frame_into(sequence, frame, &mut out).unwrap();
                let base = ((ai * 2 + ii) * 21 + offset) * 721;
                for (h, &got) in out.iter().enumerate() {
                    let d = (got as f64 - want[base + h] as f64).abs();
                    max = max.max(d);
                    sum += d;
                    if d > TOLERANCE {
                        over_tol += 1;
                    }
                    if d > 0.01 {
                        over_hundredth += 1;
                    }
                }
            }
        }
        let n = (2 * 21 * 721) as f64;
        rows.push((angle, max, sum / n, over_tol, over_hundredth));
    }
    Some(Report { height, rows, failures })
}

/// The message a caught panic carried, for the final assertion.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_owned()
    } else {
        "panicked".to_owned()
    }
}

/// The geometry gate, at flyvis's default height 9 and at height 80, the
/// height flyvis's own analyses and the reference traces use.
/// Tolerance-gated, and it prints what it observed.
///
/// The two halves load independently: a missing pair skips (or, with
/// `SEMBLANCE_REQUIRE_FIXTURES=1`, fails) only its own half. Each table
/// prints as soon as its half is computed; every failure is collected and
/// asserted once, at the end.
#[test]
fn moving_edge_offsets_match_flyvis_within_tolerance() {
    let halves = [
        (9, "moving_edge_offsets.bin", "moving_edge_offsets.json"),
        (80, "moving_edge_offsets_h80.bin", "moving_edge_offsets_h80.json"),
    ];
    let mut failures = Vec::new();
    for (height, bin, json) in halves {
        match std::panic::catch_unwind(|| geometry_report(height, bin, json)) {
            Ok(Some(report)) => {
                report.print();
                let over = report.tolerance_failures();
                failures.extend(report.failures);
                failures.extend(over);
            }
            Ok(None) => {}
            Err(panic) => failures.push(format!("height {height}: {}", panic_message(&*panic))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
```

- [ ] **Step 3: Run the gate and read both tables (flow 4)**

```bash
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis -- --exact --nocapture \
  moving_edge_checks::moving_edge_offsets_match_flyvis_within_tolerance 2>&1 | grep -v '^$' | tee target/gate.log
sed -n '/height 80, against/,/expected floor/p' target/gate.log > target/h80-gate.txt
```

Expected: the height-9 table exactly as today,

```text
moving edge geometry, height 9, against flyvis 1.2.0 (30282 values per angle):
  angle   max abs      mean   > 0.005   > 0.01   regime
      0   0.000794  0.000010         0        0   no rotation
     30   0.002823  0.001808         0        0   bilinear
     90   0.002692  0.001819         0        0   axis-aligned
    150   0.003057  0.001812         0        0   bilinear
  expected floor: 0.5 - 127/255 = 0.001961, flyvis's background after its uint8 cast
```

then `moving edge geometry, height 80, against flyvis 1.2.0 (30282 values per angle):` with the same columns and its own four rows, then `test result: ok. 1 passed`. `target/h80-gate.txt` now holds the height-80 table; Task 10 quotes it. **If any height-80 angle's max exceeds 0.005, or any hexal is over it, stop and report the table to the owner.** The first suspect is the source-row clamp in `inverse_rotate` (`moving_edge.rs:202-216`), which only a full-height bar reaches. `TOLERANCE` is never widened.

- [ ] **Step 4: Prove the halves are independent**

```bash
mkdir -p target/h80-aside
mv data/flyvis-stimuli/moving_edge_offsets_h80.bin data/flyvis-stimuli/moving_edge_offsets_h80.json target/h80-aside/
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis -- --exact --nocapture \
  moving_edge_checks::moving_edge_offsets_match_flyvis_within_tolerance 2>&1 | grep -e 'height 9,' -e 'height 80:' -e 'test result'
cargo test -p semblance-flyvis -- --exact --nocapture \
  moving_edge_checks::moving_edge_offsets_match_flyvis_within_tolerance 2>&1 | grep -e 'height 9,' -e 'skipping' -e 'test result'
mv target/h80-aside/moving_edge_offsets_h80.bin target/h80-aside/moving_edge_offsets_h80.json data/flyvis-stimuli/
shasum -a 256 -c --quiet target/reference-sums.txt && echo outputs-unchanged
```

Expected, first run: `moving edge geometry, height 9, against ...`, then `height 80: SEMBLANCE_REQUIRE_FIXTURES=1 but <repo>/crates/flyvis/../../data/flyvis-stimuli/moving_edge_offsets_h80.bin is missing`, then `test result: FAILED. 0 passed; 1 failed`. Second run: the height-9 header, two `skipping: ...moving_edge_offsets_h80...` lines, and `test result: ok. 1 passed`. Then `outputs-unchanged`.

- [ ] **Step 5: Name the new exporter in the stimulus loaders' skip messages**

In `crates/flyvis/src/test_support.rs`, both `load_stimulus_fixture` (lines 106-111) and `load_stimulus_bytes` (lines 130-135) print the same message. Replace each occurrence of

```rust
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_stimuli.py` and \
                 `uv run --project tools tools/export_moving_edges.py` \
                 to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
            );
```

with

```rust
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_stimuli.py`, \
                 `uv run --project tools tools/export_moving_edges.py` and, \
                 for the `_h80` files, `uv run --project tools tools/export_reference.py` \
                 to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
            );
```

Also replace the doc comment above `load_stimulus_fixture` (lines 92-93),

```rust
/// Loads a stimulus fixture exported from flyvis, or `None` if the export has
/// not been run. See `tools/export_stimuli.py`.
```

with

```rust
/// Loads a stimulus fixture exported from flyvis, or `None` if the export has
/// not been run. See `tools/export_stimuli.py`, `tools/export_moving_edges.py`
/// and, for the `_h80` files, `tools/export_reference.py`.
```

- [ ] **Step 6: Make the row-band comment height-dependent**

In `crates/flyvis/src/moving_edge.rs`, replace

```rust
    /// One pass over the monitor per angle finds the pixels whose rotated
    /// source lands in the bar's row band -- 88 of the monitor's 775 rows --
    /// and the 21 offsets are then cheap sweeps over that band alone.
```

with

```rust
    /// One pass over the monitor per angle finds the pixels whose rotated
    /// source lands in the bar's row band -- 88 of the monitor's 775 rows at
    /// flyvis's default height 9, all 775 at height 80, where the source-row
    /// clamp in `inverse_rotate` decides membership -- and the 21 offsets are
    /// then cheap sweeps over that band alone.
```

- [ ] **Step 7: Measure the crate's debug suite time**

```bash
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis > /dev/null 2>&1
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis 2>&1 | grep 'test result' | head -1
```

Expected: `test result: ok. 167 passed; 0 failed; ... finished in T s` for the library tests (the first run only builds). Note `T`.

- [ ] **Step 8: Replace the module doc's timing claim with the measurement**

In `crates/flyvis/src/moving_edge_checks.rs`, replace lines 29-31,

```rust
//! The geometry gate renders `GATE_ANGLES`' worth of offsets -- four, not
//! twelve -- and the whole crate's debug suite runs in about a second;
//! nothing here needs `--release` to be comfortable.
```

with these lines, writing Step 7's `T` rounded to one decimal where the text says `T`:

```rust
//! The geometry gate renders `GATE_ANGLES`' worth of offsets -- four, not
//! twelve -- at two heights. At height 80 the bar fills the monitor, so every
//! owned pixel is swept rather than an 88-row band. Measured after that
//! change: the crate's debug test suite finishes in T s
//! (`cargo test -p semblance-flyvis`, libtest's own figure).
```

- [ ] **Step 9: Full verification**

```bash
cargo build --workspace
cargo test --workspace 2>&1 | grep 'test result' | awk '{p+=$4; f+=$6} END {print p " passed, " f " failed"}'
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace 2>&1 | grep 'test result' | awk '{p+=$4; f+=$6} END {print p " passed, " f " failed"}'
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -1
cargo run --quiet -- roundtrip
grep -c '^\[\[package\]\]' Cargo.lock
git diff --stat
```

Expected: build succeeds; `203 passed, 0 failed` twice; clippy ends `Finished ...` with no warning; both roundtrip distances read `0.000000`; `8`; and the diff touches exactly `crates/flyvis/src/moving_edge.rs`, `crates/flyvis/src/moving_edge_checks.rs` and `crates/flyvis/src/test_support.rs`.

- [ ] **Step 10: Commit**

```bash
git add crates/flyvis/src/moving_edge_checks.rs crates/flyvis/src/test_support.rs crates/flyvis/src/moving_edge.rs
git commit -m "Gate the moving-edge geometry at height 80 as well as 9"
```

---

### Task 8: Cache the whole fixture set in CI

**Files:**
- Modify: `.github/workflows/ci.yml:43-51`

**Interfaces:**
- Consumes: `ci/fixtures.sha256`, `ci/fixtures-tag`, `ci/fetch-fixtures.sh` (unchanged logic).
- Produces: steps `Restore fixtures` (id `fixtures-cache`), `Fetch and verify fixtures` (unchanged), `Save fixtures`.

- [ ] **Step 1: Write the check, and see it fail**

Save as `target/check-ci-cache.py` (gitignored) and run it:

```python
import yaml

PATHS = [
    "data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json",
    "data/flyvis-build",
    "data/flyvis-stimuli",
    "data/flyvis-reference",
    "data/flyvis-pretrained/ensemble.csv",
]
steps = yaml.safe_load(open(".github/workflows/ci.yml"))["jobs"]["test"]["steps"]
names = [s.get("name") for s in steps]
r, f, s = names.index("Restore fixtures"), names.index("Fetch and verify fixtures"), names.index("Save fixtures")
assert (f, s) == (r + 1, r + 2), names
restore, fetch, save = steps[r], steps[f], steps[s]
assert restore["id"] == "fixtures-cache" and restore["uses"] == "actions/cache/restore@v4"
assert restore["with"]["path"].split() == PATHS, restore["with"]["path"]
assert save["with"]["path"] == restore["with"]["path"], "Save paths must equal Restore paths, in order"
assert restore["with"]["key"] == "fixtures-${{ hashFiles('ci/fixtures.sha256', 'ci/fixtures-tag') }}"
assert set(restore["with"]) == {"path", "key"} and set(save["with"]) == {"path", "key"}, "no restore-keys"
assert fetch == {"name": "Fetch and verify fixtures", "env": {"GH_TOKEN": "${{ github.token }}"},
                 "run": "ci/fetch-fixtures.sh"}, fetch
assert save["uses"] == "actions/cache/save@v4"
assert save["if"] == "steps.fixtures-cache.outputs.cache-hit != 'true'" and save["continue-on-error"] is True
assert save["with"]["key"] == "${{ steps.fixtures-cache.outputs.cache-primary-key }}"
assert "connectome-json" not in open(".github/workflows/ci.yml").read()
assert [x.get("uses") for x in steps].count("actions/cache@v4") == 1, "only the cargo cache stays"
print("ci.yml fixture cache: ok")
```

Run: `tools/.venv/bin/python -I target/check-ci-cache.py`
Expected: FAIL with `ValueError: 'Restore fixtures' is not in list`.

- [ ] **Step 2: Replace the connectome cache step**

In `.github/workflows/ci.yml`, replace lines 43-51,

```yaml
      - uses: actions/cache@v4
        with:
          path: data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json
          key: connectome-json-e609438ef261f4d12bf12556f4edb7a898ca4720029f9b882868967e6a0bbc73

      - name: Fetch and verify fixtures
        env:
          GH_TOKEN: ${{ github.token }}
        run: ci/fetch-fixtures.sh
```

with

```yaml
      # The whole fixture set, keyed exactly on the manifest and the tag, with
      # no restore-keys: a key change always starts clean, because stale
      # restored files would be refused by fetch-fixtures.sh's never-overwrite
      # rule. The fetch step re-verifies every restored file, so the cache only
      # ever saves a download. Restore and save are separate steps so a run
      # whose tests fail still saves a verified set.
      #
      # The Save step's paths must equal these, in this order: the cache
      # version is computed from them.
      #
      # A bad entry fails every run that restores it, at the fetch step, naming
      # the file. Recover by deleting the entry (`gh cache list --key fixtures-`
      # to find it, `gh cache delete <key>` to remove it) and re-running.
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

      - name: Fetch and verify fixtures
        env:
          GH_TOKEN: ${{ github.token }}
        run: ci/fetch-fixtures.sh

      # continue-on-error is deliberate: a failed save (say, a concurrent run
      # already reserved the key) only costs a download on the next run.
      # Correctness rests on the fetch step's verification, never on the cache.
      - name: Save fixtures
        if: steps.fixtures-cache.outputs.cache-hit != 'true'
        continue-on-error: true
        uses: actions/cache/save@v4
        with:
          path: |
            data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json
            data/flyvis-build
            data/flyvis-stimuli
            data/flyvis-reference
            data/flyvis-pretrained/ensemble.csv
          key: ${{ steps.fixtures-cache.outputs.cache-primary-key }}
```

- [ ] **Step 3: Run the check to see it pass**

Run: `tools/.venv/bin/python -I target/check-ci-cache.py && git diff --stat`
Expected: `ci.yml fixture cache: ok`, and only `.github/workflows/ci.yml` changed. The cargo cache steps, the `permissions: contents: read` block and every other step are byte-identical (`git diff` shows only the hunk above).

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "Cache the whole fixture set in CI under an exact key"
```

---

### Task 9: Publish `fixtures-2` and pin it

**Files:**
- Regenerate: `ci/fixtures.sha256`
- Modify: `ci/fixtures-tag`
- Modify: `ci/fetch-fixtures.sh:2-16` (header comment only)
- Modify: `CLAUDE.md:127-131,141-145`

**Interfaces:**
- Consumes: the exported set (Task 6), the h80 gate (Task 7), the CI cache (Task 8).
- Produces: release `fixtures-2` on `miki-przygoda/semblance` with one asset, `flyvis-fixtures.tar.gz`; `ci/fixtures.sha256` (35 lines) and `ci/fixtures-tag` (`fixtures-2`) in one commit.

- [ ] **Step 1: Confirm the 20 fixtures-1 files still verify against the old manifest**

Run: `git diff --quiet ci/ && /bin/bash ci/fetch-fixtures.sh`
Expected: `fetch-fixtures: all 20 fixtures present and verified in <repo>/data`. The tarball is built only from files that pass this.

- [ ] **Step 2: Regenerate the manifest**

Run this block from the repo root, exactly as the spec gives it:

```bash
(
set -euo pipefail
cd data
for f in moving_edge_offsets_h80.bin moving_edge_offsets_h80.json moving_edges_h80_provenance.json; do
  test -f "flyvis-stimuli/$f" || { echo "missing flyvis-stimuli/$f" >&2; exit 1; }
done
python3 -I -c 'import json, sys
a = json.load(open("flyvis-reference/PROVENANCE.json"))["export_id"]
b = json.load(open("flyvis-stimuli/moving_edges_h80_provenance.json"))["export_id"]
sys.exit(0 if a == b else "export_id mismatch: %s vs %s" % (a, b))'
{ shasum -a 256 flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json
  find flyvis-build flyvis-stimuli flyvis-reference -type f ! -name '.*' | LC_ALL=C sort | xargs shasum -a 256
  shasum -a 256 flyvis-pretrained/ensemble.csv
} > ../ci/fixtures.sha256
)
wc -l < ci/fixtures.sha256
git diff --numstat ci/fixtures.sha256
git diff ci/fixtures.sha256 | grep '^+[0-9a-f]' | awk '{print $2}'
```

Expected: `35`; `15	0	ci/fixtures.sha256`; and exactly these 15 added paths, in this order:

```text
flyvis-stimuli/moving_edge_offsets_h80.bin
flyvis-stimuli/moving_edge_offsets_h80.json
flyvis-stimuli/moving_edges_h80_provenance.json
flyvis-reference/PROVENANCE.json
flyvis-reference/centre.f32.bin
flyvis-reference/centre.f64.bin
flyvis-reference/input.f64.bin
flyvis-reference/manifest.json
flyvis-reference/settled.f32.bin
flyvis-reference/settled.f64.bin
flyvis-reference/settling_centre.f32.bin
flyvis-reference/settling_centre.f64.bin
flyvis-reference/snapshots.f32.bin
flyvis-reference/snapshots.f64.bin
flyvis-pretrained/ensemble.csv
```

If the count or the numstat differs, stop: `git checkout ci/fixtures.sha256` and find out why before going on.

- [ ] **Step 3: Set the tag**

```bash
printf 'fixtures-2\n' > ci/fixtures-tag
git diff ci/fixtures-tag
```

Expected: one line changed, `-fixtures-1` / `+fixtures-2`.

- [ ] **Step 4: Update the fetch script's header, and only its header**

In `ci/fetch-fixtures.sh`, replace lines 4-16,

```bash
# Two sources, both pinned by ci/fixtures.sha256:
#   - the connectome JSON, from the Nature supplementary zip (Springer)
#   - flyvis's own exports (data/flyvis-build, data/flyvis-stimuli), from the
#     private release named in ci/fixtures-tag
#
# Fetches only what is missing. A file that exists but fails verification is
# never overwritten -- locally it may be a deliberate re-export -- and the
# script fails naming it. Written for bash 3.2 (macOS's /bin/bash).
#
# Re-exporting fixtures (tarball, release, manifest): the commands are in
# docs/superpowers/plans/2026-10-01-ci.md, Task 3 (manifest: Task 2 Step 1).
#
# Design: docs/superpowers/specs/2026-10-01-ci-design.md
```

with

```bash
# Two sources, both pinned by ci/fixtures.sha256:
#   - the connectome JSON, from the Nature supplementary zip (Springer)
#   - flyvis's own exports (data/flyvis-build, data/flyvis-stimuli,
#     data/flyvis-reference, data/flyvis-pretrained/ensemble.csv), from the
#     private release named in ci/fixtures-tag
#
# Fetches only what is missing. A file that exists but fails verification is
# never overwritten -- locally it may be a deliberate re-export -- and the
# script fails naming it. Written for bash 3.2 (macOS's /bin/bash).
#
# Every export is dated -- in its provenance file, and ensemble.csv in its
# header -- so re-running an exporter (tools/export_params.py included) makes
# this script refuse the new file. To return to the published set, delete the
# re-exported files and run this script again. A deliberate re-export needs a
# new fixtures-N tag, with ci/fixtures.sha256 and ci/fixtures-tag changed in
# one commit.
#
# Re-exporting fixtures (tarball, release, manifest): fixtures-1 in
# docs/superpowers/plans/2026-10-01-ci.md, Task 3 (manifest: Task 2 Step 1);
# fixtures-2, which added data/flyvis-reference and ensemble.csv, in
# docs/superpowers/plans/2026-10-26-flyvis-reference-traces.md, Task 9.
#
# Design: docs/superpowers/specs/2026-10-01-ci-design.md, extended for
# fixtures-2 by docs/superpowers/specs/2026-10-26-flyvis-reference-traces-design.md
```

Then check that no code line changed:

```bash
git diff -U0 ci/fetch-fixtures.sh | grep '^[-+]' | grep -v -e '^[-+][-+]' -e '^[-+]#' | wc -l
/bin/bash -n ci/fetch-fixtures.sh && echo syntax-ok
```

Expected: `0`, then `syntax-ok`.

- [ ] **Step 5: Update `CLAUDE.md`'s fixture paragraphs**

Replace

```markdown
first is fetched per `RESOURCES.md`, the other two produced by
`tools/export_build.py`, and `tools/export_stimuli.py` with
`tools/export_moving_edges.py` respectively). A skip prints
```

with

```markdown
first is fetched per `RESOURCES.md`, the other two produced by
`tools/export_build.py`, and `tools/export_stimuli.py` with
`tools/export_moving_edges.py` respectively; `tools/export_reference.py`
also writes the three height-80 `_h80` moving-edge files). A skip prints
```

and replace

```markdown
is missing, and verifies every file against `ci/fixtures.sha256`. It never
overwrites a file that fails verification. CI runs the same script, then the
suite with `SEMBLANCE_REQUIRE_FIXTURES=1`.
```

with

```markdown
is missing, and verifies every file against `ci/fixtures.sha256`. It never
overwrites a file that fails verification. Since `fixtures-2` it also fetches
`data/flyvis-reference/` (flyvis's reference traces, from
`tools/export_reference.py`) and `data/flyvis-pretrained/ensemble.csv`; no
test reads either yet. CI runs the same script, then the
suite with `SEMBLANCE_REQUIRE_FIXTURES=1`.
```

The counts in `CLAUDE.md` (`203 tests`, `26 of those tests`) stay as they are: no test was added.

- [ ] **Step 6: Build and check the tarball**

```bash
mkdir -p target/fixtures-2
out="$PWD/target/fixtures-2/flyvis-fixtures.tar.gz"
rm -f "$out"
COPYFILE_DISABLE=1 tar -czf "$out" -C data flyvis-build flyvis-stimuli flyvis-reference flyvis-pretrained/ensemble.csv
tar -tzf "$out" | grep -v '/$' | LC_ALL=C sort > target/fixtures-2/listing
grep -c . target/fixtures-2/listing
grep -c '\._' target/fixtures-2/listing || true
diff <(sed -n '2,$p' ci/fixtures.sha256 | awk '{print $2}' | LC_ALL=C sort) target/fixtures-2/listing && echo match
ls -l "$out"; shasum -a 256 "$out"
```

Run it with `/bin/bash` (the `<( )` form). Expected: `34`; `0`; `match`; then the asset's size and sha256, both quoted in the next step.

- [ ] **Step 7: STOP and get the owner's explicit sign-off**

Tell the owner exactly what will be published: release `fixtures-2` on the private repo `miki-przygoda/semblance`, not marked latest, one asset `flyvis-fixtures.tar.gz` of <size from Step 6> bytes with sha256 <from Step 6>, holding 34 files (`flyvis-build/`, `flyvis-stimuli/` with the three new `_h80` files, `flyvis-reference/`, `flyvis-pretrained/ensemble.csv`). `fixtures-1` stays. Do not go on without a yes. The yes covers this exact file (this sha256) and nothing else.

- [ ] **Step 8: Create the release**

```bash
out="$PWD/target/fixtures-2/flyvis-fixtures.tar.gz"
gh release create fixtures-2 "$out" --latest=false \
  --title "fixtures-2: flyvis 1.2.0 exports and reference traces for CI" \
  --notes "Test fixtures for the flyvis acceptance gates and the 0.3.0 reference traces: data/flyvis-build, data/flyvis-stimuli (now with height-80 edge offsets), data/flyvis-reference and data/flyvis-pretrained/ensemble.csv, exported by tools/ from flyvis 1.2.0 (see each directory's provenance). Verified file-by-file against ci/fixtures.sha256 by ci/fetch-fixtures.sh. Not a version release."
```

Expected: the release URL.

- [ ] **Step 9: Verify the uploaded asset, and recover if the upload broke**

```bash
out="$PWD/target/fixtures-2/flyvis-fixtures.tar.gz"
gh release view fixtures-2 --json assets --jq '.assets[] | "\(.name) \(.size)"'
rm -rf target/fixtures-2/check && mkdir target/fixtures-2/check
gh release download fixtures-2 --pattern flyvis-fixtures.tar.gz --dir target/fixtures-2/check
cmp "$out" target/fixtures-2/check/flyvis-fixtures.tar.gz && echo asset-verified
```

Expected: `flyvis-fixtures.tar.gz <size from Step 6>`, then `asset-verified`. If `gh release create` failed or was interrupted after the release was created (a rerun says the release already exists), or the listed asset is missing or a different size, re-upload the same signed-off file, `shasum -a 256 "$out"` first to confirm it is the one from Step 6, with `gh release upload fixtures-2 "$out" --clobber`, then repeat this step. Never delete the release, and never upload any other file, without a new sign-off.

- [ ] **Step 10: Fresh fetch from empty (flow 6)**

```bash
t="$(mktemp -d)"
SEMBLANCE_DATA_DIR="$t" /bin/bash ci/fetch-fixtures.sh; echo rc=$?
rm -rf "$t"
```

Expected: `fetch-fixtures: fetching the connectome JSON from the Nature supplementary zip`, `fetch-fixtures: fetching flyvis-fixtures.tar.gz from release fixtures-2`, `fetch-fixtures: fetched 35 of 35 fixtures; all 35 verified`, `rc=0`.

- [ ] **Step 11: Upgrade from a fixtures-1 set (flow 7)**

```bash
t="$(mktemp -d)"
git show HEAD:ci/fixtures.sha256 | awk '{print $2}' | while read -r p; do
  mkdir -p "$t/$(dirname "$p")" && cp "data/$p" "$t/$p"
done
SEMBLANCE_DATA_DIR="$t" /bin/bash ci/fetch-fixtures.sh; echo rc=$?
git show HEAD:ci/fixtures.sha256 | (cd "$t" && shasum -a 256 -c --quiet -) && echo old-20-untouched
rm -rf "$t"
```

Expected: `fetch-fixtures: fetching flyvis-fixtures.tar.gz from release fixtures-2`, `fetch-fixtures: fetched 15 of 35 fixtures; all 35 verified`, `rc=0`, `old-20-untouched`. (`HEAD`'s manifest is still the fixtures-1 one: the commit is the next step.)

- [ ] **Step 12: Verify the local set against the new manifest, then commit**

```bash
/bin/bash ci/fetch-fixtures.sh
git add ci/fixtures.sha256 ci/fixtures-tag ci/fetch-fixtures.sh CLAUDE.md
git commit -m "Pin fixtures-2, with the reference traces and ensemble.csv"
git show --stat HEAD
```

Expected: `fetch-fixtures: all 35 fixtures present and verified in <repo>/data`, then a commit touching exactly those four files. Both `ci/` files change in this one commit, after the release exists.

---

### Task 10: The PR, CI's cache miss and hit, and a corrupt cache caught

**Files:** none on `reference-traces`. One throwaway branch, never merged, deleted after.

**Interfaces:**
- Consumes: every commit above, release `fixtures-2`, `target/reference-export.log` (Task 6), `target/h80-gate.txt` (Task 7).
- Produces: a PR from `reference-traces` into `main` whose description carries the A8 table, the exact file sizes and the height-80 figures; observed CI runs for flows 8 and 9.

- [ ] **Step 1: STOP and get the owner's go-ahead to push**

Pushing starts CI, which downloads `fixtures-2`. Confirm with the owner before pushing. Never use `--force`.

- [ ] **Step 2: Push and open the PR**

```bash
git push -u origin reference-traces
```

Then open the PR with `gh pr create --base main --head reference-traces --title "Export flyvis reference traces for 0.3.0, gate height 80, and publish fixtures-2" --body-file target/pr-body.md`, after writing `target/pr-body.md` with these sections, each filled from the named source:

```markdown
## What this does

Task 1 of 0.3.0: `tools/export_reference.py` exports flyvis 1.2.0's own traces
(models 000 and 049, dt 1/200 and 0.02, float32 and float64; 4 flashes and 8
moving edges per run) to `data/flyvis-reference/`, after checking A1–A9 in
memory. The moving-edge geometry gate now also runs at height 80. The fixture
set is published as `fixtures-2` and cached in CI under an exact key.

Spec: docs/superpowers/specs/2026-10-26-flyvis-reference-traces-design.md
Plan: docs/superpowers/plans/2026-10-26-flyvis-reference-traces.md

## Export run

<the indented lines of target/reference-export.log from `  models:` through
`  A7 gap 049 1/50: ...`, verbatim>

## A8 freeze check (all 50 checkpoints)

| figure | value |
|---|---|
| time_const below f32(0.02), total | 2,396 |
| per model (min / median / max) | 36 / 48.5 / 55 |
| models 000 / 049 | 51 / 36 |
| in [f32(0.02), 0.02] | 0 |
| largest below the floor / smallest at or above | <freeze_check.time_const from PROVENANCE.json> |
| below f32(1/200) | 0 |
| syn_strength exact zeros / negative | 919 / 0 |
| zeros per model (min / median / max) | 8 / 17.5 / 40 |
| zeros for 000 / 049 | 31 / 15 |
| connection types zero in some model / in all 50 | 418 / 0 |
| smallest positive syn_strength; counts ≤ 1e-6 / 1e-4 / 1e-2 | <freeze_check.syn_strength from PROVENANCE.json> |
| bias min / max; clamp | <freeze_check.bias from PROVENANCE.json> |

## Exact file sizes

| file | bytes |
|---|---|
| settled.f32.bin / .f64.bin | 730,704 / 1,461,408 |
| settling_centre.f32.bin / .f64.bin | 130,000 / 260,000 |
| snapshots.f32.bin / .f64.bin | 52,610,688 / 105,221,376 |
| centre.f32.bin / .f64.bin | 4,288,960 / 8,577,920 |
| input.f64.bin | 47,574,464 |
| manifest.json, PROVENANCE.json | <from `wc -c`> |
| moving_edge_offsets_h80.bin | 484,512 |
| fixtures-2 asset | <size and sha256 from Task 9 Step 6> |

## Moving-edge geometry, height 80 beside height 9

<target/h80-gate.txt, and the height-9 maxima 0.000794 / 0.002823 / 0.002692 /
0.003057 for angles 0 / 30 / 90 / 150, with one sentence comparing the regimes>

## Verification

<one line each: 42 self-tests; flows 1, 2, 3 (Task 6); flow 4 (Task 7 Steps 3-4);
build, 203 tests, clippy, roundtrip, Cargo.lock 8 packages (Task 7 Step 9);
flows 5-7 (Task 9)>

## For the owner

The A8 reading, these sizes and the D7 corrections belong in the local 0.3.0
epic README; this PR does not touch docs/project-outline/.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

Get the PROVENANCE figures with `python3 -I -c 'import json; print(json.dumps(json.load(open("data/flyvis-reference/PROVENANCE.json"))["freeze_check"], indent=2))'`.

- [ ] **Step 3: Watch the first run (flow 8, cache miss)**

```bash
gh pr checks --watch
run=$(gh run list --branch reference-traces --limit 1 --json databaseId --jq '.[0].databaseId')
gh run view "$run" --log --job "$(gh run view "$run" --json jobs --jq '.jobs[] | select(.name=="test") | .databaseId')" > target/ci-run-1.log
grep -c 'Cache not found' target/ci-run-1.log
grep 'fetch-fixtures: fetched' target/ci-run-1.log
grep -A7 'moving edge geometry, height' target/ci-run-1.log | head -16
grep -e 'Cache saved' -e 'Save fixtures' target/ci-run-1.log | head -3
```

Expected: every check green; `Cache not found` at least once (the `Restore fixtures` step, for `fixtures-<hash>`); `fetch-fixtures: fetched 35 of 35 fixtures; all 35 verified`; both height tables, with the same figures as Task 7 Step 3; and the save step's `Cache saved` line. If a table is missing, the gate did not run: stop.

- [ ] **Step 4: Re-run and see the cache hit (flow 8)**

```bash
gh run rerun "$run"
gh run watch "$run"
gh run view "$run" --log --job "$(gh run view "$run" --json jobs --jq '.jobs[] | select(.name=="test") | .databaseId')" > target/ci-run-2.log
grep 'Cache restored from key: fixtures-' target/ci-run-2.log
grep 'fetch-fixtures: all 35 fixtures present and verified' target/ci-run-2.log
grep -c 'fetching flyvis-fixtures.tar.gz' target/ci-run-2.log
```

Expected: `Cache restored from key: fixtures-<hash>`; `fetch-fixtures: all 35 fixtures present and verified in /home/runner/work/semblance/semblance/data`; `0` (no release download).

- [ ] **Step 5: A corrupt restored fixture is caught (flow 9, throwaway)**

```bash
git switch -c ci-break-cache reference-traces
```

In `.github/workflows/ci.yml`, insert between the `Restore fixtures` step and the `Fetch and verify fixtures` step:

```yaml
      - name: Throwaway - corrupt one restored fixture
        if: steps.fixtures-cache.outputs.cache-hit == 'true'
        run: printf 'X' | dd of=data/flyvis-reference/manifest.json bs=1 seek=0 count=1 conv=notrunc
```

```bash
git commit -am "Throwaway: corrupt one restored fixture before the fetch"
git push -u origin ci-break-cache
gh pr create --draft --base reference-traces --head ci-break-cache --title "Throwaway: CI must catch a corrupt restored fixture" --body "Do not merge. Verifies ci/fetch-fixtures.sh refuses a corrupted cache restore, naming the file."
gh pr view ci-break-cache --json number --jq .number > target/throwaway-pr.txt
```

The first run on this PR has no cache in its scope, so the corrupt step is skipped and the run passes and saves. Then re-run it (`gh run rerun <id>`): the restore hits, the byte is flipped, and the `Fetch and verify fixtures` step must fail, its log listing `fetch-fixtures: these fixtures exist but do not match ci/fixtures.sha256:` followed by `flyvis-reference/manifest.json`. The `Save fixtures` step must not run (the restore hit), so the corrupt set is never saved.

- [ ] **Step 6: Record and clean up**

Post one comment on the main PR linking the red run and quoting the failing lines. Then:

```bash
gh cache list --ref "refs/pull/$(cat target/throwaway-pr.txt)/merge"
gh pr close ci-break-cache --delete-branch
git switch reference-traces
git branch -D ci-break-cache 2>/dev/null || true
```

The first command lists the cache entry the throwaway PR saved; delete it with `gh cache delete <key> --ref refs/pull/<number>/merge`. It can only ever be restored by that closed PR, so this is tidiness, not safety.

- [ ] **Step 7: Hand over**

Tell the owner the PR is green with the cache hit shown, quote the A7 and A8 lines and the height-80 table, and ask for the merge go-ahead. Do not merge without it. The A8 reading, the sizes and the D7 corrections are the owner's to record in the local epic README.

---

## Spec open items

The spec review's minor open items, and where each is settled.

| item | how the plan settles it | where |
|---|---|---|
| contracts-6 (keep-list too narrow; buffer aliasing) | each sequence keeps a copy of `x`, a `.clone()` of photoreceptor row 0 (all 8 rows asserted equal first), centre and snapshots; the full traces of sequences 0 and 4 are kept until step 7, then dropped; A3/A7 run after all runs in `cross_checks` | Task 5, `play`, `run_one`, `cross_checks` |
| flyvis-5 (`stimulus_response` is a generator of numpy pairs) | consumed with `list(...)`, one batch asserted, `response` compared to `to_np(trace)` via `bits_equal` (dtype must match) | Task 5, `run_one` step 7 |
| contracts-7 (A6 covers f32 only) | A6 also runs on run 1 (f64); `threads.runs_checked: [0, 1]` | Task 5, `thread_check`; Spec gaps 3 |
| contracts-5 (`stimulus_frames` serialisation and meaning) | two-integer JSON array `[start, end]`, end exclusive; after `end` flashes are grey, edges hold their last frame; stated in the module docstring | Task 2 (`sequence_table`), Task 1 (docstring) |
| consistency-8 (inputs vs `--out`) | `DATA` and every input path derive from `ROOT` (the script's location); `--out` resolves against the cwd and only steers outputs; tested | Task 1, `test_out_defaults_to_repo_data_and_inputs_never_follow_it` |
| failure-4 + scope-9 (Save paths placeholder; `frame_semantics` bytes) | Save step's five paths written literally, same order, checked equal by `target/check-ci-cache.py`; `FRAME_SEMANTICS` fixed as three ASCII strings, no markdown, no F/P ids, tested | Task 8 Steps 1-2; Task 2 `FRAME_SEMANTICS` and `test_manifest_is_deterministic_and_valid` |
| consistency-10 + failure-3 (no recovery for a bad cache entry) | the ci.yml comment names it: `gh cache list --key fixtures-`, `gh cache delete <key>`, re-run; flow 9 proves the bad entry fails loudly and is never re-saved | Task 8 Step 2; Task 10 Step 5 |
| failure-1 (a locally re-exported `ensemble.csv` fails the fetch) | documented in `tools/README.md` and the fetch script's header: never re-export a pinned fixture by hand; delete and re-fetch, or cut a new tag | Task 1 Step 5; Task 9 Step 4 |
| failure-5 (release upload has no verification or retry) | `gh release view` size check, `gh release download` + `cmp`; retry with `gh release upload --clobber` of the same signed-off file only | Task 9 Step 9 |
| failure-6 (two exports collide on the scratch root) | an exclusive `flock` on `target/flyvis-reference.lock`, taken after preflight and before the scratch wipe, released by the OS on any exit; tested | Task 1 `acquire_lock`; Task 5 `main` |
| consistency-9 (REQUIRE panics would hide the h9 table) | each half prints as soon as it is computed; sidecar and tolerance problems are collected, not asserted; a loader panic is caught with `catch_unwind` and recorded; one assert at the end; proven by moving the h80 pair aside | Task 7 Steps 2 and 4 |
| contracts-8 (no proof every assertion ran; no manifest validator) | one printed line per assertion id A1–A9; `validate_manifest` (field table) and `verify_files` run at write step 3 and on the real set via `test_export_reference.py --validate` | Task 5; Task 2; Task 6 Steps 4 and 6 |
| scope-8 (matplotlib cache in `~`; absolute path in PROVENANCE) | `MPLCONFIGDIR` set to `target/flyvis-reference-mplconfig/` before `import flyvis`; `flyvis_root.path` is the repo-relative `target/flyvis-reference-root` | Task 5, `export` |

## Spec gaps found while planning

1. **A1 names no checkpoint file for "all 50 checkpoints".** The plan loads `chkpts/chkpt_00000`, the file `init_network("best")` loads. `export_params.py` read `best_chkpt`; both hold bit-identical network tensors in all 50 models (research F11, re-measured for this plan), and the plan's own real-data test passes against `ensemble.csv` and `pretrained.rs`. PROVENANCE records `parameter_equality.checkpoint_file`. No owner decision needed.
2. **The parameter keys are not bytes.** flyvis 1.2.0's `bias.keys` are `str` and `syn_strength.keys` are `(str, str)` tuples, so "after decoding bytes to str" is a no-op. The plan decodes defensively (`as_str`) and compares the edge keys with `CONN_TYPES` as tuples, which is what `CONN_TYPES` is. No owner decision needed.
3. **A6 extended to an f64 run.** The spec measures thread determinism on run 0 only, yet claims the export establishes that "the run is deterministic". The plan also checks run 1 (f64, 1/200) and adds `threads.runs_checked: [0, 1]` inside PROVENANCE's `threads` object. It changes no file Rust reads. The owner may ask to drop it; no decision is required to proceed. An A6 failure on run 1, as on run 0, goes to the owner (Task 6 Step 4); it is never resolved by dropping run 1.
4. **PROVENANCE carries three small additions** beyond the field table: `threads.runs_checked`, `parameter_equality.checkpoint_file`, and `flyvis_root.entries_copied` (with `flyvis_root.path` repo-relative and `models_copied: true`). PROVENANCE is volatile and only task 7 reads it. No owner decision needed.
5. **Files the spec does not name:** `tools/test_export_reference.py` (the self-tests and the manifest validator Done-when needs), `target/flyvis-reference.lock`, and `target/flyvis-reference-mplconfig/`. All are outside `data/`, so flow 1's listing is unaffected. No owner decision needed.
6. **Line references.** The "88 of the monitor's 775 rows" comment is at `moving_edge.rs:239`, not `:240`. The fetch script prints `all 35 fixtures present and verified in <dir>`; the spec quotes it without the ` in <dir>` suffix. The plan's expected outputs use the real text.
7. **The height-80 failure message names a second suspect.** The spec asks for the existing "do not widen" message plus the height; for height 80 the plan appends "at this height the source-row clamp is the first suspect", which is the spec's own guidance (F39, C21). No owner decision needed.
8. **Running the export, pushing and merging are owner-gated here.** The spec's flows run the export and push; this plan inserts STOP steps (Task 6 Step 3, Task 10 Steps 1 and 7) in addition to the spec's release sign-off.

No spec decision is changed.

## Spec coverage

Numbers are the ids of the requirement checklist this plan was written against, in spec order; each row also gives the spec lines, so it can be checked against the spec directly.

| requirement (spec lines) | task / step |
|---|---|
| 1–2 status, single offline script (L4-6, L40-41) | Tasks 1–5 |
| 3–4 2 models × 2 dts × 2 precisions; settle, 4 flashes, 8 edges (L43-48) | Task 1 `RUNS`, `DTS`, `FLASH_ARGS`, `EDGE_ARGS`; Task 5 `run_one` |
| 5 outputs to `data/flyvis-reference/` + h80 to `data/flyvis-stimuli/` (L49-52) | Task 3 `write_outputs`; Task 5 `export`; Task 6 Step 4 |
| 6 gate to h80, fixtures-2, CI cache, no Rust reads traces (L54-56) | Tasks 7, 9, 8 |
| 7 no `nv.flash_responses`/`moving_edge_responses` (L124-125) | Task 5 (`forward` and `stimulus_response` only) |
| 8 no exporter re-run; `_h80` files with sibling provenance (L132-133) | Global Constraints; Task 5 `h80`; Task 6 Step 5 |
| 9–10 CI spec/plan unedited; fixtures-1 kept; no bump | Global Constraints; Task 9 Step 7 |
| 11 non-goals | Global Constraints; File Structure (no loaders, no integrator) |
| 12 `docs/project-outline/` untouched | Global Constraints; Task 10 Step 7 |
| 13 zero deps, 8 packages, no prefixes, RNG untouched | Global Constraints; Task 7 Step 9 |
| 14–16 invocation, `--out`, `--force` (L297-305) | Task 1 `parse_args`; Task 5 Step 5; Task 6 Steps 4, 7, 9 |
| 17 exporter pattern R1–R6 (L307-313) | Task 1 Step 3 (docstring, `ROOT`/`OUT`, named constants); Task 5 (imports inside functions, print lines, `  all assertions passed`) |
| 18 refuse inherited `FLYVIS_ROOT_DIR` (L317-319, L1223) | Task 1 `preflight`, `test_preflight_refuses_an_inherited_flyvis_root`; Task 5 Step 5 |
| 19 scratch wipe, env vars (L320-321) | Task 5 `export` (process setup 2) |
| 20–21 copy the five entries, sha256 per file (L322-330) | Task 4 `copy_models`, `test_copy_models_copies_only_the_load_path_and_checks_it`; Task 5 `export` |
| 22 `weights_only=True` load of scratch copies before `init_network` (L330-335) | Task 5 `export` (process setup 3) |
| 23 `import flyvis`; device cpu; root is scratch (L336-338) | Task 5 `export` (process setup 4) |
| 24 no `use_deterministic_algorithms` (L339-341) | Task 5 `export` (comment; never called) |
| 25–27 losses for 50 dirs; argmin/argmax; exact values (L343-349) | Task 4 `read_validation_losses`, `rank_models`, `test_models_rank_as_recorded`; Task 5 `export` |
| 28 run table (L355-364) | Task 1 `RUNS`; Task 2 `test_blocks_are_contiguous_and_in_storage_order` |
| 29–30 execution order, dtype switch, dtype asserted per run (L366-369) | Task 1 `EXECUTION_ORDER`; Task 5 `export`, `run_one` |
| 31–32 dt as Python float; fresh objects per run (L371-373) | Task 1 `DTS`; Task 5 `run_one` |
| 33–34 `NetworkView`/`init_network("best")`; `.double()` after first check (L375-378) | Task 5 `run_one` steps 1–2 |
| 35–36 settle, count, settling column, settled; `return_last=True` bit-equal (L379-386) | Task 5 `run_one` steps 3–4 |
| 37 datasets with defaults (L387-394) | Task 5 `run_one` step 5 |
| 38–39 per-sequence path and keep-list (L395-412) | Task 5 `play`, `run_one` step 6 (open item contracts-6) |
| 40 `stimulus_response` equivalence after step 6 (L413-418) | Task 5 `run_one` step 7 (open item flyvis-5) |
| 41 settled untouched (L419-420) | Task 5 `run_one` step 8 |
| 42 batch size 1, recorded (L422-423) | Task 1 `BATCH_SIZE`; Task 2 manifest `batch_size`; Task 5 PROVENANCE `settling` |
| all checks before any write, bitwise (L425-430) | Task 5 `export` (write only after A1–A9); Task 4 `bits_equal` |
| 43–44 parse `pretrained.rs`; read `ensemble.csv` (L432-436) | Task 1 `parse_pretrained`, `read_ensemble` and their tests |
| 45–46 all 50 checkpoints vs csv and `pretrained.rs`; names (L438-444) | Task 4 `check_checkpoints`, `test_all_50_checkpoints_and_the_freeze_figures` |
| 47–49 per-run parameter check at three points; keys (L445-459) | Task 5 `check_network_params` (loaded, widened, after_runs) |
| 50–51 f64 `_param_api` tensors; `syn_count` log-space (L461-467) | Task 5 `check_param_dtypes`; raw values compared, never `exp` |
| 52–57 A2 precision (L469-482) | Task 5 `run_one`, `play`, `check_tensor`, `check_param_dtypes` |
| 58–63 A3 inputs at two levels; `input_rounding` (L484-498) | Task 5 `play` (8 rows), `cross_checks` |
| 64–69 A4 shapes, NaN, frame counts, arg rows (L500-513) | Task 5 `check_tensor`, `run_one`; Task 2 `test_sequence_table_matches_the_spec` |
| 70–75 A5 cross-checks (L515-542) | Task 5 `run_one` (nodes, centre, spans, indices, snapshots), `cross_checks` (flashes.bin); Task 5 tests |
| 76–77 A6 determinism (L544-550) | Task 5 `thread_check` (open item contracts-7) |
| 78–79 A7 gap ≤ 1e-2, recorded (L552-563) | Task 5 `cross_checks`; Task 6 Step 4 |
| 80–85 A8 freeze check (L565-611) | Task 4 `freeze_figures`, `FREEZE_EXPECTED`, `check_freeze`, real-data test; Task 5 print; Task 10 Step 2 |
| 86–89 A9 height-80 geometry (L613-625) | Task 5 `export` (A9), `run_one` (A9.4) |
| 90 `f32_floor_trap` recorded (L627-633) | Task 5 PROVENANCE `f32_floor_trap`; Task 5 `test_floor_dt_constants_are_f32_of_dt` |
| 91–92 precision files and the 11 files (L640-651) | Task 2 `file_layout`; Task 6 Step 6 |
| 93 deterministic manifest, volatile PROVENANCE (L650, L881) | Task 2 `test_manifest_is_deterministic_and_valid`; Task 6 Step 9 |
| 94 three new files in `flyvis-stimuli/`, no existing file opened for writing (L653-660) | Task 3 `write_outputs` (step 5 replaces only `_h80`); Task 6 Step 5 |
| 95 the gap is not stored | Task 2 `file_layout` (no gap file) |
| 96–97 `.bin` rules and block layouts (L767-783) | Task 2 `_file_entry`, `file_layout`, `pack_file` and tests |
| 98 axis meanings (L785-801) | Task 2 `FILE_STEMS`, `FRAME_SEMANTICS` |
| 99–105 `manifest.json` field by field; sequences (L803-877) | Task 2 `build_manifest`, `sequence_table`, `validate_manifest` and tests (open items contracts-5, failure-4) |
| 106–107 PROVENANCE.json (L879-910) | Task 5 `export` (`provenance`) |
| 108–110 h80 geometry files (L912-936) | Task 2 `h80_sidecar`; Task 5 `export` (`h80`, `h80_provenance`) |
| 111 `ensemble.csv` byte-for-byte (L938-951) | Task 1 `test_read_ensemble_real_file` (1,444,857 B); Task 9 Steps 2, 6 |
| 112–115 fail closed; finally before/after step 4; Ctrl-C (L1195-1213) | Task 3 `write_outputs` and its tests; Task 5 `main` |
| 116 preflight rules (L1215-1225) | Task 1 `preflight` and its tests; Task 5 Step 5 |
| 117–122 write steps 1–6 (L1229-1252) | Task 3 `write_outputs`, `check_pair` and tests |
| 123 restore `.old` only if `flyvis-reference` missing (L1254-1259) | Task 3 `test_a_crash_between_the_two_renames_restores_the_previous_set`, `test_a_crash_after_the_new_set_is_placed_deletes_nothing` |
| 124 recovery procedure (L1269-1285) | Task 1 `RECOVERY`, printed by preflight and by `write_outputs` |
| 125 reruns need `--force`; byte-identical (L1287-1292) | Task 6 Steps 7, 9; Task 9 Step 4 (header) |
| 126 A7, A8, h80 gate are findings (L1294-1297) | Task 4 Step 4; Task 6 Step 4; Task 7 Step 3 |
| 127–129 fixtures-2 contents; built from verified files (L684-702) | Task 9 Steps 1, 6 |
| 130–131 fixture-manifest block; 35 lines, `15 0` (L955-982) | Task 9 Step 2 |
| 132 tag and manifest in one commit, after the release (L704-708, L984) | Task 9 Steps 3, 12 |
| 133 tarball build and checks (L986-995) | Task 9 Step 6 |
| 134 release creation after sign-off (L998-1001) | Task 9 Steps 7–9 |
| 135–137 CI restore/save, five paths, no restore-keys (L1005-1041) | Task 8 |
| 138 docs: fetch header, `tools/README.md`, `CLAUDE.md` (L732-741) | Task 9 Steps 4–5; Task 1 Step 5 |
| 139–146 the Rust gate (L1043-1071) | Task 7 Steps 1–4 |
| 147 `test_support.rs` skip messages | Task 7 Step 5 |
| 148 `moving_edge.rs` comment | Task 7 Step 6 |
| 149 module-doc timing | Task 7 Steps 7–8 |
| 150 no test added; 203 / 26 | Task 7 Step 9; Task 9 Step 5 |
| 151–152 downstream contracts, loaders deferred (L1073-1101) | not built here (D3); manifest and files as Task 2 lays out |
| 153–158 security notes (L1299-1331) | Task 4 `load_checkpoint`; Task 5 `export` (safe load, copies, env); Task 8 comment; Task 9 Steps 6–9; Global Constraints |
| 159 flow 1 | Task 6 Steps 1–6 |
| 160 flow 2 | Task 6 Steps 7–8 |
| 161 flow 3 | Task 6 Step 9 |
| 162 flow 4 | Task 7 Steps 3–4 |
| 163 flow 5 | Task 9 Steps 2–12 |
| 164 flow 6 | Task 9 Step 10 |
| 165 flow 7 | Task 9 Step 11 |
| 166 flow 8 | Task 10 Steps 3–4 |
| 167 flow 9 | Task 10 Steps 5–6 |
| 168–179 Done when (L1379-1414) | Task 6 (export, files, manifest, flows 2–3); Task 7 (h80 gate, build/test/clippy/roundtrip/8 packages, docs); Task 9 (fixtures-2, flows 6–7, docs); Task 10 (CI green, cache hit, PR report of A8 and sizes) |
