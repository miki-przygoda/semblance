//! Acceptance tests for behaviour group `indices` of epic fidelity-measures,
//! the part `semblance validate` shows: gate A, its control, and
//! `--ensemble`. One test per behaviour id, INDICES-16 .. INDICES-19;
//! INDICES-01 .. INDICES-15 are in `crates/fidelity/tests/acceptance_indices.rs`.
//!
//! Every test drives the built `semblance` binary: it spawns it with a data
//! directory and reads its stdout, stderr and exit code. Data directories
//! are made under `CARGO_TARGET_TMPDIR` from symlinks to the workspace's
//! real fixtures; the real `data/` is never written.
//!
//! The row format of gate A is not fixed by the spec, so the tests read it
//! loosely: a gate A row is a line naming `gate A` (or `index`/`indices`),
//! and must name its model and precision and print a `{:.3e}` number.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use semblance_flyvis::parity::required_files;

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
const INDICES_DIR: &str = "flyvis-indices";

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn all(&self) -> String {
        format!("exit {:?}\n--- stdout\n{}--- stderr\n{}", self.code, self.stdout, self.stderr)
    }
}

fn validate_on(data: &Path, extra: &[&str]) -> Run {
    let o = Command::new(env!("CARGO_BIN_EXE_semblance"))
        .arg("validate")
        .arg("--data")
        .arg(data)
        .args(extra)
        .output()
        .expect("the binary runs");
    Run {
        code: o.status.code(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn real_data() -> PathBuf {
    Path::new(WORKSPACE).join("data")
}

/// Whether 0.3.0's fixtures are present; if not, a skip notice (false) or,
/// under `SEMBLANCE_REQUIRE_FIXTURES=1`, a failure.
fn fixtures_or_skip(test: &str) -> bool {
    let missing: Vec<&str> =
        required_files().iter().copied().filter(|rel| fs::metadata(real_data().join(rel)).is_err()).collect();
    if missing.is_empty() {
        return true;
    }
    if std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1") {
        panic!("{test}: SEMBLANCE_REQUIRE_FIXTURES=1 but fixtures are missing: {missing:?}");
    }
    eprintln!("skipping: {test}: fixtures missing under {}: {missing:?}", real_data().display());
    false
}

/// A fresh, empty directory under the target's temp dir.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("acceptance-indices-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A data directory linking every top-level entry of the real `data/`
/// except `flyvis-indices/`.
#[cfg(unix)]
fn without_indices(name: &str) -> PathBuf {
    let dir = fresh(name);
    for e in fs::read_dir(real_data()).unwrap() {
        let e = e.unwrap();
        if e.file_name() != INDICES_DIR {
            std::os::unix::fs::symlink(e.path(), dir.join(e.file_name())).unwrap();
        }
    }
    dir
}

/// Whether `data/flyvis-indices/` (fixtures-3), which gate A reads, is
/// present; if not, a skip notice (false) or, under
/// `SEMBLANCE_REQUIRE_FIXTURES=1`, a failure.
fn indices_or_skip(test: &str) -> bool {
    if real_data().join(INDICES_DIR).is_dir() {
        return true;
    }
    if std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1") {
        panic!("{test}: SEMBLANCE_REQUIRE_FIXTURES=1 but data/{INDICES_DIR}/ is missing");
    }
    eprintln!("skipping: {test}: data/{INDICES_DIR}/ missing under {}", real_data().display());
    false
}

/// `validate` on the real data, once per test binary; gate A needs
/// `flyvis-indices/` as well as 0.3.0's fixtures.
fn full() -> Option<&'static Run> {
    static FULL: OnceLock<Option<Run>> = OnceLock::new();
    FULL.get_or_init(|| {
        (fixtures_or_skip("validate with gate A") && indices_or_skip("validate with gate A"))
            .then(|| validate_on(&real_data(), &[]))
    })
    .as_ref()
}

/// Gate A's lines: those naming gate A or the indices.
fn gate_a(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|l| {
            let l = l.to_lowercase();
            l.contains("gate a") || l.contains("indices") || l.contains("index")
        })
        .collect()
}

/// Whether some whitespace-separated field is what `{:.3e}` prints.
fn has_3e(line: &str) -> bool {
    line.split_whitespace().any(|s| s.parse::<f64>().is_ok_and(|v| format!("{v:.3e}") == s))
}

/// INDICES-16: `validate` runs gate A for models 000 and 049 at f32 and
/// f64 over the index battery against `IndexFixture`, and every row holds
/// and prints its worst difference in `{:.3e}`.
#[test]
#[ignore = "acceptance-failing: INDICES-16"]
fn indices_16_gate_a_compares_models_000_and_049_at_both_precisions() {
    let Some(r) = full() else { return };
    assert_eq!(r.code, Some(0), "{}", r.all());
    let rows = gate_a(&r.stdout);
    for model in ["000", "049"] {
        for prec in ["f32", "f64"] {
            let mine: Vec<&&str> = rows.iter().filter(|l| l.contains(model) && l.contains(prec)).collect();
            assert!(!mine.is_empty(), "gate A rows for model {model} at {prec}:\n{}", r.stdout);
            assert!(mine.iter().all(|l| has_3e(l)), "every gate A row prints its worst difference in {{:.3e}}: {mine:?}");
        }
    }
    let text = rows.join("\n").to_lowercase();
    for index in ["fri", "edge", "dsi", "tuning"] {
        assert!(text.contains(index), "gate A covers {index}:\n{text}");
    }
    assert!(text.contains("preferred") || text.contains("pd"), "gate A covers the preferred direction:\n{text}");
}

