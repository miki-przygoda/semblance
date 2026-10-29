//! Parity with flyvis, as a library: what the settling and trace gates do,
//! callable from outside `cargo test`.
//!
//! It reads the fixtures (from a data directory, or from bytes the caller
//! already holds), settles a reference run, plays flyvis's twelve stimulus
//! sequences from the settled state, and compares every value with flyvis's
//! reference. It applies the bounds pre-registered in the specs as they are
//! written. Results and errors are typed; nothing here panics on input.
//!
//! The `#[cfg(test)]` gates in `settling_checks.rs` and `trace_checks.rs` are
//! thin wrappers over this module, so they run exactly the code `semblance
//! validate` runs.
//!
//! Spec: docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md,
//! section 1.

mod bounds;
mod inputs;
mod run;

pub use bounds::{settling_bounds, trace_bounds, Bound};
pub use inputs::{how_to_get, required_files, Fixtures, TraceData, TraceInputs};
pub use run::{diff, play, run, settle, Counts, Diff, RunResult, SequenceResult, SettlingResult, Start};

#[cfg(test)]
pub(crate) use run::{cell_type, file, Precision, HEXALS, SNAPSHOTS};

use std::path::PathBuf;

use crate::reference::Dtype;

/// The libm gates' float64 bound, for every quantity they compare.
pub const F64_BOUND: f64 = 1e-10;

/// flyvis's own float32-vs-float64 gaps, PROVENANCE.json `f32_f64_gap`:
/// (model, dt id, settled, settling_centre), as registered. The libm gate's
/// f32 bound is twice each (two f32 computations can sit on opposite sides
/// of the exact value).
pub const SETTLING_F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.4114366875261908e-06, 1.347661143569212e-06),
    ("049", "1/200", 6.1578304686449314e-06, 1.9363140433270587e-06),
    ("000", "1/50", 1.276739533651039e-06, 2.6109457484935206e-06),
    ("049", "1/50", 6.4440782598373936e-06, 1.1488733433040466e-05),
];

/// flyvis's own float32-vs-float64 gaps over the stimulus traces,
/// PROVENANCE.json `f32_f64_gap`: (model, dt id, centre, snapshots), as
/// registered. They include the moving edges' one rounding of the input to
/// f32.
pub const TRACE_F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.7249018977594233e-06, 2.6318956276583094e-06),
    ("049", "1/200", 2.600224337356849e-05, 5.1135401793800384e-05),
    ("000", "1/50", 2.5357808688397654e-06, 2.9820266052027478e-06),
    ("049", "1/50", 3.4419031081256435e-05, 1.4233448473766064e-04),
];

/// "Misses by far more than any bound": the model and mapping controls.
pub const CONTROL_MISS: f64 = 1e-3;

/// Sintel revisit trigger 1: a cell type is driven if its centre trace moves
/// by more than this (spec R8). Recorded, not a gate.
pub const COVERAGE_THRESHOLD: f64 = 1e-3;

/// The eight registered runs: (index, model, dt id, precision). The source
/// of every run's precision (spec R19).
pub const RUNS: [(u32, &str, &str, Dtype); 8] = [
    (0, "000", "1/200", Dtype::F32),
    (1, "000", "1/200", Dtype::F64),
    (2, "000", "1/50", Dtype::F32),
    (3, "000", "1/50", Dtype::F64),
    (4, "049", "1/200", Dtype::F32),
    (5, "049", "1/200", Dtype::F64),
    (6, "049", "1/50", Dtype::F32),
    (7, "049", "1/50", Dtype::F64),
];

/// Why a parity check could not run, or a state that went non-finite.
#[derive(Clone, Debug, PartialEq)]
pub enum ParityError {
    /// A required file is absent.
    Missing { rel: String, path: PathBuf },
    /// A required file is present but could not be read.
    Unreadable { rel: String, path: PathBuf, kind: std::io::ErrorKind },
    /// A file is present but does not parse, or does not fit the network.
    Malformed { rel: String, detail: String },
    /// The fixtures parse but disagree with each other or with what is
    /// registered.
    Mismatch(String),
    /// The integrator's state went NaN or infinite: a gate failure.
    NonFinite { run: u32, node: usize, cell_type: String, after: After },
    /// A bad `Options` value, or a question with no answer (an unregistered
    /// run, a bound for a control).
    Usage(String),
}

/// When a state went non-finite.
#[derive(Clone, Debug, PartialEq)]
pub enum After {
    /// After settle step `k` (the state after k + 1 Euler steps).
    SettleStep(usize),
    /// After frame `frame` of sequence `sequence`.
    Frame { frame: usize, sequence: u32 },
}

impl ParityError {
    /// Whether this error is a gate failure (exit 1) rather than
    /// verification not completing (exit 2). Only a non-finite state is.
    pub fn is_gate_failure(&self) -> bool {
        matches!(self, ParityError::NonFinite { .. })
    }
}

impl std::fmt::Display for ParityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParityError::Missing { rel, path } => write!(f, "{} not found. {}", path.display(), how_to_get(rel)),
            ParityError::Unreadable { path, kind, .. } => write!(f, "{}: cannot be read ({kind:?})", path.display()),
            ParityError::Malformed { rel, detail } => write!(f, "{rel}: {detail}"),
            ParityError::Mismatch(m) | ParityError::Usage(m) => f.write_str(m),
            ParityError::NonFinite { run, node, cell_type, after: After::SettleStep(k) } => {
                write!(f, "run {run}: node {node} ({cell_type}) is not finite after settle step {k}")
            }
            ParityError::NonFinite { run, node, cell_type, after: After::Frame { frame, sequence } } => {
                write!(f, "run {run}: node {node} ({cell_type}) is not finite after frame {frame} of sequence {sequence}")
            }
        }
    }
}

impl std::error::Error for ParityError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_as_registered() {
        let e = ParityError::NonFinite { run: 3, node: 7, cell_type: "Mi4".into(), after: After::SettleStep(2) };
        assert_eq!(e.to_string(), "run 3: node 7 (Mi4) is not finite after settle step 2");
        assert!(e.is_gate_failure());
        let e = ParityError::NonFinite { run: 1, node: 9, cell_type: "T4a".into(), after: After::Frame { frame: 4, sequence: 8 } };
        assert_eq!(e.to_string(), "run 1: node 9 (T4a) is not finite after frame 4 of sequence 8");
        let e = ParityError::Missing { rel: "flyvis-reference/x.bin".into(), path: PathBuf::from("d/flyvis-reference/x.bin") };
        assert_eq!(
            e.to_string(),
            "d/flyvis-reference/x.bin not found. Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        );
        assert!(!e.is_gate_failure());
        let e = ParityError::Unreadable {
            rel: "a".into(),
            path: PathBuf::from("d/a"),
            kind: std::io::ErrorKind::PermissionDenied,
        };
        assert_eq!(e.to_string(), "d/a: cannot be read (PermissionDenied)");
        let e = ParityError::Malformed { rel: "flyvis-pretrained/ensemble.csv".into(), detail: "empty".into() };
        assert_eq!(e.to_string(), "flyvis-pretrained/ensemble.csv: empty");
        for e in [ParityError::Mismatch("m".into()), ParityError::Usage("u".into())] {
            assert!(!e.is_gate_failure());
        }
    }

    /// The registry is the manifest's eight runs, in index order.
    #[test]
    fn the_registry_lists_the_eight_runs_in_index_order() {
        for (i, r) in RUNS.iter().enumerate() {
            assert_eq!(r.0 as usize, i);
            assert_eq!(r.3, if i % 2 == 0 { Dtype::F32 } else { Dtype::F64 });
        }
    }
}
