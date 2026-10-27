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
//! - **flyvis builds this mask in `float16`, and so do we.** Every operation
//!   of flyvis's `is_inside_hex` is emulated here with its result rounded to
//!   binary16. An `f64` mask assigns 2,052 of the 600,625 pixels differently,
//!   and that alone was enough to put the moving-edge renderer past its
//!   tolerance at bar height 80. `the_ownership_map_agrees_with_flyviss_fp16_mask`
//!   in `moving_edge_checks.rs` compares this mask with flyvis's own and
//!   requires every pixel to agree -- the gate that proves this crate's
//!   hexal ordering is flyvis's own.
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
    /// If `extent` is negative, if `ppo` is zero, or if a pixel falls inside
    /// two ommatidia.
    pub fn new(extent: i32, ppo: usize) -> Self {
        assert!(extent >= 0, "extent must not be negative, got {extent}");
        assert!(ppo > 0, "ppo must not be zero");
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

        let (owners, kernel_sum) = build_owners(monitor_px, &centres, dist_to_edge, dist_to_vertex);

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
}

/// flyvis's 60-degree rotation, `cos` and `sin` of its float16 third of pi.
///
/// flyvis takes `pi = torch.tensor(np.pi, dtype=float16)`, which is 3.140625,
/// and divides by 3 in float16, giving 1.046875. Its `cos` and `sin` are
/// computed in `f32` (0.5002793 and 0.8658641) and rounded back to binary16,
/// whose spacing in [0.5, 1) is 2^-11: 1025 * 2^-11 = 0.50048828125 and
/// 1773 * 2^-11 = 0.86572265625. Both `f32` values sit hundreds of `f32` ULPs
/// from a rounding midpoint, so no libm could disagree, but writing the
/// results down removes libm from the construction altogether.
/// `the_rotation_constants_are_flyviss_float16_cos_and_sin` re-derives them.
const COS_THIRD: f64 = 0.500_488_281_25;
const SIN_THIRD: f64 = 0.865_722_656_25;

/// Rounds an `f64` to the nearest IEEE binary16 value (ties to even), returned
/// as `f64`. Normal and subnormal range; no overflow handling, because every
/// value here stays far below binary16's largest, 65504.
///
/// Every step is exact in `f64` -- reading the exponent, a power of two,
/// division by it, `round_ties_even` -- so this is the same on every platform.
fn round_f16(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    // Exponent of the leading bit, floored at binary16's minimum normal (-14).
    let e = (((v.abs().to_bits() >> 52) & 0x7ff) as i32 - 1023).max(-14);
    // binary16 keeps 10 bits after the leading one.
    let ulp = 2f64.powi(e - 10);
    (v / ulp).round_ties_even() * ulp
}

/// The hexagon's seven vertex offsets (the last closes the loop), exactly as
/// flyvis's `is_inside_hex` builds them in float16: `[dist_to_vertex, 0]`,
/// then six successive `R60 @ v`, each product rounded to binary16 once.
///
/// `dist_to_vertex` is `2 / np.sqrt(3) * dist_to_edge`: an `f64` scalar times
/// a float16 tensor, which torch computes with the scalar as `f32`.
///
/// The rounding makes this not quite a regular hexagon, and the seventh vertex
/// need not land back on the first. That is flyvis's shape, so it is ours.
fn fp16_vertices(dist_to_edge: f64) -> [(f64, f64); 7] {
    let dist_to_vertex = round_f16((2.0 / 3.0f64.sqrt()) as f32 as f64 * round_f16(dist_to_edge));
    let mut v = [(0.0, 0.0); 7];
    v[0] = (dist_to_vertex, 0.0);
    for i in 1..7 {
        let (x, y) = v[i - 1];
        // torch accumulates a float16 matmul in f32 and rounds once; the
        // exact sum rounded once is the same binary16 value.
        v[i] = (
            round_f16(COS_THIRD * x - SIN_THIRD * y),
            round_f16(SIN_THIRD * x + COS_THIRD * y),
        );
    }
    v
}

