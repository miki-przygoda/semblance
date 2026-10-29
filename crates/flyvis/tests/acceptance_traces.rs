//! Acceptance tests for behaviour group `traces` of epic stimulus-traces: the
//! stimulus-trace gates, the stimulus check and their controls.
//!
//! One test per behaviour id (TRACES-01 .. TRACES-23).
//!
//! Two kinds of test live here, as in `acceptance_settling.rs`:
//!
//! * In-process tests drive the public API (`semblance_sim::graded`,
//!   `semblance_flyvis::{wiring, reference, stimuli, moving_edge}`) through a
//!   stage-2 run of their own -- settle, then play flyvis's twelve reference
//!   sequences -- and compare it against flyvis's reference, independently of
//!   the crate's own gate code.
//! * The gates themselves are `#[cfg(test)]` tests inside `semblance-flyvis`
//!   (`trace_checks::*`, `settling_checks::*`). To observe how they report a
//!   difference, a non-finite state, a missing fixture or a changed bound,
//!   these tests copy the workspace's sources into `CARGO_TARGET_TMPDIR`, give
//!   that copy a `data/` directory of symlinks to the real fixtures with the
//!   one file under test altered or removed, and run the gate there with
//!   `cargo test`. The real `data/` is never written. Only TRACES-19 edits the
//!   copy's source (the shared `TOLERANCE`), and restores it at once.
//!
//! Everything skips without the real fixtures, unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip into a failure.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

use semblance_connectome::{TypedNetwork, TypedNetworkParts};
use semblance_flyvis::reference::{parse_sequences, Array, BlockKey, Dtype, FileInfo, Manifest, ReferenceError, SequenceInfo};
use semblance_flyvis::stimuli::Stimulus;
use semblance_flyvis::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use semblance_flyvis::pretrained::SYN_COUNT;
use semblance_flyvis::{
    edge_syn_groups, expand, input_nodes, scatter_hexals, ConnectomeSpec, Ensemble, FlashParams, Flashes, MovingEdge,
    MovingEdgeParams, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL,
};
use semblance_sim::graded::{Graded, GradedError, GradedParams, Real};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";
const ENSEMBLE: &str = "flyvis-pretrained/ensemble.csv";
const MANIFEST: &str = "flyvis-reference/manifest.json";
const PROVENANCE: &str = "flyvis-reference/PROVENANCE.json";
const INPUT: &str = "flyvis-reference/input.f64.bin";
const CENTRE_F32: &str = "flyvis-reference/centre.f32.bin";
const CENTRE_F64: &str = "flyvis-reference/centre.f64.bin";
const SNAPSHOTS_F32: &str = "flyvis-reference/snapshots.f32.bin";
const SNAPSHOTS_F64: &str = "flyvis-reference/snapshots.f64.bin";
const SETTLED_F32: &str = "flyvis-reference/settled.f32.bin";
const SETTLED_F64: &str = "flyvis-reference/settled.f64.bin";
const SETTLING_CENTRE_F32: &str = "flyvis-reference/settling_centre.f32.bin";
const SETTLING_CENTRE_F64: &str = "flyvis-reference/settling_centre.f64.bin";

const HEXALS: usize = 721;
const NODES: usize = 45_669;
const SNAPSHOTS: usize = 6;
const F64_BOUND: f64 = 1e-10;
const TOLERANCE: f64 = 0.005;

fn data_path(rel: &str) -> PathBuf {
    Path::new(WORKSPACE).join("data").join(rel)
}

fn fixtures_required() -> bool {
    std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1")
}

fn read_fixture(rel: &str) -> Option<Vec<u8>> {
    let path = data_path(rel);
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {} is missing", path.display());
            eprintln!("skipping: {} not found; run ci/fetch-fixtures.sh", path.display());
            None
        }
    }
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("fixture is UTF-8")
}

struct Fx {
    net: TypedNetwork,
    groups: Vec<u32>,
    /// [8][721], flyvis's `input_index`.
    inputs: Vec<Vec<u32>>,
    ensemble: Ensemble,
    manifest: Manifest,
    manifest_json: String,
    sequences: Vec<SequenceInfo>,
    provenance: String,
    input: Vec<u8>,
    centre_f32: Vec<u8>,
    centre_f64: Vec<u8>,
    snapshots_f32: Vec<u8>,
    snapshots_f64: Vec<u8>,
}

static FX: OnceLock<Option<Fx>> = OnceLock::new();

fn fx() -> Option<&'static Fx> {
    FX.get_or_init(|| {
        let spec_json = text(read_fixture(SPEC)?);
        let ensemble_csv = text(read_fixture(ENSEMBLE)?);
        let manifest_json = text(read_fixture(MANIFEST)?);
        let provenance = text(read_fixture(PROVENANCE)?);
        let input = read_fixture(INPUT)?;
        let centre_f32 = read_fixture(CENTRE_F32)?;
        let centre_f64 = read_fixture(CENTRE_F64)?;
        let snapshots_f32 = read_fixture(SNAPSHOTS_F32)?;
        let snapshots_f64 = read_fixture(SNAPSHOTS_F64)?;
        for rel in [SETTLED_F32, SETTLED_F64, SETTLING_CENTRE_F32, SETTLING_CENTRE_F64] {
            read_fixture(rel)?;
        }
        let spec = ConnectomeSpec::from_json_str(&spec_json).expect("real spec parses");
        let net = expand(&spec, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL).expect("real spec expands");
        let groups = edge_syn_groups(&net).expect("2,355 groups");
        let inputs = input_nodes(&spec, &net).expect("input units are cell types");
        Some(Fx {
            ensemble: Ensemble::parse(&ensemble_csv).expect("ensemble.csv parses"),
            manifest: Manifest::parse(&manifest_json).expect("manifest.json parses"),
            sequences: parse_sequences(&manifest_json).expect("manifest.json's sequences parse"),
            net,
            groups,
            inputs,
            manifest_json,
            provenance,
            input,
            centre_f32,
            centre_f64,
            snapshots_f32,
            snapshots_f64,
        })
    })
    .as_ref()
}

fn cell_type(net: &TypedNetwork, node: usize) -> &str {
    &net.cell_types()[net.node_cell_type()[node] as usize]
}

// ---------------------------------------------------------------------------
// An independent stage-2 run through the public API
// ---------------------------------------------------------------------------

trait P: Real {
    const DTYPE: Dtype;
    const SUFFIX: &'static str;
    fn torch(g: usize) -> Self;
    fn libm(g: usize) -> Self;
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>;
    fn centre_bytes(f: &Fx) -> &[u8];
    fn snapshot_bytes(f: &Fx) -> &[u8];
}

impl P for f32 {
    const DTYPE: Dtype = Dtype::F32;
    const SUFFIX: &'static str = "f32";
    fn torch(g: usize) -> Self { SYN_COUNT_TORCH_F32[g] }
    fn libm(g: usize) -> Self { SYN_COUNT[g].exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f32(bytes, key)
    }
    fn centre_bytes(f: &Fx) -> &[u8] { &f.centre_f32 }
    fn snapshot_bytes(f: &Fx) -> &[u8] { &f.snapshots_f32 }
}

impl P for f64 {
    const DTYPE: Dtype = Dtype::F64;
    const SUFFIX: &'static str = "f64";
    fn torch(g: usize) -> Self { SYN_COUNT_TORCH_F64[g] }
    fn libm(g: usize) -> Self { f64::from(SYN_COUNT[g]).exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f64(bytes, key)
    }
    fn centre_bytes(f: &Fx) -> &[u8] { &f.centre_f64 }
    fn snapshot_bytes(f: &Fx) -> &[u8] { &f.snapshots_f64 }
}

#[derive(Clone, Copy, PartialEq)]
enum Counts {
    Torch,
    Libm,
}

/// Largest absolute error (NaN as infinity), where, how many unequal.
#[derive(Clone, Copy, Debug, Default)]
struct Diff {
    max: f64,
    at: usize,
    differ: usize,
}

fn diff(a: &[f64], b: &[f64]) -> Diff {
    assert_eq!(a.len(), b.len());
    let mut d = Diff::default();
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        let e = (x - y).abs();
        let e = if e.is_nan() { f64::INFINITY } else { e };
        d.differ += usize::from(x != y);
        if e > d.max {
            d.max = e;
            d.at = i;
        }
    }
    d
}

