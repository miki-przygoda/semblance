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
