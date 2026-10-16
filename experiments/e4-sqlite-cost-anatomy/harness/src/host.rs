//! The process side of a pilot pair (protocol.md §9): each measured half
//! runs as its own `measure-cell` child pinned to CPU 2, and a separate
//! `monitor-stream` process, pinned to the CPUs outside `{2, 8}`, samples
//! every interval for as long as the half runs. [`MeasurementHost`] is the
//! seam the pair logic ([`crate::pilot_run`]) is written against, so its
//! decisions are tested with scripted hosts and never depend on this
//! machine's real load; [`ProcessHost`] is the real thing, exercised
//! end to end by the integration tests.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::builds::Build;
use crate::driver::OperationPlan;
use crate::interference::{self, TimedSample, monotonic_ns};
use crate::measure_protocol::{self, MeasureCellRequest};
use crate::restore;
use crate::workloads::{Cell, Size};

/// One measured half of a pair: which cell, which build, and the trace it
/// must run (§2.1: both halves of a pair draw the same one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HalfRequest {
    pub cell: Cell,
    pub build: Build,
    pub plan: OperationPlan,
    pub repetition_index: usize,
    pub seed: u64,
}

/// One measured half's outcome: the child's mean wall time, the
/// `CLOCK_MONOTONIC` instants its measured window opened (the parent's
/// `go`) and closed (the child's own stamp after its last operation), and
/// every monitor sample taken around it — the first before the window
/// opened, the last (the boundary sample) the first one after it closed,
/// taken while the child holds still. The child checks and cleans up only
/// after the monitor stopped, so no sample sees that work.
#[derive(Debug, Clone, PartialEq)]
pub struct HalfRun {
    pub mean_wall_ns: f64,
    pub started_ns: u64,
    pub finished_ns: u64,
    pub samples: Vec<TimedSample>,
}

impl HalfRun {
    pub fn window(&self) -> Duration {
        Duration::from_nanos(self.finished_ns.saturating_sub(self.started_ns))
    }
}

/// What a pair needs from the machine. Failing to run anything is an
/// `Err` — an execution failure, never interference (§9).
pub trait MeasurementHost {
    /// Watches the otherwise idle machine for one monitor interval:
    /// §9's conditions, checked before a pair starts.
    fn observe_idle_machine(&self) -> io::Result<Vec<TimedSample>>;

    /// Runs one half in its own child, monitored throughout its window.
    fn run_half(&self, request: &HalfRequest) -> io::Result<HalfRun>;
}

/// The pristine snapshot directory of each size (protocol §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshots {
    pub small: PathBuf,
    pub medium: PathBuf,
}

impl Snapshots {
    /// Loads and checkpoints both sizes once under `data_dir`.
    pub fn prepare(data_dir: &Path) -> io::Result<Snapshots> {
        Snapshots::prepare_with(data_dir, Size::row_count)
    }

    fn prepare_with(data_dir: &Path, row_count_of: impl Fn(Size) -> u64) -> io::Result<Snapshots> {
        let snapshots = Snapshots {
            small: data_dir.join("small").join("pristine"),
            medium: data_dir.join("medium").join("pristine"),
        };
        for size in Size::ALL {
            restore::prepare_pristine(snapshots.dir_for(size), row_count_of(size))?;
        }
        Ok(snapshots)
    }

    pub fn dir_for(&self, size: Size) -> &Path {
        match size {
            Size::Small => &self.small,
            Size::Medium => &self.medium,
        }
    }
}

/// The real host: spawns the instrumented and passthrough binaries as
/// measurement children and the instrumented one as the monitor.
#[derive(Debug, Clone)]
pub struct ProcessHost {
    pub instrumented: PathBuf,
    pub passthrough: PathBuf,
    pub snapshots: Snapshots,
    pub scratch_dir: PathBuf,
    pub monitor_interval: Duration,
}

impl ProcessHost {
    fn binary(&self, build: Build) -> &Path {
        match build {
            Build::Instrumented => &self.instrumented,
            Build::Passthrough => &self.passthrough,
        }
    }

    fn start_monitor(&self) -> io::Result<MonitorProcess> {
        MonitorProcess::start(&self.instrumented, self.monitor_interval)
    }
}

impl MeasurementHost for ProcessHost {
    fn observe_idle_machine(&self) -> io::Result<Vec<TimedSample>> {
        let mut monitor = self.start_monitor()?;
        monitor.next_sample()?;
        monitor.next_sample()?;
        Ok(monitor.stop())
    }

