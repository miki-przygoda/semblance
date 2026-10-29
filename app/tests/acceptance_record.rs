//! Acceptance tests for behaviour group `record` of epic parity-validate:
//! the committed parity record, docs/results/0.3.0-flyvis-parity.md, and
//! its check against `semblance validate --markdown`. One test per
//! behaviour id, RECORD-01 .. RECORD-19.
//!
//! RECORD-01 .. RECORD-08 drive the built `semblance` binary: they spawn
//! `semblance validate --markdown` on the real fixtures (or on a data
//! directory of links to them with one file copied and altered, under
//! `CARGO_TARGET_TMPDIR`) and read its stdout.
//!
//! RECORD-09 .. RECORD-16 drive the record test, `app/tests/record.rs`, as a
//! user would: in a shadow copy of the workspace under `CARGO_TARGET_TMPDIR`
//! they write a record (the committed one, or the committed one altered),
//! lay out its `data/`, set or clear `SEMBLANCE_REQUIRE_FIXTURES` and
//! `SEMBLANCE_REQUIRE_LIBM_RECORD`, run `cargo test --test record`, and read
//! whether it passed and what it printed. The real record and the real
//! `data/` are never written.
//!
//! RECORD-17 .. RECORD-19 read what they are about: the committed record's
//! preamble, CI's workflow, and the tests CI's time cuts changed.
//!
//! A test that needs the real fixtures skips without them, printing
//! `skipping: ...`, unless `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns the
//! skip into a failure.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, MutexGuard, OnceLock};

use semblance_flyvis::parity::{how_to_get, required_files, CONTROL_MISS, COVERAGE_THRESHOLD, F64_BOUND};

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
const RECORD: &str = "docs/results/0.3.0-flyvis-parity.md";
const BEGIN: &str = "<!-- BEGIN semblance validate --markdown -->";
const END: &str = "<!-- END semblance validate --markdown -->";
const HOST_BEGIN: &str = "<!-- BEGIN host-specific -->";
const HOST_END: &str = "<!-- END host-specific -->";
const LIBM_BEGIN: &str = "<!-- BEGIN libm fingerprint=";
const LIBM_END: &str = "<!-- END libm -->";
/// A well-formed fingerprint no real libm has.
const FAKE_FP: &str = "f32:0:0000000000000000,f64:0:0000000000000000";
/// The record test's one fixture-reading test.
const RECORD_TEST: &str = "the_committed_record_is_what_validate_prints";

const SETTLED_F64: &str = "flyvis-reference/settled.f64.bin";
const PROVENANCE: &str = "flyvis-reference/PROVENANCE.json";
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

fn markdown_on(data: &Path, extra: &[&str]) -> Run {
    Command::new(env!("CARGO_BIN_EXE_semblance"))
        .args(["validate", "--markdown", "--data"])
        .arg(data)
        .args(extra)
        .output()
        .expect("the binary runs")
        .into()
}

// ---------------------------------------------------------------------------
// Data directories
// ---------------------------------------------------------------------------

fn real_data() -> PathBuf {
    Path::new(WORKSPACE).join("data")
}

fn require_fixtures() -> bool {
    std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1")
}

/// Whether the real fixtures are present; if not, a skip notice (false) or,
/// under `SEMBLANCE_REQUIRE_FIXTURES=1`, a failure.
fn fixtures_or_skip(test: &str) -> bool {
    let missing: Vec<&str> =
        required_files().iter().copied().filter(|rel| fs::metadata(real_data().join(rel)).is_err()).collect();
    if missing.is_empty() {
        return true;
    }
    if require_fixtures() {
        panic!("{test}: SEMBLANCE_REQUIRE_FIXTURES=1 but fixtures are missing: {missing:?}");
    }
    eprintln!("skipping: {test}: fixtures missing under {}: {missing:?}", real_data().display());
    false
}

fn tmp(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("acceptance-record-{name}"))
}

fn fresh(name: &str) -> PathBuf {
    let dir = tmp(name);
    if dir.symlink_metadata().is_ok() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
fn link(src: &Path, dst: &Path) {
    std::os::unix::fs::symlink(src, dst).unwrap();
}

#[cfg(not(unix))]
fn link(src: &Path, dst: &Path) {
    fs::copy(src, dst).unwrap();
}

/// Lays out `dir` as a data directory: every required file linked to the
/// real fixture, except those in `copied` (real copies, to alter) and
/// `absent` (left out). With `placeholders`, files are empty, not linked.
fn lay_out(dir: &Path, copied: &[&str], absent: &[&str], placeholders: bool) {
    if dir.symlink_metadata().is_ok() {
        fs::remove_dir_all(dir).unwrap();
    }
    fs::create_dir_all(dir).unwrap();
    for rel in required_files() {
        if absent.contains(rel) {
            continue;
        }
        let (src, dst) = (real_data().join(rel), dir.join(rel));
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        if placeholders {
            fs::write(&dst, b"").unwrap();
        } else if copied.contains(rel) {
            fs::copy(&src, &dst).unwrap();
        } else {
            link(&src, &dst);
        }
    }
}

fn set_f64(dir: &Path, rel: &str, i: usize, v: f64) {
    let p = dir.join(rel);
    let mut bytes = fs::read(&p).unwrap();
    bytes[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    fs::write(&p, bytes).unwrap();
}

// ---------------------------------------------------------------------------
// Shared runs of `validate --markdown`: each takes seconds, so each runs once
// ---------------------------------------------------------------------------

/// `semblance validate --markdown` on the real fixtures, default threads.
fn good() -> Option<&'static Run> {
    static GOOD: OnceLock<Run> = OnceLock::new();
    if !fixtures_or_skip("validate --markdown on the real fixtures") {
        return None;
    }
    Some(GOOD.get_or_init(|| {
        let r = markdown_on(&real_data(), &[]);
        assert_eq!(r.code, Some(0), "validate --markdown on valid fixtures must pass:\n{}", r.all());
        r
    }))
}

/// The local block's libm fingerprint.
fn here_fp(block: &str) -> String {
    let start = block.find(LIBM_BEGIN).expect("a libm marker") + LIBM_BEGIN.len();
    let len = block[start..].find(" -->").expect("the libm marker closes");
    block[start..start + len].to_string()
}

// ---------------------------------------------------------------------------
// Reading markdown
// ---------------------------------------------------------------------------

fn committed() -> String {
    fs::read_to_string(Path::new(WORKSPACE).join(RECORD)).unwrap_or_else(|e| panic!("{RECORD}: {e}"))
}

/// `text` from its one BEGIN through its one END, inclusive.
fn block_of(text: &str) -> &str {
    assert_eq!(text.matches(BEGIN).count(), 1, "one BEGIN");
    assert_eq!(text.matches(END).count(), 1, "one END");
    let (b, e) = (text.find(BEGIN).unwrap(), text.find(END).unwrap());
    &text[b..e + END.len()]
}

fn preamble_of(text: &str) -> &str {
    &text[..text.find(BEGIN).expect("a BEGIN marker")]
}

/// `text` from `begin` through `end`, inclusive, both exactly once.
fn between<'a>(text: &'a str, begin: &str, end: &str) -> &'a str {
    assert_eq!(text.matches(begin).count(), 1, "{begin} once in:\n{text}");
    assert_eq!(text.matches(end).count(), 1, "{end} once in:\n{text}");
    let (b, e) = (text.find(begin).unwrap(), text.find(end).unwrap());
    assert!(b < e, "{begin} before {end}");
    &text[b..e + end.len()]
}

