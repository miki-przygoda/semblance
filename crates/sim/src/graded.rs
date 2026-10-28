//! Graded (non-spiking) dynamics: flyvis's passive point neurons with
//! instantaneous graded-release synapses, integrated by explicit Euler.
//!
//! ```text
//! dV_i/dt = 1/max(tau_i, f32(dt)) * (-V_i + bias_i + sum_e w_e relu(V_src(e)) + x_i)
//! w_e     = sign * syn_count * max(syn_strength, 0)
//! ```
//!
//! A new path beside the spiking one, not an extension of it: none of this
//! crate's spiking traits is involved. It follows flyvis 1.2.0 operation for
//! operation -- the association of every sum, the reciprocal-then-multiply,
//! the time-constant floor at f32(dt) in both precisions, each node's inputs
//! summed in ascending edge order from zero -- because `semblance-flyvis`'s
//! settling gate holds it to bit-equality with flyvis. Do not reorder an
//! expression here, and do not use `mul_add`.
//!
//! Synapse counts arrive already in the run's precision. Where they come from
//! (flyvis's own `exp`, or Rust's) is the caller's choice.
//!
//! Spec: docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 4.

use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Neg, Sub};

use semblance_connectome::TypedNetwork;

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// The precision a run computes in: f32 (flyvis's float32 run) or f64 (its
/// float64 run). Sealed.
pub trait Real:
    Copy
    + PartialOrd
    + Debug
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + sealed::Sealed
{
    const ZERO: Self;
    const ONE: Self;
    /// Exact.
    fn from_f32(x: f32) -> Self;
    /// An `as` cast: rounds to nearest for f32.
    fn from_f64(x: f64) -> Self;
    /// Exact.
    fn to_f64(self) -> f64;
    fn is_finite(self) -> bool;
}

impl Real for f32 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    fn from_f32(x: f32) -> Self { x }
    fn from_f64(x: f64) -> Self { x as f32 }
    fn to_f64(self) -> f64 { self as f64 }
    fn is_finite(self) -> bool { f32::is_finite(self) }
}

impl Real for f64 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    fn from_f32(x: f32) -> Self { x as f64 }
    fn from_f64(x: f64) -> Self { x }
    fn to_f64(self) -> f64 { self }
    fn is_finite(self) -> bool { f64::is_finite(self) }
}

/// A model's parameters, as flyvis holds them: f32, one per cell type or
/// connection type, plus one synapse count per edge in the run's precision.
#[derive(Clone, Copy, Debug)]
pub struct GradedParams<'a, R: Real> {
    pub bias: &'a [f32],
    pub time_const: &'a [f32],
    pub syn_strength: &'a [f32],
    pub edge_syn_count: &'a [R],
}

#[derive(Clone, Debug, PartialEq)]
pub enum GradedError {
    /// A slice does not match the network.
    Length { what: &'static str, expected: usize, found: usize },
    /// A parameter or count is NaN or infinite.
    NonFinite { what: &'static str, index: usize },
    /// `dt` is not finite and positive, in f64 or as f32.
    BadDt(f64),
}

impl std::fmt::Display for GradedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GradedError::Length { what, expected, found } =>
                write!(f, "{what} has {found} values, expected {expected}"),
            GradedError::NonFinite { what, index } => write!(f, "{what}[{index}] is not finite"),
            GradedError::BadDt(dt) => write!(f, "dt {dt} is not finite and positive"),
        }
    }
}

impl std::error::Error for GradedError {}

/// flyvis's network, precomputed for one model, one dt, one precision.
#[derive(Clone, Debug)]
pub struct Graded<R: Real> {
    dt: f64,
    step_dt: R,
    bias: Vec<R>,
    inv_tau: Vec<R>,
    /// Node i's incoming edges are `offsets[i]..offsets[i + 1]` of `src` and
    /// `weight`, in ascending edge index.
    offsets: Vec<usize>,
    src: Vec<u32>,
    weight: Vec<R>,
    state: Vec<R>,
    next: Vec<R>,
}

fn max<R: Real>(a: R, b: R) -> R {
    if a > b { a } else { b }
}

fn relu<R: Real>(x: R) -> R {
    if x > R::ZERO { x } else { R::ZERO }
}

