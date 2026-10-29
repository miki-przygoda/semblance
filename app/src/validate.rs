//! `semblance validate [--data DIR] [--threads N] [--markdown]`: does this
//! build reproduce flyvis?
//!
//! The computation is `semblance_flyvis::parity`, the same code the crate's
//! gates run; this module parses the flags and formats the report, as a
//! terminal table or as the record's generated block (`--markdown`).
//!
//! Exit codes: 0, every gate holds; 1, a gate failed; 2, verification did not
//! complete (a fixture missing, unreadable, malformed or mismatched, or a bad
//! command line). Spec: docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md.

use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use semblance_flyvis::parity::{
    self, Bound, Counts, Diff, Job, Options, ParityError, Report, RunResult, SequenceResult, CONTROL_MISS,
    COVERAGE_THRESHOLD, F64_BOUND,
};
use semblance_flyvis::pretrained::SYN_COUNT;
use semblance_flyvis::reference::Dtype;

const USAGE: &str = "usage: semblance validate [--data DIR] [--threads N] [--markdown]";

/// The number of main jobs: the default thread count's cap.
const MAX_DEFAULT_THREADS: usize = 16;

const BEGIN: &str = "<!-- BEGIN semblance validate --markdown -->";
const END: &str = "<!-- END semblance validate --markdown -->";

#[derive(Debug, PartialEq)]
struct Args {
    data: PathBuf,
    threads: Option<usize>,
    markdown: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let (mut data, mut threads, mut markdown) = (None, None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data" => {
                if data.is_some() {
                    return Err("--data given twice".into());
                }
                data = Some(PathBuf::from(it.next().ok_or("--data needs a directory")?));
            }
            "--threads" => {
                if threads.is_some() {
                    return Err("--threads given twice".into());
                }
                let v = it.next().ok_or("--threads needs a number")?;
                let n = v
                    .parse::<usize>()
                    .ok()
                    .filter(|_| v.bytes().all(|b| b.is_ascii_digit()))
                    .ok_or_else(|| format!("--threads {v}: not a whole number"))?;
                if n == 0 {
                    return Err("--threads must be at least 1".into());
                }
                threads = Some(n);
            }
            "--markdown" => {
                if markdown {
                    return Err("--markdown given twice".into());
                }
                markdown = true;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args { data: data.unwrap_or_else(|| PathBuf::from("./data")), threads, markdown })
}

fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_DEFAULT_THREADS)
}

/// The stderr message when fixture files are absent: nothing was verified.
fn missing_message(data: &Path, missing: &[&str]) -> String {
    let mut s = if missing.len() == 1 {
        "validate: nothing was verified; 1 fixture file is missing:\n".to_string()
    } else {
        format!("validate: nothing was verified; {} fixture files are missing:\n", missing.len())
    };
    for rel in missing {
        let _ = writeln!(s, "  {}", data.join(rel).display());
    }
    if let [rel] = missing {
        let _ = writeln!(s, "{}", parity::how_to_get(rel));
    }
    s.push_str("Get them with ci/fetch-fixtures.sh (needs gh), or see tools/README.md.\n");
    s.push_str(
        "The result recorded for 0.3.0 is docs/results/0.3.0-flyvis-parity.md; it is not a check of this machine.\n",
    );
    s
}

/// 2 if any job could not complete for a fixture reason, else 0 if every
/// gate and control holds for all eight runs, else 1.
fn exit_code(r: &Report) -> u8 {
    if r.jobs.iter().any(|j| matches!(&j.outcome, Err(e) if !e.is_gate_failure())) {
        2
    } else if r.passed() && r.complete() {
        0
    } else {
        1
    }
}

/// Writes `text` line by line. A closed pipe stops the output and keeps
/// `code`; any other write error is 2. Never panics.
fn emit<W: Write>(out: &mut W, text: &str, code: u8) -> u8 {
    for line in text.lines() {
        match writeln!(out, "{line}") {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => return code,
            Err(_) => return 2,
        }
    }
    match out.flush() {
        Ok(()) => code,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => code,
        Err(_) => 2,
    }
}

pub fn cmd(args: &[String]) -> ExitCode {
    let a = match parse(args) {
        Ok(a) => a,
        Err(m) => {
            eprintln!("validate: {m}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let missing: Vec<&str> =
        parity::required_files().iter().copied().filter(|rel| std::fs::metadata(a.data.join(rel)).is_err()).collect();
    if !missing.is_empty() {
        eprint!("{}", missing_message(&a.data, &missing));
        return ExitCode::from(2);
    }
    let start = Instant::now();
    let opts = Options { threads: a.threads.unwrap_or_else(default_threads), ..Options::default() };
    let report = match parity::validate(&a.data, &opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("validate: nothing was verified: {e}");
            return ExitCode::from(if e.is_gate_failure() { 1 } else { 2 });
        }
    };
    let code = exit_code(&report);
    let text =
        if a.markdown { markdown(&report) } else { terminal(&report, &a.data, start.elapsed().as_secs_f64(), code) };
    ExitCode::from(emit(&mut io::stdout().lock(), &text, code))
}

// ---------------------------------------------------------------------------
// What both formats need
// ---------------------------------------------------------------------------

fn precision(p: Dtype) -> &'static str {
    match p {
        Dtype::F32 => "f32",
        Dtype::F64 => "f64",
    }
}

fn counts(c: Counts) -> &'static str {
    match c {
        Counts::Torch => "exact",
        Counts::Libm => "libm",
        Counts::IntegerNSyn => "integer",
    }
}

