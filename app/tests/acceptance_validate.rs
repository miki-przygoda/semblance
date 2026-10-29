//! Acceptance tests for behaviour group `validate` of epic parity-validate:
//! the `semblance validate` command -- its output, exit codes, threads and
//! missing data. One test per behaviour id, VALIDATE-01 .. VALIDATE-20.
//!
//! Every test drives the built `semblance` binary as a user would: it spawns
//! it with arguments, a working directory and a data directory, and reads
//! its stdout, stderr and exit code.
//!
//! The data directories are temporary, under `CARGO_TARGET_TMPDIR`: either
//! placeholder files (enough for the missing-data report, which only looks
//! at which paths exist), or symlinks to the workspace's real fixtures with
//! one file copied and altered. The real `data/` is never written.
//!
//! A test that needs the real fixtures skips without them, printing
//! `skipping: ...`, unless `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns the
//! skip into a failure.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

use semblance_flyvis::parity::{how_to_get, required_files, CONTROL_MISS, COVERAGE_THRESHOLD, F64_BOUND};

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
const USAGE: &str = "usage: semblance validate [--data DIR] [--threads N] [--markdown]";
const RECORD: &str = "docs/results/0.3.0-flyvis-parity.md";

const SETTLED_F64: &str = "flyvis-reference/settled.f64.bin";
const ENSEMBLE: &str = "flyvis-pretrained/ensemble.csv";
const MANIFEST: &str = "flyvis-reference/manifest.json";
/// Nodes in the network: one settled block's length.
const N_NODES: usize = 45669;

// ---------------------------------------------------------------------------
// Running the binary
// ---------------------------------------------------------------------------

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl From<Output> for Run {
    fn from(o: Output) -> Run {
        Run {
            code: o.status.code(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }
}

impl Run {
    fn all(&self) -> String {
        format!("exit {:?}\n--- stdout\n{}--- stderr\n{}", self.code, self.stdout, self.stderr)
    }
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_semblance"))
}

fn semblance(args: &[&str]) -> Run {
    bin().args(args).output().expect("the binary runs").into()
}

fn validate_on(data: &Path, extra: &[&str]) -> Run {
    let mut c = bin();
    c.arg("validate").arg("--data").arg(data).args(extra);
    c.output().expect("the binary runs").into()
}

// ---------------------------------------------------------------------------
// Data directories
// ---------------------------------------------------------------------------

fn real_data() -> PathBuf {
    Path::new(WORKSPACE).join("data")
}

/// Whether the 13 real fixtures are present; if not, either a skip notice
/// (returns false) or, under `SEMBLANCE_REQUIRE_FIXTURES=1`, a failure.
fn fixtures_or_skip(test: &str) -> bool {
    let missing: Vec<&str> =
        required_files().iter().copied().filter(|rel| fs::metadata(real_data().join(rel)).is_err()).collect();
    if missing.is_empty() {
        return true;
    }
    if std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1") {
        panic!("{test}: SEMBLANCE_REQUIRE_FIXTURES=1 but fixtures are missing: {missing:?}");
    }
    eprintln!("skipping: {test}: fixtures missing under {}: {missing:?}", real_data().display());
    false
}

/// A fresh, empty directory under the target's temp dir, unique to `name`.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("acceptance-validate-{name}"));
    if dir.exists() {
        // A previous run may have left a file without read permission.
        let _ = make_readable(&dir);
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_readable(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for entry in fs::read_dir(dir)? {
            let p = entry?.path();
            let meta = fs::symlink_metadata(&p)?;
            if meta.is_dir() {
                make_readable(&p)?;
            } else if meta.is_file() {
                fs::set_permissions(&p, fs::Permissions::from_mode(0o644))?;
            }
        }
    }
    Ok(())
}

/// A data directory holding an empty placeholder for every required file
/// except those in `absent`.
fn placeholders(name: &str, absent: &[&str]) -> PathBuf {
    let dir = fresh(name);
    for rel in required_files() {
        if absent.contains(rel) {
            continue;
        }
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"").unwrap();
    }
    dir
}

/// A data directory linking every required file to the real fixture, except
/// those in `copied`, which are real copies the caller may then alter.
fn linked(name: &str, copied: &[&str]) -> PathBuf {
    let dir = fresh(name);
    for rel in required_files() {
        let (src, dst) = (real_data().join(rel), dir.join(rel));
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        if copied.contains(rel) {
            fs::copy(&src, &dst).unwrap();
            continue;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src, &dst).unwrap();
        #[cfg(not(unix))]
        fs::copy(&src, &dst).unwrap();
    }
    dir
}

