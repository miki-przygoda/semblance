# Flash and Dot Stimuli Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate flyvis's circular ON/OFF flash and single-ommatidium dot stimuli in Rust, matching flyvis's own rendered output element-wise.

**Architecture:** A `Stimulus` trait producing one frame at a time, because the dot family is 5.8 GB if materialised and because a frame is what an integrator consumes per Euler step. The 721-hexal lattice ordering is not reimplemented — it is extracted from the existing `place_nodes`, which is already gated element-wise against flyvis's own node table. Correctness is established by comparing against exported flyvis arrays, not by inspection.

**Tech Stack:** Rust 1.85, edition 2024, zero external crates. Python via `uv` in `tools/` for fixture export only, run by hand.

**Spec:** `docs/superpowers/specs/2026-09-28-stimulus-flashes-and-dots-design.md`

## Global Constraints

- **Zero external dependencies.** `Cargo.lock` must stay at exactly 8 `semblance*` packages. `std` is not external. No crates.io dependency, not even for tests.
- **Rust 1.85, edition 2024.** `gen` is a reserved keyword.
- **Commit style: plain imperative subject, no `feat:`/`fix:`/`chore:` prefixes.** Match the existing log.
- **Do not run `cargo fmt`.** The repo is not rustfmt-clean and the tool reformats unrelated crates. Format new code by hand to match surrounding style.
- **Do not modify** `connectome`, `codec`, `fidelity`, `sim`, `evo`, or `app`. Only `semblance-flyvis` and `tools/`.
- **Clippy has 5 pre-existing warnings** (4 `needless_range_loop` in `sim`, 1 manual char comparison in `connectome`). Do not count those as new; do not add more.
- **Fixture-dependent tests must honour `SEMBLANCE_REQUIRE_FIXTURES=1`**, following `crates/flyvis/src/test_support.rs`. A gate that silently skips is worse than no gate.
- `data/` is gitignored. Never `git add` anything under it.
- **`dt` is always a caller-supplied argument, never a constant.** `max(tau, dt)` in the dynamics means `dt` changes which cell types are floored, so it is a modelling choice. flyvis's own response analyses use `1.0 / 200.0`.

---

### Task 1: Extract the canonical hex-lattice ordering

**Files:**
- Modify: `crates/flyvis/src/expand.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub(crate) fn hex_coords(extent: i32) -> impl Iterator<Item = (i32, i32)>`

**Why this is its own task.** `place_nodes` is gated element-wise against flyvis's own `nodes.csv` across 45,669 nodes. Refactoring it is safe only if those gates still pass, and that deserves its own review rather than being buried inside a feature.

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` in `crates/flyvis/src/expand.rs`:

```rust
    #[test]
    fn hex_coords_yields_the_full_lattice_at_extent_15() {
        assert_eq!(hex_coords(15).count(), 721);
    }

    #[test]
    fn hex_coords_starts_at_minus_extent_and_centres_at_360() {
        let c: Vec<(i32, i32)> = hex_coords(15).collect();
        assert_eq!(c[0], (-15, 0), "index 0");
        assert_eq!(c[360], (0, 0), "the origin sits at index 360");
        assert_eq!(c[720], (15, 0), "index 720");
    }

    /// The asymmetric v bounds are what make the region a hexagon. A naive
    /// `-extent..=extent` for v would give a rhombus of (2n+1)^2 = 961 cells
    /// at extent 15 -- still plausible-looking, and wrong.
    #[test]
    fn hex_coords_is_a_hexagon_not_a_rhombus() {
        assert_eq!(hex_coords(15).count(), 721);
        assert_ne!(hex_coords(15).count(), 31 * 31);
    }

    #[test]
    fn hex_coords_degenerates_correctly_at_small_extents() {
        assert_eq!(hex_coords(0).collect::<Vec<_>>(), vec![(0, 0)]);
        assert_eq!(hex_coords(1).count(), 7);
        assert_eq!(hex_coords(2).count(), 19);
    }

    /// The extraction must be behaviour-preserving: an unstrided type placed by
    /// `place_nodes` must visit exactly `hex_coords`' sequence.
    #[test]
    fn place_nodes_visits_hex_coords_order_for_an_unstrided_type() {
        let placed = place_nodes(&[node("A", Pattern::Stride { u: 1, v: 1 })], 15);
        let want: Vec<(i32, i32)> = hex_coords(15).collect();
        let got: Vec<(i32, i32)> = placed.iter().map(|p| (p.u, p.v)).collect();
        assert_eq!(got, want);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis expand`
Expected: FAIL to compile — `hex_coords` not found.

- [ ] **Step 3: Add `hex_coords` and rewrite `place_nodes` to use it**

Insert `hex_coords` immediately above `place_nodes` in `crates/flyvis/src/expand.rs`:

```rust
/// The canonical hex-lattice order: outer `u` ascending, inner `v` ascending
/// over the hexagonal bound.
///
/// This one ordering is shared by the connectome's node construction and by
/// every stimulus's hexal indexing. flyvis derives it independently in both
/// places from textually identical loops (`utils/hex_utils.py:15-42`
/// `get_hex_coords`, and `connectome/connectome.py:326-342`
/// `add_strided_nodes`). Naming it here makes that agreement explicit instead
/// of incidental -- a stimulus whose flat index disagrees with the connectome's
/// node index is spatially scrambled while keeping the right shape, the right
/// frame count and entirely plausible dynamics.
///
/// The `v` bounds are asymmetric on purpose: they are what make the region a
/// hexagon rather than a rhombus. At `extent = 15` this yields 721 pairs,
/// index 0 is `(-15, 0)` and the origin `(0, 0)` is index 360.
pub(crate) fn hex_coords(extent: i32) -> impl Iterator<Item = (i32, i32)> {
    (-extent..=extent).flat_map(move |u| {
        ((-extent).max(-extent - u)..=extent.min(extent - u)).map(move |v| (u, v))
    })
}
```

Then replace `place_nodes`'s two inner loops with a single `hex_coords` loop, keeping its doc comment and the stride filter exactly as they are:

```rust
pub(crate) fn place_nodes(nodes: &[NodeSpec], extent: i32) -> Vec<PlacedNode> {
    let mut out = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let ((u_stride, v_stride), ext) = strides_and_extent(node.pattern, extent);
        for (u, v) in hex_coords(ext) {
            if u % u_stride == 0 && v % v_stride == 0 {
                out.push(PlacedNode { cell_type: index as u16, u, v });
            }
        }
    }
    out
}
```

**Do not change `place_nodes`'s doc comment**, including the note about Python's `%` versus Rust's. It is still correct and still load-bearing.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p semblance-flyvis`
Expected: PASS. Every pre-existing `expand` test must pass **unchanged** — that is the proof the extraction preserved behaviour.

- [ ] **Step 5: Verify the connectome gates still pass**

