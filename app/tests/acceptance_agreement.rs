//! Acceptance tests for behaviour group `agreement` of epic fidelity-measures,
//! the part `semblance validate --ensemble` shows: gate B, flyvis's ensemble
//! under the paper's rules. One test per behaviour id, AGREEMENT-07 ..
//! AGREEMENT-13; AGREEMENT-01 .. AGREEMENT-06 are in
//! `crates/fidelity/tests/acceptance_agreement.rs`.
//!
//! Every test drives the built `semblance` binary on the real `data/`
//! directory, with `--ensemble`, and reads its stdout, stderr and exit
//! code. Gate B needs `data/flyvis-indices/` (fixtures-3) as well as
//! 0.3.0's fixtures.
//!
//! The format of gate B's lines is not fixed by the spec, so the tests read
//! it loosely: a gate B line is one naming `gate B` (or, after a `gate B`
//! heading, any line up to the next `gate`/`controls:` heading), and counts
//! are read as `N/M` or `N of M`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use semblance_fidelity::indices::{BINOMIAL_ALPHA, BINOMIAL_P0, D_THRESH, PD_TOLERANCE_DEG};
use semblance_flyvis::groundtruth::{KNOWN_DSI_TYPES, PREFERRED_DIRECTIONS};
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

fn real_data() -> PathBuf {
    Path::new(WORKSPACE).join("data")
}

/// Whether every fixture gate B reads is present; if not, a skip notice
/// (false) or, under `SEMBLANCE_REQUIRE_FIXTURES=1`, a failure.
fn fixtures_or_skip(test: &str) -> bool {
    let mut missing: Vec<String> = required_files()
        .iter()
        .copied()
        .filter(|rel| fs::metadata(real_data().join(rel)).is_err())
        .map(str::to_string)
        .collect();
    if !real_data().join(INDICES_DIR).is_dir() {
        missing.push(format!("{INDICES_DIR}/"));
    }
    if missing.is_empty() {
        return true;
    }
    if std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1") {
        panic!("{test}: SEMBLANCE_REQUIRE_FIXTURES=1 but fixtures are missing: {missing:?}");
    }
    eprintln!("skipping: {test}: fixtures missing under {}: {missing:?}", real_data().display());
    false
}

/// `validate --ensemble` on the real data, once per test binary.
fn ensemble() -> Option<&'static Run> {
    static RUN: OnceLock<Option<Run>> = OnceLock::new();
    RUN.get_or_init(|| {
        fixtures_or_skip("validate --ensemble").then(|| {
            let o = Command::new(env!("CARGO_BIN_EXE_semblance"))
                .arg("validate")
                .arg("--data")
                .arg(real_data())
                .arg("--ensemble")
                .output()
                .expect("the binary runs");
            Run {
                code: o.status.code(),
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            }
        })
    })
    .as_ref()
}

/// Gate B's lines: those naming gate B, and those in a section headed by
/// one.
fn gate_b(stdout: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut inside = false;
    for l in stdout.lines() {
        let low = l.to_lowercase();
        if low.contains("gate b") {
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

/// Whether `line` states the count `n` out of `of`.
fn states(line: &str, n: usize, of: usize) -> bool {
    line.contains(&format!("{n}/{of}")) || line.contains(&format!("{n} of {of}"))
}

/// Whether a line reports a failure.
fn fails(line: &str) -> bool {
    line.contains("FAIL")
}

/// The gate B lines that mention every one of `words` (case-insensitive).
fn lines_with<'a>(rows: &[&'a str], words: &[&str]) -> Vec<&'a str> {
    rows.iter().copied().filter(|l| words.iter().all(|w| l.to_lowercase().contains(w))).collect()
}

/// AGREEMENT-07: model 000 at f32, Rust's indices, the paper's polarity
/// rule: exactly 30 of the 32 polarised types agree.
#[test]
#[ignore = "acceptance-failing: AGREEMENT-07"]
fn agreement_07_model_000_agrees_on_30_of_32_polarities() {
    assert!(real_data().join(INDICES_DIR).is_dir(), "gate B reads data/{INDICES_DIR}/ (fixtures-3)");
    let Some(r) = ensemble() else { return };
    let rows = gate_b(&r.stdout);
    let pol = lines_with(&rows, &["polarity", "000"]);
    assert!(!pol.is_empty(), "a gate B polarity line for model 000:\n{}", r.all());
    assert!(pol.iter().any(|l| states(l, 30, 32) && !fails(l)), "model 000 agrees on exactly 30 of 32: {pol:?}");
    assert!(!pol.iter().any(|l| (0..=32).filter(|&n| n != 30).any(|n| states(l, n, 32))), "and no other count: {pol:?}");
}

