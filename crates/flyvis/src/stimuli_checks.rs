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
