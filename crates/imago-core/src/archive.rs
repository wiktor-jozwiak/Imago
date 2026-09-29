//! ZIP archives of optimized files.
//!
//! Images are already compressed, so entries are *stored* (no deflate): the
//! archive is created at disk speed and files are streamed one by one, never
//! loaded into memory. The archive itself is written through the same atomic
//! temporary-file mechanism as images and verified before it is moved into place.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::cancel::CancelToken;
use crate::errors::{ImagoError, Result};
use crate::filesystem::paths::{numbered, sanitize_relative};
use crate::filesystem::safe_write::{Placement, WriteOptions, write_atomic_with};

/// A file to put into an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    /// File on disk.
    pub source: PathBuf,
    /// Path inside the archive (relative, `/`-separated).
    pub name: String,
}

/// Result of creating an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSummary {
    /// Path of the archive.
    pub path: PathBuf,
    /// Number of files stored.
    pub files: usize,
    /// Archive size in bytes.
    pub size: u64,
}

/// Builds the entry name for `file` relative to `base` (e.g. the `optimized`
/// folder), falling back to the bare file name.
pub fn entry_name(file: &Path, base: Option<&Path>) -> Result<String> {
    let relative = base
        .and_then(|base| file.strip_prefix(base).ok())
        .and_then(sanitize_relative)
        .filter(|relative| !relative.as_os_str().is_empty())
        .or_else(|| file.file_name().map(PathBuf::from))
        .ok_or_else(|| ImagoError::PathTraversal {
            path: file.to_path_buf(),
        })?;
    sanitize_entry_name(&relative.to_string_lossy())
}

/// Normalizes an entry name: forward slashes, no leading slash, no `..`,
/// no drive letters, no empty components.
pub fn sanitize_entry_name(name: &str) -> Result<String> {
    let parts: Vec<&str> = name
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let unsafe_part = parts
        .iter()
        .any(|part| *part == ".." || part.contains(':') || part.chars().any(char::is_control));
    if parts.is_empty() || unsafe_part {
        return Err(ImagoError::PathTraversal {
            path: PathBuf::from(name),
        });
    }
    Ok(parts.join("/"))
}

/// Creates `destination` containing `entries`.
///
/// Duplicate names get a ` (n)` suffix. `progress` is called after each file
/// with `(done, total)`. An existing destination is replaced only when
/// `replace` is set.
pub fn create_zip(
    entries: &[ArchiveEntry],
    destination: &Path,
    replace: bool,
    cancel: &CancelToken,
    mut progress: impl FnMut(usize, usize),
) -> Result<ArchiveSummary> {
    if entries.is_empty() {
        return Err(ImagoError::Archive(
            "there are no files to put into the archive".into(),
        ));
    }
    let names = unique_names(entries)?;
    let total = entries.len();
    let options = WriteOptions {
        placement: if replace {
            Placement::Replace
        } else {
            Placement::CreateNew
        },
        fsync: true,
    };
    let mut failure: Option<ImagoError> = None;
    let write_result = write_atomic_with(
        destination,
        options,
        cancel,
        |file| {
            let mut writer = ZipWriter::new(file);
            for (index, (entry, name)) in entries.iter().zip(&names).enumerate() {
                if cancel.is_cancelled() {
                    failure = Some(ImagoError::Cancelled);
                    return Err(io::Error::other("cancelled"));
                }
                if let Err(error) = add_file(&mut writer, entry, name) {
                    let message = error.to_string();
                    failure = Some(error);
                    return Err(io::Error::other(message));
                }
                progress(index + 1, total);
            }
            writer.finish().map(|_| ()).map_err(io::Error::other)
        },
        |temp| verify_archive(temp, total),
    );
    if let Err(error) = write_result {
        return Err(failure.unwrap_or(match error {
            ImagoError::Io { source, .. } => ImagoError::Archive(source.to_string()),
            other => other,
        }));
    }
    let size = std::fs::metadata(destination)
        .map_err(|error| ImagoError::io(destination, error))?
        .len();
    Ok(ArchiveSummary {
        path: destination.to_path_buf(),
        files: total,
        size,
    })
}

fn add_file(writer: &mut ZipWriter<&mut File>, entry: &ArchiveEntry, name: &str) -> Result<()> {
    let source = File::open(&entry.source).map_err(|error| ImagoError::io(&entry.source, error))?;
    let metadata = source
        .metadata()
        .map_err(|error| ImagoError::io(&entry.source, error))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(zip_time)
        .unwrap_or_else(DateTime::default_for_write);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(metadata.len() >= u64::from(u32::MAX))
        .last_modified_time(modified);
    writer
        .start_file(name, options)
        .map_err(|error| ImagoError::Archive(error.to_string()))?;
    io::copy(&mut BufReader::new(source), writer)
        .map_err(|error| ImagoError::io(&entry.source, error))?;
    Ok(())
}

fn unique_names(entries: &[ArchiveEntry]) -> Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut names = Vec::with_capacity(entries.len());
    for entry in entries {
        let base = sanitize_entry_name(&entry.name)?;
        let mut name = base.clone();
        let mut number = 1;
        while !seen.insert(name.to_lowercase()) {
            name = numbered(Path::new(&base), number)
                .to_string_lossy()
                .replace('\\', "/");
            number += 1;
        }
        names.push(name);
    }
    Ok(names)
}