/// `text` without its host-specific section.
fn without_host(text: &str) -> String {
    let host = between(text, HOST_BEGIN, HOST_END);
    text.replacen(host, "", 1)
}

/// `text` without its libm section.
fn without_libm(text: &str) -> String {
    let b = text.find(LIBM_BEGIN).expect("a libm section");
    let e = text.find(LIBM_END).expect("a libm end") + LIBM_END.len();
    format!("{}{}", &text[..b], &text[e..])
}

/// The lines of the section starting at the line `heading`, up to the next
/// `## ` heading or `<!-- ` marker.
fn section<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = text.lines().collect();
    let found: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].starts_with(heading)).collect();
    assert_eq!(found.len(), 1, "one {heading:?} heading in:\n{text}");
    let start = found[0];
    let end = (start + 1..lines.len())
        .find(|&i| lines[i].starts_with("## ") || lines[i].starts_with("<!-- "))
        .unwrap_or(lines.len());
    lines[start..end].to_vec()
}

/// A section's table rows as cells, header and separator excluded.
fn rows<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    let table: Vec<&str> = lines.iter().copied().filter(|l| l.starts_with('|')).collect();
    table
        .iter()
        .skip(2)
        .map(|l| {
            let inner = l.strip_prefix('|').unwrap().strip_suffix('|').unwrap_or_else(|| panic!("row ends in |: {l}"));
            inner.split(" | ").map(str::trim).collect()
        })
        .collect()
}

/// The two-column key/value rows of a `| | |` table.
fn kv<'a>(lines: &[&'a str]) -> Vec<(&'a str, &'a str)> {
    rows(lines).into_iter().map(|r| (r[0], r[1])).collect()
}

fn is_e(s: &str) -> bool {
    s.parse::<f64>().is_ok_and(|v| format!("{v:e}") == s)
}

fn is_3e(s: &str) -> bool {
    s.parse::<f64>().is_ok_and(|v| format!("{v:.3e}") == s)
}

fn is_2e(s: &str) -> bool {
    s.parse::<f64>().is_ok_and(|v| format!("{v:.2e}") == s)
}

fn is_int(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// The target as `{ARCH}-{OS}`, then `-{target_env}` when that is not empty.
fn target_here() -> String {
    let env = if cfg!(target_env = "gnu") {
        "gnu"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else if cfg!(target_env = "msvc") {
        "msvc"
    } else if cfg!(target_env = "sgx") {
        "sgx"
    } else if cfg!(target_env = "uclibc") {
        "uclibc"
    } else {
        ""
    };
    let base = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    if env.is_empty() { base } else { format!("{base}-{env}") }
}

/// Today's UTC date as YYYY-MM-DD (Howard Hinnant's civil_from_days).
fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

// ---------------------------------------------------------------------------
// A shadow copy of the workspace, to run the record test against altered
// records and data directories without touching the real ones
// ---------------------------------------------------------------------------

struct Shadow {
    root: PathBuf,
    target: PathBuf,
}

static SHADOW: Mutex<Option<Shadow>> = Mutex::new(None);

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "target" || name == "data" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            copy_tree(&src, &dst);
        } else if ty.is_file() {
            fs::copy(&src, &dst).unwrap();
        }
    }
}

fn shadow() -> MutexGuard<'static, Option<Shadow>> {
    let mut guard = SHADOW.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let base = tmp("shadow");
        let root = base.join("workspace");
        let ws = Path::new(WORKSPACE);
        match fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("removing the old shadow {}: {e}", root.display()),
        }
        fs::create_dir_all(&root).unwrap();
        for f in ["Cargo.toml", "Cargo.lock", "README.md", "LICENSE", "NOTICE"] {
            if ws.join(f).exists() {
                fs::copy(ws.join(f), root.join(f)).unwrap();
            }
        }
        copy_tree(&ws.join("crates"), &root.join("crates"));
        copy_tree(&ws.join("app"), &root.join("app"));
        copy_tree(&ws.join("docs/results"), &root.join("docs/results"));
        *guard = Some(Shadow { root, target: base.join("target") });
    }
    guard
}

/// The shadow's data/.
enum Data<'a> {
    /// Every required file linked to the real fixture.
    Real,
    /// An empty directory.
    Empty,
    /// Every required file but `absent`, as empty placeholders.
    AllBut(&'a str),
}

struct Outcome {
    passed: bool,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn all(&self) -> String {
        format!("passed: {}\n--- stdout\n{}--- stderr\n{}", self.passed, self.stdout, self.stderr)
    }
}

impl Shadow {
    /// Writes the shadow's record, or removes it.
    fn record(&self, text: Option<&str>) {
        let p = self.root.join(RECORD);
        let _ = fs::remove_file(&p);
        if let Some(t) = text {
            fs::write(&p, t).unwrap();
        }
    }

    fn data(&self, d: Data) -> PathBuf {
        let dir = self.root.join("data");
        match d {
            Data::Real => lay_out(&dir, &[], &[], false),
            Data::Empty => lay_out(&dir, &[], required_files(), true),
            Data::AllBut(rel) => lay_out(&dir, &[], &[rel], true),
        }
        dir
    }

    fn cargo(&self) -> Command {
        let mut c = Command::new(option_env!("CARGO").unwrap_or("cargo"));
        c.current_dir(&self.root)
            .env("CARGO_TARGET_DIR", &self.target)
            .env_remove("SEMBLANCE_REQUIRE_FIXTURES")
            .env_remove("SEMBLANCE_REQUIRE_LIBM_RECORD");
        c
    }

    /// Runs the record test's fixture-reading test with `env` set.
    fn run_record(&self, env: &[(&str, &str)], path: Option<&Path>) -> Outcome {
        let mut c = self.cargo();
        c.args(["test", "-p", "semblance", "--test", "record", "--locked", "--offline", "--", "--exact", RECORD_TEST])
            .args(["--nocapture", "--test-threads=1"]);
        for (k, v) in env {
            c.env(k, v);
        }
        if let Some(p) = path {
            let old = std::env::var_os("PATH").unwrap_or_default();
            let mut paths = vec![p.to_path_buf()];
            paths.extend(std::env::split_paths(&old));
            c.env("PATH", std::env::join_paths(paths).unwrap());
        }
        let out = c.output().expect("cargo runs");
        let o = Outcome {
            passed: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        };
        assert!(o.stdout.contains("running 1 test"), "the record test must run exactly once:\n{}", o.all());
        o
    }
}

/// The committed record with its libm fingerprint (marker and host row)
/// replaced by `fp`.
fn with_fp(text: &str, fp: &str) -> String {
    let old = here_fp(block_of(text));
    assert_eq!(text.matches(&old).count(), 2, "the fingerprint in the marker and the host row");
    text.replace(&old, fp)
}

/// Replaces the one occurrence of `from` in `text` with `to`.
fn swap(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from:?} exactly once");
    text.replacen(from, to, 1)
}

