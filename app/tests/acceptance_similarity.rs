//! Acceptance tests for behaviour group `similarity` of epic
//! fidelity-measures, the part `semblance validate --ensemble` shows: gates
//! C and D and Zhou & Hasler's reported variant. One test per behaviour id,
//! SIMILARITY-11 .. SIMILARITY-13; SIMILARITY-01 .. SIMILARITY-10 are in
//! `crates/fidelity/tests/acceptance_similarity.rs`.
//!
//! Every test drives the built `semblance` binary on the real `data/`
//! directory and reads its stdout, stderr and exit code. Gates C and D read
//! 0.3.0's fixtures (fixtures-2) only.
//!
//! The format of the new lines is not fixed by the spec, so the tests read
//! it loosely: a gate's lines are those naming it (or, after a heading
//! naming it, any line up to the next `gate`/`controls:` heading).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use semblance_flyvis::parity::required_files;

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

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

fn real_data() -> PathBuf {
    Path::new(WORKSPACE).join("data")
}

/// Whether every fixture gates C and D read is present; if not, a skip
/// notice (false) or, under `SEMBLANCE_REQUIRE_FIXTURES=1`, a failure.
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

fn validate(extra: &[&str]) -> Run {
    let o = Command::new(env!("CARGO_BIN_EXE_semblance"))
        .arg("validate")
        .arg("--data")
        .arg(real_data())
        .args(extra)
        .output()
        .expect("the binary runs");
    Run {
        code: o.status.code(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// `validate --ensemble` on the real data, once per test binary.
fn ensemble(test: &str) -> Option<&'static Run> {
    static RUN: OnceLock<Option<Run>> = OnceLock::new();
    if !fixtures_or_skip(test) {
        return None;
    }
    RUN.get_or_init(|| Some(validate(&["--ensemble"]))).as_ref()
}

/// The lines of the gate (or section) named `name`: those naming it, and
/// those in a section headed by one.
fn section<'a>(stdout: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut inside = false;
    for l in stdout.lines() {
        let low = l.to_lowercase();
        if low.contains(name) {
            inside = true;
            out.push(l);
            continue;
        }
        if inside && (low.starts_with("gate ") || low.starts_with("controls:") || low.starts_with("coverage:")) {
            inside = false;
        }
        if inside {
            out.push(l);
        }
    }
    out
}

/// The lines mentioning every one of `words` (case-insensitive).
fn lines_with<'a>(rows: &[&'a str], words: &[&str]) -> Vec<&'a str> {
    rows.iter().copied().filter(|l| words.iter().all(|w| l.to_lowercase().contains(w))).collect()
}

/// The numbers on a line, as written.
fn numbers(line: &str) -> Vec<f64> {
    line.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e'))
        .filter_map(|w| w.trim_matches(|c| c == '.' || c == '-' || c == 'e').parse::<f64>().ok())
        .collect()
}

/// SIMILARITY-11: gate D: `cka(m, m) == 1.0` exactly for all 50 trained
/// models at f32; the 1,225 pairwise scores print as min, 5th percentile,
/// median and max, and the minimum is gate C's line.
#[test]
#[ignore = "acceptance-failing: SIMILARITY-11"]
fn similarity_11_gate_d_holds_for_all_50_and_the_1225_pairwise_scores_print() {
    let Some(r) = ensemble("similarity_11") else { return };
    assert_ne!(r.code, Some(2), "validate --ensemble verified something:\n{}", r.all());

    let d = section(&r.stdout, "gate d");
    assert!(!d.is_empty(), "gate D runs under --ensemble:\n{}", r.all());
    let text = d.join("\n");
    assert!(
        text.contains("50/50") || text.contains("50 of 50") || text.contains("all 50"),
        "gate D covers all 50 models: {d:?}"
    );
    assert!(text.contains("1.0") || text.contains("== 1"), "gate D states cka(m, m) = 1.0: {d:?}");
    assert!(!d.iter().any(|l| l.contains("FAIL")), "cka(m, m) is exactly 1.0 for every model: {d:?}");

    let pairwise: Vec<&str> =
        r.stdout.lines().filter(|l| l.contains("1225") || l.contains("1,225")).collect();
    assert!(!pairwise.is_empty(), "the 1,225 pairwise scores are reported:\n{}", r.all());
    let around = r.stdout.to_lowercase();
    for word in ["min", "median", "max"] {
        assert!(around.contains(word), "the pairwise scores print a {word}:\n{}", r.all());
    }
    assert!(
        around.contains("5th") || around.contains("p5") || around.contains("5%") || around.contains("percentile"),
        "the pairwise scores print a 5th percentile:\n{}",
        r.all()
    );
    let c = section(&r.stdout, "gate c");
    assert!(!c.is_empty(), "gate C runs under --ensemble:\n{}", r.all());
    assert!(
        lines_with(&c, &["min"]).iter().any(|l| numbers(l).iter().any(|&v| v > 0.0 && v <= 1.0)),
        "gate C's line is the trained models' minimum pairwise CKA: {c:?}"
    );

    // Kept for the record: the markdown carries the same statistics.
    let md = validate(&["--ensemble", "--markdown"]);
    assert_ne!(md.code, Some(2), "{}", md.all());
    assert!(md.stdout.contains("1225") || md.stdout.contains("1,225"), "the record keeps the pairwise scores:\n{}", md.all());
}

