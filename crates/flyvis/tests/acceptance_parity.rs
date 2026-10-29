//! Acceptance tests for behaviour group `parity` of epic parity-validate:
//! the parity library -- loading fixtures, running the gates, the pass rules
//! and the controls.
//!
//! One test per behaviour id (PARITY-01 .. PARITY-34), except PARITY-08,
//! PARITY-22 and PARITY-28, which the public API cannot reach (see the notes
//! where they would be).
//!
//! Three kinds of test live here:
//!
//! * Pure tests drive `semblance_flyvis::parity`'s public API on values they
//!   build themselves (bounds, `diff`, the pass predicates, `Provenance`).
//! * Fixture tests load the real fixtures through `Fixtures::load` and
//!   `TraceData::load`, or call `parity::validate`, sometimes on a temporary
//!   data directory of symlinks to the real files with one file altered. The
//!   real `data/` is never written.
//! * The `#[cfg(test)]` gates live inside `semblance-flyvis`. To observe them
//!   (PARITY-31..33) these tests copy the workspace's sources, unchanged, into
//!   `CARGO_TARGET_TMPDIR`, give the copy a `data/` of symlinks with one file
//!   altered or removed, and run the gate there with `cargo test`.
//!
//! Every fixture test skips without the real fixtures, printing
//! `skipping: ...`, unless `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip
//! into a failure.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

use semblance_flyvis::ensemble::{ModelParams, N_MODELS};
use semblance_flyvis::parity::{
    self, check_settling_gaps, check_trace_gaps, diff, how_to_get, libm_fingerprint, play, required_files, run,
    settle, settling_bounds, trace_bounds, validate, After, Bound, Controls, Counts, Coverage, Diff, Fixtures, Job,
    Options, ParityError, Report, RunResult, SequenceResult, SettlingResult, Start, TraceData, TraceInputs,
    CONTROL_MISS, COVERAGE_THRESHOLD, F64_BOUND, RUNS, SETTLING_F32_GAPS, TRACE_F32_GAPS,
};
use semblance_flyvis::pretrained::{CELL_TYPES, CONN_TYPES, SYN_COUNT};
use semblance_flyvis::reference::{BlockKey, Dtype, GapEntry, Manifest, Provenance, ReferenceError};
use semblance_flyvis::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use semblance_sim::graded::GradedError;

// ---------------------------------------------------------------------------
// Fixtures and temporary data directories
// ---------------------------------------------------------------------------

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";
const ENSEMBLE: &str = "flyvis-pretrained/ensemble.csv";
const MANIFEST: &str = "flyvis-reference/manifest.json";
const PROVENANCE: &str = "flyvis-reference/PROVENANCE.json";
const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

/// The 13 files the spec lists for `validate`, in its order.
const THIRTEEN: [&str; 13] = [
    SPEC,
    ENSEMBLE,
    MANIFEST,
    PROVENANCE,
    "flyvis-reference/settled.f32.bin",
    "flyvis-reference/settled.f64.bin",
    "flyvis-reference/settling_centre.f32.bin",
    "flyvis-reference/settling_centre.f64.bin",
    "flyvis-reference/input.f64.bin",
    "flyvis-reference/centre.f32.bin",
    "flyvis-reference/centre.f64.bin",
    "flyvis-reference/snapshots.f32.bin",
    "flyvis-reference/snapshots.f64.bin",
];

fn real_data() -> PathBuf {
    Path::new(WORKSPACE).join("data")
}

fn fixtures_required() -> bool {
    std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1")
}

/// Whether every one of the 13 fixtures is present. If not, prints this
/// test's own skip notice (or fails under `SEMBLANCE_REQUIRE_FIXTURES=1`).
fn have_fixtures() -> bool {
    for rel in THIRTEEN {
        let path = real_data().join(rel);
        if std::fs::metadata(&path).is_err() {
            assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {} is missing", path.display());
            eprintln!("skipping: {} not found. {}", path.display(), how_to_get(rel));
            return false;
        }
    }
    true
}

fn read_real(rel: &str) -> Vec<u8> {
    std::fs::read(real_data().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn read_real_text(rel: &str) -> String {
    String::from_utf8(read_real(rel)).expect("UTF-8")
}

/// A fresh, empty directory under `CARGO_TARGET_TMPDIR`.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-parity").join(name);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("removing {}: {e}", dir.display()),
    }
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn link(src: &Path, dst: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst).unwrap();
    #[cfg(not(unix))]
    std::fs::copy(src, dst).map(|_| ()).unwrap();
}

enum Alter {
    Replace(Vec<u8>),
    Remove,
    /// A directory where the file should be.
    Dir,
}

/// A data directory named `name` whose 13 files link to the real ones,
/// except those in `alter`.
fn relocated(name: &str, alter: &[(&str, Alter)]) -> PathBuf {
    let dir = scratch(name);
    for rel in THIRTEEN {
        let to = dir.join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        match alter.iter().find(|(r, _)| *r == rel) {
            None => link(&real_data().join(rel), &to),
            Some((_, Alter::Replace(bytes))) => std::fs::write(&to, bytes).unwrap(),
            Some((_, Alter::Remove)) => {}
            Some((_, Alter::Dir)) => std::fs::create_dir_all(&to).unwrap(),
        }
    }
    dir
}

/// The real fixture, loaded once through the public API.
fn fixtures() -> &'static Fixtures {
    static F: OnceLock<Fixtures> = OnceLock::new();
    F.get_or_init(|| Fixtures::load(&real_data()).unwrap_or_else(|e| panic!("{e}")))
}

fn traces() -> &'static TraceData {
    static T: OnceLock<TraceData> = OnceLock::new();
    T.get_or_init(|| TraceData::load(&real_data()).unwrap_or_else(|e| panic!("{e}")))
}

fn manifest() -> &'static Manifest {
    static M: OnceLock<Manifest> = OnceLock::new();
    M.get_or_init(|| Manifest::parse(&read_real_text(MANIFEST)).expect("the real manifest parses"))
}

/// The real files `Fixtures::from_parts` takes.
struct Parts {
    spec: String,
    ensemble: String,
    manifest: String,
    settling: Vec<(&'static str, Vec<u8>)>,
}

fn parts() -> Parts {
    Parts {
        spec: read_real_text(SPEC),
        ensemble: read_real_text(ENSEMBLE),
        manifest: read_real_text(MANIFEST),
        settling: SETTLING_FILES.iter().map(|&n| (n, read_real(&format!("flyvis-reference/{n}")))).collect(),
    }
}

impl Parts {
    fn build(&self) -> Fixtures {
        Fixtures::from_parts(&self.spec, &self.ensemble, &self.manifest, self.settling.clone())
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn settling_mut(&mut self, name: &str) -> &mut Vec<u8> {
        &mut self.settling.iter_mut().find(|(n, _)| *n == name).unwrap().1
    }
}

/// Byte offset of the block of `file` keyed by (run, dt, sequence).
fn block_offset(file: &str, run: Option<u32>, dt: &str, sequence: Option<u32>) -> usize {
    let info = manifest().file(file).unwrap_or_else(|| panic!("{file} is in the manifest"));
    info.blocks
        .iter()
        .find(|b| b.run == run && b.dt == dt && b.sequence == sequence)
        .unwrap_or_else(|| panic!("{file}: block {run:?} {dt} {sequence:?}"))
        .offset
}

fn put_f64(bytes: &mut [u8], at: usize, v: f64) {
    bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn put_f32(bytes: &mut [u8], at: usize, v: f32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn get_f64(bytes: &[u8], at: usize) -> f64 {
    f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

/// Runs `f`, requiring that it neither panics nor succeeds.
fn refuses<T>(what: &str, f: impl FnOnce() -> Result<T, ParityError>) -> ParityError {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(_)) => panic!("{what}: expected an error, got Ok"),
        Ok(Err(e)) => e,
        Err(_) => panic!("{what}: panicked instead of returning a ParityError"),
    }
}

/// `text` with the first occurrence of `from` at or after `after` replaced.
fn replace_after(text: &str, after: usize, from: &str, to: &str) -> String {
    let at = after + text[after..].find(from).unwrap_or_else(|| panic!("{from:?} after {after}"));
    format!("{}{}{}", &text[..at], to, &text[at + from.len()..])
}

fn find(text: &str, needle: &str) -> usize {
    text.find(needle).unwrap_or_else(|| panic!("{needle:?} not found"))
}

/// The report for runs 2 and 3 on the real data, one thread.
fn report_runs_2_3() -> &'static Report {
    static R: OnceLock<Report> = OnceLock::new();
    R.get_or_init(|| {
        validate(&real_data(), &Options { threads: 1, runs: vec![2, 3] }).unwrap_or_else(|e| panic!("{e}"))
    })
}

/// `validate` for runs 2 and 3, on 16 threads, with a +0.0 in run 2's
/// sequence-0 centre block: both run 2 jobs error, run 3's carry on.
fn report_with_run_2_broken() -> &'static Report {
    static R: OnceLock<Report> = OnceLock::new();
    R.get_or_init(|| {
        let rel = "flyvis-reference/centre.f32.bin";
        let mut bytes = read_real(rel);
        put_f32(&mut bytes, block_offset("centre.f32.bin", Some(2), "1/50", Some(0)), 0.0);
        let dir = relocated("run-2-broken", &[(rel, Alter::Replace(bytes))]);
        let r = validate(&dir, &Options { threads: 16, runs: vec![2, 3] }).unwrap_or_else(|e| panic!("{e}"));
        std::fs::remove_dir_all(&dir).unwrap();
        r
    })
}

// ---------------------------------------------------------------------------
// Values built by hand
// ---------------------------------------------------------------------------

fn diff0() -> Diff {
    Diff { max: 0.0, at: 0, differ: 0 }
}

fn sequence(s: u32) -> SequenceResult {
    SequenceResult {
        sequence: s,
        family: "flashes".into(),
        n_frames: 150,
        snapshot_frames: vec![0, 29, 59, 89, 119, 149],
        centre: diff0(),
        snapshots: diff0(),
        first: None,
    }
}

fn job(run: u32, counts: Counts) -> Job {
    let (_, model, dt, p) = RUNS[run as usize];
    Job {
        run,
        model: model.into(),
        dt: dt.into(),
        precision: p,
        counts,
        settling_bounds: settling_bounds(model, dt, p, counts).unwrap(),
        trace_bounds: trace_bounds(model, dt, p, counts).unwrap(),
        outcome: Ok(RunResult {
            settling: SettlingResult {
                run,
                model: model.into(),
                counts,
                settled: diff0(),
                settling_centre: diff0(),
                settled_len: 45669,
                centre_len: 3250,
                first_centre_difference: None,
                per_type_max: vec![0.0; 65],
            },
            sequences: (0..12).map(sequence).collect(),
            centre_movement: vec![1.0; 65],
        }),
    }
}

fn controls() -> Controls {
    let mut mirrored = sequence(8);
    mirrored.snapshots.max = 2.5;
    Controls {
        wrong_model: Ok(Diff { max: 1.0, at: 0, differ: 1 }),
        integer_counts: Ok(Diff { max: 1e-6, at: 0, differ: 1 }),
        mirrored: Ok(mirrored),
        from_flyvis: Ok(sequence(8)),
    }
}

/// A Provenance with the registered gaps and booleans.
fn registered_provenance() -> Provenance {
    Provenance {
        exported: "2026-10-27".into(),
        export_id: "x".into(),
        generator_path: "tools/export_reference.py".into(),
        generator_sha256: "0".into(),
        git_head: "0".into(),
        git_dirty: false,
        versions: vec![],
        platform: "p".into(),
        machine: "m".into(),
        device: "cpu".into(),
        uv_lock_sha256: "0".into(),
        threads_default: 12,
        threads_checked: vec![1, 12],
        threads_bit_equal: true,
        threads_runs_checked: vec![0, 1],
        models: vec![],
        all_checkpoints_equal_ensemble: true,
        stimulus_response_bit_equal: true,
        equivalence_sequences: vec![0, 4],
        input_rounding: vec![],
        gaps: SETTLING_F32_GAPS
            .iter()
            .zip(&TRACE_F32_GAPS)
            .map(|(s, t)| GapEntry {
                key: format!("{} {}", s.0, s.1),
                settled: s.2,
                settling_centre: s.3,
                centre: t.2,
                snapshots: t.3,
            })
            .collect(),
        sizes: vec![],
    }
}

fn report(runs: Vec<u32>) -> Report {
    Report {
        provenance: registered_provenance(),
        jobs: runs.iter().flat_map(|&r| [job(r, Counts::Torch), job(r, Counts::Libm)]).collect(),
        coverage: runs.iter().map(|&r| Coverage { run: r, movement: Some(vec![1.0; 65]) }).collect(),
        runs,
        cell_types: vec![],
        node_cell_type: vec![],
        centre_nodes: vec![],
        controls: controls(),
        libm: libm_fingerprint(),
        threads: 1,
    }
}

/// A valid ensemble of 50 models, each with model 000's compiled-in
/// parameters.
fn synthetic_ensemble() -> String {
    let p = ModelParams::pretrained();
    let mut out = String::from("model,kind,index,name,value\n");
    for m in 0..N_MODELS {
        for (i, t) in CELL_TYPES.iter().enumerate() {
            out.push_str(&format!("{m:03},bias,{i},{t},{}\n", p.bias[i]));
        }
        for (i, t) in CELL_TYPES.iter().enumerate() {
            out.push_str(&format!("{m:03},time_const,{i},{t},{}\n", p.time_const[i]));
        }
        for (i, (s, t)) in CONN_TYPES.iter().enumerate() {
            out.push_str(&format!("{m:03},syn_strength,{i},{s}>{t},{}\n", p.syn_strength[i]));
        }
    }
    out
}

const MANIFEST_RUNS: &str = r#""runs":[
    {"index":0,"model":"000","dt":"1/200","precision":"f32"},{"index":1,"model":"000","dt":"1/200","precision":"f64"},
    {"index":2,"model":"000","dt":"1/50","precision":"f32"},{"index":3,"model":"000","dt":"1/50","precision":"f64"},
    {"index":4,"model":"049","dt":"1/200","precision":"f32"},{"index":5,"model":"049","dt":"1/200","precision":"f64"},
    {"index":6,"model":"049","dt":"1/50","precision":"f32"},{"index":7,"model":"049","dt":"1/50","precision":"f64"}]"#;

