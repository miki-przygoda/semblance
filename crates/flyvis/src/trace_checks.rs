//! The stimulus-trace gates: epic 0.3.0's stage 2.
//!
//! Every reference run (models 000 and 049, dt 1/200 and 0.02, float32 and
//! float64) settles as in stage 1, then plays all twelve reference
//! sequences -- four flashes, eight moving edges -- each from that settled
//! state. Every frame of flyvis's own input (`input.f64.bin`) is scattered
//! onto the photoreceptors (`wiring::scatter_hexals`) and stepped. The 65
//! centre neurons are compared at every frame, and all 45,669 neurons at the
//! manifest's six snapshot frames.
//!
//!   exact  torch's synapse counts. Every value must equal flyvis's.
//!   libm   Rust's `exp`. Within the spec's bounds (R3): 1e-10 in f64, twice
//!          flyvis's own f32/f64 trace gap in f32.
//!
//! Neither is to be relaxed. A failure stops the work for diagnosis; the
//! first step is to rerun from flyvis's settled state (`Start::Flyvis`).
//! Spec: docs/superpowers/specs/2026-10-28-stimulus-traces-design.md.
//!
//! The gates are driven by flyvis's input, not Rust's stimuli (ruling R1):
//! Rust's moving edges differ from flyvis's by up to 3.9e-3, which would
//! bury an arithmetic gate. `stimulus_checks`, below, holds Rust's
//! generators to that same input separately.
//!
//! The trace files (about 220 MB) are loaded lazily, each once and only by
//! these tests: the settling gates never read them. Every test prints its
//! result (visible with `--nocapture`); read the libm ratios. All skip
//! without fixtures; `SEMBLANCE_REQUIRE_FIXTURES=1` makes a skip a failure.

use std::sync::OnceLock;

use crate::expand::{hex_coords, DEFAULT_EXTENT};
use crate::gate_support::{cell_type, diff, fixture, read, settle_state, Counts, Diff, Fixture, Precision, F64_BOUND};
use crate::json;
use crate::reference::{parse_sequences, BlockKey, Dtype, FileInfo, SequenceInfo};
use crate::test_support::{load_reference_fixture, or_skip, read_fixture_bytes, read_fixture_text};
use crate::wiring::scatter_hexals;

const HEXALS: usize = 721;

/// Snapshot frames per sequence, at every dt (`snapshots.*.bin`'s first axis).
const SNAPSHOTS: usize = 6;

/// flyvis's own float32-vs-float64 gaps over the stimulus traces,
/// PROVENANCE.json `f32_f64_gap`: (model, dt, centre, snapshots). The libm
/// gate's f32 bound is twice each (spec R3). They include the moving edges'
/// one rounding of the input to f32. `trace_bounds_match_provenance` pins
/// these to the file.
const TRACE_F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.7249018977594233e-06, 2.6318956276583094e-06),
    ("049", "1/200", 2.600224337356849e-05, 5.1135401793800384e-05),
    ("000", "1/50", 2.5357808688397654e-06, 2.9820266052027478e-06),
    ("049", "1/50", 3.4419031081256435e-05, 1.4233448473766064e-04),
];

const TRACE_FILES: [&str; 5] =
    ["input.f64.bin", "centre.f32.bin", "centre.f64.bin", "snapshots.f32.bin", "snapshots.f64.bin"];

/// One trace file's bytes, loaded on first use. Prints nothing.
fn trace_file(name: &str) -> Result<&'static [u8], String> {
    static FILES: [OnceLock<Result<Vec<u8>, String>>; 5] = [const { OnceLock::new() }; 5];
    let i = TRACE_FILES.iter().position(|&n| n == name).expect("one of TRACE_FILES");
    FILES[i]
        .get_or_init(|| read_fixture_bytes(&format!("flyvis-reference/{name}")))
        .as_ref()
        .map(Vec::as_slice)
        .map_err(String::clone)
}

