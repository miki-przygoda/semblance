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

/// Degrees to radians as numpy's `deg2rad` multiplies: `x * (pi / 180)`.
const DEG: f64 = std::f64::consts::PI / 180.0;

/// Per edge stimulus (in layout order) and cell type (in `Responses`
/// order), flyvis's edge peak: `max(0, max over the window of the raw
/// central voltage)`, with the precision of the run it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgePeaks {
    cell_types: Vec<String>,
    precision: Precision,
    peaks: Vec<Vec<f64>>,
}

impl EdgePeaks {
    /// Refuses no rows, a row that is not one value per cell type, a
    /// non-finite value, and (for an f32 run) a value that is not an f32.
    pub fn new(cell_types: Vec<String>, precision: Precision, peaks: Vec<Vec<f64>>) -> Result<Self, IndexError> {
        if peaks.is_empty() {
            return Err(IndexError::Empty("edge peaks"));
        }
        for (e, row) in peaks.iter().enumerate() {
            if row.len() != cell_types.len() {
                return Err(IndexError::Length { what: "edge peak row", expected: cell_types.len(), found: row.len() });
            }
            for (t, &v) in row.iter().enumerate() {
                if !v.is_finite() {
                    return Err(IndexError::NonFinite(format!("edge {e}'s peak of {}", cell_types[t])));
                }
                if precision == Precision::F32 && v as f32 as f64 != v {
                    return Err(IndexError::NotF32(format!("edge {e}'s peak {v} of {}", cell_types[t])));
                }
            }
        }
        Ok(EdgePeaks { cell_types, precision, peaks })
    }

    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// `[edge][cell type]`, the layout's edges in layout order.
    pub fn peaks(&self) -> &[Vec<f64>] {
        &self.peaks
    }
}

/// flyvis's edge peaks: frames outside each edge's window zeroed, every
/// value clipped at 0, the maximum taken; no baseline, no norm. A zero peak
/// is +0.0. An f32 run's window value that is not an f32 is refused.
pub fn edge_peaks(r: &Responses, b: &BatteryLayout) -> Result<EdgePeaks, IndexError> {
    b.check(r)?;
    let n = r.cell_types().len();
    let f32_run = r.precision() == Precision::F32;
    let mut rows = Vec::new();
    for (s, (_, stimulus, w)) in b.stimuli().iter().enumerate() {
        if !matches!(stimulus, Stimulus::Edge { .. }) {
            continue;
        }
        let sr = r.stimulus(s).ok_or(IndexError::CountMismatch { layout: b.len(), responses: r.len() })?;
        let mut row = vec![0.0f64; n];
        for f in w.first()..=w.last() {
            let frame = sr.frame(f).ok_or(IndexError::WindowPastEnd { stimulus: s, last: w.last(), n_frames: sr.n_frames() })?;
            for (t, (peak, &v)) in row.iter_mut().zip(frame).enumerate() {
                // every window value, not only the maximum
                if f32_run && v as f32 as f64 != v {
                    return Err(IndexError::NotF32(format!("{}'s value {v} in stimulus {s}'s window", r.cell_types()[t])));
                }
                if v > *peak {
                    *peak = v;
                }
            }
        }
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(IndexError::Empty("edge stimuli"));
    }
    EdgePeaks::new(r.cell_types().to_vec(), r.precision(), rows)
}

/// The edge reductions' view of a layout: for each intensity (0, 1), each
/// speed (ascending) and each angle (0, 30, ..., 330), the row of
/// `EdgePeaks` holding it.
struct EdgeGrid {
    speeds: [f64; N_SPEEDS],
    rows: [[[usize; N_ANGLES]; N_SPEEDS]; 2],
}

fn edge_grid(p: &EdgePeaks, b: &BatteryLayout) -> Result<EdgeGrid, IndexError> {
    let edges: Vec<(u16, f64, u8)> = b
        .stimuli()
        .iter()
        .filter_map(|(_, s, _)| match *s {
            Stimulus::Edge { angle_deg, speed, intensity } => Some((angle_deg, speed, intensity)),
            Stimulus::Flash { .. } => None,
        })
        .collect();
    if edges.is_empty() {
        return Err(IndexError::Empty("edge stimuli"));
    }
    if edges.len() != p.peaks.len() {
        return Err(IndexError::Length { what: "edge peaks", expected: edges.len(), found: p.peaks.len() });
    }
    let mut speeds: Vec<f64> = edges.iter().map(|e| e.1).collect();
    speeds.sort_by(f64::total_cmp);
    speeds.dedup();
    if speeds.len() != N_SPEEDS {
        return Err(IndexError::BatteryShape(format!("the edge reductions need {N_SPEEDS} speeds, found {}", speeds.len())));
    }
    let mut rows = [[[usize::MAX; N_ANGLES]; N_SPEEDS]; 2];
    for (k, &(angle, speed, intensity)) in edges.iter().enumerate() {
        if intensity > 1 {
            return Err(IndexError::BatteryShape(format!("edge {k}: intensity {intensity} is not 0 or 1")));
        }
        if angle % 30 != 0 {
            return Err(IndexError::BatteryShape(format!("edge {k}: angle {angle} is not one of 0, 30, ..., 330")));
        }
        let s = speeds
            .iter()
            .position(|&v| v == speed)
            .ok_or_else(|| IndexError::BatteryShape(format!("edge {k}: speed {speed} is not listed")))?;
        let slot = &mut rows[intensity as usize][s][(angle / 30) as usize];
        if *slot != usize::MAX {
            return Err(IndexError::BatteryShape(format!(
                "edge {k} repeats intensity {intensity}, speed {speed}, angle {angle}"
            )));
        }
        *slot = k;
    }
    for (i, by_speed) in rows.iter().enumerate() {
        for (s, by_angle) in by_speed.iter().enumerate() {
            if let Some(a) = by_angle.iter().position(|&k| k == usize::MAX) {
                return Err(IndexError::BatteryShape(format!(
                    "no edge at intensity {i}, speed {}, angle {}",
                    speeds[s],
                    a * 30
                )));
            }
        }
    }
    let mut sorted = [0.0; N_SPEEDS];
    sorted.copy_from_slice(&speeds);
    Ok(EdgeGrid { speeds: sorted, rows })
}