Run: `cargo test -p semblance-flyvis expand_checks -- --nocapture`
Expected: all 7 gates pass, none skipped, including `node_ordering_matches_flyvis` (45,669 nodes compared element-wise) and `every_edge_matches_flyvis` (1,513,231 edges).

**If any of these fail, the extraction is wrong.** Revert it and duplicate the loop instead. Do not weaken a passing gate.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/expand.rs
git commit -m "Extract the canonical hex-lattice ordering from place_nodes"
```

---

### Task 2: Export flyvis's rendered stimuli as fixtures

**Files:**
- Create: `tools/export_stimuli.py`
- Modify: `tools/README.md` (add a row to the Scripts table)
- Output (gitignored): `data/flyvis-stimuli/{hex_coords.csv,flashes.bin,flashes_args.csv,dots_subset.bin,dots_subset_args.csv,PROVENANCE.json}`

**Interfaces:**
- Consumes: nothing.
- Produces: the fixture files below. Later tasks parse them by exact column position and byte layout; changing either breaks them.

| File | Layout |
|---|---|
| `hex_coords.csv` | `index,u,v` — 721 rows, canonical lattice order |
| `flashes.bin` | little-endian `f32`, 4 × 600 × 721 = 1,730,400 values = 6,921,600 bytes |
| `flashes_args.csv` | `index,baseline,intensity,radius` — 4 rows, flyvis's own sequence order |
| `dots_subset.bin` | little-endian `f32`, 6 × 1400 × 721 = 6,056,400 values = 24,225,600 bytes |
| `dots_subset_args.csv` | `order,sequence,u,v,offset,intensity` — 6 rows |

- [ ] **Step 1: Write the export script**

```python
"""Export flyvis's own rendered flash and dot stimuli as Rust test fixtures.

The Rust side regenerates these from parameters rather than loading them. Two
orderings decide whether that regeneration is right -- the 721-hexal lattice
order, and the order of sequences within each family -- and neither is
announced by the arrays themselves. flashes is intensity-major; dots is
centre-major. Both are easy to state confidently and wrongly, so this dumps
flyvis's own answer for the tests to compare against.

The dot family is 1,442 x 1,400 x 721 floats = 5.8 GB, far too large to dump.
A deterministic subset is exported instead: the first, middle (the origin) and
last centre in lattice order, both polarities each.

Run by hand:  uv run --project tools tools/export_stimuli.py
"""

import datetime
import json
import pathlib
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "flyvis-stimuli"

EXTENT = 15
DT = 1.0 / 200.0                 # what flyvis's own response analyses use
DOT_SUBSET_CENTRES = (0, 360, 720)   # first, the origin, last -- in lattice order


