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
//!
//! What the gates compute -- settling, the comparison, the bounds -- lives
//! in `crate::parity`, which `semblance validate` runs too. These tests are
//! thin wrappers over it, and keep their names and their printed strings.

use crate::gate_support::fixture;
use crate::json;
use crate::parity::{self, cell_type, settle, settling_bounds, Counts, Precision, CONTROL_MISS, F64_BOUND};
use crate::pretrained::CELL_TYPES;
use crate::reference::Dtype;
use crate::test_support::load_reference_fixture;

// The registered gaps, `parity::SETTLING_F32_GAPS`: (model, dt, settled,
// settling_centre). `pre_registered_bounds_match_provenance` pins them to
// the file.
use crate::parity::SETTLING_F32_GAPS as F32_GAPS;

/// The test's name says which precision it runs; the registry must agree.
fn assert_registered<R: Precision>(run: u32) {
    assert_eq!(R::DTYPE, parity::RUNS[run as usize].3, "run {run}'s registered precision");
}

fn exact<R: Precision>(run: u32) {
    assert_registered::<R>(run);
    let Some(f) = fixture() else { return };
    let info = &f.manifest.runs[run as usize];
    let s = settle(f, run, &info.model, Counts::Torch).unwrap_or_else(|e| panic!("{e}"));
    let (settled, centre) = (s.settled, s.settling_centre);
    let n_centre = f.manifest.centre_nodes.len();
    eprintln!(
        "exact run {run} ({}, dt {}, {}): settled {} of {} differ; settling centre {} of {} differ",
        info.model, info.dt, R::SUFFIX, settled.differ, s.settled_len, centre.differ, s.centre_len
    );
    if settled.differ + centre.differ > 0 {
        let first = s
            .first_centre_difference
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

fn libm<R: Precision>(run: u32) {
    assert_registered::<R>(run);
    let Some(f) = fixture() else { return };
    let info = &f.manifest.runs[run as usize];
    let (bound_settled, bound_centre) =
        settling_bounds(&info.model, &info.dt, info.precision, Counts::Libm).unwrap_or_else(|e| panic!("{e}"));
    let (bound_settled, bound_centre) = (bound_settled.value(), bound_centre.value());
    let s = settle(f, run, &info.model, Counts::Libm).unwrap_or_else(|e| panic!("{e}"));
    let (settled, centre) = (s.settled, s.settling_centre);
    let n_centre = f.manifest.centre_nodes.len();
    let worst: Vec<String> =
        s.worst_types(3).iter().map(|&t| format!("{} {:.2e}", f.net.cell_types()[t], s.per_type_max[t])).collect();

    eprintln!(
        "libm run {run} ({}, dt {}, {}): settled max {:.3e} at node {} ({}), {:.3e} of bound; \
         settling centre max {:.3e} at step {} ({}), {:.3e} of bound; worst types {}; settled bit-equal {} of {}",
        info.model, info.dt, R::SUFFIX,
        settled.max, settled.at, cell_type(&f.net, settled.at), settled.max / bound_settled,
        centre.max, centre.at / n_centre, f.manifest.centre_nodes[centre.at % n_centre].cell_type, centre.max / bound_centre,
        worst.join(", "), s.settled_len - settled.differ, s.settled_len
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
    let d = settle(f, 3, "049", Counts::Torch).unwrap_or_else(|e| panic!("{e}")).settled;
    assert!(d.max > CONTROL_MISS, "model 049 against model 000's reference: max {:e}", d.max);
}

/// A control at the scale the libm bound exists to resolve: the spec's
/// integer counts in place of exp(ln count) must exceed the f64 bound.
/// Measured 4.8e-7 to 6.7e-6 across runs during the spec's review.
///
/// It resolves them against the f64 bound only. The f32 libm bound does not
/// resolve integer counts in 3 of the 4 f32 runs; in f32 the exact gate is
/// the guard (the graded-settling spec's 2026-10-29 amendment).
#[test]
fn the_gate_resolves_integer_counts() {
    let Some(f) = fixture() else { return };
    let d = settle(f, 3, "000", Counts::IntegerNSyn).unwrap_or_else(|e| panic!("{e}")).settled;
    assert!(d.max > F64_BOUND, "integer counts: settled max {:e}", d.max);
}
