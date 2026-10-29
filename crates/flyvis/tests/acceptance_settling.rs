//! Acceptance tests for behaviour group `settling` of epic graded-settling:
//! the settling gates and their controls.
//!
//! One test per behaviour id (SETTLING-01 .. SETTLING-13).
//!
//! Two kinds of test live here:
//!
//! * In-process tests settle the real network through the public API
//!   (`semblance_flyvis` + `semblance_sim::graded`) and compare it against
//!   flyvis's reference, independently of the crate's own gate code.
//! * The gates themselves are `#[cfg(test)]` tests inside `semblance-flyvis`
//!   that read fixtures from `<workspace>/data`. To observe how they report a
//!   difference, a non-finite state, a missing fixture or a changed bound,
//!   these tests copy the workspace's sources (unchanged) into
//!   `CARGO_TARGET_TMPDIR`, give that copy a `data/` directory of symlinks to
//!   the real fixtures with the one file under test altered or removed, and
//!   run the gate there with `cargo test`. The real `data/` is never written.
//!
//! Everything skips without the real fixtures, unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip into a failure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

use semblance_connectome::{TypedNetwork, TypedNetworkParts};
use semblance_flyvis::pretrained::{CELL_TYPES, SIGN, SYN_COUNT};
use semblance_flyvis::reference::{Array, BlockKey, Dtype, FileInfo, Manifest, ReferenceError};
use semblance_flyvis::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use semblance_flyvis::{
    edge_syn_groups, expand, input_nodes, ConnectomeSpec, Ensemble, ModelParams, WiringError,
    DEFAULT_EXTENT, DEFAULT_N_SYN_FILL,
};
use semblance_sim::graded::{Graded, GradedParams, Real};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";
const ENSEMBLE: &str = "flyvis-pretrained/ensemble.csv";
const MANIFEST: &str = "flyvis-reference/manifest.json";
const PROVENANCE: &str = "flyvis-reference/PROVENANCE.json";
const SETTLED_F32: &str = "flyvis-reference/settled.f32.bin";
const SETTLED_F64: &str = "flyvis-reference/settled.f64.bin";
const CENTRE_F32: &str = "flyvis-reference/settling_centre.f32.bin";
const CENTRE_F64: &str = "flyvis-reference/settling_centre.f64.bin";

const F64_BOUND: f64 = 1e-10;

fn data_path(rel: &str) -> PathBuf {
    Path::new(WORKSPACE).join("data").join(rel)
}

fn fixtures_required() -> bool {
    std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1")
}

/// The bytes of `data/<rel>`, or its missing-fixture notice (without the
/// `skipping: ` that `fx` prints). Prints nothing, so a cached miss can be
/// reported by every test that asks.
fn read_fixture(rel: &str) -> Result<Vec<u8>, String> {
    let path = data_path(rel);
    std::fs::read(&path).map_err(|_| {
        assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {} is missing", path.display());
        format!("{} not found; run ci/fetch-fixtures.sh", path.display())
    })
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("fixture is UTF-8")
}

struct Fx {
    spec: ConnectomeSpec,
    net: TypedNetwork,
    groups: Vec<u32>,
    inputs: Vec<u32>,
    ensemble: Ensemble,
    ensemble_csv: String,
    manifest: Manifest,
    manifest_json: String,
    provenance: String,
    bins: HashMap<&'static str, Vec<u8>>,
}

static FX: OnceLock<Result<Fx, String>> = OnceLock::new();

/// The shared fixture, built once, or `None` after printing
/// `skipping: <notice>`: every test that finds it missing prints its own.
fn fx() -> Option<&'static Fx> {
    let loaded = FX.get_or_init(|| {
        let spec_json = text(read_fixture(SPEC)?);
        let ensemble_csv = text(read_fixture(ENSEMBLE)?);
        let manifest_json = text(read_fixture(MANIFEST)?);
        let provenance = text(read_fixture(PROVENANCE)?);
        let mut bins = HashMap::new();
        for rel in [SETTLED_F32, SETTLED_F64, CENTRE_F32, CENTRE_F64] {
            bins.insert(rel, read_fixture(rel)?);
        }
        let spec = ConnectomeSpec::from_json_str(&spec_json).expect("real spec parses");
        let net = expand(&spec, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL).expect("real spec expands");
        let groups = edge_syn_groups(&net).expect("2,355 groups");
        let mut inputs: Vec<u32> = input_nodes(&spec, &net).expect("input units are cell types").into_iter().flatten().collect();
        inputs.sort_unstable();
        Ok(Fx {
            ensemble: Ensemble::parse(&ensemble_csv).expect("ensemble.csv parses"),
            manifest: Manifest::parse(&manifest_json).expect("manifest.json parses"),
            spec,
            net,
            groups,
            inputs,
            ensemble_csv,
            manifest_json,
            provenance,
            bins,
        })
    });
    match loaded {
        Ok(f) => Some(f),
        Err(notice) => {
            eprintln!("skipping: {notice}");
            None
        }
    }
}

fn cell_type(net: &TypedNetwork, node: usize) -> &str {
    &net.cell_types()[net.node_cell_type()[node] as usize]
}

