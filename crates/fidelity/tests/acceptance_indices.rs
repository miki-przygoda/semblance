//! Acceptance tests for behaviour group `indices` of epic fidelity-measures:
//! flyvis's response indices computed from a battery's responses. One test
//! per behaviour id, INDICES-01 .. INDICES-15; INDICES-16 .. INDICES-19 are
//! `semblance validate`'s and live in `app/tests/acceptance_indices.rs`.
//!
//! Every test drives `semblance_fidelity::indices`' public API the way a
//! caller would: it builds a `Responses` and a `BatteryLayout` (or peaks)
//! by hand, with known answers, and reads what the functions return.

use std::f64::consts::PI;

use semblance_fidelity::indices::{
    angular_distance_to_known, dsi, dsi_correlation_to_known, edge_peaks, edge_window, fri, fri_correlation_to_known,
    fri_peaks, fri_window, preferred_direction, tuning_r, widen_curves, BatteryLayout, EdgePeaks, FriPeaks, IndexError,
    Stimulus, Window, EDGE_OFFSETS, FRI_RADIUS, N_ANGLES, N_SPEEDS, T_PRE,
};
use semblance_sim::response::{Precision, Responses};

/// flyvis's time step, 1/200 s.
const DT: f64 = 1.0 / 200.0;
/// The moving-edge battery's speeds, in columns per second.
const SPEEDS: [f64; N_SPEEDS] = [2.4, 4.8, 9.7, 13.0, 19.0, 25.0];

// ---------------------------------------------------------------------------
// Building inputs
// ---------------------------------------------------------------------------

fn names(types: &[&str]) -> Vec<String> {
    types.iter().map(|t| t.to_string()).collect()
}

/// `n` cell type names, `c0` .. `c{n-1}`.
fn numbered(n: usize) -> Vec<String> {
    (0..n).map(|t| format!("c{t}")).collect()
}

/// Responses over `types`: one stimulus per `(label, n_frames)`, each value
/// `fill(stimulus, frame, type)`.
fn responses(
    types: &[String],
    precision: Precision,
    stimuli: &[(String, usize)],
    fill: impl Fn(usize, usize, usize) -> f64,
) -> Responses {
    let mut r = Responses::new(types.to_vec(), DT, precision).expect("responses");
    for (s, (label, n)) in stimuli.iter().enumerate() {
        let mut values = Vec::with_capacity(n * types.len());
        for f in 0..*n {
            for t in 0..types.len() {
                values.push(fill(s, f, t));
            }
        }
        r.push(label.clone(), *n, values).expect("push");
    }
    r.finish().expect("finish")
}

fn window(first: usize, last: usize) -> Window {
    Window::new(first, last).expect("window")
}

/// An f32-valued pseudo-random value in about [-0.6, 0.66], from `k`.
fn wiggle(k: usize) -> f64 {
    let k = k.wrapping_mul(2_654_435_761) % 1009;
    ((k as f32) * 0.00125f32 - 0.6f32) as f64
}

/// The full moving-edge battery's edges, in flyvis's order (intensity,
/// speed, angle), each `(label, stimulus)`.
fn edges() -> Vec<(String, Stimulus)> {
    let mut out = Vec::new();
    for intensity in 0..2u8 {
        for &speed in &SPEEDS {
            for a in 0..N_ANGLES {
                let angle_deg = (a * 30) as u16;
                out.push((format!("edge i{intensity} v{speed} a{angle_deg}"), Stimulus::Edge { angle_deg, speed, intensity }));
            }
        }
    }
    out
}

/// A layout of the 144 edges, each with window `w`.
fn edge_layout(w: Window) -> BatteryLayout {
    BatteryLayout::new(edges().into_iter().map(|(l, s)| (l, s, w)).collect()).expect("edge layout")
}

/// Edge peaks whose value for edge (intensity, speed index, angle index)
/// and type `t` is `peak(i, s, a, t)`, in `edges()` order.
fn peaks_from(types: &[String], precision: Precision, peak: impl Fn(usize, usize, usize, usize) -> f64) -> EdgePeaks {
    let mut rows = Vec::new();
    for i in 0..2 {
        for s in 0..N_SPEEDS {
            for a in 0..N_ANGLES {
                rows.push((0..types.len()).map(|t| peak(i, s, a, t)).collect());
            }
        }
    }
    EdgePeaks::new(types.to_vec(), precision, rows).expect("edge peaks")
}

/// The row of `edges()` holding (intensity, speed index, angle index).
fn row_of(i: usize, s: usize, a: usize) -> usize {
    (i * N_SPEEDS + s) * N_ANGLES + a
}

/// Population Pearson r, written independently of the crate.
fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma) * (x - ma)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb) * (y - mb)).sum();
    cov / (va.sqrt() * vb.sqrt())
}

