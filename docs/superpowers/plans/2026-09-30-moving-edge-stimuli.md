# Moving Edge Stimuli Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate flyvis's 144 `MovingEdge` sequences in Rust by rendering the bar
analytically — rotating the sampling geometry instead of the raster — and gate the
result against flyvis's own output at a measured tolerance.

**Architecture:** Three layers. `hex_eye` builds the 775x775 monitor and decides
which ommatidium owns each pixel (measured: at most one, so a single `u16` per
pixel suffices). `moving_edge` inverse-rotates each pixel about the monitor centre,
tests membership of the unrotated bar, and counts bar pixels per ommatidium — 21
offsets per angle, the only geometric work. Timing then derives all six speeds from
those 21 offsets by truncating-index resampling and three distinct pads.

**Tech Stack:** Rust 1.85 / edition 2024, no external crates. Offline Python (`uv`,
`tools/`) for fixture export only.

**Spec:** `docs/superpowers/specs/2026-09-30-moving-edge-stimuli-design.md`

**Survey (background, gitignored):**
`docs/project-outline/0.2.0-flyvis-import/task-8-stimulus-set/survey-moving-edges.md`

## Global Constraints

- **Zero external dependencies.** `Cargo.lock` contains exactly 8 packages, all
  `semblance*`. Do not add a crates.io dependency, not even for tests.
- **`tools/` is offline Python, run by hand.** No crate imports it, invokes it, or
  records it in `Cargo.lock`.
- **Rust 1.85+, edition 2024.** `gen` is a reserved keyword.
- **Commit style: plain imperative subject, no `feat:`/`fix:`/`chore:` prefixes.**
- **Do not run `cargo fmt`.**
- **`data/` and `docs/project-outline/` are gitignored — never `git add` them.**
- **All fixture tests honour `SEMBLANCE_REQUIRE_FIXTURES=1`** via
  `test_support::load_stimulus_fixture` / `load_stimulus_bytes`, which already
  implement the skip-or-fail rule. Never write a fixture test that skips silently
  by any other route.
- **`dt` is always a caller argument, never a constant.** flyvis's own response
  analyses use `1/200`; that is a default for a constructor helper, not a global.
- **Angles are degrees** in the public API and in flyvis; convert to radians only
  inside the rotation.
- **`led_width = radians(2.25)` is the spatial unit** for widths, offsets and
  height. **`omm_width = radians(5.8)` appears only in the speed-to-time
  conversion.** Using `omm_width` spatially scales every offset by 5.8/2.25 = 2.6x
  and still produces a plausible moving edge.
- Verify with: `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets`, `cargo run --quiet -- roundtrip`
  (both distances must read exactly `0.000000`).
- Clippy has 5 pre-existing warnings (4 `needless_range_loop` in `sim`, 1 manual
  char comparison in `connectome`). Do not count those as new; do not introduce
  more.

---

## Numbers verified before this plan was written

Every figure below was produced by running the installed flyvis 1.2.0 in
`tools/.venv`, not inferred from reading. Tasks assert them. **If an assertion
here fails, that is a finding to report — not a constant to update.**

**Monitor and eye** (`HexEye(721, 25)`):

| quantity | value |
|---|---|
| `n_hex_circumference` | 31 (= `2*extent + 1`) |
| monitor | 775 x 775 px = 600,625 pixels |
| `dist_w`, `dist_h` | 25.0 each (= `ppo`) |
| `dist_to_edge` | 12.5 |
| `dist_to_vertex` | 14.433756729740645 |
| hexal centres | x = `25*v + 387`, y = `25*(u + v/2) + 387`; both span 12.0 .. 762.0 |

**Pixel ownership**, this implementation's f64 arithmetic:

| quantity | value |
|---|---|
| pixels owned by exactly one hexal | 383,613 |
| pixels owned by **two or more** | **0** |
| pixels owned by none | 217,012 (36% of the monitor) |
| `kernel_sum` min / max / sum | 518 / 541 / 383,613 |

flyvis's own mask, built in **fp16**, gives 382,383 / 0 / 218,242 and min 513,
max 545. 2,052 of 600,625 pixels are assigned differently (max 12 per hexal).
That divergence is real and is one of the three components the tolerance gate
measures. **The load-bearing fact is that neither mask ever gives a pixel two
owners** — that, not the exact counts, is what makes one `u16` per pixel valid.

**Bar geometry** (`MovingEdge`: `widths=[80]`, `height=9`,
`bar_loc_horizontal=radians(0)`, `bar_loc_vertical=radians(90)`, `n_bars=1`):

| quantity | value |
|---|---|
| `bar_width_px` | 775 (the full monitor width) |
| `bar_height_px` | 87 |
| `bar_loc_vertical_px` | 387 |
| row band | `[343, 431)` — 88 rows |
| `bar_spacing` | 0 |

Per offset, columns `[start, stop)` with `start = max(loc_w, 0)`:

| offset | h_px | loc_w | cols | offset | h_px | loc_w | cols |
|---|---|---|---|---|---|---|---|
| -10 | -96 | -483 | [0, 293) | 1 | 9 | -378 | [0, 398) |
| -9 | -87 | -474 | [0, 302) | 2 | 19 | -368 | [0, 408) |
| -8 | -77 | -464 | [0, 312) | 3 | 29 | -358 | [0, 418) |
| -7 | -67 | -454 | [0, 322) | 4 | 38 | -349 | [0, 427) |
| -6 | -58 | -445 | [0, 331) | 5 | 48 | -339 | [0, 437) |
| -5 | -48 | -435 | [0, 341) | 6 | 58 | -329 | [0, 447) |
| -4 | -38 | -425 | [0, 351) | 7 | 67 | -320 | [0, 456) |
| -3 | -29 | -416 | [0, 360) | 8 | 77 | -310 | [0, 466) |
| -2 | -19 | -406 | [0, 370) | 9 | 87 | -300 | [0, 476) |
| -1 | -9 | -396 | [0, 380) | 10 | 96 | -291 | [0, 485) |
| 0 | 0 | -387 | [0, 389) | | | | |

**Timing** at `dt = 1/200`, `t_pre = t_post = 1.0`:

| speed | t_stim | resampled | pre | post | NaN tail | total |
|---|---|---|---|---|---|---|
| 2.4 | 3.394397 | 678 | 200 | 200 | 0 | 1078 |
| 4.8 | 1.697198 | 339 | 200 | 200 | 339 | 1078 |
| 9.7 | 0.839851 | 167 | 200 | 200 | 511 | 1078 |
| 13 | 0.626658 | 125 | 200 | 200 | 553 | 1078 |
| 19 | 0.428766 | 85 | 200 | 200 | 593 | 1078 |
| 25 | 0.325862 | 65 | 200 | 200 | 613 | 1078 |

`t_stim_max = 3.394397`; every sequence is 1078 frames.

**The rotation centre — the trap this plan exists to avoid.** Pillow samples the
output pixel at `x + 0.5` and its bilinear filter subtracts 0.5 again, so the
effective rotation centre in *index* space is `monitor/2 - 0.5 = 387.0`, not
`387.5`. Both were measured end-to-end against flyvis's own `render_bar`:

| rotation centre | max abs diff | hexals over 0.01 |
|---|---|---|
| **387.0 (correct)** | **0.00306** | **0** |
| 387.5 (plausible, wrong) | 0.03185 | up to 39 |

The wrong centre still renders a perfectly plausible moving edge. This is the
project's dominant failure mode — right shape, right count, every value attached
to the wrong thing — in its purest form.

**Expected gate distribution**, measured across angles 0/30/90/150/210, offsets
-10/3/10, both intensities, with the centre at 387.0: **max 0.00306, mean
~0.0018, zero hexals over 0.005.** The mean is a near-uniform floor of
`0.5 - 127/255 = 0.00196` — flyvis's background after its truncating cast to
`uint8`. That floor is expected and is not a defect.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/flyvis/src/hex_eye.rs` | **new.** Monitor geometry: hexal centres in pixel coordinates, the point-in-hexagon test, and the pixel-to-hexal owner map. Knows nothing about bars or time. |
| `crates/flyvis/src/moving_edge.rs` | **new.** Bar geometry in pixels, inverse rotation, per-offset coverage counts, the resample/pad timing, and the `Stimulus` impl. |
| `crates/flyvis/src/moving_edge_checks.rs` | **new.** Acceptance gates against flyvis's exported fixtures. `#[cfg(test)]` only. |
| `crates/flyvis/src/stimuli.rs` | **modify.** Widen `frame_count` to `f64`. |
| `crates/flyvis/src/test_support.rs` | **modify.** Name the new export script in the two stimulus skip messages. |
| `crates/flyvis/src/lib.rs` | **modify.** Declare the new modules, re-export the new public types. |
| `tools/export_moving_edges.py` | **new.** Export flyvis's rendered offsets, its owner map, and its timing as fixtures. |
| `tools/README.md` | **modify.** One row for the new script. |
| `CLAUDE.md` | **modify.** Test counts, the third export script, and the one-tolerance-gate statement. |

---

### Task 1: Hexal centres and the monitor

**Files:**
- Create: `crates/flyvis/src/hex_eye.rs`
- Modify: `crates/flyvis/src/lib.rs`

**Interfaces:**
- Consumes: `crate::expand::hex_coords(extent: i32) -> impl Iterator<Item = (i32, i32)>`
  (already exists, `pub(crate)`).
- Produces:
  - `pub struct HexEye`
  - `pub fn HexEye::new(extent: i32, ppo: usize) -> HexEye`
  - `pub fn HexEye::monitor_px(&self) -> usize`
  - `pub fn HexEye::num_hexals(&self) -> usize`
  - `pub fn HexEye::centre(&self, hexal: usize) -> Option<(f64, f64)>` — `(x, y)`
    = `(column, row)`
  - `pub fn HexEye::dist_to_edge(&self) -> f64`
  - `pub fn HexEye::dist_to_vertex(&self) -> f64`

- [ ] **Step 1: Write the failing tests**