// ---------------------------------------------------------------------------
// An independent settling run through the public API
// ---------------------------------------------------------------------------

trait P: Real {
    const DTYPE: Dtype;
    const SUFFIX: &'static str;
    fn torch(g: usize) -> Self;
    fn libm(g: usize) -> Self;
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>;
    fn bits(self) -> u64;
}

impl P for f32 {
    const DTYPE: Dtype = Dtype::F32;
    const SUFFIX: &'static str = "f32";
    fn torch(g: usize) -> Self { SYN_COUNT_TORCH_F32[g] }
    fn libm(g: usize) -> Self { SYN_COUNT[g].exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f32(bytes, key)
    }
    fn bits(self) -> u64 { u64::from(self.to_bits()) }
}

impl P for f64 {
    const DTYPE: Dtype = Dtype::F64;
    const SUFFIX: &'static str = "f64";
    fn torch(g: usize) -> Self { SYN_COUNT_TORCH_F64[g] }
    fn libm(g: usize) -> Self { f64::from(SYN_COUNT[g]).exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f64(bytes, key)
    }
    fn bits(self) -> u64 { self.to_bits() }
}

#[derive(Clone, Copy)]
enum Counts {
    Torch,
    Libm,
    IntegerNSyn,
}

struct Run {
    n_settle: usize,
    /// Rust's state after every step, bits (in the run's precision).
    rust_bits: Vec<u64>,
    flyvis_bits: Vec<u64>,
    rust_centre_bits: Vec<u64>,
    flyvis_centre_bits: Vec<u64>,
    rust: Vec<f64>,
    flyvis: Vec<f64>,
    rust_centre: Vec<f64>,
    flyvis_centre: Vec<f64>,
    settled_shape: Vec<usize>,
    centre_shape: Vec<usize>,
}

fn reference<R: P>(f: &Fx, name: &str, rel: &str, run: u32, dt: &str) -> Array<R> {
    let file = f.manifest.file(name).unwrap_or_else(|| panic!("{name} in manifest"));
    let key = BlockKey { run: Some(run), dt, sequence: None };
    R::read(file, &f.bins[rel], &key).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn settle_with<R: P>(f: &Fx, run: u32, p: &ModelParams, counts: Counts) -> Run {
    let info = &f.manifest.runs[run as usize];
    assert_eq!(info.precision, R::DTYPE);
    let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("dt listed");
    let count: Vec<R> = f
        .groups
        .iter()
        .enumerate()
        .map(|(e, &g)| match counts {
            Counts::Torch => R::torch(g as usize),
            Counts::Libm => R::libm(g as usize),
            Counts::IntegerNSyn => R::from_f32(f.net.edge_n_syn()[e]),
        })
        .collect();
    let params = GradedParams { bias: &p.bias, time_const: &p.time_const, syn_strength: &p.syn_strength, edge_syn_count: &count };
    let mut g = Graded::<R>::new(&f.net, &params, dt.dt).expect("valid parameters");
    let mut input = vec![R::ZERO; f.net.num_nodes()];
    for &i in &f.inputs {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let mut rust_centre = Vec::new();
    for _ in 0..dt.n_settle {
        g.step(&input).expect("input length");
        rust_centre.extend(centre.iter().map(|&i| g.state()[i]));
    }
    let (sf, cf) = (format!("settled.{}.bin", R::SUFFIX), format!("settling_centre.{}.bin", R::SUFFIX));
    let (sr, cr) = match R::DTYPE {
        Dtype::F32 => (SETTLED_F32, CENTRE_F32),
        Dtype::F64 => (SETTLED_F64, CENTRE_F64),
    };
    let settled = reference::<R>(f, &sf, sr, run, &info.dt);
    let fc = reference::<R>(f, &cf, cr, run, &info.dt);
    Run {
        n_settle: dt.n_settle,
        rust_bits: g.state().iter().map(|v| v.bits()).collect(),
        flyvis_bits: settled.data.iter().map(|v| v.bits()).collect(),
        rust_centre_bits: rust_centre.iter().map(|v| v.bits()).collect(),
        flyvis_centre_bits: fc.data.iter().map(|v| v.bits()).collect(),
        rust: g.state().iter().map(|v| v.to_f64()).collect(),
        flyvis: settled.data.iter().map(|v| v.to_f64()).collect(),
        rust_centre: rust_centre.iter().map(|v| v.to_f64()).collect(),
        flyvis_centre: fc.data.iter().map(|v| v.to_f64()).collect(),
        settled_shape: settled.shape,
        centre_shape: fc.shape,
    }
}

fn settle<R: P>(f: &Fx, run: u32, model: &str, counts: Counts) -> Run {
    let p = f.ensemble.model(model).unwrap_or_else(|| panic!("model {model}"));
    settle_with::<R>(f, run, p, counts)
}

/// (max |a - b|, index of the first maximum, number of unequal values)
fn max_err(a: &[f64], b: &[f64]) -> (f64, usize, usize) {
    assert_eq!(a.len(), b.len());
    let mut m = (0.0, 0, 0);
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        let e = (x - y).abs();
        // a NaN is an infinite error, not one `>` skips
        let e = if e.is_nan() { f64::INFINITY } else { e };
        m.2 += usize::from(x != y);
        if e > m.0 {
            m.0 = e;
            m.1 = i;
        }
    }
    m
}

/// flyvis's f32/f64 gap for (model, dt id, quantity), from PROVENANCE.json.
fn provenance_gap(provenance: &str, model: &str, dt: &str, quantity: &str) -> f64 {
    let (start, len) = provenance_gap_span(provenance, model, dt, quantity);
    provenance[start..start + len].parse().expect("gap is a number")
}

/// Byte span of the number for (model, dt, quantity) under `f32_f64_gap`.
fn provenance_gap_span(provenance: &str, model: &str, dt: &str, quantity: &str) -> (usize, usize) {
    let gap = provenance.find("\"f32_f64_gap\"").expect("f32_f64_gap present");
    let entry = gap + provenance[gap..].find(&format!("\"{model} {dt}\"")).expect("gap entry");
    let end = entry + provenance[entry..].find('}').expect("entry closes");
    let q = format!("\"{quantity}\":");
    let at = entry + provenance[entry..end].find(&q).expect("quantity in entry") + q.len();
    let start = at + provenance[at..].len() - provenance[at..].trim_start().len();
    let len = provenance[start..].find([',', '\n', '}']).expect("number ends");
    (start, provenance[start..start + len].trim_end().len())
}

// ---------------------------------------------------------------------------
// A shadow copy of the workspace, to run the crate's own gates against
// altered fixtures without touching the real data/
// ---------------------------------------------------------------------------

struct Shadow {
    root: PathBuf,
    target: PathBuf,
}

static SHADOW: Mutex<Option<Shadow>> = Mutex::new(None);

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "target" || name == "data" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            copy_tree(&src, &dst);
        } else if ty.is_file() {
            copy_if_changed(&src, &dst);
        }
    }
}

