//! Worker pool, memory budget and job control.
//!
//! * `N` worker threads (default: logical cores − 1) pull files from a
//!   multi-consumer queue — nothing is loaded before a worker takes the file;
//! * a byte-based [`MemoryBudget`] limits how many large images are decoded at
//!   the same time;
//! * [`JobControl`] cancels the whole job or skips single files.

use std::collections::HashMap;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::cancel::CancelToken;
use crate::errors::{ImagoError, Result};
use crate::filesystem::ClaimedPaths;
use crate::filesystem::safe_write::TempCleanup;
use crate::image_engine::pipeline::{PipelineContext, process_file};
use crate::jobs::{
    EventSink, FileId, FileReport, FileStatus, FileTask, JobEvent, JobSpec, JobSummary, millis,
};

/// Default memory budget for concurrently decoded images (2 GiB).
pub const DEFAULT_MEMORY_BUDGET: u64 = 2 * 1024 * 1024 * 1024;

/// A counting semaphore measured in bytes.
#[derive(Debug)]
pub struct MemoryBudget {
    total: u64,
    used: Mutex<u64>,
    released: Condvar,
}

/// Bytes held from a [`MemoryBudget`]; returned when dropped.
#[derive(Debug)]
pub struct Reservation<'a> {
    budget: &'a MemoryBudget,
    bytes: u64,
}

impl MemoryBudget {
    /// Creates a budget of `total` bytes (at least 1).
    pub fn new(total: u64) -> Self {
        MemoryBudget {
            total: total.max(1),
            used: Mutex::new(0),
            released: Condvar::new(),
        }
    }

    /// Total size of the budget.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Bytes currently reserved.
    pub fn in_use(&self) -> u64 {
        self.used.lock().map(|used| *used).unwrap_or(0)
    }

    /// Waits until `bytes` fit into the budget (or the budget is idle, so a
    /// single oversized image can still run alone), checking `cancel`.
    pub fn acquire(&self, bytes: u64, cancel: &CancelToken) -> Result<Reservation<'_>> {
        let bytes = bytes.clamp(1, self.total);
        let mut used = self
            .used
            .lock()
            .map_err(|_| ImagoError::Internal("memory budget lock poisoned".into()))?;
        loop {
            cancel.check()?;
            if *used == 0 || *used + bytes <= self.total {
                *used += bytes;
                return Ok(Reservation {
                    budget: self,
                    bytes,
                });
            }
            used = self
                .released
                .wait_timeout(used, Duration::from_millis(50))
                .map_err(|_| ImagoError::Internal("memory budget lock poisoned".into()))?
                .0;
        }
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if let Ok(mut used) = self.budget.used.lock() {
            *used = used.saturating_sub(self.bytes);
        }
        self.budget.released.notify_all();
    }
}

/// Cancellation handles for a running job.
#[derive(Debug, Default)]
pub struct JobControl {
    cancel: CancelToken,
    files: Mutex<HashMap<FileId, CancelToken>>,
}

impl JobControl {
    /// A fresh controller.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels the whole job.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Whether the whole job was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// The job-level token.
    pub fn token(&self) -> &CancelToken {
        &self.cancel
    }

    /// Skips one file: it is stopped if running, or never started if queued.
    pub fn skip(&self, id: FileId) {
        self.file_token(id).cancel();
    }

    /// The token of one file (created on first use, child of the job token).
    pub fn file_token(&self, id: FileId) -> CancelToken {
        match self.files.lock() {
            Ok(mut files) => files
                .entry(id)
                .or_insert_with(|| self.cancel.child())
                .clone(),
            Err(_) => self.cancel.child(),
        }
    }

    fn forget(&self, id: FileId) {
        if let Ok(mut files) = self.files.lock() {
            files.remove(&id);
        }
    }
}

