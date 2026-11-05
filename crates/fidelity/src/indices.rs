//! flyvis's four response indices, their time windows and the paper's
//! agreement rules, as pure functions over task 1a's [`Responses`].
//!
//! No files, no simulator, no `semblance-flyvis`: ground truth arrives as
//! arguments. A battery is described by a [`BatteryLayout`], matched to a
//! `Responses` by position. Every malformed input is an [`IndexError`],
//! never a score.
//!
//! Spec: docs/superpowers/specs/2026-11-04-response-indices-design.md,
//! sections 1 and 4.

use std::fmt;

use semblance_sim::response::{Precision, Responses, StimulusResponse};

/// flyvis's `t_pre` for the flash and moving-edge batteries, in seconds.
pub const T_PRE: f64 = 1.0;
/// The flash battery's `t_stim`, in seconds: the FRI window's end.
pub const FLASH_T_STIM: f64 = 1.0;
/// The only flash radius the FRI reads (task 0: radius -1 never feeds it).
pub const FRI_RADIUS: i32 = 6;
/// flyvis's moving-edge offsets `(start, end)`, `peak_responses`' defaults.
pub const EDGE_OFFSETS: (i32, i32) = (-10, 11);
/// Angles per (intensity, speed) in the edge reductions: 0, 30, ..., 330.
pub const N_ANGLES: usize = 12;
/// Speeds in the edge reductions.
pub const N_SPEEDS: usize = 6;
/// The most frames a window search will scan.
const MAX_WINDOW_FRAMES: f64 = 1e7;

/// Why an index could not be computed. Each is a refusal, never a score.
#[derive(Clone, Debug, PartialEq)]
pub enum IndexError {
    /// The layout and the responses hold different numbers of stimuli.
    CountMismatch { layout: usize, responses: usize },
    /// The layout's label at a position is not the responses' label there.
    LabelMismatch { stimulus: usize, layout: String, responses: String },
    /// A window ends at or past its stimulus's last frame.
    WindowPastEnd { stimulus: usize, last: usize, n_frames: usize },
    /// A window whose first frame is after its last.
    BadWindow { first: usize, last: usize },
    /// A stimulus no flyvis battery could hold (a NaN speed, say).
    BadStimulus { stimulus: usize, reason: String },
    /// A dt that is not finite and positive, or too small to scan.
    BadDt(f64),
    /// A numeric argument out of its range.
    BadParameter { what: &'static str, value: f64 },
    /// A cell type the computation needs is not in its input.
    MissingCellType(String),
    /// A value, input or result, that is NaN or infinite.
    NonFinite(String),
    /// A value tagged as from an f32 run that is not an f32.
    NotF32(String),
    /// An input with nothing in it.
    Empty(&'static str),
    /// The wrong stimuli for a reduction: angles, speeds, intensities or the
    /// FRI's flashes.
    BatteryShape(String),
    /// An input of the wrong length.
    Length { what: &'static str, expected: usize, found: usize },
    /// A known (ground-truth) value the rule cannot use.
    BadKnown { cell_type: String, reason: String },
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IndexError::CountMismatch { layout, responses } => {
                write!(f, "the layout has {layout} stimuli, the responses {responses}")
            }
            IndexError::LabelMismatch { stimulus, layout, responses } => {
                write!(f, "stimulus {stimulus}: the layout's label is {layout:?}, the responses' {responses:?}")
            }
            IndexError::WindowPastEnd { stimulus, last, n_frames } => {
                write!(f, "stimulus {stimulus}: its window ends at frame {last}, past its {n_frames} frames")
            }
            IndexError::BadWindow { first, last } => {
                write!(f, "a window from frame {first} to frame {last} ends before it starts")
            }
            IndexError::BadStimulus { stimulus, reason } => write!(f, "stimulus {stimulus}: {reason}"),
            IndexError::BadDt(dt) => write!(f, "dt {dt} is not finite and positive"),
            IndexError::BadParameter { what, value } => write!(f, "{what} {value} is out of range"),
            IndexError::MissingCellType(t) => write!(f, "cell type {t} is not in the responses"),
            IndexError::NonFinite(what) => write!(f, "{what} is not finite"),
            IndexError::NotF32(what) => write!(f, "{what} is not an f32, in an f32 run"),
            IndexError::Empty(what) => write!(f, "{what} is empty"),
            IndexError::BatteryShape(m) => write!(f, "battery shape: {m}"),
            IndexError::Length { what, expected, found } => write!(f, "{what}: expected {expected}, found {found}"),
            IndexError::BadKnown { cell_type, reason } => write!(f, "known value for {cell_type}: {reason}"),
        }
    }
}

impl std::error::Error for IndexError {}

/// One stimulus of a battery, as flyvis's datasets describe it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stimulus {
    /// A flash: radius -1 is full field; intensity 0 is OFF, 1 is ON.
    Flash { radius: i32, intensity: u8 },
    /// A moving edge: direction in degrees, speed in columns per second;
    /// intensity 0 is OFF, 1 is ON.
    Edge { angle_deg: u16, speed: f64, intensity: u8 },
}