def main() -> int:
    import flyvis
    from flyvis.datasets.dots import Dots
    from flyvis.datasets.flashes import Flashes
    from flyvis.utils import hex_utils

    OUT.mkdir(parents=True, exist_ok=True)

    # -- the canonical hexal ordering ----------------------------------------
    u, v = hex_utils.get_hex_coords(EXTENT)
    assert len(u) == 721, len(u)
    assert (int(u[0]), int(v[0])) == (-15, 0), (u[0], v[0])
    centre = int(np.nonzero((u == 0) & (v == 0))[0][0])
    assert centre == 360, centre
    with (OUT / "hex_coords.csv").open("w") as f:
        f.write("index,u,v\n")
        for i, (uu, vv) in enumerate(zip(u, v)):
            f.write(f"{i},{int(uu)},{int(vv)}\n")
    print(f"  hex_coords.csv: {len(u)} rows, origin at index {centre}")

    # -- flashes -------------------------------------------------------------
    fl = Flashes(dt=DT)
    arr = np.asarray(fl.flashes_dir.flashes[:], dtype=np.float32)
    assert arr.shape == (4, 600, 721), arr.shape
    (OUT / "flashes.bin").write_bytes(arr.astype("<f4").tobytes())
    with (OUT / "flashes_args.csv").open("w") as f:
        f.write("index,baseline,intensity,radius\n")
        for i, r in fl.arg_df.iterrows():
            f.write(f"{i},{float(r.baseline)!r},{float(r.intensity)!r},{int(r.radius)}\n")
    print(f"  flashes.bin: {arr.shape}, {(OUT / 'flashes.bin').stat().st_size} bytes")
    print(f"  flashes_args.csv: {len(fl.arg_df)} rows")

    # -- dots: a deterministic subset ----------------------------------------
    d = Dots(dt=DT)
    assert len(d) == 1442, len(d)
    seqs = [c * 2 + p for c in DOT_SUBSET_CENTRES for p in (0, 1)]
    blobs, rows = [], []
    for s in seqs:
        item = np.asarray(d[s], dtype=np.float32)
        assert item.shape == (1400, 721), (s, item.shape)
        blobs.append(item.astype("<f4").tobytes())
        r = d.arg_df.iloc[s]
        rows.append((s, int(r.u), int(r.v), int(r.offset), float(r.intensity)))
    (OUT / "dots_subset.bin").write_bytes(b"".join(blobs))
    with (OUT / "dots_subset_args.csv").open("w") as f:
        f.write("order,sequence,u,v,offset,intensity\n")
        for i, (s, uu, vv, off, inten) in enumerate(rows):
            f.write(f"{i},{s},{uu},{vv},{off},{inten!r}\n")
    print(f"  dots_subset.bin: {len(seqs)} sequences, "
          f"{(OUT / 'dots_subset.bin').stat().st_size} bytes")

    # -- provenance ----------------------------------------------------------
    import torch

    (OUT / "PROVENANCE.json").write_text(
        json.dumps(
            {
                "exported": datetime.date.today().isoformat(),
                "flyvis": getattr(flyvis, "__version__", "?"),
                "torch": torch.__version__,
                "numpy": np.__version__,
                "extent": EXTENT,
                "dt": DT,
                "flashes_shape": list(arr.shape),
                "n_dot_sequences_total": len(d),
                "dot_subset_centres": list(DOT_SUBSET_CENTRES),
                "dot_subset_sequences": seqs,
            },
            indent=2,
        )
        + "\n"
    )
    print("  all assertions passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Run it**

Run: `uv run --project tools tools/export_stimuli.py`
Expected: prints row/byte counts and `all assertions passed`, exit 0.

If an assert fires, **stop and report it** — a mismatch means the reference differs from what the spec recorded, and nothing downstream can be trusted.

- [ ] **Step 3: Verify the outputs on disk**

Run:
```bash
ls -la data/flyvis-stimuli/
wc -l data/flyvis-stimuli/*.csv
head -3 data/flyvis-stimuli/flashes_args.csv data/flyvis-stimuli/dots_subset_args.csv
```
Expected: `flashes.bin` exactly 6921600 bytes; `dots_subset.bin` exactly 24225600 bytes; `hex_coords.csv` 722 lines (header + 721); `flashes_args.csv` 5 lines; `dots_subset_args.csv` 7 lines.

- [ ] **Step 4: Note the script in `tools/README.md`**

Add a row to the existing Scripts table describing `export_stimuli.py` and its output directory, matching the style of the rows already there.

- [ ] **Step 5: Commit**

```bash
git add tools/export_stimuli.py tools/README.md
git commit -m "Add an exporter for flyvis's rendered flash and dot stimuli"
```

Do **not** add anything under `data/`.

---

### Task 3: The `Stimulus` trait and the hex-disc helpers

**Files:**
- Create: `crates/flyvis/src/stimuli.rs`
- Modify: `crates/flyvis/src/lib.rs` (add `pub mod stimuli;` and a re-export)

**Interfaces:**
- Consumes: `crate::expand::hex_coords` from Task 1.
- Produces:
  - `pub trait Stimulus` with `num_sequences`, `num_frames`, `num_hexals`, `frame_into`
  - `pub enum StimulusError { SequenceOutOfRange { index, count }, FrameOutOfRange { index, count }, BufferLengthMismatch { expected, actual } }` — all fields `usize`
  - `pub const FULL_FIELD: i32 = -1;`
  - `pub(crate) fn hex_distance(a: (i32, i32), b: (i32, i32)) -> i32`
  - `pub(crate) fn disc_indices(extent: i32, centre: (i32, i32), radius: i32) -> Vec<usize>`
  - `pub(crate) fn frame_count(duration: f32, dt: f32) -> usize`

- [ ] **Step 1: Write the failing tests**

Create `crates/flyvis/src/stimuli.rs` containing only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_distance_is_zero_to_itself() {
        assert_eq!(hex_distance((3, -1), (3, -1)), 0);
    }

    #[test]
    fn hex_distance_to_each_neighbour_is_one() {
        for n in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            assert_eq!(hex_distance(n, (0, 0)), 1, "neighbour {n:?}");
        }
    }

    #[test]
    fn hex_distance_matches_hand_computed_values() {
        assert_eq!(hex_distance((2, 0), (0, 0)), 2);
        assert_eq!(hex_distance((0, 2), (0, 0)), 2);
        assert_eq!(hex_distance((2, -2), (0, 0)), 2);
        assert_eq!(hex_distance((-3, 1), (0, 0)), 3);
    }

    /// Hex discs are the centred hexagonal numbers: 1, 7, 19, 37, ...
    /// 3r^2 + 3r + 1. Radius 6 is the 127 flyvis's default flash uses.
    #[test]
    fn disc_sizes_are_the_centred_hexagonal_numbers() {
        for r in 0..=6 {
            let want = (3 * r * r + 3 * r + 1) as usize;
            assert_eq!(disc_indices(15, (0, 0), r).len(), want, "radius {r}");
        }
    }

    #[test]
    fn a_full_extent_disc_is_the_whole_lattice() {
        assert_eq!(disc_indices(15, (0, 0), 15).len(), 721);
    }

    #[test]
    fn a_radius_zero_disc_at_the_origin_is_index_360() {
        assert_eq!(disc_indices(15, (0, 0), 0), vec![360]);
    }

    /// The boundary ring is INCLUDED: `<=`, not `<`.
    #[test]
    fn the_disc_boundary_is_inclusive() {
        let d = disc_indices(15, (0, 0), 1);
        let coords: Vec<(i32, i32)> = crate::expand::hex_coords(15).collect();
        assert!(d.iter().any(|&i| coords[i] == (1, 0)), "a distance-1 hexal must be in a radius-1 disc");
        assert_eq!(d.len(), 7);
    }

    #[test]
    fn a_disc_off_the_origin_is_clipped_by_the_lattice() {
        // Centred on the lattice edge, most of the disc falls outside.
        assert!(disc_indices(15, (15, 0), 2).len() < 19);
    }

    #[test]
    fn disc_indices_are_ascending() {
        let d = disc_indices(15, (0, 0), 4);
        assert!(d.windows(2).all(|w| w[0] < w[1]), "indices must be in lattice order");
    }

    /// flyvis computes `int(t / dt)`, which TRUNCATES toward zero.
    #[test]
    fn frame_count_truncates_rather_than_rounds() {
        assert_eq!(frame_count(1.0, 1.0 / 200.0), 200);
        assert_eq!(frame_count(5.0, 1.0 / 200.0), 1000);
        assert_eq!(frame_count(2.0, 1.0 / 200.0), 400);
        assert_eq!(frame_count(0.0, 1.0 / 200.0), 0);
        // 0.999 / 0.005 = 199.8 -> 199, not 200.
        assert_eq!(frame_count(0.999, 1.0 / 200.0), 199);
    }
}
```

Add to `crates/flyvis/src/lib.rs`, alongside the other `pub mod` lines:

```rust
pub mod stimuli;
```

and extend the re-export line at the bottom of the file with:

```rust
pub use stimuli::{Stimulus, StimulusError, FULL_FIELD};
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis stimuli`
Expected: FAIL to compile — `hex_distance`, `disc_indices`, `frame_count` not found.

- [ ] **Step 3: Write the implementation**

Put this **above** the test module in `crates/flyvis/src/stimuli.rs`:

```rust
//! Visual stimuli, generated to match flyvis's own rendered output.
//!
//! Each stimulus family is a set of sequences; each sequence is a run of frames;
//! each frame is one intensity per hexal -- 721 at the default extent. Which
//! cells receive those intensities is not decided here: a stimulus is a
//! lattice-indexed vector, and mapping it onto photoreceptors belongs to the
//! network layer.
//!
//! Frames are produced on demand rather than materialised. The dot family is
//! 1,442 sequences x 1,400 frames x 721 hexals -- 5.8 GB as `f32` -- and a
//! single frame is what an integrator consumes per step anyway.

use crate::expand::hex_coords;

/// The radius sentinel meaning "the entire receptor array".
///
/// flyvis spells this `-1` in a list of radii and branches on it before any hex
/// geometry runs. It is **not** a distance; treating it as one yields an empty
/// mask rather than a full field.
pub const FULL_FIELD: i32 = -1;

/// Why a stimulus frame could not be produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StimulusError {
    /// No such sequence in this family.
    SequenceOutOfRange { index: usize, count: usize },
    /// No such frame in this sequence.
    FrameOutOfRange { index: usize, count: usize },
    /// The output slice was not `num_hexals()` long.
    BufferLengthMismatch { expected: usize, actual: usize },
}

impl std::fmt::Display for StimulusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StimulusError::SequenceOutOfRange { index, count } =>
                write!(f, "sequence {index} requested, but only {count} exist"),
            StimulusError::FrameOutOfRange { index, count } =>
                write!(f, "frame {index} requested, but only {count} exist"),
            StimulusError::BufferLengthMismatch { expected, actual } =>
                write!(f, "expected a slice of {expected}, got {actual}"),
        }
    }
}

impl std::error::Error for StimulusError {}

