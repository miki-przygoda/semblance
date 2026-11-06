//! Acceptance tests for behaviour group `refusals` of epic fidelity-measures:
//! scoring never fails toward perfect. One test per behaviour id,
//! REFUSALS-01 .. REFUSALS-04 and REFUSALS-06 .. REFUSALS-09; REFUSALS-05 is
//! the `semblance` binary's and lives in `app/tests/acceptance_refusals.rs`.
//!
//! Every test drives `semblance_fidelity`'s public API the way a caller
//! would, with malformed input built by hand, and holds the call to the
//! refusal it must return instead of a score.

use semblance_codec::{Codec, Latent, WeightCodec};
use semblance_connectome::Connectome;
use semblance_fidelity::indices::{
    angular_distance_to_known, dsi, dsi_class, edge_peaks, edge_window, fri, fri_correlation_to_known, fri_peaks,
    fri_window, median_fri, polarity_agreement, preferred_direction, tuning_r, BatteryLayout, EdgePeaks, FriPeaks,
    IndexError, Stimulus, Window, FRI_RADIUS, N_ANGLES, N_SPEEDS,
};
use semblance_fidelity::similarity::{cka, hsic_unbiased, ResponseMatrix, SimilarityError};
use semblance_fidelity::{behavioural_distance, compare_behaviour, size_fidelity_curve, DriveProbe, FidelityError};
use semblance_sim::response::{Precision, ResponseError, Responses};
use semblance_sim::Behaviour;

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

/// Responses over `types`: one stimulus per `(label, n_frames)`, each value
/// `fill(stimulus, frame, type)` (kept f32-exact).
fn responses(types: &[String], stimuli: &[(String, usize)], fill: impl Fn(usize, usize, usize) -> f64) -> Responses {
    let mut r = Responses::new(types.to_vec(), DT, Precision::F32).expect("responses");
    for (s, (label, n)) in stimuli.iter().enumerate() {
        let mut values = Vec::with_capacity(n * types.len());
        for f in 0..*n {
            for t in 0..types.len() {
                values.push(fill(s, f, t) as f32 as f64);
            }
        }
        r.push(label.clone(), *n, values).expect("push");
    }
    r.finish().expect("finish")
}

fn window(first: usize, last: usize) -> Window {
    Window::new(first, last).expect("window")
}

/// The OFF and ON radius-6 flashes, 5 frames each, windowed over frames 1..=3.
fn flash_battery() -> (Vec<(String, usize)>, BatteryLayout) {
    let stimuli = vec![("flash off".to_string(), 5), ("flash on".to_string(), 5)];
    let layout = BatteryLayout::new(vec![
        ("flash off".into(), Stimulus::Flash { radius: FRI_RADIUS, intensity: 0 }, window(1, 3)),
        ("flash on".into(), Stimulus::Flash { radius: FRI_RADIUS, intensity: 1 }, window(1, 3)),
    ])
    .expect("flash layout");
    (stimuli, layout)
}

/// The moving-edge battery's edges over `speeds`, in flyvis's order
/// (intensity, speed, angle), each `(label, stimulus)`.
fn edges_over(speeds: &[f64]) -> Vec<(String, Stimulus)> {
    let mut out = Vec::new();
    for intensity in 0..2u8 {
        for &speed in speeds {
            for a in 0..N_ANGLES {
                let angle_deg = (a * 30) as u16;
                out.push((format!("edge i{intensity} v{speed} a{angle_deg}"), Stimulus::Edge { angle_deg, speed, intensity }));
            }
        }
    }
    out
}

fn edge_layout(edges: &[(String, Stimulus)], w: Window) -> BatteryLayout {
    BatteryLayout::new(edges.iter().map(|(l, s)| (l.clone(), *s, w)).collect()).expect("edge layout")
}

/// Edge peaks with one row per edge, values varying by row and type.
fn edge_peaks_for(types: &[String], n_edges: usize) -> EdgePeaks {
    let rows = (0..n_edges).map(|e| (0..types.len()).map(|t| ((e * 7 + t * 3) % 11) as f64 * 0.25).collect()).collect();
    EdgePeaks::new(types.to_vec(), Precision::F64, rows).expect("edge peaks")
}

