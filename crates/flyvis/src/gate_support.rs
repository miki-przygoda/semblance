//! The gates' fixture: read once through `test_support`, in the order the
//! gates have always read it, then built by `parity::Fixtures::from_parts`.
//!
//! What the gates compute lives in `crate::parity`, which `semblance
//! validate` runs too (spec
//! docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md,
//! section 2).

use std::sync::OnceLock;

use crate::parity::Fixtures;
use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text, REAL_SPEC};

const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

/// The fixture's files, read but not parsed.
struct Parts {
    ensemble: String,
    manifest: String,
    settling: Vec<(&'static str, Vec<u8>)>,
    spec: String,
}

static FIXTURE: OnceLock<Result<Fixtures, String>> = OnceLock::new();

/// Everything the gates read, built once. Every file is loaded (or found
/// missing) before anything is built from it. A missing file makes this
/// `None`, and every test that asks prints the notice.
pub(crate) fn fixture() -> Option<&'static Fixtures> {
    or_skip(FIXTURE.get_or_init(|| build(read())).as_ref().map_err(String::clone))
}

/// Reads ensemble.csv, manifest.json, the four settling files and the spec,
/// in that order. A missing file is its notice, unprinted.
fn read() -> Result<Parts, String> {
    let ensemble = read_fixture_text("flyvis-pretrained/ensemble.csv")?;
    let manifest = read_fixture_text("flyvis-reference/manifest.json")?;
    let mut settling = Vec::new();
    for name in SETTLING_FILES {
        settling.push((name, read_fixture_bytes(&format!("flyvis-reference/{name}"))?));
    }
    let spec = read_fixture_text(REAL_SPEC)?;
    Ok(Parts { ensemble, manifest, settling, spec })
}

/// The `Err` is only ever a missing file's notice, so a gate skips only
/// when a file is missing (spec R21). A file that is present but malformed
/// panics here: the `OnceLock` stays empty, and every gate that asks fails
/// with the error, with or without `SEMBLANCE_REQUIRE_FIXTURES`.
fn build(reads: Result<Parts, String>) -> Result<Fixtures, String> {
    let p = reads?;
    match Fixtures::from_parts(&p.spec, &p.ensemble, &p.manifest, p.settling) {
        Ok(f) => Ok(f),
        Err(e) => panic!("{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_a_skip_notice() {
        assert_eq!(build(Err("x not found. y".into())), Err("x not found. y".to_string()));
    }

    #[test]
    #[should_panic(expected = "flyvis-pretrained/ensemble.csv: ensemble.csv line 1: no header line")]
    fn a_malformed_fixture_fails_rather_than_skips() {
        let empty = Parts { ensemble: String::new(), manifest: String::new(), settling: Vec::new(), spec: String::new() };
        let _ = build(Ok(empty));
    }
}
