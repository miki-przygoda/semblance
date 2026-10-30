//! Settling, trace playback and comparison: piece 1's and piece 2's gate
//! machinery, moved here without changing what it computes.
//!
//! One core (spec R20): `settle_state` builds and settles the integrator,
//! `compare_settling` compares the settled state, and `play_from` plays
//! sequences from a settled state. `settle`, `play` and `run` are thin over
//! them, so the gates exercise exactly the code `validate` runs. Each entry
//! point is non-generic and dispatches on the run's registered precision
//! (`RUNS`, R19), so the integrator is monomorphised in this crate.

use semblance_connectome::TypedNetwork;
use semblance_sim::graded::{Graded, GradedParams, Real};

use super::{After, Fixtures, ParityError, TraceInputs, RUNS};
use crate::expand::{hex_coords, DEFAULT_EXTENT};
use crate::pretrained::SYN_COUNT;
use crate::reference::{Array, BlockKey, DtInfo, Dtype, FileInfo, ReferenceError, RunInfo};
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use crate::wiring::scatter_hexals;

pub(crate) const HEXALS: usize = 721;

/// Snapshot frames per sequence, at every dt (`snapshots.*.bin`'s first axis).
pub(crate) const SNAPSHOTS: usize = 6;

/// Where the per-edge synapse counts come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counts {
    /// flyvis's own: torch's `exp`, from `syn_count_torch.rs`.
    Torch,
    /// Rust's `exp` of `SYN_COUNT`.
    Libm,
    /// The spec's integer counts, never exponentiated: a negative control.
    IntegerNSyn,
}

/// Which settled state each sequence starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Rust's own settling: settling and stimulus, end to end.
    Rust,
    /// flyvis's `settled.<p>.bin`: the first diagnostic step, isolating the
    /// stimulus phase.
    Flyvis,
}

/// The largest absolute error, where, and how many values are not equal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diff {
    pub max: f64,
    pub at: usize,
    pub differ: usize,
}

/// A NaN on either side counts as an infinite error, so it becomes the
/// maximum and fails every bound, instead of being skipped by `>`. The two
/// slices must be the same length: every caller has checked the shapes.
pub fn diff(rust: &[f64], flyvis: &[f64]) -> Diff {
    assert_eq!(rust.len(), flyvis.len());
    let mut d = Diff { max: 0.0, at: 0, differ: 0 };
    for (i, (a, b)) in rust.iter().zip(flyvis).enumerate() {
        let e = (a - b).abs();
        let e = if e.is_nan() { f64::INFINITY } else { e };
        d.differ += usize::from(a != b);
        if e > d.max {
            d.max = e;
            d.at = i;
        }
    }
    d
}

/// One run's settling, compared with flyvis's.
#[derive(Clone, Debug, PartialEq)]
pub struct SettlingResult {
    pub run: u32,
    pub model: String,
    pub counts: Counts,
    /// Over `[n_nodes]`.
    pub settled: Diff,
    /// Over `[n_settle, 65]`.
    pub settling_centre: Diff,
    pub settled_len: usize,
    pub centre_len: usize,
    /// The first unequal settling-centre value, as an index into [n_settle, 65].
    pub first_centre_difference: Option<usize>,
    /// The settled state's largest |error| per cell type, in the network's
    /// cell-type order.
    pub per_type_max: Vec<f64>,
}

impl SettlingResult {
    /// The `n` cell types with the largest settled error, largest first; a
    /// tie keeps cell-type order.
    pub fn worst_types(&self, n: usize) -> Vec<usize> {
        let mut worst: Vec<usize> = (0..self.per_type_max.len()).collect();
        worst.sort_by(|&a, &b| self.per_type_max[b].total_cmp(&self.per_type_max[a]));
        worst.truncate(n);
        worst
    }
}

/// One sequence played and compared.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceResult {
    pub sequence: u32,
    pub family: String,
    pub n_frames: usize,
    pub snapshot_frames: Vec<usize>,
    /// Over `[n_frames, 65]`.
    pub centre: Diff,
    /// Over `[6, n_nodes]`.
    pub snapshots: Diff,
    /// The first unequal centre value, as an index into [n_frames, 65].
    pub first: Option<usize>,
}

