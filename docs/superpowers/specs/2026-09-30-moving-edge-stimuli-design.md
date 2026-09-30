# Moving edge stimuli, rendered analytically

**Date:** 2026-09-30
**Status:** approved, not yet implemented
**Epic:** 0.2.0 flyvis import, task 8 sub-epic (8c) —
`docs/project-outline/0.2.0-flyvis-import/task-8-stimulus-set/`
**Survey:** `task-8-stimulus-set/survey-moving-edges.md`
**Decision it implements:** D1, settled 2026-09-28 — render analytically, gate at a
measured tolerance

## Why

8c is the last piece of 0.2.0. Moving edges are what the Direction Selectivity
Index is computed from in 0.4.0, and DSI over T4/T5 is flyvis's headline result —
the one the project's claim is ultimately measured against.

Unlike 8a and 8b, this one cannot be gated element-wise, and that is a deliberate
choice rather than a shortfall. Understanding why is the whole of the design.

## The decision this rests on, and what it costs

flyvis renders a bar by rasterising it into a 775×775 image, rotating that image,
and mean-pooling each ommatidium's hexagonal footprint over the pixels. The
rotation goes through Pillow with a **truncating cast to `uint8` first**, and is
skipped entirely when the angle is a multiple of 360. So flyvis's twelve
directions sit on three different numerical footings:

| angle | rotation | background |
|---|---|---|
| 0° | none | 0.500000 exactly |
| 90°, 180°, 270° | axis-aligned | 0.498039 |
| the other eight | bilinear | 0.498039, edges smoothed |

`0.5 × 255 = 127.5`, truncated to `127`, and `127/255 = 0.498039`.

**We do not reproduce that.** 8c rotates the *sampling geometry* instead of the
raster: for each pixel, inverse-rotate the query point about the monitor centre
and test membership of the unrotated bar. Exact, uniform across all twelve
directions, and with no external numerics to port.

The survey estimated this piece as epic-sized, but that estimate was explicitly
conditional on bit-exactness — reproducing Pillow's 8-bit bilinear affine
rotation for eleven angles. With D1 decided, that work disappears, and what
remains is what the survey called "a small, fast, one-time precomputation with
no external numerics dependency."

### The cost, measured

flyvis's own renderer was run twice per angle, once normally and once with
**only** the `uint8` round-trip removed — identical padding, rotation and crop.
In hexal space:

| angle | max abs diff | mean | hexals over 0.01 |
|---|---|---|---|
| 0° | 0.00000 | 0.000000 | 0 |
| 30°, 90°, 150°, 210° | **0.00199** | 0.0019 | **0** |

Uniform, with no amplification at the bar's edge: each ommatidium mean-pools
roughly 625 pixels, which washes the per-pixel differences out. The concern that
hard edges versus bilinear-smoothed ones would diverge by ~0.5 at edge-crossing
hexals does not survive contact with the data.

**That measurement isolates the quantisation component only.** An analytic
renderer also replaces bilinear interpolation with exact membership; that
component is bounded by reasoning at ≲0.0016 given the pooling ratio, but is
*not* measured. **The implementation must measure it** rather than inherit this
estimate — see Testing.

## Scope

**In:** `MovingEdge` — 12 angles × 2 intensities × 6 speeds = **144 sequences**.

**Out:**

- **`MovingBar`** (432 sequences: the same machinery with `widths = [1, 2, 4]`).
  The epic names moving *edges*; bars are a superset available later at no
  design cost.
- **Bit-exact agreement with flyvis.** Ruled out by D1, deliberately.
- Any change to `Flashes`, `Dots`, or the `Stimulus` trait.
- The hexal-to-cell mapping, which is 0.3.0's.

## What is reused, not rebuilt

- **`Stimulus`** — `num_sequences` / `num_frames` / `num_hexals` / `frame_into`,
  lazy and frame-at-a-time, exactly as 8a and 8b.
- **`hex_coords`** — the single lattice ordering, already gated against flyvis's
  node table across 45,669 nodes.
- **`frame_count`** — `(t / dt) as usize`, truncating.
- **`dt` as a caller argument**, never a constant.

## The parameters

Verified against source in the survey. `MovingEdge` overrides three of
`MovingBar`'s defaults, and inheriting them wrongly is the likeliest silent error:

| parameter | value | note |
|---|---|---|
| `angles` | 0, 30, … 330 | 12 |
| `speeds` | 2.4, 4.8, 9.7, 13, 19, 25 | half-ommatidia per second |
| `intensities` | 0, 1 | |
| `offsets` | `-10..=10` | **21** positions |
| `widths` | **`[80]`** | a 180° bar — **differs** from `MovingBar`'s `[1, 2, 4]` |
| `bar_loc_horizontal` | **`radians(0)`** | **differs** from `MovingBar`'s `radians(90)` |
| `post_pad_mode` | **`"continue"`** | **differs** from `MovingBar`'s `"value"` |
| `led_width` | `radians(2.25)` | the **spatial** unit for widths and offsets |
| `omm_width` | `radians(5.8)` | **speed-to-time only**, never spatial |
| `t_pre`, `t_post` | 1.0, 1.0 | seconds |