/// Overwrites the f64 at index `i` of the (copied) file `rel` under `dir`.
fn set_f64(dir: &Path, rel: &str, i: usize, v: f64) {
    let p = dir.join(rel);
    let mut bytes = fs::read(&p).unwrap();
    bytes[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    fs::write(&p, bytes).unwrap();
}

/// settled.f64.bin holds runs 1, 3, 5, 7 in that order, one block each.
fn settled_f64_index(run: usize, node: usize) -> usize {
    assert!(run % 2 == 1);
    (run / 2) * N_NODES + node
}

// ---------------------------------------------------------------------------
// Shared runs: each full `validate` takes seconds, so each kind runs once
// ---------------------------------------------------------------------------

/// The log a trapped `git`, `uv` or `python` writes to if it is ever run.
struct GoodRun {
    run: Run,
    trap_log: PathBuf,
}

/// `semblance validate` with no `--data`, from a working directory whose
/// `./data` holds links to the real fixtures, and a PATH whose `git`, `uv`,
/// `python` and `python3` only record that they were run.
fn good() -> Option<&'static GoodRun> {
    static GOOD: OnceLock<Option<GoodRun>> = OnceLock::new();
    GOOD.get_or_init(|| {
        if !fixtures_or_skip("validate on the real fixtures") {
            return None;
        }
        let cwd = fresh("good-cwd");
        let data = linked("good-cwd/data", &[]);
        assert_eq!(data, cwd.join("data"));
        let trap = fresh("good-trap");
        let log = trap.join("ran.log");
        for tool in ["git", "uv", "python", "python3"] {
            let p = trap.join(tool);
            fs::write(&p, format!("#!/bin/sh\necho \"{tool} $*\" >> '{}'\nexit 1\n", log.display())).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let run: Run = bin()
            .arg("validate")
            .current_dir(&cwd)
            .env("PATH", &trap)
            .output()
            .expect("the binary runs")
            .into();
        Some(GoodRun { run, trap_log: log })
    })
    .as_ref()
}

const KINDS: [&str; 4] = ["exact", "nan", "nonfinite", "both"];

/// Altered fixtures, each run once per test binary.
fn altered(kind: &'static str) -> Option<&'static Run> {
    static CELLS: [OnceLock<Run>; 4] = [const { OnceLock::new() }; 4];
    if !fixtures_or_skip(kind) {
        return None;
    }
    let i = KINDS.iter().position(|k| *k == kind).expect("a known altered fixture");
    Some(CELLS[i].get_or_init(|| run_altered(kind)))
}

fn run_altered(kind: &str) -> Run {
    match kind {
        // run 3's settled value at node 100 is not flyvis's: run 3's exact
        // and libm settling rows fail, and so does the control that starts
        // from flyvis's settled state.
        "exact" => {
            let d = linked("exact", &[SETTLED_F64]);
            set_f64(&d, SETTLED_F64, settled_f64_index(3, 100), 7.0);
            validate_on(&d, &[])
        }
        // run 3's settled value at node 100 is NaN: the control that starts
        // from flyvis's settled state returns an error.
        "nan" => {
            let d = linked("nan", &[SETTLED_F64]);
            set_f64(&d, SETTLED_F64, settled_f64_index(3, 100), f64::NAN);
            validate_on(&d, &[])
        }
        // model 049's synapse strengths are 1e20: its runs' state becomes
        // non-finite during settling.
        "nonfinite" => {
            let d = linked("nonfinite", &[ENSEMBLE]);
            let p = d.join(ENSEMBLE);
            let text = fs::read_to_string(&p).unwrap();
            let out: Vec<String> = text
                .lines()
                .map(|l| {
                    let f: Vec<&str> = l.split(',').collect();
                    if f.len() == 5 && f[0] == "049" && f[1] == "syn_strength" {
                        format!("{},{},{},{},1e20", f[0], f[1], f[2], f[3])
                    } else {
                        l.to_string()
                    }
                })
                .collect();
            fs::write(&p, out.join("\n") + "\n").unwrap();
            validate_on(&d, &[])
        }
        // run 1's settled value differs (a gate failure, 1) and run 5's is
        // +0.0 (a mismatched fixture, 2), in the same run.
        "both" => {
            let d = linked("both", &[SETTLED_F64]);
            set_f64(&d, SETTLED_F64, settled_f64_index(1, 100), 7.0);
            set_f64(&d, SETTLED_F64, settled_f64_index(5, 100), 0.0);
            validate_on(&d, &[])
        }
        other => panic!("no such altered fixture: {other}"),
    }
}

// ---------------------------------------------------------------------------
// Reading the terminal table
// ---------------------------------------------------------------------------

/// The table's rows: lines starting with `settling ` or `traces `.
fn rows(stdout: &str) -> Vec<&str> {
    stdout.lines().filter(|l| l.starts_with("settling ") || l.starts_with("traces ")).collect()
}

/// The row for (stage, run, counts), counts being `exact` or `libm`.
fn row<'a>(stdout: &'a str, stage: &str, run: u32, counts: &str) -> &'a str {
    let found: Vec<&str> = rows(stdout)
        .into_iter()
        .filter(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            f.len() > 5 && f[0] == stage && f[1] == run.to_string() && f[5] == counts
        })
        .collect();
    assert_eq!(found.len(), 1, "one {stage} run {run} {counts} row:\n{stdout}");
    found[0]
}

