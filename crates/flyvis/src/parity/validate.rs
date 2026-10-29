//! `validate`: every gate and control, on a pool of threads, into one
//! report. What `semblance validate` prints is formatted from this in `app`.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::bounds::{settling_bounds, trace_bounds, Bound};
use super::checks::check_fixtures;
use super::fingerprint::{libm_fingerprint, LibmFingerprint};
use super::inputs::{read_text, required_files, PROVENANCE};
use super::run::{play, run, settle, Counts, Diff, RunResult, SequenceResult, Start};
use super::{Fixtures, ParityError, TraceData, CONTROL_MISS, F64_BOUND, RUNS};
use crate::reference::{Dtype, Provenance};

/// What to run. `runs` exists for tests; the command always runs all eight.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Worker threads, at least 1. More than there are jobs is capped.
    pub threads: usize,
    /// Non-empty, distinct, each below 8.
    pub runs: Vec<u32>,
}

impl Default for Options {
    fn default() -> Self {
        Options { threads: 1, runs: (0..RUNS.len() as u32).collect() }
    }
}

impl Options {
    fn check(&self) -> Result<(), ParityError> {
        if self.threads == 0 {
            return Err(ParityError::Usage("threads must be at least 1".into()));
        }
        if self.runs.is_empty() {
            return Err(ParityError::Usage("no runs to validate".into()));
        }
        for (i, &r) in self.runs.iter().enumerate() {
            if r as usize >= RUNS.len() {
                return Err(ParityError::Usage(format!("run {r} is not one of the {} registered runs", RUNS.len())));
            }
            if self.runs[..i].contains(&r) {
                return Err(ParityError::Usage(format!("run {r} is listed twice")));
            }
        }
        Ok(())
    }
}

/// One run with one source of synapse counts: settling, then all twelve
/// sequences from the settled state.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub run: u32,
    pub model: String,
    pub dt: String,
    pub precision: Dtype,
    pub counts: Counts,
    /// (settled, settling_centre)
    pub settling_bounds: (Bound, Bound),
    /// (centre, snapshots)
    pub trace_bounds: (Bound, Bound),
    pub outcome: Result<RunResult, ParityError>,
}

impl Job {
    /// Fails closed (spec R18): only a complete result for this job, with
    /// every exact value equal or every libm maximum within its bound.
    pub fn passed(&self) -> bool {
        let Ok(r) = &self.outcome else { return false };
        let s = &r.settling;
        if s.run != self.run || s.counts != self.counts || s.settled_len == 0 || s.centre_len == 0 {
            return false;
        }
        let complete = r.sequences.len() == 12
            && r.sequences.iter().enumerate().all(|(i, q)| q.sequence as usize == i && q.n_frames > 0 && q.snapshot_frames.len() == 6);
        if !complete {
            return false;
        }
        match self.counts {
            Counts::Torch => {
                s.settled.differ == 0
                    && s.settling_centre.differ == 0
                    && r.sequences.iter().all(|q| q.centre.differ == 0 && q.snapshots.differ == 0)
            }
            Counts::Libm => {
                let (bs, bc) = (self.settling_bounds.0.value(), self.settling_bounds.1.value());
                let (tc, ts) = (self.trace_bounds.0.value(), self.trace_bounds.1.value());
                s.settled.max <= bs
                    && s.settling_centre.max <= bc
                    && r.sequences.iter().all(|q| q.centre.max <= tc && q.snapshots.max <= ts)
            }
            Counts::IntegerNSyn => false,
        }
    }
}

/// The four controls, each on run 3 (model 000, dt 1/50, f64).
#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    /// Settled with model 049's parameters, torch's counts.
    pub wrong_model: Result<Diff, ParityError>,
    /// Settled with the spec's integer counts.
    pub integer_counts: Result<Diff, ParityError>,
    /// Sequence 8 with every frame's hexals mirrored.
    pub mirrored: Result<SequenceResult, ParityError>,
    /// Sequence 8 from flyvis's own settled state.
    pub from_flyvis: Result<SequenceResult, ParityError>,
}

impl Controls {
    /// Every control returned and holds (spec R14).
    pub fn passed(&self) -> bool {
        matches!(&self.wrong_model, Ok(d) if d.max > CONTROL_MISS)
            && matches!(&self.integer_counts, Ok(d) if d.max > F64_BOUND)
            && matches!(&self.mirrored, Ok(s) if s.snapshots.max > CONTROL_MISS)
            && matches!(&self.from_flyvis, Ok(s) if s.n_frames > 0 && s.centre.differ == 0 && s.snapshots.differ == 0)
    }
}

