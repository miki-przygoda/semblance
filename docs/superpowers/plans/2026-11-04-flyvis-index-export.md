# flyvis Index Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Export flyvis 1.2.0's own index values (FRI, DSI, preferred direction, tuning correlation, their peaks and its `*_to_known` helpers) for all 50 models at f32 and f64, with the input it fed, as `data/flyvis-indices/`, and give Rust a reader and a `Battery` over it; then stage `fixtures-3`.

**Architecture:** One offline script, `tools/export_indices.py`, beside `export_reference.py` and importing its generic helpers. It runs flyvis's response loop without flyvis's cache, calls flyvis's index functions on the result, and checks everything in memory (parameters, input, a bit-exact link to `fixtures-2`, windows, threads) before writing. Rust gets `flyvis::indices_ref` (reader, `IndexBattery`) with structure tests. The run happens on beast as a jobq job, into a staging directory.

**Tech Stack:** Python 3.12 under `uv` (`tools/`, flyvis 1.2.0, torch 2.14.0, numpy, xarray as locked in `tools/uv.lock`); Rust 1.85+, edition 2024, zero external dependencies.

**Spec:** `docs/superpowers/specs/2026-11-04-flyvis-index-export-design.md`

## Global Constraints

- Zero external Rust dependencies: `Cargo.lock` lists exactly 8 packages. `tools/` stays outside the workspace; no crate runs it.
- No new Python dependency: only what `tools/uv.lock` already pins.
- Commit subjects: plain imperative, no prefixes.
- `test_export_reference.py` must pass unchanged; `export_reference.py`'s behaviour must not change.
- **Never write into `/srv/bulk/Projects/semblance/data` or the Mac's `data/` from the script.** Output goes to `--out`, a staging directory.
- **Every cargo command runs on beast as a jobq job**: from a worktree under `~/work/semblance/`, `sh ~/work/semblance/gates/run.sh -- <cmd>`. Python jobs use `jobq submit` directly (no target pool), as Task 4 shows. Set `JOBQ_SUBMITTER=<your session>`. At most 4 agents building at once.
- Code reaches beast by `git push ssh://beast/srv/bulk/Projects/semblance/src HEAD:refs/heads/incoming-index-export`, then on beast `git -C /srv/bulk/Projects/semblance/src worktree add ~/work/semblance/index-export incoming-index-export` (later: push, then `git -C ~/work/semblance/index-export merge --ff-only incoming-index-export`).
- The `fixtures-3` release upload waits for the owner's sign-off.

## Facts found while planning (they refine the spec)

From flyvis 1.2.0's source in `tools/.venv/lib/python3.12/site-packages/flyvis/` (`F/` below), read 2026-11-04:

- **flyvis's response cache cannot tell f32 from f64.** `CheckpointedNetwork._hash_key` (`F/network/network_view.py:470-477`) and `generic_responses`' joblib key (`F/analysis/stimulus_responses.py:147-159`) ignore the dtype. The script never calls `NetworkView.flash_responses`/`moving_edge_responses`/`generic_responses`; it reimplements `compute_responses` (`stimulus_responses.py:34-108`) and the coordinates of `:204-254`, without a cache.
- **Batch size.** flyvis's stock index path uses `batch_size=4`; 0.3.0's reference and Rust's `respond` play one stimulus at a time. The export uses `batch_size=1` so its traces are the ones Rust reproduces, and the link check (Task 3, check 3) proves them equal to `fixtures-2`. It also runs model 000 at `batch_size=4` once per precision and records in `PROVENANCE.json` whether the indices are bit-identical (and the largest gap if not). That is a finding, not a failure.
- **Moving edges are NaN-padded.** `MovingEdge[i]` is always float64 and padded with NaN to 1078 frames; the valid lengths at 1/200 are 1078 / 739 / 567 / 525 / 485 / 465 for speeds 2.4 / 4.8 / 9.7 / 13 / 19 / 25. Responses after a stimulus's valid length are NaN, and flyvis's windows (ending 878 / 539 / 367 / 325 / 285 / 265) stay inside the valid frames. The export stores **only the valid frames** of each stimulus, so the Rust battery plays exactly those. (The spec's "144 stimuli of 1078 frames" is the padded length.)
- `Flashes[i]` is in the default dtype (f32 or f64); its values (0, 0.5, 1) are exact in both.
- `direction_selectivity_index` ends in `.squeeze()`, dropping `network_id` for one network (`F/analysis/moving_bar_responses.py:194`); `dsi_correlation_to_known` needs `network_id` back (`expand_dims`).
- `flash_response_index`'s shifted peaks and `correlation_to_known_tuning_curves`' per-speed r are local; the script replicates those few lines (`flash_responses.py:55-78`, `moving_bar_responses.py:442-480`) and asserts its replica's final values equal flyvis's function's output bit for bit, so the captured intermediates are provably flyvis's.
- `export_reference.py` imports safely (numpy only at top level; `main` is guarded). Generic helpers used here: `require`, `ExportError`, `parse_pretrained`, `read_ensemble`, `load_checkpoint`, `bits_equal`, `reference_params`, `check_checkpoints`, `copy_models`, `read_validation_losses`, `check_param_dtypes`, `sha256_file`, `json_bytes`, `_write_synced`, `_fsync_dir`, `git_state`, `to_np`, `max_abs_gap`, `acquire_lock`. The helpers hard-wired to `flyvis-reference` (`preflight`, `write_outputs`, the `SCRATCH`/`LOCK` constants, `run_one`, `thread_check`) are **not** used; this script has its own small equivalents, so `export_reference.py` is not modified at all. (The spec allowed parameterising them instead; not touching them is the smaller change.)

## Review Focus

1. A rerun with a populated `--out`: refuse unless `--force`, and never leave a half-written directory (write to a sibling temp dir, rename at the end). Pinned in Task 1.
2. A NaN anywhere inside a stimulus's valid frames, or a finite value after it: the export stops with the stimulus named. Pinned in Task 3 (`check_valid_frames`).
3. Two stimuli whose valid input frames are bit-identical rows: the frame dedup must share rows without merging the stimuli. Pinned in Task 1 (`dedup_frames` test).
4. A model directory whose `chkpt_00000` is not "best", or whose parameters drift from `ensemble.csv`: stop before any run. Covered by the imported `check_checkpoints` and a per-model `check_network_params` call (Task 3).
5. The Rust reader given a manifest whose array shape disagrees with its byte count: `ParityError::Malformed`, never a short read. Pinned in Task 5.

---

### Task 1: The script's pure parts, with self-tests

Everything that needs no flyvis: constants, the stimulus table, frame dedup, the manifest, writing. Tests in the existing style (plain asserts, no pytest).

**Files:**
- Create: `tools/export_indices.py`
- Create: `tools/test_export_indices.py`

**Interfaces:**
- Produces (used by Tasks 2–3 and by `test_export_indices.py`):
  - `FLASH_CFG: dict`, `EDGE_CFG: dict`, `SPEEDS`, `EDGE_WINDOW_END: dict[float, int]`, `EDGE_VALID_FRAMES: dict[float, int]`, `FRI_WINDOW = (200, 400)`, `MODELS: list[str]` (`"000"`..`"049"`), `PRECISIONS = ("f32", "f64")`.
  - `dedup_frames(stimuli: list[np.ndarray]) -> tuple[np.ndarray, list[np.ndarray]]`: distinct rows `[d, 721]` f64 in first-use order, and per stimulus a `uint32` index array.
  - `build_manifest(...) -> dict` and `validate_manifest(m: dict) -> None`.
  - `write_set(out: pathlib.Path, files: dict[str, bytes], manifest: dict, provenance: dict, force: bool) -> None`.

- [ ] **Step 1: Write the failing tests.** Create `tools/test_export_indices.py`:

```python
"""Self-tests for tools/export_indices.py. No flyvis import, no pytest.

Run by hand:  uv run --project tools tools/test_export_indices.py [test_name ...]
"""

import json
import pathlib
import sys
import tempfile
import traceback

import numpy as np

import export_indices as ei


def test_dedup_shares_identical_rows_without_merging_stimuli():
    grey = np.full(721, 0.5)
    on = np.ones(721)
    a = np.stack([grey, grey, on, grey])
    b = np.stack([on, on])
    frames, index = ei.dedup_frames([a, b])
    assert frames.shape == (2, 721) and frames.dtype == np.float64
    assert index[0].tolist() == [0, 0, 1, 0] and index[0].dtype == np.uint32
    assert index[1].tolist() == [1, 1]
    for x, ix in zip([a, b], index):
        assert np.array_equal(frames[ix], x)


def test_dedup_distinguishes_bitwise_not_numerically():
    pos, neg = np.zeros(721), np.zeros(721)
    neg[3] = -0.0
    frames, index = ei.dedup_frames([np.stack([pos, neg])])
    assert frames.shape[0] == 2 and index[0].tolist() == [0, 1]


def test_edge_tables_cover_every_speed():
    assert sorted(ei.EDGE_WINDOW_END) == sorted(ei.SPEEDS) == sorted(ei.EDGE_VALID_FRAMES)
    assert ei.EDGE_WINDOW_END == {2.4: 878, 4.8: 539, 9.7: 367, 13: 325, 19: 285, 25: 265}
    assert ei.EDGE_VALID_FRAMES == {2.4: 1078, 4.8: 739, 9.7: 567, 13: 525, 19: 485, 25: 465}
    for s in ei.SPEEDS:
        assert ei.EDGE_WINDOW_END[s] < ei.EDGE_VALID_FRAMES[s]


def manifest_example():
    flashes = [ei.flash_entry(k, 0.5, i, r, 600) for k, (i, r) in enumerate([(0, -1), (0, 6), (1, -1), (1, 6)])]
    edges = [ei.edge_entry(0, 0, 0, 2.4, 80, 1078)]
    arrays = [ei.array_entry("fri", "f32", "<f4", [50, 65])]
    return ei.build_manifest(
        cell_types=[f"T{i}" for i in range(65)],
        flashes=flashes,
        edges=edges,
        n_frames_rows=3,
        frame_index_blocks=[ei.index_block("flashes", k, 600 * k, 600) for k in range(4)]
        + [ei.index_block("edges", 0, 2400, 1078)],
        arrays=arrays,
        sha256={"frames.f64.bin": "0" * 64},
    )


def test_manifest_round_trips_and_validates():
    m = manifest_example()
    ei.validate_manifest(m)
    again = json.loads(ei.manifest_bytes(m))
    assert again == m
    assert m["batteries"]["flashes"][1]["window"] == [200, 400]
    assert m["batteries"]["edges"][0]["window"] == [200, 878]


def test_validate_manifest_refuses_a_shape_that_disagrees_with_its_bytes():
    m = manifest_example()
    m["arrays"][0]["bytes"] = 1
    try:
        ei.validate_manifest(m)
    except ei.ExportError as e:
        assert "fri" in str(e)
    else:
        raise AssertionError("a wrong byte count was accepted")


def test_write_set_refuses_a_populated_out_without_force():
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "set"
        out.mkdir()
        (out / "old").write_text("x")
        try:
            ei.write_set(out, {"a.bin": b"\x00"}, manifest_example(), {}, force=False)
        except ei.ExportError:
            pass
        else:
            raise AssertionError("overwrote a populated --out")
        assert (out / "old").exists() and not (out / "a.bin").exists()


def test_write_set_replaces_the_whole_set_with_force():
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "set"
        out.mkdir()
        (out / "old").write_text("x")
        ei.write_set(out, {"a.bin": b"\x00"}, manifest_example(), {"host": "h"}, force=True)
        assert sorted(p.name for p in out.iterdir()) == ["PROVENANCE.json", "a.bin", "manifest.json"]
        assert not list(pathlib.Path(d).glob("set.tmp*"))


def main(argv):
    names = argv[1:] or [n for n in globals() if n.startswith("test_")]
    failed = 0
    for n in names:
        try:
            globals()[n]()
            print(f"ok   {n}")
        except Exception:
            failed += 1
            print(f"FAIL {n}")
            traceback.print_exc()
    print(f"{len(names) - failed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
```

- [ ] **Step 2: Run to see them fail**

Run (beast, as a job): `~/.local/bin/jobq submit --project semblance --cores 1 --mem 1G --timeout 15m --cwd ~/work/semblance/index-export -- ~/.local/bin/uv run --project tools tools/test_export_indices.py`, then `~/.local/bin/jobq wait <id>; ~/.local/bin/jobq logs <id>`. (The first `uv run` also syncs the environment: allow it the 15 minutes.)
Expected: `ModuleNotFoundError: No module named 'export_indices'`.

- [ ] **Step 3: Write the pure parts.** Create `tools/export_indices.py`:

```python
"""Export flyvis's own index values for 0.4.0's gates A and B.

0.4.0 scores networks on the response indices flyvis was judged by. Gate A
holds Rust's index code to flyvis's own values, so this script runs flyvis
1.2.0's own index functions -- flash_response_index (radius 6), the
moving-edge peaks, direction_selectivity_index, preferred_direction,
the tuning correlations, and the three *_to_known helpers -- for all 50
models at float32 and float64, dt 1/200, and stores them with the input
flyvis fed. Before it writes anything it proves, in memory:

  - every model's parameters are the ones Rust holds (ensemble.csv);
  - the input flyvis fed each stimulus is the input stored here;
  - model 000's settled state and central traces on the stimuli shared
    with data/flyvis-reference are fixtures-2's, bit for bit (so this
    pipeline is the one 0.3.0's gates reproduce, and this machine's torch
    computes what the Mac's did);
  - the FRI window and each speed's edge window are the ones tabled here;
  - model 000's results do not depend on the thread count.

flyvis's own response cache is not used: its key ignores the dtype, so a
float64 call could return a cached float32 result. The response loop
below mirrors flyvis's compute_responses without it, one stimulus per
batch, as 0.3.0's reference was made.

Outputs, under --out (a staging directory; never data/ directly):
  frames.f64.bin, frame_index.u32.bin, the index arrays (*.f32.bin,
  *.f64.bin), manifest.json (deterministic) and PROVENANCE.json.

Spec: docs/superpowers/specs/2026-11-04-flyvis-index-export-design.md

Run by hand (on beast, as a jobq job):
  uv run --project tools tools/export_indices.py --out DIR [--force] [--threads N]
"""

import argparse
import datetime
import json
import os
import pathlib
import shutil
import sys
import time

import numpy as np

import export_reference as er
from export_reference import ExportError, require

ROOT = pathlib.Path(__file__).resolve().parent.parent
DATA = ROOT / "data"
RESULTS = DATA / "flyvis-pretrained" / "results" / "flow" / "0000"
ENSEMBLE_CSV = DATA / "flyvis-pretrained" / "ensemble.csv"
PRETRAINED_RS = ROOT / "crates" / "flyvis" / "src" / "pretrained.rs"
REFERENCE = DATA / "flyvis-reference"
UV_LOCK = ROOT / "tools" / "uv.lock"
SCRATCH = ROOT / "target" / "flyvis-indices-root"
LOCK = ROOT / "target" / "flyvis-indices.lock"

MODELS = [f"{i:03d}" for i in range(50)]
PRECISIONS = ("f32", "f64")
DT = 1 / 200
HEXALS = 721

# flyvis 1.2.0, analysis/stimulus_responses.py:270-277 and :300-310.
FLASH_CFG = {"dynamic_range": [0, 1], "t_stim": 1, "t_pre": 1.0, "dt": DT, "radius": (-1, 6), "alternations": (0, 1, 0)}
SPEEDS = (2.4, 4.8, 9.7, 13, 19, 25)
EDGE_CFG_BASE = {"offsets": (-10, 11), "intensities": [0, 1], "speeds": SPEEDS, "height": 80,
                 "post_pad_mode": "continue", "dt": DT, "t_pre": 1.0, "t_post": 1.0}

# Frames at dt 1/200, from flyvis's own time axis (task 0; re-asserted from
# flyvis's masks in check_windows).
FRI_WINDOW = (200, 400)
EDGE_WINDOW_END = {2.4: 878, 4.8: 539, 9.7: 367, 13: 325, 19: 285, 25: 265}
EDGE_VALID_FRAMES = {2.4: 1078, 4.8: 739, 9.7: 567, 13: 525, 19: 485, 25: 465}
FLASH_FRAMES = 600


# -- the stimulus table and the frames ------------------------------------------

def flash_entry(index, baseline, intensity, radius, n_frames):
    return {"index": index, "label": f"flash r{radius} i{intensity}", "baseline": baseline,
            "intensity": intensity, "radius": radius, "n_frames": n_frames, "window": list(FRI_WINDOW)}


def edge_entry(index, angle, intensity, speed, width, n_frames):
    return {"index": index, "label": f"edge a{angle} i{intensity} s{speed}", "angle": angle,
            "intensity": intensity, "speed": speed, "width": width, "n_frames": n_frames,
            "window": [FRI_WINDOW[0], EDGE_WINDOW_END[speed]]}


def dedup_frames(stimuli):
    """Distinct rows, compared bit for bit, in first-use order, and each
    stimulus's row index. Every stimulus is [n_frames, 721] float64."""
    rows, seen, index = [], {}, []
    for x in stimuli:
        x = np.ascontiguousarray(x, dtype=np.float64)
        require(x.ndim == 2 and x.shape[1] == HEXALS, "frames", f"shape {x.shape}")
        ix = np.empty(x.shape[0], dtype=np.uint32)
        for i, row in enumerate(x):
            key = row.tobytes()
            if key not in seen:
                seen[key] = len(rows)
                rows.append(row)
            ix[i] = seen[key]
        index.append(ix)
    return np.stack(rows), index


# -- the manifest -----------------------------------------------------------------

DTYPE_BYTES = {"<f4": 4, "<f8": 8, "<u4": 4}


def array_entry(name, precision, dtype, shape):
    n = int(np.prod(shape)) * DTYPE_BYTES[dtype]
    return {"name": name, "precision": precision, "file": f"{name}.{precision}.bin", "dtype": dtype,
            "shape": list(shape), "bytes": n}


def index_block(battery, stimulus, offset, count):
    return {"battery": battery, "stimulus": stimulus, "offset": offset, "count": count}


def build_manifest(cell_types, flashes, edges, n_frames_rows, frame_index_blocks, arrays, sha256):
    return {
        "format": 1,
        "flyvis_version": "1.2.0",
        "dt": DT,
        "t_pre": 1.0,
        "pre_stim_value": 0.5,
        "models": MODELS,
        "cell_types": list(cell_types),
        "speeds": list(SPEEDS),
        "batteries": {"flashes": flashes, "edges": edges},
        "frames": {"file": "frames.f64.bin", "dtype": "<f8", "rows": n_frames_rows, "hexals": HEXALS,
                   "bytes": n_frames_rows * HEXALS * 8},
        "frame_index": {"file": "frame_index.u32.bin", "dtype": "<u4", "blocks": frame_index_blocks},
        "arrays": arrays,
        "sha256": dict(sorted(sha256.items())),
    }


def manifest_bytes(m):
    return (json.dumps(m, indent=1, sort_keys=False) + "\n").encode()


def validate_manifest(m):
    require(m["format"] == 1, "manifest", f"format {m['format']}")
    for a in m["arrays"]:
        want = int(np.prod(a["shape"])) * DTYPE_BYTES[a["dtype"]]
        require(a["bytes"] == want, "manifest", f"array {a['name']}.{a['precision']}: {a['bytes']} bytes, shape gives {want}")
    for battery in ("flashes", "edges"):
        for k, s in enumerate(m["batteries"][battery]):
            require(s["index"] == k, "manifest", f"{battery}[{k}] has index {s['index']}")
            require(0 <= s["window"][0] <= s["window"][1] < s["n_frames"], "manifest", f"{battery}[{k}] window {s['window']}")
    offset = 0
    for b in m["frame_index"]["blocks"]:
        require(b["offset"] == offset, "manifest", f"frame_index block {b}: offset, expected {offset}")
        n = m["batteries"][b["battery"]][b["stimulus"]]["n_frames"]
        require(b["count"] == n, "manifest", f"frame_index block {b}: count, the stimulus has {n} frames")
        offset += b["count"]


# -- writing the set --------------------------------------------------------------

def write_set(out, files, manifest, provenance, force):
    """Writes every file into a sibling temp dir, then swaps it in for
    `out`, so `out` is never half-written."""
    out = pathlib.Path(out)
    if out.exists() and any(out.iterdir()) and not force:
        raise ExportError(f"--out {out} is not empty; pass --force to replace it")
    validate_manifest(manifest)
    tmp = out.with_name(out.name + ".tmp")
    if tmp.exists():
        shutil.rmtree(tmp)
    tmp.mkdir(parents=True)
    for name, data in files.items():
        er._write_synced(tmp / name, data)
    er._write_synced(tmp / "manifest.json", manifest_bytes(manifest))
    er._write_synced(tmp / "PROVENANCE.json", er.json_bytes(provenance))
    er._fsync_dir(tmp)
    if out.exists():
        shutil.rmtree(out)
    tmp.rename(out)
    er._fsync_dir(out.parent)
```

(`EDGE_CFG` adds `"device"` at run time, when flyvis is imported: `{**EDGE_CFG_BASE, "device": flyvis.device}`.)

- [ ] **Step 4: Run the tests**

Same job command as Step 2.
Expected: `7 passed, 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add tools/export_indices.py tools/test_export_indices.py
git commit -m "Add the pure parts of the flyvis index export, with self-tests"
```

---

### Task 2: The response loop and the index calls

**Files:**
- Modify: `tools/export_indices.py` (add the flyvis section)

**Interfaces:**
- Consumes: Task 1's constants.
- Produces: `responses(network, dataset, cfg) -> xr.Dataset` (dims `stimulus (sample, frame, channel, hex_pixel)`, `responses (network_id, sample, frame, neuron)`, coords as flyvis's `generic_responses`), and `indices(flash, edge) -> dict[str, np.ndarray]` with keys `fri_peaks [65,2]`, `fri [65]`, `edge_peaks [144,65]`, `dsi [2,65]`, `pd [2,65]`, `tuning_r [8,2,6]` (types in flyvis's `custom.where` order, returned beside the arrays), `fri_to_known []`, `dsi_to_known []`, `pd_to_known [8]`, in flyvis's computed dtypes.

- [ ] **Step 1: Add the flyvis section** to `export_indices.py` (all flyvis imports happen inside functions, after the environment is set in `main`, as in `export_reference.py`):

```python
# -- flyvis (imported only inside these functions) ---------------------------------

def responses(network, dataset, cfg):
    """flyvis's compute_responses (analysis/stimulus_responses.py:34-108) and
    generic_responses' coordinates (:204-254), without the cache, one
    stimulus per batch."""
    import xarray as xr
    ci = network.connectome.central_cells_index[:]
    stims, resps = [], []
    for stim, resp in network.stimulus_response(dataset, dataset.dt, t_pre=1.0, t_fade_in=0.0, batch_size=1):
        stims.append(stim)
        resps.append(np.take(resp, ci, axis=-1))
    s = np.concatenate(stims, 0)
    r = np.concatenate(resps, 0)[None]
    ds = xr.Dataset(
        {"stimulus": (["sample", "frame", "channel", "hex_pixel"], s),
         "responses": (["network_id", "sample", "frame", "neuron"], r)},
        coords={"sample": np.arange(len(dataset)),
                **{c: ("sample", dataset.arg_df[c].values) for c in dataset.arg_df.columns}},
    )
    nodes = network.connectome.nodes
    ds = ds.assign_coords(
        frame=np.arange(s.shape[1]),
        neuron=np.arange(r.shape[3]),
        time=("frame", np.arange(s.shape[1]).astype(float) * cfg["dt"] - cfg["t_pre"]),
        cell_type=("neuron", nodes.type[:].astype(str)[ci]),
        u=("neuron", nodes.u[:][ci]),
        v=("neuron", nodes.v[:][ci]),
        network_id=("network_id", [0]),
    )
    ds.attrs["config"] = cfg
    return ds


def fri_peaks(flash, radius=6):
    """flyvis's flash_response_index (analysis/flash_responses.py:50-78),
    transcribed line for line, stopping before the index so the shifted
    peaks are kept. `indices` proves the replica exact."""
    assert tuple(flash.attrs["config"]["alternations"]) == (0, 1, 0)
    responses = flash["responses"]
    time_query = f"{-flash.attrs['config']['dt']} <= time <= {flash.attrs['config']['t_stim']}"
    stim_response = responses.query(frame=time_query)
    stim_response = stim_response.query(sample=f"radius=={radius}")
    minimum = stim_response.min(dim=["frame", "sample"])
    stim_response += np.abs(minimum)
    r_on = stim_response.query(sample=f"intensity=={1.0}")
    r_off = stim_response.query(sample=f"intensity=={0.0}")
    on_peak = r_on.max(dim="frame").drop("sample")
    off_peak = r_off.max(dim="frame").drop("sample")
    return on_peak, off_peak


def fri_from_peaks(on_peak, off_peak):
    """flash_responses.py:81-82, in place as flyvis does it, so an f32 run
    stays f32."""
    fri = on_peak - off_peak
    fri /= on_peak + off_peak + np.array([1e-16])
    return fri


def tuning_per_speed(edge):
    """flyvis's correlation_to_known_tuning_curves
    (analysis/moving_bar_responses.py:455-473), transcribed line for line,
    stopping before the argmax over (speed, width): r per type, intensity and
    speed. `indices` proves its maximum equals flyvis's result."""
    from flyvis.analysis import moving_bar_responses as mb
    types = ["T4a", "T4b", "T4c", "T4d", "T5a", "T5b", "T5c", "T5d"]
    tuning = mb.peak_responses(edge)
    gt_tuning = mb.get_known_tuning_curves(types, np.arange(0, 360, 30))
    tuning = (
        tuning.set_index(sample=["angle", "intensity", "width", "speed"])
        .unstack("sample")
        .fillna(0.0)
        .custom.where(cell_type=types)
    )
    cell_types = [str(t) for t in tuning.cell_type.values]
    tuning["neuron"] = np.arange(tuning.coords["neuron"].size)
    correlation = xr_corr(tuning, gt_tuning)
    correlation = correlation.fillna(0.0)
    require(correlation.sizes["width"] == 1, "tuning", f"{correlation.sizes['width']} widths")
    return correlation.isel(width=0), cell_types


def xr_corr(a, b):
    import xarray as xr
    return xr.corr(a, b, dim="angle")
```

```python
def indices(flash, edge):
    from flyvis.analysis import flash_responses as fr
    from flyvis.analysis import moving_bar_responses as mb

    on, off = fri_peaks(flash)
    fri = fr.flash_response_index(flash, radius=6)
    replica = fri_from_peaks(on, off).dropna(dim="sample", how="any")
    require(er.bits_equal(np.asarray(replica).ravel(), np.asarray(fri).ravel()), "fri replica",
            "the replicated shifted peaks do not reproduce flash_response_index")

    peaks = mb.peak_responses(edge)                                   # (network_id, sample, neuron)
    dsi = mb.direction_selectivity_index(edge)                        # (neuron, intensity) after squeeze
    pd = mb.preferred_direction(edge)                                 # (network_id, neuron, intensity)
    best = mb.correlation_to_known_tuning_curves(edge)                # (network_id, neuron[8], intensity)
    per_speed, tuning_types = tuning_per_speed(edge)                  # (network_id, neuron[8], intensity, speed)
    require(er.bits_equal(np.asarray(per_speed.max("speed")).ravel(), np.asarray(best).ravel()),
            "tuning replica", "the per-speed r do not reproduce correlation_to_known_tuning_curves")

    return {
        "fri_peaks": np.stack([np.asarray(off).squeeze(), np.asarray(on).squeeze()], axis=-1),       # [65, 2] (off, on)
        "fri": np.asarray(fri).squeeze(),                                                           # [65]
        "edge_peaks": np.asarray(peaks).squeeze(0),                                                 # [144, 65]
        "dsi": np.asarray(dsi.transpose("intensity", "neuron")),                                    # [2, 65]
        "pd": np.asarray(pd.squeeze("network_id").transpose("intensity", "neuron")),                # [2, 65]
        "tuning_r": np.asarray(per_speed.squeeze("network_id").transpose("neuron", "intensity", "speed")),  # [8, 2, 6]
        "fri_to_known": np.asarray(fr.fri_correlation_to_known(fri)).squeeze(),                     # []
        "dsi_to_known": np.asarray(mb.dsi_correlation_to_known(dsi.expand_dims(network_id=[0]))).squeeze(),
        "pd_to_known": np.asarray(mb.angular_distance_to_known(pd)).squeeze(),                     # [8]
    }, tuning_types
```

`indices` returns the arrays and the tuning types' order (flyvis's `custom.where` order); `main` records that order once in the manifest as `"tuning_types"` and requires it to be the same for every model. `fri_peaks` uses `.query`, whose float formatting of `{1.0}`/`{0.0}` matches flyvis's `f'intensity=={on_intensity}'` with the defaults 1.0 and 0.0. The order of `speed` in `per_speed` is the unstacked coordinate's order: assert it equals `SPEEDS` in `main` and record it. `dsi`'s and `pd`'s `intensity` order: assert `[0, 1]`.