/// One sequence played and compared.
#[derive(Clone, Debug)]
struct Seq {
    sequence: u32,
    n_frames: usize,
    snapshot_frames: Vec<usize>,
    /// Steps taken in this sequence.
    steps: usize,
    input_shape: Vec<usize>,
    centre_shape: Vec<usize>,
    snapshots_shape: Vec<usize>,
    /// Values recorded by Rust: centre per frame, full state per snapshot.
    centre_len: usize,
    snapshots_len: usize,
    /// Every state after every step was finite.
    finite: bool,
    centre: Diff,
    snapshots: Diff,
    /// The first unequal centre value, an index into [n_frames, 65].
    first: Option<usize>,
}

/// flyvis's settling (piece 1) through the public API: `n_settle` steps of
/// grey on every photoreceptor, from the bias state.
fn settle<R: P>(f: &Fx, run: u32, counts: Counts) -> Graded<R> {
    let info = &f.manifest.runs[run as usize];
    assert_eq!(info.index, run);
    assert_eq!(info.precision, R::DTYPE, "run {run}'s precision");
    let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("dt listed");
    let p = f.ensemble.model(&info.model).expect("model in the ensemble");
    let count: Vec<R> = f
        .groups
        .iter()
        .map(|&g| match counts {
            Counts::Torch => R::torch(g as usize),
            Counts::Libm => R::libm(g as usize),
        })
        .collect();
    let params = GradedParams { bias: &p.bias, time_const: &p.time_const, syn_strength: &p.syn_strength, edge_syn_count: &count };
    let mut g = Graded::<R>::new(&f.net, &params, dt.dt).expect("valid parameters");
    let mut input = vec![R::ZERO; f.net.num_nodes()];
    for &i in f.inputs.iter().flatten() {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    for _ in 0..dt.n_settle {
        g.step(&input).expect("input length");
    }
    g
}

/// For each hexal h, the hexal at (-u, -v), from the input nodes' own
/// coordinates on the expanded network.
fn mirrored_hexals(f: &Fx) -> Vec<usize> {
    let uv: Vec<(i16, i16)> = f.inputs[0].iter().map(|&n| (f.net.node_u()[n as usize], f.net.node_v()[n as usize])).collect();
    uv.iter().map(|&(u, v)| uv.iter().position(|&p| p == (-u, -v)).expect("point-symmetric lattice")).collect()
}

/// Stage 2: settle to S0, zero the input buffer once, then per sequence
/// reset to S0, read its input block, scatter every frame and step.
fn play<R: P>(f: &Fx, run: u32, counts: Counts, sequences: &[u32], mirror: bool) -> Vec<Seq> {
    let info = &f.manifest.runs[run as usize];
    let mut g = settle::<R>(f, run, counts);
    let s0 = g.state().to_vec();
    let n = f.net.num_nodes();
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let source: Vec<usize> = if mirror { mirrored_hexals(f) } else { (0..HEXALS).collect() };
    let input_file = f.manifest.file("input.f64.bin").expect("input.f64.bin in manifest");
    let centre_file = f.manifest.file(&format!("centre.{}.bin", R::SUFFIX)).expect("centre in manifest");
    let snapshots_file = f.manifest.file(&format!("snapshots.{}.bin", R::SUFFIX)).expect("snapshots in manifest");

    let mut input = vec![R::ZERO; n];
    let mut frame = vec![R::ZERO; HEXALS];
    let mut out = Vec::new();
    for &s in sequences {
        let q = &f.sequences[s as usize];
        assert_eq!(q.index, s);
        let layout = q.frames(&info.dt).expect("every sequence has every dt");
        g.set_state(&s0).expect("S0 is finite and full length");
        let x = input_file
            .read_f64(&f.input, &BlockKey { run: None, dt: &info.dt, sequence: Some(s) })
            .expect("input block");
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(s) };
        let fc = R::read(centre_file, R::centre_bytes(f), &key).expect("centre block");
        let fs = R::read(snapshots_file, R::snapshot_bytes(f), &key).expect("snapshots block");
        let mut rc = Vec::new();
        let mut rs = Vec::new();
        let (mut steps, mut finite) = (0, true);
        for i in 0..layout.n_frames {
            let row = &x.data[i * HEXALS..(i + 1) * HEXALS];
            for (slot, &h) in frame.iter_mut().zip(&source) {
                *slot = R::from_f64(row[h]);
            }
            scatter_hexals(&f.inputs, &frame, &mut input).expect("scatter");
            g.step(&input).expect("step");
            steps += 1;
            finite &= g.state().iter().all(|v| v.is_finite());
            rc.extend(centre.iter().map(|&k| g.state()[k].to_f64()));
            if layout.snapshot_frames.contains(&i) {
                rs.extend(g.state().iter().map(|v| v.to_f64()));
            }
        }
        let fc64: Vec<f64> = fc.data.iter().map(|v| v.to_f64()).collect();
        let fs64: Vec<f64> = fs.data.iter().map(|v| v.to_f64()).collect();
        let same_len = rc.len() == fc64.len() && rs.len() == fs64.len();
        out.push(Seq {
            sequence: s,
            n_frames: layout.n_frames,
            snapshot_frames: layout.snapshot_frames.clone(),
            steps,
            input_shape: x.shape.clone(),
            centre_shape: fc.shape.clone(),
            snapshots_shape: fs.shape.clone(),
            centre_len: rc.len(),
            snapshots_len: rs.len(),
            finite,
            first: rc.iter().zip(&fc64).position(|(a, b)| a != b),
            centre: if same_len { diff(&rc, &fc64) } else { Diff { max: f64::INFINITY, at: 0, differ: usize::MAX } },
            snapshots: if same_len { diff(&rs, &fs64) } else { Diff { max: f64::INFINITY, at: 0, differ: usize::MAX } },
        });
    }
    out
}

/// All twelve sequences of `run` under `counts`, played once per process.
fn played(f: &Fx, run: u32, counts: Counts) -> &'static [Seq] {
    static CACHE: [[OnceLock<Vec<Seq>>; 2]; 8] = [const { [const { OnceLock::new() }, const { OnceLock::new() }] }; 8];
    let c = usize::from(counts == Counts::Libm);
    CACHE[run as usize][c].get_or_init(|| {
        let all: Vec<u32> = f.sequences.iter().map(|q| q.index).collect();
        match f.manifest.runs[run as usize].precision {
            Dtype::F32 => play::<f32>(f, run, counts, &all, false),
            Dtype::F64 => play::<f64>(f, run, counts, &all, false),
        }
    })
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

/// R3's (centre, snapshots) bounds for a run.
fn trace_bounds(f: &Fx, run: u32) -> (f64, f64) {
    let info = &f.manifest.runs[run as usize];
    match info.precision {
        Dtype::F64 => (F64_BOUND, F64_BOUND),
        Dtype::F32 => (
            2.0 * provenance_gap(&f.provenance, &info.model, &info.dt, "centre"),
            2.0 * provenance_gap(&f.provenance, &info.model, &info.dt, "snapshots"),
        ),
    }
}

fn block_offset(m: &Manifest, name: &str, run: Option<u32>, dt: &str, sequence: Option<u32>) -> usize {
    m.file(name)
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.run == run && b.dt == dt && b.sequence == sequence)
        .unwrap_or_else(|| panic!("{name}: no block for {run:?} {dt} {sequence:?}"))
        .offset
}

fn f64_at(bytes: &[u8], at: usize) -> f64 {
    f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

fn put_f64(bytes: &mut [u8], at: usize, v: f64) {
    bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

// ---------------------------------------------------------------------------
// Manifest text edits (for the shadow)
// ---------------------------------------------------------------------------

/// Replaces the `"shape": [...]` of `file`'s block (run, dt, sequence).
fn replace_block_shape(json: &str, file: &str, run: u32, dt: &str, sequence: u32, shape: &[usize]) -> String {
    let start = json.find(&format!("\"name\": \"{file}\"")).expect("file in manifest");
    let end = json[start + 1..].find("\"name\": \"").map_or(json.len(), |e| start + 1 + e);
    let run_pat = format!("\"run\": {run},");
    let mut at = start;
    loop {
        let r = at + json[at..end].find(&run_pat).expect("block for the run");
        let window = &json[r..(r + 120).min(end)];
        if window.contains(&format!("\"dt\": \"{dt}\"")) && window.contains(&format!("\"sequence\": {sequence},")) {
            let s = r + json[r..].find("\"shape\": [").unwrap();
            let e = s + json[s..].find(']').unwrap() + 1;
            let dims: Vec<String> = shape.iter().map(|d| d.to_string()).collect();
            let mut out = json.to_string();
            out.replace_range(s..e, &format!("\"shape\": [{}]", dims.join(", ")));
            return out;
        }
        at = r + run_pat.len();
    }
}

/// Changes sequence `index`'s `n_frames` at dt `dt`.
fn replace_n_frames(json: &str, index: u32, dt: &str, n_frames: usize) -> String {
    let seqs = json.find("\"sequences\"").expect("sequences");
    let s = seqs + json[seqs..].find(&format!("\"index\": {index},")).expect("sequence index");
    let d = s + json[s..].find(&format!("\"{dt}\": {{")).expect("dt entry");
    let k = d + json[d..].find("\"n_frames\": ").unwrap() + "\"n_frames\": ".len();
    let len = json[k..].find(',').unwrap();
    let mut out = json.to_string();
    out.replace_range(k..k + len, &n_frames.to_string());
    out
}

/// The inclusive-exclusive offsets flyvis lists for sequence `index`.
fn manifest_offsets(json: &str, index: u32) -> Vec<f64> {
    let seqs = json.find("\"sequences\"").expect("sequences");
    let s = seqs + json[seqs..].find(&format!("\"index\": {index},")).expect("sequence index");
    let o = s + json[s..].find("\"offsets\": [").expect("offsets") + "\"offsets\": [".len();
    let e = o + json[o..].find(']').unwrap();
    json[o..e].split(',').map(|v| v.trim().parse().expect("numeric offset")).collect()
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
            std::fs::write(&dst, std::fs::read(&src).unwrap()).unwrap();
        }
    }
}

