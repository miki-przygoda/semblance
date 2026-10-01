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

use crate::hex_eye::{HexEye, NO_OWNER};

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

/// How many of each ommatidium's pixels the bar covers, per angle and offset.
///
/// This is the whole of the geometric work: 12 angles x 21 offsets, built once.
/// A frame is then arithmetic on these counts -- an ommatidium's value is
/// `(covered * intensity + uncovered * background) / total`, computed from
/// integers, so it does not depend on the order pixels are summed in.
///
/// Counting rather than summing intensities also means both intensities come
/// from one pass: the cartesian image only ever holds two values.
// Consumed starting in the task that builds frames from these counts; nothing
// in this crate calls `Coverage` yet outside the `#[cfg(test)]` module below.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct Coverage {
    n_hexals: usize,
    n_offsets: usize,
    /// `angles.len() * n_offsets * n_hexals`, angle-major then offset.
    counts: Vec<u32>,
    kernel_sum: Vec<u32>,
}

#[allow(dead_code)]
impl Coverage {
    /// Renders every angle's 21 offsets.
    ///
    /// One pass over the monitor per angle finds the pixels whose rotated
    /// source lands in the bar's row band -- 88 of the monitor's 775 rows --
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
    /// whole negative sweep by a pixel. This guards both truncation sites in
    /// `BarGeometry::new`: `loc_horizontal_px` (the gap assertions, which pin
    /// its symmetry either side of zero) and `loc_w` (the absolute assertion,
    /// which pins its value directly -- a flooring regression there shifts
    /// every stop by a uniform one pixel and leaves the gaps untouched).
    #[test]
    fn offsets_truncate_toward_zero_rather_than_flooring() {
        let g = BarGeometry::new(&params(), 775);
        // offset 0 -> loc_w -387 -> stop 389; offset -1 -> loc_w -396 -> 380.
        // The gap either side of centre is 9, symmetric. Flooring the negative
        // side would make it 10.
        assert_eq!(g.cols[10].1 - g.cols[9].1, 9);
        assert_eq!(g.cols[11].1 - g.cols[10].1, 9);
        // `loc_w = trunc(h_px - 775/2)`. At offset 0 that is `trunc(-387.5)`,
        // which is -387 toward zero and -388 under `floor` -- a uniform
        // one-pixel shift of every stop, which the gaps above cannot see.
        assert_eq!(g.cols[10], (0, 389));
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

    /// An asymmetric hexal (row 412, column 437) that the bar *does* sweep,
    /// unlike the row-band exclusion test, and that is not the row/column-
    /// symmetric origin. A transposed owner read misattributes this hexal's
    /// pixels to whichever hexal sits at (col 412, row 437), so it never
    /// reaches full coverage. `coverage_is_monotonic_in_the_offset` cannot
    /// catch that: the offset windows nest, so any per-pixel value at all
    /// produces non-decreasing counts.
    #[test]
    fn an_off_diagonal_hexal_is_swept_from_zero_to_full() {
        let p = params_for(vec![0]);
        let eye = HexEye::new(p.extent, p.ppo);
        let c = Coverage::new(&p, &eye);
        let h = crate::expand::hex_coords(15).position(|q| q == (0, 2)).unwrap();
        assert_eq!(eye.centre(h), Some((437.0, 412.0)));
        let total = c.kernel_sum()[h];
        assert_eq!(c.counts(0, 0)[h], 0, "offset -10: column span 423..451 falls short of stop 293");
        assert_eq!(c.counts(0, 20)[h], total, "offset +10: stop 485 clears the whole footprint");
    }

    /// An angle's coverage must not depend on which angles were rendered
    /// beside it. `band` is reused across angles, so a missing `clear()` would
    /// let one angle's pixels leak into the next -- caught today only by
    /// accident, because one other test happens to pass two angles.
    #[test]
    fn an_angles_coverage_does_not_depend_on_its_neighbours() {
        let alone = Coverage::new(&params_for(vec![0]), &HexEye::new(15, 25));
        let beside = Coverage::new(&params_for(vec![30, 0]), &HexEye::new(15, 25));
        for o in 0..21 {
            assert_eq!(alone.counts(0, o), beside.counts(1, o), "offset index {o}");
        }
    }
}