/// A row's fields after the six leading ones (stage .. counts), split on
/// whitespace: compared, unequal, max err, where..., bound, ratio[, FAIL].
fn tail(row: &str) -> Vec<&str> {
    row.split_whitespace().skip(6).collect()
}

fn verdict(stdout: &str) -> &str {
    let v: Vec<&str> = stdout.lines().filter(|l| l.starts_with("verdict:")).collect();
    assert_eq!(v.len(), 1, "one verdict line:\n{stdout}");
    v[0]
}

fn controls(stdout: &str) -> Vec<&str> {
    let lines: Vec<&str> = stdout.lines().collect();
    let start = lines.iter().position(|l| l.starts_with("controls:")).expect("a controls line");
    lines[start..start + 4].to_vec()
}

fn coverage(stdout: &str) -> Vec<&str> {
    let lines: Vec<&str> = stdout.lines().collect();
    let start = lines.iter().position(|l| l.starts_with("coverage:")).expect("a coverage line");
    let end = lines.iter().position(|l| l.starts_with("verdict:")).expect("a verdict line");
    lines[start..end].iter().map(|l| l.get(11..).unwrap_or("")).collect()
}

/// Whether `s` is exactly what Rust's `{:.3e}` prints for the number it
/// denotes.
fn is_3e(s: &str) -> bool {
    s.parse::<f64>().is_ok_and(|v| format!("{v:.3e}") == s)
}

