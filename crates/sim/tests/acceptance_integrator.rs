//! Acceptance tests for behaviour group `integrator` of epic graded-settling:
//! the graded integrator's dynamics and errors, driven through the public
//! `semblance_sim::graded` API only.
//!
//! One test per behaviour id (INTEGRATOR-01 .. INTEGRATOR-15). The weights and
//! reciprocals the integrator precomputes are private, so every claim about
//! them is checked through what `step` writes into `state()`, bit for bit,
//! against the same expression evaluated here in the order the behaviour
//! spells out. Rust never contracts `a * b + c` into a fused multiply-add, so
//! the expected values below are exactly the unfused result.

use std::process::Command;

use semblance_connectome::{TypedNetwork, TypedNetworkParts};
use semblance_sim::graded::{Graded, GradedError, GradedParams, Real};

/// One node per entry of `node_types`, all at (0, 0). `conns` are
/// (source type, target type, sign); `edges` are (source node, target node,
/// connection type). The network's own `edge_n_syn` is filled with a sentinel
/// the integrator must not use: counts come from `GradedParams`.
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
        edge_n_syn: vec![1000.0; edges.len()],
    })
    .expect("a consistent test network")
}

fn build<R: Real>(net: &TypedNetwork, bias: &[f32], tau: &[f32], strength: &[f32], counts: &[R], dt: f64) -> Graded<R> {
    let p = GradedParams { bias, time_const: tau, syn_strength: strength, edge_syn_count: counts };
    Graded::new(net, &p, dt).expect("valid parameters")
}

fn try_build<R: Real>(net: &TypedNetwork, bias: &[f32], tau: &[f32], strength: &[f32], counts: &[R], dt: f64) -> Result<Graded<R>, GradedError> {
    let p = GradedParams { bias, time_const: tau, syn_strength: strength, edge_syn_count: counts };
    Graded::new(net, &p, dt)
}

fn bits64(xs: &[f64]) -> Vec<u64> {
    xs.iter().map(|x| x.to_bits()).collect()
}

fn bits32(xs: &[f32]) -> Vec<u32> {
    xs.iter().map(|x| x.to_bits()).collect()
}

/// Two cell types, one connection type, three nodes, two edges.
fn small() -> TypedNetwork {
    network(&[0, 0, 1], 2, &[(0, 1, 1.0)], &[(0, 2, 0), (1, 2, 0)])
}

#[test]
fn integrator_01_slice_lengths_that_do_not_match_the_network_are_errors() {
    let net = small();
    let (b, t, s, c) = (vec![0.1f32; 2], vec![0.02f32; 2], vec![0.5f32; 1], vec![2.0f64; 2]);
    assert!(try_build(&net, &b, &t, &s, &c, 0.005).is_ok(), "the matching case must build");

    for (what, bias, tau, strength, counts) in [
        ("bias short", vec![0.1f32; 1], t.clone(), s.clone(), c.clone()),
        ("bias long", vec![0.1f32; 3], t.clone(), s.clone(), c.clone()),
        ("time_const short", b.clone(), vec![0.02f32; 1], s.clone(), c.clone()),
        ("time_const long", b.clone(), vec![0.02f32; 3], s.clone(), c.clone()),
        ("syn_strength short", b.clone(), t.clone(), vec![], c.clone()),
        ("syn_strength long", b.clone(), t.clone(), vec![0.5f32; 2], c.clone()),
        ("edge_syn_count short", b.clone(), t.clone(), s.clone(), vec![2.0f64; 1]),
        ("edge_syn_count long", b.clone(), t.clone(), s.clone(), vec![2.0f64; 3]),
        // Per node rather than per cell type: three values for two types.
        ("bias per node", vec![0.1f32; 3], t.clone(), s.clone(), c.clone()),
    ] {
        let r = try_build(&net, &bias, &tau, &strength, &counts, 0.005);
        assert!(r.is_err(), "{what}: expected a GradedError, got Ok");
        // Same in f32.
        let c32: Vec<f32> = counts.iter().map(|&x| x as f32).collect();
        assert!(try_build(&net, &bias, &tau, &strength, &c32, 0.005).is_err(), "{what} (f32): expected a GradedError");
    }
}

