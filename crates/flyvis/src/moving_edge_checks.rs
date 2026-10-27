//! Acceptance criteria for the moving edge generator.
//!
//! The geometry gate below, `moving_edge_offsets_match_flyvis_within_tolerance`,
//! is bounded rather than exact: flyvis rotates a `uint8` raster through
//! Pillow's bilinear resampler and this crate rotates the sampling geometry
//! instead, by an explicit decision recorded in the spec. The two renderers
//! are not computing the same function, so this bound would still exist on a
//! machine with perfect arithmetic -- it is not a precision artifact, and it
//! is not precedent for loosening a comparison that could be exact elsewhere.
//! See `CLAUDE.md` for the fuller version of that distinction.
//!
//! `the_ownership_map_agrees_with_flyviss_fp16_mask`, further down, carries
//! no slack at all: this crate builds that mask in `float16` exactly as flyvis
//! does, so every pixel must agree.
//!
//! So the geometry gate prints its observed distribution rather than merely
//! asserting a bound. The difference between 0.002 and 0.009 agreement is the
//! difference between "that is the quantisation artifact" and "something else
//! is wrong", and a pass/fail assertion hides it.
//!
//! Everything that *can* be compared element-wise still is: the sequence
//! ordering, the resample indices, the pad lengths and the NaN boundary.
//!
//! These skip when their fixtures are absent, and the skip notice is invisible
//! under a plain `cargo test`. Set `SEMBLANCE_REQUIRE_FIXTURES=1` to turn a
//! missing fixture into a failure.
//!
//! The geometry gate renders `GATE_ANGLES`' worth of offsets -- four, not
//! twelve -- and the whole crate's debug suite runs in about a second;
//! nothing here needs `--release` to be comfortable.

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
/// the maximum to 0.031848 and puts 1,316 hexals over 0.01, concentrated in
/// angles 90 and 150 -- measured by this gate's own full 21-offset sweep
/// across its four angles. A narrower 3-offset probe (angles 0/30/90/150/210,
/// offsets -10/3/10) sees only 39 such hexals; both figures are correct for
/// what they measured, but this gate's assertion message will cite the first
/// angle in `GATE_ANGLES` order to exceed `TOLERANCE`, not the worst one.
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

/// Pulls the integers out of a `"key": [ ... ]` array, which may span lines.
/// `json_field` cannot do this: it stops at the first newline.
fn json_array(s: &str, key: &str) -> Vec<i64> {
    let needle = format!("\"{key}\":");
    let start = s.find(&needle).unwrap_or_else(|| panic!("no field {key}")) + needle.len();
    let open = start + s[start..].find('[').expect("array open");
    let close = open + s[open..].find(']').expect("array close");
    json_ints(&s[open + 1..close])
}

/// Every integer that follows a `"key":` in the document, in order.
/// `json_field` finds only the first.
fn json_field_all(s: &str, key: &str) -> Vec<i64> {
    let needle = format!("\"{key}\":");
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(&needle) {
        rest = &rest[i + needle.len()..];
        let end = rest.find([',', '}', '\n']).unwrap_or(rest.len());
        out.push(rest[..end].trim().parse().expect("integer after key"));
    }
    out
}

/// Every floating-point value that follows a `"key":` in the document, in
/// order. `json_field_all`'s `f64` sibling, for fields like `intensity` that
/// are not integers.
fn json_field_all_f64(s: &str, key: &str) -> Vec<f64> {
    let needle = format!("\"{key}\":");
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(&needle) {
        rest = &rest[i + needle.len()..];
        let end = rest.find([',', '}', '\n']).unwrap_or(rest.len());
        out.push(rest[..end].trim().parse().expect("number after key"));
    }
    out
}