/// An inclusive frame window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    first: usize,
    last: usize,
}

impl Window {
    /// The frames `first..=last`; refused if `first > last`.
    pub fn new(first: usize, last: usize) -> Result<Window, IndexError> {
        if first > last {
            return Err(IndexError::BadWindow { first, last });
        }
        Ok(Window { first, last })
    }

    pub fn first(&self) -> usize {
        self.first
    }

    pub fn last(&self) -> usize {
        self.last
    }

    /// How many frames the window holds.
    pub fn n_frames(&self) -> usize {
        self.last - self.first + 1
    }
}

/// A battery's stimuli, in battery order: each one's label, what it is, and
/// the frames its index reads. Matched to a [`Responses`] by position.
#[derive(Clone, Debug, PartialEq)]
pub struct BatteryLayout {
    stimuli: Vec<(String, Stimulus, Window)>,
}

impl BatteryLayout {
    /// Refuses an empty layout, and an edge whose speed is not finite and
    /// positive or whose angle is 360 or more.
    pub fn new(stimuli: Vec<(String, Stimulus, Window)>) -> Result<Self, IndexError> {
        if stimuli.is_empty() {
            return Err(IndexError::Empty("battery layout"));
        }
        for (s, (_, stimulus, _)) in stimuli.iter().enumerate() {
            if let Stimulus::Edge { angle_deg, speed, .. } = *stimulus {
                if !(speed.is_finite() && speed > 0.0) {
                    let reason = format!("edge speed {speed} is not finite and positive");
                    return Err(IndexError::BadStimulus { stimulus: s, reason });
                }
                if angle_deg >= 360 {
                    let reason = format!("edge angle {angle_deg} is not below 360");
                    return Err(IndexError::BadStimulus { stimulus: s, reason });
                }
            }
        }
        Ok(BatteryLayout { stimuli })
    }

    /// The number of stimuli.
    pub fn len(&self) -> usize {
        self.stimuli.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stimuli.is_empty()
    }

    pub fn stimuli(&self) -> &[(String, Stimulus, Window)] {
        &self.stimuli
    }

    /// Whether this layout describes `r`: the same number of stimuli, the
    /// same label at every position, and every window inside its stimulus's
    /// frames. Every index that reads responses runs this first.
    pub fn check(&self, r: &Responses) -> Result<(), IndexError> {
        let mismatch = IndexError::CountMismatch { layout: self.stimuli.len(), responses: r.len() };
        if self.stimuli.len() != r.len() {
            return Err(mismatch);
        }
        for (s, (label, _, w)) in self.stimuli.iter().enumerate() {
            let got = r.stimulus(s).ok_or_else(|| mismatch.clone())?;
            if got.label() != label {
                return Err(IndexError::LabelMismatch {
                    stimulus: s,
                    layout: label.clone(),
                    responses: got.label().to_string(),
                });
            }
            if w.last >= got.n_frames() {
                return Err(IndexError::WindowPastEnd { stimulus: s, last: w.last, n_frames: got.n_frames() });
            }
        }
        Ok(())
    }
}

/// The FRI's frames: flyvis's query `-dt <= time <= t_stim` on its time
/// axis `frame * dt - t_pre`, evaluated as flyvis evaluates it. At dt 1/200
/// float rounding drops frame 199, so the window is frames 200..=400.
pub fn fri_window(dt: f64) -> Result<Window, IndexError> {
    mask_window(dt, T_PRE, FLASH_T_STIM, |t| -dt <= t && t <= FLASH_T_STIM)
}