/// Sintel revisit trigger 1, for one run: each centre type's movement, from
/// the run's exact job. `None` if that job did not pass.
#[derive(Clone, Debug, PartialEq)]
pub struct Coverage {
    pub run: u32,
    pub movement: Option<Vec<f64>>,
}

/// Everything `validate` found.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub provenance: Provenance,
    /// As requested.
    pub runs: Vec<u32>,
    /// The network's cell types, in its order.
    pub cell_types: Vec<String>,
    /// Per node, an index into `cell_types`.
    pub node_cell_type: Vec<u32>,
    /// The manifest's centre nodes, one per cell type in `CELL_TYPES` order.
    pub centre_nodes: Vec<u32>,
    /// runs x [Torch, Libm], in (run, counts) order.
    pub jobs: Vec<Job>,
    pub controls: Controls,
    /// One per run, in run order.
    pub coverage: Vec<Coverage>,
    pub libm: LibmFingerprint,
    /// The worker threads used.
    pub threads: usize,
}

impl Report {
    pub fn cell_type(&self, node: usize) -> &str {
        &self.cell_types[self.node_cell_type[node] as usize]
    }

    /// Fails closed (spec R18): one job per (run, counts), each passed, the
    /// controls passed, and one coverage entry per run.
    pub fn passed(&self) -> bool {
        self.jobs.len() == 2 * self.runs.len()
            && self.runs.iter().all(|&r| {
                [Counts::Torch, Counts::Libm]
                    .iter()
                    .all(|&c| self.jobs.iter().filter(|j| j.run == r && j.counts == c).count() == 1)
            })
            && self.jobs.iter().all(Job::passed)
            && self.controls.passed()
            && self.coverage.len() == self.runs.len()
            && self.coverage.iter().zip(&self.runs).all(|(c, &r)| c.run == r)
    }

    /// All eight registered runs, in order.
    pub fn complete(&self) -> bool {
        self.runs.iter().copied().eq(0..RUNS.len() as u32)
    }
}

/// Runs `work(i)` for every i in 0..n on up to `threads` threads, the
/// calling thread included, and returns the results in index order. A
/// thread that cannot be spawned leaves its share to the others.
fn pool<T: Send>(threads: usize, n: usize, work: impl Fn(usize) -> T + Sync) -> Vec<Option<T>> {
    let slots: Vec<Mutex<Option<T>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let worker = || loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        if i >= n {
            break;
        }
        let r = work(i);
        *slots[i].lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    };
    std::thread::scope(|s| {
        for _ in 1..threads.min(n) {
            if std::thread::Builder::new().spawn_scoped(s, worker).is_err() {
                break;
            }
        }
        worker();
    });
    slots.into_iter().map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner())).collect()
}

enum Control {
    Settled(Result<Diff, ParityError>),
    Played(Result<SequenceResult, ParityError>),
}

fn unfilled(i: usize) -> ParityError {
    ParityError::Mismatch(format!("job slot {i} was never filled"))
}