/// A synthetic connectome and a copy of it with every weight shifted.
fn network_and_shifted(shift: f32) -> (Connectome, Connectome) {
    let c = Connectome::synthetic(80, 5, 9);
    let w: Vec<f32> = c.weights().iter().map(|x| x + shift).collect();
    let shifted = c.with_weights(&w);
    (c, shifted)
}

fn battery() -> Vec<DriveProbe> {
    vec![DriveProbe { k: 8, amplitude: 1.5, motor: 4, ticks: 150 }, DriveProbe { k: 4, amplitude: 2.0, motor: 4, ticks: 150 }]
}

/// A matrix of `n` rows over `p` columns, `f(row, column)` each, in f64.
fn matrix(n: usize, p: usize, f: impl Fn(usize, usize) -> f64) -> ResponseMatrix {
    let rows: Vec<Vec<f64>> = (0..n).map(|i| (0..p).map(|j| f(i, j)).collect()).collect();
    ResponseMatrix::from_rows((0..p).map(|j| format!("c{j}")).collect(), Precision::F64, &rows).expect("matrix")
}

fn varied(i: usize, j: usize) -> f64 {
    ((i * 31 + j * 17) % 13) as f64 - 6.0
}

// ---------------------------------------------------------------------------
// The legacy distances
// ---------------------------------------------------------------------------

/// REFUSALS-01: recordings that differ in channel, neuron or tick count are
/// an error, never 0.0.
#[test]
fn refusals_01_compare_behaviour_refuses_recordings_of_different_shapes() {
    let busy = Behaviour { channels: vec![vec![1.0; 50]; 4], spike_counts: vec![7; 500], ticks: 50 };

    // busy against empty: the defect that once scored a perfect 0
    let empty = Behaviour::default();
    assert_eq!(compare_behaviour(&busy, &empty), Err(FidelityError::ChannelCount { a: 4, b: 0 }));
    assert_eq!(compare_behaviour(&empty, &busy), Err(FidelityError::ChannelCount { a: 0, b: 4 }));

    let mut fewer_channels = busy.clone();
    fewer_channels.channels.pop();
    assert_eq!(compare_behaviour(&busy, &fewer_channels), Err(FidelityError::ChannelCount { a: 4, b: 3 }));

    let mut fewer_neurons = busy.clone();
    fewer_neurons.spike_counts.truncate(300);
    assert_eq!(compare_behaviour(&busy, &fewer_neurons), Err(FidelityError::NeuronCount { a: 500, b: 300 }));

    let mut shorter = busy.clone();
    shorter.ticks = 40;
    assert_eq!(compare_behaviour(&busy, &shorter), Err(FidelityError::TickCount { a: 50, b: 40 }));

    let mut ragged = busy.clone();
    ragged.channels[1].truncate(10);
    assert_eq!(compare_behaviour(&busy, &ragged), Err(FidelityError::ChannelLength { channel: 1, a: 50, b: 10 }));

    // the same shape still scores, and a recording against itself is 0
    assert_eq!(compare_behaviour(&busy, &busy), Ok(0.0));
}

/// REFUSALS-02: an empty battery is an error, even against a network
/// shifted by +5.0.
#[test]
fn refusals_02_behavioural_distance_refuses_an_empty_battery() {
    let (c, shifted) = network_and_shifted(5.0);
    let none: Vec<DriveProbe> = Vec::new();
    assert_eq!(behavioural_distance(&c, &shifted, &none), Err(FidelityError::EmptyBattery));
    assert_eq!(behavioural_distance(&c, &c, &none), Err(FidelityError::EmptyBattery));
    let err = behavioural_distance(&c, &shifted, &none).unwrap_err();
    assert!(!err.to_string().is_empty());
}

/// A codec whose decode returns a network of a different size: replaying a
/// probe through it gives a recording with a different neuron count.
struct ShrinkingCodec;

impl Codec for ShrinkingCodec {
    fn latent_dim(&self, c: &Connectome) -> usize {
        c.num_synapses()
    }
    fn encode(&self, c: &Connectome) -> Latent {
        c.weights()
    }
    fn decode(&self, _z: &Latent) -> Connectome {
        Connectome::synthetic(60, 5, 9)
    }
}