/// One run, settled once and played through every sequence from that state.
#[derive(Clone, Debug, PartialEq)]
pub struct RunResult {
    pub settling: SettlingResult,
    /// All 12, in sequence order.
    pub sequences: Vec<SequenceResult>,
    /// Per centre type, in `CELL_TYPES` order: the largest |centre value -
    /// settled centre value| over every sequence and frame, both widened to
    /// f64 before subtracting.
    pub centre_movement: Vec<f64>,
}

/// The two precisions, each with its count tables and reference reader.
pub(crate) trait Precision: Real {
    const DTYPE: Dtype;
    /// "f32" or "f64": the files' suffix and the gates' label.
    const SUFFIX: &'static str = dtype_name(Self::DTYPE);
    fn torch_count(group: usize) -> Self;
    fn libm_count(group: usize) -> Self;
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>;
}

impl Precision for f32 {
    const DTYPE: Dtype = Dtype::F32;
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F32[g] }
    fn libm_count(g: usize) -> Self { SYN_COUNT[g].exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f32(bytes, key)
    }
}

impl Precision for f64 {
    const DTYPE: Dtype = Dtype::F64;
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F64[g] }
    fn libm_count(g: usize) -> Self { (SYN_COUNT[g] as f64).exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f64(bytes, key)
    }
}

pub(crate) const fn dtype_name(p: Dtype) -> &'static str {
    match p {
        Dtype::F32 => "f32",
        Dtype::F64 => "f64",
    }
}

fn mismatch(m: String) -> ParityError {
    ParityError::Mismatch(m)
}

pub(crate) fn cell_type(net: &TypedNetwork, node: usize) -> &str {
    &net.cell_types()[net.node_cell_type()[node] as usize]
}

/// A file the manifest lists.
pub(crate) fn file<'a>(f: &'a Fixtures, name: &str) -> Result<&'a FileInfo, ParityError> {
    f.manifest.file(name).ok_or_else(|| ParityError::Malformed {
        rel: "flyvis-reference/manifest.json".into(),
        detail: format!("{name} is not in the manifest"),
    })
}

/// For each hexal h, the hexal at (-u, -v): a point reflection of the frame.
/// The lattice is point-symmetric by construction, so every hexal has one.
pub(crate) fn mirrored_hexals() -> Vec<usize> {
    let c: Vec<(i32, i32)> = hex_coords(DEFAULT_EXTENT).collect();
    c.iter()
        .enumerate()
        .map(|(h, &(u, v))| c.iter().position(|&p| p == (-u, -v)).unwrap_or(h))
        .collect()
}

/// The centre nodes' indices, each checked to be inside the network.
pub(crate) fn centre_nodes(f: &Fixtures) -> Result<Vec<usize>, ParityError> {
    let n = f.net.num_nodes();
    f.manifest
        .centre_nodes
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let node = c.node as usize;
            if node < n {
                Ok(node)
            } else {
                Err(mismatch(format!("centre node {k}: node {node} is outside the network's {n} nodes")))
            }
        })
        .collect()
}

/// Run `run`'s registered precision (R19).
pub(crate) fn registered(run: u32) -> Result<Dtype, ParityError> {
    RUNS.get(run as usize)
        .map(|r| r.3)
        .ok_or_else(|| ParityError::Usage(format!("run {run} is not one of the {} registered runs", RUNS.len())))
}

