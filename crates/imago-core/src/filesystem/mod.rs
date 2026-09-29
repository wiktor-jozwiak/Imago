//! File-system layer: scanning inputs, computing output paths and writing
//! results safely.

pub mod paths;
pub mod safe_write;
pub mod scan;

pub use paths::{ClaimedPaths, Resolution, output_path};
pub use safe_write::{Placement, WriteOptions, write_atomic};
pub use scan::{InputFile, ScanOptions, ScanReport, scan_inputs};

/// Prefix of Imago's temporary files.
pub const TEMP_PREFIX: &str = ".imago-";
/// Suffix of Imago's temporary files.
pub const TEMP_SUFFIX: &str = ".tmp";