#[test]
fn integrator_02_non_finite_parameters_or_counts_are_errors() {
    let net = small();
    let (b, t, s, c) = (vec![0.1f32; 2], vec![0.02f32; 2], vec![0.5f32; 1], vec![2.0f64; 2]);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut bias = b.clone();
        bias[1] = bad;
        assert!(try_build(&net, &bias, &t, &s, &c, 0.005).is_err(), "bias {bad}");
        let mut tau = t.clone();
        tau[0] = bad;
        assert!(try_build(&net, &b, &tau, &s, &c, 0.005).is_err(), "time_const {bad}");
        let mut strength = s.clone();
        strength[0] = bad;
        assert!(try_build(&net, &b, &t, &strength, &c, 0.005).is_err(), "syn_strength {bad}");
        let mut c32 = vec![2.0f32; 2];
        c32[1] = bad;
        assert!(try_build(&net, &b, &t, &s, &c32, 0.005).is_err(), "edge_syn_count f32 {bad}");
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut counts = c.clone();
        counts[0] = bad;
        assert!(try_build(&net, &b, &t, &s, &counts, 0.005).is_err(), "edge_syn_count f64 {bad}");
    }
}

#[test]
fn integrator_03_a_dt_that_is_not_finite_and_positive_is_an_error() {
    let net = small();
    let (b, t, s) = (vec![0.1f32; 2], vec![0.02f32; 2], vec![0.5f32; 1]);
    for dt in [0.0, -0.0, -0.005, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(try_build(&net, &b, &t, &s, &[2.0f64; 2], dt).is_err(), "f64, dt {dt}");
        assert!(try_build(&net, &b, &t, &s, &[2.0f32; 2], dt).is_err(), "f32, dt {dt}");
    }
    assert!(try_build(&net, &b, &t, &s, &[2.0f64; 2], 0.005).is_ok());
}

#[test]
fn integrator_04_initial_and_reset_state_is_the_bias_and_dt_is_kept() {
    // Types interleaved across nodes so a per-node / per-type mixup shows.
    let net = network(&[1, 0, 2, 1], 3, &[(0, 1, 1.0)], &[(1, 0, 0)]);
    let bias = [0.1f32, -0.3, 0.7];
    let (tau, strength) = ([0.02f32; 3], [0.4f32]);
    let want64: Vec<f64> = [1usize, 0, 2, 1].iter().map(|&t| bias[t] as f64).collect();
    let want32: Vec<f32> = [1usize, 0, 2, 1].iter().map(|&t| bias[t]).collect();

    let mut g64 = build(&net, &bias, &tau, &strength, &[3.0f64], 0.005);
    assert_eq!(bits64(g64.state()), bits64(&want64), "initial f64 state");
    assert_eq!(g64.dt(), 0.005);
    for _ in 0..5 {
        g64.step(&[1.0, 2.0, -1.0, 0.5]).unwrap();
    }
    assert_ne!(bits64(g64.state()), bits64(&want64), "stepping must move the state");
    g64.reset();
    assert_eq!(bits64(g64.state()), bits64(&want64), "f64 state after reset");
    assert_eq!(g64.dt(), 0.005);

    let mut g32 = build(&net, &bias, &tau, &strength, &[3.0f32], 0.1);
    assert_eq!(bits32(g32.state()), bits32(&want32), "initial f32 state");
    assert_eq!(g32.dt(), 0.1, "dt() is the f64 it was built with, not f32(dt)");
    for _ in 0..5 {
        g32.step(&[1.0, 2.0, -1.0, 0.5]).unwrap();
    }
    g32.reset();
    assert_eq!(bits32(g32.state()), bits32(&want32), "f32 state after reset");
}

#[test]
fn integrator_05_a_wrong_length_input_is_an_error_and_changes_nothing() {
    let net = small();
    let mut g = build(&net, &[0.1, 0.2], &[0.02, 0.03], &[0.5], &[2.0f64, 3.0], 0.005);
    for _ in 0..3 {
        g.step(&[1.0, -2.0, 0.5]).unwrap();
    }
    let before = bits64(g.state());
    for input in [vec![], vec![1.0], vec![1.0, 2.0], vec![1.0, 2.0, 3.0, 4.0]] {
        assert!(g.step(&input).is_err(), "input of length {}", input.len());
        assert_eq!(bits64(g.state()), before, "state after a failed step of length {}", input.len());
    }
    // And the integrator still steps normally afterwards.
    g.step(&[1.0, -2.0, 0.5]).unwrap();
    assert_ne!(bits64(g.state()), before);
}

