//! Optimization settings shared by the GUI, the CLI and the configuration file.
//!
//! All structures serialize to camelCase JSON so the frontend can mirror them
//! one-to-one (see `src/types/imago.ts`).

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::{ImagoError, Result};

/// Image formats known to the engine (input and/or output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    /// JPEG / JPG.
    Jpeg,
    /// Portable Network Graphics (including APNG).
    Png,
    /// WebP (lossy, lossless, animated).
    Webp,
    /// AV1 Image File Format.
    Avif,
    /// Tagged Image File Format.
    Tiff,
    /// Windows bitmap (input only).
    Bmp,
    /// Graphics Interchange Format.
    Gif,
    /// JPEG XL.
    Jxl,
}

impl ImageFormat {
    /// Every known format.
    pub const ALL: [ImageFormat; 8] = [
        ImageFormat::Jpeg,
        ImageFormat::Png,
        ImageFormat::Webp,
        ImageFormat::Avif,
        ImageFormat::Tiff,
        ImageFormat::Bmp,
        ImageFormat::Gif,
        ImageFormat::Jxl,
    ];

    /// Human-readable name ("JPEG", "WebP", ...).
    pub fn display_name(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Png => "PNG",
            ImageFormat::Webp => "WebP",
            ImageFormat::Avif => "AVIF",
            ImageFormat::Tiff => "TIFF",
            ImageFormat::Bmp => "BMP",
            ImageFormat::Gif => "GIF",
            ImageFormat::Jxl => "JPEG XL",
        }
    }

    /// Canonical file extension without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
            ImageFormat::Webp => "webp",
            ImageFormat::Avif => "avif",
            ImageFormat::Tiff => "tif",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Gif => "gif",
            ImageFormat::Jxl => "jxl",
        }
    }

    /// All extensions commonly used for this format (lowercase, without the dot).
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            ImageFormat::Jpeg => &["jpg", "jpeg", "jpe", "jfif"],
            ImageFormat::Png => &["png", "apng"],
            ImageFormat::Webp => &["webp"],
            ImageFormat::Avif => &["avif"],
            ImageFormat::Tiff => &["tif", "tiff"],
            ImageFormat::Bmp => &["bmp", "dib"],
            ImageFormat::Gif => &["gif"],
            ImageFormat::Jxl => &["jxl"],
        }
    }

    /// Finds a format by file extension (case-insensitive, without the dot).
    pub fn from_extension(extension: &str) -> Option<ImageFormat> {
        let lower = extension.to_ascii_lowercase();
        ImageFormat::ALL
            .into_iter()
            .find(|format| format.extensions().contains(&lower.as_str()))
    }

    /// Parses a user-supplied format name ("jpg", "webp", "jpeg-xl", ...).
    pub fn parse(name: &str) -> Option<ImageFormat> {
        match name.trim().to_ascii_lowercase().as_str() {
            "jpeg-xl" | "jpegxl" | "jxl" => Some(ImageFormat::Jxl),
            other => ImageFormat::from_extension(other),
        }
    }

    /// MIME type.
    pub fn mime_type(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Png => "image/png",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Avif => "image/avif",
            ImageFormat::Tiff => "image/tiff",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Jxl => "image/jxl",
        }
    }

    /// Whether the format can store an alpha channel.
    pub fn supports_alpha(self) -> bool {
        !matches!(self, ImageFormat::Jpeg)
    }

    /// Whether the format can store an animation (as far as Imago can write it).
    pub fn supports_animation(self) -> bool {
        matches!(self, ImageFormat::Webp | ImageFormat::Gif)
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// How the output format is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", content = "format", rename_all = "camelCase")]
pub enum FormatChoice {
    /// Imago analyses each image and picks the smallest suitable format.
    #[default]
    Auto,
    /// Keep the input format (BMP input is written as lossless PNG).
    Original,
    /// Always write the given format.
    Fixed(ImageFormat),
}

/// Encoding quality from 1 (smallest) to 100 (best).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Quality(u8);

impl Quality {
    /// Default quality used across Imago.
    pub const DEFAULT: Quality = Quality(82);
    /// Highest quality.
    pub const MAX: Quality = Quality(100);

    /// Creates a quality value, rejecting anything outside 1..=100.
    pub fn new(value: u8) -> Result<Quality> {
        if (1..=100).contains(&value) {
            Ok(Quality(value))
        } else {
            Err(ImagoError::InvalidSettings(format!(
                "quality must be between 1 and 100, got {value}"
            )))
        }
    }

    /// The raw value (1..=100).
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for Quality {
    fn default() -> Self {
        Quality::DEFAULT
    }
}

impl TryFrom<u8> for Quality {
    type Error = ImagoError;

    fn try_from(value: u8) -> Result<Self> {
        Quality::new(value)
    }
}

impl From<Quality> for u8 {
    fn from(quality: Quality) -> u8 {
        quality.0
    }
}

/// How much CPU time encoders may spend to make files smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Effort {
    /// Fastest encoder settings.
    Fast,
    /// Good compression at reasonable speed.
    #[default]
    Balanced,
    /// Maximum compression ("maximum compression mode"); slowest.
    Max,
}

/// Compression parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CompressionSettings {
    /// Quality for lossy encoders.
    pub quality: Quality,
    /// Lossless encoding (pixels are preserved exactly).
    pub lossless: bool,
    /// Encoder effort.
    pub effort: Effort,
}