/// Inserts `line` into the preamble, after the title.
fn with_preamble_line(text: &str, line: &str) -> String {
    let (title, rest) = text.split_once('\n').unwrap();
    format!("{title}\n\n{line}\n{rest}")
}

fn combined(o: &Outcome) -> String {
    format!("{}{}", o.stdout, o.stderr)
}

// ---------------------------------------------------------------------------
// RECORD-01 .. RECORD-08: what `validate --markdown` prints
// ---------------------------------------------------------------------------

/// RECORD-01: on valid fixtures, stdout is the record's generated block,
/// from BEGIN to END, holding the reference section, a host-specific
/// section, bounds as applied, the exact settling and trace stages, a libm
/// section, controls and coverage, in that order.
#[test]
fn record_01_validate_markdown_prints_the_generated_block() {
    let Some(r) = good() else { return };
    let out = &r.stdout;
    assert!(out.starts_with(&format!("{BEGIN}\n")), "stdout starts with BEGIN:\n{out}");
    assert!(out.ends_with(&format!("{END}\n")), "stdout ends with END:\n{out}");
    let parts = [
        "## Reference (flyvis side)",
        HOST_BEGIN,
        HOST_END,
        "## Bounds as applied",
        "## Stage 1: settling, exact",
        "## Stage 2: traces, exact",
        LIBM_BEGIN,
        "## Stage 1: settling, libm",
        "## Stage 2: traces, libm",
        LIBM_END,
        "## Controls",
        "## Coverage",
    ];
    let mut at = 0;
    for p in parts {
        assert_eq!(out.matches(p).count(), 1, "{p:?} exactly once:\n{out}");
        let i = out.find(p).unwrap();
        assert!(i >= at, "{p:?} out of order:\n{out}");
        at = i;
    }
    // It is the record's block: everything but the host and libm sections
    // is the committed block, byte for byte.
    let rec = committed();
    assert_eq!(
        without_libm(&without_host(block_of(&rec))),
        without_libm(&without_host(block_of(out))),
        "validate --markdown is the record's generated block"
    );
}

/// RECORD-02: outside the host-specific section the block has no wall time,
/// date, path or thread count, and is deterministic: a run on another
/// thread count prints the same bytes outside the host section.
#[test]
fn record_02_outside_the_host_section_the_block_is_deterministic() {
    let Some(r) = good() else { return };
    let other = markdown_on(&real_data(), &["--threads", "7"]);
    assert_eq!(other.code, Some(0), "{}", other.all());
    assert!(other.stdout.contains("| threads | 7 |"), "{}", other.stdout);
    let a = without_host(&r.stdout);
    let b = without_host(&other.stdout);
    assert_eq!(a, b, "the block outside the host section depends on the thread count");
    let data = real_data();
    for needle in [today(), data.display().to_string(), WORKSPACE.to_string(), "| threads |".into()] {
        assert!(!a.contains(&needle), "{needle:?} outside the host section:\n{a}");
    }
    for l in a.lines() {
        let low = l.to_lowercase();
        assert!(!low.contains("elapsed") && !low.contains("wall") && !l.contains(" s]"), "a wall time: {l}");
        assert!(!l.contains("/Users/") && !l.contains("/home/") && !l.contains("/tmp/"), "a path: {l}");
    }
    // And the same holds of the committed record's block.
    let rec = without_host(block_of(&committed()));
    assert!(!rec.contains("| threads |"), "{rec}");
}

/// RECORD-03: the host-specific section holds exactly the semblance
/// version, the target, debug assertions, threads, the libm exp counts and
/// the libm fingerprint; no rustc version or build date.
#[test]
fn record_03_the_host_section_holds_the_build_and_libm_facts() {
    let Some(r) = good() else { return };
    let host = between(&r.stdout, HOST_BEGIN, HOST_END);
    let lines: Vec<&str> = host.lines().collect();
    let rows = kv(&lines);
    let keys: Vec<&str> = rows.iter().map(|r| r.0).collect();
    assert_eq!(
        keys,
        ["semblance", "target", "debug assertions", "threads", "libm exp", "libm fingerprint"],
        "{host}"
    );
    let get = |k: &str| rows.iter().find(|r| r.0 == k).unwrap().1;
    assert_eq!(get("semblance"), env!("CARGO_PKG_VERSION"));
    assert_eq!(get("target"), target_here());
    assert_eq!(get("debug assertions"), if cfg!(debug_assertions) { "on" } else { "off" });
    let threads = get("threads");
    assert!(is_int(threads), "{threads}");
    let want_threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(16);
    assert_eq!(threads, want_threads.to_string(), "default threads");
    let fp = get("libm fingerprint");
    let parts: Vec<&str> = fp.split([',', ':']).collect();
    assert_eq!(parts.len(), 6, "{fp}");
    assert!(parts[0] == "f32" && is_int(parts[1]) && is_hex(parts[2], 16), "{fp}");
    assert!(parts[3] == "f64" && is_int(parts[4]) && is_hex(parts[5], 16), "{fp}");
    assert_eq!(
        get("libm exp"),
        format!("f32: {} of 2355 counts differ from torch's; f64: {}", parts[1], parts[4]),
        "the exp counts agree with the fingerprint"
    );
    let low = host.to_lowercase();
    assert!(!low.contains("rustc") && !low.contains("rust 1."), "no rustc version: {host}");
    assert!(!host.contains(&today()), "no build date: {host}");
    // The committed record's host section has the same rows.
    let rec = committed();
    let rec_host: Vec<&str> = between(block_of(&rec), HOST_BEGIN, HOST_END).lines().collect();
    let rec_keys: Vec<&str> = kv(&rec_host).iter().map(|r| r.0).collect();
    assert_eq!(rec_keys, keys);
}

/// RECORD-04: the libm section is wrapped in `<!-- BEGIN libm
/// fingerprint={fp} -->` .. `<!-- END libm -->`, `{fp}` the running host's
/// fingerprint, and holds the libm settling and trace stages (and nothing
/// exact).
#[test]
fn record_04_the_libm_section_names_this_hosts_fingerprint() {
    let Some(r) = good() else { return };
    let host: Vec<&str> = between(&r.stdout, HOST_BEGIN, HOST_END).lines().collect();
    let fp = kv(&host).into_iter().find(|r| r.0 == "libm fingerprint").unwrap().1;
    let marker = format!("{LIBM_BEGIN}{fp} -->");
    assert_eq!(r.stdout.lines().filter(|l| *l == marker).count(), 1, "the marker line {marker:?}:\n{}", r.stdout);
    let libm = between(&r.stdout, &marker, LIBM_END);
    let headings: Vec<&str> = libm.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(headings, ["## Stage 1: settling, libm (Rust's exp)", "## Stage 2: traces, libm (Rust's exp), per stimulus"]);
    assert_eq!(rows(&section(libm, "## Stage 1: settling, libm")).len(), 16, "{libm}");
    assert_eq!(rows(&section(libm, "## Stage 2: traces, libm")).len(), 32, "{libm}");
    let outside = without_libm(&r.stdout);
    let libm_headings: Vec<&str> = outside.lines().filter(|l| l.starts_with("## ") && l.contains("libm")).collect();
    assert!(libm_headings.is_empty(), "no libm stage outside the libm section: {libm_headings:?}");
}

