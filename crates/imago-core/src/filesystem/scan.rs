//! Expanding user-selected paths (files and folders) into image files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::cancel::CancelToken;
use crate::errors::{ImagoError, Result};
use crate::image_engine::detection::{self, Detected};

/// Options for folder scanning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanOptions {
    /// Descend into subfolders.
    pub recursive: bool,
    /// Include hidden files and folders.
    pub include_hidden: bool,
    /// Folder names that are never entered (e.g. the `optimized` output folder).
    pub exclude_dir_names: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            recursive: true,
            include_hidden: false,
            exclude_dir_names: Vec::new(),
        }
    }
}

/// An input image file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputFile {
    /// Path of the file.
    pub path: PathBuf,
    /// The folder the user selected, when the file was found inside one.
    /// Used to recreate the folder structure in the output.
    pub root: Option<PathBuf>,
    /// Size in bytes at scan time.
    pub size: u64,
}

impl InputFile {
    /// Describes a single file selected directly by the user.
    pub fn from_path(path: impl Into<PathBuf>) -> Result<InputFile> {
        let path = path.into();
        let metadata = std::fs::metadata(&path).map_err(|error| ImagoError::io(&path, error))?;
        if !metadata.is_file() {
            return Err(ImagoError::NotAFile { path });
        }
        Ok(InputFile {
            path,
            root: None,
            size: metadata.len(),
        })
    }

    /// The folder used as the base for output paths.
    pub fn base_dir(&self) -> &Path {
        self.root
            .as_deref()
            .or_else(|| self.path.parent())
            .unwrap_or_else(|| Path::new("."))
    }
}

/// Why a path found during scanning was not included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    /// Hidden file or folder (and hidden files are excluded).
    Hidden,
    /// Not an image Imago can read.
    Unsupported,
    /// The path could not be read.
    Unreadable,
    /// The same file was already added.
    Duplicate,
}

/// A path that was left out, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedPath {
    /// The path.
    pub path: PathBuf,
    /// The reason.
    pub reason: SkipReason,
}

/// Result of a scan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    /// Image files, in a stable order.
    pub files: Vec<InputFile>,
    /// Paths that were left out.
    pub skipped: Vec<SkippedPath>,
}

/// Expands files and folders into the list of images to process.
///
/// * Files selected directly are always included if their content is an image.
/// * Folders are walked (recursively if requested); files with a known image
///   extension are included, files with other extensions are sniffed.
/// * Symbolic links to folders are never followed (no loops, no escaping the
///   selected folder). Imago's own temporary files are ignored.
pub fn scan_inputs(
    paths: &[PathBuf],
    options: &ScanOptions,
    cancel: &CancelToken,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for path in paths {
        cancel.check()?;
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => return Err(ImagoError::io(path, error)),
        };
        if metadata.is_dir() {
            scan_folder(path, options, cancel, &mut seen, &mut report)?;
        } else if metadata.is_file() {
            let file = InputFile {
                path: path.clone(),
                root: None,
                size: metadata.len(),
            };
            add_candidate(file, true, &mut seen, &mut report);
        } else {
            report.skipped.push(SkippedPath {
                path: path.clone(),
                reason: SkipReason::Unsupported,
            });
        }
    }
    Ok(report)
}

fn scan_folder(
    root: &Path,
    options: &ScanOptions,
    cancel: &CancelToken,
    seen: &mut HashSet<PathBuf>,
    report: &mut ScanReport,
) -> Result<()> {
    let max_depth = if options.recursive { usize::MAX } else { 1 };
    let walker = WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            if !options.include_hidden && is_hidden(entry.path(), entry.metadata().ok().as_ref()) {
                return false;
            }
            if entry.file_type().is_dir() {
                let name = entry.file_name().to_string_lossy();
                return !options
                    .exclude_dir_names
                    .iter()
                    .any(|excluded| *excluded == name);
            }
            true
        });

    for entry in walker {
        cancel.check()?;
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                if let Some(path) = error.path() {
                    report.skipped.push(SkippedPath {
                        path: path.to_path_buf(),
                        reason: SkipReason::Unreadable,
                    });
                }
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.path();
        // Files and symlinks to files; directories are handled by the walker.
        let Ok(metadata) = std::fs::metadata(path) else {
            report.skipped.push(SkippedPath {
                path: path.to_path_buf(),
                reason: SkipReason::Unreadable,
            });
            continue;
        };
        if !metadata.is_file() || is_temporary_file(path) {
            continue;
        }
        let file = InputFile {
            path: path.to_path_buf(),
            root: Some(root.to_path_buf()),
            size: metadata.len(),
        };
        add_candidate(file, false, seen, report);
    }
    Ok(())
}

fn add_candidate(
    file: InputFile,
    explicit: bool,
    seen: &mut HashSet<PathBuf>,
    report: &mut ScanReport,
) {
    let identity = std::fs::canonicalize(&file.path).unwrap_or_else(|_| file.path.clone());
    if !seen.insert(identity) {
        report.skipped.push(SkippedPath {
            path: file.path,
            reason: SkipReason::Duplicate,
        });
        return;
    }
    // Known image extensions in folders are accepted without opening the file;
    // the content is verified again before processing. Everything else is sniffed.
    let accepted = (!explicit && detection::has_image_extension(&file.path))
        || matches!(
            detection::detect_file(&file.path),
            Ok(Detected::Supported(_))
        );
    if accepted {
        report.files.push(file);
    } else {
        report.skipped.push(SkippedPath {
            path: file.path,
            reason: SkipReason::Unsupported,
        });
    }
}

/// Whether a file or folder is hidden (dot-name everywhere, plus the hidden
/// attribute on Windows).
pub fn is_hidden(path: &Path, metadata: Option<&std::fs::Metadata>) -> bool {
    let dot_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') && name != "." && name != "..");
    dot_name || has_hidden_attribute(metadata)
}

#[cfg(windows)]
fn has_hidden_attribute(metadata: Option<&std::fs::Metadata>) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    metadata.is_some_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn has_hidden_attribute(_metadata: Option<&std::fs::Metadata>) -> bool {
    false
}

/// Imago's own temporary files (`.imago-*.tmp`).
pub fn is_temporary_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with(super::TEMP_PREFIX) && name.ends_with(super::TEMP_SUFFIX)
        })
}