fn copy_if_changed(src: &Path, dst: &Path) {
    let new = std::fs::read(src).unwrap();
    if std::fs::read(dst).ok().as_deref() != Some(&new[..]) {
        std::fs::write(dst, new).unwrap();
    }
}

fn shadow() -> MutexGuard<'static, Option<Shadow>> {
    let mut guard = SHADOW.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-settling");
        let root = base.join("workspace");
        let ws = Path::new(WORKSPACE);
        // Copy into an empty tree, so a source file deleted from the
        // workspace is not still compiled here. The sibling target dir is
        // kept. remove_dir_all removes data/'s symlinks, never their targets.
        match std::fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("removing the old shadow {}: {e}", root.display()),
        }
        std::fs::create_dir_all(&root).unwrap();
        for f in ["Cargo.toml", "Cargo.lock", "README.md"] {
            copy_if_changed(&ws.join(f), &root.join(f));
        }
        copy_tree(&ws.join("crates"), &root.join("crates"));
        copy_tree(&ws.join("app"), &root.join("app"));
        *guard = Some(Shadow { root, target: base.join("target") });
    }
    guard
}

/// What to do to one fixture in the shadow's data/.
enum Alter {
    Replace(Vec<u8>),
    Remove,
}

#[cfg(unix)]
fn link(src: &Path, dst: &Path) {
    std::os::unix::fs::symlink(src, dst).unwrap();
}

struct Outcome {
    passed: bool,
    output: String,
}

impl Shadow {
    /// Lays out data/ with every real fixture linked in, `alter` applied.
    fn data(&self, alter: &[(&str, Alter)]) {
        let data = self.root.join("data");
        if data.symlink_metadata().is_ok() {
            std::fs::remove_dir_all(&data).unwrap();
        }
        std::fs::create_dir_all(&data).unwrap();
        let real = Path::new(WORKSPACE).join("data");
        for dir in ["flyvis-supplementary", "flyvis-build", "flyvis-stimuli"] {
            if real.join(dir).exists() {
                link(&real.join(dir), &data.join(dir));
            }
        }
        for dir in ["flyvis-pretrained", "flyvis-reference"] {
            std::fs::create_dir_all(data.join(dir)).unwrap();
            for entry in std::fs::read_dir(real.join(dir)).unwrap() {
                let entry = entry.unwrap();
                link(&entry.path(), &data.join(dir).join(entry.file_name()));
            }
        }
        for (rel, a) in alter {
            let path = data.join(rel);
            let _ = std::fs::remove_file(&path);
            if let Alter::Replace(bytes) = a {
                std::fs::write(&path, bytes).unwrap();
            }
        }
    }

    /// Runs one of semblance-flyvis's own lib tests in the shadow.
    fn gate(&self, test: &str, require: bool) -> Outcome {
        let mut cmd = Command::new(option_env!("CARGO").unwrap_or("cargo"));
        cmd.current_dir(&self.root)
            .env("CARGO_TARGET_DIR", &self.target)
            .args(["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--", "--exact"])
            .arg(format!("settling_checks::{test}"))
            .args(["--nocapture", "--test-threads=1"]);
        if require {
            cmd.env("SEMBLANCE_REQUIRE_FIXTURES", "1");
        } else {
            cmd.env_remove("SEMBLANCE_REQUIRE_FIXTURES");
        }
        let out = cmd.output().expect("cargo runs");
        let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(output.contains("running 1 test"), "the gate {test} must run exactly once:\n{output}");
        Outcome { passed: out.status.success(), output }
    }
}