/// Checks RECORD-05's formats on one generated block.
fn check_formats(block: &str, what: &str) {
    // Integers have no separators.
    for l in block.lines() {
        let b = l.as_bytes();
        for i in 1..b.len().saturating_sub(1) {
            if matches!(b[i], b',' | b'\'') {
                assert!(!(b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit()), "{what}: a digit separator: {l}");
            }
        }
        for w in l.split(|c: char| c.is_whitespace() || c == '|') {
            let grouped = w.contains('_') && w.split('_').all(|g| !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit()));
            assert!(!grouped, "{what}: a digit separator: {l}");
        }
    }
    // Reference: input rounding {:e}, validation losses {}, SHAs in full.
    let reference = kv(&section(block, "## Reference"));
    let get = |k: &str| reference.iter().find(|r| r.0 == k).unwrap_or_else(|| panic!("{what}: no {k} row")).1;
    let rounding = get("input rounding");
    for v in rounding.split([' ', ';']).filter(|t| t.starts_with(|c: char| c.is_ascii_digit()) && t.contains('e')) {
        assert!(is_e(v), "{what}: input rounding {v}");
    }
    for m in get("models").split("; ") {
        let loss = m.split("validation loss ").nth(1).unwrap().split(',').next().unwrap();
        assert_eq!(format!("{}", loss.parse::<f64>().unwrap()), loss, "{what}: {m}");
        let sha = m.split("checkpoint sha256 ").nth(1).unwrap();
        assert!(is_hex(sha, 64), "{what}: {m}");
    }
    let exporter = get("exporter");
    for w in exporter.split([' ', ';', ',']) {
        if w.len() >= 40 && w.bytes().all(|b| b.is_ascii_hexdigit()) {
            assert!(w.len() == 64 || w.len() == 40, "{what}: a truncated hash {w}");
        }
    }
    assert_eq!(exporter.matches("sha256 ").count(), 2, "{what}: {exporter}");
    // Bounds as applied: {:e}.
    for r in rows(&section(block, "## Bounds as applied")) {
        assert!(r[2] == "==" || is_e(r[2]), "{what}: bound {r:?}");
        if let Some(x) = r[3].rsplit("= 2 × ").next().filter(|_| r[3].contains("= 2 × ")) {
            assert!(is_e(x), "{what}: derivation {r:?}");
        }
    }
    // Exact stages: integers.
    for h in ["## Stage 1: settling, exact", "## Stage 2: traces, exact"] {
        for r in rows(&section(block, h)) {
            let n = r.len();
            assert!(is_int(r[n - 3]) && is_int(r[n - 2]) && (is_int(r[n - 1]) || is_3e(r[n - 1])), "{what}: {r:?}");
        }
    }
    // libm stages: {:.3e}, worst types {:.2e}.
    for r in rows(&section(block, "## Stage 1: settling, libm")) {
        assert!(is_int(r[5]) && is_int(r[6]), "{what}: {r:?}");
        assert!(is_3e(r[7]) && is_3e(r[9]) && is_3e(r[10]), "{what}: {r:?}");
        if !r[11].is_empty() {
            for t in r[11].split(", ") {
                let v = t.rsplit(' ').next().unwrap();
                assert!(is_2e(v), "{what}: worst type {t} in {r:?}");
            }
        }
    }
    for r in rows(&section(block, "## Stage 2: traces, libm")) {
        assert!(is_int(r[6]) && is_int(r[7]), "{what}: {r:?}");
        assert!(is_3e(r[8]) && is_3e(r[10]) && is_3e(r[11]), "{what}: {r:?}");
    }
    // Controls: measured {:.3e}, the mirrored control {:e} in full,
    // thresholds {:e}.
    let controls = rows(&section(block, "## Controls"));
    assert_eq!(controls.len(), 4, "{what}");
    for r in &controls {
        let measured = r[2];
        let nums: Vec<&str> = measured
            .split([' ', ','])
            .filter(|t| t.starts_with(|c: char| c.is_ascii_digit()) && t.contains('e'))
            .collect();
        if r[0].contains("mirrored") {
            assert!(!nums.is_empty(), "{what}: {r:?}");
            for v in nums {
                assert!(is_e(v), "{what}: mirrored {v}");
            }
        } else {
            for v in nums {
                assert!(is_3e(v), "{what}: {v} in {r:?}");
            }
        }
        if let Some(t) = r[3].strip_prefix("> ") {
            assert!(is_e(t), "{what}: threshold {t}");
        } else {
            assert_eq!(r[3], "==", "{what}: {r:?}");
        }
    }
    assert_eq!(controls[0][3], format!("> {CONTROL_MISS:e}"));
    assert_eq!(controls[1][3], format!("> {F64_BOUND:e}"));
    // Coverage: {:.3e}, the threshold {:e}.
    let cov = section(block, "## Coverage");
    assert!(cov.iter().any(|l| l.contains(&format!("exceeds {COVERAGE_THRESHOLD:e}:"))), "{what}: {cov:?}");
    for r in rows(&cov) {
        if r[1].starts_with("not measured") {
            continue;
        }
        assert!(is_3e(r[2]) && is_3e(r[4]) && is_3e(r[5]) && is_3e(r[6]), "{what}: {r:?}");
        for t in r[3].split(", ") {
            assert!(is_3e(t.rsplit(' ').next().unwrap()), "{what}: next two {r:?}");
        }
    }
}

/// RECORD-05: integers have no separators; bounds as applied, thresholds
/// and input rounding `{:e}`; other values, bounds and ratios `{:.3e}`;
/// worst-type values `{:.2e}`; the mirrored control `{:e}`; validation
/// losses `{}`; SHAs in full. Checked on this host's output and on the
/// committed block.
#[test]
fn record_05_numbers_are_printed_in_their_formats() {
    check_formats(block_of(&committed()), "the committed block");
    let Some(r) = good() else { return };
    check_formats(block_of(&r.stdout), "validate's output");
}

/// RECORD-06: equal input rounding prints one value "at 1/200 and 1/50";
/// different values print `{a:e} at 1/200; {b:e} at 1/50`.
#[test]
fn record_06_input_rounding_prints_one_value_when_both_are_equal() {
    let Some(r) = good() else { return };
    let prov = fs::read_to_string(real_data().join(PROVENANCE)).unwrap();
    let at = prov.find("\"input_rounding\"").expect("PROVENANCE has input_rounding");
    let value = |key: &str| -> (usize, usize, f64) {
        let k = prov[at..].find(&format!("\"{key}\"")).unwrap() + at;
        let colon = prov[k..].find(':').unwrap() + k + 1;
        let start = colon + prov[colon..].len() - prov[colon..].trim_start().len();
        let len = prov[start..].find([',', '\n', '}']).unwrap();
        let s = prov[start..start + len].trim_end();
        (start, s.len(), s.parse().unwrap())
    };
    let (_, _, a) = value("1/200");
    let (start, len, b) = value("1/50");
    assert_eq!(a.to_bits(), b.to_bits(), "the real fixtures' input rounding is equal at both dts");
    let row = r.stdout.lines().find(|l| l.starts_with("| input rounding |")).expect("an input-rounding row");
    assert_eq!(row, format!("| input rounding | {a:e} at 1/200 and 1/50 (the f32 runs' moving edges) |"));

    // Unequal: 1/50's value altered in a copied PROVENANCE.json.
    let d = tmp("06-unequal");
    lay_out(&d, &[PROVENANCE], &[], false);
    let altered = format!("{}1.5e-08{}", &prov[..start], &prov[start + len..]);
    fs::write(d.join(PROVENANCE), altered).unwrap();
    let u = markdown_on(&d, &[]);
    assert_eq!(u.code, Some(0), "{}", u.all());
    let row = u.stdout.lines().find(|l| l.starts_with("| input rounding |")).expect("an input-rounding row");
    assert_eq!(row, format!("| input rounding | {a:e} at 1/200; 1.5e-8 at 1/50 (the f32 runs' moving edges) |"));
}

