//! flyvis's trained ensemble: the free parameters of all 50 models, read from
//! `data/flyvis-pretrained/ensemble.csv` (written by `tools/export_params.py`).
//!
//! `pretrained.rs` compiles in model 000 only. This reads the same 734 values
//! for every model, positionally, in `pretrained.rs`'s orderings, and refuses
//! any row out of place: a misread row would attach a parameter to the wrong
//! cell type while everything downstream still runs.
//!
//! No file I/O -- callers hand in the text.

use crate::pretrained::{BIAS, CELL_TYPES, CONN_TYPES, SYN_STRENGTH, TIME_CONST};

/// Models in flyvis's pretrained ensemble.
pub const N_MODELS: usize = 50;
/// Rows per model: 65 biases, 65 time constants, 604 synapse strengths.
pub const ROWS_PER_MODEL: usize = 65 + 65 + 604;
const HEADER: &str = "model,kind,index,name,value";

/// One model's 734 free parameters, positional as in `pretrained.rs`.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelParams {
    pub bias: [f32; 65],
    pub time_const: [f32; 65],
    pub syn_strength: [f32; 604],
}

impl ModelParams {
    /// Model 000, from the compiled-in constants in `pretrained.rs`.
    pub fn pretrained() -> Self {
        ModelParams { bias: BIAS, time_const: TIME_CONST, syn_strength: SYN_STRENGTH }
    }
}

/// All 50 models, in file order (`000` to `049`).
#[derive(Clone, Debug, PartialEq)]
pub struct Ensemble {
    models: Vec<(String, ModelParams)>,
}

/// Why `ensemble.csv` was refused. `line` is 1-based.
#[derive(Clone, Debug, PartialEq)]
pub struct EnsembleError {
    pub line: usize,
    pub kind: EnsembleErrorKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EnsembleErrorKind {
    /// The file contains a `\r`; it is LF-only.
    CarriageReturn,
    /// Nothing but `#` lines.
    MissingHeader,
    BadHeader(String),
    /// A `#` line after the header.
    CommentAfterHeader,
    /// A row without exactly five fields.
    FieldCount(usize),
    /// `model` is not three ASCII digits.
    BadModel(String),
    ModelOutOfOrder { expected: String, found: String },
    BadKind { expected: &'static str, found: String },
    BadIndex { expected: usize, found: String },
    BadName { expected: String, found: String },
    /// Not a finite `f32`.
    BadValue(String),
    TooFewRows { expected: usize, found: usize },
    TooManyRows { expected: usize },
}

impl std::fmt::Display for EnsembleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EnsembleErrorKind::*;
        write!(f, "ensemble.csv line {}: ", self.line)?;
        match &self.kind {
            CarriageReturn => write!(f, "contains a carriage return; the file is LF-only"),
            MissingHeader => write!(f, "no header line"),
            BadHeader(h) => write!(f, "header is {h:?}, expected {HEADER:?}"),
            CommentAfterHeader => write!(f, "a # line after the header"),
            FieldCount(n) => write!(f, "{n} fields, expected 5"),
            BadModel(m) => write!(f, "model {m:?} is not three digits"),
            ModelOutOfOrder { expected, found } => write!(f, "model {found}, expected {expected}"),
            BadKind { expected, found } => write!(f, "kind {found:?}, expected {expected}"),
            BadIndex { expected, found } => write!(f, "index {found:?}, expected {expected}"),
            BadName { expected, found } => write!(f, "name {found:?}, expected {expected:?}"),
            BadValue(v) => write!(f, "value {v:?} is not a finite f32"),
            TooFewRows { expected, found } => write!(f, "{found} rows, expected {expected}"),
            TooManyRows { expected } => write!(f, "more than {expected} rows"),
        }
    }
}

impl std::error::Error for EnsembleError {}

/// The (kind, index, name) that row `s` of every model must carry.
fn slot(s: usize) -> (&'static str, usize, String) {
    if s < 65 {
        ("bias", s, CELL_TYPES[s].to_string())
    } else if s < 130 {
        ("time_const", s - 65, CELL_TYPES[s - 65].to_string())
    } else {
        let (src, tar) = CONN_TYPES[s - 130];
        ("syn_strength", s - 130, format!("{src}>{tar}"))
    }
}

