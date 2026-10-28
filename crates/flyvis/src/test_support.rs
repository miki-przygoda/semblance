//! Test-only helpers shared across this crate's test modules.
//!
//! Kept as its own module so the download URL for the real flyvis file lives
//! in exactly one place, instead of being copy-pasted into every test module
//! that needs to skip when the real data is absent.

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

/// Loads the real, published flyvis connectome spec as a string, or `None`
/// if it hasn't been fetched. The file is gitignored real data (see
/// `RESOURCES.md` for how to get it); tests that depend on it must skip via
/// this path rather than failing outright.
pub(crate) fn load_real_spec() -> Option<String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json"
    );
    match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Download \
                 https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
                 and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/, \
                 or run ci/fetch-fixtures.sh"
            );
            None
        }
    }
}

/// Loads a fixture exported from flyvis's own build, or `None` if the export
/// has not been run. See `tools/export_build.py`.
pub(crate) fn load_build_fixture(name: &str) -> Option<String> {
    let path = format!(
        "{}/../../data/flyvis-build/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
            );
            None
        }
    }
}

/// Loads a binary fixture exported from flyvis's own build, or `None` if the
/// export has not been run. See `tools/export_build.py`.
pub(crate) fn load_build_bytes(name: &str) -> Option<Vec<u8>> {
    let path = format!(
        "{}/../../data/flyvis-build/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
            );
            None
        }
    }
}

/// Loads a stimulus fixture exported from flyvis, or `None` if the export has
/// not been run. See `tools/export_stimuli.py`, `tools/export_moving_edges.py`
/// and, for the `_h80` files, `tools/export_reference.py`.
pub(crate) fn load_stimulus_fixture(name: &str) -> Option<String> {
    let path = format!(
        "{}/../../data/flyvis-stimuli/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_stimuli.py`, \
                 `uv run --project tools tools/export_moving_edges.py` and, \
                 for the `_h80` files, `uv run --project tools tools/export_reference.py` \
                 to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
            );
            None
        }
    }
}

/// Loads a binary stimulus fixture, or `None` if the export has not been run.
pub(crate) fn load_stimulus_bytes(name: &str) -> Option<Vec<u8>> {
    let path = format!(
        "{}/../../data/flyvis-stimuli/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_stimuli.py`, \
                 `uv run --project tools tools/export_moving_edges.py` and, \
                 for the `_h80` files, `uv run --project tools tools/export_reference.py` \
                 to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
            );
            None
        }
    }
}

/// Loads a file from `data/flyvis-pretrained/`, or `None` if it is absent.
/// See `tools/export_params.py`.
pub(crate) fn load_pretrained_fixture(name: &str) -> Option<String> {
    let path = format!(
        "{}/../../data/flyvis-pretrained/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Run ci/fetch-fixtures.sh, or \
                 `uv run --project tools tools/export_params.py`."
            );
            None
        }
    }
}

/// The real spec and its expansion, or `None` if the spec has not been
/// fetched. Every gate that needs the real network builds it here.
pub(crate) fn real_spec_and_network(
) -> Option<(crate::spec::ConnectomeSpec, semblance_connectome::TypedNetwork)> {
    let raw = load_real_spec()?;
    let spec = crate::spec::ConnectomeSpec::from_json_str(&raw).expect("real spec must load");
    let net = crate::expand::expand(&spec, crate::expand::DEFAULT_EXTENT, crate::expand::DEFAULT_N_SYN_FILL)
        .expect("real spec must expand");
    Some((spec, net))
}

/// `data/flyvis-reference/`, from `tools/export_reference.py`.
pub(crate) fn reference_dir() -> String {
    format!("{}/../../data/flyvis-reference", env!("CARGO_MANIFEST_DIR"))
}

fn reference_missing(path: &str) {
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    eprintln!(
        "skipping: {path} not found. Run ci/fetch-fixtures.sh, or \
         `uv run --project tools tools/export_reference.py`."
    );
}

/// A text file from `data/flyvis-reference/`, or `None` if it is absent.
pub(crate) fn load_reference_fixture(name: &str) -> Option<String> {
    let path = format!("{}/{name}", reference_dir());
    std::fs::read_to_string(&path).map_err(|_| reference_missing(&path)).ok()
}

/// A binary file from `data/flyvis-reference/`, or `None` if it is absent.
pub(crate) fn load_reference_bytes(name: &str) -> Option<Vec<u8>> {
    let path = format!("{}/{name}", reference_dir());
    std::fs::read(&path).map_err(|_| reference_missing(&path)).ok()
}
