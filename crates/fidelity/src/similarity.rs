//! The representational axis: how alike two networks represent a battery,
//! as debiased linear CKA over each cell type's central response time
//! course, plus Zhou & Hasler's own variant, computed exactly and reported.
//!
//! Pure, over task 1a's [`Responses`]. Every malformed input is a
//! [`SimilarityError`], never a score.
//!
//! Spec: docs/superpowers/specs/2026-11-04-representational-axis-design.md,
//! section 1.

use std::fmt;

use semblance_sim::response::{Precision, Responses};

/// Why a similarity could not be computed. Each is a refusal, never a score.
#[derive(Clone, Debug, PartialEq)]
pub enum SimilarityError {
    RowMismatch { x: usize, y: usize },
    ColumnMismatch { x: usize, y: usize },
    /// Too few rows, columns, conditions or entries (the unbiased HSIC
    /// divides by n - 3, so it needs 4 rows).
    TooFew { what: &'static str, need: usize, found: usize },
    /// `HSIC(X, X)` or `HSIC(Y, Y)` is not positive: no scale to divide by.
    NonPositiveHsic { which: &'static str, value: f64 },
    NonFinite(String),
    OnsetCount { onsets: usize, stimuli: usize },
    OnsetPastEnd { stimulus: usize, onset: usize, n_frames: usize },
    /// A value tagged as from an f32 run that is not an f32.
    NotF32 { row: usize, column: usize },
    Length { what: &'static str, expected: usize, found: usize },
    NotSymmetric { i: usize, j: usize },
    /// An input with no variation where the measure needs some.
    Degenerate(String),
}

impl fmt::Display for SimilarityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimilarityError::RowMismatch { x, y } => write!(f, "the matrices have {x} and {y} rows"),
            SimilarityError::ColumnMismatch { x, y } => write!(f, "the matrices have {x} and {y} columns"),
            SimilarityError::TooFew { what, need, found } => write!(f, "{what}: need at least {need}, found {found}"),
            SimilarityError::NonPositiveHsic { which, value } => write!(f, "HSIC({which}, {which}) is {value}, not positive"),
            SimilarityError::NonFinite(what) => write!(f, "{what} is not finite"),
            SimilarityError::OnsetCount { onsets, stimuli } => write!(f, "{onsets} onsets for {stimuli} stimuli"),
            SimilarityError::OnsetPastEnd { stimulus, onset, n_frames } => {
                write!(f, "stimulus {stimulus}: onset {onset} is past its {n_frames} frames")
            }
            SimilarityError::NotF32 { row, column } => write!(f, "row {row}, column {column}: an f32 run's value is not an f32"),
            SimilarityError::Length { what, expected, found } => write!(f, "{what}: expected {expected}, found {found}"),
            SimilarityError::NotSymmetric { i, j } => write!(f, "the RDM differs at ({i}, {j}) and ({j}, {i})"),
            SimilarityError::Degenerate(what) => f.write_str(what),
        }
    }
}

impl std::error::Error for SimilarityError {}

/// One network's responses as a matrix: a row per (stimulus, frame) from
/// each stimulus's onset to its end, in battery order; a column per cell
/// type. Held as f32 for an f32 run (every value is an f32, so nothing is
/// lost) and f64 otherwise: gate D holds 50 at once. The real flash and
/// edge batteries at onset 200 give 65,416 rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseMatrix {
    cell_types: Vec<String>,
    n_rows: usize,
    data: Store,
}

#[derive(Clone, Debug, PartialEq)]
enum Store {
    F32(Vec<f32>),
    F64(Vec<f64>),
}

impl ResponseMatrix {
    /// Every frame from `onset[s]` to the end of stimulus `s`. Refuses an
    /// onset count that is not one per stimulus, and an onset at or past
    /// its stimulus's last frame, so every stimulus gives at least one row.
    pub fn from_responses(r: &Responses, onset: &[usize]) -> Result<Self, SimilarityError> {
        if onset.len() != r.len() {
            return Err(SimilarityError::OnsetCount { onsets: onset.len(), stimuli: r.len() });
        }
        let mut values = Vec::new();
        for (s, &on) in onset.iter().enumerate() {
            let st = r.stimulus(s).ok_or(SimilarityError::OnsetCount { onsets: onset.len(), stimuli: r.len() })?;
            if on >= st.n_frames() {
                return Err(SimilarityError::OnsetPastEnd { stimulus: s, onset: on, n_frames: st.n_frames() });
            }
            for f in on..st.n_frames() {
                if let Some(row) = st.frame(f) {
                    values.extend_from_slice(row);
                }
            }
        }
        Self::build(r.cell_types().to_vec(), r.precision(), values)
    }