/// The smaller angle between two directions in radians.
fn angle_between(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(2.0 * PI);
    d.min(2.0 * PI - d)
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

// ---------------------------------------------------------------------------
// INDICES-01 .. 03: purity and windows
// ---------------------------------------------------------------------------

/// INDICES-01: the index functions are pure: the crate does not depend on
/// `semblance-flyvis`, the module touches no files, processes or
/// environment, and ground truth arrives only as arguments: the same inputs
/// give the same outputs, from in-memory data alone.
#[test]
fn indices_01_index_functions_are_pure_and_take_ground_truth_as_data() {
    let manifest = include_str!("../Cargo.toml");
    assert!(!manifest.contains("semblance-flyvis"), "semblance-fidelity must not depend on semblance-flyvis");
    let src = include_str!("../src/indices.rs");
    for forbidden in ["std::fs", "std::process", "std::env", "std::net", "File::", "semblance_flyvis", "include_bytes!", "include_str!"]
    {
        assert!(!src.contains(forbidden), "indices.rs mentions {forbidden}");
    }

    // Built entirely in memory; computed twice; identical.
    let types = numbered(3);
    let w = fri_window(DT).unwrap();
    let stimuli: Vec<(String, usize)> = vec![("off".into(), 420), ("on".into(), 420)];
    let layout = BatteryLayout::new(vec![
        ("off".into(), Stimulus::Flash { radius: FRI_RADIUS, intensity: 0 }, w),
        ("on".into(), Stimulus::Flash { radius: FRI_RADIUS, intensity: 1 }, w),
    ])
    .unwrap();
    let r = responses(&types, Precision::F64, &stimuli, |s, f, t| wiggle(s * 10_000 + f * 7 + t));
    let once = fri(&fri_peaks(&r, &layout).unwrap()).unwrap();
    let twice = fri(&fri_peaks(&r, &layout).unwrap()).unwrap();
    assert_eq!(once, twice);

    let known = [("c0", 1i8), ("c1", -1), ("c2", 1)];
    let a = fri_correlation_to_known(&once, &types, &known).unwrap();
    let b = fri_correlation_to_known(&once, &types, &known).unwrap();
    assert_eq!(a.to_bits(), b.to_bits());
}

/// INDICES-02: `fri_window(dt)` evaluates flyvis's query on flyvis's time
/// axis; at dt 1/200 that is frames 200..=400, frame 199 dropped by float
/// rounding (exactly, 199 * dt - 1 = -dt, which the query keeps).
#[test]
fn indices_02_the_fri_window_is_flyviss_float_expression() {
    assert_eq!(fri_window(DT), Ok(window(200, 400)));

    // Frame 199 is on the boundary in exact arithmetic, and float rounding
    // is what drops it: its time is computed below -dt.
    let t199 = 199.0 * DT - T_PRE;
    assert!(t199 < -DT, "199 * dt - t_pre = {t199:e} rounds below -dt");

    // The same expression decides every dt: the window is whatever
    // frames satisfy `-dt <= f * dt - t_pre <= t_stim`, nothing tabled.
    for dt in [1.0 / 200.0, 1.0 / 50.0, 1.0 / 100.0, 0.001, 0.003, 0.007, 1.0 / 30.0] {
        let keep = |f: usize| {
            let t = f as f64 * dt - T_PRE;
            -dt <= t && t <= 1.0
        };
        let frames: Vec<usize> = (0..10_000).filter(|&f| keep(f)).collect();
        let want = window(*frames.first().unwrap(), *frames.last().unwrap());
        assert_eq!(fri_window(dt), Ok(want), "dt {dt}");
    }
}

/// INDICES-03: at flyvis's dt the edge windows end at frames 878, 539, 367,
/// 325, 285 and 265 for the six speeds. The fixture-gated half, against task
/// 2's index manifest, is `indices_03_edge_windows_equal_the_index_manifests`.
#[test]
fn indices_03_edge_windows_end_where_flyviss_masks_do() {
    let table = [(2.4, 878), (4.8, 539), (9.7, 367), (13.0, 325), (19.0, 285), (25.0, 265)];
    for (speed, last) in table {
        let w = edge_window(speed, DT, T_PRE, EDGE_OFFSETS).unwrap_or_else(|e| panic!("speed {speed}: {e}"));
        assert_eq!(w.last(), last, "speed {speed}");
        assert!(w.first() <= w.last());
    }
}

/// The workspace's `data/` directory.
fn real_data() -> std::path::PathBuf {
    std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data")).to_path_buf()
}

/// `data/flyvis-indices/manifest.json` (fixtures-3), or `None` after a skip
/// notice; under `SEMBLANCE_REQUIRE_FIXTURES=1` a missing file fails.
fn index_manifest_or_skip(test: &str) -> Option<String> {
    let path = real_data().join("flyvis-indices").join("manifest.json");
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) => {
            if std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1") {
                panic!("{test}: SEMBLANCE_REQUIRE_FIXTURES=1 but {} is unreadable: {e}", path.display());
            }
            eprintln!("skipping: {test}: {} missing: {e}", path.display());
            None
        }
    }
}