Create `crates/flyvis/src/hex_eye.rs` with the module doc, then this test module
at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn eye() -> HexEye {
        HexEye::new(15, 25)
    }

    /// 721 ommatidia tile a lattice 31 across, and flyvis sizes the monitor
    /// `ppo * n_hex_circumference`.
    #[test]
    fn the_default_monitor_is_775_square() {
        let e = eye();
        assert_eq!(e.monitor_px(), 775);
        assert_eq!(e.num_hexals(), 721);
    }

    /// flyvis derives the circumference from the ommatidium count:
    /// `2 * (-1/2 + sqrt(1/4 - (1 - n)/3)) + 1`. We take it from the extent as
    /// `2 * extent + 1`. Both must agree, or the monitor is the wrong size.
    #[test]
    fn the_circumference_agrees_with_flyviss_derivation() {
        for extent in 1..=20i32 {
            let n = (3 * extent * extent + 3 * extent + 1) as f64;
            let flyvis = 2.0 * (-0.5 + (0.25 - (1.0 - n) / 3.0).sqrt()) + 1.0;
            assert!(
                (flyvis - (2 * extent + 1) as f64).abs() < 1e-9,
                "extent {extent}: flyvis says {flyvis}"
            );
        }
    }

    /// `dist_w = dist_h = monitor / (2 * extent + 1)`, which reduces to `ppo`.
    #[test]
    fn the_hexal_pitch_is_one_ommatidium_of_pixels() {
        let e = eye();
        assert_eq!(e.dist_to_edge(), 12.5, "(dist_w + dist_h) / 4");
        assert!((e.dist_to_vertex() - 14.433756729740645).abs() < 1e-12);
    }

    /// Centres are in `hex_coords` order, so a hexal index means the same thing
    /// here as everywhere else in the crate. Index 360 is the origin.
    #[test]
    fn centres_are_in_lattice_order_with_the_origin_at_index_360() {
        let e = eye();
        assert_eq!(e.centre(0), Some((25.0 * 0.0 + 387.0, 25.0 * -15.0 + 387.0)));
        assert_eq!(e.centre(360), Some((387.0, 387.0)), "the origin hexal");
        assert_eq!(e.centre(720), Some((387.0, 762.0)));
        assert_eq!(e.centre(721), None);
    }

    /// x = dist_w * v + monitor/2, y = dist_h * (u + v/2) + monitor/2, with
    /// INTEGER division for the centring shift -- 387, not 387.5.
    #[test]
    fn centres_follow_flyviss_closed_form() {
        let e = eye();
        for (i, (u, v)) in crate::expand::hex_coords(15).enumerate() {
            let want = (
                25.0 * v as f64 + 387.0,
                25.0 * (u as f64 + v as f64 / 2.0) + 387.0,
            );
            let got = e.centre(i).unwrap();
            assert!(
                (got.0 - want.0).abs() < 1e-12 && (got.1 - want.1).abs() < 1e-12,
                "hexal {i} at ({u}, {v}): got {got:?}, want {want:?}"
            );
        }
    }

    /// The lattice spans 12.0 .. 762.0 in both axes -- inset by half an
    /// ommatidium from each monitor edge.
    #[test]
    fn the_lattice_spans_12_to_762_in_both_axes() {
        let e = eye();
        let (mut lo_x, mut hi_x, mut lo_y, mut hi_y) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for i in 0..e.num_hexals() {
            let (x, y) = e.centre(i).unwrap();
            lo_x = lo_x.min(x);
            hi_x = hi_x.max(x);
            lo_y = lo_y.min(y);
            hi_y = hi_y.max(y);
        }
        assert_eq!((lo_x, hi_x), (12.0, 762.0));
        assert_eq!((lo_y, hi_y), (12.0, 762.0));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-flyvis hex_eye`
Expected: FAIL to compile — `HexEye` does not exist.

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/flyvis/src/hex_eye.rs`:

```rust
//! The rendering monitor and which ommatidium owns each of its pixels.
//!
//! flyvis renders a stimulus into a square cartesian image and then takes, per
//! ommatidium, the plain arithmetic mean of the pixels whose centres fall inside
//! that ommatidium's hexagonal footprint. There is no Gaussian and no
//! convolution anywhere in that path -- the kernel is a hard point-in-hexagon
//! indicator, built once and reused for every frame.
//!
//! This module reproduces the geometry. It knows nothing about bars, angles or
//! time.
//!
//! Two things about flyvis's layout are worth stating, because both look like
//! bugs and neither is ours to fix:
//!
//! - **The footprints do not tile the monitor.** The lattice pitch is `ppo` in
//!   both axes while the hexagons are regular, so 36% of the monitor's pixels
//!   fall inside no ommatidium at all. They are simply never sampled.
//! - **flyvis builds this mask in `float16`.** We use `f64`. 2,052 of 600,625
//!   pixels end up assigned differently, which is one of the components the
//!   moving-edge tolerance gate measures.
//!
//! What both masks agree on, and what this module depends on, is that **no
//! pixel is ever inside two footprints**. That is checked in
//! [`HexEye::new`], not assumed: it is what lets one `u16` per pixel replace
//! flyvis's dense `(600625, 721)` boolean, 1.2 MB against 433 MB.

use crate::expand::hex_coords;

/// The owner of a pixel that falls inside no ommatidium.
pub const NO_OWNER: u16 = u16::MAX;

/// The rendering monitor, its ommatidium centres, and the pixel-to-ommatidium
/// map.
#[derive(Clone, Debug)]
pub struct HexEye {
    monitor_px: usize,
    /// Per hexal, in `hex_coords` order: `(x, y)` = `(column, row)`.
    centres: Vec<(f64, f64)>,
    dist_to_edge: f64,
    dist_to_vertex: f64,
    /// Row-major, `monitor_px * monitor_px`. [`NO_OWNER`] where no ommatidium
    /// claims the pixel.
    owners: Vec<u16>,
    /// Pixels owned, per hexal. flyvis calls this `kernel_sum`.
    kernel_sum: Vec<u32>,
}

impl HexEye {
    /// Builds the monitor and the ownership map.
    ///
    /// `extent` is the hex radius (15 gives 721 ommatidia) and `ppo` the pixels
    /// per ommatidium (flyvis uses 25, giving a 775x775 monitor).
    ///
    /// # Panics
    ///
    /// If any pixel falls inside two ommatidia, which would invalidate the
    /// one-owner-per-pixel representation, or if `extent` is negative.
    pub fn new(extent: i32, ppo: usize) -> Self {
        assert!(extent >= 0, "extent must not be negative, got {extent}");
        let across = 2 * extent + 1;
        let monitor_px = ppo * across as usize;

        // flyvis: `dist_w = img_width / (2n + 1)`, `dist_h` likewise, and the
        // grid is centred by INTEGER division -- 387, not 387.5, at the
        // defaults.
        let dist_w = monitor_px as f64 / across as f64;
        let dist_h = dist_w;
        let shift = (monitor_px / 2) as f64;
        let centres: Vec<(f64, f64)> = hex_coords(extent)
            .map(|(u, v)| {
                (
                    dist_w * v as f64 + shift,
                    dist_h * (u as f64 + v as f64 / 2.0) + shift,
                )
            })
            .collect();

        let dist_to_edge = (dist_w + dist_h) / 4.0;
        let dist_to_vertex = 2.0 / 3.0f64.sqrt() * dist_to_edge;

        let (owners, kernel_sum) =
            build_owners(monitor_px, &centres, dist_to_edge, dist_to_vertex);

        HexEye { monitor_px, centres, dist_to_edge, dist_to_vertex, owners, kernel_sum }
    }

    /// The monitor's side, in pixels. It is square.
    pub fn monitor_px(&self) -> usize {
        self.monitor_px
    }

    /// Number of ommatidia -- 721 at extent 15.
    pub fn num_hexals(&self) -> usize {
        self.centres.len()
    }

    /// An ommatidium's centre as `(x, y)` = `(column, row)`.
    pub fn centre(&self, hexal: usize) -> Option<(f64, f64)> {
        self.centres.get(hexal).copied()
    }

    /// Distance from an ommatidium's centre to the middle of one of its edges.
    pub fn dist_to_edge(&self) -> f64 {
        self.dist_to_edge
    }

    /// Distance from an ommatidium's centre to one of its vertices.
    pub fn dist_to_vertex(&self) -> f64 {
        self.dist_to_vertex
    }
}
```

Leave `build_owners` unwritten for now — Task 2 adds it. To keep this task
compiling and testable on its own, add this placeholder immediately above the
test module and **delete it in Task 2**:

```rust
/// Placeholder: Task 2 replaces this with the real point-in-hexagon scan.
fn build_owners(
    monitor_px: usize,
    centres: &[(f64, f64)],
    _dist_to_edge: f64,
    _dist_to_vertex: f64,
) -> (Vec<u16>, Vec<u32>) {
    (vec![NO_OWNER; monitor_px * monitor_px], vec![0; centres.len()])
}
```

Then declare the module in `crates/flyvis/src/lib.rs`, in the existing
alphabetical-ish block, after `mod expand_checks;`:

```rust
pub mod hex_eye;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-flyvis hex_eye`
Expected: PASS, 6 tests.

Then: `cargo clippy -p semblance-flyvis --all-targets`
Expected: no new warnings. `owners`/`kernel_sum` are unread until Task 2 — if
clippy flags them as dead, add `#[allow(dead_code)]` on those two fields with the
comment `// read in Task 2` and remove it in Task 2.

- [ ] **Step 5: Commit**

```bash
git add crates/flyvis/src/hex_eye.rs crates/flyvis/src/lib.rs
git commit -m "Lay out the rendering monitor's ommatidium centres"
```

---

### Task 2: Pixel ownership

**Files:**
- Modify: `crates/flyvis/src/hex_eye.rs`

**Interfaces:**
- Consumes: `HexEye` internals from Task 1 — `monitor_px: usize`,
  `centres: Vec<(f64, f64)>`, `dist_to_edge: f64`, `dist_to_vertex: f64`,
  `owners: Vec<u16>`, `kernel_sum: Vec<u32>`, and `pub const NO_OWNER: u16`.
- Produces:
  - `pub fn HexEye::owner(&self, col: usize, row: usize) -> Option<usize>` —
    `None` when no ommatidium claims that pixel
  - `pub fn HexEye::owners(&self) -> &[u16]` — row-major, `NO_OWNER` for unowned
  - `pub fn HexEye::kernel_sum(&self) -> &[u32]` — pixels owned, per hexal
  - `pub(crate) fn contains(centre: (f64, f64), dist_to_vertex: f64, x: f64, y: f64) -> bool`

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` in `crates/flyvis/src/hex_eye.rs`:

```rust
    /// The load-bearing invariant. flyvis stores a dense (600625, 721)
    /// boolean; one `u16` per pixel only replaces it if no pixel is ever
    /// inside two footprints. Measured true in f64 AND in flyvis's own fp16
    /// mask -- but checked, not assumed, because the lattice pitch and the
    /// hexagon size are set independently and nothing in flyvis enforces this.
    #[test]
    fn no_pixel_belongs_to_two_ommatidia() {
        let e = eye();
        // HexEye::new panics if this is violated; reaching here is the
        // assertion. Re-derive it independently so the test is not merely
        // re-running the constructor's own check.
        let mut claims = vec![0u8; 775 * 775];
        for h in 0..e.num_hexals() {
            let c = e.centre(h).unwrap();
            let dtv = e.dist_to_vertex();
            let lo_c = (c.0 - dtv).floor().max(0.0) as usize;
            let hi_c = ((c.0 + dtv).ceil() as usize).min(774);
            let lo_r = (c.1 - dtv).floor().max(0.0) as usize;
            let hi_r = ((c.1 + dtv).ceil() as usize).min(774);
            for row in lo_r..=hi_r {
                for col in lo_c..=hi_c {
                    if contains(c, dtv, col as f64, row as f64) {
                        claims[row * 775 + col] += 1;
                    }
                }
            }
        }
        assert_eq!(claims.iter().filter(|&&n| n > 1).count(), 0, "pixels claimed twice");
    }

    /// Measured against flyvis 1.2.0. If these move, something in the geometry
    /// moved -- report it, do not re-pin the constants.
    #[test]
    fn ownership_counts_match_what_was_measured() {
        let e = eye();
        let owned = e.owners().iter().filter(|&&o| o != NO_OWNER).count();
        assert_eq!(owned, 383_613, "pixels owned by some ommatidium");
        assert_eq!(775 * 775 - owned, 217_012, "pixels owned by none (36%)");
        let ks = e.kernel_sum();
        assert_eq!(ks.len(), 721);
        assert_eq!(ks.iter().sum::<u32>() as usize, owned, "kernel sums must total the owned pixels");
        assert_eq!(*ks.iter().min().unwrap(), 518, "smallest footprint, clipped by the monitor edge");
        assert_eq!(*ks.iter().max().unwrap(), 541, "largest footprint, fully on the monitor");
    }

    /// Every ommatidium samples something. A zero here would divide by zero in
    /// the renderer.
    #[test]
    fn every_ommatidium_owns_at_least_one_pixel() {
        assert!(eye().kernel_sum().iter().all(|&n| n > 0));
    }

    /// The origin hexal is index 360 and sits on pixel (387, 387).
    #[test]
    fn the_centre_pixel_belongs_to_the_origin_hexal() {
        let e = eye();
        assert_eq!(e.owner(387, 387), Some(360));
    }

    /// `owner` takes (column, row) -- NOT (row, column). Swapping them
    /// transposes the whole eye and still returns 721 plausible values, so this
    /// picks a hexal that is far off the diagonal.
    #[test]
    fn owner_takes_column_then_row() {
        let e = eye();
        // Hexal (u, v) = (0, 10): x = 25*10 + 387 = 637, y = 25*5 + 387 = 512.
        let h = crate::expand::hex_coords(15).position(|p| p == (0, 10)).unwrap();
        assert_eq!(e.centre(h), Some((637.0, 512.0)));
        assert_eq!(e.owner(637, 512), Some(h), "(column, row)");
        assert_ne!(e.owner(512, 637), Some(h), "(row, column) must NOT work");
    }

    /// The monitor's corners fall outside the hexagonal lattice entirely.
    #[test]
    fn the_monitor_corners_belong_to_no_ommatidium() {
        let e = eye();
        for (c, r) in [(0, 0), (774, 0), (0, 774), (774, 774)] {
            assert_eq!(e.owner(c, r), None, "corner ({c}, {r})");
        }
    }

    #[test]
    fn owner_rejects_coordinates_off_the_monitor() {
        let e = eye();
        assert_eq!(e.owner(775, 0), None);
        assert_eq!(e.owner(0, 775), None);
    }

    /// Hand-computed: the hexagon has vertices on the horizontal axis at
    /// +/-dist_to_vertex and flats above and below at +/-dist_to_edge.
    #[test]
    fn contains_agrees_with_hand_computed_cases() {
        let c = (100.0, 100.0);
        let dtv = 2.0 / 3.0f64.sqrt() * 12.5;
        assert!(contains(c, dtv, 100.0, 100.0), "the centre");
        assert!(contains(c, dtv, 114.0, 100.0), "just inside the right vertex");
        assert!(!contains(c, dtv, 115.0, 100.0), "just outside the right vertex");
        assert!(contains(c, dtv, 100.0, 112.0), "just inside the bottom flat");
        assert!(!contains(c, dtv, 100.0, 113.0), "just outside the bottom flat");
        // A corner of the bounding box, outside the hexagon.
        assert!(!contains(c, dtv, 114.0, 112.0));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-flyvis hex_eye`
Expected: FAIL to compile — `contains`, `owner`, `owners`, `kernel_sum` do not
exist.

- [ ] **Step 3: Write the implementation**

Delete the Task 1 placeholder `build_owners` and any `#[allow(dead_code)]` added
in Task 1. Add these accessors to the `impl HexEye` block:

```rust
    /// Which ommatidium owns the pixel at `(col, row)`, or `None` if none does.
    ///
    /// Note the argument order: **column then row**. 36% of the monitor's
    /// pixels are owned by nothing.
    pub fn owner(&self, col: usize, row: usize) -> Option<usize> {
        if col >= self.monitor_px || row >= self.monitor_px {
            return None;
        }
        match self.owners[row * self.monitor_px + col] {
            NO_OWNER => None,
            h => Some(h as usize),
        }
    }

    /// The whole ownership map, row-major, [`NO_OWNER`] where unowned.
    pub fn owners(&self) -> &[u16] {
        &self.owners
    }

    /// Pixels owned, per ommatidium. flyvis calls this `kernel_sum`, and it is
    /// the divisor that turns a pixel sum into an ommatidium's mean.
    pub fn kernel_sum(&self) -> &[u32] {
        &self.kernel_sum
    }
```

And these free functions, above the test module:

```rust
/// Whether the point `(x, y)` lies inside the hexagon centred on `centre`.
///
/// Six half-plane sign tests, one per edge, exactly as flyvis does it: for each
/// edge, evaluate the edge's line at the query point and at the hexagon's own
/// centre, and require the two to fall on the same side.
///
/// `tilt` is zero throughout flyvis, so the vertices sit at 0, 60, ... 300
/// degrees -- a vertex on the horizontal axis, flats above and below.
///
/// A point exactly on an edge has sign 0, which matches neither `+1` nor `-1`,
/// so it is **excluded**. That is flyvis's behaviour (`torch.sign`), not an
/// oversight, and it is part of why no pixel gets two owners.
pub(crate) fn contains(centre: (f64, f64), dist_to_vertex: f64, x: f64, y: f64) -> bool {
    let (xc, yc) = centre;
    for i in 0..6 {
        let a = i as f64 * std::f64::consts::FRAC_PI_3;
        let b = (i + 1) as f64 * std::f64::consts::FRAC_PI_3;
        let (x1, y1) = (dist_to_vertex * a.cos() + xc, dist_to_vertex * a.sin() + yc);
        let (x2, y2) = (dist_to_vertex * b.cos() + xc, dist_to_vertex * b.sin() + yc);
        let slope = (y2 - y1) / (x2 - x1);
        let f_centre = y1 + slope * (xc - x1) - yc;
        let f_point = y1 + slope * (x - x1) - y;
        if sign(f_centre) != sign(f_point) {
            return false;
        }
    }
    true
}

/// `-1`, `0` or `1`, matching `torch.sign`. Zero is its own case, so a point on
/// an edge is outside.
fn sign(v: f64) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

/// Scans every pixel of every ommatidium's bounding box and records the owner.
///
/// # Panics
///
/// If a pixel falls inside two ommatidia. One `u16` per pixel cannot represent
/// that, and silently keeping the first or last owner would corrupt every frame
/// by a little.
fn build_owners(
    monitor_px: usize,
    centres: &[(f64, f64)],
    _dist_to_edge: f64,
    dist_to_vertex: f64,
) -> (Vec<u16>, Vec<u32>) {
    assert!(
        centres.len() < NO_OWNER as usize,
        "{} ommatidia does not fit a u16 owner index",
        centres.len()
    );
    let mut owners = vec![NO_OWNER; monitor_px * monitor_px];
    let mut kernel_sum = vec![0u32; centres.len()];
    let last = monitor_px - 1;

    for (h, &c) in centres.iter().enumerate() {
        let lo_c = (c.0 - dist_to_vertex).floor().max(0.0) as usize;
        let hi_c = ((c.0 + dist_to_vertex).ceil().max(0.0) as usize).min(last);
        let lo_r = (c.1 - dist_to_vertex).floor().max(0.0) as usize;
        let hi_r = ((c.1 + dist_to_vertex).ceil().max(0.0) as usize).min(last);
        if lo_c > hi_c || lo_r > hi_r {
            continue;
        }
        for row in lo_r..=hi_r {
            for col in lo_c..=hi_c {
                if !contains(c, dist_to_vertex, col as f64, row as f64) {
                    continue;
                }
                let slot = &mut owners[row * monitor_px + col];
                assert_eq!(
                    *slot, NO_OWNER,
                    "pixel ({col}, {row}) is inside both ommatidium {} and {h}",
                    *slot
                );
                *slot = h as u16;
                kernel_sum[h] += 1;
            }
        }
    }
    (owners, kernel_sum)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-flyvis hex_eye`
Expected: PASS, 14 tests.

If `ownership_counts_match_what_was_measured` fails, **stop and report the
observed counts.** Those numbers came from the same arithmetic run in numpy
against flyvis 1.2.0; a difference means the formula diverged, not that the
constant is stale.

Run: `cargo clippy -p semblance-flyvis --all-targets`
Expected: no new warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/flyvis/src/hex_eye.rs
git commit -m "Decide which ommatidium owns each monitor pixel"
```

---

### Task 3: Bar geometry and the inverse rotation

**Files:**
- Create: `crates/flyvis/src/moving_edge.rs`
- Modify: `crates/flyvis/src/lib.rs`

**Interfaces:**
- Consumes: nothing from Tasks 1-2 yet.
- Produces:
  - `pub struct MovingEdgeParams` with the fields listed below
  - `pub fn MovingEdgeParams::flyvis_defaults(dt: f64) -> MovingEdgeParams`
  - `pub(crate) struct BarGeometry { pub rows: (i64, i64), pub cols: Vec<(i64, i64)> }`
  - `pub(crate) fn BarGeometry::new(params: &MovingEdgeParams, monitor_px: usize) -> BarGeometry`
  - `pub(crate) fn BarGeometry::num_offsets(&self) -> usize`
  - `pub(crate) fn inverse_rotate(angle_deg: i32, monitor_px: usize, col: usize, row: usize) -> (i64, i64)`
    — returns the clamped integer source pixel `(x, y)` = `(column, row)`

- [ ] **Step 1: Write the failing tests**

Create `crates/flyvis/src/moving_edge.rs` with this test module at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> MovingEdgeParams {
        MovingEdgeParams::flyvis_defaults(1.0 / 200.0)
    }

    /// The three MovingEdge overrides. Every one of them is inherited wrongly
    /// from MovingBar by default, and every one produces a plausible edge.
    #[test]
    fn the_defaults_are_movingedges_not_movingbars() {
        let p = params();
        assert_eq!(p.width, 80, "MovingBar uses [1, 2, 4]");
        assert_eq!(p.bar_loc_horizontal, 0.0, "MovingBar uses radians(90)");
        assert_eq!(p.angles.len(), 12);
        assert_eq!(p.angles[1], 30);
        assert_eq!(p.speeds, vec![2.4, 4.8, 9.7, 13.0, 19.0, 25.0]);
        assert_eq!(p.intensities, vec![0.0, 1.0]);
        assert_eq!(p.offsets, (-10, 10));
        assert_eq!(p.height, 9);
        assert_eq!((p.t_pre, p.t_post), (1.0, 1.0));
    }

    /// `led_width` (2.25 deg) is the SPATIAL unit; `omm_width` (5.8 deg) only
    /// ever converts speeds into durations. Swapping them scales every offset
    /// by 2.6x and still sweeps an edge across the eye.
    #[test]
    fn the_two_angular_units_are_not_interchangeable() {
        let p = params();
        assert!((p.led_width - 2.25f64.to_radians()).abs() < 1e-15);
        assert!((p.omm_width - 5.8f64.to_radians()).abs() < 1e-15);
    }

    #[test]
    fn the_bar_covers_88_rows_at_the_monitor_centre() {
        let g = BarGeometry::new(&params(), 775);
        assert_eq!(g.rows, (343, 431), "bar_height_px 87 centred on row 387");
    }

    /// Every one of the 21 offsets, against flyvis's own integer arithmetic.
    /// The bar is anchored at column 0 and its right edge sweeps rightward.
    #[test]
    fn the_21_offsets_match_flyviss_pixel_arithmetic() {
        let g = BarGeometry::new(&params(), 775);
        let want: [(i64, i64); 21] = [
            (0, 293), (0, 302), (0, 312), (0, 322), (0, 331), (0, 341), (0, 351),
            (0, 360), (0, 370), (0, 380), (0, 389), (0, 398), (0, 408), (0, 418),
            (0, 427), (0, 437), (0, 447), (0, 456), (0, 466), (0, 476), (0, 485),
        ];
        assert_eq!(g.num_offsets(), 21);
        assert_eq!(g.cols.as_slice(), want.as_slice());
    }

    /// The right edge advances monotonically, which is what makes this an edge
    /// sweeping in one direction rather than a bar flickering.
    #[test]
    fn the_leading_edge_only_ever_advances() {
        let g = BarGeometry::new(&params(), 775);
        assert!(g.cols.windows(2).all(|w| w[0].1 < w[1].1));
    }

    /// `int()` truncates toward zero, so offset -1 gives -9 px and offset +1
    /// gives +9. `floor` would give -10 for the negative half and shift the
    /// whole negative sweep by a pixel.
    #[test]
    fn offsets_truncate_toward_zero_rather_than_flooring() {
        let g = BarGeometry::new(&params(), 775);
        // offset 0 -> loc_w -387 -> stop 389; offset -1 -> loc_w -396 -> 380.
        // The gap either side of centre is 9, symmetric. Flooring the negative
        // side would make it 10.
        assert_eq!(g.cols[10].1 - g.cols[9].1, 9);
        assert_eq!(g.cols[11].1 - g.cols[10].1, 9);
    }

    /// No rotation at 0 degrees: the source pixel is the query pixel.
    #[test]
    fn zero_degrees_is_the_identity() {
        for (c, r) in [(0, 0), (387, 387), (774, 774), (100, 700)] {
            assert_eq!(inverse_rotate(0, 775, c, r), (c as i64, r as i64));
        }
    }

    /// The rotation centre is `monitor/2 - 0.5` = 387.0, NOT 387.5. Pillow
    /// samples the output pixel at `x + 0.5` and its bilinear filter subtracts
    /// 0.5 again. Measured end to end: 387.0 agrees with flyvis to 0.003 and
    /// 387.5 to 0.032, with up to 39 hexals over 0.01.
    #[test]
    fn the_rotation_centre_is_387_not_387_5() {
        // 180 degrees about 387.0 maps 387 + d to 387 - d, exactly.
        assert_eq!(inverse_rotate(180, 775, 387, 387), (387, 387));
        assert_eq!(inverse_rotate(180, 775, 400, 387), (374, 387));
        assert_eq!(inverse_rotate(180, 775, 387, 400), (387, 374));
        // About 387.5 these would be 375 and 375. Assert they are not.
        assert_ne!(inverse_rotate(180, 775, 400, 387), (375, 387));
    }

    /// 90 degrees is a quarter turn about the same centre.
    #[test]
    fn ninety_degrees_is_an_exact_quarter_turn() {
        assert_eq!(inverse_rotate(90, 775, 387, 387), (387, 387));
        // (col, row) = (400, 387) -> offset (13, 0) -> rotated to (0, 13).
        assert_eq!(inverse_rotate(90, 775, 400, 387), (387, 400));
        assert_eq!(inverse_rotate(90, 775, 387, 400), (374, 387));
    }

    /// Inverse-rotating by an angle and then by its negation returns the
    /// original pixel, up to the rounding each step does.
    #[test]
    fn rotating_by_an_angle_and_back_returns_the_pixel() {
        for angle in [30, 60, 120, 150, 210, 240, 300, 330] {
            for (c, r) in [(387, 387), (400, 390), (300, 500), (200, 200)] {
                let (x, y) = inverse_rotate(angle, 775, c, r);
                let (bx, by) = inverse_rotate(360 - angle, 775, x as usize, y as usize);
                assert!(
                    (bx - c as i64).abs() <= 1 && (by - r as i64).abs() <= 1,
                    "angle {angle}: ({c}, {r}) -> ({x}, {y}) -> ({bx}, {by})"
                );
            }
        }
    }

    /// Source coordinates are clamped to the monitor, which reproduces flyvis's
    /// `np.pad(..., mode="edge")` before the rotation. Without the clamp,
    /// pixels that rotate in from off-monitor would read as background and put
    /// a spurious grey wedge in every rotated frame.
    #[test]
    fn source_coordinates_are_clamped_to_the_monitor() {
        for angle in [30, 150, 210, 330] {
            for (c, r) in [(0, 0), (774, 0), (0, 774), (774, 774), (387, 0)] {
                let (x, y) = inverse_rotate(angle, 775, c, r);
                assert!((0..775).contains(&x) && (0..775).contains(&y), "angle {angle} at ({c}, {r})");
            }
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-flyvis moving_edge`
Expected: FAIL to compile — nothing in this module exists.

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/flyvis/src/moving_edge.rs`:

```rust
//! Moving edges: a wide bar whose leading edge sweeps across the eye, at
//! twelve directions and six speeds.
//!
//! # This family is gated at a tolerance, and it is the only one that is
//!
//! Every other comparison in this workspace is element-wise: the connectome's
//! 1,513,231 edges, the flash and dot families, the lattice ordering. This one
//! is not, and reading "moving edges are gated against flyvis" as the same
//! claim would be wrong.
//!
//! flyvis rasterises the bar into a 775x775 image, casts it to `uint8` --
//! truncating, so its background 0.5 becomes 127/255 = 0.498039 -- and rotates
//! that image through Pillow's bilinear resampler for eleven of the twelve
//! angles. Reproducing those numbers bit-exactly would mean reimplementing
//! Pillow's affine resampling.
//!
//! Instead this rotates the *sampling geometry*: for each pixel, inverse-rotate
//! the query point about the monitor centre and test membership of the
//! unrotated bar. Exact, uniform across all twelve directions, and with no
//! external numerics to port. The cost is that our values and flyvis's differ
//! by a small amount, measured at **max 0.00306 per hexal** with **zero hexals
//! over 0.005** -- dominated by a near-uniform floor of `0.5 - 127/255 =
//! 0.00196`, which is flyvis's quantised background and nothing else.
//!
//! # Structure
//!
//! flyvis never renders per frame. It renders the 21 offset positions once per
//! (angle, intensity), then derives every speed from them by truncating-index
//! selection. A six-speed sweep costs 21 renders, not six sequences' worth.
//! This does the same.

use crate::expand::hex_coords;
use crate::hex_eye::{HexEye, NO_OWNER};
use crate::stimuli::{frame_count, Stimulus, StimulusError};

/// Parameters for a family of moving edges.
///
/// Every real-valued field is `f64`. This family's correctness is geometric --
/// a half-pixel error in the rotation centre moves the agreement with flyvis
/// from 0.003 to 0.032 -- and the frame counts are differences of truncations,
/// where a last-bit difference can move a boundary by a whole frame.
#[derive(Clone, Debug, PartialEq)]
pub struct MovingEdgeParams {
    /// Hex radius of the receptor array. 15 gives 721 hexals.
    pub extent: i32,
    /// Pixels per ommatidium on the rendering monitor. flyvis uses 25, giving
    /// 775x775.
    pub ppo: usize,
    /// Sweep directions, in **degrees**.
    pub angles: Vec<i32>,
    /// Speeds in half-ommatidia per second.
    pub speeds: Vec<f64>,
    /// Edge intensities. The background sits between them.
    pub intensities: Vec<f64>,
    /// First and last offset, inclusive, in half-ommatidia.
    pub offsets: (i32, i32),
    /// Bar width in half-ommatidia. `MovingEdge` hardcodes 80 -- a 180-degree
    /// bar, wide enough that one of its edges is always off-monitor, which is
    /// what turns a bar into an edge. `MovingBar` uses `[1, 2, 4]`.
    pub width: i32,
    /// Bar height in half-ommatidia.
    pub height: i32,
    /// Where offset 0 sits before rotation, in radians. `MovingEdge` uses
    /// `radians(0)`; `MovingBar` uses `radians(90)`.
    pub bar_loc_horizontal: f64,
    /// Vertical anchor, in radians. `radians(90)` is the monitor's middle.
    pub bar_loc_vertical: f64,
    /// Background intensity.
    pub bg_intensity: f64,
    /// The **spatial** unit: widths, offsets and height are multiples of it.
    pub led_width: f64,
    /// The **temporal** unit: converts speeds into durations, and nothing else.
    pub omm_width: f64,
    /// Seconds of background before the sweep.
    pub t_pre: f64,
    /// Seconds after the sweep, holding the final frame.
    pub t_post: f64,
    /// Integration step, in seconds.
    pub dt: f64,
}

impl MovingEdgeParams {
    /// flyvis's `MovingEdge` defaults, with `dt` supplied by the caller.
    ///
    /// There is deliberately no default `dt`. The dynamics use
    /// `1 / max(tau, dt)`, so `dt` decides which cell types are floored -- it
    /// is a modelling choice, not a convenience. flyvis's own response analyses
    /// use `1.0 / 200.0`.
    pub fn flyvis_defaults(dt: f64) -> Self {
        MovingEdgeParams {
            extent: 15,
            ppo: 25,
            angles: vec![0, 30, 60, 90, 120, 150, 180, 210, 240, 270, 300, 330],
            speeds: vec![2.4, 4.8, 9.7, 13.0, 19.0, 25.0],
            intensities: vec![0.0, 1.0],
            offsets: (-10, 10),
            width: 80,
            height: 9,
            bar_loc_horizontal: 0.0,
            bar_loc_vertical: std::f64::consts::FRAC_PI_2,
            bg_intensity: 0.5,
            led_width: 2.25f64.to_radians(),
            omm_width: 5.8f64.to_radians(),
            t_pre: 1.0,
            t_post: 1.0,
            dt,
        }
    }
}

/// The unrotated bar's extent in monitor pixels, per offset.
///
/// The row band is fixed; only the columns move. flyvis's bar is 775 px wide on
/// a 775 px monitor, so its left edge is always off-monitor and `start` clamps
/// to 0 -- what sweeps is the right edge.
#[derive(Clone, Debug)]
pub(crate) struct BarGeometry {
    /// `[first, last)` rows.
    pub rows: (i64, i64),
    /// Per offset, `[first, last)` columns.
    pub cols: Vec<(i64, i64)>,
}

impl BarGeometry {
    /// Reproduces flyvis's integer pixel arithmetic exactly.
    ///
    /// Every cast below is a truncation toward zero, matching Python's `int()`.
    /// Rust's `as` on `f64` truncates toward zero too, so `-9.6875 as i64` is
    /// `-9` in both languages -- `floor` would give `-10` and shift the whole
    /// negative half of the sweep by a pixel.
    pub(crate) fn new(params: &MovingEdgeParams, monitor_px: usize) -> Self {
        let monitor = monitor_px as f64;
        let ppo = params.ppo as f64;
        let pi = std::f64::consts::PI;

        let bar_width_px =
            (params.width as f64 * params.led_width / params.omm_width * ppo) as i64;
        let bar_height_px =
            (params.height as f64 * params.led_width / params.omm_width * ppo) as i64;
        let loc_vertical_px = (monitor * params.bar_loc_vertical / pi) as i64;

        let rows = (
            (loc_vertical_px as f64 - bar_height_px as f64 / 2.0) as i64,
            (loc_vertical_px as f64 + bar_height_px as f64 / 2.0) as i64 + 1,
        );

        let cols = (params.offsets.0..=params.offsets.1)
            .map(|k| {
                let theta = params.bar_loc_horizontal + k as f64 * params.led_width;
                let loc_horizontal_px = (monitor * theta / pi) as i64;
                let loc_w =
                    (loc_horizontal_px as f64 - bar_width_px as f64 / 2.0) as i64;
                // flyvis loops over `n_bars`; MovingEdge always has one, so the
                // spacing term drops out and only the first bar is drawn.
                (loc_w.max(0), loc_w + bar_width_px + 1)
            })
            .collect();

        BarGeometry { rows, cols }
    }

    /// How many offset positions the sweep visits -- 21 at the defaults.
    pub(crate) fn num_offsets(&self) -> usize {
        self.cols.len()
    }
}

/// The source pixel that `(col, row)` samples when the image is rotated by
/// `angle_deg`, clamped to the monitor.
///
/// # The centre is `monitor/2 - 0.5`, not `monitor/2`
///
/// Pillow rotates about `(w/2, h/2)` in *continuous* coordinates, but its
/// transform samples the output pixel at `x + 0.5` and its bilinear filter
/// subtracts 0.5 before interpolating. The two halves cancel into a rotation
/// about `w/2 - 0.5` in *index* coordinates -- 387.0 on a 775 px monitor, which
/// is also exactly where the origin ommatidium sits.
///
/// Measured end to end against flyvis's own renderer: 387.0 agrees to a maximum
/// of 0.00306 per hexal, 387.5 to 0.03185 with up to 39 hexals over 0.01. Both
/// produce a perfectly plausible moving edge.
///
/// # The clamp
///
/// flyvis pads the image to its diagonal with `mode="edge"` before rotating, so
/// pixels that rotate in from off-monitor take the nearest edge pixel's value.
/// Clamping the source coordinate reproduces that exactly. Returning background
/// instead would put a grey wedge in the corner of every rotated frame.
///
/// No angle needs special-casing. At 0 degrees the sine is exactly zero and
/// this is the identity; at 90, 180 and 270 the residual sine or cosine is
/// around 1e-16, which shifts the source by at most 5e-14 px and cannot change
/// a rounded integer.
pub(crate) fn inverse_rotate(
    angle_deg: i32,
    monitor_px: usize,
    col: usize,
    row: usize,
) -> (i64, i64) {
    let last = monitor_px as i64 - 1;
    let centre = monitor_px as f64 / 2.0 - 0.5;
    let (sin, cos) = (angle_deg as f64).to_radians().sin_cos();
    let (dx, dy) = (col as f64 - centre, row as f64 - centre);
    let x = (cos * dx - sin * dy + centre).round() as i64;
    let y = (sin * dx + cos * dy + centre).round() as i64;
    (x.clamp(0, last), y.clamp(0, last))
}
```

`hex_coords`, `HexEye`, `NO_OWNER`, `frame_count`, `Stimulus` and `StimulusError`
are imported here but first used in Tasks 4 and 5. If the compiler warns about
unused imports, **delete the unused ones now and re-add them in the task that
needs them** rather than silencing the warning.

Declare the module in `crates/flyvis/src/lib.rs`, after `pub mod hex_eye;`:

```rust
pub mod moving_edge;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-flyvis moving_edge`
Expected: PASS, 11 tests.

Run: `cargo clippy -p semblance-flyvis --all-targets`
Expected: no new warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/flyvis/src/moving_edge.rs crates/flyvis/src/lib.rs
git commit -m "Place the moving edge bar in monitor pixels and rotate the sampling geometry"
```

---

### Task 4: Coverage — the 21 offsets, rendered

**Files:**
- Modify: `crates/flyvis/src/moving_edge.rs`

**Interfaces:**
- Consumes: `HexEye::new`, `HexEye::owners`, `HexEye::kernel_sum`,
  `HexEye::monitor_px`, `HexEye::num_hexals`, `NO_OWNER` (Tasks 1-2);
  `MovingEdgeParams`, `BarGeometry`, `inverse_rotate` (Task 3).
- Produces:
  - `pub(crate) struct Coverage`
  - `pub(crate) fn Coverage::new(params: &MovingEdgeParams, eye: &HexEye) -> Coverage`
  - `pub(crate) fn Coverage::counts(&self, angle_index: usize, offset_index: usize) -> &[u32]`
    — bar-pixel count per hexal
  - `pub(crate) fn Coverage::kernel_sum(&self) -> &[u32]`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/flyvis/src/moving_edge.rs`:

```rust
    /// Building all twelve angles is the expensive part; most tests only need
    /// one or two.
    fn params_for(angles: Vec<i32>) -> MovingEdgeParams {
        let mut p = MovingEdgeParams::flyvis_defaults(1.0 / 200.0);
        p.angles = angles;
        p
    }

    #[test]
    fn coverage_never_exceeds_an_ommatidiums_own_pixels() {
        let p = params_for(vec![0, 30]);
        let eye = HexEye::new(p.extent, p.ppo);
        let c = Coverage::new(&p, &eye);
        let ks = c.kernel_sum();
        for a in 0..2 {
            for o in 0..21 {
                for (h, (&n, &total)) in c.counts(a, o).iter().zip(ks).enumerate() {
                    assert!(n <= total, "angle {a} offset {o} hexal {h}: {n} of {total}");
                }
            }
        }
    }

    /// The bar only ever grows as the offset advances, at every angle. This is
    /// what makes the sweep an edge passing over the eye rather than a bar
    /// appearing and disappearing.
    #[test]
    fn coverage_is_monotonic_in_the_offset() {
        let p = params_for(vec![0, 90, 150]);
        let eye = HexEye::new(p.extent, p.ppo);
        let c = Coverage::new(&p, &eye);
        for a in 0..3 {
            for o in 1..21 {
                for (h, (&prev, &now)) in
                    c.counts(a, o - 1).iter().zip(c.counts(a, o)).enumerate()
                {
                    assert!(now >= prev, "angle {a} offset {o} hexal {h}: {prev} -> {now}");
                }
            }
        }
    }

    /// At 0 degrees the bar is a vertical edge sweeping rightward over a
    /// horizontal band 88 rows tall. The origin hexal (index 360, at pixel
    /// 387, 387) sits inside that band and is uncovered early in the sweep and
    /// covered late.
    #[test]
    fn the_origin_hexal_is_swept_partway_through() {
        let p = params_for(vec![0]);
        let eye = HexEye::new(p.extent, p.ppo);
        let c = Coverage::new(&p, &eye);
        let total = c.kernel_sum()[360];
        assert_eq!(c.counts(0, 0)[360], 0, "offset -10: the edge has not arrived");
        assert_eq!(c.counts(0, 20)[360], total, "offset +10: fully swept");
    }

    /// A hexal well above the bar's row band never sees it at 0 degrees, at any
    /// offset. Hexal (u, v) = (-14, 0) sits at row 37, far above rows 343..431.
    #[test]
    fn hexals_outside_the_row_band_are_never_covered_at_zero_degrees() {
        let p = params_for(vec![0]);
        let eye = HexEye::new(p.extent, p.ppo);
        let c = Coverage::new(&p, &eye);
        let h = crate::expand::hex_coords(15).position(|q| q == (-14, 0)).unwrap();
        assert_eq!(eye.centre(h), Some((387.0, 37.0)));
        for o in 0..21 {
            assert_eq!(c.counts(0, o)[h], 0, "offset index {o}");
        }
    }

    /// Rotating the band by 90 degrees must move which hexals it touches. If
    /// the rotation were dropped, every angle would give the same coverage --
    /// and 144 identical-looking sequences.
    #[test]
    fn different_angles_cover_different_hexals() {
        let p = params_for(vec![0, 90]);
        let eye = HexEye::new(p.extent, p.ppo);
        let c = Coverage::new(&p, &eye);
        let touched = |a: usize| -> Vec<bool> {
            c.counts(a, 20).iter().zip(c.kernel_sum()).map(|(&n, &t)| n > 0 && n < t).collect()
        };
        assert_ne!(touched(0), touched(1), "0 and 90 degrees must differ");
    }

    /// The whole family's totals: 12 angles x 21 offsets.
    #[test]
    fn coverage_holds_one_array_per_angle_and_offset() {
        let p = MovingEdgeParams::flyvis_defaults(1.0 / 200.0);
        let eye = HexEye::new(p.extent, p.ppo);
        let c = Coverage::new(&p, &eye);
        assert_eq!(c.counts(11, 20).len(), 721);
        assert_eq!(c.kernel_sum().len(), 721);
        assert_eq!(c.kernel_sum().iter().sum::<u32>(), 383_613);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-flyvis moving_edge`
Expected: FAIL to compile — `Coverage` does not exist.

- [ ] **Step 3: Write the implementation**

Add to `crates/flyvis/src/moving_edge.rs`, above the test module:

```rust
/// How many of each ommatidium's pixels the bar covers, per angle and offset.
///
/// This is the whole of the geometric work: 12 angles x 21 offsets, built once.
/// A frame is then arithmetic on these counts -- an ommatidium's value is
/// `(covered * intensity + uncovered * background) / total`, computed from
/// integers, so it does not depend on the order pixels are summed in.
///
/// Counting rather than summing intensities also means both intensities come
/// from one pass: the cartesian image only ever holds two values.
#[derive(Clone, Debug)]
pub(crate) struct Coverage {
    n_hexals: usize,
    n_offsets: usize,
    /// `angles.len() * n_offsets * n_hexals`, angle-major then offset.
    counts: Vec<u32>,
    kernel_sum: Vec<u32>,
}

impl Coverage {
    /// Renders every angle's 21 offsets.
    ///
    /// One pass over the monitor per angle finds the pixels whose rotated
    /// source lands in the bar's row band -- about 11% of the owned pixels --
    /// and the 21 offsets are then cheap sweeps over that band alone.
    pub(crate) fn new(params: &MovingEdgeParams, eye: &HexEye) -> Self {
        let geometry = BarGeometry::new(params, eye.monitor_px());
        let n_offsets = geometry.num_offsets();
        let n_hexals = eye.num_hexals();
        let monitor = eye.monitor_px();
        let owners = eye.owners();

        let mut counts = vec![0u32; params.angles.len() * n_offsets * n_hexals];
        let mut band: Vec<(u16, i64)> = Vec::new();

        for (a, &angle) in params.angles.iter().enumerate() {
            band.clear();
            for row in 0..monitor {
                for col in 0..monitor {
                    let owner = owners[row * monitor + col];
                    if owner == NO_OWNER {
                        continue;
                    }
                    let (x, y) = inverse_rotate(angle, monitor, col, row);
                    if y < geometry.rows.0 || y >= geometry.rows.1 {
                        continue;
                    }
                    band.push((owner, x));
                }
            }
            for (o, &(start, stop)) in geometry.cols.iter().enumerate() {
                let base = (a * n_offsets + o) * n_hexals;
                for &(owner, x) in &band {
                    if x >= start && x < stop {
                        counts[base + owner as usize] += 1;
                    }
                }
            }
        }

        Coverage { n_hexals, n_offsets, counts, kernel_sum: eye.kernel_sum().to_vec() }
    }

    /// The per-hexal bar-pixel counts for one angle and offset.
    pub(crate) fn counts(&self, angle_index: usize, offset_index: usize) -> &[u32] {
        let base = (angle_index * self.n_offsets + offset_index) * self.n_hexals;
        &self.counts[base..base + self.n_hexals]
    }

    /// Pixels owned, per hexal -- the divisor.
    pub(crate) fn kernel_sum(&self) -> &[u32] {
        &self.kernel_sum
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-flyvis moving_edge`
Expected: PASS, 17 tests.

Run the release build too — `coverage_holds_one_array_per_angle_and_offset`
builds all twelve angles and is the slowest test in the crate:

Run: `cargo test -p semblance-flyvis --release moving_edge`
Expected: PASS.

If the debug run takes more than about 30 seconds, say so in the report rather
than reducing the test's coverage.

Run: `cargo clippy -p semblance-flyvis --all-targets`
Expected: no new warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/flyvis/src/moving_edge.rs
git commit -m "Render the twenty-one offset positions to per-ommatidium coverage"
```

---

### Task 5: Timing and the `Stimulus` implementation

**Files:**
- Modify: `crates/flyvis/src/moving_edge.rs`
- Modify: `crates/flyvis/src/stimuli.rs` (widen `frame_count` to `f64`)
- Modify: `crates/flyvis/src/lib.rs` (re-exports)

**Interfaces:**
- Consumes: `Coverage::new`, `Coverage::counts`, `Coverage::kernel_sum` (Task 4);
  `MovingEdgeParams` (Task 3); `HexEye::new` (Task 1);
  `crate::stimuli::{Stimulus, StimulusError}`.
- Produces:
  - `pub struct MovingEdge` implementing `Stimulus`
  - `pub fn MovingEdge::new(params: MovingEdgeParams) -> MovingEdge`
  - `pub fn MovingEdge::angle_of(&self, sequence: usize) -> Option<i32>`
  - `pub fn MovingEdge::intensity_of(&self, sequence: usize) -> Option<f32>`
  - `pub fn MovingEdge::speed_of(&self, sequence: usize) -> Option<f64>`
  - `pub fn MovingEdge::t_stim_of(&self, sequence: usize) -> Option<f64>`
  - `pub fn MovingEdge::valid_frames(&self, sequence: usize) -> Option<usize>`
  - `pub(crate) fn frame_count(duration: f64, dt: f64) -> usize` (signature change)

**Decision this task settles, which the spec left open.** flyvis pads each
sequence a **third** time, with `NaN`, purely so sequences of different speeds
can be stacked into one tensor; its own consumers strip that tail
(`filter_post` checks `np.isnan`). The `Stimulus` trait requires a uniform
`num_frames()` across the family, so that tail has to be represented somehow.
This implementation reproduces flyvis exactly — `frame_into` writes `f32::NAN`
past a sequence's own end — and exposes `valid_frames(sequence)` so a consumer
can stop before it. The alternative, extending the `continue` pad to the common
length, would put a plausible non-NaN value where flyvis has a sentinel, and an
integrator stepping it would produce a real-looking trace from frames that do
not exist.

- [ ] **Step 1: Widen `frame_count` to `f64`**

In `crates/flyvis/src/stimuli.rs`, change the signature and doc:

```rust
/// How many frames a duration occupies.
///
/// flyvis computes `int(t / dt)`, which **truncates** toward zero. Rounding
/// instead would add a spurious frame at many duration/step combinations.
///
/// `f64` because the moving-edge pads are *differences* of truncations --
/// `int((t_stim + t_pre) / dt) - int(t_stim / dt)` -- where a last-bit
/// difference can move a boundary by a whole frame.
pub(crate) fn frame_count(duration: f64, dt: f64) -> usize {
    (duration / dt) as usize
}
```

Update the two call sites in the same file. In `Flashes::new`:

```rust
        let n_pre = frame_count(params.t_pre as f64, params.dt as f64);
        let n_stim = frame_count(params.t_stim as f64, params.dt as f64);
```

In `Dots::new`:

```rust
        let n_pre = frame_count(params.t_pre as f64, params.dt as f64);
        let n_stim = frame_count(params.t_stim as f64, params.dt as f64);
        let n_post = frame_count(params.t_post as f64, params.dt as f64);
```

`FlashParams` and `DotParams` keep their `f32` fields; only the helper widens.

- [ ] **Step 2: Run the existing stimulus tests to prove the widening changed nothing**

Run: `cargo test -p semblance-flyvis stimuli`
Expected: PASS, every existing test, unchanged. `frame_count_truncates_rather_than_rounds`
now runs in `f64` and must still report 200 / 1000 / 400 / 0 / 199.

If any flash or dot test changes behaviour, **stop and report** — the widening
was expected to be neutral for these values and a change means a frame boundary
moved.

- [ ] **Step 3: Write the failing tests**

Add to `mod tests` in `crates/flyvis/src/moving_edge.rs`:

```rust
    fn edge_for(angles: Vec<i32>) -> MovingEdge {
        MovingEdge::new(params_for(angles))
    }

    #[test]
    fn moving_edges_have_flyvis_dimensions() {
        let e = MovingEdge::new(MovingEdgeParams::flyvis_defaults(1.0 / 200.0));
        assert_eq!(e.num_sequences(), 144, "12 angles x 2 intensities x 6 speeds");
        assert_eq!(e.num_frames(), 1078, "t_stim_max + t_pre + t_post at dt = 1/200");
        assert_eq!(e.num_hexals(), 721);
    }

    /// flyvis builds its `arg_df` with `product(angles, widths, intensities,
    /// zip(t_stim, speeds))`, and `product` varies its LAST argument fastest.
    /// So: angle-major, then intensity, then speed. Any other grouping
    /// mislabels 143 of the 144 sequences while producing arrays of identical
    /// shape.
    #[test]
    fn sequences_are_angle_major_then_intensity_then_speed() {
        let e = MovingEdge::new(MovingEdgeParams::flyvis_defaults(1.0 / 200.0));
        assert_eq!(
            (e.angle_of(0), e.intensity_of(0), e.speed_of(0)),
            (Some(0), Some(0.0), Some(2.4))
        );
        assert_eq!(
            (e.angle_of(1), e.intensity_of(1), e.speed_of(1)),
            (Some(0), Some(0.0), Some(4.8)),
            "speed varies fastest"
        );
        assert_eq!(
            (e.angle_of(6), e.intensity_of(6), e.speed_of(6)),
            (Some(0), Some(1.0), Some(2.4)),
            "intensity varies next"
        );
        assert_eq!(
            (e.angle_of(12), e.intensity_of(12), e.speed_of(12)),
            (Some(30), Some(0.0), Some(2.4)),
            "angle varies slowest"
        );
        assert_eq!(
            (e.angle_of(143), e.intensity_of(143), e.speed_of(143)),
            (Some(330), Some(1.0), Some(25.0))
        );
        assert_eq!(e.angle_of(144), None);
        assert_eq!(e.intensity_of(144), None);
        assert_eq!(e.speed_of(144), None);
    }

    /// `t_stim = n_offsets * led_width / (speed * omm_width)`. This is the only
    /// place `omm_width` appears, and using `led_width` here instead would make
    /// every sweep 2.6x too long.
    #[test]
    fn t_stim_follows_from_the_speed_through_omm_width() {
        let e = MovingEdge::new(MovingEdgeParams::flyvis_defaults(1.0 / 200.0));
        let want = [3.394397, 1.697198, 0.839851, 0.626658, 0.428766, 0.325862];
        for (s, w) in want.iter().enumerate() {
            let got = e.t_stim_of(s).unwrap();
            assert!((got - w).abs() < 1e-6, "speed index {s}: got {got}, want {w}");
        }
    }

    /// Every sequence is 1078 frames, but a fast sweep's real content ends far
    /// earlier and the rest is NaN. Measured against flyvis.
    #[test]
    fn valid_frames_shorten_as_the_speed_rises() {
        let e = MovingEdge::new(MovingEdgeParams::flyvis_defaults(1.0 / 200.0));
        let want = [1078, 739, 567, 525, 485, 465];
        for (s, w) in want.iter().enumerate() {
            assert_eq!(e.valid_frames(s), Some(*w), "speed index {s}");
        }
        assert_eq!(e.valid_frames(144), None);
    }

    /// Past its own end a sequence is NaN, not background and not a held
    /// frame. flyvis uses NaN as a stacking sentinel and strips it before use;
    /// so must any consumer here.
    #[test]
    fn frames_past_a_sequences_end_are_nan() {
        let e = edge_for(vec![0]);
        let mut out = vec![0.0f32; 721];
        // Sequence 5 is angle 0, intensity 0, speed 25 -- 465 valid frames.
        e.frame_into(5, 464, &mut out).unwrap();
        assert!(out.iter().all(|x| x.is_finite()), "frame 464 is the last real one");
        e.frame_into(5, 465, &mut out).unwrap();
        assert!(out.iter().all(|x| x.is_nan()), "frame 465 is past the end");
        e.frame_into(5, 1077, &mut out).unwrap();
        assert!(out.iter().all(|x| x.is_nan()));
    }

    /// The first 200 frames are uniform background at every speed.
    #[test]
    fn the_first_200_frames_are_uniform_background() {
        let e = edge_for(vec![0]);
        let mut out = vec![0.0f32; 721];
        for s in [0usize, 5] {
            e.frame_into(s, 0, &mut out).unwrap();
            assert!(out.iter().all(|&x| x == 0.5), "sequence {s} frame 0");
            e.frame_into(s, 199, &mut out).unwrap();
            assert!(out.iter().all(|&x| x == 0.5), "sequence {s} frame 199");
            e.frame_into(s, 200, &mut out).unwrap();
            assert!(
                out.iter().any(|&x| x != 0.5),
                "sequence {s} frame 200 is the first stimulus frame"
            );
        }
    }

    /// `post_pad_mode = "continue"` holds the LAST swept frame, not the
    /// background. Padding with background instead would make every edge appear
    /// to vanish at the end of its sweep -- during exactly the window an offset
    /// response is measured in.
    #[test]
    fn the_post_pad_holds_the_final_frame_rather_than_the_background() {
        let e = edge_for(vec![0]);
        let mut last_stim = vec![0.0f32; 721];
        let mut padded = vec![0.0f32; 721];
        // Sequence 5: angle 0, intensity 0, speed 25. 200 pre + 65 stim = 265.
        e.frame_into(5, 264, &mut last_stim).unwrap();
        e.frame_into(5, 464, &mut padded).unwrap();
        assert_eq!(padded, last_stim, "the pad repeats the last swept frame");
        assert!(padded.iter().any(|&x| x != 0.5), "and it is not the background");
    }

    /// Verified against `torch.linspace(0, 20, n).long()` for all six speeds:
    /// computing `(k * step) as usize` in `f64` gives the identical index at
    /// every tick, even though torch builds the linspace in `float32`. The
    /// fixture gate in `moving_edge_checks` compares them element-wise anyway.
    /// `linspace(0, 20, n).long()` TRUNCATES toward zero rather than rounding
    /// to nearest, so the source offsets are picked slightly early. Rounding
    /// would choose a different offset for some ticks at every speed.
    #[test]
    fn the_resample_truncates_rather_than_rounds() {
        let e = MovingEdge::new(MovingEdgeParams::flyvis_defaults(1.0 / 200.0));
        // Speed 25 gives 65 resampled frames, step 20/64 = 0.3125.
        // Tick 4 is at 1.25 -> offset 1 by truncation, 1 by rounding;
        // tick 2 is at 0.625 -> 0 by truncation, 1 by rounding.
        assert_eq!(e.resample_index(5, 2), Some(0), "0.625 truncates to 0");
        assert_eq!(e.resample_index(5, 4), Some(1));
        assert_eq!(e.resample_index(5, 0), Some(0));
        assert_eq!(e.resample_index(5, 64), Some(20), "the sweep always ends at +10");
    }

    /// A background hexal reads exactly 0.5 here. flyvis reads 127/255 =
    /// 0.498039, because it casts its image to uint8 and 255 * 0.5 = 127.5
    /// truncates to 127. That difference is the floor of the tolerance gate and
    /// is deliberate -- do not introduce it here to make the numbers match.
    #[test]
    fn our_background_is_exactly_one_half() {
        let e = edge_for(vec![0]);
        let mut out = vec![0.0f32; 721];
        e.frame_into(0, 0, &mut out).unwrap();
        assert_eq!(out[0], 0.5);
        assert_ne!(out[0], 127.0 / 255.0);
    }

    /// A bright edge brightens and a dark edge darkens, from the same geometry.
    #[test]
    fn the_two_intensities_move_in_opposite_directions() {
        let e = edge_for(vec![0]);
        let (mut dark, mut bright) = (vec![0.0f32; 721], vec![0.0f32; 721]);
        // Sequences 0..5 are intensity 0; 6..11 are intensity 1. Speed 2.4.
        e.frame_into(0, 877, &mut dark).unwrap();
        e.frame_into(6, 877, &mut bright).unwrap();
        assert!(dark[360] < 0.5, "a dark edge darkens the swept hexal");
        assert!(bright[360] > 0.5, "a bright edge brightens it");
        assert!((dark[360] + bright[360] - 1.0).abs() < 1e-6, "mirrored about 0.5");
    }

    #[test]
    fn moving_edges_reject_a_bad_sequence_frame_or_buffer() {
        let e = edge_for(vec![0]);
        let mut out = vec![0.0f32; 721];
        assert_eq!(
            e.frame_into(12, 0, &mut out),
            Err(StimulusError::SequenceOutOfRange { index: 12, count: 12 })
        );
        assert_eq!(
            e.frame_into(0, 1078, &mut out),
            Err(StimulusError::FrameOutOfRange { index: 1078, count: 1078 })
        );
        let mut short = vec![0.0f32; 720];
        assert_eq!(
            e.frame_into(0, 0, &mut short),
            Err(StimulusError::BufferLengthMismatch { expected: 721, actual: 720 })
        );
    }
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p semblance-flyvis moving_edge`
Expected: FAIL to compile — `MovingEdge` does not exist.

- [ ] **Step 5: Write the implementation**

Add to `crates/flyvis/src/moving_edge.rs`, above the test module:

```rust
/// One speed's frame layout within the common sequence length.
#[derive(Clone, Copy, Debug)]
struct Timing {
    /// Background frames before the sweep.
    n_pre: usize,
    /// Swept frames.
    n_stim: usize,
    /// Frames past which the sequence is NaN.
    n_valid: usize,
}

/// A family of moving edges: every angle, intensity and speed.
#[derive(Clone, Debug)]
pub struct MovingEdge {
    n_hexals: usize,
    n_frames: usize,
    n_offsets: usize,
    bg_intensity: f64,
    angles: Vec<i32>,
    intensities: Vec<f64>,
    speeds: Vec<f64>,
    t_stim: Vec<f64>,
    /// Per speed, the offset index each swept frame selects.
    indices: Vec<Vec<usize>>,
    timing: Vec<Timing>,
    coverage: Coverage,
}

impl MovingEdge {
    /// Builds the family. All the geometry is rendered here.
    ///
    /// # Panics
    ///
    /// If any speed resamples to zero frames, which leaves the `"continue"`
    /// pad with no frame to hold. flyvis fails the same way, less clearly.
    pub fn new(params: MovingEdgeParams) -> Self {
        let eye = HexEye::new(params.extent, params.ppo);
        let n_hexals = eye.num_hexals();
        let coverage = Coverage::new(&params, &eye);
        let n_offsets = (params.offsets.1 - params.offsets.0 + 1) as usize;

        // flyvis: `t_stim = n_offsets * led_width / (speed * omm_width)`. The
        // ONLY place omm_width is used.
        let t_stim: Vec<f64> = params
            .speeds
            .iter()
            .map(|&s| (n_offsets as f64 * params.led_width) / (s * params.omm_width))
            .collect();
        let t_stim_max = t_stim.iter().copied().fold(f64::MIN, f64::max);
        let n_frames = frame_count(t_stim_max + params.t_pre + params.t_post, params.dt);

        let mut indices = Vec::with_capacity(t_stim.len());
        let mut timing = Vec::with_capacity(t_stim.len());
        for (i, &t) in t_stim.iter().enumerate() {
            let n_stim = frame_count(t, params.dt);
            assert!(
                n_stim > 0,
                "speed {} resamples to zero frames at dt {}",
                params.speeds[i], params.dt
            );
            // flyvis: `torch.linspace(0, n_offsets - 1, n_stim).long()`, whose
            // `.long()` truncates toward zero rather than rounding to nearest.
            let last = (n_offsets - 1) as f64;
            let step = if n_stim > 1 { last / (n_stim - 1) as f64 } else { 0.0 };
            indices.push((0..n_stim).map(|k| (k as f64 * step) as usize).collect());

            // Each pad is the DIFFERENCE of two truncations, exactly as
            // flyvis's `pad` computes `int(t_stim / dt) - stim.shape[-2]`.
            // Computing `int(t_pre / dt)` directly would differ at some
            // duration/step combinations.
            let through_pre = frame_count(t + params.t_pre, params.dt);
            let through_post = frame_count(t + params.t_pre + params.t_post, params.dt);
            timing.push(Timing {
                n_pre: through_pre - n_stim,
                n_stim,
                n_valid: through_post,
            });
        }

        MovingEdge {
            n_hexals,
            n_frames,
            n_offsets,
            bg_intensity: params.bg_intensity,
            angles: params.angles,
            intensities: params.intensities,
            speeds: params.speeds,
            t_stim,
            indices,
            timing,
            coverage,
        }
    }

    /// `(angle, intensity, speed)` indices for a sequence.
    ///
    /// flyvis orders its sequences angle-major, then intensity, then speed --
    /// `product` varies its last argument fastest.
    fn decode(&self, sequence: usize) -> Option<(usize, usize, usize)> {
        if sequence >= self.num_sequences() {
            return None;
        }
        let n_speeds = self.speeds.len();
        let n_intensities = self.intensities.len();
        Some((
            sequence / (n_speeds * n_intensities),
            (sequence / n_speeds) % n_intensities,
            sequence % n_speeds,
        ))
    }

    /// The sweep direction for a sequence, in degrees.
    pub fn angle_of(&self, sequence: usize) -> Option<i32> {
        self.decode(sequence).map(|(a, _, _)| self.angles[a])
    }

    /// The edge intensity for a sequence.
    pub fn intensity_of(&self, sequence: usize) -> Option<f32> {
        self.decode(sequence).map(|(_, i, _)| self.intensities[i] as f32)
    }

    /// The sweep speed for a sequence, in half-ommatidia per second.
    pub fn speed_of(&self, sequence: usize) -> Option<f64> {
        self.decode(sequence).map(|(_, _, s)| self.speeds[s])
    }

    /// How long the sweep itself lasts, in seconds.
    pub fn t_stim_of(&self, sequence: usize) -> Option<f64> {
        self.decode(sequence).map(|(_, _, s)| self.t_stim[s])
    }

    /// How many of this sequence's [`num_frames`](Stimulus::num_frames) frames
    /// are real.
    ///
    /// Every sequence is padded to a common length so the family is
    /// rectangular, but a fast sweep finishes early and the rest is `NaN`.
    /// Stepping an integrator past this point produces a real-looking trace
    /// from frames that do not exist.
    pub fn valid_frames(&self, sequence: usize) -> Option<usize> {
        self.decode(sequence).map(|(_, _, s)| self.timing[s].n_valid)
    }

    /// Which offset position a swept frame selects, counting from the first
    /// swept frame. `None` past the end of the sweep.
    pub fn resample_index(&self, sequence: usize, swept_frame: usize) -> Option<usize> {
        let (_, _, s) = self.decode(sequence)?;
        self.indices[s].get(swept_frame).copied()
    }
}

impl Stimulus for MovingEdge {
    fn num_sequences(&self) -> usize {
        self.angles.len() * self.intensities.len() * self.speeds.len()
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
        let Some((a, i, s)) = self.decode(sequence) else {
            return Err(StimulusError::SequenceOutOfRange {
                index: sequence,
                count: self.num_sequences(),
            });
        };
        if frame >= self.n_frames {
            return Err(StimulusError::FrameOutOfRange { index: frame, count: self.n_frames });
        }
        if out.len() != self.n_hexals {
            return Err(StimulusError::BufferLengthMismatch {
                expected: self.n_hexals,
                actual: out.len(),
            });
        }

        let t = self.timing[s];
        let offset = if frame < t.n_pre {
            None
        } else if frame < t.n_pre + t.n_stim {
            Some(self.indices[s][frame - t.n_pre])
        } else if frame < t.n_valid {
            // `post_pad_mode = "continue"`: hold the last SWEPT frame, which is
            // the last resampled offset -- not necessarily the last offset, and
            // never the background.
            Some(*self.indices[s].last().expect("n_stim > 0, checked in new"))
        } else {
            out.fill(f32::NAN);
            return Ok(());
        };

        match offset {
            None => out.fill(self.bg_intensity as f32),
            Some(o) => {
                let intensity = self.intensities[i];
                let counts = self.coverage.counts(a, o);
                let totals = self.coverage.kernel_sum();
                for (slot, (&covered, &total)) in
                    out.iter_mut().zip(counts.iter().zip(totals))
                {
                    let covered = covered as f64;
                    let total = total as f64;
                    *slot = ((covered * intensity + (total - covered) * self.bg_intensity)
                        / total) as f32;
                }
            }
        }
        Ok(())
    }
}
```

The `n_offsets` field is set from the params and must agree with the coverage's
own count; if the compiler reports it unused, remove the field rather than
silencing the warning — `Coverage` already knows the count.

Then re-export from `crates/flyvis/src/lib.rs`, extending the existing
`pub use stimuli::{...}` block with a new line after it:

```rust
pub use moving_edge::{MovingEdge, MovingEdgeParams};
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p semblance-flyvis`
Expected: PASS, all tests including the 11 new ones.

Run: `cargo test --workspace`
Expected: PASS, no failures.

Run: `cargo clippy --workspace --all-targets`
Expected: the 5 pre-existing warnings, no more.

Run: `cargo run --quiet -- roundtrip`
Expected: both distances read exactly `0.000000`.

- [ ] **Step 7: Commit**

```bash
git add crates/flyvis/src/moving_edge.rs crates/flyvis/src/stimuli.rs crates/flyvis/src/lib.rs
git commit -m "Resample the offsets into timed moving edge sequences"
```

---

### Task 6: The fixture export script

**Files:**
- Create: `tools/export_moving_edges.py`
- Modify: `crates/flyvis/src/test_support.rs`
- Modify: `tools/README.md`

**Interfaces:**
- Produces, in `data/flyvis-stimuli/` (gitignored):
  - `moving_edge_offsets.bin` — `<f4`, shape `(8, 21, 721)`: flyvis's rendered
    offsets for angles 0, 30, 90, 150 x intensities 0, 1, angle-major
  - `moving_edge_offsets.json` — the angle/intensity order and the shape
  - `moving_edge_timing.json` — per speed: `t_stim`, `n_stim`, `n_pre`,
    `n_valid`, `indices`; plus `n_frames` and `t_stim_max`
  - `hex_eye_owner.bin` — `<u2`, 600,625 entries, `65535` where unowned
  - `hex_eye_kernel_sum.json` — 721 integers
  - `moving_edges_provenance.json`

**Why these four angles.** 0 is the one angle flyvis does not rotate at all, so
agreement there should be tightest and isolates the mask. 90 is axis-aligned:
rotation but no interpolation. 30 and 150 both go through Pillow's bilinear
resampler. Those three regimes are expected to differ from each other, and a
subset that lumps them together cannot show it.

- [ ] **Step 1: Write the script**

Create `tools/export_moving_edges.py`:

```python
"""Export flyvis's own moving-edge renderings as Rust test fixtures.

The Rust side renders these analytically -- it rotates the sampling geometry
rather than the raster -- so unlike the flash and dot fixtures these are NOT
expected to match element-wise. They are the reference for a tolerance gate,
and the gate's job is to report the observed distribution, not to clear a
threshold quietly.

Three artifacts, for three different questions:

  moving_edge_offsets  what flyvis's renderer produces, for four angles chosen
                       to cover its three numerical regimes: 0 (no rotation at
                       all), 90 (axis-aligned), 30 and 150 (bilinear).
  hex_eye_owner        which ommatidium flyvis assigns each monitor pixel to.
                       flyvis builds this in float16 and we build it in f64, so
                       this localises a gate failure to the mask rather than to
                       the bar.
  moving_edge_timing   the resample indices and pad lengths. These ARE checked
                       element-wise -- only the geometry carries a tolerance.

Run by hand:  uv run --project tools tools/export_moving_edges.py
"""

import datetime
import json
import pathlib
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "flyvis-stimuli"

DT = 1.0 / 200.0                 # what flyvis's own response analyses use
GATE_ANGLES = [0, 30, 90, 150]   # no rotation / bilinear / axis-aligned / bilinear
N_OMMATIDIA = 721
PPO = 25
MONITOR = 775


def main() -> int:
    import torch

    import flyvis
    from flyvis.datasets.moving_bar import MovingEdge
    from flyvis.datasets.rendering.eye import HexEye

    OUT.mkdir(parents=True, exist_ok=True)

    # -- flyvis's pixel-to-ommatidium mask ------------------------------------
    eye = HexEye(N_OMMATIDIA, PPO)
    is_inside = eye.is_inside
    assert tuple(is_inside.shape) == (MONITOR * MONITOR, N_OMMATIDIA), is_inside.shape
    n_owners = is_inside.sum(dim=1).cpu().numpy()
    assert n_owners.max() <= 1, f"a pixel is inside {n_owners.max()} ommatidia"
    owner = is_inside.float().argmax(dim=1).cpu().numpy().astype(np.int64)
    owner[n_owners == 0] = 65535
    (OUT / "hex_eye_owner.bin").write_bytes(owner.astype("<u2").tobytes())

    kernel_sum = eye.kernel_sum.cpu().numpy().astype(int)
    assert kernel_sum.shape == (N_OMMATIDIA,), kernel_sum.shape
    assert kernel_sum.min() > 0, "an ommatidium owns no pixels"
    (OUT / "hex_eye_kernel_sum.json").write_text(
        json.dumps(kernel_sum.tolist()) + "\n"
    )
    print(f"  hex_eye_owner.bin: {int((owner != 65535).sum())} owned, "
          f"{int((owner == 65535).sum())} unowned")
    print(f"  hex_eye_kernel_sum.json: min {kernel_sum.min()} max {kernel_sum.max()} "
          f"sum {kernel_sum.sum()}")

    # -- the rendered offsets, for four angles --------------------------------
    edge = MovingEdge(dt=DT, angles=GATE_ANGLES)
    offsets = np.asarray(edge.wrap.offsets[:], dtype=np.float64)
    # RenderedOffsets stacks `product(angles, widths, intensities)`; MovingEdge
    # has one width, so this is angle-major, intensity-minor.
    assert offsets.shape == (len(GATE_ANGLES) * 2, 21, N_OMMATIDIA), offsets.shape
    (OUT / "moving_edge_offsets.bin").write_bytes(
        offsets.astype("<f4").tobytes()
    )
    groups = [
        {"index": i, "angle": int(a), "intensity": float(v)}
        for i, (a, v) in enumerate((a, v) for a in GATE_ANGLES for v in (0.0, 1.0))
    ]
    (OUT / "moving_edge_offsets.json").write_text(
        json.dumps(
            {"shape": list(offsets.shape), "groups": groups, "dtype": "<f4"}, indent=2
        )
        + "\n"
    )
    print(f"  moving_edge_offsets.bin: {offsets.shape}, "
          f"{(OUT / 'moving_edge_offsets.bin').stat().st_size} bytes")

    # flyvis's background is 127/255, not 0.5 -- its image is cast to uint8
    # before rotation and 255 * 0.5 = 127.5 truncates. Assert it, so the gate's
    # expected floor is recorded rather than remembered.
    angle0_bright = offsets[1]          # angle 0, intensity 1
    assert np.isclose(angle0_bright.min(), 0.5, atol=1e-6), angle0_bright.min()
    angle30_bright = offsets[3]         # angle 30, intensity 1
    quantised = float(angle30_bright.min())
    print(f"  background at angle 0: {angle0_bright.min():.6f} (unrotated, exact)")
    print(f"  background at angle 30: {quantised:.6f} (127/255 = {127/255:.6f})")

    # -- timing ---------------------------------------------------------------
    speeds = [float(s) for s in edge.speeds]
    t_stim = [float(t) for t in edge.t_stim]
    n_frames = int(next(iter(edge.sequences.values())).shape[-2])
    timing = []
    for speed, t in zip(speeds, t_stim):
        seq = edge.sequences[speed]
        finite = ~torch.isnan(seq[0]).any(dim=-1)
        n_valid = int(finite.sum())
        assert bool(finite[:n_valid].all()), "the NaN tail is not contiguous"
        n_stim = int(edge.indices[speed].shape[0])
        timing.append({
            "speed": speed,
            "t_stim": t,
            "n_stim": n_stim,
            "n_pre": int((t + edge.t_pre) / DT) - n_stim,
            "n_valid": n_valid,
            "indices": [int(i) for i in edge.indices[speed].cpu().numpy()],
        })
    (OUT / "moving_edge_timing.json").write_text(
        json.dumps(
            {
                "dt": DT,
                "n_frames": n_frames,
                "t_stim_max": float(edge.t_stim_max),
                "n_sequences_full_family": 12 * 2 * len(speeds),
                "speeds": timing,
            },
            indent=2,
        )
        + "\n"
    )
    for row in timing:
        print(f"  speed {row['speed']:>5}: t_stim {row['t_stim']:.6f} "
              f"stim {row['n_stim']:>4} pre {row['n_pre']:>3} valid {row['n_valid']:>5} "
              f"of {n_frames}")

    # -- provenance -----------------------------------------------------------
    (OUT / "moving_edges_provenance.json").write_text(
        json.dumps(
            {
                "exported": datetime.date.today().isoformat(),
                "flyvis": getattr(flyvis, "__version__", "?"),
                "torch": torch.__version__,
                "numpy": np.__version__,
                "dt": DT,
                "gate_angles": GATE_ANGLES,
                "n_ommatidia": N_OMMATIDIA,
                "ppo": PPO,
                "monitor_px": MONITOR,
                "offsets_shape": list(offsets.shape),
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

- [ ] **Step 2: Run the script**

Run: `uv run --project tools tools/export_moving_edges.py`

Expected output, matching the figures this plan was written against:

```
  hex_eye_owner.bin: 382383 owned, 218242 unowned
  hex_eye_kernel_sum.json: min 513 max 545 sum 382383
  moving_edge_offsets.bin: (8, 21, 721), 484344 bytes
  background at angle 0: 0.500000 (unrotated, exact)
  background at angle 30: 0.498039 (127/255 = 0.498039)
  speed   2.4: t_stim 3.394397 stim  678 pre 200 valid  1078 of 1078
  speed   4.8: t_stim 1.697198 stim  339 pre 200 valid   739 of 1078
  speed   9.7: t_stim 0.839851 stim  167 pre 200 valid   567 of 1078
  speed    13: t_stim 0.626658 stim  125 pre 200 valid   525 of 1078
  speed    19: t_stim 0.428766 stim   85 pre 200 valid   485 of 1078
  speed    25: t_stim 0.325862 stim   65 pre 200 valid   465 of 1078
  all assertions passed
```

The first run renders 8 angle/intensity combinations and takes on the order of a
minute. flyvis caches the result in its own `renderings` directory, keyed by
config, so a second run is fast.

If `assert n_owners.max() <= 1` fires, **stop** — the one-`u16`-per-pixel
representation in `hex_eye` is invalid and Task 2's design has to change.

- [ ] **Step 3: Point the skip messages at the right script**

In `crates/flyvis/src/test_support.rs`, both `load_stimulus_fixture` and
`load_stimulus_bytes` tell a reader to run `export_stimuli.py`. There are now two
scripts writing into that directory. Change both messages:

```rust
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_stimuli.py` and \
                 `uv run --project tools tools/export_moving_edges.py` \
                 to generate the stimulus fixtures."
            );
```

- [ ] **Step 4: Add the script to `tools/README.md`**

Add a row to the table, after the `export_stimuli.py` row:

```markdown
| `export_moving_edges.py` | flyvis's own rendered moving-edge offsets for four angles, its pixel-to-ommatidium mask, and the resample/pad timing — the reference for the one tolerance-gated comparison in the workspace | `data/flyvis-stimuli/` |
```

- [ ] **Step 5: Verify nothing else broke**

Run: `cargo test --workspace`
Expected: PASS, unchanged.

- [ ] **Step 6: Commit**

```bash
git add tools/export_moving_edges.py tools/README.md crates/flyvis/src/test_support.rs
git commit -m "Export flyvis's moving edge renderings as fixtures"
```

`data/` is gitignored — do not add the generated files.

---

### Task 7: The acceptance gates

**Files:**
- Create: `crates/flyvis/src/moving_edge_checks.rs`
- Modify: `crates/flyvis/src/lib.rs`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: `HexEye::new`, `HexEye::owners`, `HexEye::kernel_sum`, `NO_OWNER`
  (Tasks 1-2); `MovingEdge::new`, `MovingEdgeParams::flyvis_defaults`,
  `MovingEdge::{angle_of, intensity_of, speed_of, t_stim_of, valid_frames,
  resample_index}`, the `Stimulus` impl (Task 5);
  `test_support::{load_stimulus_fixture, load_stimulus_bytes}`; the fixtures from
  Task 6.
- Produces: no library API. Tests only.

**What is gated how.** Only the *geometry* carries a tolerance. The mask
divergence, the sequence ordering, the resample indices, the pad lengths and the
NaN boundary are all compared element-wise, exactly as the flash and dot gates
are. Keeping the tolerance to the one place it is unavoidable is what stops
"moving edges are gated against flyvis" from quietly meaning less than it does
elsewhere.

- [ ] **Step 1: Write the gates**

Create `crates/flyvis/src/moving_edge_checks.rs`:

```rust
//! Acceptance criteria for the moving edge generator.
//!
//! **This module holds the only tolerance-gated comparison in the workspace.**
//! Everywhere else -- the connectome's 1,513,231 edges, the flash and dot
//! families, the lattice order -- agreement with flyvis is element-wise. Here
//! it cannot be: flyvis rotates a `uint8` raster through Pillow's bilinear
//! resampler and this crate rotates the sampling geometry instead, by an
//! explicit decision recorded in the spec.
//!
//! So the geometry gate prints its observed distribution rather than merely
//! asserting a bound. The difference between 0.002 and 0.009 agreement is the
//! difference between "that is the quantisation artifact" and "something else
//! is wrong", and a pass/fail assertion hides it.
//!
//! Everything that *can* be compared element-wise still is: the ownership map,
//! the sequence ordering, the resample indices, the pad lengths and the NaN
//! boundary.
//!
//! These skip when their fixtures are absent, and the skip notice is invisible
//! under a plain `cargo test`. Set `SEMBLANCE_REQUIRE_FIXTURES=1` to turn a
//! missing fixture into a failure.
//!
//! The geometry gate renders twelve angles' worth of offsets and is slow in
//! debug; `cargo test --release` is the comfortable way to run it.

use crate::hex_eye::{HexEye, NO_OWNER};
use crate::moving_edge::{MovingEdge, MovingEdgeParams};
use crate::stimuli::Stimulus;
use crate::test_support::{load_stimulus_bytes, load_stimulus_fixture};

const DT: f64 = 1.0 / 200.0;
const GATE_ANGLES: [i32; 4] = [0, 30, 90, 150];

/// The bound this gate asserts, per hexal.
///
/// Measured before implementation, across angles 0/30/90/150/210, offsets
/// -10/3/10 and both intensities: max 0.00306, mean 0.0018, zero hexals over
/// 0.005. The mean is a near-uniform floor of `0.5 - 127/255 = 0.00196` --
/// flyvis's background after its truncating cast to `uint8`.
///
/// If the observed distribution exceeds this, **do not widen it.** That would
/// mean the analytic and rasterised renderers differ by more than quantisation,
/// and the cause needs finding before the gate means anything. The likeliest
/// cause is the rotation centre: `monitor/2` instead of `monitor/2 - 0.5` moves
/// the maximum to 0.032 and puts up to 39 hexals over 0.01.
const TOLERANCE: f64 = 0.005;

fn f32s(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn u16s(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// Pulls the integers out of a flat JSON array of numbers.
fn json_ints(s: &str) -> Vec<i64> {
    s.split(|c: char| !(c.is_ascii_digit() || c == '-'))
        .filter(|t| !t.is_empty())
        .map(|t| t.parse().unwrap())
        .collect()
}

/// Reads one `"key": <number>` out of a JSON object, as a string.
fn json_field<'a>(s: &'a str, key: &str) -> &'a str {
    let needle = format!("\"{key}\":");
    let start = s.find(&needle).unwrap_or_else(|| panic!("no field {key}")) + needle.len();
    let rest = &s[start..];
    let end = rest.find([',', '}', '\n']).unwrap_or(rest.len());
    rest[..end].trim()
}

/// The geometry gate. Tolerance-gated, and it prints what it observed.
#[test]
fn moving_edge_offsets_match_flyvis_within_tolerance() {
    let (Some(bytes), Some(manifest)) = (
        load_stimulus_bytes("moving_edge_offsets.bin"),
        load_stimulus_fixture("moving_edge_offsets.json"),
    ) else {
        return;
    };
    let want = f32s(&bytes);
    assert_eq!(want.len(), 8 * 21 * 721, "fixture value count");
    let angles = json_ints(json_field(&manifest, "shape"));
    assert_eq!(angles[0], 8, "4 angles x 2 intensities");

    let mut params = MovingEdgeParams::flyvis_defaults(DT);
    params.angles = GATE_ANGLES.to_vec();
    let e = MovingEdge::new(params);

    // Per regime, so the three numerical footings stay distinguishable.
    let mut report: Vec<(i32, f64, f64, usize, usize)> = Vec::new();
    let mut out = vec![0.0f32; 721];

    for (ai, &angle) in GATE_ANGLES.iter().enumerate() {
        let (mut max, mut sum, mut over_tol, mut over_hundredth) = (0.0f64, 0.0f64, 0, 0);
        for (ii, _) in [0.0f32, 1.0].iter().enumerate() {
            // Sequence index is angle-major, then intensity, then speed. The
            // slowest speed (index 0) sweeps all 21 offsets over 678 frames; we
            // compare the offsets directly by stepping to the frame that
            // selects each one.
            let sequence = (ai * 2 + ii) * e.speed_count();
            for offset in 0..21 {
                let frame = e.first_frame_selecting(sequence, offset).unwrap();
                e.frame_into(sequence, frame, &mut out).unwrap();
                let base = ((ai * 2 + ii) * 21 + offset) * 721;
                for (h, &got) in out.iter().enumerate() {
                    let d = (got as f64 - want[base + h] as f64).abs();
                    max = max.max(d);
                    sum += d;
                    if d > TOLERANCE {
                        over_tol += 1;
                    }
                    if d > 0.01 {
                        over_hundredth += 1;
                    }
                }
            }
        }
        let n = (2 * 21 * 721) as f64;
        report.push((angle, max, sum / n, over_tol, over_hundredth));
    }

    // Print BEFORE asserting: a failing gate must still say what it saw.
    eprintln!("moving edge geometry, against flyvis 1.2.0 ({} values per angle):", 2 * 21 * 721);
    eprintln!("  angle   max abs      mean   > {TOLERANCE}   > 0.01   regime");
    for &(angle, max, mean, over_tol, over_hundredth) in &report {
        let regime = match angle {
            0 => "no rotation",
            90 | 180 | 270 => "axis-aligned",
            _ => "bilinear",
        };
        eprintln!("  {angle:>5}   {max:.6}  {mean:.6}   {over_tol:>7}   {over_hundredth:>6}   {regime}");
    }
    eprintln!(
        "  expected floor: 0.5 - 127/255 = {:.6}, flyvis's background after its uint8 cast",
        0.5 - 127.0 / 255.0
    );

    for &(angle, max, _, over_tol, _) in &report {
        assert!(
            max <= TOLERANCE,
            "angle {angle}: max abs difference {max:.6} exceeds {TOLERANCE}. \
             Do not widen this bound -- see the TOLERANCE doc comment. \
             The likeliest cause is the rotation centre."
        );
        assert_eq!(over_tol, 0, "angle {angle}: hexals over tolerance");
    }
}

/// The ownership map, against flyvis's own. Element-wise, with a bound on the
/// disagreement rather than on a value.
///
/// This is a localisation aid, not a correctness claim: flyvis builds its mask
/// in `float16` and this crate builds it in `f64`, so some disagreement is
/// expected and its size is what matters. Measured: 2,052 of 600,625 pixels.
#[test]
fn the_ownership_map_agrees_with_flyviss_fp16_mask() {
    let (Some(bytes), Some(ks_json)) = (
        load_stimulus_bytes("hex_eye_owner.bin"),
        load_stimulus_fixture("hex_eye_kernel_sum.json"),
    ) else {
        return;
    };
    let want = u16s(&bytes);
    assert_eq!(want.len(), 775 * 775, "fixture pixel count");

    let eye = HexEye::new(15, 25);
    let got = eye.owners();
    assert_eq!(got.len(), want.len());

    let differing = got.iter().zip(&want).filter(|(a, b)| a != b).count();
    let ours_unowned = got.iter().filter(|&&o| o == NO_OWNER).count();
    let theirs_unowned = want.iter().filter(|&&o| o == NO_OWNER).count();

    let want_ks: Vec<i64> = json_ints(&ks_json);
    assert_eq!(want_ks.len(), 721);
    let ks_delta: i64 = eye
        .kernel_sum()
        .iter()
        .zip(&want_ks)
        .map(|(&a, &b)| (a as i64 - b).abs())
        .max()
        .unwrap();

    eprintln!("pixel ownership, f64 here against flyvis's float16:");
    eprintln!("  differing assignments: {differing} of {}", want.len());
    eprintln!("  unowned pixels: {ours_unowned} here, {theirs_unowned} in flyvis");
    eprintln!("  largest per-ommatidium pixel-count difference: {ks_delta}");

    assert!(
        differing <= 4_000,
        "{differing} pixels assigned differently; 2,052 was measured. \
         A large jump means the geometry diverged, not that fp16 got noisier."
    );
    assert!(ks_delta <= 24, "largest kernel-sum difference {ks_delta}; 12 was measured");
}

/// The sequence ordering and the published figures.
#[test]
fn the_full_family_has_flyviss_shape() {
    let Some(timing) = load_stimulus_fixture("moving_edge_timing.json") else { return };
    let e = MovingEdge::new(MovingEdgeParams::flyvis_defaults(DT));
    let n_frames: usize = json_field(&timing, "n_frames").parse().unwrap();
    let n_sequences: usize = json_field(&timing, "n_sequences_full_family").parse().unwrap();

    assert_eq!(e.num_sequences(), n_sequences, "144 = 12 x 2 x 6");
    assert_eq!(e.num_frames(), n_frames, "1078 frames");
    assert_eq!(e.num_hexals(), 721);
}

/// The resample indices and the pad boundaries, element-wise.
#[test]
fn the_timing_matches_flyvis_frame_for_frame() {
    let Some(json) = load_stimulus_fixture("moving_edge_timing.json") else { return };
    let e = MovingEdge::new(MovingEdgeParams::flyvis_defaults(DT));

    // One object per speed, in the same order as `speeds`.
    let blocks: Vec<&str> = json.split("\"speed\":").skip(1).collect();
    assert_eq!(blocks.len(), 6, "six speeds");

    for (s, block) in blocks.iter().enumerate() {
        let n_stim: usize = json_field(block, "n_stim").parse().unwrap();
        let n_valid: usize = json_field(block, "n_valid").parse().unwrap();
        let indices = json_ints(json_field(block, "indices"));
        assert_eq!(indices.len(), n_stim, "speed index {s}: index count");

        // Sequence `s` is angle 0, intensity 0, speed index `s`.
        assert_eq!(e.valid_frames(s), Some(n_valid), "speed index {s}: valid frames");
        for (k, &want) in indices.iter().enumerate() {
            assert_eq!(
                e.resample_index(s, k),
                Some(want as usize),
                "speed index {s}, swept frame {k}"
            );
        }
        assert_eq!(e.resample_index(s, n_stim), None, "speed index {s}: past the sweep");
    }
}

/// The NaN tail starts exactly where flyvis's does.
#[test]
fn the_nan_boundary_matches_flyvis() {
    let Some(json) = load_stimulus_fixture("moving_edge_timing.json") else { return };
    let mut params = MovingEdgeParams::flyvis_defaults(DT);
    params.angles = vec![0];
    let e = MovingEdge::new(params);

    let blocks: Vec<&str> = json.split("\"speed\":").skip(1).collect();
    let mut out = vec![0.0f32; 721];
    for (s, block) in blocks.iter().enumerate() {
        let n_valid: usize = json_field(block, "n_valid").parse().unwrap();
        e.frame_into(s, n_valid - 1, &mut out).unwrap();
        assert!(out.iter().all(|x| x.is_finite()), "speed index {s}: frame {} must be real", n_valid - 1);
        if n_valid < e.num_frames() {
            e.frame_into(s, n_valid, &mut out).unwrap();
            assert!(out.iter().all(|x| x.is_nan()), "speed index {s}: frame {n_valid} must be NaN");
        }
    }
}
```

The gate calls two accessors that do not exist yet. Add them to
`impl MovingEdge` in `crates/flyvis/src/moving_edge.rs`:

```rust
    /// How many speeds the family sweeps. Sequence indices step by this within
    /// one (angle, intensity) pair.
    pub fn speed_count(&self) -> usize {
        self.speeds.len()
    }

    /// The first frame of `sequence` that selects `offset`, counting from frame
    /// zero, or `None` if the sweep never selects it.
    ///
    /// The resample repeats and skips offsets, so a fast sweep does not visit
    /// all 21. The slowest speed always does.
    pub fn first_frame_selecting(&self, sequence: usize, offset: usize) -> Option<usize> {
        let (_, _, s) = self.decode(sequence)?;
        let k = self.indices[s].iter().position(|&o| o == offset)?;
        Some(self.timing[s].n_pre + k)
    }
}
```

And declare the checks module in `crates/flyvis/src/lib.rs`, after
`pub mod moving_edge;`:

```rust
#[cfg(test)]
mod moving_edge_checks;
```

- [ ] **Step 2: Run the gates with fixtures present**

Run: `cargo test -p semblance-flyvis --release moving_edge_checks -- --nocapture`

Expected: PASS, with a table resembling:

```
moving edge geometry, against flyvis 1.2.0 (30282 values per angle):
  angle   max abs      mean   > 0.005   > 0.01   regime
      0   0.000794  0.000010         0        0   no rotation
     30   0.002570  0.001850         0        0   bilinear
     90   0.002690  0.001880         0        0   axis-aligned
    150   0.003060  0.001850         0        0   bilinear
  expected floor: 0.5 - 127/255 = 0.001961, flyvis's background after its uint8 cast
pixel ownership, f64 here against flyvis's float16:
  differing assignments: 2052 of 600625
  unowned pixels: 217012 here, 218242 in flyvis
  largest per-ommatidium pixel-count difference: 12
```

The exact values may differ in the last digits; the shape of the table and the
zero counts in both threshold columns are what matter. **If any angle exceeds
0.005, stop and report the table** — check the rotation centre first.

- [ ] **Step 3: Prove the gates actually run**

Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --release`
Expected: PASS. This is the run that proves no gate is silently skipping.

Then temporarily move one fixture aside and confirm the failure is loud:

```bash
mv data/flyvis-stimuli/moving_edge_offsets.bin /tmp/
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis --release moving_edge_checks
mv /tmp/moving_edge_offsets.bin data/flyvis-stimuli/
```

Expected: the middle command FAILS with `SEMBLANCE_REQUIRE_FIXTURES=1 but ... is
missing`. Report that you saw it fail.

- [ ] **Step 4: Update `CLAUDE.md`**

Run `cargo test --workspace` and `cargo test --workspace -- --nocapture` and use
the counts they actually report. Then, in `CLAUDE.md`:

1. In the "Verifying a change" block, replace `# 150 tests, 0 failures` with the
   new count.
2. Replace `21 of those tests skip when ...` with the new skip count, and extend
   the sentence naming the export scripts so it reads
   `... (all three are gitignored real-data; the first is fetched per
   RESOURCES.md, the other two produced by tools/export_build.py, and
   tools/export_stimuli.py with tools/export_moving_edges.py respectively)`.
3. Add this paragraph immediately after the `roundtrip` paragraph:

```markdown
**One comparison in this workspace carries a tolerance, and only one.** The
moving-edge stimuli (`crates/flyvis/src/moving_edge_checks.rs`) are gated
against flyvis at a measured bound rather than element-wise, because flyvis
rotates a `uint8` raster through Pillow's bilinear resampler and this crate
rotates the sampling geometry instead — a decision recorded in
`docs/superpowers/specs/2026-09-30-moving-edge-stimuli-design.md`. That gate
prints its observed distribution; read it, do not just watch it pass. Every
other comparison — the connectome's 1,513,231 edges, the flash and dot
families, the lattice order — is element-wise, and "gated against flyvis"
means something stronger everywhere else.
```

- [ ] **Step 5: Full verification**

```bash
cargo build --workspace
cargo test --workspace
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --release
cargo clippy --workspace --all-targets
cargo run --quiet -- roundtrip
```

Expected: all pass; clippy shows the 5 pre-existing warnings and no more;
`roundtrip` prints `0.000000` twice.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/moving_edge_checks.rs crates/flyvis/src/moving_edge.rs crates/flyvis/src/lib.rs CLAUDE.md
git commit -m "Gate the moving edges against flyvis at a measured tolerance"
```

---

## Plan self-review

**Spec coverage.**

| Spec requirement | Task |
|---|---|
| `MovingEdge` only, 144 sequences | 5 (`num_sequences`), 7 (gated) |
| Reuse `Stimulus`, `hex_coords`, `frame_count`, `dt` as an argument | 1, 5 |
| The three `MovingEdge` overrides (`widths=[80]`, `bar_loc_horizontal=radians(0)`, `post_pad_mode="continue"`) | 3 (first two), 5 (the third) |
| `led_width` spatial vs `omm_width` temporal | 3 (params test), 5 (`t_stim`) |
| 21 static offsets, then `resample` | 4 (offsets), 5 (resample) |
| Padding, and `"continue"` holding the last frame | 5 |
| Ommatidium geometry | 1 |
| Pixel-to-ommatidium `u16` assignment, **with the one-owner check** | 2 (checked in `HexEye::new` and independently in a test); flyvis's own mask is checked in 6's script |
| Analytic bar membership by inverse-rotating the query point | 3, 4 |
| Tolerance gate that **prints the distribution** | 7 |
| Subset covering 0, 90, and two bilinear angles | 6 (`GATE_ANGLES = [0, 30, 90, 150]`) |
| Published figures: 144, 721, 21, frame counts | 5, 7 |
| `SEMBLANCE_REQUIRE_FIXTURES=1` honoured | 7 step 3 proves it |
| Module doc states this is the only tolerance gate | 5 (`moving_edge.rs`), 7 (`moving_edge_checks.rs`, `CLAUDE.md`) |
| Stop rather than widen if the distribution exceeds ~0.005 | 7 (`TOLERANCE` doc, assertion message) |
| Debug-vs-release cost noted | 4 step 4, 7 module doc |

**Gaps the spec left, and how this plan closed them.**

- **The NaN tail.** The spec says "pad ... to a length common across speeds" but
  flyvis uses a third pad with `NaN` as a stacking sentinel. Task 5 reproduces it
  and adds `valid_frames`; the reasoning is stated there.
- **The rotation centre.** The spec says "inverse-rotate `p` about the monitor
  centre" without saying where that is in index coordinates. Measured before
  writing: `monitor/2 - 0.5`. Task 3 pins it with a test that rejects the
  plausible wrong answer.
- **Pixel sharing.** The spec required a check and a fallback. Measured: no pixel
  is ever shared, in f64 or in flyvis's fp16. The check stays (Task 2, and the
  export script asserts it against flyvis's own mask); the fallback is not built,
  because building an unexercised second representation would be the larger risk.
- **`MovingBar`** is out of scope, as the spec says. Nothing here forecloses it:
  `MovingEdgeParams::width` is a field, not a constant.

**Type consistency.** `HexEye::centre` returns `(f64, f64)` as `(column, row)`
and every consumer uses that order; `owner(col, row)` matches and Task 2 has a
test that rejects the transpose. `Coverage::counts(angle_index, offset_index)`
and `Coverage::kernel_sum()` both return `&[u32]` of length `n_hexals`.
`inverse_rotate` returns `(i64, i64)` as `(x, y)` = `(column, row)`, and
`BarGeometry::rows`/`cols` are `(i64, i64)` half-open ranges compared against
those. `frame_count` is `f64` from Task 5 onward and Task 5 step 2 proves the
widening is neutral for the existing families.
