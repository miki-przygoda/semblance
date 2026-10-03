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
//! - **flyvis builds this mask in `float16`.** We use `f64`. Measured offline
//!   against flyvis 1.2.0, 2,052 of 600,625 pixels end up assigned
//!   differently. `the_ownership_map_agrees_with_flyviss_fp16_mask` in
//!   `moving_edge_checks.rs` compares exactly those two masks and pins that
//!   count against a bound of 4,000 -- the gate that proves this crate's
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

/// Whether the point `(x, y)` lies inside the hexagon centred on `centre`.
///
/// Six half-plane sign tests, one per edge, exactly as flyvis does it: for each
/// edge, evaluate the edge's line at the query point and at the hexagon's own
/// centre, and require the two to fall on the same side.
///
/// `tilt` is zero throughout flyvis, so the vertices sit at 0, 60, ... 300
/// degrees -- a vertex on the horizontal axis, flats above and below. Rather
/// than build those vertices with `cos`/`sin`, this uses their exact closed
/// forms in terms of `dist_to_vertex` (the vertex on the horizontal axis) and
/// `dist_to_edge` (the flats above and below) -- `dist_to_vertex * sin(60deg)`
/// is mathematically `dist_to_edge`, so no trigonometry is needed to get
/// there.
///
/// This is not merely style: the half-plane test below compares signs of a
/// value that passes through exactly zero right at the boundary, so a single
/// ULP of difference in a vertex coordinate can flip which side of an edge a
/// boundary pixel falls on. `cos`/`sin` are not required by IEEE 754 to be
/// correctly rounded, so they are not guaranteed to produce the same bits on
/// every platform -- measured against the closed form used here, five of the
/// six vertices differ from what `cos`/`sin` compute, by 1 to 14 ULP
/// depending on the vertex and axis, and at 180 degrees `cos`/`sin` gives
/// `-4.64e-15` where the closed form gives an exact `0.0`. `f64::sqrt` (used
/// to derive `dist_to_vertex` from `dist_to_edge`) *is* required to be
/// correctly rounded, so this construction -- unlike one built from
/// `cos`/`sin` -- is **bit-exact**: the same bits on every platform, not
/// merely the same bits `cos`/`sin` would have produced.
///
/// A point exactly on an edge has sign 0, which matches neither `+1` nor `-1`,
/// so it is **excluded**. That is flyvis's behaviour (`torch.sign`), not an
/// oversight, and it is part of why no pixel gets two owners.
pub(crate) fn contains(
    centre: (f64, f64),
    dist_to_edge: f64,
    dist_to_vertex: f64,
    x: f64,
    y: f64,
) -> bool {
    let (xc, yc) = centre;
    let verts = [
        (dist_to_vertex, 0.0),
        (dist_to_vertex / 2.0, dist_to_edge),
        (-dist_to_vertex / 2.0, dist_to_edge),
        (-dist_to_vertex, 0.0),
        (-dist_to_vertex / 2.0, -dist_to_edge),
        (dist_to_vertex / 2.0, -dist_to_edge),
        (dist_to_vertex, 0.0),
    ];
    for i in 0..6 {
        let (x1, y1) = (verts[i].0 + xc, verts[i].1 + yc);
        let (x2, y2) = (verts[i + 1].0 + xc, verts[i + 1].1 + yc);
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
                if !contains(c, dist_to_edge, dist_to_vertex, col as f64, row as f64) {
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

    /// Re-derives ownership from the module's geometry primitives alone --
    /// `centre`, `dist_to_edge`, `dist_to_vertex` and `contains` -- without
    /// reading anything `build_owners` wrote. Returns how many ommatidia claim
    /// each pixel, and the owner each one would get.
    fn independent_scan(e: &HexEye) -> (Vec<u8>, Vec<u16>) {
        let mut claims = vec![0u8; 775 * 775];
        let mut owner = vec![NO_OWNER; 775 * 775];
        for h in 0..e.num_hexals() {
            let c = e.centre(h).unwrap();
            let dte = e.dist_to_edge();
            let dtv = e.dist_to_vertex();
            let lo_c = (c.0 - dtv).floor().max(0.0) as usize;
            let hi_c = ((c.0 + dtv).ceil() as usize).min(774);
            let lo_r = (c.1 - dtv).floor().max(0.0) as usize;
            let hi_r = ((c.1 + dtv).ceil() as usize).min(774);
            for row in lo_r..=hi_r {
                for col in lo_c..=hi_c {
                    if contains(c, dte, dtv, col as f64, row as f64) {
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
        let dte = 12.5;
        let dtv = 2.0 / 3.0f64.sqrt() * dte;
        assert!(contains(c, dte, dtv, 100.0, 100.0), "the centre");
        assert!(contains(c, dte, dtv, 114.0, 100.0), "just inside the right vertex");
        assert!(!contains(c, dte, dtv, 115.0, 100.0), "just outside the right vertex");
        assert!(contains(c, dte, dtv, 100.0, 112.0), "just inside the bottom flat");
        assert!(!contains(c, dte, dtv, 100.0, 113.0), "just outside the bottom flat");
        // A corner of the bounding box, outside the hexagon.
        assert!(!contains(c, dte, dtv, 114.0, 112.0));
    }
}
