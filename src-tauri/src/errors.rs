//! Errors returned to the frontend by IPC commands.

use std::path::Path;

use imago_core::ImagoError;
use serde::Serialize;

/// Serializable command failure. `code` is stable and translated by the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    /// Stable machine-readable code.
    pub code: String,
    /// English technical message (also logged).
    pub message: String,
    /// Suggested alternative, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative: Option<String>,
    /// How to fix the problem, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl CommandError {
    /// An unexpected failure in the adapter itself.
    pub fn internal(message: impl Into<String>) -> Self {
        let message = message.into();
        tracing::error!(%message, "command failed");
        CommandError {
            code: "internal_error".into(),
            message,
            alternative: None,
            hint: None,
        }
    }

    /// The frontend asked for a path it was never given.
    pub fn forbidden(path: &Path) -> Self {
        tracing::warn!(path = %path.display(), "rejected access to an unregistered path");
        CommandError {
            code: "path_not_allowed".into(),
            message: format!("{} was not added in this session", path.display()),
            alternative: None,
            hint: None,
        }
    }
}

impl From<ImagoError> for CommandError {
    fn from(error: ImagoError) -> Self {
        let info = error.to_info();
        tracing::warn!(code = info.code, message = %info.message, "command error");
        CommandError {
            code: info.code.to_owned(),
            message: info.message,
            alternative: info.alternative,
            hint: info.hint,
        }
    }
}

impl From<tauri::Error> for CommandError {
    fn from(error: tauri::Error) -> Self {
        CommandError::internal(error.to_string())
    }
}

/// Result type of IPC commands.
pub type CommandResult<T> = Result<T, CommandError>;