#[test]
fn integrator_06_one_step_follows_the_exact_order_of_operations() {
    // Node 3 (type 1) receives four edges, listed out of target order and
    // interleaved with edges to other nodes. Values are chosen so that a
    // different summation order gives a different f64 result.
    let net = network(
        &[0, 0, 0, 1, 0],
        2,
        &[(0, 1, 1.0), (0, 1, -1.0), (1, 0, 1.0)],
        &[(2, 3, 0), (3, 4, 2), (0, 3, 1), (1, 3, 0), (4, 3, 0)],
    );
    let bias = [0.3f32, 0.1];
    let tau = [0.02f32, 0.05];
    let strength = [0.7f32, 1.3, 0.9];
    let counts = [1.0e16f64, 2.5, 1.0e16, 3.0, 0.1];
    let dt = 0.005;
    let mut g = build(&net, &bias, &tau, &strength, &counts, dt);
    // Give the sources distinct positive states first.
    g.step(&[1.0, 2.0, 3.0, 0.0, 4.0]).unwrap();
    let v: Vec<f64> = g.state().to_vec();
    let input = [0.25, -0.5, 0.125, 0.75, 1.5];
    g.step(&input).unwrap();

    let relu = |x: f64| if x > 0.0 { x } else { 0.0 };
    let w = |sign: f32, count: f64, st: f32| (sign as f64) * count * (st as f64).max(0.0);
    let node_bias = [0.3f32 as f64, 0.3f32 as f64, 0.3f32 as f64, 0.1f32 as f64, 0.3f32 as f64];
    let node_inv_tau = [
        1.0 / (0.02f32 as f64),
        1.0 / (0.02f32 as f64),
        1.0 / (0.02f32 as f64),
        1.0 / (0.05f32 as f64),
        1.0 / (0.02f32 as f64),
    ];
    // Incoming edges per node, ascending edge index.
    let incoming: [&[(usize, f64)]; 5] = [
        &[],
        &[],
        &[],
        &[
            (2, w(1.0, 1.0e16, 0.7)),  // edge 0
            (0, w(-1.0, 1.0e16, 1.3)), // edge 2
            (1, w(1.0, 3.0, 0.7)),     // edge 3
            (4, w(1.0, 0.1, 0.7)),     // edge 4
        ],
        &[(3, w(1.0, 2.5, 0.9))], // edge 1
    ];
    let mut want = [0.0f64; 5];
    for i in 0..5 {
        let mut s = 0.0f64;
        for &(j, wj) in incoming[i] {
            s += wj * relu(v[j]);
        }
        let vel = node_inv_tau[i] * (((-v[i] + node_bias[i]) + s) + input[i]);
        want[i] = v[i] + vel * dt;
    }
    assert_eq!(bits64(g.state()), bits64(&want), "got {:?}, want {:?}", g.state(), want);

    // The order is observable: summing node 3's edges in reverse gives another value.
    let mut rev = 0.0f64;
    for &(j, wj) in incoming[3].iter().rev() {
        rev += wj * relu(v[j]);
    }
    let mut fwd = 0.0f64;
    for &(j, wj) in incoming[3] {
        fwd += wj * relu(v[j]);
    }
    assert_ne!(rev.to_bits(), fwd.to_bits(), "test values must make the summation order matter");
}