/// The text after `"key"` and its colon, whitespace trimmed.
fn after_key<'a>(obj: &'a str, key: &str) -> &'a str {
    let quoted = format!("\"{key}\"");
    let at = obj.find(&quoted).unwrap_or_else(|| panic!("no {quoted} in {obj}"));
    let rest = obj[at + quoted.len()..].trim_start();
    rest.strip_prefix(':').unwrap_or_else(|| panic!("no ':' after {quoted} in {obj}")).trim_start()
}

/// Each edge stimulus in the manifest's `batteries.edges`, as
/// `(label, speed, (first, last))`. The manifest is written by
/// `tools/export_indices.py`; an edge entry holds no nested object.
fn manifest_edge_windows(manifest: &str) -> Vec<(String, f64, (usize, usize))> {
    let mut rest = after_key(after_key(manifest, "batteries"), "edges").strip_prefix('[').expect("edges is an array");
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix(',') {
            rest = r.trim_start();
        }
        if rest.starts_with(']') {
            break;
        }
        let body = rest.strip_prefix('{').unwrap_or_else(|| panic!("edge entry expected at {:.40}", rest));
        let end = body.find('}').expect("edge entry closes");
        let obj = &body[..end];
        rest = &body[end + 1..];
        let label = after_key(obj, "label");
        let label = label[1..label[1..].find('"').expect("label string") + 1].to_string();
        let speed = after_key(obj, "speed");
        let speed: f64 = speed[..speed.find([',', '}', ' ']).unwrap_or(speed.len())].parse().expect("speed number");
        let w = after_key(obj, "window");
        let w = &w[1..w.find(']').expect("window array closes")];
        let bounds: Vec<usize> = w.split(',').map(|v| v.trim().parse().expect("window frame")).collect();
        assert_eq!(bounds.len(), 2, "{label}: window {w}");
        out.push((label, speed, (bounds[0], bounds[1])));
    }
    out
}

/// INDICES-03, fixture-gated half: every edge stimulus's window in task 2's
/// index manifest (`data/flyvis-indices/manifest.json`, read from flyvis's
/// own masks) equals `edge_window` at its speed, first frame and last.
/// Deferred to fidelity-gates with gate A (requirement R5): it reads
/// fixtures-3, which no build stages yet.
#[test]
#[ignore = "acceptance-failing: INDICES-03"]
fn indices_03_edge_windows_equal_the_index_manifests() {
    let Some(manifest) = index_manifest_or_skip("INDICES-03 against the index manifest") else {
        return;
    };
    let edges = manifest_edge_windows(&manifest);
    assert_eq!(edges.len(), 2 * N_ANGLES * N_SPEEDS, "the edge battery's stimuli");
    for speed in SPEEDS {
        let n = edges.iter().filter(|(_, s, _)| *s == speed).count();
        assert_eq!(n, 2 * N_ANGLES, "edges at speed {speed}");
    }
    for (label, speed, (first, last)) in edges {
        let w = edge_window(speed, DT, T_PRE, EDGE_OFFSETS).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!((w.first(), w.last()), (first, last), "{label}: speed {speed}");
    }
}

/// The scanner the fixture-gated INDICES-03 test relies on reads the
/// manifest layout task 2's export plan fixes, so that test cannot pass by
/// finding no edges.
#[test]
fn the_index_manifest_scanner_reads_every_edge_window() {
    let sample = r#"{"format": 1, "batteries": {"flashes": [{"index": 0, "label": "flash r6 i1", "baseline": 0.5,
        "intensity": 1, "radius": 6, "n_frames": 2, "window": [0, 1]}],
        "edges": [{"index": 0, "label": "edge a0 i0 s2.4", "angle": 0, "intensity": 0, "speed": 2.4, "width": 80,
        "n_frames": 3, "window": [0, 2]},{"index":1,"label":"edge a30 i1 s13","angle":30,"intensity":1,"speed":13,
        "width":80,"n_frames":600,"window":[200,325]}]}, "files": {"edges": []}}"#;
    assert_eq!(
        manifest_edge_windows(sample),
        vec![("edge a0 i0 s2.4".to_string(), 2.4, (0, 2)), ("edge a30 i1 s13".to_string(), 13.0, (200, 325))]
    );
}

// ---------------------------------------------------------------------------
// INDICES-04 .. 06: the FRI
// ---------------------------------------------------------------------------

const FLASH_FRAMES: usize = 420;

/// The flash battery: radius -1 and radius 6, OFF and ON, each over the
/// FRI window, radius -1 first.
fn flash_battery() -> (Vec<(String, usize)>, BatteryLayout) {
    let w = fri_window(DT).unwrap();
    let entries = [(-1, 0u8), (-1, 1), (FRI_RADIUS, 0), (FRI_RADIUS, 1)];
    let stimuli = entries.iter().map(|&(r, i)| (format!("flash r{r} i{i}"), FLASH_FRAMES)).collect();
    let layout = BatteryLayout::new(
        entries
            .iter()
            .map(|&(radius, intensity)| (format!("flash r{radius} i{intensity}"), Stimulus::Flash { radius, intensity }, w))
            .collect(),
    )
    .unwrap();
    (stimuli, layout)
}

