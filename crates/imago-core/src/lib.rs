//! # imago-core
//!
//! The engine behind **Imago — Image Optimizer** ("Fast. Local. Smaller images.").
//!
//! Everything that touches pixels, files or jobs lives here, so the desktop
//! application (Tauri) and the `imago` command-line tool share exactly the same
//! behaviour and data structures. This crate never talks to the network and
//! never spawns external processes.

#![warn(missing_docs)]

pub mod app;
pub mod archive;
pub mod cancel;
pub mod codecs;
pub mod config;
pub mod errors;
pub mod filesystem;
pub mod image_engine;
pub mod jobs;
pub mod metadata;
pub mod preview;
pub mod settings;
pub mod workers;

pub use cancel::CancelToken;
pub use errors::{ErrorInfo, ImagoError, Result};
pub use settings::{FormatChoice, ImageFormat, OptimizeSettings, Quality};

use serde::Serialize;

/// Short product name.
pub const APP_NAME: &str = "Imago";
/// Full product name.
pub const APP_FULL_NAME: &str = "Imago Image Optimizer";
/// English tagline.
pub const TAGLINE: &str = "Fast. Local. Smaller images.";
/// Polish tagline.
pub const TAGLINE_PL: &str = "Szybsze obrazy. Mniejsze pliki. Zero chmury.";
/// Engine version (shared by the GUI and the CLI).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Static information about the application, shown in the UI and in `--version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Short product name ("Imago").
    pub name: &'static str,
    /// Full product name.
    pub full_name: &'static str,
    /// English tagline.
    pub tagline: &'static str,
    /// Polish tagline.
    pub tagline_pl: &'static str,
    /// Engine version.
    pub version: &'static str,
}

/// Returns static information about the application.
pub fn app_info() -> AppInfo {
    AppInfo {
        name: APP_NAME,
        full_name: APP_FULL_NAME,
        tagline: TAGLINE,
        tagline_pl: TAGLINE_PL,
        version: VERSION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_uses_product_name() {
        let info = app_info();
        assert_eq!(info.name, "Imago");
        assert_eq!(info.full_name, "Imago Image Optimizer");
        assert_eq!(info.tagline, "Fast. Local. Smaller images.");
        assert!(!info.version.is_empty());
    }
}
