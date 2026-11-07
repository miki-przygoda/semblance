//! Gate C's controls: networks that should not look like the fly.
//!
//! Each control is applied to each of the 50 trained models, giving 50
//! draws per control, each seeded through `semblance_rng` by
//! [`control_seed`]. A draw whose state goes non-finite is redrawn with the
//! next seed in its sequence ([`draw_stable`]), as Zhou & Hasler
//! rejection-sample. Running the draws through a battery and scoring them is
//! gate C, in `validate --ensemble`.
//! The rewired control ([`rewire`]) is ours, not Zhou & Hasler's.
//!
//! Spec: docs/superpowers/specs/2026-11-04-representational-axis-design.md,
//! section 2.

use std::collections::{HashMap, HashSet};
use std::fmt;

use semblance_connectome::{TypedNetwork, TypedNetworkParts};
use semblance_rng::Rng;

use super::ParityError;
use crate::ensemble::{Ensemble, ModelParams};

/// The four controls (owner's answer Q5), in seed order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control {
    /// The model's biases and time constants; every strength 0.
    Zeroed,
    /// The model's strengths permuted across connection types.
    Shuffled,
    /// Every parameter uniform within the ensemble's range for it.
    RandomParams,
    /// The model's parameters on a randomly rewired topology (ours, not
    /// Zhou & Hasler's).
    Rewired,
}

impl Control {
    pub const ALL: [Control; 4] = [Control::Zeroed, Control::Shuffled, Control::RandomParams, Control::Rewired];

    /// The control's index in [`control_seed`].
    pub const fn index(self) -> u64 {
        match self {
            Control::Zeroed => 0,
            Control::Shuffled => 1,
            Control::RandomParams => 2,
            Control::Rewired => 3,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Control::Zeroed => "zeroed",
            Control::Shuffled => "shuffled",
            Control::RandomParams => "random parameters",
            Control::Rewired => "rewired",
        }
    }

    /// Whether every draw is the same, so redrawing cannot help.
    pub const fn deterministic(self) -> bool {
        matches!(self, Control::Zeroed)
    }
}

/// The constant every control seed starts from.
pub const SEED_BASE: u64 = 0x5EB1_A9CE_0000_0000;

/// The most redraws one draw gets after its original attempt (RA §2:
/// "redrawn ... up to 10 times"): attempts 0..=MAX_REDRAWS. The original and
/// all 10 redraws non-finite is gate C's failure.
pub const MAX_REDRAWS: u32 = 10;

/// The seed of `control`'s draw for the model at ensemble position `model`,
/// attempt `attempt` (0 for the first draw, +1 per redraw):
///
/// ```text
/// SEED_BASE ^ (control.index() << 24) ^ (model << 8) ^ attempt
/// ```
///
/// Distinct for every model below 65,536 and attempt below 256.
pub fn control_seed(control: Control, model: usize, attempt: u32) -> u64 {
    SEED_BASE ^ (control.index() << 24) ^ ((model as u64) << 8) ^ attempt as u64
}

/// The zeroed control: `p`'s biases and time constants, all 604 strengths 0.
pub fn zeroed(p: &ModelParams) -> ModelParams {
    ModelParams { syn_strength: [0.0; 604], ..p.clone() }
}

/// The shuffled control: `p`'s 604 strengths permuted across connection
/// types by Durstenfeld's Fisher–Yates. A connection type's sign is the
/// network's, not the strength's, so every sign is kept.
pub fn shuffled(p: &ModelParams, rng: &mut Rng) -> ModelParams {
    let mut s = p.syn_strength;
    for i in (1..s.len()).rev() {
        let j = rng.below(i + 1);
        s.swap(i, j);
    }
    ModelParams { syn_strength: s, ..p.clone() }
}

/// Each of the 734 parameters' minimum and maximum over a set of models.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamRange {
    min: ModelParams,
    max: ModelParams,
}