impl<R: Real> Graded<R> {
    pub fn new(net: &TypedNetwork, params: &GradedParams<R>, dt: f64) -> Result<Self, GradedError> {
        // The floor and the f32 step use f32(dt), so that must be finite and
        // positive too: a dt that rounds to 0 or overflows in f32 would make
        // inv_tau 1/0 or 1/inf.
        let dt32 = dt as f32;
        if !(dt.is_finite() && dt > 0.0 && dt32.is_finite() && dt32 > 0.0) {
            return Err(GradedError::BadDt(dt));
        }
        let n_types = net.cell_types().len();
        for (what, len, expected) in [
            ("bias", params.bias.len(), n_types),
            ("time_const", params.time_const.len(), n_types),
            ("syn_strength", params.syn_strength.len(), net.conn_types().len()),
            ("edge_syn_count", params.edge_syn_count.len(), net.num_edges()),
        ] {
            if len != expected {
                return Err(GradedError::Length { what, expected, found: len });
            }
        }
        for (what, values) in [
            ("bias", params.bias),
            ("time_const", params.time_const),
            ("syn_strength", params.syn_strength),
        ] {
            if let Some(index) = values.iter().position(|v| !v.is_finite()) {
                return Err(GradedError::NonFinite { what, index });
            }
        }
        if let Some(index) = params.edge_syn_count.iter().position(|v| !v.is_finite()) {
            return Err(GradedError::NonFinite { what: "edge_syn_count", index });
        }

        // flyvis floors at torch.tensor(dt).float() in both precisions
        // (dynamics.py:209), and takes the reciprocal before multiplying.
        let floor = R::from_f32(dt32);
        let type_inv_tau: Vec<R> =
            params.time_const.iter().map(|&t| R::ONE / max(R::from_f32(t), floor)).collect();
        let bias: Vec<R> = net.node_cell_type().iter().map(|&t| R::from_f32(params.bias[t as usize])).collect();
        let inv_tau: Vec<R> = net.node_cell_type().iter().map(|&t| type_inv_tau[t as usize]).collect();

        // By target, keeping ascending edge order within each node.
        let n = net.num_nodes();
        let mut offsets = vec![0usize; n + 1];
        for &t in net.edge_target() {
            offsets[t as usize + 1] += 1;
        }
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut cursor = offsets.clone();
        let mut src = vec![0u32; net.num_edges()];
        let mut weight = vec![R::ZERO; net.num_edges()];
        for e in 0..net.num_edges() {
            let c = net.edge_conn_type()[e] as usize;
            // sign * syn_count * syn_strength, left to right (dynamics.py:165-167),
            // with syn_strength clamped at zero first (network.py:489-498).
            let w = R::from_f32(net.conn_sign()[c])
                * params.edge_syn_count[e]
                * max(R::from_f32(params.syn_strength[c]), R::ZERO);
            let t = net.edge_target()[e] as usize;
            src[cursor[t]] = net.edge_source()[e];
            weight[cursor[t]] = w;
            cursor[t] += 1;
        }

        Ok(Graded {
            dt,
            step_dt: R::from_f64(dt),
            state: bias.clone(),
            next: vec![R::ZERO; n],
            bias,
            inv_tau,
            offsets,
            src,
            weight,
        })
    }

    /// One explicit Euler step with `input` (one value per node) added to
    /// every node's velocity. A failed step changes nothing.
    pub fn step(&mut self, input: &[R]) -> Result<(), GradedError> {
        let n = self.state.len();
        if input.len() != n {
            return Err(GradedError::Length { what: "input", expected: n, found: input.len() });
        }
        let Graded { step_dt, bias, inv_tau, offsets, src, weight, state, next, .. } = self;
        for (i, out) in next.iter_mut().enumerate() {
            let (lo, hi) = (offsets[i], offsets[i + 1]);
            let mut s = R::ZERO;
            for (&j, &w) in src[lo..hi].iter().zip(&weight[lo..hi]) {
                s = s + w * relu(state[j as usize]);
            }
            let v = state[i];
            // ((-v + bias) + sum) + x, then times 1/tau, then v + vel * dt
            // (dynamics.py:207-218, network.py:406).
            let vel = inv_tau[i] * (((-v + bias[i]) + s) + input[i]);
            *out = v + vel * *step_dt;
        }
        std::mem::swap(state, next);
        Ok(())
    }

    /// Back to the initial state: every node at its bias.
    pub fn reset(&mut self) {
        self.state.copy_from_slice(&self.bias);
    }

    pub fn state(&self) -> &[R] {
        &self.state
    }