/// A moving edge's frames: flyvis's `time_window` mask for `speed` (columns
/// per second) and `offsets`, its float expressions in its order, unsimplified.
pub fn edge_window(speed: f64, dt: f64, t_pre: f64, offsets: (i32, i32)) -> Result<Window, IndexError> {
    if !(speed.is_finite() && speed > 0.0) {
        return Err(IndexError::BadParameter { what: "edge speed", value: speed });
    }
    if !(t_pre.is_finite() && t_pre >= 0.0) {
        return Err(IndexError::BadParameter { what: "t_pre", value: t_pre });
    }
    let (start, end) = offsets;
    let from_degree = start as f64 * 2.25;
    let to_degree = (end as i64 - 1) as f64 * 2.25;
    let from_column = from_degree / 5.8;
    let mut to_column = to_degree / 5.8;
    let start_in_columns = start as f64 * 2.25 / 5.8;
    to_column += 2.25 / 5.8;
    let t_start = (start_in_columns.abs() - from_column.abs()) / speed;
    let t_end = t_start + (to_column - from_column) / speed;
    mask_window(dt, t_pre, t_end, |t| t >= t_start && t <= t_end)
}

/// The first and last frame whose flyvis time `f * dt - t_pre` satisfies
/// `keep`, scanning up to the frame just past `t_last`. Both masks are
/// intervals of a monotone time axis, so the frames between are kept too.
fn mask_window(dt: f64, t_pre: f64, t_last: f64, keep: impl Fn(f64) -> bool) -> Result<Window, IndexError> {
    if !(dt.is_finite() && dt > 0.0) {
        return Err(IndexError::BadDt(dt));
    }
    let frames = ((t_last + t_pre) / dt).ceil() + 2.0;
    if !(frames.is_finite() && (0.0..=MAX_WINDOW_FRAMES).contains(&frames)) {
        return Err(IndexError::BadDt(dt));
    }
    let (mut first, mut last) = (None, None);
    for f in 0..=frames as usize {
        if keep(f as f64 * dt - t_pre) {
            first.get_or_insert(f);
            last = Some(f);
        }
    }
    match (first, last) {
        (Some(a), Some(b)) => Window::new(a, b),
        _ => Err(IndexError::Empty("time window")),
    }
}

/// Per cell type, in `Responses` order, the FRI's two shifted window peaks
/// `[off, on]`, with the precision `fri` emulates.
#[derive(Clone, Debug, PartialEq)]
pub struct FriPeaks {
    cell_types: Vec<String>,
    precision: Precision,
    peaks: Vec<[f64; 2]>,
}

impl FriPeaks {
    /// Refuses no peaks, a count that is not one pair per cell type, a
    /// non-finite peak, and (for an f32 run) a peak that is not an f32.
    pub fn new(cell_types: Vec<String>, precision: Precision, peaks: Vec<[f64; 2]>) -> Result<Self, IndexError> {
        if peaks.is_empty() {
            return Err(IndexError::Empty("FRI peaks"));
        }
        if peaks.len() != cell_types.len() {
            return Err(IndexError::Length { what: "FRI peaks", expected: cell_types.len(), found: peaks.len() });
        }
        for (t, pair) in peaks.iter().enumerate() {
            for &v in pair {
                if !v.is_finite() {
                    return Err(IndexError::NonFinite(format!("the FRI peak of {}", cell_types[t])));
                }
                if precision == Precision::F32 && v as f32 as f64 != v {
                    return Err(IndexError::NotF32(format!("the FRI peak {v} of {}", cell_types[t])));
                }
            }
        }
        Ok(FriPeaks { cell_types, precision, peaks })
    }

    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// `[off, on]` per cell type.
    pub fn peaks(&self) -> &[[f64; 2]] {
        &self.peaks
    }
}