/// The output with the verdict's wall time and the "this build" line (it
/// names the thread count) removed: what must not depend on threads.
fn thread_independent(stdout: &str) -> String {
    stdout
        .lines()
        .filter(|l| !l.starts_with("this build:"))
        .map(|l| if l.starts_with("verdict:") { l.rsplit_once("  [").map_or(l, |(a, _)| a) } else { l })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// VALIDATE-01 .. VALIDATE-20
// ---------------------------------------------------------------------------

/// VALIDATE-01: `semblance validate` runs all eight runs' gates and the four
/// controls and prints one table to stdout; `--data` defaults to `./data`.
#[test]
fn validate_01_runs_every_gate_and_control_into_one_table_reading_data_by_default() {
    let Some(g) = good() else { return };
    let out = &g.run.stdout;
    assert!(out.contains("\ndata:        ./data\n"), "the default data directory is ./data:\n{}", g.run.all());
    let tables = out.lines().filter(|l| l.starts_with("stage ")).count();
    assert_eq!(tables, 1, "one table:\n{out}");
    for stage in ["settling", "traces"] {
        for run in 0..8 {
            for counts in ["exact", "libm"] {
                row(out, stage, run, counts);
            }
        }
    }
    assert_eq!(rows(out).len(), 32, "{out}");
    let c = controls(out);
    for name in [
        "wrong model (049 vs run 3)",
        "integer counts (run 3)",
        "mirrored mapping (run 3, seq 8)",
        "flyvis's settled start (run 3, seq 8)",
    ] {
        assert!(c.iter().any(|l| l.contains(name)), "control {name}:\n{out}");
    }
    assert!(!g.run.stderr.contains("missing"), "{}", g.run.all());
}

/// VALIDATE-02: every exact job bit-equal, every libm job within its bound,
/// every control holding, all eight runs: exit 0.
#[test]
fn validate_02_exits_0_when_every_gate_and_control_holds() {
    let Some(g) = good() else { return };
    assert_eq!(g.run.code, Some(0), "{}", g.run.all());
    assert!(verdict(&g.run.stdout).starts_with("verdict:   PASS — 16 exact rows bit-equal, 16 libm rows within bounds"));
    assert!(!g.run.stdout.contains("FAIL"), "{}", g.run.stdout);
}

/// VALIDATE-03: an exact value that differs, a libm bound exceeded, a
/// control that does not hold or errored, or a non-finite state: exit 1, the
/// full table still prints with each failing row ending in `FAIL`, and the
/// verdict names every failing job and control.
#[test]
fn validate_03_exits_1_prints_the_full_table_and_names_every_failure() {
    // Exact differs, libm bound exceeded, a control does not hold.
    let Some(r) = altered("exact") else { return };
    assert_eq!(r.code, Some(1), "{}", r.all());
    assert_eq!(rows(&r.stdout).len(), 32, "the full table:\n{}", r.stdout);
    let failing: Vec<&str> = rows(&r.stdout).into_iter().filter(|l| l.ends_with("FAIL")).collect();
    let exact = row(&r.stdout, "settling", 3, "exact");
    let libm = row(&r.stdout, "settling", 3, "libm");
    assert!(exact.ends_with("  FAIL") && libm.ends_with("  FAIL"), "{}", r.stdout);
    assert_eq!(failing, vec![exact, libm], "only the failing rows end in FAIL:\n{}", r.stdout);
    let control = controls(&r.stdout).into_iter().find(|l| l.contains("flyvis's settled start")).unwrap();
    assert!(control.ends_with("FAIL"), "{control}");
    let v = verdict(&r.stdout);
    for name in ["settling run 3 exact", "settling run 3 libm", "control flyvis's settled start (run 3, seq 8)"] {
        assert!(v.contains(name), "the verdict names {name}: {v}");
    }
    assert!(v.starts_with("verdict:   FAIL — "), "{v}");

    // A non-finite state (and the control that errors because of it).
    let Some(n) = altered("nonfinite") else { return };
    assert_eq!(n.code, Some(1), "{}", n.all());
    assert_eq!(rows(&n.stdout).len(), 32, "the full table:\n{}", n.stdout);
    let v = verdict(&n.stdout);
    for stage in ["settling", "traces"] {
        for run in 4..8 {
            for counts in ["exact", "libm"] {
                let l = row(&n.stdout, stage, run, counts);
                assert!(l.ends_with("  FAIL") && l.contains("is not finite after"), "{l}");
                assert!(v.contains(&format!("{stage} run {run} {counts}")), "{v}");
            }
        }
        for run in 0..4 {
            for counts in ["exact", "libm"] {
                assert!(!row(&n.stdout, stage, run, counts).ends_with("FAIL"), "{}", n.stdout);
            }
        }
    }
    assert!(v.contains("control wrong model (049 vs run 3)"), "{v}");
}

/// VALIDATE-04: a fixture missing, unreadable, malformed or mismatched:
/// exit 2 and nothing counts as verified, even if some jobs finished; 2 wins
/// over 1. (A job slot holding another job's result cannot be produced from
/// the command line; the parity group tests it through the library.)
#[test]
fn validate_04_exits_2_on_a_fixture_that_does_not_fit_and_2_wins_over_1() {
    if !fixtures_or_skip("validate_04") {
        return;
    }
    // Missing.
    let d = linked("04-missing", &[]);
    fs::remove_file(d.join(MANIFEST)).unwrap();
    let r = validate_on(&d, &[]);
    assert_eq!(r.code, Some(2), "missing: {}", r.all());
    assert!(r.stdout.is_empty() && r.stderr.contains("nothing was verified"), "missing: {}", r.all());

    // Malformed.
    let d = linked("04-malformed", &[MANIFEST]);
    fs::write(d.join(MANIFEST), b"{ this is not json").unwrap();
    let r = validate_on(&d, &[]);
    assert_eq!(r.code, Some(2), "malformed: {}", r.all());
    assert!(r.stdout.is_empty() && r.stderr.contains("nothing was verified"), "malformed: {}", r.all());

    // Mismatched: a settling file one value short of its manifest size.
    let d = linked("04-short", &[SETTLED_F64]);
    let p = d.join(SETTLED_F64);
    let bytes = fs::read(&p).unwrap();
    fs::write(&p, &bytes[..bytes.len() - 8]).unwrap();
    let r = validate_on(&d, &[]);
    assert_eq!(r.code, Some(2), "short: {}", r.all());
    assert!(r.stdout.is_empty() && r.stderr.contains("nothing was verified"), "short: {}", r.all());

    // Unreadable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let d = linked("04-unreadable", &[ENSEMBLE]);
        fs::set_permissions(d.join(ENSEMBLE), fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(d.join(ENSEMBLE)).is_err() {
            let r = validate_on(&d, &[]);
            assert_eq!(r.code, Some(2), "unreadable: {}", r.all());
            assert!(r.stdout.is_empty() && r.stderr.contains("nothing was verified"), "unreadable: {}", r.all());
        }
        fs::set_permissions(d.join(ENSEMBLE), fs::Permissions::from_mode(0o644)).unwrap();
    }

    // Mismatched inside one job (a reference value of +0.0, run 5) while
    // another job fails its gate (run 1): 2, though most jobs finished.
    let Some(b) = altered("both") else { return };
    assert_eq!(b.code, Some(2), "2 wins over 1: {}", b.all());
    assert!(row(&b.stdout, "settling", 1, "exact").ends_with("FAIL"), "{}", b.stdout);
    assert!(row(&b.stdout, "settling", 5, "exact").contains("±0.0"), "{}", b.stdout);
    let v = verdict(&b.stdout);
    assert!(v.contains("NOT VERIFIED") && v.contains("nothing counts as verified"), "{v}");
    assert!(!v.contains("PASS"), "{v}");
}

/// VALIDATE-05: an unknown flag, a flag given twice, a flag missing its
/// value, `--threads 0`, or a non-integer `--threads`: a message and the
/// usage line on stderr, exit 2.
#[test]
fn validate_05_a_bad_command_line_prints_usage_and_exits_2() {
    let dir = placeholders("05", &[]);
    let d = dir.to_str().unwrap();
    let cases: Vec<Vec<&str>> = vec![
        vec!["validate", "--bogus"],
        vec!["validate", "--data", d, "--verbose"],
        vec!["validate", "-t", "4"],
        vec!["validate", "--data", d, "--data", d],
        vec!["validate", "--threads", "2", "--threads", "2"],
        vec!["validate", "--markdown", "--markdown"],
        vec!["validate", "--data"],
        vec!["validate", "--threads"],
        vec!["validate", "--threads", "0"],
        vec!["validate", "--threads", "x"],
        vec!["validate", "--threads", "1.5"],
        vec!["validate", "--threads", "-1"],
        vec!["validate", "--threads", ""],
    ];
    for args in cases {
        let r = semblance(&args);
        assert_eq!(r.code, Some(2), "{args:?}: {}", r.all());
        assert!(r.stdout.is_empty(), "{args:?}: {}", r.all());
        let first = r.stderr.lines().next().unwrap_or("");
        assert!(first.starts_with("validate: ") && first.len() > "validate: ".len(), "{args:?}: a message: {}", r.all());
        assert!(r.stderr.lines().any(|l| l == USAGE), "{args:?}: the usage line: {}", r.all());
    }
}

/// VALIDATE-06: an unknown command (not a flag of validate) exits 1.
#[test]
fn validate_06_an_unknown_command_exits_1() {
    for cmd in ["frobnicate", "valdiate", "--data", "--threads", "--markdown"] {
        let r = semblance(&[cmd]);
        assert_eq!(r.code, Some(1), "{cmd}: {}", r.all());
        assert!(r.stderr.contains(&format!("unknown command: {cmd}")), "{cmd}: {}", r.all());
    }
}

/// VALIDATE-07: with no `--threads`, `available_parallelism()` (or 1)
/// capped at 16; an explicit value above 16 is accepted and capped; results
/// do not depend on the thread count.
#[test]
fn validate_07_threads_default_to_available_parallelism_capped_at_16_and_do_not_change_results() {
    let Some(g) = good() else { return };
    let want = std::thread::available_parallelism().map_or(1, |n| n.get()).min(16);
    let threads_of = |out: &str| -> String {
        let l = out.lines().find(|l| l.starts_with("this build:")).expect("a this-build line").to_string();
        l.rsplit(", ").next().unwrap().to_string()
    };
    assert_eq!(threads_of(&g.run.stdout), format!("{want} threads"), "{}", g.run.stdout);

    let d = linked("07", &[]);
    let many = validate_on(&d, &["--threads", "64"]);
    assert_eq!(many.code, Some(0), "--threads 64 is accepted: {}", many.all());
    assert_eq!(threads_of(&many.stdout), "16 threads", "capped at 16:\n{}", many.stdout);

    let few = validate_on(&d, &["--threads", "3"]);
    assert_eq!(few.code, Some(0), "{}", few.all());
    assert_eq!(threads_of(&few.stdout), "3 threads", "{}", few.stdout);

    // The data line differs (./data against --data); compare from the
    // reference line on.
    let body = |s: &str| thread_independent(&s[s.find("reference:").unwrap()..]);
    assert_eq!(body(&few.stdout), body(&many.stdout), "3 threads against 64");
    assert_eq!(body(&few.stdout), body(&g.run.stdout), "3 threads against the default");
}

/// VALIDATE-08: with fixture files absent: stderr names how many, lists each
/// absent path (only those) one per line, points to `ci/fetch-fixtures.sh`
/// and tools/README.md, says the record is not a check of this machine;
/// exit 2, and the record is never printed as a result.
#[test]
fn validate_08_missing_fixtures_are_listed_exactly_with_where_to_get_them() {
    let absent = [ENSEMBLE, MANIFEST, "flyvis-reference/snapshots.f32.bin"];
    let dir = placeholders("08", &absent);
    let r = validate_on(&dir, &[]);
    assert_eq!(r.code, Some(2), "{}", r.all());
    assert!(r.stdout.is_empty(), "nothing on stdout: {}", r.all());
    let lines: Vec<&str> = r.stderr.lines().collect();
    assert_eq!(lines[0], "validate: nothing was verified; 3 fixture files are missing:", "{}", r.stderr);
    let listed: Vec<&str> = lines.iter().filter(|l| l.starts_with("  ")).map(|l| &l[2..]).collect();
    let mut want: Vec<String> =
        required_files().iter().filter(|rel| absent.contains(rel)).map(|rel| dir.join(rel).display().to_string()).collect();
    want.sort();
    let mut got: Vec<String> = listed.iter().map(|s| s.to_string()).collect();
    got.sort();
    assert_eq!(got, want, "only the absent paths, one per line:\n{}", r.stderr);
    assert!(lines[1..4].iter().all(|l| l.starts_with("  ")), "the paths follow the first line:\n{}", r.stderr);
    assert!(r.stderr.contains("ci/fetch-fixtures.sh"), "{}", r.stderr);
    assert!(r.stderr.contains("tools/README.md"), "{}", r.stderr);
    assert!(r.stderr.contains(RECORD) && r.stderr.contains("not a check of this machine"), "{}", r.stderr);
    // The record's contents are not printed as if they were a result.
    assert!(!r.stderr.contains("PASS") && !r.stderr.contains("bit-equal"), "{}", r.stderr);
}

/// VALIDATE-09: an empty data directory: stderr lists all 13 required paths
/// and names `ci/fetch-fixtures.sh`; exit 2.
#[test]
fn validate_09_an_empty_data_directory_lists_all_13_paths() {
    let dir = fresh("09");
    let r = validate_on(&dir, &[]);
    assert_eq!(r.code, Some(2), "{}", r.all());
    assert!(r.stderr.starts_with("validate: nothing was verified; 13 fixture files are missing:\n"), "{}", r.stderr);
    assert_eq!(required_files().len(), 13);
    for rel in required_files() {
        assert!(r.stderr.contains(&format!("  {}\n", dir.join(rel).display())), "{rel}:\n{}", r.stderr);
    }
    assert_eq!(r.stderr.lines().filter(|l| l.starts_with("  ")).count(), 13, "{}", r.stderr);
    assert!(r.stderr.contains("ci/fetch-fixtures.sh"), "{}", r.stderr);
    assert!(r.stdout.is_empty(), "{}", r.all());
}

/// VALIDATE-10: exactly one fixture file missing: its `how_to_get` hint is
/// printed in full.
#[test]
fn validate_10_one_missing_file_prints_its_hint_in_full() {
    for (i, rel) in [
        ENSEMBLE,
        "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json",
        "flyvis-reference/centre.f64.bin",
    ]
    .into_iter()
    .enumerate()
    {
        let dir = placeholders(&format!("10-{i}"), &[rel]);
        let r = validate_on(&dir, &[]);
        assert_eq!(r.code, Some(2), "{rel}: {}", r.all());
        assert!(r.stderr.starts_with("validate: nothing was verified; 1 fixture file is missing:\n"), "{}", r.stderr);
        assert!(r.stderr.contains(&format!("  {}\n", dir.join(rel).display())), "{}", r.stderr);
        assert!(r.stderr.contains(how_to_get(rel)), "{rel}: the hint in full:\n{}", r.stderr);
    }
}

/// VALIDATE-11: the layout of a successful run's output.
#[test]
fn validate_11_the_terminal_output_has_header_table_controls_coverage_and_verdict() {
    let Some(g) = good() else { return };
    let out = &g.run.stdout;
    let lines: Vec<&str> = out.lines().collect();
    // The header: label: value.
    let labels = ["data:", "reference:", "models:", "this build:", "libm exp:"];
    for (i, label) in labels.iter().enumerate() {
        let l = lines[1 + i];
        assert!(l.starts_with(label), "header line {i} is {label}: {l}");
        assert!(l.len() > 13 && l[..13].ends_with(' ') && !l[13..].starts_with(' '), "label: value, aligned: {l}");
    }
    let libm = lines[5];
    let fp = libm.split("(fingerprint ").nth(1).expect("a fingerprint").trim_end_matches(')');
    assert!(fp.starts_with("f32:") && fp.contains(",f64:"), "{libm}");
    // The table header and its columns.
    let header = lines.iter().position(|l| l.starts_with("stage ")).expect("a table header");
    let cols: Vec<&str> = lines[header].split_whitespace().collect();
    assert_eq!(
        cols,
        ["stage", "run", "model", "dt", "prec", "counts", "compared", "unequal", "max", "err", "where", "bound", "ratio"],
        "{}",
        lines[header]
    );
    // One settling and one traces row per (run, counts), fixed width: every
    // row's leading columns line up with the header's.
    let rws = &lines[header + 1..header + 33];
    let mut want = Vec::new();
    for stage in ["settling", "traces"] {
        for run in 0..8 {
            for counts in ["exact", "libm"] {
                want.push((stage.to_string(), run.to_string(), counts.to_string()));
            }
        }
    }
    let got: Vec<(String, String, String)> = rws
        .iter()
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f[0].to_string(), f[1].to_string(), f[5].to_string())
        })
        .collect();
    assert_eq!(got, want, "rows in order:\n{out}");
    let at = |name: &str| lines[header].find(name).unwrap();
    for l in rws {
        for (col, start) in [("run", at("run")), ("model", at("model")), ("dt", at("dt ")), ("prec", at("prec"))] {
            assert!(l.as_bytes()[start - 1] == b' ' && l.as_bytes()[start] != b' ', "{col} column aligned: {l}");
        }
    }
    // Then the controls, coverage and verdict, in that order.
    let ctl = lines.iter().position(|l| l.starts_with("controls:")).unwrap();
    let cov = lines.iter().position(|l| l.starts_with("coverage:")).unwrap();
    let ver = lines.iter().position(|l| l.starts_with("verdict:")).unwrap();
    assert!(header + 33 <= ctl && ctl + 4 == cov && cov < ver && ver == lines.len() - 1, "{out}");
    // Coverage: each run's weakest-type / libm-centre-bound ratio, run order.
    let ratios = coverage(out).into_iter().find(|l| l.starts_with("weakest / libm centre bound, runs 0-7: ")).expect("ratios");
    let values: Vec<&str> = ratios["weakest / libm centre bound, runs 0-7: ".len()..].split(' ').collect();
    assert_eq!(values.len(), 8, "{ratios}");
    assert!(values.iter().all(|v| is_3e(v)), "{ratios}");
    // The verdict ends in wall time.
    let v = lines[ver];
    assert!(v.ends_with(" s]"), "{v}");
    let secs = &v[v.rfind('[').unwrap() + 1..v.len() - 3];
    assert!(secs.parse::<f64>().is_ok() && secs.contains('.'), "wall time: {v}");
}

