# Flash and dot stimuli, generated in Rust

**Date:** 2026-09-28
**Status:** approved, not yet implemented
**Epic:** 0.2.0 flyvis import, task 8 sub-epic (8a and 8b) —
`docs/project-outline/0.2.0-flyvis-import/task-8-stimulus-set/`
**Surveys this argues from:** `survey-flashes.md`, `survey-dots-and-shared.md`

## Why

0.2.0's remaining task is the stimulus set. Its sub-epic splits into three
families; this spec covers the two that need no eye model — circular ON/OFF
flashes and single-ommatidium dots. Moving edges (8c) are blocked on an open
decision about whether to reproduce a Pillow rasterisation artifact, and are out
of scope here.

Flashes are not merely the easy one. The Flash Response Index is the headline
behavioural metric in 0.4.0, covering 32 cell types, and it is computed entirely
from this stimulus. Dots are what a receptive-field measurement needs.

## Scope

**In:**

1. A shared hex-coordinate iterator, extracted from the existing `place_nodes`.
2. A minimal `Stimulus` trait: lazy, frame-at-a-time.
3. `Flashes` — 4 sequences.
4. `Dots` — 1,442 sequences.
5. `tools/export_stimuli.py` and fixture-gated acceptance tests.

**Out — deliberately:**

- **Moving edges and bars (8c).** Blocked on decision D1 in the sub-epic README.
- **Sintel.** Explicitly deferred; a real external download.
- **The hexal-to-cell mapping.** Which cells receive a stimulus is 0.3.0's task 4.
  This spec stops at a 721-vector per frame.
- **Any forward pass.** No dynamics here.
- Changes to `connectome`, `codec`, `fidelity`, `sim`, `evo`, `app`.

## Placement

`semblance-flyvis`, a new `stimuli` module. These reproduce *flyvis's* stimuli
and depend on flyvis's lattice conventions, so they belong beside the importer,
not in a general crate.

This does mean `semblance-sim` — which will hold 0.3.0's integrator and does not
depend on `flyvis` — cannot reach them directly. That is correct and not a
problem: the integrator consumes `x_t: &[f32]` per step, and whatever drives a
validation run supplies it. The driver may depend on both.

## The lattice ordering is already correct — extract it, do not rewrite it

The single highest risk in this work was that a stimulus's flat index `k` might
not correspond to the same `(u, v)` the connectome assigned to node `k`. Every
stimulus is a 721-vector consumed by photoreceptor index, and a mismatch
scrambles it spatially while leaving shape, frame count and dynamics entirely
plausible — the same failure class as the `syn_count` log-space trap and the
`Lawf1` index shift.

**It is already solved.** `hex_utils.get_hex_coords` and the connectome's
`add_strided_nodes` are the same loop, verified side by side, and
`crates/flyvis/src/expand.rs::place_nodes` already implements it and is gated
against flyvis's own `nodes.csv`.

So: **extract the bare loop** so the shared ordering is one named, tested thing
rather than an incidental agreement between two call sites.

```rust
/// The canonical hex-lattice order: outer `u` ascending, inner `v` ascending
/// over the hexagonal bound. This single ordering is shared by the connectome's
/// node construction and by every stimulus's hexal indexing; flyvis derives it
/// independently in both places from textually identical loops.
pub(crate) fn hex_coords(extent: i32) -> impl Iterator<Item = (i32, i32)> {
    (-extent..=extent).flat_map(move |u| {
        ((-extent).max(-extent - u)..=extent.min(extent - u)).map(move |v| (u, v))
    })
}
```

`place_nodes` is rewritten to consume it, keeping its stride filter. Its existing
tests must continue to pass unchanged — that is the proof the extraction is
behaviour-preserving.

At `extent = 15` this yields 721 pairs, index 0 is `(-15, 0)`, and the centre
`(0, 0)` is at index **360**.

## The API: lazy, because dots cannot be materialised

Dots is 1,442 sequences × 1,400 frames × 721 hexals = **5.8 GB** as `f32`. It
cannot be returned as a batch. flyvis has the same constraint and solves it with
`__getitem__`; this mirrors that.