/// Runs every gate and control on the fixtures under `data`.
///
/// 1. Every required file is present (else `Missing`, for the first absent).
/// 2. The fixture checks pass (else a fixture error), before any arithmetic.
/// 3. The 2 x runs jobs run on `min(threads, jobs)` threads; each job's
///    error stays in its outcome.
/// 4. The four controls run on the same pool, and coverage is read from the
///    exact jobs that passed.
pub fn validate(data: &Path, opts: &Options) -> Result<Report, ParityError> {
    opts.check()?;
    for rel in required_files() {
        let path = data.join(rel);
        if let Err(e) = std::fs::metadata(&path) {
            return Err(match e.kind() {
                std::io::ErrorKind::NotFound => ParityError::Missing { rel: rel.to_string(), path },
                kind => ParityError::Unreadable { rel: rel.to_string(), path, kind },
            });
        }
    }
    let provenance = Provenance::parse(&read_text(data, PROVENANCE)?)
        .map_err(|e| ParityError::Malformed { rel: PROVENANCE.into(), detail: e.to_string() })?;
    let f = Fixtures::load(data)?;
    let t = TraceData::load(data)?;
    check_fixtures(&f, &t, &provenance)?;

    let mut jobs = Vec::new();
    for &r in &opts.runs {
        let (_, model, dt, p) = RUNS[r as usize];
        for counts in [Counts::Torch, Counts::Libm] {
            jobs.push(Job {
                run: r,
                model: model.to_string(),
                dt: dt.to_string(),
                precision: p,
                counts,
                settling_bounds: settling_bounds(model, dt, p, counts)?,
                trace_bounds: trace_bounds(model, dt, p, counts)?,
                outcome: Err(unfilled(jobs.len())),
            });
        }
    }
    let threads = opts.threads.min(jobs.len());
    let outcomes = pool(threads, jobs.len(), |i| run(&f, &t.view(jobs[i].precision), jobs[i].run, jobs[i].counts));
    for (i, (job, outcome)) in jobs.iter_mut().zip(outcomes).enumerate() {
        job.outcome = match outcome {
            None => Err(unfilled(i)),
            Some(Ok(r)) if r.settling.run != job.run || r.settling.counts != job.counts => Err(ParityError::Mismatch(
                format!("job slot {i} holds run {} {:?}", r.settling.run, r.settling.counts),
            )),
            Some(o) => o,
        };
    }

    let t64 = t.view(Dtype::F64);
    let mut controls = pool(threads.min(4), 4, |i| match i {
        0 => Control::Settled(settle(&f, 3, "049", Counts::Torch).map(|s| s.settled)),
        1 => Control::Settled(settle(&f, 3, "000", Counts::IntegerNSyn).map(|s| s.settled)),
        2 => Control::Played(first(play(&f, &t64, 3, Counts::Torch, Start::Rust, &[8], true))),
        _ => Control::Played(first(play(&f, &t64, 3, Counts::Torch, Start::Flyvis, &[8], false))),
    })
    .into_iter();
    let mut settled = |i: usize| match controls.next() {
        Some(Some(Control::Settled(r))) => r,
        _ => Err(unfilled(i)),
    };
    let wrong_model = settled(0);
    let integer_counts = settled(1);
    let mut played = |i: usize| match controls.next() {
        Some(Some(Control::Played(r))) => r,
        _ => Err(unfilled(i)),
    };
    let mirrored = played(2);
    let from_flyvis = played(3);

    let coverage = opts
        .runs
        .iter()
        .map(|&r| Coverage {
            run: r,
            movement: jobs
                .iter()
                .find(|j| j.run == r && j.counts == Counts::Torch && j.passed())
                .and_then(|j| j.outcome.as_ref().ok())
                .map(|o| o.centre_movement.clone()),
        })
        .collect();
    Ok(Report {
        provenance,
        runs: opts.runs.clone(),
        cell_types: f.net.cell_types().to_vec(),
        node_cell_type: f.net.node_cell_type().iter().map(|&t| u32::from(t)).collect(),
        centre_nodes: f.manifest.centre_nodes.iter().map(|c| c.node).collect(),
        jobs,
        controls: Controls { wrong_model, integer_counts, mirrored, from_flyvis },
        coverage,
        libm: libm_fingerprint(),
        threads,
    })
}

