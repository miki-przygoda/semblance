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
        raise ExportError(f"{check} failed{_where(run, sequence)}: {detail}")


def _where(run, sequence) -> str:
    parts = []
    if run is not None:
        parts.append(f"run {run}")
    if sequence is not None:
        parts.append(f"sequence {sequence}")
    return f" ({', '.join(parts)})" if parts else ""


@contextlib.contextmanager
def located(run, sequence=None):
    """Re-raises any exception that is not an ExportError (an IndexError from
    torch, a KeyError from flyvis) as one naming the run and sequence, with the
    original chained. ExportError and KeyboardInterrupt pass through as they are."""
    try:
        yield
    except ExportError:
        raise
    except Exception as e:
        raise ExportError(f"run failed{_where(run, sequence)}: {type(e).__name__}: {e}") from e


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
            try:
                out.mkdir()
            except FileNotFoundError as e:
                raise ExportError(f"the parent of --out {out} does not exist; create it first") from e
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
        with located(run, k):
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
        with located(run, k):
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
        with located(r):
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
    # The run log stays in order when stderr goes to a separate file.
    sys.stdout.reconfigure(line_buffering=True)
    args = None
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
    except KeyboardInterrupt:
        print("export_reference: interrupted", file=sys.stderr)
        # From write step 4 on, the marker stays behind and a human recovers.
        if args is not None and args.out.is_dir() and any(
                p.name.startswith(".flyvis-reference.incomplete-") for p in args.out.iterdir()):
            print(RECOVERY, file=sys.stderr)
        return 130


if __name__ == "__main__":
    sys.exit(main())