/// REFUSALS-03: `size_fidelity_curve` propagates both legacy errors.
#[test]
fn refusals_03_size_fidelity_curve_propagates_both_legacy_errors() {
    let (c, _) = network_and_shifted(0.0);
    let lossless: Vec<Box<dyn Codec>> = vec![Box::new(WeightCodec::from_connectome(&c))];

    // an empty battery
    let none: Vec<DriveProbe> = Vec::new();
    assert_eq!(size_fidelity_curve(&c, &lossless, &none).map(|v| v.len()), Err(FidelityError::EmptyBattery));

    // a shape mismatch between the reference's and the decoded network's recordings
    let shrinking: Vec<Box<dyn Codec>> = vec![Box::new(WeightCodec::from_connectome(&c)), Box::new(ShrinkingCodec)];
    assert_eq!(
        size_fidelity_curve(&c, &shrinking, &battery()).map(|v| v.len()),
        Err(FidelityError::NeuronCount { a: 80, b: 60 })
    );

    // and a well-formed sweep still produces its point
    let points = size_fidelity_curve(&c, &lossless, &battery()).expect("a lossless sweep");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].behavioural_distance, 0.0);
}

/// REFUSALS-04: with a non-empty battery, a different network is at a
/// positive distance and the same network at exactly 0.
#[test]
fn refusals_04_a_non_empty_battery_tells_a_different_network_from_the_same_one() {
    let (c, shifted) = network_and_shifted(0.5);
    let b = battery();
    assert!(!b.is_empty());
    let different = behavioural_distance(&c, &shifted, &b).expect("a different network");
    assert!(different > 0.0 && different.is_finite(), "a different network behaves differently: {different}");
    assert_eq!(behavioural_distance(&c, &c, &b), Ok(0.0));
    let decoded = {
        let codec = WeightCodec::from_connectome(&c);
        codec.decode(&codec.encode(&c))
    };
    assert_eq!(behavioural_distance(&c, &decoded, &b), Ok(0.0));
}

// ---------------------------------------------------------------------------
// The response indices
// ---------------------------------------------------------------------------

/// REFUSALS-06: a layout that does not describe the responses is refused by
/// `check` and by every index that reads responses, never scored.
#[test]
fn refusals_06_a_layout_that_does_not_describe_the_responses_is_refused() {
    let types = names(&["R1", "T4a", "T5a"]);
    let (stimuli, layout) = flash_battery();
    let fill = |s: usize, f: usize, t: usize| (s + f + t) as f64 * 0.125;
    let good = responses(&types, &stimuli, fill);
    assert_eq!(layout.check(&good), Ok(()));
    assert!(fri_peaks(&good, &layout).is_ok());

    // a different stimulus count
    let one = responses(&types, &stimuli[..1], fill);
    let three = responses(&types, &[stimuli.clone(), vec![("extra".to_string(), 5)]].concat(), fill);
    for (r, n) in [(&one, 1), (&three, 3)] {
        let want = IndexError::CountMismatch { layout: 2, responses: n };
        assert_eq!(layout.check(r), Err(want.clone()));
        assert_eq!(fri_peaks(r, &layout), Err(want.clone()));
        assert_eq!(edge_peaks(r, &layout), Err(want));
    }

    // a label mismatch
    let relabelled = responses(&types, &[stimuli[0].clone(), ("flash ON".to_string(), 5)], fill);
    let want = IndexError::LabelMismatch { stimulus: 1, layout: "flash on".into(), responses: "flash ON".into() };
    assert_eq!(layout.check(&relabelled), Err(want.clone()));
    assert_eq!(fri_peaks(&relabelled, &layout), Err(want));

    // a window ending at or past a stimulus's last frame
    let short = responses(&types, &[stimuli[0].clone(), (stimuli[1].0.clone(), 3)], fill);
    let want = IndexError::WindowPastEnd { stimulus: 1, last: 3, n_frames: 3 };
    assert_eq!(layout.check(&short), Err(want.clone()));
    assert_eq!(fri_peaks(&short, &layout), Err(want));

    // the same for an edge battery and edge_peaks
    let edges = edges_over(&SPEEDS);
    let edge_stimuli: Vec<(String, usize)> = edges.iter().map(|(l, _)| (l.clone(), 4)).collect();
    let edge_r = responses(&types, &edge_stimuli, fill);
    let past = edge_layout(&edges, window(1, 4));
    assert_eq!(past.check(&edge_r), Err(IndexError::WindowPastEnd { stimulus: 0, last: 4, n_frames: 4 }));
    assert_eq!(edge_peaks(&edge_r, &past), Err(IndexError::WindowPastEnd { stimulus: 0, last: 4, n_frames: 4 }));
    assert!(edge_peaks(&edge_r, &edge_layout(&edges, window(1, 3))).is_ok());
}