/// A manifest that parses, with `runs` and four settling files of one value.
fn synthetic_manifest(runs: &str) -> String {
    let file = |name: &str, dtype: &str, bytes: usize| {
        format!(
            r#"{{"name":"{name}","dtype":"{dtype}","axes":["n"],"bytes":{bytes},
                 "blocks":[{{"run":0,"dt":"1/50","sequence":null,"offset":0,"bytes":{bytes},"shape":[1]}}]}}"#
        )
    };
    format!(
        r#"{{"schema_version":1,"byte_order":"little","array_order":"C","n_nodes":1,"t_pre":1.0,"pre_stim_value":0.5,
          "dts":[{{"id":"1/50","dt":0.02,"floor_dt_f32":0.019999999552965164,"n_settle":50}}],
          {runs},"centre_nodes":[],"files":[{},{},{},{}]}}"#,
        file("settled.f32.bin", "<f4", 4),
        file("settled.f64.bin", "<f8", 8),
        file("settling_centre.f32.bin", "<f4", 4),
        file("settling_centre.f64.bin", "<f8", 8)
    )
}

fn synthetic_settling(short: Option<&str>) -> Vec<(&'static str, Vec<u8>)> {
    SETTLING_FILES
        .iter()
        .map(|&n| {
            let len = if n.contains("f32") { 4 } else { 8 };
            (n, vec![0u8; if Some(n) == short { len - 1 } else { len }])
        })
        .collect()
}

/// 64-bit FNV-1a, written here from its definition.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------
// PARITY-01
// ---------------------------------------------------------------------------

const PROBE: &str = "PARITY_01_PROBE_DIR";

/// The three readers on an empty directory, one line each.
fn probe(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for (what, r) in [
        ("Fixtures::load", catch_unwind(|| Fixtures::load(dir).map(|_| ()))),
        ("TraceData::load", catch_unwind(|| TraceData::load(dir).map(|_| ()))),
        ("validate", catch_unwind(|| validate(dir, &Options::default()).map(|_| ()))),
    ] {
        out.push(match r {
            Ok(Ok(())) => format!("PROBE {what}: Ok"),
            Ok(Err(e)) => format!("PROBE {what}: {e:?}"),
            Err(_) => format!("PROBE {what}: panicked"),
        });
    }
    out
}

/// The parity module's sources with their `#[cfg(test)] mod tests` removed.
fn parity_sources() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/parity");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let code = match text.find("#[cfg(test)]\nmod tests") {
            Some(at) => text[..at].to_string(),
            None => text,
        };
        out.push((path.file_name().unwrap().to_string_lossy().into_owned(), code));
    }
    assert!(out.len() >= 7, "the parity module's files: {:?}", out.iter().map(|(n, _)| n).collect::<Vec<_>>());
    out
}

#[test]
fn parity_01_readers_read_only_the_directory_they_are_given() {
    if let Some(dir) = std::env::var_os(PROBE) {
        for line in probe(Path::new(&dir)) {
            println!("{line}");
        }
        return;
    }

    // An empty directory: each reader reports a file under it, and nothing else.
    let empty = scratch("parity-01-empty");
    match Fixtures::load(&empty) {
        Err(ParityError::Missing { rel, path }) => {
            assert_eq!(rel, ENSEMBLE);
            assert_eq!(path, empty.join(ENSEMBLE));
        }
        other => panic!("Fixtures::load on an empty dir: {:?}", other.map(|_| ())),
    }
    match TraceData::load(&empty) {
        Err(ParityError::Missing { rel, path }) => {
            assert_eq!(rel, MANIFEST);
            assert_eq!(path, empty.join(MANIFEST));
        }
        other => panic!("TraceData::load on an empty dir: {:?}", other.map(|_| ())),
    }
    match validate(&empty, &Options::default()) {
        Err(ParityError::Missing { rel, path }) => {
            assert_eq!(rel, SPEC);
            assert_eq!(path, empty.join(SPEC));
        }
        other => panic!("validate on an empty dir: {:?}", other.map(|_| ())),
    }

    // SEMBLANCE_REQUIRE_FIXTURES changes nothing: the same results, no panic,
    // with it set and unset. Each is this test, run by cargo in the shadow
    // workspace with the probe directory set.
    let mut outputs = Vec::new();
    {
        let guard = shadow();
        let sh = guard.as_ref().unwrap();
        for require in [true, false] {
            let o = sh.cargo_with(
                &[
                    "test", "-p", "semblance-flyvis", "--test", "acceptance_parity", "--locked", "--offline", "--",
                    "--exact", "parity_01_readers_read_only_the_directory_they_are_given", "--nocapture",
                ],
                require,
                &[(PROBE, empty.as_os_str())],
            );
            assert!(o.passed, "probe with require={require}:\n{}{}", o.stdout, o.stderr);
            let lines: Vec<String> = o.stdout.lines().filter(|l| l.starts_with("PROBE ")).map(str::to_string).collect();
            assert_eq!(lines.len(), 3, "{}", o.stdout);
            assert!(lines.iter().all(|l| l.contains("Missing")), "{lines:?}");
            outputs.push(lines);
        }
    }
    assert_eq!(outputs[0], outputs[1], "SEMBLANCE_REQUIRE_FIXTURES must not change what the readers do");

    // No checkout-relative or compile-time path, no environment, in the library.
    for (name, code) in parity_sources() {
        for banned in ["SEMBLANCE_REQUIRE_FIXTURES", "CARGO_MANIFEST_DIR", "env!(", "env::var", "current_dir"] {
            assert!(!code.contains(banned), "parity/{name} contains {banned:?} outside its tests");
        }
    }

    // A directory elsewhere, of links to the real files, reads the same data.
    if !have_fixtures() {
        return;
    }
    let dir = relocated("parity-01-elsewhere", &[]);
    assert_eq!(&Fixtures::load(&dir).unwrap_or_else(|e| panic!("{e}")), fixtures());
    assert_eq!(&TraceData::load(&dir).unwrap_or_else(|e| panic!("{e}")), traces());
    std::fs::remove_dir_all(&dir).unwrap();
}

// ---------------------------------------------------------------------------
// PARITY-02
// ---------------------------------------------------------------------------

