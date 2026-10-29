//! What the settling and trace gates share: the fixture, the two precisions,
//! where synapse counts come from, settling itself, and the comparison.
//!
//! Moved here from `settling_checks.rs` (spec
//! docs/superpowers/specs/2026-10-28-stimulus-traces-design.md, section 3)
//! without changing what it does. `tests/acceptance_settling.rs` matches the
//! panic and print strings below byte for byte: do not reword them.

use std::sync::OnceLock;

use semblance_connectome::TypedNetwork;
use semblance_sim::graded::{Graded, GradedParams, Real};

use crate::ensemble::Ensemble;
use crate::pretrained::SYN_COUNT;
use crate::reference::{Array, BlockKey, Dtype, FileInfo, Manifest, ReferenceError};
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text, read_real_spec_and_network};
use crate::wiring::{edge_syn_groups, input_nodes};

/// The libm gates' float64 bound, for every quantity they compare.
pub(crate) const F64_BOUND: f64 = 1e-10;

const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

pub(crate) struct Fixture {
    pub(crate) net: TypedNetwork,
    pub(crate) groups: Vec<u32>,
    /// The photoreceptor nodes per input type, in `spec.input_units` order,
    /// each in node order: flyvis's [8, 721] `input_index`.
    pub(crate) input_nodes: Vec<Vec<u32>>,
    pub(crate) ensemble: Ensemble,
    pub(crate) manifest: Manifest,
    files: Vec<(&'static str, Vec<u8>)>,
}

static FIXTURE: OnceLock<Result<Fixture, String>> = OnceLock::new();

/// Everything the gates read, built once. Every file is loaded (or found
/// missing) before anything is built from it. A missing file makes this
/// `None`, and every test that asks prints the notice.
pub(crate) fn fixture() -> Option<&'static Fixture> {
    or_skip(FIXTURE.get_or_init(load).as_ref().map_err(String::clone))
}

fn load() -> Result<Fixture, String> {
    let ensemble = read_fixture_text("flyvis-pretrained/ensemble.csv")?;
    let manifest = read_fixture_text("flyvis-reference/manifest.json")?;
    let mut files = Vec::new();
    for name in SETTLING_FILES {
        files.push((name, read_fixture_bytes(&format!("flyvis-reference/{name}"))?));
    }
    let (spec, net) = read_real_spec_and_network()?;
    let groups = edge_syn_groups(&net).expect("2,355 synapse-count groups");
    let inputs = input_nodes(&spec, &net).expect("input units are cell types");
    Ok(Fixture {
        groups,
        input_nodes: inputs,
        ensemble: Ensemble::parse(&ensemble).expect("ensemble.csv must parse"),
        manifest: Manifest::parse(&manifest).expect("manifest.json must parse"),
        files,
        net,
    })
}

pub(crate) fn cell_type(net: &TypedNetwork, node: usize) -> &str {
    &net.cell_types()[net.node_cell_type()[node] as usize]
}

/// The two precisions, each with its count tables and reference reader.
pub(crate) trait Precision: Real {
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
pub(crate) enum Counts {
    /// flyvis's own: torch's `exp`, from `syn_count_torch.rs`.
    Torch,
    /// Rust's `exp` of `SYN_COUNT`.
    Libm,
    /// The spec's integer counts, never exponentiated: a negative control.
    IntegerNSyn,
}

/// One settling run, Rust's and flyvis's values widened to f64.
pub(crate) struct Settled {
    pub(crate) rust: Vec<f64>,
    pub(crate) flyvis: Vec<f64>,
    pub(crate) rust_centre: Vec<f64>,
    pub(crate) flyvis_centre: Vec<f64>,
}

/// Builds `model`'s integrator at reference run `run`'s dt and precision and
/// settles it, as flyvis does: `n_settle` steps of grey on every
/// photoreceptor. Returns the settled integrator and the 65 centre values
/// recorded after every step, widened to f64.
pub(crate) fn settle_state<R: Precision>(f: &Fixture, run: u32, model: &str, counts: Counts) -> (Graded<R>, Vec<f64>) {
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
    for &i in f.input_nodes.iter().flatten() {
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
    (g, rust_centre)
}

/// Settles `model`'s network at reference run `run`'s dt and precision, and
/// reads that run's reference.
pub(crate) fn settle<R: Precision>(f: &Fixture, run: u32, model: &str, counts: Counts) -> Settled {
    let (g, rust_centre) = settle_state::<R>(f, run, model, counts);
    let info = &f.manifest.runs[run as usize];
    let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("the run's dt is listed");
    let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
    let settled = read::<R>(f, &format!("settled.{}.bin", R::SUFFIX), &key);
    assert_eq!(settled.shape, [f.net.num_nodes()], "settled shape");
    let flyvis_centre = read::<R>(f, &format!("settling_centre.{}.bin", R::SUFFIX), &key);
    assert_eq!(flyvis_centre.shape, [dt.n_settle, f.manifest.centre_nodes.len()], "settling_centre shape");
    Settled {
        rust: g.state().iter().map(|v| v.to_f64()).collect(),
        flyvis: settled.data.iter().map(|v| v.to_f64()).collect(),
        rust_centre,
        flyvis_centre: flyvis_centre.data.iter().map(|v| v.to_f64()).collect(),
    }
}

/// One block of a settling file (`settled.*`, `settling_centre.*`).
pub(crate) fn read<R: Precision>(f: &Fixture, name: &str, key: &BlockKey) -> Array<R> {
    let file = f.manifest.file(name).unwrap_or_else(|| panic!("{name} is not in the manifest"));
    let bytes = &f.files.iter().find(|(n, _)| *n == name).expect("loaded by fixture()").1;
    R::read(file, bytes, key).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The largest absolute error, where, and how many values are not equal.
pub(crate) struct Diff {
    pub(crate) max: f64,
    pub(crate) at: usize,
    pub(crate) differ: usize,
}

/// A NaN on either side counts as an infinite error, so it becomes the
/// maximum and fails every bound, instead of being skipped by `>`.
pub(crate) fn diff(rust: &[f64], flyvis: &[f64]) -> Diff {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nan_reference_value_makes_the_maximum_infinite() {
        let d = diff(&[1.0, 2.0, 3.0], &[1.0, f64::NAN, 3.5]);
        assert_eq!(d.max, f64::INFINITY, "a NaN must not be skipped");
        assert_eq!(d.at, 1);
        assert_eq!(d.differ, 2);
    }
}