fn version<'a>(r: &'a Report, name: &str) -> &'a str {
    r.provenance.versions.iter().find(|(k, _)| k == name).map_or("?", |(_, v)| v.as_str())
}

/// "000 (best)": the model and its rank in PROVENANCE.
fn model(r: &Report, id: &str) -> String {
    match r.provenance.models.iter().find(|m| m.id == id) {
        Some(m) => format!("{id} ({})", m.rank),
        None => id.to_string(),
    }
}

/// The target this binary was built for: arch-os, plus the environment if any.
fn target() -> String {
    let env = if cfg!(target_env = "gnu") {
        "-gnu"
    } else if cfg!(target_env = "musl") {
        "-musl"
    } else if cfg!(target_env = "msvc") {
        "-msvc"
    } else {
        ""
    };
    format!("{}-{}{env}", std::env::consts::ARCH, std::env::consts::OS)
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// The centre type `k`'s name: the cell type of the manifest's centre node.
fn centre_type(r: &Report, k: usize) -> &str {
    r.cell_type(r.centre_nodes[k] as usize)
}

/// A maximum: `0` when nothing differs, otherwise `{:.3e}`.
fn max_text(d: f64) -> String {
    if d == 0.0 { "0".to_string() } else { format!("{d:.3e}") }
}

fn ratio(max: f64, bound: &Bound) -> f64 {
    max / bound.value()
}

/// One stage of one job, summarised: what the terminal row prints.
struct Row {
    compared: usize,
    unequal: usize,
    max: f64,
    /// For a libm row, max / bound; NaN for an exact row.
    ratio: f64,
    bound: f64,
    place: String,
    holds: bool,
}

/// Picks the worse of several (max, bound, where) candidates: by ratio for a
/// libm job, by maximum for an exact one. The last wins a tie, as the gates'
/// `max_by` does, so the terminal names what the gates print.
fn worst(job: &Job, candidates: Vec<(Diff, Bound, String)>) -> (f64, f64, f64, String) {
    let exact = job.counts == Counts::Torch;
    let mut best: Option<(f64, f64, f64, String)> = None;
    for (d, b, place) in candidates {
        let key = if exact { d.max } else { ratio(d.max, &b) };
        if best.as_ref().is_none_or(|(_, _, k, _)| key.total_cmp(k).is_ge()) {
            best = Some((d.max, b.value(), key, place));
        }
    }
    best.unwrap_or((0.0, 0.0, 0.0, "-".into()))
}

fn settling_row(r: &Report, job: &Job, o: &RunResult) -> Row {
    let s = &o.settling;
    let n_centre = r.centre_nodes.len();
    let candidates = vec![
        (s.settled, job.settling_bounds.0, format!("settled, node {} ({})", s.settled.at, r.cell_type(s.settled.at))),
        (
            s.settling_centre,
            job.settling_bounds.1,
            format!("centre, step {} ({})", s.settling_centre.at / n_centre, centre_type(r, s.settling_centre.at % n_centre)),
        ),
    ];
    let (max, bound, key, place) = worst(job, candidates);
    let holds = match job.counts {
        Counts::Torch => s.settled.differ + s.settling_centre.differ == 0,
        _ => s.settled.max <= job.settling_bounds.0.value() && s.settling_centre.max <= job.settling_bounds.1.value(),
    };
    Row {
        compared: s.settled_len + s.centre_len,
        unequal: s.settled.differ + s.settling_centre.differ,
        max,
        ratio: if job.counts == Counts::Torch { f64::NAN } else { key },
        bound,
        place,
        holds,
    }
}

fn centre_place(r: &Report, q: &SequenceResult, long: bool) -> String {
    let n_centre = r.centre_nodes.len();
    let k = q.centre.at % n_centre;
    if long {
        format!("seq {}, frame {}, node {} ({})", q.sequence, q.centre.at / n_centre, r.centre_nodes[k], centre_type(r, k))
    } else {
        format!("centre, seq {} frame {} ({})", q.sequence, q.centre.at / n_centre, centre_type(r, k))
    }
}

fn snapshot_place(r: &Report, q: &SequenceResult, long: bool) -> String {
    let n = r.node_cell_type.len();
    let node = q.snapshots.at % n;
    let frame = q.snapshot_frames.get(q.snapshots.at / n).copied().unwrap_or(0);
    if long {
        format!("seq {}, frame {frame}, node {node} ({})", q.sequence, r.cell_type(node))
    } else {
        format!("snaps, seq {} frame {frame} ({})", q.sequence, r.cell_type(node))
    }
}

fn trace_row(r: &Report, job: &Job, sequences: &[&SequenceResult]) -> Row {
    let n = r.node_cell_type.len();
    let n_centre = r.centre_nodes.len();
    let mut candidates = Vec::new();
    for q in sequences {
        candidates.push((q.centre, job.trace_bounds.0, centre_place(r, q, false)));
        candidates.push((q.snapshots, job.trace_bounds.1, snapshot_place(r, q, false)));
    }
    let (max, bound, key, place) = worst(job, candidates);
    let holds = sequences.iter().all(|q| match job.counts {
        Counts::Torch => q.centre.differ + q.snapshots.differ == 0,
        _ => q.centre.max <= job.trace_bounds.0.value() && q.snapshots.max <= job.trace_bounds.1.value(),
    });
    Row {
        compared: sequences.iter().map(|q| q.n_frames * n_centre + q.snapshot_frames.len() * n).sum(),
        unequal: sequences.iter().map(|q| q.centre.differ + q.snapshots.differ).sum(),
        max,
        ratio: if job.counts == Counts::Torch { f64::NAN } else { key },
        bound,
        place,
        holds,
    }
}

/// The sequences of each stimulus family, in order of first appearance.
fn families(o: &RunResult) -> Vec<(String, Vec<&SequenceResult>)> {
    let mut out: Vec<(String, Vec<&SequenceResult>)> = Vec::new();
    for q in &o.sequences {
        match out.iter_mut().find(|(f, _)| *f == q.family) {
            Some((_, v)) => v.push(q),
            None => out.push((q.family.clone(), vec![q])),
        }
    }
    out
}

fn family_name(f: &str) -> &str {
    match f {
        "moving_edge" => "moving edges",
        other => other,
    }
}

/// "0–3", or "0, 2, 5" if the indices are not consecutive.
fn sequence_range(v: &[&SequenceResult]) -> String {
    let ids: Vec<u32> = v.iter().map(|q| q.sequence).collect();
    match (ids.first(), ids.last()) {
        (Some(a), Some(b)) if ids.windows(2).all(|w| w[1] == w[0] + 1) && ids.len() > 1 => format!("{a}–{b}"),
        _ => ids.iter().map(u32::to_string).collect::<Vec<_>>().join(", "),
    }
}

/// One run's coverage, from its exact job: (weakest type, its movement),
/// the next two, the median, and the run's libm centre bound.
struct RunCoverage {
    run: u32,
    order: Vec<usize>,
    movement: Vec<f64>,
    bound: Option<f64>,
}

impl RunCoverage {
    fn weakest(&self) -> f64 {
        self.movement[self.order[0]]
    }
    fn median(&self) -> f64 {
        self.movement[self.order[self.order.len() / 2]]
    }
}

/// Per run: `Some` if measured. Ordered by `total_cmp`; a tie goes to the
/// lower cell-type index.
fn coverage(r: &Report) -> Vec<(u32, Option<RunCoverage>)> {
    r.coverage
        .iter()
        .map(|c| {
            let measured = c.movement.as_ref().filter(|m| !m.is_empty()).map(|m| {
                let mut order: Vec<usize> = (0..m.len()).collect();
                order.sort_by(|&a, &b| m[a].total_cmp(&m[b]).then(a.cmp(&b)));
                let bound =
                    r.jobs.iter().find(|j| j.run == c.run && j.counts == Counts::Libm).map(|j| j.trace_bounds.0.value());
                RunCoverage { run: c.run, order, movement: m.clone(), bound }
            });
            (c.run, measured)
        })
        .collect()
}

/// The weakest movement over every measured run: (run, type, movement). A
/// tie goes to the lowest run, then the lowest cell-type index.
fn overall_weakest(cov: &[(u32, Option<RunCoverage>)]) -> Option<(u32, usize, f64)> {
    let mut best: Option<(u32, usize, f64)> = None;
    for c in cov.iter().filter_map(|(_, c)| c.as_ref()) {
        let k = c.order[0];
        let m = c.movement[k];
        if best.is_none_or(|(_, _, b)| m.total_cmp(&b).is_lt()) {
            best = Some((c.run, k, m));
        }
    }
    best
}

/// Every (run, type) that is not driven: movement not above the threshold.
fn undriven(r: &Report, cov: &[(u32, Option<RunCoverage>)]) -> Vec<String> {
    let mut out = Vec::new();
    for c in cov.iter().filter_map(|(_, c)| c.as_ref()) {
        for (k, &m) in c.movement.iter().enumerate() {
            if m.partial_cmp(&COVERAGE_THRESHOLD) != Some(std::cmp::Ordering::Greater) {
                out.push(format!("{} in run {} ({m:.3e})", centre_type(r, k), c.run));
            }
        }
    }
    out
}

struct ControlRow {
    terminal_name: &'static str,
    record_name: &'static str,
    terminal: String,
    record: String,
    must: String,
    holds: bool,
}

fn controls(r: &Report) -> [ControlRow; 4] {
    let c = &r.controls;
    let settled = |d: &Result<Diff, ParityError>, above: f64| match d {
        Ok(d) => (format!("settled max {:.3e}", d.max), d.max > above),
        Err(e) => (e.to_string(), false),
    };
    let (wrong, wrong_holds) = settled(&c.wrong_model, CONTROL_MISS);
    let (integer, integer_holds) = settled(&c.integer_counts, F64_BOUND);
    let n = r.node_cell_type.len();
    let n_centre = r.centre_nodes.len();
    let (mirrored_t, mirrored_r, mirrored_holds) = match &c.mirrored {
        Ok(s) => (
            format!("snapshots max {:e}", s.snapshots.max),
            format!("snapshots max {:e}, centre max {:e}", s.snapshots.max, s.centre.max),
            s.snapshots.max > CONTROL_MISS,
        ),
        Err(e) => (e.to_string(), e.to_string(), false),
    };
    let (flyvis_t, flyvis_r, flyvis_holds) = match &c.from_flyvis {
        Ok(s) => {
            let unequal = s.centre.differ + s.snapshots.differ;
            let compared = s.n_frames * n_centre + s.snapshot_frames.len() * n;
            (format!("{unequal} unequal"), format!("{unequal} of {compared} unequal"), unequal == 0 && s.n_frames > 0)
        }
        Err(e) => (e.to_string(), e.to_string(), false),
    };
    [
        ControlRow {
            terminal_name: "wrong model (049 vs run 3)",
            record_name: "model 049's parameters against model 000's reference",
            record: wrong.clone(),
            terminal: wrong,
            must: format!("> {CONTROL_MISS:e}"),
            holds: wrong_holds,
        },
        ControlRow {
            terminal_name: "integer counts (run 3)",
            record_name: "the spec's integer counts in place of exp(ln count)",
            record: integer.clone(),
            terminal: integer,
            must: format!("> {F64_BOUND:e}"),
            holds: integer_holds,
        },
        ControlRow {
            terminal_name: "mirrored mapping (run 3, seq 8)",
            record_name: "hexals mirrored (u, v) → (−u, −v), sequence 8",
            terminal: mirrored_t,
            record: mirrored_r,
            must: format!("> {CONTROL_MISS:e}"),
            holds: mirrored_holds,
        },
        ControlRow {
            terminal_name: "flyvis's settled start (run 3, seq 8)",
            record_name: "start from flyvis's settled.f64.bin, sequence 8",
            terminal: flyvis_t,
            record: flyvis_r,
            must: "==".into(),
            holds: flyvis_holds,
        },
    ]
}

// ---------------------------------------------------------------------------
// The terminal table
// ---------------------------------------------------------------------------

fn terminal(r: &Report, data: &Path, seconds: f64, code: u8) -> String {
    let p = &r.provenance;
    let mut s = String::new();
    let _ = writeln!(s, "semblance validate: does this build reproduce flyvis? (epic 0.3.0)");
    let _ = writeln!(s, "data:        {}", data.display());
    let _ = writeln!(
        s,
        "reference:   export {} ({}); flyvis {}, torch {}; {}, {}",
        p.export_id,
        p.exported,
        version(r, "flyvis"),
        version(r, "torch"),
        p.platform,
        p.device
    );
    let models: Vec<String> = p
        .models
        .iter()
        .map(|m| format!("{} {} ({}, validation loss {:.3})", m.id, m.name, m.rank, m.validation_loss))
        .collect();
    let _ = writeln!(s, "models:      {}", models.join(", "));
    let _ = writeln!(
        s,
        "this build:  semblance {}, {}, debug assertions {}, {} threads",
        env!("CARGO_PKG_VERSION"),
        target(),
        on_off(cfg!(debug_assertions)),
        r.threads
    );
    let _ = writeln!(
        s,
        "libm exp:    f32 differs from torch's on {} of {} counts, f64 on {} (fingerprint {})",
        r.libm.f32_differ,
        SYN_COUNT.len(),
        r.libm.f64_differ,
        r.libm
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "{:<10}{:<5}{:<7}{:<7}{:<6}{:<6}{:>10}{:>9}{:>11}  {:<31}{:<11}ratio",
        "stage", "run", "model", "dt", "prec", "counts", "compared", "unequal", "max err", "where", "bound"
    );
    let mut failures = Vec::new();
    let mut worst: Option<(f64, &str, u32)> = None;
    let (mut exact_rows, mut libm_rows) = (0, 0);
    for stage in ["settling", "traces"] {
        for job in &r.jobs {
            let lead = format!(
                "{:<10}{:<5}{:<7}{:<7}{:<6}{:<6}",
                stage,
                job.run,
                job.model,
                job.dt,
                precision(job.precision),
                counts(job.counts)
            );
            let row = match &job.outcome {
                Ok(o) if stage == "settling" => settling_row(r, job, o),
                Ok(o) => trace_row(r, job, &o.sequences.iter().collect::<Vec<_>>()),
                Err(e) => {
                    let _ = writeln!(s, "{lead}  {e}  FAIL");
                    failures.push(format!("{stage} run {} {}", job.run, counts(job.counts)));
                    continue;
                }
            };
            let exact = job.counts == Counts::Torch;
            let (max, place, bound, ratio) = if exact && row.holds {
                ("0".to_string(), "-".to_string(), "==".to_string(), "-".to_string())
            } else if exact {
                (format!("{:.3e}", row.max), row.place.clone(), "==".to_string(), "-".to_string())
            } else {
                (format!("{:.3e}", row.max), row.place.clone(), format!("{:.3e}", row.bound), format!("{:.3e}", row.ratio))
            };
            let fail = if row.holds { "" } else { "  FAIL" };
            let _ = writeln!(
                s,
                "{lead}{:>10}{:>9}{:>11}  {:<31}{:<11}{ratio}{fail}",
                row.compared, row.unequal, max, place, bound
            );
            if exact {
                exact_rows += 1;
            } else {
                libm_rows += 1;
                if worst.is_none_or(|(w, _, _)| row.ratio > w) {
                    worst = Some((row.ratio, stage, job.run));
                }
            }
            if !row.holds {
                failures.push(format!("{stage} run {} {}", job.run, counts(job.counts)));
            }
        }
    }

    let _ = writeln!(s);
    for (i, c) in controls(r).iter().enumerate() {
        let label = if i == 0 { "controls:" } else { "" };
        let must = format!("must be {}", c.must);
        let holds = if c.holds { "holds" } else { "FAIL" };
        let _ = writeln!(s, "{label:<11}{:<39}{:<34}{:<17}{holds}", c.terminal_name, c.terminal, must);
        if !c.holds {
            failures.push(format!("control {}", c.terminal_name));
        }
    }

    let cov = coverage(r);
    let mut lines = Vec::new();
    for (run, c) in &cov {
        if c.is_none() {
            lines.push(format!("run {run} not measured: exact job failed"));
        }
    }
    if let Some((run, k, m)) = overall_weakest(&cov) {
        lines.push(format!(
            "weakest cell type moves {m:.3e} ({}, run {run}); threshold {COVERAGE_THRESHOLD:e} (driven if above), recorded, not a gate:",
            centre_type(r, k)
        ));
        let undriven = undriven(r, &cov);
        if undriven.is_empty() && cov.iter().all(|(_, c)| c.is_some()) {
            lines.push("every type is driven in every run, so Sintel's revisit trigger 1 does not fire;".into());
        } else if undriven.is_empty() {
            lines.push("every type is driven in every run measured;".into());
        } else {
            lines.push(format!("not driven: {}, so Sintel's revisit trigger 1 fires;", undriven.join(", ")));
        }
        let ratios: Vec<String> = cov
            .iter()
            .map(|(_, c)| match c.as_ref().and_then(|c| c.bound.map(|b| c.weakest() / b)) {
                Some(x) => format!("{x:.3e}"),
                None => "-".into(),
            })
            .collect();
        let runs = if r.complete() {
            "0-7".to_string()
        } else {
            r.runs.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
        };
        lines.push(format!("weakest / libm centre bound, runs {runs}: {}", ratios.join(" ")));
    }
    for (i, l) in lines.iter().enumerate() {
        let label = if i == 0 { "coverage:" } else { "" };
        let _ = writeln!(s, "{label:<11}{l}");
    }

    let verdict = match code {
        0 => {
            let (w, stage, run) = worst.unwrap_or((0.0, "-", 0));
            format!(
                "PASS — {exact_rows} exact rows bit-equal, {libm_rows} libm rows within bounds (worst {w:.3e}, {stage} run {run}), 4 controls hold"
            )
        }
        1 => format!("FAIL — {}", if failures.is_empty() { "not every run was checked".into() } else { failures.join(", ") }),
        _ => format!("NOT VERIFIED — a fixture did not fit; nothing counts as verified: {}", failures.join(", ")),
    };
    let _ = writeln!(s, "verdict:   {verdict}  [{seconds:.1} s]");
    s
}

// ---------------------------------------------------------------------------
// The record's generated block
// ---------------------------------------------------------------------------

fn markdown(r: &Report) -> String {
    let p = &r.provenance;
    let mut s = String::new();
    let _ = writeln!(s, "{BEGIN}");
    let _ = writeln!(s, "## Reference (flyvis side)");
    let _ = writeln!(s, "| | |\n|---|---|");
    let _ = writeln!(s, "| export | {}, exported {} |", p.export_id, p.exported);
    let _ = writeln!(
        s,
        "| exporter | {}, sha256 {}; git {} ({}); uv.lock sha256 {} |",
        p.generator_path,
        p.generator_sha256,
        p.git_head,
        if p.git_dirty { "dirty" } else { "clean" },
        p.uv_lock_sha256
    );
    let versions: Vec<String> = p.versions.iter().map(|(k, v)| format!("{k} {v}")).collect();
    let _ = writeln!(s, "| versions | {} |", versions.join(", "));
    let join = |v: &[u32]| v.iter().map(u32::to_string).collect::<Vec<_>>().join(" and ");
    let _ = writeln!(
        s,
        "| machine | {} ({}), device {}; default {} threads, runs {} bit-equal at {} |",
        p.platform,
        p.machine,
        p.device,
        p.threads_default,
        join(&p.threads_runs_checked),
        join(&p.threads_checked)
    );
    let models: Vec<String> = p
        .models
        .iter()
        .map(|m| {
            format!(
                "{} {}, {}, validation loss {}, checkpoint sha256 {}",
                m.id, m.name, m.rank, m.validation_loss, m.chkpt_00000_sha256
            )
        })
        .collect();
    let _ = writeln!(s, "| models | {} |", models.join("; "));
    let _ = writeln!(s, "| parameters | all 50 checkpoints equal ensemble.csv |");
    let seqs: Vec<String> = p.equivalence_sequences.iter().map(u32::to_string).collect();
    let _ = writeln!(
        s,
        "| equivalence | the export's forward path equals stimulus_response bit for bit (sequences {}) |",
        seqs.join(", ")
    );
    let rounding = match p.input_rounding.as_slice() {
        [first, rest @ ..] if rest.iter().all(|x| x.1.to_bits() == first.1.to_bits()) => {
            let ids: Vec<&str> = p.input_rounding.iter().map(|x| x.0.as_str()).collect();
            format!("{:e} at {}", first.1, ids.join(" and "))
        }
        all => all.iter().map(|(dt, v)| format!("{v:e} at {dt}")).collect::<Vec<_>>().join("; "),
    };
    let _ = writeln!(s, "| input rounding | {rounding} (the f32 runs' moving edges) |");
    let _ = writeln!(
        s,
        "| files | {} reference files, each the size the manifest and PROVENANCE give |",
        p.sizes.len()
    );
    let _ = writeln!(s);

    let _ = writeln!(s, "<!-- BEGIN host-specific -->");
    let _ = writeln!(s, "## This run (Rust side)");
    let _ = writeln!(s, "| | |\n|---|---|");
    let _ = writeln!(s, "| semblance | {} |", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(s, "| target | {} |", target());
    let _ = writeln!(s, "| debug assertions | {} |", on_off(cfg!(debug_assertions)));
    let _ = writeln!(s, "| threads | {} |", r.threads);
    let _ = writeln!(
        s,
        "| libm exp | f32: {} of {} counts differ from torch's; f64: {} |",
        r.libm.f32_differ,
        SYN_COUNT.len(),
        r.libm.f64_differ
    );
    let _ = writeln!(s, "| libm fingerprint | {} |", r.libm);
    let _ = writeln!(s, "<!-- END host-specific -->");
    let _ = writeln!(s);

    let _ = writeln!(s, "## Bounds as applied");
    let _ = writeln!(s, "| run | quantity | bound | derivation |\n|---|---|---|---|");
    let _ = writeln!(
        s,
        "| all | every quantity, exact | == | torch's counts (syn_count_torch.rs); every value must equal flyvis's |"
    );
    for job in r.jobs.iter().filter(|j| j.counts == Counts::Libm) {
        let bounds = [job.settling_bounds.0, job.settling_bounds.1, job.trace_bounds.0, job.trace_bounds.1];
        if bounds.iter().all(|b| *b == Bound::F64) {
            let _ = writeln!(s, "| {} | every quantity, libm | {:e} | the f64 bound |", job.run, F64_BOUND);
            continue;
        }
        for b in bounds {
            match b {
                Bound::F32 { gap, model, dt, quantity } => {
                    let _ = writeln!(
                        s,
                        "| {} | {}, libm | {:e} | 2 × f32_f64_gap[\"{model} {dt}\"].{quantity} = 2 × {gap:e} |",
                        job.run,
                        quantity.replace('_', " "),
                        b.value()
                    );
                }
                other => {
                    let _ = writeln!(s, "| {} | libm | {:e} | {other:?} |", job.run, other.value());
                }
            }
        }
    }
    let _ = writeln!(s);

    let exact: Vec<&Job> = r.jobs.iter().filter(|j| j.counts == Counts::Torch).collect();
    let libm: Vec<&Job> = r.jobs.iter().filter(|j| j.counts == Counts::Libm).collect();
    let lead = |j: &Job| format!("| {} | {} | {} | {} |", j.run, model(r, &j.model), j.dt, precision(j.precision));

    let _ = writeln!(s, "## Stage 1: settling, exact (torch's counts)");
    let _ = writeln!(
        s,
        "| run | model | dt | precision | quantity | compared | unequal | max error |\n|---|---|---|---|---|---|---|---|"
    );
    for j in &exact {
        match &j.outcome {
            Ok(o) => {
                let st = &o.settling;
                for (q, d, n) in [("settled", st.settled, st.settled_len), ("settling centre", st.settling_centre, st.centre_len)] {
                    let _ = writeln!(s, "{} {q} | {n} | {} | {} |", lead(j), d.differ, max_text(d.max));
                }
            }
            Err(e) => {
                let _ = writeln!(s, "{} not run: {e} | | | |", lead(j));
            }
        }
    }
    let _ = writeln!(s);

    let n = r.node_cell_type.len();
    let n_centre = r.centre_nodes.len();
    let _ = writeln!(s, "## Stage 2: traces, exact (torch's counts), per stimulus");
    let _ = writeln!(
        s,
        "| run | model | dt | precision | stimulus | sequences | quantity | compared | unequal | max error |\n\
         |---|---|---|---|---|---|---|---|---|---|"
    );
    for j in &exact {
        match &j.outcome {
            Ok(o) => {
                for (family, qs) in families(o) {
                    let range = sequence_range(&qs);
                    for (q, compared, d) in group(&qs, n, n_centre) {
                        let _ = writeln!(
                            s,
                            "{} {} | {range} | {q} | {compared} | {} | {} |",
                            lead(j),
                            family_name(&family),
                            d.differ,
                            max_text(d.max)
                        );
                    }
                }
            }
            Err(e) => {
                let _ = writeln!(s, "{} not run: {e} | | | | | |", lead(j));
            }
        }
    }
    let _ = writeln!(s);

    let _ = writeln!(s, "<!-- BEGIN libm fingerprint={} -->", r.libm);
    let _ = writeln!(s, "## Stage 1: settling, libm (Rust's exp)");
    let _ = writeln!(
        s,
        "| run | model | dt | precision | quantity | compared | unequal | max error | where | bound | ratio | worst types |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for j in &libm {
        match &j.outcome {
            Ok(o) => {
                let st = &o.settling;
                let worst: Vec<String> = st
                    .worst_types(3)
                    .iter()
                    .map(|&t| format!("{} {:.2e}", r.cell_types[t], st.per_type_max[t]))
                    .collect();
                let (sb, cb) = j.settling_bounds;
                let _ = writeln!(
                    s,
                    "{} settled | {} | {} | {:.3e} | node {} ({}) | {:.3e} | {:.3e} | {} |",
                    lead(j),
                    st.settled_len,
                    st.settled.differ,
                    st.settled.max,
                    st.settled.at,
                    r.cell_type(st.settled.at),
                    sb.value(),
                    ratio(st.settled.max, &sb),
                    worst.join(", ")
                );
                let c = st.settling_centre;
                let _ = writeln!(
                    s,
                    "{} settling centre | {} | {} | {:.3e} | step {} ({}) | {:.3e} | {:.3e} | |",
                    lead(j),
                    st.centre_len,
                    c.differ,
                    c.max,
                    c.at / n_centre,
                    centre_type(r, c.at % n_centre),
                    cb.value(),
                    ratio(c.max, &cb)
                );
            }
            Err(e) => {
                let _ = writeln!(s, "{} not run: {e} | | | | | | | |", lead(j));
            }
        }
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "## Stage 2: traces, libm (Rust's exp), per stimulus");
    let _ = writeln!(
        s,
        "| run | model | dt | precision | stimulus | quantity | compared | unequal | max error | where | bound | ratio |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for j in &libm {
        match &j.outcome {
            Ok(o) => {
                for (family, qs) in families(o) {
                    for (quantity, bound) in [("centre", j.trace_bounds.0), ("snapshots", j.trace_bounds.1)] {
                        let pick = |q: &SequenceResult| if quantity == "centre" { q.centre } else { q.snapshots };
                        let compared: usize = qs
                            .iter()
                            .map(|q| if quantity == "centre" { q.n_frames * n_centre } else { q.snapshot_frames.len() * n })
                            .sum();
                        let unequal: usize = qs.iter().map(|q| pick(q).differ).sum();
                        let mut at = qs[0];
                        for q in &qs {
                            if pick(q).max.total_cmp(&pick(at).max).is_ge() {
                                at = q;
                            }
                        }
                        let max = pick(at).max;
                        let place =
                            if quantity == "centre" { centre_place(r, at, true) } else { snapshot_place(r, at, true) };
                        let _ = writeln!(
                            s,
                            "{} {} | {quantity} | {compared} | {unequal} | {max:.3e} | {place} | {:.3e} | {:.3e} |",
                            lead(j),
                            family_name(&family),
                            bound.value(),
                            ratio(max, &bound)
                        );
                    }
                }
            }
            Err(e) => {
                let _ = writeln!(s, "{} not run: {e} | | | | | | | |", lead(j));
            }
        }
    }
    let _ = writeln!(s, "<!-- END libm -->");
    let _ = writeln!(s);

    let _ = writeln!(s, "## Controls");
    let _ = writeln!(s, "| control | run | measured | must be | holds |\n|---|---|---|---|---|");
    for c in controls(r) {
        let _ = writeln!(s, "| {} | 3 | {} | {} | {} |", c.record_name, c.record, c.must, if c.holds { "yes" } else { "no" });
    }
    let _ = writeln!(s);

    let cov = coverage(r);
    let _ = writeln!(s, "## Coverage: Sintel revisit trigger 1");
    let _ = writeln!(
        s,
        "Movement of a cell type = max over the 12 sequences and every frame of |centre trace − settled centre|, \
         widened to f64, from the exact jobs (equal to flyvis's where the exact job passed). A type is driven if its \
         movement exceeds {COVERAGE_THRESHOLD:e}: recorded, not a gate."
    );
    let _ = writeln!(
        s,
        "| run | weakest type | movement | next two | median | libm centre bound | weakest / bound |\n\
         |---|---|---|---|---|---|---|"
    );
    for (run, c) in &cov {
        match c {
            Some(c) => {
                let k = c.order[0];
                let next: Vec<String> =
                    c.order[1..3.min(c.order.len())].iter().map(|&t| format!("{} {:.3e}", centre_type(r, t), c.movement[t])).collect();
                let (bound, ratio) = match c.bound {
                    Some(b) => (format!("{b:.3e}"), format!("{:.3e}", c.weakest() / b)),
                    None => ("-".into(), "-".into()),
                };
                let _ = writeln!(
                    s,
                    "| {run} | {} | {:.3e} | {} | {:.3e} | {bound} | {ratio} |",
                    centre_type(r, k),
                    c.weakest(),
                    next.join(", "),
                    c.median()
                );
            }
            None => {
                let _ = writeln!(s, "| {run} | not measured: exact job failed | | | | | |");
            }
        }
    }
    let _ = writeln!(s);
    let unmeasured: Vec<String> = cov.iter().filter(|(_, c)| c.is_none()).map(|(r, _)| r.to_string()).collect();
    let undriven = undriven(r, &cov);
    let sentence = match overall_weakest(&cov) {
        _ if !unmeasured.is_empty() => format!(
            "Runs {} were not measured, because their exact jobs failed, so this does not show that every type is \
             driven. This covers the centre column only.",
            unmeasured.join(", ")
        ),
        Some((run, k, m)) if undriven.is_empty() => format!(
            "Every one of the {} cell types moves by more than {COVERAGE_THRESHOLD:e} in every run (the weakest, {} in \
             run {run}, by {m:.3e}): no type is undriven by flashes and moving edges, and trigger 1 does not fire. \
             This covers the centre column only.",
            r.centre_nodes.len(),
            centre_type(r, k)
        ),
        Some(_) => format!(
            "Not driven (moving by no more than {COVERAGE_THRESHOLD:e}): {}. Trigger 1 fires. This covers the centre \
             column only.",
            undriven.join(", ")
        ),
        None => "No run was measured. This covers the centre column only.".to_string(),
    };
    let _ = writeln!(s, "{sentence}");
    let _ = writeln!(s, "{END}");
    s
}

/// A family's centre and snapshot totals: (quantity, compared, combined Diff).
fn group(qs: &[&SequenceResult], n: usize, n_centre: usize) -> [(&'static str, usize, Diff); 2] {
    let fold = |pick: fn(&SequenceResult) -> Diff| {
        qs.iter().fold(Diff { max: 0.0, at: 0, differ: 0 }, |acc, q| {
            let d = pick(q);
            Diff { max: if d.max > acc.max { d.max } else { acc.max }, at: 0, differ: acc.differ + d.differ }
        })
    };
    [
        ("centre", qs.iter().map(|q| q.n_frames * n_centre).sum(), fold(|q| q.centre)),
        ("snapshots", qs.iter().map(|q| q.snapshot_frames.len() * n).sum(), fold(|q| q.snapshots)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flags_parse_once_each() {
        assert_eq!(parse(&args(&[])), Ok(Args { data: PathBuf::from("./data"), threads: None, markdown: false }));
        assert_eq!(
            parse(&args(&["--markdown", "--threads", "4", "--data", "d"])),
            Ok(Args { data: PathBuf::from("d"), threads: Some(4), markdown: true })
        );
        for bad in [
            &["--threads", "0"][..],
            &["--threads", "x"],
            &["--threads", "+3"],
            &["--threads"],
            &["--data"],
            &["--data", "a", "--data", "b"],
            &["--markdown", "--markdown"],
            &["--bogus"],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
    }

    struct Pipe {
        writes: usize,
        kind: io::ErrorKind,
    }

    impl Write for Pipe {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.writes == 0 {
                return Err(io::Error::new(self.kind, "closed"));
            }
            self.writes -= 1;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// `validate | head`: a closed pipe stops the output and keeps the code.
    #[test]
    fn a_broken_pipe_keeps_the_exit_code_and_does_not_panic() {
        let text = "a\nb\nc\n";
        for code in [0, 1] {
            assert_eq!(emit(&mut Pipe { writes: 1, kind: io::ErrorKind::BrokenPipe }, text, code), code);
        }
        assert_eq!(emit(&mut Pipe { writes: 1, kind: io::ErrorKind::Other }, text, 0), 2);
        assert_eq!(emit(&mut Pipe { writes: 100, kind: io::ErrorKind::Other }, text, 1), 1);
    }

    #[test]
    fn the_missing_message_lists_every_absent_file() {
        let m = missing_message(Path::new("./data"), &["flyvis-pretrained/ensemble.csv", "flyvis-reference/manifest.json"]);
        assert_eq!(
            m,
            "validate: nothing was verified; 2 fixture files are missing:\n  \
             ./data/flyvis-pretrained/ensemble.csv\n  ./data/flyvis-reference/manifest.json\n\
             Get them with ci/fetch-fixtures.sh (needs gh), or see tools/README.md.\n\
             The result recorded for 0.3.0 is docs/results/0.3.0-flyvis-parity.md; it is not a check of this machine.\n"
        );
        let one = missing_message(Path::new("d"), &["flyvis-pretrained/ensemble.csv"]);
        assert!(one.starts_with("validate: nothing was verified; 1 fixture file is missing:\n  d/flyvis-pretrained/ensemble.csv\n"));
        assert!(one.contains(parity::how_to_get("flyvis-pretrained/ensemble.csv")));
    }
}