fn line_with<'a>(output: &'a str, needle: &str) -> &'a str {
    output.lines().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("no line with {needle:?} in:\n{output}"))
}

/// The text between `before` and the next `until` character.
fn field<'a>(line: &'a str, before: &str, until: char) -> &'a str {
    let at = line.find(before).unwrap_or_else(|| panic!("{before:?} not in {line:?}")) + before.len();
    let rest = &line[at..];
    &rest[..rest.find(until).unwrap_or(rest.len())]
}

fn block_offset(m: &Manifest, name: &str, run: u32) -> usize {
    m.file(name).unwrap().blocks.iter().find(|b| b.run == Some(run)).unwrap().offset
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

// ---------------------------------------------------------------------------
// SETTLING-01
// ---------------------------------------------------------------------------

fn assert_exact<R: P>(f: &Fx, run: u32) {
    let info = &f.manifest.runs[run as usize];
    let s = settle::<R>(f, run, &info.model, Counts::Torch);
    assert_eq!(s.rust_bits.len(), 45_669);
    assert_eq!(s.rust_centre_bits.len(), s.n_settle * 65);
    let settled = s.rust_bits.iter().zip(&s.flyvis_bits).filter(|(a, b)| a != b).count();
    let centre = s.rust_centre_bits.iter().zip(&s.flyvis_centre_bits).filter(|(a, b)| a != b).count();
    assert_eq!((settled, centre), (0, 0), "run {run} ({}, {}, {}): settled / centre values differing", info.model, info.dt, R::SUFFIX);
}

#[test]
fn settling_01_torch_counts_settle_bit_equal_to_flyvis_in_all_eight_runs() {
    let Some(f) = fx() else { return };
    for run in 0..8u32 {
        match f.manifest.runs[run as usize].precision {
            Dtype::F32 => assert_exact::<f32>(f, run),
            Dtype::F64 => assert_exact::<f64>(f, run),
        }
    }
}

// ---------------------------------------------------------------------------
// SETTLING-02
// ---------------------------------------------------------------------------

#[test]
fn settling_02_an_exact_gate_failure_names_counts_first_step_and_largest_error() {
    let Some(f) = fx() else { return };
    let (node, step, c) = (1234usize, 37usize, 10usize);
    let mut settled = f.bins[SETTLED_F32].clone();
    let at = block_offset(&f.manifest, "settled.f32.bin", 0) + 4 * node;
    let original = f32_at(&settled, at);
    settled[at..at + 4].copy_from_slice(&1000.0f32.to_le_bytes());
    let mut centre = f.bins[CENTRE_F32].clone();
    let cat = block_offset(&f.manifest, "settling_centre.f32.bin", 0) + 4 * (step * 65 + c);
    let co = f32_at(&centre, cat);
    centre[cat..cat + 4].copy_from_slice(&(co + 1000.0).to_le_bytes());

    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[(SETTLED_F32, Alter::Replace(settled)), (CENTRE_F32, Alter::Replace(centre))]);
    let out = sh.gate("settling_exact_run_0_000_1_200_f32", true);
    assert!(!out.passed, "a differing value must fail the exact gate:\n{}", out.output);
    let o = &out.output;
    assert!(o.contains("1 settled and 1 settling-centre values differ"), "count of differing values:\n{o}");
    let want_step = format!("first difference at settle step {step} ({})", f.manifest.centre_nodes[c].cell_type);
    assert!(o.contains(&want_step), "expected {want_step:?}:\n{o}");
    let err = (f64::from(original) - 1000.0).abs();
    let want_err = format!("largest settled error {err:e} at node {node} ({})", cell_type(&f.net, node));
    assert!(o.contains(&want_err), "expected {want_err:?}:\n{o}");
}

// ---------------------------------------------------------------------------
// SETTLING-03
// ---------------------------------------------------------------------------

fn assert_libm<R: P>(f: &Fx, run: u32) {
    let info = &f.manifest.runs[run as usize];
    let (bs, bc) = match R::DTYPE {
        Dtype::F64 => (F64_BOUND, F64_BOUND),
        Dtype::F32 => (
            2.0 * provenance_gap(&f.provenance, &info.model, &info.dt, "settled"),
            2.0 * provenance_gap(&f.provenance, &info.model, &info.dt, "settling_centre"),
        ),
    };
    let s = settle::<R>(f, run, &info.model, Counts::Libm);
    let (ms, _, _) = max_err(&s.rust, &s.flyvis);
    let (mc, _, _) = max_err(&s.rust_centre, &s.flyvis_centre);
    eprintln!("SETTLING-03 run {run}: settled {ms:e} (bound {bs:e}), centre {mc:e} (bound {bc:e})");
    assert!(ms <= bs, "run {run}: settled max {ms:e} exceeds {bs:e}");
    assert!(mc <= bc, "run {run}: settling-centre max {mc:e} exceeds {bc:e}");
}

#[test]
fn settling_03_rusts_exp_stays_within_the_pre_registered_bounds() {
    let Some(f) = fx() else { return };
    for run in 0..8u32 {
        match f.manifest.runs[run as usize].precision {
            Dtype::F32 => assert_libm::<f32>(f, run),
            Dtype::F64 => assert_libm::<f64>(f, run),
        }
    }
}