/// INDICES-04: `fri_peaks` reads only the radius-6 flashes inside the FRI
/// window; per type it shifts by the absolute minimum over the window and
/// both intensities, and returns the raw shifted peaks [65][off, on].
#[test]
fn indices_04_fri_peaks_shift_by_the_shared_minimum_over_radius_6_flashes() {
    let types = numbered(65);
    let (stimuli, layout) = flash_battery();
    let w = fri_window(DT).unwrap();
    let inside = |f: usize| f >= w.first() && f <= w.last();
    // Outside the window, or on radius -1, values that would dominate any
    // min or max if they were read.
    let fill = |s: usize, f: usize, t: usize| -> f64 {
        if s < 2 {
            return if f % 2 == 0 { 50.0 } else { -50.0 };
        }
        if !inside(f) {
            return if f % 2 == 0 { 40.0 } else { -40.0 };
        }
        if t == 0 {
            // hand-checked type: OFF min -0.25, OFF max 0.5, ON max 1.0
            return match (s, f) {
                (2, 250) => -0.25,
                (2, 300) => 0.5,
                (3, 260) => 1.0,
                _ => 0.0,
            };
        }
        wiggle(s * 100_000 + f * 97 + t)
    };
    let r = responses(&types, Precision::F64, &stimuli, fill);
    let p = fri_peaks(&r, &layout).unwrap();

    assert_eq!(p.cell_types(), &types[..]);
    assert_eq!(p.peaks().len(), 65);
    assert_eq!(p.peaks()[0], [0.75, 1.25], "min -0.25 shifted onto 0.5 and 1.0");

    for t in 0..65 {
        let off: Vec<f64> = (w.first()..=w.last()).map(|f| fill(2, f, t)).collect();
        let on: Vec<f64> = (w.first()..=w.last()).map(|f| fill(3, f, t)).collect();
        let shift = off.iter().chain(&on).copied().fold(f64::INFINITY, f64::min).abs();
        let peak = |v: &[f64]| v.iter().map(|x| x + shift).fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(p.peaks()[t], [peak(&off), peak(&on)], "type {t}");
    }
}

/// INDICES-05: `fri` returns one value per type, `(on - off) / (on + off +
/// 1e-16)`: for an f32 run an f32 numerator, the f64 epsilon, stored f32.
#[test]
fn indices_05_fri_is_on_minus_off_over_on_plus_off_plus_epsilon() {
    let types = numbered(65);
    let pairs: Vec<[f64; 2]> = (0..65).map(|t| [wiggle(3 * t + 1).abs(), wiggle(5 * t + 2).abs()]).collect();

    let f32_run = fri(&FriPeaks::new(types.clone(), Precision::F32, pairs.clone()).unwrap()).unwrap();
    assert_eq!(f32_run.len(), 65);
    for (t, (&v, &[off, on])) in f32_run.iter().zip(&pairs).enumerate() {
        let (on, off) = (on as f32, off as f32);
        let want = (((on - off) as f64) / ((on + off) as f64 + 1e-16)) as f32 as f64;
        assert_eq!(v.to_bits(), want.to_bits(), "type {t}");
        assert_eq!(v as f32 as f64, v, "type {t}: an f32 run's FRI is f32-valued");
    }

    let f64_run = fri(&FriPeaks::new(types, Precision::F64, pairs.clone()).unwrap()).unwrap();
    for (t, (&v, &[off, on])) in f64_run.iter().zip(&pairs).enumerate() {
        assert_eq!(v.to_bits(), ((on - off) / (on + off + 1e-16)).to_bits(), "type {t}");
    }

    // Both peaks zero: the epsilon keeps it finite, 0.
    let zero = fri(&FriPeaks::new(vec!["z".into()], Precision::F32, vec![[0.0, 0.0]]).unwrap()).unwrap();
    assert_eq!(zero, vec![0.0]);
}

/// INDICES-06: an F32 run computes each step in f32 (shift, peaks,
/// numerator), the epsilon and divide in f64, and stores f32; the same
/// values as an F64 run are f64 throughout, and the two differ where f32
/// rounding shows.
#[test]
fn indices_06_an_f32_run_follows_flyviss_dtypes_operation_by_operation() {
    // OFF: min -2^-25 (the shift), max 0. ON: max 1 + 2^-23. In f32,
    // 1 + 2^-23 + 2^-25 rounds back to 1 + 2^-23; in f64 it is exact.
    let small = 2f64.powi(-25);
    let big = 1.0 + 2f64.powi(-23);
    let types = numbered(2);
    let (stimuli, layout) = flash_battery();
    let fill = |s: usize, f: usize, _t: usize| -> f64 {
        match (s, f) {
            (2, 300) => -small,
            (3, 300) => big,
            _ => 0.0,
        }
    };
    let r32 = responses(&types, Precision::F32, &stimuli, fill);
    let r64 = responses(&types, Precision::F64, &stimuli, fill);
    let (p32, p64) = (fri_peaks(&r32, &layout).unwrap(), fri_peaks(&r64, &layout).unwrap());
    assert_eq!(p32.precision(), Precision::F32);

    let shift32 = (-small as f32).abs();
    let want32 = [(0.0f32 + shift32) as f64, (big as f32 + shift32) as f64];
    assert_eq!(p32.peaks()[0], want32, "shift and peaks in f32");
    assert_eq!(p64.peaks()[0], [small, big + small], "shift and peaks in f64");
    assert_ne!(p32.peaks()[0], p64.peaks()[0], "f32 rounding is visible");

    let [off, on] = want32.map(|v| v as f32);
    let num = on - off; // f32
    let den = (on + off) as f64 + 1e-16; // f32 sum, f64 epsilon
    let want = ((num as f64) / den) as f32 as f64;
    let f32_fri = fri(&p32).unwrap();
    assert_eq!(f32_fri[0].to_bits(), want.to_bits(), "f32 numerator, f64 epsilon and divide, stored f32");

    let f64_fri = fri(&p64).unwrap();
    let [off, on] = [small, big + small];
    assert_eq!(f64_fri[0].to_bits(), ((on - off) / (on + off + 1e-16)).to_bits(), "f64 throughout");
    assert_ne!(f64_fri[0] as f32 as f64, f64_fri[0], "an F64 run's FRI is not rounded to f32");
}