/// The numeric value immediately at the start of a speed block -- the text
/// right after the `"speed":` the block was split on.
fn block_speed(block: &str) -> f64 {
    let end = block.find([',', '}', '\n']).unwrap_or(block.len());
    block[..end].trim().parse().expect("speed value")
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

    // The fixture's own account of what it holds, against what this gate
    // assumes. Without this, reordering GATE_ANGLES without regenerating the
    // fixtures would surface only as a tolerance failure blaming the rotation
    // centre.
    assert_eq!(
        json_array(&manifest, "shape"),
        vec![(GATE_ANGLES.len() * 2) as i64, 21, 721],
        "fixture shape"
    );
    assert_eq!(
        json_field_all(&manifest, "index"),
        (0..(GATE_ANGLES.len() * 2) as i64).collect::<Vec<_>>(),
        "fixture group indices"
    );
    assert_eq!(
        json_field_all(&manifest, "angle"),
        GATE_ANGLES.iter().flat_map(|&a| [a as i64, a as i64]).collect::<Vec<_>>(),
        "fixture group order must be angle-major, intensity-minor"
    );
    assert_eq!(
        json_field_all_f64(&manifest, "intensity"),
        GATE_ANGLES.iter().flat_map(|_| [0.0, 1.0]).collect::<Vec<_>>(),
        "fixture group order must be angle-major, intensity-minor"
    );

    let mut params = MovingEdgeParams::flyvis_defaults(DT);
    params.angles = GATE_ANGLES.to_vec();
    let e = MovingEdge::new(params);

    // Per regime, so the three numerical footings stay distinguishable.
    let mut report: Vec<(i32, f64, f64, usize, usize)> = Vec::new();
    let mut out = vec![0.0f32; 721];

    for (ai, &angle) in GATE_ANGLES.iter().enumerate() {
        let (mut max, mut sum, mut over_tol, mut over_hundredth) = (0.0f64, 0.0f64, 0, 0);
        for ii in 0..2 {
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

/// The ownership map, against flyvis's own. Element-wise and exact.
///
/// flyvis builds its mask in `float16`, and `hex_eye` emulates that
/// construction operation by operation, so the two maps are equal by
/// construction: every one of the 600,625 pixels, and every kernel sum.
/// An `f64` mask disagreed on 2,052 pixels -- enough to push the moving-edge
/// renderer past its tolerance at bar height 80 -- so a single differing
/// pixel here is a real break, not noise.
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

    eprintln!("pixel ownership, float16 here against flyvis's float16:");
    eprintln!("  differing assignments: {differing} of {}", want.len());
    eprintln!("  unowned pixels: {ours_unowned} here, {theirs_unowned} in flyvis");
    eprintln!("  largest per-ommatidium pixel-count difference: {ks_delta}");

    assert_eq!(
        differing, 0,
        "{differing} pixels assigned differently from flyvis; the float16 emulation in hex_eye diverged"
    );
    assert_eq!(ks_delta, 0, "largest kernel-sum difference {ks_delta}");
}

/// The sequence ordering and the published figures.
///
/// Reads only sequence counts and timing, never geometry, so -- like
/// [`the_timing_matches_flyvis_frame_for_frame`] and unlike the geometry gate
/// above -- it restricts `angles` to a single value rather than paying for
/// all twelve of `flyvis_defaults`'s. `num_sequences` scales with the angle
/// count, so the comparison scales back up by the angle count this test left
/// out, rather than comparing a one-angle family against flyvis's twelve.
#[test]
fn the_full_family_has_flyviss_shape() {
    let Some(timing) = load_stimulus_fixture("moving_edge_timing.json") else { return };
    let mut params = MovingEdgeParams::flyvis_defaults(DT);
    let n_angles = params.angles.len();
    params.angles = vec![params.angles[0]];
    let e = MovingEdge::new(params);
    let n_frames: usize = json_field(&timing, "n_frames").parse().unwrap();
    let n_sequences: usize = json_field(&timing, "n_sequences_full_family").parse().unwrap();

    assert_eq!(e.num_sequences() * n_angles, n_sequences, "144 = 12 x 2 x 6");
    assert_eq!(e.num_frames(), n_frames, "1078 frames");
    assert_eq!(e.num_hexals(), 721);
}

/// The resample indices and the pad boundaries, element-wise.
///
/// None of what this reads -- speed, frame counts, resample indices -- varies
/// with the angle count, so it restricts `angles` the same way
/// [`the_nan_boundary_matches_flyvis`] does rather than paying for all twelve
/// of `flyvis_defaults`'s purely to read them.
#[test]
fn the_timing_matches_flyvis_frame_for_frame() {
    let Some(json) = load_stimulus_fixture("moving_edge_timing.json") else { return };
    let mut params = MovingEdgeParams::flyvis_defaults(DT);
    params.angles = vec![0];
    let e = MovingEdge::new(params);

    // One object per speed, in the same order as `speeds`.
    let blocks: Vec<&str> = json.split("\"speed\":").skip(1).collect();
    assert_eq!(blocks.len(), 6, "six speeds");

    for (s, block) in blocks.iter().enumerate() {
        // Sequence `s` is angle 0, intensity 0, speed index `s` -- but only if
        // this block really is that speed. Check it rather than assume it.
        assert_eq!(
            e.speed_of(s),
            Some(block_speed(block)),
            "speed index {s}: block position does not match MovingEdge's own order"
        );

        let n_stim: usize = json_field(block, "n_stim").parse().unwrap();
        let n_valid: usize = json_field(block, "n_valid").parse().unwrap();
        let indices = json_array(block, "indices");
        assert_eq!(indices.len(), n_stim, "speed index {s}: index count");

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
        assert_eq!(
            e.speed_of(s),
            Some(block_speed(block)),
            "speed index {s}: block position does not match MovingEdge's own order"
        );

        let n_valid: usize = json_field(block, "n_valid").parse().unwrap();
        e.frame_into(s, n_valid - 1, &mut out).unwrap();
        assert!(out.iter().all(|x| x.is_finite()), "speed index {s}: frame {} must be real", n_valid - 1);
        if n_valid < e.num_frames() {
            e.frame_into(s, n_valid, &mut out).unwrap();
            assert!(out.iter().all(|x| x.is_nan()), "speed index {s}: frame {n_valid} must be NaN");
        }
    }
}