```rust
/// A family of rendered stimulus sequences, generated on demand.
pub trait Stimulus {
    /// Number of distinct sequences in this family.
    fn num_sequences(&self) -> usize;
    /// Frames per sequence. Uniform across the family.
    fn num_frames(&self) -> usize;
    /// Hexals per frame — 721 at extent 15.
    fn num_hexals(&self) -> usize;

    /// Writes one frame's intensities into `out`, which must be
    /// `num_hexals()` long.
    ///
    /// Frame-at-a-time rather than sequence-at-a-time because this is what an
    /// integrator consumes: one stimulus vector per Euler step. A whole dot
    /// sequence is 4 MB; the family is 5.8 GB.
    fn frame_into(&self, sequence: usize, frame: usize, out: &mut [f32])
        -> Result<(), StimulusError>;
}
```

`StimulusError` covers out-of-range sequence, out-of-range frame, and wrong
buffer length. Same validate-don't-trust posture as `TypedNetwork::new`.

**Each family also exposes what a sequence *is*.** The orderings below are
deterministic and documented, so a consumer could re-derive them — but 0.4.0's
FRI has to separate ON from OFF, and re-deriving an ordering at every call site
is exactly how ordering bugs enter. These are not on the trait, because the two
families are described by different things:

```rust
impl Flashes {
    /// The flash intensity for a sequence: `dynamic_range[0]` or `[1]`.
    pub fn intensity_of(&self, sequence: usize) -> Option<f32>;
    /// The radius, with `-1` meaning full field.
    pub fn radius_of(&self, sequence: usize) -> Option<i32>;
}

impl Dots {
    /// The lit centre's lattice coordinate.
    pub fn centre_of(&self, sequence: usize) -> Option<(i32, i32)>;
    /// The flash intensity: `2 * bg_intensity - intensity`, or `intensity`.
    pub fn intensity_of(&self, sequence: usize) -> Option<f32>;
}
```

## Shared semantics

**`dt` is a generation-time parameter, never a constant.** `Network.stimulus_response`
assigns `stim_dataset.dt = dt` before pulling any item, so stimuli render at
whatever the caller supplies. flyvis's own response analyses supply **1/200**.
`0.02` is the Sintel optical-flow *training* step and belongs to no part of this
pipeline. See `finding-dt-and-frozen-time-constants.md` — this is not cosmetic;
`dt` changes which cell types `max(tau, dt)` floors.

**Frame counts truncate.** flyvis computes `int(t / dt)`, truncating toward zero.
Rust must use `(t / dt) as usize`, not `.round()`. At the defaults this lands on
exactly 200/1000/400 with no off-by-one, but the rule is truncation.

**`resample` is frame repeat, not interpolation.** `linspace(0, n-1, n_target).long()`
truncates to integer indices and selects. For these two families the input is
always a single static frame, so it degenerates to "repeat this frame N times".
The generator therefore needs no resampling machinery at all — just a frame count.

## Flashes (8a)

```rust
pub struct FlashParams {
    pub extent: i32,              // 15
    pub dynamic_range: [f32; 2],  // [0.0, 1.0]
    pub t_stim: f32,              // 1.0 s
    pub t_pre: f32,               // 1.0 s
    pub dt: f32,                  // caller's; flyvis analyses use 1.0 / 200.0
    pub radii: Vec<i32>,          // [-1, 6]
    pub alternations: Vec<u8>,    // [0, 1, 0]
}
```

`baseline = (dynamic_range[0] + dynamic_range[1]) / 2`, i.e. 0.5 by default.

Per sequence: every hexal at `baseline`, then the selected set at `intensity`.
Frames are `alternations` expanded — `0` yields `(t_pre / dt) as usize` baseline
frames, `1` yields `(t_stim / dt) as usize` stimulus frames. Defaults give
200 + 200 + 200 = **600 frames**.

**`radius = -1` is a full-field sentinel, not a geometric radius.** It selects
every hexal and touches no hex geometry. A port that treats it as a distance
produces an empty or nonsensical mask.

**The disc** at radius `r` is every `(u, v)` with hex distance ≤ `r` from the
centre — boundary **included**. Hex distance is
`(|du| + |du+dv| + |dv|) / 2`, always an exact integer. Radius 6 selects 127 of
721.