#[test]
fn parity_02_from_parts_refuses_malformed_input_and_neither_constructor_reads_provenance() {
    let ensemble = synthetic_ensemble();
    let one_run = MANIFEST_RUNS[..MANIFEST_RUNS.find("},").unwrap() + 1].to_string() + "]";
    type Case<'a> = (&'a str, &'a str, String, Vec<(&'static str, Vec<u8>)>, &'a str);
    let cases: [Case; 4] = [
        ("an empty spec", &ensemble, synthetic_manifest(MANIFEST_RUNS), synthetic_settling(None), SPEC),
        (
            "a truncated ensemble",
            &ensemble[..ensemble.len() / 2],
            synthetic_manifest(MANIFEST_RUNS),
            synthetic_settling(None),
            ENSEMBLE,
        ),
        ("a manifest with one run", &ensemble, synthetic_manifest(&one_run), synthetic_settling(None), MANIFEST),
        (
            "a settling file one byte short",
            &ensemble,
            synthetic_manifest(MANIFEST_RUNS),
            synthetic_settling(Some("settled.f64.bin")),
            "flyvis-reference/settled.f64.bin",
        ),
    ];
    for (what, ensemble_csv, manifest_json, files, rel) in cases {
        match refuses(what, || Fixtures::from_parts("", ensemble_csv, &manifest_json, files)) {
            ParityError::Malformed { rel: got, detail } => assert_eq!(got, rel, "{what}: {detail}"),
            other => panic!("{what}: expected Malformed naming {rel}, got {other:?}"),
        }
    }

    if !have_fixtures() {
        return;
    }
    // from_parts needs nothing but its arguments: the real contents, passed
    // in, build what load builds.
    assert_eq!(&parts().build(), fixtures());

    // Neither constructor reads PROVENANCE.json: absent, or a directory.
    for (name, alter) in [("parity-02-no-provenance", Alter::Remove), ("parity-02-provenance-dir", Alter::Dir)] {
        let dir = relocated(name, &[(PROVENANCE, alter)]);
        assert_eq!(&Fixtures::load(&dir).unwrap_or_else(|e| panic!("{name}: {e}")), fixtures());
        assert_eq!(&TraceData::load(&dir).unwrap_or_else(|e| panic!("{name}: {e}")), traces());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

// ---------------------------------------------------------------------------
// PARITY-03
// ---------------------------------------------------------------------------

#[test]
fn parity_03_bad_input_to_any_entry_point_is_an_error_never_a_panic() {
    // validate without fixtures: a missing directory.
    let nowhere = scratch("parity-03-nowhere").join("absent");
    assert!(matches!(
        refuses("validate on a missing dir", || validate(&nowhere, &Options::default())),
        ParityError::Missing { .. }
    ));

    if !have_fixtures() {
        return;
    }
    let f = fixtures();
    let t = traces();
    let t64 = t.view(Dtype::F64);

    assert!(matches!(refuses("settle run 8", || settle(f, 8, "000", Counts::Torch)), ParityError::Usage(_)));
    assert!(matches!(
        refuses("settle an unknown model", || settle(f, 3, "999", Counts::Torch)),
        ParityError::Mismatch(_)
    ));
    assert!(matches!(
        refuses("play run 8", || play(f, &t64, 8, Counts::Torch, Start::Rust, &[8], false)),
        ParityError::Usage(_)
    ));
    assert!(matches!(
        refuses("play an unknown sequence", || play(f, &t64, 3, Counts::Torch, Start::Rust, &[12], false)),
        ParityError::Mismatch(_)
    ));
    let no_sequences = TraceInputs { sequences: &[], ..t64 };
    assert!(matches!(
        refuses("play with no sequences listed", || play(f, &no_sequences, 3, Counts::Torch, Start::Rust, &[0], false)),
        ParityError::Mismatch(_)
    ));
    let short_centre = TraceInputs { centre: &t64.centre[..t64.centre.len() - 1], ..t64 };
    assert!(matches!(
        refuses("play with a truncated centre file", || play(f, &short_centre, 3, Counts::Torch, Start::Rust, &[8], false)),
        ParityError::Malformed { .. }
    ));
    let no_input = TraceInputs { input: &[], ..t64 };
    assert!(matches!(
        refuses("run with an empty input file", || run(f, &no_input, 3, Counts::Torch)),
        ParityError::Malformed { .. }
    ));
    let short_snapshots = TraceInputs { snapshots: &t64.snapshots[..8], ..t64 };
    assert!(matches!(
        refuses("run with a truncated snapshots file", || run(f, &short_snapshots, 3, Counts::Torch)),
        ParityError::Malformed { .. }
    ));
    assert!(matches!(refuses("run run 9", || run(f, &t64, 9, Counts::Torch)), ParityError::Usage(_)));

    // validate: unreadable, malformed and mismatched files.
    let opts = Options { threads: 1, runs: vec![3] };
    type Case<'a> = (&'a str, &'a str, Alter, fn(&ParityError) -> bool);
    let cases: [Case; 5] = [
        ("parity-03-manifest-dir", MANIFEST, Alter::Dir, |e| matches!(e, ParityError::Unreadable { .. })),
        ("parity-03-ensemble-garbage", ENSEMBLE, Alter::Replace(b"not,a,csv\n".to_vec()), |e| {
            matches!(e, ParityError::Malformed { rel, .. } if rel == ENSEMBLE)
        }),
        ("parity-03-provenance-garbage", PROVENANCE, Alter::Replace(b"{".to_vec()), |e| {
            matches!(e, ParityError::Malformed { rel, .. } if rel == PROVENANCE)
        }),
        ("parity-03-manifest-not-utf8", MANIFEST, Alter::Replace(vec![0xff, 0xfe, 0x00]), |e| {
            matches!(e, ParityError::Malformed { rel, .. } if rel == MANIFEST)
        }),
        ("parity-03-input-empty", "flyvis-reference/input.f64.bin", Alter::Replace(vec![]), |e| {
            matches!(e, ParityError::Mismatch(m) if m.starts_with("input.f64.bin: 0 bytes"))
        }),
    ];
    for (name, rel, alter, ok) in cases {
        let dir = relocated(name, &[(rel, alter)]);
        let e = refuses(name, || validate(&dir, &opts));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(ok(&e), "{name}: {e:?}");
    }
}

// ---------------------------------------------------------------------------
// PARITY-04, PARITY-05
// ---------------------------------------------------------------------------

#[test]
fn parity_04_bounds_are_exact_for_torch_registered_for_libm_and_absent_otherwise() {
    for &(_, model, dt, p) in &RUNS {
        for (f, table, names) in [
            (settling_bounds as fn(&str, &str, Dtype, Counts) -> _, &SETTLING_F32_GAPS, ["settled", "settling_centre"]),
            (trace_bounds, &TRACE_F32_GAPS, ["centre", "snapshots"]),
        ] {
            assert_eq!(f(model, dt, p, Counts::Torch), Ok((Bound::Exact, Bound::Exact)));
            let libm = f(model, dt, p, Counts::Libm).unwrap_or_else(|e| panic!("{model} {dt}: {e}"));
            match p {
                Dtype::F64 => assert_eq!(libm, (Bound::F64, Bound::F64)),
                Dtype::F32 => {
                    let row = table.iter().find(|g| g.0 == model && g.1 == dt).unwrap();
                    match libm {
                        (Bound::F32 { gap: a, quantity: qa, .. }, Bound::F32 { gap: b, quantity: qb, .. }) => {
                            assert_eq!((a.to_bits(), b.to_bits()), (row.2.to_bits(), row.3.to_bits()));
                            assert_eq!([qa, qb], names);
                        }
                        other => panic!("{model} {dt} f32 libm: {other:?}"),
                    }
                }
            }
            assert!(f(model, dt, p, Counts::IntegerNSyn).is_err(), "integer counts have no bound");
        }
    }
    for counts in [Counts::Torch, Counts::Libm, Counts::IntegerNSyn] {
        assert_eq!(
            settling_bounds("001", "1/50", Dtype::F32, counts),
            Err(ParityError::Mismatch("no registered bound for model 001 at dt 1/50".into()))
        );
        assert_eq!(
            trace_bounds("000", "1/100", Dtype::F64, counts),
            Err(ParityError::Mismatch("no registered bound for model 000 at dt 1/100".into()))
        );
    }
}

#[test]
fn parity_05_a_bound_is_its_registered_value() {
    assert_eq!(Bound::Exact.value().to_bits(), 0.0f64.to_bits());
    assert_eq!(Bound::F64.value(), F64_BOUND);
    assert_eq!(F64_BOUND, 1e-10);
    for &(model, dt, a, b) in SETTLING_F32_GAPS.iter().chain(&TRACE_F32_GAPS) {
        for gap in [a, b] {
            let bound = Bound::F32 { gap, model, dt, quantity: "settled" };
            assert_eq!(bound.value().to_bits(), (2.0 * gap).to_bits(), "{model} {dt}");
        }
    }
}

// ---------------------------------------------------------------------------
// PARITY-06, PARITY-07
// ---------------------------------------------------------------------------

#[test]
fn parity_06_the_registered_gaps_pass_and_one_ulp_off_is_a_mismatch_naming_it() {
    let p = registered_provenance();
    assert_eq!(check_settling_gaps(&p), Ok(()));
    assert_eq!(check_trace_gaps(&p), Ok(()));
    for i in 0..4 {
        for q in ["settled", "settling_centre", "centre", "snapshots"] {
            for up in [true, false] {
                let mut p = registered_provenance();
                let g = &mut p.gaps[i];
                let v = match q {
                    "settled" => &mut g.settled,
                    "settling_centre" => &mut g.settling_centre,
                    "centre" => &mut g.centre,
                    _ => &mut g.snapshots,
                };
                *v = f64::from_bits(if up { v.to_bits() + 1 } else { v.to_bits() - 1 });
                let key = g.key.clone();
                let (wrong, right) = if matches!(q, "settled" | "settling_centre") {
                    (check_settling_gaps(&p), check_trace_gaps(&p))
                } else {
                    (check_trace_gaps(&p), check_settling_gaps(&p))
                };
                match wrong {
                    Err(ParityError::Mismatch(m)) => assert!(m.starts_with(&format!("{key} {q}")), "{m}"),
                    other => panic!("{key} {q}: {other:?}"),
                }
                assert_eq!(right, Ok(()), "{key} {q} is not the other table's");
                // The bound comes from the registry, not from PROVENANCE.
                let (model, dt) = key.split_once(' ').unwrap();
                let libm = settling_bounds(model, dt, Dtype::F32, Counts::Libm).unwrap();
                let row = SETTLING_F32_GAPS.iter().find(|r| r.0 == model && r.1 == dt).unwrap();
                assert_eq!(libm.0.value().to_bits(), (2.0 * row.2).to_bits());
            }
        }
    }
    let mut p = registered_provenance();
    p.gaps.remove(2);
    assert!(matches!(check_settling_gaps(&p), Err(ParityError::Mismatch(_))), "a missing gap entry");
}

/// A tiny JSON value, so PROVENANCE variants can be built and printed.
#[derive(Clone)]
enum J {
    O(Vec<(&'static str, J)>),
    A(Vec<J>),
    S(&'static str),
    N(f64),
    B(bool),
}

impl J {
    fn print(&self) -> String {
        match self {
            J::O(kv) => format!(
                "{{{}}}",
                kv.iter().map(|(k, v)| format!("{:?}: {}", k, v.print())).collect::<Vec<_>>().join(", ")
            ),
            J::A(v) => format!("[{}]", v.iter().map(J::print).collect::<Vec<_>>().join(", ")),
            J::S(s) => format!("{s:?}"),
            J::N(n) => format!("{n:?}"),
            J::B(b) => b.to_string(),
        }
    }

    /// The value at a path of object keys and `[i]` indices.
    fn at(&mut self, path: &[&str]) -> &mut J {
        let Some((first, rest)) = path.split_first() else { return self };
        match self {
            J::O(kv) => kv.iter_mut().find(|(k, _)| k == first).unwrap().1.at(rest),
            J::A(v) => v[first.trim_matches(['[', ']']).parse::<usize>().unwrap()].at(rest),
            _ => panic!("no {first}"),
        }
    }

    fn remove(&mut self, path: &[&str]) {
        let (last, parent) = path.split_last().unwrap();
        match self.at(parent) {
            J::O(kv) => kv.retain(|(k, _)| k != last),
            _ => panic!("not an object"),
        }
    }
}

fn minimal_provenance() -> J {
    use J::*;
    O(vec![
        ("exported", S("2026-10-27")),
        ("export_id", S("e")),
        ("generator", O(vec![("path", S("tools/export_reference.py")), ("sha256", S("ab"))])),
        ("git", O(vec![("head", S("c4")), ("dirty", B(false))])),
        ("versions", O(vec![("python", S("3.12.11"))])),
        ("machine", O(vec![("platform", S("p")), ("machine", S("arm64"))])),
        ("device", S("cpu")),
        ("uv_lock_sha256", S("54")),
        (
            "threads",
            O(vec![
                ("default", N(12.0)),
                ("determinism_check", A(vec![N(1.0), N(12.0)])),
                ("bit_equal", B(true)),
                ("runs_checked", A(vec![N(0.0), N(1.0)])),
            ]),
        ),
        (
            "models",
            A(vec![O(vec![
                ("id", S("000")),
                ("name", S("flow/0000/000")),
                ("rank", S("best")),
                ("validation_loss", N(5.25)),
                ("chkpt_00000_sha256", S("d0")),
            ])]),
        ),
        ("parameter_equality", O(vec![("all_50_checkpoints_equal_ensemble_csv", B(true))])),
        ("equivalence", O(vec![("stimulus_response_bit_equal", B(true)), ("sequences", A(vec![N(0.0), N(4.0)]))])),
        ("input_rounding", O(vec![("1/50", N(0.5))])),
        (
            "f32_f64_gap",
            O(vec![(
                "000 1/50",
                O(vec![("settled", N(1.0)), ("settling_centre", N(2.0)), ("centre", N(3.0)), ("snapshots", N(4.0))]),
            )]),
        ),
        ("sizes", O(vec![("centre.f64.bin", N(8.0))])),
    ])
}

#[test]
fn parity_07_provenance_parse_names_a_missing_or_mistyped_field_by_its_path() {
    // A minimal valid document parses, with each field read from its path.
    let p = Provenance::parse(&minimal_provenance().print()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(p.generator_path, "tools/export_reference.py");
    assert!(!p.git_dirty && p.threads_bit_equal && p.all_checkpoints_equal_ensemble && p.stimulus_response_bit_equal);
    assert_eq!(p.threads_checked, [1, 12]);
    assert_eq!(p.models.len(), 1);
    assert_eq!(p.models[0].validation_loss, 5.25);
    assert_eq!(p.gaps, [GapEntry { key: "000 1/50".into(), settled: 1.0, settling_centre: 2.0, centre: 3.0, snapshots: 4.0 }]);
    assert_eq!(p.sizes, [("centre.f64.bin".to_string(), 8)]);
    assert_eq!(p.versions, [("python".to_string(), "3.12.11".to_string())]);

    // Unknown extra fields are ignored.
    let mut extra = minimal_provenance();
    if let J::O(kv) = &mut extra {
        kv.push(("wall_time_s", J::N(12.5)));
        kv.push(("flyvis_root", J::O(vec![("path", J::S("x"))])));
    }
    if let J::O(kv) = extra.at(&["git"]) {
        kv.push(("branch", J::S("main")));
    }
    assert_eq!(Provenance::parse(&extra.print()), Ok(p));

    // Every field the parser reads, missing and then mistyped.
    let fields: [&[&str]; 31] = [
        &["exported"],
        &["export_id"],
        &["generator"],
        &["generator", "path"],
        &["generator", "sha256"],
        &["git"],
        &["git", "head"],
        &["git", "dirty"],
        &["versions"],
        &["machine"],
        &["machine", "platform"],
        &["machine", "machine"],
        &["device"],
        &["uv_lock_sha256"],
        &["threads"],
        &["threads", "default"],
        &["threads", "determinism_check"],
        &["threads", "bit_equal"],
        &["threads", "runs_checked"],
        &["models"],
        &["models", "[0]", "id"],
        &["models", "[0]", "validation_loss"],
        &["models", "[0]", "chkpt_00000_sha256"],
        &["parameter_equality"],
        &["parameter_equality", "all_50_checkpoints_equal_ensemble_csv"],
        &["equivalence"],
        &["equivalence", "stimulus_response_bit_equal"],
        &["equivalence", "sequences"],
        &["input_rounding"],
        &["f32_f64_gap", "000 1/50", "snapshots"],
        &["sizes"],
    ];
    for field in fields {
        let p = field.join(".").replace(".[", "[");
        let mut doc = minimal_provenance();
        doc.remove(field);
        let e = Provenance::parse(&doc.print()).expect_err(&format!("{p} missing"));
        assert_eq!(e, ReferenceError::ProvenanceMissing(p.clone()), "{p} missing");
        assert_eq!(e.to_string(), format!("PROVENANCE.json has no {p}"));

        let mut doc = minimal_provenance();
        let v = doc.at(field);
        *v = if matches!(v, J::S(_)) { J::N(1.0) } else { J::S("wrong type") };
        match Provenance::parse(&doc.print()) {
            Err(ReferenceError::ProvenanceMissing(got)) => {
                assert!(got == p || got.starts_with(&format!("{p}.")), "{p} mistyped: names {got}");
                assert_eq!(
                    ReferenceError::ProvenanceMissing(got.clone()).to_string(),
                    format!("PROVENANCE.json has no {got}")
                );
            }
            other => panic!("{p} mistyped: {other:?}"),
        }
    }
    // A mistyped entry inside an object names the entry.
    for (field, p) in [(&["sizes", "centre.f64.bin"][..], "sizes.centre.f64.bin"), (&["versions", "python"], "versions.python")] {
        let mut doc = minimal_provenance();
        *doc.at(field) = J::B(true);
        assert_eq!(Provenance::parse(&doc.print()), Err(ReferenceError::ProvenanceMissing(p.into())));
    }
}

// PARITY-08: not reachable through the public API. `Fixtures` can only be
// built by `Fixtures::from_parts` (or `load`, which calls it), and
// `from_parts` refuses any manifest whose runs are not `RUNS` -- precision
// included -- as `Malformed` (PARITY-02's "a manifest with one run"). No
// public value can reach `settle`/`play`/`run` with a manifest precision that
// differs from the registry.

// ---------------------------------------------------------------------------
// PARITY-09, PARITY-10
// ---------------------------------------------------------------------------

#[test]
fn parity_09_trace_inputs_of_the_other_precision_are_refused() {
    if !have_fixtures() {
        return;
    }
    let f = fixtures();
    let t = traces();
    for (run_, view, want) in
        [(3u32, Dtype::F32, "trace inputs are f32, run 3 is f64"), (2, Dtype::F64, "trace inputs are f64, run 2 is f32")]
    {
        let ti = t.view(view);
        assert_eq!(ti.precision, view);
        assert_eq!(
            play(f, &ti, run_, Counts::Torch, Start::Rust, &[8], false).map(|_| ()),
            Err(ParityError::Mismatch(want.into()))
        );
        assert_eq!(run(f, &ti, run_, Counts::Libm).map(|_| ()), Err(ParityError::Mismatch(want.into())));
    }
}

#[test]
fn parity_10_run_is_settle_then_play_field_for_field() {
    if !have_fixtures() {
        return;
    }
    let f = fixtures();
    let t = traces().view(Dtype::F64);
    let all: Vec<u32> = (0..12).collect();
    let r = run(f, &t, 3, Counts::Torch).unwrap_or_else(|e| panic!("{e}"));
    let s = settle(f, 3, "000", Counts::Torch).unwrap_or_else(|e| panic!("{e}"));
    let p = play(f, &t, 3, Counts::Torch, Start::Rust, &all, false).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r.settling, s);
    assert_eq!(r.sequences, p);
    assert_eq!(r.sequences.len(), 12);
    assert_eq!(r.centre_movement.len(), CELL_TYPES.len());
}

// ---------------------------------------------------------------------------
// PARITY-11 .. PARITY-14
// ---------------------------------------------------------------------------

const SIGNED_ZERO: &str = ": reference value {i} is ±0.0; == is bit equality only without signed zeros";

fn signed_zero_message(m: &str, file: &str, i: usize, sequence: Option<u32>) {
    let tail = SIGNED_ZERO.replace("{i}", &i.to_string());
    assert!(m.starts_with(&format!("{file} ")), "{m}");
    assert!(m.ends_with(&tail), "{m}");
    let key = &m[file.len()..m.len() - tail.len()];
    assert!(key.contains("run 3") && key.contains("1/50"), "{m}");
    if let Some(s) = sequence {
        assert!(key.contains(&format!("sequence {s}")), "{m}");
    }
}

#[test]
fn parity_11_a_signed_zero_in_the_reference_is_refused() {
    if !have_fixtures() {
        return;
    }
    // In the settled state.
    let at = block_offset("settled.f64.bin", Some(3), "1/50", None);
    for z in [0.0f64, -0.0] {
        let mut p = parts();
        put_f64(p.settling_mut("settled.f64.bin"), at + 5 * 8, z);
        let f = p.build();
        match settle(&f, 3, "000", Counts::Torch) {
            Err(ParityError::Mismatch(m)) => signed_zero_message(&m, "settled.f64.bin", 5, None),
            other => panic!("{z:?} in settled.f64.bin: {:?}", other.map(|_| ())),
        }
    }

    // In a played sequence's centre trace.
    let f = fixtures();
    let t = traces().view(Dtype::F64);
    let at = block_offset("centre.f64.bin", Some(3), "1/50", Some(8));
    for z in [0.0f64, -0.0] {
        let mut centre = t.centre.to_vec();
        put_f64(&mut centre, at + 7 * 8, z);
        let ti = TraceInputs { centre: &centre, ..t };
        match play(f, &ti, 3, Counts::Torch, Start::Rust, &[8], false) {
            Err(ParityError::Mismatch(m)) => signed_zero_message(&m, "centre.f64.bin", 7, Some(8)),
            other => panic!("{z:?} in centre.f64.bin: {:?}", other.map(|_| ())),
        }
    }

    // The comparison is still ==: one ulp off a non-zero value is one difference.
    let mut centre = t.centre.to_vec();
    let v = get_f64(&centre, at + 7 * 8);
    assert!(v != 0.0);
    let off = f64::from_bits(v.to_bits() + 1);
    put_f64(&mut centre, at + 7 * 8, off);
    let ti = TraceInputs { centre: &centre, ..t };
    let played = play(f, &ti, 3, Counts::Torch, Start::Rust, &[8], false).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(played[0].centre.differ, 1);
    assert_eq!(played[0].centre.at, 7);
    assert_eq!(played[0].centre.max, (v - off).abs());
}

#[test]
fn parity_12_a_nan_makes_the_maximum_infinite_and_counts_as_a_difference() {
    let d = diff(&[1.0, 2.0, 3.0], &[1.0, f64::NAN, 3.5]);
    assert_eq!(d.max, f64::INFINITY);
    assert_eq!(d.at, 1);
    assert_eq!(d.differ, 2);
    let d = diff(&[f64::NAN, 1.0], &[0.5, 1.0]);
    assert_eq!((d.max, d.at, d.differ), (f64::INFINITY, 0, 1));
    let d = diff(&[2.0, f64::NAN], &[2.0, f64::NAN]);
    assert_eq!((d.max, d.at, d.differ), (f64::INFINITY, 1, 1), "NaN is never equal, even to NaN");
    assert!(d.max > CONTROL_MISS, "an infinite maximum fails every bound");
}

/// PARITY-13. Of the three sources the behaviour names, only `set_state`
/// can be reached with public inputs: `Graded::new` refuses only non-finite
/// parameters, a bad dt or wrong lengths, which `Ensemble::parse` and the
/// manifest checks stop first, and `scatter_hexals` only a frame that is not
/// 721 hexals, which the core never builds. A NaN or an infinity in flyvis's
/// settled state, restarted from with `Start::Flyvis`, reaches `set_state`.
#[test]
fn parity_13_a_graded_error_becomes_a_mismatch_naming_the_run() {
    if !have_fixtures() {
        return;
    }
    let t = traces().view(Dtype::F64);
    let at = block_offset("settled.f64.bin", Some(3), "1/50", None);
    for (bad, index) in [(f64::NAN, 9usize), (f64::INFINITY, 40000)] {
        let mut p = parts();
        put_f64(p.settling_mut("settled.f64.bin"), at + index * 8, bad);
        let f = p.build();
        let want = format!("run 3: {}", GradedError::NonFinite { what: "state", index });
        assert_eq!(
            refuses("start from a non-finite settled state", || play(&f, &t, 3, Counts::Torch, Start::Flyvis, &[8], false)),
            ParityError::Mismatch(want)
        );
    }
}

#[test]
fn parity_14_a_state_that_goes_non_finite_is_a_gate_failure_naming_where() {
    if !have_fixtures() {
        return;
    }
    // During settling: model 000's synaptic strengths at 3e38.
    let mut p = parts();
    p.ensemble = p
        .ensemble
        .lines()
        .map(|l| match l.strip_prefix("000,syn_strength,") {
            Some(_) => format!("{},3e38", l.rsplit_once(',').unwrap().0),
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let f = p.build();
    for run_ in [2u32, 3] {
        match settle(&f, run_, "000", Counts::Torch) {
            Err(e @ ParityError::NonFinite { .. }) => {
                let ParityError::NonFinite { run: r, node, ref cell_type, after: After::SettleStep(k) } = e else {
                    panic!("run {run_}: not a settle step: {e:?}")
                };
                assert_eq!(r, run_);
                let net = f.network();
                assert_eq!(cell_type, &net.cell_types()[net.node_cell_type()[node] as usize]);
                assert_eq!(e.to_string(), format!("run {run_}: node {node} ({cell_type}) is not finite after settle step {k}"));
                assert!(e.is_gate_failure());
            }
            other => panic!("run {run_}: expected NonFinite, got {:?}", other.map(|_| ())),
        }
    }

    // During playback: a NaN in frame 3 of sequence 8's input.
    let f = fixtures();
    let t = traces().view(Dtype::F64);
    let mut input = t.input.to_vec();
    let at = block_offset("input.f64.bin", None, "1/50", Some(8));
    put_f64(&mut input, at + (3 * 721) * 8, f64::NAN);
    let ti = TraceInputs { input: &input, ..t };
    match play(f, &ti, 3, Counts::Torch, Start::Rust, &[8], false) {
        Err(e @ ParityError::NonFinite { .. }) => {
            let ParityError::NonFinite { run: r, node, ref cell_type, after: After::Frame { frame, sequence } } = e else {
                panic!("not a frame: {e:?}")
            };
            assert_eq!((r, frame, sequence), (3, 3, 8));
            let net = f.network();
            assert_eq!(cell_type, &net.cell_types()[net.node_cell_type()[node] as usize]);
            assert_eq!(e.to_string(), format!("run 3: node {node} ({cell_type}) is not finite after frame 3 of sequence 8"));
            assert!(e.is_gate_failure());
        }
        other => panic!("expected NonFinite, got {:?}", other.map(|_| ())),
    }

    // Only this variant is a gate failure.
    for e in [
        ParityError::Missing { rel: "a".into(), path: "a".into() },
        ParityError::Unreadable { rel: "a".into(), path: "a".into(), kind: std::io::ErrorKind::PermissionDenied },
        ParityError::Malformed { rel: "a".into(), detail: "b".into() },
        ParityError::Mismatch("m".into()),
        ParityError::Usage("u".into()),
    ] {
        assert!(!e.is_gate_failure(), "{e:?}");
    }
}

// ---------------------------------------------------------------------------
// PARITY-15 .. PARITY-19: validate's checks before any job
// ---------------------------------------------------------------------------

#[test]
fn parity_15_bad_options_are_a_usage_error() {
    assert_eq!(Options::default(), Options { threads: 1, runs: (0..8).collect() });
    let nowhere = scratch("parity-15").join("absent");
    for opts in [
        Options { threads: 0, ..Options::default() },
        Options { runs: vec![], ..Options::default() },
        Options { runs: vec![8], ..Options::default() },
        Options { runs: vec![3, 100], ..Options::default() },
        Options { runs: vec![2, 3, 2], ..Options::default() },
    ] {
        let e = refuses(&format!("{opts:?}"), || validate(&nowhere, &opts));
        assert!(matches!(e, ParityError::Usage(_)), "{opts:?}: {e:?}");
        assert!(!e.is_gate_failure());
    }
}

#[test]
fn parity_16_a_missing_required_file_is_reported_first_before_any_parsing() {
    assert_eq!(required_files(), &THIRTEEN[..]);
    // Empty placeholder files: presence is all that is checked first.
    let make = |name: &str, absent: &[&str]| {
        let dir = scratch(name);
        for rel in THIRTEEN {
            std::fs::create_dir_all(dir.join(rel).parent().unwrap()).unwrap();
            if !absent.contains(&rel) {
                std::fs::write(dir.join(rel), b"").unwrap();
            }
        }
        dir
    };
    for (i, rel) in THIRTEEN.iter().enumerate() {
        let dir = make(&format!("parity-16-{i}"), &[rel]);
        match refuses(rel, || validate(&dir, &Options::default())) {
            ParityError::Missing { rel: got, path } => {
                assert_eq!(got, *rel);
                assert_eq!(path, dir.join(rel));
                let e = ParityError::Missing { rel: got, path: path.clone() };
                assert_eq!(e.to_string(), format!("{} not found. {}", path.display(), how_to_get(rel)));
            }
            other => panic!("{rel} absent: {other:?}"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
    // Two absent: the first in required order is the one reported.
    let dir = make("parity-16-two", &[THIRTEEN[12], THIRTEEN[4]]);
    match validate(&dir, &Options::default()) {
        Err(ParityError::Missing { rel, .. }) => assert_eq!(rel, THIRTEEN[4]),
        other => panic!("{:?}", other.map(|_| ())),
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `validate` on a relocated copy with `rel` replaced by `bytes`.
fn validate_altered(name: &str, rel: &str, bytes: Vec<u8>) -> ParityError {
    let dir = relocated(name, &[(rel, Alter::Replace(bytes))]);
    let e = refuses(name, || validate(&dir, &Options { threads: 2, runs: vec![3] }));
    std::fs::remove_dir_all(&dir).unwrap();
    e
}

fn mismatch(e: ParityError) -> String {
    match e {
        ParityError::Mismatch(m) => m,
        other => panic!("expected a Mismatch, got {other:?}"),
    }
}

#[test]
fn parity_17_the_fixture_checks_refuse_an_inconsistent_reference_before_any_job() {
    if !have_fixtures() {
        return;
    }
    let text = read_real_text(MANIFEST);
    let n = manifest().n_nodes;

    // The manifest's runs are not RUNS: run 7's precision changed. The spec
    // calls every such failure a fixture error; `from_parts` refuses it
    // first, as Malformed naming the manifest.
    let runs_at = find(&text, "\"runs\"");
    let r7 = runs_at + find(&text[runs_at..], "\"index\": 7,");
    let altered = replace_after(&text, r7, "\"precision\": \"f64\"", "\"precision\": \"f32\"");
    match validate_altered("parity-17-runs", MANIFEST, altered.into_bytes()) {
        ParityError::Mismatch(m) => assert!(m.contains("run"), "{m}"),
        ParityError::Malformed { rel, detail } => assert_eq!(rel, MANIFEST, "{detail}"),
        other => panic!("runs not RUNS: {other:?}"),
    }

    // A centre node outside the network, checked before indexing.
    let centre_at = find(&text, "\"centre_nodes\"");
    let first = &manifest().centre_nodes[0];
    let altered = replace_after(&text, centre_at, &format!("\"node\": {},", first.node), &format!("\"node\": {n},"));
    assert_eq!(
        mismatch(validate_altered("parity-17-outside", MANIFEST, altered.into_bytes())),
        format!("centre node 0: node {n} is outside the network's {n} nodes")
    );
    let altered = replace_after(&text, centre_at, &format!("\"node\": {},", first.node), "\"node\": 4294967295,");
    assert_eq!(
        mismatch(validate_altered("parity-17-outside-max", MANIFEST, altered.into_bytes())),
        format!("centre node 0: node 4294967295 is outside the network's {n} nodes")
    );

    // A centre node of the wrong cell type.
    let second = manifest().centre_nodes[1].node;
    let altered = replace_after(&text, centre_at, &format!("\"node\": {},", first.node), &format!("\"node\": {second},"));
    let m = mismatch(validate_altered("parity-17-type", MANIFEST, altered.into_bytes()));
    assert!(m.starts_with(&format!("centre node 0: node {second} is ")), "{m}");

    // Sequences: not indexed 0-11.
    let seq_at = find(&text, "\"sequences\"");
    let altered = replace_after(&text, seq_at, "\"index\": 11,", "\"index\": 12,");
    assert_eq!(
        mismatch(validate_altered("parity-17-index", MANIFEST, altered.into_bytes())),
        "manifest sequences must be listed in index order: entry 11 is sequence 12"
    );

    // Sequences: one with 5 snapshot frames.
    let s0 = seq_at + find(&text[seq_at..], "\"1/50\": {");
    let snaps = s0 + find(&text[s0..], "\"snapshot_frames\": [") + "\"snapshot_frames\": [".len();
    let end = snaps + find(&text[snaps..], "]");
    let kept: Vec<&str> = text[snaps..end].split(',').map(str::trim).collect();
    assert_eq!(kept.len(), 6);
    let altered = format!("{}{}{}", &text[..snaps], kept[..5].join(", "), &text[end..]);
    assert_eq!(
        mismatch(validate_altered("parity-17-snapshots", MANIFEST, altered.into_bytes())),
        "sequence 0's snapshot frames: expected 6, found 5"
    );

    // A block of the wrong shape (same size, so the manifest still parses).
    let file_at = find(&text, "\"name\": \"settling_centre.f64.bin\"");
    let block = file_at + find(&text[file_at..], "\"run\": 3,");
    let shape = block + find(&text[block..], "\"shape\": [") + "\"shape\": [".len();
    let end = shape + find(&text[shape..], "]");
    let altered = format!("{}65, 50{}", &text[..shape], &text[end..]);
    assert_eq!(
        mismatch(validate_altered("parity-17-shape", MANIFEST, altered.into_bytes())),
        "run 3: settling_centre shape: expected [50, 65], found [65, 50]"
    );

    // A registered gap that differs.
    let (model, dt, gap, _) = SETTLING_F32_GAPS[1];
    let m = mismatch(validate_altered("parity-17-gap", PROVENANCE, provenance_with_gap(model, dt, "settled", gap)));
    assert!(m.starts_with(&format!("{model} {dt} settled")), "{m}");
}

/// The real PROVENANCE.json with one `f32_f64_gap` value one ulp up.
fn provenance_with_gap(model: &str, dt: &str, quantity: &str, gap: f64) -> Vec<u8> {
    let text = read_real_text(PROVENANCE);
    let gaps = find(&text, "\"f32_f64_gap\"");
    let k = gaps + find(&text[gaps..], &format!("\"{model} {dt}\""));
    let label = format!("\"{quantity}\": ");
    let at = k + find(&text[k..], &label) + label.len();
    let len = text[at..].find([',', '}', '\n']).unwrap();
    let value: f64 = text[at..at + len].trim().parse().unwrap();
    assert_eq!(value.to_bits(), gap.to_bits(), "the real {model} {dt} {quantity} gap");
    format!("{}{:e}{}", &text[..at], f64::from_bits(gap.to_bits() + 1), &text[at + len..]).into_bytes()
}

#[test]
fn parity_18_a_file_whose_length_disagrees_is_refused_by_name_and_nothing_is_hashed() {
    if !have_fixtures() {
        return;
    }
    // One byte short of the manifest's `bytes`.
    let rel = "flyvis-reference/centre.f64.bin";
    let mut bytes = read_real(rel);
    bytes.pop();
    let len = bytes.len();
    assert_eq!(
        mismatch(validate_altered("parity-18-short", rel, bytes)),
        format!("centre.f64.bin: {len} bytes, manifest says {}", len + 1)
    );

    // PROVENANCE's `sizes` disagrees.
    let text = read_real_text(PROVENANCE);
    let sizes = find(&text, "\"sizes\"");
    let real = std::fs::metadata(real_data().join("flyvis-reference/input.f64.bin")).unwrap().len();
    let altered = replace_after(&text, sizes, &format!("\"input.f64.bin\": {real}"), &format!("\"input.f64.bin\": {}", real + 1));
    assert_eq!(
        mismatch(validate_altered("parity-18-sizes", PROVENANCE, altered.into_bytes())),
        format!("input.f64.bin: {real} bytes, PROVENANCE sizes says {}", real + 1)
    );

    // No hash is checked: every recorded sha256 of the reference files
    // replaced, and validate still runs and passes.
    let text = read_real_text(MANIFEST);
    let mut out = String::new();
    let mut rest = text.as_str();
    let mut replaced = 0;
    while let Some(i) = rest.find("\"sha256\": \"") {
        let start = i + "\"sha256\": \"".len();
        let end = start + rest[start..].find('"').unwrap();
        out.push_str(&rest[..start]);
        out.push_str(&"0".repeat(end - start));
        rest = &rest[end..];
        replaced += 1;
    }
    out.push_str(rest);
    assert!(replaced >= 9, "the manifest records a sha256 per file: {replaced}");
    let dir = relocated("parity-18-hashes", &[(MANIFEST, Alter::Replace(out.into_bytes()))]);
    let r = validate(&dir, &Options { threads: 4, runs: vec![3] });
    std::fs::remove_dir_all(&dir).unwrap();
    let r = r.unwrap_or_else(|e| panic!("validate hashed something: {e}"));
    assert!(r.passed(), "run 3 still passes with every sha256 changed");
}

#[test]
fn parity_19_a_provenance_boolean_read_the_other_way_is_a_mismatch_naming_its_path() {
    if !have_fixtures() {
        return;
    }
    let text = read_real_text(PROVENANCE);
    let cases = [
        ("\"git\"", "\"dirty\": false", "\"dirty\": true", "git.dirty is true"),
        ("\"threads\"", "\"bit_equal\": true", "\"bit_equal\": false", "threads.bit_equal is false"),
        (
            "\"parameter_equality\"",
            "\"all_50_checkpoints_equal_ensemble_csv\": true",
            "\"all_50_checkpoints_equal_ensemble_csv\": false",
            "parameter_equality.all_50_checkpoints_equal_ensemble_csv is false",
        ),
        (
            "\"equivalence\"",
            "\"stimulus_response_bit_equal\": true",
            "\"stimulus_response_bit_equal\": false",
            "equivalence.stimulus_response_bit_equal is false",
        ),
    ];
    for (i, (section, from, to, what)) in cases.into_iter().enumerate() {
        let altered = replace_after(&text, find(&text, section), from, to);
        assert_eq!(
            mismatch(validate_altered(&format!("parity-19-{i}"), PROVENANCE, altered.into_bytes())),
            format!("PROVENANCE {what}: the reference is not the one registered")
        );
    }
}

// ---------------------------------------------------------------------------
// PARITY-20, PARITY-21: the jobs and the pool
// ---------------------------------------------------------------------------

#[test]
fn parity_20_validate_runs_two_jobs_per_run_in_order_and_keeps_each_jobs_error() {
    if !have_fixtures() {
        return;
    }
    let order = |r: &Report| r.jobs.iter().map(|j| (j.run, j.counts)).collect::<Vec<_>>();
    let want = vec![(2, Counts::Torch), (2, Counts::Libm), (3, Counts::Torch), (3, Counts::Libm)];

    let ok = report_runs_2_3();
    assert_eq!(order(ok), want);
    assert_eq!(ok.threads, 1);
    assert!(ok.jobs.iter().all(Job::passed), "every job passes on the real data");
    for j in &ok.jobs {
        let (_, model, dt, p) = RUNS[j.run as usize];
        assert_eq!((j.model.as_str(), j.dt.as_str(), j.precision), (model, dt, p));
    }

    // 16 threads asked, 4 jobs: 4 workers. Run 2's jobs fail; 3's carry on.
    let broken = report_with_run_2_broken();
    assert_eq!(order(broken), want);
    assert_eq!(broken.threads, 4, "min(threads, jobs)");
    for j in &broken.jobs[..2] {
        match &j.outcome {
            Err(ParityError::Mismatch(m)) => assert!(m.contains("±0.0"), "{m}"),
            other => panic!("run 2 {:?}: {:?}", j.counts, other.as_ref().map(|_| ())),
        }
        assert!(!j.passed());
    }
    for (a, b) in broken.jobs[2..].iter().zip(&ok.jobs[2..]) {
        assert!(a.passed(), "run 3 {:?} carries on", a.counts);
        assert_eq!(a, b, "run 3 {:?} is unaffected by run 2's error", a.counts);
    }
    assert!(!broken.passed());

    // Runs [3] on 8 threads: 2 jobs, 2 workers.
    let one = validate(&real_data(), &Options { threads: 8, runs: vec![3] }).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(one.threads, 2);
    assert_eq!(order(&one), [(3, Counts::Torch), (3, Counts::Libm)]);
    assert_eq!(one.jobs[..], ok.jobs[2..]);
    // (A worker that cannot be spawned is not something a test can cause
    // from outside; that branch is not exercised here.)
}

#[test]
fn parity_21_the_thread_count_changes_nothing() {
    if !have_fixtures() {
        return;
    }
    let one = report_runs_2_3();
    let four = validate(&real_data(), &Options { threads: 4, runs: vec![2, 3] }).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((one.threads, four.threads), (1, 4));
    assert_eq!(one.jobs, four.jobs);
    assert_eq!(one.controls, four.controls);
    assert_eq!(one.coverage, four.coverage);
    assert!(one.passed() && four.passed());
}

// PARITY-22: not reachable through the public API. The replacement happens
// inside `validate` after its pool finishes; every job there is filled by
// `run` for that job's own run and counts, and `validate` takes no closure or
// result from the caller, so no input can make a slot hold another job's
// result. `Job::passed`'s own refusal of a mismatched run or counts is
// PARITY-23.

// ---------------------------------------------------------------------------
// PARITY-23 .. PARITY-26: the pass rules, the controls and coverage
// ---------------------------------------------------------------------------

#[test]
fn parity_23_a_job_passes_only_when_complete_its_own_and_within_its_bounds() {
    for r in [2u32, 3] {
        assert!(job(r, Counts::Torch).passed() && job(r, Counts::Libm).passed(), "run {r}");
    }
    let edit = |counts: Counts, f: &dyn Fn(&mut RunResult)| {
        let mut j = job(2, counts);
        f(j.outcome.as_mut().unwrap());
        j.passed()
    };
    let mut j = job(2, Counts::Torch);
    j.outcome = Err(ParityError::Mismatch("x".into()));
    assert!(!j.passed(), "an Err outcome");
    let mut j = job(2, Counts::Libm);
    j.outcome = Err(ParityError::NonFinite { run: 2, node: 0, cell_type: "R1".into(), after: After::SettleStep(0) });
    assert!(!j.passed(), "a non-finite outcome");
    assert!(!edit(Counts::Torch, &|r| {
        r.sequences.pop();
    }), "11 sequences");
    assert!(!edit(Counts::Torch, &|r| r.sequences.push(sequence(12))), "13 sequences");
    assert!(!edit(Counts::Torch, &|r| r.sequences.swap(3, 4)), "out of order");
    assert!(!edit(Counts::Torch, &|r| r.sequences[0].sequence = 1), "a repeated sequence");
    assert!(!edit(Counts::Torch, &|r| r.settling.run = 3), "another run's result");
    assert!(!edit(Counts::Libm, &|r| r.settling.counts = Counts::Torch), "other counts' result");
    assert!(!edit(Counts::Torch, &|r| r.settling.counts = Counts::Libm), "other counts' result");
    // Exact: every differ is 0.
    assert!(!edit(Counts::Torch, &|r| r.settling.settled.differ = 1));
    assert!(!edit(Counts::Torch, &|r| r.settling.settling_centre.differ = 1));
    assert!(!edit(Counts::Torch, &|r| r.sequences[11].centre.differ = 1));
    assert!(!edit(Counts::Torch, &|r| r.sequences[5].snapshots.differ = 1));
    // Exact does not look at max; libm does not look at differ.
    assert!(edit(Counts::Torch, &|r| r.sequences[5].snapshots.max = 1e-30));
    assert!(edit(Counts::Libm, &|r| r.sequences[5].snapshots.differ = 1000));
    // Libm: every max <= its bound's value, at the bound included.
    let j = job(2, Counts::Libm);
    let (bs, bc) = (j.settling_bounds.0.value(), j.settling_bounds.1.value());
    let (tc, ts) = (j.trace_bounds.0.value(), j.trace_bounds.1.value());
    assert!(bs > 0.0 && bc > 0.0 && tc > 0.0 && ts > 0.0);
    let up = |v: f64| f64::from_bits(v.to_bits() + 1);
    assert!(edit(Counts::Libm, &|r| r.settling.settled.max = bs));
    assert!(!edit(Counts::Libm, &|r| r.settling.settled.max = up(bs)));
    assert!(edit(Counts::Libm, &|r| r.settling.settling_centre.max = bc));
    assert!(!edit(Counts::Libm, &|r| r.settling.settling_centre.max = up(bc)));
    assert!(edit(Counts::Libm, &|r| r.sequences[7].centre.max = tc));
    assert!(!edit(Counts::Libm, &|r| r.sequences[7].centre.max = up(tc)));
    assert!(edit(Counts::Libm, &|r| r.sequences[7].snapshots.max = ts));
    assert!(!edit(Counts::Libm, &|r| r.sequences[7].snapshots.max = up(ts)));
    assert!(!edit(Counts::Libm, &|r| r.sequences[0].centre.max = f64::INFINITY));
    // A run 3 libm job's bound is F64_BOUND.
    let mut j = job(3, Counts::Libm);
    j.outcome.as_mut().unwrap().settling.settled.max = F64_BOUND;
    assert!(j.passed());
    j.outcome.as_mut().unwrap().settling.settled.max = up(F64_BOUND);
    assert!(!j.passed());
}

#[test]
fn parity_24_the_controls_pass_only_when_all_four_returned_and_hold() {
    assert!(controls().passed());
    let e = || ParityError::Mismatch("x".into());
    let check = |what: &str, f: &dyn Fn(&mut Controls), want: bool| {
        let mut c = controls();
        f(&mut c);
        assert_eq!(c.passed(), want, "{what}");
    };
    check("wrong model errored", &|c| c.wrong_model = Err(e()), false);
    check("integer counts errored", &|c| c.integer_counts = Err(e()), false);
    check("mirrored errored", &|c| c.mirrored = Err(e()), false);
    check("flyvis start errored", &|c| c.from_flyvis = Err(e()), false);
    let up = |v: f64| f64::from_bits(v.to_bits() + 1);
    check("wrong model at CONTROL_MISS", &|c| c.wrong_model.as_mut().unwrap().max = CONTROL_MISS, false);
    check("wrong model past CONTROL_MISS", &|c| c.wrong_model.as_mut().unwrap().max = up(CONTROL_MISS), true);
    check("integer counts at F64_BOUND", &|c| c.integer_counts.as_mut().unwrap().max = F64_BOUND, false);
    check("integer counts past F64_BOUND", &|c| c.integer_counts.as_mut().unwrap().max = up(F64_BOUND), true);
    check("mirrored snapshots at CONTROL_MISS", &|c| c.mirrored.as_mut().unwrap().snapshots.max = CONTROL_MISS, false);
    check("mirrored snapshots past CONTROL_MISS", &|c| c.mirrored.as_mut().unwrap().snapshots.max = up(CONTROL_MISS), true);
    check("flyvis start, a centre difference", &|c| c.from_flyvis.as_mut().unwrap().centre.differ = 1, false);
    check("flyvis start, a snapshot difference", &|c| c.from_flyvis.as_mut().unwrap().snapshots.differ = 1, false);
    assert_eq!(CONTROL_MISS, 1e-3);

    // On the real data, as validate runs them.
    if !have_fixtures() {
        return;
    }
    let c = &report_runs_2_3().controls;
    assert!(c.passed(), "{c:?}");
    assert!(c.wrong_model.as_ref().unwrap().max > CONTROL_MISS);
    assert!(c.integer_counts.as_ref().unwrap().max > F64_BOUND);
    let m = c.mirrored.as_ref().unwrap();
    assert_eq!(m.sequence, 8);
    assert!(m.snapshots.max > CONTROL_MISS);
    let s = c.from_flyvis.as_ref().unwrap();
    assert_eq!(s.sequence, 8);
    assert_eq!((s.centre.differ, s.snapshots.differ), (0, 0));
    // The wrong-model control is run 3 settled with 049's parameters.
    let f = fixtures();
    assert_eq!(c.wrong_model.as_ref().unwrap(), &settle(f, 3, "049", Counts::Torch).unwrap().settled);
    assert_eq!(c.integer_counts.as_ref().unwrap(), &settle(f, 3, "000", Counts::IntegerNSyn).unwrap().settled);
}

#[test]
fn parity_25_a_report_passes_only_with_every_job_once_and_is_complete_only_for_all_eight() {
    assert!(report(vec![2, 3]).passed());
    assert!(report((0..8).collect()).passed());
    assert!(!report(vec![2, 3]).complete());
    assert!(report((0..8).collect()).complete());
    assert!(!report(vec![0, 1, 2, 3, 4, 5, 7, 6]).complete(), "out of order");
    assert!(!report((0..7).collect()).complete(), "seven runs");

    let broken = |what: &str, f: &dyn Fn(&mut Report)| {
        let mut r = report(vec![2, 3]);
        f(&mut r);
        assert!(!r.passed(), "{what}");
    };
    broken("a missing job", &|r| {
        r.jobs.pop();
    });
    broken("an extra job", &|r| r.jobs.push(job(2, Counts::Torch)));
    broken("a duplicated (run, counts)", &|r| r.jobs[3] = job(3, Counts::Torch));
    broken("another run's jobs", &|r| r.jobs[0] = job(4, Counts::Torch));
    broken("a failed job", &|r| r.jobs[1].outcome = Err(ParityError::Mismatch("x".into())));
    broken("failed controls", &|r| r.controls.wrong_model = Err(ParityError::Mismatch("x".into())));
    broken("a missing coverage entry", &|r| {
        r.coverage.pop();
    });
    broken("an extra coverage entry", &|r| r.coverage.push(Coverage { run: 3, movement: None }));
    broken("coverage for the wrong run", &|r| r.coverage[0].run = 5);

    // Coverage is recorded, not a gate: an unmeasured or undriven run passes.
    let mut r = report(vec![2, 3]);
    r.coverage[0].movement = None;
    r.coverage[1].movement = Some(vec![0.0; 65]);
    assert!(r.passed());
}

/// Each centre type's movement, computed from flyvis's reference: the
/// exact job is bit-equal to it, so Rust's centre and settled values are
/// flyvis's.
fn reference_movement(run_: u32) -> Vec<f64> {
    let (_, _, dt, p) = RUNS[run_ as usize];
    let sfx = if p == Dtype::F32 { "f32" } else { "f64" };
    let m = manifest();
    let read = |name: &str, key: &BlockKey| -> Vec<f64> {
        let bytes = read_real(&format!("flyvis-reference/{name}"));
        let info = m.file(name).unwrap();
        match p {
            Dtype::F32 => info.read_f32(&bytes, key).unwrap().data.iter().map(|&v| f64::from(v)).collect(),
            Dtype::F64 => info.read_f64(&bytes, key).unwrap().data,
        }
    };
    let settled = read(&format!("settled.{sfx}.bin"), &BlockKey { run: Some(run_), dt, sequence: None });
    let centre_nodes: Vec<usize> = m.centre_nodes.iter().map(|c| c.node as usize).collect();
    let settled_centre: Vec<f64> = centre_nodes.iter().map(|&n| settled[n]).collect();
    let bytes = read_real(&format!("flyvis-reference/centre.{sfx}.bin"));
    let info = m.file(&format!("centre.{sfx}.bin")).unwrap();
    let mut movement = vec![0.0f64; centre_nodes.len()];
    for s in 0..12u32 {
        let key = BlockKey { run: Some(run_), dt, sequence: Some(s) };
        let values: Vec<f64> = match p {
            Dtype::F32 => info.read_f32(&bytes, &key).unwrap().data.iter().map(|&v| f64::from(v)).collect(),
            Dtype::F64 => info.read_f64(&bytes, &key).unwrap().data,
        };
        for row in values.chunks_exact(centre_nodes.len()) {
            for (k, v) in row.iter().enumerate() {
                movement[k] = movement[k].max((v - settled_centre[k]).abs());
            }
        }
    }
    movement
}

#[test]
fn parity_26_coverage_is_the_exact_jobs_centre_movement_and_none_when_it_failed() {
    assert_eq!(COVERAGE_THRESHOLD, 1e-3);
    if !have_fixtures() {
        return;
    }
    let r = report_runs_2_3();
    assert_eq!(r.coverage.iter().map(|c| c.run).collect::<Vec<_>>(), [2, 3]);
    for c in &r.coverage {
        let movement = c.movement.as_ref().unwrap_or_else(|| panic!("run {} passed but has no coverage", c.run));
        let torch = r.jobs.iter().find(|j| j.run == c.run && j.counts == Counts::Torch).unwrap();
        assert!(torch.passed());
        assert_eq!(movement, &torch.outcome.as_ref().unwrap().centre_movement);
        let want = reference_movement(c.run);
        assert_eq!(movement.len(), CELL_TYPES.len());
        for (k, (a, b)) in movement.iter().zip(&want).enumerate() {
            assert_eq!(a.to_bits(), b.to_bits(), "run {} {}: {a:e} vs {b:e}", c.run, CELL_TYPES[k]);
        }
        let driven = movement.iter().filter(|&&m| m > COVERAGE_THRESHOLD).count();
        eprintln!("run {}: {driven} of 65 centre types driven", c.run);
        assert!(driven > 0);
    }
    // The run whose exact job failed is not measured; the other still is.
    let broken = report_with_run_2_broken();
    assert_eq!(broken.coverage[0], Coverage { run: 2, movement: None });
    assert_eq!(broken.coverage[1], r.coverage[1]);
}

// ---------------------------------------------------------------------------
// PARITY-27, PARITY-29, PARITY-30
// ---------------------------------------------------------------------------

#[test]
fn parity_27_the_libm_fingerprint_counts_differences_from_torch_and_hashes_the_bits() {
    let f = libm_fingerprint();
    let a: Vec<f32> = SYN_COUNT.iter().map(|&c| c.exp()).collect();
    let b: Vec<f64> = SYN_COUNT.iter().map(|&c| f64::from(c).exp()).collect();
    assert_eq!(a.len(), 2355);
    let f32_differ = a.iter().zip(&SYN_COUNT_TORCH_F32).filter(|(x, y)| x.to_bits() != y.to_bits()).count();
    let f64_differ = b.iter().zip(&SYN_COUNT_TORCH_F64).filter(|(x, y)| x.to_bits() != y.to_bits()).count();
    assert_eq!((f.f32_differ, f.f64_differ), (f32_differ, f64_differ));
    assert!(f.f32_differ <= 200 && f.f64_differ <= 200, "{f}");
    let bytes32: Vec<u8> = a.iter().flat_map(|v| v.to_le_bytes()).collect();
    let bytes64: Vec<u8> = b.iter().flat_map(|v| v.to_le_bytes()).collect();
    assert_eq!(f.f32_fnv, fnv1a(&bytes32));
    assert_eq!(f.f64_fnv, fnv1a(&bytes64));
    assert_eq!(
        f.to_string(),
        format!("f32:{}:{:016x},f64:{}:{:016x}", f32_differ, fnv1a(&bytes32), f64_differ, fnv1a(&bytes64))
    );
    assert_eq!(libm_fingerprint(), f, "the same host gives the same fingerprint");
    eprintln!("libm fingerprint: {f}");
}

// PARITY-28: not reachable through the public API. `Precision` and its
// `libm_count` are `pub(crate)`; an integration test cannot call them.

#[test]
fn parity_29_the_fingerprint_hash_is_fnv_1a() {
    // The reference implementation here meets FNV-1a's published vectors...
    assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
    assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    // ...and the fingerprint's hashes are that function of its bits.
    let f = libm_fingerprint();
    let a: Vec<u8> = SYN_COUNT.iter().flat_map(|&c| c.exp().to_le_bytes()).collect();
    let b: Vec<u8> = SYN_COUNT.iter().flat_map(|&c| f64::from(c).exp().to_le_bytes()).collect();
    assert_eq!((f.f32_fnv, f.f64_fnv), (fnv1a(&a), fnv1a(&b)));
}

#[test]
fn parity_30_how_to_get_keeps_its_six_texts() {
    assert_eq!(
        how_to_get("flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json"),
        "Download https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/\
         41586_2024_7939_MOESM3_ESM.zip and extract Lappalainen_et_al_connectome_avg_filters.json into \
         data/flyvis-supplementary/, or run ci/fetch-fixtures.sh"
    );
    assert_eq!(
        how_to_get("flyvis-build/nodes.csv"),
        "Run `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
    );
    assert_eq!(
        how_to_get("flyvis-stimuli/flashes.bin"),
        "Run `uv run --project tools tools/export_stimuli.py`, `uv run --project tools \
         tools/export_moving_edges.py` and, for the `_h80` files, `uv run --project tools \
         tools/export_reference.py` to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
    );
    assert_eq!(
        how_to_get("flyvis-pretrained/ensemble.csv"),
        "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_params.py`."
    );
    assert_eq!(
        how_to_get("flyvis-reference/manifest.json"),
        "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
    );
    assert_eq!(how_to_get("elsewhere/x"), "Run ci/fetch-fixtures.sh.");
    assert_eq!(how_to_get(""), "Run ci/fetch-fixtures.sh.");
    // By directory only.
    assert_eq!(how_to_get("flyvis-reference"), how_to_get("flyvis-reference/anything/else.bin"));
}

// ---------------------------------------------------------------------------
// PARITY-31 .. PARITY-33: the #[cfg(test)] gates, in a shadow workspace
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
        let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-parity-shadow");
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

struct Outcome {
    passed: bool,
    stdout: String,
    stderr: String,
}

impl Shadow {
    /// Lays out data/ with every real fixture linked in, `alter` applied.
    fn data(&self, alter: &[(&str, Alter)]) {
        let data = self.root.join("data");
        if data.symlink_metadata().is_ok() {
            std::fs::remove_dir_all(&data).unwrap();
        }
        std::fs::create_dir_all(&data).unwrap();
        let real = real_data();
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
            match a {
                Alter::Replace(bytes) => std::fs::write(&path, bytes).unwrap(),
                Alter::Remove => {}
                Alter::Dir => std::fs::create_dir_all(&path).unwrap(),
            }
        }
    }

    fn cargo(&self, args: &[&str], require: bool) -> Outcome {
        self.cargo_with(args, require, &[])
    }

    fn cargo_with(&self, args: &[&str], require: bool, env: &[(&str, &std::ffi::OsStr)]) -> Outcome {
        let mut cmd = Command::new(option_env!("CARGO").unwrap_or("cargo"));
        cmd.current_dir(&self.root).env("CARGO_TARGET_DIR", &self.target).args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        if require {
            cmd.env("SEMBLANCE_REQUIRE_FIXTURES", "1");
        } else {
            cmd.env_remove("SEMBLANCE_REQUIRE_FIXTURES");
        }
        let out = cmd.output().expect("cargo runs");
        Outcome {
            passed: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// One of semblance-flyvis's own lib tests, by full path.
    fn gate(&self, test: &str, require: bool) -> Outcome {
        let o = self.cargo(
            &["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--", "--exact", test, "--nocapture", "--test-threads=1"],
            require,
        );
        assert!(o.stdout.contains("running 1 test"), "the gate {test} must run exactly once:\n{}{}", o.stdout, o.stderr);
        o
    }
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

const TRACE_TESTS: [&str; 22] = [
    "trace_exact_run_0_000_1_200_f32",
    "trace_libm_run_0_000_1_200_f32",
    "trace_exact_run_1_000_1_200_f64",
    "trace_libm_run_1_000_1_200_f64",
    "trace_exact_run_2_000_1_50_f32",
    "trace_libm_run_2_000_1_50_f32",
    "trace_exact_run_3_000_1_50_f64",
    "trace_libm_run_3_000_1_50_f64",
    "trace_exact_run_4_049_1_200_f32",
    "trace_libm_run_4_049_1_200_f32",
    "trace_exact_run_5_049_1_200_f64",
    "trace_libm_run_5_049_1_200_f64",
    "trace_exact_run_6_049_1_50_f32",
    "trace_libm_run_6_049_1_50_f32",
    "trace_exact_run_7_049_1_50_f64",
    "trace_libm_run_7_049_1_50_f64",
    "trace_bounds_match_provenance",
    "snapshots_agree_with_the_centre_column",
    "the_trace_gate_sees_a_wrong_mapping",
    "a_run_from_flyviss_settled_state_is_bit_equal_too",
    "stimulus_checks::rust_flashes_equal_flyviss_input",
    "stimulus_checks::rust_moving_edges_match_flyviss_input",
];

/// The hand-written gate tests' doc comments, as they read before the
/// extraction (`git show a174645:crates/flyvis/src/{settling,trace}_checks.rs`).
const DOCS: [(&str, &str, &[&str]); 10] = [
    ("settling_checks", "reference_runs_are_the_eight_expected", &[
        "/// The manifest's runs are exactly the eight the tests above are named for,",
        "/// and its centre nodes are the expanded network's, one per cell type.",
    ]),
    ("settling_checks", "pre_registered_bounds_match_provenance", &["/// The gaps above are PROVENANCE.json's, bit for bit."]),
    ("settling_checks", "the_gate_tells_models_apart", &[
        "/// A control: the wrong model's parameters, against run 3's reference",
        "/// (model 000, dt 0.02, f64), must miss by far more than any bound. A gate",
        "/// that compared nothing to nothing would pass this.",
    ]),
    ("settling_checks", "the_gate_resolves_integer_counts", &[
        "/// A control at the scale the libm bound exists to resolve: the spec's",
        "/// integer counts in place of exp(ln count) must exceed the f64 bound.",
        "/// Measured 4.8e-7 to 6.7e-6 across runs during the spec's review.",
        "///",
        "/// It resolves them against the f64 bound only. The f32 libm bound does not",
        "/// resolve integer counts in 3 of the 4 f32 runs; in f32 the exact gate is",
        "/// the guard (the graded-settling spec's 2026-10-29 amendment).",
    ]),
    ("trace_checks", "trace_bounds_match_provenance", &["/// The gaps above are PROVENANCE.json's, bit for bit."]),
    ("trace_checks", "snapshots_agree_with_the_centre_column", &[
        "/// `centre[snapshot_frames]` is `snapshots[:, centre_nodes]` in flyvis's",
        "/// own reference (fact H7). The gates index both by that assumption.",
    ]),
    ("trace_checks", "the_trace_gate_sees_a_wrong_mapping", &[
        "/// A negative control: run 3 (000, 0.02, f64), sequence 8 (the 90 degree",
        "/// OFF edge, which is not point-symmetric), with every frame's hexals",
        "/// mirrored (u, v) -> (-u, -v) before scattering. A wrong mapping must miss",
        "/// by far more than any bound. (A full-field flash would not change.)",
        "/// Measured during the spec's review: about 2.5.",
    ]),
    ("trace_checks", "a_run_from_flyviss_settled_state_is_bit_equal_too", &[
        "/// R2's first diagnostic step works: restarting from flyvis's own settled",
        "/// state, run 3, sequence 8 is still bit-equal (settling is exact, so the",
        "/// two starts are the same state).",
    ]),
    ("trace_checks", "rust_flashes_equal_flyviss_input", &[
        "/// Flashes at both dts, matched to the reference by intensity and",
        "/// radius, equal flyvis's input exactly: frame count, stimulus window",
        "/// and every value.",
    ]),
    ("trace_checks", "rust_moving_edges_match_flyviss_input", &[
        "/// Moving edges at height 80, speed 13, angles 0/30/90/150, matched by",
        "/// angle, intensity and speed. Frame count and window are exact; every",
        "/// value is within the geometry gate's `TOLERANCE` (the two renderers",
        "/// are different functions); 0 degrees, where flyvis rotates nothing, is",
        "/// bit-equal to the f32 input flyvis's float32 runs fed.",
    ]),
];

/// The print and panic format strings C1-C4, C9-C11 and C14-C15, as they
/// read before the extraction. Each must still be in the crate's sources.
const FORMATS: [&str; 11] = [
    "\"exact run {run} ({}, dt {}, {}): settled {} of {} differ; settling centre {} of {} differ\"",
    "\"run {run} is not bit-equal to flyvis: {} settled and {} settling-centre values differ; \\\n             first difference at {}; largest settled error {:e} at node {} ({})\"",
    "\"libm run {run} ({}, dt {}, {}): settled max {:.3e} at node {} ({}), {:.3e} of bound; \\\n         settling centre max {:.3e} at step {} ({}), {:.3e} of bound; worst types {}; settled bit-equal {} of {}\"",
    "\"run {run}: Rust's exp exceeds the pre-registered bound: settled {:e} (bound {:e}), \\\n         settling centre {:e} (bound {:e})\"",
    "\"trace exact run {run} ({}, dt {}, {}): {} of {} sequences bit-equal, centre and snapshots\"",
    "\"run {run} is not bit-equal to flyvis under stimulus:\\n{}\"",
    "\"sequence {}: {} of {} centre and {} of {} snapshot values differ; first difference at {first}; \\\n             largest centre error {:e}; largest snapshot error {:e} at node {} ({})\"",
    "\"trace libm run {run} ({}, dt {}, {}): worst sequence {}, {:.3e} of bound; \\",
    "snapshots max {:.3e} in sequence {} at frame {} node {} ({}), {:.3e} of bound; bit-equal {equal} of {total}\"",
    "\"mirrored mapping, run 3, sequence 8: snapshots max {:e}, centre max {:e}\"",
    "\"run {} sequence {} snapshot {k} (frame {frame}), node {node} ({}): centre {a:e}, snapshots {b:e}\"",
];

/// The doc comment directly above `#[test] fn name`.
fn doc_of(src: &str, name: &str) -> Vec<String> {
    let lines: Vec<&str> = src.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&format!("fn {name}(")))
        .unwrap_or_else(|| panic!("fn {name} not found"));
    let mut j = i;
    let mut attrs = Vec::new();
    while j > 0 && lines[j - 1].trim_start().starts_with("#[") {
        j -= 1;
        attrs.push(lines[j].trim());
    }
    assert!(attrs.contains(&"#[test]"), "{name} is a test: {attrs:?}");
    let mut doc = Vec::new();
    while j > 0 && lines[j - 1].trim_start().starts_with("///") {
        j -= 1;
        doc.insert(0, lines[j].trim().to_string());
    }
    doc
}

#[test]
fn parity_31_the_gates_keep_their_names_docs_strings_and_registry_check() {
    let src = Path::new(WORKSPACE).join("crates/flyvis/src");
    let settling = std::fs::read_to_string(src.join("settling_checks.rs")).unwrap();
    let trace = std::fs::read_to_string(src.join("trace_checks.rs")).unwrap();

    // They call parity.
    for (name, text) in [("settling_checks", &settling), ("trace_checks", &trace)] {
        assert!(text.contains("crate::parity"), "{name} uses parity");
    }
    assert!(settling.contains("settle(f, run, &info.model, Counts::Torch)") || settling.contains("parity::settle"));
    assert!(trace.contains("play(") || trace.contains("parity::play"));

    // Doc comments, byte for byte.
    for (file, name, want) in DOCS {
        let text = if file == "settling_checks" { &settling } else { &trace };
        assert_eq!(doc_of(text, name), want.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "{file}::{name}");
    }

    // Print and panic strings, byte for byte, somewhere in the crate.
    let mut all = String::new();
    let mut stack = vec![src.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                all.push_str(&std::fs::read_to_string(&p).unwrap());
            }
        }
    }
    for f in FORMATS {
        assert!(settling.contains(f) || trace.contains(f), "format string kept in the gate files: {f}");
    }
    // C5, C12: now the core's NonFinite display (PARITY-14 checks its output).
    assert!(all.contains("is not finite after settle step {k}"));
    assert!(all.contains("is not finite after frame {frame} of sequence {sequence}"));
    // C6, C13: the core's shape and dt messages.
    for s in ["settling steps at dt {}", "settling_centre shape", "settled shape", "centre shape, sequence {s}", "snapshots shape, sequence {s}"] {
        assert!(all.contains(s), "{s}");
    }

    // Each generated test checks its precision against the registry.
    for (name, text, mac) in [("settling_checks", &settling, "settling_runs!"), ("trace_checks", &trace, "trace_runs!")] {
        let body = &text[find(text, &format!("macro_rules! {}", mac.trim_end_matches('!')))..];
        let body = &body[..find(body, "\n}\n")];
        assert!(body.contains("$t:ty"), "{name}: the macro keeps $t");
        assert!(body.contains("exact::<$t>($run)") && body.contains("libm::<$t>($run)"), "{name}: {body}");
        for f in ["fn exact<R: Precision>(run: u32) {", "fn libm<R: Precision>(run: u32) {"] {
            let at = find(text, f) + f.len();
            let first = text[at..].lines().map(str::trim).find(|l| !l.is_empty()).unwrap();
            let checks = first == "assert_registered::<R>(run);"
                || first.starts_with("assert_eq!(R::DTYPE, parity::RUNS[run as usize].3");
            assert!(checks, "{name}: {f} starts with the registry check, not {first:?}");
        }
        if text.contains("fn assert_registered") {
            assert!(text.contains("assert_eq!(R::DTYPE, parity::RUNS[run as usize].3"), "{name}");
        }
    }
    assert!(settling.contains("fn assert_registered") || settling.contains("R::DTYPE, parity::RUNS"));

    // Names (C16), and no compiler warning from either file (C17), in a build.
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[]);
    let o = sh.cargo(&["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--", "--list"], false);
    assert!(o.passed, "listing the lib tests:\n{}{}", o.stdout, o.stderr);
    for f in ["settling_checks.rs", "trace_checks.rs"] {
        assert!(!o.stderr.contains(f), "{f} emits a compiler diagnostic:\n{}", o.stderr);
    }
    let listed = |prefix: &str| -> Vec<String> {
        let mut v: Vec<String> = o
            .stdout
            .lines()
            .filter_map(|l| l.strip_suffix(": test"))
            .filter_map(|l| l.strip_prefix(prefix))
            .map(str::to_string)
            .collect();
        v.sort();
        v
    };
    let mut want: Vec<String> = SETTLING_TESTS.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(listed("settling_checks::"), want, "exactly the 20 settling_checks:: tests");
    let mut want: Vec<String> = TRACE_TESTS.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(listed("trace_checks::"), want, "the trace_checks:: tests");
}

const SETTLING_GATE: &str = "settling_checks::settling_exact_run_3_000_1_50_f64";
const TRACE_GATE: &str = "trace_checks::trace_exact_run_3_000_1_50_f64";
const PARITY_FIXTURE_TESTS: [&str; 2] =
    ["parity::run::tests::run_equals_settle_then_play", "parity::validate::tests::validate_is_bit_identical_across_thread_counts"];

#[test]
fn parity_32_a_malformed_gate_fixture_fails_every_gate_that_uses_it() {
    if !have_fixtures() {
        return;
    }
    let ensemble = read_real_text(ENSEMBLE);
    let cases: [(&str, Vec<u8>, &str); 2] = [
        ("empty", Vec::new(), "flyvis-pretrained/ensemble.csv: ensemble.csv line 1: no header line"),
        ("truncated", ensemble.as_bytes()[..ensemble.len() / 2].to_vec(), "flyvis-pretrained/ensemble.csv: "),
    ];
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    for (what, bytes, message) in cases {
        sh.data(&[(ENSEMBLE, Alter::Replace(bytes))]);
        for gate in [SETTLING_GATE, TRACE_GATE, "settling_checks::the_gate_tells_models_apart"] {
            for require in [false, true] {
                let o = sh.gate(gate, require);
                let out = format!("{}{}", o.stdout, o.stderr);
                assert!(!o.passed, "{what} ensemble.csv, {gate}, require={require}: must fail, not skip:\n{out}");
                assert!(out.contains(message), "{what}: {gate} fails with the error {message:?}:\n{out}");
                assert!(!out.contains("skipping: "), "{what}: {gate} must not skip:\n{out}");
            }
        }
    }
    sh.data(&[]);
}

#[test]
fn parity_33_a_missing_fixture_is_a_skip_notice_or_under_require_a_failure() {
    if !have_fixtures() {
        return;
    }
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(&[(ENSEMBLE, Alter::Remove)]);
    let tail = format!("/crates/flyvis/../../data/{ENSEMBLE}");
    for gate in [SETTLING_GATE, TRACE_GATE].into_iter().chain(PARITY_FIXTURE_TESTS) {
        let o = sh.gate(gate, false);
        let out = format!("{}{}", o.stdout, o.stderr);
        assert!(o.passed, "{gate} skips and passes without ensemble.csv:\n{out}");
        let line = o
            .stderr
            .lines()
            .find(|l| l.starts_with("skipping: "))
            .unwrap_or_else(|| panic!("{gate}: no skip notice:\n{out}"));
        let want_end = format!("{tail} not found. {}", how_to_get(ENSEMBLE));
        assert!(line.ends_with(&want_end), "{gate}: {line}");
        let path = &line["skipping: ".len()..line.len() - format!(" not found. {}", how_to_get(ENSEMBLE)).len()];
        assert!(Path::new(path).is_absolute(), "{gate}: {path}");
        assert!(Path::new(path).starts_with(&sh.root) || path.contains("acceptance-parity-shadow/workspace"), "{path}");

        let o = sh.gate(gate, true);
        let out = format!("{}{}", o.stdout, o.stderr);
        assert!(!o.passed, "{gate} fails under SEMBLANCE_REQUIRE_FIXTURES=1:\n{out}");
        assert!(out.contains(&format!("SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing")), "{gate}:\n{out}");
    }
    sh.data(&[]);
}

// ---------------------------------------------------------------------------
// PARITY-34
// ---------------------------------------------------------------------------

#[test]
fn parity_34_validate_refuses_a_gap_one_ulp_off_or_a_file_one_byte_short() {
    if !have_fixtures() {
        return;
    }
    for &(model, dt, centre, snapshots) in &TRACE_F32_GAPS {
        for (quantity, gap) in [("centre", centre), ("snapshots", snapshots)] {
            let name = format!("parity-34-{model}-{}-{quantity}", dt.replace('/', "_"));
            let m = mismatch(validate_altered(&name, PROVENANCE, provenance_with_gap(model, dt, quantity, gap)));
            assert!(m.starts_with(&format!("{model} {dt} {quantity}: PROVENANCE ")), "{m}");
        }
    }
    for &(model, dt, settled, settling_centre) in &SETTLING_F32_GAPS {
        for (quantity, gap) in [("settled", settled), ("settling_centre", settling_centre)] {
            let name = format!("parity-34-{model}-{}-{quantity}", dt.replace('/', "_"));
            let m = mismatch(validate_altered(&name, PROVENANCE, provenance_with_gap(model, dt, quantity, gap)));
            assert!(m.starts_with(&format!("{model} {dt} {quantity}: PROVENANCE ")), "{m}");
        }
    }
    let rel = "flyvis-reference/centre.f64.bin";
    let mut bytes = read_real(rel);
    bytes.pop();
    let len = bytes.len();
    assert_eq!(
        mismatch(validate_altered("parity-34-short", rel, bytes)),
        format!("centre.f64.bin: {len} bytes, manifest says {}", len + 1)
    );
}

#[test]
fn registry_sanity_the_runs_used_here_are_the_registered_ones() {
    // Guards the hand-built values above: they assume runs 2 and 3 are
    // (000, 1/50) in f32 and f64.
    assert_eq!(RUNS[2], (2, "000", "1/50", Dtype::F32));
    assert_eq!(RUNS[3], (3, "000", "1/50", Dtype::F64));
    let _ = parity::F64_BOUND;
}