/// VALIDATE-12: a passing exact row shows `0` unequal, `==` as bound and `-`
/// for where and ratio; a failing exact row prints its maximum.
#[test]
fn validate_12_exact_rows_print_0_eq_eq_and_dashes_or_their_maximum() {
    let Some(g) = good() else { return };
    for stage in ["settling", "traces"] {
        for run in 0..8 {
            let t = tail(row(&g.run.stdout, stage, run, "exact"));
            // compared, unequal, max err, where, bound, ratio
            assert_eq!(t.len(), 6, "{t:?}");
            assert!(t[0].parse::<usize>().unwrap() > 0, "{t:?}");
            assert_eq!(&t[1..], ["0", "0", "-", "==", "-"], "{stage} run {run}");
        }
    }
    let Some(r) = altered("exact") else { return };
    let l = row(&r.stdout, "settling", 3, "exact");
    let t = tail(l);
    assert_eq!(t[1], "1", "one value differs: {l}");
    assert!(is_3e(t[2]) && t[2].parse::<f64>().unwrap() > 0.0, "its maximum: {l}");
    assert!(l.contains("node 100"), "where it is: {l}");
    assert!(l.contains(" == ") && l.ends_with("FAIL"), "{l}");
}

/// VALIDATE-13: values print with `{:.3e}` (unpadded exponent), thresholds
/// with `{:e}` of their constants, validation losses with `{:.3}`.
#[test]
fn validate_13_floats_use_3e_thresholds_e_and_losses_3() {
    let Some(g) = good() else { return };
    let out = &g.run.stdout;
    for l in rows(out) {
        let t = tail(l);
        let (max, bound, ratio) = (t[2], t[t.len() - 2], t[t.len() - 1]);
        if l.split_whitespace().nth(5) == Some("libm") {
            for v in [max, bound, ratio] {
                assert!(is_3e(v), "{v:?} is {{:.3e}}: {l}");
                let exp = v.split('e').nth(1).unwrap().trim_start_matches('-');
                assert!(!exp.starts_with('+') && (exp == "0" || !exp.starts_with('0')), "unpadded: {l}");
            }
        }
    }
    let ctl = controls(out);
    assert!(ctl[0].contains(&format!("must be > {CONTROL_MISS:e}")) && ctl[0].contains("must be > 1e-3"), "{}", ctl[0]);
    assert!(ctl[1].contains(&format!("must be > {F64_BOUND:e}")) && ctl[1].contains("must be > 1e-10"), "{}", ctl[1]);
    for c in &ctl[..2] {
        let v = c.split("settled max ").nth(1).unwrap().split_whitespace().next().unwrap();
        assert!(is_3e(v), "{c}");
    }
    let cov = coverage(out);
    assert!(cov[0].contains(&format!("threshold {COVERAGE_THRESHOLD:e} ")) && cov[0].contains("threshold 1e-3 "), "{}", cov[0]);
    let m = cov[0].split("moves ").nth(1).unwrap().split(' ').next().unwrap();
    assert!(is_3e(m), "{}", cov[0]);
    let models = out.lines().find(|l| l.starts_with("models:")).unwrap();
    let losses: Vec<&str> =
        models.split("validation loss ").skip(1).map(|s| s.split(')').next().unwrap()).collect();
    assert_eq!(losses.len(), 2, "{models}");
    for v in losses {
        assert_eq!(format!("{:.3}", v.parse::<f64>().unwrap()), v, "{models}");
    }
}