fn shadow() -> MutexGuard<'static, Option<Shadow>> {
    let mut guard = SHADOW.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-traces");
        let root = base.join("workspace");
        let ws = Path::new(WORKSPACE);
        match std::fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("removing the old shadow {}: {e}", root.display()),
        }
        std::fs::create_dir_all(&root).unwrap();
        for f in ["Cargo.toml", "Cargo.lock", "README.md"] {
            std::fs::write(root.join(f), std::fs::read(ws.join(f)).unwrap()).unwrap();
        }
        copy_tree(&ws.join("crates"), &root.join("crates"));
        copy_tree(&ws.join("app"), &root.join("app"));
        *guard = Some(Shadow { root, target: base.join("target") });
    }
    guard
}

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

    fn cargo(&self) -> Command {
        let mut cmd = Command::new(option_env!("CARGO").unwrap_or("cargo"));
        cmd.current_dir(&self.root).env("CARGO_TARGET_DIR", &self.target);
        cmd
    }

    /// Runs one of semblance-flyvis's own lib tests (full path) in the shadow.
    fn gate(&self, test: &str, require: bool) -> Outcome {
        let mut cmd = self.cargo();
        cmd.args(["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--", "--exact"])
            .arg(test)
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

/// `{:.3e}` shape: one digit, a point, three digits, `e`, an exponent.
fn is_3e(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 7
        && b[0].is_ascii_digit()
        && b[1] == b'.'
        && b[2..5].iter().all(u8::is_ascii_digit)
        && b[5] == b'e'
        && s[6..].trim_start_matches('-').chars().all(|c| c.is_ascii_digit())
        && s.len() > 6
}

const EXACT_GATES: [&str; 8] = [
    "trace_exact_run_0_000_1_200_f32",
    "trace_exact_run_1_000_1_200_f64",
    "trace_exact_run_2_000_1_50_f32",
    "trace_exact_run_3_000_1_50_f64",
    "trace_exact_run_4_049_1_200_f32",
    "trace_exact_run_5_049_1_200_f64",
    "trace_exact_run_6_049_1_50_f32",
    "trace_exact_run_7_049_1_50_f64",
];

fn tc(name: &str) -> String {
    format!("trace_checks::{name}")
}

fn sc(name: &str) -> String {
    format!("trace_checks::stimulus_checks::{name}")
}

// ---------------------------------------------------------------------------
// TRACES-01, TRACES-02: Graded::set_state
// ---------------------------------------------------------------------------

/// Three nodes, two cell types, one excitatory connection type, two edges.
fn small() -> TypedNetwork {
    TypedNetwork::new(TypedNetworkParts {
        cell_types: vec!["A".into(), "B".into()],
        node_cell_type: vec![0, 0, 1],
        node_u: vec![0; 3],
        node_v: vec![0; 3],
        conn_types: vec![(0, 1)],
        conn_sign: vec![1.0],
        edge_source: vec![0, 1],
        edge_target: vec![2, 2],
        edge_conn_type: vec![0, 0],
        edge_n_syn: vec![1.0; 2],
    })
    .expect("a consistent test network")
}

fn small_graded<R: Real>(counts: &[R]) -> Graded<R> {
    let p = GradedParams { bias: &[0.1, -0.2], time_const: &[0.03, 0.05], syn_strength: &[0.7], edge_syn_count: counts };
    Graded::new(&small(), &p, 0.005).expect("valid")
}

fn set_state_then_step<R: Real>(counts: &[R], state: &[R], input: &[R]) {
    // Reach a state by stepping, then restart a fresh integrator from it.
    let mut a = small_graded(counts);
    for _ in 0..7 {
        a.step(input).unwrap();
    }
    let reached = a.state().to_vec();
    let mut b = small_graded(counts);
    b.set_state(&reached).expect("a finite state of the right length");
    assert_eq!(format!("{:?}", b.state()), format!("{reached:?}"));
    a.step(input).unwrap();
    b.step(input).unwrap();
    assert_eq!(format!("{:?}", a.state()), format!("{:?}", b.state()), "set_state + step == stepping from that state");

    // An arbitrary finite state: the step is flyvis's Euler step from it.
    let mut c = small_graded(counts);
    c.set_state(state).unwrap();
    c.step(input).unwrap();
    let bias = [R::from_f32(0.1), R::from_f32(0.1), R::from_f32(-0.2)];
    let inv_tau = [R::ONE / R::from_f32(0.03), R::ONE / R::from_f32(0.03), R::ONE / R::from_f32(0.05)];
    let relu = |x: R| if x > R::ZERO { x } else { R::ZERO };
    let w = |e: usize| R::from_f32(1.0) * counts[e] * R::from_f32(0.7);
    let sums = [R::ZERO, R::ZERO, R::ZERO + w(0) * relu(state[0]) + w(1) * relu(state[1])];
    let want: Vec<R> = (0..3)
        .map(|i| {
            let vel = inv_tau[i] * (((-state[i] + bias[i]) + sums[i]) + input[i]);
            state[i] + vel * R::from_f64(0.005)
        })
        .collect();
    assert_eq!(format!("{:?}", c.state()), format!("{want:?}"), "one Euler step from the set state");
}

#[test]
fn traces_01_set_state_then_step_equals_stepping_from_that_state() {
    set_state_then_step::<f64>(&[2.0, 3.0], &[0.4, -1.3, 0.25], &[0.5, 0.0, 0.0]);
    set_state_then_step::<f32>(&[2.0, 3.0], &[0.4, -1.3, 0.25], &[0.5, 0.0, 0.0]);
}

fn set_state_refuses<R: Real>(counts: &[R], nan: R, inf: R) {
    let input = [R::from_f64(0.5), R::ZERO, R::ZERO];
    let mut g = small_graded(counts);
    for _ in 0..3 {
        g.step(&input).unwrap();
    }
    let before = format!("{:?}", g.state());
    let mut twin = g.clone();
    let f = R::from_f64(0.25);
    let cases: Vec<(Vec<R>, bool)> = vec![
        (vec![], true),
        (vec![f; 2], true),
        (vec![f; 4], true),
        (vec![nan, f, f], false),
        (vec![f, inf, f], false),
        (vec![f, f, -inf], false),
    ];
    for (state, is_length) in cases {
        let r = g.set_state(&state);
        match r {
            Err(GradedError::Length { expected: 3, found, .. }) if is_length => assert_eq!(found, state.len()),
            Err(GradedError::NonFinite { index, .. }) if !is_length => assert!(!state[index].is_finite()),
            other => panic!("set_state({state:?}) must be a GradedError of the right kind, got {other:?}"),
        }
        assert_eq!(format!("{:?}", g.state()), before, "a refused state changes nothing");
    }
    g.step(&input).unwrap();
    twin.step(&input).unwrap();
    assert_eq!(format!("{:?}", g.state()), format!("{:?}", twin.state()), "and later steps are unaffected");
}

#[test]
fn traces_02_set_state_refuses_a_wrong_length_or_non_finite_state() {
    set_state_refuses::<f64>(&[2.0, 3.0], f64::NAN, f64::INFINITY);
    set_state_refuses::<f32>(&[2.0, 3.0], f32::NAN, f32::INFINITY);
}

// ---------------------------------------------------------------------------
// TRACES-03 .. TRACES-07: the exact trace gates
// ---------------------------------------------------------------------------

#[test]
fn traces_03_a_stage_2_run_plays_every_sequence_from_s0_with_flyviss_input() {
    let Some(f) = fx() else { return };
    // Following exactly the procedure the behaviour describes reproduces
    // flyvis bit for bit, in both precisions.
    for run in [0u32, 3] {
        let p = played(f, run, Counts::Torch);
        assert_eq!(p.iter().map(|s| s.sequence).collect::<Vec<_>>(), (0..12).collect::<Vec<u32>>(), "sequences 0-11");
        for s in p {
            assert_eq!(s.input_shape, [s.n_frames, HEXALS], "run {run} sequence {}: input block shape", s.sequence);
            assert_eq!(s.steps, s.n_frames, "run {run} sequence {}: one step per frame", s.sequence);
            assert_eq!((s.centre.differ, s.snapshots.differ), (0, 0), "run {run} sequence {}", s.sequence);
        }
    }
    // Without the reset to S0 between sequences, sequence 1 would not match:
    // continuing from sequence 0's end state misses.
    let info = &f.manifest.runs[3];
    let mut g = settle::<f64>(f, 3, Counts::Torch);
    let input_file = f.manifest.file("input.f64.bin").unwrap();
    let mut input = vec![0.0f64; NODES];
    for s in 0..2u32 {
        let x = input_file.read_f64(&f.input, &BlockKey { run: None, dt: &info.dt, sequence: Some(s) }).unwrap();
        let n_frames = x.shape[0];
        for i in 0..n_frames {
            scatter_hexals(&f.inputs, &x.data[i * HEXALS..(i + 1) * HEXALS], &mut input).unwrap();
            g.step(&input).unwrap();
        }
    }
    let key = BlockKey { run: Some(3), dt: &info.dt, sequence: Some(1) };
    let fs = f.manifest.file("snapshots.f64.bin").unwrap().read_f64(&f.snapshots_f64, &key).unwrap();
    let last = &fs.data[(SNAPSHOTS - 1) * NODES..];
    let d = diff(g.state(), last);
    assert!(d.differ > 0, "without resetting to S0 the trace must not be bit-equal (max {:e})", d.max);

    // And the crate's gate, which runs this procedure, passes run 3.
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate(&tc("trace_exact_run_3_000_1_50_f64"), true);
    assert!(out.passed, "trace_exact_run_3 must pass:\n{}", out.output);
}

#[test]
fn traces_04_each_step_records_the_centre_snapshots_and_scans_for_non_finite() {
    let Some(f) = fx() else { return };
    for s in played(f, 3, Counts::Torch) {
        assert_eq!(s.centre_len, s.n_frames * 65, "sequence {}: 65 centre values per frame", s.sequence);
        assert_eq!(s.snapshots_len, SNAPSHOTS * NODES, "sequence {}: the full state at each snapshot frame", s.sequence);
        assert_eq!(s.snapshot_frames.len(), SNAPSHOTS);
        assert!(s.finite, "sequence {}: every state finite", s.sequence);
    }
    // The gate scans the state after every step: an infinite input value at
    // frame 10 of sequence 0 makes it fail there, naming node and cell type.
    let info = &f.manifest.runs[3];
    let mut input = f.input.clone();
    let at = block_offset(&f.manifest, "input.f64.bin", None, &info.dt, Some(0)) + 8 * (10 * HEXALS);
    put_f64(&mut input, at, f64::INFINITY);
    let node = f.inputs.iter().map(|t| t[0] as usize).min().unwrap();
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[(INPUT, Alter::Replace(input))]);
    let out = sh.gate(&tc("trace_exact_run_3_000_1_50_f64"), true);
    assert!(!out.passed, "a non-finite state must fail:\n{}", out.output);
    let want = format!("run 3: node {node} ({}) is not finite after frame 10 of sequence 0", cell_type(&f.net, node));
    assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);
}