fn verify_archive(path: &Path, expected: usize) -> Result<()> {
    let file = File::open(path).map_err(|error| ImagoError::io(path, error))?;
    let archive =
        ZipArchive::new(BufReader::new(file)).map_err(|error| ImagoError::VerificationFailed {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?;
    if archive.len() != expected {
        return Err(ImagoError::VerificationFailed {
            path: path.to_path_buf(),
            reason: format!("expected {expected} entries, found {}", archive.len()),
        });
    }
    Ok(())
}

/// Converts a file time to a ZIP timestamp (UTC, 2-second precision).
fn zip_time(time: SystemTime) -> Option<DateTime> {
    let seconds = time.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let days = i64::try_from(seconds / 86_400).ok()?;
    let second_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    DateTime::from_date_and_time(
        u16::try_from(year).ok()?,
        month,
        day,
        (second_of_day / 3600) as u8,
        (second_of_day % 3600 / 60) as u8,
        (second_of_day % 60) as u8,
    )
    .ok()
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian calendar.
fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u8;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u8;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn entry_names_are_sanitized() {
        assert_eq!(sanitize_entry_name("a/b/c.png").unwrap(), "a/b/c.png");
        assert_eq!(sanitize_entry_name("\\a\\.\\b.png").unwrap(), "a/b.png");
        assert!(sanitize_entry_name("../evil.png").is_err());
        assert!(sanitize_entry_name("C:/evil.png").is_err());
        assert!(sanitize_entry_name("").is_err());
        assert_eq!(
            entry_name(
                Path::new("/out/optimized/sub/x.webp"),
                Some(Path::new("/out/optimized"))
            )
            .unwrap(),
            "sub/x.webp"
        );
        assert_eq!(
            entry_name(Path::new("/elsewhere/y.webp"), Some(Path::new("/out"))).unwrap(),
            "y.webp"
        );
    }

    #[test]
    fn civil_dates_are_correct() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(20_725), (2026, 9, 29));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn creates_a_readable_archive_with_unique_names() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.webp");
        let b = dir.path().join("b.webp");
        std::fs::write(&a, b"first").unwrap();
        std::fs::write(&b, b"second").unwrap();
        let entries = vec![
            ArchiveEntry {
                source: a.clone(),
                name: "photos/a.webp".into(),
            },
            ArchiveEntry {
                source: b,
                name: "photos/A.webp".into(),
            },
            ArchiveEntry {
                source: a,
                name: "zdjęcia/żółw.webp".into(),
            },
        ];
        let destination = dir.path().join("out.zip");
        let mut calls = 0;
        let summary = create_zip(
            &entries,
            &destination,
            false,
            &CancelToken::new(),
            |_, _| calls += 1,
        )
        .unwrap();
        assert_eq!((summary.files, calls), (3, 3));

        let mut archive = ZipArchive::new(File::open(&destination).unwrap()).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_owned())
            .collect();
        assert_eq!(
            names,
            vec!["photos/a.webp", "photos/A (1).webp", "zdjęcia/żółw.webp"]
        );
        let mut content = String::new();
        archive
            .by_index(1)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert_eq!(content, "second");
    }

    #[test]
    fn existing_archive_is_not_replaced_without_permission() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.png");
        std::fs::write(&source, b"x").unwrap();
        let destination = dir.path().join("out.zip");
        std::fs::write(&destination, b"keep").unwrap();
        let entries = vec![ArchiveEntry {
            source,
            name: "a.png".into(),
        }];
        let error = create_zip(
            &entries,
            &destination,
            false,
            &CancelToken::new(),
            |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error.code(), "output_exists");
        assert_eq!(std::fs::read(&destination).unwrap(), b"keep");
        create_zip(&entries, &destination, true, &CancelToken::new(), |_, _| {}).unwrap();
        assert_ne!(std::fs::read(&destination).unwrap(), b"keep");
    }

    #[test]
    fn missing_source_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let entries = vec![ArchiveEntry {
            source: dir.path().join("missing.png"),
            name: "missing.png".into(),
        }];
        let destination = dir.path().join("out.zip");
        let error = create_zip(
            &entries,
            &destination,
            false,
            &CancelToken::new(),
            |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error.code(), "not_found");
        assert!(!destination.exists());
        let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn empty_archives_are_refused_and_cancellation_works() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("out.zip");
        assert_eq!(
            create_zip(&[], &destination, false, &CancelToken::new(), |_, _| {})
                .unwrap_err()
                .code(),
            "archive_error"
        );
        let source = dir.path().join("a.png");
        std::fs::write(&source, b"x").unwrap();
        let token = CancelToken::new();
        token.cancel();
        let entries = vec![ArchiveEntry {
            source,
            name: "a.png".into(),
        }];
        assert!(
            create_zip(&entries, &destination, false, &token, |_, _| {})
                .unwrap_err()
                .is_cancelled()
        );
        assert!(!destination.exists());
    }
}
