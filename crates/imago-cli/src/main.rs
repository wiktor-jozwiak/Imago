//! `imago` — command-line interface of Imago Image Optimizer.
//!
//! Uses exactly the same engine and settings as the desktop application.

mod args;
mod report;

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use clap::Parser;
use imago_core::app;
use imago_core::archive::{ArchiveSummary, create_zip};
use imago_core::codecs::capabilities;
use imago_core::filesystem::scan_inputs;
use imago_core::jobs::{FileReport, FileStatus, FileTask, JobEvent, JobSpec};
use imago_core::settings::ConflictPolicy;
use imago_core::workers::{JobControl, run_job};

use crate::args::Cli;
use crate::report::{JsonReport, Printer, Verbosity, display_path};

/// Exit code: some files failed.
const EXIT_FAILURES: u8 = 1;
/// Exit code: nothing to do or a fatal error.
const EXIT_FATAL: u8 = 3;
/// Exit code: interrupted by Ctrl+C.
const EXIT_CANCELLED: u8 = 130;

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(&cli);
    if cli.list_formats {
        print_formats();
        return ExitCode::SUCCESS;
    }
    match run(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(EXIT_FATAL)
        }
    }
}

fn run(cli: &Cli) -> Result<u8, String> {
    for input in &cli.inputs {
        if !input.exists() {
            return Err(format!("input not found: {}", input.display()));
        }
    }
    let single_file = cli.inputs.len() == 1 && cli.inputs[0].is_file();
    let plan = cli.plan(single_file)?;
    let verbosity = if cli.quiet {
        Verbosity::Quiet
    } else if cli.verbose > 0 {
        Verbosity::Verbose
    } else {
        Verbosity::Normal
    };

    let control = Arc::new(JobControl::new());
    install_ctrl_c(Arc::clone(&control));

    let scan_options = app::scan_options_for(&plan.settings, plan.scan);
    let scan = scan_inputs(&cli.inputs, &scan_options, control.token())
        .map_err(|error| error.to_string())?;
    if scan.files.is_empty() {
        let hint = if cli.recursive {
            ""
        } else {
            " (use --recursive to include subfolders)"
        };
        return Err(format!("no supported images found{hint}"));
    }

    let tasks: Vec<FileTask> = scan
        .files
        .into_iter()
        .enumerate()
        .map(|(index, input)| FileTask {
            id: index as u64,
            input,
        })
        .collect();
    let inputs: Vec<_> = tasks.iter().map(|task| task.input.clone()).collect();
    let printer = Printer::new(verbosity, cli.json, tasks.len());
    if verbosity == Verbosity::Verbose {
        for skipped in &scan.skipped {
            printer.info(format!(
                "ignored {} ({:?})",
                display_path(&skipped.path),
                skipped.reason
            ));
        }
    }

    let spec = JobSpec {
        files: tasks,
        settings: plan.settings,
    };
    let reports = Mutex::new(Vec::<FileReport>::new());
    let sink = |event: JobEvent| {
        printer.handle(&event);
        if let JobEvent::FileFinished { report } = event
            && let Ok(mut reports) = reports.lock()
        {
            reports.push(*report);
        }
    };
    let summary = run_job(&spec, &control, &sink).map_err(|error| error.to_string())?;
    let mut reports = reports
        .into_inner()
        .map_err(|_| "internal error: report lock poisoned".to_string())?;
    reports.sort_by_key(|report| report.id);

    let archive = match (&cli.zip, control.is_cancelled()) {
        (Some(zip), false) => Some(write_zip(
            zip, &reports, &inputs, &spec, &control, &printer,
        )?),
        _ => None,
    };

    printer.summary(&summary, archive.as_ref());
    if cli.json {
        let report = JsonReport {
            summary: &summary,
            files: &reports,
            archive: archive.as_ref(),
        };
        let json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
        println!("{json}");
    }

    Ok(if summary.was_cancelled {
        EXIT_CANCELLED
    } else if summary.failed > 0 {
        EXIT_FAILURES
    } else {
        0
    })
}

fn write_zip(
    zip: &std::path::Path,
    reports: &[FileReport],
    inputs: &[imago_core::filesystem::InputFile],
    spec: &JobSpec,
    control: &JobControl,
    printer: &Printer,
) -> Result<ArchiveSummary, String> {
    let outputs: Vec<_> = reports
        .iter()
        .filter(|report| report.status == FileStatus::Done)
        .filter_map(|report| {
            let input = inputs.get(usize::try_from(report.id).ok()?)?.clone();
            Some((input, report.output_path.clone()?))
        })
        .collect();
    if outputs.is_empty() {
        return Err("no optimized files to put into the ZIP archive".into());
    }
    let entries =
        app::archive_entries(&outputs, &spec.settings).map_err(|error| error.to_string())?;
    let replace = spec.settings.output.conflict == ConflictPolicy::Overwrite;
    printer.info(format!("Creating {}…", display_path(zip)));
    create_zip(&entries, zip, replace, control.token(), |_, _| {}).map_err(|error| {
        match error.code() {
            "output_exists" => format!(
                "{} already exists (use --overwrite to replace it)",
                zip.display()
            ),
            _ => format!("cannot create ZIP archive: {error}"),
        }
    })
}

/// First Ctrl+C cancels gracefully (temporary files are removed); a second one exits.
fn install_ctrl_c(control: Arc<JobControl>) {
    let result = ctrlc::set_handler(move || {
        if control.is_cancelled() {
            std::process::exit(i32::from(EXIT_CANCELLED));
        }
        eprintln!("\nCancelling… (press Ctrl+C again to quit immediately)");
        control.cancel();
    });
    if let Err(error) = result {
        tracing::warn!(%error, "Ctrl+C handler not installed");
    }
}

fn init_logging(cli: &Cli) {
    use tracing_subscriber::EnvFilter;
    let default = match (cli.quiet, cli.verbose) {
        (true, _) | (false, 0 | 1) => "off",
        (false, 2) => "imago_core=debug",
        (false, _) => "debug",
    };
    let filter = EnvFilter::try_from_env("IMAGO_LOG").unwrap_or_else(|_| EnvFilter::new(default));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}

fn print_formats() {
    println!(
        "{:<9} {:<6} {:<6} {:<7} {:<9} {:<6} {:<10}",
        "FORMAT", "READ", "WRITE", "LOSSY", "LOSSLESS", "ALPHA", "ANIMATION"
    );
    let yes_no = |value: bool| if value { "yes" } else { "no" };
    for capability in capabilities() {
        println!(
            "{:<9} {:<6} {:<6} {:<7} {:<9} {:<6} {:<10}{}",
            capability.format.display_name(),
            yes_no(capability.decode),
            yes_no(capability.encode),
            yes_no(capability.encode && capability.lossy),
            yes_no(capability.encode && capability.lossless),
            yes_no(capability.alpha),
            yes_no(capability.animation),
            capability
                .unavailable_reason
                .as_deref()
                .map(|reason| format!("  ({reason})"))
                .unwrap_or_default()
        );
    }
}
