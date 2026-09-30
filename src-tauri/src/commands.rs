//! IPC commands exposed to the frontend.
//!
//! File dialogs run on the Rust side: every path Imago writes to comes from a
//! native dialog, and every path the webview may read (thumbnails, previews)
//! was added by the user or written by Imago in this session.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use imago_core::app::{self, FileInfo};
use imago_core::archive::{ArchiveSummary, create_zip};
use imago_core::codecs::{FormatCapability, capabilities};
use imago_core::config::{self, AppConfig};
use imago_core::filesystem::{InputFile, ScanOptions, scan_inputs};
use imago_core::jobs::{FileTask, JobEvent, JobSpec};
use imago_core::preview::render_preview;
use imago_core::settings::{OptimizeSettings, default_workers};
use imago_core::workers::{JobControl, run_job};
use imago_core::{AppInfo, CancelToken};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::errors::{CommandError, CommandResult};
use crate::state::AppState;

/// Static information for the UI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoPayload {
    /// Name, taglines and version.
    pub info: AppInfo,
    /// What each format can do in this build.
    pub capabilities: Vec<FormatCapability>,
    /// Default number of workers on this machine.
    pub default_workers: usize,
    /// Operating system (`windows`, `macos`, `linux`).
    pub platform: &'static str,
    /// Folder with log files.
    pub log_dir: Option<PathBuf>,
}

/// Product information, capabilities and defaults.
#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfoPayload {
    AppInfoPayload {
        info: imago_core::app_info(),
        capabilities: capabilities(),
        default_workers: default_workers(),
        platform: std::env::consts::OS,
        log_dir: state.log_dir.clone(),
    }
}

/// The loaded configuration and a warning if it had to be reset.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPayload {
    /// Configuration.
    pub config: AppConfig,
    /// Problem found while loading (shown once).
    pub warning: Option<String>,
}

/// Returns the saved configuration.
#[tauri::command]
pub fn load_config(state: State<'_, AppState>) -> CommandResult<ConfigPayload> {
    let config = state
        .config
        .lock()
        .map_err(|_| CommandError::internal("configuration lock poisoned"))?
        .clone();
    let warning = state
        .config_warning
        .lock()
        .ok()
        .and_then(|mut warning| warning.take());
    Ok(ConfigPayload { config, warning })
}

/// Saves the configuration.
#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config: AppConfig) -> CommandResult<()> {
    config.settings.validate()?;
    if let Ok(mut current) = state.config.lock() {
        *current = config.clone();
    }
    let Some(path) = state.config_path.clone() else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || config::save(&path, &config))
        .await
        .map_err(CommandError::from)??;
    Ok(())
}

/// Every extension the engine reads (JPEG, PNG, ..., HEIC).
fn image_extensions() -> Vec<&'static str> {
    imago_core::settings::ImageFormat::ALL
        .iter()
        .flat_map(|format| format.extensions().iter().copied())
        .collect()
}

/// Native "open files" dialog.
#[tauri::command]
pub async fn pick_files(app: AppHandle, title: String) -> CommandResult<Vec<PathBuf>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(title)
            .add_filter("Images", &image_extensions())
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| path.into_path().ok())
            .collect()
    })
    .await
    .map_err(CommandError::from)
}

/// Native "choose folder" dialog.
#[tauri::command]
pub async fn pick_folder(app: AppHandle, title: String) -> CommandResult<Option<PathBuf>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(title)
            .blocking_pick_folder()
            .and_then(|path| path.into_path().ok())
    })
    .await
    .map_err(CommandError::from)
}

/// A scanned file with its header information.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedFile {
    /// The input.
    pub input: InputFile,
    /// Header information (format, dimensions, animation, alpha).
    pub info: FileInfo,
}

/// Result of scanning dropped or selected paths.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPayload {
    /// Images found.
    pub files: Vec<ScannedFile>,
    /// Number of paths left out (unsupported, hidden, duplicates, unreadable).
    pub skipped: usize,
}

/// Expands files and folders into images and reads their headers.
#[tauri::command]
pub async fn scan_paths(
    state: State<'_, AppState>,
    paths: Vec<PathBuf>,
    scan: ScanOptions,
    settings: OptimizeSettings,
) -> CommandResult<ScanPayload> {
    let registry = Arc::clone(&state.paths);
    tauri::async_runtime::spawn_blocking(move || -> CommandResult<ScanPayload> {
        let options = app::scan_options_for(&settings, scan);
        let report = scan_inputs(&paths, &options, &CancelToken::new())?;
        let info = app::describe(&report.files, &settings.limits);
        let files = report
            .files
            .into_iter()
            .zip(info)
            .map(|(input, info)| {
                registry.register(&input.path);
                ScannedFile { input, info }
            })
            .collect();
        Ok(ScanPayload {
            files,
            skipped: report.skipped.len(),
        })
    })
    .await
    .map_err(CommandError::from)?
}

/// Starts a job in the background; progress is streamed through `on_event`.
#[tauri::command]
pub fn start_job(
    state: State<'_, AppState>,
    files: Vec<FileTask>,
    settings: OptimizeSettings,
    on_event: Channel<JobEvent>,
) -> CommandResult<u64> {
    settings.validate()?;
    if let Some(task) = files
        .iter()
        .find(|task| !state.paths.contains(&task.input.path))
    {
        return Err(CommandError::forbidden(&task.input.path));
    }
    let job_id = state.next_job_id();
    let control = Arc::new(JobControl::new());
    state
        .jobs
        .lock()
        .map_err(|_| CommandError::internal("job registry lock poisoned"))?
        .insert(job_id, Arc::clone(&control));
    let registry = Arc::clone(&state.paths);
    let jobs = Arc::clone(&state.jobs);

    std::thread::Builder::new()
        .name(format!("imago-job-{job_id}"))
        .spawn(move || {
            let spec = JobSpec { files, settings };
            let sink = |event: JobEvent| {
                if let JobEvent::FileFinished { report } = &event
                    && let Some(output) = &report.output_path
                {
                    registry.register(output);
                }
                if let Err(error) = on_event.send(event) {
                    tracing::warn!(%error, "progress event could not be delivered");
                }
            };
            if let Err(error) = run_job(&spec, &control, &sink) {
                tracing::error!(%error, "job failed to start");
            }
            if let Ok(mut jobs) = jobs.lock() {
                jobs.remove(&job_id);
            }
        })
        .map_err(|error| CommandError::internal(format!("cannot start the job: {error}")))?;
    tracing::info!(job_id, "job started");
    Ok(job_id)
}