/// Run `run`'s manifest entry and dt, with piece 1's checks in their order:
/// the run's index, its precision against the registry, the dt and the
/// number of settling steps. Returns the registered precision too.
pub(crate) fn checked_run(f: &Fixtures, run: u32) -> Result<(&RunInfo, &DtInfo, Dtype), ParityError> {
    let want = registered(run)?;
    let info = f.manifest.runs.get(run as usize).ok_or_else(|| mismatch(format!("run {run} is not in the manifest")))?;
    if info.index != run {
        return Err(mismatch(format!("manifest runs must be listed in index order: entry {run} is run {}", info.index)));
    }
    if info.precision != want {
        return Err(mismatch(format!(
            "run {run}'s precision: manifest {}, expected {}",
            dtype_name(info.precision),
            dtype_name(want)
        )));
    }
    let dt = f
        .manifest
        .dts
        .iter()
        .find(|d| d.id == info.dt)
        .ok_or_else(|| mismatch(format!("run {run}'s dt {} is not listed in the manifest", info.dt)))?;
    let want_dt = match dt.id.as_str() {
        "1/200" => 1.0 / 200.0,
        "1/50" => 0.02,
        other => return Err(mismatch(format!("unexpected dt id {other}"))),
    };
    if dt.dt != want_dt {
        return Err(mismatch(format!("dt {}: manifest {}, expected {want_dt}", dt.id, dt.dt)));
    }
    let n_settle = (f.manifest.t_pre / dt.dt) as usize;
    if dt.n_settle != n_settle {
        return Err(mismatch(format!(
            "settling steps at dt {}: manifest {}, t_pre / dt gives {n_settle}",
            dt.id, dt.n_settle
        )));
    }
    Ok((info, dt, want))
}

/// One block of a settling file (`settled.*`, `settling_centre.*`).
fn read<R: Precision>(f: &Fixtures, name: &str, key: &BlockKey) -> Result<Array<R>, ParityError> {
    let rel = format!("flyvis-reference/{name}");
    let info = file(f, name)?;
    let bytes = &f
        .settling
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| ParityError::Malformed { rel: rel.clone(), detail: "not loaded".into() })?
        .1;
    R::read(info, bytes, key).map_err(|e| ParityError::Malformed { rel, detail: e.to_string() })
}

/// "Bit for bit" is `==` on f64-widened values, which is bit equality only
/// when no reference value is +0.0 or -0.0. The reference has none; a
/// fixture that does is refused rather than compared.
fn no_signed_zero(name: &str, key: &BlockKey, values: &[f64]) -> Result<(), ParityError> {
    match values.iter().position(|&v| v == 0.0) {
        None => Ok(()),
        Some(i) => {
            let mut at = Vec::new();
            if let Some(r) = key.run {
                at.push(format!("run {r}"));
            }
            at.push(format!("dt {}", key.dt));
            if let Some(s) = key.sequence {
                at.push(format!("sequence {s}"));
            }
            Err(mismatch(format!(
                "{name} ({}): reference value {i} is ±0.0; == is bit equality only without signed zeros",
                at.join(", ")
            )))
        }
    }
}

fn widen<R: Real>(values: &[R]) -> Vec<f64> {
    values.iter().map(|v| v.to_f64()).collect()
}

