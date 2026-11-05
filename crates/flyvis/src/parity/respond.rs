//! Responses for any network: settle it as the 0.3.0 gates do, play a
//! battery from the settled state, and record each cell type's central
//! value at every frame.
//!
//! Spec: docs/superpowers/specs/2026-11-04-graded-responses-design.md,
//! section 2.

use semblance_sim::response::{Precision as Prec, Responses};

use super::run::{cell_type, file, mismatch, settle_core, step_frame, Ctx, Precision, HEXALS};
use super::{Counts, Fixtures, ParityError, TraceInputs};
use crate::ensemble::ModelParams;
use crate::reference::{BlockKey, Dtype};

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
    match precision {
        Dtype::F32 => respond_as::<f32>(f, params, dt, counts, battery),
        Dtype::F64 => respond_as::<f64>(f, params, dt, counts, battery),
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
    let (mut g, _) = settle_core::<R>(f, params, dt, steps.round() as usize, counts, &[], Ctx::Respond)?;
    let settled = g.state().to_vec();

    let types: Vec<String> = centre.iter().map(|&k| cell_type(&f.net, k).to_string()).collect();
    let tag = match R::DTYPE {
        Dtype::F32 => Prec::F32,
        Dtype::F64 => Prec::F64,
    };
    let mut out = Responses::new(types, dt, tag).map_err(|e| mismatch(e.to_string()))?;

    // Zeroed once, as `play_from`: every frame overwrites the same input nodes.
    let mut input = vec![R::ZERO; f.net.num_nodes()];
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
                let cell_type = cell_type(&f.net, k).to_string();
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
}
