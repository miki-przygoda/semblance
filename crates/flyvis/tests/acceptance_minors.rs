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
//! No test reads git history: a CI checkout is shallow, so the commit the
//! epic started from is not there. Where a behaviour says "unchanged"
//! (MINORS-02, -05, -12, -15), these tests check what the current tree does
//! or contains; whether it matches the earlier commit is a review-time check.
//!
//! No test runs Python or anything under `tools/` (CLAUDE.md: no crate
//! invokes `tools/`). MINORS-05 checks only the tree: the committed
//! `syn_count_torch.rs` and its exporter carry no date. Rerunning
//! `tools/export_syn_count.py` and diffing its output is an owner step, in
//! `docs/superpowers/temper/stimulus-traces/owner-steps.sh`.
//!
//! Everything else that needs the real fixtures skips without them, unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip into a failure.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

use semblance_flyvis::reference::Manifest;

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
    let now = current();
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

/// Every `.rs` file under `crates/`, skipping build output.
fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                rust_sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn minors_05_syn_count_torch_is_undated_and_no_rust_source_runs_its_exporter() {
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

    // CLAUDE.md: no crate invokes `tools/`, so the byte-for-byte rerun is an
    // owner step (docs/superpowers/temper/stimulus-traces/owner-steps.sh),
    // and no Rust source, tests included, starts a process other than cargo
    // or looks for the tools' Python.
    let mut files = Vec::new();
    rust_sources(&Path::new(WORKSPACE).join("crates"), &mut files);
    assert!(!files.is_empty());
    // Built from pieces so this test does not match itself.
    let venv = [".venv", "/bin/"].concat();
    let env_var = ["SEMBLANCE_", "TOOLS_PYTHON"].concat();
    let spawn = ["Command", "::new("].concat();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        assert!(!text.contains(&venv), "{} looks for a Python virtualenv", path.display());
        assert!(!text.contains(&env_var), "{} reads {env_var}", path.display());
        for (i, line) in text.lines().enumerate() {
            if let Some(at) = line.find(&spawn) {
                let arg = &line[at + spawn.len()..];
                assert!(
                    arg.starts_with("cargo") || arg.starts_with("option_env!(\"CARGO\")"),
                    "{}:{} starts a process other than cargo: {line}",
                    path.display(),
                    i + 1
                );
            }
        }
    }
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

/// The `exact run` and `libm run` lines.
fn settling_results(out: &Outcome) -> Vec<&str> {
    out.output.lines().filter(|l| l.starts_with("exact run ") || l.starts_with("libm run ")).collect()
}

#[test]
fn minors_15_settling_and_stimulus_gate_results_are_unchanged() {
    if !have(&GATE_FIXTURES) || !have(&["flyvis-stimuli/flashes.bin", "flyvis-stimuli/moving_edge_offsets.bin"]) {
        return;
    }
    let _guard = lock();
    let now = current();
    now.data(&[]);

    // The settling gates pass, and the exact gate finds every value bit-equal.
    let a = now.all("settling_checks::", true);
    assert!(a.passed && a.output.contains("test result: ok. 20 passed; 0 failed"), "the settling gates pass:\n{}", a.output);
    let ra = settling_results(&a);
    assert_eq!(ra.len(), 16, "eight exact and eight libm results:\n{}", a.output);
    for l in ra.iter().filter(|l| l.starts_with("exact")) {
        assert!(field(l, "): settled ", ' ') == "0" && field(l, "; settling centre ", ' ') == "0", "bit-equal: {l}");
    }
    assert_eq!(ra.iter().filter(|l| l.starts_with("libm")).count(), 8, "eight libm results:\n{}", a.output);

    // The stimulus checks that predate the epic still pass.
    for module in ["stimuli_checks::", "moving_edge_checks::"] {
        let x = now.all(module, true);
        assert!(x.passed && x.output.contains("test result: ok."), "{module} passes:\n{}", x.output);
        assert!(!x.output.contains("running 0 tests"), "{module} ran no tests:\n{}", x.output);
    }

    // The trace gates are not re-run here (spec P1, docs/superpowers/specs/
    // 2026-10-29-flyvis-parity-validate-design.md, section 8): all 22 run
    // directly in the same suite under SEMBLANCE_REQUIRE_FIXTURES=1, TRACES-06
    // requires every exact line in its `12 of 12` form, and the parity record
    // test compares every trace number `semblance validate` prints.
}
