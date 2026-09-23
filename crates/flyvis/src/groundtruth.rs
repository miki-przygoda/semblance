//! flyvis's curated ground truth, transcribed from published literature.
//!
//! Source: `TuragaLab/flyvis`, `flyvis/utils/groundtruth_utils.py`
//! Transcribed at commit `9a516686d158aa568e2035bf65ec15646fb020a8` (2024-11-08).
//! Repo HEAD at transcription: `92b3845cc426dd309a1a0e1b3890156c42e14021`, release v1.2.0.
//!
//! Generated mechanically from that file, not hand-copied. Upstream spells
//! `INSUFFICIENT_DATA` as `unsufficient_data`; the spelling is corrected here.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polarity {
    /// Depolarizes to light increment.
    On,
    /// Depolarizes to light decrement.
    Off,
    /// No established contrast selectivity.
    Unknown,
}

/// Contrast selectivity per cell type. 65 entries; 32 have known polarity.
pub const POLARITY: &[(&str, Polarity)] = &[
    ("R1", Polarity::On),
    ("R2", Polarity::On),
    ("R3", Polarity::On),
    ("R4", Polarity::On),
    ("R5", Polarity::On),
    ("R6", Polarity::On),
    ("R7", Polarity::On),
    ("R8", Polarity::On),
    ("L1", Polarity::Off),
    ("L2", Polarity::Off),
    ("L3", Polarity::Off),
    ("L4", Polarity::Off),
    ("L5", Polarity::On),
    ("Lawf1", Polarity::Unknown),
    ("Lawf2", Polarity::Unknown),
    ("Am", Polarity::Unknown),
    ("C2", Polarity::Unknown),
    ("C3", Polarity::On),
    ("CT1(Lo1)", Polarity::Off),
    ("CT1(M10)", Polarity::On),
    ("Mi1", Polarity::On),
    ("Mi2", Polarity::Unknown),
    ("Mi3", Polarity::Unknown),
    ("Mi4", Polarity::On),
    ("Mi9", Polarity::Off),
    ("Mi10", Polarity::Unknown),
    ("Mi11", Polarity::Unknown),
    ("Mi12", Polarity::Unknown),
    ("Mi13", Polarity::Unknown),
    ("Mi14", Polarity::Unknown),
    ("Mi15", Polarity::Unknown),
    ("T1", Polarity::Unknown),
    ("T2", Polarity::Unknown),
    ("T2a", Polarity::Unknown),
    ("T3", Polarity::Unknown),
    ("T4a", Polarity::On),
    ("T4b", Polarity::On),
    ("T4c", Polarity::On),
    ("T4d", Polarity::On),
    ("T5a", Polarity::Off),
    ("T5b", Polarity::Off),
    ("T5c", Polarity::Off),
    ("T5d", Polarity::Off),
    ("Tm1", Polarity::Off),
    ("Tm2", Polarity::Off),
    ("Tm3", Polarity::On),
    ("Tm4", Polarity::Off),
    ("Tm5Y", Polarity::Unknown),
    ("Tm5a", Polarity::Unknown),
    ("Tm5b", Polarity::Unknown),
    ("Tm5c", Polarity::Unknown),
    ("Tm9", Polarity::Off),
    ("Tm16", Polarity::Unknown),
    ("Tm20", Polarity::Unknown),
    ("Tm28", Polarity::Unknown),
    ("Tm30", Polarity::Unknown),
    ("TmY3", Polarity::Unknown),
    ("TmY4", Polarity::Unknown),
    ("TmY5a", Polarity::Unknown),
    ("TmY9", Polarity::Unknown),
    ("TmY10", Polarity::Unknown),
    ("TmY13", Polarity::Unknown),
    ("TmY14", Polarity::Unknown),
    ("TmY15", Polarity::Unknown),
    ("TmY18", Polarity::Unknown),
];

/// Preferred direction in degrees, for the direction-selective T4/T5 subtypes.
pub const PREFERRED_DIRECTIONS: &[(&str, u16)] = &[
    ("T4a", 180),
    ("T4b", 0),
    ("T4c", 90),
    ("T4d", 270),
    ("T5a", 180),
    ("T5b", 0),
    ("T5c", 90),
    ("T5d", 270),
];