/// Builds `model`'s integrator at reference run `run`'s dt and precision and
/// settles it, as flyvis does: `n_settle` steps of grey on every
/// photoreceptor. Returns the settled integrator and the centre values
/// recorded after every step, widened to f64.
fn settle_state<R: Precision>(
    f: &Fixtures,
    run: u32,
    dt: &DtInfo,
    model: &str,
    counts: Counts,
    centre: &[usize],
) -> Result<(Graded<R>, Vec<f64>), ParityError> {
    let p = f.ensemble.model(model).ok_or_else(|| mismatch(format!("model {model} is not in the ensemble")))?;
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
    let mut g = Graded::<R>::new(&f.net, &params, dt.dt).map_err(|e| mismatch(format!("run {run}: {e}")))?;

    let mut input = vec![R::ZERO; f.net.num_nodes()];
    for &i in f.input_nodes.iter().flatten() {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    let mut rust_centre = Vec::new();
    for step in 0..dt.n_settle {
        g.step(&input).map_err(|e| mismatch(format!("run {run}: {e}")))?;
        if let Some(i) = g.state().iter().position(|v| !v.is_finite()) {
            let cell_type = cell_type(&f.net, i).to_string();
            return Err(ParityError::NonFinite { run, node: i, cell_type, after: After::SettleStep(step) });
        }
        // settle step `step` is the state after step + 1 Euler steps
        rust_centre.extend(centre.iter().map(|&i| g.state()[i].to_f64()));
    }
    Ok((g, rust_centre))
}

/// Reads run `run`'s settling reference and compares the settled state and
/// the centre trace with it.
#[allow(clippy::too_many_arguments)]
fn compare_settling<R: Precision>(
    f: &Fixtures,
    run: u32,
    info: &RunInfo,
    dt: &DtInfo,
    model: &str,
    counts: Counts,
    state: &[R],
    rust_centre: Vec<f64>,
) -> Result<SettlingResult, ParityError> {
    let n = f.net.num_nodes();
    let n_centre = f.manifest.centre_nodes.len();
    let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
    let settled_name = format!("settled.{}.bin", R::SUFFIX);
    let settled = read::<R>(f, &settled_name, &key)?;
    if settled.shape != [n] {
        return Err(mismatch(format!("settled shape: expected [{n}], found {:?}", settled.shape)));
    }
    let centre_name = format!("settling_centre.{}.bin", R::SUFFIX);
    let flyvis_centre = read::<R>(f, &centre_name, &key)?;
    if flyvis_centre.shape != [dt.n_settle, n_centre] {
        return Err(mismatch(format!(
            "settling_centre shape: expected [{}, {n_centre}], found {:?}",
            dt.n_settle, flyvis_centre.shape
        )));
    }
    let rust = widen(state);
    let flyvis = widen(&settled.data);
    let flyvis_centre = widen(&flyvis_centre.data);
    no_signed_zero(&settled_name, &key, &flyvis)?;
    no_signed_zero(&centre_name, &key, &flyvis_centre)?;

    let mut per_type_max = vec![0.0f64; f.net.cell_types().len()];
    for (i, (a, b)) in rust.iter().zip(&flyvis).enumerate() {
        let t = f.net.node_cell_type()[i] as usize;
        per_type_max[t] = per_type_max[t].max((a - b).abs());
    }
    Ok(SettlingResult {
        run,
        model: model.to_string(),
        counts,
        settled: diff(&rust, &flyvis),
        settling_centre: diff(&rust_centre, &flyvis_centre),
        settled_len: rust.len(),
        centre_len: rust_centre.len(),
        first_centre_difference: rust_centre.iter().zip(&flyvis_centre).position(|(a, b)| a != b),
        per_type_max,
    })
}

/// Plays `sequences` from `settled`, each with flyvis's input (mirrored
/// hexal by hexal if `mirror`), and compares every frame's centre and every
/// snapshot with flyvis's. Also returns each centre type's largest movement
/// away from `settled`, over everything played.
#[allow(clippy::too_many_arguments)]
fn play_from<R: Precision>(
    f: &Fixtures,
    t: &TraceInputs,
    run: u32,
    info: &RunInfo,
    centre: &[usize],
    g: &mut Graded<R>,
    settled: &[R],
    sequences: &[u32],
    mirror: bool,
) -> Result<(Vec<SequenceResult>, Vec<f64>), ParityError> {
    let n = f.net.num_nodes();
    let source: Vec<usize> = if mirror { mirrored_hexals() } else { (0..HEXALS).collect() };
    let input_file = file(f, "input.f64.bin")?;
    let centre_name = format!("centre.{}.bin", R::SUFFIX);
    let snapshots_name = format!("snapshots.{}.bin", R::SUFFIX);
    let centre_file = file(f, &centre_name)?;
    let snapshots_file = file(f, &snapshots_name)?;
    let malformed = |name: &str, e: ReferenceError| ParityError::Malformed {
        rel: format!("flyvis-reference/{name}"),
        detail: e.to_string(),
    };
    let settled_centre: Vec<f64> = centre.iter().map(|&k| settled[k].to_f64()).collect();
    let mut movement = vec![0.0f64; centre.len()];

    // Zeroed once per run: every frame overwrites the same input nodes, and
    // every other node stays 0, as in flyvis's zeroed buffer.
    let mut input = vec![R::ZERO; n];
    let mut frame = vec![R::ZERO; HEXALS];
    let mut played = Vec::with_capacity(sequences.len());
    for &s in sequences {
        let q = t
            .sequences
            .get(s as usize)
            .ok_or_else(|| mismatch(format!("sequence {s} is not in the manifest's sequences")))?;
        if q.index != s {
            return Err(mismatch(format!(
                "manifest sequences must be listed in index order: entry {s} is sequence {}",
                q.index
            )));
        }
        let layout = q.frames(&info.dt).ok_or_else(|| mismatch(format!("sequence {s} has no frames at dt {}", info.dt)))?;
        let n_frames = layout.n_frames;
        if layout.snapshot_frames.len() != SNAPSHOTS {
            return Err(mismatch(format!(
                "sequence {s}'s snapshot frames: expected {SNAPSHOTS}, found {}",
                layout.snapshot_frames.len()
            )));
        }
        g.set_state(settled).map_err(|e| mismatch(format!("run {run}: {e}")))?;

        let x = input_file
            .read_f64(t.input, &BlockKey { run: None, dt: &info.dt, sequence: Some(s) })
            .map_err(|e| malformed("input.f64.bin", e))?;
        if x.shape != [n_frames, HEXALS] {
            return Err(mismatch(format!("input shape, sequence {s}: expected [{n_frames}, {HEXALS}], found {:?}", x.shape)));
        }
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(s) };
        let flyvis_centre = R::read(centre_file, t.centre, &key).map_err(|e| malformed(&centre_name, e))?;
        if flyvis_centre.shape != [n_frames, centre.len()] {
            return Err(mismatch(format!(
                "centre shape, sequence {s}: expected [{n_frames}, {}], found {:?}",
                centre.len(),
                flyvis_centre.shape
            )));
        }
        let flyvis_snapshots = R::read(snapshots_file, t.snapshots, &key).map_err(|e| malformed(&snapshots_name, e))?;
        if flyvis_snapshots.shape != [SNAPSHOTS, n] {
            return Err(mismatch(format!(
                "snapshots shape, sequence {s}: expected [{SNAPSHOTS}, {n}], found {:?}",
                flyvis_snapshots.shape
            )));
        }
        let flyvis_centre = widen(&flyvis_centre.data);
        let flyvis_snapshots = widen(&flyvis_snapshots.data);
        no_signed_zero(&centre_name, &key, &flyvis_centre)?;
        no_signed_zero(&snapshots_name, &key, &flyvis_snapshots)?;

        let mut rust_centre = Vec::with_capacity(n_frames * centre.len());
        let mut rust_snapshots = Vec::with_capacity(SNAPSHOTS * n);
        for i in 0..n_frames {
            let row = &x.data[i * HEXALS..(i + 1) * HEXALS];
            for (slot, &h) in frame.iter_mut().zip(&source) {
                *slot = R::from_f64(row[h]);
            }
            scatter_hexals(&f.input_nodes, &frame, &mut input).map_err(|e| mismatch(format!("run {run}: {e}")))?;
            g.step(&input).map_err(|e| mismatch(format!("run {run}: {e}")))?;
            if let Some(k) = g.state().iter().position(|v| !v.is_finite()) {
                let cell_type = cell_type(&f.net, k).to_string();
                return Err(ParityError::NonFinite { run, node: k, cell_type, after: After::Frame { frame: i, sequence: s } });
            }
            // frame i is the state after i + 1 steps from the settled state
            for (j, &k) in centre.iter().enumerate() {
                let v = g.state()[k].to_f64();
                rust_centre.push(v);
                movement[j] = movement[j].max((v - settled_centre[j]).abs());
            }
            if layout.snapshot_frames.contains(&i) {
                rust_snapshots.extend(g.state().iter().map(|v| v.to_f64()));
            }
        }
        played.push(SequenceResult {
            sequence: s,
            family: q.family.clone(),
            n_frames,
            snapshot_frames: layout.snapshot_frames.clone(),
            first: rust_centre.iter().zip(&flyvis_centre).position(|(a, b)| a != b),
            centre: diff(&rust_centre, &flyvis_centre),
            snapshots: diff(&rust_snapshots, &flyvis_snapshots),
        });
    }
    Ok((played, movement))
}

