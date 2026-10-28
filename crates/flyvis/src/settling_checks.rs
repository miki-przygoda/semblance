//! The settling gates: epic 0.3.0's stage 1.
//!
//! flyvis settles its network from the bias state for 1.0 s of uniform grey
//! before every stimulus. These tests run `semblance_sim::graded` through
//! the same settling for every run in task 1's reference set -- models 000
//! and 049, dt 1/200 and 0.02, float32 and float64 -- and compare the
//! settled state (45,669 values) and the 65 centre neurons at every settling
//! step against flyvis's.
//!
//! Two gates over the same eight runs:
//!
//!   exact  synapse counts as flyvis's torch computed them
//!          (`syn_count_torch.rs`). Every value must equal flyvis's.
//!   libm   synapse counts from Rust's own `exp`. Every value must lie within
//!          a bound fixed in the spec before any comparison ran.
//!
//! Neither is to be relaxed. A failure is a discrepancy to find in flyvis's
//! source, recorded as an amendment to the spec:
//! docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 6.
//!
//! Each test prints its result (visible with `--nocapture`). Read the libm
//! ratios, do not just watch them pass. All skip without fixtures;
//! `SEMBLANCE_REQUIRE_FIXTURES=1` makes a skip a failure (see `CLAUDE.md`).

use std::sync::OnceLock;

use semblance_connectome::TypedNetwork;
use semblance_sim::graded::{Graded, GradedParams, Real};

use crate::ensemble::Ensemble;
use crate::json;
use crate::pretrained::{CELL_TYPES, SYN_COUNT};
use crate::reference::{Array, BlockKey, Dtype, FileInfo, Manifest, ReferenceError};
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use crate::test_support::{load_pretrained_fixture, load_reference_bytes, load_reference_fixture, real_spec_and_network};
use crate::wiring::{edge_syn_groups, input_nodes};

/// flyvis's own float32-vs-float64 gaps, PROVENANCE.json `f32_f64_gap`:
/// (model, dt, settled, settling_centre). The libm gate's f32 bound is twice
/// each (two f32 computations can sit on opposite sides of the exact value).
/// `pre_registered_bounds_match_provenance` pins these to the file.
const F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.4114366875261908e-06, 1.347661143569212e-06),
    ("049", "1/200", 6.1578304686449314e-06, 1.9363140433270587e-06),
    ("000", "1/50", 1.276739533651039e-06, 2.6109457484935206e-06),
    ("049", "1/50", 6.4440782598373936e-06, 1.1488733433040466e-05),
];

/// The libm gate's float64 bound, for both quantities.
const F64_BOUND: f64 = 1e-10;

const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

struct Fixture {
    net: TypedNetwork,
    groups: Vec<u32>,
    /// Every photoreceptor node, ascending.
    inputs: Vec<u32>,
    ensemble: Ensemble,
    manifest: Manifest,
    files: Vec<(&'static str, Vec<u8>)>,
}

static FIXTURE: OnceLock<Option<Fixture>> = OnceLock::new();

/// Everything the gates read, built once. Every file is loaded (or skipped)
/// through `test_support` before anything is built from it.
fn fixture() -> Option<&'static Fixture> {
    FIXTURE.get_or_init(load).as_ref()
}

fn load() -> Option<Fixture> {
    let ensemble = load_pretrained_fixture("ensemble.csv")?;
    let manifest = load_reference_fixture("manifest.json")?;
    let mut files = Vec::new();
    for name in SETTLING_FILES {
        files.push((name, load_reference_bytes(name)?));
    }
    let (spec, net) = real_spec_and_network()?;
    let groups = edge_syn_groups(&net).expect("2,355 synapse-count groups");
    let mut inputs: Vec<u32> = input_nodes(&spec, &net).expect("input units are cell types").into_iter().flatten().collect();
    inputs.sort_unstable();
    Some(Fixture {
        groups,
        inputs,
        ensemble: Ensemble::parse(&ensemble).expect("ensemble.csv must parse"),
        manifest: Manifest::parse(&manifest).expect("manifest.json must parse"),
        files,
        net,
    })
}

fn cell_type(net: &TypedNetwork, node: usize) -> &str {
    &net.cell_types()[net.node_cell_type()[node] as usize]
}

/// The two precisions, each with its count tables and reference reader.
trait Precision: Real {
    const DTYPE: Dtype;
    const SUFFIX: &'static str;
    fn torch_count(group: usize) -> Self;
    fn libm_count(group: usize) -> Self;
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>;
}