// ---------------------------------------------------------------------------
// INDICES-07 .. 11: the edges
// ---------------------------------------------------------------------------

/// INDICES-07: `edge_peaks` is `max(0, max over the window)` of the raw
/// voltage per edge and type, frames outside the window ignored, no
/// baseline or normalisation, shaped [144][65]; flashes are not edges.
#[test]
fn indices_07_edge_peaks_are_clipped_window_maxima_shaped_144_by_65() {
    let types = numbered(65);
    let w = window(3, 6);
    let n_frames = 10;
    let mut entries: Vec<(String, Stimulus, Window)> = vec![
        ("flash a".into(), Stimulus::Flash { radius: FRI_RADIUS, intensity: 0 }, w),
        ("flash b".into(), Stimulus::Flash { radius: FRI_RADIUS, intensity: 1 }, w),
    ];
    entries.extend(edges().into_iter().map(|(l, s)| (l, s, w)));
    let stimuli: Vec<(String, usize)> = entries.iter().map(|(l, _, _)| (l.clone(), n_frames)).collect();
    let layout = BatteryLayout::new(entries).unwrap();

    let fill = |s: usize, f: usize, t: usize| -> f64 {
        if f < w.first() || f > w.last() {
            return 99.0; // outside the window: never a peak
        }
        match t {
            0 => (-0.5f32 - f as f32 * 0.01f32) as f64, // all negative: clipped to +0.0
            1 => 3.0,                    // constant: no baseline subtracted
            _ => wiggle(s * 1000 + f * 31 + t),
        }
    };
    let r = responses(&types, Precision::F32, &stimuli, fill);
    let p = edge_peaks(&r, &layout).unwrap();

    assert_eq!(p.peaks().len(), 144);
    assert!(p.peaks().iter().all(|row| row.len() == 65));
    assert_eq!(p.cell_types(), &types[..]);
    for (e, row) in p.peaks().iter().enumerate() {
        let s = e + 2; // the two flashes come first
        assert!(row[0] == 0.0 && row[0].is_sign_positive(), "edge {e}: an all-negative window peaks at +0.0");
        assert_eq!(row[1], 3.0, "edge {e}: raw voltage, not baseline-subtracted or normalised");
        for (t, &got) in row.iter().enumerate().skip(2) {
            let max = (w.first()..=w.last()).map(|f| fill(s, f, t)).fold(f64::NEG_INFINITY, f64::max);
            assert_eq!(got, max.max(0.0), "edge {e}, type {t}");
        }
    }
}

/// A complex-sum DSI written from the formula, for comparison.
fn dsi_reference(p: &EdgePeaks, t: usize) -> [f64; 2] {
    let mut out = [0.0; 2];
    for s in 0..N_SPEEDS {
        let mut len = [0.0; 2];
        let mut tot = [0.0; 2];
        for i in 0..2 {
            let (mut re, mut im) = (0.0, 0.0);
            for a in 0..N_ANGLES {
                let r = p.peaks()[row_of(i, s, a)][t];
                let th = (a as f64 * 30.0).to_radians();
                re += r * th.cos();
                im += r * th.sin();
                tot[i] += r.abs();
            }
            len[i] = re.hypot(im);
        }
        let norm = tot[0].max(tot[1]) + 1e-15;
        for i in 0..2 {
            out[i] += len[i] / norm / N_SPEEDS as f64;
        }
    }
    out
}