// ---------------------------------------------------------------------------
// SETTLING-04
// ---------------------------------------------------------------------------

#[test]
fn settling_04_a_libm_gate_prints_maxima_ratios_worst_types_and_bit_equal_count() {
    let Some(f) = fx() else { return };
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate("settling_libm_run_0_000_1_200_f32", true);
    assert!(out.passed, "libm run 0 must pass:\n{}", out.output);
    let line = line_with(&out.output, "libm run 0 ");

    let s = settle::<f32>(f, 0, "000", Counts::Libm);
    let (ms, _, differ) = max_err(&s.rust, &s.flyvis);
    let (mc, _, _) = max_err(&s.rust_centre, &s.flyvis_centre);
    let bs = 2.0 * provenance_gap(&f.provenance, "000", "1/200", "settled");
    let bc = 2.0 * provenance_gap(&f.provenance, "000", "1/200", "settling_centre");

    // settled maximum, its node and cell type, its ratio to the bound
    assert_eq!(field(line, "settled max ", ' '), format!("{ms:.3e}"), "{line}");
    let node: usize = field(line, " at node ", ' ').parse().unwrap();
    assert_eq!((s.rust[node] - s.flyvis[node]).abs(), ms, "printed node holds the settled maximum: {line}");
    assert_eq!(field(line, &format!(" at node {node} ("), ')'), cell_type(&f.net, node), "{line}");
    let after_settled = &line[line.find(" at node ").unwrap()..];
    assert_eq!(field(after_settled, "), ", ' '), format!("{:.3e}", ms / bs), "settled ratio: {line}");

    // settling-centre maximum, its step and cell type, its ratio
    assert_eq!(field(line, "settling centre max ", ' '), format!("{mc:.3e}"), "{line}");
    let step: usize = field(line, " at step ", ' ').parse().unwrap();
    let ty = field(line, &format!(" at step {step} ("), ')');
    let holds = (0..65).any(|c| {
        let i = step * 65 + c;
        f.manifest.centre_nodes[c].cell_type == ty && (s.rust_centre[i] - s.flyvis_centre[i]).abs() == mc
    });
    assert!(holds, "printed step and cell type hold the centre maximum: {line}");
    let after_centre = &line[line.find(" at step ").unwrap()..];
    assert_eq!(field(after_centre, "), ", ' '), format!("{:.3e}", mc / bc), "centre ratio: {line}");

    // the three cell types with the largest settled error
    let mut per_type: HashMap<&str, f64> = HashMap::new();
    for i in 0..s.rust.len() {
        let e = per_type.entry(cell_type(&f.net, i)).or_insert(0.0);
        *e = e.max((s.rust[i] - s.flyvis[i]).abs());
    }
    let worst: Vec<&str> = field(line, "worst types ", ';').split(", ").collect();
    assert_eq!(worst.len(), 3, "{line}");
    let mut named = Vec::new();
    for w in worst {
        let (t, v) = w.split_once(' ').unwrap();
        assert_eq!(v, format!("{:.2e}", per_type[t]), "{t}: {line}");
        named.push(t);
    }
    let floor = named.iter().map(|t| per_type[t]).fold(f64::INFINITY, f64::min);
    for (t, e) in &per_type {
        if !named.contains(t) {
            assert!(*e <= floor, "{t} ({e:e}) beats a listed worst type: {line}");
        }
    }

    // bit-equal settled values
    assert_eq!(
        field(line, "settled bit-equal ", '\n'),
        format!("{} of {}", s.rust.len() - differ, s.rust.len()),
        "{line}"
    );
}

// ---------------------------------------------------------------------------
// SETTLING-05
// ---------------------------------------------------------------------------

/// The text of the file section of `name` in the manifest JSON.
fn file_section(json: &str, name: &str) -> (usize, usize) {
    let start = json.find(&format!("\"name\": \"{name}\"")).unwrap_or_else(|| panic!("{name} in manifest"));
    let end = json[start + 1..].find("\"name\":").map_or(json.len(), |e| start + 1 + e);
    (start, end)
}

fn replace_in_section(json: &str, name: &str, edits: &[(&str, &str)]) -> String {
    let (s, e) = file_section(json, name);
    let mut section = json[s..e].to_string();
    for (from, to) in edits {
        let at = section.find(from).unwrap_or_else(|| panic!("{from:?} in {name}"));
        section.replace_range(at..at + from.len(), to);
    }
    format!("{}{}{}", &json[..s], section, &json[e..])
}

/// Replaces the first block's shape array in `name`'s section.
fn replace_first_shape(json: &str, name: &str, shape: &str) -> String {
    let (s, e) = file_section(json, name);
    let blocks = s + json[s..e].find("\"blocks\"").unwrap();
    let at = blocks + json[blocks..].find("\"shape\": [").unwrap();
    let close = at + json[at..].find(']').unwrap();
    format!("{}\"shape\": {shape}{}", &json[..at], &json[close + 1..])
}