impl ParamRange {
    /// The range over `models`; refused (`Usage`) if there are none.
    pub fn over<'a>(models: impl IntoIterator<Item = &'a ModelParams>) -> Result<ParamRange, ParityError> {
        let mut it = models.into_iter();
        let first = it.next().ok_or_else(|| ParityError::Usage("a parameter range needs at least one model".into()))?;
        let (mut min, mut max) = (first.clone(), first.clone());
        for m in it {
            for ((lo, hi), &v) in slots_mut(&mut min).zip(slots_mut(&mut max)).zip(slots(m)) {
                *lo = lo.min(v);
                *hi = hi.max(v);
            }
        }
        Ok(ParamRange { min, max })
    }

    /// The range over every model of the ensemble.
    pub fn of_ensemble(e: &Ensemble) -> Result<ParamRange, ParityError> {
        Self::over(e.ids().filter_map(|id| e.model(id)))
    }

    pub fn min(&self) -> &ModelParams {
        &self.min
    }

    pub fn max(&self) -> &ModelParams {
        &self.max
    }
}

/// The random-parameters control: each parameter `lo + (hi - lo) * u` in
/// f64, `u = rng.next_f32()`, rounded to f32 and clamped to `[lo, hi]`, in
/// the 734-row order (biases, time constants, strengths). Every parameter
/// consumes one draw, constant or not.
pub fn random_params(range: &ParamRange, rng: &mut Rng) -> ModelParams {
    let mut out = range.min.clone();
    for ((v, &lo), &hi) in slots_mut(&mut out).zip(slots(&range.min)).zip(slots(&range.max)) {
        let u = rng.next_f32();
        let x = (lo as f64 + (hi as f64 - lo as f64) * u as f64) as f32;
        *v = x.max(lo).min(hi);
    }
    out
}

/// One non-finite attempt that was redrawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Redraw {
    pub attempt: u32,
    pub seed: u64,
    pub error: ParityError,
}

/// A stable draw: its value, the attempt and seed that gave it, and the
/// attempts before it that went non-finite.
#[derive(Clone, Debug, PartialEq)]
pub struct Draw<T> {
    pub value: T,
    pub attempt: u32,
    pub seed: u64,
    pub redraws: Vec<Redraw>,
}

/// Why a draw has no value.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawError {
    /// Every attempt went non-finite: gate C's failure.
    Unstable { control: Control, model: usize, redraws: Vec<Redraw> },
    /// An attempt failed for another reason: verification did not complete.
    Failed { control: Control, model: usize, attempt: u32, error: ParityError },
}

impl DrawError {
    /// Whether this is a gate failure (exit 1) rather than verification
    /// not completing (exit 2).
    pub fn is_gate_failure(&self) -> bool {
        matches!(self, DrawError::Unstable { .. })
    }
}

impl fmt::Display for DrawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DrawError::Unstable { control, model, redraws } => {
                let seeds: Vec<String> = redraws.iter().map(|r| format!("{:#x}", r.seed)).collect();
                write!(
                    f,
                    "{} control, model {model}: {} draws in a row went non-finite (seeds {})",
                    control.name(),
                    redraws.len(),
                    seeds.join(", ")
                )
            }
            DrawError::Failed { control, model, attempt, error } => {
                write!(f, "{} control, model {model}, attempt {attempt}: {error}", control.name())
            }
        }
    }
}

impl std::error::Error for DrawError {}

/// Draws `control` for the model at position `model`: calls `run` with
/// attempt 0's seed, and while it returns a non-finite state
/// (`ParityError::is_gate_failure`) calls it again with the next attempt's
/// seed, up to [`MAX_REDRAWS`] redraws (attempts 0..=MAX_REDRAWS; one
/// attempt for the deterministic zeroed control). Any other error ends the
/// draw. The redraws are returned for the caller to print and record.
pub fn draw_stable<T>(
    control: Control,
    model: usize,
    mut run: impl FnMut(u64) -> Result<T, ParityError>,
) -> Result<Draw<T>, DrawError> {
    let last = if control.deterministic() { 0 } else { MAX_REDRAWS };
    let mut redraws = Vec::new();
    for attempt in 0..=last {
        let seed = control_seed(control, model, attempt);
        match run(seed) {
            Ok(value) => return Ok(Draw { value, attempt, seed, redraws }),
            Err(error) if error.is_gate_failure() => redraws.push(Redraw { attempt, seed, error }),
            Err(error) => return Err(DrawError::Failed { control, model, attempt, error }),
        }
    }
    Err(DrawError::Unstable { control, model, redraws })
}

/// The most draws one connection type gets for an unused (source, target)
/// pair before `rewire` gives up.
pub const MAX_SOURCE_TRIES: usize = 1000;