/// A family of rendered stimulus sequences, generated on demand.
pub trait Stimulus {
    /// Number of distinct sequences in this family.
    fn num_sequences(&self) -> usize;
    /// Frames per sequence. Uniform across the family.
    fn num_frames(&self) -> usize;
    /// Hexals per frame -- 721 at extent 15.
    fn num_hexals(&self) -> usize;
    /// Writes one frame's intensities into `out`, which must be
    /// [`num_hexals`](Stimulus::num_hexals) long.
    fn frame_into(
        &self,
        sequence: usize,
        frame: usize,
        out: &mut [f32],
    ) -> Result<(), StimulusError>;
}

/// Distance between two axial hex coordinates.
///
/// `(|du| + |du + dv| + |dv|) / 2`. The numerator is always even, so the
/// division is exact and this returns a true integer distance.
pub(crate) fn hex_distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (du, dv) = (a.0 - b.0, a.1 - b.1);
    (du.abs() + (du + dv).abs() + dv.abs()) / 2
}

/// Lattice indices of every hexal within `radius` of `centre`.
///
/// The boundary ring is **included** (`<=`). Returned in ascending lattice
/// order, so the result indexes directly into a frame buffer. A disc centred
/// away from the origin is clipped by the lattice edge, exactly as flyvis's is.
pub(crate) fn disc_indices(extent: i32, centre: (i32, i32), radius: i32) -> Vec<usize> {
    hex_coords(extent)
        .enumerate()
        .filter(|&(_, p)| hex_distance(p, centre) <= radius)
        .map(|(i, _)| i)
        .collect()
}

