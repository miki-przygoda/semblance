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
pub(crate) const REAL_SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";

/// `rel`'s path under the workspace's `data/`, as every notice prints it.
fn data_path(rel: &str) -> String {
    format!("{}/../../data/{rel}", env!("CARGO_MANIFEST_DIR"))
}

/// A missing fixture's notice, `<path> not found. <how to get it>`, without
/// the `skipping: ` its printer adds. Under `SEMBLANCE_REQUIRE_FIXTURES=1`
/// this panics instead, naming the path.
fn missing(path: &str, rel: &str) -> String {
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    format!("{path} not found. {}", crate::parity::how_to_get(rel))
}

/// The workspace's `data/` directory, the same one `data_path` names.
pub(crate) fn data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}/../../data", env!("CARGO_MANIFEST_DIR")))
}

/// A fresh, empty scratch directory for one test, under this build's own
/// target dir (`<target>/<profile>/test-scratch/<name>-<pid>`, found from the
/// test binary, which cargo puts in `<target>/<profile>/deps/`). Never under
/// `std::env::temp_dir()`: on a shared host that is a world-writable `/tmp`,
/// where another local account can pre-create a predictable path, plant
/// symlinks in it and have a test write through them. Lib unit tests have no
/// `CARGO_TARGET_TMPDIR`; this is their equivalent.
///
/// A leftover dir of the same name (from a run that died before cleaning up)
/// is removed first, and any failure to remove it panics rather than being
/// ignored. The dir itself is made with `create_dir`, which fails if anything
/// is already at the path, so the test always owns what it gets.
pub(crate) fn scratch_dir(name: &str) -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("the test binary's path");
    let profile = exe.parent().and_then(|deps| deps.parent()).expect("<target>/<profile>/deps/<binary>");
    let base = profile.join("test-scratch");
    std::fs::create_dir_all(&base).unwrap_or_else(|e| panic!("{}: {e}", base.display()));
    let dir = base.join(format!("{name}-{}", std::process::id()));
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("cannot clear the stale scratch dir {}: {e}", dir.display()),
    }
    std::fs::create_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    dir
}

/// Writes `bytes` to a new file at `path`, refusing (`AlreadyExists`) if
/// anything is already there. `create_new` is `O_CREAT | O_EXCL`, which never
/// follows a symlink at `path`, so a planted link cannot redirect the write.
pub(crate) fn write_new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)?.write_all(bytes)
}

/// Whether every file in `rels` is present under `data/`, by metadata only:
/// `Err` of the first absent one's notice, unprinted. Under
/// `SEMBLANCE_REQUIRE_FIXTURES=1` an absent file panics instead.
pub(crate) fn require_present(rels: &[&str]) -> Result<(), String> {
    for rel in rels {
        let path = data_path(rel);
        if std::fs::metadata(&path).is_err() {
            return Err(missing(&path, rel));
        }
    }
    Ok(())
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

    /// A scratch dir lives under this build's own target dir, never in the
    /// shared temp dir another local account can write to, and is fresh.
    #[test]
    fn a_scratch_dir_is_fresh_and_under_the_target_dir_not_the_shared_tmp() {
        let dir = scratch_dir("scratch-self-test");
        let exe = std::env::current_exe().unwrap();
        let target = exe.parent().unwrap().parent().unwrap();
        assert!(dir.starts_with(target), "{} is not under {}", dir.display(), target.display());
        assert!(!dir.starts_with(std::env::temp_dir()), "{} is in the shared temp dir", dir.display());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "a scratch dir starts empty");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `write_new` refuses a path that already exists, a planted symlink
    /// above all, and leaves the link's target untouched.
    #[cfg(unix)]
    #[test]
    fn write_new_refuses_a_planted_symlink_and_never_follows_it() {
        let dir = scratch_dir("write-new-self-test");
        let victim = dir.join("victim");
        std::fs::write(&victim, b"untouched").unwrap();
        let planted = dir.join("planted");
        std::os::unix::fs::symlink(&victim, &planted).unwrap();
        let e = write_new(&planted, b"overwritten").expect_err("a planted link must be refused");
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists, "{e}");
        assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
        write_new(&dir.join("fresh"), b"new").unwrap();
        assert_eq!(std::fs::read(dir.join("fresh")).unwrap(), b"new");
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