/// `peak * e^{i theta}` for angle index `a` (angle `30 a` degrees), as
/// numpy's complex128 product with a real peak.
fn polar(peak: f64, a: usize) -> (f64, f64) {
    let theta = (a * 30) as f64 * DEG;
    (peak * theta.cos(), peak * theta.sin())
}

fn mean_in_order(v: &[f64]) -> f64 {
    let mut s = 0.0;
    for &x in v {
        s += x;
    }
    s / v.len() as f64
}

fn finite_pair(what: &str, types: &[String], out: &[Vec<f64>; 2]) -> Result<(), IndexError> {
    for (i, v) in out.iter().enumerate() {
        if let Some(t) = v.iter().position(|x| !x.is_finite()) {
            return Err(IndexError::NonFinite(format!("the {what} of {} at intensity {i}", types[t])));
        }
    }
    Ok(())
}

/// flyvis's direction selectivity index per intensity: per speed,
/// `|sum_theta z| / (max over intensity of sum_theta |z| + 1e-15)` with
/// `z = peak e^{i theta}` (one denominator shared by ON and OFF), then the
/// mean over the six speeds. Sums over angle ascend in angle, the mean
/// ascends in speed, whatever the layout's order.
pub fn dsi(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError> {
    let g = edge_grid(p, b)?;
    let n = p.cell_types.len();
    let mut out = [vec![0.0; n], vec![0.0; n]];
    for t in 0..n {
        let mut per_speed = [[0.0f64; N_SPEEDS]; 2];
        for s in 0..N_SPEEDS {
            let mut length = [0.0f64; 2];
            let mut total = [0.0f64; 2];
            for (i, (len, tot)) in length.iter_mut().zip(total.iter_mut()).enumerate() {
                let (mut re, mut im, mut sum_abs) = (0.0f64, 0.0f64, 0.0f64);
                for (a, &row) in g.rows[i][s].iter().enumerate() {
                    let (zr, zi) = polar(p.peaks[row][t], a);
                    re += zr;
                    im += zi;
                    sum_abs += zr.hypot(zi);
                }
                *len = re.hypot(im);
                *tot = sum_abs;
            }
            let norm = total[0].max(total[1]);
            for (ps, &len) in per_speed.iter_mut().zip(&length) {
                ps[s] = len / (norm + 1e-15);
            }
        }
        for (o, ps) in out.iter_mut().zip(&per_speed) {
            o[t] = mean_in_order(ps);
        }
    }
    finite_pair("DSI", &p.cell_types, &out)?;
    Ok(out)
}

/// flyvis's preferred direction per intensity: the angle (`atan2(im, re)`,
/// in (-pi, pi]) of the un-normalised sum of `peak e^{i theta}` over every
/// speed (ascending) and angle (ascending).
pub fn preferred_direction(p: &EdgePeaks, b: &BatteryLayout) -> Result<[Vec<f64>; 2], IndexError> {
    let g = edge_grid(p, b)?;
    let n = p.cell_types.len();
    let mut out = [vec![0.0; n], vec![0.0; n]];
    for (i, o) in out.iter_mut().enumerate() {
        for (t, slot) in o.iter_mut().enumerate() {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for by_angle in &g.rows[i] {
                for (a, &row) in by_angle.iter().enumerate() {
                    let (zr, zi) = polar(p.peaks[row][t], a);
                    re += zr;
                    im += zi;
                }
            }
            *slot = im.atan2(re);
        }
    }
    finite_pair("preferred direction", &p.cell_types, &out)?;
    Ok(out)
}

/// Pearson r per known type, intensity and speed, against Maisak's curves.
#[derive(Clone, Debug, PartialEq)]
pub struct TuningR {
    cell_types: Vec<String>,
    speeds: [f64; N_SPEEDS],
    r: Vec<[[f64; N_SPEEDS]; 2]>,
}

impl TuningR {
    /// The known types, in the order `tuning_r` was given them.
    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    /// The speeds, ascending: the order of every `[f64; N_SPEEDS]`.
    pub fn speeds(&self) -> [f64; N_SPEEDS] {
        self.speeds
    }

    /// `[intensity][speed]` for one known type.
    pub fn r(&self, cell_type: &str) -> Option<[[f64; N_SPEEDS]; 2]> {
        self.cell_types.iter().position(|t| t == cell_type).map(|k| self.r[k])
    }

    /// Per intensity, the best r over the speeds and its speed index: the
    /// first on ties, as numpy's argmax.
    pub fn best(&self, cell_type: &str) -> Option<[(f64, usize); 2]> {
        let r = self.r(cell_type)?;
        Some(r.map(|by_speed| {
            let mut best = (by_speed[0], 0);
            for (s, &v) in by_speed.iter().enumerate().skip(1) {
                if v > best.0 {
                    best = (v, s);
                }
            }
            best
        }))
    }
}

/// flyvis's correlation to Maisak 2013's tuning curves (`absmax=False`):
/// per known type, intensity and speed, the Pearson r over the 12 angles of
/// the peaks against the curve, a constant input scoring 0 (`fillna(0)`).
/// `known` holds f64 curves equal to flyvis's literals (see
/// [`widen_curves`]). Flyvis demeans an f32 run's peaks in f32; this runs in
/// f64, as the spec says, and gate A's f32 rows are bounded.
pub fn tuning_r(p: &EdgePeaks, b: &BatteryLayout, known: &[(&str, [f64; 12])]) -> Result<TuningR, IndexError> {
    let g = edge_grid(p, b)?;
    let mut cell_types = Vec::with_capacity(known.len());
    let mut r = Vec::with_capacity(known.len());
    for &(name, curve) in known {
        let t = column(&p.cell_types, name)?;
        if curve.iter().any(|v| !v.is_finite()) {
            return Err(IndexError::BadKnown { cell_type: name.to_string(), reason: "the tuning curve is not finite".into() });
        }
        let mut by = [[0.0f64; N_SPEEDS]; 2];
        for (i, by_speed) in by.iter_mut().enumerate() {
            for (s, slot) in by_speed.iter_mut().enumerate() {
                let peaks: Vec<f64> = g.rows[i][s].iter().map(|&row| p.peaks[row][t]).collect();
                let v = pearson(&peaks, &curve);
                *slot = if v.is_nan() { 0.0 } else { v };
            }
        }
        cell_types.push(name.to_string());
        r.push(by);
    }
    Ok(TuningR { cell_types, speeds: g.speeds, r })
}

/// Maisak's f32 curves (`groundtruth::TUNING_CURVES`) as flyvis's f64
/// literals: each f32 through its shortest decimal string, then parsed as
/// f64 (0.745f32 becomes 0.745, not 0.7450000047683716).
pub fn widen_curves<'a>(curves: &[(&'a str, [f32; 12])]) -> Vec<(&'a str, [f64; 12])> {
    curves.iter().map(|&(name, c)| (name, c.map(|x| format!("{x}").parse().unwrap_or(x as f64)))).collect()
}