    fn run_half(&self, request: &HalfRequest) -> io::Result<HalfRun> {
        let measure = MeasureCellRequest {
            workload: request.cell.workload,
            synchronous: request.cell.synchronous,
            row_count: request.cell.size.row_count(),
            plan: request.plan,
            seed: request.seed,
            pristine_dir: self.snapshots.dir_for(request.cell.size).to_path_buf(),
            scratch_dir: self.scratch_dir.clone(),
        };
        let mut child = MeasureChild::spawn(self.binary(request.build), &measure)?;
        child.wait_until_ready()?;

        let mut monitor = self.start_monitor()?;
        monitor.next_sample()?;
        let started_ns = monotonic_ns();
        child.go()?;
        // The child stamps the end of its last operation and then holds
        // still: the monitor takes the sample that covers the stamp, is
        // stopped, and only then is the child released to check and clean
        // up, so none of that reaches a judged sample.
        let finished_ns = child.wait_until_done()?;
        let samples = monitor.finish_after(finished_ns)?;
        child.release()?;
        let mean_wall_ns = child.finish()?;
        Ok(HalfRun {
            mean_wall_ns,
            started_ns,
            finished_ns,
            samples,
        })
    }
}

/// How long to wait for a monitor sample, at least: a floor under ten
/// intervals so a short test interval does not make a busy machine look
/// like a dead monitor.
const MINIMUM_SAMPLE_PATIENCE: Duration = Duration::from_secs(2);
const SAMPLE_PATIENCE_INTERVALS: u32 = 10;

/// How long to wait for the next sample of a monitor sampling at `interval`.
fn sample_patience(interval: Duration) -> Duration {
    (interval * SAMPLE_PATIENCE_INTERVALS).max(MINIMUM_SAMPLE_PATIENCE)
}

/// The pinned `monitor-stream` child and the thread reading its lines.
struct MonitorProcess {
    child: Child,
    samples: Receiver<io::Result<TimedSample>>,
    reader: Option<JoinHandle<()>>,
    patience: Duration,
    received: Vec<TimedSample>,
}

impl MonitorProcess {
    fn start(binary: &Path, interval: Duration) -> io::Result<Self> {
        let mut child = Command::new(binary)
            .arg("monitor-stream")
            .arg(interval.as_millis().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("the monitor's stdout was not piped"))?;
        let (sender, samples) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let parsed = line.and_then(|line| {
                    interference::parse_sample_line(&line).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("could not parse monitor output: {line:?}"),
                        )
                    })
                });
                if sender.send(parsed).is_err() {
                    return;
                }
            }
        });
        Ok(MonitorProcess {
            child,
            samples,
            reader: Some(reader),
            patience: sample_patience(interval),
            received: Vec::new(),
        })
    }

    /// Waits for the next sample the monitor wrote.
    fn next_sample(&mut self) -> io::Result<TimedSample> {
        match self.samples.recv_timeout(self.patience) {
            Ok(sample) => {
                let sample = sample?;
                self.received.push(sample);
                Ok(sample)
            }
            Err(RecvTimeoutError::Timeout) => Err(io::Error::other(format!(
                "the monitor wrote no sample within {:?}",
                self.patience
            ))),
            Err(RecvTimeoutError::Disconnected) => {
                Err(io::Error::other("the monitor exited before sampling"))
            }
        }
    }

    /// Keeps the monitor sampling until a sample is taken after
    /// `finished_ns` — the boundary sample, whose interval covers the end
    /// of the window — and stops it there.
    fn finish_after(mut self, finished_ns: u64) -> io::Result<Vec<TimedSample>> {
        while self.next_sample()?.at_ns <= finished_ns {}
        Ok(self.stop())
    }

    /// Ends the monitor and returns every sample consumed so far.
    fn stop(mut self) -> Vec<TimedSample> {
        self.shut_down();
        std::mem::take(&mut self.received)
    }

    fn shut_down(&mut self) {
        // The monitor never ends by itself, and a process that is already
        // gone is as stopped as one just killed: neither error matters.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for MonitorProcess {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// The `measure-cell` child and the pipes of its ready/go conversation.
struct MeasureChild {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr: File,
    description: String,
}

impl MeasureChild {
    fn spawn(binary: &Path, request: &MeasureCellRequest) -> io::Result<Self> {
        // A file rather than a pipe: nothing needs a reader thread, and the
        // text is there to quote if the child fails.
        let stderr = tempfile::tempfile()?;
        let mut child = Command::new(binary)
            .arg("measure-cell")
            .args(request.to_args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr.try_clone()?))
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("the child's stdout was not piped"))?;
        Ok(MeasureChild {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr,
            description: format!(
                "measure-cell for {} against {}",
                request.workload.label(),
                binary.display()
            ),
        })
    }

    /// Reads the child's `ready` line: its snapshot is restored and its
    /// statements prepared.
    fn wait_until_ready(&mut self) -> io::Result<()> {
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        if line.trim() == measure_protocol::READY_LINE {
            Ok(())
        } else {
            Err(self.failure(&format!("did not say ready, wrote {line:?}")))
        }
    }

    fn go(&mut self) -> io::Result<()> {
        self.write_line(measure_protocol::GO_LINE)
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| io::Error::other("the child's stdin was not piped"))?;
        writeln!(stdin, "{line}")?;
        stdin.flush()
    }

    /// Reads lines until the child's `done` line and returns the instant
    /// it carries: the end of the child's last operation.
    fn wait_until_done(&mut self) -> io::Result<u64> {
        let mut line = String::new();
        if self.stdout.read_line(&mut line)? == 0 {
            let status = self.child.wait()?;
            return Err(self.failure(&format!("exited with {status} before finishing")));
        }
        measure_protocol::parse_done_ns(&line)
            .ok_or_else(|| self.failure(&format!("wrote no done line, wrote {line:?}")))
    }

    /// Lets the child out of its hold at the `done` stamp.
    fn release(&mut self) -> io::Result<()> {
        self.write_line(measure_protocol::RELEASE_LINE)
    }

    /// Reads the child's result and waits for it to exit.
    fn finish(&mut self) -> io::Result<f64> {
        let mut rest = String::new();
        self.stdout.read_to_string(&mut rest)?;
        let status = self.child.wait()?;
        if !status.success() {
            return Err(self.failure(&format!("exited with {status}")));
        }
        measure_protocol::parse_mean_wall_ns(&rest)
            .ok_or_else(|| self.failure(&format!("wrote no mean_wall_ns line, wrote {rest:?}")))
    }

    /// An error naming the child and what its stderr said.
    fn failure(&mut self, what: &str) -> io::Error {
        let mut said = String::new();
        let read = self
            .stderr
            .rewind()
            .and_then(|()| self.stderr.read_to_string(&mut said));
        if let Err(err) = read {
            said = format!("(its stderr could not be read: {err})");
        }
        io::Error::other(format!("{} {what}: {}", self.description, said.trim()))
    }
}