/// AGREEMENT-08: the median FRI over all 50 and over the top 10 (000-009);
/// gate B's polarity holds if either agrees on 32 of 32, and the output
/// names which.
#[test]
#[ignore = "acceptance-failing: AGREEMENT-08"]
fn agreement_08_the_median_fri_agrees_on_32_of_32_over_all_50_or_the_top_10() {
    assert!(real_data().join(INDICES_DIR).is_dir(), "gate B reads data/{INDICES_DIR}/ (fixtures-3)");
    let Some(r) = ensemble() else { return };
    let rows = gate_b(&r.stdout);
    let median = lines_with(&rows, &["median"]);
    let all_50: Vec<&str> =
        median.iter().copied().filter(|l| l.contains("50") && !l.to_lowercase().contains("top")).collect();
    let top_10: Vec<&str> = median.iter().copied().filter(|l| l.to_lowercase().contains("top") || l.contains("009")).collect();
    assert!(!all_50.is_empty(), "the median over all 50 is reported:\n{}", r.all());
    assert!(!top_10.is_empty(), "the median over the top 10 (000-009) is reported:\n{}", r.all());
    let full = |ls: &[&str]| ls.iter().any(|l| states(l, 32, 32));
    assert!(full(&all_50) || full(&top_10), "either median agrees on 32 of 32: {median:?}");
    assert!(
        median.iter().any(|l| {
            let low = l.to_lowercase();
            (low.contains("all 50") || low.contains("top 10") || low.contains("000-009") || low.contains("000–009"))
                && (low.contains("hold") || low.contains("pass") || low.contains("ok"))
        }),
        "the output names which median made the gate hold: {median:?}"
    );
    assert_eq!(r.code, Some(0), "{}", r.all());
}

/// AGREEMENT-09: the binomial rule over the 50 models' DSIs classifies the
/// 18 known types as the paper does; any difference between the paper's
/// classification and `groundtruth::KNOWN_DSI_TYPES`'s labels is printed.
#[test]
#[ignore = "acceptance-failing: AGREEMENT-09"]
fn agreement_09_the_binomial_rule_classifies_the_18_types_as_the_paper_does() {
    assert_eq!(KNOWN_DSI_TYPES.len(), 18);
    assert!(real_data().join(INDICES_DIR).is_dir(), "gate B reads data/{INDICES_DIR}/ (fixtures-3)");
    let Some(r) = ensemble() else { return };
    let rows = gate_b(&r.stdout);
    let class = lines_with(&rows, &["dsi"]);
    assert!(!class.is_empty(), "a gate B DSI classification line:\n{}", r.all());
    assert!(class.iter().any(|l| states(l, 18, 18) && !fails(l)), "the classification equals the paper's on all 18: {class:?}");
    let text = rows.join("\n").to_lowercase();
    assert!(
        text.contains("known_dsi_types") || text.contains("groundtruth"),
        "the comparison with groundtruth::KNOWN_DSI_TYPES's labels is printed:\n{text}"
    );
}

/// AGREEMENT-10: model 000's preferred directions, paired T4 at intensity 1
/// and T5 at 0: all 8 of T4a-d and T5a-d within 45°.
#[test]
#[ignore = "acceptance-failing: AGREEMENT-10"]
fn agreement_10_model_000_has_all_8_preferred_directions_within_45_degrees() {
    assert_eq!(PREFERRED_DIRECTIONS.len(), 8);
    assert!(real_data().join(INDICES_DIR).is_dir(), "gate B reads data/{INDICES_DIR}/ (fixtures-3)");
    let Some(r) = ensemble() else { return };
    let rows = gate_b(&r.stdout);
    let pd: Vec<&str> = rows
        .iter()
        .copied()
        .filter(|l| {
            let low = l.to_lowercase();
            low.contains("preferred") || low.contains(" pd")
        })
        .collect();
    assert!(!pd.is_empty(), "a gate B preferred-direction line:\n{}", r.all());
    assert!(pd.iter().any(|l| states(l, 8, 8) && l.contains("45") && !fails(l)), "all 8 within 45°: {pd:?}");
}