#[test]
fn traces_05_a_sequences_reference_shapes_are_asserted() {
    let Some(f) = fx() else { return };
    for run in [0u32, 3] {
        for s in played(f, run, Counts::Torch) {
            assert_eq!(s.centre_shape, [s.n_frames, 65], "run {run} sequence {}: centre shape", s.sequence);
            assert_eq!(s.snapshots_shape, [SNAPSHOTS, NODES], "run {run} sequence {}: snapshots shape", s.sequence);
        }
    }
    // The gate refuses a reference of the wrong shape (same byte count).
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    let n0 = f.sequences[0].frames("1/50").unwrap().n_frames;
    for (file, shape, msg) in [
        ("centre.f64.bin", vec![65, n0], "centre shape, sequence 0"),
        ("snapshots.f64.bin", vec![NODES, SNAPSHOTS], "snapshots shape, sequence 0"),
    ] {
        let m = replace_block_shape(&f.manifest_json, file, 3, "1/50", 0, &shape);
        assert!(Manifest::parse(&m).is_ok(), "the edited manifest still parses");
        sh.data(&[(MANIFEST, Alter::Replace(m.into_bytes()))]);
        let out = sh.gate(&tc("trace_exact_run_3_000_1_50_f64"), true);
        assert!(!out.passed, "{file} with shape {shape:?} must fail:\n{}", out.output);
        assert!(out.output.contains(msg), "expected {msg:?}:\n{}", out.output);
    }
}

#[test]
fn traces_06_torch_counts_trace_bit_equal_to_flyvis_in_all_eight_runs() {
    let Some(f) = fx() else { return };
    for run in 0..8u32 {
        let p = played(f, run, Counts::Torch);
        assert_eq!(p.len(), 12);
        for s in p {
            assert_eq!(s.first, None, "run {run} sequence {}: no centre value differs", s.sequence);
            assert_eq!(
                (s.centre.differ, s.snapshots.differ),
                (0, 0),
                "run {run} sequence {}: centre / snapshot values differing",
                s.sequence
            );
        }
    }
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    for (run, name) in EXACT_GATES.iter().enumerate() {
        let out = sh.gate(&tc(name), true);
        assert!(out.passed, "{name} must pass:\n{}", out.output);
        let info = &f.manifest.runs[run];
        let want = format!(
            "trace exact run {run} ({}, dt {}, {}): 12 of 12 sequences bit-equal, centre and snapshots",
            info.model, info.dt, if info.precision == Dtype::F32 { "f32" } else { "f64" }
        );
        assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);
    }
}

#[test]
fn traces_07_an_exact_trace_failure_prints_counts_first_frame_and_largest_error() {
    let Some(f) = fx() else { return };
    let (seq, frame, c) = (5u32, 10usize, 20usize);
    let layout = f.sequences[seq as usize].frames("1/50").unwrap();
    assert!(!layout.snapshot_frames.contains(&frame));
    let mut centre = f.centre_f64.clone();
    let at = block_offset(&f.manifest, "centre.f64.bin", Some(3), "1/50", Some(seq)) + 8 * (frame * 65 + c);
    let original = f64_at(&centre, at);
    put_f64(&mut centre, at, original + 1000.0);
    let err = (original - (original + 1000.0)).abs();

    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[(CENTRE_F64, Alter::Replace(centre))]);
    let out = sh.gate(&tc("trace_exact_run_3_000_1_50_f64"), true);
    assert!(!out.passed, "a differing value must fail:\n{}", out.output);
    let o = &out.output;
    assert!(o.contains("11 of 12 sequences bit-equal"), "{o}");
    let want = format!(
        "sequence {seq}: 1 of {} centre and 0 of {} snapshot values differ; first difference at frame {frame} ({}); \
         largest centre error {err:e}",
        layout.n_frames * 65,
        SNAPSHOTS * NODES,
        f.manifest.centre_nodes[c].cell_type
    );
    assert!(o.contains(&want), "expected {want:?}:\n{o}");
    assert!(!o.contains("sequence 4:") && !o.contains("sequence 6:"), "only the differing sequence is reported:\n{o}");
}

// ---------------------------------------------------------------------------
// TRACES-08, TRACES-09: the libm trace gates
// ---------------------------------------------------------------------------

#[test]
fn traces_08_rusts_exp_traces_within_the_spec_bounds_in_all_eight_runs() {
    let Some(f) = fx() else { return };
    for run in 0..8u32 {
        let (bc, bs) = trace_bounds(f, run);
        let p = played(f, run, Counts::Libm);
        assert_eq!(p.len(), 12);
        for s in p {
            assert!(s.centre.max <= bc, "run {run} sequence {}: centre {:e} over {bc:e}", s.sequence, s.centre.max);
            assert!(s.snapshots.max <= bs, "run {run} sequence {}: snapshots {:e} over {bs:e}", s.sequence, s.snapshots.max);
        }
        let worst = p.iter().map(|s| (s.centre.max / bc).max(s.snapshots.max / bs)).fold(0.0, f64::max);
        eprintln!("TRACES-08 run {run}: worst ratio {worst:.3e} (centre bound {bc:e}, snapshots bound {bs:e})");
    }
    // the f32 bounds are flyvis's gaps, doubled, centre and snapshots apart
    assert_eq!(trace_bounds(f, 0), (2.0 * 1.7249018977594233e-06, 2.0 * 2.6318956276583094e-06));
}

