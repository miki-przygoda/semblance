//! Responses for any network: settle it as the 0.3.0 gates do, play a
//! battery from the settled state, and record each cell type's central
//! value at every frame.
//!
//! Spec: docs/superpowers/specs/2026-11-04-graded-responses-design.md,
//! section 2.

use semblance_connectome::TypedNetwork;
use semblance_sim::response::{Precision as Prec, Responses};

use super::run::{cell_type, file, mismatch, settle_core, step_frame, Ctx, Precision, HEXALS};
use super::{Counts, Fixtures, ParityError, TraceInputs};
use crate::ensemble::ModelParams;
use crate::reference::{BlockKey, Dtype};
use crate::syn_count_torch::SYN_COUNT_TORCH_F32;

/// For each cell type, in network order, its node at `(u, v) = (0, 0)`:
/// the central column flyvis's indices read. Derived from the network, so
/// scoring needs no reference traces, and checked against the manifest's
/// `centre_nodes`, which must name the same nodes in the same order.
pub fn central_nodes(f: &Fixtures) -> Result<Vec<usize>, ParityError> {
    let net = &f.net;
    let mut found: Vec<Option<usize>> = vec![None; net.cell_types().len()];
    let at_origin = net.node_u().iter().zip(net.node_v()).zip(net.node_cell_type()).enumerate();
    for (node, ((&u, &v), &t)) in at_origin {
        if u == 0 && v == 0 && found[t as usize].replace(node).is_some() {
            return Err(mismatch(format!("cell type {} has more than one node at (0, 0)", net.cell_types()[t as usize])));
        }
    }
    let nodes = found
        .iter()
        .enumerate()
        .map(|(t, n)| n.ok_or_else(|| mismatch(format!("cell type {} has no node at (0, 0)", net.cell_types()[t]))))
        .collect::<Result<Vec<usize>, ParityError>>()?;
    let manifest: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    if manifest != nodes {
        return Err(mismatch(format!(
            "the network's central nodes {nodes:?} differ from the manifest's centre_nodes {manifest:?}"
        )));
    }
    Ok(nodes)
}

/// A battery of stimuli, handed over one at a time: the full moving-edge
/// battery's input is about 900 MB if held at once.
pub trait Battery {
    /// How many stimuli `next_stimulus` will yield.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Writes the next stimulus's frames into `out` (cleared first; rows of
    /// 721 hexal values in `hex_coords` order) and returns its label, or
    /// `None` when the battery is done. The frames must have been rendered at
    /// the `dt` passed to `respond`; nothing checks it.
    fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError>;
}

/// Settles `params` on the fixtures' network at `dt` and `precision`, as the
/// 0.3.0 gates settle, then plays every stimulus of `battery` from the
/// settled state and records each cell type's central value at every frame.
/// The battery's frames must have been rendered at this `dt`; nothing checks
/// it.
pub fn respond(
    f: &Fixtures,
    params: &ModelParams,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    respond_on(f, &f.net, &f.groups, params, dt, precision, counts, battery)
}

/// `respond` on another topology over the fixtures' nodes: gate C's rewired
/// control, and nothing else (owner's answer Q5; decision 6 keeps `respond`
/// on the fixed topology). `groups[e]` is edge `e`'s synapse-count group,
/// carried from the edge it was rebuilt from, so torch's counts follow each
/// edge and exactness is unaffected. Refuses, as `Usage`, a network whose
/// cell types or nodes differ from the fixtures' network, groups of the
/// wrong length, and a group past the 2355 tabled.
#[allow(clippy::too_many_arguments)]
pub fn respond_network(
    f: &Fixtures,
    net: &TypedNetwork,
    groups: &[u32],
    params: &ModelParams,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    let same_nodes = net.cell_types() == f.net.cell_types()
        && net.node_cell_type() == f.net.node_cell_type()
        && net.node_u() == f.net.node_u()
        && net.node_v() == f.net.node_v();
    if !same_nodes {
        return Err(ParityError::Usage("respond_network: the network's nodes differ from the fixtures' network".into()));
    }
    if groups.len() != net.num_edges() {
        return Err(ParityError::Usage(format!(
            "respond_network: {} synapse-count groups for {} edges",
            groups.len(),
            net.num_edges()
        )));
    }
    if let Some((e, &g)) = groups.iter().enumerate().find(|&(_, &g)| g as usize >= SYN_COUNT_TORCH_F32.len()) {
        return Err(ParityError::Usage(format!(
            "respond_network: edge {e} names synapse-count group {g}, but only {} exist",
            SYN_COUNT_TORCH_F32.len()
        )));
    }
    respond_on(f, net, groups, params, dt, precision, counts, battery)
}