/// flyvis's FRI peaks (`flash_responses.py`): over the radius-6 flashes'
/// shared window, each cell type's minimum over the window and both
/// intensities, `+= abs(minimum)`, then each intensity's maximum. An f32
/// run does it in f32, an f64 run in f64. An f32 run's window value that is
/// not an f32 is refused (`NotF32`).
pub fn fri_peaks(r: &Responses, b: &BatteryLayout) -> Result<FriPeaks, IndexError> {
    b.check(r)?;
    let (off, on, w) = fri_pair(b)?;
    let missing = || IndexError::CountMismatch { layout: b.len(), responses: r.len() };
    let (s_off, s_on) = (r.stimulus(off).ok_or_else(missing)?, r.stimulus(on).ok_or_else(missing)?);
    let mut peaks = Vec::with_capacity(r.cell_types().len());
    let f32_run = r.precision() == Precision::F32;
    for t in 0..r.cell_types().len() {
        let (x, y) = (window_trace(s_off, t, w), window_trace(s_on, t, w));
        // Responses does not check an f32 run's values: refuse one that is
        // not an f32 rather than round it silently.
        if let Some(&v) = x.iter().chain(&y).find(|&&v| f32_run && v as f32 as f64 != v) {
            return Err(IndexError::NotF32(format!("{}'s value {v} in the FRI window", r.cell_types()[t])));
        }
        peaks.push(match r.precision() {
            Precision::F32 => shifted_peaks_f32(&x, &y),
            Precision::F64 => shifted_peaks_f64(&x, &y),
        });
    }
    FriPeaks::new(r.cell_types().to_vec(), r.precision(), peaks)
}

/// The flash response index per cell type, `(on - off) / (on + off + 1e-16)`,
/// in numpy's dtypes: for an f32 run an f32 numerator and sum, the f64
/// epsilon add and divide, stored f32; for an f64 run all f64.
pub fn fri(p: &FriPeaks) -> Result<Vec<f64>, IndexError> {
    let mut out = Vec::with_capacity(p.peaks.len());
    for (t, &[off, on]) in p.peaks.iter().enumerate() {
        let v = match p.precision {
            Precision::F32 => {
                let (on, off) = (on as f32, off as f32);
                let num = on - off;
                let den = (on + off) as f64 + 1e-16;
                ((num as f64) / den) as f32 as f64
            }
            Precision::F64 => (on - off) / (on + off + 1e-16),
        };
        if !v.is_finite() {
            return Err(IndexError::NonFinite(format!("the FRI of {}", p.cell_types[t])));
        }
        out.push(v);
    }
    Ok(out)
}

/// The OFF and ON radius-6 flashes' positions and their shared window.
fn fri_pair(b: &BatteryLayout) -> Result<(usize, usize, Window), IndexError> {
    let mut found: [Option<(usize, Window)>; 2] = [None, None];
    for (s, (_, stimulus, w)) in b.stimuli().iter().enumerate() {
        if let Stimulus::Flash { radius: FRI_RADIUS, intensity } = *stimulus {
            let slot = found.get_mut(intensity as usize).ok_or_else(|| {
                IndexError::BatteryShape(format!("stimulus {s}: a radius-{FRI_RADIUS} flash of intensity {intensity}, not 0 or 1"))
            })?;
            if slot.is_some() {
                return Err(IndexError::BatteryShape(format!(
                    "stimulus {s}: a second radius-{FRI_RADIUS} flash of intensity {intensity}"
                )));
            }
            *slot = Some((s, *w));
        }
    }
    match found {
        [Some((off, a)), Some((on, b))] if a == b => Ok((off, on, a)),
        [Some(_), Some(_)] => Err(IndexError::BatteryShape(format!("the radius-{FRI_RADIUS} flashes have different windows"))),
        _ => Err(IndexError::BatteryShape(format!(
            "the FRI needs one radius-{FRI_RADIUS} flash of intensity 0 and one of intensity 1"
        ))),
    }
}

/// Cell type `t`'s values over window `w` (checked against the stimulus by
/// `BatteryLayout::check`).
fn window_trace(s: &StimulusResponse, t: usize, w: Window) -> Vec<f64> {
    s.trace(t).map(|tr| tr.skip(w.first()).take(w.n_frames()).collect()).unwrap_or_default()
}

fn shifted_peaks_f32(off: &[f64], on: &[f64]) -> [f64; 2] {
    let shift = off.iter().chain(on).map(|&v| v as f32).fold(f32::INFINITY, f32::min).abs();
    let peak = |vals: &[f64]| vals.iter().map(|&v| v as f32 + shift).fold(f32::NEG_INFINITY, f32::max) as f64;
    [peak(off), peak(on)]
}

