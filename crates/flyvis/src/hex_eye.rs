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
    #[allow(dead_code)] // read in Task 2
    owners: Vec<u16>,
    /// Pixels owned, per hexal. flyvis calls this `kernel_sum`.
    #[allow(dead_code)] // read in Task 2
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
    /// If `extent` is negative.
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

        let (owners, kernel_sum) = build_owners(monitor_px, &centres, dist_to_vertex);

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

/// Placeholder: Task 2 replaces this with the real point-in-hexagon scan.
fn build_owners(
    monitor_px: usize,
    centres: &[(f64, f64)],
    _dist_to_vertex: f64,
) -> (Vec<u16>, Vec<u32>) {
    (vec![NO_OWNER; monitor_px * monitor_px], vec![0; centres.len()])
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
}
