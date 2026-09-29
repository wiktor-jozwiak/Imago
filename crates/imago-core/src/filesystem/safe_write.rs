//! Crash-safe file output.
//!
//! 1. create a temporary file next to the destination (same file system,
//!    random name, exclusive creation, private permissions);
//! 2. write the data;
//! 3. flush and (optionally) `fsync`;
//! 4. verify the temporary file by reading it back;
//! 5. atomically move it into place — without replacing an existing file
//!    unless replacing was explicitly requested;
//! 6. the temporary file disappears in every case (moved, or removed on drop).
//!
//! The original input is never opened for writing; an interrupted write can at
//! worst leave a `.imago-*.tmp` file behind, which later jobs clean up.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tempfile::{Builder, NamedTempFile};

use crate::cancel::CancelToken;
use crate::errors::{ImagoError, Result};
use crate::filesystem::{TEMP_PREFIX, TEMP_SUFFIX};

/// How the final rename behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Fail with [`ImagoError::OutputExists`] if the destination exists.
    CreateNew,
    /// Atomically replace an existing destination.
    Replace,
}

/// Options for [`write_atomic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOptions {
    /// Rename behaviour.
    pub placement: Placement,
    /// Flush the data to disk before renaming.
    pub fsync: bool,
}

/// Writes `data` to `destination` atomically.
///
/// `verify` receives the path of the fully written temporary file and must
/// return `Ok` only if the file is correct.
pub fn write_atomic(
    destination: &Path,
    data: &[u8],
    options: WriteOptions,
    cancel: &CancelToken,
    verify: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    write_atomic_with(
        destination,
        options,
        cancel,
        |file| file.write_all(data),
        verify,
    )
}

/// Like [`write_atomic`], with a custom writer (used to stream large outputs
/// and to simulate I/O failures in tests).
pub fn write_atomic_with(
    destination: &Path,
    options: WriteOptions,
    cancel: &CancelToken,
    write: impl FnOnce(&mut File) -> io::Result<()>,
    verify: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let directory = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(directory).map_err(|error| ImagoError::io(directory, error))?;

    let mut temp = create_temp_file(directory)?;
    write(temp.as_file_mut()).map_err(|error| ImagoError::io(destination, error))?;
    temp.as_file_mut()
        .flush()
        .map_err(|error| ImagoError::io(destination, error))?;
    if options.fsync {
        temp.as_file()
            .sync_all()
            .map_err(|error| ImagoError::io(destination, error))?;
    }

    // Cancelled or invalid output: returning drops `temp`, which deletes it.
    cancel.check()?;
    verify(temp.path())?;
    cancel.check()?;

    match options.placement {
        Placement::CreateNew => temp.persist_noclobber(destination).map_err(|error| {
            if error.error.kind() == io::ErrorKind::AlreadyExists {
                ImagoError::OutputExists {
                    path: destination.to_path_buf(),
                }
            } else {
                ImagoError::io(destination, error.error)
            }
        })?,
        Placement::Replace => temp
            .persist(destination)
            .map_err(|error| ImagoError::io(destination, error.error))?,
    };
    sync_directory(directory, options.fsync);
    Ok(())
}

fn create_temp_file(directory: &Path) -> Result<NamedTempFile> {
    Builder::new()
        .prefix(TEMP_PREFIX)
        .suffix(TEMP_SUFFIX)
        .tempfile_in(directory)
        .map_err(|error| ImagoError::io(directory, error))
}

/// Makes the rename itself durable on Unix (no-op elsewhere).
fn sync_directory(directory: &Path, enabled: bool) {
    #[cfg(unix)]
    if enabled && let Ok(handle) = File::open(directory) {
        // Best effort: the data is already safe; this only protects the rename.
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = (directory, enabled);
}

/// Copies file permissions from the original (Unix); temporary files are
/// created private, which is wrong for images the user wants to share.
pub fn copy_permissions(from: &Path, to: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let permissions = std::fs::metadata(from)
            .map_err(|error| ImagoError::io(from, error))?
            .permissions();
        std::fs::set_permissions(to, permissions).map_err(|error| ImagoError::io(to, error))?;
    }
    #[cfg(not(unix))]
    let _ = (from, to);
    Ok(())
}

/// Copies the modification (and access) time of `from` to `to`.
pub fn copy_file_times(from: &Path, to: &Path) -> Result<()> {
    let metadata = std::fs::metadata(from).map_err(|error| ImagoError::io(from, error))?;
    let modified = metadata
        .modified()
        .map_err(|error| ImagoError::io(from, error))?;
    let mut times = std::fs::FileTimes::new().set_modified(modified);
    if let Ok(accessed) = metadata.accessed() {
        times = times.set_accessed(accessed);
    }
    let file = File::options()
        .write(true)
        .open(to)
        .map_err(|error| ImagoError::io(to, error))?;
    file.set_times(times)
        .map_err(|error| ImagoError::io(to, error))
}

/// Age after which leftover temporary files are considered abandoned.
pub const STALE_TEMP_AGE: Duration = Duration::from_secs(3600);