/// How many frames a duration occupies.
///
/// flyvis computes `int(t / dt)`, which **truncates** toward zero. Rounding
/// instead would add a spurious frame at many duration/step combinations.
pub(crate) fn frame_count(duration: f32, dt: f32) -> usize {
    (duration / dt) as usize
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p semblance-flyvis stimuli`
Expected: PASS, 10 tests.

- [ ] **Step 5: Check clippy**

Run: `cargo clippy -p semblance-flyvis --all-targets`

Expected: **three `dead_code` warnings**, for `hex_distance`, `disc_indices` and `frame_count`. They are `pub(crate)` and used only from `#[cfg(test)]` code until Task 4 consumes them, so a lib build flags them while `cargo test` does not.

**Do not suppress them** with `#[allow(dead_code)]`, and do not widen `pub(crate)` to `pub` to silence them — that would enlarge the crate's API for a bookkeeping reason. They disappear on their own in Task 4, which is the gate that must be clean. `Stimulus`, `StimulusError` and `FULL_FIELD` are `pub` and re-exported, so those three will not warn.

Run: `cargo test -p semblance-flyvis stimuli`
Expected: still PASS, and no `dead_code` warnings in the test build.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/stimuli.rs crates/flyvis/src/lib.rs
git commit -m "Add the stimulus trait and hex-disc helpers"
```

---

### Task 4: Circular ON/OFF flashes

**Files:**
- Modify: `crates/flyvis/src/stimuli.rs`
- Modify: `crates/flyvis/src/lib.rs` (extend the re-export)

**Interfaces:**
- Consumes: `Stimulus`, `StimulusError`, `FULL_FIELD`, `hex_distance`, `disc_indices`, `frame_count` from Task 3; `hex_coords` from Task 1.
- Produces:
  - `pub struct FlashParams { pub extent: i32, pub dynamic_range: [f32; 2], pub t_stim: f32, pub t_pre: f32, pub dt: f32, pub radii: Vec<i32>, pub alternations: Vec<u8> }`
  - `impl FlashParams { pub fn flyvis_defaults(dt: f32) -> Self }`
  - `pub struct Flashes` with `pub fn new(params: FlashParams) -> Self`, `pub fn intensity_of(&self, sequence: usize) -> Option<f32>`, `pub fn radius_of(&self, sequence: usize) -> Option<i32>`
  - `impl Stimulus for Flashes`

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `crates/flyvis/src/stimuli.rs`:

```rust
    fn flashes() -> Flashes {
        Flashes::new(FlashParams::flyvis_defaults(1.0 / 200.0))
    }

    #[test]
    fn flashes_have_flyvis_dimensions() {
        let f = flashes();
        assert_eq!(f.num_sequences(), 4);
        assert_eq!(f.num_frames(), 600, "200 pre + 200 stim + 200 pre");
        assert_eq!(f.num_hexals(), 721);
    }

    /// Ordering is intensity-major, radius-minor -- flyvis builds it with
    /// `product(zip(baseline, intensity), radius)` and `product` varies its
    /// LAST argument fastest. Radius-major would mislabel every sequence
    /// after index 0 while producing arrays of identical shape.
    #[test]
    fn flash_sequences_are_intensity_major() {
        let f = flashes();
        let got: Vec<(f32, i32)> = (0..4)
            .map(|i| (f.intensity_of(i).unwrap(), f.radius_of(i).unwrap()))
            .collect();
        assert_eq!(got, vec![(0.0, -1), (0.0, 6), (1.0, -1), (1.0, 6)]);
    }

    #[test]
    fn a_baseline_frame_is_uniform_grey() {
        let f = flashes();
        let mut out = vec![0.0; 721];
        f.frame_into(3, 0, &mut out).unwrap();
        assert!(out.iter().all(|&x| x == 0.5), "pre-stimulus must be uniform 0.5");
        f.frame_into(3, 599, &mut out).unwrap();
        assert!(out.iter().all(|&x| x == 0.5), "post-stimulus must be uniform 0.5");
    }

    #[test]
    fn a_disc_stimulus_frame_lights_exactly_127_hexals() {
        let f = flashes();
        let mut out = vec![0.0; 721];
        f.frame_into(3, 200, &mut out).unwrap(); // sequence 3 = ON, radius 6
        assert_eq!(out.iter().filter(|&&x| x == 1.0).count(), 127);
        assert_eq!(out.iter().filter(|&&x| x == 0.5).count(), 721 - 127);
        assert_eq!(out[360], 1.0, "the origin is inside the disc");
    }

    #[test]
    fn a_full_field_stimulus_frame_lights_everything() {
        let f = flashes();
        let mut out = vec![0.0; 721];
        f.frame_into(2, 200, &mut out).unwrap(); // sequence 2 = ON, full field
        assert!(out.iter().all(|&x| x == 1.0));
    }

    #[test]
    fn an_off_flash_darkens_rather_than_brightens() {
        let f = flashes();
        let mut out = vec![0.0; 721];
        f.frame_into(1, 200, &mut out).unwrap(); // sequence 1 = OFF, radius 6
        assert_eq!(out.iter().filter(|&&x| x == 0.0).count(), 127);
        assert_eq!(out[360], 0.0);
    }

    #[test]
    fn the_stimulus_block_is_frames_200_to_399() {
        let f = flashes();
        let lit = |frame: usize| {
            let mut o = vec![0.0; 721];
            f.frame_into(3, frame, &mut o).unwrap();
            o[360] == 1.0
        };
        assert!(!lit(199), "frame 199 is still pre-stimulus");
        assert!(lit(200), "frame 200 is the first stimulus frame");
        assert!(lit(399), "frame 399 is the last stimulus frame");
        assert!(!lit(400), "frame 400 is post-stimulus");
    }

    #[test]
    fn flashes_reject_a_bad_sequence_frame_or_buffer() {
        let f = flashes();
        let mut out = vec![0.0; 721];
        assert_eq!(
            f.frame_into(4, 0, &mut out),
            Err(StimulusError::SequenceOutOfRange { index: 4, count: 4 })
        );
        assert_eq!(
            f.frame_into(0, 600, &mut out),
            Err(StimulusError::FrameOutOfRange { index: 600, count: 600 })
        );
        let mut short = vec![0.0; 720];
        assert_eq!(
            f.frame_into(0, 0, &mut short),
            Err(StimulusError::BufferLengthMismatch { expected: 721, actual: 720 })
        );
    }

    #[test]
    fn flash_accessors_return_none_past_the_end() {
        let f = flashes();
        assert_eq!(f.intensity_of(4), None);
        assert_eq!(f.radius_of(4), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis stimuli`
Expected: FAIL to compile — `Flashes`, `FlashParams` not found.

- [ ] **Step 3: Write the implementation**

Add to `crates/flyvis/src/stimuli.rs`, after the helpers and before the test module:

```rust
/// Parameters for a family of circular flashes.
#[derive(Clone, Debug, PartialEq)]
pub struct FlashParams {
    /// Hex radius of the receptor array. 15 gives 721 hexals.
    pub extent: i32,
    /// The two intensity extremes. Their mean is the baseline.
    pub dynamic_range: [f32; 2],
    /// Seconds of stimulus per `1` in `alternations`.
    pub t_stim: f32,
    /// Seconds of baseline per `0` in `alternations`.
    pub t_pre: f32,
    /// Integration step, in seconds.
    pub dt: f32,
    /// Radii to render. [`FULL_FIELD`] means the whole array.
    pub radii: Vec<i32>,
    /// Block sequence: `0` baseline, anything else stimulus.
    pub alternations: Vec<u8>,
}

impl FlashParams {
    /// flyvis's defaults, with `dt` supplied by the caller.
    ///
    /// There is deliberately no default `dt`. The dynamics use
    /// `1 / max(tau, dt)`, so `dt` decides which cell types are floored --
    /// it is a modelling choice, not a convenience. flyvis's own response
    /// analyses use `1.0 / 200.0`.
    pub fn flyvis_defaults(dt: f32) -> Self {
        FlashParams {
            extent: 15,
            dynamic_range: [0.0, 1.0],
            t_stim: 1.0,
            t_pre: 1.0,
            dt,
            radii: vec![FULL_FIELD, 6],
            alternations: vec![0, 1, 0],
        }
    }
}

/// Circular ON/OFF flashes on the hex lattice.
#[derive(Clone, Debug)]
pub struct Flashes {
    n_hexals: usize,
    n_frames: usize,
    baseline: f32,
    /// Per sequence: `(intensity, radius)`, intensity-major.
    sequences: Vec<(f32, i32)>,
    /// Per sequence: lit hexals, or `None` for a full field.
    masks: Vec<Option<Vec<usize>>>,
    /// Per block: `(frame count, is a stimulus block)`.
    blocks: Vec<(usize, bool)>,
}

impl Flashes {
    /// Builds the family. All masks are computed once here.
    pub fn new(params: FlashParams) -> Self {
        let n_hexals = hex_coords(params.extent).count();
        let baseline = (params.dynamic_range[0] + params.dynamic_range[1]) / 2.0;

        // Intensity-major, radius-minor: flyvis uses
        // `product(zip(baseline, intensity), radius)`, and `product` varies its
        // last argument fastest.
        let mut sequences = Vec::new();
        let mut masks = Vec::new();
        for &intensity in &params.dynamic_range {
            for &radius in &params.radii {
                sequences.push((intensity, radius));
                masks.push(if radius == FULL_FIELD {
                    None
                } else {
                    Some(disc_indices(params.extent, (0, 0), radius))
                });
            }
        }

        let n_pre = frame_count(params.t_pre, params.dt);
        let n_stim = frame_count(params.t_stim, params.dt);
        let blocks: Vec<(usize, bool)> = params
            .alternations
            .iter()
            .map(|&a| if a == 0 { (n_pre, false) } else { (n_stim, true) })
            .collect();
        let n_frames = blocks.iter().map(|b| b.0).sum();

        Flashes { n_hexals, n_frames, baseline, sequences, masks, blocks }
    }

    /// The flash intensity for a sequence.
    pub fn intensity_of(&self, sequence: usize) -> Option<f32> {
        self.sequences.get(sequence).map(|s| s.0)
    }

    /// The radius for a sequence. [`FULL_FIELD`] means the whole array.
    pub fn radius_of(&self, sequence: usize) -> Option<i32> {
        self.sequences.get(sequence).map(|s| s.1)
    }
}

impl Stimulus for Flashes {
    fn num_sequences(&self) -> usize {
        self.sequences.len()
    }

    fn num_frames(&self) -> usize {
        self.n_frames
    }

    fn num_hexals(&self) -> usize {
        self.n_hexals
    }

    fn frame_into(
        &self,
        sequence: usize,
        frame: usize,
        out: &mut [f32],
    ) -> Result<(), StimulusError> {
        if sequence >= self.sequences.len() {
            return Err(StimulusError::SequenceOutOfRange {
                index: sequence,
                count: self.sequences.len(),
            });
        }
        if frame >= self.n_frames {
            return Err(StimulusError::FrameOutOfRange { index: frame, count: self.n_frames });
        }
        if out.len() != self.n_hexals {
            return Err(StimulusError::BufferLengthMismatch {
                expected: self.n_hexals,
                actual: out.len(),
            });
        }

        let mut remaining = frame;
        let mut lit = false;
        for &(len, is_stim) in &self.blocks {
            if remaining < len {
                lit = is_stim;
                break;
            }
            remaining -= len;
        }

        out.fill(self.baseline);
        if lit {
            let intensity = self.sequences[sequence].0;
            match &self.masks[sequence] {
                None => out.fill(intensity),
                Some(indices) => {
                    for &i in indices {
                        out[i] = intensity;
                    }
                }
            }
        }
        Ok(())
    }
}
```

Extend the re-export in `crates/flyvis/src/lib.rs`:

```rust
pub use stimuli::{FlashParams, Flashes, Stimulus, StimulusError, FULL_FIELD};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p semblance-flyvis stimuli`
Expected: PASS, 19 tests.

- [ ] **Step 5: Confirm the dead_code warnings have cleared**

Run: `cargo clippy -p semblance-flyvis --all-targets`
Expected: **zero** `dead_code` warnings — `Flashes` now consumes `hex_distance`, `disc_indices` and `frame_count`. If any remain, something this task was meant to wire up is not wired up; investigate rather than suppressing it.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/stimuli.rs crates/flyvis/src/lib.rs
git commit -m "Generate flyvis's circular ON/OFF flashes"
```

---

### Task 5: Single-ommatidium dots

**Files:**
- Modify: `crates/flyvis/src/stimuli.rs`
- Modify: `crates/flyvis/src/lib.rs` (extend the re-export)

**Interfaces:**
- Consumes: everything from Tasks 3 and 4.
- Produces:
  - `pub struct DotParams { pub extent: i32, pub dot_column_radius: i32, pub bg_intensity: f32, pub intensity: f32, pub t_stim: f32, pub t_pre: f32, pub t_post: f32, pub dt: f32 }`
  - `impl DotParams { pub fn flyvis_defaults(dt: f32) -> Self }`
  - `pub struct Dots` with `pub fn new(params: DotParams) -> Self`, `pub fn centre_of(&self, sequence: usize) -> Option<(i32, i32)>`, `pub fn intensity_of(&self, sequence: usize) -> Option<f32>`
  - `impl Stimulus for Dots`

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `crates/flyvis/src/stimuli.rs`:

```rust
    fn dots() -> Dots {
        Dots::new(DotParams::flyvis_defaults(1.0 / 200.0))
    }

    #[test]
    fn dots_have_flyvis_dimensions() {
        let d = dots();
        assert_eq!(d.num_sequences(), 1442, "721 centres x 2 polarities");
        assert_eq!(d.num_frames(), 1400, "400 pre + 1000 stim + 0 post");
        assert_eq!(d.num_hexals(), 721);
    }

    /// Ordering is centre-major, intensity-minor -- the OPPOSITE grouping from
    /// flashes, though both follow the same `product`-varies-last rule. flyvis
    /// uses `product(zip(u, v, offsets, coordinate_indices), intensities)`.
    #[test]
    fn dot_sequences_are_centre_major() {
        let d = dots();
        assert_eq!(d.centre_of(0), Some((-15, 0)));
        assert_eq!(d.centre_of(1), Some((-15, 0)), "same centre, other polarity");
        assert_eq!(d.centre_of(2), Some((-15, 1)), "next centre");
        assert_eq!(d.centre_of(720), Some((0, 0)), "centre 360 is the origin");
        assert_eq!(d.centre_of(1441), Some((15, 0)), "last centre, bright");
    }

    #[test]
    fn dot_polarities_are_dark_then_bright() {
        let d = dots();
        assert_eq!(d.intensity_of(0), Some(0.0), "2 * 0.5 - 1.0 = 0.0, the dark flash");
        assert_eq!(d.intensity_of(1), Some(1.0), "the bright flash");
        assert_eq!(d.intensity_of(720), Some(0.0));
        assert_eq!(d.intensity_of(721), Some(1.0));
    }

    #[test]
    fn a_dot_lights_exactly_one_hexal() {
        let d = dots();
        let mut out = vec![0.0; 721];
        d.frame_into(720, 400, &mut out).unwrap(); // origin centre, dark, first stim frame
        assert_eq!(out.iter().filter(|&&x| x != 0.5).count(), 1);
        assert_eq!(out[360], 0.0, "the origin hexal is the dark dot");
    }

    #[test]
    fn a_bright_dot_lights_the_same_hexal() {
        let d = dots();
        let mut out = vec![0.0; 721];
        d.frame_into(721, 400, &mut out).unwrap(); // origin centre, bright
        assert_eq!(out[360], 1.0);
        assert_eq!(out.iter().filter(|&&x| x == 0.5).count(), 720);
    }

    #[test]
    fn the_dot_stimulus_block_is_frames_400_to_1399() {
        let d = dots();
        let lit = |frame: usize| {
            let mut o = vec![0.0; 721];
            d.frame_into(721, frame, &mut o).unwrap();
            o[360] == 1.0
        };
        assert!(!lit(399), "frame 399 is still pre-stimulus");
        assert!(lit(400), "frame 400 is the first stimulus frame");
        assert!(lit(1399), "frame 1399 is the last, since t_post is 0");
    }

    #[test]
    fn a_dot_pre_stimulus_frame_is_uniform_background() {
        let d = dots();
        let mut out = vec![0.0; 721];
        d.frame_into(721, 0, &mut out).unwrap();
        assert!(out.iter().all(|&x| x == 0.5));
    }

    /// A wider dot lights a filled disc, using the same inclusive rule as flashes.
    #[test]
    fn a_wider_dot_lights_a_filled_disc() {
        let mut p = DotParams::flyvis_defaults(1.0 / 200.0);
        p.dot_column_radius = 1;
        let d = Dots::new(p);
        let mut out = vec![0.0; 721];
        d.frame_into(721, 400, &mut out).unwrap();
        assert_eq!(out.iter().filter(|&&x| x == 1.0).count(), 7);
    }

    #[test]
    fn dots_reject_a_bad_sequence_frame_or_buffer() {
        let d = dots();
        let mut out = vec![0.0; 721];
        assert_eq!(
            d.frame_into(1442, 0, &mut out),
            Err(StimulusError::SequenceOutOfRange { index: 1442, count: 1442 })
        );
        assert_eq!(
            d.frame_into(0, 1400, &mut out),
            Err(StimulusError::FrameOutOfRange { index: 1400, count: 1400 })
        );
        let mut short = vec![0.0; 100];
        assert_eq!(
            d.frame_into(0, 0, &mut short),
            Err(StimulusError::BufferLengthMismatch { expected: 721, actual: 100 })
        );
    }

    #[test]
    fn dot_accessors_return_none_past_the_end() {
        let d = dots();
        assert_eq!(d.centre_of(1442), None);
        assert_eq!(d.intensity_of(1442), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis stimuli`
Expected: FAIL to compile — `Dots`, `DotParams` not found.

- [ ] **Step 3: Write the implementation**

Add to `crates/flyvis/src/stimuli.rs`, after `Flashes` and before the test module:

```rust
/// Parameters for a family of single-ommatidium dot flashes.
#[derive(Clone, Debug, PartialEq)]
pub struct DotParams {
    /// Hex radius of the receptor array. 15 gives 721 hexals.
    pub extent: i32,
    /// Radius of the lit disc around each centre. 0 lights one hexal.
    pub dot_column_radius: i32,
    /// Background intensity.
    pub bg_intensity: f32,
    /// The bright flash intensity. The dark one is its mirror below background.
    pub intensity: f32,
    /// Seconds the dot is shown.
    pub t_stim: f32,
    /// Seconds of background before the dot.
    pub t_pre: f32,
    /// Seconds of background after the dot.
    pub t_post: f32,
    /// Integration step, in seconds.
    pub dt: f32,
}

impl DotParams {
    /// flyvis's defaults, with `dt` supplied by the caller. See
    /// [`FlashParams::flyvis_defaults`] for why `dt` has no default.
    pub fn flyvis_defaults(dt: f32) -> Self {
        DotParams {
            extent: 15,
            dot_column_radius: 0,
            bg_intensity: 0.5,
            intensity: 1.0,
            t_stim: 5.0,
            t_pre: 2.0,
            t_post: 0.0,
            dt,
        }
    }
}

/// Single-ommatidium flashes: every lattice position, both polarities.
#[derive(Clone, Debug)]
pub struct Dots {
    n_hexals: usize,
    n_frames: usize,
    n_pre: usize,
    n_stim: usize,
    bg_intensity: f32,
    /// Lattice coordinate of each centre, in lattice order.
    centres: Vec<(i32, i32)>,
    /// Lit hexals per centre.
    masks: Vec<Vec<usize>>,
    /// `[dark, bright]`.
    intensities: [f32; 2],
}

impl Dots {
    /// Builds the family. All 721 masks are computed once here.
    pub fn new(params: DotParams) -> Self {
        let centres: Vec<(i32, i32)> = hex_coords(params.extent).collect();
        let n_hexals = centres.len();
        let masks: Vec<Vec<usize>> = centres
            .iter()
            .map(|&c| disc_indices(params.extent, c, params.dot_column_radius))
            .collect();

        let n_pre = frame_count(params.t_pre, params.dt);
        let n_stim = frame_count(params.t_stim, params.dt);
        let n_post = frame_count(params.t_post, params.dt);

        Dots {
            n_hexals,
            n_frames: n_pre + n_stim + n_post,
            n_pre,
            n_stim,
            bg_intensity: params.bg_intensity,
            centres,
            masks,
            // flyvis: `[2 * bg_intensity - intensity, intensity]` -- a dark
            // flash mirrored below background, then a bright one.
            intensities: [
                2.0 * params.bg_intensity - params.intensity,
                params.intensity,
            ],
        }
    }

    /// The lit centre for a sequence.
    pub fn centre_of(&self, sequence: usize) -> Option<(i32, i32)> {
        self.centres.get(sequence / 2).copied()
    }

    /// The flash intensity for a sequence: dark for even, bright for odd.
    pub fn intensity_of(&self, sequence: usize) -> Option<f32> {
        if sequence >= self.num_sequences() {
            return None;
        }
        Some(self.intensities[sequence % 2])
    }
}

impl Stimulus for Dots {
    fn num_sequences(&self) -> usize {
        self.centres.len() * 2
    }

    fn num_frames(&self) -> usize {
        self.n_frames
    }

    fn num_hexals(&self) -> usize {
        self.n_hexals
    }

    fn frame_into(
        &self,
        sequence: usize,
        frame: usize,
        out: &mut [f32],
    ) -> Result<(), StimulusError> {
        if sequence >= self.num_sequences() {
            return Err(StimulusError::SequenceOutOfRange {
                index: sequence,
                count: self.num_sequences(),
            });
        }
        if frame >= self.n_frames {
            return Err(StimulusError::FrameOutOfRange { index: frame, count: self.n_frames });
        }
        if out.len() != self.n_hexals {
            return Err(StimulusError::BufferLengthMismatch {
                expected: self.n_hexals,
                actual: out.len(),
            });
        }

        out.fill(self.bg_intensity);
        if frame >= self.n_pre && frame < self.n_pre + self.n_stim {
            let intensity = self.intensities[sequence % 2];
            for &i in &self.masks[sequence / 2] {
                out[i] = intensity;
            }
        }
        Ok(())
    }
}
```

Extend the re-export in `crates/flyvis/src/lib.rs`:

```rust
pub use stimuli::{
    DotParams, Dots, FlashParams, Flashes, Stimulus, StimulusError, FULL_FIELD,
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p semblance-flyvis stimuli`
Expected: PASS, 29 tests.

- [ ] **Step 5: Check the whole workspace**

Run:
```bash
cargo test --workspace
cargo clippy --workspace --all-targets
```
Expected: all pass; only the 5 documented pre-existing clippy warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/stimuli.rs crates/flyvis/src/lib.rs
git commit -m "Generate flyvis's single-ommatidium dot stimuli"
```

---

### Task 6: Fixture-gated acceptance tests

**Files:**
- Create: `crates/flyvis/src/stimuli_checks.rs`
- Modify: `crates/flyvis/src/test_support.rs` (add two loaders)
- Modify: `crates/flyvis/src/lib.rs` (add the test module)
- Modify: `CLAUDE.md` (test and skip counts)

**Interfaces:**
- Consumes: everything above.
- Produces: nothing consumed later.

- [ ] **Step 1: Add the fixture loaders**

Append to `crates/flyvis/src/test_support.rs`, following the existing loaders exactly — including the `fixtures_required()` assert, which must come **before** the `eprintln!` and the `None`:

```rust
/// Loads a stimulus fixture exported from flyvis, or `None` if the export has
/// not been run. See `tools/export_stimuli.py`.
pub(crate) fn load_stimulus_fixture(name: &str) -> Option<String> {
    let path = format!(
        "{}/../../data/flyvis-stimuli/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_stimuli.py` to generate it."
            );
            None
        }
    }
}

/// Loads a binary stimulus fixture, or `None` if the export has not been run.
pub(crate) fn load_stimulus_bytes(name: &str) -> Option<Vec<u8>> {
    let path = format!(
        "{}/../../data/flyvis-stimuli/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_stimuli.py` to generate it."
            );
            None
        }
    }
}
```

- [ ] **Step 2: Write the failing acceptance tests**

Create `crates/flyvis/src/stimuli_checks.rs`:

```rust
//! Acceptance criteria for the stimulus generators.
//!
//! These compare against flyvis's own rendered arrays (`tools/export_stimuli.py`)
//! rather than against our own derivation. Two orderings decide correctness --
//! the 721-hexal lattice order and the sequence order within each family -- and
//! neither is announced by the arrays themselves.
//!
//! All of these skip when their fixtures are absent, and the skip notice is
//! invisible under a plain `cargo test`. Set `SEMBLANCE_REQUIRE_FIXTURES=1` to
//! turn a missing fixture into a failure; see `test_support::fixtures_required`.

