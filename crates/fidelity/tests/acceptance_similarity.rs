//! Acceptance tests for behaviour group `similarity` of epic
//! fidelity-measures: the representational axis, debiased linear CKA and
//! Zhou & Hasler's peak-voltage variant. One test per behaviour id,
//! SIMILARITY-01 .. SIMILARITY-10; SIMILARITY-11 .. SIMILARITY-13 are
//! `semblance validate --ensemble`'s and live in
//! `app/tests/acceptance_similarity.rs`.
//!
//! Every test drives `semblance_fidelity::similarity`'s public API the way a
//! caller would: it builds `Responses`, `ResponseMatrix`es, peak vectors or
//! RDMs by hand, with known answers or an independent reference computed
//! here, and reads what the functions return.

use semblance_fidelity::similarity::{
    cka, cka_biased, cosine_rdm, hsic_unbiased, kendall_tau_a, mean_peaks, peak_vectors, spearman, Rdm, ResponseMatrix,
    ZH_TYPES,
};
use semblance_rng::Rng;
use semblance_sim::response::{Precision, Responses};

/// flyvis's time step, 1/200 s.
const DT: f64 = 1.0 / 200.0;

// ---------------------------------------------------------------------------
// Building inputs
// ---------------------------------------------------------------------------

/// `n` cell type names, `c0` .. `c{n-1}`.
fn numbered(n: usize) -> Vec<String> {
    (0..n).map(|t| format!("c{t}")).collect()
}

/// Responses over `n_types` types: one stimulus per entry of `frames`, each
/// value `fill(stimulus, frame, type)`.
fn responses(n_types: usize, precision: Precision, frames: &[usize], fill: impl Fn(usize, usize, usize) -> f64) -> Responses {
    let mut r = Responses::new(numbered(n_types), DT, precision).expect("responses");
    for (s, &n) in frames.iter().enumerate() {
        let mut values = Vec::with_capacity(n * n_types);
        for f in 0..n {
            for t in 0..n_types {
                values.push(fill(s, f, t));
            }
        }
        r.push(format!("stimulus {s}"), n, values).expect("push");
    }
    r.finish().expect("finish")
}

/// An f32-valued value unique enough per (stimulus, frame, type), and not
/// representable in fewer bits than an f32 has.
fn f32_value(s: usize, f: usize, t: usize) -> f64 {
    (((s * 1000 + f) * 100 + t) as f32 * 1.000_000_1f32 + 0.3f32) as f64
}

/// `n` rows of `p` gaussian values, scaled and shifted per column so the
/// columns differ in mean and spread, as responses do.
fn random_rows(rng: &mut Rng, n: usize, p: usize) -> Vec<Vec<f64>> {
    (0..n)
        .map(|_| (0..p).map(|c| 0.1 * c as f64 + (1.0 + 0.25 * c as f64) * rng.gaussian() as f64).collect())
        .collect()
}

/// `y = x A + noise`: related to `x`, so HSIC between them is well away
/// from zero.
fn related_rows(rng: &mut Rng, x: &[Vec<f64>], q: usize, noise: f64) -> Vec<Vec<f64>> {
    let p = x[0].len();
    let a: Vec<Vec<f64>> = (0..p).map(|_| (0..q).map(|_| rng.gaussian() as f64).collect()).collect();
    x.iter()
        .map(|row| (0..q).map(|j| (0..p).map(|k| row[k] * a[k][j]).sum::<f64>() + noise * rng.gaussian() as f64).collect())
        .collect()
}

fn matrix(rows: &[Vec<f64>]) -> ResponseMatrix {
    ResponseMatrix::from_rows(numbered(rows[0].len()), Precision::F64, rows).expect("matrix")
}

/// Rows rounded to f32, for an F32 matrix.
fn f32_rows(rows: &[Vec<f64>]) -> Vec<Vec<f64>> {
    rows.iter().map(|r| r.iter().map(|&v| v as f32 as f64).collect()).collect()
}