/// The position of cell type `name` in `types`.
fn column(types: &[String], name: &str) -> Result<usize, IndexError> {
    types.iter().position(|t| t == name).ok_or_else(|| IndexError::MissingCellType(name.to_string()))
}

/// xarray's `corr` over one dimension: the population covariance over the
/// product of the population standard deviations, each sum in order. NaN
/// when either input is constant.
fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (mean_in_order(a), mean_in_order(b));
    let (mut cov, mut va, mut vb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b) {
        let (dx, dy) = (x - ma, y - mb);
        cov += dx * dy;
        va += dx * dx;
        vb += dy * dy;
    }
    (cov / n) / ((va / n).sqrt() * (vb / n).sqrt())
}

/// The paper's DSI threshold: the 99% quantile of a 100x angle-permutation
/// null (Lappalainen et al. 2024, Methods, "DSI").
pub const D_THRESH: f64 = 0.357;
/// The binomial test's null rate (Methods, "Determining whether a cell type
/// with asymmetric inputs counts as direction selective").
pub const BINOMIAL_P0: f64 = 0.1;
/// The binomial test's level (the same section): P < 0.05.
pub const BINOMIAL_ALPHA: f64 = 0.05;
/// The preferred-direction tolerance, in degrees. Borrowed: the paper uses a
/// 45° acceptance angle in model selection (Methods, "Model selection"), not
/// as a rule for preferred direction; this epic applies it there and the
/// record says so.
pub const PD_TOLERANCE_DEG: f64 = 45.0;

/// How many of a known set agree, out of how many, and which did not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agreement {
    pub count: usize,
    pub of: usize,
    pub misses: Vec<String>,
}