/// The rewired control's network. Our addition, not Zhou & Hasler's (they
/// do not rewire; owner's answer Q5): it tests the topology, where their
/// shuffles test the strengths.
#[derive(Clone, Debug, PartialEq)]
pub struct Rewired {
    /// The same cell types and nodes, the connection types with new
    /// sources, the surviving edges in their original order.
    pub net: TypedNetwork,
    /// Per surviving edge, its original edge's synapse-count group.
    pub groups: Vec<u32>,
    /// Per surviving edge, the index of the original edge it was rebuilt from.
    pub kept: Vec<u32>,
    /// Per connection type, its new source cell type.
    pub sources: Vec<u16>,
    /// Edges dropped because the new source type has no node in their
    /// source column; the caller prints it.
    pub dropped: usize,
}

/// The rewired control: each connection type, in index order, draws a new
/// presynaptic cell type uniformly from all of them (`rng.below`),
/// redrawing while an earlier connection type already holds that (source,
/// target) pair, at most [`MAX_SOURCE_TRIES`] times. Each edge is rebuilt
/// from the new type's node in its old source's column `(u, v)`, keeping its
/// target, connection type (so its sign and strength), synapse count and
/// synapse-count group; an edge whose column the new type lacks is dropped.
/// Refused (`Usage`): `groups` not one per edge, a connection type that
/// finds no unused pair, a result `TypedNetwork::new` refuses.
pub fn rewire(net: &TypedNetwork, groups: &[u32], rng: &mut Rng) -> Result<Rewired, ParityError> {
    if groups.len() != net.num_edges() {
        return Err(ParityError::Usage(format!("rewire: {} synapse-count groups for {} edges", groups.len(), net.num_edges())));
    }
    let n_types = net.cell_types().len();
    if n_types == 0 && !net.conn_types().is_empty() {
        return Err(ParityError::Usage("rewire: connection types but no cell types".into()));
    }
    let mut taken: HashSet<(u16, u16)> = HashSet::new();
    let mut sources = Vec::with_capacity(net.conn_types().len());
    for (c, &(_, target)) in net.conn_types().iter().enumerate() {
        let mut found = None;
        for _ in 0..MAX_SOURCE_TRIES {
            let s = rng.below(n_types) as u16;
            if taken.insert((s, target)) {
                found = Some(s);
                break;
            }
        }
        let s = found.ok_or_else(|| {
            ParityError::Usage(format!("rewire: connection type {c} found no unused source type in {MAX_SOURCE_TRIES} draws"))
        })?;
        sources.push(s);
    }

    let mut at: HashMap<(u16, i16, i16), u32> = HashMap::with_capacity(net.num_nodes());
    for (node, ((&t, &u), &v)) in net.node_cell_type().iter().zip(net.node_u()).zip(net.node_v()).enumerate() {
        at.insert((t, u, v), node as u32);
    }
    let (mut edge_source, mut edge_target, mut edge_conn_type, mut edge_n_syn) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut kept_groups, mut kept) = (Vec::new(), Vec::new());
    let mut dropped = 0usize;
    for (e, &g) in groups.iter().enumerate() {
        let c = net.edge_conn_type()[e];
        let old = net.edge_source()[e] as usize;
        match at.get(&(sources[c as usize], net.node_u()[old], net.node_v()[old])) {
            Some(&new) => {
                edge_source.push(new);
                edge_target.push(net.edge_target()[e]);
                edge_conn_type.push(c);
                edge_n_syn.push(net.edge_n_syn()[e]);
                kept_groups.push(g);
                kept.push(e as u32);
            }
            None => dropped += 1,
        }
    }
    let parts = TypedNetworkParts {
        cell_types: net.cell_types().to_vec(),
        node_cell_type: net.node_cell_type().to_vec(),
        node_u: net.node_u().to_vec(),
        node_v: net.node_v().to_vec(),
        conn_types: sources.iter().zip(net.conn_types()).map(|(&s, &(_, t))| (s, t)).collect(),
        conn_sign: net.conn_sign().to_vec(),
        edge_source,
        edge_target,
        edge_conn_type,
        edge_n_syn,
    };
    let rewired = TypedNetwork::new(parts).map_err(|e| ParityError::Usage(format!("rewire: {e}")))?;
    Ok(Rewired { net: rewired, groups: kept_groups, kept, sources, dropped })
}

/// The 734 parameters in row order: biases, time constants, strengths.
fn slots(p: &ModelParams) -> impl Iterator<Item = &f32> + '_ {
    p.bias.iter().chain(&p.time_const).chain(&p.syn_strength)
}

