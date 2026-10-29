//! Acceptance tests for behaviour group `minors` of epic stimulus-traces:
//! the integrator fixes carried over from PR #12.
//!
//! One test per behaviour id that concerns `semblance_sim::graded`
//! (MINORS-06, -07, -08, -10, -11) or its acceptance file (MINORS-14). The
//! rest of the group is in `crates/flyvis/tests/acceptance_minors.rs`.
//!
//! Everything is driven through the public API. `relu` is private, so
//! MINORS-11 observes it through what `step` writes into `state()`: a
//! source node's value reaches its target only through `relu`.
//!
//! MINORS-14 runs `acceptance_integrator.rs`'s INTEGRATOR-15 through cargo,
//! in a target directory of its own under this test's `CARGO_TARGET_TMPDIR`,
//! after planting a stale file in the shadow source it builds and a marker in
//! the shadow's `target`.

use std::path::Path;
use std::process::Command;

use semblance_connectome::{TypedNetwork, TypedNetworkParts};
use semblance_sim::graded::{Graded, GradedError, GradedParams, Real};

/// One node per entry of `node_types`. `conns` are (source type, target
/// type, sign); `edges` are (source node, target node, connection type).
fn network(node_types: &[u16], n_types: usize, conns: &[(u16, u16, f32)], edges: &[(u32, u32, u16)]) -> TypedNetwork {
    TypedNetwork::new(TypedNetworkParts {
        cell_types: (0..n_types).map(|i| format!("T{i}")).collect(),
        node_cell_type: node_types.to_vec(),
        node_u: vec![0; node_types.len()],
        node_v: vec![0; node_types.len()],
        conn_types: conns.iter().map(|&(a, b, _)| (a, b)).collect(),
        conn_sign: conns.iter().map(|&(_, _, s)| s).collect(),
        edge_source: edges.iter().map(|&(s, _, _)| s).collect(),
        edge_target: edges.iter().map(|&(_, t, _)| t).collect(),
        edge_conn_type: edges.iter().map(|&(_, _, c)| c).collect(),
        edge_n_syn: vec![1.0; edges.len()],
    })
    .expect("a consistent test network")
}

/// Two cell types, node 0 (type 0) feeding node 1 (type 1) with weight 1.
fn pair() -> TypedNetwork {
    network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)])
}

fn try_new<R: Real>(net: &TypedNetwork, dt: f64) -> Result<Graded<R>, GradedError> {
    let counts = [R::ONE];
    let p = GradedParams { bias: &[0.0, 0.0], time_const: &[0.5, 0.5], syn_strength: &[1.0], edge_syn_count: &counts };
    Graded::new(net, &p, dt)
}

/// `Graded::new` refuses `dt` with `BadDt` carrying `dt` itself, bit for bit.
fn refused<R: Real>(dt: f64) -> bool {
    matches!(try_new::<R>(&pair(), dt), Err(GradedError::BadDt(got)) if got.to_bits() == dt.to_bits())
}

// ---------------------------------------------------------------------------
// MINORS-06
// ---------------------------------------------------------------------------

#[test]
fn minors_06_a_dt_that_is_subnormal_as_f32_is_refused_with_bad_dt() {
    let half = f32::MIN_POSITIVE as f64 / 2.0;
    let smallest = f64::from(f32::from_bits(1));
    // Both are positive and finite in f64, and both are subnormal as f32.
    for dt in [half, smallest, f32::MIN_POSITIVE as f64 * 0.75, f64::from(f32::from_bits(0x007f_ffff))] {
        assert!(dt.is_finite() && dt > 0.0, "{dt:e} is a positive finite f64");
        let dt32 = dt as f32;
        assert!(dt32 > 0.0 && !dt32.is_normal(), "{dt:e} is subnormal as f32 ({dt32:e})");
        assert!(refused::<f64>(dt), "f64 integrator must refuse dt {dt:e}");
        assert!(refused::<f32>(dt), "f32 integrator must refuse dt {dt:e}");
    }
    let Err(err) = try_new::<f64>(&pair(), half) else { panic!("refused") };
    assert!(err.to_string().contains("not a normal f32"), "the message names the f32 condition: {err}");
}

// ---------------------------------------------------------------------------
// MINORS-07
// ---------------------------------------------------------------------------

