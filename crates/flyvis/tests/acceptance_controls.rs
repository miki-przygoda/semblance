//! Acceptance tests for behaviour group `controls` of epic
//! fidelity-measures: gate C's four controls (zeroed, shuffled, random
//! parameters, rewired), their seeds and redraws, and `respond_network` on a
//! rewired topology, through `semblance_flyvis::parity::controls` and
//! `semblance_flyvis::parity::respond_network`.
//!
//! One test per behaviour id, CONTROLS-01 .. CONTROLS-08; CONTROLS-09 ..
//! CONTROLS-12, which `semblance validate --ensemble` shows, are in
//! `app/tests/acceptance_controls.rs`.
//!
//! CONTROLS-01 and CONTROLS-02 need no data. The others read 0.3.0's
//! fixtures (the network, the ensemble, the reference manifest) under
//! `data/`, and skip with one `skipping:` notice each when any is absent,
//! unless `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns the skip into a
//! failure.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use semblance_connectome::{TypedNetwork, TypedNetworkParts};
use semblance_flyvis::edge_syn_groups;
use semblance_flyvis::parity::controls::{
    control_seed, draw_stable, random_params, rewire, shuffled, zeroed, Control, DrawError, ParamRange,
    MAX_REDRAWS, SEED_BASE,
};
use semblance_flyvis::parity::{required_files, respond, respond_network, Battery, Counts, Fixtures, ParityError};
use semblance_flyvis::reference::Dtype;
use semblance_flyvis::{Ensemble, ModelParams};
use semblance_rng::Rng;
use semblance_sim::response::Responses;

const N_MODELS: usize = 50;
const N_STRENGTHS: usize = 604;
const N_PARAMS: usize = 734;
const HEXALS: usize = 721;

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

struct Real {
    f: Fixtures,
    ensemble: Ensemble,
    groups: Vec<u32>,
}

/// The real fixtures, loaded once; `None` with one `skipping:` notice per
/// asking test when any is missing (a failure under
/// `SEMBLANCE_REQUIRE_FIXTURES=1`). The miss is cached, not the notice.
fn real(test: &str) -> Option<&'static Real> {
    static REAL: OnceLock<Result<Real, Vec<String>>> = OnceLock::new();
    let r = REAL.get_or_init(|| {
        let missing: Vec<String> = required_files()
            .iter()
            .filter(|rel| fs::metadata(data().join(rel)).is_err())
            .map(|rel| rel.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(missing);
        }
        let f = Fixtures::load(&data()).expect("the fixtures load");
        let csv = fs::read_to_string(data().join("flyvis-pretrained/ensemble.csv")).expect("ensemble.csv reads");
        let ensemble = Ensemble::parse(&csv).expect("ensemble.csv parses");
        let groups = edge_syn_groups(f.network()).expect("the network's synapse-count groups");
        Ok(Real { f, ensemble, groups })
    });
    match r {
        Ok(real) => Some(real),
        Err(missing) => {
            if std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1") {
                panic!("{test}: SEMBLANCE_REQUIRE_FIXTURES=1 but fixtures are missing under {}: {missing:?}", data().display());
            }
            eprintln!("skipping: {test}: fixtures missing under {}: {missing:?}", data().display());
            None
        }
    }
}

/// The 50 trained models, in file order.
fn models(r: &Real) -> Vec<&ModelParams> {
    let ms: Vec<&ModelParams> = r.ensemble.ids().map(|id| r.ensemble.model(id).expect("listed id")).collect();
    assert_eq!(ms.len(), N_MODELS);
    ms
}