    /// A matrix from rows of one value per cell type, checked as
    /// `from_responses`' are.
    pub fn from_rows(cell_types: Vec<String>, precision: Precision, rows: &[Vec<f64>]) -> Result<Self, SimilarityError> {
        let p = cell_types.len();
        if p == 0 {
            return Err(SimilarityError::TooFew { what: "columns", need: 1, found: 0 });
        }
        for row in rows {
            if row.len() != p {
                return Err(SimilarityError::Length { what: "row", expected: p, found: row.len() });
            }
        }
        Self::build(cell_types, precision, rows.concat())
    }

    fn build(cell_types: Vec<String>, precision: Precision, values: Vec<f64>) -> Result<Self, SimilarityError> {
        let p = cell_types.len();
        if p == 0 {
            return Err(SimilarityError::TooFew { what: "columns", need: 1, found: 0 });
        }
        if values.is_empty() {
            return Err(SimilarityError::TooFew { what: "rows", need: 1, found: 0 });
        }
        if let Some(i) = values.iter().position(|v| !v.is_finite()) {
            return Err(SimilarityError::NonFinite(format!("row {}, column {}", i / p, i % p)));
        }
        let n_rows = values.len() / p;
        let data = match precision {
            Precision::F32 => {
                let mut out = Vec::with_capacity(values.len());
                for (i, &v) in values.iter().enumerate() {
                    let w = v as f32;
                    if w as f64 != v {
                        return Err(SimilarityError::NotF32 { row: i / p, column: i % p });
                    }
                    out.push(w);
                }
                Store::F32(out)
            }
            Precision::F64 => Store::F64(values),
        };
        Ok(ResponseMatrix { cell_types, n_rows, data })
    }

    pub fn n_rows(&self) -> usize {
        self.n_rows
    }

    pub fn n_cols(&self) -> usize {
        self.cell_types.len()
    }

    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    pub fn precision(&self) -> Precision {
        match self.data {
            Store::F32(_) => Precision::F32,
            Store::F64(_) => Precision::F64,
        }
    }

    /// Row `i`, widened to f64.
    pub fn row(&self, i: usize) -> Option<Vec<f64>> {
        if i >= self.n_rows {
            return None;
        }
        let mut out = vec![0.0; self.n_cols()];
        self.row_into(i, &mut out);
        Some(out)
    }

    fn row_into(&self, i: usize, out: &mut [f64]) {
        let p = self.n_cols();
        match &self.data {
            Store::F32(v) => {
                for (o, &x) in out.iter_mut().zip(&v[i * p..(i + 1) * p]) {
                    *o = x as f64;
                }
            }
            Store::F64(v) => out.copy_from_slice(&v[i * p..(i + 1) * p]),
        }
    }
}

/// Song et al. (2012)'s unbiased HSIC with linear kernels, in feature
/// space: from `X'Y`, the row norms and the column sums, never the n x n
/// Gram matrix, accumulated in f64. Every sum runs in an order symmetric in
/// `x` and `y`, so `hsic_unbiased(x, y)` and `hsic_unbiased(y, x)` are the
/// same bits.
pub fn hsic_unbiased(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError> {
    if x.n_rows != y.n_rows {
        return Err(SimilarityError::RowMismatch { x: x.n_rows, y: y.n_rows });
    }
    if x.n_cols() != y.n_cols() {
        return Err(SimilarityError::ColumnMismatch { x: x.n_cols(), y: y.n_cols() });
    }
    let (n, p) = (x.n_rows, x.n_cols());
    if n < 4 {
        return Err(SimilarityError::TooFew { what: "rows", need: 4, found: n });
    }
    let mut m = vec![0.0f64; p * p];
    let (mut sx, mut sy) = (vec![0.0f64; p], vec![0.0f64; p]);
    let (mut rx, mut ry) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut sum_rx, mut sum_ry, mut sum_rxry) = (0.0f64, 0.0f64, 0.0f64);
    let (mut xi, mut yi) = (vec![0.0f64; p], vec![0.0f64; p]);
    for i in 0..n {
        x.row_into(i, &mut xi);
        y.row_into(i, &mut yi);
        for (row, &xa) in m.chunks_exact_mut(p).zip(&xi) {
            for (cell, &yb) in row.iter_mut().zip(&yi) {
                *cell += xa * yb;
            }
        }
        for ((a, b), (&u, &v)) in sx.iter_mut().zip(sy.iter_mut()).zip(xi.iter().zip(&yi)) {
            *a += u;
            *b += v;
        }
        let (nx, ny) = (dot(&xi, &xi), dot(&yi, &yi));
        rx.push(nx);
        ry.push(ny);
        sum_rx += nx;
        sum_ry += ny;
        sum_rxry += nx * ny;
    }
    let mut kl = 0.0f64;
    for i in 0..n {
        x.row_into(i, &mut xi);
        y.row_into(i, &mut yi);
        kl += (dot(&xi, &sx) - rx[i]) * (dot(&yi, &sy) - ry[i]);
    }
    let tr = frobenius_sq_symmetric(&m, p) - sum_rxry;
    let k1 = dot(&sx, &sx) - sum_rx;
    let l1 = dot(&sy, &sy) - sum_ry;
    let nf = n as f64;
    let h = (tr + k1 * l1 / ((nf - 1.0) * (nf - 2.0)) - 2.0 / (nf - 2.0) * kl) / (nf * (nf - 3.0));
    if !h.is_finite() {
        return Err(SimilarityError::NonFinite("HSIC".into()));
    }
    Ok(h)
}