`pd_to_known`'s order is flyvis's (`angular_distance_to_known` concatenates T4a–d then T5a–d); record that order in the manifest as `"pd_to_known_types"`.

- [ ] **Step 2: Smoke-run on one model, both precisions, writing nothing.** Add a temporary `if __name__ == "__main__":` block that sets the environment and loads model 000 exactly as Task 3's `main` does (copy those lines), builds both datasets, calls `responses` and `indices`, and prints each array's shape and dtype and the wall time. Task 3 replaces the block with `main`.

Run (beast): `~/.local/bin/jobq submit --project semblance --cores 8 --mem 8G --timeout 1h --cwd ~/work/semblance/index-export -- ~/.local/bin/uv run --project tools tools/export_indices.py --smoke 000`
Expected: shapes `[65,2] [65] [144,65] [2,65] [2,65] [8,2,6] [] [] [8]` at both precisions; both replica `require`s pass; wall time per model printed (record it: Task 4's timeout comes from it).

- [ ] **Step 3: Commit**

```bash
git add tools/export_indices.py
git commit -m "Run flyvis's response loop and index functions without its cache"
```

---

### Task 3: The checks, `main`, and the provenance

**Files:**
- Modify: `tools/export_indices.py` (checks and `main`), `tools/test_export_indices.py` (tests for `check_valid_frames`)

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces: the complete script: `main(argv) -> int`, flags `--out DIR` (required), `--force`, `--threads N` (default: `os.cpu_count()` capped by the job's claim, passed explicitly), `--smoke MODEL` (runs one model, all checks, writes nothing).

- [ ] **Step 1: Write the failing test** for the valid-frame check (append to `test_export_indices.py`):

```python
def test_check_valid_frames_finds_nan_inside_and_values_after():
    r = np.zeros((2, 10, 3))
    r[:, 6:, :] = np.nan
    ei.check_valid_frames(r, [6, 6], "edges")            # exact: passes
    bad = r.copy(); bad[1, 2, 0] = np.nan
    for x, n in [(bad, [6, 6]), (r, [6, 7]), (r, [5, 6])]:
        try:
            ei.check_valid_frames(x, n, "edges")
        except ei.ExportError:
            continue
        raise AssertionError(f"accepted {n}")
```

- [ ] **Step 2: Implement the checks** in `export_indices.py`:

```python
def check_valid_frames(resp, valid, battery):
    """resp [sample, frame, neuron]: finite through each stimulus's valid
    frames and NaN after them (flyvis pads moving edges with NaN)."""
    for s, n in enumerate(valid):
        require(np.isfinite(resp[s, :n]).all(), "valid frames", f"{battery} stimulus {s}: non-finite before frame {n}")
        require(np.isnan(resp[s, n:]).all(), "valid frames", f"{battery} stimulus {s}: finite after frame {n}")


def check_windows(flash, edge):
    """FRI_WINDOW and EDGE_WINDOW_END, read from flyvis's own time axis and
    masks rather than trusted."""
    from flyvis.analysis import moving_bar_responses as mb
    t = flash.time.values
    frames = np.flatnonzero((t >= -DT) & (t <= flash.attrs["config"]["t_stim"]))
    require((frames[0], frames[-1]) == FRI_WINDOW, "windows", f"FRI frames {frames[0]}..={frames[-1]}")
    masks = mb.get_time_masks(edge, from_column=-22.5 / 5.8, to_column=22.5 / 5.8)  # peak_responses' defaults
    for s, speed in enumerate(edge.speed.values):
        on = np.flatnonzero(np.asarray(masks.isel(sample=s)))
        require((on[0], on[-1]) == (FRI_WINDOW[0], EDGE_WINDOW_END[float(speed)]), "windows",
                f"edge sample {s} (speed {speed}): frames {on[0]}..={on[-1]}")
```

(Before relying on `get_time_masks`' keyword names, open `moving_bar_responses.py:41-120` and use exactly the arguments `peak_responses` passes; the planning report gives `from_column=from_degree/5.8, to_column=to_degree/5.8` with degrees `±22.5`.)

```python
def link_to_fixtures2(network, flash, edge, precision):
    """Model 000's settled state and central traces equal fixtures-2's
    reference (runs 0 and 1: model 000, dt 1/200), bit for bit."""
    import torch
    m = json.loads((REFERENCE / "manifest.json").read_text())
    run = 0 if precision == "f32" else 1
    dtype, np_dtype = ("<f4", np.float32) if precision == "f32" else ("<f8", np.float64)
    settled = er.to_np(network.steady_state(t_pre=1.0, dt=DT, batch_size=1, value=0.5)).ravel()
    ref = read_block(m, f"settled.{precision}.bin", run=run, dt="1/200", sequence=None).astype(np_dtype)
    require(er.bits_equal(settled.astype(np_dtype), ref), "link", f"{precision}: settled state differs from fixtures-2")
    for q in m["sequences"]:
        fam, args = q["family"], q["args"]
        if fam == "flashes":
            ds, sel = flash, (flash.intensity == args["intensity"]) & (flash.radius == args["radius"])
        else:
            ds, sel = edge, (edge.angle == args["angle"]) & (edge.intensity == args["intensity"]) & (edge.speed == args["speed"])
        (s,) = np.flatnonzero(np.asarray(sel))
        ref = read_block(m, f"centre.{precision}.bin", run=run, dt="1/200", sequence=q["index"])
        got = ds["responses"].values[0, s, : ref.shape[0]].astype(np_dtype)
        require(er.bits_equal(got, ref.astype(np_dtype)), "link", f"{precision}: sequence {q['index']} ({fam}) differs from fixtures-2")
        inp = read_block(m, "input.f64.bin", run=None, dt="1/200", sequence=q["index"])
        require(er.bits_equal(ds["stimulus"].values[s, : inp.shape[0], 0], inp), "link",
                f"sequence {q['index']}: input differs from fixtures-2's input.f64.bin")
```

`read_block(m, name, run, dt, sequence)` reads one block of a `flyvis-reference` file by the manifest's `files[].blocks[]` entry with those three keys (offset, bytes, shape; dtype from the file entry) and returns it reshaped. The reference's centre column order is `manifest["centre_nodes"]`; assert once that its `cell_type`s equal `flash.cell_type.values` in order, before comparing.

```python
def thread_invariance(model_fn, threads):
    """Model 000's indices at 1 thread equal those at `threads`, bit for bit."""
    import torch
    a = model_fn()
    torch.set_num_threads(1)
    try:
        b = model_fn()
    finally:
        torch.set_num_threads(threads)
    for k in a:
        require(er.bits_equal(np.asarray(a[k]).ravel(), np.asarray(b[k]).ravel()), "threads", f"{k} depends on the thread count")


def batch4_finding(network, flash_ds, edge_ds, ours):
    """flyvis's stock batch_size=4 against ours (1): recorded, not required."""
    # same as `responses` with batch_size=4, then `indices`; returns
    # {key: max |difference|} (0.0 everywhere when bit-identical)
```

(Write `batch4_finding` in full: call a `responses(..., batch_size=4)` variant by giving `responses` a `batch_size=1` keyword, then `indices`, then `{k: er.max_abs_gap(ours[k], theirs[k]) for k in ours}`.)

`main` (replace the Task 2 smoke block):

```python
def main(argv):
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=pathlib.Path)
    p.add_argument("--force", action="store_true")
    p.add_argument("--threads", type=int, required=True)
    p.add_argument("--smoke")
    a = p.parse_args(argv[1:])
    require(a.smoke or a.out, "usage", "--out is required unless --smoke")
    require("FLYVIS_ROOT_DIR" not in os.environ, "environment", "an inherited FLYVIS_ROOT_DIR is refused")
    lock = er.acquire_lock(LOCK)  # held until exit
    if SCRATCH.exists():
        shutil.rmtree(SCRATCH)
    (SCRATCH / "results" / "flow" / "0000").mkdir(parents=True)
    ids = [a.smoke] if a.smoke else MODELS
    er.copy_models(RESULTS, SCRATCH / "results" / "flow" / "0000", ids)
    os.environ.update(FLYVIS_ROOT_DIR=str(SCRATCH), CUDA_VISIBLE_DEVICES="", MPLCONFIGDIR=str(SCRATCH / "mpl"))

    import torch, flyvis
    from flyvis import NetworkView
    from flyvis.datasets.flashes import Flashes
    from flyvis.datasets.moving_bar import MovingEdge
    torch.set_num_threads(a.threads)
    pretrained = er.parse_pretrained(PRETRAINED_RS.read_text())
    ensemble = er.read_ensemble(ENSEMBLE_CSV.read_text())
    ckpts = {m: er.load_checkpoint(SCRATCH / "results/flow/0000" / m / "chkpts/chkpt_00000") for m in ids}
    er.check_checkpoints(ckpts, ensemble, pretrained) if not a.smoke else None

    started = time.time()
    out, inputs, cell_types, tables, timing, gaps4, tuning_types = {}, None, None, None, {}, {}, None
    for precision in PRECISIONS:
        torch.set_default_dtype(torch.float32 if precision == "f32" else torch.float64)
        flash_ds = Flashes(**FLASH_CFG)
        edge_cfg = {**EDGE_CFG_BASE, "device": flyvis.device}
        edge_ds = MovingEdge(**edge_cfg)
        for m in ids:
            t0 = time.time()
            nv = NetworkView(f"flow/0000/{m}")
            require(nv.get_checkpoint("best").name == "chkpt_00000", "checkpoint", f"model {m}: best is not chkpt_00000")
            network = nv.init_network("best")
            if precision == "f64":
                network.double()
            er.check_param_dtypes(network, torch.get_default_dtype(), run=m)
            run_model = lambda: indices(responses(network, flash_ds, FLASH_CFG), responses(network, edge_ds, edge_cfg))[0]
            flash, edge = responses(network, flash_ds, FLASH_CFG), responses(network, edge_ds, edge_cfg)
            if m == "000":
                check_windows(flash, edge)
                link_to_fixtures2(network, flash, edge, precision)
                thread_invariance(run_model, a.threads)
                gaps4[precision] = batch4_finding(network, flash_ds, edge_ds, indices(flash, edge)[0])
            check_valid_frames(flash["responses"].values[0], [FLASH_FRAMES] * len(flash_ds), "flashes")
            valid = [EDGE_VALID_FRAMES[float(s)] for s in edge.speed.values]
            check_valid_frames(edge["responses"].values[0], valid, "edges")
            ix, types8 = indices(flash, edge)
            require(tuning_types in (None, types8), "tuning", f"model {m}: tuning types {types8}")
            tuning_types = types8
            for k, v in ix.items():
                out.setdefault((k, precision), []).append(v)
            stim = [flash["stimulus"].values[s, :, 0] for s in range(len(flash_ds))] + \
                   [edge["stimulus"].values[s, :n, 0] for s, n in enumerate(valid)]
            if inputs is None:
                inputs, cell_types = stim, list(flash.cell_type.values)
                tables = stimulus_tables(flash, edge, valid)
            else:
                for x, y in zip(inputs, stim):
                    require(er.bits_equal(x.astype(np.float64), y.astype(np.float64)), "input",
                            f"model {m} {precision}: the input differs from the first model's")
            timing[f"{m}.{precision}"] = round(time.time() - t0, 1)
            print(f"{m} {precision}: {timing[f'{m}.{precision}']} s", flush=True)
    if a.smoke:
        print("smoke: every check passed; nothing written")
        return 0

    frames, index = dedup_frames([np.asarray(x, dtype=np.float64) for x in inputs])
    for x, ix in zip(inputs, index):
        require(er.bits_equal(frames[ix], np.asarray(x, dtype=np.float64)), "input", "dedup does not rebuild the input")
    files, arrays, gaps = pack_arrays(out)
    files["frames.f64.bin"] = frames.astype("<f8").tobytes()
    files["frame_index.u32.bin"] = np.concatenate(index).astype("<u4").tobytes()
    blocks, offset = [], 0
    for battery, entries in (("flashes", tables[0]), ("edges", tables[1])):
        for e in entries:
            blocks.append(index_block(battery, e["index"], offset, e["n_frames"]))
            offset += e["n_frames"]
    sha = {name: hashlib_sha256(data) for name, data in files.items()}
    manifest = build_manifest(cell_types, tables[0], tables[1], frames.shape[0], blocks, arrays, sha)
    manifest["pd_to_known_types"] = ["T4a", "T4b", "T4c", "T4d", "T5a", "T5b", "T5c", "T5d"]
    manifest["tuning_types"] = tuning_types
    provenance = {
        "generated": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "host": os.uname().nodename, "machine": os.uname().machine,
        "flyvis": flyvis.__version__, "torch": torch.__version__, "numpy": np.__version__,
        "uv_lock_sha256": er.sha256_file(UV_LOCK), "git": er.git_state(),
        "threads": a.threads, "batch_size": 1, "seconds": round(time.time() - started, 1), "per_model_seconds": timing,
        "f32_f64_gap": gaps, "batch4_vs_batch1_max_gap": gaps4,
    }
    write_set(a.out, files, manifest, provenance, a.force)
    print(f"wrote {a.out}: {len(files)} files, {frames.shape[0]} distinct frames")
    return 0
```

with the helpers:

```python
def hashlib_sha256(data):
    import hashlib
    return hashlib.sha256(data).hexdigest()


def stimulus_tables(flash, edge, valid):
    fl = [flash_entry(s, float(flash.baseline.values[s]), int(flash.intensity.values[s]), int(flash.radius.values[s]), FLASH_FRAMES)
          for s in range(flash.sizes["sample"])]
    ed = [edge_entry(s, int(edge.angle.values[s]), int(edge.intensity.values[s]), float(edge.speed.values[s]),
                     int(edge.width.values[s]), valid[s]) for s in range(edge.sizes["sample"])]
    require(len(fl) == 4 and len(ed) == 144, "stimuli", f"{len(fl)} flashes, {len(ed)} edges")
    return fl, ed


def pack_arrays(out):
    """Stacks each index over the 50 models, in its computed dtype, and the
    f32-vs-f64 gap per index (over every model and entry)."""
    files, arrays, gaps = {}, [], {}
    names = sorted({k for k, _ in out})
    for name in names:
        for precision in PRECISIONS:
            x = np.stack(out[(name, precision)])
            dtype = {np.dtype("float32"): "<f4", np.dtype("float64"): "<f8"}[x.dtype]
            arrays.append(array_entry(name, precision, dtype, list(x.shape)))
            files[f"{name}.{precision}.bin"] = x.astype(dtype).tobytes()
        gaps[name] = er.max_abs_gap(np.stack(out[(name, "f32")]).astype(np.float64), np.stack(out[(name, "f64")]))
    return files, arrays, gaps


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except ExportError as e:
        print(f"export_indices: {e}", file=sys.stderr)
        sys.exit(1)
```

Check, before writing `main`'s signature uses: `er.check_param_dtypes`'s `run` argument is used only in messages (it takes an int in `export_reference.py`; pass the model id, or adapt the call to the helper's real signature at `export_reference.py:1019`). `er.copy_models` returns a dict of sha256s (`:924`): keep it in `provenance["models_sha256"]`. `er.max_abs_gap` must accept NaN-free arrays only; `pd` may hold `±π` wrap differences between precisions, which is a real gap and is recorded as is.

- [ ] **Step 3: Run the self-tests and a smoke run**

Self-tests (Task 1 job command): expected `8 passed, 0 failed`.
Smoke: `~/.local/bin/jobq submit --project semblance --cores 8 --mem 8G --timeout 1h --cwd ~/work/semblance/index-export -- ~/.local/bin/uv run --project tools tools/export_indices.py --smoke 000 --threads 8`
Expected: `smoke: every check passed; nothing written`.
**If the link check fails** (beast's torch differs from the Mac's): stop. Report the first differing sequence and precision to the owner; per the spec, the export is then rerun on the Mac and the failure recorded as a portability finding. Do not loosen the check.

- [ ] **Step 4: Commit**

```bash
git add tools/export_indices.py tools/test_export_indices.py
git commit -m "Check the index export against fixtures-2, the windows and the threads before writing"
```

---

### Task 4: Run the export on beast

**Files:** none in git.

- [ ] **Step 1: Check the machine:** `~/.local/bin/jobq machine`, `df -h /srv/fast /srv/bulk`, `cat /proc/loadavg`.

- [ ] **Step 2: Submit.** Timeout: 2 × 50 × the smoke run's per-model seconds × 1.5, rounded up to the hour (at most 8h).

```sh
cd ~/work/semblance/index-export
JOBQ_SUBMITTER=<session> ~/.local/bin/jobq submit --project semblance --name semblance-export-indices \
  --cores 8 --mem 8G --timeout <T> --cwd "$PWD" -- \
  ~/.local/bin/uv run --project tools tools/export_indices.py \
  --out /srv/bulk/Projects/semblance/staging/fixtures-3/flyvis-indices --threads 8
~/.local/bin/jobq wait <id>; ~/.local/bin/jobq logs <id> | tail -20
```

Expected: `wrote …/flyvis-indices: … files, … distinct frames`, exit 0.

- [ ] **Step 3: Validate the staged set independently:** recompute every sha256 against `manifest.json` and run `validate_manifest`:

`~/.local/bin/jobq submit --project semblance --cores 1 --mem 1G --timeout 10m --cwd ~/work/semblance/index-export -- ~/.local/bin/uv run --project tools python -c "import json,hashlib,pathlib,export_indices as ei; d=pathlib.Path('/srv/bulk/Projects/semblance/staging/fixtures-3/flyvis-indices'); m=json.loads((d/'manifest.json').read_text()); ei.validate_manifest(m); bad=[n for n,h in m['sha256'].items() if hashlib.sha256((d/n).read_bytes()).hexdigest()!=h]; print('bad', bad); assert not bad"`

(run with `--cwd` set so `tools/` is on the path: prefix the command with `env PYTHONPATH=tools`.)
Expected: `bad []`.

- [ ] **Step 4: Read `PROVENANCE.json`** and note for the owner: total time, the f32/f64 gaps per index, and `batch4_vs_batch1_max_gap`.

---

### Task 5: The Rust reader and `IndexBattery`

Needs task 1a's `Battery` (plan `2026-11-04-graded-responses.md`, Task 4). Branch this task from the 1a branch once it is merged, or after it.

**Files:**
- Create: `crates/flyvis/src/indices_ref.rs`
- Modify: `crates/flyvis/src/lib.rs` (`pub mod indices_ref;`), `crates/flyvis/src/parity/inputs.rs` (`required_files`, `how_to_get`)

**Interfaces:**
- Consumes: `crate::json` (the crate's JSON reader, as `reference.rs` uses), `parity::{Battery, ParityError}`.
- Produces:
  - `pub struct IndexFixture` with `pub fn load(data: &Path) -> Result<IndexFixture, ParityError>`, `pub fn from_bytes(manifest_json: &str, files: Vec<(String, Vec<u8>)>) -> Result<IndexFixture, ParityError>`;
  - `pub fn cell_types(&self) -> &[String]`, `pub fn models(&self) -> &[String]`, `pub fn flashes(&self) -> &[StimulusEntry]`, `pub fn edges(&self) -> &[StimulusEntry]`;
  - `pub fn array(&self, name: &str, precision: Dtype) -> Result<IndexArray, ParityError>` (`IndexArray { shape: Vec<usize>, data: Vec<f64> }`, widened);
  - `pub enum BatteryKind { Flashes, Edges }`, `pub fn battery(&self, kind: BatteryKind) -> IndexBattery<'_>`;
  - `pub struct StimulusEntry { pub index: usize, pub label: String, pub n_frames: usize, pub window: (usize, usize), pub intensity: u8, pub radius: Option<i32>, pub angle: Option<u16>, pub speed: Option<f64>, pub width: Option<u16> }`.

- [ ] **Step 1: Write the failing tests** at the bottom of `indices_ref.rs`, starting from a small hand-built manifest (no fixtures) for the error paths, then fixture-gated ones:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text};

    /// One flash of 2 frames, one edge of 3, two distinct frames, one f32
    /// array of shape [50, 65].
    fn tiny() -> (String, Vec<(String, Vec<u8>)>) {
        let manifest = r#"{"format":1,"flyvis_version":"1.2.0","dt":0.005,"t_pre":1.0,"pre_stim_value":0.5,
          "models":["000"],"cell_types":["R1","T4a"],"speeds":[2.4],
          "batteries":{"flashes":[{"index":0,"label":"flash r6 i1","baseline":0.5,"intensity":1,"radius":6,"n_frames":2,"window":[0,1]}],
                       "edges":[{"index":0,"label":"edge a0 i0 s2.4","angle":0,"intensity":0,"speed":2.4,"width":80,"n_frames":3,"window":[0,2]}]},
          "frames":{"file":"frames.f64.bin","dtype":"<f8","rows":2,"hexals":721,"bytes":11536},
          "frame_index":{"file":"frame_index.u32.bin","dtype":"<u4","blocks":[
             {"battery":"flashes","stimulus":0,"offset":0,"count":2},{"battery":"edges","stimulus":0,"offset":2,"count":3}]},
          "arrays":[{"name":"fri","precision":"f32","file":"fri.f32.bin","dtype":"<f4","shape":[1,2],"bytes":8}],
          "sha256":{}}"#;
        let mut frames = vec![0.5f64; 721];
        frames.extend(vec![1.0f64; 721]);
        let frames: Vec<u8> = frames.iter().flat_map(|v| v.to_le_bytes()).collect();
        let index: Vec<u8> = [0u32, 1, 0, 0, 1].iter().flat_map(|v| v.to_le_bytes()).collect();
        let fri: Vec<u8> = [0.25f32, -0.5].iter().flat_map(|v| v.to_le_bytes()).collect();
        (manifest.to_string(), vec![
            ("frames.f64.bin".into(), frames),
            ("frame_index.u32.bin".into(), index),
            ("fri.f32.bin".into(), fri),
        ])
    }

    #[test]
    fn a_tiny_fixture_reads_back() {
        let (m, files) = tiny();
        let x = IndexFixture::from_bytes(&m, files).unwrap();
        assert_eq!(x.cell_types(), ["R1", "T4a"]);
        assert_eq!(x.array("fri", Dtype::F32).unwrap(), IndexArray { shape: vec![1, 2], data: vec![0.25, -0.5] });
        let mut b = x.battery(BatteryKind::Edges);
        assert_eq!(b.len(), 1);
        let mut out = Vec::new();
        assert_eq!(b.next_stimulus(&mut out).unwrap().as_deref(), Some("edge a0 i0 s2.4"));
        assert_eq!(out.len(), 3 * 721);
        assert_eq!((out[0], out[721], out[2 * 721]), (0.5, 0.5, 1.0));
        assert_eq!(b.next_stimulus(&mut out).unwrap(), None);
    }

    #[test]
    fn a_shape_that_disagrees_with_its_bytes_is_malformed() {
        let (m, files) = tiny();
        let m = m.replace(r#""shape":[1,2],"bytes":8"#, r#""shape":[1,3],"bytes":8"#);
        assert!(matches!(IndexFixture::from_bytes(&m, files), Err(ParityError::Malformed { .. })));
    }

    #[test]
    fn a_frame_index_past_the_last_row_is_malformed() {
        let (m, mut files) = tiny();
        files[1].1 = [0u32, 1, 0, 0, 2].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert!(matches!(IndexFixture::from_bytes(&m, files), Err(ParityError::Malformed { .. })));
    }

    #[test]
    fn a_missing_array_is_a_mismatch() {
        let (m, files) = tiny();
        let x = IndexFixture::from_bytes(&m, files).unwrap();
        assert!(matches!(x.array("dsi", Dtype::F32), Err(ParityError::Mismatch(_))));
    }

    fn real() -> Option<IndexFixture> {
        or_skip(IndexFixture::load(&crate::test_support::data_dir()).map_err(|e| match e {
            ParityError::Missing { .. } => e.to_string(),
            other => panic!("{other}"),
        }))
    }

    #[test]
    fn the_real_fixture_has_the_tabled_batteries() {
        let Some(x) = real() else { return };
        assert_eq!((x.flashes().len(), x.edges().len(), x.models().len(), x.cell_types().len()), (4, 144, 50, 65));
        for e in x.flashes() {
            assert_eq!((e.n_frames, e.window), (600, (200, 400)));
        }
        let ends = [(2.4, 878, 1078), (4.8, 539, 739), (9.7, 367, 567), (13.0, 325, 525), (19.0, 285, 485), (25.0, 265, 465)];
        for e in x.edges() {
            let &(_, end, n) = ends.iter().find(|(s, _, _)| Some(*s) == e.speed).expect("a tabled speed");
            assert_eq!((e.n_frames, e.window), (n, (200, end)), "{}", e.label);
        }
        for (name, shape) in [("fri", vec![50, 65]), ("edge_peaks", vec![50, 144, 65]), ("tuning_r", vec![50, 8, 2, 6])] {
            for p in [Dtype::F32, Dtype::F64] {
                assert_eq!(x.array(name, p).unwrap().shape, shape, "{name}");
            }
        }
    }

    /// The flashes and the speed-13 edges the reference holds expand to
    /// fixtures-2's input.f64.bin, frame for frame.
    #[test]
    fn the_shared_stimuli_expand_to_fixtures_2s_input() {
        let Some(x) = real() else { return };
        let Some(text) = or_skip(read_fixture_text("flyvis-reference/manifest.json")) else { return };
        let Some(input) = or_skip(read_fixture_bytes("flyvis-reference/input.f64.bin")) else { return };
        let manifest = crate::reference::Manifest::parse(&text).unwrap();
        let sequences = crate::reference::parse_sequences(&text).unwrap();
        let file = manifest.file("input.f64.bin").unwrap();
        for q in &sequences {
            let want = file.read_f64(&input, &crate::reference::BlockKey { run: None, dt: "1/200", sequence: Some(q.index) }).unwrap();
            let (kind, entries) = if q.family == "flashes" { (BatteryKind::Flashes, x.flashes()) } else { (BatteryKind::Edges, x.edges()) };
            let s = entries
                .iter()
                .position(|e| {
                    Some(e.intensity as f64) == q.arg("intensity")
                        && e.radius.map(f64::from) == q.arg("radius").or(e.radius.map(f64::from)).filter(|_| q.family == "flashes")
                        && (q.family == "flashes" || (e.angle.map(f64::from) == q.arg("angle") && e.speed == q.arg("speed")))
                })
                .expect("the reference stimulus is in the battery");
            let got = x.expand(kind, s).unwrap();
            assert_eq!(&got[..], &want.data[..], "sequence {}", q.index);
        }
    }
}
```

(Simplify the matching closure when implementing: match flashes on `(intensity, radius)` and edges on `(angle, intensity, speed)` in two explicit branches; the closure above is dense on purpose to fit, and should not be kept as is.)

- [ ] **Step 2: Run to see them fail**

`sh ~/work/semblance/gates/run.sh -- cargo test -p semblance-flyvis indices_ref`
Expected: compile errors.

- [ ] **Step 3: Implement `indices_ref.rs`.** Structure:

```rust
//! flyvis's own index values and the input it fed, as exported by
//! `tools/export_indices.py` into `data/flyvis-indices/`: the reference
//! gate A holds Rust's index code to.
//!
//! Spec: docs/superpowers/specs/2026-11-04-flyvis-index-export-design.md,
//! section 5.

use std::path::Path;

use crate::json::{self, Value};
use crate::parity::{Battery, ParityError};
use crate::reference::Dtype;

const DIR: &str = "flyvis-indices";

pub struct IndexFixture {
    cell_types: Vec<String>,
    models: Vec<String>,
    flashes: Vec<StimulusEntry>,
    edges: Vec<StimulusEntry>,
    frames: Vec<f64>,                 // rows × 721
    index: Vec<u32>,                  // every frame of every stimulus, flashes then edges
    starts: Vec<usize>,               // per stimulus (flashes then edges): offset into `index`
    arrays: Vec<(String, Dtype, Vec<usize>, Vec<u8>)>,
}
```

- `load(data)`: reads `flyvis-indices/manifest.json`, then each file the manifest names, through `parity::inputs::read_bytes`/`read_text` (make those two `pub(crate)` already; they are) so a missing file is `ParityError::Missing` with `how_to_get`'s remedy; then `from_bytes`.
- `from_bytes`: parses with `crate::json` the way `reference.rs::Manifest::parse` does (look at its `number`, `array`, `count` helpers and reuse them if `pub(crate)`, else add `pub(crate)` to them); checks `format == 1`; every array's `bytes == product(shape) × dtype size` and equals its file's length; `frames` length `== rows × 721 × 8`; `frame_index` blocks contiguous from 0, counts equal each stimulus's `n_frames`, total equal the file's length / 4, every index `< rows`; every window inside its stimulus. Any failure: `ParityError::Malformed { rel: format!("{DIR}/{file}"), detail }`.
- `array(name, precision)`: finds `(name, precision)`; absent is `ParityError::Mismatch(format!("{DIR}: no array {name}.{suffix}"))`; decodes `<f4`/`<f8` little-endian, widened to f64.
- `expand(kind, s) -> Result<Vec<f64>, ParityError>`: the stimulus's frames, `n_frames × 721`, from `frames` through `index`.
- `IndexBattery<'a> { x: &'a IndexFixture, kind: BatteryKind, next: usize }`, `impl Battery`: `len` is the battery's stimulus count; `next_stimulus` writes `expand(kind, next)` into `out` and returns the entry's `label`.

Register in `lib.rs`: `pub mod indices_ref;` and `pub use indices_ref::{BatteryKind, IndexArray, IndexBattery, IndexFixture, StimulusEntry};`.

In `parity/inputs.rs`, `how_to_get`: map any `flyvis-indices/…` rel to `"Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_indices.py --out DIR` and copy DIR into data/flyvis-indices/."`. Do **not** add the files to `required_files()`: that list is what `validate` needs today, and gate A (task 3) adds them when it starts reading them.

- [ ] **Step 4: Run the tests** (the staged fixture is not in `data/` yet; the fixture-gated ones skip):

`sh ~/work/semblance/gates/run.sh -- cargo test -p semblance-flyvis indices_ref -- --nocapture`
Expected: the four tiny-fixture tests pass; the two real-fixture tests print `skipping: …flyvis-indices/manifest.json not found…`.

Then run them against the staged set without touching `data/`: in the worktree, `rm data && mkdir data && for d in /srv/bulk/Projects/semblance/data/*; do ln -s "$d" data/; done && ln -s /srv/bulk/Projects/semblance/staging/fixtures-3/flyvis-indices data/flyvis-indices` (a private `data/` of links), then
`sh ~/work/semblance/gates/run.sh -- env SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis indices_ref`
Expected: all 6 pass. Restore afterwards: `rm -r data && ln -s /srv/bulk/Projects/semblance/data data`.

- [ ] **Step 5: Clippy and gate**

`sh ~/work/semblance/gates/run.sh -- cargo clippy --workspace --all-targets -- -D warnings`; `sh ~/work/semblance/gates/gate-quick.sh`.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/indices_ref.rs crates/flyvis/src/lib.rs crates/flyvis/src/parity/inputs.rs crates/flyvis/src/reference.rs
git commit -m "Read flyvis's exported index values, with a battery over their input"
```

---

### Task 6: Stage `fixtures-3`

**Files:**
- Modify: `ci/fixtures-tag`, `ci/fixtures.sha256`, `CLAUDE.md`, `README.md`, `tools/README.md`

- [ ] **Step 1: Copy the staged set to the Mac** (beast has no GitHub; the release is made from the Mac):
`scp -r beast:/srv/bulk/Projects/semblance/staging/fixtures-3/flyvis-indices data/flyvis-indices`
then verify: `cd data/flyvis-indices && python3 -c "import json,hashlib; m=json.load(open('manifest.json')); print([n for n,h in m['sha256'].items() if hashlib.sha256(open(n,'rb').read()).hexdigest()!=h])"`
Expected: `[]`.

- [ ] **Step 2: Update the fixture list.** `ci/fixtures-tag`: `fixtures-3`. Append to `ci/fixtures.sha256` one line per new file, `sha256  flyvis-indices/<name>`, including `manifest.json` and `PROVENANCE.json` (compute with `shasum -a 256`). Keep the existing 35 lines unchanged.

- [ ] **Step 3: Docs.** `tools/README.md`'s script table gains:

```markdown
| `export_indices.py` | flyvis's own index values for 0.4.0's gate A -- FRI, DSI, preferred direction, tuning correlation, their peaks and the `*_to_known` helpers -- for all 50 models in float32 and float64, with the input flyvis fed (distinct frames plus a per-stimulus index). Every check runs before anything is written | `data/flyvis-indices/` (via a staging `--out`) |
```

`CLAUDE.md` "Verifying a change": add `data/flyvis-indices/` to the gitignored real-data list with its source (`tools/export_indices.py`), and update the skip and test counts (measured as in plan `2026-11-04-graded-responses.md` Task 5, Step 1). `README.md`: the same skip count.

- [ ] **Step 4: Commit**

```bash
git add ci/fixtures-tag ci/fixtures.sha256 CLAUDE.md README.md tools/README.md
git commit -m "Register fixtures-3: flyvis's exported index values"
```

- [ ] **Step 5: Stop for the owner.** The release itself (`gh release create fixtures-3 …` with every file under `data/` that `ci/fixtures.sha256` lists) waits for the owner's sign-off. Report: the file list and sizes, `PROVENANCE.json`'s gaps, timing and batch-4 finding, and whether the link check passed on beast.

## Self-review notes

- Spec §1 (script, reuse): Tasks 1–3 (reuse is by import only; nothing in `export_reference.py` changes, noted above). §2 checks 1–6: `check_checkpoints` + `check_param_dtypes` (1), dedup rebuild + cross-model input equality + `link_to_fixtures2`'s input check (2), `link_to_fixtures2` (3), `check_windows` (4), `thread_invariance` (5), `check_valid_frames`, `stimulus_tables`, `validate_manifest` (6). §3 output: Tasks 1, 3. §4 beast: Task 4. §5 Rust: Task 5. §6 release: Task 6.
- Refinements to the spec, each stated in "Facts found while planning": no cache; `batch_size=1` with the batch-4 finding recorded; valid frames only; no edits to `export_reference.py`.
- Two transcriptions (`fri_peaks`, `tuning_per_speed`) are deliberately left to be copied from flyvis's source rather than paraphrased here; each is guarded by a `require` that its result reproduces flyvis's function bit for bit.