/// REFUSALS-07: a cell type absent from the responses is `MissingCellType`.
#[test]
fn refusals_07_a_cell_type_absent_from_the_responses_is_missing() {
    let types = names(&["R1", "T4a", "T5a"]);
    let edges = edges_over(&SPEEDS);
    let edge_stimuli: Vec<(String, usize)> = edges.iter().map(|(l, _)| (l.clone(), 4)).collect();
    let r = responses(&types, &edge_stimuli, |s, f, t| ((s * 5 + f * 3 + t) % 7) as f64 * 0.25);
    let layout = edge_layout(&edges, window(1, 3));
    let peaks = edge_peaks(&r, &layout).expect("edge peaks");
    assert_eq!(r.type_index("T4b"), None);

    let curve = [1.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5];
    assert_eq!(tuning_r(&peaks, &layout, &[("T4b", curve)]), Err(IndexError::MissingCellType("T4b".into())));
    assert!(tuning_r(&peaks, &layout, &[("T4a", curve)]).is_ok());

    let pd = preferred_direction(&peaks, &layout).expect("preferred directions");
    assert_eq!(angular_distance_to_known(&pd, r.cell_types(), &[("T5c", 90)]), Err(IndexError::MissingCellType("T5c".into())));

    let fri_values = vec![0.5, -0.25, 0.75];
    assert_eq!(
        fri_correlation_to_known(&fri_values, r.cell_types(), &[("R1", 1), ("L1", -1)]),
        Err(IndexError::MissingCellType("L1".into()))
    );
    assert_eq!(polarity_agreement(&fri_values, r.cell_types(), &[("Mi1", 1)]), Err(IndexError::MissingCellType("Mi1".into())));
}

/// Holds `e` to its variant's name: a compile error here when a variant is
/// added names the test that must cover it.
fn index_variant(e: &IndexError) -> &'static str {
    match e {
        IndexError::CountMismatch { .. } => "CountMismatch",
        IndexError::LabelMismatch { .. } => "LabelMismatch",
        IndexError::WindowPastEnd { .. } => "WindowPastEnd",
        IndexError::BadWindow { .. } => "BadWindow",
        IndexError::BadStimulus { .. } => "BadStimulus",
        IndexError::BadDt(_) => "BadDt",
        IndexError::BadParameter { .. } => "BadParameter",
        IndexError::MissingCellType(_) => "MissingCellType",
        IndexError::NonFinite(_) => "NonFinite",
        IndexError::NotF32(_) => "NotF32",
        IndexError::Empty(_) => "Empty",
        IndexError::BatteryShape(_) => "BatteryShape",
        IndexError::Length { .. } => "Length",
        IndexError::BadKnown { .. } => "BadKnown",
    }
}

const INDEX_VARIANTS: [&str; 14] = [
    "CountMismatch",
    "LabelMismatch",
    "WindowPastEnd",
    "BadWindow",
    "BadStimulus",
    "BadDt",
    "BadParameter",
    "MissingCellType",
    "NonFinite",
    "NotF32",
    "Empty",
    "BatteryShape",
    "Length",
    "BadKnown",
];

fn response_variant(e: &ResponseError) -> &'static str {
    match e {
        ResponseError::NoCellTypes => "NoCellTypes",
        ResponseError::RepeatedCellType(_) => "RepeatedCellType",
        ResponseError::BadDt(_) => "BadDt",
        ResponseError::NoFrames { .. } => "NoFrames",
        ResponseError::Shape { .. } => "Shape",
        ResponseError::NonFinite { .. } => "NonFinite",
        ResponseError::EmptyBattery => "EmptyBattery",
    }
}

const RESPONSE_VARIANTS: [&str; 7] = ["NoCellTypes", "RepeatedCellType", "BadDt", "NoFrames", "Shape", "NonFinite", "EmptyBattery"];