/// VALIDATE-14: a run whose exact job did not pass reads `coverage: run {r}
/// not measured: exact job failed`.
#[test]
fn validate_14_coverage_says_a_run_with_a_failed_exact_job_was_not_measured() {
    let Some(r) = altered("exact") else { return };
    let line = r.stdout.lines().find(|l| l.contains("not measured")).expect("a not-measured line");
    assert_eq!(line, "coverage:  run 3 not measured: exact job failed", "{}", r.stdout);
    assert_eq!(r.stdout.matches("not measured").count(), 1, "only run 3:\n{}", r.stdout);
    let ratios = coverage(&r.stdout).into_iter().find(|l| l.starts_with("weakest / libm centre bound")).unwrap();
    let values: Vec<&str> = ratios.rsplit(": ").next().unwrap().split(' ').collect();
    assert_eq!(values[3], "-", "{ratios}");
}

/// VALIDATE-15: a control that returned an error prints the error in place
/// of its measurement, then `FAIL`, and the exit code is 1.
#[test]
fn validate_15_an_errored_control_prints_its_error_and_fail_and_exits_1() {
    let Some(r) = altered("nan") else { return };
    assert_eq!(r.code, Some(1), "{}", r.all());
    let c = controls(&r.stdout).into_iter().find(|l| l.contains("flyvis's settled start")).unwrap();
    assert!(c.ends_with("FAIL"), "{c}");
    assert!(!c.contains("unequal"), "the error, not a measurement: {c}");
    assert!(c.contains("run 3") && c.contains("not finite"), "the error: {c}");
    assert!(verdict(&r.stdout).contains("control flyvis's settled start (run 3, seq 8)"), "{}", r.stdout);
}

