//! Acceptance tests for behaviour group `controls` of epic
//! fidelity-measures, the part `semblance validate --ensemble` shows: gate
//! C, the four controls' 50 draws each scored against the trained models.
//! One test per behaviour id, CONTROLS-09 .. CONTROLS-12; CONTROLS-01 ..
//! CONTROLS-08 are in `crates/flyvis/tests/acceptance_controls.rs`.
//!
//! Every test drives the built `semblance` binary on the real `data/`
//! directory, with `--ensemble`, and reads its stdout, stderr and exit
//! code. Gate C reads 0.3.0's fixtures (fixtures-2) only.
//!
//! The format of gate C's lines is not fixed by the spec, so the tests read
//! it loosely: a gate C line is one naming `gate C` (or, after a heading
//! naming it, any line up to the next `gate`/`controls:`/`coverage:`
//! heading), and a control's lines are those naming it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use semblance_flyvis::parity::required_files;

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

/// The four controls as `validate` names them.
const CONTROLS: [&str; 4] = ["zeroed", "shuffled", "random parameters", "rewired"];

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

/// Whether every fixture gate C reads is present; if not, a skip notice
/// (false) or, under `SEMBLANCE_REQUIRE_FIXTURES=1`, a failure.
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

/// `validate --ensemble` on the real data, once per test binary.
fn ensemble(test: &str) -> Option<&'static Run> {
    static RUN: OnceLock<Run> = OnceLock::new();
    if !fixtures_or_skip(test) {
        return None;
    }
    Some(RUN.get_or_init(|| {
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
    }))
}

/// The lines of the section named `name`: those naming it, and those in a
/// section headed by one.
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

/// Gate C's lines, which must exist once verification completed.
fn gate_c(r: &Run) -> Vec<&str> {
    assert_ne!(r.code, Some(2), "validate --ensemble verified something:\n{}", r.all());
    let c = section(&r.stdout, "gate c");
    assert!(!c.is_empty(), "gate C runs under --ensemble:\n{}", r.all());
    c
}

/// CONTROLS-09: each control's 50 draws, scored by CKA against their parent
/// models, all fall below the trained pairwise minimum.
#[test]
#[ignore = "acceptance-failing: CONTROLS-09"]
fn controls_09_every_control_draw_scores_below_the_trained_pairwise_minimum() {
    let Some(r) = ensemble("controls_09") else { return };
    let c = gate_c(r);
    assert!(
        lines_with(&c, &["min"]).iter().any(|l| numbers(l).iter().any(|&v| v > 0.0 && v <= 1.0)),
        "gate C states the trained pairwise minimum: {c:?}"
    );
    for name in CONTROLS {
        let rows = lines_with(&c, &[name]);
        assert!(!rows.is_empty(), "gate C reports the {name} control: {c:?}");
        let text = rows.join("\n");
        assert!(
            text.contains("50/50") || text.contains("50 of 50") || text.contains("all 50"),
            "all 50 {name} draws are below the minimum: {rows:?}"
        );
        assert!(lines_with(&rows, &["cka"]).len() + lines_with(&rows, &["min"]).len() > 0, "{name}: scored by CKA: {rows:?}");
        assert!(!rows.iter().any(|l| l.contains("FAIL")), "{name}: every draw below the trained minimum: {rows:?}");
    }
    assert_eq!(r.code, Some(0), "gate C holds:\n{}", r.all());
}

/// CONTROLS-10: under gate B's binomial rule, each control's 50 draws agree
/// with the paper's classification of the 18 types on fewer types than the
/// trained 50 do.
#[test]
#[ignore = "acceptance-failing: CONTROLS-10"]
fn controls_10_each_control_agrees_with_the_papers_binomial_classes_on_fewer_types() {
    let Some(r) = ensemble("controls_10") else { return };
    let c = gate_c(r);
    let binom: Vec<&str> = c
        .iter()
        .copied()
        .filter(|l| {
            let low = l.to_lowercase();
            low.contains("binomial") || low.contains("dsi") || low.contains("direction selective")
        })
        .collect();
    assert!(!binom.is_empty(), "gate C reports the binomial rule over the controls: {c:?}");
    let trained = lines_with(&binom, &["trained"]);
    assert!(!trained.is_empty(), "the trained 50's agreement is stated beside the controls: {binom:?}");
    let of_18 = |l: &str| numbers(l).windows(2).find(|w| w[1] == 18.0).map(|w| w[0]);
    let trained_n = trained.iter().find_map(|l| of_18(l)).expect("the trained count, out of 18");
    for name in CONTROLS {
        let rows = lines_with(&binom, &[name]);
        assert!(!rows.is_empty(), "{name}: the binomial rule's agreement is printed: {binom:?}");
        let n = rows.iter().find_map(|l| of_18(l)).unwrap_or_else(|| panic!("{name}: a count out of 18: {rows:?}"));
        assert!(n < trained_n, "{name}: agrees on {n} of 18, not fewer than the trained {trained_n}");
        assert!(!rows.iter().any(|l| l.contains("FAIL")), "{rows:?}");
    }
}

/// CONTROLS-11: for each control, the median over its 50 draws of how many
/// of the 8 motion types have a preferred direction within 45 degrees is
/// below model 000's count.
#[test]
#[ignore = "acceptance-failing: CONTROLS-11"]
fn controls_11_each_controls_median_pd_count_is_below_model_000s() {
    let Some(r) = ensemble("controls_11") else { return };
    let c = gate_c(r);
    let pd: Vec<&str> = c
        .iter()
        .copied()
        .filter(|l| {
            let low = l.to_lowercase();
            low.contains("preferred") || low.contains(" pd") || low.contains("45")
        })
        .collect();
    assert!(!pd.is_empty(), "gate C reports preferred directions over the controls: {c:?}");
    let of_8 = |l: &str| numbers(l).windows(2).find(|w| w[1] == 8.0).map(|w| w[0]);
    let base = lines_with(&pd, &["000"]).iter().find_map(|l| of_8(l)).expect("model 000's count, out of 8");
    for name in CONTROLS {
        let rows = lines_with(&pd, &[name]);
        assert!(!rows.is_empty(), "{name}: its preferred directions are reported: {pd:?}");
        let n = lines_with(&rows, &["median"]).iter().find_map(|l| of_8(l)).unwrap_or_else(|| panic!("{name}: a median out of 8: {rows:?}"));
        assert!(n < base, "{name}: median {n} of 8 is not below model 000's {base}");
        assert!(!rows.iter().any(|l| l.contains("FAIL")), "{rows:?}");
    }
}

/// CONTROLS-12: each control's polarity and tuning are printed and not gated.
#[test]
#[ignore = "acceptance-failing: CONTROLS-12"]
fn controls_12_each_controls_polarity_and_tuning_are_printed_and_not_gated() {
    let Some(r) = ensemble("controls_12") else { return };
    let c = gate_c(r);
    for name in CONTROLS {
        let rows = lines_with(&c, &[name]);
        for what in ["polarity", "tuning"] {
            let lines = lines_with(&rows, &[what]);
            assert!(!lines.is_empty(), "{name}: its {what} is printed: {rows:?}");
            assert!(
                lines.iter().all(|l| !l.contains("PASS") && !l.contains("FAIL")),
                "{name}: {what} is reported, not gated: {lines:?}"
            );
            assert!(lines.iter().any(|l| !numbers(l).is_empty()), "{name}: {what} has a value: {lines:?}");
        }
    }
}