**Sequence ordering is intensity-major, radius-minor.** flyvis builds it with
`product(zip(baseline, intensity), radius)`, and `product` varies its last
argument fastest:

| index | intensity | radius |
|---|---|---|
| 0 | 0.0 (OFF) | −1 (full field) |
| 1 | 0.0 (OFF) | 6 (disc) |
| 2 | 1.0 (ON) | −1 (full field) |
| 3 | 1.0 (ON) | 6 (disc) |

Getting this backwards mislabels every sample after index 0, and nothing about
the array announces it.

## Dots (8b)

```rust
pub struct DotParams {
    pub extent: i32,             // 15
    pub dot_column_radius: i32,  // 0 — a single ommatidium
    pub bg_intensity: f32,       // 0.5
    pub intensity: f32,          // 1.0
    pub t_stim: f32,             // 5.0 s
    pub t_pre: f32,              // 2.0 s
    pub t_post: f32,             // 0.0 s
    pub dt: f32,                 // caller's
}
```

Intensities are `[2 * bg_intensity - intensity, intensity]` → `[0.0, 1.0]` by
default: a dark flash mirrored below background, and a bright one. Both
polarities are separate sequences for every centre.

Per sequence: `(t_pre / dt) as usize` background frames, then
`(t_stim / dt) as usize` frames with the lit hexals at `intensity`, then
`(t_post / dt) as usize` background frames. Defaults give 400 + 1000 + 0 =
**1,400 frames**.

At `dot_column_radius = 0` exactly one hexal is lit; at `r > 0` it is the filled
disc of radius `r` around that centre, using the same distance rule as flashes.

**Sequence ordering is centre-major, intensity-minor** — the *opposite* grouping
from flashes, though both follow the same `product`-varies-last rule. flyvis
builds it with `product(zip(u, v, offsets, coordinate_indices), intensities)`:

```
sequence = centre_index * 2 + polarity     where polarity 0 = dark (0.0), 1 = bright (1.0)
```

721 centres × 2 polarities = **1,442 sequences**. Centres are enumerated in
`hex_coords` order, so centre 0 is `(-15, 0)` and centre 360 is the origin.

## Testing

Three layers, weakest dependency last — the pattern that made the connectome
expansion trustworthy.

**Unit.** `hex_coords` yields 721 pairs at extent 15, index 0 is `(-15, 0)`,
index 360 is `(0, 0)`; the hex-distance function agrees with hand-computed
values; a radius-6 disc has 127 members; frame counts truncate rather than round.
`place_nodes`'s existing tests must pass unchanged after the extraction.

**Fixture-gated.** `tools/export_stimuli.py` dumps flyvis's own rendered arrays:
`RenderedFlashes.flashes` as `(4, 600, 721)` float32, and a chosen subset of
`Dots()` sequences with their `arg_df` rows. Rust compares element-wise. Dots is
too large to dump whole, so the script exports a deterministic subset —
**centre 0, centre 360 (the origin), and centre 720, both polarities**, six
sequences — chosen to cover the first, middle and last of the centre ordering,
which is where an ordering error shows.

**Ordering.** Assert the sequence orderings above against the exported `arg_df`,
not against our own derivation. The two families order oppositely and both are
easy to state confidently and wrongly.

Every fixture test must honour `SEMBLANCE_REQUIRE_FIXTURES=1`, following
`test_support.rs`. A stimulus gate that silently skips is worse than none.

## Risks

**The extraction could change `place_nodes`.** It is used by the connectome
expansion, which is gated element-wise against flyvis across 45,669 nodes. If
those gates fail after the refactor, the extraction is wrong — revert it and
duplicate the loop rather than weakening a passing gate.

**`HexLattice.filled_circle` hardcodes its embedding lattice to extent 15**,
independent of both `radius` and `boxfilter.extent`. At the defaults this is
invisible because both are 15. A Rust port that derives the embedding extent
from its parameters is *more* correct and will diverge from flyvis at any other
extent. Reproduce flyvis's behaviour at extent 15, and record the divergence
rather than silently fixing it.

**Dots' size invites shortcuts.** 5.8 GB is unmaterialisable, so a reviewer
cannot diff the whole family. The subset gate is what stands in for that — do not
let it shrink to a single sequence.
