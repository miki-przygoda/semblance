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

## Provenance rule

Anything exported from here is an input to a later result, so it has to be
traceable. Every script must record, in its output or beside it: the flyvis
version, the upstream commit or release it read, and the date. An exported file
whose origin cannot be reconstructed is not usable as evidence.