/// How the image dimensions change.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum ResizeMode {
    /// Keep the original dimensions.
    #[default]
    None,
    /// Target width and/or height. With `keep_aspect` and both values, the
    /// image fits inside the box; without `keep_aspect` it is stretched.
    #[serde(rename_all = "camelCase")]
    Dimensions {
        /// Target width in pixels.
        width: Option<u32>,
        /// Target height in pixels.
        height: Option<u32>,
        /// Preserve the aspect ratio.
        keep_aspect: bool,
    },
    /// Downscale only when the image exceeds these bounds (aspect is kept).
    #[serde(rename_all = "camelCase")]
    Fit {
        /// Maximum width in pixels.
        max_width: Option<u32>,
        /// Maximum height in pixels.
        max_height: Option<u32>,
    },
    /// Scale both dimensions by a percentage (e.g. 50.0).
    Percent {
        /// Scale factor in percent.
        percent: f32,
    },
}

/// Resizing parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ResizeSettings {
    /// Resize mode.
    pub mode: ResizeMode,
    /// Allow making images larger than the original.
    pub allow_upscale: bool,
}

/// Metadata handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MetadataSettings {
    /// Remove EXIF, XMP, comments and other non-essential metadata.
    pub strip_metadata: bool,
    /// Keep a non-sRGB ICC profile. When `false`, pixels are converted to
    /// sRGB before the profile is dropped, so colours stay correct.
    pub keep_icc: bool,
    /// Copy the original modification time to the output file.
    pub preserve_timestamps: bool,
    /// Rotate/flip pixels according to the EXIF orientation tag.
    pub auto_orient: bool,
}

impl Default for MetadataSettings {
    fn default() -> Self {
        MetadataSettings {
            strip_metadata: true,
            keep_icc: true,
            preserve_timestamps: false,
            auto_orient: true,
        }
    }
}

/// Where output files are written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OutputLocation {
    /// A subfolder (e.g. `optimized`) created next to the inputs.
    Subfolder {
        /// Folder name; must be a single path component.
        name: String,
    },
    /// A user-selected folder.
    Directory {
        /// Destination folder.
        path: PathBuf,
    },
    /// The same folder as each input file.
    SameFolder,
    /// One exact output file (single-file jobs only, used by the CLI). The
    /// extension is corrected when it does not match the output format.
    File {
        /// Output file path.
        path: PathBuf,
    },
}

impl Default for OutputLocation {
    fn default() -> Self {
        OutputLocation::Subfolder {
            name: DEFAULT_SUBFOLDER.to_owned(),
        }
    }
}

/// Default name of the automatically created output subfolder.
pub const DEFAULT_SUBFOLDER: &str = "optimized";

/// What happens when the output file already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    /// Pick a free name: `photo (1).webp`.
    #[default]
    Rename,
    /// Leave the existing file alone and mark the input as skipped.
    Skip,
    /// Replace the existing file (the GUI asks for confirmation first).
    Overwrite,
}

/// What to do when no candidate is smaller than the original file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WhenLarger {
    /// Write the original bytes instead (optionally with metadata removed
    /// losslessly). Applies to auto mode and to same-format optimization.
    #[default]
    KeepOriginal,
    /// Always write the re-encoded result ("force conversion").
    WriteAnyway,
}

/// Output file placement and naming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputSettings {
    /// Destination.
    pub location: OutputLocation,
    /// Suffix appended to the file stem, e.g. `-min`.
    pub suffix: String,
    /// Recreate the input folder structure below the destination.
    pub preserve_structure: bool,
    /// Existing-file policy.
    pub conflict: ConflictPolicy,
    /// Explicit permission to replace the original input files.
    pub allow_overwrite_originals: bool,
    /// Policy for results larger than the original.
    pub when_larger: WhenLarger,
    /// Flush file contents to disk before the atomic rename.
    pub fsync: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        OutputSettings {
            location: OutputLocation::default(),
            suffix: String::new(),
            preserve_structure: true,
            conflict: ConflictPolicy::default(),
            allow_overwrite_originals: false,
            when_larger: WhenLarger::default(),
            fsync: true,
        }
    }
}

