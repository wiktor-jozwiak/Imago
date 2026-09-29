//! Application layer: high-level operations shared by the GUI and the CLI.

use std::fs::File;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;

use crate::archive::{ArchiveEntry, entry_name};
use crate::cancel::CancelToken;
use crate::errors::{ErrorInfo, ImagoError, Result};
use crate::filesystem::paths::destination_dir;
use crate::filesystem::safe_write::{Placement, WriteOptions, write_atomic_with};
use crate::filesystem::{InputFile, ScanOptions};
use crate::image_engine::probe_file;
use crate::settings::{ImageFormat, Limits, OptimizeSettings, OutputLocation};

/// What the list view shows about an input before it is processed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    /// The file.
    pub path: PathBuf,
    /// Size in bytes.
    pub size: u64,
    /// Detected format.
    pub format: Option<ImageFormat>,
    /// Width after EXIF orientation.
    pub width: Option<u32>,
    /// Height after EXIF orientation.
    pub height: Option<u32>,
    /// More than one frame.
    pub animated: bool,
    /// The pixel format has an alpha channel.
    pub has_alpha: bool,
    /// Why the file cannot be processed, if known up front.
    pub error: Option<ErrorInfo>,
}

/// Reads headers of many files in parallel (no pixel decoding).
pub fn describe(files: &[InputFile], limits: &Limits) -> Vec<FileInfo> {
    files
        .par_iter()
        .map(|file| match probe_file(&file.path, limits) {
            Ok(probe) => {
                let (width, height) = probe.display_dimensions();
                FileInfo {
                    path: file.path.clone(),
                    size: probe.file_size,
                    format: Some(probe.format),
                    width: Some(width),
                    height: Some(height),
                    animated: probe.animated,
                    has_alpha: probe.has_alpha,
                    error: None,
                }
            }
            Err(error) => FileInfo {
                path: file.path.clone(),
                size: file.size,
                format: None,
                width: None,
                height: None,
                animated: false,
                has_alpha: false,
                error: Some(error.to_info()),
            },
        })
        .collect()
}

/// Scan options that never pick up Imago's own output subfolder.
pub fn scan_options_for(settings: &OptimizeSettings, mut options: ScanOptions) -> ScanOptions {
    if let OutputLocation::Subfolder { name } = &settings.output.location {
        let name = name.trim().to_owned();
        if !options.exclude_dir_names.contains(&name) {
            options.exclude_dir_names.push(name);
        }
    }
    options
}

/// ZIP entries for written outputs, keeping the folder structure relative to
/// each output's destination folder.
pub fn archive_entries(
    outputs: &[(InputFile, PathBuf)],
    settings: &OptimizeSettings,
) -> Result<Vec<ArchiveEntry>> {
    outputs
        .iter()
        .map(|(input, output)| {
            let base = destination_dir(input, &settings.output);
            Ok(ArchiveEntry {
                source: output.clone(),
                name: entry_name(output, Some(&base))?,
            })
        })
        .collect()
}

/// Copies a result to a user-chosen location ("download"), atomically and
/// verified. Returns the number of bytes copied.
pub fn export_file(source: &Path, destination: &Path, replace: bool) -> Result<u64> {
    let size = std::fs::metadata(source)
        .map_err(|error| ImagoError::io(source, error))?
        .len();
    if same_file::is_same_file(source, destination).unwrap_or(false) {
        return Ok(size);
    }
    let options = WriteOptions {
        placement: if replace {
            Placement::Replace
        } else {
            Placement::CreateNew
        },
        fsync: true,
    };
    write_atomic_with(
        destination,
        options,
        &CancelToken::new(),
        |file| {
            let mut reader = BufReader::new(File::open(source)?);
            io::copy(&mut reader, file).map(|_| ())
        },
        |temp| {
            let written = std::fs::metadata(temp)
                .map_err(|error| ImagoError::io(temp, error))?
                .len();
            if written == size {
                Ok(())
            } else {
                Err(ImagoError::VerificationFailed {
                    path: temp.to_path_buf(),
                    reason: format!("copied {written} of {size} bytes"),
                })
            }
        },
    )?;
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgb};

    #[test]
    fn describe_reports_dimensions_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("a.png");
        DynamicImage::ImageRgb8(ImageBuffer::from_pixel(7, 5, Rgb([1, 2, 3])))
            .save(&good)
            .unwrap();
        let bad = dir.path().join("b.png");
        std::fs::write(&bad, b"nope").unwrap();
        let files = vec![
            InputFile::from_path(&good).unwrap(),
            InputFile::from_path(&bad).unwrap(),
        ];
        let info = describe(&files, &Limits::default());
        assert_eq!(info[0].format, Some(ImageFormat::Png));
        assert_eq!((info[0].width, info[0].height), (Some(7), Some(5)));
        assert_eq!(info[1].error.as_ref().unwrap().code, "unsupported_format");
    }

    #[test]
    fn output_subfolder_is_excluded_from_scans() {
        let options = scan_options_for(&OptimizeSettings::default(), ScanOptions::default());
        assert_eq!(options.exclude_dir_names, vec!["optimized".to_string()]);
    }

    #[test]
    fn export_copies_and_protects_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("out.webp");
        std::fs::write(&source, b"result").unwrap();
        let target = dir.path().join("copy").join("saved.webp");
        assert_eq!(export_file(&source, &target, false).unwrap(), 6);
        assert_eq!(std::fs::read(&target).unwrap(), b"result");
        assert_eq!(
            export_file(&source, &target, false).unwrap_err().code(),
            "output_exists"
        );
        std::fs::write(&source, b"newer").unwrap();
        export_file(&source, &target, true).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"newer");
    }

    #[test]
    fn archive_entries_keep_structure_below_the_output_folder() {
        let input = InputFile {
            path: PathBuf::from("/photos/2024/a.png"),
            root: Some(PathBuf::from("/photos")),
            size: 1,
        };
        let output = PathBuf::from("/photos/optimized/2024/a.webp");
        let entries = archive_entries(&[(input, output)], &OptimizeSettings::default()).unwrap();
        assert_eq!(entries[0].name, "2024/a.webp");
    }
}