impl Drop for MeasureChild {
    fn drop(&mut self) {
        // A child that already exited makes both calls fail harmlessly.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod kiln_e4_24 {
        use super::*;

        #[test]
        fn a_half_reports_its_window_from_its_two_instants() {
            let run = HalfRun {
                mean_wall_ns: 1.0,
                started_ns: 1_000,
                finished_ns: 4_500,
                samples: Vec::new(),
            };
            assert_eq!(run.window(), Duration::from_nanos(3_500));
        }

        #[test]
        fn a_window_that_ends_before_it_starts_is_empty_not_negative() {
            let run = HalfRun {
                mean_wall_ns: 1.0,
                started_ns: 5,
                finished_ns: 1,
                samples: Vec::new(),
            };
            assert_eq!(run.window(), Duration::ZERO);
        }

        #[test]
        fn the_monitor_gets_ten_intervals_to_write_a_sample_and_never_less_than_the_floor() {
            assert_eq!(
                sample_patience(Duration::from_secs(1)),
                Duration::from_secs(10)
            );
            assert_eq!(
                sample_patience(Duration::from_millis(50)),
                MINIMUM_SAMPLE_PATIENCE
            );
        }
    }

    mod kiln_e4_23 {
        use super::*;

        #[test]
        fn preparing_loads_one_snapshot_per_size_with_that_sizes_rows() {
            let data_dir = tempfile::tempdir().expect("tempdir");

            let snapshots = Snapshots::prepare_with(data_dir.path(), |size| match size {
                Size::Small => 5,
                Size::Medium => 7,
            })
            .expect("prepare");

            for (size, rows) in [(Size::Small, 5), (Size::Medium, 7)] {
                let db_path = data_dir.path().join("restored.db");
                let wal_path = data_dir.path().join("restored.db-wal");
                restore::restore_pristine(snapshots.dir_for(size), &db_path, &wal_path)
                    .expect("restore");
                let conn = rusqlite::Connection::open(&db_path).unwrap();
                assert_eq!(crate::workloads::checks::row_count(&conn).unwrap(), rows);
            }
        }

        #[test]
        fn each_size_has_its_own_snapshot_directory() {
            let snapshots = Snapshots {
                small: PathBuf::from("/data/small/pristine"),
                medium: PathBuf::from("/data/medium/pristine"),
            };
            assert_eq!(snapshots.dir_for(Size::Small), snapshots.small);
            assert_eq!(snapshots.dir_for(Size::Medium), snapshots.medium);
        }
    }
}