/// INDICES-17: gate A's control (model 049's responses against model 000's
/// fixture) is listed among the controls and holds: the FRI and the edge
/// peaks miss by more than `CONTROL_MISS`.
#[test]
#[ignore = "acceptance-failing: INDICES-17"]
fn indices_17_gate_a_control_scores_049_against_000_and_misses() {
    let Some(r) = full() else { return };
    let lines: Vec<&str> = r.stdout.lines().collect();
    let start = lines.iter().position(|l| l.starts_with("controls:")).expect("a controls section");
    let control: Vec<&&str> = lines[start..]
        .iter()
        .take_while(|l| !l.starts_with("coverage:"))
        .filter(|l| {
            let low = l.to_lowercase();
            (low.contains("gate a") || low.contains("ind")) && l.contains("049") && l.contains("000")
        })
        .collect();
    assert!(!control.is_empty(), "a gate A control scoring 049 against 000:\n{}", r.stdout);
    assert!(control.iter().all(|l| !l.contains("FAIL")), "the control holds (it misses as it must): {control:?}");
    assert_eq!(r.code, Some(0), "{}", r.all());
}

/// INDICES-18: without `--ensemble`, a data directory lacking
/// `flyvis-indices/` is a fixture missing: exit 2, naming it; and with it,
/// the table gains gate A rows with per-model wall time beside 0.3.0's 32
/// rows.
#[cfg(unix)]
#[test]
#[ignore = "acceptance-failing: INDICES-18"]
fn indices_18_validate_needs_flyvis_indices_and_adds_gate_a_rows() {
    if !fixtures_or_skip("validate without flyvis-indices") {
        return;
    }
    let dir = without_indices("18");
    let r = validate_on(&dir, &["--threads", "4"]);
    assert_eq!(r.code, Some(2), "a missing flyvis-indices/ is exit 2:\n{}", r.all());
    assert!(r.stderr.contains(INDICES_DIR), "the missing directory is named:\n{}", r.all());

    let Some(full) = full() else { return };
    let old = full.stdout.lines().filter(|l| l.starts_with("settling ") || l.starts_with("traces ")).count();
    assert_eq!(old, 32, "0.3.0's rows are still there:\n{}", full.stdout);
    assert!(!gate_a(&full.stdout).is_empty(), "gate A rows are added:\n{}", full.stdout);
    for model in ["000", "049"] {
        assert!(
            full.stdout.lines().any(|l| l.contains(model) && (l.contains(" s") || l.contains("s)") || l.contains("wall"))),
            "per-model wall time for {model}:\n{}",
            full.stdout
        );
    }
}

/// INDICES-19: `validate --ensemble` is accepted, and adds gate A over all
/// 50 models at f32 and gate B; plain CI never passes it.
#[test]
#[ignore = "acceptance-failing: INDICES-19"]
fn indices_19_validate_ensemble_adds_all_50_models_and_gate_b() {
    let ci = fs::read_to_string(Path::new(WORKSPACE).join(".github/workflows/ci.yml")).expect("ci.yml");
    let plain_validate: Vec<&str> = ci.lines().filter(|l| l.contains("semblance validate")).collect();
    assert!(!plain_validate.is_empty(), "CI runs validate");
    for l in &plain_validate {
        assert!(!l.contains("--ensemble"), "plain CI does not pass --ensemble: {l}");
    }

    // Accepted on the command line: an empty data directory is missing
    // fixtures (exit 2), not a bad command line.
    let empty = fresh("19-empty");
    let r = validate_on(&empty, &["--ensemble"]);
    assert!(!r.stderr.contains("unknown argument") && !r.stderr.contains("usage:"), "--ensemble is accepted:\n{}", r.all());
    assert_eq!(r.code, Some(2), "{}", r.all());

    if !real_data().join(INDICES_DIR).is_dir() || !fixtures_or_skip("validate --ensemble") {
        assert!(
            !std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1"),
            "SEMBLANCE_REQUIRE_FIXTURES=1 but data/{INDICES_DIR}/ is missing"
        );
        eprintln!("skipping: validate --ensemble on the real fixtures: data/{INDICES_DIR}/ missing");
        return;
    }
    let r = validate_on(&real_data(), &["--ensemble"]);
    let rows = gate_a(&r.stdout);
    for m in 0..50 {
        let model = format!("{m:03}");
        assert!(rows.iter().any(|l| l.contains(&model) && l.contains("f32")), "gate A for model {model} at f32:\n{}", r.stdout);
    }
    assert!(r.stdout.to_lowercase().contains("gate b"), "gate B runs:\n{}", r.stdout);
}

/// A fixture-gated test here skips without `data/flyvis-indices/`, as every
/// fixture-gated test does: none asserts a fixture is present ahead of its
/// skip helper, which would make it fail, not skip, in a checkout without
/// the data.
#[test]
fn no_test_asserts_a_fixture_is_present_instead_of_skipping() {
    let pat = concat!("assert!(real_data()", ".join(");
    let hits: Vec<&str> = include_str!("acceptance_indices.rs").lines().filter(|l| l.contains(pat)).collect();
    assert!(hits.is_empty(), "fixture presence asserted instead of skipped: {hits:?}");
}