/// AGREEMENT-11: tuning r for model 000 and the ensemble median, printed
/// per type, not gated.
#[test]
#[ignore = "acceptance-failing: AGREEMENT-11"]
fn agreement_11_tuning_r_is_printed_per_type_and_not_gated() {
    assert!(real_data().join(INDICES_DIR).is_dir(), "gate B reads data/{INDICES_DIR}/ (fixtures-3)");
    let Some(r) = ensemble() else { return };
    let rows = gate_b(&r.stdout);
    let tuning = lines_with(&rows, &["tuning"]);
    assert!(!tuning.is_empty(), "gate B prints tuning r:\n{}", r.all());
    let text = rows.join("\n");
    for (name, _) in PREFERRED_DIRECTIONS {
        assert!(
            text.lines().any(|l| l.contains(name) && l.split_whitespace().any(|w| w.parse::<f64>().is_ok())),
            "tuning r is printed for {name}:\n{text}"
        );
    }
    assert!(
        tuning.iter().any(|l| l.contains("000")) && tuning.iter().any(|l| l.to_lowercase().contains("median")),
        "for model 000 and the ensemble median: {tuning:?}"
    );
    assert!(
        tuning.iter().all(|l| !l.contains("FAIL") && !l.contains("PASS")),
        "tuning r is printed, not gated: {tuning:?}"
    );
}

/// AGREEMENT-12: the same rules over flyvis's own exported values for all
/// 50 give the same counts, sets and verdicts as over Rust's.
#[test]
#[ignore = "acceptance-failing: AGREEMENT-12"]
fn agreement_12_the_rules_over_flyviss_values_give_rusts_counts_sets_and_verdicts() {
    assert!(real_data().join(INDICES_DIR).is_dir(), "gate B reads data/{INDICES_DIR}/ (fixtures-3)");
    let Some(r) = ensemble() else { return };
    let rows = gate_b(&r.stdout);
    let cross: Vec<&str> = rows
        .iter()
        .copied()
        .filter(|l| {
            let low = l.to_lowercase();
            low.contains("cross") || low.contains("flyvis's own") || low.contains("flyvis values")
        })
        .collect();
    assert!(!cross.is_empty(), "gate B prints its cross-check against flyvis's exported values:\n{}", r.all());
    assert!(cross.iter().all(|l| !fails(l)), "the cross-check agrees: {cross:?}");
    assert_eq!(r.code, Some(0), "{}", r.all());
}

/// AGREEMENT-13: when the paper's numbers do not reproduce, gate B fails
/// (exit 1) and says what it found; its rules stay the paper's, never
/// loosened to pass.
#[test]
#[ignore = "acceptance-failing: AGREEMENT-13"]
fn agreement_13_gate_b_fails_rather_than_loosening_a_rule() {
    // The rules are the paper's, whatever the run finds.
    assert_eq!((D_THRESH, BINOMIAL_P0, BINOMIAL_ALPHA, PD_TOLERANCE_DEG), (0.357, 0.1, 0.05, 45.0));

    assert!(real_data().join(INDICES_DIR).is_dir(), "gate B reads data/{INDICES_DIR}/ (fixtures-3)");
    let Some(r) = ensemble() else { return };
    let rows = gate_b(&r.stdout);
    assert!(!rows.is_empty(), "gate B runs under --ensemble:\n{}", r.all());
    let failed: Vec<&str> = rows.iter().copied().filter(|l| fails(l)).collect();
    if failed.is_empty() {
        assert_eq!(r.code, Some(0), "gate B held, so validate exits 0:\n{}", r.all());
    } else {
        assert_eq!(r.code, Some(1), "a gate B failure is exit 1, not a pass:\n{}", r.all());
        assert!(
            failed.iter().all(|l| l.chars().any(|c| c.is_ascii_digit())),
            "each failing gate B line says what was found: {failed:?}"
        );
    }
}
