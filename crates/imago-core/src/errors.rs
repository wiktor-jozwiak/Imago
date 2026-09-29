//! Standardized errors.
//!
//! Every failure that can reach a user is an [`ImagoError`] variant with a
//! stable machine-readable [`code`](ImagoError::code). The GUI translates codes
//! into localized messages; the full technical message goes to the log.

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Result type used across the engine.
pub type Result<T, E = ImagoError> = std::result::Result<T, E>;

/// All errors produced by the Imago engine.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImagoError {
    /// The input path does not exist.
    #[error("file not found: {}", path.display())]
    NotFound {
        /// Path that was not found.
        path: PathBuf,
    },

    /// The process is not allowed to read or write the path.
    #[error("permission denied: {}", path.display())]
    PermissionDenied {
        /// Path that could not be accessed.
        path: PathBuf,
    },

    /// The destination file system is full (or a quota was exceeded).
    #[error("not enough disk space to write {}", path.display())]
    DiskFull {
        /// Path that was being written.
        path: PathBuf,
    },

    /// Any other I/O error.
    #[error("I/O error on {}: {source}", path.display())]
    Io {
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },

    /// The input exists but is not a regular file (directory, device, ...).
    #[error("not a regular file: {}", path.display())]
    NotAFile {
        /// Offending path.
        path: PathBuf,
    },

    /// The input file exceeds the configured size limit.
    #[error("file is too large ({size} bytes, limit is {limit} bytes)")]
    FileTooLarge {
        /// Actual size in bytes.
        size: u64,
        /// Configured limit in bytes.
        limit: u64,
    },

    /// The image dimensions exceed the configured limits.
    #[error("image dimensions {width}x{height} exceed the limit: {reason}")]
    DimensionsTooLarge {
        /// Image width in pixels.
        width: u32,
        /// Image height in pixels.
        height: u32,
        /// Which limit was exceeded.
        reason: String,
    },

    /// The file content is not a supported image format.
    #[error("unsupported file format{}", detected_suffix(.detected))]
    UnsupportedFormat {
        /// Detected format name, if the signature was recognized.
        detected: Option<String>,
    },

    /// The format is known but its codec is not available in this build.
    #[error("{format} is not available in this build: {reason}")]
    FormatUnavailable {
        /// Format name.
        format: String,
        /// Why it is unavailable.
        reason: String,
        /// Suggested alternative format.
        alternative: Option<String>,
        /// How to enable the format.
        how_to_enable: Option<String>,
    },

    /// Decoding failed (corrupted or truncated file, unsupported variant).
    #[error("could not decode {format} image: {message}")]
    Decode {
        /// Input format.
        format: String,
        /// Decoder message.
        message: String,
    },

    /// Encoding failed.
    #[error("could not encode {format} image: {message}")]
    Encode {
        /// Output format.
        format: String,
        /// Encoder message.
        message: String,
    },

    /// The output file exists and the conflict policy does not allow replacing it.
    #[error("output file already exists: {}", path.display())]
    OutputExists {
        /// Existing output path.
        path: PathBuf,
    },

    /// Writing would replace the original input without explicit permission.
    #[error("refusing to overwrite the original file {} without explicit permission", path.display())]
    WouldOverwriteOriginal {
        /// The protected original.
        path: PathBuf,
    },

    /// A computed path tried to escape its root directory.
    #[error("unsafe path rejected: {}", path.display())]
    PathTraversal {
        /// The rejected path.
        path: PathBuf,
    },

    /// The freshly written file could not be read back correctly.
    #[error("written file {} failed verification: {reason}", path.display())]
    VerificationFailed {
        /// Path of the written (temporary) file.
        path: PathBuf,
        /// What did not match.
        reason: String,
    },

    /// Settings are inconsistent or out of range.
    #[error("invalid settings: {0}")]
    InvalidSettings(String),

    /// The operation was cancelled by the user.
    #[error("operation cancelled")]
    Cancelled,

    /// Creating a ZIP archive failed.
    #[error("archive error: {0}")]
    Archive(String),

    /// Loading or saving the configuration failed.
    #[error("configuration error: {0}")]
    Config(String),

    /// A bug or an unexpected panic inside a codec.
    #[error("internal error: {0}")]
    Internal(String),
}

fn detected_suffix(detected: &Option<String>) -> String {
    detected
        .as_deref()
        .map(|name| format!(" ({name})"))
        .unwrap_or_default()
}