    pub fn dt(&self) -> f64 {
        self.dt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semblance_connectome::TypedNetworkParts;

    /// One node per entry of `node_types`, all at (0, 0): position plays no
    /// part in the dynamics. `conns` are (source type, target type, sign);
    /// `edges` are (source node, target node, connection type).
    fn network(node_types: &[u16], n_types: usize, conns: &[(u16, u16, f32)], edges: &[(u32, u32, u16)]) -> TypedNetwork {
        TypedNetwork::new(TypedNetworkParts {
            cell_types: (0..n_types).map(|i| format!("T{i}")).collect(),
            node_cell_type: node_types.to_vec(),
            node_u: vec![0; node_types.len()],
            node_v: vec![0; node_types.len()],
            conn_types: conns.iter().map(|&(a, b, _)| (a, b)).collect(),
            conn_sign: conns.iter().map(|&(_, _, s)| s).collect(),
            edge_source: edges.iter().map(|e| e.0).collect(),
            edge_target: edges.iter().map(|e| e.1).collect(),
            edge_conn_type: edges.iter().map(|e| e.2).collect(),
            edge_n_syn: vec![1.0; edges.len()],
        })
        .expect("test network is consistent")
    }

    fn graded<R: Real>(net: &TypedNetwork, bias: &[f32], tau: &[f32], strength: &[f32], counts: &[R], dt: f64) -> Graded<R> {
        let p = GradedParams { bias, time_const: tau, syn_strength: strength, edge_syn_count: counts };
        Graded::new(net, &p, dt).expect("valid parameters")
    }

    fn r<R: Real>(x: f32) -> R {
        R::from_f32(x)
    }

    /// A lone neuron starting at bias under constant input x approaches
    /// bias + x as bias + x(1 - (1 - dt/tau)^k). A round-off comparison
    /// against a formula, not a bound on the port.
    fn closed_form<R: Real>(tol: f64) {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<R>(&net, &[0.3], &[0.05], &[], &[], 0.01);
        for _ in 0..100 {
            g.step(&[r::<R>(0.2)]).unwrap();
        }
        let rate = R::from_f64(0.01).to_f64() / (0.05f32 as f64);
        let want = 0.3f32 as f64 + 0.2f32 as f64 * (1.0 - (1.0 - rate).powi(100));
        let got = g.state()[0].to_f64();
        assert!(((got - want) / want).abs() < tol, "{got} vs {want}");
    }

    #[test]
    fn a_lone_neuron_follows_the_closed_form_f64() { closed_form::<f64>(1e-12) }
    #[test]
    fn a_lone_neuron_follows_the_closed_form_f32() { closed_form::<f32>(1e-5) }

    fn relu_reads_the_source<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let zero = [R::ZERO, R::ZERO];
        // A negative source contributes nothing: the target stays at its bias.
        let mut g = graded::<R>(&net, &[-0.5, -0.2], &[0.1, 0.1], &[1.0], &[R::ONE], 0.01);
        g.step(&zero).unwrap();
        assert_eq!(g.state()[1], r::<R>(-0.2));
        // A positive source drives a target whose own state is negative.
        let mut g = graded::<R>(&net, &[0.5, -0.2], &[0.1, 0.1], &[1.0], &[R::ONE], 0.01);
        g.step(&zero).unwrap();
        let b = r::<R>(-0.2);
        let s = R::ZERO + R::ONE * r::<R>(0.5);
        let want = b + ((R::ONE / r::<R>(0.1)) * (((-b + b) + s) + R::ZERO)) * R::from_f64(0.01);
        assert_eq!(g.state()[1], want);
    }

    #[test]
    fn relu_reads_the_source_f32() { relu_reads_the_source::<f32>() }
    #[test]
    fn relu_reads_the_source_f64() { relu_reads_the_source::<f64>() }