/// Debiased linear CKA: `H(X,Y) / sqrt(H(X,X) H(Y,Y))`. `cka(x, x)` is
/// exactly 1 (`H / sqrt(H H)` is exact in binary floating point) and
/// `cka(a, b) == cka(b, a)` bit for bit, both by construction.
pub fn cka(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError> {
    let hxy = hsic_unbiased(x, y)?;
    let hxx = hsic_unbiased(x, x)?;
    if !(hxx.is_finite() && hxx > 0.0) {
        return Err(SimilarityError::NonPositiveHsic { which: "x", value: hxx });
    }
    let hyy = hsic_unbiased(y, y)?;
    if !(hyy.is_finite() && hyy > 0.0) {
        return Err(SimilarityError::NonPositiveHsic { which: "y", value: hyy });
    }
    let c = hxy / (hxx * hyy).sqrt();
    if !c.is_finite() {
        return Err(SimilarityError::NonFinite("CKA".into()));
    }
    Ok(c)
}

/// The dot product, summed in order.
fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut s = 0.0f64;
    for (&u, &v) in a.iter().zip(b) {
        s += u * v;
    }
    s
}

/// The squared Frobenius norm of the p x p row-major `m`, summed so that
/// `m` and its transpose give the same bits: the diagonal, then each
/// `a < b` pair as `m[a][b]^2 + m[b][a]^2`.
fn frobenius_sq_symmetric(m: &[f64], p: usize) -> f64 {
    let mut s = 0.0f64;
    for a in 0..p {
        let d = m[a * p + a];
        s += d * d;
        for b in a + 1..p {
            let (u, v) = (m[a * p + b], m[b * p + a]);
            s += u * u + v * v;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use semblance_rng::Rng;
    use semblance_sim::response::Responses;

    fn names(p: usize) -> Vec<String> {
        (0..p).map(|t| format!("t{t}")).collect()
    }

    /// `n` rows of `p` values uniform in [-1, 1), from `seed`.
    fn random_rows(seed: u64, n: usize, p: usize) -> Vec<Vec<f64>> {
        let mut rng = Rng::seed_from_u64(seed);
        let mut rows = Vec::with_capacity(n);
        for _ in 0..n {
            let mut row = Vec::with_capacity(p);
            for _ in 0..p {
                row.push(rng.next_f32() as f64 * 2.0 - 1.0);
            }
            rows.push(row);
        }
        rows
    }

    /// `x` plus `scale` times independent noise: something in common with `x`.
    fn related(x: &[Vec<f64>], seed: u64, scale: f64) -> Vec<Vec<f64>> {
        let noise = random_rows(seed, x.len(), x[0].len());
        x.iter().zip(&noise).map(|(a, b)| a.iter().zip(b).map(|(u, v)| u + scale * v).collect()).collect()
    }

    fn matrix(rows: &[Vec<f64>]) -> ResponseMatrix {
        ResponseMatrix::from_rows(names(rows[0].len()), Precision::F64, rows).unwrap()
    }

    /// The estimator straight from its definition, with the n x n Gram
    /// matrices: test-only, for small n.
    fn hsic_gram(x: &[Vec<f64>], y: &[Vec<f64>]) -> f64 {
        let n = x.len();
        let gram = |m: &[Vec<f64>]| -> Vec<Vec<f64>> {
            (0..n)
                .map(|i| (0..n).map(|j| if i == j { 0.0 } else { m[i].iter().zip(&m[j]).map(|(a, b)| a * b).sum() }).collect())
                .collect()
        };
        let (k, l) = (gram(x), gram(y));
        let nf = n as f64;
        let tr: f64 = (0..n).map(|i| (0..n).map(|j| k[i][j] * l[j][i]).sum::<f64>()).sum();
        let (sk, sl): (f64, f64) = (k.iter().flatten().sum(), l.iter().flatten().sum());
        let kl: f64 = (0..n).map(|i| k[i].iter().sum::<f64>() * l[i].iter().sum::<f64>()).sum();
        (tr + sk * sl / ((nf - 1.0) * (nf - 2.0)) - 2.0 / (nf - 2.0) * kl) / (nf * (nf - 3.0))
    }

    /// `rows` times a product of Givens rotations of every adjacent column pair.
    fn rotate(rows: &[Vec<f64>], seed: u64) -> Vec<Vec<f64>> {
        let p = rows[0].len();
        let mut rng = Rng::seed_from_u64(seed);
        let mut out = rows.to_vec();
        for a in 0..p - 1 {
            let theta = rng.next_f32() as f64 * std::f64::consts::TAU;
            let (c, s) = (theta.cos(), theta.sin());
            for row in out.iter_mut() {
                let (u, v) = (row[a], row[a + 1]);
                row[a] = c * u - s * v;
                row[a + 1] = s * u + c * v;
            }
        }
        out
    }

    #[test]
    fn hsic_matches_the_gram_matrix_definition() {
        for (k, n) in [4usize, 7, 20, 50].into_iter().enumerate() {
            let x = random_rows(10 + k as u64, n, 6);
            let y = related(&x, 20 + k as u64, 0.3);
            for (a, b) in [(&x, &y), (&x, &x), (&y, &y)] {
                let (got, want) = (hsic_unbiased(&matrix(a), &matrix(b)).unwrap(), hsic_gram(a, b));
                assert!((got - want).abs() <= 1e-12 * want.abs(), "n {n}: {got} vs {want}");
            }
        }
    }

    #[test]
    fn cka_is_exactly_1_on_itself_and_exactly_symmetric() {
        for seed in 0..10u64 {
            let x = random_rows(seed, 30, 8);
            let y = related(&x, seed + 100, 0.7);
            let (mx, my) = (matrix(&x), matrix(&y));
            assert_eq!(cka(&mx, &mx), Ok(1.0));
            assert_eq!(cka(&mx, &my).unwrap().to_bits(), cka(&my, &mx).unwrap().to_bits());
        }
    }

    #[test]
    fn cka_ignores_rotation_and_isotropic_scaling_but_not_column_scaling() {
        let x = random_rows(3, 40, 6);
        let y = related(&x, 4, 0.5);
        let base = cka(&matrix(&x), &matrix(&y)).unwrap();
        let rotated = cka(&matrix(&x), &matrix(&rotate(&y, 5))).unwrap();
        let scaled: Vec<Vec<f64>> = y.iter().map(|r| r.iter().map(|v| v * 2.5).collect()).collect();
        let column: Vec<Vec<f64>> = y.iter().map(|r| r.iter().enumerate().map(|(j, v)| if j == 0 { v * 10.0 } else { *v }).collect()).collect();
        assert!((rotated - base).abs() <= 1e-12, "{rotated} vs {base}");
        assert!((cka(&matrix(&x), &matrix(&scaled)).unwrap() - base).abs() <= 1e-12);
        assert!((cka(&matrix(&x), &matrix(&column)).unwrap() - base).abs() > 1e-3);
    }

    #[test]
    fn a_response_matrix_is_every_frame_from_each_onset_in_battery_order() {
        let mut r = Responses::new(names(2), 0.005, Precision::F32).unwrap();
        r.push("a".into(), 3, vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5]).unwrap();
        r.push("b".into(), 4, vec![3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5]).unwrap();
        let r = r.finish().unwrap();
        let m = ResponseMatrix::from_responses(&r, &[1, 2]).unwrap();
        assert_eq!((m.n_rows(), m.n_cols(), m.precision()), (4, 2, Precision::F32));
        assert_eq!(m.cell_types(), r.cell_types());
        let rows: Vec<Vec<f64>> = (0..4).map(|i| m.row(i).unwrap()).collect();
        assert_eq!(rows, vec![vec![1.0, 1.5], vec![2.0, 2.5], vec![5.0, 5.5], vec![6.0, 6.5]]);
        assert_eq!(m.row(4), None);
        assert_eq!(ResponseMatrix::from_responses(&r, &[1]), Err(SimilarityError::OnsetCount { onsets: 1, stimuli: 2 }));
        assert_eq!(
            ResponseMatrix::from_responses(&r, &[1, 4]),
            Err(SimilarityError::OnsetPastEnd { stimulus: 1, onset: 4, n_frames: 4 })
        );
    }

    #[test]
    fn rows_tagged_f32_must_be_f32s() {
        assert_eq!(
            ResponseMatrix::from_rows(names(2), Precision::F32, &[vec![0.5, 0.1]]),
            Err(SimilarityError::NotF32 { row: 0, column: 1 })
        );
        let m = ResponseMatrix::from_rows(names(2), Precision::F32, &[vec![0.5, 0.25]]).unwrap();
        assert_eq!(m.row(0), Some(vec![0.5, 0.25]));
        // Responses does not check an f32 run's values; from_responses refuses them
        let mut r = Responses::new(names(2), 0.005, Precision::F32).unwrap();
        r.push("a".into(), 2, vec![0.5, 0.25, 0.5, 0.1]).unwrap();
        let r = r.finish().unwrap();
        assert_eq!(ResponseMatrix::from_responses(&r, &[0]), Err(SimilarityError::NotF32 { row: 1, column: 1 }));
    }

    #[test]
    fn matrices_built_by_hand_are_checked() {
        assert_eq!(ResponseMatrix::from_rows(vec![], Precision::F64, &[]), Err(SimilarityError::TooFew { what: "columns", need: 1, found: 0 }));
        assert_eq!(ResponseMatrix::from_rows(names(2), Precision::F64, &[]), Err(SimilarityError::TooFew { what: "rows", need: 1, found: 0 }));
        assert_eq!(
            ResponseMatrix::from_rows(names(2), Precision::F64, &[vec![1.0]]),
            Err(SimilarityError::Length { what: "row", expected: 2, found: 1 })
        );
        assert!(matches!(ResponseMatrix::from_rows(names(1), Precision::F64, &[vec![f64::NAN]]), Err(SimilarityError::NonFinite(_))));
    }

    #[test]
    fn cka_refuses_what_it_cannot_score() {
        let x = matrix(&random_rows(1, 6, 3));
        assert_eq!(cka(&x, &matrix(&random_rows(2, 5, 3))), Err(SimilarityError::RowMismatch { x: 6, y: 5 }));
        assert_eq!(cka(&x, &matrix(&random_rows(2, 6, 4))), Err(SimilarityError::ColumnMismatch { x: 3, y: 4 }));
        let three = matrix(&random_rows(1, 3, 3));
        assert_eq!(cka(&three, &three), Err(SimilarityError::TooFew { what: "rows", need: 4, found: 3 }));
        // a constant matrix has HSIC(X, X) = 0: no scale to divide by
        let flat = matrix(&vec![vec![1.0, 2.0, 3.0]; 6]);
        assert!(matches!(cka(&flat, &x), Err(SimilarityError::NonPositiveHsic { which: "x", .. })), "{:?}", cka(&flat, &x));
        assert!(matches!(cka(&x, &flat), Err(SimilarityError::NonPositiveHsic { which: "y", .. })));
    }

    #[test]
    fn errors_display_as_registered() {
        let cases = [
            (SimilarityError::RowMismatch { x: 6, y: 5 }, "the matrices have 6 and 5 rows"),
            (SimilarityError::ColumnMismatch { x: 65, y: 64 }, "the matrices have 65 and 64 columns"),
            (SimilarityError::TooFew { what: "rows", need: 4, found: 3 }, "rows: need at least 4, found 3"),
            (SimilarityError::NonPositiveHsic { which: "x", value: 0.0 }, "HSIC(x, x) is 0, not positive"),
            (SimilarityError::NonFinite("row 2, column 3".into()), "row 2, column 3 is not finite"),
            (SimilarityError::OnsetCount { onsets: 1, stimuli: 2 }, "1 onsets for 2 stimuli"),
            (SimilarityError::OnsetPastEnd { stimulus: 1, onset: 4, n_frames: 4 }, "stimulus 1: onset 4 is past its 4 frames"),
            (SimilarityError::NotF32 { row: 0, column: 1 }, "row 0, column 1: an f32 run's value is not an f32"),
            (SimilarityError::Length { what: "row", expected: 2, found: 1 }, "row: expected 2, found 1"),
            (SimilarityError::NotSymmetric { i: 0, j: 2 }, "the RDM differs at (0, 2) and (2, 0)"),
            (SimilarityError::Degenerate("an RDM's upper triangle is constant".into()), "an RDM's upper triangle is constant"),
        ];
        for (e, want) in cases {
            assert_eq!(e.to_string(), want);
        }
    }
}