fn check_libm_line(f: &Fx, run: u32, line: &str) {
    let p = played(f, run, Counts::Libm);
    let (bc, bs) = trace_bounds(f, run);
    let n_centre = 65;
    let ratio = |s: &Seq| (s.centre.max / bc).max(s.snapshots.max / bs);
    let worst = p.iter().max_by(|a, b| ratio(a).total_cmp(&ratio(b))).unwrap();
    assert_eq!(field(line, "worst sequence ", ','), worst.sequence.to_string(), "{line}");
    let r = field(line, &format!("worst sequence {}, ", worst.sequence), ' ');
    assert!(is_3e(r), "ratio in {{:.3e}}: {r:?} in {line}");
    assert_eq!(r, format!("{:.3e}", ratio(worst)), "{line}");

    let c = p.iter().max_by(|a, b| a.centre.max.total_cmp(&b.centre.max)).unwrap();
    let cnode = &f.manifest.centre_nodes[c.centre.at % n_centre];
    let want_c = format!(
        "centre max {:.3e} in sequence {} at frame {} node {} ({}), {:.3e} of bound",
        c.centre.max, c.sequence, c.centre.at / n_centre, cnode.node, cnode.cell_type, c.centre.max / bc
    );
    assert!(line.contains(&want_c), "expected {want_c:?} in {line}");

    let s = p.iter().max_by(|a, b| a.snapshots.max.total_cmp(&b.snapshots.max)).unwrap();
    let snode = s.snapshots.at % NODES;
    let want_s = format!(
        "snapshots max {:.3e} in sequence {} at frame {} node {snode} ({}), {:.3e} of bound",
        s.snapshots.max,
        s.sequence,
        s.snapshot_frames[s.snapshots.at / NODES],
        cell_type(&f.net, snode),
        s.snapshots.max / bs
    );
    assert!(line.contains(&want_s), "expected {want_s:?} in {line}");

    let (equal, total) = p.iter().fold((0, 0), |(e, t), s| {
        let size = s.n_frames * n_centre + SNAPSHOTS * NODES;
        (e + size - s.centre.differ - s.snapshots.differ, t + size)
    });
    assert!(line.ends_with(&format!("bit-equal {equal} of {total}")), "bit-equal fraction {equal} of {total}: {line}");
}

#[test]
fn traces_09_a_libm_trace_run_prints_worst_sequence_maxima_ratios_and_bit_equal_fraction() {
    let Some(f) = fx() else { return };
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    for (run, name) in [(2u32, "trace_libm_run_2_000_1_50_f32"), (3, "trace_libm_run_3_000_1_50_f64")] {
        let out = sh.gate(&tc(name), true);
        assert!(out.passed, "{name} must pass:\n{}", out.output);
        let line = line_with(&out.output, &format!("trace libm run {run} "));
        check_libm_line(f, run, line);
    }
}

// ---------------------------------------------------------------------------
// TRACES-10 .. TRACES-12: bounds, reference self-consistency, wrong mapping
// ---------------------------------------------------------------------------

#[test]
fn traces_10_the_trace_bounds_are_provenances_gaps_bit_for_bit() {
    let Some(f) = fx() else { return };
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let test = tc("trace_bounds_match_provenance");
    let out = sh.gate(&test, true);
    assert!(out.passed, "unaltered PROVENANCE.json must match:\n{}", out.output);
    for model in ["000", "049"] {
        for dt in ["1/200", "1/50"] {
            for q in ["centre", "snapshots"] {
                let (start, len) = provenance_gap_span(&f.provenance, model, dt, q);
                let v: f64 = f.provenance[start..start + len].parse().unwrap();
                let next = f64::from_bits(v.to_bits() + 1);
                let mut p = f.provenance.clone();
                p.replace_range(start..start + len, &format!("{next:e}"));
                assert_eq!(provenance_gap(&p, model, dt, q).to_bits(), next.to_bits());
                sh.data(&[(PROVENANCE, Alter::Replace(p.into_bytes()))]);
                let out = sh.gate(&test, true);
                assert!(!out.passed, "{model} {dt} {q} one ulp off must fail:\n{}", out.output);
                assert!(out.output.contains(&format!("{model} {dt} {q}")), "{}", out.output);
            }
        }
    }
}

fn centre_column_agrees<R: P>(f: &Fx) -> usize {
    let centre_file = f.manifest.file(&format!("centre.{}.bin", R::SUFFIX)).unwrap();
    let snapshots_file = f.manifest.file(&format!("snapshots.{}.bin", R::SUFFIX)).unwrap();
    let mut checked = 0;
    for info in f.manifest.runs.iter().filter(|r| r.precision == R::DTYPE) {
        for q in &f.sequences {
            let layout = q.frames(&info.dt).unwrap();
            let key = BlockKey { run: Some(info.index), dt: &info.dt, sequence: Some(q.index) };
            let c = R::read(centre_file, R::centre_bytes(f), &key).unwrap();
            let s = R::read(snapshots_file, R::snapshot_bytes(f), &key).unwrap();
            for (k, &frame) in layout.snapshot_frames.iter().enumerate() {
                for (j, cn) in f.manifest.centre_nodes.iter().enumerate() {
                    let (a, b) = (c.data[frame * 65 + j].to_f64(), s.data[k * NODES + cn.node as usize].to_f64());
                    assert_eq!(a.to_bits(), b.to_bits(), "run {} sequence {} frame {frame} node {}", info.index, q.index, cn.node);
                }
            }
            checked += 1;
        }
    }
    checked
}

#[test]
fn traces_11_snapshots_agree_with_the_centre_column() {
    let Some(f) = fx() else { return };
    assert_eq!(centre_column_agrees::<f32>(f), 48);
    assert_eq!(centre_column_agrees::<f64>(f), 48);

    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let test = tc("snapshots_agree_with_the_centre_column");
    let out = sh.gate(&test, true);
    assert!(out.passed, "the reference agrees with itself:\n{}", out.output);
    // one snapshot value at a centre node, changed, must fail it
    let node = f.manifest.centre_nodes[7].node as usize;
    let frame = f.sequences[0].frames("1/200").unwrap().snapshot_frames[1];
    let mut snaps = f.snapshots_f64.clone();
    let at = block_offset(&f.manifest, "snapshots.f64.bin", Some(1), "1/200", Some(0)) + 8 * (NODES + node);
    let v = f64_at(&snaps, at);
    put_f64(&mut snaps, at, v + 1e-12);
    sh.data(&[(SNAPSHOTS_F64, Alter::Replace(snaps))]);
    let out = sh.gate(&test, true);
    assert!(!out.passed, "a changed snapshot value must fail:\n{}", out.output);
    let want = format!("run 1 sequence 0 snapshot 1 (frame {frame}), node {node}");
    assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);
}

#[test]
fn traces_12_a_mirrored_mapping_misses_by_more_than_1e_3() {
    let Some(f) = fx() else { return };
    let p = &play::<f64>(f, 3, Counts::Torch, &[8], true)[0];
    assert_eq!(p.sequence, 8);
    assert_eq!(f.sequences[8].family, "moving_edge");
    assert_eq!(f.sequences[8].arg("angle"), Some(90.0));
    assert_eq!(f.sequences[8].arg("intensity"), Some(0.0), "the OFF edge");
    assert!(p.snapshots.max > 1e-3, "mirrored: snapshots max {:e}", p.snapshots.max);

    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate(&tc("the_trace_gate_sees_a_wrong_mapping"), true);
    assert!(out.passed, "the control must pass:\n{}", out.output);
    let line = line_with(&out.output, "mirrored mapping, run 3, sequence 8: snapshots max ");
    let printed: f64 = field(line, "snapshots max ", ',').parse().unwrap();
    assert_eq!(printed.to_bits(), p.snapshots.max.to_bits(), "the gate's mirror equals ours: {line}");
}

// ---------------------------------------------------------------------------
// TRACES-13 .. TRACES-15: loading, the settling gates, skipping
// ---------------------------------------------------------------------------