/// Song et al. (2012)'s unbiased HSIC with linear kernels, directly from
/// the n x n Gram matrices: K = X X', L = Y Y', diagonals zeroed, then
/// [tr(KL) + 1'K1 1'L1 / ((n-1)(n-2)) - 2/(n-2) 1'KL1] / (n(n-3)).
fn hsic_gram(x: &[Vec<f64>], y: &[Vec<f64>]) -> f64 {
    let n = x.len();
    let gram = |m: &[Vec<f64>]| -> Vec<Vec<f64>> {
        (0..n)
            .map(|i| (0..n).map(|j| if i == j { 0.0 } else { m[i].iter().zip(&m[j]).map(|(a, b)| a * b).sum() }).collect())
            .collect()
    };
    let (k, l) = (gram(x), gram(y));
    let mut tr = 0.0;
    for i in 0..n {
        for j in 0..n {
            tr += k[i][j] * l[j][i];
        }
    }
    let sum_k: f64 = k.iter().flatten().sum();
    let sum_l: f64 = l.iter().flatten().sum();
    let k1: Vec<f64> = k.iter().map(|r| r.iter().sum()).collect();
    let l1: Vec<f64> = l.iter().map(|r| r.iter().sum()).collect();
    let kl1: f64 = k1.iter().zip(&l1).map(|(a, b)| a * b).sum();
    let nf = n as f64;
    (tr + sum_k * sum_l / ((nf - 1.0) * (nf - 2.0)) - 2.0 / (nf - 2.0) * kl1) / (nf * (nf - 3.0))
}

/// A random p x p orthogonal matrix: Gram-Schmidt on gaussian columns.
fn orthogonal(rng: &mut Rng, p: usize) -> Vec<Vec<f64>> {
    let mut q: Vec<Vec<f64>> = Vec::new();
    while q.len() < p {
        let mut v: Vec<f64> = (0..p).map(|_| rng.gaussian() as f64).collect();
        for _ in 0..2 {
            for u in &q {
                let d: f64 = v.iter().zip(u).map(|(a, b)| a * b).sum();
                for (a, b) in v.iter_mut().zip(u) {
                    *a -= d * b;
                }
            }
        }
        let norm = v.iter().map(|a| a * a).sum::<f64>().sqrt();
        if norm > 1e-6 {
            q.push(v.into_iter().map(|a| a / norm).collect());
        }
    }
    q
}

