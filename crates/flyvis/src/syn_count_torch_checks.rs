//! The torch-computed synapse counts must be `exp` of `SYN_COUNT`, to within
//! the last bit or two -- and must differ from Rust's own `exp` on only the
//! few values where torch's and the C library's rounding disagree. A table
//! written from the wrong source (the logs themselves, f32 values widened
//! into the f64 table, another model's counts) fails one or the other.

use crate::pretrained::SYN_COUNT;
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};

/// At most this many entries may differ from std's `exp`. The 2026-10-28
/// measurement found about 105 of 2,355 in each precision.
const MAX_DIFFERING: usize = 200;

#[test]
fn the_f32_table_is_exp_of_syn_count_to_within_two_ulps() {
    let mut differ = 0;
    for (i, (&ln, &torch)) in SYN_COUNT.iter().zip(SYN_COUNT_TORCH_F32.iter()).enumerate() {
        let libm = ln.exp();
        let ulps = (torch.to_bits() as i64 - libm.to_bits() as i64).unsigned_abs();
        assert!(ulps <= 2, "entry {i}: torch {torch:e}, libm {libm:e}, {ulps} ulps apart");
        differ += usize::from(ulps != 0);
    }
    assert!(differ <= MAX_DIFFERING, "{differ} entries differ from f32::exp");
}

#[test]
fn the_f64_table_is_exp_of_syn_count_to_within_two_ulps() {
    let mut differ = 0;
    for (i, (&ln, &torch)) in SYN_COUNT.iter().zip(SYN_COUNT_TORCH_F64.iter()).enumerate() {
        let libm = (ln as f64).exp();
        let ulps = (torch.to_bits() as i128 - libm.to_bits() as i128).unsigned_abs();
        assert!(ulps <= 2, "entry {i}: torch {torch:e}, libm {libm:e}, {ulps} ulps apart");
        differ += usize::from(ulps != 0);
    }
    assert!(differ <= MAX_DIFFERING, "{differ} entries differ from f64::exp");
}