#[test]
fn traces_13_trace_files_are_loaded_lazily_and_only_by_trace_tests() {
    if fx().is_none() {
        return;
    }
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    // Without any trace file, the settling gates still pass under
    // SEMBLANCE_REQUIRE_FIXTURES=1, and so does the trace-bounds check.
    sh.data(&[
        (INPUT, Alter::Remove),
        (CENTRE_F32, Alter::Remove),
        (CENTRE_F64, Alter::Remove),
        (SNAPSHOTS_F32, Alter::Remove),
        (SNAPSHOTS_F64, Alter::Remove),
    ]);
    for test in [
        "settling_checks::settling_exact_run_0_000_1_200_f32",
        "settling_checks::settling_libm_run_3_000_1_50_f64",
        "settling_checks::reference_runs_are_the_eight_expected",
        "settling_checks::the_gate_tells_models_apart",
        "trace_checks::trace_bounds_match_provenance",
    ] {
        let out = sh.gate(test, true);
        assert!(out.passed, "{test} must not need the trace files:\n{}", out.output);
        assert!(!out.output.contains("skipping"), "{test} must not skip:\n{}", out.output);
    }
    // Each trace file is its own: without the f32 trace files, an f64 trace
    // test still runs in full, while an f32 one fails naming its file.
    sh.data(&[(CENTRE_F32, Alter::Remove), (SNAPSHOTS_F32, Alter::Remove)]);
    let out = sh.gate(&tc("the_trace_gate_sees_a_wrong_mapping"), true);
    assert!(out.passed && out.output.contains("mirrored mapping, run 3"), "f64 trace test without f32 files:\n{}", out.output);
    let out = sh.gate(&tc("trace_exact_run_2_000_1_50_f32"), true);
    assert!(!out.passed, "an f32 trace test needs its files:\n{}", out.output);
    assert!(out.output.contains("data/flyvis-reference/centre.f32.bin is missing"), "{}", out.output);
    // The stimulus check needs only input.f64.bin of the trace files.
    sh.data(&[(CENTRE_F32, Alter::Remove), (CENTRE_F64, Alter::Remove), (SNAPSHOTS_F32, Alter::Remove), (SNAPSHOTS_F64, Alter::Remove)]);
    let out = sh.gate(&sc("rust_flashes_equal_flyviss_input"), true);
    assert!(out.passed, "the flash check without centre/snapshot files:\n{}", out.output);
}

const SETTLING_TESTS: [&str; 20] = [
    "settling_exact_run_0_000_1_200_f32",
    "settling_libm_run_0_000_1_200_f32",
    "settling_exact_run_1_000_1_200_f64",
    "settling_libm_run_1_000_1_200_f64",
    "settling_exact_run_2_000_1_50_f32",
    "settling_libm_run_2_000_1_50_f32",
    "settling_exact_run_3_000_1_50_f64",
    "settling_libm_run_3_000_1_50_f64",
    "settling_exact_run_4_049_1_200_f32",
    "settling_libm_run_4_049_1_200_f32",
    "settling_exact_run_5_049_1_200_f64",
    "settling_libm_run_5_049_1_200_f64",
    "settling_exact_run_6_049_1_50_f32",
    "settling_libm_run_6_049_1_50_f32",
    "settling_exact_run_7_049_1_50_f64",
    "settling_libm_run_7_049_1_50_f64",
    "reference_runs_are_the_eight_expected",
    "pre_registered_bounds_match_provenance",
    "the_gate_tells_models_apart",
    "the_gate_resolves_integer_counts",
];

#[test]
fn traces_14_the_settling_tests_keep_names_paths_behaviour_and_strings() {
    let Some(f) = fx() else { return };
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh
        .cargo()
        .args(["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--", "--list"])
        .output()
        .expect("cargo runs");
    let list = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "{list}{}", String::from_utf8_lossy(&out.stderr));
    let listed: Vec<&str> =
        list.lines().filter_map(|l| l.strip_suffix(": test")).filter_map(|l| l.strip_prefix("settling_checks::")).collect();
    for name in SETTLING_TESTS {
        assert!(listed.contains(&name), "settling_checks::{name} must still exist:\n{list}");
    }
    assert_eq!(listed.len(), SETTLING_TESTS.len(), "no settling test added or lost: {listed:?}");

    // every one still passes
    for name in SETTLING_TESTS {
        let out = sh.gate(&format!("settling_checks::{name}"), true);
        assert!(out.passed, "settling_checks::{name}:\n{}", out.output);
    }

    // the exact gate's print, byte for byte as before
    let out = sh.gate("settling_checks::settling_exact_run_3_000_1_50_f64", true);
    let n_settle = f.manifest.dts.iter().find(|d| d.id == "1/50").unwrap().n_settle;
    let want = format!(
        "exact run 3 (000, dt 1/50, f64): settled 0 of {NODES} differ; settling centre 0 of {} differ",
        n_settle * 65
    );
    assert!(out.output.lines().any(|l| l.ends_with(&want)), "expected the line {want:?}:\n{}", out.output);

    // the exact gate's panic, byte for byte as before
    let mut settled = std::fs::read(data_path(SETTLED_F64)).unwrap();
    let node = 1234usize;
    let at = block_offset(&f.manifest, "settled.f64.bin", Some(3), "1/50", None) + 8 * node;
    let original = f64_at(&settled, at);
    put_f64(&mut settled, at, 1000.0);
    sh.data(&[(SETTLED_F64, Alter::Replace(settled))]);
    let out = sh.gate("settling_checks::settling_exact_run_3_000_1_50_f64", true);
    assert!(!out.passed);
    let want = format!(
        "run 3 is not bit-equal to flyvis: 1 settled and 0 settling-centre values differ; \
         first difference at the settled state only; largest settled error {:e} at node {node} ({})",
        (original - 1000.0).abs(),
        cell_type(&f.net, node)
    );
    assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);

    // the libm gate's print: as before, with only the ratios now {:.3e}
    sh.data(&[]);
    let out = sh.gate("settling_checks::settling_libm_run_3_000_1_50_f64", true);
    let line = line_with(&out.output, "libm run 3 (000, dt 1/50, f64): settled max ");
    let r1 = field(&line[line.find(" at node ").unwrap()..], "), ", ' ');
    let r2 = field(&line[line.find(" at step ").unwrap()..], "), ", ' ');
    assert!(is_3e(r1) && is_3e(r2), "ratios in {{:.3e}}: {r1:?} {r2:?} in {line}");
    let shape = line
        .replace(&format!("{r1} of bound"), "R of bound")
        .replace(&format!("{r2} of bound"), "R of bound");
    let parts = [
        "libm run 3 (000, dt 1/50, f64): settled max ",
        " at node ",
        "), R of bound; settling centre max ",
        " at step ",
        "), R of bound; worst types ",
        "; settled bit-equal ",
        &format!(" of {NODES}"),
    ];
    let mut rest = shape.as_str();
    for p in parts {
        let i = rest.find(p).unwrap_or_else(|| panic!("{p:?} missing from {line}"));
        rest = &rest[i + p.len()..];
    }
    assert!(rest.is_empty(), "nothing after the bit-equal count: {line}");
}

#[test]
fn traces_15_a_missing_trace_fixture_skips_or_with_require_fails() {
    if fx().is_none() {
        return;
    }
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    for (rel, test) in [
        (SNAPSHOTS_F64, tc("trace_exact_run_3_000_1_50_f64")),
        (CENTRE_F32, tc("trace_libm_run_2_000_1_50_f32")),
        (INPUT, tc("the_trace_gate_sees_a_wrong_mapping")),
        (INPUT, sc("rust_moving_edges_match_flyviss_input")),
        (SNAPSHOTS_F32, tc("snapshots_agree_with_the_centre_column")),
    ] {
        sh.data(&[(rel, Alter::Remove)]);
        let path = format!("data/{rel}");
        let out = sh.gate(&test, false);
        assert!(out.passed, "{test} without {rel} must skip and pass:\n{}", out.output);
        assert!(
            out.output.lines().any(|l| l.starts_with("skipping:") && l.contains(&format!("{path} not found"))),
            "{test}: skip notice naming {path}:\n{}",
            out.output
        );
        let out = sh.gate(&test, true);
        assert!(!out.passed, "{test} without {rel} under SEMBLANCE_REQUIRE_FIXTURES=1 must fail:\n{}", out.output);
        assert!(
            out.output.contains("SEMBLANCE_REQUIRE_FIXTURES=1 but") && out.output.contains(&format!("{path} is missing")),
            "{test}: failure naming {path}:\n{}",
            out.output
        );
    }
}

// ---------------------------------------------------------------------------
// TRACES-16 .. TRACES-22: Rust's stimuli against flyvis's input
// ---------------------------------------------------------------------------