impl Ensemble {
    /// Parses the whole file. See the module doc for what is refused.
    pub fn parse(csv: &str) -> Result<Self, EnsembleError> {
        use EnsembleErrorKind::*;
        let err = |line: usize, kind: EnsembleErrorKind| EnsembleError { line, kind };

        let mut lines: Vec<&str> = csv.split('\n').collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        if let Some(i) = lines.iter().position(|l| l.contains('\r')) {
            return Err(err(i + 1, CarriageReturn));
        }
        let header = lines
            .iter()
            .position(|l| !l.starts_with('#'))
            // Empty input has no lines at all; the header was expected at line 1.
            .ok_or_else(|| err(lines.len().max(1), MissingHeader))?;
        if lines[header] != HEADER {
            return Err(err(header + 1, BadHeader(lines[header].to_string())));
        }

        let rows = &lines[header + 1..];
        let expected = N_MODELS * ROWS_PER_MODEL;
        let mut models = Vec::with_capacity(N_MODELS);
        let mut current = ModelParams { bias: [0.0; 65], time_const: [0.0; 65], syn_strength: [0.0; 604] };
        for (r, row) in rows.iter().enumerate() {
            let line = header + 2 + r;
            // Checked row by row, not up front, so an earlier bad row (a stray
            // `#` line, say) is reported at its own line rather than as a
            // surplus at the end.
            if r >= expected {
                return Err(err(line, TooManyRows { expected }));
            }
            let (m, s) = (r / ROWS_PER_MODEL, r % ROWS_PER_MODEL);
            if row.starts_with('#') {
                return Err(err(line, CommentAfterHeader));
            }
            let f: Vec<&str> = row.split(',').collect();
            if f.len() != 5 {
                return Err(err(line, FieldCount(f.len())));
            }
            if f[0].len() != 3 || !f[0].bytes().all(|b| b.is_ascii_digit()) {
                return Err(err(line, BadModel(f[0].to_string())));
            }
            let model = format!("{m:03}");
            if f[0] != model {
                return Err(err(line, ModelOutOfOrder { expected: model, found: f[0].to_string() }));
            }
            let (kind, index, name) = slot(s);
            if f[1] != kind {
                return Err(err(line, BadKind { expected: kind, found: f[1].to_string() }));
            }
            if f[2] != index.to_string() {
                return Err(err(line, BadIndex { expected: index, found: f[2].to_string() }));
            }
            if f[3] != name {
                return Err(err(line, BadName { expected: name, found: f[3].to_string() }));
            }
            let value = f[4]
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| err(line, BadValue(f[4].to_string())))?;
            match kind {
                "bias" => current.bias[index] = value,
                "time_const" => current.time_const[index] = value,
                _ => current.syn_strength[index] = value,
            }
            if s == ROWS_PER_MODEL - 1 {
                models.push((model, current.clone()));
            }
        }
        if rows.len() < expected {
            return Err(err(lines.len(), TooFewRows { expected, found: rows.len() }));
        }
        Ok(Ensemble { models })
    }

    /// One model's parameters, by its three-digit id (`"000"`..`"049"`).
    pub fn model(&self, id: &str) -> Option<&ModelParams> {
        self.models.iter().find(|(m, _)| m == id).map(|(_, p)| p)
    }

    /// The model ids, in file order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.models.iter().map(|(m, _)| m.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::load_pretrained_fixture;
    use EnsembleErrorKind::*;

    /// A valid file of `n_models` models, every one holding model 000's
    /// values. One `#` line and the header come first, so data row r is on
    /// line r + 3.
    fn csv(n_models: usize) -> String {
        let p = ModelParams::pretrained();
        let mut out = String::from("# synthetic\nmodel,kind,index,name,value\n");
        for m in 0..n_models {
            for s in 0..ROWS_PER_MODEL {
                let (kind, i, name) = slot(s);
                let v = match kind {
                    "bias" => p.bias[i],
                    "time_const" => p.time_const[i],
                    _ => p.syn_strength[i],
                };
                out.push_str(&format!("{m:03},{kind},{i},{name},{v}\n"));
            }
        }
        out
    }

    /// `text` with its 1-based `line` replaced by `new`.
    fn with_line(text: &str, line: usize, new: &str) -> String {
        let mut lines: Vec<&str> = text.split('\n').collect();
        lines[line - 1] = new;
        lines.join("\n")
    }

    fn error(text: &str) -> EnsembleError {
        Ensemble::parse(text).expect_err("must be refused")
    }

    fn bits(v: &[f32]) -> Vec<u32> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    #[test]
    fn parses_a_valid_file() {
        let e = Ensemble::parse(&csv(N_MODELS)).expect("valid");
        let ids: Vec<&str> = e.ids().collect();
        assert_eq!(ids.len(), N_MODELS);
        assert_eq!((ids[0], ids[49]), ("000", "049"));
        assert_eq!(e.model("049"), Some(&ModelParams::pretrained()));
        assert_eq!(e.model("050"), None);
    }

    #[test]
    fn accepts_a_file_without_a_trailing_newline() {
        let text = csv(N_MODELS);
        assert!(Ensemble::parse(text.trim_end_matches('\n')).is_ok());
    }

    #[test]
    fn rejects_a_carriage_return() {
        let text = csv(N_MODELS).replacen('\n', "\r\n", 1);
        assert_eq!(error(&text), EnsembleError { line: 1, kind: CarriageReturn });
    }

    #[test]
    fn rejects_a_file_with_no_header() {
        assert_eq!(error("# only a comment\n").kind, MissingHeader);
    }

    #[test]
    fn rejects_a_header_with_a_field_missing() {
        let text = with_line(&csv(N_MODELS), 2, "model,kind,index,name");
        assert_eq!(error(&text), EnsembleError { line: 2, kind: BadHeader("model,kind,index,name".into()) });
    }

    #[test]
    fn rejects_a_comment_after_the_header() {
        let text = csv(N_MODELS).replacen("value\n", "value\n# late\n", 1);
        assert_eq!(error(&text), EnsembleError { line: 3, kind: CommentAfterHeader });
    }

    #[test]
    fn rejects_a_shuffled_row() {
        let text = csv(N_MODELS);
        let lines: Vec<&str> = text.split('\n').collect();
        let swapped = with_line(&with_line(&text, 3, lines[3]), 4, lines[2]);
        assert_eq!(
            error(&swapped),
            EnsembleError { line: 3, kind: BadIndex { expected: 0, found: "1".into() } }
        );
    }

    #[test]
    fn rejects_a_wrong_name() {
        let text = csv(N_MODELS);
        let row = text.split('\n').nth(2).unwrap().replacen(",R1,", ",R2,", 1);
        assert_eq!(
            error(&with_line(&text, 3, &row)),
            EnsembleError { line: 3, kind: BadName { expected: "R1".into(), found: "R2".into() } }
        );
    }

    #[test]
    fn rejects_a_non_finite_value() {
        let text = with_line(&csv(N_MODELS), 3, "000,bias,0,R1,inf");
        assert_eq!(error(&text), EnsembleError { line: 3, kind: BadValue("inf".into()) });
    }

    #[test]
    fn rejects_a_short_row() {
        let text = with_line(&csv(N_MODELS), 3, "000,bias,0,R1");
        assert_eq!(error(&text), EnsembleError { line: 3, kind: FieldCount(4) });
    }

    #[test]
    fn rejects_a_duplicate_model() {
        let text = csv(N_MODELS);
        let line = 3 + ROWS_PER_MODEL; // first row of model 001
        let row = text.split('\n').nth(line - 1).unwrap().replacen("001,", "000,", 1);
        assert_eq!(
            error(&with_line(&text, line, &row)),
            EnsembleError { line, kind: ModelOutOfOrder { expected: "001".into(), found: "000".into() } }
        );
    }

    #[test]
    fn rejects_49_models() {
        assert!(matches!(error(&csv(49)).kind, TooFewRows { found, .. } if found == 49 * ROWS_PER_MODEL));
    }

    #[test]
    fn rejects_51_models() {
        assert!(matches!(error(&csv(51)).kind, TooManyRows { .. }));
    }

    /// The published file's model 000 is the compiled-in model 000, bit for bit.
    #[test]
    fn model_000_equals_the_compiled_in_parameters() {
        let Some(text) = load_pretrained_fixture("ensemble.csv") else { return };
        let e = Ensemble::parse(&text).expect("the published ensemble must parse");
        let ids: Vec<&str> = e.ids().collect();
        assert_eq!(ids, (0..N_MODELS).map(|m| format!("{m:03}")).collect::<Vec<_>>());
        let (got, want) = (e.model("000").unwrap(), ModelParams::pretrained());
        assert_eq!(bits(&got.bias), bits(&want.bias), "bias");
        assert_eq!(bits(&got.time_const), bits(&want.time_const), "time_const");
        assert_eq!(bits(&got.syn_strength), bits(&want.syn_strength), "syn_strength");
    }

    /// The exporter's reader rounds decimal -> f64 -> f32; Rust rounds
    /// decimal -> f32 once. They must agree on every value of every model,
    /// not just model 000.
    #[test]
    fn every_value_rounds_the_same_once_or_twice() {
        let Some(text) = load_pretrained_fixture("ensemble.csv") else { return };
        for line in text.lines().filter(|l| !l.starts_with('#')).skip(1) {
            let s = line.rsplit(',').next().unwrap();
            let once: f32 = s.parse().unwrap();
            let twice = s.parse::<f64>().unwrap() as f32;
            assert_eq!(once.to_bits(), twice.to_bits(), "{line}");
        }
    }
}