**`led_width` versus `omm_width` is the trap here.** "Half ommatidia" scales by
`led_width`, the LED pitch — not by `omm_width`, the ommatidial pitch, which
appears only when converting speeds into durations. Using `omm_width` spatially
would scale every offset by 5.8/2.25 ≈ 2.6× and still produce a plausible
moving edge.

## Structure: 21 static positions, then time

flyvis never renders per frame. It renders the **21 offset positions** once per
(angle, width, intensity), then derives every speed from them:

1. Render 21 static frames — the only geometric work.
2. `resample` to `(t_stim / dt) as usize` frames, where `t_stim` follows from the
   speed via `omm_width`. This is the **same truncating-index selection** 8a and
   8b already use: `linspace(0, 20, n).long()`, frame repeat and skip, never
   interpolation.
3. Pad by `t_pre` before and `t_post` after, to a length common across speeds —
   faster speeds have shorter `t_stim` and so pad further.

So a 6× speed sweep costs 21 renders, not six sequences' worth.

`post_pad_mode = "continue"` means the trailing pad repeats the **final offset
frame**, not the background. Padding with background instead would make every
edge appear to vanish at the end of its sweep.

## The renderer

Three pieces, each independently testable.

**1. Ommatidium geometry.** Each of the 721 lattice positions maps to a centre in
monitor pixel coordinates. The monitor is `ppo × n_hex_circumference` square —
775×775 at the defaults.

**2. Pixel-to-ommatidium assignment.** Point-in-hexagon is six half-plane sign
tests. flyvis materialises this as a dense `(600625, 721)` boolean — 433 MB. We
store one `u16` per pixel instead, about 1.2 MB, **provided a pixel belongs to at
most one ommatidium**. That is what a tiling implies but is not what flyvis's
data structure asserts, so the implementation must check it and fall back to a
per-pixel list if boundary pixels turn out to be shared.

**3. Bar membership, analytically.** For pixel `p` at angle `θ`: inverse-rotate
`p` about the monitor centre by `θ`, then test whether it lies within the
unrotated bar's half-plane pair. No raster rotation, no interpolation, no
quantisation. A frame is then the per-ommatidium mean of its pixels' intensities.

## Testing

Three layers. The middle one is new to this project and is the reason this spec
needs care.

**Unit.** Ommatidium centres tile without gaps; the point-in-hexagon test agrees
with hand-computed cases; inverse-rotation by θ then θ is the identity; the 21
offsets span `-10..=10`; `resample` truncates; `post_pad_mode = "continue"`
repeats the last frame rather than the background.

**Tolerance-gated, against flyvis.** `tools/export_moving_edges.py` exports
flyvis's own rendered offsets for a chosen subset of angles. Rust renders the
same and compares.

**This is the first gate in this project that is not element-wise, and the report
must say so.** It asserts a *stated* bound, and it must **print the observed
distribution** — max, mean, and the count of hexals above 0.01 — not merely that
some threshold was cleared. A gate that only says "under tolerance" hides
whether the agreement is 0.002 or 0.009, and the difference between those is the
difference between "the quantisation artifact" and "something else is wrong".

Choose the subset to include **0°** (where flyvis applies no rotation at all, so
agreement should be tightest), **90°** (axis-aligned, quantisation only), and at
least two of the eight bilinear angles. Those three regimes are expected to
differ from each other, and a gate that lumps them together cannot show it.

**Published figures.** 144 sequences; 721 hexals; 21 offsets; frame counts
consistent with each speed's `t_stim`.

All fixture tests honour `SEMBLANCE_REQUIRE_FIXTURES=1`.

## Risks

**The tolerance could come out worse than measured.** The ≲0.0016 bound on the
interpolation component is reasoning, not measurement. If the observed
distribution exceeds roughly 0.005 per hexal, stop and report it rather than
widening the bound to fit — that would mean the analytic and rasterised
renderers differ by more than quantisation, and the cause needs finding before
the gate means anything.

**A gate with a tolerance is weaker than one without, and this is the project's
only one.** Everything else compares element-wise. State that plainly in the
module doc, so nobody later reads "moving edges are gated against flyvis" as
implying the same strength as the connectome's 1,513,231-edge comparison.

**`post_pad_mode` and the three `MovingEdge` overrides** are inherited wrongly by
default. Every one produces a plausible moving edge.

**Scale.** 775×775 pixels × 21 offsets × 12 angles is 195M membership tests per
intensity. Trivial in release, slow in debug — the fixture gate should note it,
as the dot gate already does.