    fn negative_strength_is_clamped<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let mut g = graded::<R>(&net, &[0.5, 0.25], &[0.1, 0.1], &[-2.0], &[R::ONE], 0.01);
        g.step(&[R::ZERO, R::ZERO]).unwrap();
        assert_eq!(g.state()[1], r::<R>(0.25), "a clamped weight leaves the target at rest");
    }

    #[test]
    fn negative_strength_is_clamped_f32() { negative_strength_is_clamped::<f32>() }
    #[test]
    fn negative_strength_is_clamped_f64() { negative_strength_is_clamped::<f64>() }

    #[test]
    fn the_floor_is_f32_dt_widened_in_f64() {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<f64>(&net, &[0.0], &[0.001], &[], &[], 0.005);
        g.step(&[1.0]).unwrap();
        let floor = 0.005f32 as f64; // 0.004999999888241291
        let want = 0.0 + ((1.0 / floor) * (((-0.0 + 0.0) + 0.0) + 1.0)) * 0.005;
        assert_eq!(g.state()[0], want);
        assert_ne!(g.state()[0], 1.0, "a floor at the f64 dt would give exactly 1");
    }

    #[test]
    fn above_the_floor_the_rate_is_one_over_tau() {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<f64>(&net, &[0.0], &[0.02], &[], &[], 0.005);
        g.step(&[1.0]).unwrap();
        assert_eq!(g.state()[0], ((1.0 / (0.02f32 as f64)) * 1.0) * 0.005);
    }

    #[test]
    fn the_floor_in_f32_is_f32_dt() {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<f32>(&net, &[0.0], &[0.001], &[], &[], 0.02);
        g.step(&[1.0]).unwrap();
        assert_eq!(g.state()[0], ((1.0f32 / 0.02f32) * 1.0) * 0.02f32);
    }

    #[test]
    fn input_is_a_current_not_a_clamp() {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<f64>(&net, &[0.0], &[0.1], &[], &[], 0.01);
        g.step(&[1.0]).unwrap();
        assert_eq!(g.state()[0], ((1.0 / (0.1f32 as f64)) * 1.0) * 0.01);
        assert!(g.state()[0] < 0.2, "one step moves the state a fraction of the way");
    }

    fn reads_the_old_state<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0), (1, 0, 1.0)], &[(0, 1, 0), (1, 0, 1)]);
        let mut g = graded::<R>(&net, &[1.0, 2.0], &[0.1, 0.1], &[1.0, 1.0], &[R::ONE, R::ONE], 0.01);
        g.step(&[R::ZERO, R::ZERO]).unwrap();
        let at = |b: f32, source: f32| {
            let b = r::<R>(b);
            let s = R::ZERO + R::ONE * r::<R>(source);
            b + ((R::ONE / r::<R>(0.1)) * (((-b + b) + s) + R::ZERO)) * R::from_f64(0.01)
        };
        assert_eq!(g.state()[0], at(1.0, 2.0));
        assert_eq!(g.state()[1], at(2.0, 1.0), "node 1 must see node 0's old state, 1.0");
    }

    #[test]
    fn every_node_reads_the_old_state_f32() { reads_the_old_state::<f32>() }
    #[test]
    fn every_node_reads_the_old_state_f64() { reads_the_old_state::<f64>() }

    fn weight_is_sign_times_count_times_strength<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, -1.0)], &[(0, 1, 0)]);
        let mut g = graded::<R>(&net, &[1.0, 0.0], &[0.1, 0.1], &[0.5], &[r::<R>(3.0)], 0.01);
        g.step(&[R::ZERO, R::ZERO]).unwrap();
        let w = r::<R>(-1.0) * r::<R>(3.0) * r::<R>(0.5);
        let s = R::ZERO + w * R::ONE;
        let want = R::ZERO + ((R::ONE / r::<R>(0.1)) * (((-R::ZERO + R::ZERO) + s) + R::ZERO)) * R::from_f64(0.01);
        assert_eq!(g.state()[1], want);
    }

    #[test]
    fn weight_is_sign_times_count_times_strength_f32() { weight_is_sign_times_count_times_strength::<f32>() }
    #[test]
    fn weight_is_sign_times_count_times_strength_f64() { weight_is_sign_times_count_times_strength::<f64>() }

    /// Node 3's inputs, in edge order: +1e8, then -1e8, then +1. In f32,
    /// (1e8 - 1e8) + 1 = 1, but (1e8 + 1) - 1e8 = 0. An edge into node 4
    /// sits between them, so a by-target index must still keep edge order.
    #[test]
    fn sums_each_nodes_inputs_in_ascending_edge_order() {
        let net = network(
            &[0, 1, 2, 3, 4],
            5,
            &[(0, 3, 1.0), (1, 3, -1.0), (2, 3, 1.0), (0, 4, 1.0)],
            &[(0, 3, 0), (0, 4, 3), (1, 3, 1), (2, 3, 2)],
        );
        let mut g = graded::<f32>(&net, &[1.0, 1.0, 1.0, 0.0, 0.0], &[0.1; 5], &[1e8, 1e8, 1.0, 1.0], &[1.0; 4], 0.01);
        g.step(&[0.0; 5]).unwrap();
        let want = 0.0 + ((1.0 / 0.1f32) * (((-0.0f32 + 0.0) + 1.0) + 0.0)) * 0.01f32;
        assert_eq!(g.state()[3], want);
    }

    #[test]
    fn reset_returns_every_node_to_its_bias() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let mut g = graded::<f64>(&net, &[0.5, -0.25], &[0.1, 0.1], &[1.0], &[1.0], 0.01);
        g.step(&[1.0, 1.0]).unwrap();
        g.reset();
        assert_eq!(g.state(), [0.5, -0.25]);
    }

    #[test]
    fn refuses_bad_parameters() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let new = |b: &[f32], t: &[f32], s: &[f32], c: &[f64], dt: f64| {
            Graded::<f64>::new(&net, &GradedParams { bias: b, time_const: t, syn_strength: s, edge_syn_count: c }, dt)
                .map(|_| ())
        };
        use GradedError::*;
        let (b, t, s, c) = (&[0.0, 0.0][..], &[0.1, 0.1][..], &[1.0][..], &[1.0][..]);
        assert_eq!(new(b, t, s, c, 0.01), Ok(()));
        assert_eq!(new(&[0.0], t, s, c, 0.01), Err(Length { what: "bias", expected: 2, found: 1 }));
        assert_eq!(new(b, &[0.1], s, c, 0.01), Err(Length { what: "time_const", expected: 2, found: 1 }));
        assert_eq!(new(b, t, &[1.0, 1.0], c, 0.01), Err(Length { what: "syn_strength", expected: 1, found: 2 }));
        assert_eq!(new(b, t, s, &[], 0.01), Err(Length { what: "edge_syn_count", expected: 1, found: 0 }));
        assert_eq!(new(&[f32::NAN, 0.0], t, s, c, 0.01), Err(NonFinite { what: "bias", index: 0 }));
        assert_eq!(new(b, &[0.1, f32::INFINITY], s, c, 0.01), Err(NonFinite { what: "time_const", index: 1 }));
        assert_eq!(new(b, t, &[f32::NAN], c, 0.01), Err(NonFinite { what: "syn_strength", index: 0 }));
        assert_eq!(new(b, t, s, &[f64::INFINITY], 0.01), Err(NonFinite { what: "edge_syn_count", index: 0 }));
        for dt in [0.0, -0.01, f64::NAN, f64::INFINITY] {
            assert!(matches!(new(b, t, s, c, dt), Err(BadDt(_))), "dt {dt}");
        }
    }

    /// The floor and the f32 step use f32(dt), so a dt that is positive and
    /// finite in f64 but 0 or infinite in f32 must be refused in both
    /// precisions: otherwise inv_tau is 1/0 or 1/inf.
    #[test]
    fn refuses_a_dt_that_is_zero_or_infinite_in_f32() {
        let net = network(&[0], 1, &[], &[]);
        for dt in [1e-46, f64::from(f32::MAX) * 2.0, 1e39] {
            for t in [&[0.0f32][..], &[-1.0], &[0.05]] {
                let p64 = GradedParams { bias: &[0.5], time_const: t, syn_strength: &[], edge_syn_count: &[] };
                assert_eq!(Graded::<f64>::new(&net, &p64, dt).map(|_| ()), Err(GradedError::BadDt(dt)), "f64 dt {dt}");
                let p32 = GradedParams { bias: &[0.5], time_const: t, syn_strength: &[], edge_syn_count: &[] };
                assert_eq!(Graded::<f32>::new(&net, &p32, dt).map(|_| ()), Err(GradedError::BadDt(dt)), "f32 dt {dt}");
            }
        }
        // The smallest positive f32 is still a usable dt.
        let tiny = f64::from(f32::from_bits(1));
        let p = GradedParams { bias: &[0.5], time_const: &[0.0], syn_strength: &[], edge_syn_count: &[] };
        assert!(Graded::<f64>::new(&net, &p, tiny).is_ok());
    }

    #[test]
    fn a_failed_step_leaves_the_state_alone() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let mut g = graded::<f64>(&net, &[0.5, -0.25], &[0.1, 0.1], &[1.0], &[1.0], 0.01);
        g.step(&[0.0, 0.0]).unwrap();
        let before = g.state().to_vec();
        assert_eq!(g.step(&[0.0]), Err(GradedError::Length { what: "input", expected: 2, found: 1 }));
        assert_eq!(g.state(), &before[..]);
    }
}
