//! Acceptance tests for behaviour group `minors` of epic stimulus-traces:
//! the settling-gate and export fixes carried over from PR #12.
//!
//! One test per behaviour id that concerns `semblance-flyvis`, its tools or
//! its acceptance files (MINORS-01 .. -05, -09, -12, -13, -15). The
//! integrator's ids are in `crates/sim/tests/acceptance_minors.rs`.
//!
//! The fixture readers, the loaders and the gates are `#[cfg(test)]` code
//! inside `semblance-flyvis`, so they are observed the way a person running
//! the suite observes them: these tests copy the workspace's sources
//! (unchanged) into `CARGO_TARGET_TMPDIR`, give the copy a `data/` of
//! symlinks to the real fixtures with the file under test removed or
//! altered, and run the crate's own tests there with `cargo test`, reading
//! what they print and whether they pass. The real `data/` is never written.
//!
//! "Unchanged" (MINORS-02, MINORS-15) is checked against a second copy,
//! `git archive`d from `BASELINE`, the commit the epic started from: the
//! same tests run in both, and their output is compared.
//!
//! MINORS-05 reruns `tools/export_syn_count.py` on a scratch copy of the
//! paths it reads and writes; it skips when the tools' Python environment or
//! flyvis's checkpoints are absent, whatever `SEMBLANCE_REQUIRE_FIXTURES`
//! says, because neither is a fixture CI fetches.
//!
//! Everything else that needs the real fixtures skips without them, unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip into a failure.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};

use semblance_flyvis::reference::Manifest;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The commit stimulus-traces started from: PR #12's code, before the fixes.
const BASELINE: &str = "9d4adfb63439173e25846cc53e7ce32cdd9aff79";

const SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";
const ENSEMBLE: &str = "flyvis-pretrained/ensemble.csv";
const MANIFEST: &str = "flyvis-reference/manifest.json";
const PROVENANCE: &str = "flyvis-reference/PROVENANCE.json";
const SETTLED_F32: &str = "flyvis-reference/settled.f32.bin";
const SETTLED_F64: &str = "flyvis-reference/settled.f64.bin";
const CENTRE_F64: &str = "flyvis-reference/settling_centre.f64.bin";

const DATA_DIRS: [&str; 5] =
    ["flyvis-supplementary", "flyvis-build", "flyvis-stimuli", "flyvis-pretrained", "flyvis-reference"];

/// Everything the settling and trace gates read.
const GATE_FIXTURES: [&str; 13] = [
    SPEC,
    ENSEMBLE,
    MANIFEST,
    PROVENANCE,
    SETTLED_F32,
    SETTLED_F64,
    "flyvis-reference/settling_centre.f32.bin",
    CENTRE_F64,
    "flyvis-reference/input.f64.bin",
    "flyvis-reference/centre.f32.bin",
    "flyvis-reference/centre.f64.bin",
    "flyvis-reference/snapshots.f32.bin",
    "flyvis-reference/snapshots.f64.bin",
];

fn data_path(rel: &str) -> PathBuf {
    Path::new(WORKSPACE).join("data").join(rel)
}

fn fixtures_required() -> bool {
    std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1")
}

/// Whether every file in `rels` is present. If one is not: a skip notice,
/// or under `SEMBLANCE_REQUIRE_FIXTURES=1` a failure.
fn have(rels: &[&str]) -> bool {
    for rel in rels {
        let path = data_path(rel);
        if !path.exists() {
            assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {} is missing", path.display());
            eprintln!("skipping: {} not found; run ci/fetch-fixtures.sh", path.display());
            return false;
        }
    }
    true
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn tmp(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-minors").join(name)
}

fn remove_tree(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("removing {}: {e}", dir.display()),
    }
}

fn read_source(rel: &str) -> String {
    let path = Path::new(WORKSPACE).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A file as committed at `BASELINE`.
fn baseline_source(rel: &str) -> String {
    let out = Command::new("git")
        .current_dir(WORKSPACE)
        .args(["show", &format!("{BASELINE}:{rel}")])
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git show {BASELINE}:{rel}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("UTF-8 source")
}

/// The text of `fn <name>` from its `fn` line to its closing `}` at column 0.
fn function(source: &str, name: &str) -> String {
    let start = source.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("fn {name} not found"));
    let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
    let end = start + source[start..].find("\n}\n").unwrap_or_else(|| panic!("fn {name} does not close")) + 3;
    source[line_start..end].to_string()
}

