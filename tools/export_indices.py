"""Export flyvis's own index values for 0.4.0's gates A and B.

0.4.0 scores networks on the response indices flyvis was judged by. Gate A
holds Rust's index code to flyvis's own values, so this script runs flyvis
1.2.0's own index functions -- flash_response_index (radius 6), the
moving-edge peaks, direction_selectivity_index, preferred_direction,
the tuning correlations, and the three *_to_known helpers -- for all 50
models at float32 and for models 000 and 049 at float64, dt 1/200, and
stores them with the input flyvis fed. Before it writes anything it proves:

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

It runs in shards (the plan's amendment of 2026-11-04): each shard computes
some models at some precisions and writes its raw results to --shard-out;
--merge then checks the shards against each other and writes the set.
Model 000's once-per-export checks run in whichever shard computes model 000
at a given precision.

Outputs of --merge, under --out (a staging directory; never data/ directly):
  frames.f64.bin, frame_index.u32.bin, the index arrays (*.f32.bin with a
  leading axis of 50 models, *.f64.bin with one of 2: 000 and 049),
  manifest.json (deterministic) and PROVENANCE.json.

Spec: docs/superpowers/specs/2026-11-04-flyvis-index-export-design.md

Run by hand (on beast, as jobq jobs):
  uv run --project tools tools/export_indices.py --threads N [--models IDS] [--precisions P] --shard-out DIR
  uv run --project tools tools/export_indices.py --threads N --smoke MODEL [--precisions P]   (writes nothing)
  uv run --project tools tools/export_indices.py --merge SHARD_DIR [SHARD_DIR ...] --out DIR [--force]
"""

import argparse
import datetime
import hashlib
import io
import json
import os
import pathlib
import re
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
# Each shard gets its own, suffixed with "-<shard name>": parallel shards never
# share or wipe each other's flyvis root. See scratch_root and lock_path.
SCRATCH = ROOT / "target" / "flyvis-indices-root"
LOCK = ROOT / "target" / "flyvis-indices.lock"
# Never written to, whichever way --out or --shard-out is spelled.
FORBIDDEN_OUT = (DATA, pathlib.Path("/srv/bulk/Projects/semblance/data"))

MODELS = [f"{i:03d}" for i in range(50)]
F64_MODELS = ["000", "049"]
EXPORT_MODELS = {"f32": MODELS, "f64": F64_MODELS}
PRECISIONS = ("f32", "f64")
DT = 1 / 200
HEXALS = 721

# flyvis 1.2.0, analysis/stimulus_responses.py:270-277 and :300-310.
FLASH_CFG = {"dynamic_range": [0, 1], "t_stim": 1, "t_pre": 1.0, "dt": DT, "radius": (-1, 6), "alternations": (0, 1, 0)}
SPEEDS = (2.4, 4.8, 9.7, 13, 19, 25)
EDGE_CFG_BASE = {"offsets": (-10, 11), "intensities": [0, 1], "speeds": SPEEDS, "height": 80,
                 "post_pad_mode": "continue", "dt": DT, "t_pre": 1.0, "t_post": 1.0}
# EDGE_CFG adds "device" at run time, when flyvis is imported:
# {**EDGE_CFG_BASE, "device": flyvis.device}. Until then it is a copy of the base.
EDGE_CFG = dict(EDGE_CFG_BASE)
PD_TO_KNOWN_TYPES = ["T4a", "T4b", "T4c", "T4d", "T5a", "T5b", "T5c", "T5d"]

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