#[test]
fn settling_05_a_run_checks_dt_n_settle_shapes_and_step_alignment() {
    let Some(f) = fx() else { return };
    // In process: what the gate checks holds for the real reference.
    let dts: Vec<(&str, f64)> = f.manifest.dts.iter().map(|d| (d.id.as_str(), d.dt)).collect();
    assert_eq!(dts, [("1/200", 1.0 / 200.0), ("1/50", 0.02)]);
    for d in &f.manifest.dts {
        assert_eq!(d.n_settle, (f.manifest.t_pre / d.dt) as usize, "n_settle at {}", d.id);
    }
    let s = settle::<f64>(f, 3, "000", Counts::Torch);
    assert_eq!(s.settled_shape, [45_669]);
    assert_eq!(s.centre_shape, [s.n_settle, 65]);
    // the value recorded after step k is flyvis's settle step k - 1 ...
    assert_eq!(s.rust_centre_bits, s.flyvis_centre_bits, "after step k == settle step k - 1");
    // ... and not settle step k (an off-by-one would be caught)
    let shifted = s.rust_centre_bits[65..].iter().zip(&s.flyvis_centre_bits).filter(|(a, b)| a != b).count();
    assert!(shifted > 0, "an off-by-one alignment must not also match");

    // In the gate: each check fires when its premise is broken.
    let m = &f.manifest_json;
    let bad_dt = m.replacen("\"dt\": 0.005,", "\"dt\": 0.0051,", 1);
    assert_ne!(&bad_dt, m);
    let bad_n = m.replacen("\"n_settle\": 200", "\"n_settle\": 199", 1);
    assert_ne!(&bad_n, m);
    let bad_centre = replace_first_shape(m, "settling_centre.f32.bin", "[65, 200]");
    let bad_settled_json = replace_first_shape(
        &replace_in_section(
            m,
            "settled.f32.bin",
            &[
                ("\"bytes\": 730704", "\"bytes\": 730700"),
                ("\"bytes\": 182676", "\"bytes\": 182672"),
                ("\"offset\": 182676", "\"offset\": 182672"),
                ("\"offset\": 365352", "\"offset\": 365348"),
                ("\"offset\": 548028", "\"offset\": 548024"),
            ],
        ),
        "settled.f32.bin",
        "[45668]",
    );
    let mut bad_settled_bin = f.bins[SETTLED_F32].clone();
    let end0 = block_offset(&f.manifest, "settled.f32.bin", 0) + 4 * 45_669;
    bad_settled_bin.drain(end0 - 4..end0);
    // every altered manifest must still parse, so the gate's own check is what fails
    for json in [&bad_dt, &bad_n, &bad_centre, &bad_settled_json] {
        Manifest::parse(json).expect("altered manifest still parses");
    }

    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    let cases: [(Vec<(&str, Alter)>, &str); 4] = [
        (vec![(MANIFEST, Alter::Replace(bad_dt.into_bytes()))], "dt 1/200"),
        (vec![(MANIFEST, Alter::Replace(bad_n.into_bytes()))], "settling steps at dt 1/200"),
        (vec![(MANIFEST, Alter::Replace(bad_centre.into_bytes()))], "settling_centre shape"),
        (
            vec![(MANIFEST, Alter::Replace(bad_settled_json.into_bytes())), (SETTLED_F32, Alter::Replace(bad_settled_bin))],
            "settled shape",
        ),
    ];
    for (alter, want) in cases {
        sh.data(&alter);
        let out = sh.gate("settling_exact_run_0_000_1_200_f32", true);
        assert!(!out.passed, "{want}: the gate must fail:\n{}", out.output);
        assert!(out.output.contains(want), "{want}: expected the gate's assertion:\n{}", out.output);
    }
}

// ---------------------------------------------------------------------------
// SETTLING-06
// ---------------------------------------------------------------------------