/// VALIDATE-16: stdout closed early: output stops and the command exits
/// with the code it had computed, never panicking; any other write error
/// is exit 2.
#[test]
fn validate_16_a_closed_stdout_keeps_the_exit_code_and_another_write_error_is_2() {
    if !fixtures_or_skip("validate_16") {
        return;
    }
    // A failing run (exit 1) whose stdout is closed before it writes.
    let d = linked("16-exact", &[SETTLED_F64]);
    set_f64(&d, SETTLED_F64, settled_f64_index(3, 100), 7.0);
    let mut child = bin()
        .arg("validate")
        .arg("--data")
        .arg(&d)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    drop(child.stdout.take());
    let r: Run = child.wait_with_output().unwrap().into();
    assert_eq!(r.code, Some(1), "the computed code, not a panic: {}", r.all());
    assert!(!r.stderr.contains("panicked"), "{}", r.stderr);

    // A passing run (exit 0) whose stdout is a file it may not grow: with
    // the file-size limit at 0 and SIGXFSZ ignored, every write fails with
    // EFBIG, a write error other than a closed pipe: exit 2. (A read-only
    // descriptor would not do: Rust's stdout treats EBADF as success.)
    #[cfg(unix)]
    {
        let d = linked("16-good", &[]);
        let sink = fresh("16-sink").join("stdout.txt");
        let r: Run = Command::new("/bin/sh")
            .arg("-c")
            .arg("trap '' XFSZ; ulimit -f 0; exec \"$0\" validate --data \"$1\" > \"$2\"")
            .arg(env!("CARGO_BIN_EXE_semblance"))
            .arg(&d)
            .arg(&sink)
            .output()
            .expect("sh runs the binary")
            .into();
        assert_eq!(r.code, Some(2), "{}", r.all());
        assert!(!r.stderr.contains("panicked"), "{}", r.stderr);
        assert_eq!(fs::metadata(&sink).unwrap().len(), 0, "nothing could be written");
    }
}

