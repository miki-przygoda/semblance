//! The pre-registered bounds, as written in the specs: equality for torch's
//! counts, 1e-10 in f64 and twice flyvis's own f32/f64 gap in f32 for
//! Rust's `exp`.

use super::run::Counts;
use super::{ParityError, F64_BOUND, SETTLING_F32_GAPS, TRACE_F32_GAPS};
use crate::reference::Dtype;

/// One quantity's bound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bound {
    /// rust == flyvis for every value.
    Exact,
    /// max |rust - flyvis| <= 1e-10.
    F64,
    /// max |rust - flyvis| <= 2.0 * gap. `model` and `dt` are the registered
    /// table's own strings; the PROVENANCE key is "{model} {dt}".
    /// `quantity` is one of "settled", "settling_centre", "centre",
    /// "snapshots".
    F32 { gap: f64, model: &'static str, dt: &'static str, quantity: &'static str },
}

impl Bound {
    /// The largest |rust - flyvis| allowed: 0 for `Exact`.
    pub fn value(&self) -> f64 {
        match self {
            Bound::Exact => 0.0,
            Bound::F64 => F64_BOUND,
            Bound::F32 { gap, .. } => 2.0 * gap,
        }
    }
}

type Gaps = [(&'static str, &'static str, f64, f64); 4];

fn bounds(
    table: &Gaps,
    quantities: [&'static str; 2],
    model: &str,
    dt: &str,
    p: Dtype,
    counts: Counts,
) -> Result<(Bound, Bound), ParityError> {
    let &(model, dt, a, b) = table
        .iter()
        .find(|g| g.0 == model && g.1 == dt)
        .ok_or_else(|| ParityError::Mismatch(format!("no registered bound for model {model} at dt {dt}")))?;
    match (counts, p) {
        (Counts::Torch, _) => Ok((Bound::Exact, Bound::Exact)),
        (Counts::Libm, Dtype::F64) => Ok((Bound::F64, Bound::F64)),
        (Counts::Libm, Dtype::F32) => Ok((
            Bound::F32 { gap: a, model, dt, quantity: quantities[0] },
            Bound::F32 { gap: b, model, dt, quantity: quantities[1] },
        )),
        (Counts::IntegerNSyn, _) => Err(ParityError::Usage(
            "integer counts are a control with its own threshold, not a gate with a bound".into(),
        )),
    }
}

/// The settling bounds for `model` at dt id `dt` in precision `p`:
/// (settled, settling_centre).
pub fn settling_bounds(model: &str, dt: &str, p: Dtype, counts: Counts) -> Result<(Bound, Bound), ParityError> {
    bounds(&SETTLING_F32_GAPS, ["settled", "settling_centre"], model, dt, p, counts)
}

/// The trace bounds for `model` at dt id `dt` in precision `p`:
/// (centre, snapshots).
pub fn trace_bounds(model: &str, dt: &str, p: Dtype, counts: Counts) -> Result<(Bound, Bound), ParityError> {
    bounds(&TRACE_F32_GAPS, ["centre", "snapshots"], model, dt, p, counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::RUNS;

    #[test]
    fn a_bound_is_its_registered_value() {
        assert_eq!(Bound::Exact.value(), 0.0);
        assert_eq!(Bound::F64.value(), 1e-10);
        let b = Bound::F32 { gap: 1.4114366875261908e-06, model: "000", dt: "1/200", quantity: "settled" };
        assert_eq!(b.value().to_bits(), (2.0 * 1.4114366875261908e-06f64).to_bits());
    }

    #[test]
    fn every_registered_run_has_its_bounds() {
        for &(_, model, dt, p) in &RUNS {
            for (f, table, names) in [
                (settling_bounds as fn(&str, &str, Dtype, Counts) -> _, &SETTLING_F32_GAPS, ["settled", "settling_centre"]),
                (trace_bounds, &TRACE_F32_GAPS, ["centre", "snapshots"]),
            ] {
                assert_eq!(f(model, dt, p, Counts::Torch), Ok((Bound::Exact, Bound::Exact)));
                let libm = f(model, dt, p, Counts::Libm).unwrap();
                match p {
                    Dtype::F64 => assert_eq!(libm, (Bound::F64, Bound::F64)),
                    Dtype::F32 => {
                        let row = table.iter().find(|g| g.0 == model && g.1 == dt).unwrap();
                        assert_eq!(libm.0, Bound::F32 { gap: row.2, model: row.0, dt: row.1, quantity: names[0] });
                        assert_eq!(libm.1, Bound::F32 { gap: row.3, model: row.0, dt: row.1, quantity: names[1] });
                        assert_eq!(libm.0.value().to_bits(), (2.0 * row.2).to_bits());
                        assert_eq!(libm.1.value().to_bits(), (2.0 * row.3).to_bits());
                    }
                }
                assert!(matches!(f(model, dt, p, Counts::IntegerNSyn), Err(ParityError::Usage(_))));
            }
        }
    }

    #[test]
    fn an_unregistered_model_or_dt_has_no_bound() {
        for counts in [Counts::Torch, Counts::Libm] {
            assert_eq!(
                settling_bounds("001", "1/50", Dtype::F32, counts),
                Err(ParityError::Mismatch("no registered bound for model 001 at dt 1/50".into()))
            );
            assert_eq!(
                trace_bounds("000", "1/100", Dtype::F64, counts),
                Err(ParityError::Mismatch("no registered bound for model 000 at dt 1/100".into()))
            );
        }
    }
}