/// flyvis's `fri_correlation_to_known`: the Pearson r of the FRI against
/// the known polarity (+1 or -1) over the known types, in `known`'s order.
pub fn fri_correlation_to_known(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<f64, IndexError> {
    let (x, y) = known_polarities(fri, types, known)?;
    pearson_defined(&x, &y, "the FRI's correlation to the known polarities")
}

/// flyvis's `dsi_correlation_to_known`: the Pearson r, over the known DSI
/// types in `types`' order, of the DSI maxed over intensity against 1.0 for
/// a motion-tuned type and 0.0 otherwise.
pub fn dsi_correlation_to_known(
    dsi: &[Vec<f64>; 2],
    types: &[String],
    known_dsi_types: &[&str],
    motion_tuning: &[&str],
) -> Result<f64, IndexError> {
    for d in dsi {
        if d.len() != types.len() {
            return Err(IndexError::Length { what: "DSI", expected: types.len(), found: d.len() });
        }
    }
    if known_dsi_types.is_empty() {
        return Err(IndexError::Empty("known DSI types"));
    }
    for name in known_dsi_types {
        column(types, name)?;
    }
    for name in motion_tuning {
        if !known_dsi_types.contains(name) {
            let reason = "a motion-tuned type is not among the known DSI types".to_string();
            return Err(IndexError::BadKnown { cell_type: name.to_string(), reason });
        }
    }
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for (t, name) in types.iter().enumerate() {
        if !known_dsi_types.contains(&name.as_str()) {
            continue;
        }
        let v = dsi[0][t].max(dsi[1][t]);
        if !v.is_finite() {
            return Err(IndexError::NonFinite(format!("the DSI of {name}")));
        }
        x.push(v);
        y.push(if motion_tuning.contains(&name.as_str()) { 1.0 } else { 0.0 });
    }
    pearson_defined(&x, &y, "the DSI's correlation to the known motion tuning")
}

/// flyvis's `angular_distance_to_known`: per known type, in `known`'s order,
/// the angular distance in radians from its preferred direction (T4 types
/// at intensity 1, T5 types at intensity 0) to the known direction.
pub fn angular_distance_to_known(pd: &[Vec<f64>; 2], types: &[String], known: &[(&str, u16)]) -> Result<Vec<f64>, IndexError> {
    for d in pd {
        if d.len() != types.len() {
            return Err(IndexError::Length { what: "preferred directions", expected: types.len(), found: d.len() });
        }
    }
    if known.is_empty() {
        return Err(IndexError::Empty("known preferred directions"));
    }
    known
        .iter()
        .map(|&(name, deg)| {
            let v = pd[motion_intensity(name)?][column(types, name)?];
            if !v.is_finite() {
                return Err(IndexError::NonFinite(format!("the preferred direction of {name}")));
            }
            Ok(simple_angle_distance(v, known_radians(deg)))
        })
        .collect()
}

/// The paper's polarity rule: sign(FRI) against the known polarity over the
/// known types, with no dead zone. An FRI of exactly 0 has no sign: a miss.
pub fn polarity_agreement(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<Agreement, IndexError> {
    let (x, y) = known_polarities(fri, types, known)?;
    let misses: Vec<String> = known
        .iter()
        .zip(x.iter().zip(&y))
        .filter(|&(_, (&v, &pol))| !((v > 0.0 && pol > 0.0) || (v < 0.0 && pol < 0.0)))
        .map(|(&(name, _), _)| name.to_string())
        .collect();
    Ok(Agreement { count: known.len() - misses.len(), of: known.len(), misses })
}

/// The per-type median over models, as `np.median`: an even count averages
/// the two middle values. Callers use all 50 models and the top 10
/// (000 to 009, the released order).
pub fn median_fri(per_model: &[Vec<f64>]) -> Result<Vec<f64>, IndexError> {
    let n_types = per_model_shape(per_model)?;
    let n = per_model.len();
    let mut out = Vec::with_capacity(n_types);
    for t in 0..n_types {
        let mut col: Vec<f64> = per_model.iter().map(|v| v[t]).collect();
        col.sort_by(f64::total_cmp);
        out.push(if n % 2 == 1 { col[n / 2] } else { (col[n / 2 - 1] + col[n / 2]) / 2.0 });
    }
    Ok(out)
}

/// P(X >= k) for X ~ Binomial(n, p0), summed in f64 from i = k up, each
/// term `C(n, i) p0^i (1 - p0)^(n - i)` with C(n, i) an exact integer
/// rounded once.
pub fn binomial_upper_tail(n: u32, k: u32, p0: f64) -> f64 {
    if k == 0 {
        return 1.0;
    }
    if k > n {
        return 0.0;
    }
    let q = 1.0 - p0;
    let mut sum = 0.0f64;
    for i in k..=n {
        sum += choose(n, i) * p0.powi(i as i32) * q.powi((n - i) as i32);
    }
    sum
}

/// The paper's direction-selectivity rule, per type: k = how many models'
/// DSI exceed `d_thresh` (strictly); selective iff
/// `binomial_upper_tail(n_models, k, p0) < alpha` (strictly).
/// `per_model[m][t]` is model m's DSI for type t, already reduced over
/// intensity by the caller (the gates pass the max over intensity, as
/// `dsi_correlation_to_known` does).
pub fn dsi_class(per_model: &[Vec<f64>], d_thresh: f64, p0: f64, alpha: f64) -> Result<Vec<bool>, IndexError> {
    if !d_thresh.is_finite() {
        return Err(IndexError::BadParameter { what: "d_thresh", value: d_thresh });
    }
    if !(0.0..=1.0).contains(&p0) {
        return Err(IndexError::BadParameter { what: "p0", value: p0 });
    }
    if alpha.is_nan() || alpha <= 0.0 || alpha > 1.0 {
        return Err(IndexError::BadParameter { what: "alpha", value: alpha });
    }
    let n_types = per_model_shape(per_model)?;
    let n = per_model.len() as u32;
    Ok((0..n_types)
        .map(|t| {
            let k = per_model.iter().filter(|m| m[t] > d_thresh).count() as u32;
            binomial_upper_tail(n, k, p0) < alpha
        })
        .collect())
}

/// The preferred-direction rule: a known type agrees when its angular
/// distance (`angular_distance_to_known`'s pairing) is at most
/// `tolerance_deg` degrees.
pub fn pd_within(pd: &[Vec<f64>; 2], types: &[String], known_deg: &[(&str, u16)], tolerance_deg: f64) -> Result<Agreement, IndexError> {
    if !(tolerance_deg.is_finite() && tolerance_deg >= 0.0) {
        return Err(IndexError::BadParameter { what: "tolerance in degrees", value: tolerance_deg });
    }
    let d = angular_distance_to_known(pd, types, known_deg)?;
    let misses: Vec<String> = known_deg
        .iter()
        .zip(&d)
        .filter(|&(_, &dist)| dist * (180.0 / std::f64::consts::PI) > tolerance_deg)
        .map(|(&(name, _), _)| name.to_string())
        .collect();
    Ok(Agreement { count: d.len() - misses.len(), of: d.len(), misses })
}

/// Per known type, its FRI and its polarity as f64, after the checks.
fn known_polarities(fri: &[f64], types: &[String], known: &[(&str, i8)]) -> Result<(Vec<f64>, Vec<f64>), IndexError> {
    if fri.len() != types.len() {
        return Err(IndexError::Length { what: "FRI", expected: types.len(), found: fri.len() });
    }
    if known.is_empty() {
        return Err(IndexError::Empty("known polarities"));
    }
    let (mut x, mut y) = (Vec::with_capacity(known.len()), Vec::with_capacity(known.len()));
    for &(name, pol) in known {
        if pol != 1 && pol != -1 {
            return Err(IndexError::BadKnown { cell_type: name.to_string(), reason: format!("polarity {pol} is not +1 or -1") });
        }
        let v = fri[column(types, name)?];
        if !v.is_finite() {
            return Err(IndexError::NonFinite(format!("the FRI of {name}")));
        }
        x.push(v);
        y.push(pol as f64);
    }
    Ok((x, y))
}

/// `pearson`, refusing the NaN of a constant input.
fn pearson_defined(a: &[f64], b: &[f64], what: &str) -> Result<f64, IndexError> {
    let r = pearson(a, b);
    if r.is_finite() { Ok(r) } else { Err(IndexError::NonFinite(format!("{what} (a constant input has no Pearson r)"))) }
}

/// The number of types per model, after refusing no models, an empty
/// model, ragged models and non-finite values.
fn per_model_shape(per_model: &[Vec<f64>]) -> Result<usize, IndexError> {
    let first = per_model.first().ok_or(IndexError::Empty("per-model values"))?;
    if first.is_empty() {
        return Err(IndexError::Empty("a model's values"));
    }
    for (m, v) in per_model.iter().enumerate() {
        if v.len() != first.len() {
            return Err(IndexError::Length { what: "a model's values", expected: first.len(), found: v.len() });
        }
        if let Some(t) = v.iter().position(|x| !x.is_finite()) {
            return Err(IndexError::NonFinite(format!("model {m}'s value for type {t}")));
        }
    }
    Ok(first.len())
}

/// T4 types are read at intensity 1 (ON), T5 types at intensity 0 (OFF).
fn motion_intensity(name: &str) -> Result<usize, IndexError> {
    if name.starts_with("T4") {
        Ok(1)
    } else if name.starts_with("T5") {
        Ok(0)
    } else {
        Err(IndexError::BadKnown { cell_type: name.to_string(), reason: "only T4 and T5 types have a known preferred direction".into() })
    }
}

/// flyvis's known directions in radians: 0, pi/2, pi, 3 pi/2.
fn known_radians(deg: u16) -> f64 {
    use std::f64::consts::PI;
    match deg {
        0 => 0.0,
        90 => PI / 2.0,
        180 => PI,
        270 => 3.0 * PI / 2.0,
        d => d as f64 * DEG,
    }
}

/// flyvis's `simple_angle_distance` with `upper = pi`.
fn simple_angle_distance(a: f64, b: f64) -> f64 {
    use std::f64::consts::PI;
    let tau = 2.0 * PI;
    let (a, b) = (py_mod(a, tau), py_mod(b, tau));
    let mut y = if a >= b { a - b } else { b - a };
    if y > PI {
        y = tau - y;
    }
    y / PI * PI
}

/// numpy's `%` on floats: C's fmod, moved into the divisor's sign; a zero
/// takes the divisor's sign.
fn py_mod(a: f64, m: f64) -> f64 {
    let r = a % m;
    if r == 0.0 {
        0.0f64.copysign(m)
    } else if (r < 0.0) != (m < 0.0) {
        r + m
    } else {
        r
    }
}

/// C(n, i), exact as a u128 while it fits (always for n <= 128), rounded
/// once to f64; past that, a running f64 product.
fn choose(n: u32, i: u32) -> f64 {
    let i = i.min(n - i);
    let mut c: u128 = 1;
    for j in 0..i {
        match c.checked_mul((n - j) as u128) {
            Some(v) => c = v / (j as u128 + 1),
            None => {
                let mut f = 1.0f64;
                for j in 0..i {
                    f = f * (n - j) as f64 / (j + 1) as f64;
                }
                return f;
            }
        }
    }
    c as f64
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

    const SPEEDS: [f64; 6] = [2.4, 4.8, 9.7, 13.0, 19.0, 25.0];

    /// The real edge battery's shape over `speeds` (in that order): intensity,
    /// then speed, then angle. Windows are frame 0; the reductions never read
    /// them.
    fn edge_layout(speeds: &[f64]) -> BatteryLayout {
        let angles: Vec<u16> = (0..12u16).map(|a| a * 30).collect();
        edge_layout_in(&[0, 1], speeds, &angles)
    }

    /// The edge battery listed intensity by intensity, speed by speed and
    /// angle by angle, each in the order given.
    fn edge_layout_in(intensities: &[u8], speeds: &[f64], angles: &[u16]) -> BatteryLayout {
        let mut v = Vec::new();
        for &intensity in intensities {
            for &speed in speeds {
                for &angle_deg in angles {
                    let label = format!("edge {intensity} {speed} {angle_deg}");
                    v.push(entry(&label, Stimulus::Edge { angle_deg, speed, intensity }, window(0, 0)));
                }
            }
        }
        BatteryLayout::new(v).unwrap()
    }

    /// Edge peaks over `b`'s edges: `f(intensity, speed, angle, type)`.
    fn edge_peaks_for(b: &BatteryLayout, types: &[&str], f: impl Fn(u8, f64, u16, usize) -> f64) -> EdgePeaks {
        let rows = b
            .stimuli()
            .iter()
            .filter_map(|(_, s, _)| match *s {
                Stimulus::Edge { angle_deg, speed, intensity } => {
                    Some((0..types.len()).map(|t| f(intensity, speed, angle_deg, t)).collect::<Vec<f64>>())
                }
                Stimulus::Flash { .. } => None,
            })
            .collect();
        EdgePeaks::new(types.iter().map(|t| t.to_string()).collect(), Precision::F64, rows).unwrap()
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * b.abs().max(1.0)
    }

    #[test]
    fn edge_peaks_are_the_clipped_window_maximum() {
        let b = BatteryLayout::new(vec![
            entry("flash", Stimulus::Flash { radius: 6, intensity: 1 }, window(0, 3)),
            entry("e0", Stimulus::Edge { angle_deg: 0, speed: 19.0, intensity: 1 }, window(1, 2)),
            entry("e1", Stimulus::Edge { angle_deg: 30, speed: 19.0, intensity: 1 }, window(1, 3)),
        ])
        .unwrap();
        // A: a big value outside the window is ignored. B: all negative, so +0.0.
        let r = responses(&["A", "B"], Precision::F64, &[("flash", 4), ("e0", 4), ("e1", 4)], |s, f, t| match (s, t) {
            (1, 0) => [9.0, 0.5, 0.25, 9.0][f],
            (2, 0) => [9.0, 0.1, 0.7, 0.3][f],
            (_, _) => -1.0 - f as f64,
        });
        let p = edge_peaks(&r, &b).unwrap();
        assert_eq!(p.peaks(), &[vec![0.5, 0.0], vec![0.7, 0.0]]);
        assert!(p.peaks().iter().all(|row| row[1].to_bits() == 0), "a zero peak is +0.0");
        let renamed = BatteryLayout::new(vec![
            entry("flash", Stimulus::Flash { radius: 6, intensity: 1 }, window(0, 3)),
            entry("e0", Stimulus::Edge { angle_deg: 0, speed: 19.0, intensity: 1 }, window(1, 2)),
            entry("E1", Stimulus::Edge { angle_deg: 30, speed: 19.0, intensity: 1 }, window(1, 3)),
        ])
        .unwrap();
        assert!(matches!(edge_peaks(&r, &renamed), Err(IndexError::LabelMismatch { stimulus: 2, .. })));
        let flashes_only = BatteryLayout::new(vec![entry("flash", Stimulus::Flash { radius: 6, intensity: 1 }, window(0, 3))]).unwrap();
        let r1 = responses(&["A"], Precision::F64, &[("flash", 4)], |_, _, _| 0.0);
        assert_eq!(edge_peaks(&r1, &flashes_only), Err(IndexError::Empty("edge stimuli")));
    }

    #[test]
    fn the_dsi_shares_one_denominator_between_on_and_off() {
        // ON: peak 1 at 0 and 180 degrees (vector ~0, sum of |z| 2). OFF:
        // peak 0.5 at 0 degrees (vector 0.5). OFF's DSI is 0.5 over ON's 2.
        let b = edge_layout(&SPEEDS);
        let p = edge_peaks_for(&b, &["A"], |i, _, angle, _| match (i, angle) {
            (1, 0) | (1, 180) => 1.0,
            (0, 0) => 0.5,
            _ => 0.0,
        });
        let d = dsi(&p, &b).unwrap();
        assert!(close(d[0][0], 0.5 / (2.0 + 1e-15), 1e-15), "{}", d[0][0]);
        assert!(d[1][0] < 1e-15, "{}", d[1][0]);
        // Mirrored, so OFF holds the larger sum of |z|: the denominator is the
        // max over intensity, not always ON's.
        let p = edge_peaks_for(&b, &["A"], |i, _, angle, _| match (i, angle) {
            (0, 0) | (0, 180) => 1.0,
            (1, 0) => 0.5,
            _ => 0.0,
        });
        let d = dsi(&p, &b).unwrap();
        assert!(close(d[1][0], 0.5 / (2.0 + 1e-15), 1e-15), "{}", d[1][0]);
        assert!(d[0][0] < 1e-15, "{}", d[0][0]);
        // A peak so large that hypot overflows: the DSI is not finite, refused.
        let huge = edge_peaks_for(&b, &["A"], |_, _, _, _| 1e308);
        assert!(matches!(dsi(&huge, &b), Err(IndexError::NonFinite(_))));
    }

    #[test]
    fn the_dsi_is_the_mean_over_the_six_speeds() {
        // ON only: peak 1 at 0 degrees and a at 180, so each speed's DSI is
        // (1 - a) / (1 + a + 1e-15).
        let a = [0.0, 1.0 / 3.0, 0.5, 0.0, 0.0, 0.0];
        let b = edge_layout(&SPEEDS);
        let p = edge_peaks_for(&b, &["A"], |i, speed, angle, _| {
            let s = SPEEDS.iter().position(|&v| v == speed).unwrap();
            match (i, angle) {
                (1, 0) => 1.0,
                (1, 180) => a[s],
                _ => 0.0,
            }
        });
        let want = a.iter().map(|x| (1.0 - x) / (1.0 + x + 1e-15)).sum::<f64>() / 6.0;
        assert!(close(dsi(&p, &b).unwrap()[1][0], want, 1e-12));
    }

    #[test]
    fn the_preferred_direction_sums_over_speeds_unnormalised() {
        // speed 2.4: peak 3 at 0 degrees; speed 4.8: peak 1 at 90 degrees.
        // Unnormalised: atan2(1, 3); normalised per speed it would be pi/4.
        let b = edge_layout(&SPEEDS);
        let p = edge_peaks_for(&b, &["A"], |i, speed, angle, _| match (i, speed, angle) {
            (0, 2.4, 0) => 3.0,
            (0, 4.8, 90) => 1.0,
            _ => 0.0,
        });
        let pd = preferred_direction(&p, &b).unwrap();
        assert!(close(pd[0][0], 0.3217505543966422, 1e-12), "{}", pd[0][0]);
        assert_eq!(pd[1][0], 0.0, "an all-zero response points at angle 0");
    }

    #[test]
    fn tuning_keeps_the_best_speed_first_on_ties_and_a_flat_curve_scores_0() {
        let curve: [f64; 12] = std::array::from_fn(|a| a as f64);
        let b = edge_layout(&SPEEDS);
        // ON: speeds 9.7 and 19 match the curve exactly (identical r); the
        // others are scrambled. OFF: flat everywhere, so r is NaN, filled 0.
        let p = edge_peaks_for(&b, &["T4a", "Mi1"], |i, speed, angle, _| {
            let a = (angle / 30) as f64;
            match i {
                1 if speed == 9.7 || speed == 19.0 => 2.0 * a + 1.0,
                1 => ((angle / 30) * 7 % 12) as f64,
                _ => 0.5,
            }
        });
        let t = tuning_r(&p, &b, &[("T4a", curve)]).unwrap();
        assert_eq!(t.cell_types(), &["T4a".to_string()]);
        assert_eq!(t.speeds(), SPEEDS);
        let r = t.r("T4a").unwrap();
        assert_eq!(r[0], [0.0; 6], "fillna(0) on a flat curve");
        assert!(close(r[1][2], 1.0, 1e-12) && r[1][2] == r[1][4]);
        let best = t.best("T4a").unwrap();
        assert_eq!(best[0], (0.0, 0));
        assert_eq!(best[1], (r[1][2], 2), "the first of two equal speeds");
        assert_eq!(t.r("Mi1"), None);
        assert_eq!(tuning_r(&p, &b, &[("T5a", curve)]), Err(IndexError::MissingCellType("T5a".into())));
        let mut broken = curve;
        broken[3] = f64::NAN;
        assert!(matches!(tuning_r(&p, &b, &[("T4a", broken)]), Err(IndexError::BadKnown { .. })));
    }

    #[test]
    fn the_edge_reductions_do_not_depend_on_the_layouts_order() {
        let up = edge_layout(&SPEEDS);
        let mut reversed = SPEEDS;
        reversed.reverse();
        let down = edge_layout(&reversed);
        let f = |i: u8, speed: f64, angle: u16, t: usize| (speed * 0.1 + angle as f64 / 360.0 + i as f64 + t as f64).sin().abs();
        let (pu, pd) = (edge_peaks_for(&up, &["T4a"], f), edge_peaks_for(&down, &["T4a"], f));
        assert_eq!(dsi(&pu, &up), dsi(&pd, &down));
        assert_eq!(preferred_direction(&pu, &up), preferred_direction(&pd, &down));
        let curve: [f64; 12] = std::array::from_fn(|a| (a as f64).cos());
        assert_eq!(tuning_r(&pu, &up, &[("T4a", curve)]), tuning_r(&pd, &down, &[("T4a", curve)]));
        // Intensities 1 then 0, speeds scrambled, angles descending within
        // each (intensity, speed) group, and a response far from symmetric
        // in the angle: an angle taken from a row's position, or an
        // assumed intensity order, moves the DSI and PD by far more than
        // rounding.
        let descending: Vec<u16> = (0..12u16).rev().map(|a| a * 30).collect();
        let scrambled = edge_layout_in(&[1, 0], &[13.0, 2.4, 25.0, 4.8, 19.0, 9.7], &descending);
        let g = |i: u8, speed: f64, angle: u16, _t: usize| (angle as f64 / 30.0 + 1.0) * (1.0 + speed / 100.0) + i as f64;
        let (gu, gs) = (edge_peaks_for(&up, &["T4a"], g), edge_peaks_for(&scrambled, &["T4a"], g));
        assert_eq!(dsi(&gu, &up), dsi(&gs, &scrambled));
        assert_eq!(preferred_direction(&gu, &up), preferred_direction(&gs, &scrambled));
        assert_eq!(tuning_r(&gu, &up, &[("T4a", curve)]), tuning_r(&gs, &scrambled, &[("T4a", curve)]));
    }

    #[test]
    fn the_edge_reductions_refuse_the_wrong_battery_shape() {
        let p_for = |b: &BatteryLayout| edge_peaks_for(b, &["A"], |_, _, _, _| 1.0);
        let five = edge_layout(&SPEEDS[..5]);
        assert!(matches!(dsi(&p_for(&five), &five), Err(IndexError::BatteryShape(_))));
        let full = edge_layout(&SPEEDS);
        // drop one angle: 143 edges
        let short = BatteryLayout::new(full.stimuli()[1..].to_vec()).unwrap();
        assert!(matches!(dsi(&p_for(&short), &short), Err(IndexError::BatteryShape(_))));
        // repeat one angle in place of another
        let mut twice = full.stimuli().to_vec();
        twice[1].1 = Stimulus::Edge { angle_deg: 0, speed: 2.4, intensity: 0 };
        let twice = BatteryLayout::new(twice).unwrap();
        assert!(matches!(preferred_direction(&p_for(&twice), &twice), Err(IndexError::BatteryShape(_))));
        // an intensity that is neither 0 nor 1
        let mut bright = full.stimuli().to_vec();
        bright[0].1 = Stimulus::Edge { angle_deg: 0, speed: 2.4, intensity: 2 };
        let bright = BatteryLayout::new(bright).unwrap();
        assert!(matches!(tuning_r(&p_for(&bright), &bright, &[]), Err(IndexError::BatteryShape(_))));
        // peaks that do not match the layout's edge count
        let p = p_for(&full);
        assert_eq!(dsi(&p, &short), Err(IndexError::Length { what: "edge peaks", expected: 143, found: 144 }));
    }

    #[test]
    fn edge_peaks_built_by_hand_are_checked() {
        let names = || vec!["A".to_string(), "B".to_string()];
        assert_eq!(EdgePeaks::new(names(), Precision::F64, vec![]), Err(IndexError::Empty("edge peaks")));
        assert_eq!(
            EdgePeaks::new(names(), Precision::F64, vec![vec![1.0]]),
            Err(IndexError::Length { what: "edge peak row", expected: 2, found: 1 })
        );
        assert!(matches!(EdgePeaks::new(names(), Precision::F64, vec![vec![1.0, f64::NAN]]), Err(IndexError::NonFinite(_))));
        assert!(matches!(EdgePeaks::new(names(), Precision::F32, vec![vec![1.0, 0.1]]), Err(IndexError::NotF32(_))));
    }

    #[test]
    fn edge_peaks_refuse_an_f32_run_holding_non_f32_values() {
        // 0.1 is below the window's maximum 0.5, so only a check of every
        // window value catches it.
        let b = BatteryLayout::new(vec![entry("e0", Stimulus::Edge { angle_deg: 0, speed: 19.0, intensity: 1 }, window(1, 2))]).unwrap();
        let r = responses(&["A"], Precision::F32, &[("e0", 4)], |_, f, _| if f == 1 { 0.1 } else { 0.5 });
        assert!(matches!(edge_peaks(&r, &b), Err(IndexError::NotF32(_))));
        let ok = responses(&["A"], Precision::F32, &[("e0", 4)], |_, f, _| if f == 1 { 0.25 } else { 0.5 });
        assert_eq!(edge_peaks(&ok, &b).unwrap().peaks(), &[vec![0.5]]);
    }

    #[test]
    fn widened_curves_are_flyviss_decimal_literals() {
        let w = widen_curves(&[("T4a", [0.745f32; 12])]);
        assert_eq!(w[0].0, "T4a");
        assert_eq!(w[0].1, [0.745f64; 12]);
        assert_ne!(0.745f32 as f64, 0.745f64, "a plain widening would not be");
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn the_fri_correlation_to_known_is_pearson_over_the_known_types() {
        let types = names(&["A", "B", "C", "D"]);
        let r = fri_correlation_to_known(&[0.5, -0.5, 0.25, 9.0], &types, &[("A", 1), ("B", -1), ("C", 1)]).unwrap();
        assert!(close(r, 0.970725343394151, 1e-12), "{r}");
        assert!(matches!(fri_correlation_to_known(&[0.5, -0.5, 0.25, 9.0], &types, &[("A", 1), ("C", 1)]), Err(IndexError::NonFinite(_))));
        assert_eq!(fri_correlation_to_known(&[0.5, -0.5, 0.25, 9.0], &types, &[("E", 1), ("A", -1)]), Err(IndexError::MissingCellType("E".into())));
    }

    #[test]
    fn the_dsi_correlation_to_known_maxes_over_intensity_against_a_motion_label() {
        let types = names(&["X", "T4a", "Mi1", "T5a"]);
        let dsi = [vec![9.0, 0.2, 0.9, 0.1], vec![9.0, 0.8, 0.1, 0.7]];
        let r = dsi_correlation_to_known(&dsi, &types, &["T5a", "Mi1", "T4a"], &["T4a", "T5a"]).unwrap();
        assert!(close(r, -0.8660254037844385, 1e-12), "{r}");
        let e = dsi_correlation_to_known(&dsi, &types, &["T4a", "Mi1"], &["T5a"]);
        assert!(matches!(e, Err(IndexError::BadKnown { .. })), "{e:?}");
        assert_eq!(dsi_correlation_to_known(&dsi, &types, &["T4b"], &[]), Err(IndexError::MissingCellType("T4b".into())));
        assert_eq!(dsi_correlation_to_known(&dsi, &types, &[], &[]), Err(IndexError::Empty("known DSI types")));
        let ragged = [vec![9.0, 0.2, 0.9], vec![9.0, 0.8, 0.1, 0.7]];
        assert_eq!(
            dsi_correlation_to_known(&ragged, &types, &["T4a"], &[]),
            Err(IndexError::Length { what: "DSI", expected: 4, found: 3 })
        );
        let flat = [vec![0.5; 4], vec![0.5; 4]];
        let e = dsi_correlation_to_known(&flat, &types, &["T5a", "Mi1", "T4a"], &["T4a", "T5a"]);
        assert!(matches!(e, Err(IndexError::NonFinite(_))), "a constant DSI has no Pearson r: {e:?}");
    }

    #[test]
    fn angular_distances_read_t4_at_on_and_t5_at_off_and_wrap() {
        use std::f64::consts::PI;
        let types = names(&["T4a", "T5b", "T4d"]);
        // T4a at ON: -pi + 0.1 against 180 degrees. T5b at OFF: -0.05 against
        // 0. T4d at ON: 3 pi / 2 - 0.2 against 270. The other intensity's
        // values would be far off.
        let pd = [vec![0.0, -0.05, 0.0], vec![-PI + 0.1, 2.0, 3.0 * PI / 2.0 - 0.2]];
        let d = angular_distance_to_known(&pd, &types, &[("T4a", 180), ("T5b", 0), ("T4d", 270)]).unwrap();
        assert!(close(d[0], 0.1, 1e-12) && close(d[1], 0.05, 1e-12) && close(d[2], 0.2, 1e-12), "{d:?}");
        let e = angular_distance_to_known(&pd, &types, &[("Mi1", 0)]);
        assert!(matches!(e, Err(IndexError::BadKnown { .. })), "{e:?}");
        assert_eq!(angular_distance_to_known(&pd, &types, &[("T4c", 0)]), Err(IndexError::MissingCellType("T4c".into())));
        assert_eq!(angular_distance_to_known(&pd, &types, &[]), Err(IndexError::Empty("known preferred directions")));
        let ragged = [vec![0.0; 3], vec![0.0; 2]];
        assert_eq!(
            angular_distance_to_known(&ragged, &types, &[("T4a", 180)]),
            Err(IndexError::Length { what: "preferred directions", expected: 3, found: 2 })
        );
        let nan = [vec![0.0; 3], vec![f64::NAN, 0.0, 0.0]];
        assert!(matches!(angular_distance_to_known(&nan, &types, &[("T4a", 180)]), Err(IndexError::NonFinite(_))));
    }

    #[test]
    fn polarity_agreement_has_no_dead_zone_and_zero_is_a_miss() {
        let types = names(&["A", "B", "C", "D"]);
        let a = polarity_agreement(&[0.5, -0.2, 0.0, 1e-9], &types, &[("A", 1), ("B", 1), ("C", -1), ("D", 1)]).unwrap();
        assert_eq!(a, Agreement { count: 2, of: 4, misses: vec!["B".into(), "C".into()] });
        assert!(matches!(polarity_agreement(&[0.5; 4], &types, &[("A", 0)]), Err(IndexError::BadKnown { .. })));
        assert_eq!(polarity_agreement(&[0.5; 4], &types, &[]), Err(IndexError::Empty("known polarities")));
        assert_eq!(
            polarity_agreement(&[0.5; 3], &types, &[("A", 1)]),
            Err(IndexError::Length { what: "FRI", expected: 4, found: 3 })
        );
        assert!(matches!(polarity_agreement(&[f64::NAN, 0.0, 0.0, 0.0], &types, &[("A", 1)]), Err(IndexError::NonFinite(_))));
    }

    #[test]
    fn the_median_fri_is_numpys_median_per_type() {
        assert_eq!(median_fri(&[vec![1.0, 4.0], vec![3.0, 2.0], vec![2.0, 8.0]]), Ok(vec![2.0, 4.0]));
        assert_eq!(median_fri(&[vec![1.0], vec![2.0], vec![3.0], vec![10.0]]), Ok(vec![2.5]));
        // the empty battery's analogue: no models is a refusal, never a score
        assert_eq!(median_fri(&[]), Err(IndexError::Empty("per-model values")));
        assert_eq!(median_fri(&[vec![]]), Err(IndexError::Empty("a model's values")));
        assert_eq!(median_fri(&[vec![1.0, 2.0], vec![1.0]]), Err(IndexError::Length { what: "a model's values", expected: 2, found: 1 }));
        assert!(matches!(median_fri(&[vec![1.0], vec![f64::INFINITY]]), Err(IndexError::NonFinite(_))));
    }

    #[test]
    fn binomial_upper_tail_matches_hand_values() {
        assert_eq!(binomial_upper_tail(1, 1, 0.1), 0.1);
        assert_eq!(binomial_upper_tail(7, 0, 0.1), 1.0);
        assert_eq!(binomial_upper_tail(3, 4, 0.1), 0.0);
        for (n, k, want) in [(2, 1, 0.19), (2, 2, 0.01), (3, 2, 0.028), (10, 3, 0.0701908264), (10, 4, 0.0127951984)] {
            let got = binomial_upper_tail(n, k, 0.1);
            assert!(close(got, want, 1e-14), "n {n}, k {k}: {got}");
        }
        assert!(close(binomial_upper_tail(50, 9, 0.1), 0.057867205718094276, 1e-13));
        assert!(close(binomial_upper_tail(50, 10, 0.1), 0.024537935704591455, 1e-13));
    }

    #[test]
    fn dsi_class_is_the_papers_binomial_rule() {
        // type 0: 10 of 50 above d_thresh (selective); type 1: 9 (not);
        // type 2: exactly d_thresh everywhere (not: the test is strict).
        let per_model: Vec<Vec<f64>> = (0..50)
            .map(|m| vec![if m < 10 { 0.5 } else { 0.1 }, if m < 9 { 0.5 } else { 0.1 }, D_THRESH])
            .collect();
        assert_eq!(dsi_class(&per_model, D_THRESH, BINOMIAL_P0, BINOMIAL_ALPHA), Ok(vec![true, false, false]));
        assert!(matches!(dsi_class(&per_model, D_THRESH, 1.5, BINOMIAL_ALPHA), Err(IndexError::BadParameter { what: "p0", .. })));
        assert!(matches!(dsi_class(&per_model, D_THRESH, BINOMIAL_P0, f64::NAN), Err(IndexError::BadParameter { what: "alpha", .. })));
        assert_eq!(dsi_class(&[], D_THRESH, BINOMIAL_P0, BINOMIAL_ALPHA), Err(IndexError::Empty("per-model values")));
    }

    #[test]
    fn pd_within_uses_the_t4_on_t5_off_pairing_and_the_tolerance() {
        use std::f64::consts::PI;
        let types = names(&["T4a", "T5a"]);
        // T4a at ON is 0.1 rad from 180 degrees; T5a at OFF is 180 degrees off.
        let pd = [vec![0.0, 0.0], vec![PI - 0.1, PI]];
        let a = pd_within(&pd, &types, &[("T4a", 180), ("T5a", 180)], PD_TOLERANCE_DEG).unwrap();
        assert_eq!(a, Agreement { count: 1, of: 2, misses: vec!["T5a".into()] });
        assert!(matches!(pd_within(&pd, &types, &[("T4a", 180)], -1.0), Err(IndexError::BadParameter { .. })));
        assert!(matches!(pd_within(&pd, &types, &[("T4a", 180)], f64::NAN), Err(IndexError::BadParameter { .. })));
    }

    #[test]
    fn the_rule_constants_are_the_papers() {
        assert_eq!((D_THRESH, BINOMIAL_P0, BINOMIAL_ALPHA, PD_TOLERANCE_DEG), (0.357, 0.1, 0.05, 45.0));
    }
}
