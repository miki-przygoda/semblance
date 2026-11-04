//! One network's central responses to a battery of stimuli: each cell
//! type's central-column value at every frame of every stimulus.
//!
//! Every response index 0.4.0 computes reads this, and nothing else. It
//! knows nothing about the stimuli beyond their labels and frame counts:
//! the code that describes a battery pairs with it by position.
//!
//! Spec: docs/superpowers/specs/2026-11-04-graded-responses-design.md,
//! section 1.

use std::fmt;

/// Which integrator precision produced a set of responses. The values are
/// stored widened to f64; widening an f32 is exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    F32,
    F64,
}

/// Why a set of responses was refused. Each is an error, never a score.
#[derive(Clone, Debug, PartialEq)]
pub enum ResponseError {
    NoCellTypes,
    RepeatedCellType(String),
    BadDt(f64),
    NoFrames { stimulus: usize },
    Shape { stimulus: usize, expected: usize, found: usize },
    NonFinite { stimulus: usize, frame: usize, cell_type: String },
    EmptyBattery,
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResponseError::NoCellTypes => f.write_str("responses need at least one cell type"),
            ResponseError::RepeatedCellType(t) => write!(f, "cell type {t} is listed twice"),
            ResponseError::BadDt(dt) => write!(f, "dt {dt} is not finite and positive"),
            ResponseError::NoFrames { stimulus } => write!(f, "stimulus {stimulus} has no frames"),
            ResponseError::Shape { stimulus, expected, found } => {
                write!(f, "stimulus {stimulus}: expected {expected} values, found {found}")
            }
            ResponseError::NonFinite { stimulus, frame, cell_type } => {
                write!(f, "stimulus {stimulus}: {cell_type} is not finite at frame {frame}")
            }
            ResponseError::EmptyBattery => f.write_str("a battery with no stimuli has no responses"),
        }
    }
}

impl std::error::Error for ResponseError {}

/// One network's central responses to one battery of stimuli.
#[derive(Clone, Debug, PartialEq)]
pub struct Responses {
    cell_types: Vec<String>,
    dt: f64,
    precision: Precision,
    stimuli: Vec<StimulusResponse>,
}

/// One stimulus's responses: `n_frames` rows of one value per cell type,
/// row-major. Row `i` is the state after `i + 1` steps from the settled
/// state.
#[derive(Clone, Debug, PartialEq)]
pub struct StimulusResponse {
    label: String,
    n_frames: usize,
    n_types: usize,
    values: Vec<f64>,
}

impl Responses {
    /// An empty set for `cell_types`, in the order every frame lists them.
    pub fn new(cell_types: Vec<String>, dt: f64, precision: Precision) -> Result<Self, ResponseError> {
        if cell_types.is_empty() {
            return Err(ResponseError::NoCellTypes);
        }
        for (i, t) in cell_types.iter().enumerate() {
            if cell_types[..i].contains(t) {
                return Err(ResponseError::RepeatedCellType(t.clone()));
            }
        }
        if !(dt.is_finite() && dt > 0.0) {
            return Err(ResponseError::BadDt(dt));
        }
        Ok(Responses { cell_types, dt, precision, stimuli: Vec::new() })
    }

    /// Appends one stimulus: `n_frames` rows of `cell_types().len()` values.
    pub fn push(&mut self, label: String, n_frames: usize, values: Vec<f64>) -> Result<(), ResponseError> {
        let stimulus = self.stimuli.len();
        if n_frames == 0 {
            return Err(ResponseError::NoFrames { stimulus });
        }
        let n_types = self.cell_types.len();
        match n_frames.checked_mul(n_types) {
            Some(expected) if expected == values.len() => {}
            expected => {
                return Err(ResponseError::Shape {
                    stimulus,
                    expected: expected.unwrap_or(usize::MAX),
                    found: values.len(),
                })
            }
        }
        if let Some(i) = values.iter().position(|v| !v.is_finite()) {
            return Err(ResponseError::NonFinite {
                stimulus,
                frame: i / n_types,
                cell_type: self.cell_types[i % n_types].clone(),
            });
        }
        self.stimuli.push(StimulusResponse { label, n_frames, n_types, values });
        Ok(())
    }

    /// The finished set; a battery with no stimuli is refused.
    pub fn finish(self) -> Result<Self, ResponseError> {
        if self.stimuli.is_empty() { Err(ResponseError::EmptyBattery) } else { Ok(self) }
    }

    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    /// The column of cell type `name`, or `None`.
    pub fn type_index(&self, name: &str) -> Option<usize> {
        self.cell_types.iter().position(|t| t == name)
    }

    pub fn dt(&self) -> f64 {
        self.dt
    }

    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// The number of stimuli.
    pub fn len(&self) -> usize {
        self.stimuli.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stimuli.is_empty()
    }