/// Cancels a running job.
#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: u64) {
    if let Some(job) = state.job(job_id) {
        tracing::info!(job_id, "job cancelled by the user");
        job.cancel();
    }
}

/// Skips one file of a running job.
#[tauri::command]
pub fn skip_file(state: State<'_, AppState>, job_id: u64, file_id: u64) {
    if let Some(job) = state.job(job_id) {
        job.skip(file_id);
    }
}

/// Renders a thumbnail or preview (PNG/JPEG bytes) of a known file.
#[tauri::command]
pub async fn render_image(
    state: State<'_, AppState>,
    path: PathBuf,
    max_side: u32,
) -> CommandResult<Response> {
    if !state.paths.contains(&path) {
        return Err(CommandError::forbidden(&path));
    }
    let limits = state
        .config
        .lock()
        .map(|config| config.settings.limits)
        .unwrap_or_default();
    let preview =
        tauri::async_runtime::spawn_blocking(move || render_preview(&path, max_side, &limits))
            .await
            .map_err(CommandError::from)??;
    Ok(Response::new(preview.data))
}

/// Shows a file in the system file manager.
#[tauri::command]
pub fn reveal_path(app: AppHandle, state: State<'_, AppState>, path: PathBuf) -> CommandResult<()> {
    if !state.paths.contains(&path) {
        return Err(CommandError::forbidden(&path));
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|error| CommandError::internal(error.to_string()))
}

/// Opens a folder that contains optimized files.
#[tauri::command]
pub fn open_folder(app: AppHandle, state: State<'_, AppState>, path: PathBuf) -> CommandResult<()> {
    let is_log_dir = state.log_dir.as_deref() == Some(path.as_path());
    if !(is_log_dir || (path.is_dir() && state.paths.contains_under(&path))) {
        return Err(CommandError::forbidden(&path));
    }
    if is_log_dir {
        std::fs::create_dir_all(&path)
            .map_err(|error| CommandError::from(imago_core::ImagoError::io(&path, error)))?;
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| CommandError::internal(error.to_string()))
}

/// Saves a copy of one result where the user chooses ("download").
#[tauri::command]
pub async fn export_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
    title: String,
) -> CommandResult<Option<PathBuf>> {
    if !state.paths.contains(&path) {
        return Err(CommandError::forbidden(&path));
    }
    tauri::async_runtime::spawn_blocking(move || -> CommandResult<Option<PathBuf>> {
        let mut dialog = app.dialog().file().set_title(title);
        if let Some(name) = path.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
            dialog = dialog.add_filter(extension.to_uppercase(), &[extension]);
        }
        let Some(destination) = dialog
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
        else {
            return Ok(None);
        };
        // The native dialog already asked before replacing an existing file.
        app::export_file(&path, &destination, true)?;
        Ok(Some(destination))
    })
    .await
    .map_err(CommandError::from)?
}

/// One written output and the input it came from.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipItem {
    /// The input (for the folder structure inside the archive).
    pub input: InputFile,
    /// The optimized file.
    pub output: PathBuf,
}

/// Packs optimized files into a ZIP archive chosen with a save dialog.
#[tauri::command]
pub async fn create_zip_archive(
    app: AppHandle,
    state: State<'_, AppState>,
    items: Vec<ZipItem>,
    settings: OptimizeSettings,
    title: String,
) -> CommandResult<Option<ArchiveSummary>> {
    if let Some(item) = items
        .iter()
        .find(|item| !state.paths.contains(&item.output))
    {
        return Err(CommandError::forbidden(&item.output));
    }
    let default_name = state
        .config
        .lock()
        .map(|config| config.zip.file_name.clone())
        .unwrap_or_else(|_| "imago-optimized.zip".into());
    tauri::async_runtime::spawn_blocking(move || -> CommandResult<Option<ArchiveSummary>> {
        let outputs: Vec<(InputFile, PathBuf)> = items
            .into_iter()
            .map(|item| (item.input, item.output))
            .collect();
        let entries = app::archive_entries(&outputs, &settings)?;
        let mut dialog = app
            .dialog()
            .file()
            .set_title(title)
            .set_file_name(default_name)
            .add_filter("ZIP", &["zip"]);
        if let Some((input, _)) = outputs.first() {
            dialog = dialog.set_directory(imago_core::filesystem::paths::destination_dir(
                input,
                &settings.output,
            ));
        }
        let Some(destination) = dialog
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
        else {
            return Ok(None);
        };
        let destination = with_zip_extension(&destination);
        let summary = create_zip(&entries, &destination, true, &CancelToken::new(), |_, _| {})?;
        tracing::info!(path = %summary.path.display(), files = summary.files, "ZIP created");
        Ok(Some(summary))
    })
    .await
    .map_err(CommandError::from)?
}

fn with_zip_extension(path: &Path) -> PathBuf {
    let is_zip = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
    if is_zip {
        path.to_path_buf()
    } else {
        path.with_extension("zip")
    }
}
