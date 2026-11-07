//! Acceptance tests for behaviour group `agreement` of epic fidelity-measures:
//! the paper's agreement rules and constants. One test per behaviour id,
//! AGREEMENT-01 .. AGREEMENT-06; AGREEMENT-07 .. AGREEMENT-13 are gate B's,
//! which `semblance validate --ensemble` runs, and live in
//! `app/tests/acceptance_agreement.rs`.
//!
//! Every test drives `semblance_fidelity::indices`' public API the way a
//! caller would, with inputs built by hand whose answers are known.

use std::f64::consts::PI;

use semblance_fidelity::indices::{
    binomial_upper_tail, dsi_class, median_fri, pd_within, polarity_agreement, Agreement, BINOMIAL_ALPHA, BINOMIAL_P0,
    D_THRESH, IndexError, PD_TOLERANCE_DEG,
};

const DEG: f64 = PI / 180.0;

fn names(types: &[&str]) -> Vec<String> {
    types.iter().map(|t| t.to_string()).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * b.abs().max(1e-300)
}

/// AGREEMENT-01: the four constants hold the paper's values, are defined
/// together in one place, and each carries its citation; the PD tolerance
/// says it is borrowed from the paper's model selection.
#[test]
fn agreement_01_the_constants_live_in_one_place_with_their_citations() {
    assert_eq!(D_THRESH, 0.357);
    assert_eq!(BINOMIAL_P0, 0.1);
    assert_eq!(BINOMIAL_ALPHA, 0.05);
    assert_eq!(PD_TOLERANCE_DEG, 45.0);

    // The documentation a reader of the crate sees: the constants' doc
    // comments in the module that defines them.
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/indices.rs")).expect("indices.rs");
    let lines: Vec<&str> = src.lines().collect();
    let def = |name: &str| {
        lines
            .iter()
            .position(|l| l.trim_start().starts_with(&format!("pub const {name}:")))
            .unwrap_or_else(|| panic!("{name} is defined in indices.rs"))
    };
    let doc = |name: &str| {
        let at = def(name);
        let mut out = Vec::new();
        for l in lines[..at].iter().rev() {
            match l.trim_start().strip_prefix("///") {
                Some(d) => out.push(d.trim()),
                None => break,
            }
        }
        out.reverse();
        out.join(" ")
    };

    // One place: the four definitions sit together.
    let at: Vec<usize> = ["D_THRESH", "BINOMIAL_P0", "BINOMIAL_ALPHA", "PD_TOLERANCE_DEG"].iter().map(|n| def(n)).collect();
    let span = at.iter().max().unwrap() - at.iter().min().unwrap();
    assert!(span <= 20, "the four constants are defined together (lines {at:?})");
    for name in ["D_THRESH", "BINOMIAL_P0", "BINOMIAL_ALPHA", "PD_TOLERANCE_DEG"] {
        assert_eq!(src.matches(&format!("pub const {name}:")).count(), 1, "{name} is defined once");
    }

    // Each with its citation.
    let d = doc("D_THRESH");
    assert!(d.contains("Lappalainen") && d.contains("Methods"), "D_THRESH cites the paper: {d}");
    for name in ["BINOMIAL_P0", "BINOMIAL_ALPHA"] {
        let d = doc(name);
        assert!(d.contains("Methods") || d.contains("section"), "{name} cites the paper's section: {d}");
    }
    let d = doc("PD_TOLERANCE_DEG");
    assert!(d.to_lowercase().contains("borrowed"), "PD_TOLERANCE_DEG says it is borrowed: {d}");
    assert!(d.contains("Model selection") || d.contains("model selection"), "from the paper's model selection: {d}");

    // The rules are recorded as the paper's.
    for rule in ["pub fn polarity_agreement", "pub fn dsi_class"] {
        let at = lines.iter().position(|l| l.starts_with(rule)).unwrap_or_else(|| panic!("{rule}"));
        let mut d = String::new();
        for l in lines[..at].iter().rev() {
            match l.trim_start().strip_prefix("///") {
                Some(x) => d = format!("{x} {d}"),
                None => break,
            }
        }
        assert!(d.contains("paper's"), "{rule} is recorded as the paper's rule: {d}");
    }
}