fn input_block(f: &Fx, dt: &str, sequence: u32) -> Array<f64> {
    f.manifest
        .file("input.f64.bin")
        .unwrap()
        .read_f64(&f.input, &BlockKey { run: None, dt, sequence: Some(sequence) })
        .expect("input block")
}

#[test]
fn traces_16_rust_flashes_equal_flyviss_input_exactly() {
    let Some(f) = fx() else { return };
    for dt in &f.manifest.dts {
        let flashes = Flashes::new(FlashParams::flyvis_defaults(dt.dt));
        assert_eq!(flashes.num_sequences(), 4);
        let mut out = vec![0.0f32; HEXALS];
        for s in 0..4 {
            let (intensity, radius) = (flashes.intensity_of(s).unwrap(), flashes.radius_of(s).unwrap());
            let m: Vec<&SequenceInfo> = f
                .sequences
                .iter()
                .filter(|q| {
                    q.family == "flashes"
                        && q.arg("intensity") == Some(f64::from(intensity))
                        && q.arg("radius") == Some(f64::from(radius))
                })
                .collect();
            assert_eq!(m.len(), 1, "dt {}: flash {s} matches one reference sequence", dt.id);
            let layout = m[0].frames(&dt.id).unwrap();
            assert_eq!(flashes.num_frames(), layout.n_frames, "dt {}: flash {s} frame count", dt.id);
            let x = input_block(f, &dt.id, m[0].index);
            let mut frames = Vec::new();
            for i in 0..layout.n_frames {
                flashes.frame_into(s, i, &mut out).unwrap();
                for (h, (&r, &x)) in out.iter().zip(&x.data[i * HEXALS..(i + 1) * HEXALS]).enumerate() {
                    assert!(f64::from(r) == x, "dt {}: flash {s} frame {i} hexal {h}: {r} vs {x}", dt.id);
                }
                frames.push(out.clone());
            }
            // the stimulus window: the frames that differ from the baseline
            let lit: Vec<usize> = (0..layout.n_frames).filter(|&i| frames[i] != frames[0]).collect();
            let window = (lit[0], lit[lit.len() - 1] + 1);
            assert_eq!(lit.len(), window.1 - window.0, "one contiguous stimulus block");
            assert_eq!(window, layout.stimulus_frames, "dt {}: flash {s} window", dt.id);
        }
    }
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate(&sc("rust_flashes_equal_flyviss_input"), true);
    assert!(out.passed, "{}", out.output);
}

fn edge_params(dt: f64) -> MovingEdgeParams {
    let mut p = MovingEdgeParams::flyvis_defaults(dt);
    p.height = 80;
    p.speeds = vec![13.0];
    p.angles = vec![0, 30, 90, 150];
    p
}

/// (Rust edge sequence, matched reference sequence) at one dt.
fn matched_edges<'a>(f: &'a Fx, edges: &MovingEdge, dt: &str) -> Vec<(usize, &'a SequenceInfo)> {
    (0..edges.num_sequences())
        .map(|s| {
            let (a, i, v) = (edges.angle_of(s).unwrap(), edges.intensity_of(s).unwrap(), edges.speed_of(s).unwrap());
            let m: Vec<&SequenceInfo> = f
                .sequences
                .iter()
                .filter(|q| {
                    q.family == "moving_edge"
                        && q.arg("angle") == Some(f64::from(a))
                        && q.arg("intensity") == Some(f64::from(i))
                        && q.arg("speed") == Some(v)
                })
                .collect();
            assert_eq!(m.len(), 1, "dt {dt}: edge {s} (angle {a}, intensity {i}, speed {v}) matches one sequence");
            (s, m[0])
        })
        .collect()
}

/// Per dt: (per-angle max |Rust - flyvis|, 0 degree frames bit-equal to x as f32, every frame within TOLERANCE).
struct EdgeCompare {
    dt: String,
    worst: [f64; 4],
    zero_bit_equal: bool,
    zero_frames: usize,
    within: bool,
}

fn edge_compare(f: &'static Fx) -> &'static [EdgeCompare] {
    static C: OnceLock<Vec<EdgeCompare>> = OnceLock::new();
    C.get_or_init(|| {
        let angles = [0, 30, 90, 150];
        f.manifest
            .dts
            .iter()
            .map(|dt| {
                let edges = MovingEdge::new(edge_params(dt.dt));
                let mut out = vec![0.0f32; HEXALS];
                let mut c = EdgeCompare { dt: dt.id.clone(), worst: [0.0; 4], zero_bit_equal: true, zero_frames: 0, within: true };
                for (s, q) in matched_edges(f, &edges, &dt.id) {
                    let x = input_block(f, &dt.id, q.index);
                    let angle = edges.angle_of(s).unwrap();
                    let a = angles.iter().position(|&g| g == angle).unwrap();
                    for i in 0..q.frames(&dt.id).unwrap().n_frames {
                        edges.frame_into(s, i, &mut out).unwrap();
                        let row = &x.data[i * HEXALS..(i + 1) * HEXALS];
                        if angle == 0 {
                            c.zero_frames += 1;
                            c.zero_bit_equal &= out.iter().zip(row).all(|(r, &x)| r.to_bits() == (x as f32).to_bits());
                        }
                        for (r, &x) in out.iter().zip(row) {
                            let d = (f64::from(*r) - x).abs();
                            c.within &= d <= TOLERANCE;
                            c.worst[a] = c.worst[a].max(d);
                        }
                    }
                }
                c
            })
            .collect()
    })
}

#[test]
fn traces_17_moving_edges_match_the_manifests_sequences_offsets_and_windows() {
    let Some(f) = fx() else { return };
    for dt in &f.manifest.dts {
        let params = edge_params(dt.dt);
        assert_eq!(params.offsets, (-10, 10), "default offsets, inclusive");
        let edges = MovingEdge::new(params.clone());
        assert_eq!(edges.num_sequences(), 8);
        let matched = matched_edges(f, &edges, &dt.id);
        let mut seen: Vec<u32> = matched.iter().map(|(_, q)| q.index).collect();
        seen.sort_unstable();
        assert_eq!(seen, (4..12).collect::<Vec<u32>>(), "the eight reference edges, each once");
        for (s, q) in matched {
            let offsets = manifest_offsets(&f.manifest_json, q.index);
            assert_eq!(offsets, [-10.0, 11.0], "sequence {}: flyvis's exclusive offsets", q.index);
            assert_eq!(offsets, [f64::from(params.offsets.0), f64::from(params.offsets.1 + 1)]);
            assert_eq!(q.arg("height"), Some(80.0));
            let layout = q.frames(&dt.id).unwrap();
            assert_eq!(edges.num_frames(), layout.n_frames, "dt {}: edge {s} n_frames", dt.id);
            assert_eq!(edges.valid_frames(s), Some(layout.n_frames), "dt {}: edge {s} valid frames", dt.id);
            assert_eq!(edges.stimulus_frames(s), Some(layout.stimulus_frames), "dt {}: edge {s} window", dt.id);
        }
    }
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate(&sc("rust_moving_edges_match_flyviss_input"), true);
    assert!(out.passed, "{}", out.output);
    // and the check asserts the offsets: a manifest edge listing [-10, 10) fails it
    let s = f.manifest_json.find("\"sequences\"").unwrap();
    let o = s + f.manifest_json[s..].find("\"offsets\": [").unwrap();
    let e = o + f.manifest_json[o..].find(']').unwrap() + 1;
    let mut m = f.manifest_json.clone();
    m.replace_range(o..e, "\"offsets\": [-10, 10]");
    sh.data(&[(MANIFEST, Alter::Replace(m.into_bytes()))]);
    let out = sh.gate(&sc("rust_moving_edges_match_flyviss_input"), true);
    assert!(!out.passed, "wrong offsets must fail:\n{}", out.output);
    assert!(out.output.contains("sequence 4's offsets"), "{}", out.output);
}

#[test]
fn traces_18_stimulus_frames_returns_the_window_or_none() {
    let Some(f) = fx() else { return };
    for dt in &f.manifest.dts {
        let edges = MovingEdge::new(edge_params(dt.dt));
        for (s, q) in matched_edges(f, &edges, &dt.id) {
            let (start, end) = edges.stimulus_frames(s).expect("an existing sequence");
            assert_eq!((start, end), q.frames(&dt.id).unwrap().stimulus_frames);
            assert!(start < end && end <= edges.num_frames());
        }
        for s in [8, 9, 100, usize::MAX] {
            assert_eq!(edges.stimulus_frames(s), None, "sequence {s} does not exist");
        }
    }
    // also with several speeds: each sequence its own window
    let mut p = MovingEdgeParams::flyvis_defaults(0.005);
    p.speeds = vec![13.0, 26.0];
    let e = MovingEdge::new(p);
    let n = e.num_sequences();
    for s in 0..n {
        let (start, end) = e.stimulus_frames(s).unwrap();
        assert!(start < end && end <= e.valid_frames(s).unwrap(), "sequence {s}");
    }
    assert_ne!(e.stimulus_frames(0), e.stimulus_frames(1), "different speeds, different windows");
    assert_eq!(e.stimulus_frames(n), None);
}

#[test]
fn traces_19_every_moving_edge_frame_lies_within_the_shared_tolerance() {
    let Some(f) = fx() else { return };
    for c in edge_compare(f) {
        assert!(c.within, "dt {}: a frame lies outside {TOLERANCE}", c.dt);
        assert!(c.worst.iter().all(|&w| w <= TOLERANCE), "dt {}: {:?}", c.dt, c.worst);
    }
    // The check's bound is the geometry gate's TOLERANCE itself, not a copy:
    // tightening that one constant (in the shadow copy only) fails the check.
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate(&sc("rust_moving_edges_match_flyviss_input"), true);
    assert!(out.passed && out.output.contains("(TOLERANCE 0.005)"), "{}", out.output);
    let file = sh.root.join("crates/flyvis/src/moving_edge_checks.rs");
    let src = std::fs::read_to_string(&file).unwrap();
    let decl = "pub(crate) const TOLERANCE: f64 = 0.005;";
    assert_eq!(src.matches(decl).count(), 1, "the constant is declared once, pub(crate), in moving_edge_checks.rs");
    std::fs::write(&file, src.replace(decl, "pub(crate) const TOLERANCE: f64 = 1e-9;")).unwrap();
    let tightened = sh.gate(&sc("rust_moving_edges_match_flyviss_input"), true);
    std::fs::write(&file, &src).unwrap();
    assert!(!tightened.passed, "the check must follow moving_edge_checks::TOLERANCE:\n{}", tightened.output);
    assert!(tightened.output.contains("apart"), "{}", tightened.output);
}

#[test]
fn traces_20_zero_degree_edges_equal_the_f32_input_bit_for_bit() {
    let Some(f) = fx() else { return };
    for c in edge_compare(f) {
        assert!(c.zero_frames > 0, "dt {}: the 0 degree sequences were compared", c.dt);
        assert!(c.zero_bit_equal, "dt {}: a 0 degree frame differs from x as f32", c.dt);
    }
}

#[test]
fn traces_21_the_moving_edge_check_prints_the_per_angle_maximum() {
    let Some(f) = fx() else { return };
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let out = sh.gate(&sc("rust_moving_edges_match_flyviss_input"), true);
    assert!(out.passed, "{}", out.output);
    for c in edge_compare(f) {
        let want = format!(
            "moving edges at dt {}: max |Rust - flyvis| at 0/30/90/150 degrees: {:.3e} {:.3e} {:.3e} {:.3e}",
            c.dt, c.worst[0], c.worst[1], c.worst[2], c.worst[3]
        );
        assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);
    }
}