/// The manifest's twelve sequences, parsed once. Prints nothing.
fn sequences() -> Result<&'static [SequenceInfo], String> {
    static SEQUENCES: OnceLock<Result<Vec<SequenceInfo>, String>> = OnceLock::new();
    SEQUENCES
        .get_or_init(|| {
            let text = read_fixture_text("flyvis-reference/manifest.json")?;
            Ok(parse_sequences(&text).expect("manifest.json's sequences must parse"))
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(String::clone)
}

/// Everything one precision's trace run reads.
struct Traces {
    f: &'static Fixture,
    sequences: &'static [SequenceInfo],
    input: &'static [u8],
    centre: &'static [u8],
    snapshots: &'static [u8],
}

fn load_traces<R: Precision>(f: &'static Fixture) -> Result<Traces, String> {
    Ok(Traces {
        f,
        sequences: sequences()?,
        input: trace_file("input.f64.bin")?,
        centre: trace_file(&format!("centre.{}.bin", R::SUFFIX))?,
        snapshots: trace_file(&format!("snapshots.{}.bin", R::SUFFIX))?,
    })
}

/// The traces for precision `R`, or `None` after printing a skip notice.
fn traces<R: Precision>() -> Option<Traces> {
    or_skip(load_traces::<R>(fixture()?))
}

fn file<'a>(f: &'a Fixture, name: &str) -> &'a FileInfo {
    f.manifest.file(name).unwrap_or_else(|| panic!("{name} is not in the manifest"))
}

/// For each hexal h, the hexal at (-u, -v): a point reflection of the frame.
fn mirrored_hexals() -> Vec<usize> {
    let c: Vec<(i32, i32)> = hex_coords(DEFAULT_EXTENT).collect();
    c.iter()
        .map(|&(u, v)| c.iter().position(|&p| p == (-u, -v)).expect("the lattice is point-symmetric"))
        .collect()
}

/// Which settled state each sequence starts from.
#[derive(Clone, Copy)]
enum Start {
    /// Rust's own settling (spec R2): settling and stimulus, end to end.
    Rust,
    /// flyvis's `settled.<p>.bin`: R2's first diagnostic step, isolating the
    /// stimulus phase.
    Flyvis,
}

/// One sequence played and compared, Rust's and flyvis's values widened to
/// f64.
struct Played {
    sequence: u32,
    n_frames: usize,
    snapshot_frames: Vec<usize>,
    /// Over [n_frames, 65].
    centre: Diff,
    /// Over [6, 45,669].
    snapshots: Diff,
    /// The first unequal centre value, as an index into [n_frames, 65].
    first: Option<usize>,
}