/// RECORD-07: a run whose exact job failed reads `not measured: exact job
/// failed` in the coverage table, and the closing sentence names the
/// unmeasured run instead of claiming every type is driven.
#[test]
fn record_07_coverage_names_a_run_whose_exact_job_failed() {
    if !fixtures_or_skip("record_07") {
        return;
    }
    // run 3's settled value at node 100 is not flyvis's: run 3's exact job fails.
    let d = tmp("07-exact");
    lay_out(&d, &[SETTLED_F64], &[], false);
    // settled.f64.bin holds runs 1, 3, 5, 7, one block each: run 3 is block 1.
    set_f64(&d, SETTLED_F64, N_NODES + 100, 7.0);
    let r = markdown_on(&d, &[]);
    assert_eq!(r.code, Some(1), "{}", r.all());
    let cov = section(&r.stdout, "## Coverage");
    let table = rows(&cov);
    assert_eq!(table.len(), 8, "{cov:?}");
    for row in &table {
        if row[0] == "3" {
            assert_eq!(row[1], "not measured: exact job failed", "{row:?}");
        } else {
            assert!(!row[1].contains("not measured"), "{row:?}");
        }
    }
    let closing: Vec<&str> = cov.iter().copied().filter(|l| !l.is_empty() && !l.starts_with('|')).skip(2).collect();
    let closing = closing.join(" ");
    assert!(closing.contains('3') && closing.contains("not measured"), "names run 3: {closing}");
    assert!(!closing.contains("Every one of the 65"), "does not claim every type is driven: {closing}");
    assert!(closing.contains("does not show that every type is driven"), "{closing}");
}

/// RECORD-08: the coverage section states the absolute 1e-3 threshold
/// (driven if above, recorded not a gate) and per run the weakest type, its
/// movement, the next two, the median, the libm centre bound and weakest /
/// bound.
#[test]
fn record_08_coverage_states_the_threshold_and_each_runs_weakest_types() {
    let Some(r) = good() else { return };
    let check = |block: &str, what: &str| {
        let cov = section(block, "## Coverage");
        let text = cov.join("\n");
        assert!(
            text.contains(&format!("A type is driven if its movement exceeds {COVERAGE_THRESHOLD:e}: recorded, not a gate.")),
            "{what}: {text}"
        );
        let header = cov.iter().find(|l| l.starts_with("| run |")).expect("a header");
        assert_eq!(*header, "| run | weakest type | movement | next two | median | libm centre bound | weakest / bound |");
        let bounds = rows(&section(block, "## Bounds as applied"));
        let table = rows(&cov);
        assert_eq!(table.iter().map(|r| r[0]).collect::<Vec<_>>(), ["0", "1", "2", "3", "4", "5", "6", "7"], "{what}");
        let mut weakest = (f64::INFINITY, "", "");
        for row in &table {
            let m: f64 = row[2].parse().unwrap();
            assert!(m > COVERAGE_THRESHOLD, "{what}: driven {row:?}");
            let next: Vec<f64> = row[3].split(", ").map(|t| t.rsplit(' ').next().unwrap().parse().unwrap()).collect();
            assert_eq!(next.len(), 2, "{what}: {row:?}");
            let median: f64 = row[4].parse().unwrap();
            assert!(m <= next[0] && next[0] <= next[1] && next[1] <= median, "{what}: sorted {row:?}");
            // The libm centre bound is that run's `centre, libm` bound.
            let run = row[0];
            let bound = bounds
                .iter()
                .find(|b| b[0] == run && (b[1] == "centre, libm" || b[1] == "every quantity, libm"))
                .unwrap_or_else(|| panic!("{what}: no centre bound for run {run}"));
            let b: f64 = bound[2].parse().unwrap();
            assert_eq!(row[5], format!("{b:.3e}"), "{what}: {row:?}");
            let ratio: f64 = row[6].parse().unwrap();
            assert!(((ratio - m / b) / ratio).abs() < 2e-3, "{what}: weakest / bound {row:?}");
            if m < weakest.0 {
                weakest = (m, row[0], row[1]);
            }
        }
        let closing = cov.last().unwrap();
        assert!(
            closing.contains(&format!("the weakest, {} in run ", weakest.2)) && closing.contains(&format!("by {:.3e})", weakest.0)),
            "{what}: {closing}"
        );
        assert!(closing.contains("Every one of the 65 cell types moves by more than 1e-3 in every run"), "{what}: {closing}");
    };
    check(block_of(&committed()), "the committed block");
    check(block_of(&r.stdout), "validate's output");
}

// ---------------------------------------------------------------------------
// RECORD-09 .. RECORD-16: the record test
// ---------------------------------------------------------------------------

/// RECORD-09: the record test fails, never skips, on any host and with or
/// without fixtures, unless the committed file is well formed and its
/// preamble obeys the numeral rule.
#[test]
fn record_09_a_malformed_record_fails_with_or_without_fixtures() {
    let rec = committed();
    let block = block_of(&rec).to_string();
    let host = between(&block, HOST_BEGIN, HOST_END).to_string();
    let libm_b = block.find(LIBM_BEGIN).unwrap();
    let libm = &block[libm_b..block.find(LIBM_END).unwrap() + LIBM_END.len()];
    let fp = here_fp(&block);
    let cases: Vec<(&str, Option<String>)> = vec![
        ("no record file", None),
        ("the validate block twice", Some(format!("{rec}\n{block}\n"))),
        ("no END", Some(swap(&rec, END, ""))),
        ("no BEGIN", Some(swap(&rec, BEGIN, ""))),
        ("no host-specific section", Some(swap(&rec, &host, ""))),
        ("the host-specific section twice", Some(swap(&rec, &host, &format!("{host}\n{host}")))),
        ("no libm section", Some(swap(&rec, libm, ""))),
        ("two libm sections", Some(swap(&rec, libm, &format!("{libm}\n{libm}")))),
        ("an unparsable fingerprint", Some(with_fp(&rec, "f32:104:not-hex,f64:104:83eefcb97e81d132"))),
        ("a marker that disagrees with the host row", Some(swap(&rec, &format!("{LIBM_BEGIN}{fp} -->"), &format!("{LIBM_BEGIN}{FAKE_FP} -->")))),
        ("a preamble numeral the block lacks", Some(with_preamble_line(&rec, "The worst ratio is about 0.86."))),
    ];
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    // The committed record passes without fixtures (it skips at the fixture
    // step, after the record's checks).
    sh.data(Data::Empty);
    sh.record(Some(&rec));
    let ok = sh.run_record(&[], None);
    assert!(ok.passed, "the committed record passes its own checks:\n{}", ok.all());
    for (what, text) in &cases {
        sh.record(text.as_deref());
        let o = sh.run_record(&[], None);
        assert!(!o.passed, "{what}: must fail without fixtures:\n{}", o.all());
        assert!(!combined(&o).contains("skipping:"), "{what}: failed before the fixture step, not skipped:\n{}", o.all());
    }
    // With fixtures present too, a malformed record fails before validate runs.
    if fixtures_or_skip("record_09 with fixtures") {
        sh.data(Data::Real);
        for (what, text) in [&cases[0], &cases[9], &cases[10]] {
            sh.record(text.as_deref());
            let o = sh.run_record(&[], None);
            assert!(!o.passed, "{what}: must fail with fixtures:\n{}", o.all());
        }
    }
    sh.record(Some(&rec));
}