/// Cleans every output directory of stale temporary files once per job.
#[derive(Debug, Default)]
pub struct TempCleanup {
    cleaned: std::sync::Mutex<std::collections::HashSet<PathBuf>>,
}

impl TempCleanup {
    /// Removes stale temporary files from `directory` the first time it is seen.
    pub fn clean_once(&self, directory: &Path) {
        let first_time = self
            .cleaned
            .lock()
            .map(|mut cleaned| cleaned.insert(directory.to_path_buf()))
            .unwrap_or(false);
        if first_time {
            let removed = cleanup_stale_temp_files(directory, STALE_TEMP_AGE);
            if removed > 0 {
                tracing::info!(removed, directory = %directory.display(), "removed stale temporary files");
            }
        }
    }
}

/// Removes Imago temporary files older than `min_age` left in `directory` by
/// an interrupted process. Returns how many were removed.
pub fn cleanup_stale_temp_files(directory: &Path, min_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if !super::scan::is_temporary_file(&path) {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= min_age);
        if old_enough
            && entry.file_type().is_ok_and(|kind| kind.is_file())
            && std::fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(placement: Placement) -> WriteOptions {
        WriteOptions {
            placement,
            fsync: true,
        }
    }

    fn temp_files(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| super::super::scan::is_temporary_file(path))
            .collect()
    }

    #[test]
    fn writes_new_file_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested").join("out.bin");
        write_atomic(
            &target,
            b"hello",
            options(Placement::CreateNew),
            &CancelToken::new(),
            |temp| {
                assert_eq!(std::fs::read(temp).unwrap(), b"hello");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
        assert!(temp_files(target.parent().unwrap()).is_empty());
    }

    #[test]
    fn create_new_never_replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");
        std::fs::write(&target, b"keep me").unwrap();
        let error = write_atomic(
            &target,
            b"new",
            options(Placement::CreateNew),
            &CancelToken::new(),
            |_| Ok(()),
        )
        .unwrap_err();
        assert_eq!(error.code(), "output_exists");
        assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
        assert!(temp_files(dir.path()).is_empty());
    }

    #[test]
    fn replace_swaps_content_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");
        std::fs::write(&target, b"old").unwrap();
        write_atomic(
            &target,
            b"new",
            options(Placement::Replace),
            &CancelToken::new(),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn failed_verification_keeps_the_destination_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");
        std::fs::write(&target, b"original").unwrap();
        let error = write_atomic(
            &target,
            b"broken",
            options(Placement::Replace),
            &CancelToken::new(),
            |temp| {
                Err(ImagoError::VerificationFailed {
                    path: temp.to_path_buf(),
                    reason: "test".into(),
                })
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), "verification_failed");
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert!(temp_files(dir.path()).is_empty());
    }

    #[test]
    fn disk_full_is_reported_and_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");
        std::fs::write(&target, b"original").unwrap();
        let error = write_atomic_with(
            &target,
            options(Placement::Replace),
            &CancelToken::new(),
            |file| {
                file.write_all(b"partial")?;
                Err(io::Error::from(io::ErrorKind::StorageFull))
            },
            |_| Ok(()),
        )
        .unwrap_err();
        assert_eq!(error.code(), "disk_full");
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert!(temp_files(dir.path()).is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn real_enospc_from_dev_full_is_classified() {
        use std::fs::OpenOptions;
        let Ok(mut full) = OpenOptions::new().write(true).open("/dev/full") else {
            return;
        };
        let error = full
            .write_all(&[0u8; 4096])
            .and_then(|()| full.flush())
            .unwrap_err();
        assert_eq!(ImagoError::io("/dev/full", error).code(), "disk_full");
    }

    #[test]
    fn cancellation_discards_the_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");
        let token = CancelToken::new();
        token.cancel();
        let error = write_atomic(
            &target,
            b"data",
            options(Placement::CreateNew),
            &token,
            |_| Ok(()),
        )
        .unwrap_err();
        assert!(error.is_cancelled());
        assert!(!target.exists());
        assert!(temp_files(dir.path()).is_empty());
    }

    #[test]
    fn stale_temp_files_are_cleaned_only_when_old() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join(".imago-abc123.tmp");
        std::fs::write(&stale, b"x").unwrap();
        let unrelated = dir.path().join("keep.tmp");
        std::fs::write(&unrelated, b"x").unwrap();
        assert_eq!(
            cleanup_stale_temp_files(dir.path(), Duration::from_secs(3600)),
            0
        );
        assert_eq!(cleanup_stale_temp_files(dir.path(), Duration::ZERO), 1);
        assert!(!stale.exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn file_times_are_copied() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a");
        let target = dir.path().join("b");
        std::fs::write(&source, b"a").unwrap();
        std::fs::write(&target, b"b").unwrap();
        let old = SystemTime::now() - Duration::from_secs(86_400 * 30);
        File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
        copy_file_times(&source, &target).unwrap();
        let copied = std::fs::metadata(&target).unwrap().modified().unwrap();
        let difference = copied.duration_since(old).unwrap_or_else(|e| e.duration());
        assert!(difference < Duration::from_secs(2));
    }
}
