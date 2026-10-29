//! Test-only helpers shared across this crate's test modules.
//!
//! Kept as its own module so the download URL for the real flyvis file lives
//! in exactly one place, instead of being copy-pasted into every test module
//! that needs to skip when the real data is absent.
//!
//! Two layers. `read_fixture_bytes` / `read_fixture_text` return a missing
//! file's notice as `Err` without printing it, so a fixture built once (the
//! gates' `OnceLock`s) can hand the same notice to every test that asks.
//! The `load_*` loaders wrap them for a test that loads its own file: they
//! print `skipping: <notice>` and return `None`.

use semblance_connectome::TypedNetwork;

use crate::spec::ConnectomeSpec;

/// Whether missing fixtures must fail the suite instead of skipping it.
///
/// libtest captures `eprintln!` for *passing* tests, so a skip notice is
/// invisible under plain `cargo test`: with the data absent the suite reports
/// the same pass count as a full run. Measured, not assumed.
///
/// Set `SEMBLANCE_REQUIRE_FIXTURES=1` — as CI must — to turn every skip into a
/// failure. Without it the acceptance gates can silently verify nothing.
fn fixtures_required() -> bool {
    std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1")
}

/// The real, published flyvis connectome spec, under `data/`.
const REAL_SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";

/// `rel`'s path under the workspace's `data/`, as every notice prints it.
fn data_path(rel: &str) -> String {
    format!("{}/../../data/{rel}", env!("CARGO_MANIFEST_DIR"))
}