def build_manifest(cell_types, flashes, edges, n_frames_rows, frame_index_blocks, arrays, sha256,
                   models=None, tuning_types=None):
    return {
        "format": 1,
        "flyvis_version": "1.2.0",
        "dt": DT,
        "t_pre": 1.0,
        "pre_stim_value": 0.5,
        "models": {p: list(ids) for p, ids in (models or EXPORT_MODELS).items()},
        "cell_types": list(cell_types),
        "speeds": list(SPEEDS),
        "tuning_types": list(tuning_types or PD_TO_KNOWN_TYPES),
        "pd_to_known_types": list(PD_TO_KNOWN_TYPES),
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
    require(sorted(m["models"]) == sorted(PRECISIONS), "manifest", f"models keyed by {sorted(m['models'])}")
    for a in m["arrays"]:
        want = int(np.prod(a["shape"])) * DTYPE_BYTES[a["dtype"]]
        require(a["bytes"] == want, "manifest", f"array {a['name']}.{a['precision']}: {a['bytes']} bytes, shape gives {want}")
        require(a["file"] == f"{a['name']}.{a['precision']}.bin", "manifest", f"array {a['name']}.{a['precision']}: file {a['file']}")
        n = len(m["models"][a["precision"]])
        require(a["shape"][:1] == [n], "manifest",
                f"array {a['name']}.{a['precision']}: leading axis {a['shape'][:1]}, {n} models at {a['precision']}")
    names = {p: sorted(a["name"] for a in m["arrays"] if a["precision"] == p) for p in PRECISIONS}
    require(names["f32"] == names["f64"], "manifest", f"arrays differ between precisions: {names}")
    require(len({(a["name"], a["precision"]) for a in m["arrays"]}) == len(m["arrays"]), "manifest", "an array is listed twice")
    for battery in ("flashes", "edges"):
        for k, s in enumerate(m["batteries"][battery]):
            require(s["index"] == k, "manifest", f"{battery}[{k}] has index {s['index']}")
            require(0 <= s["window"][0] <= s["window"][1] < s["n_frames"], "manifest", f"{battery}[{k}] window {s['window']}")
    # frame_index: one block per stimulus, in battery order, contiguous.
    want_order = [(battery, k) for battery in ("flashes", "edges") for k in range(len(m["batteries"][battery]))]
    got_order = [(b["battery"], b["stimulus"]) for b in m["frame_index"]["blocks"]]
    require(got_order == want_order, "manifest", "frame_index blocks do not cover every stimulus once, in order")
    offset = 0
    for b in m["frame_index"]["blocks"]:
        require(b["offset"] == offset, "manifest", f"frame_index block {b}: offset, expected {offset}")
        n = m["batteries"][b["battery"]][b["stimulus"]]["n_frames"]
        require(b["count"] == n, "manifest", f"frame_index block {b}: count, the stimulus has {n} frames")
        offset += b["count"]
    # sha256: exactly the written data files, each a sha256.
    files = {m["frames"]["file"], m["frame_index"]["file"]} | {a["file"] for a in m["arrays"]}
    require(set(m["sha256"]) == files, "manifest",
            f"sha256 covers {sorted(set(m['sha256']) ^ files)} wrongly (missing or extra)")
    for name, h in m["sha256"].items():
        require(isinstance(h, str) and re.fullmatch(r"[0-9a-f]{64}", h) is not None, "manifest", f"sha256 of {name}: {h!r}")


def validate_files(m, files):
    """The data files against their manifest: the same set as the sha256 map,
    each with its hash and length, and every frame index inside frames."""
    require(set(files) == set(m["sha256"]), "files", f"files {sorted(files)} vs sha256 {sorted(m['sha256'])}")
    for name, data in files.items():
        require(hashlib_sha256(data) == m["sha256"][name], "files", f"{name}: sha256 differs from the manifest")
    require(len(files[m["frames"]["file"]]) == m["frames"]["bytes"], "files", "frames: length differs from the manifest")
    for a in m["arrays"]:
        require(len(files[a["file"]]) == a["bytes"], "files", f"{a['file']}: length differs from the manifest")
    total = sum(b["count"] for b in m["frame_index"]["blocks"])
    ix = np.frombuffer(files[m["frame_index"]["file"]], dtype="<u4")
    require(ix.size == total, "files", f"frame_index holds {ix.size} entries, its blocks {total}")
    require(ix.size == 0 or int(ix.max()) < m["frames"]["rows"], "files",
            f"frame_index reaches row {int(ix.max()) if ix.size else None}; frames has {m['frames']['rows']}")


# -- writing the set --------------------------------------------------------------

def write_set(out, files, manifest, provenance, force):
    """Writes every file into a sibling temp dir, then swaps it in for
    `out`, so `out` is never half-written."""
    out = pathlib.Path(out)
    refuse_data_dir(out)
    if out.exists() and any(out.iterdir()) and not force:
        raise ExportError(f"--out {out} is not empty; pass --force to replace it")
    validate_manifest(manifest)
    validate_files(manifest, files)
    tmp = out.with_name(out.name + ".tmp")
    if tmp.exists():
        print(f"removing a stale {tmp} left by an earlier run", flush=True)
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


def refuse_data_dir(path):
    """No output of this script lands under data/: the set is staged and
    published by hand."""
    real = pathlib.Path(path).resolve()
    for d in FORBIDDEN_OUT:
        d = d.resolve()
        require(real != d and d not in real.parents, "output", f"{path} is inside {d}; write to a staging directory")


def hashlib_sha256(data):
    return hashlib.sha256(data).hexdigest()


# -- flyvis (imported only inside these functions) ---------------------------------

def responses(network, dataset, cfg, batch_size=1):
    """flyvis's compute_responses (analysis/stimulus_responses.py:34-108) and
    generic_responses' coordinates (:204-254), without the cache, one
    stimulus per batch (batch_size=4 only for the batch-4 finding)."""
    import xarray as xr
    ci = network.connectome.central_cells_index[:]
    stims, resps = [], []
    for stim, resp in network.stimulus_response(dataset, dataset.dt, t_pre=1.0, t_fade_in=0.0, batch_size=batch_size):
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
    types = nodes.type[:].astype(str)
    u_all, v_all = nodes.u[:], nodes.v[:]
    u_in, v_in = np.array([(u_all[i], v_all[i]) for i, t in enumerate(types) if t == "R1"]).T
    ds = ds.assign_coords(
        frame=np.arange(s.shape[1]),
        channel=np.arange(s.shape[2]),
        hex_pixel=np.arange(s.shape[3]),
        neuron=np.arange(r.shape[3]),
        time=("frame", np.arange(s.shape[1]).astype(float) * cfg["dt"] - cfg["t_pre"]),
        cell_type=("neuron", types[ci]),
        u=("neuron", u_all[ci]),
        v=("neuron", v_all[ci]),
        u_in=("hex_pixel", u_in),
        v_in=("hex_pixel", v_in),
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


def indices(flash, edge):
    """Every index array for one model at one precision, in flyvis's
    computed dtypes, and the tuning types' order (flyvis's custom.where)."""
    from flyvis.analysis import flash_responses as fr
    from flyvis.analysis import moving_bar_responses as mb

    on, off = fri_peaks(flash)
    fri = fr.flash_response_index(flash, radius=6)
    replica = fri_from_peaks(on, off).dropna(dim="sample", how="any")
    require(replica.dims == fri.dims, "fri replica", f"dims {replica.dims} vs {fri.dims}")
    require(er.bits_equal(np.asarray(replica).ravel(), np.asarray(fri).ravel()), "fri replica",
            "the replicated shifted peaks do not reproduce flash_response_index")

    peaks = mb.peak_responses(edge)                                   # (network_id, sample, neuron)
    dsi = mb.direction_selectivity_index(edge)                        # (neuron, intensity) after squeeze
    pd = mb.preferred_direction(edge)                                 # (network_id, neuron, intensity)
    best = mb.correlation_to_known_tuning_curves(edge)                # (network_id, neuron[8], intensity)
    per_speed, tuning_types = tuning_per_speed(edge)                  # (network_id, neuron[8], intensity, speed)
    per_speed_max = per_speed.max("speed")
    require(per_speed_max.dims == best.dims, "tuning replica", f"dims {per_speed_max.dims} vs {best.dims}")
    require(er.bits_equal(np.asarray(per_speed_max).ravel(), np.asarray(best).ravel()),
            "tuning replica", "the per-speed r do not reproduce correlation_to_known_tuning_curves")
    require([float(s) for s in per_speed.speed.values] == [float(s) for s in SPEEDS], "tuning",
            f"speed order {per_speed.speed.values.tolist()}")
    for name, a in (("dsi", dsi), ("pd", pd), ("tuning", per_speed)):
        require(a.intensity.values.tolist() == [0, 1], name, f"intensity order {a.intensity.values.tolist()}")

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




# -- the checks -------------------------------------------------------------------

def check_valid_frames(resp, valid, battery):
    """x [sample, frame, ...]: finite through each stimulus's valid frames
    and NaN after them. Holds for the input: flyvis pads moving edges with
    NaN (datasets/moving_bar.py:337-346). Responses lag it; see
    response_nan_lags."""
    require(len(valid) == resp.shape[0], "valid frames", f"{battery}: {len(valid)} lengths for {resp.shape[0]} stimuli")
    for s, n in enumerate(valid):
        require(0 < n <= resp.shape[1], "valid frames", f"{battery} stimulus {s}: {n} valid of {resp.shape[1]} frames")
        require(np.isfinite(resp[s, :n]).all(), "valid frames", f"{battery} stimulus {s}: non-finite before frame {n}")
        require(np.isnan(resp[s, n:]).all(), "valid frames", f"{battery} stimulus {s}: not NaN after frame {n}")


def response_nan_lags(resp, valid, battery):
    """resp [sample, frame, neuron]: finite through each stimulus's valid
    frames; after them each neuron stays finite for a fixed number of frames
    (the steps the NaN input takes to reach it through the network: 0 for
    the photoreceptors, 3 for T5 in model 000), then is NaN to the last
    frame. That lag must be the same for every padded stimulus. Returns it
    per neuron, or None when no stimulus is padded. flyvis's own filter_post
    (datasets/moving_bar.py:720-726) allows for the same propagation."""
    lags = None
    for s, n in enumerate(valid):
        require(0 < n <= resp.shape[1], "valid frames", f"{battery} stimulus {s}: {n} valid of {resp.shape[1]} frames")
        require(np.isfinite(resp[s, :n]).all(), "valid frames", f"{battery} stimulus {s}: non-finite before frame {n}")
        tail = resp[s, n:]
        if tail.shape[0] == 0:
            continue
        nan = np.isnan(tail)
        require(nan[-1].all(), "valid frames", f"{battery} stimulus {s}: a neuron is not NaN at the last frame")
        first = nan.argmax(axis=0)
        after = np.arange(tail.shape[0])[:, None] >= first[None, :]
        require(bool((nan == after).all()) and bool(np.isfinite(tail[~after]).all()), "valid frames",
                f"{battery} stimulus {s}: after frame {n} a neuron is not finite-then-NaN")
        require(lags is None or np.array_equal(first, lags), "valid frames",
                f"{battery} stimulus {s}: NaN reaches the neurons at lags {first.tolist()}, not {lags if lags is None else lags.tolist()}")
        lags = first
    return None if lags is None else [int(x) for x in lags]


def check_windows(flash, edge):
    """FRI_WINDOW and EDGE_WINDOW_END, read from flyvis's own time axis and
    masks rather than trusted: every frame of each window, not just its ends."""
    from flyvis.analysis import moving_bar_responses as mb
    cfg = flash.attrs["config"]
    # flash_response_index's own query (flash_responses.py:67).
    frames = flash["responses"].query(frame=f"{-cfg['dt']} <= time <= {cfg['t_stim']}").frame.values
    require(frames.tolist() == list(range(FRI_WINDOW[0], FRI_WINDOW[1] + 1)), "windows",
            f"FRI frames {frames[:1].tolist()}..={frames[-1:].tolist()} ({len(frames)} frames)")
    # peak_responses' defaults (moving_bar_responses.py:59-66), computed as it computes them.
    ecfg = edge.attrs["config"]
    from_degree = ecfg["offsets"][0] * 2.25
    to_degree = (ecfg["offsets"][1] - 1) * 2.25
    masks = mb.get_time_masks(edge, from_column=from_degree / 5.8, to_column=to_degree / 5.8)
    for s, speed in enumerate(edge.speed.values):
        on = np.flatnonzero(np.asarray(masks.isel(sample=s)))
        want = list(range(FRI_WINDOW[0], EDGE_WINDOW_END[float(speed)] + 1))
        require(on.tolist() == want, "windows",
                f"edge sample {s} (speed {speed}): frames {on[:1].tolist()}..={on[-1:].tolist()} ({len(on)} frames)")


_REFERENCE = {}


def reference_manifest():
    """data/flyvis-reference's manifest, validated by export_reference.py's
    own validator, with every file's length and sha256 verified once."""
    if "m" not in _REFERENCE:
        m = json.loads((REFERENCE / "manifest.json").read_text())
        er.validate_manifest(m)
        er.verify_files(m["files"], REFERENCE)
        _REFERENCE["m"] = m
    return _REFERENCE["m"]


def read_block(m, name, run, dt, sequence):
    """One block of a flyvis-reference file, by the manifest's
    files[].blocks[] entry keyed (run, dt, sequence), in the file's dtype and
    the block's shape."""
    entries = [f for f in m["files"] if f["name"] == name]
    require(len(entries) == 1, "link", f"{len(entries)} manifest entries for {name}")
    f = entries[0]
    blocks = [b for b in f["blocks"] if (b["run"], b["dt"], b["sequence"]) == (run, dt, sequence)]
    require(len(blocks) == 1, "link", f"{name}: {len(blocks)} blocks for run {run}, dt {dt}, sequence {sequence}")
    b = blocks[0]
    with open(REFERENCE / name, "rb") as fh:
        fh.seek(b["offset"])
        data = fh.read(b["bytes"])
    require(len(data) == b["bytes"], "link", f"{name}: short read at {b['offset']}")
    return np.frombuffer(data, dtype=f["dtype"]).astype(er.NUMPY_DTYPE[f["dtype"]]).reshape(b["shape"])


F64_LINK_BOUND = 1e-10    # 0.3.0's libm rule at f64; never raise it


def _max_abs(a, b):
    """max |a - b| in float64 after exact widening; both sides finite."""
    a, b = np.asarray(a, dtype=np.float64), np.asarray(b, dtype=np.float64)
    require(a.shape == b.shape, "link", f"shapes {a.shape} vs {b.shape}")
    require(bool(np.isfinite(a).all() and np.isfinite(b).all()), "link", "non-finite values")
    return float(np.max(np.abs(a - b))) if a.size else 0.0


def link_record(diff, flyvis_gap, precision):
    """One array's link: its max difference from fixtures-2, the bound
    (1e-10 at f64; at f32 twice flyvis's own f32/f64 gap for that array, from
    fixtures-2's f32 and f64 files: the trace gates' rule) and their ratio."""
    bound = F64_LINK_BOUND if precision == "f64" else 2.0 * flyvis_gap
    require(bound > 0.0 and np.isfinite(bound), "link", f"{precision}: bound {bound!r}")
    return {"max_abs_diff": diff, "bound": bound, "ratio": diff / bound}


def link_to_fixtures2(network, flash, edge, precision):
    """Model 000's settled state and central traces against fixtures-2's
    reference (model 000, dt 1/200, this precision), within link_record's
    bound per array; the input fed equals the reference's input.f64.bin bit
    for bit. Prints every difference and ratio before requiring any. Beast's
    torch differs from the Mac's in the last bits (plan, Amendment 2)."""
    m = reference_manifest()

    def run_of(p):
        runs = [r["index"] for r in m["runs"] if (r["model"], r["dt"], r["precision"]) == ("000", "1/200", p)]
        require(len(runs) == 1, "link", f"{len(runs)} reference runs for model 000, dt 1/200, {p}")
        return runs[0]
    run, run32, run64 = run_of(precision), run_of("f32"), run_of("f64")
    np_dtype = np.float32 if precision == "f32" else np.float64
    centre_types = [c["cell_type"] for c in m["centre_nodes"]]
    require(centre_types == [str(t) for t in flash.cell_type.values], "link",
            "the reference's centre cell types differ from the responses' neuron order")

    settled = er.to_np(network.steady_state(1.0, DT, 1, 0.5, return_last=True).nodes.activity).ravel()
    require(settled.dtype == np_dtype, "link", f"{precision}: settled state is {settled.dtype}")
    ref = read_block(m, f"settled.{precision}.bin", run, "1/200", None)
    settled_diff = _max_abs(settled, ref)
    settled_gap = _max_abs(read_block(m, "settled.f32.bin", run32, "1/200", None),
                           read_block(m, "settled.f64.bin", run64, "1/200", None))
    centre_diffs, centre_gaps = [], []

    for q in m["sequences"]:
        fam, args = q["family"], q["args"]
        if fam == "flashes":
            ds = flash
            sel = (ds.baseline == args["baseline"]) & (ds.intensity == args["intensity"]) & (ds.radius == args["radius"])
            stim_dtype, n_valid = np_dtype, lambda s: FLASH_FRAMES
        else:
            require(fam == "moving_edge", "link", f"sequence {q['index']}: family {fam}")
            ds = edge
            sel = ((ds.angle == args["angle"]) & (ds.intensity == args["intensity"])
                   & (ds.speed == args["speed"]) & (ds.width == args["width"]))
            stim_dtype, n_valid = np.float64, lambda s: EDGE_VALID_FRAMES[float(ds.speed.values[s])]
        hits = np.flatnonzero(np.asarray(sel))
        require(len(hits) == 1, "link", f"sequence {q['index']} ({fam} {args}) matches {len(hits)} stimuli")
        s = int(hits[0])
        ref = read_block(m, f"centre.{precision}.bin", run, "1/200", q["index"])
        n = ref.shape[0]
        require(n == n_valid(s), "link", f"sequence {q['index']}: {n} reference frames, {n_valid(s)} valid here")
        got = ds["responses"].values[0, s, :n]
        require(got.dtype == np_dtype, "link", f"{precision}: sequence {q['index']} responses are {got.dtype}")
        centre_diffs.append(_max_abs(got, ref))
        centre_gaps.append(_max_abs(read_block(m, "centre.f32.bin", run32, "1/200", q["index"]),
                                    read_block(m, "centre.f64.bin", run64, "1/200", q["index"])))
        inp = read_block(m, "input.f64.bin", None, "1/200", q["index"])
        x = ds["stimulus"].values[s, :n, 0]
        require(x.dtype == stim_dtype, "link", f"sequence {q['index']}: input is {x.dtype}, expected {np.dtype(stim_dtype)}")
        require(er.bits_equal(x.astype(np.float64), inp), "link",
                f"sequence {q['index']}: input differs from fixtures-2's input.f64.bin")

    arrays = {"settled": link_record(settled_diff, settled_gap, precision),
              "centre": link_record(max(centre_diffs), max(centre_gaps), precision)}
    for name, r in arrays.items():
        print(f"link {precision} {name}: max |diff| {r['max_abs_diff']:.3e}, bound {r['bound']:.3e}, "
              f"ratio {r['ratio']:.3e}", flush=True)
    print(f"link {precision} centre per sequence: " + ", ".join(f"{d:.3e}" for d in centre_diffs), flush=True)
    for name, r in arrays.items():
        require(r["ratio"] <= 1.0, "link", f"{precision}: {name} differs from fixtures-2 by {r['max_abs_diff']:.3e}, "
                f"over its bound {r['bound']:.3e} (ratio {r['ratio']:.3e})")
    return {"run": run, "sequences": len(m["sequences"]), "arrays": arrays,
            "centre_max_abs_diff_per_sequence": centre_diffs, "inputs": "bit-equal"}


def thread_invariance(ours, model_fn, threads):
    """Model 000's indices at 1 thread equal `ours`, computed at `threads`,
    bit for bit."""
    import torch
    torch.set_num_threads(1)
    try:
        theirs = model_fn()
    finally:
        torch.set_num_threads(threads)
    require(sorted(ours) == sorted(theirs), "threads", f"keys {sorted(ours)} vs {sorted(theirs)}")
    for k in ours:
        a, b = np.asarray(ours[k]), np.asarray(theirs[k])
        require(a.shape == b.shape and er.bits_equal(a.ravel(), b.ravel()), "threads",
                f"{k} at 1 thread differs from {k} at {threads} threads")


def gap_record(a, b):
    """How two index arrays differ: bit-equal or not, the max |difference|
    over the entries finite in both, and how many entries are NaN in only one."""
    a, b = np.asarray(a), np.asarray(b)
    if a.shape != b.shape:
        return {"bit_equal": False, "shapes": [list(a.shape), list(b.shape)]}
    x, y = a.astype(np.float64), b.astype(np.float64)
    both = np.isfinite(x) & np.isfinite(y)
    d = np.abs(x - y)[both]
    return {"bit_equal": a.dtype == b.dtype and er.bits_equal(a.ravel(), b.ravel()),
            "max_abs_gap": float(d.max()) if d.size else 0.0,
            "nonfinite_mismatches": int((np.isfinite(x) != np.isfinite(y)).sum())}


def batch4_finding(network, flash_ds, edge_ds, flash_cfg, edge_cfg, ours):
    """flyvis's stock batch_size=4 against ours (1): recorded, not required.
    A failure of flyvis's batch-4 path is recorded as the finding."""
    try:
        theirs = indices(responses(network, flash_ds, flash_cfg, batch_size=4),
                         responses(network, edge_ds, edge_cfg, batch_size=4))[0]
    except (ExportError, RuntimeError, ValueError, IndexError, KeyError) as e:
        # torch's and xarray's failures, and indices' replica guards: the finding.
        # Anything else (MemoryError, KeyboardInterrupt, a bug here) still stops the run.
        print(f"batch-4 finding: flyvis's batch-4 path failed: {type(e).__name__}: {e}", flush=True)
        return {"error": f"{type(e).__name__}: {e}"}
    return {k: gap_record(ours[k], theirs[k]) for k in ours}


# Spec section 2, check 6: every index array's shape (per model) and a float dtype.
INDEX_SHAPES = {"fri_peaks": (65, 2), "fri": (65,), "edge_peaks": (144, 65), "dsi": (2, 65), "pd": (2, 65),
                "tuning_r": (8, 2, 6), "fri_to_known": (), "dsi_to_known": (), "pd_to_known": (8,)}


def check_index_arrays(ix, first, mid, precision):
    """Right after indices(), before anything is written: every index has
    its shape, a float dtype, only finite values, and the dtype the first
    model had at this precision (`first`, {key: dtype}, filled on first use)."""
    require(sorted(ix) == sorted(INDEX_SHAPES), "indices", f"model {mid} {precision}: keys {sorted(ix)}")
    for k, v in ix.items():
        a = np.asarray(v)
        where = f"model {mid} {precision}: {k}"
        require(a.dtype in (np.dtype("float32"), np.dtype("float64")), "indices", f"{where} is {a.dtype}")
        require(a.shape == INDEX_SHAPES[k], "indices", f"{where} has shape {a.shape}, expected {INDEX_SHAPES[k]}")
        require(bool(np.isfinite(a).all()), "indices", f"{where} has {int((~np.isfinite(a)).sum())} non-finite values")
        require(first.setdefault(k, a.dtype) == a.dtype, "indices", f"{where} is {a.dtype}, the first model's {first[k]}")


PHOTORECEPTORS = [f"R{i}" for i in range(1, 9)]
MAX_NAN_LAG = 10


def check_nan_lags(lags):
    """The NaN lag is where the padding's NaN reaches each neuron: the
    photoreceptors are fed it directly (lag 0), and nothing is more than a
    few synapses away (0-3 frames observed in model 000)."""
    for t in PHOTORECEPTORS:
        require(lags.get(t) == 0, "valid frames", f"photoreceptor {t} has NaN lag {lags.get(t)}, expected 0")
    worst = max(lags.values())
    require(0 <= min(lags.values()) and worst <= MAX_NAN_LAG, "valid frames",
            f"NaN lags span {min(lags.values())}..{worst} frames; at most {MAX_NAN_LAG} allowed")


def stimulus_tables(flash, edge, valid):
    fl = [flash_entry(s, float(flash.baseline.values[s]), int(flash.intensity.values[s]), int(flash.radius.values[s]),
                      FLASH_FRAMES) for s in range(flash.sizes["sample"])]
    ed = [edge_entry(s, int(edge.angle.values[s]), int(edge.intensity.values[s]), float(edge.speed.values[s]),
                     int(edge.width.values[s]), valid[s]) for s in range(edge.sizes["sample"])]
    require(len(fl) == 4 and len(ed) == 144, "stimuli", f"{len(fl)} flashes, {len(ed)} edges")
    return {"flashes": fl, "edges": ed}


# -- one shard: run the models ------------------------------------------------------

SHARD_NAME = re.compile(r"[A-Za-z0-9._-]+")


def scratch_root(name):
    return SCRATCH.with_name(f"{SCRATCH.name}-{name}")


def lock_path(name):
    return LOCK.with_name(f"{LOCK.stem}-{name}{LOCK.suffix}")


def run_models(ids, precisions, threads, name):
    """Computes every index of `ids` at `precisions`, with every check, and
    returns the shard (see save_shard). Writes only under target/."""
    git = er.git_state()                 # asked first: a broken git fails in seconds
    started = time.monotonic()
    scratch = scratch_root(name)
    mplconfig = ROOT / "target" / f"flyvis-indices-mplconfig-{name}"
    models_dir = scratch / "results" / "flow" / "0000"
    for d in (scratch, mplconfig):
        shutil.rmtree(d, ignore_errors=True)
        d.mkdir(parents=True)
    models_dir.mkdir(parents=True)
    os.environ.update(FLYVIS_ROOT_DIR=str(scratch), CUDA_VISIBLE_DEVICES="", MPLCONFIGDIR=str(mplconfig))
    copies = er.copy_models(RESULTS, models_dir, ids)

    import torch
    torch.set_num_threads(threads)
    pre = er.parse_pretrained(PRETRAINED_RS.read_text())
    ensemble = er.read_ensemble(ENSEMBLE_CSV.read_text())
    ckpts = {m: er.load_checkpoint(models_dir / m / er.CHECKPOINT) for m in ids}   # weights_only=True
    er.check_checkpoints(ckpts, ensemble, pre)
    refs = {m: er.reference_params(m, pre, ensemble) for m in ids}
    cells = list(pre["CELL_TYPES"])
    conns = [tuple(c) for c in pre["CONN_TYPES"]]
    del ckpts

    import flyvis
    from flyvis.datasets.flashes import Flashes
    from flyvis.datasets.moving_bar import MovingEdge
    from flyvis.network import NetworkView
    require(flyvis.device.type == "cpu", "setup", f"flyvis device is {flyvis.device}")
    require(pathlib.Path(flyvis.root_dir).resolve() == scratch.resolve(), "setup", f"flyvis root is {flyvis.root_dir}")
    print(f"{name}: flyvis {flyvis.__version__}, torch {torch.__version__}, {torch.get_num_threads()} threads, "
          f"models {','.join(ids)} at {','.join(precisions)}; checkpoints equal ensemble.csv", flush=True)

    shard = {"name": name, "models": {p: list(ids) for p in precisions}, "arrays": {},
             "cell_types": None, "tuning_types": None, "tables": None, "inputs": None, "edge_nan_lag": None,
             "timing": {}, "check_seconds": {}, "dataset_seconds": {}, "checks": {}, "batch4": {}}
    index_dtypes = {p: {} for p in precisions}
    for precision in precisions:
        want = torch.float32 if precision == "f32" else torch.float64
        np_dtype = np.float32 if precision == "f32" else np.float64
        torch.set_default_dtype(want)
        flash_cfg = dict(FLASH_CFG)
        edge_cfg = {**EDGE_CFG_BASE, "device": flyvis.device}
        t_ds = time.monotonic()
        flash_ds = Flashes(**flash_cfg)
        edge_ds = MovingEdge(**edge_cfg)            # renders the edges once per shard (cached in scratch)
        shard["dataset_seconds"][precision] = round(time.monotonic() - t_ds, 1)
        print(f"{name}: {precision} datasets built in {shard['dataset_seconds'][precision]} s", flush=True)
        for mid in ids:
            t0 = time.monotonic()
            require(torch.get_default_dtype() == want, "A2", f"default dtype is {torch.get_default_dtype()}", mid)
            nv = NetworkView(f"flow/0000/{mid}")
            best = pathlib.Path(nv.get_checkpoint("best")).resolve()
            require(best == (models_dir / mid / er.CHECKPOINT).resolve(), "checkpoint",
                    f"model {mid}: best checkpoint is {best}")
            network = nv.init_network("best")
            er.check_network_params(network, refs[mid], cells, conns, "loaded", mid)
            if precision == "f64":
                network.double()
                er.check_network_params(network, refs[mid], cells, conns, "widened", mid)
            er.check_param_dtypes(network, want, mid)

            flash = responses(network, flash_ds, flash_cfg)
            edge = responses(network, edge_ds, edge_cfg)
            valid = [EDGE_VALID_FRAMES[float(sp)] for sp in edge.speed.values]
            check_valid_frames(flash["stimulus"].values, [FLASH_FRAMES] * len(flash_ds), "flash input")
            check_valid_frames(edge["stimulus"].values, valid, "edge input")
            require(response_nan_lags(flash["responses"].values[0], [FLASH_FRAMES] * len(flash_ds), "flashes") is None,
                    "valid frames", "a flash is padded")
            lags = response_nan_lags(edge["responses"].values[0], valid, "edges")
            require(lags is not None, "valid frames", "no edge is padded")
            lags = dict(zip([str(t) for t in edge.cell_type.values], lags))
            check_nan_lags(lags)
            require(shard["edge_nan_lag"] in (None, lags), "valid frames",
                    f"model {mid} {precision}: NaN lags {lags}, the first model's {shard['edge_nan_lag']}")
            shard["edge_nan_lag"] = lags
            ix, types8 = indices(flash, edge)
            check_index_arrays(ix, index_dtypes[precision], mid, precision)
            shard["timing"][f"{mid}.{precision}"] = round(time.monotonic() - t0, 1)

            if mid == "000":
                t1 = time.monotonic()
                check_windows(flash, edge)
                print(f"{name}: 000 {precision}: windows hold", flush=True)
                link = link_to_fixtures2(network, flash, edge, precision)
                print(f"{name}: 000 {precision}: settled state and {link['sequences']} central traces within "
                      f"their bounds of fixtures-2 run {link['run']}; inputs bit-equal", flush=True)
                thread_invariance(ix, lambda: indices(responses(network, flash_ds, flash_cfg),
                                                      responses(network, edge_ds, edge_cfg))[0], threads)
                print(f"{name}: 000 {precision}: indices at 1 thread equal those at {threads}", flush=True)
                shard["batch4"][precision] = b4 = batch4_finding(network, flash_ds, edge_ds, flash_cfg, edge_cfg, ix)
                print(f"{name}: 000 {precision}: batch-4 finding {json.dumps(b4)}", flush=True)
                shard["checks"][precision] = {"windows": "passed", "link_to_fixtures2": "passed",
                                              "link": link, "thread_invariance": "passed",
                                              "threads_compared": [1, threads]}
                shard["check_seconds"][precision] = round(time.monotonic() - t1, 1)

            er.check_network_params(network, refs[mid], cells, conns, "after_runs", mid)
            er.check_param_dtypes(network, want, mid)
            require(shard["tuning_types"] in (None, types8), "tuning", f"model {mid}: tuning types {types8}")
            shard["tuning_types"] = types8
            cell_types = [str(t) for t in flash.cell_type.values]
            require(shard["cell_types"] in (None, cell_types), "cell types", f"model {mid} {precision}: neuron order differs")
            shard["cell_types"] = cell_types
            tables = stimulus_tables(flash, edge, valid)
            require(shard["tables"] in (None, tables), "stimuli", f"model {mid} {precision}: stimulus table differs")
            shard["tables"] = tables
            fx = flash["stimulus"].values
            ex = edge["stimulus"].values
            require(fx.dtype == np_dtype and ex.dtype == np.float64, "input",
                    f"model {mid} {precision}: stimulus dtypes {fx.dtype}, {ex.dtype}")
            stim = [fx[s, :, 0].astype(np.float64) for s in range(fx.shape[0])] + \
                   [ex[s, :n, 0].astype(np.float64) for s, n in enumerate(valid)]
            if shard["inputs"] is None:
                shard["inputs"] = stim
            else:
                for k, (x, y) in enumerate(zip(shard["inputs"], stim, strict=True)):
                    require(er.bits_equal(x, y), "input", f"model {mid} {precision}: stimulus {k} differs from the first model's")
            for k, v in ix.items():
                shard["arrays"].setdefault(k, {}).setdefault(precision, {})[mid] = np.asarray(v)
            print(f"{name}: {mid} {precision}: {shard['timing'][f'{mid}.{precision}']} s", flush=True)
            del network, nv, flash, edge, fx, ex, stim
        del flash_ds, edge_ds
    torch.set_default_dtype(torch.float32)
    shard["environment"] = {
        "host": os.uname().nodename, "machine": os.uname().machine,
        "flyvis": flyvis.__version__, "torch": torch.__version__, "numpy": np.__version__,
        "uv_lock_sha256": er.sha256_file(UV_LOCK), "git": git,
        "threads": threads, "batch_size": 1, "models_sha256": copies,
        "seconds": round(time.monotonic() - started, 1),
    }
    return shard


# -- shards on disk -----------------------------------------------------------------

def _npy_bytes(a):
    buf = io.BytesIO()
    np.save(buf, np.asarray(a), allow_pickle=False)     # not ascontiguousarray: it makes a 0-d array 1-d
    return buf.getvalue()


def _npy_load(path):
    return np.load(path, allow_pickle=False)


def save_shard(directory, shard):
    """A shard's raw results, written into a sibling .partial directory and
    renamed into place, so `directory` is either absent or whole:
      shard.json                      everything but the arrays and inputs
      arrays/<name>.<prec>.<id>.npy   one per index array, precision and model
      inputs.npz                      the stimuli, deduplicated: frames, index
    """
    directory = pathlib.Path(directory)
    refuse_data_dir(directory)
    require(not directory.exists(), "shard", f"{directory} exists; a shard is never overwritten")
    frames, index = dedup_frames(shard["inputs"])
    for x, ix in zip(shard["inputs"], index):
        require(er.bits_equal(frames[ix], x), "input", "dedup does not rebuild the input")
    tmp = directory.with_name(directory.name + ".partial")
    if tmp.exists():
        print(f"removing a stale {tmp} left by an earlier run", flush=True)
        shutil.rmtree(tmp)
    (tmp / "arrays").mkdir(parents=True)
    layout = {}
    for name, by_p in sorted(shard["arrays"].items()):
        for p, by_m in sorted(by_p.items()):
            for mid, a in sorted(by_m.items()):
                er._write_synced(tmp / "arrays" / f"{name}.{p}.{mid}.npy", _npy_bytes(a))
                layout.setdefault(name, {}).setdefault(p, {})[mid] = {"dtype": a.dtype.str, "shape": list(a.shape)}
    buf = io.BytesIO()
    np.savez(buf, frames=frames, index=np.concatenate(index), counts=np.array([len(i) for i in index], dtype=np.int64))
    er._write_synced(tmp / "inputs.npz", buf.getvalue())
    meta = {k: shard[k] for k in ("name", "models", "cell_types", "tuning_types", "tables", "edge_nan_lag", "timing",
                                   "check_seconds", "dataset_seconds", "checks", "batch4", "environment")}
    meta = {"format": 1, **meta, "arrays": layout}
    er._write_synced(tmp / "shard.json", er.json_bytes(meta))     # last: a shard.json means a whole shard
    er._fsync_dir(tmp / "arrays")
    er._fsync_dir(tmp)
    require(not directory.exists(), "shard", f"{directory} appeared while this shard ran")
    tmp.rename(directory)
    er._fsync_dir(directory.parent)


def load_shard(directory):
    """save_shard's inverse, checking every listed array against its record."""
    directory = pathlib.Path(directory)
    require((directory / "shard.json").is_file(), "merge", f"{directory} holds no shard.json")
    meta = json.loads((directory / "shard.json").read_text())
    require(meta.get("format") == 1, "merge", f"{directory}: shard format {meta.get('format')}")
    expected = {f"{name}.{p}.{mid}.npy" for name, by_p in meta["arrays"].items()
                for p, by_m in by_p.items() for mid in by_m}
    present = {q.name for q in directory.iterdir()}
    require(present == {"shard.json", "inputs.npz", "arrays"}, "merge",
            f"{directory}: unexpected entries {sorted(present - {'shard.json', 'inputs.npz', 'arrays'})} "
            f"or missing {sorted({'shard.json', 'inputs.npz', 'arrays'} - present)}")
    listed = {q.name for q in (directory / "arrays").iterdir()}
    require(listed == expected, "merge", f"{directory}/arrays: unlisted files {sorted(listed - expected)}, "
            f"missing {sorted(expected - listed)}")
    arrays = {}
    for name, by_p in meta["arrays"].items():
        for p, by_m in by_p.items():
            require(sorted(by_m) == sorted(meta["models"].get(p, [])), "merge",
                    f"{directory}: {name}.{p} holds models {sorted(by_m)}, the shard ran {meta['models'].get(p)}")
            for mid, rec in by_m.items():
                a = _npy_load(directory / "arrays" / f"{name}.{p}.{mid}.npy")
                require(a.dtype.str == rec["dtype"] and list(a.shape) == rec["shape"], "merge",
                        f"{directory}: {name}.{p}.{mid} is {a.dtype.str} {list(a.shape)}, recorded {rec}")
                arrays.setdefault(name, {}).setdefault(p, {})[mid] = a
    with np.load(directory / "inputs.npz", allow_pickle=False) as z:
        require(sorted(z.files) == ["counts", "frames", "index"], "merge", f"{directory}/inputs.npz holds {sorted(z.files)}")
        inputs = {k: z[k] for k in ("frames", "index", "counts")}
    for k, dt in (("frames", np.float64), ("index", np.uint32), ("counts", np.int64)):
        require(inputs[k].dtype == dt, "merge", f"{directory}/inputs.npz: {k} is {inputs[k].dtype}")
    return {**meta, "arrays": arrays, "inputs": inputs, "directory": str(directory)}


# -- the merge ----------------------------------------------------------------------

CHECKS_PASSED = ("windows", "link_to_fixtures2", "thread_invariance")


# What every shard must share with the shard that ran model 000's checks at
# f32: those checks proved the link and thread invariance only for its host,
# build and thread count (and torch's bits differ between machines).
SHARED_ENVIRONMENT = ("host", "machine", "flyvis", "torch", "numpy", "uv_lock_sha256", "threads")


def merge(shards, merged_git):
    """Checks the shards against each other and the export's model set, and
    returns (files, manifest, provenance) for write_set. `merged_git` is the
    merging checkout's er.git_state(), passed in by main."""
    require(len(shards) >= 1, "merge", "no shards")
    names = [sh["name"] for sh in shards]
    require(len(set(names)) == len(names), "merge", f"shard names repeat: {names}")
    owner = {}
    for sh in shards:
        require(set(sh["models"]) <= set(PRECISIONS), "merge", f"shard {sh['name']}: precisions {sorted(sh['models'])}")
        for p, ids in sh["models"].items():
            for mid in ids:
                require((p, mid) not in owner, "merge",
                        f"model {mid} at {p} is in shard {owner.get((p, mid))} and shard {sh['name']}")
                owner[(p, mid)] = sh
    for p in PRECISIONS:
        got = {mid for q, mid in owner if q == p}
        missing, extra = sorted(set(EXPORT_MODELS[p]) - got), sorted(got - set(EXPORT_MODELS[p]))
        require(not missing and not extra, "merge", f"{p}: models missing {missing}, unexpected {extra}")

    first = owner[("f32", "000")]
    env = first["environment"]
    for sh in shards:
        if sh is first:
            continue
        for key in ("cell_types", "tuning_types", "tables", "edge_nan_lag"):
            require(sh[key] == first[key], "merge", f"shard {sh['name']}: {key} differ from shard {first['name']}'s")
        for key in ("frames", "index", "counts"):
            require(er.bits_equal(sh["inputs"][key], first["inputs"][key]), "merge",
                    f"shard {sh['name']}: inputs ({key}) differ from shard {first['name']}'s")
        for key in SHARED_ENVIRONMENT:
            require(sh["environment"][key] == env[key], "merge",
                    f"shard {sh['name']}: {key} {sh['environment'][key]!r}, shard {first['name']} "
                    f"(model 000's checks) {env[key]!r}")
        for key in ("head", "dirty"):
            require(sh["environment"]["git"][key] == env["git"][key], "merge",
                    f"shard {sh['name']}: git {key} {sh['environment']['git'][key]!r}, shard {first['name']} "
                    f"(model 000's checks) {env['git'][key]!r}")
        require(sorted(sh["arrays"]) == sorted(first["arrays"]), "merge", f"shard {sh['name']}: array names differ")

    checks, batch4, links = {}, {}, {}
    for p in PRECISIONS:
        sh = owner[(p, "000")]
        c = sh["checks"].get(p)
        require(c is not None and all(c.get(k) == "passed" for k in CHECKS_PASSED) and "link" in c, "merge",
                f"model 000's checks at {p} did not pass in shard {sh['name']}: {c}")
        require(p in sh["batch4"], "merge", f"shard {sh['name']}: no batch-4 finding at {p}")
        checks[p] = {**{k: v for k, v in c.items() if k != "link"}, "shard": sh["name"]}
        links[p] = c["link"]
        batch4[p] = sh["batch4"][p]

    # The arrays, stacked in model order, each in its computed dtype.
    files, arrays, gaps, stacked = {}, [], {}, {}
    for name in sorted(first["arrays"]):
        for p in PRECISIONS:
            parts = [owner[(p, mid)]["arrays"][name][p][mid] for mid in EXPORT_MODELS[p]]
            dt, shape = parts[0].dtype, parts[0].shape
            require(all(x.dtype == dt and x.shape == shape for x in parts), "merge",
                    f"{name}.{p}: dtype or shape differs between models")
            dtype = {np.dtype("float32"): "<f4", np.dtype("float64"): "<f8"}.get(dt)
            require(dtype is not None, "merge", f"{name}.{p} is {dt}")
            x = np.stack(parts)
            stacked[(name, p)] = x
            arrays.append(array_entry(name, p, dtype, list(x.shape)))
            files[f"{name}.{p}.bin"] = x.astype(dtype).tobytes()
        a = np.stack([stacked[(name, "f32")][MODELS.index(mid)] for mid in F64_MODELS]).astype(np.float64)
        b = stacked[(name, "f64")].astype(np.float64)
        require(np.isfinite(a).all() and np.isfinite(b).all(), "gap", f"{name}: non-finite values in models {F64_MODELS}")
        gaps[name] = er.max_abs_gap(a, b)

    # The input, deduplicated, and its index blocks.
    inp = first["inputs"]
    tables = first["tables"]
    entries = [("flashes", e) for e in tables["flashes"]] + [("edges", e) for e in tables["edges"]]
    require(inp["counts"].tolist() == [e["n_frames"] for _, e in entries], "merge", "input lengths differ from the stimulus table")
    require(inp["frames"].dtype == np.float64 and inp["frames"].ndim == 2 and inp["frames"].shape[1] == HEXALS, "merge",
            f"frames {inp['frames'].dtype} {inp['frames'].shape}")
    blocks, offset = [], 0
    for battery, e in entries:
        blocks.append(index_block(battery, e["index"], offset, e["n_frames"]))
        offset += e["n_frames"]
    files["frames.f64.bin"] = inp["frames"].astype("<f8").tobytes()
    files["frame_index.u32.bin"] = inp["index"].astype("<u4").tobytes()
    sha = {name: hashlib_sha256(data) for name, data in files.items()}
    manifest = build_manifest(first["cell_types"], tables["flashes"], tables["edges"], inp["frames"].shape[0],
                              blocks, arrays, sha, models=EXPORT_MODELS, tuning_types=first["tuning_types"])
    provenance = {
        "generated": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "host": env["host"], "machine": env["machine"],
        "flyvis": env["flyvis"], "torch": env["torch"], "numpy": env["numpy"],
        "uv_lock_sha256": env["uv_lock_sha256"], "git": env["git"],
        "threads": env["threads"], "batch_size": 1,
        "seconds": round(sum(sh["environment"]["seconds"] for sh in shards), 1),
        "per_model_seconds": {k: v for sh in shards for k, v in sh["timing"].items()},
        "f32_f64_gap": gaps,
        "f32_f64_gap_models": F64_MODELS,
        "batch4_vs_batch1_max_gap": {p: (b if "error" in b else {k: r.get("max_abs_gap") for k, r in b.items()})
                                     for p, b in batch4.items()},
        "batch4_vs_batch1": batch4,
        "link_to_fixtures2": links,
        "models_sha256": {mid: h for sh in shards for mid, h in sh["environment"]["models_sha256"].items()},
        "models": EXPORT_MODELS,
        "checks": checks,
        "edge_nan_lag_frames": first["edge_nan_lag"],
        "merged_on": {"host": os.uname().nodename, "machine": os.uname().machine, "git": merged_git},
        "shards": {sh["name"]: {"models": sh["models"], "seconds": sh["environment"]["seconds"],
                                "per_model_seconds": sh["timing"], "check_seconds": sh["check_seconds"],
                                "dataset_seconds": sh.get("dataset_seconds", {})}
                   for sh in shards},
    }
    return files, manifest, provenance


# -- main ---------------------------------------------------------------------------

def parse_list(text, allowed, what):
    items = [t for t in text.split(",") if t]
    require(items and len(set(items)) == len(items) and set(items) <= set(allowed), "usage", f"{what} {text!r}")
    return [x for x in allowed if x in items]          # canonical order: f32 before f64, models ascending


def main(argv):
    p = argparse.ArgumentParser(prog="export_indices.py")
    p.add_argument("--threads", type=int)
    p.add_argument("--models")
    p.add_argument("--precisions")
    p.add_argument("--shard-out", type=pathlib.Path)
    p.add_argument("--smoke")
    p.add_argument("--merge", nargs="+", type=pathlib.Path)
    p.add_argument("--out", type=pathlib.Path)
    p.add_argument("--force", action="store_true")
    a = p.parse_args(argv[1:])
    modes = [m for m in ("smoke", "shard_out", "merge") if getattr(a, m) is not None]
    require(len(modes) == 1, "usage", "give exactly one of --smoke, --shard-out, --merge")

    if a.merge:
        require(a.out is not None, "usage", "--merge needs --out")
        require(a.threads is None and a.models is None and a.precisions is None, "usage",
                "--threads, --models and --precisions belong to the shards, not the merge")
        refuse_data_dir(a.out)
        files, manifest, provenance = merge([load_shard(d) for d in a.merge], er.git_state())
        write_set(a.out, files, manifest, provenance, a.force)
        print(f"wrote {a.out}: {len(files)} files, {manifest['frames']['rows']} distinct frames; "
              f"f32 x {len(MODELS)}, f64 x {len(F64_MODELS)}")
        return 0

    require(a.out is None and not a.force, "usage", "--out and --force belong to --merge")
    require(a.threads is not None and a.threads >= 1, "usage", "--threads N is required to compute")
    require("FLYVIS_ROOT_DIR" not in os.environ, "environment", "an inherited FLYVIS_ROOT_DIR is refused")
    precisions = parse_list(a.precisions or "f32", PRECISIONS, "--precisions")
    if a.smoke:
        require(a.models is None, "usage", "--smoke takes one model; --models is for shards")
        ids, name = parse_list(a.smoke, MODELS, "--smoke"), "smoke"
    else:
        ids, name = parse_list(a.models or ",".join(MODELS), MODELS, "--models"), a.shard_out.name
        require(SHARD_NAME.fullmatch(name) is not None, "usage", f"shard name {name!r}")
        refuse_data_dir(a.shard_out)
        require(not a.shard_out.exists(), "shard", f"{a.shard_out} exists; a shard is never overwritten")
    lock = er.acquire_lock(lock_path(name))       # held until exit
    try:
        shard = run_models(ids, precisions, a.threads, name)
        if a.smoke:
            print("smoke: every check passed; nothing written")
            return 0
        save_shard(a.shard_out, shard)
        print(f"wrote shard {a.shard_out}: models {','.join(ids)} at {','.join(precisions)}, "
              f"{shard['environment']['seconds']} s")
        return 0
    finally:
        lock.close()


if __name__ == "__main__":
    sys.stdout.reconfigure(line_buffering=True)
    try:
        sys.exit(main(sys.argv))
    except ExportError as e:
        print(f"export_indices: {e}", file=sys.stderr)
        sys.exit(1)
