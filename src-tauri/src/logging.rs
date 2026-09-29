//! Logging: daily-rotated files in the application's data folder (last 7
//! days kept), plus the console in debug builds. Logs contain file paths and
//! technical errors, never image data, and never leave the computer.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// Starts logging. The returned guard must live as long as the application
/// (dropping it flushes pending log lines).
pub fn init(log_dir: Option<&Path>) -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_env("IMAGO_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    // No ANSI colors anywhere: span fields are formatted once and shared by all
    // layers, so colored console output would leak escape codes into log files.
    let console = cfg!(debug_assertions).then(|| {
        fmt::layer()
            .with_ansi(false)
            .with_target(false)
            .with_writer(std::io::stderr)
    });

    let file_writer = log_dir.and_then(|dir| {
        Builder::new()
            .rotation(Rotation::DAILY)
            .filename_prefix("imago")
            .filename_suffix("log")
            .max_log_files(7)
            .build(dir)
            .ok()
    });
    let (file_layer, guard) = match file_writer {
        Some(writer) => {
            let (writer, guard) = tracing_appender::non_blocking(writer);
            (
                Some(
                    fmt::layer()
                        .with_ansi(false)
                        .with_target(false)
                        .with_writer(writer),
                ),
                Some(guard),
            )
        }
        None => (None, None),
    };

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(console)
        .with(file_layer)
        .try_init();
    guard
}

/// Sends panics to the log (they are also caught per file by the engine).
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "panic");
        default(info);
    }));
}