#[test]
fn integrator_07_mutual_connections_read_the_old_state() {
    // Two nodes, each exciting the other.
    let net = network(&[0, 1], 2, &[(0, 1, 1.0), (1, 0, 1.0)], &[(0, 1, 0), (1, 0, 1)]);
    let bias = [0.5f32, 0.25];
    let tau = [0.0625f32, 0.125];
    let strength = [2.0f32, 4.0];
    let counts = [3.0f64, 1.5];
    let dt = 0.015625; // 2^-6
    let mut g = build(&net, &bias, &tau, &strength, &counts, dt);
    g.step(&[1.0, -2.0]).unwrap();

    // By hand, from v = (0.5, 0.25); both weights are 6 (1 * 3 * 2 and 1 * 1.5 * 4),
    // node 0's rate is 1/0.0625 = 16 and node 1's is 1/0.125 = 8:
    // node 0: s = 6 * 0.25 = 1.5;  vel = 16 * ((-0.5 + 0.5) + 1.5 + 1)   = 40; v = 0.5 + 40/64  = 1.125
    // node 1: s = 6 * 0.5  = 3.0;  vel = 8  * ((-0.25 + 0.25) + 3.0 - 2) = 8;  v = 0.25 + 8/64  = 0.375
    // Had node 1 read node 0's new state (1.125), it would be 0.25 + 8 * (6.75 - 2) / 64 = 0.84375.
    assert_eq!(g.state(), &[1.125, 0.375]);

    // A second step, also by hand, again Jacobi-style from (1.125, 0.375):
    // node 0: s = 6 * 0.375 = 2.25;  vel = 16 * ((-1.125 + 0.5) + 2.25 + 0)  = 26; v = 1.125 + 26/64 = 1.53125
    // node 1: s = 6 * 1.125 = 6.75;  vel = 8  * ((-0.375 + 0.25) + 6.75 + 0) = 53; v = 0.375 + 53/64 = 1.203125
    g.step(&[0.0, 0.0]).unwrap();
    assert_eq!(g.state(), &[1.53125, 1.203125]);
}

#[test]
fn integrator_08_edge_weight_is_sign_times_count_times_clamped_strength() {
    // Source node 0 (type 0) drives target node 1 (type 1) through one edge of
    // an inhibitory connection type 1; connection type 0 is unused and has a
    // different strength, so taking the wrong type's parameters shows.
    let net = network(&[0, 1], 2, &[(0, 1, 1.0), (0, 1, -1.0)], &[(0, 1, 1)]);
    let bias = [0.7f32, 0.0];
    let tau = [0.02f32, 0.03];
    let strength = [5.0f32, 0.3];
    let dt = 0.005;
    // A count no f32 can hold: it must be used as given, not rounded through f32.
    let count = 2.7f64;
    let mut g = build(&net, &bias, &tau, &strength, &[count], dt);
    g.step(&[0.0, 0.0]).unwrap();

    let w = (-1.0f32 as f64) * count * (0.3f32 as f64).max(0.0);
    let s = 0.0 + w * (0.7f32 as f64);
    let want = 0.0 + ((1.0 / (0.03f32 as f64)) * (((-0.0 + 0.0) + s) + 0.0)) * dt;
    assert_eq!(g.state()[1].to_bits(), want.to_bits(), "got {}, want {want}", g.state()[1]);

    let rounded = (-1.0f32 as f64) * ((count as f32) as f64) * (0.3f32 as f64);
    let alt = ((1.0 / (0.03f32 as f64)) * (0.0 + rounded * (0.7f32 as f64))) * dt;
    assert_ne!(alt.to_bits(), want.to_bits(), "test values must make count rounding observable");

    // The same in f32.
    let mut g32 = build(&net, &bias, &tau, &strength, &[2.7f32], dt);
    g32.step(&[0.0, 0.0]).unwrap();
    let sign32 = -1.0f32;
    let w32 = sign32 * 2.7f32 * 0.3f32.max(0.0);
    let s32 = 0.0f32 + w32 * 0.7f32;
    let want32 = 0.0f32 + ((1.0f32 / 0.03f32) * (((-0.0f32 + 0.0) + s32) + 0.0)) * (dt as f32);
    assert_eq!(g32.state()[1].to_bits(), want32.to_bits(), "f32: got {}, want {want32}", g32.state()[1]);
}