/// `rows · m`.
fn times(rows: &[Vec<f64>], m: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let q = m[0].len();
    rows.iter().map(|r| (0..q).map(|j| r.iter().zip(m).map(|(a, mr)| a * mr[j]).sum()).collect()).collect()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

/// A peak vector from its non-zero entries.
fn peak(entries: &[(usize, f64)]) -> [f64; ZH_TYPES] {
    let mut v = [0.0; ZH_TYPES];
    for &(i, x) in entries {
        v[i] = x;
    }
    v
}

/// An n x n RDM whose upper triangle, row by row, is `upper`.
fn rdm(n: usize, upper: &[f64]) -> Rdm {
    let mut d = vec![0.0; n * n];
    let mut k = 0;
    for i in 0..n {
        for j in i + 1..n {
            d[i * n + j] = upper[k];
            d[j * n + i] = upper[k];
            k += 1;
        }
    }
    assert_eq!(k, upper.len());
    Rdm::new(n, d).expect("rdm")
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// SIMILARITY-01: rows are (stimulus, frame) from each onset to the
/// stimulus's end, in battery order; columns the 65 types; the grey frames
/// before each onset are left out; an F32 run is held as f32 without loss,
/// anything else as f64.
#[test]
fn similarity_01_the_response_matrix_is_every_post_onset_frame_in_battery_order() {
    let frames = [7usize, 4, 9];
    let onsets = [3usize, 0, 8];
    // Pre-stimulus frames carry a marker no post-onset frame does.
    let fill = |s: usize, f: usize, t: usize| if f < onsets[s] { -999.0 } else { f32_value(s, f, t) };
    let r = responses(ZH_TYPES, Precision::F32, &frames, fill);
    let m = ResponseMatrix::from_responses(&r, &onsets).expect("matrix");

    assert_eq!(m.n_cols(), ZH_TYPES);
    assert_eq!(m.cell_types(), r.cell_types());
    assert_eq!(m.n_rows(), 4 + 4 + 1, "7 - 3, 4 - 0 and 9 - 8 frames");
    assert_eq!(m.precision(), Precision::F32);
    let mut i = 0;
    for (s, (&n, &on)) in frames.iter().zip(&onsets).enumerate() {
        for f in on..n {
            let row = m.row(i).expect("row");
            let want: Vec<f64> = (0..ZH_TYPES).map(|t| f32_value(s, f, t)).collect();
            assert_eq!(row, want, "row {i} is stimulus {s}, frame {f}, bit for bit");
            i += 1;
        }
    }
    assert!(m.row(i).is_none(), "no row past the last post-onset frame");
    for k in 0..m.n_rows() {
        assert!(m.row(k).unwrap().iter().all(|&v| v != -999.0), "no grey pre-stimulus frame is a row");
    }

    // An F64 run keeps values an f32 cannot hold.
    let fine = |s: usize, f: usize, t: usize| f32_value(s, f, t) + 1e-12 * (1 + t) as f64;
    let r64 = responses(ZH_TYPES, Precision::F64, &frames, fine);
    let m64 = ResponseMatrix::from_responses(&r64, &onsets).expect("matrix");
    assert_eq!(m64.precision(), Precision::F64);
    assert_eq!(m64.n_rows(), m.n_rows());
    assert_eq!(m64.row(0).unwrap(), (0..ZH_TYPES).map(|t| fine(0, 3, t)).collect::<Vec<_>>());
    let last = m64.n_rows() - 1;
    assert_eq!(m64.row(last).unwrap(), (0..ZH_TYPES).map(|t| fine(2, 8, t)).collect::<Vec<_>>());
}

/// SIMILARITY-02: the feature-space unbiased HSIC agrees with a direct
/// Gram-matrix implementation on small random matrices (n <= 50) to within
/// 1e-12 relative.
#[test]
fn similarity_02_hsic_unbiased_matches_the_gram_matrix_form() {
    let mut rng = Rng::seed_from_u64(0x5151_0002);
    let mut checked = 0;
    for &(n, p) in &[(4usize, 1usize), (5, 3), (8, 2), (13, 7), (20, 5), (31, 12), (50, 65), (50, 4)] {
        let xr = random_rows(&mut rng, n, p);
        let yr = related_rows(&mut rng, &xr, p, 0.5);
        let (x, y) = (matrix(&xr), matrix(&yr));
        for (a, b, ar, br) in [(&x, &y, &xr, &yr), (&x, &x, &xr, &xr), (&y, &y, &yr, &yr)] {
            let got = hsic_unbiased(a, b).expect("hsic");
            let want = hsic_gram(ar, br);
            assert!(rel(got, want) <= 1e-12, "n {n}, p {p}: feature space {got:e}, Gram {want:e}, rel {:e}", rel(got, want));
            checked += 1;
        }
        // And from an F32 matrix, widened to f64.
        let (xf, yf) = (f32_rows(&xr), f32_rows(&yr));
        let a = ResponseMatrix::from_rows(numbered(p), Precision::F32, &xf).expect("f32 matrix");
        let b = ResponseMatrix::from_rows(numbered(p), Precision::F32, &yf).expect("f32 matrix");
        let (got, want) = (hsic_unbiased(&a, &b).expect("hsic"), hsic_gram(&xf, &yf));
        assert!(rel(got, want) <= 1e-12, "f32, n {n}, p {p}: {got:e} vs {want:e}");
    }
    assert_eq!(checked, 24);
}

/// SIMILARITY-03: `cka` is `H(X,Y) / sqrt(H(X,X) H(Y,Y))`, each H the same
/// `hsic_unbiased`, H(X,X) called with the same argument twice.
#[test]
fn similarity_03_cka_is_hsic_xy_over_the_root_of_hsic_xx_hsic_yy() {
    let mut rng = Rng::seed_from_u64(0x5151_0003);
    for &(n, p) in &[(4usize, 2usize), (12, 5), (40, 65)] {
        let xr = random_rows(&mut rng, n, p);
        let yr = related_rows(&mut rng, &xr, p, 1.0);
        let (x, y) = (matrix(&xr), matrix(&yr));
        let h = |a: &ResponseMatrix, b: &ResponseMatrix| hsic_unbiased(a, b).expect("hsic");
        let want = h(&x, &y) / (h(&x, &x) * h(&y, &y)).sqrt();
        let got = cka(&x, &y).expect("cka");
        assert_eq!(got.to_bits(), want.to_bits(), "n {n}, p {p}: cka {got:e}, H/sqrt(HH) {want:e}");
    }
}

/// SIMILARITY-04: `cka(x, x)` is exactly 1.0 for any valid x, F32 or F64,
/// with no special case: a copy of x, a distinct value, gives 1.0 too.
#[test]
fn similarity_04_cka_of_a_matrix_with_itself_is_exactly_one() {
    let mut rng = Rng::seed_from_u64(0x5151_0004);
    let mut checked = 0;
    for n in [4usize, 5, 9, 17, 50, 200] {
        for p in [1usize, 2, 7, 65] {
            let rows = random_rows(&mut rng, n, p);
            for (precision, rows) in [(Precision::F64, rows.clone()), (Precision::F32, f32_rows(&rows))] {
                let x = ResponseMatrix::from_rows(numbered(p), precision, &rows).expect("matrix");
                // Unbiased HSIC(X, X) can fall to 0 or below on a tiny
                // sample; such an x has no CKA and is not a valid input.
                if hsic_unbiased(&x, &x).expect("hsic") <= 0.0 {
                    assert!(cka(&x, &x).is_err(), "n {n}, p {p}: no scale, no score");
                    continue;
                }
                assert_eq!(cka(&x, &x).expect("cka"), 1.0, "n {n}, p {p}, {precision:?}");
                let copy = ResponseMatrix::from_rows(numbered(p), precision, &rows).expect("matrix");
                assert_eq!(cka(&x, &copy).expect("cka"), 1.0, "n {n}, p {p}, {precision:?}, a separate copy");
                checked += 1;
            }
        }
    }
    // A response matrix from a real Responses, too.
    let r = responses(ZH_TYPES, Precision::F32, &[30, 25], f32_value);
    let x = ResponseMatrix::from_responses(&r, &[5, 5]).expect("matrix");
    assert_eq!(cka(&x, &x).expect("cka"), 1.0);
    assert!(checked >= 40, "only {checked} of 48 matrices were valid");
}

/// SIMILARITY-05: `cka(a, b)` and `cka(b, a)` are the same bits.
#[test]
fn similarity_05_cka_is_symmetric_bit_for_bit() {
    let mut rng = Rng::seed_from_u64(0x5151_0005);
    for n in [4usize, 6, 11, 50, 300] {
        for p in [1usize, 3, 65] {
            let ar = random_rows(&mut rng, n, p);
            for br in [random_rows(&mut rng, n, p), related_rows(&mut rng, &ar, p, 0.3)] {
                for precision in [Precision::F64, Precision::F32] {
                    let (ar, br) = if precision == Precision::F32 { (f32_rows(&ar), f32_rows(&br)) } else { (ar.clone(), br.clone()) };
                    let a = ResponseMatrix::from_rows(numbered(p), precision, &ar).expect("matrix");
                    let b = ResponseMatrix::from_rows(numbered(p), precision, &br).expect("matrix");
                    let (ab, ba) = match (cka(&a, &b), cka(&b, &a)) {
                        (Ok(ab), Ok(ba)) => (ab, ba),
                        (ab, ba) => {
                            assert!(ab.is_err() && ba.is_err(), "both orders refuse alike: {ab:?} vs {ba:?}");
                            continue;
                        }
                    };
                    assert_eq!(ab.to_bits(), ba.to_bits(), "n {n}, p {p}, {precision:?}: {ab:e} vs {ba:e}");
                    let (hab, hba) = (hsic_unbiased(&a, &b).unwrap(), hsic_unbiased(&b, &a).unwrap());
                    assert_eq!(hab.to_bits(), hba.to_bits(), "HSIC too");
                }
            }
        }
    }
}

/// SIMILARITY-06: CKA is unchanged, to within 1e-12, by an orthogonal
/// transform or an isotropic scaling of one argument, and changed by a
/// per-column scaling.
#[test]
fn similarity_06_cka_is_invariant_to_rotation_and_isotropic_scaling_only() {
    let mut rng = Rng::seed_from_u64(0x5151_0006);
    for &(n, p) in &[(10usize, 3usize), (40, 8), (120, 65)] {
        let xr = random_rows(&mut rng, n, p);
        let yr = related_rows(&mut rng, &xr, p, 1.0);
        let (x, y) = (matrix(&xr), matrix(&yr));
        let base = cka(&x, &y).expect("cka");

        let q = orthogonal(&mut rng, p);
        let rotated = cka(&x, &matrix(&times(&yr, &q))).expect("cka");
        assert!((rotated - base).abs() <= 1e-12, "n {n}, p {p}: rotated {rotated:e} vs {base:e}");
        let rotated_x = cka(&matrix(&times(&xr, &q)), &y).expect("cka");
        assert!((rotated_x - base).abs() <= 1e-12, "n {n}, p {p}: x rotated {rotated_x:e} vs {base:e}");

        for c in [0.001, 0.37, 3.0, 1e4] {
            let scaled: Vec<Vec<f64>> = yr.iter().map(|r| r.iter().map(|v| v * c).collect()).collect();
            let s = cka(&x, &matrix(&scaled)).expect("cka");
            assert!((s - base).abs() <= 1e-12, "n {n}, p {p}, scale {c}: {s:e} vs {base:e}");
        }

        if p > 1 {
            let per_col: Vec<Vec<f64>> =
                yr.iter().map(|r| r.iter().enumerate().map(|(j, v)| v * (1.0 + 3.0 * j as f64)).collect()).collect();
            let s = cka(&x, &matrix(&per_col)).expect("cka");
            assert!((s - base).abs() > 1e-6, "n {n}, p {p}: a per-column scaling changes CKA ({s:e} vs {base:e})");
        }
    }
}

/// SIMILARITY-07: per condition, the peak raw central voltage per type over
/// the whole trace, grey frames included, not rectified: 65 values.
#[test]
fn similarity_07_peak_vectors_are_each_types_raw_maximum_over_the_whole_trace() {
    let frames = [6usize, 3, 10];
    // Type t's value: negative everywhere for odd t (the peak is the least
    // negative), with a spike at frame (s + t) % n.
    let fill = |s: usize, f: usize, t: usize| {
        let base = if t % 2 == 1 { -5.0 - t as f64 } else { 0.25 * t as f64 - 1.0 } + 0.01 * f as f64;
        let n = frames[s];
        if f == (s + t) % n { base + 2.0 + s as f64 } else { base }
    };
    let r = responses(ZH_TYPES, Precision::F64, &frames, fill);
    let v = peak_vectors(&r).expect("peaks");
    assert_eq!(v.len(), frames.len(), "one vector per condition");
    for (s, &n) in frames.iter().enumerate() {
        assert_eq!(v[s].len(), ZH_TYPES);
        for (t, &got) in v[s].iter().enumerate() {
            let want = (0..n).map(|f| fill(s, f, t)).fold(f64::NEG_INFINITY, f64::max);
            assert_eq!(got, want, "stimulus {s}, type {t}");
        }
    }
    // Odd types stay negative: no rectification.
    assert!(v[0][1] < 0.0 && v[1][3] < 0.0);
    // A peak at frame 0, the first (grey) frame, counts: type 0 of stimulus 0.
    assert_eq!(v[0][0], fill(0, 0, 0));
}

/// SIMILARITY-08: the cosine RDM, `1 - a.b / (|a| |b| + 1e-10)`, against
/// hand-computed small cases.
#[test]
fn similarity_08_cosine_rdm_matches_hand_computed_cases() {
    let e0 = peak(&[(0, 1.0)]);
    let e1 = peak(&[(1, 1.0)]);
    let both = peak(&[(0, 1.0), (1, 1.0)]);
    let twice = peak(&[(0, 2.0)]);
    let opposite = peak(&[(0, -3.0)]);
    let three_four = peak(&[(10, 3.0), (64, 4.0)]);
    let four_three = peak(&[(10, 4.0), (64, 3.0)]);
    let d = cosine_rdm(&[e0, e1, both, twice, opposite, three_four, four_three]).expect("rdm");
    assert_eq!(d.n(), 7);

    let cases: [(usize, usize, f64); 7] = [
        (0, 1, 1.0 - 0.0 / (1.0 + 1e-10)),                     // orthogonal
        (0, 2, 1.0 - 1.0 / (2f64.sqrt() + 1e-10)),             // 45 degrees
        (0, 3, 1.0 - 2.0 / (2.0 + 1e-10)),                     // parallel: not 0, by the epsilon
        (0, 4, 1.0 - (-3.0) / (3.0 + 1e-10)),                  // opposite
        (3, 4, 1.0 - (-6.0) / (6.0 + 1e-10)),                  // opposite, scaled
        (5, 6, 1.0 - 24.0 / (25.0 + 1e-10)),                   // 3-4-5
        (1, 5, 1.0 - 0.0 / (5.0 + 1e-10)),                     // disjoint support
    ];
    for (i, j, want) in cases {
        let got = d.get(i, j).expect("entry");
        assert!((got - want).abs() <= 1e-15, "({i}, {j}): {got:e}, by hand {want:e}");
        assert_eq!(d.get(j, i), Some(got), "symmetric at ({i}, {j})");
    }
    // The epsilon is there: a parallel pair is about 5e-11, not 0.
    let parallel = d.get(0, 3).unwrap();
    assert!(parallel > 4e-11 && parallel < 6e-11, "parallel pair {parallel:e}");
    for i in 0..7 {
        assert_eq!(d.get(i, i), Some(0.0), "diagonal {i}");
    }
}

/// SIMILARITY-09: Spearman (ties averaged) and Kendall tau-A over the upper
/// triangle, against hand-computed small cases, ties included.
#[test]
fn similarity_09_spearman_and_kendall_tau_a_match_hand_computed_cases() {
    let up = rdm(4, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

    // No ties, two adjacent swaps: sum d^2 = 4, rho = 1 - 6*4/(6*35) = 31/35;
    // 15 pairs, 2 discordant: tau = 11/15.
    let swapped = rdm(4, &[1.0, 3.0, 2.0, 4.0, 6.0, 5.0]);
    assert!((spearman(&up, &swapped).unwrap() - 31.0 / 35.0).abs() <= 1e-12);
    assert_eq!(kendall_tau_a(&up, &swapped).unwrap(), 11.0 / 15.0);

    // Ties in one: ranks [1.5, 1.5, 3, 5, 5, 5] against 1..6:
    // rho = 15 / sqrt(15 * 17.5) = sqrt(6/7); 4 tied pairs count as neither,
    // 11 concordant: tau-a = 11/15 (tau-b would not be).
    let tied = rdm(4, &[0.1, 0.1, 0.2, 0.3, 0.3, 0.3]);
    assert!((spearman(&tied, &up).unwrap() - (6.0f64 / 7.0).sqrt()).abs() <= 1e-12);
    assert!((spearman(&up, &tied).unwrap() - (6.0f64 / 7.0).sqrt()).abs() <= 1e-12);
    assert_eq!(kendall_tau_a(&tied, &up).unwrap(), 11.0 / 15.0);

    // Ties in both: [1,1,2,2,3,3] vs [1,1,1,2,2,2]. Ranks [1.5,1.5,3.5,3.5,5.5,5.5]
    // and [2,2,2,5,5,5]: deviations [-2,-2,0,0,2,2] and [-1.5]*3,[1.5]*3;
    // cov 12, var 16 and 13.5: rho = 12 / sqrt(216) = sqrt(2/3).
    // Pairs: concordant 8, discordant 0 (ties in either: 2 + 6 - 1 = 7): 8/15.
    let a = rdm(4, &[1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    let b = rdm(4, &[1.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
    assert!((spearman(&a, &b).unwrap() - (2.0f64 / 3.0).sqrt()).abs() <= 1e-12);
    assert_eq!(kendall_tau_a(&a, &b).unwrap(), 8.0 / 15.0);

    // Reversed: -1 both ways.
    let down = rdm(4, &[6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
    assert!((spearman(&up, &down).unwrap() + 1.0).abs() <= 1e-12);
    assert_eq!(kendall_tau_a(&up, &down).unwrap(), -1.0);
    assert!((spearman(&up, &up).unwrap() - 1.0).abs() <= 1e-12);
    assert_eq!(kendall_tau_a(&up, &up).unwrap(), 1.0);

    // Only the upper triangle is read: the diagonal is 0 by construction,
    // and spearman on 3x3 (3 entries) agrees with the hand value 1 - 6*2/24.
    let s3 = spearman(&rdm(3, &[1.0, 2.0, 3.0]), &rdm(3, &[2.0, 1.0, 3.0])).unwrap();
    assert!((s3 - 0.5).abs() <= 1e-12, "{s3}");
    assert_eq!(kendall_tau_a(&rdm(3, &[1.0, 2.0, 3.0]), &rdm(3, &[2.0, 1.0, 3.0])).unwrap(), 1.0 / 3.0);
}

/// SIMILARITY-10: on ensemble-mean peak vectors, Zhou & Hasler's biased
/// linear CKA (columns centred, `|Xc'Yc|^2 / (|Xc'Xc| |Yc'Yc|)`); a single
/// reported number, no permutation test and no p-value.
#[test]
fn similarity_10_cka_biased_on_ensemble_mean_peaks() {
    let mut rng = Rng::seed_from_u64(0x5151_0010);
    let conditions = 24;
    // Two "ensembles" of 5 models each, built through Responses -> peak_vectors.
    let ensemble = |rng: &mut Rng, offset: f64| -> Vec<Vec<[f64; ZH_TYPES]>> {
        (0..5)
            .map(|_| {
                let frames = vec![4usize; conditions];
                let noise: Vec<f64> = (0..conditions * 4 * ZH_TYPES).map(|_| rng.gaussian() as f64).collect();
                let r = responses(ZH_TYPES, Precision::F64, &frames, |s, f, t| {
                    offset + ((s * 7 + t * 3) % 11) as f64 * 0.1 + 0.3 * noise[(s * 4 + f) * ZH_TYPES + t]
                });
                peak_vectors(&r).expect("peaks")
            })
            .collect()
    };
    let (ma, mb) = (ensemble(&mut rng, 0.0), ensemble(&mut rng, 2.0));
    let (x, y) = (mean_peaks(&ma).expect("mean"), mean_peaks(&mb).expect("mean"));

    // The ensemble means are the elementwise means.
    for c in 0..conditions {
        for t in 0..ZH_TYPES {
            let want = ma.iter().map(|m| m[c][t]).sum::<f64>() / 5.0;
            assert!((x[c][t] - want).abs() <= 1e-15);
        }
    }

    // By hand, in the Gram form: Kc = Xc Xc', tr(Kc Lc) / sqrt(tr(Kc Kc) tr(Lc Lc)).
    let centred = |m: &[[f64; ZH_TYPES]]| -> Vec<Vec<f64>> {
        let n = m.len() as f64;
        let means: Vec<f64> = (0..ZH_TYPES).map(|t| m.iter().map(|r| r[t]).sum::<f64>() / n).collect();
        m.iter().map(|r| (0..ZH_TYPES).map(|t| r[t] - means[t]).collect()).collect()
    };
    let gram = |m: &[Vec<f64>]| -> Vec<Vec<f64>> {
        m.iter().map(|a| m.iter().map(|b| a.iter().zip(b).map(|(u, v)| u * v).sum()).collect()).collect()
    };
    let tr = |a: &[Vec<f64>], b: &[Vec<f64>]| -> f64 {
        let mut s = 0.0;
        for i in 0..a.len() {
            for j in 0..a.len() {
                s += a[i][j] * b[j][i];
            }
        }
        s
    };
    let (k, l) = (gram(&centred(&x)), gram(&centred(&y)));
    let want = tr(&k, &l) / (tr(&k, &k) * tr(&l, &l)).sqrt();

    let got: f64 = cka_biased(&x, &y).expect("biased cka"); // one number: no p-value comes back
    assert!(rel(got, want) <= 1e-12, "biased CKA {got:e}, by hand {want:e}");
    assert!(got > 0.0 && got <= 1.0);
    assert_eq!(cka_biased(&y, &x).unwrap().to_bits(), got.to_bits(), "symmetric");
    assert!((cka_biased(&x, &x).unwrap() - 1.0).abs() <= 1e-12);

    // Biased, not the debiased CKA: on the same data as ResponseMatrix rows,
    // the two differ.
    let as_rows = |m: &[[f64; ZH_TYPES]]| -> Vec<Vec<f64>> { m.iter().map(|r| r.to_vec()).collect() };
    let debiased = cka(&matrix(&as_rows(&x)), &matrix(&as_rows(&y))).expect("cka");
    assert!((debiased - got).abs() > 1e-9, "biased {got:e} vs debiased {debiased:e}");
}