fn slots_mut(p: &mut ModelParams) -> impl Iterator<Item = &mut f32> + '_ {
    p.bias.iter_mut().chain(p.time_const.iter_mut()).chain(p.syn_strength.iter_mut())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_support::fixture;
    use crate::parity::{respond_network, Battery, Counts};
    use crate::reference::Dtype;
    use semblance_connectome::TypedNetworkParts;

    /// Model 000 scaled by `1 + k / 100` everywhere: a synthetic ensemble member.
    fn scaled(k: u32) -> ModelParams {
        let mut p = ModelParams::pretrained();
        for v in slots_mut(&mut p) {
            *v *= 1.0 + k as f32 / 100.0;
        }
        p
    }

    fn unstable() -> ParityError {
        ParityError::NonFiniteSettle { node: 7, cell_type: "T4a".into(), step: 3 }
    }

    #[test]
    fn the_seed_formula_is_pinned_and_injective() {
        assert_eq!(SEED_BASE, 0x5EB1_A9CE_0000_0000);
        assert_eq!(control_seed(Control::Shuffled, 0, 0), 0x5EB1_A9CE_0100_0000);
        assert_eq!(control_seed(Control::Shuffled, 7, 2), 0x5EB1_A9CE_0100_0702);
        assert_eq!(control_seed(Control::Rewired, 49, 9), 0x5EB1_A9CE_0300_3109);
        assert_eq!(control_seed(Control::Rewired, 49, MAX_REDRAWS), 0x5EB1_A9CE_0300_310A);
        let mut seen = std::collections::HashSet::new();
        for c in Control::ALL {
            for m in 0..50 {
                for a in 0..=MAX_REDRAWS {
                    assert!(seen.insert(control_seed(c, m, a)), "{c:?} {m} {a}");
                }
            }
        }
        let names: Vec<&str> = Control::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names, ["zeroed", "shuffled", "random parameters", "rewired"]);
        assert_eq!(Control::ALL.map(|c| c.index()), [0, 1, 2, 3]);
    }

    #[test]
    fn zeroed_keeps_biases_and_time_constants_and_zeroes_all_604_strengths() {
        let p = scaled(3);
        let z = zeroed(&p);
        assert_eq!(z.syn_strength.len(), 604);
        assert!(z.syn_strength.iter().all(|s| s.to_bits() == 0), "every strength is +0.0");
        assert_eq!((z.bias, z.time_const), (p.bias, p.time_const));
    }

    #[test]
    fn shuffled_is_a_durstenfeld_permutation_of_the_strengths() {
        let mut p = ModelParams::pretrained();
        for (i, s) in p.syn_strength.iter_mut().enumerate() {
            *s = i as f32;
        }
        let seed = control_seed(Control::Shuffled, 0, 0);
        let q = shuffled(&p, &mut Rng::seed_from_u64(seed));
        assert_eq!(&q.syn_strength[..6], &[272.0, 155.0, 332.0, 262.0, 416.0, 459.0]);
        assert_eq!(&q.syn_strength[601..], &[9.0, 447.0, 44.0]);
        let mut sorted = q.syn_strength.to_vec();
        sorted.sort_by(f32::total_cmp);
        assert_eq!(sorted, p.syn_strength.to_vec());
        assert_eq!((q.bias, q.time_const), (p.bias, p.time_const));
        assert_eq!(shuffled(&p, &mut Rng::seed_from_u64(seed)), q, "the same seed, the same draw");
    }

    #[test]
    fn random_parameters_lie_within_each_parameters_range_and_repeat_by_seed() {
        let mut models = [scaled(0), scaled(5), scaled(10)];
        for m in models.iter_mut() {
            m.bias[0] = 0.25;
        }
        let range = ParamRange::over(&models).unwrap();
        let mut seen = std::collections::HashSet::new();
        for seed in 0..20u64 {
            let p = random_params(&range, &mut Rng::seed_from_u64(seed));
            for ((&v, &lo), &hi) in slots(&p).zip(slots(range.min())).zip(slots(range.max())) {
                assert!(lo <= v && v <= hi, "seed {seed}: {v} outside [{lo}, {hi}]");
            }
            assert_eq!(p.bias[0], 0.25, "a constant parameter stays constant");
            assert_eq!(random_params(&range, &mut Rng::seed_from_u64(seed)), p, "the same seed, the same draw");
            seen.insert(p.bias[1].to_bits());
        }
        assert!(seen.len() >= 2, "a drawn value depends on its draw");
        // slot 1 (bias[1]) takes the second draw: lo + (hi - lo) u in f64
        let mut g = Rng::seed_from_u64(0);
        g.next_f32();
        let u = g.next_f32();
        let (lo, hi) = (range.min().bias[1], range.max().bias[1]);
        let want = ((lo as f64 + (hi as f64 - lo as f64) * u as f64) as f32).max(lo).min(hi);
        assert_eq!(random_params(&range, &mut Rng::seed_from_u64(0)).bias[1], want);
        // exactly 734 draws, whatever the data
        let mut a = Rng::seed_from_u64(1);
        random_params(&range, &mut a);
        let mut b = Rng::seed_from_u64(1);
        for _ in 0..734 {
            b.next_f32();
        }
        assert_eq!(a.next_u64(), b.next_u64());
        assert!(matches!(ParamRange::over(std::iter::empty()), Err(ParityError::Usage(_))));
    }

    #[test]
    fn an_unstable_draw_is_redrawn_with_the_next_seed() {
        let mut seen = Vec::new();
        let d = draw_stable(Control::Shuffled, 4, |seed| {
            seen.push(seed);
            if seen.len() < 4 { Err(unstable()) } else { Ok(seed) }
        })
        .unwrap();
        let want: Vec<u64> = (0..4).map(|a| control_seed(Control::Shuffled, 4, a)).collect();
        assert_eq!(seen, want);
        assert_eq!((d.value, d.attempt, d.seed), (want[3], 3, want[3]));
        let redraws: Vec<(u32, u64)> = d.redraws.iter().map(|r| (r.attempt, r.seed)).collect();
        assert_eq!(redraws, vec![(0, want[0]), (1, want[1]), (2, want[2])]);
    }

    #[test]
    fn an_original_and_ten_unstable_redraws_are_a_gate_failure_and_zeroed_gets_one() {
        let mut seeds = Vec::new();
        let e = draw_stable(Control::RandomParams, 0, |seed| {
            seeds.push(seed);
            Err::<(), _>(unstable())
        })
        .unwrap_err();
        assert!(e.is_gate_failure());
        // the original draw plus 10 redraws: 11 attempts
        assert_eq!(MAX_REDRAWS, 10);
        assert!(matches!(e, DrawError::Unstable { ref redraws, .. } if redraws.len() == 11), "{e}");
        let want: Vec<u64> = (0..=MAX_REDRAWS).map(|a| control_seed(Control::RandomParams, 0, a)).collect();
        assert_eq!(seeds, want);
        // a draw that needs its 10th redraw still counts
        let mut calls = 0;
        let d = draw_stable(Control::RandomParams, 0, |seed| {
            calls += 1;
            if calls <= 10 { Err(unstable()) } else { Ok(seed) }
        })
        .unwrap();
        assert_eq!((d.attempt, d.redraws.len()), (10, 10));
        let e = draw_stable(Control::Zeroed, 0, |_| Err::<(), _>(unstable())).unwrap_err();
        assert!(matches!(e, DrawError::Unstable { ref redraws, .. } if redraws.len() == 1), "{e}");
    }

    #[test]
    fn any_other_error_ends_the_draw_without_a_redraw() {
        let mut calls = 0;
        let e = draw_stable(Control::Shuffled, 0, |_| {
            calls += 1;
            Err::<(), _>(ParityError::Usage("bad".into()))
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(!e.is_gate_failure());
        assert_eq!(e, DrawError::Failed { control: Control::Shuffled, model: 0, attempt: 0, error: ParityError::Usage("bad".into()) });
    }

    #[test]
    fn errors_display_as_registered() {
        let e = DrawError::Failed { control: Control::Rewired, model: 3, attempt: 2, error: ParityError::Usage("bad".into()) };
        assert_eq!(e.to_string(), "rewired control, model 3, attempt 2: bad");
        let redraws = vec![Redraw { attempt: 0, seed: 0x10, error: unstable() }, Redraw { attempt: 1, seed: 0x11, error: unstable() }];
        let e = DrawError::Unstable { control: Control::RandomParams, model: 5, redraws };
        assert_eq!(e.to_string(), "random parameters control, model 5: 2 draws in a row went non-finite (seeds 0x10, 0x11)");
    }

    /// Fixture-gated: the range over the real 50 models holds every model.
    #[test]
    fn the_real_ensembles_range_holds_every_model() {
        let Some(f) = fixture() else { return };
        let range = ParamRange::of_ensemble(&f.ensemble).unwrap_or_else(|e| panic!("{e}"));
        for id in f.ensemble.ids() {
            let m = f.ensemble.model(id).unwrap();
            for ((&v, &lo), &hi) in slots(m).zip(slots(range.min())).zip(slots(range.max())) {
                assert!(lo <= v && v <= hi, "model {id}");
            }
        }
        let constant = slots(range.min()).zip(slots(range.max())).filter(|(a, b)| a == b).count();
        eprintln!("random-parameter range over the 50 models: {constant} of 734 parameters are the same in every model");
        assert!(constant < 734);
    }

    /// Fixture-gated: on the real network a shuffled model 000 keeps every
    /// edge's sign, because the sign is the connection type's.
    #[test]
    fn shuffled_strengths_keep_every_connections_sign() {
        let Some(f) = fixture() else { return };
        let p = f.ensemble.model("000").unwrap();
        assert!(p.syn_strength.iter().all(|&s| s >= 0.0), "model 000's strengths are non-negative");
        let q = shuffled(p, &mut Rng::seed_from_u64(control_seed(Control::Shuffled, 0, 0)));
        let mut w = vec![0.0f32; f.net.num_edges()];
        f.net.weights_into(&q.syn_strength, &mut w).unwrap();
        for (e, (&x, &ct)) in w.iter().zip(f.net.edge_conn_type()).enumerate() {
            assert!(x == 0.0 || x.signum() == f.net.conn_sign()[ct as usize], "edge {e}");
        }
    }

    /// Three cell types on two columns: A and B at (0, 0) and (1, 0), C at
    /// (0, 0) only. Connection types A>B, B>C, C>A, C>B (types 0 and 3 share
    /// target B, so their draws can collide); five edges, groups 10..14.
    fn small() -> (TypedNetwork, Vec<u32>) {
        let net = TypedNetwork::new(TypedNetworkParts {
            cell_types: vec!["A".into(), "B".into(), "C".into()],
            node_cell_type: vec![0, 0, 1, 1, 2],
            node_u: vec![0, 1, 0, 1, 0],
            node_v: vec![0, 0, 0, 0, 0],
            conn_types: vec![(0, 1), (1, 2), (2, 0), (2, 1)],
            conn_sign: vec![1.0, -1.0, 1.0, 1.0],
            edge_source: vec![0, 1, 3, 4, 4],
            edge_target: vec![2, 2, 4, 1, 2],
            edge_conn_type: vec![0, 0, 1, 2, 3],
            edge_n_syn: vec![2.0, 3.0, 1.0, 4.0, 5.0],
        })
        .unwrap();
        (net, vec![10, 11, 12, 13, 14])
    }

    /// What every rewired network keeps, checked edge by edge.
    fn assert_rewired(net: &TypedNetwork, groups: &[u32], r: &Rewired) {
        assert_eq!(r.kept.len() + r.dropped, net.num_edges());
        assert_eq!((r.net.num_edges(), r.groups.len()), (r.kept.len(), r.kept.len()));
        assert_eq!(r.net.conn_sign(), net.conn_sign());
        assert_eq!(r.net.cell_types(), net.cell_types());
        assert_eq!((r.net.node_cell_type(), r.net.node_u(), r.net.node_v()), (net.node_cell_type(), net.node_u(), net.node_v()));
        let mut pairs = std::collections::HashSet::new();
        for (c, (&(s, t), &(_, t0))) in r.net.conn_types().iter().zip(net.conn_types()).enumerate() {
            assert_eq!(t, t0, "connection type {c} keeps its target");
            assert_eq!(s, r.sources[c]);
            assert!(pairs.insert((s, t)), "connection type {c} repeats a (source, target) pair");
        }
        for (k, &e) in r.kept.iter().enumerate() {
            let e = e as usize;
            let (old, new) = (net.edge_source()[e] as usize, r.net.edge_source()[k] as usize);
            assert_eq!(r.net.edge_target()[k], net.edge_target()[e]);
            assert_eq!(r.net.edge_conn_type()[k], net.edge_conn_type()[e]);
            assert_eq!(r.net.edge_n_syn()[k].to_bits(), net.edge_n_syn()[e].to_bits());
            assert_eq!(r.groups[k], groups[e]);
            assert_eq!((r.net.node_u()[new], r.net.node_v()[new]), (net.node_u()[old], net.node_v()[old]), "edge {e} keeps its source column");
            assert_eq!(r.net.node_cell_type()[new], r.sources[net.edge_conn_type()[e] as usize]);
        }
        assert!(r.kept.windows(2).all(|w| w[0] < w[1]), "kept edges stay in order");
    }

    #[test]
    fn rewiring_keeps_targets_columns_counts_and_signs_and_passes_validation() {
        let (net, groups) = small();
        let (mut dropped, mut sources) = (Vec::new(), Vec::new());
        for model in 0..20 {
            let seed = control_seed(Control::Rewired, model, 0);
            let r = rewire(&net, &groups, &mut Rng::seed_from_u64(seed)).unwrap();
            assert_rewired(&net, &groups, &r);
            assert_ne!(r.sources[0], r.sources[3], "model {model}: types 0 and 3 share target B");
            assert_eq!(rewire(&net, &groups, &mut Rng::seed_from_u64(seed)).unwrap(), r, "the same seed, the same draw");
            dropped.push(r.dropped);
            sources.push(r.sources.clone());
        }
        // Models 0 and 4 collide: type 3's first draw is type 0's source
        // (2, C), so it is redrawn from the stream (3 redraws for model 0,
        // 1 for model 4), not skipped forward; types not yet processed do
        // not block a pair (model 0's type 0 takes C>B, type 3's original).
        assert_eq!(
            sources[..6],
            [vec![2u16, 1, 1, 0], vec![0, 2, 0, 2], vec![2, 1, 0, 1], vec![2, 0, 0, 0], vec![2, 1, 1, 1], vec![2, 2, 2, 1]],
            "pins rewire's use of the frozen stream"
        );
        assert_eq!(dropped[..6], [1, 1, 1, 1, 1, 2], "pins rewire's use of the frozen stream");
    }

    #[test]
    fn a_target_with_more_connection_types_than_cell_types_is_refused_not_looped() {
        let crowded = TypedNetwork::new(TypedNetworkParts {
            cell_types: vec!["A".into()],
            node_cell_type: vec![0],
            node_u: vec![0],
            node_v: vec![0],
            conn_types: vec![(0, 0), (0, 0)],
            conn_sign: vec![1.0, 1.0],
            edge_source: vec![],
            edge_target: vec![],
            edge_conn_type: vec![],
            edge_n_syn: vec![],
        })
        .unwrap();
        let e = rewire(&crowded, &[], &mut Rng::seed_from_u64(1)).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.contains("no unused source type")), "{e}");
    }

    #[test]
    fn rewiring_refuses_groups_that_are_not_one_per_edge() {
        let (net, _) = small();
        let e = rewire(&net, &[0], &mut Rng::seed_from_u64(1)).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.contains("groups for")), "{e}");
    }

    /// One grey stimulus of `frames` frames.
    struct Grey {
        frames: usize,
        done: bool,
    }

    impl Battery for Grey {
        fn len(&self) -> usize {
            1
        }
        fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError> {
            if self.done {
                return Ok(None);
            }
            self.done = true;
            out.clear();
            out.resize(self.frames * 721, 0.5);
            Ok(Some("grey".into()))
        }
    }

    /// Fixture-gated: the real network rewires validly, and the rewired
    /// network either responds through `respond_network` or goes non-finite
    /// (a redraw, for gate C), never another error.
    #[test]
    fn the_real_network_rewires_and_responds_or_goes_non_finite() {
        let Some(f) = fixture() else { return };
        let seed = control_seed(Control::Rewired, 0, 0);
        let r = rewire(&f.net, &f.groups, &mut Rng::seed_from_u64(seed)).unwrap_or_else(|e| panic!("{e}"));
        assert_rewired(&f.net, &f.groups, &r);
        eprintln!("rewired control, model 000, attempt 0: {} of {} edges dropped", r.dropped, f.net.num_edges());
        let p = f.ensemble.model("000").unwrap();
        let mut b = Grey { frames: 2, done: false };
        match respond_network(f, &r.net, &r.groups, p, 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b) {
            Ok(resp) => eprintln!("rewired model 000 responds: {} frames of {} types", resp.stimulus(0).unwrap().n_frames(), resp.cell_types().len()),
            Err(e) => {
                assert!(e.is_gate_failure(), "{e}");
                eprintln!("rewired model 000 went non-finite: {e}");
            }
        }
    }
}