#[test]
fn integrator_09_a_negative_strength_is_clamped_to_zero() {
    let with_edge = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
    let without = network(&[0, 1], 2, &[(0, 1, 1.0)], &[]);
    let bias = [0.9f32, 0.2];
    let tau = [0.02f32, 0.04];
    let input = [0.3, -0.6];
    for strength in [-0.5f32, -1.0e6] {
        let mut g = build(&with_edge, &bias, &tau, &[strength], &[7.0f64], 0.005);
        let mut h = build(&without, &bias, &tau, &[strength], &[] as &[f64], 0.005);
        let mut zero = build(&with_edge, &bias, &tau, &[0.0], &[7.0f64], 0.005);
        for k in 0..10 {
            g.step(&input).unwrap();
            h.step(&input).unwrap();
            zero.step(&input).unwrap();
            assert_eq!(bits64(g.state()), bits64(h.state()), "strength {strength}, step {k}: edge contributed");
            assert_eq!(bits64(g.state()), bits64(zero.state()), "strength {strength}, step {k}: not the same as 0");
        }
        // f32 too.
        let mut g32 = build(&with_edge, &bias, &tau, &[strength], &[7.0f32], 0.005);
        let mut h32 = build(&without, &bias, &tau, &[strength], &[] as &[f32], 0.005);
        for _ in 0..10 {
            g32.step(&[0.3, -0.6]).unwrap();
            h32.step(&[0.3, -0.6]).unwrap();
        }
        assert_eq!(bits32(g32.state()), bits32(h32.state()), "f32, strength {strength}");
    }
}

#[test]
fn integrator_10_a_negative_presynaptic_state_contributes_nothing() {
    // Node 0 (type 0) has a negative bias and stays negative; it drives node 1
    // (type 1) whose own bias is tried at both signs, through excitatory and
    // inhibitory connection types.
    for sign in [1.0f32, -1.0] {
        for target_bias in [-0.4f32, 0.4] {
            let with_edge = network(&[0, 1], 2, &[(0, 1, sign)], &[(0, 1, 0)]);
            let without = network(&[0, 1], 2, &[(0, 1, sign)], &[]);
            let bias = [-0.8f32, target_bias];
            let tau = [0.02f32, 0.03];
            let mut g = build(&with_edge, &bias, &tau, &[2.0], &[5.0f64], 0.005);
            let mut h = build(&without, &bias, &tau, &[2.0], &[] as &[f64], 0.005);
            for k in 0..20 {
                g.step(&[0.0, 0.0]).unwrap();
                h.step(&[0.0, 0.0]).unwrap();
                assert!(g.state()[0] < 0.0, "the source must stay negative");
                assert_eq!(
                    bits64(g.state()),
                    bits64(h.state()),
                    "sign {sign}, target bias {target_bias}, step {k}"
                );
            }
        }
    }
    // Control: the same edge from a positive source does contribute.
    let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
    let mut g = build(&net, &[0.8, -0.4], &[0.02, 0.03], &[2.0], &[5.0f64], 0.005);
    g.step(&[0.0, 0.0]).unwrap();
    assert_ne!(g.state()[1], -0.4f32 as f64);
}

#[test]
fn integrator_11_a_time_constant_above_dt_gives_rate_one_over_tau() {
    let net = network(&[0], 1, &[], &[]);
    let (b, tau, x, dt) = (0.25f32, 0.02f32, 3.0f64, 0.005f64);
    let mut g = build(&net, &[b], &[tau], &[], &[] as &[f64], dt);
    g.step(&[x]).unwrap();
    let v = b as f64;
    let want = v + ((1.0 / (tau as f64)) * (((-v + v) + 0.0) + x)) * dt;
    assert_eq!(g.state()[0].to_bits(), want.to_bits(), "f64: got {}, want {want}", g.state()[0]);

    let mut g32 = build(&net, &[b], &[tau], &[], &[] as &[f32], dt);
    g32.step(&[x as f32]).unwrap();
    let want32 = b + ((1.0f32 / tau) * (((-b + b) + 0.0) + x as f32)) * (dt as f32);
    assert_eq!(g32.state()[0].to_bits(), want32.to_bits(), "f32: got {}, want {want32}", g32.state()[0]);
}