fn shifted_peaks_f64(off: &[f64], on: &[f64]) -> [f64; 2] {
    let shift = off.iter().chain(on).copied().fold(f64::INFINITY, f64::min).abs();
    let peak = |vals: &[f64]| vals.iter().map(|&v| v + shift).fold(f64::NEG_INFINITY, f64::max);
    [peak(off), peak(on)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use semblance_sim::response::{Precision, Responses};

    /// Responses over `types` at dt 1/200: one stimulus per `(label,
    /// n_frames)`, each value `fill(stimulus, frame, type)`.
    fn responses(
        types: &[&str],
        precision: Precision,
        stimuli: &[(&str, usize)],
        fill: impl Fn(usize, usize, usize) -> f64,
    ) -> Responses {
        let names = types.iter().map(|t| t.to_string()).collect();
        let mut r = Responses::new(names, 1.0 / 200.0, precision).unwrap();
        for (s, &(label, n)) in stimuli.iter().enumerate() {
            let mut values = Vec::with_capacity(n * types.len());
            for f in 0..n {
                for t in 0..types.len() {
                    values.push(fill(s, f, t));
                }
            }
            r.push(label.to_string(), n, values).unwrap();
        }
        r.finish().unwrap()
    }

    fn window(first: usize, last: usize) -> Window {
        Window::new(first, last).unwrap()
    }

    fn entry(label: &str, stimulus: Stimulus, w: Window) -> (String, Stimulus, Window) {
        (label.to_string(), stimulus, w)
    }

    #[test]
    fn the_fri_window_is_flyviss_query_frames_200_to_400_at_1_200() {
        assert_eq!(fri_window(1.0 / 200.0), Ok(window(200, 400)));
        assert_eq!(fri_window(1.0 / 50.0), Ok(window(50, 100)));
    }

    #[test]
    fn the_edge_windows_are_flyviss_masks() {
        let table = [(2.4, 878), (4.8, 539), (9.7, 367), (13.0, 325), (19.0, 285), (25.0, 265)];
        for (speed, last) in table {
            assert_eq!(edge_window(speed, 1.0 / 200.0, T_PRE, EDGE_OFFSETS), Ok(window(200, last)), "speed {speed}");
        }
        // evaluated from flyvis's expressions, not a table: dt 1/50, and
        // speeds the battery does not use
        let table = [(2.4, 219), (4.8, 134), (9.7, 91), (13.0, 81), (19.0, 71), (25.0, 66)];
        for (speed, last) in table {
            assert_eq!(edge_window(speed, 1.0 / 50.0, T_PRE, EDGE_OFFSETS), Ok(window(50, last)), "speed {speed} at 1/50");
        }
        assert_eq!(edge_window(5.0, 1.0 / 200.0, T_PRE, EDGE_OFFSETS), Ok(window(200, 525)));
        assert_eq!(edge_window(7.0, 1.0 / 200.0, T_PRE, EDGE_OFFSETS), Ok(window(200, 432)));
    }

    #[test]
    fn windows_refuse_a_bad_dt_speed_or_t_pre() {
        for dt in [0.0, -0.005, f64::NAN, f64::INFINITY, 1e-12] {
            assert!(matches!(fri_window(dt), Err(IndexError::BadDt(_))), "dt {dt}");
            assert!(matches!(edge_window(19.0, dt, T_PRE, EDGE_OFFSETS), Err(IndexError::BadDt(_))), "dt {dt}");
        }
        for speed in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let e = edge_window(speed, 0.005, T_PRE, EDGE_OFFSETS);
            assert!(matches!(e, Err(IndexError::BadParameter { what: "edge speed", .. })), "speed {speed}: {e:?}");
        }
        let e = edge_window(19.0, 0.005, f64::NAN, EDGE_OFFSETS);
        assert!(matches!(e, Err(IndexError::BadParameter { what: "t_pre", .. })), "{e:?}");
    }

    #[test]
    fn a_window_that_ends_before_it_starts_is_refused() {
        assert_eq!(Window::new(5, 4), Err(IndexError::BadWindow { first: 5, last: 4 }));
        let w = window(3, 3);
        assert_eq!((w.first(), w.last(), w.n_frames()), (3, 3, 1));
    }

    #[test]
    fn a_layout_refuses_no_stimuli_and_impossible_edges() {
        assert_eq!(BatteryLayout::new(vec![]), Err(IndexError::Empty("battery layout")));
        for (speed, angle) in [(f64::NAN, 0), (0.0, 0), (-2.4, 0), (f64::INFINITY, 0), (2.4, 360)] {
            let e = BatteryLayout::new(vec![entry("e", Stimulus::Edge { angle_deg: angle, speed, intensity: 1 }, window(0, 0))]);
            assert!(matches!(e, Err(IndexError::BadStimulus { stimulus: 0, .. })), "speed {speed}, angle {angle}: {e:?}");
        }
    }

    #[test]
    fn check_accepts_a_matching_layout_and_refuses_every_mismatch() {
        let r = responses(&["A", "B"], Precision::F64, &[("off", 4), ("on", 4)], |_, f, _| f as f64);
        let flash = |intensity| Stimulus::Flash { radius: 6, intensity };
        let good = BatteryLayout::new(vec![entry("off", flash(0), window(1, 3)), entry("on", flash(1), window(1, 3))]).unwrap();
        assert_eq!(good.check(&r), Ok(()));
        assert_eq!((good.len(), good.is_empty(), good.stimuli()[1].0.as_str()), (2, false, "on"));

        // Chore defect 2's analogue: a battery whose shape disagrees with the
        // responses is an error, never a score.
        let short = BatteryLayout::new(vec![entry("off", flash(0), window(1, 3))]).unwrap();
        assert_eq!(short.check(&r), Err(IndexError::CountMismatch { layout: 1, responses: 2 }));
        let renamed = BatteryLayout::new(vec![entry("off", flash(0), window(1, 3)), entry("ON", flash(1), window(1, 3))]).unwrap();
        assert_eq!(
            renamed.check(&r),
            Err(IndexError::LabelMismatch { stimulus: 1, layout: "ON".into(), responses: "on".into() })
        );
        let long = BatteryLayout::new(vec![entry("off", flash(0), window(1, 4)), entry("on", flash(1), window(1, 3))]).unwrap();
        assert_eq!(long.check(&r), Err(IndexError::WindowPastEnd { stimulus: 0, last: 4, n_frames: 4 }));
    }

    #[test]
    fn errors_display_as_registered() {
        let cases = [
            (IndexError::CountMismatch { layout: 148, responses: 147 }, "the layout has 148 stimuli, the responses 147"),
            (
                IndexError::LabelMismatch { stimulus: 2, layout: "a".into(), responses: "b".into() },
                "stimulus 2: the layout's label is \"a\", the responses' \"b\"",
            ),
            (
                IndexError::WindowPastEnd { stimulus: 3, last: 878, n_frames: 739 },
                "stimulus 3: its window ends at frame 878, past its 739 frames",
            ),
            (IndexError::BadWindow { first: 5, last: 4 }, "a window from frame 5 to frame 4 ends before it starts"),
            (IndexError::BadDt(0.0), "dt 0 is not finite and positive"),
            (IndexError::BadParameter { what: "edge speed", value: -1.0 }, "edge speed -1 is out of range"),
            (IndexError::MissingCellType("T4a".into()), "cell type T4a is not in the responses"),
            (IndexError::NonFinite("the FRI of Mi1".into()), "the FRI of Mi1 is not finite"),
            (IndexError::NotF32("the FRI peak of Mi1".into()), "the FRI peak of Mi1 is not an f32, in an f32 run"),
            (IndexError::Empty("battery layout"), "battery layout is empty"),
            (IndexError::BatteryShape("no edge at intensity 1".into()), "battery shape: no edge at intensity 1"),
            (IndexError::Length { what: "FRI", expected: 65, found: 64 }, "FRI: expected 65, found 64"),
            (
                IndexError::BadKnown { cell_type: "Mi1".into(), reason: "polarity 0 is not +1 or -1".into() },
                "known value for Mi1: polarity 0 is not +1 or -1",
            ),
        ];
        for (e, want) in cases {
            assert_eq!(e.to_string(), want);
        }
        let e = IndexError::BadStimulus { stimulus: 7, reason: "edge angle 360 is not below 360".into() };
        assert_eq!(e.to_string(), "stimulus 7: edge angle 360 is not below 360");
    }

    /// The FRI battery used below: radius -1 and radius 6 flashes, both
    /// intensities, 5 frames each, the window frames 1..=3.
    fn fri_layout() -> BatteryLayout {
        let flash = |radius, intensity| Stimulus::Flash { radius, intensity };
        BatteryLayout::new(vec![
            entry("full off", flash(-1, 0), window(1, 3)),
            entry("off", flash(6, 0), window(1, 3)),
            entry("on", flash(6, 1), window(1, 3)),
            entry("full on", flash(-1, 1), window(1, 3)),
        ])
        .unwrap()
    }

    const FRI_STIMULI: [(&str, usize); 4] = [("full off", 5), ("off", 5), ("on", 5), ("full on", 5)];

    #[test]
    fn fri_peaks_shift_by_the_minimum_over_both_intensities() {
        // A: OFF window -2, 1, 0 and ON window -1, 3, 2, so min -2, shift +2.
        // B: OFF window 1, 2, 3 and ON window 2, 4, 1, so min 1, shift +1:
        // flyvis adds |min|, it does not subtract min.
        let a = [[9.0, -2.0, 1.0, 0.0, 9.0], [9.0, -1.0, 3.0, 2.0, 9.0]];
        let b = [[0.0, 1.0, 2.0, 3.0, 0.0], [0.0, 2.0, 4.0, 1.0, 0.0]];
        let r = responses(&["A", "B"], Precision::F64, &FRI_STIMULI, |s, f, t| match s {
            1 | 2 => [a, b][t][s - 1][f],
            _ => -100.0, // the radius -1 flashes never feed the FRI
        });
        let p = fri_peaks(&r, &fri_layout()).unwrap();
        assert_eq!(p.peaks(), &[[3.0, 5.0], [4.0, 5.0]]);
        assert_eq!((p.precision(), p.cell_types()), (Precision::F64, &["A".to_string(), "B".to_string()][..]));
        let fri = fri(&p).unwrap();
        assert_eq!(fri, vec![(5.0 - 3.0) / (5.0 + 3.0 + 1e-16), (5.0 - 4.0) / (5.0 + 4.0 + 1e-16)]);
    }

    #[test]
    fn an_f32_runs_fri_follows_numpys_dtypes_step_by_step() {
        // OFF peak v = 1e-8 (an f32), ON peak 1. In f32, 1 - v and 1 + v are
        // both 1.0, and 1.0 / (1.0 + 1e-16) is 1.0; in f64 they are not.
        let v = 1e-8f32 as f64;
        let fill = |s: usize, f: usize, _t: usize| match (s, f) {
            (1, 2) => v,
            (2, 2) => 1.0,
            _ => 0.0,
        };
        let f32_run = responses(&["A"], Precision::F32, &FRI_STIMULI, fill);
        let f64_run = responses(&["A"], Precision::F64, &FRI_STIMULI, fill);
        let x = fri(&fri_peaks(&f32_run, &fri_layout()).unwrap()).unwrap();
        let y = fri(&fri_peaks(&f64_run, &fri_layout()).unwrap()).unwrap();
        assert_eq!(x, vec![1.0]);
        assert!(y[0] < 1.0, "{}", y[0]);
        assert_eq!(y, vec![(1.0 - v) / (1.0 + v + 1e-16)]);
        // every stored f32-run FRI is an f32
        assert_eq!(x[0] as f32 as f64, x[0]);
    }

    #[test]
    fn an_f32_runs_epsilon_is_added_in_f64_and_the_shift_in_f32() {
        // Peaks [off 0, on 2^-54]: only numpy's promotion gives this value.
        // An f32 epsilon gives 0.3569593131542206, no epsilon 1.0, and all
        // f64 without the final f32 rounding 0.35695929707772656 (the f64
        // run's value).
        let tiny = 2f64.powi(-54);
        let p = FriPeaks::new(vec!["A".into()], Precision::F32, vec![[0.0, tiny]]).unwrap();
        assert_eq!(fri(&p).unwrap(), vec![0.3569592833518982]);
        let p = FriPeaks::new(vec!["A".into()], Precision::F64, vec![[0.0, tiny]]).unwrap();
        assert_eq!(fri(&p).unwrap(), vec![0.35695929707772656]);
        // A nonzero shift: OFF window -1, 0.25, 0 and ON window 0, 0.5, 0, so
        // min -1, shift +1 in f32: peaks [1.25, 1.5].
        let fill = |s: usize, f: usize, _t: usize| match (s, f) {
            (1, 1) => -1.0,
            (1, 2) => 0.25,
            (2, 2) => 0.5,
            _ => 0.0,
        };
        let r = responses(&["A"], Precision::F32, &FRI_STIMULI, fill);
        let p = fri_peaks(&r, &fri_layout()).unwrap();
        assert_eq!(p.peaks(), &[[1.25, 1.5]]);
        assert_eq!(fri(&p).unwrap(), vec![((0.25f32 as f64) / (2.75f32 as f64 + 1e-16)) as f32 as f64]);
    }

    #[test]
    fn fri_peaks_refuse_an_f32_run_holding_non_f32_values() {
        // Responses does not check an f32 run's values; fri_peaks must not
        // round them silently.
        let r = responses(&["A"], Precision::F32, &FRI_STIMULI, |_, _, _| 0.1);
        assert!(matches!(fri_peaks(&r, &fri_layout()), Err(IndexError::NotF32(_))));
    }

    #[test]
    fn the_fri_needs_exactly_one_radius_6_flash_per_intensity_in_one_window() {
        let r = responses(&["A"], Precision::F64, &FRI_STIMULI, |_, _, _| 0.5);
        let flash = |radius, intensity| Stimulus::Flash { radius, intensity };
        let with = |second: Stimulus, w: Window| {
            BatteryLayout::new(vec![
                entry("full off", flash(-1, 0), window(1, 3)),
                entry("off", flash(6, 0), window(1, 3)),
                entry("on", second, w),
                entry("full on", flash(-1, 1), window(1, 3)),
            ])
            .unwrap()
        };
        for (second, w) in [
            (flash(-1, 1), window(1, 3)), // no ON radius-6 flash
            (flash(6, 0), window(1, 3)),  // two OFF ones
            (flash(6, 2), window(1, 3)),  // an intensity that is neither
            (flash(6, 1), window(1, 2)),  // two windows
        ] {
            let e = fri_peaks(&r, &with(second, w));
            assert!(matches!(e, Err(IndexError::BatteryShape(_))), "{second:?} {w:?}: {e:?}");
        }
    }

    #[test]
    fn fri_peaks_refuse_responses_the_layout_does_not_describe() {
        // chore defect 2's analogue on the new path
        let r = responses(&["A"], Precision::F64, &FRI_STIMULI[..3], |_, _, _| 0.5);
        assert_eq!(fri_peaks(&r, &fri_layout()), Err(IndexError::CountMismatch { layout: 4, responses: 3 }));
    }

    #[test]
    fn fri_peaks_built_by_hand_are_checked() {
        let names = || vec!["A".to_string(), "B".to_string()];
        assert_eq!(FriPeaks::new(vec![], Precision::F64, vec![]), Err(IndexError::Empty("FRI peaks")));
        assert_eq!(
            FriPeaks::new(names(), Precision::F64, vec![[0.0, 1.0]]),
            Err(IndexError::Length { what: "FRI peaks", expected: 2, found: 1 })
        );
        assert!(matches!(
            FriPeaks::new(names(), Precision::F64, vec![[0.0, 1.0], [f64::NAN, 1.0]]),
            Err(IndexError::NonFinite(_))
        ));
        assert!(matches!(
            FriPeaks::new(names(), Precision::F32, vec![[0.0, 1.0], [0.1, 1.0]]),
            Err(IndexError::NotF32(_))
        ));
        assert!(FriPeaks::new(names(), Precision::F32, vec![[0.0, 1.0], [0.5, 1.0]]).is_ok());
    }

    #[test]
    fn an_fri_that_is_not_finite_is_refused() {
        // off = -1e-16, on = 0: the numerator is 1e-16
        let p = FriPeaks::new(vec!["A".into()], Precision::F64, vec![[-1e-16, 0.0]]).unwrap();
        // on + off + 1e-16 is 0, so the quotient is not finite
        assert!(matches!(fri(&p), Err(IndexError::NonFinite(_))), "{:?}", fri(&p));
    }
}
