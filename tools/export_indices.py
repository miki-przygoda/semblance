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


# -- temporary smoke run (Task 3 replaces this with main) --------------------------

def _smoke(mid, precisions=PRECISIONS):
    """Loads one model as export_reference.py does, runs both batteries at
    both precisions, and prints each array's shape and dtype. Writes nothing
    outside target/."""
    require("FLYVIS_ROOT_DIR" not in os.environ, "setup", "FLYVIS_ROOT_DIR is set in the environment; unset it")
    mplconfig = ROOT / "target" / "flyvis-indices-mplconfig"
    for d in (SCRATCH, mplconfig):
        shutil.rmtree(d, ignore_errors=True)
        d.mkdir(parents=True)
    os.environ["FLYVIS_ROOT_DIR"] = str(SCRATCH)
    os.environ["CUDA_VISIBLE_DEVICES"] = ""
    os.environ["MPLCONFIGDIR"] = str(mplconfig)
    er.copy_models(RESULTS, SCRATCH / "results" / "flow" / "0000", [mid])
    import torch

    torch.load(SCRATCH / "results" / "flow" / "0000" / mid / er.CHECKPOINT, map_location="cpu", weights_only=True)
    import flyvis
    from flyvis.datasets.flashes import Flashes
    from flyvis.datasets.moving_bar import MovingEdge
    from flyvis.network import NetworkView

    require(flyvis.device.type == "cpu", "setup", f"flyvis device is {flyvis.device}")
    require(pathlib.Path(flyvis.root_dir).resolve() == SCRATCH.resolve(), "setup", f"flyvis root is {flyvis.root_dir}")
    edge_cfg = {**EDGE_CFG_BASE, "device": flyvis.device}
    print(f"smoke: flyvis {flyvis.__version__}, model {mid}, torch threads {torch.get_num_threads()}", flush=True)

    for prec in precisions:
        want = torch.float32 if prec == "f32" else torch.float64
        torch.set_default_dtype(want)
        t0 = time.monotonic()
        nv = NetworkView(f"flow/0000/{mid}")
        best = pathlib.Path(nv.get_checkpoint("best")).resolve()
        require(best == (SCRATCH / "results" / "flow" / "0000" / mid / er.CHECKPOINT).resolve(), "setup",
                f"best checkpoint is {best}")
        network = nv.init_network("best")
        if prec == "f64":
            network.double()
        flash_cfg = dict(FLASH_CFG)
        flash = responses(network, Flashes(**flash_cfg), flash_cfg)
        t1 = time.monotonic()
        edge = responses(network, MovingEdge(**edge_cfg), edge_cfg)
        t2 = time.monotonic()
        out, tuning_types = indices(flash, edge)
        t3 = time.monotonic()
        print(f"{prec}: flash stimulus {flash['stimulus'].shape} {flash['stimulus'].dtype}, "
              f"responses {flash['responses'].shape} {flash['responses'].dtype}", flush=True)
        print(f"{prec}: edge stimulus {edge['stimulus'].shape} {edge['stimulus'].dtype}, "
              f"responses {edge['responses'].shape} {edge['responses'].dtype}", flush=True)
        for name, a in out.items():
            print(f"{prec}: {name:13s} {list(a.shape)} {a.dtype}", flush=True)
        print(f"{prec}: tuning types {tuning_types}", flush=True)
        print(f"{prec}: replicas exact; wall flashes {t1 - t0:.1f}s, edges {t2 - t1:.1f}s, "
              f"indices {t3 - t2:.1f}s, model total {t3 - t0:.1f}s", flush=True)
        del network, nv, flash, edge
    torch.set_default_dtype(torch.float32)
    return 0


if __name__ == "__main__":
    if len(sys.argv) in (3, 4) and sys.argv[1] == "--smoke" and set(sys.argv[3:]) <= set(PRECISIONS):
        sys.exit(_smoke(sys.argv[2], tuple(sys.argv[3:]) or PRECISIONS))
    sys.exit("usage: export_indices.py --smoke MODEL [f32|f64]  (main arrives in Task 3)")