use crate::expand::hex_coords;
use crate::stimuli::{DotParams, Dots, FlashParams, Flashes, Stimulus};
use crate::test_support::{load_stimulus_bytes, load_stimulus_fixture};

const DT: f32 = 1.0 / 200.0;

fn f32s(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// The lattice order, against flyvis's own.
#[test]
fn hex_coords_match_flyvis() {
    let Some(csv) = load_stimulus_fixture("hex_coords.csv") else { return };
    let want: Vec<(i32, i32)> = csv
        .lines()
        .skip(1)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            (f[1].parse().unwrap(), f[2].trim().parse().unwrap())
        })
        .collect();
    assert_eq!(want.len(), 721, "fixture row count");
    let got: Vec<(i32, i32)> = hex_coords(15).collect();
    assert_eq!(got, want);
}

/// Every flash frame, against flyvis's rendered array.
#[test]
fn flashes_match_flyvis() {
    let Some(bytes) = load_stimulus_bytes("flashes.bin") else { return };
    let want = f32s(&bytes);
    assert_eq!(want.len(), 4 * 600 * 721, "fixture value count");

    let f = Flashes::new(FlashParams::flyvis_defaults(DT));
    assert_eq!(f.num_sequences(), 4);
    assert_eq!(f.num_frames(), 600);
    assert_eq!(f.num_hexals(), 721);

    let mut out = vec![0.0f32; 721];
    for s in 0..4 {
        for fr in 0..600 {
            f.frame_into(s, fr, &mut out).unwrap();
            let base = (s * 600 + fr) * 721;
            for (h, &got) in out.iter().enumerate() {
                let w = want[base + h];
                assert!(
                    (got - w).abs() < 1e-6,
                    "sequence {s}, frame {fr}, hexal {h}: got {got}, flyvis has {w}"
                );
            }
        }
    }
}

