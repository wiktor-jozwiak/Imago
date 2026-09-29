//! Job model shared by the GUI and the CLI: tasks, statuses, progress events,
//! per-file reports and job summaries.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::errors::ErrorInfo;
use crate::filesystem::InputFile;
use crate::settings::{ImageFormat, OptimizeSettings};

/// Identifier of a file within a job (assigned by the caller).
pub type FileId = u64;

/// One file to process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTask {
    /// Caller-assigned identifier, echoed in every event.
    pub id: FileId,
    /// The input file.
    pub input: InputFile,
}

/// A batch of files processed with the same settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    /// Files to process, in order.
    pub files: Vec<FileTask>,
    /// Settings for every file.
    pub settings: OptimizeSettings,
}

/// Final status of a file (the UI statuses from the specification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    /// Waiting in the queue ("Oczekuje").
    Pending,
    /// Being processed ("Przetwarzanie").
    Processing,
    /// Written successfully ("Ukończono").
    Done,
    /// Intentionally not written ("Pominięto").
    Skipped,
    /// Stopped by the user ("Anulowano").
    Cancelled,
    /// Failed ("Błąd").
    Failed,
}

/// Pipeline stage, reported while a file is processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// Reading and validating the file.
    Reading,
    /// Detecting format, dimensions, alpha and animation.
    Analyzing,
    /// Decoding pixels.
    Decoding,
    /// Orientation, resizing and color conversion.
    Transforming,
    /// Encoding candidates.
    Encoding,
    /// Writing and verifying the output.
    Writing,
}

impl Stage {
    /// Approximate overall progress (0..1) at the start of this stage.
    pub fn progress(self) -> f32 {
        match self {
            Stage::Reading => 0.02,
            Stage::Analyzing => 0.08,
            Stage::Decoding => 0.15,
            Stage::Transforming => 0.35,
            Stage::Encoding => 0.45,
            Stage::Writing => 0.92,
        }
    }
}

/// Time spent in each stage, in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageTimings {
    /// Reading the file and analysing the header.
    pub read_ms: f64,
    /// Decoding.
    pub decode_ms: f64,
    /// Orientation, resizing and color conversion.
    pub resize_ms: f64,
    /// Encoding all candidates.
    pub encode_ms: f64,
    /// Writing, verifying and moving the output.
    pub write_ms: f64,
    /// Whole file.
    pub total_ms: f64,
}

/// Converts a duration to fractional milliseconds.
pub fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Why the output looks the way it does (rendered as text by the UI/CLI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DecisionReason {
    /// The format was chosen by the user.
    Requested,
    /// Auto mode picked the smallest candidate.
    Smallest,
    /// No candidate was smaller than the original, so the original was kept
    /// (possibly with metadata removed losslessly).
    KeptOriginal,
}

/// Size of one encoded candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSize {
    /// Candidate format.
    pub format: ImageFormat,
    /// Whether the candidate was lossless.
    pub lossless: bool,
    /// Encoded size in bytes.
    pub size: u64,
}

/// The decision made for a file, e.g. "WebP chosen: 71% smaller, transparency kept".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    /// Output format.
    pub format: ImageFormat,
    /// Reason.
    pub reason: DecisionReason,
    /// Size reduction versus the original, in percent (negative = larger).
    pub saved_percent: f64,
    /// Transparency was present and preserved.
    pub kept_transparency: bool,
    /// Animation was present and preserved.
    pub kept_animation: bool,
    /// Every candidate that was tried.
    pub candidates: Vec<CandidateSize>,
}

impl Decision {
    /// English explanation, used by the CLI and in logs.
    pub fn message(&self) -> String {
        let format = self.format.display_name();
        let mut extras = Vec::new();
        if self.kept_transparency {
            extras.push("kept transparency");
        }
        if self.kept_animation {
            extras.push("kept the animation");
        }
        let extras = if extras.is_empty() {
            String::new()
        } else {
            format!(" and {}", extras.join(" and "))
        };
        match self.reason {
            DecisionReason::Smallest => format!(
                "Chose {format} because it reduced the file size by {:.0}%{extras}.",
                self.saved_percent
            ),
            DecisionReason::KeptOriginal => {
                format!("Kept the original {format}: no re-encoded variant was smaller.")
            }
            DecisionReason::Requested => {
                format!(
                    "Wrote {format} as requested ({:+.0}% size change).",
                    -self.saved_percent
                )
            }
        }
    }
}

/// A non-fatal note about how a file was processed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    /// Stable code, e.g. `animation_kept_in_input_format`.
    pub code: String,
    /// English description.
    pub message: String,
}

impl Warning {
    /// Creates a warning.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Warning {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

/// Everything known about one processed file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReport {
    /// File identifier.
    pub id: FileId,
    /// Input path.
    pub input_path: PathBuf,
    /// Output path (when written, or the existing file when skipped).
    pub output_path: Option<PathBuf>,
    /// Final status.
    pub status: FileStatus,
    /// Detected input format.
    pub input_format: Option<ImageFormat>,
    /// Output format.
    pub output_format: Option<ImageFormat>,
    /// Input size in bytes.
    pub input_size: u64,
    /// Output size in bytes.
    pub output_size: Option<u64>,
    /// Input dimensions (as displayed, after EXIF orientation).
    pub input_dimensions: Option<(u32, u32)>,
    /// Output dimensions.
    pub output_dimensions: Option<(u32, u32)>,
    /// Size reduction in percent (negative when the file grew).
    pub reduction_percent: Option<f64>,
    /// Timings.
    pub timings: StageTimings,
    /// Format decision.
    pub decision: Option<Decision>,
    /// Non-fatal notes.
    pub warnings: Vec<Warning>,
    /// Error, when the status is `Failed` (or the reason for `Skipped`).
    pub error: Option<ErrorInfo>,
}