/// Settles reference run `run`, then plays `sequences` from the settled
/// state, each with flyvis's input (mirrored hexal by hexal if `mirror`), and
/// compares every frame's centre and every snapshot with flyvis's.
fn play<R: Precision>(t: &Traces, run: u32, counts: Counts, start: Start, sequences: &[u32], mirror: bool) -> Vec<Played> {
    let f = t.f;
    let info = &f.manifest.runs[run as usize];
    let (mut g, _) = settle_state::<R>(f, run, &info.model, counts);
    if let Start::Flyvis = start {
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
        let settled = read::<R>(f, &format!("settled.{}.bin", R::SUFFIX), &key);
        g.set_state(&settled.data).expect("flyvis's settled state: one finite value per node");
    }
    let settled = g.state().to_vec();
    let n = f.net.num_nodes();
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let source: Vec<usize> = if mirror { mirrored_hexals() } else { (0..HEXALS).collect() };
    let input_file = file(f, "input.f64.bin");
    let centre_file = file(f, &format!("centre.{}.bin", R::SUFFIX));
    let snapshots_file = file(f, &format!("snapshots.{}.bin", R::SUFFIX));

    // Zeroed once per run: every frame overwrites the same input nodes, and
    // every other node stays 0, as in flyvis's zeroed buffer.
    let mut input = vec![R::ZERO; n];
    let mut frame = vec![R::ZERO; HEXALS];
    let mut played = Vec::with_capacity(sequences.len());
    for &s in sequences {
        let q = &t.sequences[s as usize];
        assert_eq!(q.index, s, "manifest sequences must be listed in index order");
        let layout = q.frames(&info.dt).unwrap_or_else(|| panic!("sequence {s} has no frames at dt {}", info.dt));
        let n_frames = layout.n_frames;
        assert_eq!(layout.snapshot_frames.len(), SNAPSHOTS, "sequence {s}'s snapshot frames");
        g.set_state(&settled).expect("the settled state: one finite value per node");

        let x = input_file
            .read_f64(t.input, &BlockKey { run: None, dt: &info.dt, sequence: Some(s) })
            .unwrap_or_else(|e| panic!("input.f64.bin: {e}"));
        assert_eq!(x.shape, [n_frames, HEXALS], "input shape, sequence {s}");
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(s) };
        let flyvis_centre = R::read(centre_file, t.centre, &key).unwrap_or_else(|e| panic!("{}: {e}", centre_file.name));
        assert_eq!(flyvis_centre.shape, [n_frames, centre.len()], "centre shape, sequence {s}");
        let flyvis_snapshots =
            R::read(snapshots_file, t.snapshots, &key).unwrap_or_else(|e| panic!("{}: {e}", snapshots_file.name));
        assert_eq!(flyvis_snapshots.shape, [SNAPSHOTS, n], "snapshots shape, sequence {s}");

        let mut rust_centre = Vec::with_capacity(n_frames * centre.len());
        let mut rust_snapshots = Vec::with_capacity(SNAPSHOTS * n);
        for i in 0..n_frames {
            let row = &x.data[i * HEXALS..(i + 1) * HEXALS];
            for (slot, &h) in frame.iter_mut().zip(&source) {
                *slot = R::from_f64(row[h]);
            }
            scatter_hexals(&f.input_nodes, &frame, &mut input).expect("721 hexals onto 8 x 721 input nodes");
            g.step(&input).expect("one input value per node");
            if let Some(k) = g.state().iter().position(|v| !v.is_finite()) {
                panic!("run {run}: node {k} ({}) is not finite after frame {i} of sequence {s}", cell_type(&f.net, k));
            }
            // frame i is the state after i + 1 steps from the settled state
            rust_centre.extend(centre.iter().map(|&k| g.state()[k].to_f64()));
            if layout.snapshot_frames.contains(&i) {
                rust_snapshots.extend(g.state().iter().map(|v| v.to_f64()));
            }
        }
        let flyvis_centre: Vec<f64> = flyvis_centre.data.iter().map(|v| v.to_f64()).collect();
        let flyvis_snapshots: Vec<f64> = flyvis_snapshots.data.iter().map(|v| v.to_f64()).collect();
        played.push(Played {
            sequence: s,
            n_frames,
            snapshot_frames: layout.snapshot_frames.clone(),
            first: rust_centre.iter().zip(&flyvis_centre).position(|(a, b)| a != b),
            centre: diff(&rust_centre, &flyvis_centre),
            snapshots: diff(&rust_snapshots, &flyvis_snapshots),
        });
    }
    played
}

fn all_sequences(t: &Traces) -> Vec<u32> {
    t.sequences.iter().map(|q| q.index).collect()
}

fn exact<R: Precision>(run: u32) {
    let Some(t) = traces::<R>() else { return };
    let f = t.f;
    let info = &f.manifest.runs[run as usize];
    let played = play::<R>(&t, run, Counts::Torch, Start::Rust, &all_sequences(&t), false);
    let (n, n_centre) = (f.net.num_nodes(), f.manifest.centre_nodes.len());
    let mut failures = Vec::new();
    for p in &played {
        if p.centre.differ + p.snapshots.differ == 0 {
            continue;
        }
        let first = p.first.map_or_else(
            || "the snapshots only".to_string(),
            |i| format!("frame {} ({})", i / n_centre, f.manifest.centre_nodes[i % n_centre].cell_type),
        );
        failures.push(format!(
            "sequence {}: {} of {} centre and {} of {} snapshot values differ; first difference at {first}; \
             largest centre error {:e}; largest snapshot error {:e} at node {} ({})",
            p.sequence,
            p.centre.differ,
            p.n_frames * n_centre,
            p.snapshots.differ,
            SNAPSHOTS * n,
            p.centre.max,
            p.snapshots.max,
            p.snapshots.at % n,
            cell_type(&f.net, p.snapshots.at % n)
        ));
    }
    eprintln!(
        "trace exact run {run} ({}, dt {}, {}): {} of {} sequences bit-equal, centre and snapshots",
        info.model, info.dt, R::SUFFIX, played.len() - failures.len(), played.len()
    );
    assert!(failures.is_empty(), "run {run} is not bit-equal to flyvis under stimulus:\n{}", failures.join("\n"));
}

