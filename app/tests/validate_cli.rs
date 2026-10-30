//! The `semblance` binary's command line, without fixtures: `validate`'s
//! usage errors and its report of missing data, `help`, and `roundtrip`.
//! Each test spawns the built binary.

use std::path::PathBuf;
use std::process::{Command, Output};

fn semblance(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_semblance")).args(args).output().expect("the binary runs")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// An empty directory under the target's temp dir, unique to `name`.
fn empty_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("validate-cli-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn validate_with_no_data_lists_all_thirteen_files_and_exits_2() {
    let dir = empty_dir("no-data");
    let out = semblance(&["validate", "--data", dir.to_str().unwrap()]);
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(out.stdout.is_empty(), "nothing is printed as a result: {}", text(&out.stdout));
    assert!(err.starts_with("validate: nothing was verified; 13 fixture files are missing:\n"), "{err}");
    for rel in semblance_flyvis::parity::required_files() {
        assert!(err.contains(&format!("  {}\n", dir.join(rel).display())), "{rel} not listed:\n{err}");
    }
    assert!(err.contains("ci/fetch-fixtures.sh"), "{err}");
    assert!(err.contains("docs/results/0.3.0-flyvis-parity.md; it is not a check of this machine."), "{err}");
}

/// Only an absent file is missing. A data path that exists but cannot be
/// read (here `--data` names a regular file, so every fixture path under it
/// is `NotADirectory`) reaches the library's `Unreadable`: exit 2 with the
/// real error, never the missing-file list and its fetch instructions.
#[test]
fn validate_reports_an_unreadable_fixture_as_unreadable_not_missing() {
    let dir = empty_dir("not-a-dir");
    let file = dir.join("data");
    std::fs::write(&file, b"").unwrap();
    let out = semblance(&["validate", "--data", file.to_str().unwrap()]);
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(out.stdout.is_empty(), "nothing is printed as a result: {}", text(&out.stdout));
    assert!(err.starts_with("validate: nothing was verified: "), "{err}");
    assert!(err.contains(": cannot be read (NotADirectory)"), "{err}");
    assert!(!err.contains("missing") && !err.contains("ci/fetch-fixtures.sh"), "names the wrong problem: {err}");
}

/// The finding's own case: a `chmod 000` fixture directory, its files
/// present, is a permission error, not 11 missing files.
#[cfg(unix)]
#[test]
fn validate_reports_a_permission_denied_fixture_dir_as_unreadable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = empty_dir("denied");
    let locked = dir.join("flyvis-reference");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("manifest.json"), b"{}").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let probe = std::fs::metadata(locked.join("manifest.json"));
    let out = semblance(&["validate", "--data", dir.to_str().unwrap()]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    if probe.is_ok() {
        return; // root reads through 000: nothing to test here
    }
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
    assert!(err.starts_with("validate: nothing was verified: "), "{err}");
    assert!(err.contains(": cannot be read (PermissionDenied)"), "{err}");
    assert!(!err.contains("ci/fetch-fixtures.sh"), "names the wrong problem: {err}");
}

#[test]
fn validate_refuses_a_bad_command_line_with_exit_2() {
    for args in [
        &["validate", "--threads", "0"][..],
        &["validate", "--threads", "x"],
        &["validate", "--threads"],
        &["validate", "--bogus"],
        &["validate", "--data", "a", "--data", "b"],
    ] {
        let out = semblance(args);
        let err = text(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {err}");
        assert!(err.starts_with("validate: "), "{args:?}: {err}");
        assert!(err.contains("usage: semblance validate [--data DIR] [--threads N] [--markdown]"), "{args:?}: {err}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn an_unknown_command_still_exits_1() {
    let out = semblance(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("unknown command: frobnicate"));
}

#[test]
fn help_lists_validate_and_no_longer_says_everything_works_with_no_data() {
    let out = semblance(&["help"]);
    assert!(out.status.success());
    let help = text(&out.stdout);
    assert!(help.contains("  validate    check that this build reproduces flyvis bit for bit\n"), "{help}");
    assert!(help.contains("[--data DIR] [--threads N] [--markdown]"), "{help}");
    assert!(help.contains("demo, roundtrip and evolve run on a synthetic graph and need no data."), "{help}");
    assert!(!help.contains("Everything runs on a synthetic graph so it works with no data"), "{help}");
}

#[test]
fn roundtrip_is_still_lossless() {
    let out = semblance(&["roundtrip"]);
    assert!(out.status.success());
    let s = text(&out.stdout);
    assert!(s.contains("structural distance:  0.000000"), "{s}");
    assert!(s.contains("behavioural distance: 0.000000"), "{s}");
}