fn accepts_min_positive<R: Real>() {
    let dt = f32::MIN_POSITIVE as f64;
    let mut g = try_new::<R>(&pair(), dt).unwrap_or_else(|e| panic!("dt {dt:e} must be accepted: {e}"));
    assert_eq!(g.dt().to_bits(), dt.to_bits(), "the integrator keeps dt");
    // and it is a working integrator: one step from the bias stays finite
    g.step(&[R::ZERO, R::ZERO]).expect("one input per node");
    assert!(g.state().iter().all(|v| v.is_finite()), "{:?}", g.state());
}

#[test]
fn minors_07_dt_equal_to_the_smallest_normal_f32_is_accepted() {
    accepts_min_positive::<f32>();
    accepts_min_positive::<f64>();
}

// ---------------------------------------------------------------------------
// MINORS-08
// ---------------------------------------------------------------------------

#[test]
fn minors_08_zero_negative_and_non_finite_dt_are_still_refused() {
    let cases = [
        0.0,
        -0.0,
        -0.005,
        -1.0,
        // negative and normal as f32: only `dt > 0` refuses these
        -(f32::MIN_POSITIVE as f64),
        -(f32::MAX as f64),
        f64::NAN,
        -f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        // finite in f64 but zero or infinite as f32
        1e-300,
        1e300,
    ];
    for dt in cases {
        assert!(refused::<f64>(dt), "f64 integrator must refuse dt {dt:e}");
        assert!(refused::<f32>(dt), "f32 integrator must refuse dt {dt:e}");
    }
    // a usable dt still builds
    assert!(try_new::<f64>(&pair(), 0.005).is_ok());
    assert!(try_new::<f32>(&pair(), 0.005).is_ok());
}

// ---------------------------------------------------------------------------
// MINORS-10
// ---------------------------------------------------------------------------

fn nan_via_trait<R: Real>(x: R) -> bool {
    x.is_nan()
}

#[test]
fn minors_10_real_is_nan_reports_whether_a_value_is_nan() {
    for x in [f32::NAN, -f32::NAN, f32::from_bits(0x7fc0_0001), f32::from_bits(0x7f80_0001), f32::from_bits(0xffc0_0000)] {
        assert!(x.is_nan());
        assert!(<f32 as Real>::is_nan(x), "f32 {:#x}", x.to_bits());
        assert!(nan_via_trait(x), "f32 {:#x} through a generic", x.to_bits());
        assert!(<f64 as Real>::is_nan(f64::from(x)), "f64 from {:#x}", x.to_bits());
    }
    assert!(<f64 as Real>::is_nan(f64::from_bits(0x7ff0_0000_0000_0001)));
    for x in [0.0f32, -0.0, 1.0, -1.0, f32::MIN_POSITIVE, f32::from_bits(1), f32::MAX, f32::MIN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!<f32 as Real>::is_nan(x), "f32 {x:e}");
        assert!(!nan_via_trait(x), "f32 {x:e} through a generic");
        assert!(!<f64 as Real>::is_nan(f64::from(x)), "f64 {x:e}");
        assert!(!nan_via_trait(f64::from(x)), "f64 {x:e} through a generic");
    }
}

// ---------------------------------------------------------------------------
// MINORS-11
// ---------------------------------------------------------------------------
//
// Node 0 feeds node 1 with weight 1 (sign 1, count 1, strength 1). Both
// biases are 0, both time constants 0.5 (rate 2), dt 0.25. From node 1 at 0
// with no input, one step leaves node 1 at
//     0 + 2 * (((-0 + 0) + 1 * relu(x)) + 0) * 0.25  =  relu(x) / 2,
// exactly, where x is node 0's value before the step.

fn net_and_integrator<R: Real>() -> (TypedNetwork, Graded<R>) {
    let net = pair();
    let g = {
        let counts = [R::ONE];
        let p = GradedParams { bias: &[0.0, 0.0], time_const: &[0.5, 0.5], syn_strength: &[1.0], edge_syn_count: &counts };
        Graded::new(&net, &p, 0.25).expect("valid parameters")
    };
    (net, g)
}

/// Node 1 after one step from node 0 at the finite `x` (set with `set_state`).
fn target_after_finite<R: Real>(x: R) -> R {
    let (_net, mut g) = net_and_integrator::<R>();
    g.set_state(&[x, R::ZERO]).expect("a finite state");
    g.step(&[R::ZERO, R::ZERO]).unwrap();
    g.state()[1]
}