#[test]
fn integrator_12_a_time_constant_at_or_below_dt_is_floored_at_f32_dt() {
    let net = network(&[0], 1, &[], &[]);
    let (b, x, dt) = (0.25f32, 3.0f64, 0.005f64);
    for tau in [0.001f32, 0.0, -0.5, 0.005] {
        // f64: the floor is f32(dt) widened, not dt itself.
        let mut g = build(&net, &[b], &[tau], &[], &[] as &[f64], dt);
        g.step(&[x]).unwrap();
        let v = b as f64;
        let floor = (dt as f32) as f64;
        let inv = 1.0 / (tau as f64).max(floor);
        let want = v + (inv * (((-v + v) + 0.0) + x)) * dt;
        assert_eq!(g.state()[0].to_bits(), want.to_bits(), "f64, tau {tau}: got {}, want {want}", g.state()[0]);
        if tau != 0.005 {
            let wrong = v + ((1.0 / dt) * (((-v + v) + 0.0) + x)) * dt;
            assert_ne!(wrong.to_bits(), want.to_bits(), "test values must tell f32(dt) from dt");
        }

        // f32.
        let mut g32 = build(&net, &[b], &[tau], &[], &[] as &[f32], dt);
        g32.step(&[x as f32]).unwrap();
        let inv32 = 1.0f32 / tau.max(dt as f32);
        let want32 = b + (inv32 * (((-b + b) + 0.0) + x as f32)) * (dt as f32);
        assert_eq!(g32.state()[0].to_bits(), want32.to_bits(), "f32, tau {tau}");
    }
}

#[test]
fn integrator_13_the_step_is_from_f64_dt_and_parameters_widen_exactly() {
    let net = network(&[0], 1, &[], &[]);
    let (b, tau, x, dt) = (0.1f32, 0.3f32, 0.7f64, 0.1f64);

    // f64: steps by dt itself; f32 parameters widened exactly.
    let mut g = build(&net, &[b], &[tau], &[], &[] as &[f64], dt);
    assert_eq!(g.state()[0].to_bits(), (b as f64).to_bits(), "bias widened exactly, not re-parsed");
    assert_ne!(g.state()[0], 0.1f64);
    g.step(&[x]).unwrap();
    let v = b as f64;
    let inv = 1.0 / (tau as f64);
    let want = v + (inv * (((-v + v) + 0.0) + x)) * dt;
    assert_eq!(g.state()[0].to_bits(), want.to_bits(), "f64 step: got {}, want {want}", g.state()[0]);
    let by_f32_dt = v + (inv * (((-v + v) + 0.0) + x)) * ((dt as f32) as f64);
    assert_ne!(by_f32_dt.to_bits(), want.to_bits(), "test values must tell dt from f32(dt)");

    // f32: steps by f32(dt).
    let mut g32 = build(&net, &[b], &[tau], &[], &[] as &[f32], dt);
    g32.step(&[x as f32]).unwrap();
    let want32 = b + ((1.0f32 / tau) * (((-b + b) + 0.0) + x as f32)) * (dt as f32);
    assert_eq!(g32.state()[0].to_bits(), want32.to_bits(), "f32 step: got {}, want {want32}", g32.state()[0]);
}

#[test]
fn integrator_14_a_lone_neuron_relaxes_toward_bias_plus_input() {
    let net = network(&[0], 1, &[], &[]);
    let (b, tau, x, dt) = (0.5f32, 0.05f32, 2.0f64, 0.005f64);
    // The closed form uses the time constant the integrator actually holds:
    // the f32 parameter, widened.
    let tau64 = tau as f64;
    let closed = |k: i32| (b as f64) + x * (1.0 - (1.0 - dt / tau64).powi(k));

    let mut g = build(&net, &[b], &[tau], &[], &[] as &[f64], dt);
    let mut g32 = build(&net, &[b], &[tau], &[], &[] as &[f32], dt);
    for k in 1..=2000 {
        g.step(&[x]).unwrap();
        g32.step(&[x as f32]).unwrap();
        let want = closed(k);
        let rel64 = ((g.state()[0] - want) / want).abs();
        assert!(rel64 <= 1e-12, "f64 step {k}: got {}, want {want}, relative {rel64:e}", g.state()[0]);
        let rel32 = ((g32.state()[0] as f64 - want) / want).abs();
        assert!(rel32 <= 1e-5, "f32 step {k}: got {}, want {want}, relative {rel32:e}", g32.state()[0]);
        if k == 1 {
            // A current, not a clamp: one step moves only part of the way.
            assert!(g.state()[0] < (b as f64) + x - 0.5, "one step must not set the state to bias + x");
        }
    }
    let target = (b as f64) + x;
    assert!((g.state()[0] - target).abs() < 1e-12, "f64 approaches bias + x: {}", g.state()[0]);
    assert!((g32.state()[0] as f64 - target).abs() < 1e-5, "f32 approaches bias + x: {}", g32.state()[0]);
}

