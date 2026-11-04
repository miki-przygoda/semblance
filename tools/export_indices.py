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
# EDGE_CFG adds "device" at run time, when flyvis is imported:
# {**EDGE_CFG_BASE, "device": flyvis.device}. Until then it is the base.
EDGE_CFG = EDGE_CFG_BASE

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