/// Node 1 after node 0 is driven to `x` by one step of input `x` from rest
/// (0 + 2 * x * 0.25 = x / 2, so x = inf, -inf or NaN stays itself), then
/// one more step without input.
fn target_after_driven<R: Real>(x: R) -> (R, R) {
    let (_net, mut g) = net_and_integrator::<R>();
    g.step(&[x, R::ZERO]).unwrap();
    let source = g.state()[0];
    g.step(&[R::ZERO, R::ZERO]).unwrap();
    (source, g.state()[1])
}

fn relu_through_step<R: Real>(name: &str) {
    let half = |x: f64| R::from_f64(x / 2.0);
    // positive inputs return themselves
    for x in [3.0, 0.5, 1.0e-30, 12345.0] {
        let got = target_after_finite(R::from_f64(x));
        assert_eq!(got.to_f64().to_bits(), half(x).to_f64().to_bits(), "{name}: relu({x:e}) must be {x:e}");
    }
    // other finite inputs return zero
    for x in [-2.0, -1.0e-30, 0.0, -0.0] {
        let got = target_after_finite(R::from_f64(x));
        assert_eq!(got.to_f64(), 0.0, "{name}: relu({x:e}) must be 0, node 1 got {got:?}");
    }
    // +inf returns itself
    let (src, got) = target_after_driven(R::from_f64(f64::INFINITY));
    assert_eq!(src.to_f64(), f64::INFINITY, "{name}: node 0 driven to +inf");
    assert_eq!(got.to_f64(), f64::INFINITY, "{name}: relu(+inf) must be +inf, node 1 got {got:?}");
    // -inf returns zero
    let (src, got) = target_after_driven(R::from_f64(f64::NEG_INFINITY));
    assert_eq!(src.to_f64(), f64::NEG_INFINITY, "{name}: node 0 driven to -inf");
    assert_eq!(got.to_f64(), 0.0, "{name}: relu(-inf) must be 0, node 1 got {got:?}");
    // NaN passes through, as torch.relu does
    let (src, got) = target_after_driven(R::from_f64(f64::NAN));
    assert!(src.is_nan(), "{name}: node 0 driven to NaN");
    assert!(got.is_nan(), "{name}: relu(NaN) must be NaN (torch.relu), node 1 got {got:?}");
}

#[test]
fn minors_11_relu_passes_nan_through_keeps_positives_and_zeroes_the_rest() {
    relu_through_step::<f32>("f32");
    relu_through_step::<f64>("f64");
}

// ---------------------------------------------------------------------------
// MINORS-14
// ---------------------------------------------------------------------------

#[test]
fn minors_14_integrator_15_clears_its_shadow_sources_and_keeps_its_target() {
    let nested = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-minors-14");
    // INTEGRATOR-15 builds under CARGO_TARGET_TMPDIR, which is `<target>/tmp`.
    let base = nested.join("tmp").join("integrator_15_sealed");
    let stale: Vec<_> = ["uses_real", "impls_real"]
        .iter()
        .map(|c| base.join(c).join("src").join("stale_left_by_an_earlier_run.rs"))
        .collect();
    for s in &stale {
        std::fs::create_dir_all(s.parent().unwrap()).unwrap();
        std::fs::write(s, "compile_error!(\"stale\");\n").unwrap();
    }
    let marker = base.join("target").join("kept-by-minors-14");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "keep").unwrap();

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let out = Command::new(cargo)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .env("CARGO_TARGET_DIR", &nested)
        .args(["test", "-p", "semblance-sim", "--test", "acceptance_integrator", "--locked", "--offline", "--"])
        .args(["--exact", "integrator_15_real_is_sealed_and_exactly_f32_and_f64", "--nocapture"])
        .output()
        .expect("cargo runs");
    let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(output.contains("running 1 test"), "INTEGRATOR-15 must run exactly once:\n{output}");
    assert!(out.status.success(), "INTEGRATOR-15 must pass:\n{output}");
    for s in &stale {
        assert!(!s.exists(), "{} survived: the copied source was not removed before copying", s.display());
    }
    for c in ["uses_real", "impls_real"] {
        assert!(base.join(c).join("src/lib.rs").exists(), "{c} was written afresh");
    }
    assert!(marker.exists(), "{} was removed: the shadow's target must be kept", marker.display());
}
