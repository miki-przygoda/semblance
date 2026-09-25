//! Tests for the generated `pretrained` constants.
//!
//! These live outside `pretrained.rs` on purpose: that file is regenerated
//! wholesale by `tools/export_params.py`, so anything written into it is lost on
//! the next export. Generated data, hand-written checks.

use crate::pretrained::*;

#[test]
fn table_sizes_match_flyvis() {
    assert_eq!(CELL_TYPES.len(), 65);
    assert_eq!(
        CONN_TYPES.len(),
        604,
        "realised connection types, not the 605 declared in the spec"
    );
    assert_eq!(BIAS.len(), 65);
    assert_eq!(TIME_CONST.len(), 65);
    assert_eq!(SYN_STRENGTH.len(), 604);
    assert_eq!(SIGN.len(), 604);
    assert_eq!(SYN_COUNT.len(), 2355);
    assert_eq!(FREE_PARAMETER_COUNT, 734, "the paper's headline count");
}

#[test]
fn values_are_finite() {
    for (name, xs) in [
        ("BIAS", &BIAS[..]),
        ("TIME_CONST", &TIME_CONST[..]),
        ("SYN_STRENGTH", &SYN_STRENGTH[..]),
        ("SIGN", &SIGN[..]),
        ("SYN_COUNT", &SYN_COUNT[..]),
    ] {
        assert!(
            xs.iter().all(|x| x.is_finite()),
            "{name} contains a non-finite value"
        );
    }
}

/// These must be trained values, not the spec's placeholders. If this fails the
/// export read an initialisation checkpoint rather than `best_chkpt`.
#[test]
fn parameters_are_trained_not_placeholders() {
    let distinct = {
        let mut v: Vec<u32> = BIAS.iter().map(|x| x.to_bits()).collect();
        v.sort_unstable();
        v.dedup();
        v.len()
    };
    assert!(distinct > 1, "the spec ships bias = 3.5 for every cell type");
    assert!(
        TIME_CONST.iter().all(|&x| x > 0.0),
        "the spec ships time_constant = null"
    );
    assert!(SYN_STRENGTH.iter().any(|&x| x > 0.0));
}

/// `SYN_COUNT` is stored as ln(count) -- flyvis parameterises it as a Lognormal
/// whose `semantic_values` are `raw.exp()`. Anything consuming it as a synapse
/// count must exponentiate first. This pins the log-space reading so a future
/// change that "helpfully" exponentiates at export time fails loudly.
#[test]
fn syn_count_is_stored_in_log_space() {
    // exp() of these must land on plausible synapse counts, not on ~1.
    let counts: Vec<f32> = SYN_COUNT.iter().map(|x| x.exp()).collect();
    let max = counts.iter().cloned().fold(f32::MIN, f32::max);
    assert!(max > 100.0, "exp(SYN_COUNT) max was {max}; values look already-exponentiated");
    // and the raw values must be small, as logs are
    let raw_max = SYN_COUNT.iter().cloned().fold(f32::MIN, f32::max);
    assert!(raw_max < 10.0, "SYN_COUNT max was {raw_max}; values look like raw counts, not logs");
}

#[test]
fn sign_is_only_plus_or_minus_one() {
    assert!(SIGN.iter().all(|&x| x == 1.0 || x == -1.0));
}

#[test]
fn syn_strength_is_non_negative() {
    // flyvis clamps this parameter to non-negative during training.
    assert!(SYN_STRENGTH.iter().all(|&x| x >= 0.0));
}

#[test]
fn names_are_unique() {
    let mut c: Vec<&str> = CELL_TYPES.to_vec();
    c.sort_unstable();
    c.dedup();
    assert_eq!(c.len(), CELL_TYPES.len(), "duplicate cell type");

    let mut p: Vec<(&str, &str)> = CONN_TYPES.to_vec();
    p.sort_unstable();
    p.dedup();
    assert_eq!(p.len(), CONN_TYPES.len(), "duplicate connection type");
}

/// The positional-alignment trap: every endpoint named by a connection must be a
/// known cell type, or a parameter attaches to nothing.
#[test]
fn connection_endpoints_are_known_cell_types() {
    let known: std::collections::BTreeSet<&str> = CELL_TYPES.iter().copied().collect();
    for (src, tar) in CONN_TYPES {
        assert!(known.contains(src), "{src} is not a known cell type");
        assert!(known.contains(tar), "{tar} is not a known cell type");
    }
}

/// The parameters index flyvis's ordering; the published spec describes the same
/// 65 types. A mismatch means the two disagree about the substrate.
#[test]
fn cell_types_match_the_published_spec() {
    let Some(raw) = crate::test_support::load_real_spec() else {
        return;
    };
    let spec = crate::spec::ConnectomeSpec::from_json_str(&raw).unwrap();
    let spec_names: std::collections::BTreeSet<&str> =
        spec.nodes.iter().map(|n| n.name.as_str()).collect();
    let ours: std::collections::BTreeSet<&str> = CELL_TYPES.iter().copied().collect();
    assert_eq!(
        ours, spec_names,
        "parameter cell types differ from the published spec"
    );
}