/// REFUSALS-08: non-finite values, empty inputs and the wrong number of
/// angles or speeds are refused with the matching `IndexError`; every
/// `IndexError` and `ResponseError` variant is produced here at least once.
#[test]
fn refusals_08_malformed_index_input_is_refused_with_the_matching_error() {
    let mut seen_index: Vec<&str> = Vec::new();
    fn expect(seen: &mut Vec<&'static str>, got: Result<(), IndexError>, want: IndexError) {
        assert_eq!(got, Err(want.clone()));
        seen.push(index_variant(&want));
    }
    macro_rules! index {
        ($got:expr, $want:expr $(,)?) => {
            expect(&mut seen_index, $got, $want)
        };
    }
    let types = names(&["R1", "T4a", "T5a"]);
    let fill = |s: usize, f: usize, t: usize| (s + 2 * f + 3 * t) as f64 * 0.125;
    let (stimuli, layout) = flash_battery();

    // a Responses whose shape disagrees with the layout
    let r1 = responses(&types, &stimuli[..1], fill);
    index!(fri_peaks(&r1, &layout).map(drop), IndexError::CountMismatch { layout: 2, responses: 1 });
    let relabel = responses(&types, &[("x".to_string(), 5), stimuli[1].clone()], fill);
    index!(layout.check(&relabel), IndexError::LabelMismatch { stimulus: 0, layout: "flash off".into(), responses: "x".into() });
    let short = responses(&types, &[("flash off".to_string(), 2), stimuli[1].clone()], fill);
    index!(layout.check(&short), IndexError::WindowPastEnd { stimulus: 0, last: 3, n_frames: 2 });

    // an empty battery, and other empty input
    index!(BatteryLayout::new(Vec::new()).map(drop), IndexError::Empty("battery layout"));
    index!(median_fri(&[]).map(drop), IndexError::Empty("per-model values"));
    index!(median_fri(&[vec![]]).map(drop), IndexError::Empty("a model's values"));
    index!(FriPeaks::new(Vec::new(), Precision::F64, Vec::new()).map(drop), IndexError::Empty("FRI peaks"));
    index!(EdgePeaks::new(types.clone(), Precision::F64, Vec::new()).map(drop), IndexError::Empty("edge peaks"));
    index!(fri_correlation_to_known(&[0.5, -0.5, 0.1], &types, &[]).map(drop), IndexError::Empty("known polarities"));
    let flash_r = responses(&types, &stimuli, fill);
    index!(edge_peaks(&flash_r, &layout).map(drop), IndexError::Empty("edge stimuli"));

    // non-finite values
    index!(
        FriPeaks::new(names(&["R1"]), Precision::F64, vec![[1.0, f64::NAN]]).map(drop),
        IndexError::NonFinite("the FRI peak of R1".into()),
    );
    index!(
        fri_correlation_to_known(&[f64::INFINITY, -0.5, 0.1], &types, &[("R1", 1), ("T4a", -1)]).map(drop),
        IndexError::NonFinite("the FRI of R1".into()),
    );
    assert_eq!(median_fri(&[vec![0.1, f64::NAN, 0.3]]), Err(IndexError::NonFinite("model 0's value for type 1".into())));
    let constant = fri_correlation_to_known(&[0.5, 0.5, 0.5], &types, &[("R1", 1), ("T4a", -1)]);
    assert!(matches!(constant, Err(IndexError::NonFinite(_))), "{constant:?}");
    // a peak pair summing to -1e-16 gives an infinite FRI
    let blowup = FriPeaks::new(names(&["R1"]), Precision::F64, vec![[-1e-16, 0.0]]).expect("finite peaks");
    assert_eq!(fri(&blowup), Err(IndexError::NonFinite("the FRI of R1".into())));

    // the wrong number of speeds, and of angles
    let five = edges_over(&SPEEDS[..5]);
    let five_layout = edge_layout(&five, window(0, 1));
    let five_peaks = edge_peaks_for(&types, five.len());
    for got in [dsi(&five_peaks, &five_layout).map(drop), preferred_direction(&five_peaks, &five_layout).map(drop)] {
        index!(got, IndexError::BatteryShape(format!("the edge reductions need {N_SPEEDS} speeds, found 5")));
    }
    let mut eleven = edges_over(&SPEEDS);
    eleven.retain(|(_, s)| !matches!(s, Stimulus::Edge { angle_deg: 330, intensity: 1, .. }));
    let eleven_layout = edge_layout(&eleven, window(0, 1));
    let eleven_peaks = edge_peaks_for(&types, eleven.len());
    let got = dsi(&eleven_peaks, &eleven_layout);
    assert!(matches!(&got, Err(IndexError::BatteryShape(m)) if m.contains("angle 330")), "{got:?}");
    let mut off_grid = edges_over(&SPEEDS);
    off_grid[0].1 = Stimulus::Edge { angle_deg: 45, speed: SPEEDS[0], intensity: 0 };
    let got = dsi(&edge_peaks_for(&types, off_grid.len()), &edge_layout(&off_grid, window(0, 1)));
    assert!(matches!(&got, Err(IndexError::BatteryShape(m)) if m.contains("angle 45")), "{got:?}");
    // peaks that are not one row per edge
    let full = edges_over(&SPEEDS);
    index!(
        dsi(&edge_peaks_for(&types, 10), &edge_layout(&full, window(0, 1))).map(drop),
        IndexError::Length { what: "edge peaks", expected: full.len(), found: 10 },
    );
    // the FRI's flashes missing
    let edge_only = BatteryLayout::new(vec![("e".into(), full[0].1, window(0, 1))]).expect("layout");
    let edge_only_r = responses(&types, &[("e".to_string(), 3)], fill);
    let got = fri_peaks(&edge_only_r, &edge_only);
    assert!(matches!(got, Err(IndexError::BatteryShape(_))), "{got:?}");
    seen_index.push("BatteryShape");

    // the remaining variants
    index!(Window::new(5, 2).map(drop), IndexError::BadWindow { first: 5, last: 2 });
    let bad_speed = BatteryLayout::new(vec![("e".into(), Stimulus::Edge { angle_deg: 0, speed: f64::NAN, intensity: 1 }, window(0, 1))]);
    assert!(matches!(bad_speed, Err(IndexError::BadStimulus { stimulus: 0, .. })), "{bad_speed:?}");
    seen_index.push("BadStimulus");
    index!(fri_window(0.0).map(drop), IndexError::BadDt(0.0));
    index!(edge_window(-1.0, DT, 1.0, (-10, 11)).map(drop), IndexError::BadParameter { what: "edge speed", value: -1.0 });
    index!(
        dsi_class(&[vec![0.5]], 0.357, 0.1, 0.0).map(drop),
        IndexError::BadParameter { what: "alpha", value: 0.0 },
    );
    index!(
        fri_correlation_to_known(&[0.5, -0.5, 0.1], &types, &[("R1", 1), ("Mi1", -1)]).map(drop),
        IndexError::MissingCellType("Mi1".into()),
    );
    index!(
        FriPeaks::new(names(&["R1"]), Precision::F32, vec![[0.1, 0.5]]).map(drop),
        IndexError::NotF32("the FRI peak 0.1 of R1".into()),
    );
    index!(
        FriPeaks::new(types.clone(), Precision::F64, vec![[0.1, 0.5]]).map(drop),
        IndexError::Length { what: "FRI peaks", expected: 3, found: 1 },
    );
    index!(
        polarity_agreement(&[0.5, -0.5, 0.1], &types, &[("R1", 0)]).map(drop),
        IndexError::BadKnown { cell_type: "R1".into(), reason: "polarity 0 is not +1 or -1".into() },
    );

    for v in INDEX_VARIANTS {
        assert!(seen_index.contains(&v), "no refusal test produced IndexError::{v}");
    }

    // every ResponseError variant
    let mut seen_response: Vec<&str> = Vec::new();
    let mut response = |got: Result<(), ResponseError>, want: ResponseError| {
        assert_eq!(got, Err(want.clone()));
        seen_response.push(response_variant(&want));
    };
    response(Responses::new(Vec::new(), DT, Precision::F32).map(drop), ResponseError::NoCellTypes);
    response(
        Responses::new(names(&["R1", "R1"]), DT, Precision::F32).map(drop),
        ResponseError::RepeatedCellType("R1".into()),
    );
    response(Responses::new(types.clone(), -DT, Precision::F32).map(drop), ResponseError::BadDt(-DT));
    let mut r = Responses::new(types.clone(), DT, Precision::F32).expect("responses");
    response(r.push("a".into(), 0, Vec::new()), ResponseError::NoFrames { stimulus: 0 });
    response(r.push("a".into(), 2, vec![0.0; 5]), ResponseError::Shape { stimulus: 0, expected: 6, found: 5 });
    response(
        r.push("a".into(), 2, vec![0.0, 0.0, 0.0, 0.0, f64::NAN, 0.0]),
        ResponseError::NonFinite { stimulus: 0, frame: 1, cell_type: "T4a".into() },
    );
    response(r.finish().map(drop), ResponseError::EmptyBattery);
    for v in RESPONSE_VARIANTS {
        assert!(seen_response.contains(&v), "no refusal test produced ResponseError::{v}");
    }
}