/// RECORD-10: every preamble numeral (a maximal run of `0-9 . e + -`
/// starting with a digit, containing `.` or `e` or three or more digits)
/// occurs verbatim in the block, except versions (two or more `.`) and
/// tokens on the `Generated:` line.
#[test]
fn record_10_preamble_numerals_must_occur_in_the_block() {
    let rec = committed();
    let generated = rec.lines().find(|l| l.contains("Generated:")).expect("a Generated: line").to_string();
    let cases: Vec<(&str, String, bool)> = vec![
        ("a ratio the block prints", with_preamble_line(&rec, "The worst ratio in the record is 7.428e-1."), true),
        ("a sentence-final numeral", with_preamble_line(&rec, "The weakest moves by 3.773e-3."), true),
        ("a rounded ratio", with_preamble_line(&rec, "The worst ratio is about 0.86."), false),
        ("three digits", with_preamble_line(&rec, "That is 744 times the bound."), false),
        ("an exponent the block lacks", with_preamble_line(&rec, "A bound of 1e-11."), false),
        ("two digits", with_preamble_line(&rec, "12 sequences, 65 types, 50 models."), true),
        ("versions", with_preamble_line(&rec, "flyvis 1.2.0 and semblance 10.20.30 and 98.76.54.32."), true),
        ("the Generated: line", swap(&rec, &generated, &format!("{generated}, 999.5 and 12345 and 0.86")), true),
    ];
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(Data::Empty);
    for (what, text, pass) in &cases {
        sh.record(Some(text));
        let o = sh.run_record(&[], None);
        assert_eq!(o.passed, *pass, "{what}:\n{}", o.all());
        if !pass {
            assert!(!combined(&o).contains("skipping:"), "{what}: failed, not skipped:\n{}", o.all());
        }
    }
    let o = {
        sh.record(Some(&cases[2].1));
        sh.run_record(&[], None)
    };
    assert!(combined(&o).contains("0.86"), "the failure names the token:\n{}", o.all());
    sh.record(Some(&rec));
}

