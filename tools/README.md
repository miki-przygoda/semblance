# tools

Offline Python. **Not part of the Rust workspace, and not a dependency of it.**

## Why this exists at all

The workspace has zero external dependencies and that is deliberate — see
`CLAUDE.md`. This directory does not change that. Nothing here is imported,
linked, or invoked by any crate, and nothing here appears in `Cargo.lock`. The
Rust side reads plain files; this is where a few of those plain files get made.

It exists for exactly one reason: **flyvis's trained parameters ship as PyTorch
checkpoints**, and reading a torch pickle from dependency-free Rust is not a
sensible use of anyone's time.

The connectome itself needs none of this. That was established during epic 0.2.0
— the tiling and edge construction reimplement exactly in Rust, and the published
JSON is parsed directly. The line is:

| artifact | how it is read |
|---|---|
| connectome spec (`…avg_filters.json`) | **Rust, directly** — parsed from the published file |
| curated ground truth | **Rust constants** — transcribed once from upstream |
| **trained parameters** | **here** — exported once to plain files |

If you find yourself adding a script that the Rust side needs at *runtime*, stop.
That is the boundary this directory is meant to hold.

## Setup

```bash
cd tools
uv sync
```

`uv` reads `.python-version` (3.12) and fetches that interpreter if it is missing.
The host default is newer than flyvis supports, so the pin is load-bearing rather
than decorative — flyvis 1.2.0 declares `requires-python = ">=3.9,<3.13"`.

The virtualenv lands in `tools/.venv/` and is gitignored. `uv.lock` **is**
committed — it is the reproducibility record for anything exported from here.

## Running something

```bash
uv run python <script>.py
```

## Scripts

| Script | What it exports | Destination |
|---|---|---|
| `download_pretrained.py` | flyvis's pretrained model ensemble, fetched from upstream | `data/flyvis-pretrained/` |
| `export_params.py` | the validation model's trained parameters, plus the full ensemble as data | `crates/flyvis/src/pretrained.rs`, `data/flyvis-pretrained/ensemble.csv` |
| `export_build.py` | flyvis's own built connectome — nodes, connection types, offsets, scipy's convex hulls, and matplotlib's containment verdicts — as fixtures the Rust expansion is tested against | `data/flyvis-build/` |
| `export_stimuli.py` | flyvis's own rendered flash stimuli in full, and a deterministic three-centre subset of the dot stimuli, alongside the hexal lattice order and each family's sequence order — as fixtures the Rust stimulus generators are checked against | `data/flyvis-stimuli/` |
| `export_moving_edges.py` | flyvis's own rendered moving-edge offsets for four angles, its pixel-to-ommatidium mask, and the resample/pad timing — the reference for the one tolerance-gated comparison in the workspace | `data/flyvis-stimuli/` |
| `export_reference.py` | flyvis's own voltage traces for 0.3.0's gates — models 000 and 049, dt 1/200 and 0.02, float32 and float64: the settled state, the centre column at every settling step and frame, all cells at 6 frames per sequence, and the input movies flyvis fed — plus the moving-edge geometry at height 80. Every check runs before anything is written | `data/flyvis-reference/`, and the three `*_h80*` files in `data/flyvis-stimuli/` |

`export_reference.py` sets `FLYVIS_ROOT_DIR` itself, to a scratch root under
`target/` that it wipes and recreates on every run, and refuses to start if the
variable is already set: flyvis writes `connectome/`, `renderings/` and each
model's `__cache__/` under its root, and those caches' keys ignore the torch
dtype. It writes nothing under `data/` except its outputs, refuses to replace
them without `--force`, and holds a lock so two runs cannot share the scratch
root. Its self-tests need no flyvis:
`uv run --project tools tools/test_export_reference.py`.
`uv run --project tools python tools/test_export_reference.py --validate <dir>`
checks an export's manifest and its 9 binaries; it does not check the three
`_h80` files or the `export_id` pair.

**Do not re-export a pinned fixture by hand.** `ci/fetch-fixtures.sh` checks
every file in `ci/fixtures.sha256` byte for byte, and every export is dated —
in its provenance file, or for `ensemble.csv` in its header. Re-running
`export_params.py`, or any exporter above, therefore makes the fetch script
refuse the new file whenever the re-export's bytes differ from the pinned ones. To return to the published set, delete the re-exported
files and run `ci/fetch-fixtures.sh`; a deliberate re-export needs a new
`fixtures-N`, with `ci/fixtures.sha256` and `ci/fixtures-tag` changed in one
commit.

## Provenance rule

Anything exported from here is an input to a later result, so it has to be
traceable. Every script must record, in its output or beside it: the flyvis
version, the upstream commit or release it read, and the date. An exported file
whose origin cannot be reconstructed is not usable as evidence.