#[test]
fn integrator_15_real_is_sealed_and_exactly_f32_and_f64() {
    // from_f32 exact, from_f64 an `as` cast, to_f64 exact.
    for x in [0.1f32, -3.3, 1.0e-40, f32::MAX, f32::MIN_POSITIVE, 0.0, -0.0] {
        assert_eq!(<f64 as Real>::from_f32(x).to_bits(), (x as f64).to_bits());
        assert_eq!(<f32 as Real>::from_f32(x).to_bits(), x.to_bits());
        assert_eq!(<f32 as Real>::to_f64(x).to_bits(), (x as f64).to_bits());
        assert_eq!((<f32 as Real>::to_f64(x) as f32).to_bits(), x.to_bits(), "to_f64 round-trips");
    }
    for x in [0.1f64, 0.005, -2.7, 1.0e300, 1.0e-320, 0.0] {
        assert_eq!(<f32 as Real>::from_f64(x).to_bits(), (x as f32).to_bits());
        assert_eq!(<f64 as Real>::from_f64(x).to_bits(), x.to_bits());
        assert_eq!(<f64 as Real>::to_f64(x).to_bits(), x.to_bits());
    }
    assert_eq!(<f32 as Real>::ZERO, 0.0f32);
    assert_eq!(<f32 as Real>::ONE, 1.0f32);
    assert_eq!(<f64 as Real>::ZERO, 0.0f64);
    assert_eq!(<f64 as Real>::ONE, 1.0f64);

    // Sealed: a downstream crate can use Real with f32 and f64 but cannot
    // implement it for its own type. Checked by compiling two tiny crates
    // against this one.
    let sim_dir = env!("CARGO_MANIFEST_DIR");
    let base = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("integrator_15_sealed");
    let check = |name: &str, body: &str| -> (bool, String) {
        let dir = base.join(name);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
                 [dependencies]\nsemblance-sim = {{ path = {sim_dir:?} }}\n\n[workspace]\n"
            ),
        )
        .unwrap();
        std::fs::write(dir.join("src/lib.rs"), body).unwrap();
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let out = Command::new(cargo)
            .arg("check")
            .arg("--offline")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(dir.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", base.join("target"))
            .env_remove("RUSTC_WRAPPER")
            .output()
            .expect("run cargo check");
        (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
    };

    let (ok, err) = check(
        "uses_real",
        "use semblance_sim::graded::Real;\n\
         pub fn one<R: Real>() -> R { R::ONE }\n\
         pub fn both() -> (f32, f64) { (one::<f32>(), one::<f64>()) }\n",
    );
    assert!(ok, "using Real with f32 and f64 downstream must compile:\n{err}");

    let (ok, err) = check(
        "impls_real",
        "use semblance_sim::graded::Real;\n\
         #[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]\n\
         pub struct Mine(f64);\n\
         impl std::ops::Add for Mine { type Output = Self; fn add(self, o: Self) -> Self { Mine(self.0 + o.0) } }\n\
         impl std::ops::Sub for Mine { type Output = Self; fn sub(self, o: Self) -> Self { Mine(self.0 - o.0) } }\n\
         impl std::ops::Mul for Mine { type Output = Self; fn mul(self, o: Self) -> Self { Mine(self.0 * o.0) } }\n\
         impl std::ops::Div for Mine { type Output = Self; fn div(self, o: Self) -> Self { Mine(self.0 / o.0) } }\n\
         impl std::ops::Neg for Mine { type Output = Self; fn neg(self) -> Self { Mine(-self.0) } }\n\
         impl Real for Mine {\n\
             const ZERO: Self = Mine(0.0);\n\
             const ONE: Self = Mine(1.0);\n\
             fn from_f32(x: f32) -> Self { Mine(x as f64) }\n\
             fn from_f64(x: f64) -> Self { Mine(x) }\n\
             fn to_f64(self) -> f64 { self.0 }\n\
             fn is_finite(self) -> bool { self.0.is_finite() }\n\
         }\n",
    );
    assert!(!ok, "implementing Real for a downstream type must not compile");
    assert!(err.contains("Sealed"), "the failure must be the seal, got:\n{err}");
}
