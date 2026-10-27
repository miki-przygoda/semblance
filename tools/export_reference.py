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