/// INDICES-08: the DSI per intensity is `|sum r e^{i theta}|` over the
/// larger of the two intensities' `sum |r|` (plus 1e-15), averaged over
/// the six speeds.
#[test]
fn indices_08_dsi_shares_the_denominator_across_intensities_and_averages_speeds() {
    let types = names(&["A", "B", "C", "D"]);
    let p = peaks_from(&types, Precision::F64, |i, s, a, t| match t {
        // only angle 0; ON twice OFF: ON ~1, OFF ~0.5 (shared denominator)
        0 => {
            if a == 0 {
                (s + 1) as f64 * if i == 1 { 2.0 } else { 1.0 }
            } else {
                0.0
            }
        }
        // the same at every angle: no direction
        1 => 1.0,
        // ON: speeds 0..3 tuned to angle 0, speeds 3..6 untuned; OFF silent
        2 => {
            if i == 1 && (s >= 3 || a == 0) {
                1.0
            } else {
                0.0
            }
        }
        _ => wiggle(i * 1000 + s * 100 + a * 7).abs(),
    });
    let layout = edge_layout(window(0, 0));
    let [off, on] = dsi(&p, &layout).unwrap();
    assert_eq!((off.len(), on.len()), (4, 4));

    assert!(close(on[0], 1.0, 1e-12) && close(off[0], 0.5, 1e-12), "A: on {} off {}", on[0], off[0]);
    assert!(on[1] < 1e-12 && off[1] < 1e-12, "B: on {} off {}", on[1], off[1]);
    assert!(close(on[2], 0.5, 1e-12) && off[2] == 0.0, "C: the mean over speeds, on {} off {}", on[2], off[2]);
    for t in 0..4 {
        let [o, n] = dsi_reference(&p, t);
        assert!(close(off[t], o, 1e-12) && close(on[t], n, 1e-12), "type {t}: {} {} against {o} {n}", off[t], on[t]);
    }
}

/// INDICES-09: the preferred direction per intensity is the angle of the
/// un-normalised sum of `r e^{i theta}` over every angle and speed.
#[test]
fn indices_09_preferred_direction_is_the_angle_of_the_unnormalised_sum() {
    let types = names(&["A", "B"]);
    let p = peaks_from(&types, Precision::F64, |i, s, a, t| match (t, i, s, a) {
        // ON: speed 0 at 0 deg with 10, speed 1 at 90 deg with 1
        (0, 1, 0, 0) => 10.0,
        (0, 1, 1, 3) => 1.0,
        // OFF: 180 deg at every speed
        (0, 0, _, 6) => 2.0,
        (1, _, _, _) => wiggle(i * 1000 + s * 100 + a * 7).abs(),
        _ => 0.0,
    });
    let layout = edge_layout(window(0, 0));
    let [off, on] = preferred_direction(&p, &layout).unwrap();

    let want = 1f64.atan2(10.0);
    assert!(close(on[0], want, 1e-12), "ON {}: atan2(1, 10) = {want}, weighted by magnitude", on[0]);
    assert!(!close(on[0], PI / 4.0, 1e-3), "not an average of per-speed unit vectors");
    assert!(angle_between(off[0], PI) < 1e-12, "OFF {} points at 180 deg", off[0]);

    for (i, pd) in [&off, &on].into_iter().enumerate() {
        let (mut re, mut im) = (0.0, 0.0);
        for s in 0..N_SPEEDS {
            for a in 0..N_ANGLES {
                let r = p.peaks()[row_of(i, s, a)][1];
                let th = (a as f64 * 30.0).to_radians();
                re += r * th.cos();
                im += r * th.sin();
            }
        }
        assert!(angle_between(pd[1], im.atan2(re)) < 1e-12, "B at intensity {i}");
        assert!(pd[1] > -PI - 1e-15 && pd[1] <= PI + 1e-15);
    }
}

const TUNED: [&str; 8] = ["T4a", "T4b", "T4c", "T4d", "T5a", "T5b", "T5c", "T5d"];

/// Eight f32 tuning curves, one per T4/T5 type.
fn curves() -> Vec<(&'static str, [f32; 12])> {
    TUNED
        .iter()
        .enumerate()
        .map(|(k, &name)| {
            let mut c = [0f32; 12];
            for (a, v) in c.iter_mut().enumerate() {
                *v = ((((a + 3 * k) % 12) as f32 - 5.5) / 6.0).abs() * 0.9 + 0.05;
            }
            (name, c)
        })
        .collect()
}