/// The 734 parameters in row order: biases, time constants, strengths.
fn slots(p: &ModelParams) -> Vec<f32> {
    p.bias.iter().chain(&p.time_const).chain(&p.syn_strength).copied().collect()
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// A small battery: a few grey, bright and dark frames, at any dt.
struct Small {
    stimuli: Vec<(String, Vec<f64>)>,
    next: usize,
}

fn small(grey: f64) -> Small {
    let block = |v: f64, n: usize| vec![v; n * HEXALS];
    let mut ramp = Vec::new();
    for i in 0..8 {
        ramp.extend((0..HEXALS).map(|h| ((i * 37 + h) % 11) as f64 / 10.0));
    }
    Small {
        stimuli: vec![
            ("grey".into(), block(grey, 6)),
            ("bright".into(), block(1.0, 8)),
            ("dark".into(), block(0.0, 8)),
            ("pattern".into(), ramp),
        ],
        next: 0,
    }
}

impl Battery for Small {
    fn len(&self) -> usize {
        self.stimuli.len()
    }

    fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError> {
        out.clear();
        let Some((label, frames)) = self.stimuli.get(self.next) else { return Ok(None) };
        self.next += 1;
        out.extend_from_slice(frames);
        Ok(Some(label.clone()))
    }
}

/// The coarsest reference dt: the fewest settling steps.
fn coarse_dt(f: &Fixtures) -> f64 {
    f.manifest().dts.iter().map(|d| d.dt).fold(0.0, f64::max)
}

fn all_values(r: &Responses) -> Vec<f64> {
    let mut out = Vec::new();
    for s in 0..r.len() {
        let st = r.stimulus(s).expect("stimulus");
        for i in 0..st.n_frames() {
            out.extend_from_slice(st.frame(i).expect("frame"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// CONTROLS-01
// ---------------------------------------------------------------------------

/// CONTROLS-01: each of the 50 models under each of the 4 controls is seeded
/// through `Rng::seed_from_u64` with a seed from (control index, model index)
/// by a fixed formula, and the same pair gives the same draw.
#[test]
fn controls_01_every_draw_is_seeded_from_control_and_model_by_a_fixed_formula() {
    assert_eq!(Control::ALL, [Control::Zeroed, Control::Shuffled, Control::RandomParams, Control::Rewired]);
    assert_eq!(Control::ALL.map(|c| c.index()), [0, 1, 2, 3], "control indices in seed order");

    let mut seen = HashSet::new();
    for c in Control::ALL {
        for m in 0..N_MODELS {
            let seed = control_seed(c, m, 0);
            // The formula, written out: fixed, not drawn or hashed.
            assert_eq!(seed, SEED_BASE ^ (c.index() << 24) ^ ((m as u64) << 8), "{c:?} model {m}");
            assert_eq!(seed, control_seed(c, m, 0), "the same (control, model) gives the same seed");
            assert!(seen.insert(seed), "{c:?} model {m}: seed {seed:#x} is shared with another pair");
        }
    }
    assert_eq!(seen.len(), 4 * N_MODELS);
    assert_eq!(SEED_BASE, 0x5EB1_A9CE_0000_0000);
    assert_eq!(control_seed(Control::Shuffled, 0, 0), 0x5EB1_A9CE_0100_0000);
    assert_eq!(control_seed(Control::Rewired, 49, 0), 0x5EB1_A9CE_0300_3100);

    // The draw a seed gives: the runner hands each attempt its seed, and the
    // same pair draws the same values through `Rng::seed_from_u64`.
    let p = ModelParams::pretrained();
    for c in [Control::Shuffled, Control::RandomParams] {
        for m in [0usize, 17, 49] {
            let d = draw_stable(c, m, Ok::<u64, ParityError>).expect("a draw");
            assert_eq!((d.attempt, d.seed), (0, control_seed(c, m, 0)), "{c:?} {m}");
            assert!(d.redraws.is_empty());
            let a = shuffled(&p, &mut Rng::seed_from_u64(d.seed));
            let b = shuffled(&p, &mut Rng::seed_from_u64(control_seed(c, m, 0)));
            assert_eq!(bits(&a.syn_strength), bits(&b.syn_strength), "{c:?} {m}: the same pair gives the same draw");
        }
    }
    let range = ParamRange::over([&p, &zeroed(&p)]).expect("a range");
    let one = random_params(&range, &mut Rng::seed_from_u64(control_seed(Control::RandomParams, 3, 0)));
    let two = random_params(&range, &mut Rng::seed_from_u64(control_seed(Control::RandomParams, 3, 0)));
    assert_eq!(bits(&slots(&one)), bits(&slots(&two)), "the same pair gives the same random parameters");
    let other = random_params(&range, &mut Rng::seed_from_u64(control_seed(Control::RandomParams, 4, 0)));
    assert_ne!(bits(&slots(&one)), bits(&slots(&other)), "another model's seed draws differently");
}

// ---------------------------------------------------------------------------
// CONTROLS-02
// ---------------------------------------------------------------------------

/// CONTROLS-02: the zeroed control keeps the biases and time constants and
/// sets all 604 strengths to 0, deterministically.
#[test]
fn controls_02_zeroed_keeps_biases_and_time_constants_and_zeroes_all_604_strengths() {
    let p = ModelParams::pretrained();
    let z = zeroed(&p);
    assert_eq!(bits(&z.bias), bits(&p.bias), "biases kept");
    assert_eq!(bits(&z.time_const), bits(&p.time_const), "time constants kept");
    assert_eq!(z.syn_strength.len(), N_STRENGTHS);
    assert!(z.syn_strength.iter().all(|&s| s.to_bits() == 0), "all 604 strengths are +0.0");
    assert!(p.syn_strength.iter().any(|&s| s != 0.0), "the model had strengths to zero");
    assert_eq!(zeroed(&p), z, "deterministic");
    assert_eq!(zeroed(&z), z);

    // No seed changes it: the runner gives the zeroed control one attempt.
    assert!(Control::Zeroed.deterministic());
    assert!(Control::ALL.iter().filter(|c| c.deterministic()).eq([Control::Zeroed].iter()));
    let mut calls = 0;
    let e = draw_stable(Control::Zeroed, 0, |_| {
        calls += 1;
        Err::<(), _>(ParityError::NonFiniteSettle { node: 0, cell_type: "R1".into(), step: 1 })
    })
    .unwrap_err();
    assert_eq!(calls, 1, "a deterministic control is not redrawn");
    assert!(e.is_gate_failure());
}

// ---------------------------------------------------------------------------
// CONTROLS-03
// ---------------------------------------------------------------------------

/// CONTROLS-03: the shuffled control's 604 strengths are a Fisher-Yates
/// permutation (via `Rng::below`) across connection types, every sign
/// unchanged.
#[test]
fn controls_03_shuffled_is_a_fisher_yates_permutation_with_every_sign_unchanged() {
    // Pinned: 0, 1, .., 603 under (shuffled, model 0, attempt 0).
    let mut ids = ModelParams::pretrained();
    for (i, s) in ids.syn_strength.iter_mut().enumerate() {
        *s = i as f32;
    }
    let q = shuffled(&ids, &mut Rng::seed_from_u64(control_seed(Control::Shuffled, 0, 0)));
    let perm: Vec<usize> = q.syn_strength.iter().map(|&s| s as usize).collect();
    assert_eq!(perm[..6], [272, 155, 332, 262, 416, 459], "the pinned shuffle begins");
    assert_eq!(perm[N_STRENGTHS - 3..], [9, 447, 44], "the pinned shuffle ends");

    // Durstenfeld's Fisher-Yates, written out with `Rng::below`.
    let fisher_yates = |seed: u64| {
        let mut rng = Rng::seed_from_u64(seed);
        let mut v: Vec<usize> = (0..N_STRENGTHS).collect();
        for i in (1..N_STRENGTHS).rev() {
            let j = rng.below(i + 1);
            v.swap(i, j);
        }
        v
    };
    for m in [0usize, 1, 25, 49] {
        let seed = control_seed(Control::Shuffled, m, 0);
        let got: Vec<usize> =
            shuffled(&ids, &mut Rng::seed_from_u64(seed)).syn_strength.iter().map(|&s| s as usize).collect();
        assert_eq!(got, fisher_yates(seed), "model {m}");
    }

    let Some(r) = real("controls_03") else { return };
    let net = r.f.network();
    assert_eq!(net.conn_types().len(), N_STRENGTHS);
    let mut w_p = vec![0.0f32; net.num_edges()];
    let mut w_q = vec![0.0f32; net.num_edges()];
    for (m, p) in models(r).into_iter().enumerate() {
        assert!(p.syn_strength.iter().all(|&s| s >= 0.0), "model {m}: trained strengths carry no sign");
        let q = shuffled(p, &mut Rng::seed_from_u64(control_seed(Control::Shuffled, m, 0)));
        assert_eq!(bits(&q.bias), bits(&p.bias), "model {m}: biases kept");
        assert_eq!(bits(&q.time_const), bits(&p.time_const), "model {m}: time constants kept");
        let (mut a, mut b) = (bits(&p.syn_strength), bits(&q.syn_strength));
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "model {m}: the same 604 strengths, permuted");
        assert_ne!(bits(&p.syn_strength), bits(&q.syn_strength), "model {m}: actually shuffled");
        let moved = p.syn_strength.iter().zip(&q.syn_strength).filter(|(x, y)| x != y).count();
        assert!(moved > N_STRENGTHS / 2, "model {m}: across connection types ({moved} moved)");

        // Signs stay with the connection type: every edge's weight keeps its sign.
        net.weights_into(&p.syn_strength, &mut w_p).unwrap();
        net.weights_into(&q.syn_strength, &mut w_q).unwrap();
        for (e, (&x, &y)) in w_p.iter().zip(&w_q).enumerate() {
            let c = net.edge_conn_type()[e] as usize;
            if y != 0.0 {
                assert_eq!(y.is_sign_negative(), net.conn_sign()[c] < 0.0, "model {m}, edge {e}");
            }
            if x != 0.0 && y != 0.0 {
                assert_eq!(x.is_sign_negative(), y.is_sign_negative(), "model {m}, edge {e}: sign unchanged");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CONTROLS-04
// ---------------------------------------------------------------------------

/// CONTROLS-04: random parameters draw each of the 734 parameters uniformly
/// (`Rng::next_f32`) within that parameter's min..max over the 50 models.
#[test]
fn controls_04_random_parameters_are_uniform_within_each_parameters_range_over_the_50() {
    let Some(r) = real("controls_04") else { return };
    let ms = models(r);
    let all: Vec<Vec<f32>> = ms.iter().map(|p| slots(p)).collect();
    let lo: Vec<f32> = (0..N_PARAMS).map(|k| all.iter().map(|v| v[k]).fold(f32::INFINITY, f32::min)).collect();
    let hi: Vec<f32> = (0..N_PARAMS).map(|k| all.iter().map(|v| v[k]).fold(f32::NEG_INFINITY, f32::max)).collect();

    let range = ParamRange::of_ensemble(&r.ensemble).expect("the ensemble's range");
    assert_eq!(bits(&slots(range.min())), bits(&lo), "minimum over the 50, per parameter");
    assert_eq!(bits(&slots(range.max())), bits(&hi), "maximum over the 50, per parameter");
    assert_eq!(ParamRange::over(ms.iter().copied()).expect("range"), range);
    let varying = (0..N_PARAMS).filter(|&k| lo[k] < hi[k]).count();
    assert!(varying > N_PARAMS / 2, "most parameters vary over the 50 ({varying})");

    let mut draws: Vec<Vec<f32>> = Vec::new();
    for m in 0..N_MODELS {
        let seed = control_seed(Control::RandomParams, m, 0);
        let x = slots(&random_params(&range, &mut Rng::seed_from_u64(seed)));
        assert_eq!(x.len(), N_PARAMS);
        // One `next_f32` per parameter, in row order, mapped onto lo..hi.
        let mut rng = Rng::seed_from_u64(seed);
        for k in 0..N_PARAMS {
            let u = rng.next_f32();
            assert!(x[k].is_finite() && lo[k] <= x[k] && x[k] <= hi[k], "model {m}, parameter {k}: {} outside {}..{}", x[k], lo[k], hi[k]);
            let want = lo[k] as f64 + (hi[k] as f64 - lo[k] as f64) * u as f64;
            let ulp = (want.abs() as f32).max(f32::MIN_POSITIVE) as f64 * f32::EPSILON as f64;
            assert!((x[k] as f64 - want).abs() <= ulp, "model {m}, parameter {k}: {} is not lo + (hi - lo) * u = {want}", x[k]);
        }
        draws.push(x);
    }
    // Uniform across the range: over the 50 draws each varying parameter
    // spreads, and the draws' mean sits near the middle.
    let mut centred = 0;
    for k in (0..N_PARAMS).filter(|&k| lo[k] < hi[k]) {
        let v: Vec<f64> = draws.iter().map(|d| ((d[k] - lo[k]) as f64) / ((hi[k] - lo[k]) as f64)).collect();
        let distinct: HashSet<u64> = v.iter().map(|x| x.to_bits()).collect();
        assert!(distinct.len() > 40, "parameter {k}: the 50 draws spread ({} distinct)", distinct.len());
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        if (mean - 0.5).abs() < 0.15 {
            centred += 1;
        }
    }
    assert!(centred as f64 > 0.95 * varying as f64, "means near the middle of the range: {centred} of {varying}");
}

// ---------------------------------------------------------------------------
// CONTROLS-05
// ---------------------------------------------------------------------------

/// CONTROLS-05: the rewired control keeps each connection type's target,
/// offsets, counts, sign and strength and draws a new presynaptic type;
/// edges are rebuilt from the new type's node in the same source column,
/// edges whose column it lacks are dropped and counted, and the network
/// passes `TypedNetwork::new`.
#[test]
fn controls_05_rewired_keeps_each_connection_types_edges_and_draws_new_sources() {
    let Some(r) = real("controls_05") else { return };
    let net = r.f.network();
    let mut at: HashMap<(u16, i16, i16), u32> = HashMap::new();
    for n in 0..net.num_nodes() {
        at.insert((net.node_cell_type()[n], net.node_u()[n], net.node_v()[n]), n as u32);
    }
    for m in [0usize, 1, 2, 49] {
        let seed = control_seed(Control::Rewired, m, 0);
        let w = rewire(net, &r.groups, &mut Rng::seed_from_u64(seed)).expect("a rewired network");
        let again = rewire(net, &r.groups, &mut Rng::seed_from_u64(seed)).expect("a rewired network");
        assert_eq!(w, again, "model {m}: the same seed rewires the same way");

        let new = &w.net;
        assert_eq!(new.cell_types(), net.cell_types(), "model {m}: cell types kept");
        assert_eq!(new.node_cell_type(), net.node_cell_type());
        assert_eq!((new.node_u(), new.node_v()), (net.node_u(), net.node_v()), "model {m}: nodes kept");
        assert_eq!(new.conn_types().len(), net.conn_types().len(), "model {m}: every connection type kept");
        assert_eq!(new.conn_sign(), net.conn_sign(), "model {m}: every sign kept");
        assert_eq!(w.sources.len(), net.conn_types().len());
        let mut changed = 0;
        for (c, (&(s, t), &(s0, t0))) in new.conn_types().iter().zip(net.conn_types()).enumerate() {
            assert_eq!(t, t0, "model {m}, connection type {c}: target kept");
            assert_eq!(s, w.sources[c], "model {m}, connection type {c}: the drawn source");
            assert!((s as usize) < net.cell_types().len());
            if s != s0 {
                changed += 1;
            }
        }
        assert!(changed > net.conn_types().len() / 2, "model {m}: new presynaptic types drawn ({changed} changed)");

        // Each surviving edge: rebuilt from its original, in the same source column.
        assert_eq!(w.kept.len(), new.num_edges());
        assert_eq!(w.groups.len(), new.num_edges());
        assert!(w.kept.windows(2).all(|k| k[0] < k[1]), "model {m}: edges in their original order");
        for (i, &e) in w.kept.iter().enumerate() {
            let e = e as usize;
            let c = net.edge_conn_type()[e];
            assert_eq!(new.edge_conn_type()[i], c, "model {m}, edge {i}: connection type (sign, strength) kept");
            assert_eq!(new.edge_target()[i], net.edge_target()[e], "model {m}, edge {i}: target kept");
            assert_eq!(new.edge_n_syn()[i].to_bits(), net.edge_n_syn()[e].to_bits(), "model {m}, edge {i}: count kept");
            assert_eq!(w.groups[i], r.groups[e], "model {m}, edge {i}: synapse-count group kept");
            let (src, old) = (new.edge_source()[i] as usize, net.edge_source()[e] as usize);
            assert_eq!(new.node_cell_type()[src], w.sources[c as usize], "model {m}, edge {i}: from the new type");
            assert_eq!((new.node_u()[src], new.node_v()[src]), (net.node_u()[old], net.node_v()[old]), "model {m}, edge {i}: same source column, so the same offset");
        }
        // Dropped: exactly the edges whose column the new type lacks.
        let lacking = (0..net.num_edges())
            .filter(|&e| {
                let c = net.edge_conn_type()[e] as usize;
                let old = net.edge_source()[e] as usize;
                !at.contains_key(&(w.sources[c], net.node_u()[old], net.node_v()[old]))
            })
            .count();
        assert_eq!(w.dropped, lacking, "model {m}: the dropped count");
        assert_eq!(w.dropped + w.kept.len(), net.num_edges(), "model {m}: every edge kept or dropped");
        eprintln!("rewired control, model {m}: {} edges dropped of {}", w.dropped, net.num_edges());

        // The result passes `TypedNetwork::new`, and takes the 604 strengths.
        let parts = TypedNetworkParts {
            cell_types: new.cell_types().to_vec(),
            node_cell_type: new.node_cell_type().to_vec(),
            node_u: new.node_u().to_vec(),
            node_v: new.node_v().to_vec(),
            conn_types: new.conn_types().to_vec(),
            conn_sign: new.conn_sign().to_vec(),
            edge_source: new.edge_source().to_vec(),
            edge_target: new.edge_target().to_vec(),
            edge_conn_type: new.edge_conn_type().to_vec(),
            edge_n_syn: new.edge_n_syn().to_vec(),
        };
        assert_eq!(&TypedNetwork::new(parts).expect("passes TypedNetwork::new"), new);
        let mut out = vec![0.0f32; new.num_edges()];
        new.weights_into(&ModelParams::pretrained().syn_strength, &mut out).expect("strengths per connection type");
    }
}

// ---------------------------------------------------------------------------
// CONTROLS-06
// ---------------------------------------------------------------------------

/// A network with `n` cell types, one node each at (0, 0), and connection
/// types `conn` (one edge each).
fn tiny(n: usize, conn: &[(u16, u16)]) -> TypedNetwork {
    TypedNetwork::new(TypedNetworkParts {
        cell_types: (0..n).map(|i| format!("T{i}")).collect(),
        node_cell_type: (0..n as u16).collect(),
        node_u: vec![0; n],
        node_v: vec![0; n],
        conn_types: conn.to_vec(),
        conn_sign: conn.iter().enumerate().map(|(i, _)| if i % 2 == 0 { 1.0 } else { -1.0 }).collect(),
        edge_source: conn.iter().map(|&(s, _)| s as u32).collect(),
        edge_target: conn.iter().map(|&(_, t)| t as u32).collect(),
        edge_conn_type: (0..conn.len() as u16).collect(),
        edge_n_syn: vec![1.0; conn.len()],
    })
    .expect("a valid tiny network")
}

/// CONTROLS-06: a rewiring draw that would give two connection types the
/// same (source, target) pair is redrawn.
#[test]
fn controls_06_a_draw_repeating_a_source_target_pair_is_redrawn() {
    // Two connection types onto the same target, over 3 cell types: find a
    // seed whose second draw repeats the first, and check it was redrawn
    // rather than kept or bumped to a neighbour.
    let net = tiny(3, &[(0, 2), (1, 2)]);
    let groups = [0u32, 1];
    let mut collided = 0;
    for seed in 0..200u64 {
        let w = rewire(&net, &groups, &mut Rng::seed_from_u64(seed)).expect("rewired");
        assert_ne!(w.sources[0], w.sources[1], "seed {seed}: two connection types share (source, target)");
        let mut rng = Rng::seed_from_u64(seed);
        let first = rng.below(3) as u16;
        let mut second = rng.below(3) as u16;
        if second == first {
            collided += 1;
            while second == first {
                second = rng.below(3) as u16;
            }
        }
        assert_eq!(w.sources, vec![first, second], "seed {seed}: a repeated pair is redrawn from the stream");
    }
    assert!(collided > 20, "the seeds exercised the redraw ({collided} collisions)");

    // Different targets may share a source.
    let shared = (0..200u64).any(|seed| {
        let w = rewire(&tiny(3, &[(0, 1), (0, 2)]), &groups, &mut Rng::seed_from_u64(seed)).unwrap();
        w.sources[0] == w.sources[1]
    });
    assert!(shared, "only the (source, target) pair must be unique");

    // No pair left: refused, never a duplicated pair.
    let e = rewire(&tiny(1, &[(0, 0), (0, 0)]), &groups, &mut Rng::seed_from_u64(1)).unwrap_err();
    assert!(matches!(e, ParityError::Usage(_)), "{e:?}");

    let Some(r) = real("controls_06") else { return };
    let net = r.f.network();
    let mut redrawn = 0;
    for m in 0..N_MODELS {
        let w = rewire(net, &r.groups, &mut Rng::seed_from_u64(control_seed(Control::Rewired, m, 0))).expect("rewired");
        let pairs: HashSet<(u16, u16)> = w.net.conn_types().iter().copied().collect();
        assert_eq!(pairs.len(), net.conn_types().len(), "model {m}: every (source, target) pair is unique");
        // Replaying the stream: how many draws hit a pair already held.
        let mut rng = Rng::seed_from_u64(control_seed(Control::Rewired, m, 0));
        let mut taken = HashSet::new();
        for (c, &(_, t)) in net.conn_types().iter().enumerate() {
            loop {
                let s = rng.below(net.cell_types().len()) as u16;
                if taken.insert((s, t)) {
                    assert_eq!(s, w.sources[c], "model {m}, connection type {c}");
                    break;
                }
                redrawn += 1;
            }
        }
    }
    assert!(redrawn > 0, "the real network's draws repeated some pair and were redrawn");
    eprintln!("rewired control: {redrawn} draws redrawn over the 50 models");
}

// ---------------------------------------------------------------------------
// CONTROLS-07
// ---------------------------------------------------------------------------

/// CONTROLS-07: `respond_network` responds on a rewired topology with the
/// torch synapse counts following each edge (exactness unaffected); it is
/// control-only, and `respond` keeps taking the 734 parameters over the
/// fixed topology.
#[test]
fn controls_07_respond_network_responds_on_a_rewired_topology_with_counts_following_each_edge() {
    // `respond` keeps its fixed-topology signature: no network, 734 parameters.
    let _: RespondFn = respond;
    let p = ModelParams::pretrained();
    assert_eq!(p.bias.len() + p.time_const.len() + p.syn_strength.len(), N_PARAMS);

    let Some(r) = real("controls_07") else { return };
    let f = &r.f;
    let net = f.network();
    let dt = coarse_dt(f);
    let grey = f.manifest().pre_stim_value;

    // On the fixtures' own network and groups it is `respond`, bit for bit.
    for prec in [Dtype::F32, Dtype::F64] {
        let fixed = respond(f, &p, dt, prec, Counts::Torch, &mut small(grey)).expect("respond");
        let same = respond_network(f, net, &r.groups, &p, dt, prec, Counts::Torch, &mut small(grey)).expect("respond_network");
        assert_eq!(bits_f64(&all_values(&same)), bits_f64(&all_values(&fixed)), "{prec:?}: exactness unaffected");
        assert_eq!(same, fixed, "{prec:?}");
    }

    // On a rewired topology.
    let fixed = respond(f, &p, dt, Dtype::F32, Counts::Torch, &mut small(grey)).unwrap();
    let d = draw_stable(Control::Rewired, 0, |seed| {
        let w = rewire(net, &r.groups, &mut Rng::seed_from_u64(seed))?;
        let resp = respond_network(f, &w.net, &w.groups, &p, dt, Dtype::F32, Counts::Torch, &mut small(grey))?;
        Ok((w, resp))
    })
    .expect("a stable rewired draw");
    let (w, rewired) = &d.value;
    for rd in &d.redraws {
        eprintln!("rewired control, model 0: attempt {} (seed {:#x}) redrawn: {}", rd.attempt, rd.seed, rd.error);
    }
    assert_eq!(rewired.cell_types(), fixed.cell_types(), "the same central cell types");
    assert_eq!(rewired.len(), fixed.len());
    let (a, b) = (all_values(rewired), all_values(&fixed));
    assert!(a.iter().all(|v| v.is_finite()));
    assert_ne!(bits_f64(&a), bits_f64(&b), "it responded on the rewired topology, not the fixed one");
    // Repeatable bit for bit: exactness is a property of the run, not the topology.
    let again =
        respond_network(f, &w.net, &w.groups, &p, dt, Dtype::F32, Counts::Torch, &mut small(grey)).expect("again");
    assert_eq!(bits_f64(&all_values(&again)), bits_f64(&a), "the same rewired draw responds the same");

    // The counts follow each edge: the carried groups are the ones used.
    assert!(w.kept.iter().zip(&w.groups).all(|(&e, &g)| r.groups[e as usize] == g));
    let mut shifted = w.groups.clone();
    shifted.rotate_left(1);
    let wrong =
        respond_network(f, &w.net, &shifted, &p, dt, Dtype::F32, Counts::Torch, &mut small(grey)).expect("still runs");
    assert_ne!(bits_f64(&all_values(&wrong)), bits_f64(&a), "the per-edge groups decide the counts");

    // Control-only inputs are checked, never guessed.
    let short = &w.groups[..w.groups.len() - 1];
    let e = respond_network(f, &w.net, short, &p, dt, Dtype::F32, Counts::Torch, &mut small(grey)).unwrap_err();
    assert!(matches!(e, ParityError::Usage(_)), "{e:?}");
    let mut past = w.groups.clone();
    past[0] = u32::MAX;
    let e = respond_network(f, &w.net, &past, &p, dt, Dtype::F32, Counts::Torch, &mut small(grey)).unwrap_err();
    assert!(matches!(e, ParityError::Usage(_)), "{e:?}");
    let other_nodes = tiny(net.cell_types().len(), &[(0, 1)]);
    let e = respond_network(f, &other_nodes, &[0], &p, dt, Dtype::F32, Counts::Torch, &mut small(grey)).unwrap_err();
    assert!(matches!(e, ParityError::Usage(_)), "{e:?}");
}

/// The signature of respond: the fixtures' network, 734 parameters, no topology.
type RespondFn = fn(&Fixtures, &ModelParams, f64, Dtype, Counts, &mut dyn Battery) -> Result<Responses, ParityError>;

fn bits_f64(v: &[f64]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

// ---------------------------------------------------------------------------
// CONTROLS-08
// ---------------------------------------------------------------------------

fn non_finite() -> ParityError {
    ParityError::NonFiniteSettle { node: 3, cell_type: "L1".into(), step: 7 }
}

/// CONTROLS-08: a draw whose state goes non-finite is redrawn with the next
/// seed up to 10 times, each redraw recorded; ten redraws in a row all
/// non-finite is a gate C failure.
#[test]
fn controls_08_a_non_finite_draw_is_redrawn_up_to_ten_times_then_fails_gate_c() {
    assert_eq!(MAX_REDRAWS, 10);
    for c in [Control::Shuffled, Control::RandomParams, Control::Rewired] {
        let m = 11;
        // Three non-finite attempts, then a stable one: redrawn with the next seeds.
        let mut seeds = Vec::new();
        let d = draw_stable(c, m, |seed| {
            seeds.push(seed);
            if seeds.len() <= 3 { Err(non_finite()) } else { Ok(seed) }
        })
        .expect("stable on the fourth attempt");
        let want: Vec<u64> = (0..4).map(|a| control_seed(c, m, a)).collect();
        assert_eq!(seeds, want, "{c:?}: each redraw takes the next seed");
        assert_eq!((d.attempt, d.seed, d.value), (3, want[3], want[3]));
        assert_eq!(d.redraws.len(), 3, "{c:?}: every redraw recorded");
        for (a, rd) in d.redraws.iter().enumerate() {
            assert_eq!((rd.attempt, rd.seed), (a as u32, want[a]));
            assert_eq!(rd.error, non_finite());
        }

        // The tenth redraw stable: still a draw.
        let mut n = 0;
        let d = draw_stable(c, m, |seed| {
            n += 1;
            if n <= 10 { Err(non_finite()) } else { Ok(seed) }
        })
        .expect("the tenth redraw is still allowed");
        assert_eq!((d.attempt, d.redraws.len()), (10, 10));

        // Every redraw non-finite: a gate C failure, naming every seed.
        let mut calls = 0;
        let e = draw_stable(c, m, |_| {
            calls += 1;
            Err::<(), _>(non_finite())
        })
        .unwrap_err();
        assert_eq!(calls, 1 + MAX_REDRAWS, "{c:?}: the original and up to 10 redraws");
        assert!(e.is_gate_failure(), "{c:?}: {e}");
        let DrawError::Unstable { control, model, redraws } = &e else { panic!("{e:?}") };
        assert_eq!((*control, *model, redraws.len()), (c, m, 1 + MAX_REDRAWS as usize));
        let text = e.to_string();
        for a in 0..=MAX_REDRAWS {
            assert!(text.contains(&format!("{:#x}", control_seed(c, m, a))), "the failure names seed {a}: {text}");
        }

        // A non-numerical error is not redrawn and is not a gate failure.
        let e = draw_stable(c, m, |_| Err::<(), _>(ParityError::Usage("bad".into()))).unwrap_err();
        assert!(!e.is_gate_failure(), "{e:?}");
    }

    // Simulated: a real draw that blows up is redrawn with the next seed.
    let Some(r) = real("controls_08") else { return };
    let f = &r.f;
    let dt = coarse_dt(f);
    let grey = f.manifest().pre_stim_value;
    let mut blown = ModelParams::pretrained();
    for s in blown.syn_strength.iter_mut() {
        *s = 1e15;
    }
    let first = control_seed(Control::RandomParams, 0, 0);
    let d = draw_stable(Control::RandomParams, 0, |seed| {
        let p = if seed - first < 2 { blown.clone() } else { ModelParams::pretrained() };
        respond(f, &p, dt, Dtype::F32, Counts::Torch, &mut small(grey))
    })
    .expect("stable once the strengths are sane");
    assert_eq!((d.attempt, d.seed), (2, control_seed(Control::RandomParams, 0, 2)));
    assert_eq!(d.redraws.len(), 2);
    for rd in &d.redraws {
        assert!(rd.error.is_gate_failure(), "a non-finite state: {:?}", rd.error);
        eprintln!("random parameters control, model 0: attempt {} (seed {:#x}) redrawn: {}", rd.attempt, rd.seed, rd.error);
    }
}