#[allow(clippy::too_many_arguments)]
fn respond_on(
    f: &Fixtures,
    net: &TypedNetwork,
    groups: &[u32],
    params: &ModelParams,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    match precision {
        Dtype::F32 => respond_as::<f32>(f, net, groups, params, dt, counts, battery),
        Dtype::F64 => respond_as::<f64>(f, net, groups, params, dt, counts, battery),
    }
}

/// `respond` for an ensemble model, by id (`"000"` to `"049"`).
pub fn respond_model(
    f: &Fixtures,
    id: &str,
    dt: f64,
    precision: Dtype,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    let p = f.ensemble.model(id).ok_or_else(|| ParityError::Usage(format!("model {id} is not in the ensemble")))?;
    respond(f, p, dt, precision, counts, battery)
}

/// The most settling steps `respond` will run: the 0.3.0 runs take 200 and 50.
const MAX_SETTLE_STEPS: f64 = 1e6;

fn respond_as<R: Precision>(
    f: &Fixtures,
    net: &TypedNetwork,
    groups: &[u32],
    params: &ModelParams,
    dt: f64,
    counts: Counts,
    battery: &mut dyn Battery,
) -> Result<Responses, ParityError> {
    let t_pre = f.manifest.t_pre;
    let steps = t_pre / dt;
    if !(dt.is_finite() && dt > 0.0 && steps.is_finite()
            && (1.0..=MAX_SETTLE_STEPS).contains(&steps)
            && (steps - steps.round()).abs() <= 1e-9) {
        return Err(ParityError::Usage(format!("dt {dt} does not divide t_pre {t_pre} into 1 to {MAX_SETTLE_STEPS} whole settling steps")));
    }
    let centre = central_nodes(f)?;
    let (mut g, _) = settle_core::<R>(f, net, groups, params, dt, steps.round() as usize, counts, &[], Ctx::Respond)?;
    let settled = g.state().to_vec();

    let types: Vec<String> = centre.iter().map(|&k| cell_type(net, k).to_string()).collect();
    let tag = match R::DTYPE {
        Dtype::F32 => Prec::F32,
        Dtype::F64 => Prec::F64,
    };
    let mut out = Responses::new(types, dt, tag).map_err(|e| mismatch(e.to_string()))?;

    // Zeroed once, as `play_from`: every frame overwrites the same input nodes.
    let mut input = vec![R::ZERO; net.num_nodes()];
    let mut frame = vec![R::ZERO; HEXALS];
    let mut frames = Vec::new();
    let mut s = 0usize;
    loop {
        frames.clear();
        let Some(label) = battery.next_stimulus(&mut frames)? else { break };
        if frames.is_empty() || frames.len() % HEXALS != 0 {
            return Err(ParityError::Usage(format!(
                "stimulus {s} ({label}): {} values is not a whole number of {HEXALS}-hexal frames",
                frames.len()
            )));
        }
        g.set_state(&settled).map_err(|e| mismatch(e.to_string()))?;
        let n_frames = frames.len() / HEXALS;
        let mut values = Vec::with_capacity(n_frames * centre.len());
        for (i, row) in frames.chunks_exact(HEXALS).enumerate() {
            for (slot, &v) in frame.iter_mut().zip(row) {
                *slot = R::from_f64(v);
            }
            if let Some(k) = step_frame(f, &mut g, &frame, &mut input).map_err(mismatch)? {
                let cell_type = cell_type(net, k).to_string();
                return Err(ParityError::NonFiniteResponse { stimulus: s, label, frame: i, node: k, cell_type });
            }
            values.extend(centre.iter().map(|&k| g.state()[k].to_f64()));
        }
        out.push(label, n_frames, values).map_err(|e| mismatch(e.to_string()))?;
        s += 1;
    }
    if s != battery.len() {
        return Err(mismatch(format!("the battery said {} stimuli and yielded {s}", battery.len())));
    }
    out.finish().map_err(|e| ParityError::Usage(e.to_string()))
}

/// flyvis's reference input as a battery: the manifest's sequences at one
/// dt, in index order, labelled `"{index} {family}"`.
pub struct ReferenceBattery<'a> {
    f: &'a Fixtures,
    t: &'a TraceInputs<'a>,
    dt: String,
    next: usize,
}