/// SIMILARITY-12: Zhou & Hasler's variant over the 50 trained models and
/// the 200 control draws: pairwise RDM Spearman mean ± sd beside their
/// 0.721 ± 0.150 and 0.838 ± 0.059, trained-vs-control beside 0.686 and
/// 0.846, not gated, saying Rust's generator was used.
#[test]
#[ignore = "acceptance-failing: SIMILARITY-12"]
fn similarity_12_zhou_and_haslers_variant_prints_beside_the_papers_numbers() {
    let Some(r) = ensemble("similarity_12") else { return };
    assert_ne!(r.code, Some(2), "validate --ensemble verified something:\n{}", r.all());
    let lines: Vec<&str> = r
        .stdout
        .lines()
        .filter(|l| {
            let low = l.to_lowercase();
            low.contains("zhou") || low.contains("z&h") || low.contains("spearman") || low.contains("rdm")
        })
        .collect();
    assert!(!lines.is_empty(), "Zhou & Hasler's variant is reported:\n{}", r.all());
    let text = r.stdout.clone();
    for paper in ["0.721", "0.150", "0.838", "0.059", "0.686", "0.846"] {
        assert!(text.contains(paper), "the paper's {paper} is printed beside ours:\n{}", r.all());
    }
    assert!(
        lines_with(&lines, &["spearman"]).iter().any(|l| l.contains('±') || l.contains("+/-") || l.contains("sd")),
        "the pairwise RDM Spearman is printed as mean ± sd: {lines:?}"
    );
    let low = text.to_lowercase();
    assert!(low.contains("control"), "trained-vs-control values are printed:\n{}", r.all());
    assert!(
        low.contains("movingedge") || low.contains("moving edge") || low.contains("rust's generator") || low.contains("rust generator"),
        "the output says Rust's moving-edge generator was used:\n{}",
        r.all()
    );
    let paper_lines: Vec<&str> = text
        .lines()
        .filter(|l| ["0.721", "0.838", "0.686", "0.846"].iter().any(|p| l.contains(p)))
        .collect();
    assert!(
        paper_lines.iter().all(|l| !l.contains("FAIL") && !l.contains("PASS")),
        "Zhou & Hasler's comparison is reported, not gated: {paper_lines:?}"
    );
    assert!(!low.contains("p-value") && !low.contains("permutation"), "no Nili permutation test or p-value is reproduced");
}

/// SIMILARITY-13: gates C and D under `validate --ensemble` print per-model
/// and total times and use the same exit codes; plain `validate`, which is
/// what CI runs, does not run them.
#[test]
#[ignore = "acceptance-failing: SIMILARITY-13"]
fn similarity_13_ensemble_gates_print_their_times_and_plain_validate_skips_them() {
    let Some(r) = ensemble("similarity_13") else { return };
    assert!(matches!(r.code, Some(0) | Some(1)), "the same exit codes: 0 or 1 once verified:\n{}", r.all());
    assert!(!r.stderr.contains("panicked"), "{}", r.all());
    let verdict_fail = r.stdout.lines().any(|l| l.contains("FAIL"));
    assert_eq!(r.code == Some(1), verdict_fail, "exit 1 exactly when something failed:\n{}", r.all());

    // Per-model times: a time on a line for each of the 50 models.
    let timed = |l: &str| {
        l.split_whitespace().any(|w| {
            let w = w.trim_end_matches([',', ')', ';']);
            (w.ends_with("ms") && w[..w.len() - 2].parse::<f64>().is_ok())
                || (w.ends_with('s') && w[..w.len() - 1].parse::<f64>().is_ok())
        })
    };
    for m in 0..50 {
        let id = format!("{m:03}");
        assert!(
            r.stdout.lines().any(|l| l.contains(&id) && timed(l)),
            "model {id}'s time is printed:\n{}",
            r.all()
        );
    }
    assert!(
        r.stdout.lines().any(|l| l.to_lowercase().contains("total") && timed(l)),
        "the total time is printed:\n{}",
        r.all()
    );

    // Plain validate (CI's command) does not run gates C or D.
    let plain = validate(&[]);
    let low = plain.stdout.to_lowercase();
    assert!(!low.contains("gate c") && !low.contains("gate d"), "plain validate runs neither gate:\n{}", plain.all());
    let ci = fs::read_to_string(Path::new(WORKSPACE).join(".github/workflows/ci.yml")).expect("ci.yml");
    assert!(!ci.contains("--ensemble"), "CI does not run validate --ensemble");
}