fn check_inputs(t: &TraceInputs, run: u32, want: Dtype) -> Result<(), ParityError> {
    if t.precision != want {
        return Err(mismatch(format!(
            "trace inputs are {}, run {run} is {}",
            dtype_name(t.precision),
            dtype_name(want)
        )));
    }
    Ok(())
}

fn settle_as<R: Precision>(f: &Fixtures, run: u32, model: &str, counts: Counts) -> Result<SettlingResult, ParityError> {
    let (info, dt, _) = checked_run(f, run)?;
    let centre = centre_nodes(f)?;
    let (g, rust_centre) = settle_state::<R>(f, run, dt, model, counts, &centre)?;
    compare_settling::<R>(f, run, info, dt, model, counts, g.state(), rust_centre)
}

#[allow(clippy::too_many_arguments)]
fn play_as<R: Precision>(
    f: &Fixtures,
    t: &TraceInputs,
    run: u32,
    counts: Counts,
    start: Start,
    sequences: &[u32],
    mirror: bool,
) -> Result<Vec<SequenceResult>, ParityError> {
    let (info, dt, _) = checked_run(f, run)?;
    let centre = centre_nodes(f)?;
    let (mut g, _) = settle_state::<R>(f, run, dt, &info.model, counts, &centre)?;
    if let Start::Flyvis = start {
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
        let settled = read::<R>(f, &format!("settled.{}.bin", R::SUFFIX), &key)?;
        g.set_state(&settled.data).map_err(|e| mismatch(format!("run {run}: {e}")))?;
    }
    let settled = g.state().to_vec();
    Ok(play_from::<R>(f, t, run, info, &centre, &mut g, &settled, sequences, mirror)?.0)
}