#[test]
fn traces_22_a_stimulus_frame_off_or_a_frame_count_mismatch_fails_naming_it() {
    let Some(f) = fx() else { return };
    let guard = shadow();
    let sh = guard.as_ref().unwrap();

    // a moving-edge value outside TOLERANCE
    let (seq, frame, hexal) = (6u32, 60usize, 100usize);
    let mut input = f.input.clone();
    let at = block_offset(&f.manifest, "input.f64.bin", None, "1/50", Some(seq)) + 8 * (frame * HEXALS + hexal);
    let v = f64_at(&input, at);
    put_f64(&mut input, at, v + 0.1);
    sh.data(&[(INPUT, Alter::Replace(input))]);
    let out = sh.gate(&sc("rust_moving_edges_match_flyviss_input"), true);
    assert!(!out.passed, "{}", out.output);
    let want = format!("dt 1/50: sequence {seq} (angle 30) frame {frame} hexal {hexal}");
    assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);

    // a flash value off by anything
    let (seq, frame, hexal) = (2u32, 70usize, 5usize);
    let mut input = f.input.clone();
    let at = block_offset(&f.manifest, "input.f64.bin", None, "1/50", Some(seq)) + 8 * (frame * HEXALS + hexal);
    let v = f64_at(&input, at);
    put_f64(&mut input, at, v - 1e-6);
    sh.data(&[(INPUT, Alter::Replace(input))]);
    let out = sh.gate(&sc("rust_flashes_equal_flyviss_input"), true);
    assert!(!out.passed, "{}", out.output);
    let want = format!("dt 1/50: sequence {seq} frame {frame} hexal {hexal}");
    assert!(out.output.contains(&want), "expected {want:?}:\n{}", out.output);

    // a frame-count mismatch, for an edge and a flash
    for (seq, n, test, want) in [
        (6u32, 132usize, sc("rust_moving_edges_match_flyviss_input"), "dt 1/50: sequence 6's frame count"),
        (2, 151, sc("rust_flashes_equal_flyviss_input"), "dt 1/50: sequence 2's frame count"),
    ] {
        let m = replace_n_frames(&f.manifest_json, seq, "1/50", n);
        assert_eq!(parse_sequences(&m).unwrap()[seq as usize].frames("1/50").unwrap().n_frames, n);
        sh.data(&[(MANIFEST, Alter::Replace(m.into_bytes()))]);
        let out = sh.gate(&test, true);
        assert!(!out.passed, "{test} with n_frames {n}:\n{}", out.output);
        assert!(out.output.contains(want), "expected {want:?}:\n{}", out.output);
    }
}

// ---------------------------------------------------------------------------
// TRACES-23: the dev profile
// ---------------------------------------------------------------------------

#[test]
fn traces_23_sim_and_flyvis_build_at_opt_level_3_without_fast_math_and_stay_exact() {
    let Some(f) = fx() else { return };
    let ws = Path::new(WORKSPACE);
    let toml = std::fs::read_to_string(ws.join("Cargo.toml")).unwrap();
    for krate in ["semblance-sim", "semblance-flyvis"] {
        let h = format!("[profile.dev.package.{krate}]");
        let at = toml.find(&h).unwrap_or_else(|| panic!("{h} in Cargo.toml")) + h.len();
        let body = &toml[at..at + toml[at..].find("\n[").unwrap_or(toml.len() - at)];
        let keys: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        assert_eq!(keys, ["opt-level = 3"], "{h}: opt-level 3 and nothing else");
    }
    // settings only: the comments explain why no such flag is set
    let settings: String = toml.lines().map(|l| l.split('#').next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    for bad in ["rustflags", "target-cpu", "fast-math", "fast_math", "target-feature", "+fma"] {
        assert!(!settings.contains(bad), "Cargo.toml must not set {bad}");
    }
    for cfg in [".cargo/config.toml", ".cargo/config"] {
        if let Ok(c) = std::fs::read_to_string(ws.join(cfg)) {
            assert!(!c.contains("rustflags") && !c.contains("target-cpu"), "{cfg} must not add flags");
        }
    }

    // The real build: rustc for both crates at opt-level 3, no such flags.
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    for lib in ["crates/sim/src/lib.rs", "crates/flyvis/src/lib.rs"] {
        let p = sh.root.join(lib);
        let src = std::fs::read(&p).unwrap();
        std::fs::write(&p, src).unwrap(); // a fresh mtime: rebuild and show the command
    }
    let out = sh
        .cargo()
        .args(["build", "-p", "semblance-flyvis", "-p", "semblance-sim", "-v", "--locked", "--offline"])
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .expect("cargo runs");
    let log = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{log}");
    for krate in ["semblance_sim", "semblance_flyvis"] {
        let cmd = log
            .lines()
            .find(|l| l.contains("Running") && l.contains(&format!("--crate-name {krate} ")))
            .unwrap_or_else(|| panic!("no rustc command for {krate}:\n{log}"));
        assert!(cmd.contains("opt-level=3"), "{krate} at opt-level 3: {cmd}");
        for bad in ["target-cpu", "fast-math", "target-feature", "llvm-args", "fma"] {
            assert!(!cmd.contains(bad), "{krate} built with {bad}: {cmd}");
        }
    }

    // and under that profile the exact gates still pass bit for bit
    for run in [2u32, 3] {
        assert!(played(f, run, Counts::Torch).iter().all(|s| s.centre.differ + s.snapshots.differ == 0), "run {run}");
    }
    for name in ["trace_exact_run_2_000_1_50_f32", "trace_exact_run_3_000_1_50_f64"] {
        let out = sh.gate(&tc(name), true);
        assert!(out.passed && out.output.contains("12 of 12 sequences bit-equal"), "{name}:\n{}", out.output);
    }
}