/// AGREEMENT-02: sign agreement over the known types, counted out of how
/// many, with the misses named; an FRI of exactly 0 has no sign and misses;
/// types not in `known` are not counted.
#[test]
fn agreement_02_polarity_agreement_counts_sign_agreement_and_names_misses() {
    let types = names(&["L1", "L2", "Mi1", "Tm3", "R1", "Tm9", "Am"]);
    let fri = vec![-0.8, 0.2, 0.5, 0.0, 0.9, -0.1, 0.7];
    let known: &[(&str, i8)] = &[("L1", -1), ("L2", -1), ("Mi1", 1), ("Tm3", 1), ("R1", 1), ("Tm9", -1)];
    let a = polarity_agreement(&fri, &types, known).expect("agreement");
    assert_eq!(a, Agreement { count: 4, of: 6, misses: vec!["L2".to_string(), "Tm3".to_string()] });

    let all_right = polarity_agreement(&[-1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -5.0], &types, known).unwrap();
    assert_eq!((all_right.count, all_right.of, all_right.misses.len()), (6, 6, 0));

    // A known type missing from the types is refused, not scored.
    assert!(polarity_agreement(&fri, &types, &[("T4a", 1)]).is_err());
}

/// AGREEMENT-03: the per-type median across models, as numpy's: odd counts
/// take the middle value, even counts average the two middle values.
#[test]
fn agreement_03_median_fri_is_the_per_type_median_across_models() {
    let odd = vec![vec![0.1, -0.5, 3.0], vec![0.4, -0.2, 1.0], vec![-0.3, -0.9, 2.0]];
    assert_eq!(median_fri(&odd).unwrap(), vec![0.1, -0.5, 2.0]);

    let even = vec![vec![1.0, -4.0], vec![3.0, 0.0], vec![-2.0, 2.0], vec![5.0, -1.0]];
    assert_eq!(median_fri(&even).unwrap(), vec![2.0, -0.5]);

    // 50 models, as gate B uses: type 0 holds model index m, so the median
    // is (24 + 25) / 2.
    let fifty: Vec<Vec<f64>> = (0..50).map(|m| vec![m as f64, -(m as f64)]).collect();
    assert_eq!(median_fri(&fifty).unwrap(), vec![24.5, -24.5]);

    assert!(median_fri(&[]).is_err(), "no models is refused");
}

/// AGREEMENT-04: the exact one-sided upper tail, against values computed by
/// hand.
#[test]
fn agreement_04_binomial_upper_tail_is_exact_for_small_n() {
    // P(X >= 2), n = 3, p = 0.1: 3 * 0.01 * 0.9 + 0.001 = 0.028.
    assert!(close(binomial_upper_tail(3, 2, 0.1).unwrap(), 0.028), "{}", binomial_upper_tail(3, 2, 0.1).unwrap());
    // P(X >= 3), n = 5, p = 0.1: 10 * 0.001 * 0.81 + 5 * 0.0001 * 0.9 + 0.00001 = 0.00856.
    assert!(close(binomial_upper_tail(5, 3, 0.1).unwrap(), 0.00856), "{}", binomial_upper_tail(5, 3, 0.1).unwrap());
    // P(X >= 1), n = 4, p = 0.5: 1 - 1/16.
    assert_eq!(binomial_upper_tail(4, 1, 0.5).unwrap(), 0.9375);
    // P(X >= 2), n = 2, p = 0.1: 0.01.
    assert!(close(binomial_upper_tail(2, 2, 0.1).unwrap(), 0.01));
    // P(X >= 0) is certain; P(X >= n + 1) is impossible.
    assert_eq!(binomial_upper_tail(7, 0, 0.1).unwrap(), 1.0);
    assert_eq!(binomial_upper_tail(7, 8, 0.1).unwrap(), 0.0);
    // P(X >= n) = p^n.
    assert!(close(binomial_upper_tail(6, 6, 0.5).unwrap(), 1.0 / 64.0));
    // Summed in f64: a tail far below f32's range of precision survives.
    let tiny = binomial_upper_tail(50, 50, 0.1).unwrap();
    assert!(tiny > 0.0 && close(tiny, 1e-50), "{tiny:e}");
    // A p0 that is no probability is refused, never summed.
    for p0 in [f64::NAN, -0.1, 2.0] {
        assert!(
            matches!(binomial_upper_tail(5, 3, p0), Err(IndexError::BadParameter { what: "p0", .. })),
            "p0 {p0}"
        );
    }
}