fn run_as<R: Precision>(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts) -> Result<RunResult, ParityError> {
    let (info, dt, _) = checked_run(f, run)?;
    let centre = centre_nodes(f)?;
    let (mut g, rust_centre) = settle_state::<R>(f, run, dt, &info.model, counts, &centre)?;
    let settled = g.state().to_vec();
    let settling = compare_settling::<R>(f, run, info, dt, &info.model, counts, &settled, rust_centre)?;
    let all: Vec<u32> = t.sequences.iter().map(|q| q.index).collect();
    let (sequences, centre_movement) = play_from::<R>(f, t, run, info, &centre, &mut g, &settled, &all, false)?;
    Ok(RunResult { settling, sequences, centre_movement })
}

/// Settles `model`'s network at reference run `run`'s dt and registered
/// precision, and compares it with that run's reference.
pub fn settle(f: &Fixtures, run: u32, model: &str, counts: Counts) -> Result<SettlingResult, ParityError> {
    match registered(run)? {
        Dtype::F32 => settle_as::<f32>(f, run, model, counts),
        Dtype::F64 => settle_as::<f64>(f, run, model, counts),
    }
}

/// Settles run `run` with its own model (or starts from flyvis's settled
/// state), then plays `sequences` from that state and compares them.
pub fn play(
    f: &Fixtures,
    t: &TraceInputs,
    run: u32,
    counts: Counts,
    start: Start,
    sequences: &[u32],
    mirror: bool,
) -> Result<Vec<SequenceResult>, ParityError> {
    let p = registered(run)?;
    check_inputs(t, run, p)?;
    match p {
        Dtype::F32 => play_as::<f32>(f, t, run, counts, start, sequences, mirror),
        Dtype::F64 => play_as::<f64>(f, t, run, counts, start, sequences, mirror),
    }
}