#[test]
fn settling_06_a_non_finite_state_fails_naming_node_cell_type_and_step() {
    let Some(f) = fx() else { return };
    let csv: String = f
        .ensemble_csv
        .lines()
        .map(|l| {
            if l.starts_with("000,syn_strength,") {
                let (head, _) = l.rsplit_once(',').unwrap();
                format!("{head},3e38\n")
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    let ensemble = Ensemble::parse(&csv).expect("huge but finite strengths parse");
    let p = ensemble.model("000").unwrap();

    // where the public integrator first goes non-finite, f32, run 0's dt
    let dt = f.manifest.dts.iter().find(|d| d.id == "1/200").unwrap();
    let count: Vec<f32> = f.groups.iter().map(|&g| SYN_COUNT_TORCH_F32[g as usize]).collect();
    let params = GradedParams { bias: &p.bias, time_const: &p.time_const, syn_strength: &p.syn_strength, edge_syn_count: &count };
    let mut g = Graded::<f32>::new(&f.net, &params, dt.dt).unwrap();
    let mut input = vec![0.0f32; f.net.num_nodes()];
    for &i in &f.inputs {
        input[i as usize] = f.manifest.pre_stim_value as f32;
    }
    let mut first = None;
    for step in 0..dt.n_settle {
        g.step(&input).unwrap();
        if let Some(i) = g.state().iter().position(|v| !v.is_finite()) {
            first = Some((i, step));
            break;
        }
    }
    let (node, step) = first.expect("the altered model must produce a non-finite state");

    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[(ENSEMBLE, Alter::Replace(csv.into_bytes()))]);
    let out = sh.gate("settling_exact_run_0_000_1_200_f32", true);
    assert!(!out.passed, "a non-finite state must fail the run:\n{}", out.output);
    let want = format!("node {node} ({}) is not finite after settle step {step}", cell_type(&f.net, node));
    assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);
}

// ---------------------------------------------------------------------------
// SETTLING-07
// ---------------------------------------------------------------------------

#[test]
fn settling_07_the_manifest_lists_the_eight_runs_and_the_networks_centre_types() {
    let Some(f) = fx() else { return };
    let want = [
        (0u32, "000", "1/200", Dtype::F32), (1, "000", "1/200", Dtype::F64),
        (2, "000", "1/50", Dtype::F32), (3, "000", "1/50", Dtype::F64),
        (4, "049", "1/200", Dtype::F32), (5, "049", "1/200", Dtype::F64),
        (6, "049", "1/50", Dtype::F32), (7, "049", "1/50", Dtype::F64),
    ];
    let got: Vec<_> = f.manifest.runs.iter().map(|r| (r.index, r.model.as_str(), r.dt.as_str(), r.precision)).collect();
    assert_eq!(got, want);
    assert_eq!(f.manifest.centre_nodes.len(), 65);
    for (k, c) in f.manifest.centre_nodes.iter().enumerate() {
        assert_eq!(cell_type(&f.net, c.node as usize), c.cell_type, "centre node {k}");
    }
    // and the crate's own gate agrees
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate("reference_runs_are_the_eight_expected", true);
    assert!(out.passed, "{}", out.output);
}

// ---------------------------------------------------------------------------
// SETTLING-08
// ---------------------------------------------------------------------------

#[test]
fn settling_08_the_gates_bounds_are_provenances_gaps_bit_for_bit() {
    let Some(f) = fx() else { return };
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate("pre_registered_bounds_match_provenance", true);
    assert!(out.passed, "unaltered PROVENANCE.json must match:\n{}", out.output);
    // one ulp off in any of the eight gaps must fail the gate
    for model in ["000", "049"] {
        for dt in ["1/200", "1/50"] {
            for q in ["settled", "settling_centre"] {
                let (start, len) = provenance_gap_span(&f.provenance, model, dt, q);
                let v: f64 = f.provenance[start..start + len].parse().unwrap();
                let next = f64::from_bits(v.to_bits() + 1);
                let mut p = f.provenance.clone();
                p.replace_range(start..start + len, &format!("{next:e}"));
                assert!((provenance_gap(&p, model, dt, q) - next) == 0.0);
                sh.data(&[(PROVENANCE, Alter::Replace(p.into_bytes()))]);
                let out = sh.gate("pre_registered_bounds_match_provenance", true);
                assert!(!out.passed, "{model} {dt} {q} one ulp off must fail:\n{}", out.output);
                assert!(out.output.contains(&format!("{model} {dt} {q}")), "{}", out.output);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SETTLING-09, SETTLING-10
// ---------------------------------------------------------------------------

#[test]
fn settling_09_the_wrong_model_misses_by_more_than_1e_3() {
    let Some(f) = fx() else { return };
    let s = settle::<f64>(f, 3, "049", Counts::Torch);
    let (m, _, _) = max_err(&s.rust, &s.flyvis);
    assert!(m > 1e-3, "model 049 against model 000's reference: {m:e}");
}

#[test]
fn settling_10_integer_counts_exceed_the_f64_bound() {
    let Some(f) = fx() else { return };
    let s = settle::<f64>(f, 3, "000", Counts::IntegerNSyn);
    let (m, _, _) = max_err(&s.rust, &s.flyvis);
    assert!(m > F64_BOUND, "integer counts: settled max {m:e}");
}

// ---------------------------------------------------------------------------
// SETTLING-11
// ---------------------------------------------------------------------------

#[test]
fn settling_11_a_missing_fixture_skips_or_with_require_fails_naming_it() {
    if fx().is_none() {
        return;
    }
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    let cases = [
        (ENSEMBLE, "settling_exact_run_0_000_1_200_f32"),
        (MANIFEST, "the_gate_tells_models_apart"),
        (CENTRE_F64, "settling_libm_run_1_000_1_200_f64"),
        (PROVENANCE, "pre_registered_bounds_match_provenance"),
    ];
    for (rel, test) in cases {
        sh.data(&[(rel, Alter::Remove)]);
        let path = format!("data/{rel}");
        let out = sh.gate(test, false);
        assert!(out.passed, "{test} without {rel} must skip and pass:\n{}", out.output);
        assert!(
            out.output.lines().any(|l| l.starts_with("skipping:") && l.contains(&format!("{path} not found"))),
            "{test}: skip notice naming {path}:\n{}",
            out.output
        );
        let out = sh.gate(test, true);
        assert!(!out.passed, "{test} without {rel} under SEMBLANCE_REQUIRE_FIXTURES=1 must fail:\n{}", out.output);
        assert!(
            out.output.contains("SEMBLANCE_REQUIRE_FIXTURES=1 but") && out.output.contains(&format!("{path} is missing")),
            "{test}: failure naming {path}:\n{}",
            out.output
        );
    }
}

// ---------------------------------------------------------------------------
// SETTLING-12
// ---------------------------------------------------------------------------

fn ulps32(a: f32, b: f32) -> u32 {
    assert!(a > 0.0 && b > 0.0);
    a.to_bits().abs_diff(b.to_bits())
}

fn ulps64(a: f64, b: f64) -> u64 {
    assert!(a > 0.0 && b > 0.0);
    a.to_bits().abs_diff(b.to_bits())
}

#[test]
fn settling_12_torch_count_tables_sit_within_two_ulps_of_std_exp() {
    assert_eq!(SYN_COUNT_TORCH_F32.len(), SYN_COUNT.len());
    assert_eq!(SYN_COUNT_TORCH_F64.len(), SYN_COUNT.len());
    let (mut d32, mut d64) = (0, 0);
    for (g, &ln) in SYN_COUNT.iter().enumerate() {
        let u = ulps32(SYN_COUNT_TORCH_F32[g], ln.exp());
        assert!(u <= 2, "f32 group {g}: {u} ulps from std exp");
        d32 += usize::from(u != 0);
        let u = ulps64(SYN_COUNT_TORCH_F64[g], f64::from(ln).exp());
        assert!(u <= 2, "f64 group {g}: {u} ulps from std exp");
        d64 += usize::from(u != 0);
    }
    eprintln!("SETTLING-12: f32 table differs from std exp on {d32}, f64 on {d64}");
    assert!(d32 <= 200, "f32 table differs on {d32} entries");
    assert!(d64 <= 200, "f64 table differs on {d64} entries");
}

// ---------------------------------------------------------------------------
// SETTLING-13
// ---------------------------------------------------------------------------

#[test]
fn settling_13_wiring_helpers_on_the_real_network() {
    // failure paths, on a network built here
    let tiny = TypedNetwork::new(TypedNetworkParts {
        cell_types: vec!["A".into(), "B".into()],
        node_cell_type: vec![0, 1],
        node_u: vec![0, 0],
        node_v: vec![0, 0],
        conn_types: vec![(0, 1)],
        conn_sign: vec![1.0],
        edge_source: vec![0],
        edge_target: vec![1],
        edge_conn_type: vec![0],
        edge_n_syn: vec![1.0],
    })
    .unwrap();
    assert_eq!(edge_syn_groups(&tiny), Err(WiringError::GroupCount { expected: 2355, found: 1 }));
    let node = |n: &str| {
        format!(
            r#"{{"name":"{n}","pattern":["stride",[1,1]],"activation":"relu","bias":0.0,
                "bias_fixed":false,"time_constant":0.05,"time_constant_fixed":false}}"#
        )
    };
    let bad = ConnectomeSpec::from_json_str(&format!(
        r#"{{"nodes":[{},{}],"edges":[],"input_units":["A","Z"],"output_units":[]}}"#,
        node("A"),
        node("B")
    ))
    .unwrap();
    assert_eq!(input_nodes(&bad, &tiny), Err(WiringError::UnknownInputUnit("Z".into())));

    let Some(f) = fx() else { return };
    let net = &f.net;
    // groups: one per edge, 2,355 of them, numbered by first appearance
    assert_eq!(f.groups.len(), 1_513_231);
    let mut next = 0u32;
    for (e, &g) in f.groups.iter().enumerate() {
        assert!(g <= next, "edge {e}: group {g} appears before group {next}");
        if g == next {
            next += 1;
        }
    }
    assert_eq!(next, 2355);
    // and edges share a group exactly when (conn type, du, dv) match
    let mut key_of: Vec<Option<(u16, i16, i16)>> = vec![None; 2355];
    for (e, &g) in f.groups.iter().enumerate() {
        let (s, t) = (net.edge_source()[e] as usize, net.edge_target()[e] as usize);
        let key = (net.edge_conn_type()[e], net.node_u()[t] - net.node_u()[s], net.node_v()[t] - net.node_v()[s]);
        match key_of[g as usize] {
            None => key_of[g as usize] = Some(key),
            Some(k) => assert_eq!(k, key, "edge {e} in group {g}"),
        }
    }
    let mut keys: Vec<_> = key_of.iter().map(|k| k.unwrap()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 2355, "distinct groups have distinct keys");

    // input nodes: 8 lists of 721, in input_units order
    let inputs = input_nodes(&f.spec, net).unwrap();
    assert_eq!(inputs.len(), 8);
    for (k, nodes) in inputs.iter().enumerate() {
        assert_eq!(nodes.len(), 721, "input unit {k}");
        let unit = &f.spec.input_units[k];
        let want: Vec<u32> = (0..net.num_nodes() as u32).filter(|&i| cell_type(net, i as usize) == unit).collect();
        assert_eq!(nodes, &want, "input unit {k} ({unit})");
    }

    // sign: the network's equals the checkpoint's, bit for bit
    assert_eq!(net.conn_sign().len(), 604);
    for (k, (a, b)) in net.conn_sign().iter().zip(SIGN.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "connection type {k}");
    }
    assert_eq!(CELL_TYPES.len(), 65);
}
