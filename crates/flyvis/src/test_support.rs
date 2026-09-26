//! Test-only helpers shared across this crate's test modules.
//!
//! Kept as its own module so the download URL for the real flyvis file lives
//! in exactly one place, instead of being copy-pasted into every test module
//! that needs to skip when the real data is absent.

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
            eprintln!(
                "skipping: {path} not found. Download \
                 https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
                 and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/"
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
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_build.py` to generate it."
            );
            None
        }
    }
}