/// The flash sequence ordering, against flyvis's own `arg_df`.
#[test]
fn flash_ordering_matches_flyvis() {
    let Some(csv) = load_stimulus_fixture("flashes_args.csv") else { return };
    let f = Flashes::new(FlashParams::flyvis_defaults(DT));
    let rows: Vec<(f32, i32)> = csv
        .lines()
        .skip(1)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            (c[2].parse().unwrap(), c[3].trim().parse().unwrap())
        })
        .collect();
    assert_eq!(rows.len(), 4);
    for (i, &(intensity, radius)) in rows.iter().enumerate() {
        assert_eq!(f.intensity_of(i), Some(intensity), "intensity of sequence {i}");
        assert_eq!(f.radius_of(i), Some(radius), "radius of sequence {i}");
    }
}

/// The exported dot subset, frame for frame.
#[test]
fn dot_subset_matches_flyvis() {
    let (Some(bytes), Some(csv)) = (
        load_stimulus_bytes("dots_subset.bin"),
        load_stimulus_fixture("dots_subset_args.csv"),
    ) else {
        return;
    };
    let want = f32s(&bytes);
    let seqs: Vec<usize> = csv
        .lines()
        .skip(1)
        .filter(|l| !l.is_empty())
        .map(|l| l.split(',').nth(1).unwrap().parse().unwrap())
        .collect();
    assert_eq!(seqs.len(), 6, "subset size");
    assert_eq!(want.len(), 6 * 1400 * 721, "fixture value count");

    let d = Dots::new(DotParams::flyvis_defaults(DT));
    assert_eq!(d.num_sequences(), 1442);
    assert_eq!(d.num_frames(), 1400);

    let mut out = vec![0.0f32; 721];
    for (order, &s) in seqs.iter().enumerate() {
        for fr in 0..1400 {
            d.frame_into(s, fr, &mut out).unwrap();
            let base = (order * 1400 + fr) * 721;
            for (h, &got) in out.iter().enumerate() {
                let w = want[base + h];
                assert!(
                    (got - w).abs() < 1e-6,
                    "sequence {s}, frame {fr}, hexal {h}: got {got}, flyvis has {w}"
                );
            }
        }
    }
}