/// INDICES-10: `tuning_r` gives the Pearson r over the 12 angles per known
/// type, intensity and speed ([8][2][6]), 0 for a flat curve, and the best
/// speed's r per type and intensity.
#[test]
fn indices_10_tuning_r_is_pearson_over_angles_with_fillna_and_a_best_speed() {
    let types = names(&["L1", "T4a", "T4b", "T4c", "T4d", "Mi1", "T5a", "T5b", "T5c", "T5d"]);
    let known32 = curves();
    let known = widen_curves(&known32);
    let curve_of = |t: usize| known.iter().find(|(n, _)| *n == types[t]).map(|(_, c)| *c);
    let best_speed = 3;
    let p = peaks_from(&types, Precision::F64, |i, s, a, t| {
        let Some(c) = curve_of(t) else { return wiggle(t * 7 + a) };
        match (i, s) {
            (0, _) => 0.25,                                // OFF flat: r undefined, filled 0
            (1, s) if s == best_speed => 3.0 * c[a] + 0.5, // exact: r = 1
            (1, s) => c[a] + wiggle(s * 131 + a * 17 + t).abs() * 0.5,
            _ => unreachable!(),
        }
    });
    let layout = edge_layout(window(0, 0));
    let r = tuning_r(&p, &layout, &known).unwrap();

    assert_eq!(r.cell_types(), &names(&TUNED)[..], "the 8 known types, in the order given");
    assert_eq!(r.speeds(), SPEEDS);
    for name in TUNED {
        let by = r.r(name).unwrap_or_else(|| panic!("{name}"));
        let t = types.iter().position(|x| x == name).unwrap();
        let curve = curve_of(t).unwrap();
        assert!(by[0].iter().all(|&v| v == 0.0), "{name}: a flat curve scores 0, {:?}", by[0]);
        for (s, &got) in by[1].iter().enumerate() {
            let peaks: Vec<f64> = (0..N_ANGLES).map(|a| p.peaks()[row_of(1, s, a)][t]).collect();
            let want = pearson(&peaks, &curve);
            assert!(close(got, want, 1e-12), "{name} speed {s}: {got} against {want}");
        }
        assert!(close(by[1][best_speed], 1.0, 1e-12), "{name}: the exact speed has r 1");
        let [b_off, b_on] = r.best(name).unwrap();
        assert_eq!(b_on.1, best_speed, "{name}: the best speed is kept");
        assert_eq!(b_on.0, by[1][best_speed]);
        assert_eq!(b_off, (0.0, 0), "{name}: all zeros, the first wins");
    }
}

/// INDICES-11: from f32 peaks, the DSI, preferred direction and tuning r
/// are computed in f64: the results equal those from the same values
/// tagged f64, and are not rounded to f32.
#[test]
fn indices_11_edge_reductions_run_in_f64_from_peaks_of_any_precision() {
    let types = names(&["T4a", "T4b", "T4c", "T4d", "T5a", "T5b", "T5c", "T5d", "Mi1"]);
    let peak = |i: usize, s: usize, a: usize, t: usize| wiggle(i * 7919 + s * 613 + a * 37 + t * 5).abs();
    let p32 = peaks_from(&types, Precision::F32, peak);
    let p64 = peaks_from(&types, Precision::F64, peak);
    let layout = edge_layout(window(0, 0));
    let known = widen_curves(&curves());

    let (d32, d64) = (dsi(&p32, &layout).unwrap(), dsi(&p64, &layout).unwrap());
    let (pd32, pd64) = (preferred_direction(&p32, &layout).unwrap(), preferred_direction(&p64, &layout).unwrap());
    let (r32, r64) = (tuning_r(&p32, &layout, &known).unwrap(), tuning_r(&p64, &layout, &known).unwrap());
    assert_eq!(d32, d64, "DSI");
    assert_eq!(pd32, pd64, "preferred direction");
    assert_eq!(r32, r64, "tuning r");

    let not_f32 = |v: &f64| *v as f32 as f64 != *v;
    assert!(d32.iter().flatten().any(not_f32), "some DSI carries f64 precision");
    assert!(pd32.iter().flatten().any(not_f32), "some preferred direction carries f64 precision");
    assert!(TUNED.iter().flat_map(|n| r32.r(n).unwrap().into_iter().flatten()).any(|v| not_f32(&v)), "tuning r in f64");
}

// ---------------------------------------------------------------------------
// INDICES-12 .. 15: names and flyvis's helpers
// ---------------------------------------------------------------------------

/// INDICES-12: a type requested by name is found at `Responses::type_index`
/// whatever the column order, and a missing one is `MissingCellType`.
#[test]
fn indices_12_cell_types_are_found_by_name_through_type_index() {
    // Known types scattered among others, in no particular order.
    let types = names(&["T5d", "Mi1", "T4c", "T5a", "L2", "T4a", "T5b", "T4d", "Tm3", "T4b", "T5c"]);
    let w = window(0, 1);
    let layout = edge_layout(w);
    let stimuli: Vec<(String, usize)> = edges().into_iter().map(|(l, _)| (l, 2)).collect();
    let r = responses(&types, Precision::F64, &stimuli, |s, f, t| wiggle(s * 211 + f * 3 + t * 19));
    let p = edge_peaks(&r, &layout).unwrap();
    let known = widen_curves(&curves());
    let tr = tuning_r(&p, &layout, &known).unwrap();

    for (name, curve) in &known {
        let col = r.type_index(name).unwrap();
        let peaks: Vec<f64> = (0..N_ANGLES).map(|a| p.peaks()[row_of(1, 2, a)][col]).collect();
        assert!(close(tr.r(name).unwrap()[1][2], pearson(&peaks, curve), 1e-12), "{name} at column {col}");
    }

    // The helpers find names the same way.
    let pd = preferred_direction(&p, &layout).unwrap();
    let d = angular_distance_to_known(&pd, &types, &[("T4c", 90)]).unwrap();
    let col = r.type_index("T4c").unwrap();
    assert!(close(d[0], angle_between(pd[1][col], PI / 2.0), 1e-12));

    let missing = tuning_r(&p, &layout, &[("Nope", [0.0; 12])]);
    assert_eq!(missing, Err(IndexError::MissingCellType("Nope".into())));
}