/// Serializable error description sent to the user interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    /// Stable machine-readable code, e.g. `disk_full`.
    pub code: &'static str,
    /// Human-readable English message with technical details.
    pub message: String,
    /// Suggested alternative (for unavailable formats).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative: Option<String>,
    /// How to fix or enable something, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ImagoError {
    /// Classifies an [`io::Error`] into the most specific variant.
    pub fn io(path: impl AsRef<Path>, source: io::Error) -> Self {
        let path = path.as_ref().to_path_buf();
        match source.kind() {
            io::ErrorKind::NotFound => Self::NotFound { path },
            io::ErrorKind::PermissionDenied => Self::PermissionDenied { path },
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => Self::DiskFull { path },
            _ if is_disk_full_os_error(&source) => Self::DiskFull { path },
            _ => Self::Io { path, source },
        }
    }

    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::PermissionDenied { .. } => "permission_denied",
            Self::DiskFull { .. } => "disk_full",
            Self::Io { .. } => "io_error",
            Self::NotAFile { .. } => "not_a_file",
            Self::FileTooLarge { .. } => "file_too_large",
            Self::DimensionsTooLarge { .. } => "dimensions_too_large",
            Self::UnsupportedFormat { .. } => "unsupported_format",
            Self::FormatUnavailable { .. } => "format_unavailable",
            Self::Decode { .. } => "decode_failed",
            Self::Encode { .. } => "encode_failed",
            Self::OutputExists { .. } => "output_exists",
            Self::WouldOverwriteOriginal { .. } => "would_overwrite_original",
            Self::PathTraversal { .. } => "path_traversal",
            Self::VerificationFailed { .. } => "verification_failed",
            Self::InvalidSettings(_) => "invalid_settings",
            Self::Cancelled => "cancelled",
            Self::Archive(_) => "archive_error",
            Self::Config(_) => "config_error",
            Self::Internal(_) => "internal_error",
        }
    }

    /// Whether this error represents a user cancellation rather than a failure.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    /// Converts the error into a serializable description for the UI.
    pub fn to_info(&self) -> ErrorInfo {
        let (alternative, hint) = match self {
            Self::FormatUnavailable {
                alternative,
                how_to_enable,
                ..
            } => (alternative.clone(), how_to_enable.clone()),
            _ => (None, None),
        };
        ErrorInfo {
            code: self.code(),
            message: self.to_string(),
            alternative,
            hint,
        }
    }
}

/// Detects "disk full" OS error codes that std does not map to `StorageFull`.
fn is_disk_full_os_error(error: &io::Error) -> bool {
    // ERROR_HANDLE_DISK_FULL (39) and ERROR_DISK_FULL (112) on Windows,
    // ENOSPC (28) and EDQUOT (122 on Linux, 69 on macOS) on Unix.
    #[cfg(windows)]
    const CODES: &[i32] = &[39, 112];
    #[cfg(target_os = "linux")]
    const CODES: &[i32] = &[28, 122];
    #[cfg(target_os = "macos")]
    const CODES: &[i32] = &[28, 69];
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    const CODES: &[i32] = &[28];
    error
        .raw_os_error()
        .is_some_and(|code| CODES.contains(&code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_are_classified() {
        let not_found = ImagoError::io("a.png", io::Error::from(io::ErrorKind::NotFound));
        assert_eq!(not_found.code(), "not_found");

        let denied = ImagoError::io("a.png", io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(denied.code(), "permission_denied");

        let full = ImagoError::io("a.png", io::Error::from(io::ErrorKind::StorageFull));
        assert_eq!(full.code(), "disk_full");

        let other = ImagoError::io("a.png", io::Error::other("boom"));
        assert_eq!(other.code(), "io_error");
    }

    #[cfg(windows)]
    #[test]
    fn windows_disk_full_code_is_recognized() {
        let error = ImagoError::io("a.png", io::Error::from_raw_os_error(112));
        assert_eq!(error.code(), "disk_full");
    }

    #[test]
    fn unavailable_format_carries_alternative_and_hint() {
        let error = ImagoError::FormatUnavailable {
            format: "AVIF".into(),
            reason: "built without the `avif` feature".into(),
            alternative: Some("WebP".into()),
            how_to_enable: Some("rebuild with `--features avif`".into()),
        };
        let info = error.to_info();
        assert_eq!(info.code, "format_unavailable");
        assert_eq!(info.alternative.as_deref(), Some("WebP"));
        assert!(info.hint.is_some());
        assert!(info.message.contains("AVIF"));
    }

    #[test]
    fn unsupported_format_message_mentions_detected_format() {
        let error = ImagoError::UnsupportedFormat {
            detected: Some("HEIC".into()),
        };
        assert_eq!(error.to_string(), "unsupported file format (HEIC)");
        let unknown = ImagoError::UnsupportedFormat { detected: None };
        assert_eq!(unknown.to_string(), "unsupported file format");
    }

    #[test]
    fn cancellation_is_not_a_failure() {
        assert!(ImagoError::Cancelled.is_cancelled());
        assert!(!ImagoError::Internal("x".into()).is_cancelled());
    }
}
