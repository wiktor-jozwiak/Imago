//! Terminal output: progress bar, one line per file, summary and JSON report.

use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use imago_core::archive::ArchiveSummary;
use imago_core::jobs::{FileReport, FileStatus, JobEvent, JobSummary};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::Serialize;

/// How much to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// Errors only.
    Quiet,
    /// One line per file and a summary.
    Normal,
    /// Also decisions, warnings and timings.
    Verbose,
}

/// Prints job events. Shared by worker threads.
pub struct Printer {
    verbosity: Verbosity,
    json: bool,
    bar: Option<ProgressBar>,
}

impl Printer {
    /// Creates a printer; the progress bar is shown only on an interactive terminal.
    pub fn new(verbosity: Verbosity, json: bool, total: usize) -> Self {
        let interactive = std::io::stderr().is_terminal();
        let bar = (interactive && verbosity != Verbosity::Quiet && !json).then(|| {
            let bar =
                ProgressBar::with_draw_target(Some(total as u64), ProgressDrawTarget::stderr());
            let style = ProgressStyle::with_template("{spinner} [{bar:32}] {pos}/{len} {wide_msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("=> ");
            bar.set_style(style);
            bar.enable_steady_tick(Duration::from_millis(120));
            bar
        });
        Printer {
            verbosity,
            json,
            bar,
        }
    }

    /// Handles one engine event.
    pub fn handle(&self, event: &JobEvent) {
        match event {
            JobEvent::Started { total, workers } => {
                if self.verbosity == Verbosity::Verbose && !self.json {
                    self.line(format!(
                        "Optimizing {total} file(s) with {workers} worker(s)…"
                    ));
                }
            }
            JobEvent::FileStarted { .. } | JobEvent::FileProgress { .. } => {}
            JobEvent::FileFinished { report } => {
                if let Some(bar) = &self.bar {
                    bar.inc(1);
                    bar.set_message(file_name(&report.input_path));
                }
                self.print_file(report);
            }
            JobEvent::Finished { .. } => {
                if let Some(bar) = &self.bar {
                    bar.finish_and_clear();
                }
            }
        }
    }

    fn print_file(&self, report: &FileReport) {
        if self.json {
            if report.status == FileStatus::Failed {
                eprintln!(
                    "error: {}: {}",
                    report.input_path.display(),
                    error_message(report)
                );
            }
            return;
        }
        let name = display_path(&report.input_path);
        match report.status {
            FileStatus::Done => {
                if self.verbosity == Verbosity::Quiet {
                    return;
                }
                let output = report
                    .output_path
                    .as_deref()
                    .map(display_path)
                    .unwrap_or_default();
                let sizes = match report.output_size {
                    Some(output_size) => format!(
                        "{} → {}  {}",
                        format_bytes(report.input_size),
                        format_bytes(output_size),
                        format_change(report.reduction_percent.unwrap_or(0.0))
                    ),
                    None => String::new(),
                };
                let format = report
                    .output_format
                    .map(|format| format.display_name())
                    .unwrap_or_default();
                self.line(format!("✓ {name} → {output}  {sizes}  {format}"));
                if self.verbosity == Verbosity::Verbose {
                    if let Some(decision) = &report.decision {
                        self.line(format!("    {}", decision.message()));
                    }
                    self.line(format!(
                        "    decode {:.0} ms · resize {:.0} ms · encode {:.0} ms · write {:.0} ms · total {:.0} ms",
                        report.timings.decode_ms,
                        report.timings.resize_ms,
                        report.timings.encode_ms,
                        report.timings.write_ms,
                        report.timings.total_ms
                    ));
                }
                self.print_warnings(report);
            }
            FileStatus::Skipped => {
                if self.verbosity != Verbosity::Quiet {
                    let reason = report
                        .error
                        .as_ref()
                        .map(|error| error.message.clone())
                        .unwrap_or_else(|| "skipped".into());
                    self.line(format!("– {name}: skipped ({reason})"));
                }
            }
            FileStatus::Cancelled => {
                if self.verbosity == Verbosity::Verbose {
                    self.line(format!("– {name}: cancelled"));
                }
            }
            FileStatus::Failed => {
                self.error_line(format!("✗ {name}: {}", error_message(report)));
            }
            FileStatus::Pending | FileStatus::Processing => {}
        }
    }

    fn print_warnings(&self, report: &FileReport) {
        if self.verbosity == Verbosity::Quiet {
            return;
        }
        for warning in &report.warnings {
            self.line(format!("    warning: {}", warning.message));
        }
    }

    /// Prints the job summary.
    pub fn summary(&self, summary: &JobSummary, archive: Option<&ArchiveSummary>) {
        if self.json || self.verbosity == Verbosity::Quiet {
            return;
        }
        let saved = summary.saved_bytes();
        let saved_text = if saved >= 0 {
            format!(
                "saved {} ({:.0}%)",
                format_bytes(saved.unsigned_abs()),
                summary.saved_percent()
            )
        } else {
            format!("grew by {}", format_bytes(saved.unsigned_abs()))
        };
        eprintln!();
        eprintln!(
            "Done: {} optimized, {} skipped, {} failed{} — {saved_text} in {:.1} s",
            summary.done,
            summary.skipped,
            summary.failed,
            if summary.cancelled > 0 {
                format!(", {} cancelled", summary.cancelled)
            } else {
                String::new()
            },
            summary.elapsed_ms / 1000.0
        );
        if let Some(archive) = archive {
            eprintln!(
                "ZIP: {} ({} file(s), {})",
                display_path(&archive.path),
                archive.files,
                format_bytes(archive.size)
            );
        }
    }

    /// Prints a plain informational line (respects quiet mode).
    pub fn info(&self, message: impl AsRef<str>) {
        if self.verbosity != Verbosity::Quiet && !self.json {
            self.line(message.as_ref().to_owned());
        }
    }

    fn line(&self, text: String) {
        match &self.bar {
            Some(bar) => bar.println(text),
            None => eprintln!("{text}"),
        }
    }

    fn error_line(&self, text: String) {
        match &self.bar {
            Some(bar) => bar.suspend(|| eprintln!("{text}")),
            None => eprintln!("{text}"),
        }
    }
}

/// Machine-readable report printed with `--json`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonReport<'a> {
    /// Totals.
    pub summary: &'a JobSummary,
    /// Every file.
    pub files: &'a [FileReport],
    /// The ZIP archive, if one was created.
    pub archive: Option<&'a ArchiveSummary>,
}

fn error_message(report: &FileReport) -> String {
    let Some(error) = &report.error else {
        return "unknown error".into();
    };
    let mut message = error.message.clone();
    if let Some(alternative) = &error.alternative {
        message.push_str(&format!(" (try {alternative})"));
    }
    if let Some(hint) = &error.hint {
        message.push_str(&format!(" — {hint}"));
    }
    message
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Shows paths relative to the working directory when possible.
pub fn display_path(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(&cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

/// Human-readable size with binary units.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// `-71%` for a reduction, `+12%` for growth.
pub fn format_change(reduction_percent: f64) -> String {
    if reduction_percent >= 0.0 {
        format!("−{reduction_percent:.0}%")
    } else {
        format!("+{:.0}%", -reduction_percent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
    }

    #[test]
    fn changes_have_a_sign() {
        assert_eq!(format_change(71.4), "−71%");
        assert_eq!(format_change(-12.0), "+12%");
    }
}