/// INDICES-13: `fri_correlation_to_known` is the Pearson r of the FRI
/// against the known polarity over the 32 polarised types.
#[test]
fn indices_13_fri_correlation_to_known_is_pearson_over_the_32_polarised_types() {
    let types = numbered(65);
    let polarised: Vec<String> = (0..32).map(|k| format!("c{}", 2 * k)).collect();
    let known: Vec<(&str, i8)> = polarised.iter().enumerate().map(|(k, n)| (n.as_str(), if k % 3 == 0 { -1 } else { 1 })).collect();
    let fri: Vec<f64> = (0..65).map(|t| wiggle(t * 41 + 3)).collect();

    let r = fri_correlation_to_known(&fri, &types, &known).unwrap();
    let x: Vec<f64> = known.iter().map(|(n, _)| fri[types.iter().position(|t| t == n).unwrap()]).collect();
    let y: Vec<f64> = known.iter().map(|&(_, p)| p as f64).collect();
    assert!(close(r, pearson(&x, &y), 1e-12), "{r} against {}", pearson(&x, &y));

    // FRIs of the right sign and equal size: r = 1. The 33 unknown types
    // do not count, whatever they hold.
    let mut perfect = vec![123.0; 65];
    for (n, p) in &known {
        perfect[types.iter().position(|t| t == n).unwrap()] = 0.4 * *p as f64;
    }
    assert!(close(fri_correlation_to_known(&perfect, &types, &known).unwrap(), 1.0, 1e-12));
}

/// INDICES-14: `dsi_correlation_to_known` is the Pearson r over the 18
/// known types of the DSI maxed over intensity against a 0/1 label.
#[test]
fn indices_14_dsi_correlation_to_known_is_pearson_of_max_dsi_against_labels() {
    let types = numbered(30);
    let known: Vec<String> = (0..18).map(|k| format!("c{}", k + 5)).collect();
    let known_ref: Vec<&str> = known.iter().map(String::as_str).collect();
    let tuned: Vec<&str> = known_ref.iter().copied().step_by(2).collect(); // 9 of 18
    let dsi = [(0..30).map(|t| wiggle(t * 13 + 1).abs()).collect::<Vec<f64>>(), (0..30).map(|t| wiggle(t * 29 + 7).abs()).collect()];

    let r = dsi_correlation_to_known(&dsi, &types, &known_ref, &tuned).unwrap();
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for (t, name) in types.iter().enumerate() {
        if known_ref.contains(&name.as_str()) {
            x.push(dsi[0][t].max(dsi[1][t]));
            y.push(if tuned.contains(&name.as_str()) { 1.0 } else { 0.0 });
        }
    }
    assert_eq!(x.len(), 18);
    assert!(close(r, pearson(&x, &y), 1e-12), "{r} against {}", pearson(&x, &y));
}

/// INDICES-15: `angular_distance_to_known` reads T4 types at intensity 1
/// and T5 types at intensity 0 and returns each one's angular distance, the
/// shorter way round, to its known 180/0/90/270 degrees.
#[test]
fn indices_15_angular_distance_pairs_t4_with_on_and_t5_with_off() {
    let types = names(&["Mi1", "T4a", "T4b", "T4c", "T4d", "T5a", "T5b", "T5c", "T5d"]);
    let known: Vec<(&str, u16)> =
        vec![("T4a", 180), ("T4b", 0), ("T4c", 90), ("T4d", 270), ("T5a", 180), ("T5b", 0), ("T5c", 90), ("T5d", 270)];
    // [off, on]; the wrong intensity holds a decoy far from the answer.
    let decoy = 1.234;
    let mut pd = [vec![decoy; 9], vec![decoy; 9]];
    let set = |pd: &mut [Vec<f64>; 2], name: &str, i: usize, v: f64| pd[i][types.iter().position(|t| t == name).unwrap()] = v;
    set(&mut pd, "T4a", 1, -PI + 0.25); // across the +-pi seam
    set(&mut pd, "T4b", 1, 0.1);
    set(&mut pd, "T4c", 1, PI / 2.0 - 0.2);
    set(&mut pd, "T4d", 1, -PI / 2.0 + 0.3); // 270 deg is -90 deg
    set(&mut pd, "T5a", 0, 3.0);
    set(&mut pd, "T5b", 0, -0.4);
    set(&mut pd, "T5c", 0, 2.0);
    set(&mut pd, "T5d", 0, -1.0);

    let d = angular_distance_to_known(&pd, &types, &known).unwrap();
    let want = [0.25, 0.1, 0.2, 0.3, PI - 3.0, 0.4, 2.0 - PI / 2.0, PI / 2.0 - 1.0];
    assert_eq!(d.len(), 8);
    for ((name, _), (&got, &w)) in known.iter().zip(d.iter().zip(&want)) {
        assert!(close(got, w, 1e-12), "{name}: {got} against {w}");
    }
}