/// What happens to animated images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AnimationPolicy {
    /// Keep the animation. If the chosen format cannot animate, the input
    /// format is kept and a warning is reported.
    #[default]
    Preserve,
    /// Use only the first frame (requires explicit user consent).
    FirstFrame,
}

/// What happens to transparency when the target format has no alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TransparencyPolicy {
    /// Never remove transparency; keep the input format instead and warn.
    #[default]
    Preserve,
    /// Flatten onto a white background (requires explicit user consent).
    FlattenWhite,
}

/// Safety limits applied before decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    /// Maximum input file size in bytes.
    pub max_file_bytes: u64,
    /// Maximum width or height in pixels.
    pub max_dimension: u32,
    /// Maximum number of pixels (width × height).
    pub max_pixels: u64,
    /// Maximum total pixels over all frames of an animation.
    pub max_animation_pixels: u64,
    /// Memory budget for concurrently decoded images, in bytes
    /// (`None` = a quarter of physical memory, at least 512 MiB).
    pub memory_budget_bytes: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_file_bytes: 512 * 1024 * 1024,
            max_dimension: 32_768,
            max_pixels: 150_000_000,
            max_animation_pixels: 1_000_000_000,
            memory_budget_bytes: None,
        }
    }
}

/// Complete description of how files are optimized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OptimizeSettings {
    /// Output format selection.
    pub format: FormatChoice,
    /// Compression parameters.
    pub compression: CompressionSettings,
    /// Resizing.
    pub resize: ResizeSettings,
    /// Metadata handling.
    pub metadata: MetadataSettings,
    /// Output placement and naming.
    pub output: OutputSettings,
    /// Animated inputs.
    pub animation: AnimationPolicy,
    /// Transparency when the target format has no alpha.
    pub transparency: TransparencyPolicy,
    /// Safety limits.
    pub limits: Limits,
    /// Number of worker threads (`None` = logical cores − 1, at least 1).
    pub workers: Option<usize>,
}

/// Upper bound for the worker count accepted from users.
pub const MAX_WORKERS: usize = 256;

impl OptimizeSettings {
    /// Checks that the settings are consistent.
    pub fn validate(&self) -> Result<()> {
        match self.resize.mode {
            ResizeMode::None => {}
            ResizeMode::Dimensions { width, height, .. } => {
                if width.is_none() && height.is_none() {
                    return Err(invalid("resize needs a width or a height"));
                }
                if width == Some(0) || height == Some(0) {
                    return Err(invalid("resize dimensions must be greater than zero"));
                }
            }
            ResizeMode::Fit {
                max_width,
                max_height,
            } => {
                if max_width.is_none() && max_height.is_none() {
                    return Err(invalid("fit needs a maximum width or height"));
                }
                if max_width == Some(0) || max_height == Some(0) {
                    return Err(invalid("maximum dimensions must be greater than zero"));
                }
            }
            ResizeMode::Percent { percent } => {
                if !percent.is_finite() || percent <= 0.0 || percent > 1000.0 {
                    return Err(invalid("scale must be between 0 and 1000 percent"));
                }
            }
        }

        match &self.output.location {
            OutputLocation::Subfolder { name } => validate_subfolder_name(name)?,
            OutputLocation::Directory { path } => {
                if path.as_os_str().is_empty() {
                    return Err(invalid("output folder must not be empty"));
                }
            }
            OutputLocation::SameFolder => {}
            OutputLocation::File { path } => {
                if path.file_name().is_none() {
                    return Err(invalid("output file must have a file name"));
                }
            }
        }

        validate_suffix(&self.output.suffix)?;

        if let Some(workers) = self.workers
            && (workers == 0 || workers > MAX_WORKERS)
        {
            return Err(invalid(format!(
                "worker count must be between 1 and {MAX_WORKERS}"
            )));
        }

        let limits = &self.limits;
        if limits.max_file_bytes == 0 || limits.max_dimension == 0 || limits.max_pixels == 0 {
            return Err(invalid("limits must be greater than zero"));
        }
        Ok(())
    }

    /// Effective number of worker threads.
    pub fn effective_workers(&self) -> usize {
        self.workers
            .unwrap_or_else(default_workers)
            .clamp(1, MAX_WORKERS)
    }
}

/// Default worker count: logical cores minus one, at least one.
pub fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|cores| cores.get().saturating_sub(1))
        .unwrap_or(1)
        .max(1)
}

fn invalid(message: impl Into<String>) -> ImagoError {
    ImagoError::InvalidSettings(message.into())
}

/// Characters that are invalid in file names on at least one supported OS.
const FORBIDDEN_NAME_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