/// VALIDATE-17: `semblance help` lists validate with its flags, says demo,
/// roundtrip and evolve need no data while validate needs the fixtures
/// under data/ via `ci/fetch-fixtures.sh`, names the record, and no longer
/// says everything works with no data.
#[test]
fn validate_17_help_lists_validate_and_what_needs_data() {
    let r = semblance(&["help"]);
    assert_eq!(r.code, Some(0), "{}", r.all());
    let help = r.stdout.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(r.stdout.lines().any(|l| l.trim_start().starts_with("validate ")), "{}", r.stdout);
    assert!(help.contains("validate check that this build reproduces flyvis bit for bit [--data DIR] [--threads N] [--markdown]"), "{help}");
    assert!(help.contains("demo, roundtrip and evolve run on a synthetic graph and need no data."), "{help}");
    assert!(help.contains("validate needs flyvis's reference fixtures under data/"), "{help}");
    assert!(help.contains("ci/fetch-fixtures.sh"), "{help}");
    assert!(help.contains(RECORD), "{help}");
    assert!(!help.contains("works with no data"), "{help}");
}

/// VALIDATE-18: `semblance roundtrip` still prints both distances as
/// exactly 0.000000.
#[test]
fn validate_18_roundtrip_is_still_lossless() {
    let r = semblance(&["roundtrip"]);
    assert_eq!(r.code, Some(0), "{}", r.all());
    assert!(r.stdout.lines().any(|l| l == "structural distance:  0.000000"), "{}", r.stdout);
    assert!(r.stdout.lines().any(|l| l.starts_with("behavioural distance: 0.000000")), "{}", r.stdout);
}

/// VALIDATE-19: the CLI's module doc says std-only by design, with no
/// suggestion to swap in `clap`.
#[test]
fn validate_19_the_cli_module_doc_is_std_only_by_design() {
    let src = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs")).unwrap();
    let doc: String = src
        .lines()
        .take_while(|l| l.starts_with("//!") || l.trim().is_empty())
        .filter(|l| l.starts_with("//!"))
        .map(|l| l.trim_start_matches("//!").trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(doc.contains("std-only by design"), "{doc}");
    assert!(!doc.to_lowercase().contains("clap"), "{doc}");
}

/// VALIDATE-20: `semblance validate` reads only the data directory: it never
/// runs `tools/` or git. Run from a directory with no workspace in it, with
/// a PATH whose git, uv and python only record that they ran.
#[test]
fn validate_20_never_runs_tools_or_git() {
    let Some(g) = good() else { return };
    assert_eq!(g.run.code, Some(0), "it verified from ./data alone: {}", g.run.all());
    let ran = fs::read_to_string(&g.trap_log).unwrap_or_default();
    assert!(ran.is_empty(), "validate ran: {ran}");
}