/// The dot sequence ordering, against flyvis's own `arg_df`.
#[test]
fn dot_ordering_matches_flyvis() {
    let Some(csv) = load_stimulus_fixture("dots_subset_args.csv") else { return };
    let d = Dots::new(DotParams::flyvis_defaults(DT));
    let mut checked = 0;
    for line in csv.lines().skip(1).filter(|l| !l.is_empty()) {
        let c: Vec<&str> = line.split(',').collect();
        let seq: usize = c[1].parse().unwrap();
        let u: i32 = c[2].parse().unwrap();
        let v: i32 = c[3].parse().unwrap();
        let intensity: f32 = c[5].trim().parse().unwrap();
        assert_eq!(d.centre_of(seq), Some((u, v)), "centre of sequence {seq}");
        assert_eq!(d.intensity_of(seq), Some(intensity), "intensity of sequence {seq}");
        checked += 1;
    }
    assert_eq!(checked, 6);
}
```

Add to `crates/flyvis/src/lib.rs`, beside the other `#[cfg(test)]` modules:

```rust
#[cfg(test)]
mod stimuli_checks;
```

- [ ] **Step 3: Run them to verify they fail, then pass**

Run: `cargo test -p semblance-flyvis stimuli_checks -- --nocapture`

Expected: they compile and PASS against the real fixtures, with **no `skipping:` lines** — Task 2 generated the data, so these must execute.

These are large comparisons: 1.7M values for flashes and 6.1M for the dot subset. In a debug build that takes a while and is not a hang. If it is unreasonably slow, run `cargo test -p semblance-flyvis --release stimuli_checks`.

If a comparison fails, the message names the sequence, frame and hexal. **Stop and report it.** Do not widen the tolerance or reorder a sequence list to force a pass.

- [ ] **Step 4: Verify the gates fail loudly without data**

Run:
```bash
mv data/flyvis-stimuli /tmp/stim-hold
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis stimuli_checks
mv /tmp/stim-hold data/flyvis-stimuli
```
Expected: the middle command FAILS, with each failure naming the missing path. Then restore and confirm everything passes again. Record both outcomes in your report — a gate never observed failing has not been verified.

- [ ] **Step 5: Run the whole workspace**

Run:
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run --quiet -- roundtrip
```
Expected: all pass; `roundtrip` prints exactly `0.000000` for both distances; clippy shows only the 5 pre-existing warnings.

- [ ] **Step 6: Update the documented counts in `CLAUDE.md`**

`CLAUDE.md` currently says `111 tests, 0 failures` and `16 of those tests skip`. Both change.

**Read the new numbers from actual output, do not compute them.** For the test count, take it from `cargo test --workspace`. For the skip count, move the three data directories aside, run with `-- --nocapture --test-threads=1`, count the distinct tests that print `skipping:`, then restore and confirm the suite passes again.

Also add `data/flyvis-stimuli/` to the sentence listing the gitignored data directories, alongside `data/flyvis-supplementary/` and `data/flyvis-build/`.

- [ ] **Step 7: Mark the epic tasks done**

In `docs/project-outline/0.2.0-flyvis-import/task-8-stimulus-set/README.md`, mark 8a and 8b complete with their commit hashes. This file is gitignored — edit it, but do not `git add` it.

- [ ] **Step 8: Commit**

```bash
git add crates/flyvis/src/stimuli_checks.rs crates/flyvis/src/test_support.rs crates/flyvis/src/lib.rs CLAUDE.md
git commit -m "Check the generated stimuli against flyvis's rendered output"
```