/// 12-point direction tuning curves, from Maisak et al. 2013 Fig. 3g,h.
pub const TUNING_CURVES: &[(&str, [f32; 12])] = &[
    (
        "T4b",
        [
            0.745, 0.785, 0.305, 0.265, 0.25, 0.335, 0.285, 0.23, 0.14, 0.11, 0.26, 0.775,
        ],
    ),
    (
        "T4a",
        [
            0.04, 0.05, 0.165, 0.19, 0.29, 0.5, 1.0, 0.585, 0.31, 0.11, 0.06, 0.04,
        ],
    ),
    (
        "T4c",
        [
            0.04, 0.2, 0.57, 0.775, 0.715, 0.325, 0.22, 0.13, 0.13, 0.22, 0.155, 0.11,
        ],
    ),
    (
        "T4d",
        [
            0.07, 0.07, 0.12, 0.17, 0.21, 0.14, 0.21, 0.29, 0.67, 0.655, 0.7, 0.51,
        ],
    ),
    (
        "T5b",
        [
            0.83, 0.855, 0.405, 0.16, 0.175, 0.24, 0.3, 0.25, 0.13, 0.2, 0.3, 0.655,
        ],
    ),
    (
        "T5a",
        [
            0.2, 0.295, 0.265, 0.2, 0.25, 0.65, 1.0, 0.59, 0.29, 0.21, 0.2, 0.22,
        ],
    ),
    (
        "T5c",
        [
            0.2, 0.44, 0.69, 0.595, 0.745, 0.525, 0.34, 0.255, 0.34, 0.295, 0.26, 0.28,
        ],
    ),
    (
        "T5d",
        [
            0.17, 0.21, 0.21, 0.2, 0.19, 0.15, 0.185, 0.42, 0.85, 0.79, 0.685, 0.565,
        ],
    ),
];

/// Cell types with established motion tuning.
pub const MOTION_TUNING: &[&str] = &["T5a", "T5b", "T5c", "T5d", "T4a", "T4b", "T4c", "T4d"];

/// Cell types established as NOT motion tuned.
pub const NO_MOTION_TUNING: &[&str] = &[
    "Tm1", "Tm2", "Tm4", "Tm9", "CT1(Lo1)", "Mi1", "Tm3", "Mi4", "Mi9", "CT1(M10)",
];

/// Cell types assigned to the ON pathway.
pub const ON_PATHWAY: &[&str] = &[
    "L1", "L3", "L5", "Mi1", "Tm3", "Mi4", "Mi9", "CT1(M10)", "T4a", "T4b", "T4c", "T4d",
];

/// Cell types assigned to the OFF pathway.
pub const OFF_PATHWAY: &[&str] = &[
    "L2", "L3", "L4", "Tm1", "Tm2", "Tm4", "Tm9", "CT1(Lo1)", "T5a", "T5b", "T5c", "T5d",
];

/// Cell types with insufficient published data; exclude from metrics.
pub const INSUFFICIENT_DATA: &[&str] = &[
    "Tm5a", "Tm5b", "Tm5c", "Tm16", "Tm28", "Tm30", "TmY9", "TmY10", "TmY13",
];

/// Cell types whose published measurements are noisy; treat with care.
pub const NOISY_DATA: &[&str] = &["Tm1", "Tm2", "Tm5Y", "Tm9"];

/// Cell types with an established direction-selectivity answer, either way.
pub const KNOWN_DSI_TYPES: &[&str] = &[
    "Tm1", "Tm2", "Tm4", "Tm9", "CT1(Lo1)", "Mi1", "Tm3", "Mi4", "Mi9", "CT1(M10)", "T5a", "T5b",
    "T5c", "T5d", "T4a", "T4b", "T4c", "T4d",
];

#[cfg(test)]
mod tests {
    use super::*;
    // Test-only: this crate's production code deliberately keeps
    // `groundtruth.rs` independent of `spec.rs` (no shared root of trust).
    // Pulling in the real connectome spec here, to check `POLARITY` against
    // it, is fine precisely because it is confined to tests.
    use crate::spec::ConnectomeSpec;
    use crate::test_support::load_real_spec;

    #[test]
    fn table_sizes_match_upstream() {
        assert_eq!(POLARITY.len(), 65);
        assert_eq!(PREFERRED_DIRECTIONS.len(), 8);
        assert_eq!(TUNING_CURVES.len(), 8);
        assert_eq!(MOTION_TUNING.len(), 8);
        assert_eq!(NO_MOTION_TUNING.len(), 10);
        assert_eq!(KNOWN_DSI_TYPES.len(), 18);
        assert_eq!(INSUFFICIENT_DATA.len(), 9);
        assert_eq!(NOISY_DATA.len(), 4);
        assert_eq!(ON_PATHWAY.len(), 12);
        assert_eq!(OFF_PATHWAY.len(), 12);
    }