    pub fn stimulus(&self, s: usize) -> Option<&StimulusResponse> {
        self.stimuli.get(s)
    }
}

impl StimulusResponse {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn n_frames(&self) -> usize {
        self.n_frames
    }

    /// Frame `i`: one value per cell type, in `Responses::cell_types` order.
    pub fn frame(&self, i: usize) -> Option<&[f64]> {
        if i >= self.n_frames {
            return None;
        }
        Some(&self.values[i * self.n_types..(i + 1) * self.n_types])
    }

    /// Cell type `t` over every frame, or `None` if there is no column `t`.
    pub fn trace(&self, t: usize) -> Option<impl Iterator<Item = f64> + '_> {
        if t >= self.n_types {
            return None;
        }
        Some(self.values.iter().skip(t).step_by(self.n_types).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types() -> Vec<String> {
        vec!["R1".into(), "T4a".into()]
    }

    /// Two stimuli: 3 frames and 1 frame, two types.
    fn two() -> Responses {
        let mut r = Responses::new(types(), 0.005, Precision::F32).unwrap();
        r.push("a".into(), 3, vec![1.0, 10.0, 2.0, 20.0, 3.0, 30.0]).unwrap();
        r.push("b".into(), 1, vec![-1.0, -2.0]).unwrap();
        r.finish().unwrap()
    }

    #[test]
    fn frames_and_traces_read_back_what_was_pushed() {
        let r = two();
        assert_eq!(r.len(), 2);
        assert_eq!((r.dt(), r.precision()), (0.005, Precision::F32));
        assert_eq!(r.type_index("T4a"), Some(1));
        assert_eq!(r.type_index("T5a"), None);
        let a = r.stimulus(0).unwrap();
        assert_eq!((a.label(), a.n_frames()), ("a", 3));
        assert_eq!(a.frame(1), Some(&[2.0, 20.0][..]));
        assert_eq!(a.frame(3), None);
        assert_eq!(a.trace(1).unwrap().collect::<Vec<_>>(), vec![10.0, 20.0, 30.0]);
        assert_eq!(r.stimulus(1).unwrap().trace(0).unwrap().collect::<Vec<_>>(), vec![-1.0]);
        assert!(r.stimulus(2).is_none());
    }

    #[test]
    fn a_trace_past_the_last_type_is_none() {
        assert!(two().stimulus(0).unwrap().trace(2).is_none());
    }

    #[test]
    fn construction_refuses_bad_cell_types_and_dt() {
        assert_eq!(Responses::new(vec![], 0.005, Precision::F64), Err(ResponseError::NoCellTypes));
        let twice = vec!["R1".to_string(), "R1".to_string()];
        assert_eq!(Responses::new(twice, 0.005, Precision::F64), Err(ResponseError::RepeatedCellType("R1".into())));
        for dt in [0.0, -0.005, f64::NAN, f64::INFINITY] {
            assert!(matches!(Responses::new(types(), dt, Precision::F64), Err(ResponseError::BadDt(_))));
        }
    }

    #[test]
    fn push_refuses_empty_ragged_and_non_finite_stimuli() {
        let mut r = Responses::new(types(), 0.005, Precision::F32).unwrap();
        assert_eq!(r.push("x".into(), 0, vec![]), Err(ResponseError::NoFrames { stimulus: 0 }));
        assert_eq!(
            r.push("x".into(), 2, vec![1.0, 2.0, 3.0]),
            Err(ResponseError::Shape { stimulus: 0, expected: 4, found: 3 })
        );
        assert_eq!(
            r.push("x".into(), usize::MAX, vec![1.0]),
            Err(ResponseError::Shape { stimulus: 0, expected: usize::MAX, found: 1 })
        );
        assert_eq!(
            r.push("x".into(), 2, vec![1.0, 2.0, 3.0, f64::NAN]),
            Err(ResponseError::NonFinite { stimulus: 0, frame: 1, cell_type: "T4a".into() })
        );
        assert!(r.is_empty(), "a refused stimulus is not kept");
    }

    #[test]
    fn an_empty_battery_does_not_finish() {
        let r = Responses::new(types(), 0.005, Precision::F32).unwrap();
        assert_eq!(r.finish(), Err(ResponseError::EmptyBattery));
    }

    #[test]
    fn errors_display_as_registered() {
        let e = ResponseError::NonFinite { stimulus: 4, frame: 9, cell_type: "Mi1".into() };
        assert_eq!(e.to_string(), "stimulus 4: Mi1 is not finite at frame 9");
        let e = ResponseError::Shape { stimulus: 1, expected: 130, found: 129 };
        assert_eq!(e.to_string(), "stimulus 1: expected 130 values, found 129");
        assert_eq!(ResponseError::EmptyBattery.to_string(), "a battery with no stimuli has no responses");
    }
}
