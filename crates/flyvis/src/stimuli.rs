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
}
