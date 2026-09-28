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