/// R3's bounds for one run: (centre, snapshots).
fn trace_bounds(model: &str, dt: &str, precision: Dtype) -> (f64, f64) {
    match precision {
        Dtype::F64 => (F64_BOUND, F64_BOUND),
        Dtype::F32 => {
            let &(_, _, c, s) =
                TRACE_F32_GAPS.iter().find(|g| g.0 == model && g.1 == dt).expect("a gap for every f32 run");
            (2.0 * c, 2.0 * s)
        }
    }
}

fn libm<R: Precision>(run: u32) {
    let Some(t) = traces::<R>() else { return };
    let f = t.f;
    let info = &f.manifest.runs[run as usize];
    let (bound_centre, bound_snapshots) = trace_bounds(&info.model, &info.dt, info.precision);
    let played = play::<R>(&t, run, Counts::Libm, Start::Rust, &all_sequences(&t), false);
    let (n, n_centre) = (f.net.num_nodes(), f.manifest.centre_nodes.len());

    let ratio = |p: &Played| (p.centre.max / bound_centre).max(p.snapshots.max / bound_snapshots);
    let worst = played.iter().max_by(|a, b| ratio(a).total_cmp(&ratio(b))).expect("twelve sequences");
    let c = played.iter().max_by(|a, b| a.centre.max.total_cmp(&b.centre.max)).expect("twelve sequences");
    let s = played.iter().max_by(|a, b| a.snapshots.max.total_cmp(&b.snapshots.max)).expect("twelve sequences");
    let c_node = &f.manifest.centre_nodes[c.centre.at % n_centre];
    let s_node = s.snapshots.at % n;
    let (equal, total) = played.iter().fold((0, 0), |(equal, total), p| {
        let size = p.n_frames * n_centre + SNAPSHOTS * n;
        (equal + size - p.centre.differ - p.snapshots.differ, total + size)
    });
    eprintln!(
        "trace libm run {run} ({}, dt {}, {}): worst sequence {}, {:.3e} of bound; \
         centre max {:.3e} in sequence {} at frame {} node {} ({}), {:.3e} of bound; \
         snapshots max {:.3e} in sequence {} at frame {} node {} ({}), {:.3e} of bound; bit-equal {equal} of {total}",
        info.model, info.dt, R::SUFFIX, worst.sequence, ratio(worst),
        c.centre.max, c.sequence, c.centre.at / n_centre, c_node.node, c_node.cell_type, c.centre.max / bound_centre,
        s.snapshots.max, s.sequence, s.snapshot_frames[s.snapshots.at / n], s_node, cell_type(&f.net, s_node),
        s.snapshots.max / bound_snapshots,
    );
    let over: Vec<String> = played
        .iter()
        .filter(|p| p.centre.max > bound_centre || p.snapshots.max > bound_snapshots)
        .map(|p| format!("sequence {}: centre {:e}, snapshots {:e}", p.sequence, p.centre.max, p.snapshots.max))
        .collect();
    assert!(
        over.is_empty(),
        "run {run}: Rust's exp exceeds the spec's bounds (centre {bound_centre:e}, snapshots {bound_snapshots:e}):\n{}",
        over.join("\n")
    );
}

macro_rules! trace_runs {
    ($($exact:ident, $libm:ident, $run:expr, $t:ty;)*) => {
        $(
            #[test]
            fn $exact() { exact::<$t>($run) }
            #[test]
            fn $libm() { libm::<$t>($run) }
        )*
    };
}

trace_runs! {
    trace_exact_run_0_000_1_200_f32, trace_libm_run_0_000_1_200_f32, 0, f32;
    trace_exact_run_1_000_1_200_f64, trace_libm_run_1_000_1_200_f64, 1, f64;
    trace_exact_run_2_000_1_50_f32, trace_libm_run_2_000_1_50_f32, 2, f32;
    trace_exact_run_3_000_1_50_f64, trace_libm_run_3_000_1_50_f64, 3, f64;
    trace_exact_run_4_049_1_200_f32, trace_libm_run_4_049_1_200_f32, 4, f32;
    trace_exact_run_5_049_1_200_f64, trace_libm_run_5_049_1_200_f64, 5, f64;
    trace_exact_run_6_049_1_50_f32, trace_libm_run_6_049_1_50_f32, 6, f32;
    trace_exact_run_7_049_1_50_f64, trace_libm_run_7_049_1_50_f64, 7, f64;
}