fn first(r: Result<Vec<SequenceResult>, ParityError>) -> Result<SequenceResult, ParityError> {
    r?.into_iter().next().ok_or_else(|| ParityError::Mismatch("a control played no sequence".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::SettlingResult;
    use crate::test_support::{data_dir, or_skip, require_present};

    fn diff0() -> Diff {
        Diff { max: 0.0, at: 0, differ: 0 }
    }

    fn sequence(s: u32) -> SequenceResult {
        SequenceResult {
            sequence: s,
            family: "flashes".into(),
            n_frames: 150,
            snapshot_frames: vec![0, 29, 59, 89, 119, 149],
            centre: diff0(),
            snapshots: diff0(),
            first: None,
        }
    }

    fn job(run: u32, counts: Counts) -> Job {
        let (_, model, dt, p) = RUNS[run as usize];
        Job {
            run,
            model: model.into(),
            dt: dt.into(),
            precision: p,
            counts,
            settling_bounds: settling_bounds(model, dt, p, counts).unwrap(),
            trace_bounds: trace_bounds(model, dt, p, counts).unwrap(),
            outcome: Ok(RunResult {
                settling: SettlingResult {
                    run,
                    model: model.into(),
                    counts,
                    settled: diff0(),
                    settling_centre: diff0(),
                    settled_len: 45669,
                    centre_len: 3250,
                    first_centre_difference: None,
                    per_type_max: vec![0.0; 65],
                },
                sequences: (0..12).map(sequence).collect(),
                centre_movement: vec![1.0; 65],
            }),
        }
    }

    fn controls() -> Controls {
        let mut mirrored = sequence(8);
        mirrored.snapshots.max = 2.5;
        Controls {
            wrong_model: Ok(Diff { max: 1.0, at: 0, differ: 1 }),
            integer_counts: Ok(Diff { max: 1e-6, at: 0, differ: 1 }),
            mirrored: Ok(mirrored),
            from_flyvis: Ok(sequence(8)),
        }
    }

    fn report(runs: Vec<u32>) -> Report {
        Report {
            provenance: crate::parity::checks::registered_provenance(),
            jobs: runs.iter().flat_map(|&r| [job(r, Counts::Torch), job(r, Counts::Libm)]).collect(),
            coverage: runs.iter().map(|&r| Coverage { run: r, movement: Some(vec![1.0; 65]) }).collect(),
            runs,
            cell_types: vec![],
            node_cell_type: vec![],
            centre_nodes: vec![],
            controls: controls(),
            libm: libm_fingerprint(),
            threads: 1,
        }
    }

    #[test]
    fn options_refuse_zero_threads_no_runs_an_unknown_run_and_a_repeat() {
        assert_eq!(Options::default(), Options { threads: 1, runs: (0..8).collect() });
        assert_eq!(Options::default().check(), Ok(()));
        for opts in [
            Options { threads: 0, ..Options::default() },
            Options { runs: vec![], ..Options::default() },
            Options { runs: vec![8], ..Options::default() },
            Options { runs: vec![2, 3, 2], ..Options::default() },
        ] {
            assert!(matches!(opts.check(), Err(ParityError::Usage(_))), "{opts:?}");
            assert!(matches!(validate(Path::new("/nonexistent"), &opts), Err(ParityError::Usage(_))), "{opts:?}");
        }
    }

    #[test]
    fn a_job_passes_only_when_complete_and_within_its_bounds() {
        assert!(job(2, Counts::Torch).passed() && job(2, Counts::Libm).passed());
        let mut j = job(2, Counts::Torch);
        j.outcome = Err(ParityError::Mismatch("x".into()));
        assert!(!j.passed(), "an error");
        let mut j = job(2, Counts::Torch);
        j.outcome.as_mut().unwrap().sequences.pop();
        assert!(!j.passed(), "11 sequences");
        let mut j = job(2, Counts::Torch);
        j.outcome.as_mut().unwrap().settling.run = 3;
        assert!(!j.passed(), "another run's result");
        let mut j = job(2, Counts::Libm);
        j.outcome.as_mut().unwrap().settling.counts = Counts::Torch;
        assert!(!j.passed(), "other counts' result");
        let mut j = job(2, Counts::Torch);
        j.outcome.as_mut().unwrap().sequences[5].snapshots.differ = 1;
        assert!(!j.passed(), "an exact value differs");
        let mut j = job(2, Counts::Libm);
        let bound = j.trace_bounds.1.value();
        j.outcome.as_mut().unwrap().sequences[5].snapshots = Diff { max: bound, at: 0, differ: 9 };
        assert!(j.passed(), "a libm maximum at its bound holds");
        j.outcome.as_mut().unwrap().sequences[5].snapshots.max = f64::from_bits(bound.to_bits() + 1);
        assert!(!j.passed(), "a libm maximum past its bound");
        let mut j = job(2, Counts::Libm);
        j.outcome.as_mut().unwrap().settling.settled.max = f64::INFINITY;
        assert!(!j.passed(), "an infinite maximum");
    }

    #[test]
    fn controls_pass_only_when_every_one_returned_and_holds() {
        assert!(controls().passed());
        let e = || ParityError::Mismatch("x".into());
        let mut c = controls();
        c.wrong_model = Err(e());
        assert!(!c.passed());
        let mut c = controls();
        c.integer_counts = Ok(Diff { max: 1e-10, at: 0, differ: 1 });
        assert!(!c.passed(), "not above the f64 bound");
        let mut c = controls();
        c.mirrored = Err(e());
        assert!(!c.passed());
        let mut c = controls();
        c.from_flyvis.as_mut().unwrap().centre.differ = 1;
        assert!(!c.passed());
    }

    #[test]
    fn a_report_passes_only_with_every_job_once() {
        assert!(report(vec![2, 3]).passed());
        assert!(!report(vec![2, 3]).complete());
        assert!(report((0..8).collect()).complete());
        let mut r = report(vec![2, 3]);
        r.jobs.pop();
        assert!(!r.passed(), "a missing job");
        let mut r = report(vec![2, 3]);
        r.jobs[3] = job(3, Counts::Torch);
        assert!(!r.passed(), "a duplicated (run, counts)");
        let mut r = report(vec![2, 3]);
        r.coverage.pop();
        assert!(!r.passed(), "a missing coverage entry");
        let mut r = report(vec![2, 3]);
        r.controls.from_flyvis = Err(ParityError::Mismatch("x".into()));
        assert!(!r.passed(), "a control errored");
        assert!(!report(vec![0, 1, 2, 3, 4, 5, 7, 6]).complete(), "out of order");
    }

    #[test]
    fn the_pool_returns_every_result_in_order_at_any_thread_count() {
        for threads in [1, 3, 16] {
            let r = pool(threads, 10, |i| i * i);
            assert_eq!(r, (0..10).map(|i| Some(i * i)).collect::<Vec<_>>(), "{threads} threads");
        }
    }

    /// The thread count changes nothing: every job, control and coverage
    /// value is equal at 1 and at 4 threads.
    #[test]
    fn validate_is_bit_identical_across_thread_counts() {
        let Some(()) = or_skip(require_present(required_files())) else { return };
        let one = validate(&data_dir(), &Options { threads: 1, runs: vec![2, 3] }).unwrap_or_else(|e| panic!("{e}"));
        let four = validate(&data_dir(), &Options { threads: 4, runs: vec![2, 3] }).unwrap_or_else(|e| panic!("{e}"));
        assert!(one.passed(), "runs 2 and 3 pass");
        assert_eq!((one.threads, four.threads), (1, 4));
        assert_eq!(one.jobs, four.jobs);
        assert_eq!(one.controls, four.controls);
        assert_eq!(one.coverage, four.coverage);
    }

    /// A temporary data dir of links to the real files, with one replaced.
    #[cfg(unix)]
    fn altered_data(test: &str, rel: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("semblance-parity-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for r in required_files() {
            let to = dir.join(r);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            if *r == rel {
                std::fs::write(&to, bytes).unwrap();
            } else {
                std::os::unix::fs::symlink(data_dir().join(r), &to).unwrap();
            }
        }
        dir
    }

    #[cfg(unix)]
    #[test]
    fn validate_refuses_a_provenance_gap_one_ulp_off() {
        let Some(()) = or_skip(require_present(required_files())) else { return };
        let text = std::fs::read_to_string(data_dir().join(PROVENANCE)).unwrap();
        let (model, dt, gap, _) = crate::parity::TRACE_F32_GAPS[3];
        let printed = format!("{gap:e}");
        let k = text.find(&format!("\"{model} {dt}\"")).expect("the gap's key");
        let at = k + text[k..].find("\"centre\": ").expect("its centre gap") + "\"centre\": ".len();
        let len = text[at..].find([',', '}']).unwrap();
        let value: f64 = text[at..at + len].trim().parse().unwrap();
        assert_eq!(value.to_bits(), gap.to_bits(), "{printed}");
        let altered = format!("{}{:e}{}", &text[..at], f64::from_bits(gap.to_bits() + 1), &text[at + len..]);
        let dir = altered_data("gap", PROVENANCE, altered.as_bytes());
        let r = validate(&dir, &Options::default());
        std::fs::remove_dir_all(&dir).unwrap();
        match r {
            Err(ParityError::Mismatch(m)) => assert!(m.starts_with(&format!("{model} {dt} centre: PROVENANCE ")), "{m}"),
            other => panic!("expected the gap's Mismatch, got {:?}", other.map(|_| ())),
        }
    }

    #[cfg(unix)]
    #[test]
    fn validate_refuses_a_file_whose_size_disagrees() {
        let Some(()) = or_skip(require_present(required_files())) else { return };
        let rel = "flyvis-reference/centre.f64.bin";
        let mut bytes = std::fs::read(data_dir().join(rel)).unwrap();
        bytes.pop();
        let dir = altered_data("size", rel, &bytes);
        let r = validate(&dir, &Options::default());
        std::fs::remove_dir_all(&dir).unwrap();
        match r {
            Err(ParityError::Mismatch(m)) => {
                assert_eq!(m, format!("centre.f64.bin: {} bytes, manifest says {}", bytes.len(), bytes.len() + 1))
            }
            other => panic!("expected the size Mismatch, got {:?}", other.map(|_| ())),
        }
    }
}