impl FileReport {
    /// An empty report for a task that has not been processed yet.
    pub fn pending(task: &FileTask) -> Self {
        FileReport {
            id: task.id,
            input_path: task.input.path.clone(),
            output_path: None,
            status: FileStatus::Pending,
            input_format: None,
            output_format: None,
            input_size: task.input.size,
            output_size: None,
            input_dimensions: None,
            output_dimensions: None,
            reduction_percent: None,
            timings: StageTimings::default(),
            decision: None,
            warnings: Vec::new(),
            error: None,
        }
    }
}

/// Totals for a finished (or cancelled) job.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    /// Number of files in the job.
    pub total: usize,
    /// Written successfully.
    pub done: usize,
    /// Skipped.
    pub skipped: usize,
    /// Failed.
    pub failed: usize,
    /// Cancelled.
    pub cancelled: usize,
    /// Total input bytes of written files.
    pub input_bytes: u64,
    /// Total output bytes of written files.
    pub output_bytes: u64,
    /// Wall-clock time of the job in milliseconds.
    pub elapsed_ms: f64,
    /// The job was cancelled before finishing.
    pub was_cancelled: bool,
}

impl JobSummary {
    /// Adds a file report to the totals.
    pub fn record(&mut self, report: &FileReport) {
        match report.status {
            FileStatus::Done => {
                self.done += 1;
                self.input_bytes += report.input_size;
                self.output_bytes += report.output_size.unwrap_or(0);
            }
            FileStatus::Skipped => self.skipped += 1,
            FileStatus::Failed => self.failed += 1,
            FileStatus::Cancelled => self.cancelled += 1,
            FileStatus::Pending | FileStatus::Processing => {}
        }
    }

    /// Bytes saved over all written files.
    pub fn saved_bytes(&self) -> i64 {
        self.input_bytes as i64 - self.output_bytes as i64
    }

    /// Overall reduction in percent.
    pub fn saved_percent(&self) -> f64 {
        if self.input_bytes == 0 {
            0.0
        } else {
            self.saved_bytes() as f64 / self.input_bytes as f64 * 100.0
        }
    }
}

/// Progress events, in order, for one job.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JobEvent {
    /// The job started.
    #[serde(rename_all = "camelCase")]
    Started {
        /// Number of files.
        total: usize,
        /// Worker threads in use.
        workers: usize,
    },
    /// A worker picked up a file.
    #[serde(rename_all = "camelCase")]
    FileStarted {
        /// File identifier.
        id: FileId,
    },
    /// A file moved to another stage.
    #[serde(rename_all = "camelCase")]
    FileProgress {
        /// File identifier.
        id: FileId,
        /// Current stage.
        stage: Stage,
        /// Approximate progress of this file (0..1).
        progress: f32,
    },
    /// A file finished (in any final status).
    #[serde(rename_all = "camelCase")]
    FileFinished {
        /// The report.
        report: Box<FileReport>,
    },
    /// The job finished.
    #[serde(rename_all = "camelCase")]
    Finished {
        /// Totals.
        summary: JobSummary,
    },
}

/// Receives job events. Implementations must be cheap and must not block.
pub trait EventSink: Send + Sync {
    /// Handles one event.
    fn emit(&self, event: JobEvent);
}

impl<F: Fn(JobEvent) + Send + Sync> EventSink for F {
    fn emit(&self, event: JobEvent) {
        self(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(reason: DecisionReason) -> Decision {
        Decision {
            format: ImageFormat::Webp,
            reason,
            saved_percent: 71.2,
            kept_transparency: true,
            kept_animation: false,
            candidates: Vec::new(),
        }
    }

    #[test]
    fn decision_messages_explain_the_choice() {
        assert_eq!(
            decision(DecisionReason::Smallest).message(),
            "Chose WebP because it reduced the file size by 71% and kept transparency."
        );
        assert!(
            decision(DecisionReason::KeptOriginal)
                .message()
                .contains("Kept the original")
        );
    }

    #[test]
    fn summary_counts_statuses_and_bytes() {
        let task = FileTask {
            id: 1,
            input: InputFile {
                path: PathBuf::from("a.png"),
                root: None,
                size: 1000,
            },
        };
        let mut summary = JobSummary::default();
        let mut done = FileReport::pending(&task);
        done.status = FileStatus::Done;
        done.output_size = Some(250);
        summary.record(&done);
        let mut failed = FileReport::pending(&task);
        failed.status = FileStatus::Failed;
        summary.record(&failed);
        assert_eq!((summary.done, summary.failed), (1, 1));
        assert_eq!(summary.saved_bytes(), 750);
        assert!((summary.saved_percent() - 75.0).abs() < 1e-9);
    }

    #[test]
    fn events_serialize_with_a_type_tag() {
        let json = serde_json::to_string(&JobEvent::FileProgress {
            id: 7,
            stage: Stage::Encoding,
            progress: 0.5,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"type":"fileProgress","id":7,"stage":"encoding","progress":0.5}"#
        );
    }
}