fn validate_subfolder_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(invalid("output subfolder name must not be empty"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(invalid("output subfolder name must be a plain folder name"));
    }
    if trimmed
        .chars()
        .any(|c| FORBIDDEN_NAME_CHARS.contains(&c) || c.is_control())
    {
        return Err(invalid(format!(
            "output subfolder name contains forbidden characters: {name}"
        )));
    }
    Ok(())
}

fn validate_suffix(suffix: &str) -> Result<()> {
    if suffix.len() > 64 {
        return Err(invalid("file name suffix is too long (max 64 bytes)"));
    }
    if suffix
        .chars()
        .any(|c| FORBIDDEN_NAME_CHARS.contains(&c) || c.is_control())
    {
        return Err(invalid(format!(
            "file name suffix contains forbidden characters: {suffix}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_specification() {
        let settings = OptimizeSettings::default();
        assert_eq!(settings.compression.quality.get(), 82);
        assert_eq!(settings.format, FormatChoice::Auto);
        assert!(settings.metadata.strip_metadata);
        assert!(settings.metadata.auto_orient);
        assert_eq!(
            settings.output.location,
            OutputLocation::Subfolder {
                name: "optimized".into()
            }
        );
        assert!(!settings.output.allow_overwrite_originals);
        assert_eq!(settings.animation, AnimationPolicy::Preserve);
        assert_eq!(settings.transparency, TransparencyPolicy::Preserve);
        assert!(settings.effective_workers() >= 1);
        settings.validate().unwrap();
    }

    #[test]
    fn default_workers_leaves_one_core_free() {
        let cores = std::thread::available_parallelism().unwrap().get();
        assert_eq!(default_workers(), cores.saturating_sub(1).max(1));
    }

    #[test]
    fn quality_rejects_out_of_range_values() {
        assert!(Quality::new(0).is_err());
        assert!(Quality::new(101).is_err());
        assert_eq!(Quality::new(1).unwrap().get(), 1);
        assert_eq!(Quality::new(100).unwrap().get(), 100);
    }

    #[test]
    fn quality_deserialization_is_validated() {
        assert!(serde_json::from_str::<Quality>("0").is_err());
        assert_eq!(serde_json::from_str::<Quality>("70").unwrap().get(), 70);
    }

    #[test]
    fn formats_are_found_by_extension_case_insensitively() {
        assert_eq!(ImageFormat::from_extension("JPG"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("TIFF"), Some(ImageFormat::Tiff));
        assert_eq!(ImageFormat::from_extension("heic"), None);
        assert_eq!(ImageFormat::parse("jpeg-xl"), Some(ImageFormat::Jxl));
        assert_eq!(ImageFormat::parse(" WebP "), Some(ImageFormat::Webp));
    }

    #[test]
    fn only_jpeg_lacks_alpha_and_only_webp_gif_animate() {
        for format in ImageFormat::ALL {
            assert_eq!(format.supports_alpha(), format != ImageFormat::Jpeg);
        }
        assert!(ImageFormat::Webp.supports_animation());
        assert!(ImageFormat::Gif.supports_animation());
        assert!(!ImageFormat::Avif.supports_animation());
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = OptimizeSettings {
            format: FormatChoice::Fixed(ImageFormat::Avif),
            resize: ResizeSettings {
                mode: ResizeMode::Fit {
                    max_width: Some(2400),
                    max_height: None,
                },
                allow_upscale: false,
            },
            ..OptimizeSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#""format":{"mode":"fixed","format":"avif"}"#));
        assert!(json.contains(r#""maxWidth":2400"#));
        let back: OptimizeSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, settings);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let settings: OptimizeSettings = serde_json::from_str(r#"{"workers": 3}"#).unwrap();
        assert_eq!(settings.workers, Some(3));
        assert_eq!(settings.compression.quality.get(), 82);
    }

    #[test]
    fn invalid_settings_are_rejected() {
        let mut settings = OptimizeSettings::default();
        settings.resize.mode = ResizeMode::Dimensions {
            width: None,
            height: None,
            keep_aspect: true,
        };
        assert!(settings.validate().is_err());

        let mut settings = OptimizeSettings::default();
        settings.resize.mode = ResizeMode::Percent { percent: 0.0 };
        assert!(settings.validate().is_err());

        let mut settings = OptimizeSettings::default();
        settings.output.location = OutputLocation::Subfolder { name: "..".into() };
        assert!(settings.validate().is_err());

        let mut settings = OptimizeSettings::default();
        settings.output.location = OutputLocation::Subfolder { name: "a/b".into() };
        assert!(settings.validate().is_err());

        let mut settings = OptimizeSettings::default();
        settings.output.suffix = "../x".into();
        assert!(settings.validate().is_err());

        let settings = OptimizeSettings {
            workers: Some(0),
            ..OptimizeSettings::default()
        };
        assert!(settings.validate().is_err());
    }
}
