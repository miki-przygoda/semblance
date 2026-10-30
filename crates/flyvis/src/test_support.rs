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
        if let Err(e) = std::fs::metadata(&path) {
            return Err(read_error(&path, rel, e));
        }
    }
    Ok(())
}

/// What an io error reading the fixture at `path` means: a missing file
/// (`NotFound`) is its notice, for the caller to skip with. Anything else --
/// permission denied, not UTF-8, a directory -- is a fixture that is present
/// but broken, and panics naming the path and the error, with or without
/// `SEMBLANCE_REQUIRE_FIXTURES` (spec R21): it never becomes a skip.
fn read_error(path: &str, rel: &str, e: std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => missing(path, rel),
        _ => panic!("{path}: present but cannot be read: {e}"),
    }
}

/// The bytes at `path` (`data/<rel>`), or its missing-fixture notice.
fn read_bytes_at(path: &str, rel: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| read_error(path, rel, e))
}

/// The text at `path` (`data/<rel>`), or its missing-fixture notice.
fn read_text_at(path: &str, rel: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| read_error(path, rel, e))
}

/// The bytes of `data/<rel>`, or its missing-fixture notice. Prints nothing.
/// A file that is present but cannot be read panics instead (R21).
pub(crate) fn read_fixture_bytes(rel: &str) -> Result<Vec<u8>, String> {
    read_bytes_at(&data_path(rel), rel)
}

/// The text of `data/<rel>`, or its missing-fixture notice. Prints nothing.
/// A file that is present but cannot be read, or is not UTF-8, panics
/// instead (R21).
pub(crate) fn read_fixture_text(rel: &str) -> Result<String, String> {
    read_text_at(&data_path(rel), rel)
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

    /// A fixture that is present but not UTF-8 is broken, not missing: the
    /// text read panics with the real error rather than handing back the
    /// skip notice (spec R21). The bytes read of the same file succeeds.
    #[test]
    fn a_present_fixture_that_is_not_utf8_panics_rather_than_skips() {
        let dir = scratch_dir("fixture-not-utf8");
        let path = dir.join("manifest.json");
        write_new(&path, b"{}\xff").unwrap();
        let path = path.to_str().unwrap().to_string();
        assert_eq!(read_bytes_at(&path, "flyvis-reference/manifest.json"), Ok(b"{}\xff".to_vec()));
        let text = std::panic::catch_unwind(|| read_text_at(&path, "flyvis-reference/manifest.json"));
        std::fs::remove_dir_all(&dir).unwrap();
        let msg = *text.expect_err("a non-UTF-8 fixture must panic, not skip").downcast::<String>().unwrap();
        assert!(msg.starts_with(&format!("{path}: present but cannot be read: ")), "{msg}");
        assert!(!msg.contains("not found") && !msg.contains("missing"), "names the wrong problem: {msg}");
    }

    /// A fixture path that is present but cannot be read (here a directory)
    /// panics with the real error, for both reads, never the skip notice.
    #[test]
    fn a_present_fixture_that_cannot_be_read_panics_rather_than_skips() {
        let dir = scratch_dir("fixture-unreadable");
        let path = dir.to_str().unwrap().to_string();
        let bytes = std::panic::catch_unwind(|| read_bytes_at(&path, "flyvis-reference/centre.f64.bin"));
        let text = std::panic::catch_unwind(|| read_text_at(&path, "flyvis-reference/manifest.json"));
        std::fs::remove_dir_all(&dir).unwrap();
        for r in [bytes.map(|_| ()), text.map(|_| ())] {
            let msg = *r.expect_err("an unreadable fixture must panic, not skip").downcast::<String>().unwrap();
            assert!(msg.starts_with(&format!("{path}: present but cannot be read: ")), "{msg}");
        }
    }

    /// Only `NotFound` is a missing fixture; a permission error, which a
    /// `chmod 000` fixture or directory gives, is not, and panics naming it.
    #[test]
    fn only_not_found_is_a_missing_fixture() {
        let r = std::panic::catch_unwind(|| {
            let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
            read_error("d/ensemble.csv", "flyvis-pretrained/ensemble.csv", denied)
        });
        let msg = *r.expect_err("permission denied must panic, not skip").downcast::<String>().unwrap();
        assert!(msg.starts_with("d/ensemble.csv: present but cannot be read: "), "{msg}");
        assert!(msg.to_lowercase().contains("permission denied"), "{msg}");
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