/// How to produce a file under `data/`, by its directory.
fn how_to_get(rel: &str) -> &'static str {
    match rel.split('/').next() {
        Some("flyvis-supplementary") => {
            "Download \
             https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
             and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/, \
             or run ci/fetch-fixtures.sh"
        }
        Some("flyvis-build") => {
            "Run `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-stimuli") => {
            "Run `uv run --project tools tools/export_stimuli.py`, \
             `uv run --project tools tools/export_moving_edges.py` and, \
             for the `_h80` files, `uv run --project tools tools/export_reference.py` \
             to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-pretrained") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_params.py`."
        }
        Some("flyvis-reference") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        }
        _ => "Run ci/fetch-fixtures.sh.",
    }
}

/// A missing fixture's notice, `<path> not found. <how to get it>`, without
/// the `skipping: ` its printer adds. Under `SEMBLANCE_REQUIRE_FIXTURES=1`
/// this panics instead, naming the path.
fn missing(path: &str, rel: &str) -> String {
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    format!("{path} not found. {}", how_to_get(rel))
}

/// The bytes of `data/<rel>`, or its missing-fixture notice. Prints nothing.
pub(crate) fn read_fixture_bytes(rel: &str) -> Result<Vec<u8>, String> {
    let path = data_path(rel);
    std::fs::read(&path).map_err(|_| missing(&path, rel))
}

/// The text of `data/<rel>`, or its missing-fixture notice. Prints nothing.
pub(crate) fn read_fixture_text(rel: &str) -> Result<String, String> {
    let path = data_path(rel);
    std::fs::read_to_string(&path).map_err(|_| missing(&path, rel))
}

/// `Some` of a loaded fixture, or `None` after printing
/// `skipping: <notice>`. Every gate test that skips goes through this, so
/// each one prints its own notice (visible under `--nocapture`).
pub(crate) fn or_skip<T>(loaded: Result<T, String>) -> Option<T> {
    match loaded {
        Ok(v) => Some(v),
        Err(notice) => {
            eprintln!("skipping: {notice}");
            None
        }
    }
}

/// Loads the real, published flyvis connectome spec as a string, or `None`
/// if it hasn't been fetched. The file is gitignored real data (see
/// `RESOURCES.md` for how to get it); tests that depend on it must skip via
/// this path rather than failing outright.
pub(crate) fn load_real_spec() -> Option<String> {
    or_skip(read_fixture_text(REAL_SPEC))
}

/// Loads a fixture exported from flyvis's own build, or `None` if the export
/// has not been run. See `tools/export_build.py`.
pub(crate) fn load_build_fixture(name: &str) -> Option<String> {
    or_skip(read_fixture_text(&format!("flyvis-build/{name}")))
}

/// Loads a binary fixture exported from flyvis's own build, or `None` if the
/// export has not been run. See `tools/export_build.py`.
pub(crate) fn load_build_bytes(name: &str) -> Option<Vec<u8>> {
    or_skip(read_fixture_bytes(&format!("flyvis-build/{name}")))
}

/// Loads a stimulus fixture exported from flyvis, or `None` if the export has
/// not been run. See `tools/export_stimuli.py`, `tools/export_moving_edges.py`
/// and, for the `_h80` files, `tools/export_reference.py`.
pub(crate) fn load_stimulus_fixture(name: &str) -> Option<String> {
    or_skip(read_fixture_text(&format!("flyvis-stimuli/{name}")))
}

/// Loads a binary stimulus fixture, or `None` if the export has not been run.
pub(crate) fn load_stimulus_bytes(name: &str) -> Option<Vec<u8>> {
    or_skip(read_fixture_bytes(&format!("flyvis-stimuli/{name}")))
}

/// Loads a file from `data/flyvis-pretrained/`, or `None` if it is absent.
/// See `tools/export_params.py`.
pub(crate) fn load_pretrained_fixture(name: &str) -> Option<String> {
    or_skip(read_fixture_text(&format!("flyvis-pretrained/{name}")))
}

/// The real spec and its expansion, or the missing spec's notice. Prints
/// nothing.
pub(crate) fn read_real_spec_and_network() -> Result<(ConnectomeSpec, TypedNetwork), String> {
    let raw = read_fixture_text(REAL_SPEC)?;
    let spec = ConnectomeSpec::from_json_str(&raw).expect("real spec must load");
    let net = crate::expand::expand(&spec, crate::expand::DEFAULT_EXTENT, crate::expand::DEFAULT_N_SYN_FILL)
        .expect("real spec must expand");
    Ok((spec, net))
}

/// The real spec and its expansion, or `None` if the spec has not been
/// fetched. Every gate that needs the real network builds it here.
pub(crate) fn real_spec_and_network() -> Option<(ConnectomeSpec, TypedNetwork)> {
    or_skip(read_real_spec_and_network())
}

/// `data/flyvis-reference/`, from `tools/export_reference.py`.
pub(crate) fn reference_dir() -> String {
    data_path("flyvis-reference")
}

/// A text file from `data/flyvis-reference/`, or `None` if it is absent.
pub(crate) fn load_reference_fixture(name: &str) -> Option<String> {
    or_skip(read_fixture_text(&format!("flyvis-reference/{name}")))
}

/// A binary file from `data/flyvis-reference/`, or `None` if it is absent.
pub(crate) fn load_reference_bytes(name: &str) -> Option<Vec<u8>> {
    or_skip(read_fixture_bytes(&format!("flyvis-reference/{name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing file is an `Err` carrying its notice, unprinted and without
    /// the `skipping: ` prefix; under `SEMBLANCE_REQUIRE_FIXTURES=1` it is a
    /// panic instead.
    #[test]
    fn a_missing_fixture_is_an_error_naming_its_path() {
        let rel = "flyvis-reference/no-such-file.bin";
        match std::panic::catch_unwind(|| (read_fixture_bytes(rel), read_fixture_text(rel))) {
            Ok((bytes, text)) => {
                assert!(!fixtures_required(), "under SEMBLANCE_REQUIRE_FIXTURES=1 this must panic");
                for notice in [bytes.expect_err("the file is absent"), text.expect_err("the file is absent")] {
                    assert!(
                        notice.ends_with(
                            "data/flyvis-reference/no-such-file.bin not found. Run ci/fetch-fixtures.sh, \
                             or `uv run --project tools tools/export_reference.py`."
                        ),
                        "{notice}"
                    );
                    assert!(!notice.starts_with("skipping"), "the printer adds the prefix: {notice}");
                }
            }
            Err(_) => assert!(fixtures_required(), "only SEMBLANCE_REQUIRE_FIXTURES=1 makes this panic"),
        }
    }
}
