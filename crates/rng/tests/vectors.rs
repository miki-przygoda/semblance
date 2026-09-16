//! Frozen stream vectors — the executable half of the stability contract in
//! the crate docs.
//!
//! These values were generated once from the implementation and then frozen.
//! If a change to `semblance-rng` makes this file fail, that change altered
//! the stream and is a breaking change: either revert it, or bump the major
//! version and regenerate these vectors deliberately.
//!
//! Exception: a failure confined to the `gaussian` values, on an unusual
//! host libm, is a portability issue rather than a broken stream — `gaussian`
//! depends on `logf`/`cosf` rounding (see the crate docs' stability contract)
//! and is not bit-frozen the way the integer methods are. Diagnose that
//! before assuming the generator changed.

use semblance_rng::Rng;

type Vector = (u64, [u64; 4], [f32; 4], [usize; 4], [f32; 4]);

const VECTORS: &[Vector] = &[
    (0u64, [5987356902031041503, 7051070477665621255, 6633766593972829180, 211316841551650330], [0.32457525, 0.38223928, 0.35961717, 0.011455476], [3, 55, 80, 30], [-1.1079086, 1.4264823, 1.1755726, 0.31343874]),
    (1u64, [14971601782005023387, 13781649495232077965, 1847458086238483744, 13765271635752736470], [0.8116121, 0.7471047, 0.10015088, 0.74621683], [87, 65, 44, 70], [-0.0117530525, -0.050988797, -1.5489122, -0.16080354]),
    (42u64, [15021278609987233951, 5881210131331364753, 18149643915985481100, 12933668939759105464], [0.8143051, 0.318821, 0.9838941, 0.7011356], [51, 53, 0, 64], [-0.26860726, -0.054462254, -0.5785753, -1.6093373]),
    (11400714819323198485u64, [6409272458699751175, 6888991682673849350, 7292715602953447895, 3353322912996036996], [0.34744734, 0.37345296, 0.3953389, 0.18178397], [75, 50, 95, 96], [-1.0181316, 0.56621087, -0.26571754, -0.59811836]),
    (18446744073709551615u64, [6254647548650071986, 16610832622747802512, 16422857234328439435, 5048281510058307187], [0.33906507, 0.900475, 0.89028484, 0.27366787], [86, 12, 35, 87], [1.1924427, -0.07143011, -0.75056595, -0.49518487]),
];

#[test]
fn stream_matches_frozen_vectors() {
    assert_eq!(VECTORS.len(), 5, "expected one vector per pinned seed");

    for &(seed, u64s, f32s, belows, gaussians) in VECTORS {
        let mut r = Rng::seed_from_u64(seed);
        for (i, &want) in u64s.iter().enumerate() {
            assert_eq!(r.next_u64(), want, "next_u64 #{i} changed for seed {seed:#x}");
        }

        let mut r = Rng::seed_from_u64(seed);
        for (i, &want) in f32s.iter().enumerate() {
            assert_eq!(r.next_f32(), want, "next_f32 #{i} changed for seed {seed:#x}");
        }

        let mut r = Rng::seed_from_u64(seed);
        for (i, &want) in belows.iter().enumerate() {
            assert_eq!(r.below(100), want, "below #{i} changed for seed {seed:#x}");
        }

        let mut r = Rng::seed_from_u64(seed);
        for (i, &want) in gaussians.iter().enumerate() {
            assert_eq!(r.gaussian(), want, "gaussian #{i} changed for seed {seed:#x}");
        }
    }
}