impl Precision for f32 {
    const DTYPE: Dtype = Dtype::F32;
    const SUFFIX: &'static str = "f32";
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F32[g] }
    fn libm_count(g: usize) -> Self { SYN_COUNT[g].exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f32(bytes, key)
    }
}

impl Precision for f64 {
    const DTYPE: Dtype = Dtype::F64;
    const SUFFIX: &'static str = "f64";
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F64[g] }
    fn libm_count(g: usize) -> Self { (SYN_COUNT[g] as f64).exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f64(bytes, key)
    }
}

/// Where the per-edge synapse counts come from.
#[derive(Clone, Copy)]
enum Counts {
    /// flyvis's own: torch's `exp`, from `syn_count_torch.rs`.
    Torch,
    /// Rust's `exp` of `SYN_COUNT`.
    Libm,
    /// The spec's integer counts, never exponentiated: a negative control.
    IntegerNSyn,
}

/// One settling run, Rust's and flyvis's values widened to f64.
struct Settled {
    rust: Vec<f64>,
    flyvis: Vec<f64>,
    rust_centre: Vec<f64>,
    flyvis_centre: Vec<f64>,
}

/// Settles `model`'s network at reference run `run`'s dt and precision, and
/// reads that run's reference.
fn settle<R: Precision>(f: &Fixture, run: u32, model: &str, counts: Counts) -> Settled {
    let info = &f.manifest.runs[run as usize];
    assert_eq!(info.index, run, "manifest runs must be listed in index order");
    assert_eq!(info.precision, R::DTYPE, "run {run}'s precision");
    let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("the run's dt is listed");
    let want_dt = match dt.id.as_str() {
        "1/200" => 1.0 / 200.0,
        "1/50" => 0.02,
        other => panic!("unexpected dt id {other}"),
    };
    assert_eq!(dt.dt, want_dt, "dt {}", dt.id);
    assert_eq!(dt.n_settle, (f.manifest.t_pre / dt.dt) as usize, "settling steps at dt {}", dt.id);

    let p = f.ensemble.model(model).expect("the model is in the ensemble");
    let count: Vec<R> = f
        .groups
        .iter()
        .enumerate()
        .map(|(e, &g)| match counts {
            Counts::Torch => R::torch_count(g as usize),
            Counts::Libm => R::libm_count(g as usize),
            Counts::IntegerNSyn => R::from_f32(f.net.edge_n_syn()[e]),
        })
        .collect();
    let params = GradedParams { bias: &p.bias, time_const: &p.time_const, syn_strength: &p.syn_strength, edge_syn_count: &count };
    let mut g = Graded::<R>::new(&f.net, &params, dt.dt).expect("the real network and parameters are valid");

    let mut input = vec![R::ZERO; f.net.num_nodes()];
    for &i in &f.inputs {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let mut rust_centre = Vec::with_capacity(dt.n_settle * centre.len());
    for step in 0..dt.n_settle {
        g.step(&input).expect("one input value per node");
        if let Some(i) = g.state().iter().position(|v| !v.is_finite()) {
            panic!("run {run}: node {i} ({}) is not finite after settle step {step}", cell_type(&f.net, i));
        }
        // settle step `step` is the state after step + 1 Euler steps
        rust_centre.extend(centre.iter().map(|&i| g.state()[i].to_f64()));
    }

    let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
    let settled = read::<R>(f, &format!("settled.{}.bin", R::SUFFIX), &key);
    assert_eq!(settled.shape, [f.net.num_nodes()], "settled shape");
    let flyvis_centre = read::<R>(f, &format!("settling_centre.{}.bin", R::SUFFIX), &key);
    assert_eq!(flyvis_centre.shape, [dt.n_settle, centre.len()], "settling_centre shape");
    Settled {
        rust: g.state().iter().map(|v| v.to_f64()).collect(),
        flyvis: settled.data.iter().map(|v| v.to_f64()).collect(),
        rust_centre,
        flyvis_centre: flyvis_centre.data.iter().map(|v| v.to_f64()).collect(),
    }
}

fn read<R: Precision>(f: &Fixture, name: &str, key: &BlockKey) -> Array<R> {
    let file = f.manifest.file(name).unwrap_or_else(|| panic!("{name} is not in the manifest"));
    let bytes = &f.files.iter().find(|(n, _)| *n == name).expect("loaded by fixture()").1;
    R::read(file, bytes, key).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The largest absolute error, where, and how many values are not equal.
struct Diff {
    max: f64,
    at: usize,
    differ: usize,
}

fn diff(rust: &[f64], flyvis: &[f64]) -> Diff {
    assert_eq!(rust.len(), flyvis.len());
    let mut d = Diff { max: 0.0, at: 0, differ: 0 };
    for (i, (a, b)) in rust.iter().zip(flyvis).enumerate() {
        let e = (a - b).abs();
        d.differ += usize::from(a != b);
        if e > d.max {
            d.max = e;
            d.at = i;
        }
    }
    d
}

fn exact<R: Precision>(run: u32) {
    let Some(f) = fixture() else { return };
    let info = &f.manifest.runs[run as usize];
    let s = settle::<R>(f, run, &info.model, Counts::Torch);
    let (settled, centre) = (diff(&s.rust, &s.flyvis), diff(&s.rust_centre, &s.flyvis_centre));
    let n_centre = f.manifest.centre_nodes.len();
    eprintln!(
        "exact run {run} ({}, dt {}, {}): settled {} of {} differ; settling centre {} of {} differ",
        info.model, info.dt, R::SUFFIX, settled.differ, s.rust.len(), centre.differ, s.rust_centre.len()
    );
    if settled.differ + centre.differ > 0 {
        let first = s
            .rust_centre
            .iter()
            .zip(&s.flyvis_centre)
            .position(|(a, b)| a != b)
            .map(|i| format!("settle step {} ({})", i / n_centre, f.manifest.centre_nodes[i % n_centre].cell_type));
        panic!(
            "run {run} is not bit-equal to flyvis: {} settled and {} settling-centre values differ; \
             first difference at {}; largest settled error {:e} at node {} ({})",
            settled.differ,
            centre.differ,
            first.unwrap_or_else(|| "the settled state only".into()),
            settled.max,
            settled.at,
            cell_type(&f.net, settled.at)
        );
    }
}

fn bounds(model: &str, dt: &str, precision: Dtype) -> (f64, f64) {
    match precision {
        Dtype::F64 => (F64_BOUND, F64_BOUND),
        Dtype::F32 => {
            let &(_, _, s, c) = F32_GAPS.iter().find(|g| g.0 == model && g.1 == dt).expect("a gap for every f32 run");
            (2.0 * s, 2.0 * c)
        }
    }
}

fn libm<R: Precision>(run: u32) {
    let Some(f) = fixture() else { return };
    let info = &f.manifest.runs[run as usize];
    let (bound_settled, bound_centre) = bounds(&info.model, &info.dt, info.precision);
    let s = settle::<R>(f, run, &info.model, Counts::Libm);
    let (settled, centre) = (diff(&s.rust, &s.flyvis), diff(&s.rust_centre, &s.flyvis_centre));
    let n_centre = f.manifest.centre_nodes.len();

    let mut per_type = vec![0.0f64; f.net.cell_types().len()];
    for (i, (a, b)) in s.rust.iter().zip(&s.flyvis).enumerate() {
        let t = f.net.node_cell_type()[i] as usize;
        per_type[t] = per_type[t].max((a - b).abs());
    }
    let mut worst: Vec<usize> = (0..per_type.len()).collect();
    worst.sort_by(|&a, &b| per_type[b].total_cmp(&per_type[a]));
    let worst: Vec<String> = worst[..3].iter().map(|&t| format!("{} {:.2e}", f.net.cell_types()[t], per_type[t])).collect();

    eprintln!(
        "libm run {run} ({}, dt {}, {}): settled max {:.3e} at node {} ({}), {:.3} of bound; \
         settling centre max {:.3e} at step {} ({}), {:.3} of bound; worst types {}; settled bit-equal {} of {}",
        info.model, info.dt, R::SUFFIX,
        settled.max, settled.at, cell_type(&f.net, settled.at), settled.max / bound_settled,
        centre.max, centre.at / n_centre, f.manifest.centre_nodes[centre.at % n_centre].cell_type, centre.max / bound_centre,
        worst.join(", "), s.rust.len() - settled.differ, s.rust.len()
    );
    assert!(
        settled.max <= bound_settled && centre.max <= bound_centre,
        "run {run}: Rust's exp exceeds the pre-registered bound: settled {:e} (bound {:e}), \
         settling centre {:e} (bound {:e})",
        settled.max, bound_settled, centre.max, bound_centre
    );
}

macro_rules! settling_runs {
    ($($exact:ident, $libm:ident, $run:expr, $t:ty;)*) => {
        $(
            #[test]
            fn $exact() { exact::<$t>($run) }
            #[test]
            fn $libm() { libm::<$t>($run) }
        )*
    };
}

settling_runs! {
    settling_exact_run_0_000_1_200_f32, settling_libm_run_0_000_1_200_f32, 0, f32;
    settling_exact_run_1_000_1_200_f64, settling_libm_run_1_000_1_200_f64, 1, f64;
    settling_exact_run_2_000_1_50_f32, settling_libm_run_2_000_1_50_f32, 2, f32;
    settling_exact_run_3_000_1_50_f64, settling_libm_run_3_000_1_50_f64, 3, f64;
    settling_exact_run_4_049_1_200_f32, settling_libm_run_4_049_1_200_f32, 4, f32;
    settling_exact_run_5_049_1_200_f64, settling_libm_run_5_049_1_200_f64, 5, f64;
    settling_exact_run_6_049_1_50_f32, settling_libm_run_6_049_1_50_f32, 6, f32;
    settling_exact_run_7_049_1_50_f64, settling_libm_run_7_049_1_50_f64, 7, f64;
}

/// The manifest's runs are exactly the eight the tests above are named for,
/// and its centre nodes are the expanded network's, one per cell type.
#[test]
fn reference_runs_are_the_eight_expected() {
    let Some(f) = fixture() else { return };
    let want = [
        (0u32, "000", "1/200", Dtype::F32), (1, "000", "1/200", Dtype::F64),
        (2, "000", "1/50", Dtype::F32), (3, "000", "1/50", Dtype::F64),
        (4, "049", "1/200", Dtype::F32), (5, "049", "1/200", Dtype::F64),
        (6, "049", "1/50", Dtype::F32), (7, "049", "1/50", Dtype::F64),
    ];
    let got: Vec<_> = f.manifest.runs.iter().map(|r| (r.index, r.model.as_str(), r.dt.as_str(), r.precision)).collect();
    assert_eq!(got, want);
    assert_eq!(f.manifest.centre_nodes.len(), CELL_TYPES.len());
    for (k, c) in f.manifest.centre_nodes.iter().enumerate() {
        assert_eq!(cell_type(&f.net, c.node as usize), c.cell_type, "centre node {k}");
        assert_eq!(c.cell_type, CELL_TYPES[k], "centre node {k} in CELL_TYPES order");
    }
}

/// The gaps above are PROVENANCE.json's, bit for bit.
#[test]
fn pre_registered_bounds_match_provenance() {
    let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
    let root = json::parse(&text).expect("PROVENANCE.json must parse");
    let gaps = root.get("f32_f64_gap").expect("PROVENANCE.json has f32_f64_gap");
    for &(model, dt, settled, centre) in &F32_GAPS {
        let key = format!("{model} {dt}");
        let g = gaps.get(&key).unwrap_or_else(|| panic!("no gap for {key}"));
        for (name, want) in [("settled", settled), ("settling_centre", centre)] {
            let got = g.get(name).and_then(|v| v.as_f64()).unwrap_or_else(|| panic!("{key} {name}"));
            assert_eq!(got.to_bits(), want.to_bits(), "{key} {name}: PROVENANCE {got:e}, gate {want:e}");
        }
    }
}

/// A control: the wrong model's parameters, against run 3's reference
/// (model 000, dt 0.02, f64), must miss by far more than any bound. A gate
/// that compared nothing to nothing would pass this.
#[test]
fn the_gate_tells_models_apart() {
    let Some(f) = fixture() else { return };
    let s = settle::<f64>(f, 3, "049", Counts::Torch);
    let d = diff(&s.rust, &s.flyvis);
    assert!(d.max > 1e-3, "model 049 against model 000's reference: max {:e}", d.max);
}

/// A control at the scale the libm bound exists to resolve: the spec's
/// integer counts in place of exp(ln count) must exceed the f64 bound.
/// Measured 4.8e-7 to 6.7e-6 across runs during the spec's review.
#[test]
fn the_gate_resolves_integer_counts() {
    let Some(f) = fixture() else { return };
    let s = settle::<f64>(f, 3, "000", Counts::IntegerNSyn);
    let d = diff(&s.rust, &s.flyvis);
    assert!(d.max > F64_BOUND, "integer counts: settled max {:e}", d.max);
}