/// Whether the point `(x, y)` lies inside the hexagon centred on `centre`,
/// with vertex offsets `verts` from [`fp16_vertices`].
///
/// Six half-plane sign tests, one per edge, exactly as flyvis's
/// `is_inside_hex` runs them with `dtype=torch.float16`: for each edge,
/// evaluate the edge's line at the query point and at the hexagon's own
/// centre, and require the two to fall on the same side. Every `+ - * /` is
/// rounded to binary16, in the order Python evaluates flyvis's expressions.
/// torch's CPU half arithmetic computes each operation in `f32` and rounds to
/// binary16; computing it in `f64` and rounding gives the same value, because
/// both formats carry more than twice binary16's 11 bits, so double rounding
/// cannot occur. That makes this mask bit-equal to flyvis's by construction.
///
/// A point exactly on an edge has sign 0, which matches neither `+1` nor `-1`,
/// so it is **excluded**. That is flyvis's behaviour (`torch.sign`), not an
/// oversight, and it is part of why no pixel gets two owners.
fn contains_fp16(verts: &[(f64, f64); 7], centre: (f64, f64), x: f64, y: f64) -> bool {
    let (xc, yc) = (round_f16(centre.0), round_f16(centre.1));
    let (x, y) = (round_f16(x), round_f16(y));
    for i in 0..6 {
        let (x1, y1) = (round_f16(verts[i].0 + xc), round_f16(verts[i].1 + yc));
        let (x2, y2) = (round_f16(verts[i + 1].0 + xc), round_f16(verts[i + 1].1 + yc));
        let slope = round_f16(round_f16(y2 - y1) / round_f16(x2 - x1));
        let f_centre = round_f16(round_f16(y1 + round_f16(slope * round_f16(xc - x1))) - yc);
        let f_point = round_f16(round_f16(y1 + round_f16(slope * round_f16(x - x1))) - y);
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
    dist_to_edge: f64,
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
    let verts = fp16_vertices(dist_to_edge);
    // The float16 vertices reach a few hundredths of a pixel past the exact
    // ones; one pixel of margin on the scan box covers that.
    let dist_to_vertex = dist_to_vertex + 1.0;

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
                if !contains_fp16(&verts, c, col as f64, row as f64) {
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

    /// A second binary16 rounding, written separately from [`round_f16`] so
    /// the oracle below shares none of the production arithmetic: this one
    /// rounds the `f64` significand as an integer, by shifting off the bits
    /// binary16 has no room for and rounding ties to even by hand.
    fn half(v: f64) -> f64 {
        if v == 0.0 {
            return v;
        }
        let bits = v.to_bits();
        let negative = bits >> 63 == 1;
        let exp = ((bits >> 52) & 0x7ff) as i32 - 1023;
        assert!(exp > -1023 && exp < 16, "{v} is outside what this oracle handles");
        let zero = if negative { -0.0 } else { 0.0 };
        let significand = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
        // v = significand * 2^(exp - 52). binary16's last kept bit is worth
        // 2^(exp - 10) for normals and 2^-24 for subnormals.
        let keep_lsb = (exp - 10).max(-24);
        let drop = (keep_lsb - (exp - 52)) as u32;
        if drop > 53 {
            // Below half of binary16's smallest subnormal: rounds to zero.
            return zero;
        }
        let (kept, rest) = (significand >> drop, significand & ((1u64 << drop) - 1));
        let halfway = 1u64 << (drop - 1);
        let rounded = if rest > halfway || (rest == halfway && kept & 1 == 1) { kept + 1 } else { kept };
        let magnitude = rounded as f64 * 2f64.powi(keep_lsb);
        if negative { -magnitude } else { magnitude }
    }

    /// Re-derives ownership without reading anything `build_owners` wrote and
    /// without calling the production float16 path: its own rounding
    /// ([`half`]), its own `cos`/`sin` of flyvis's float16 third of pi, its
    /// own vertex construction and its own half-plane test, written from
    /// flyvis's `is_inside_hex`. Returns how many ommatidia claim each pixel,
    /// and the owner each one would get.
    fn independent_scan(e: &HexEye) -> (Vec<u8>, Vec<u16>) {
        let third = half(half(std::f64::consts::PI) / 3.0);
        let (cos, sin) = (half((third as f32).cos() as f64), half((third as f32).sin() as f64));
        let mut verts = vec![(half((2.0 / 3.0f64.sqrt()) as f32 as f64 * half(e.dist_to_edge())), 0.0)];
        for _ in 0..6 {
            let &(x, y) = verts.last().unwrap();
            verts.push((half(cos * x - sin * y), half(sin * x + cos * y)));
        }
        let side = |v: f64| v.partial_cmp(&0.0).unwrap();
        let inside = |c: (f64, f64), px: f64, py: f64| {
            let (xc, yc, px, py) = (half(c.0), half(c.1), half(px), half(py));
            verts.windows(2).all(|w| {
                let (x1, y1) = (half(w[0].0 + xc), half(w[0].1 + yc));
                let (x2, y2) = (half(w[1].0 + xc), half(w[1].1 + yc));
                let slope = half(half(y2 - y1) / half(x2 - x1));
                let at = |qx: f64, qy: f64| half(half(y1 + half(slope * half(qx - x1))) - qy);
                side(at(xc, yc)) == side(at(px, py))
            })
        };
        let mut claims = vec![0u8; 775 * 775];
        let mut owner = vec![NO_OWNER; 775 * 775];
        for h in 0..e.num_hexals() {
            let c = e.centre(h).unwrap();
            // Generously wider than any vertex reaches.
            let reach = e.dist_to_edge() * 1.5;
            let lo_c = (c.0 - reach).floor().max(0.0) as usize;
            let hi_c = ((c.0 + reach).ceil() as usize).min(774);
            let lo_r = (c.1 - reach).floor().max(0.0) as usize;
            let hi_r = ((c.1 + reach).ceil() as usize).min(774);
            for row in lo_r..=hi_r {
                for col in lo_c..=hi_c {
                    if inside(c, col as f64, row as f64) {
                        let idx = row * 775 + col;
                        claims[idx] += 1;
                        owner[idx] = h as u16;
                    }
                }
            }
        }
        (claims, owner)
    }

    /// The geometry: no two footprints overlap. This is what makes one `u16`
    /// per pixel able to replace flyvis's dense (600625, 721) boolean.
    #[test]
    fn no_pixel_belongs_to_two_ommatidia() {
        let (claims, _) = independent_scan(&eye());
        assert_eq!(claims.iter().filter(|&&n| n > 1).count(), 0, "pixels claimed twice");
    }

    /// The scan: `build_owners` wrote what the geometry implies. A mutation
    /// test showed the invariant check above passes even when `build_owners`
    /// is corrupted, because it never reads what `build_owners` produced.
    #[test]
    fn build_owners_agrees_with_an_independent_scan() {
        let e = eye();
        let (_, want) = independent_scan(&e);
        if let Some(i) = want.iter().zip(e.owners()).position(|(a, b)| a != b) {
            panic!(
                "pixel (col {}, row {}): independent scan says {:?}, build_owners says {:?}",
                i % 775, i / 775, want[i], e.owners()[i]
            );
        }
        // kernel_sum must be the same map, counted.
        let mut counts = vec![0u32; e.num_hexals()];
        for &o in &want {
            if o != NO_OWNER {
                counts[o as usize] += 1;
            }
        }
        assert_eq!(counts, e.kernel_sum());
    }

    /// flyvis 1.2.0's own figures, counted from its mask as exported to
    /// `data/flyvis-stimuli/hex_eye_owner.bin` (and its per-ommatidium
    /// `hex_eye_kernel_sum.json`, which agrees): 382,383 pixels owned,
    /// 218,242 owned by none, footprints from 513 to 545 pixels. They are
    /// written down here so the check runs without the fixture.
    ///
    /// If these move, something in the geometry moved -- report it, do not
    /// re-pin the constants to match the code.
    #[test]
    fn ownership_counts_match_what_was_measured() {
        let e = eye();
        let owned = e.owners().iter().filter(|&&o| o != NO_OWNER).count();
        assert_eq!(owned, 382_383, "pixels owned by some ommatidium");
        assert_eq!(775 * 775 - owned, 218_242, "pixels owned by none (36%)");
        let ks = e.kernel_sum();
        assert_eq!(ks.len(), 721);
        assert_eq!(ks.iter().sum::<u32>() as usize, owned, "kernel sums must total the owned pixels");
        assert_eq!(*ks.iter().min().unwrap(), 513, "smallest footprint");
        assert_eq!(*ks.iter().max().unwrap(), 545, "largest footprint");
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
    /// +/-dist_to_vertex (14.4375 in float16) and flats above and below at
    /// +/-dist_to_edge (12.5).
    #[test]
    fn contains_fp16_agrees_with_hand_computed_cases() {
        let c = (100.0, 100.0);
        let v = fp16_vertices(12.5);
        assert_eq!(v[0], (14.4375, 0.0), "f16(2 / sqrt(3) * 12.5)");
        assert!(contains_fp16(&v, c, 100.0, 100.0), "the centre");
        assert!(contains_fp16(&v, c, 114.0, 100.0), "just inside the right vertex");
        assert!(!contains_fp16(&v, c, 115.0, 100.0), "just outside the right vertex");
        assert!(contains_fp16(&v, c, 100.0, 112.0), "just inside the bottom flat");
        assert!(!contains_fp16(&v, c, 100.0, 113.0), "just outside the bottom flat");
        // A corner of the bounding box, outside the hexagon.
        assert!(!contains_fp16(&v, c, 114.0, 112.0));
        // The right vertex itself lies on two edges; on an edge is outside,
        // as `torch.sign` makes it.
        assert!(!contains_fp16(&v, c, 100.0 + v[0].0, 100.0), "on the right vertex");
    }

    /// The hard-coded rotation is flyvis's: f16(pi) = 3.140625, divided by 3
    /// in float16 = 1.046875, `cos`/`sin` in `f32`, rounded to binary16. The
    /// first assertions check the rounding helper itself against binary16's
    /// grid: 11 significant bits, ties to even, subnormals spaced 2^-24.
    #[test]
    fn the_rotation_constants_are_flyviss_float16_cos_and_sin() {
        assert_eq!(round_f16(1.0 + 2f64.powi(-11)), 1.0, "a tie rounds to even, down");
        assert_eq!(round_f16(1.0 + 3.0 * 2f64.powi(-11)), 1.0 + 2f64.powi(-9), "a tie rounds to even, up");
        assert_eq!(round_f16(2049.0), 2048.0, "above 2048 the spacing is 2");
        assert_eq!(round_f16(-774.3), -774.5, "spacing 0.5 in [512, 1024)");
        assert_eq!(round_f16(2f64.powi(-24) * 1.4), 2f64.powi(-24), "subnormal");

        let pi = round_f16(std::f64::consts::PI);
        assert_eq!(pi, 3.140625);
        let third = round_f16(pi / 3.0);
        assert_eq!(third, 1.046875);
        assert_eq!(round_f16((third as f32).cos() as f64), COS_THIRD);
        assert_eq!(round_f16((third as f32).sin() as f64), SIN_THIRD);
    }
}