    #[test]
    fn known_dsi_types_is_the_union_of_motion_tuning_classes() {
        let known: std::collections::BTreeSet<&str> = KNOWN_DSI_TYPES.iter().copied().collect();
        let union: std::collections::BTreeSet<&str> =
            NO_MOTION_TUNING.iter().chain(MOTION_TUNING.iter()).copied().collect();
        assert_eq!(known, union, "KNOWN_DSI_TYPES must equal NO_MOTION_TUNING ++ MOTION_TUNING");
    }

    #[test]
    fn t4_t5_name_sets_agree() {
        let preferred: std::collections::BTreeSet<&str> =
            PREFERRED_DIRECTIONS.iter().map(|(n, _)| *n).collect();
        let tuning: std::collections::BTreeSet<&str> =
            TUNING_CURVES.iter().map(|(n, _)| *n).collect();
        let motion: std::collections::BTreeSet<&str> = MOTION_TUNING.iter().copied().collect();
        assert_eq!(preferred, tuning, "PREFERRED_DIRECTIONS and TUNING_CURVES must name the same cells");
        assert_eq!(preferred, motion, "PREFERRED_DIRECTIONS and MOTION_TUNING must name the same cells");
    }

    #[test]
    fn polarity_distribution_matches_the_paper() {
        let on = POLARITY.iter().filter(|(_, p)| *p == Polarity::On).count();
        let off = POLARITY.iter().filter(|(_, p)| *p == Polarity::Off).count();
        let unknown = POLARITY
            .iter()
            .filter(|(_, p)| *p == Polarity::Unknown)
            .count();
        assert_eq!(on, 18);
        assert_eq!(off, 14);
        assert_eq!(unknown, 33);
        assert_eq!(on + off, 32, "the paper's 32 contrast-selective cell types");
    }

    /// Catches a typo'd cell type, which no count test would notice and which
    /// would silently drop a type from the metric later.
    #[test]
    fn every_named_cell_type_exists_in_polarity() {
        let known: std::collections::BTreeSet<&str> = POLARITY.iter().map(|(n, _)| *n).collect();
        let mut named: Vec<&str> = Vec::new();
        named.extend(PREFERRED_DIRECTIONS.iter().map(|(n, _)| *n));
        named.extend(TUNING_CURVES.iter().map(|(n, _)| *n));
        named.extend(MOTION_TUNING.iter().copied());
        named.extend(NO_MOTION_TUNING.iter().copied());
        named.extend(KNOWN_DSI_TYPES.iter().copied());
        named.extend(INSUFFICIENT_DATA.iter().copied());
        named.extend(NOISY_DATA.iter().copied());
        named.extend(ON_PATHWAY.iter().copied());
        named.extend(OFF_PATHWAY.iter().copied());
        for n in named {
            assert!(known.contains(n), "{n} is not a cell type in POLARITY");
        }
    }

    /// `every_named_cell_type_exists_in_polarity` above treats `POLARITY` as
    /// the root of trust for cell-type names, but `POLARITY` itself is never
    /// checked against anything -- a typo inside it would pass every other
    /// test in this file, including the one built to catch typos. Check it
    /// against the real connectome file's node names instead.
    #[test]
    fn polarity_names_match_the_connectome_node_names_exactly() {
        let Some(raw) = load_real_spec() else { return };
        let s = ConnectomeSpec::from_json_str(&raw).expect("real spec must load");

        let file_names: std::collections::BTreeSet<&str> =
            s.nodes.iter().map(|n| n.name.as_str()).collect();
        let polarity_names: std::collections::BTreeSet<&str> =
            POLARITY.iter().map(|(n, _)| *n).collect();

        assert_eq!(
            polarity_names, file_names,
            "POLARITY's cell-type names must match the connectome file's node names exactly"
        );
    }

    #[test]
    fn polarity_names_are_unique() {
        let set: std::collections::BTreeSet<&str> = POLARITY.iter().map(|(n, _)| *n).collect();
        assert_eq!(set.len(), POLARITY.len(), "duplicate cell type in POLARITY");
    }

    #[test]
    fn t4_t5_preferred_directions_are_cardinal() {
        for (name, deg) in PREFERRED_DIRECTIONS {
            assert!(
                matches!(deg, 0 | 90 | 180 | 270),
                "{name} has direction {deg}"
            );
        }
        assert_eq!(PREFERRED_DIRECTIONS.len(), 8);
    }

    #[test]
    fn tuning_curves_are_normalised_twelve_point() {
        for (name, curve) in TUNING_CURVES {
            assert_eq!(curve.len(), 12, "{name}");
            assert!(
                curve.iter().all(|v| (0.0..=1.0).contains(v)),
                "{name} out of [0,1]"
            );
        }
    }
}