/// Settle once, then play all of the manifest's sequences from that state:
/// both stages from one settle.
pub fn run(f: &Fixtures, t: &TraceInputs, run: u32, counts: Counts) -> Result<RunResult, ParityError> {
    let p = registered(run)?;
    check_inputs(t, run, p)?;
    match p {
        Dtype::F32 => run_as::<f32>(f, t, run, counts),
        Dtype::F64 => run_as::<f64>(f, t, run, counts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_support::fixture;
    use crate::reference::parse_sequences;
    use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text};

    #[test]
    fn a_nan_reference_value_makes_the_maximum_infinite() {
        let d = diff(&[1.0, 2.0, 3.0], &[1.0, f64::NAN, 3.5]);
        assert_eq!(d.max, f64::INFINITY, "a NaN must not be skipped");
        assert_eq!(d.at, 1);
        assert_eq!(d.differ, 2);
    }

    #[test]
    fn signed_zeros_in_the_reference_are_refused() {
        let key = BlockKey { run: Some(3), dt: "1/50", sequence: Some(8) };
        assert_eq!(no_signed_zero("centre.f64.bin", &key, &[1.0, -2.0]), Ok(()));
        for z in [0.0, -0.0] {
            assert_eq!(
                no_signed_zero("centre.f64.bin", &key, &[1.0, z]),
                Err(ParityError::Mismatch(
                    "centre.f64.bin (run 3, dt 1/50, sequence 8): reference value 1 is ±0.0; \
                     == is bit equality only without signed zeros"
                        .into()
                ))
            );
        }
    }

    #[test]
    fn worst_types_are_largest_first_and_keep_order_on_ties() {
        let r = SettlingResult {
            run: 0,
            model: "000".into(),
            counts: Counts::Libm,
            settled: Diff { max: 0.0, at: 0, differ: 0 },
            settling_centre: Diff { max: 0.0, at: 0, differ: 0 },
            settled_len: 4,
            centre_len: 0,
            first_centre_difference: None,
            per_type_max: vec![1.0, 3.0, 1.0, 2.0],
        };
        assert_eq!(r.worst_types(3), [1, 3, 0]);
        assert_eq!(r.worst_types(9), [1, 3, 0, 2]);
    }

    #[test]
    fn the_mirror_is_a_point_reflection_of_the_lattice() {
        let m = mirrored_hexals();
        let c: Vec<(i32, i32)> = hex_coords(DEFAULT_EXTENT).collect();
        assert_eq!(m.len(), HEXALS);
        for (h, &k) in m.iter().enumerate() {
            assert_eq!(c[k], (-c[h].0, -c[h].1));
            assert_eq!(m[k], h);
        }
    }

    /// The f64 trace files, read by the test itself.
    struct F64Traces {
        sequences: Vec<crate::reference::SequenceInfo>,
        input: Vec<u8>,
        centre: Vec<u8>,
        snapshots: Vec<u8>,
    }

    fn f64_traces() -> Option<F64Traces> {
        let text = or_skip(read_fixture_text("flyvis-reference/manifest.json"))?;
        let sequences = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let input = or_skip(read_fixture_bytes("flyvis-reference/input.f64.bin"))?;
        let centre = or_skip(read_fixture_bytes("flyvis-reference/centre.f64.bin"))?;
        let snapshots = or_skip(read_fixture_bytes("flyvis-reference/snapshots.f64.bin"))?;
        Some(F64Traces { sequences, input, centre, snapshots })
    }

    /// R20: `run` is `settle` then `play` from the same state, field for
    /// field, because all three are the same core.
    #[test]
    fn run_equals_settle_then_play() {
        let Some(f) = fixture() else { return };
        let Some(x) = f64_traces() else { return };
        let t = TraceInputs { precision: Dtype::F64, sequences: &x.sequences, input: &x.input, centre: &x.centre, snapshots: &x.snapshots };
        let all: Vec<u32> = (0..12).collect();
        let r = run(f, &t, 3, Counts::Torch).unwrap_or_else(|e| panic!("{e}"));
        let s = settle(f, 3, "000", Counts::Torch).unwrap_or_else(|e| panic!("{e}"));
        let p = play(f, &t, 3, Counts::Torch, Start::Rust, &all, false).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.settling, s);
        assert_eq!(r.sequences, p);
        assert_eq!(r.centre_movement.len(), 65);
        assert!(r.centre_movement.iter().all(|m| m.is_finite() && *m > 0.0), "{:?}", r.centre_movement);
    }

    /// R19: the trace inputs' precision must be the run's.
    #[test]
    fn play_refuses_trace_inputs_of_the_other_precision() {
        let Some(f) = fixture() else { return };
        let t = TraceInputs { precision: Dtype::F32, sequences: &[], input: &[], centre: &[], snapshots: &[] };
        for result in [
            play(f, &t, 3, Counts::Torch, Start::Rust, &[8], false).map(|_| ()),
            run(f, &t, 3, Counts::Torch).map(|_| ()),
        ] {
            assert_eq!(result, Err(ParityError::Mismatch("trace inputs are f32, run 3 is f64".into())));
        }
        assert!(matches!(settle(f, 8, "000", Counts::Torch), Err(ParityError::Usage(_))));
    }
}
