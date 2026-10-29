//! Which `exp` this host's libm computes, as a fingerprint.
//!
//! The libm gates use Rust's `f32::exp` and `f64::exp`, which call the
//! platform's libm, so their maxima are facts about one host. The record
//! compares its libm section only on a host whose fingerprint is the
//! record's (spec R9). This is an identifier, not a security hash.

use std::hint::black_box;

use crate::pretrained::SYN_COUNT;
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};

/// This host's `exp` of every synapse count, summarised: how many of the
/// 2,355 groups differ in their bits from torch's table, and a 64-bit FNV-1a
/// over the little-endian bits of all 2,355 values, in group order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibmFingerprint {
    pub f32_differ: usize,
    pub f64_differ: usize,
    pub f32_fnv: u64,
    pub f64_fnv: u64,
}

impl std::fmt::Display for LibmFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "f32:{}:{:016x},f64:{}:{:016x}", self.f32_differ, self.f32_fnv, self.f64_differ, self.f64_fnv)
    }
}

/// 64-bit FNV-1a.
fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// This host's `f32::exp` of every synapse count, computed here and not
/// through the gate code. `black_box` keeps the compiler from folding the
/// call at build time, so the value is the running host's libm.
fn exp_f32() -> Vec<f32> {
    SYN_COUNT.iter().map(|&c| f32::exp(black_box(c))).collect()
}

/// As [`exp_f32`], in f64.
fn exp_f64() -> Vec<f64> {
    SYN_COUNT.iter().map(|&c| f64::exp(black_box(c as f64))).collect()
}

/// The fingerprint of this host's libm.
pub fn libm_fingerprint() -> LibmFingerprint {
    let (a, b) = (exp_f32(), exp_f64());
    LibmFingerprint {
        f32_differ: a.iter().zip(&SYN_COUNT_TORCH_F32).filter(|(x, y)| x.to_bits() != y.to_bits()).count(),
        f64_differ: b.iter().zip(&SYN_COUNT_TORCH_F64).filter(|(x, y)| x.to_bits() != y.to_bits()).count(),
        f32_fnv: fnv1a(a.iter().flat_map(|v| v.to_le_bytes())),
        f64_fnv: fnv1a(b.iter().flat_map(|v| v.to_le_bytes())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::Precision;

    #[test]
    fn fnv1a_matches_its_published_test_vectors() {
        assert_eq!(fnv1a(*b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(*b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a(*b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn the_fingerprint_prints_as_registered() {
        let f = LibmFingerprint { f32_differ: 104, f64_differ: 108, f32_fnv: 0xab, f64_fnv: 0x1234_5678_9abc_def0 };
        assert_eq!(f.to_string(), "f32:104:00000000000000ab,f64:108:123456789abcdef0");
    }

    /// If the gate's count formula changes, this fails: the fingerprint does
    /// not silently follow it, so the libm sections cannot switch themselves
    /// off.
    #[test]
    fn the_libm_fingerprint_is_independent_of_the_gate() {
        let (a, b) = (exp_f32(), exp_f64());
        assert_eq!(a.len(), SYN_COUNT.len());
        for g in 0..SYN_COUNT.len() {
            assert_eq!(<f32 as Precision>::libm_count(g).to_bits(), a[g].to_bits(), "group {g}, f32");
            assert_eq!(<f64 as Precision>::libm_count(g).to_bits(), b[g].to_bits(), "group {g}, f64");
        }
    }

    /// Each count is at most 200, as `syn_count_torch_checks` bounds it, and
    /// equals an independent recount. No lower bound: a host whose libm
    /// agrees with torch everywhere is a legitimate host.
    #[test]
    fn the_libm_fingerprint_counts_differences_from_torch() {
        let f = libm_fingerprint();
        let mut f32_differ = 0;
        let mut f64_differ = 0;
        for g in 0..SYN_COUNT.len() {
            if SYN_COUNT[g].exp().to_bits() != SYN_COUNT_TORCH_F32[g].to_bits() {
                f32_differ += 1;
            }
            if (SYN_COUNT[g] as f64).exp().to_bits() != SYN_COUNT_TORCH_F64[g].to_bits() {
                f64_differ += 1;
            }
        }
        assert_eq!((f.f32_differ, f.f64_differ), (f32_differ, f64_differ));
        assert!(f.f32_differ <= 200 && f.f64_differ <= 200, "{f}");
        eprintln!("libm fingerprint on this host: {f}");
    }
}