// ---------------------------------------------------------------------------
// The similarity
// ---------------------------------------------------------------------------

/// REFUSALS-09: mismatched, too-small, scale-free or non-finite matrices are
/// a `SimilarityError` from `hsic_unbiased` and `cka`, never a similarity.
#[test]
fn refusals_09_hsic_and_cka_refuse_malformed_matrices() {
    let x = matrix(8, 3, varied);
    let y = matrix(8, 3, |i, j| varied(i + 3, j + 1));
    assert!(hsic_unbiased(&x, &y).is_ok());
    assert!(cka(&x, &y).is_ok());

    // a row count mismatch
    let fewer_rows = matrix(7, 3, varied);
    assert_eq!(hsic_unbiased(&x, &fewer_rows), Err(SimilarityError::RowMismatch { x: 8, y: 7 }));
    assert_eq!(cka(&x, &fewer_rows), Err(SimilarityError::RowMismatch { x: 8, y: 7 }));

    // a column count mismatch
    let more_cols = matrix(8, 4, varied);
    assert_eq!(hsic_unbiased(&x, &more_cols), Err(SimilarityError::ColumnMismatch { x: 3, y: 4 }));
    assert_eq!(cka(&more_cols, &x), Err(SimilarityError::ColumnMismatch { x: 4, y: 3 }));

    // fewer than 4 rows
    let three = matrix(3, 3, varied);
    let three_b = matrix(3, 3, |i, j| varied(i + 1, j));
    let too_few = SimilarityError::TooFew { what: "rows", need: 4, found: 3 };
    assert_eq!(hsic_unbiased(&three, &three_b), Err(too_few.clone()));
    assert_eq!(cka(&three, &three_b), Err(too_few));
    assert!(hsic_unbiased(&matrix(4, 3, varied), &matrix(4, 3, |i, j| varied(i + 1, j))).is_ok());

    // a non-positive H(X,X): a matrix with no variation has no scale
    let zero = matrix(8, 3, |_, _| 0.0);
    assert_eq!(hsic_unbiased(&zero, &zero), Ok(0.0));
    assert_eq!(cka(&zero, &y), Err(SimilarityError::NonPositiveHsic { which: "x", value: 0.0 }));
    assert_eq!(cka(&y, &zero), Err(SimilarityError::NonPositiveHsic { which: "y", value: 0.0 }));

    // non-finite values: refused when the matrix is built, so never reach
    // the similarity; and values whose products overflow are refused by it
    let names3: Vec<String> = (0..3).map(|j| format!("c{j}")).collect();
    let mut rows: Vec<Vec<f64>> = (0..8).map(|i| (0..3).map(|j| varied(i, j)).collect()).collect();
    rows[5][2] = f64::NAN;
    assert_eq!(
        ResponseMatrix::from_rows(names3.clone(), Precision::F64, &rows),
        Err(SimilarityError::NonFinite("row 5, column 2".into()))
    );
    rows[5][2] = f64::INFINITY;
    assert!(matches!(ResponseMatrix::from_rows(names3, Precision::F64, &rows), Err(SimilarityError::NonFinite(_))));
    let huge = matrix(8, 3, |i, j| varied(i, j) * 1e200);
    let got = hsic_unbiased(&huge, &huge);
    assert!(matches!(got, Err(SimilarityError::NonFinite(_))), "{got:?}");
    let got = cka(&huge, &x);
    assert!(matches!(got, Err(SimilarityError::NonFinite(_))), "{got:?}");
}