/// RECORD-11: a missing fixture prints `skipping: {path} not found.
/// {how_to_get(rel)}` and passes; under `SEMBLANCE_REQUIRE_FIXTURES=1` it
/// panics with `SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing`.
#[test]
fn record_11_a_missing_fixture_skips_or_under_the_flag_fails() {
    let rec = committed();
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.record(Some(&rec));
    let data_dir = format!("{}/app/../data", sh.root.display());
    let first = required_files()[0];
    let last = *required_files().last().unwrap();
    for (layout, rel) in [(Data::Empty, first), (Data::AllBut(last), last)] {
        sh.data(layout);
        let path = format!("{data_dir}/{rel}");
        let o = sh.run_record(&[], None);
        assert!(o.passed, "a missing fixture skips:\n{}", o.all());
        let want = format!("skipping: {path} not found. {}", how_to_get(rel));
        assert!(combined(&o).contains(&want), "expected {want:?}:\n{}", o.all());
        let o = sh.run_record(&[("SEMBLANCE_REQUIRE_FIXTURES", "1")], None);
        assert!(!o.passed, "under SEMBLANCE_REQUIRE_FIXTURES=1 a missing fixture fails:\n{}", o.all());
        let want = format!("SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
        assert!(combined(&o).contains(&want), "expected {want:?}:\n{}", o.all());
    }
}

/// A PATH directory whose git, uv, python and python3 only record that they
/// were run, and the log they write to.
fn trap() -> (PathBuf, PathBuf) {
    let dir = fresh("trap");
    let log = dir.join("ran.log");
    for tool in ["git", "uv", "python", "python3"] {
        let p = dir.join(tool);
        fs::write(&p, format!("#!/bin/sh\necho \"{tool} $*\" >> '{}'\nexit 1\n", log.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    (dir, log)
}

/// RECORD-12: with fixtures present the record test runs the built binary
/// (never tools/ or git), requires exit 0, removes the host-specific
/// sections, and compares the rest byte for byte, naming the first
/// differing line of each side.
#[test]
fn record_12_the_record_test_compares_validates_block_byte_for_byte() {
    if !fixtures_or_skip("record_12") {
        return;
    }
    let rec = committed();
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(Data::Real);

    // The host-specific section is never compared, whatever it holds.
    let host_altered = rec
        .replacen("| threads | ", "| threads | 99", 1)
        .replacen("| debug assertions | ", "| debug assertions | maybe ", 1);
    assert_ne!(host_altered, rec);
    sh.record(Some(&host_altered));
    let (path, log) = trap();
    let o = sh.run_record(&[], Some(&path));
    assert!(o.passed, "the committed record matches validate's output:\n{}", o.all());
    let ran = fs::read_to_string(&log).unwrap_or_default();
    assert!(ran.is_empty(), "the record test ran: {ran}");

    // Outside the host and libm sections, one changed number fails, naming
    // the first differing line of each side.
    let line = rec.lines().find(|l| l.starts_with("| model 049's parameters against model 000's reference |")).unwrap();
    let measured = line.split(" | ").nth(2).unwrap();
    let changed_measured = format!("{measured}0");
    let changed = line.replacen(measured, &changed_measured, 1);
    sh.record(Some(&swap(&rec, line, &changed)));
    let o = sh.run_record(&[], None);
    assert!(!o.passed, "a changed control must fail:\n{}", o.all());
    let out = combined(&o);
    assert!(out.contains(&changed) && out.contains(line), "both sides of the first differing line:\n{out}");
    sh.record(Some(&rec));
}

/// RECORD-13: where this host's libm fingerprint is the record's, the libm
/// section is compared byte for byte too.
#[test]
fn record_13_an_equal_fingerprint_compares_the_libm_section() {
    let Some(g) = good() else { return };
    // A record made from this host's own output: its fingerprint is ours.
    let rec = format!("# A record\n\n**Generated:** here\n\n{}", g.stdout);
    let first_libm_row = between(&g.stdout, LIBM_BEGIN, LIBM_END)
        .lines()
        .find(|l| l.starts_with("| 0 |"))
        .expect("a libm row for run 0")
        .to_string();
    let cells: Vec<&str> = first_libm_row.split(" | ").collect();
    let unequal = cells[6];
    let changed_row = first_libm_row.replacen(&format!(" | {unequal} | "), &format!(" | {}1 | ", unequal), 1);
    assert_ne!(changed_row, first_libm_row);

    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(Data::Real);
    sh.record(Some(&rec));
    let o = sh.run_record(&[], None);
    assert!(o.passed, "this host's own block matches:\n{}", o.all());
    assert!(!combined(&o).contains("libm sections not compared"), "the libm section was compared:\n{}", o.all());
    let o = sh.run_record(&[("SEMBLANCE_REQUIRE_LIBM_RECORD", "1")], None);
    assert!(o.passed, "an equal fingerprint satisfies SEMBLANCE_REQUIRE_LIBM_RECORD=1:\n{}", o.all());

    sh.record(Some(&swap(&rec, &first_libm_row, &changed_row)));
    let o = sh.run_record(&[], None);
    assert!(!o.passed, "a changed libm row must fail when fingerprints agree:\n{}", o.all());
    let out = combined(&o);
    assert!(out.contains(&changed_row) && out.contains(&first_libm_row), "both sides of the differing libm line:\n{out}");
    sh.record(Some(&committed()));
}

/// RECORD-14: where the fingerprints differ, `SEMBLANCE_REQUIRE_LIBM_RECORD=1`
/// fails with `SEMBLANCE_REQUIRE_LIBM_RECORD=1 but this host's libm
/// fingerprint {x} differs from the record's {y}`.
#[test]
fn record_14_require_libm_record_fails_on_a_different_fingerprint() {
    let Some(g) = good() else { return };
    let here = here_fp(&g.stdout);
    let rec = with_fp(&committed(), FAKE_FP);
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(Data::Real);
    sh.record(Some(&rec));
    let o = sh.run_record(&[("SEMBLANCE_REQUIRE_LIBM_RECORD", "1")], None);
    assert!(!o.passed, "a different fingerprint under the flag must fail:\n{}", o.all());
    let want = format!("SEMBLANCE_REQUIRE_LIBM_RECORD=1 but this host's libm fingerprint {here} differs from the record's {FAKE_FP}");
    assert!(combined(&o).contains(&want), "expected {want:?}:\n{}", o.all());
    sh.record(Some(&committed()));
}

/// RECORD-15: where the fingerprints differ and the flag is unset, the libm
/// sections are not compared, the test says so on stderr, and everything
/// else is still compared.
#[test]
fn record_15_a_different_fingerprint_skips_only_the_libm_section() {
    let Some(g) = good() else { return };
    let here = here_fp(&g.stdout);
    let base = with_fp(&committed(), FAKE_FP);
    // The libm section's contents altered too: not compared.
    let libm = between(block_of(&base), LIBM_BEGIN, LIBM_END).to_string();
    let rows_only: String = libm.lines().filter(|l| l.starts_with("| 0 |")).take(1).collect();
    let altered_libm = libm.replacen(&rows_only, "| 0 | nothing like the real row |", 1);
    let rec = swap(&base, &libm, &altered_libm);

    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.data(Data::Real);
    sh.record(Some(&rec));
    let o = sh.run_record(&[], None);
    assert!(o.passed, "the libm section is not compared on another libm:\n{}", o.all());
    let want = format!("record: libm sections not compared: this host's libm fingerprint {here} differs from the record's {FAKE_FP}");
    assert!(o.stderr.contains(&want), "expected on stderr {want:?}:\n{}", o.all());

    // Everything else still is.
    let line = rec.lines().find(|l| l.starts_with("| start from flyvis's settled.f64.bin")).unwrap();
    let changed = line.replacen("| 0 of ", "| 1 of ", 1);
    sh.record(Some(&swap(&rec, line, &changed)));
    let o = sh.run_record(&[], None);
    assert!(!o.passed, "outside the libm section a difference still fails:\n{}", o.all());
    assert!(combined(&o).contains(&changed), "{}", o.all());
    sh.record(Some(&committed()));
}

/// RECORD-16: the record test's text functions are unit-tested on synthetic
/// strings, and those tests pass: run here as the suite runs them, every
/// test in `app/tests/record.rs` but the one that reads fixtures.
#[test]
fn record_16_the_record_tests_text_functions_are_unit_tested_and_pass() {
    let guard = shadow();
    let sh = guard.as_ref().unwrap();
    sh.record(Some(&committed()));
    sh.data(Data::Empty);
    let out = sh
        .cargo()
        .args(["test", "-p", "semblance", "--test", "record", "--locked", "--offline", "--", "--skip", RECORD_TEST])
        .output()
        .expect("cargo runs");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "the record test's unit tests pass:\n{text}");
    let n: usize = text
        .lines()
        .find_map(|l| l.strip_prefix("running ").and_then(|r| r.split(' ').next()).and_then(|n| n.parse().ok()))
        .expect("a running line");
    assert!(n >= 5, "placeholder, markers, host fingerprint, numerals and the libm decision: {n} tests:\n{text}");
    assert!(text.contains(&format!("test result: ok. {n} passed; 0 failed")), "{text}");
    // What those unit tests cover, through the record test itself:
    // placeholder replacement (RECORD-15), markers (RECORD-09), a disagreeing
    // host fingerprint (RECORD-09), numerals (RECORD-10) and the libm
    // decision on two fingerprints and a flag (RECORD-13, -14, -15).
}

// ---------------------------------------------------------------------------
// RECORD-17 .. RECORD-19: the preamble, CI, and the time cuts
// ---------------------------------------------------------------------------

/// RECORD-17: the hand-written preamble covers the claim and its three
/// gates, exact and libm, never relaxing, the ratio explanation quoting the
/// block's worst ratio, the recording host, the fixtures, syn_count_torch.rs,
/// the Sintel decision with runs 2, 4 and 6's ratios, and one `Generated:`
/// line with toolchain, CI run URL and date.
#[test]
fn record_17_the_preamble_covers_what_the_record_must_say() {
    let rec = committed();
    let pre = preamble_of(&rec);
    let flat = pre.split_whitespace().collect::<Vec<_>>().join(" ");
    let block = block_of(&rec);
    for needle in [
        "## What 0.3.0 claims",
        "Three gates",
        "**exact.**",
        "**libm.**",
        "**controls.**",
        "`==`",
        "+0.0 or -0.0",
        "ever relaxed",
        "f32",
        "f64",
        "ubuntu-latest",
        "fixtures-2",
        "ci/fixtures.sha256",
        "syn_count_torch.rs",
        "Sintel",
        "trigger 1",
    ] {
        assert!(flat.contains(needle), "the preamble lacks {needle:?}");
    }
    // The worst ratio from the block's libm section, quoted.
    let libm = between(block, LIBM_BEGIN, LIBM_END);
    let mut ratios: Vec<&str> = rows(&section(libm, "## Stage 1: settling, libm")).into_iter().map(|r| r[10]).collect();
    ratios.extend(rows(&section(libm, "## Stage 2: traces, libm")).into_iter().map(|r| r[11]));
    let worst = ratios
        .into_iter()
        .map(|s| (s.parse::<f64>().unwrap(), s.to_string()))
        .fold((0.0, String::new()), |a, b| if b.0 > a.0 { b } else { a });
    assert!(worst.0 > 0.0, "a worst ratio");
    assert!(flat.contains(&format!("worst ratio in the record is {}", worst.1)), "quotes the worst ratio {}: {flat}", worst.1);
    // Runs 2, 4 and 6's coverage ratios, as printed.
    let cov = rows(&section(block, "## Coverage"));
    for run in ["2", "4", "6"] {
        let r = cov.iter().find(|r| r[0] == run).unwrap();
        assert!(flat.contains(r[6]), "run {run}'s ratio {} in the Sintel paragraph", r[6]);
    }
    // One Generated: line: toolchain, CI run URL, date.
    let generated_lines: Vec<&str> = pre.lines().filter(|l| l.contains("Generated:")).collect();
    assert_eq!(generated_lines.len(), 1, "one Generated: line");
    let g = generated_lines[0];
    assert!(g.contains("Rust 1."), "a toolchain: {g}");
    assert!(g.contains("https://github.com/") && g.contains("/actions/runs/"), "a CI run URL: {g}");
    let has_date = g.as_bytes().windows(10).any(|w| {
        w.iter().enumerate().all(|(i, b)| if i == 4 || i == 7 { *b == b'-' } else { b.is_ascii_digit() })
    });
    assert!(has_date, "a date: {g}");
}

/// The `test` job's text in ci.yml.
fn test_job(ci: &str) -> &str {
    let start = ci.find("\n  test:\n").expect("a test job");
    let rest = &ci[start + 1..];
    // The job ends at the next line indented by exactly two spaces.
    let mut len = 0;
    for (i, l) in rest.split_inclusive('\n').enumerate() {
        if i > 0 && l.starts_with("  ") && !l.starts_with("   ") {
            break;
        }
        len += l.len();
    }
    &rest[..len]
}

/// The steps of a job, each as its text.
fn steps(job: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in job.lines() {
        if l.starts_with("      - ") {
            out.push(String::new());
        }
        if let Some(s) = out.last_mut() {
            s.push_str(l);
            s.push('\n');
        }
    }
    out
}

/// RECORD-18: CI's test job runs `target/debug/semblance validate
/// --markdown > parity-record.md` after Build and before the test step,
/// uploads it as `parity-record` with `if: always()` through a pinned
/// upload-artifact, lets a gate failure fail the job, and sets
/// `SEMBLANCE_REQUIRE_LIBM_RECORD: "1"` on the test step.
#[test]
fn record_18_ci_writes_and_uploads_the_record_and_requires_the_libm_record() {
    let ci = fs::read_to_string(Path::new(WORKSPACE).join(".github/workflows/ci.yml")).unwrap();
    let job = test_job(&ci);
    let st = steps(job);
    let pos = |pred: &dyn Fn(&str) -> bool, what: &str| -> usize {
        let found: Vec<usize> = (0..st.len()).filter(|&i| pred(&st[i])).collect();
        assert_eq!(found.len(), 1, "one {what} step in the test job:\n{job}");
        found[0]
    };
    let build = pos(&|s| s.contains("run: cargo build --workspace"), "Build");
    let validate = pos(&|s| s.contains("run: target/debug/semblance validate --markdown > parity-record.md"), "validate");
    let upload = pos(&|s| s.contains("actions/upload-artifact@"), "upload");
    let test = pos(&|s| s.contains("run: cargo test --workspace"), "test");
    assert!(build < validate && validate < test, "after Build, before the tests");
    assert!(validate < upload && upload < test, "the upload follows the validate step");
    let v = &st[validate];
    assert!(!v.contains("continue-on-error") && !v.contains("|| true"), "a gate failure fails the job: {v}");
    let u = &st[upload];
    assert!(u.contains("uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02"), "{u}");
    assert!(u.contains("if: always()"), "{u}");
    assert!(u.contains("name: parity-record"), "{u}");
    assert!(u.contains("path: parity-record.md"), "{u}");
    let t = &st[test];
    assert!(t.contains("SEMBLANCE_REQUIRE_LIBM_RECORD: \"1\""), "{t}");
    assert!(t.contains("SEMBLANCE_REQUIRE_FIXTURES: \"1\""), "{t}");
}

/// The body of `fn name` in `src`, up to the next top-level item.
fn body<'a>(src: &'a str, name: &str) -> &'a str {
    let start = src.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &src[start..];
    let end = rest.find("\n}\n").map_or(rest.len(), |e| e + 3);
    &rest[..end]
}

/// RECORD-19: MINORS-15 no longer re-runs `trace_checks::`; TRACES-06 runs
/// the 8 exact trace gates in one nested invocation asserting `running 8
/// tests` and the same 8 lines; SEQUENCES-08 has no nested cargo and its new
/// name; the test job's cache no longer lists `target`; and no control,
/// bound or TOLERANCE changed.
#[test]
fn record_19_the_ci_time_cuts_keep_every_gate() {
    let ws = Path::new(WORKSPACE);
    let read = |p: &str| fs::read_to_string(ws.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));

    let minors = read("crates/flyvis/tests/acceptance_minors.rs");
    let m15 = body(&minors, "minors_15_settling_and_stimulus_gate_results_are_unchanged");
    let code: String = m15.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    assert!(!code.contains("trace_checks::"), "MINORS-15 re-runs the trace gates:\n{m15}");

    let traces = read("crates/flyvis/tests/acceptance_traces.rs");
    let t6 = body(&traces, "traces_06_torch_counts_trace_bit_equal_to_flyvis_in_all_eight_runs");
    assert_eq!(t6.matches(".cargo()").count() + t6.matches("Command::new").count(), 1, "one nested invocation:\n{t6}");
    assert!(t6.contains("trace_checks::trace_exact_run_"), "the exact trace gates:\n{t6}");
    assert!(t6.contains("\"running 8 tests\""), "asserts running 8 tests:\n{t6}");
    assert!(t6.contains("12 of 12 sequences bit-equal, centre and snapshots"), "the same 8 lines:\n{t6}");
    assert!(t6.contains("for (run, name) in EXACT_GATES"), "all 8 gates:\n{t6}");

    let seqs = read("crates/flyvis/tests/acceptance_sequences.rs");
    assert!(!seqs.contains("fn sequences_08_manifest_parse_its_tests_and_acceptance_reference_are_unchanged_and_pass"));
    let s8 = body(&seqs, "sequences_08_the_manifest_and_sequences_parse_in_process");
    assert!(!s8.contains("Command") && !s8.contains("cargo()") && !s8.contains("\"cargo\""), "no nested cargo:\n{s8}");

    let ci = read(".github/workflows/ci.yml");
    let job = test_job(&ci);
    for s in steps(job).iter().filter(|s| s.contains("uses: actions/cache@")) {
        let paths: Vec<&str> = s.lines().map(str::trim).collect();
        assert!(!paths.contains(&"target"), "the test job's cache lists target:\n{s}");
    }

    // No control, bound or tolerance moved.
    assert_eq!(F64_BOUND, 1e-10);
    assert_eq!(CONTROL_MISS, 1e-3);
    assert_eq!(COVERAGE_THRESHOLD, 1e-3);
    let me = read("crates/flyvis/src/moving_edge_checks.rs");
    assert!(me.contains("const TOLERANCE: f64 = 0.005;"), "the geometry gate's TOLERANCE");
}