/// AGREEMENT-05: a type is direction selective when the number of models
/// with DSI strictly above `d_thresh` passes the one-sided binomial test
/// against p0 at P strictly below alpha. With 50 models and p0 = 0.1,
/// P(X >= 9) is about 0.058 and P(X >= 10) about 0.025, so 10 models pass
/// and 9 do not.
#[test]
fn agreement_05_dsi_class_applies_the_binomial_test_to_models_above_threshold() {
    let n = 50;
    assert!(binomial_upper_tail(n, 9, BINOMIAL_P0).unwrap() >= BINOMIAL_ALPHA);
    assert!(binomial_upper_tail(n, 10, BINOMIAL_P0).unwrap() < BINOMIAL_ALPHA);

    // Types: all above; 10 above; 9 above; 10 exactly at the threshold
    // (not above); none above.
    let per_model: Vec<Vec<f64>> = (0..n as usize)
        .map(|m| {
            vec![
                0.9,
                if m < 10 { 0.5 } else { 0.1 },
                if m < 9 { 0.5 } else { 0.1 },
                if m < 10 { D_THRESH } else { 0.0 },
                0.2,
            ]
        })
        .collect();
    let class = dsi_class(&per_model, D_THRESH, BINOMIAL_P0, BINOMIAL_ALPHA).expect("class");
    assert_eq!(class, vec![true, true, false, false, false]);

    // The verdict follows the parameters passed in, not the constants.
    let lower = dsi_class(&per_model, 0.3, BINOMIAL_P0, BINOMIAL_ALPHA).unwrap();
    assert_eq!(lower, vec![true, true, false, true, false]);
    let looser = dsi_class(&per_model, D_THRESH, BINOMIAL_P0, 0.06).unwrap();
    assert_eq!(looser, vec![true, true, true, false, false]);
}

/// AGREEMENT-06: preferred directions within the tolerance of the known
/// ones, compared the shorter way round, T4 types read at intensity 1 and
/// T5 types at intensity 0, with the misses named.
#[test]
fn agreement_06_pd_within_counts_types_within_tolerance_the_shorter_way_round() {
    let types = names(&["Mi1", "T4a", "T4b", "T5a", "T5b"]);
    let known: &[(&str, u16)] = &[("T4a", 180), ("T4b", 0), ("T5a", 90), ("T5b", 270)];
    // Intensity 0 (OFF) and 1 (ON), per type.
    let mut pd = [vec![0.0; 5], vec![0.0; 5]];
    pd[1][1] = (180.0 - 40.0) * DEG; // T4a: 40° off: within.
    pd[0][1] = 0.0; //                  T4a at OFF is not read.
    pd[1][2] = 350.0 * DEG; //          T4b: 350° is 10° from 0° the shorter way.
    pd[0][3] = (90.0 + 50.0) * DEG; //  T5a: 50° off: a miss.
    pd[1][3] = 90.0 * DEG; //           T5a at ON is not read.
    pd[0][4] = -46.0 * DEG; //          T5b: -46° is 44° from 270° the shorter way.
    let a = pd_within(&pd, &types, known, PD_TOLERANCE_DEG).expect("agreement");
    assert_eq!(a, Agreement { count: 3, of: 4, misses: vec!["T5a".to_string()] });

    let tight = pd_within(&pd, &types, known, 20.0).unwrap();
    assert_eq!(tight, Agreement { count: 1, of: 4, misses: vec!["T4a".to_string(), "T5a".to_string(), "T5b".to_string()] });
}