impl<'a> ReferenceBattery<'a> {
    pub fn new(f: &'a Fixtures, t: &'a TraceInputs<'a>, dt_id: &str) -> Self {
        ReferenceBattery { f, t, dt: dt_id.to_string(), next: 0 }
    }
}

impl Battery for ReferenceBattery<'_> {
    fn len(&self) -> usize {
        self.t.sequences.len()
    }

    fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError> {
        let Some(q) = self.t.sequences.get(self.next) else { return Ok(None) };
        self.next += 1;
        if q.index as usize != self.next - 1 {
            return Err(mismatch(format!(
                "manifest sequences must be listed in index order: entry {} is sequence {}",
                self.next - 1,
                q.index
            )));
        }
        let n = q
            .frames(&self.dt)
            .ok_or_else(|| mismatch(format!("sequence {} has no frames at dt {}", q.index, self.dt)))?
            .n_frames;
        let key = BlockKey { run: None, dt: &self.dt, sequence: Some(q.index) };
        let x = file(self.f, "input.f64.bin")?.read_f64(self.t.input, &key).map_err(|e| ParityError::Malformed {
            rel: "flyvis-reference/input.f64.bin".into(),
            detail: e.to_string(),
        })?;
        if x.shape != [n, HEXALS] {
            return Err(mismatch(format!(
                "input shape, sequence {}: expected [{n}, {HEXALS}], found {:?}",
                q.index, x.shape
            )));
        }
        out.clear();
        out.extend_from_slice(&x.data);
        Ok(Some(format!("{} {}", q.index, q.family)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_support::fixture;
    use crate::parity::{Counts, TraceInputs};
    use crate::reference::{parse_sequences, BlockKey, Dtype, SequenceInfo};
    use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text};
    use std::sync::OnceLock;
    use semblance_connectome::TypedNetworkParts;
    use crate::syn_count_torch::SYN_COUNT_TORCH_F32;

    /// A battery from a fixed list of frame buffers, for the error paths.
    struct Fixed {
        stimuli: Vec<Vec<f64>>,
        claimed: usize,
        next: usize,
    }

    impl Battery for Fixed {
        fn len(&self) -> usize {
            self.claimed
        }
        fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError> {
            let Some(s) = self.stimuli.get(self.next) else { return Ok(None) };
            self.next += 1;
            out.clear();
            out.extend_from_slice(s);
            Ok(Some(format!("fixed {}", self.next - 1)))
        }
    }

    fn grey(frames: usize) -> Vec<f64> {
        vec![0.5; frames * HEXALS]
    }

    #[test]
    fn a_stimulus_that_is_not_whole_frames_is_a_usage_error() {
        let Some(f) = fixture() else { return };
        for bad in [Vec::new(), vec![0.5; HEXALS + 1]] {
            let mut b = Fixed { stimuli: vec![bad], claimed: 1, next: 0 };
            let e = respond_model(f, "000", 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
            assert!(matches!(e, ParityError::Usage(ref m) if m.contains("whole number")), "{e}");
        }
    }

    #[test]
    fn a_dt_that_does_not_divide_t_pre_is_a_usage_error() {
        let Some(f) = fixture() else { return };
        for dt in [0.003, 0.0, -0.005, f64::NAN, 1e-12] {
            let mut b = Fixed { stimuli: vec![grey(2)], claimed: 1, next: 0 };
            let e = respond_model(f, "000", dt, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
            assert!(matches!(e, ParityError::Usage(_)), "dt {dt}: {e}");
        }
    }

    #[test]
    fn a_reference_battery_over_reordered_sequences_is_a_mismatch() {
        let Some(f) = fixture() else { return };
        let Some((mut sequences, input, centre)) = reference("f32") else { return };
        sequences.swap(0, 1);
        let t = TraceInputs { precision: Dtype::F32, sequences: &sequences, input, centre: &centre, snapshots: &[] };
        let mut b = ReferenceBattery::new(f, &t, "1/200");
        let e = b.next_stimulus(&mut Vec::new()).unwrap_err();
        assert!(matches!(e, ParityError::Mismatch(ref m) if m.contains("index order")), "{e}");
    }

    #[test]
    fn a_battery_that_yields_fewer_than_it_claims_is_a_mismatch() {
        let Some(f) = fixture() else { return };
        let mut b = Fixed { stimuli: vec![grey(2)], claimed: 2, next: 0 };
        let e = respond_model(f, "000", 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
        assert!(matches!(e, ParityError::Mismatch(ref m) if m.contains("said 2")), "{e}");
    }

    #[test]
    fn an_unknown_model_and_bad_parameters_are_usage_errors() {
        let Some(f) = fixture() else { return };
        let mut b = Fixed { stimuli: vec![grey(2)], claimed: 1, next: 0 };
        let e = respond_model(f, "050", 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.contains("050")), "{e}");
        let mut p = f.ensemble.model("000").unwrap().clone();
        p.time_const[0] = f32::NAN;
        let mut b = Fixed { stimuli: vec![grey(2)], claimed: 1, next: 0 };
        let e = respond(f, &p, 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.starts_with("network: ")), "{e}");
    }

    #[test]
    fn grey_responses_have_the_batterys_shape() {
        let Some(f) = fixture() else { return };
        let mut b = Fixed { stimuli: vec![grey(3), grey(1)], claimed: 2, next: 0 };
        let r = respond_model(f, "000", 1.0 / 200.0, Dtype::F64, Counts::Torch, &mut b).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.len(), 2);
        assert_eq!(r.cell_types().len(), f.network().cell_types().len());
        assert_eq!(r.precision(), semblance_sim::response::Precision::F64);
        assert_eq!((r.stimulus(0).unwrap().n_frames(), r.stimulus(1).unwrap().n_frames()), (3, 1));
        assert_eq!(r.stimulus(1).unwrap().label(), "fixed 1");
    }

    /// The reference's sequences, input and centre traces for one precision.
    fn reference(suffix: &str) -> Option<(Vec<SequenceInfo>, &'static [u8], Vec<u8>)> {
        static INPUT: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();
        let input = or_skip(
            INPUT
                .get_or_init(|| read_fixture_bytes("flyvis-reference/input.f64.bin"))
                .as_ref()
                .map(Vec::as_slice)
                .map_err(String::clone),
        )?;
        let text = or_skip(read_fixture_text("flyvis-reference/manifest.json"))?;
        let sequences = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let centre = or_skip(read_fixture_bytes(&format!("flyvis-reference/centre.{suffix}.bin")))?;
        Some((sequences, input, centre))
    }

    /// `respond` reproduces run `run`'s reference centre traces: model
    /// `model` at the run's dt and precision, torch counts, from the
    /// Rust-settled state, every sequence, frame and type. Returns how many
    /// values differ and the largest difference.
    fn reproduces<R: crate::parity::Precision>(run: u32, model: &str) -> Option<(usize, f64)> {
        let f = fixture()?;
        let (sequences, input, centre) = reference(R::SUFFIX)?;
        let info = &f.manifest.runs[run as usize];
        assert_eq!(info.precision, R::DTYPE, "run {run}'s registered precision");
        let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("the run's dt is listed").dt;
        let t = TraceInputs { precision: R::DTYPE, sequences: &sequences, input, centre: &centre, snapshots: &[] };
        let mut battery = ReferenceBattery::new(f, &t, &info.dt);
        let r = respond_model(f, model, dt, R::DTYPE, Counts::Torch, &mut battery).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.len(), sequences.len());
        let n_types = r.cell_types().len();
        let centre_file = file(f, &format!("centre.{}.bin", R::SUFFIX)).unwrap();
        let (mut differ, mut max) = (0usize, 0.0f64);
        for (s, q) in sequences.iter().enumerate() {
            let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(q.index) };
            let flyvis = R::read(centre_file, &centre, &key).unwrap();
            let got = r.stimulus(s).unwrap();
            assert_eq!(flyvis.shape, [got.n_frames(), n_types], "sequence {}", q.index);
            for (i, want) in flyvis.data.iter().enumerate() {
                let want = want.to_f64();
                assert_ne!(want, 0.0, "a ±0.0 reference value makes == more than bit equality");
                let d = (got.frame(i / n_types).unwrap()[i % n_types] - want).abs();
                if d != 0.0 {
                    differ += 1;
                    max = max.max(d);
                }
            }
        }
        eprintln!(
            "respond run {run} ({model}, dt {}, {}): {differ} values differ from flyvis's centre traces, largest {max:.3e}",
            info.dt,
            R::SUFFIX
        );
        Some((differ, max))
    }

    #[test]
    fn respond_reproduces_flyviss_centre_traces_at_1_200() {
        for (differ, _) in [reproduces::<f32>(0, "000"), reproduces::<f64>(1, "000")].into_iter().flatten() {
            assert_eq!(differ, 0);
        }
    }

    #[test]
    fn respond_reproduces_flyviss_centre_traces_at_1_50() {
        for (differ, _) in [reproduces::<f32>(2, "000"), reproduces::<f64>(3, "000")].into_iter().flatten() {
            assert_eq!(differ, 0);
        }
    }

    /// The control: model 049 played with run 0's settings misses run 0's
    /// traces by more than `CONTROL_MISS`, so the reproduction above can fail.
    #[test]
    fn another_models_responses_do_not_reproduce_run_0() {
        if let Some((_, max)) = reproduces::<f32>(0, "049") {
            assert!(max > crate::parity::CONTROL_MISS, "model 049 came within {max:e} of model 000's traces");
        }
    }

    #[test]
    fn the_central_nodes_are_the_manifests_centre_nodes() {
        let Some(f) = fixture() else { return };
        let nodes = central_nodes(f).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(nodes.len(), f.network().cell_types().len());
        for (k, c) in f.manifest().centre_nodes.iter().enumerate() {
            assert_eq!(f.network().cell_types()[f.network().node_cell_type()[nodes[k]] as usize], c.cell_type);
        }
    }

    /// A network's parts, for building a variant of it.
    fn parts(net: &TypedNetwork) -> TypedNetworkParts {
        TypedNetworkParts {
            cell_types: net.cell_types().to_vec(),
            node_cell_type: net.node_cell_type().to_vec(),
            node_u: net.node_u().to_vec(),
            node_v: net.node_v().to_vec(),
            conn_types: net.conn_types().to_vec(),
            conn_sign: net.conn_sign().to_vec(),
            edge_source: net.edge_source().to_vec(),
            edge_target: net.edge_target().to_vec(),
            edge_conn_type: net.edge_conn_type().to_vec(),
            edge_n_syn: net.edge_n_syn().to_vec(),
        }
    }

    /// `respond_network` on the fixtures' own network and groups is
    /// `respond`, bit for bit, at both precisions.
    #[test]
    fn respond_network_on_the_fixtures_own_network_is_respond() {
        let Some(f) = fixture() else { return };
        let p = f.ensemble.model("000").unwrap();
        let flash: Vec<f64> = (0..2 * HEXALS).map(|i| (i % 2) as f64).collect();
        for precision in [Dtype::F32, Dtype::F64] {
            let battery = || Fixed { stimuli: vec![grey(3), flash.clone()], claimed: 2, next: 0 };
            let a = respond(f, p, 1.0 / 200.0, precision, Counts::Torch, &mut battery()).unwrap_or_else(|e| panic!("{e}"));
            let b = respond_network(f, &f.net, &f.groups, p, 1.0 / 200.0, precision, Counts::Torch, &mut battery())
                .unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(a.len(), b.len());
            for s in 0..a.len() {
                let (x, y) = (a.stimulus(s).unwrap(), b.stimulus(s).unwrap());
                assert_eq!((x.label(), x.n_frames()), (y.label(), y.n_frames()));
                for i in 0..x.n_frames() {
                    let bits = |row: &[f64]| row.iter().map(|v| v.to_bits()).collect::<Vec<u64>>();
                    assert_eq!(bits(x.frame(i).unwrap()), bits(y.frame(i).unwrap()), "{precision:?}, stimulus {s}, frame {i}");
                }
            }
        }
    }

    #[test]
    fn respond_network_refuses_a_network_whose_nodes_differ() {
        let Some(f) = fixture() else { return };
        let mut q = parts(&f.net);
        q.node_u[0] += 1;
        let moved = TypedNetwork::new(q).unwrap();
        let mut b = Fixed { stimuli: vec![grey(1)], claimed: 1, next: 0 };
        let p = ModelParams::pretrained();
        let e = respond_network(f, &moved, &f.groups, &p, 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
        assert!(matches!(e, ParityError::Usage(ref m) if m.contains("nodes differ")), "{e}");
    }

    #[test]
    fn respond_network_refuses_synapse_count_groups_that_do_not_fit_without_panicking() {
        let Some(f) = fixture() else { return };
        let p = ModelParams::pretrained();
        let mut short = f.groups.clone();
        short.pop();
        let mut past = f.groups.clone();
        past[0] = SYN_COUNT_TORCH_F32.len() as u32;
        for (groups, expect) in [(short, "groups for"), (past, "names synapse-count group")] {
            let mut b = Fixed { stimuli: vec![grey(1)], claimed: 1, next: 0 };
            let e = respond_network(f, &f.net, &groups, &p, 1.0 / 200.0, Dtype::F32, Counts::Torch, &mut b).unwrap_err();
            assert!(matches!(e, ParityError::Usage(ref m) if m.contains(expect)), "{e}");
        }
    }
}