/// Runs a job to completion (or cancellation), emitting events on `sink`.
///
/// Returns an error only when the settings are invalid; per-file problems are
/// reported in the file reports.
pub fn run_job(spec: &JobSpec, control: &JobControl, sink: &dyn EventSink) -> Result<JobSummary> {
    spec.settings.validate()?;
    if matches!(
        spec.settings.output.location,
        crate::settings::OutputLocation::File { .. }
    ) && spec.files.len() > 1
    {
        return Err(ImagoError::InvalidSettings(
            "an exact output file can only be used with a single input file".into(),
        ));
    }
    let started = Instant::now();
    let total = spec.files.len();
    let workers = spec.settings.effective_workers().min(total).max(1);
    tracing::info!(total, workers, "job started");
    sink.emit(JobEvent::Started { total, workers });

    let budget = MemoryBudget::new(
        spec.settings
            .limits
            .memory_budget_bytes
            .unwrap_or(DEFAULT_MEMORY_BUDGET),
    );
    let claims = ClaimedPaths::new();
    let cleanup = TempCleanup::default();
    let summary = Mutex::new(JobSummary {
        total,
        ..JobSummary::default()
    });

    let (sender, receiver) = crossbeam_channel::unbounded::<&FileTask>();
    for task in &spec.files {
        // The receiver is alive here, so sending cannot fail.
        let _ = sender.send(task);
    }
    drop(sender);

    let work = || {
        while let Ok(task) = receiver.recv() {
            let report = process_task(task, spec, control, sink, &budget, &claims, &cleanup);
            if let Ok(mut summary) = summary.lock() {
                summary.record(&report);
            }
            sink.emit(JobEvent::FileFinished {
                report: Box::new(report),
            });
        }
    };

    std::thread::scope(|scope| {
        let mut spawned = 0;
        for index in 0..workers {
            let worker = std::thread::Builder::new()
                .name(format!("imago-worker-{index}"))
                .spawn_scoped(scope, work);
            match worker {
                Ok(_) => spawned += 1,
                Err(error) => tracing::warn!(%error, "could not start worker thread"),
            }
        }
        if spawned == 0 {
            // No threads available: process on the calling thread.
            work();
        }
    });

    let mut summary = summary
        .into_inner()
        .map_err(|_| ImagoError::Internal("job summary lock poisoned".into()))?;
    summary.elapsed_ms = millis(started.elapsed());
    summary.was_cancelled = control.is_cancelled();
    tracing::info!(
        done = summary.done,
        skipped = summary.skipped,
        failed = summary.failed,
        cancelled = summary.cancelled,
        elapsed_ms = summary.elapsed_ms,
        "job finished"
    );
    sink.emit(JobEvent::Finished {
        summary: summary.clone(),
    });
    Ok(summary)
}

fn process_task(
    task: &FileTask,
    spec: &JobSpec,
    control: &JobControl,
    sink: &dyn EventSink,
    budget: &MemoryBudget,
    claims: &ClaimedPaths,
    cleanup: &TempCleanup,
) -> FileReport {
    let token = control.file_token(task.id);
    let report = if control.is_cancelled() {
        stopped_report(task, FileStatus::Cancelled)
    } else if token.is_cancelled() {
        stopped_report(task, FileStatus::Skipped)
    } else {
        sink.emit(JobEvent::FileStarted { id: task.id });
        let progress = |stage: crate::jobs::Stage| {
            sink.emit(JobEvent::FileProgress {
                id: task.id,
                stage,
                progress: stage.progress(),
            });
        };
        let context = PipelineContext {
            settings: &spec.settings,
            budget,
            claims,
            cleanup,
            cancel: &token,
            job_cancel: control.token(),
            progress: &progress,
        };
        process_file(task, &context)
    };
    control.forget(task.id);
    report
}

fn stopped_report(task: &FileTask, status: FileStatus) -> FileReport {
    let mut report = FileReport::pending(task);
    report.status = status;
    if status == FileStatus::Skipped {
        report.error = Some(crate::errors::ErrorInfo {
            code: "skipped_by_user",
            message: "skipped by the user".into(),
            alternative: None,
            hint: None,
        });
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn budget_blocks_until_memory_is_released() {
        let budget = Arc::new(MemoryBudget::new(100));
        let first = budget.acquire(80, &CancelToken::new()).unwrap();
        let peak = Arc::new(AtomicU64::new(0));
        std::thread::scope(|scope| {
            let worker_budget = budget.clone();
            let peak = peak.clone();
            scope.spawn(move || {
                let _second = worker_budget.acquire(50, &CancelToken::new()).unwrap();
                peak.fetch_max(worker_budget.in_use(), Ordering::SeqCst);
            });
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(budget.in_use(), 80, "second reservation must wait");
            drop(first);
        });
        assert!(peak.load(Ordering::SeqCst) <= 100);
        assert_eq!(budget.in_use(), 0);
    }

    #[test]
    fn oversized_request_runs_alone() {
        let budget = MemoryBudget::new(10);
        let reservation = budget.acquire(1_000, &CancelToken::new()).unwrap();
        assert_eq!(budget.in_use(), 10);
        drop(reservation);
        assert_eq!(budget.in_use(), 0);
    }

    #[test]
    fn waiting_for_memory_can_be_cancelled() {
        let budget = MemoryBudget::new(10);
        let _held = budget.acquire(10, &CancelToken::new()).unwrap();
        let token = CancelToken::new();
        token.cancel();
        assert!(budget.acquire(5, &token).unwrap_err().is_cancelled());
    }

    #[test]
    fn skipping_a_file_does_not_cancel_the_job() {
        let control = JobControl::new();
        control.skip(3);
        assert!(control.file_token(3).is_cancelled());
        assert!(!control.file_token(4).is_cancelled());
        assert!(!control.is_cancelled());
        control.cancel();
        assert!(control.file_token(4).is_cancelled());
    }
}