/// The `///` lines directly above `fn <name>` (attributes skipped).
fn doc_comment(source: &str, name: &str) -> String {
    let start = source.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("fn {name} not found"));
    let mut doc: Vec<&str> = Vec::new();
    for line in source[..start].lines().rev().skip(1) {
        let l = line.trim_start();
        if l.starts_with("#[") {
            continue;
        }
        if let Some(text) = l.strip_prefix("///") {
            doc.push(text.trim());
        } else {
            break;
        }
    }
    doc.reverse();
    doc.join(" ")
}

// ---------------------------------------------------------------------------
// Shadow copies of the workspace
// ---------------------------------------------------------------------------

struct Shadow {
    root: PathBuf,
    target: PathBuf,
}

/// Every shadow's `data/` is rearranged per test: one test at a time.
static LOCK: Mutex<()> = Mutex::new(());
static CURRENT: OnceLock<Shadow> = OnceLock::new();
static BASE: OnceLock<Shadow> = OnceLock::new();

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// The workspace's sources as they stand.
fn current() -> &'static Shadow {
    CURRENT.get_or_init(|| {
        let (root, target) = (tmp("current/workspace"), tmp("current/target"));
        remove_tree(&root);
        std::fs::create_dir_all(&root).unwrap();
        let ws = Path::new(WORKSPACE);
        for f in ["Cargo.toml", "Cargo.lock", "README.md"] {
            std::fs::copy(ws.join(f), root.join(f)).unwrap();
        }
        copy_tree(&ws.join("crates"), &root.join("crates"));
        copy_tree(&ws.join("app"), &root.join("app"));
        Shadow { root, target }
    })
}