/// The gaps above are PROVENANCE.json's, bit for bit.
#[test]
fn trace_bounds_match_provenance() {
    let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
    let root = json::parse(&text).expect("PROVENANCE.json must parse");
    let gaps = root.get("f32_f64_gap").expect("PROVENANCE.json has f32_f64_gap");
    for &(model, dt, centre, snapshots) in &TRACE_F32_GAPS {
        let key = format!("{model} {dt}");
        let g = gaps.get(&key).unwrap_or_else(|| panic!("no gap for {key}"));
        for (name, want) in [("centre", centre), ("snapshots", snapshots)] {
            let got = g.get(name).and_then(|v| v.as_f64()).unwrap_or_else(|| panic!("{key} {name}"));
            assert_eq!(got.to_bits(), want.to_bits(), "{key} {name}: PROVENANCE {got:e}, gate {want:e}");
        }
    }
}

fn snapshots_hold_the_centre_column<R: Precision>() {
    let Some(t) = traces::<R>() else { return };
    let f = t.f;
    let n = f.net.num_nodes();
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let centre_file = file(f, &format!("centre.{}.bin", R::SUFFIX));
    let snapshots_file = file(f, &format!("snapshots.{}.bin", R::SUFFIX));
    let mut checked = 0;
    for info in f.manifest.runs.iter().filter(|r| r.precision == R::DTYPE) {
        for q in t.sequences {
            let layout = q.frames(&info.dt).unwrap_or_else(|| panic!("sequence {} at dt {}", q.index, info.dt));
            let key = BlockKey { run: Some(info.index), dt: &info.dt, sequence: Some(q.index) };
            let c = R::read(centre_file, t.centre, &key).unwrap_or_else(|e| panic!("{e}"));
            let s = R::read(snapshots_file, t.snapshots, &key).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(c.shape, [layout.n_frames, centre.len()], "run {} sequence {}", info.index, q.index);
            assert_eq!(s.shape, [SNAPSHOTS, n], "run {} sequence {}", info.index, q.index);
            for (k, &frame) in layout.snapshot_frames.iter().enumerate() {
                for (j, &node) in centre.iter().enumerate() {
                    let (a, b) = (c.data[frame * centre.len() + j].to_f64(), s.data[k * n + node].to_f64());
                    assert_eq!(
                        a.to_bits(),
                        b.to_bits(),
                        "run {} sequence {} snapshot {k} (frame {frame}), node {node} ({}): centre {a:e}, snapshots {b:e}",
                        info.index, q.index, cell_type(&f.net, node)
                    );
                }
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 4 * t.sequences.len(), "four {} runs, every sequence", R::SUFFIX);
}

/// `centre[snapshot_frames]` is `snapshots[:, centre_nodes]` in flyvis's
/// own reference (fact H7). The gates index both by that assumption.
#[test]
fn snapshots_agree_with_the_centre_column() {
    snapshots_hold_the_centre_column::<f32>();
    snapshots_hold_the_centre_column::<f64>();
}

/// A negative control: run 3 (000, 0.02, f64), sequence 8 (the 90 degree
/// OFF edge, which is not point-symmetric), with every frame's hexals
/// mirrored (u, v) -> (-u, -v) before scattering. A wrong mapping must miss
/// by far more than any bound. (A full-field flash would not change.)
/// Measured during the spec's review: about 2.5.
#[test]
fn the_trace_gate_sees_a_wrong_mapping() {
    let Some(t) = traces::<f64>() else { return };
    let p = &play::<f64>(&t, 3, Counts::Torch, Start::Rust, &[8], true)[0];
    eprintln!("mirrored mapping, run 3, sequence 8: snapshots max {:e}, centre max {:e}", p.snapshots.max, p.centre.max);
    assert!(p.snapshots.max > 1e-3, "a mirrored mapping must miss by far more than any bound: snapshots max {:e}", p.snapshots.max);
}

/// R2's first diagnostic step works: restarting from flyvis's own settled
/// state, run 3, sequence 8 is still bit-equal (settling is exact, so the
/// two starts are the same state).
#[test]
fn a_run_from_flyviss_settled_state_is_bit_equal_too() {
    let Some(t) = traces::<f64>() else { return };
    let p = &play::<f64>(&t, 3, Counts::Torch, Start::Flyvis, &[8], false)[0];
    assert_eq!((p.centre.differ, p.snapshots.differ), (0, 0), "run 3, sequence 8, from settled.f64.bin");
}

/// Rust's own stimulus generators against the input flyvis fed (spec
/// section 4). Not part of the gates above, which use flyvis's input.
mod stimulus_checks {
    use super::*;
    use crate::moving_edge::{MovingEdge, MovingEdgeParams};
    use crate::moving_edge_checks::TOLERANCE;
    use crate::reference::Manifest;
    use crate::stimuli::{frame_count, FlashParams, Flashes, Stimulus};

    /// The manifest's text and `input.f64.bin`, or `None` after a notice.
    fn reference_input() -> Option<(String, &'static [u8])> {
        let text = or_skip(read_fixture_text("flyvis-reference/manifest.json"))?;
        let bytes = or_skip(trace_file("input.f64.bin"))?;
        Some((text, bytes))
    }

    /// Flashes at both dts, matched to the reference by intensity and
    /// radius, equal flyvis's input exactly: frame count, stimulus window
    /// and every value.
    #[test]
    fn rust_flashes_equal_flyviss_input() {
        let Some((text, bytes)) = reference_input() else { return };
        let m = Manifest::parse(&text).expect("manifest.json must parse");
        let sequences = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let input = m.file("input.f64.bin").expect("input.f64.bin is in the manifest");
        for dt in &m.dts {
            let params = FlashParams::flyvis_defaults(dt.dt);
            let n_pre = frame_count(params.t_pre, params.dt);
            let window = (n_pre, n_pre + frame_count(params.t_stim, params.dt));
            let flashes = Flashes::new(params);
            assert_eq!(flashes.num_sequences(), 4);
            let mut out = vec![0.0f32; HEXALS];
            for s in 0..flashes.num_sequences() {
                let (intensity, radius) = (flashes.intensity_of(s).unwrap(), flashes.radius_of(s).unwrap());
                let matching: Vec<&SequenceInfo> = sequences
                    .iter()
                    .filter(|q| {
                        q.family == "flashes"
                            && q.arg("intensity") == Some(f64::from(intensity))
                            && q.arg("radius") == Some(f64::from(radius))
                    })
                    .collect();
                assert_eq!(matching.len(), 1, "dt {}: Rust flash {s} (intensity {intensity}, radius {radius})", dt.id);
                let q = matching[0];
                let layout = q.frames(&dt.id).expect("every sequence has every dt");
                assert_eq!(flashes.num_frames(), layout.n_frames, "dt {}: sequence {}'s frame count", dt.id, q.index);
                assert_eq!(window, layout.stimulus_frames, "dt {}: sequence {}'s stimulus window", dt.id, q.index);
                let x = input
                    .read_f64(bytes, &BlockKey { run: None, dt: &dt.id, sequence: Some(q.index) })
                    .unwrap_or_else(|e| panic!("input.f64.bin: {e}"));
                assert_eq!(x.shape, [layout.n_frames, HEXALS]);
                for i in 0..layout.n_frames {
                    flashes.frame_into(s, i, &mut out).expect("a frame in range");
                    for (h, (&rust, &flyvis)) in out.iter().zip(&x.data[i * HEXALS..(i + 1) * HEXALS]).enumerate() {
                        assert!(
                            f64::from(rust) == flyvis,
                            "dt {}: sequence {} frame {i} hexal {h}: Rust {rust:e}, flyvis {flyvis:e}",
                            dt.id, q.index
                        );
                    }
                }
            }
            eprintln!("flashes at dt {}: all 4 sequences equal flyvis's input, every frame and hexal", dt.id);
        }
    }

    /// Moving edges at height 80, speed 13, angles 0/30/90/150, matched by
    /// angle, intensity and speed. Frame count and window are exact; every
    /// value is within the geometry gate's `TOLERANCE` (the two renderers
    /// are different functions); 0 degrees, where flyvis rotates nothing, is
    /// bit-equal to the f32 input flyvis's float32 runs fed.
    #[test]
    fn rust_moving_edges_match_flyviss_input() {
        let Some((text, bytes)) = reference_input() else { return };
        let m = Manifest::parse(&text).expect("manifest.json must parse");
        let sequences = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let root = json::parse(&text).expect("manifest.json must parse");
        let input = m.file("input.f64.bin").expect("input.f64.bin is in the manifest");
        let angles = [0, 30, 90, 150];
        for dt in &m.dts {
            let mut params = MovingEdgeParams::flyvis_defaults(dt.dt);
            params.height = 80;
            params.speeds = vec![13.0];
            params.angles = angles.to_vec();
            assert_eq!(params.offsets, (-10, 10), "the default offsets, inclusive");
            let edges = MovingEdge::new(params.clone());
            assert_eq!(edges.num_sequences(), 8);
            let mut out = vec![0.0f32; HEXALS];
            let mut worst = [0.0f64; 4];
            for s in 0..edges.num_sequences() {
                let (angle, intensity, speed) =
                    (edges.angle_of(s).unwrap(), edges.intensity_of(s).unwrap(), edges.speed_of(s).unwrap());
                let matching: Vec<&SequenceInfo> = sequences
                    .iter()
                    .filter(|q| {
                        q.family == "moving_edge"
                            && q.arg("angle") == Some(f64::from(angle))
                            && q.arg("intensity") == Some(f64::from(intensity))
                            && q.arg("speed") == Some(speed)
                    })
                    .collect();
                assert_eq!(matching.len(), 1, "dt {}: Rust edge {s} (angle {angle}, intensity {intensity})", dt.id);
                let q = matching[0];
                assert_eq!(
                    (q.arg("height"), q.arg("width")),
                    (Some(f64::from(params.height)), Some(f64::from(params.width))),
                    "sequence {}'s bar",
                    q.index
                );
                // flyvis's offsets are an exclusive range, [-10, 11); ours
                // are inclusive, (-10, 10)
                let offsets: Vec<f64> = root
                    .get("sequences")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(q.index as usize))
                    .and_then(|v| v.get("args"))
                    .and_then(|v| v.get("offsets"))
                    .and_then(|v| v.as_array())
                    .expect("the manifest gives each edge's offsets")
                    .iter()
                    .map(|v| v.as_f64().expect("a numeric offset"))
                    .collect();
                assert_eq!(
                    offsets,
                    [f64::from(params.offsets.0), f64::from(params.offsets.1 + 1)],
                    "sequence {}'s offsets",
                    q.index
                );
                let layout = q.frames(&dt.id).expect("every sequence has every dt");
                assert_eq!(
                    (edges.num_frames(), edges.valid_frames(s)),
                    (layout.n_frames, Some(layout.n_frames)),
                    "dt {}: sequence {}'s frame count",
                    dt.id, q.index
                );
                assert_eq!(edges.stimulus_frames(s), Some(layout.stimulus_frames), "dt {}: sequence {}'s window", dt.id, q.index);
                let x = input
                    .read_f64(bytes, &BlockKey { run: None, dt: &dt.id, sequence: Some(q.index) })
                    .unwrap_or_else(|e| panic!("input.f64.bin: {e}"));
                assert_eq!(x.shape, [layout.n_frames, HEXALS]);
                let a = angles.iter().position(|&g| g == angle).expect("one of the four angles");
                for i in 0..layout.n_frames {
                    edges.frame_into(s, i, &mut out).expect("a frame in range");
                    for (h, (&rust, &flyvis)) in out.iter().zip(&x.data[i * HEXALS..(i + 1) * HEXALS]).enumerate() {
                        let d = (f64::from(rust) - flyvis).abs();
                        assert!(
                            d <= TOLERANCE,
                            "dt {}: sequence {} (angle {angle}) frame {i} hexal {h}: Rust {rust:e}, flyvis {flyvis:e}, {d:e} apart",
                            dt.id, q.index
                        );
                        if angle == 0 {
                            assert_eq!(
                                rust.to_bits(),
                                (flyvis as f32).to_bits(),
                                "dt {}: 0 degree sequence {} frame {i} hexal {h}: Rust {rust:e}, flyvis's f32 input {:e}",
                                dt.id, q.index, flyvis as f32
                            );
                        }
                        worst[a] = worst[a].max(d);
                    }
                }
            }
            eprintln!(
                "moving edges at dt {}: max |Rust - flyvis| at 0/30/90/150 degrees: {:.3e} {:.3e} {:.3e} {:.3e} \
                 (TOLERANCE {TOLERANCE}); 0 degrees bit-equal to flyvis's f32 input",
                dt.id, worst[0], worst[1], worst[2], worst[3]
            );
        }
    }
}