/// The sources at `BASELINE`.
fn baseline() -> &'static Shadow {
    BASE.get_or_init(|| {
        let (root, target) = (tmp("baseline/workspace"), tmp("baseline/target"));
        remove_tree(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut git = Command::new("git")
            .current_dir(WORKSPACE)
            .args(["archive", "--format=tar", BASELINE, "Cargo.toml", "Cargo.lock", "README.md", "crates", "app"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("git runs");
        let tar = Command::new("tar")
            .arg("-x")
            .arg("-C")
            .arg(&root)
            .stdin(git.stdout.take().unwrap())
            .status()
            .expect("tar runs");
        assert!(git.wait().unwrap().success() && tar.success(), "git archive {BASELINE}");
        Shadow { root, target }
    })
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

impl Outcome {
    fn lines_with(&self, needle: &str) -> Vec<&str> {
        self.output.lines().filter(|l| l.contains(needle)).collect()
    }
}

impl Shadow {
    /// Lays out `data/`: each real directory linked whole, or, if `alter`
    /// touches it, entry by entry with `alter` applied.
    fn data(&self, alter: &[(&str, Alter)]) {
        let data = self.root.join("data");
        if data.symlink_metadata().is_ok() {
            std::fs::remove_dir_all(&data).unwrap();
        }
        std::fs::create_dir_all(&data).unwrap();
        let real = Path::new(WORKSPACE).join("data");
        for dir in DATA_DIRS {
            if !real.join(dir).exists() {
                continue;
            }
            if alter.iter().any(|(rel, _)| rel.starts_with(&format!("{dir}/"))) {
                std::fs::create_dir_all(data.join(dir)).unwrap();
                for entry in std::fs::read_dir(real.join(dir)).unwrap() {
                    let entry = entry.unwrap();
                    link(&entry.path(), &data.join(dir).join(entry.file_name()));
                }
            } else {
                link(&real.join(dir), &data.join(dir));
            }
        }
        for (rel, a) in alter {
            let path = data.join(rel);
            std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            if let Alter::Replace(bytes) = a {
                std::fs::write(&path, bytes).unwrap();
            }
        }
    }

    fn cargo_test(&self, args: &[&str], require: bool) -> Outcome {
        let mut cmd = Command::new(cargo());
        cmd.current_dir(&self.root)
            .env("CARGO_TARGET_DIR", &self.target)
            .args(["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--"])
            .args(args);
        if require {
            cmd.env("SEMBLANCE_REQUIRE_FIXTURES", "1");
        } else {
            cmd.env_remove("SEMBLANCE_REQUIRE_FIXTURES");
        }
        let out = cmd.output().expect("cargo runs");
        let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        Outcome { passed: out.status.success(), output }
    }

    /// Runs one of semblance-flyvis's own lib tests, by its full path.
    fn one(&self, test: &str, require: bool) -> Outcome {
        let out = self.cargo_test(&["--exact", test, "--nocapture", "--test-threads=1"], require);
        assert!(out.output.contains("running 1 test"), "{test} must run exactly once:\n{}", out.output);
        out
    }

    /// Runs every lib test whose path contains `filter`.
    fn all(&self, filter: &str, require: bool) -> Outcome {
        self.cargo_test(&[filter, "--nocapture", "--test-threads=1"], require)
    }

    /// The lib tests whose path starts with `prefix`.
    fn list(&self, prefix: &str) -> Vec<String> {
        let out = self.cargo_test(&["--list"], false);
        assert!(out.passed, "listing tests:\n{}", out.output);
        out.output
            .lines()
            .filter_map(|l| l.strip_suffix(": test"))
            .filter(|t| t.starts_with(prefix))
            .map(str::to_string)
            .collect()
    }

    /// `text` with this shadow's root replaced by `<ROOT>`.
    fn normalise(&self, text: &str) -> String {
        text.replace(&self.root.display().to_string(), "<ROOT>")
    }
}

fn block_offset(m: &Manifest, name: &str, run: u32) -> usize {
    m.file(name).unwrap().blocks.iter().find(|b| b.run == Some(run)).unwrap().offset
}

fn real_manifest() -> Manifest {
    let text = std::fs::read_to_string(data_path(MANIFEST)).unwrap();
    Manifest::parse(&text).expect("the real manifest parses")
}

/// The text between `before` and the next `until` character.
fn field<'a>(line: &'a str, before: &str, until: char) -> &'a str {
    let at = line.find(before).unwrap_or_else(|| panic!("{before:?} not in {line:?}")) + before.len();
    let rest = &line[at..];
    &rest[..rest.find(until).unwrap_or(rest.len())]
}

/// `d.ddde<exp>`: what `{:.3e}` prints for a finite value.
fn is_sci3(s: &str) -> bool {
    let Some((mantissa, exp)) = s.split_once('e') else { return false };
    let b = mantissa.as_bytes();
    let exp = exp.strip_prefix('-').unwrap_or(exp);
    b.len() == 5
        && b[0].is_ascii_digit()
        && b[1] == b'.'
        && b[2..].iter().all(u8::is_ascii_digit)
        && !exp.is_empty()
        && exp.bytes().all(|c| c.is_ascii_digit())
}

/// The two `<ratio> of bound` fields of a libm line.
fn ratios(line: &str) -> (&str, &str) {
    let after_settled = &line[line.find(" at node ").expect("settled maximum")..];
    let after_centre = &line[line.find(" at step ").expect("centre maximum")..];
    (field(after_settled, "), ", ' '), field(after_centre, "), ", ' '))
}

/// A libm line with its two ratio fields blanked.
fn without_ratios(line: &str) -> String {
    let (s, c) = ratios(line);
    line.replacen(&format!("), {s} of bound"), "), _ of bound", 1).replacen(&format!("), {c} of bound"), "), _ of bound", 1)
}

fn reference_notice(path_tail: &str) -> String {
    format!("{path_tail} not found. Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`.")
}

// ---------------------------------------------------------------------------
// MINORS-01
// ---------------------------------------------------------------------------

#[test]
fn minors_01_a_missing_file_is_one_unprinted_notice_or_under_require_a_panic() {
    if !have(&GATE_FIXTURES) {
        return;
    }
    let _guard = lock();
    let sh = current();
    // manifest.json is read as text, settled.f64.bin as bytes, both by the
    // gates' shared fixture, which hands the reader's Err to `or_skip`.
    for rel in [MANIFEST, SETTLED_F64] {
        sh.data(&[(rel, Alter::Remove)]);
        let tail = format!("data/{rel}");
        let out = sh.one("settling_checks::settling_exact_run_1_000_1_200_f64", false);
        assert!(out.passed, "without {rel} the gate must skip and pass:\n{}", out.output);
        // The reader prints nothing: the notice appears once, from the printer.
        let mentions = out.lines_with(&format!("{tail} not found"));
        assert_eq!(mentions.len(), 1, "{rel}: one notice, printed once:\n{}", out.output);
        assert!(mentions[0].starts_with("skipping: "), "{rel}: only the printer prints it: {}", mentions[0]);
        assert!(mentions[0].ends_with(&reference_notice(&tail)), "{rel}: the notice: {}", mentions[0]);

        let out = sh.one("settling_checks::settling_exact_run_1_000_1_200_f64", true);
        assert!(!out.passed, "{rel} missing under SEMBLANCE_REQUIRE_FIXTURES=1 must fail:\n{}", out.output);
        assert!(
            out.lines_with("SEMBLANCE_REQUIRE_FIXTURES=1 but").iter().any(|l| l.contains(&format!("{tail} is missing"))),
            "{rel}: the panic names the file:\n{}",
            out.output
        );
        assert!(out.lines_with("skipping:").is_empty(), "{rel}: a panic, not a skip:\n{}", out.output);
    }
}

// ---------------------------------------------------------------------------
// MINORS-02
// ---------------------------------------------------------------------------

/// (test, the one fixture it loads through the loader under test)
const LOADER_CASES: [(&str, &str, &str); 8] = [
    ("load_real_spec", "spec::tests::real_spec_matches_the_published_figures", SPEC),
    ("load_build_fixture", "expand_checks::connection_type_ordering_matches_flyvis", "flyvis-build/conn_types.csv"),
    ("load_build_bytes", "expand_checks::every_edge_matches_flyvis", "flyvis-build/edges.bin"),
    ("load_stimulus_fixture", "stimuli_checks::hex_coords_match_flyvis", "flyvis-stimuli/hex_coords.csv"),
    ("load_stimulus_bytes", "stimuli_checks::flashes_match_flyvis", "flyvis-stimuli/flashes.bin"),
    ("load_pretrained_fixture", "ensemble::tests::model_000_equals_the_compiled_in_parameters", ENSEMBLE),
    ("load_reference_fixture", "settling_checks::pre_registered_bounds_match_provenance", PROVENANCE),
    ("load_reference_bytes", "reference::tests::every_real_settled_block_reads_at_its_shape", SETTLED_F32),
];

/// What a loader prints and how the test ends, without and with
/// `SEMBLANCE_REQUIRE_FIXTURES=1`, with the shadow's root normalised.
fn loader_behaviour(sh: &Shadow, test: &str, rel: &str) -> (bool, Vec<String>, bool, Vec<String>) {
    sh.data(&[(rel, Alter::Remove)]);
    let tail = format!("data/{rel}");
    let skip = sh.one(test, false);
    let notices = skip.lines_with(&tail).iter().map(|l| sh.normalise(l)).collect();
    let require = sh.one(test, true);
    let panics = require.lines_with("SEMBLANCE_REQUIRE_FIXTURES=1 but").iter().map(|l| sh.normalise(l)).collect();
    (skip.passed, notices, require.passed, panics)
}

#[test]
fn minors_02_the_loaders_still_print_their_notice_and_behave_as_before() {
    let rels: Vec<&str> = LOADER_CASES.iter().map(|c| c.2).collect();
    if !have(&rels) || !have(&[MANIFEST]) {
        return;
    }
    let _guard = lock();
    let (now, then) = (current(), baseline());
    for (loader, test, rel) in LOADER_CASES {
        let tail = format!("data/{rel}");
        let (passed, notices, require_passed, panics) = loader_behaviour(now, test, rel);
        assert!(passed, "{loader}: {test} without {rel} must skip and pass");
        assert!(
            notices.len() == 1 && notices[0].starts_with("skipping: ") && notices[0].contains(&format!("{tail} not found. ")),
            "{loader}: one printed notice naming {tail}, got {notices:?}"
        );
        assert!(!require_passed, "{loader}: {test} without {rel} under SEMBLANCE_REQUIRE_FIXTURES=1 must fail");
        assert!(panics.iter().any(|l| l.contains(&format!("{tail} is missing"))), "{loader}: the panic names {tail}: {panics:?}");

        let before = loader_behaviour(then, test, rel);
        assert_eq!(
            (passed, &notices, require_passed, &panics),
            (before.0, &before.1, before.2, &before.3),
            "{loader}: {test} must behave as it did at {BASELINE}"
        );
    }
}

// ---------------------------------------------------------------------------
// MINORS-03
// ---------------------------------------------------------------------------

/// Gate tests that read only PROVENANCE.json, not the gate fixture.
const PROVENANCE_ONLY: [&str; 2] =
    ["settling_checks::pre_registered_bounds_match_provenance", "trace_checks::trace_bounds_match_provenance"];

#[test]
fn minors_03_every_gate_test_prints_its_own_skip_notice() {
    if !have(&GATE_FIXTURES) {
        return;
    }
    let _guard = lock();
    let sh = current();
    let mut tests = sh.list("settling_checks::");
    tests.extend(sh.list("trace_checks::"));
    tests.retain(|t| !PROVENANCE_ONLY.contains(&t.as_str()));
    assert!(tests.len() >= 19 + 16, "the settling and trace gates: {tests:?}");
    // The fixture is built once; with the manifest missing it is an Err.
    sh.data(&[(MANIFEST, Alter::Remove)]);
    let want = format!("skipping: {}", sh.root.join("crates/flyvis/../../data").join(MANIFEST).display());
    let mut silent = Vec::new();
    for test in &tests {
        let out = sh.one(test, false);
        assert!(out.passed, "{test} must skip and pass:\n{}", out.output);
        // At least one: a test that checks both precisions asks twice.
        let notices = out.lines_with("skipping: ");
        if notices.is_empty() || !notices.iter().all(|n| n.starts_with(&want)) {
            silent.push(format!("{test}: {notices:?}"));
        }
    }
    assert!(silent.is_empty(), "each gate test must print `skipping: <manifest> not found`:\n{}", silent.join("\n"));
    // and they skip together, one notice each, in a single run as well
    let out = sh.all("settling_checks::", false);
    assert!(out.passed, "{}", out.output);
    let n = tests.iter().filter(|t| t.starts_with("settling_checks::")).count();
    assert_eq!(out.lines_with(&want).len(), n, "one notice per settling gate test in one run, not one per fixture build:\n{}", out.output);
}

// ---------------------------------------------------------------------------
// MINORS-04
// ---------------------------------------------------------------------------

#[test]
fn minors_04_libm_ratios_are_printed_in_scientific_notation() {
    // SETTLING-04 in acceptance_settling.rs asserts the new text.
    let settling = read_source("crates/flyvis/tests/acceptance_settling.rs");
    let s04 = function(&settling, "settling_04_a_libm_gate_prints_maxima_ratios_worst_types_and_bit_equal_count");
    assert!(s04.contains("format!(\"{:.3e}\", ms / bs)"), "SETTLING-04 asserts the settled ratio as {{:.3e}}:\n{s04}");
    assert!(s04.contains("format!(\"{:.3e}\", mc / bc)"), "SETTLING-04 asserts the centre ratio as {{:.3e}}:\n{s04}");
    assert!(!s04.contains("format!(\"{:.3}\", "), "SETTLING-04 no longer asserts {{:.3}}:\n{s04}");

    if !have(&GATE_FIXTURES) {
        return;
    }
    let _guard = lock();
    let sh = current();
    sh.data(&[]);
    let out = sh.all("settling_checks::settling_libm_run_", true);
    assert!(out.passed && out.output.contains("8 passed"), "the eight libm gates pass:\n{}", out.output);
    let lines: Vec<&str> = out.output.lines().filter(|l| l.starts_with("libm run ")).collect();
    assert_eq!(lines.len(), 8, "{}", out.output);
    for line in lines {
        let (s, c) = ratios(line);
        for (what, r) in [("settled", s), ("centre", c)] {
            assert!(is_sci3(r), "{what} ratio {r:?} is not {{:.3e}}: {line}");
            assert_ne!(r.parse::<f64>().unwrap(), 0.0, "{what} ratio reads zero: {line}");
        }
        if line.contains(", f64)") {
            // ratio = max / 1e-10, both printed to four significant digits
            for (max, r) in [(field(line, "settled max ", ' '), s), (field(line, "settling centre max ", ' '), c)] {
                let (max, r): (f64, f64) = (max.parse().unwrap(), r.parse().unwrap());
                let want = max / 1e-10;
                assert!((r - want).abs() <= 1e-3 * want, "ratio {r:e} vs max {max:e} / 1e-10: {line}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MINORS-05
// ---------------------------------------------------------------------------

/// The tools' Python: `SEMBLANCE_TOOLS_PYTHON`, else `tools/.venv` beside
/// this workspace or beside the checkout `data/` really lives in.
fn tools_python() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SEMBLANCE_TOOLS_PYTHON") {
        return Some(PathBuf::from(p));
    }
    let mut roots = vec![PathBuf::from(WORKSPACE)];
    if let Ok(real) = std::fs::canonicalize(Path::new(WORKSPACE).join("data")) {
        roots.push(real.parent().unwrap().to_path_buf());
    }
    roots.into_iter().map(|r| r.join("tools/.venv/bin/python")).find(|p| p.exists())
}

#[test]
fn minors_05_regenerating_syn_count_torch_leaves_it_byte_for_byte_and_undated() {
    const OUT: &str = "crates/flyvis/src/syn_count_torch.rs";
    let committed = read_source(OUT);
    let header: Vec<&str> = committed.lines().take_while(|l| l.starts_with("//!")).collect();
    assert!(header.iter().any(|l| l.contains("GENERATED by `tools/export_syn_count.py`")), "{header:?}");
    for l in &header {
        assert!(!l.contains("exported"), "the header carries no export date: {l}");
        let digits: Vec<bool> = l.bytes().map(|b| b.is_ascii_digit()).collect();
        let dated = (0..digits.len().saturating_sub(9)).any(|i| {
            digits[i..i + 4].iter().all(|&d| d) && &l[i + 4..i + 5] == "-" && digits[i + 5] && digits[i + 6] && &l[i + 7..i + 8] == "-"
        });
        assert!(!dated, "the header carries no date: {l}");
    }
    let script = read_source("tools/export_syn_count.py");
    assert!(!script.contains("datetime") && !script.contains("exported"), "the script writes no date");

    // Only the header changed since the baseline: every value is the same.
    let body = |s: &str| s.lines().skip_while(|l| l.starts_with("//!")).map(str::to_string).collect::<Vec<_>>();
    let before = baseline_source(OUT);
    assert_eq!(body(&committed), body(&before), "no value in {OUT} changed");
    let before_header: Vec<&str> = before.lines().take_while(|l| l.starts_with("//!")).collect();
    assert_eq!(header.len(), before_header.len());

    // Rerun the script on a scratch copy of what it reads and writes.
    let checkpoints = ["000", "049"].map(|m| data_path(&format!("flyvis-pretrained/results/flow/0000/{m}/chkpts/chkpt_00000")));
    let Some(python) = tools_python() else {
        eprintln!("skipping the rerun: no tools/.venv (or SEMBLANCE_TOOLS_PYTHON) with flyvis and torch");
        return;
    };
    if let Some(c) = checkpoints.iter().find(|c| !c.exists()) {
        eprintln!("skipping the rerun: {} not found; run tools/download_pretrained.py", c.display());
        return;
    }
    let root = tmp("export-syn-count");
    remove_tree(&root);
    std::fs::create_dir_all(root.join("tools")).unwrap();
    std::fs::create_dir_all(root.join("crates/flyvis/src")).unwrap();
    for f in ["tools/export_syn_count.py", "tools/export_reference.py", "crates/flyvis/src/pretrained.rs"] {
        std::fs::copy(Path::new(WORKSPACE).join(f), root.join(f)).unwrap();
    }
    link(&std::fs::canonicalize(Path::new(WORKSPACE).join("data")).unwrap(), &root.join("data"));
    let out = Command::new(&python).arg("-B").arg(root.join("tools/export_syn_count.py")).output().expect("python runs");
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() && log.contains("ModuleNotFoundError") {
        eprintln!("skipping the rerun: {} lacks flyvis or torch:\n{log}", python.display());
        return;
    }
    assert!(out.status.success(), "export_syn_count.py must succeed:\n{log}");
    let regenerated = std::fs::read_to_string(root.join(OUT)).expect("the script wrote its output");
    assert!(regenerated == committed, "a rerun must rewrite {OUT} byte for byte:\n{log}");
}

// ---------------------------------------------------------------------------
// MINORS-09
// ---------------------------------------------------------------------------

/// `max_err` from `acceptance_settling.rs`, compiled on its own and run.
fn run_max_err(source: &str) -> String {
    let f = function(source, "max_err");
    let dir = tmp("max-err");
    remove_tree(&dir.join("src"));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"max_err_probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.rs"),
        format!(
            "{f}\nfn main() {{\n\
             let (m, at, differ) = max_err(&[1.0, 2.0, 3.0, 4.0], &[1.0, f64::NAN, 3.5, 4.0]);\n\
             println!(\"{{m:?}} {{at}} {{differ}}\");\n\
             let (m, at, differ) = max_err(&[f64::NAN, 2.0], &[0.0, 2.25]);\n\
             println!(\"{{m:?}} {{at}} {{differ}}\");\n}}\n"
        ),
    )
    .unwrap();
    let out = Command::new(cargo())
        .args(["run", "--quiet", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .expect("cargo runs");
    assert!(out.status.success(), "max_err compiles and runs:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn nan_at(bytes: &mut [u8], at: usize) {
    bytes[at..at + 8].copy_from_slice(&f64::NAN.to_le_bytes());
}

#[test]
fn minors_09_a_nan_reference_value_is_an_infinite_error() {
    // acceptance_settling.rs's max_err
    let got = run_max_err(&read_source("crates/flyvis/tests/acceptance_settling.rs"));
    assert_eq!(got, "inf 1 2\ninf 0 2\n", "max_err treats a NaN error as infinite (max, at, differ)");

    if !have(&GATE_FIXTURES) {
        return;
    }
    let m = real_manifest();
    let (node, step, c) = (7usize, 3usize, 5usize);
    let mut settled = std::fs::read(data_path(SETTLED_F64)).unwrap();
    nan_at(&mut settled, block_offset(&m, "settled.f64.bin", 1) + 8 * node);
    let mut centre = std::fs::read(data_path(CENTRE_F64)).unwrap();
    nan_at(&mut centre, block_offset(&m, "settling_centre.f64.bin", 1) + 8 * (step * 65 + c));

    let _guard = lock();
    let sh = current();
    // A NaN in the settled reference alone: the libm gate must fail on it.
    // Skipped by `>`, it would pass, as every other value is within bound.
    sh.data(&[(SETTLED_F64, Alter::Replace(settled.clone()))]);
    let out = sh.one("settling_checks::settling_libm_run_1_000_1_200_f64", true);
    assert!(!out.passed, "a NaN in the reference must fail the libm gate:\n{}", out.output);
    assert!(out.output.contains(&format!("settled max inf at node {node} (")), "maximum is infinite, at the NaN:\n{}", out.output);
    assert!(out.output.contains("settled inf (bound 1e-10)"), "the failure reports an infinite maximum:\n{}", out.output);

    // In both quantities, and in the exact gate.
    sh.data(&[(SETTLED_F64, Alter::Replace(settled)), (CENTRE_F64, Alter::Replace(centre))]);
    let out = sh.one("settling_checks::settling_libm_run_1_000_1_200_f64", true);
    assert!(!out.passed, "{}", out.output);
    assert!(out.output.contains(&format!("settling centre max inf at step {step} (")), "{}", out.output);
    assert!(out.output.contains("settling centre inf (bound 1e-10)"), "{}", out.output);
    let out = sh.one("settling_checks::settling_exact_run_1_000_1_200_f64", true);
    assert!(!out.passed, "{}", out.output);
    assert!(out.output.contains(&format!("largest settled error inf at node {node} (")), "{}", out.output);
}

// ---------------------------------------------------------------------------
// MINORS-12
// ---------------------------------------------------------------------------

#[test]
fn minors_12_the_integer_count_control_is_documented_against_the_f64_bound() {
    const CHECKS: &str = "crates/flyvis/src/settling_checks.rs";
    const NAME: &str = "the_gate_resolves_integer_counts";
    let now = read_source(CHECKS);
    let doc = doc_comment(&now, NAME);
    assert!(doc.contains("against the f64 bound"), "the doc comment says it resolves against the f64 bound: {doc}");
    assert!(doc.contains("in f32 the exact gate is the guard"), "the doc comment names the f32 guard: {doc}");
    // no assertion changed: the function's body is as it was
    assert_eq!(function(&now, NAME), function(&baseline_source(CHECKS), NAME), "{NAME}'s code must not change");

    let spec = read_source("docs/superpowers/specs/2026-10-28-graded-settling-design.md");
    let at = spec.find("## Amendment (2026-10-29): what the integer-count control resolves").expect("piece 1's spec is amended");
    let amendment = spec[at..].split("\n## ").next().unwrap().split_whitespace().collect::<Vec<_>>().join(" ");
    for want in ["No assertion changes.", NAME, "holds for the f64 bound", "does not hold for the f32 bound", "In f32, the exact gate is what guards against integer counts."] {
        assert!(amendment.contains(want), "the amendment says {want:?}:\n{amendment}");
    }
}

// ---------------------------------------------------------------------------
// MINORS-13
// ---------------------------------------------------------------------------

#[test]
fn minors_13_acceptance_settling_clears_its_shadow_sources_and_keeps_its_target() {
    if !have(&GATE_FIXTURES) {
        return;
    }
    let nested = tmp("nested-settling");
    // acceptance_settling.rs's shadow lives under CARGO_TARGET_TMPDIR, `<target>/tmp`.
    let base = nested.join("tmp").join("acceptance-settling");
    let stale = base.join("workspace/crates/flyvis/src/stale_left_by_an_earlier_run.rs");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    std::fs::write(&stale, "compile_error!(\"stale\");\n").unwrap();
    let marker = base.join("target").join("kept-by-minors-13");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "keep").unwrap();

    let out = Command::new(cargo())
        .current_dir(WORKSPACE)
        .env("CARGO_TARGET_DIR", &nested)
        .env("SEMBLANCE_REQUIRE_FIXTURES", "1")
        .args(["test", "-p", "semblance-flyvis", "--test", "acceptance_settling", "--locked", "--offline", "--"])
        .args(["--exact", "settling_07_the_manifest_lists_the_eight_runs_and_the_networks_centre_types", "--nocapture"])
        .output()
        .expect("cargo runs");
    let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(output.contains("running 1 test"), "SETTLING-07 must run exactly once:\n{output}");
    assert!(out.status.success(), "SETTLING-07 must pass:\n{output}");
    assert!(!stale.exists(), "{} survived: the copied source was not removed before copying", stale.display());
    assert!(base.join("workspace/crates/flyvis/src/lib.rs").exists(), "the sources were copied afresh");
    assert!(marker.exists(), "{} was removed: the sibling target dir must be kept", marker.display());
}

// ---------------------------------------------------------------------------
// MINORS-15
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

/// The `exact run` and `libm run` lines, libm ratios blanked (their format
/// is MINORS-04's change).
fn settling_results(out: &Outcome) -> Vec<String> {
    out.output
        .lines()
        .filter(|l| l.starts_with("exact run ") || l.starts_with("libm run "))
        .map(|l| if l.starts_with("libm") { without_ratios(l) } else { l.to_string() })
        .collect()
}

/// What a module's tests print, without libtest's own lines.
fn printed(out: &Outcome) -> Vec<&str> {
    out.output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !["test ", "running ", "ok", "   Compiling", "    Finished", "     Running"].iter().any(|p| l.starts_with(p)))
        .collect()
}

#[test]
fn minors_15_settling_and_stimulus_gate_results_are_unchanged() {
    if !have(&GATE_FIXTURES) || !have(&["flyvis-stimuli/flashes.bin", "flyvis-stimuli/moving_edge_offsets.bin"]) {
        return;
    }
    let _guard = lock();
    let (now, then) = (current(), baseline());
    now.data(&[]);
    then.data(&[]);

    // The settling gates: every result as it was before the fixes.
    let a = now.all("settling_checks::", true);
    assert!(a.passed && a.output.contains("test result: ok. 20 passed; 0 failed"), "the settling gates pass:\n{}", a.output);
    let b = then.all("settling_checks::", true);
    assert!(b.passed, "the baseline's settling gates pass:\n{}", b.output);
    let (ra, rb) = (settling_results(&a), settling_results(&b));
    assert_eq!(ra.len(), 16, "eight exact and eight libm results:\n{}", a.output);
    for l in ra.iter().filter(|l| l.starts_with("exact")) {
        assert!(field(l, "): settled ", ' ') == "0" && field(l, "; settling centre ", ' ') == "0", "bit-equal: {l}");
    }
    assert_eq!(ra, rb, "the settling gates report what they did at {BASELINE}");

    // The stimulus checks that predate the epic: same verdict, same output.
    for module in ["stimuli_checks::", "moving_edge_checks::"] {
        let (x, y) = (now.all(module, true), then.all(module, true));
        assert!(x.passed && y.passed, "{module} passes now and at {BASELINE}:\n{}\n{}", x.output, y.output);
        assert_eq!(printed(&x), printed(&y), "{module} prints what it did at {BASELINE}");
    }

    // The trace gates (with the stimulus checks beside them) are new in this
    // epic, so they have no earlier result: with the fixes they must pass,
    // and every exact trace gate must find nothing differing.
    let t = now.all("trace_checks::", true);
    assert!(t.passed && t.output.contains("test result: ok."), "the trace gates pass:\n{}", t.output);
    let exact: Vec<&str> = t.output.lines().filter(|l| l.starts_with("trace exact run ")).collect();
    assert_eq!(exact.len(), 8, "the eight exact trace gates print their results:\n{}", t.output);
    for l in exact {
        let (equal, total) = field(l, "): ", ' ').parse::<usize>().ok().zip(field(l, " of ", ' ').parse::<usize>().ok()).expect("counts");
        assert!(equal == total && total > 0, "every sequence bit-equal: {l}");
    }
    assert_eq!(t.output.lines().filter(|l| l.starts_with("trace libm run ")).count(), 8, "{}", t.output);
}
