//! Command-line arguments and their translation into engine settings.

use std::path::{Path, PathBuf};

use clap::{ArgAction, Parser};
use imago_core::filesystem::ScanOptions;
use imago_core::settings::{
    AnimationPolicy, CompressionSettings, ConflictPolicy, Effort, FormatChoice, ImageFormat,
    LargeImages, Limits, MetadataSettings, OptimizeSettings, OutputLocation, OutputSettings,
    Quality, ResizeMode, ResizeSettings, TransparencyPolicy, WhenLarger,
};

const EXAMPLES: &str = "\
Examples:
  imago input.jpg --output optimized.jpg --format webp --quality 82
  imago ./photos --output ./optimized --format auto --quality 82
  imago ./photos --output ./optimized --recursive --workers 8
  imago ./photos --output ./optimized --width 2400 --remove-metadata
  imago ./photos --output ./optimized --format avif --quality 70
  imago ./photos --output ./optimized --lossless
  imago ./photos --output ./optimized --skip-existing
  imago ./photos --zip photos.zip

Exit codes:
  0  every file was optimized or skipped
  1  some files failed
  2  invalid arguments
  3  nothing to do or a fatal error (no images found, invalid settings, ZIP failed)
  130 cancelled with Ctrl+C

All processing happens locally. Imago never uploads your images.";

/// Imago Image Optimizer — Fast. Local. Smaller images.
///
/// Compresses, converts and resizes images on your computer.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "imago",
    bin_name = "imago",
    version = imago_core::VERSION,
    about = "Imago Image Optimizer — Fast. Local. Smaller images.",
    after_help = EXAMPLES,
    max_term_width = 100
)]
pub struct Cli {
    /// Image files and/or folders to optimize.
    #[arg(value_name = "INPUT", required_unless_present = "list_formats")]
    pub inputs: Vec<PathBuf>,

    // ---------------------------------------------------------------- output
    /// Output folder; with a single input file this may also be the output file name.
    /// Default: an `optimized` folder next to the inputs.
    #[arg(short, long, value_name = "PATH", help_heading = "Output")]
    pub output: Option<PathBuf>,

    /// Text added to every output file name, e.g. `-min`.
    #[arg(long, value_name = "TEXT", help_heading = "Output")]
    pub suffix: Option<String>,

    /// Put all outputs into one folder instead of recreating subfolders.
    #[arg(long, help_heading = "Output")]
    pub flat: bool,

    /// Leave existing output files alone and skip those inputs.
    #[arg(long, conflicts_with = "overwrite", help_heading = "Output")]
    pub skip_existing: bool,

    /// Replace existing output files (default: pick a new name such as `photo (1).webp`).
    #[arg(long, help_heading = "Output")]
    pub overwrite: bool,

    /// Allow replacing the input files themselves (requires --overwrite).
    #[arg(long, requires = "overwrite", help_heading = "Output")]
    pub allow_overwrite_originals: bool,

    /// Write the re-encoded file even when it is larger than the original.
    #[arg(long, help_heading = "Output")]
    pub force: bool,

    /// Also pack all optimized files into this ZIP archive.
    #[arg(long, value_name = "FILE", help_heading = "Output")]
    pub zip: Option<PathBuf>,

    /// Skip the disk flush before files are moved into place (faster, less safe).
    #[arg(long, help_heading = "Output")]
    pub no_fsync: bool,

    // ------------------------------------------------------------ format
    /// Output format: auto, original, jpeg, png, webp, avif, tiff, gif or jxl.
    #[arg(short, long, value_name = "FORMAT", value_parser = parse_format, help_heading = "Format and quality")]
    pub format: Option<FormatChoice>,

    /// Quality from 1 (smallest) to 100 (best).
    #[arg(long, value_name = "1-100", default_value_t = 82,
          value_parser = clap::value_parser!(u8).range(1..=100), help_heading = "Format and quality")]
    pub quality: u8,

    /// Lossless compression (pixels are kept exactly).
    #[arg(long, help_heading = "Format and quality")]
    pub lossless: bool,

    /// Spend much more time to squeeze out extra bytes.
    #[arg(long, conflicts_with = "fast", help_heading = "Format and quality")]
    pub max_compression: bool,

    /// Fastest encoder settings.
    #[arg(long, help_heading = "Format and quality")]
    pub fast: bool,

    /// Keep animations only as their first frame.
    #[arg(long, help_heading = "Format and quality")]
    pub first_frame: bool,

    /// Allow replacing transparency with a white background (e.g. for JPEG).
    #[arg(long, help_heading = "Format and quality")]
    pub flatten: bool,

    // ------------------------------------------------------------ resize
    /// Target width in pixels (the height follows unless --stretch).
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..), help_heading = "Resize")]
    pub width: Option<u32>,

    /// Target height in pixels (the width follows unless --stretch).
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..), help_heading = "Resize")]
    pub height: Option<u32>,

    /// With both --width and --height: stretch instead of keeping proportions.
    #[arg(long, help_heading = "Resize")]
    pub stretch: bool,

    /// Shrink images wider than this (proportions kept).
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..),
          conflicts_with_all = ["width", "height"], help_heading = "Resize")]
    pub max_width: Option<u32>,

    /// Shrink images taller than this (proportions kept).
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..),
          conflicts_with_all = ["width", "height"], help_heading = "Resize")]
    pub max_height: Option<u32>,

    /// Scale by a percentage, e.g. 50.
    #[arg(long, value_name = "PERCENT", conflicts_with_all = ["width", "height", "max_width", "max_height"],
          help_heading = "Resize")]
    pub scale: Option<f32>,

    /// Allow making images larger than the original.
    #[arg(long, help_heading = "Resize")]
    pub allow_upscale: bool,

    // ---------------------------------------------------------- metadata
    /// Remove EXIF, XMP and comments (default).
    #[arg(long, conflicts_with = "keep_metadata", help_heading = "Metadata")]
    pub remove_metadata: bool,

    /// Keep EXIF metadata (GPS, camera data, ...).
    #[arg(long, help_heading = "Metadata")]
    pub keep_metadata: bool,

    /// Convert wide-gamut images to sRGB and drop their color profile.
    #[arg(long, help_heading = "Metadata")]
    pub strip_icc: bool,

    /// Copy the modification date of each input to its output.
    #[arg(long, help_heading = "Metadata")]
    pub preserve_timestamps: bool,

    /// Do not rotate pixels according to the EXIF orientation.
    #[arg(long, help_heading = "Metadata")]
    pub no_auto_orient: bool,

    // ------------------------------------------------------------ inputs
    /// Include images in subfolders.
    #[arg(short, long, help_heading = "Inputs")]
    pub recursive: bool,

    /// Include hidden files and folders.
    #[arg(long, help_heading = "Inputs")]
    pub include_hidden: bool,

    // ------------------------------------------------------- performance
    /// Number of worker threads (default: logical cores − 1).
    #[arg(short = 'j', long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..=256),
          help_heading = "Performance and limits")]
    pub workers: Option<u16>,

    /// Largest input file in megabytes.
    #[arg(long, value_name = "MB", value_parser = clap::value_parser!(u64).range(1..),
          help_heading = "Performance and limits")]
    pub max_file_size: Option<u64>,

    /// Largest image in megapixels.
    #[arg(long, value_name = "MP", value_parser = clap::value_parser!(u64).range(1..),
          help_heading = "Performance and limits")]
    pub max_megapixels: Option<u64>,

    /// Memory for images decoded at the same time, in megabytes.
    #[arg(long, value_name = "MB", value_parser = clap::value_parser!(u64).range(64..),
          help_heading = "Performance and limits")]
    pub memory_budget: Option<u64>,

    /// Reject images above the limits instead of processing JPEG and PNG
    /// row by row (large-image mode).
    #[arg(long, help_heading = "Performance and limits")]
    pub no_large_image_mode: bool,

    // --------------------------------------------------------- reporting
    /// Print only errors.
    #[arg(short, long, conflicts_with = "verbose", help_heading = "Reporting")]
    pub quiet: bool,

    /// More details (-v: decisions and warnings, -vv: debug log).
    #[arg(short, long, action = ArgAction::Count, help_heading = "Reporting")]
    pub verbose: u8,

    /// Print a JSON report to standard output.
    #[arg(long, help_heading = "Reporting")]
    pub json: bool,

    /// List the supported formats and exit.
    #[arg(long, help_heading = "Reporting")]
    pub list_formats: bool,
}

fn parse_format(value: &str) -> Result<FormatChoice, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(FormatChoice::Auto),
        "original" | "keep" | "same" => Ok(FormatChoice::Original),
        other => ImageFormat::parse(other).map(FormatChoice::Fixed).ok_or_else(|| {
            format!("unknown format `{value}` (use auto, original, jpeg, png, webp, avif, tiff, gif or jxl)")
        }),
    }
}

/// Settings derived from the command line.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Engine settings.
    pub settings: OptimizeSettings,
    /// Folder scanning options.
    pub scan: ScanOptions,
}

impl Cli {
    /// Builds engine settings. `single_file` tells whether exactly one input
    /// file (not folder) was given.
    pub fn plan(&self, single_file: bool) -> Result<Plan, String> {
        let output_file = self
            .output
            .as_deref()
            .filter(|output| single_file && looks_like_file(output));
        let format = match (self.format, output_file) {
            (Some(format), _) => format,
            (None, Some(file)) => file
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(ImageFormat::from_extension)
                .map_or(FormatChoice::Auto, FormatChoice::Fixed),
            (None, None) => FormatChoice::Auto,
        };

        let location = match (&self.output, output_file) {
            (_, Some(file)) => OutputLocation::File {
                path: file.to_path_buf(),
            },
            (Some(folder), None) => OutputLocation::Directory {
                path: folder.clone(),
            },
            (None, None) => OutputLocation::default(),
        };

        let resize_mode = if let Some(percent) = self.scale {
            ResizeMode::Percent { percent }
        } else if self.max_width.is_some() || self.max_height.is_some() {
            ResizeMode::Fit {
                max_width: self.max_width,
                max_height: self.max_height,
            }
        } else if self.width.is_some() || self.height.is_some() {
            ResizeMode::Dimensions {
                width: self.width,
                height: self.height,
                keep_aspect: !self.stretch,
            }
        } else {
            ResizeMode::None
        };

        let defaults = Limits::default();
        let settings = OptimizeSettings {
            format,
            compression: CompressionSettings {
                quality: Quality::new(self.quality).map_err(|error| error.to_string())?,
                lossless: self.lossless,
                effort: if self.max_compression {
                    Effort::Max
                } else if self.fast {
                    Effort::Fast
                } else {
                    Effort::Balanced
                },
            },
            resize: ResizeSettings {
                mode: resize_mode,
                allow_upscale: self.allow_upscale,
            },
            metadata: MetadataSettings {
                strip_metadata: !self.keep_metadata,
                keep_icc: !self.strip_icc,
                preserve_timestamps: self.preserve_timestamps,
                auto_orient: !self.no_auto_orient,
            },
            output: OutputSettings {
                location,
                suffix: self.suffix.clone().unwrap_or_default(),
                preserve_structure: !self.flat,
                conflict: if self.skip_existing {
                    ConflictPolicy::Skip
                } else if self.overwrite {
                    ConflictPolicy::Overwrite
                } else {
                    ConflictPolicy::Rename
                },
                allow_overwrite_originals: self.allow_overwrite_originals,
                when_larger: if self.force {
                    WhenLarger::WriteAnyway
                } else {
                    WhenLarger::KeepOriginal
                },
                fsync: !self.no_fsync,
            },
            animation: if self.first_frame {
                AnimationPolicy::FirstFrame
            } else {
                AnimationPolicy::Preserve
            },
            transparency: if self.flatten {
                TransparencyPolicy::FlattenWhite
            } else {
                TransparencyPolicy::Preserve
            },
            limits: Limits {
                max_file_bytes: self
                    .max_file_size
                    .map_or(defaults.max_file_bytes, |mb| mb.saturating_mul(1024 * 1024)),
                max_pixels: self
                    .max_megapixels
                    .map_or(defaults.max_pixels, |mp| mp.saturating_mul(1_000_000)),
                memory_budget_bytes: self.memory_budget.map(|mb| mb.saturating_mul(1024 * 1024)),
                ..defaults
            },
            large_images: if self.no_large_image_mode {
                LargeImages::Reject
            } else {
                LargeImages::Stream
            },
            workers: self.workers.map(usize::from),
        };
        settings.validate().map_err(|error| error.to_string())?;

        Ok(Plan {
            settings,
            scan: ScanOptions {
                recursive: self.recursive,
                include_hidden: self.include_hidden,
                exclude_dir_names: Vec::new(),
            },
        })
    }
}

/// A single-file `--output` is treated as a file name when it has an image
/// extension and is not an existing folder.
fn looks_like_file(path: &Path) -> bool {
    !path.is_dir()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| ImageFormat::from_extension(extension).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("imago").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn defaults_follow_the_specification() {
        let plan = parse(&["photos"]).plan(false).unwrap();
        let settings = plan.settings;
        assert_eq!(settings.format, FormatChoice::Auto);
        assert_eq!(settings.compression.quality.get(), 82);
        assert!(settings.metadata.strip_metadata);
        assert_eq!(settings.output.location, OutputLocation::default());
        assert!(!plan.scan.recursive);
    }

    #[test]
    fn single_file_output_name_sets_the_format() {
        let cli = parse(&["in.png", "--output", "out/result.webp"]);
        let plan = cli.plan(true).unwrap();
        assert_eq!(plan.settings.format, FormatChoice::Fixed(ImageFormat::Webp));
        assert_eq!(
            plan.settings.output.location,
            OutputLocation::File {
                path: PathBuf::from("out/result.webp")
            }
        );
        // An explicit format wins over the extension.
        let plan = parse(&["in.png", "-o", "x.jpg", "--format", "webp"])
            .plan(true)
            .unwrap();
        assert_eq!(plan.settings.format, FormatChoice::Fixed(ImageFormat::Webp));
    }

    #[test]
    fn folder_output_is_a_directory() {
        let plan = parse(&["photos", "--output", "optimized.jpg"])
            .plan(false)
            .unwrap();
        assert!(matches!(
            plan.settings.output.location,
            OutputLocation::Directory { .. }
        ));
    }

    #[test]
    fn resize_options_map_to_modes() {
        let width = parse(&["a", "--width", "2400"]).plan(false).unwrap();
        assert_eq!(
            width.settings.resize.mode,
            ResizeMode::Dimensions {
                width: Some(2400),
                height: None,
                keep_aspect: true
            }
        );
        let fit = parse(&["a", "--max-width", "100", "--max-height", "50"])
            .plan(false)
            .unwrap();
        assert!(matches!(fit.settings.resize.mode, ResizeMode::Fit { .. }));
        let scale = parse(&["a", "--scale", "50"]).plan(false).unwrap();
        assert_eq!(
            scale.settings.resize.mode,
            ResizeMode::Percent { percent: 50.0 }
        );
        assert!(parse(&["a", "--scale", "0"]).plan(false).is_err());
    }

    #[test]
    fn conflicting_options_are_rejected_by_the_parser() {
        let parse_err = |args: &[&str]| {
            Cli::try_parse_from(std::iter::once("imago").chain(args.iter().copied())).is_err()
        };
        assert!(parse_err(&["a", "--skip-existing", "--overwrite"]));
        assert!(parse_err(&["a", "--quality", "0"]));
        assert!(parse_err(&["a", "--quality", "101"]));
        assert!(parse_err(&["a", "--format", "bogus"]));
        assert!(parse_err(&["a", "--allow-overwrite-originals"]));
        assert!(parse_err(&["a", "--width", "10", "--max-width", "5"]));
        assert!(parse_err(&[]));
    }

    #[test]
    fn formats_parse_with_aliases() {
        assert_eq!(
            parse_format("jpg"),
            Ok(FormatChoice::Fixed(ImageFormat::Jpeg))
        );
        assert_eq!(
            parse_format("JPEG-XL"),
            Ok(FormatChoice::Fixed(ImageFormat::Jxl))
        );
        assert_eq!(parse_format("original"), Ok(FormatChoice::Original));
        assert_eq!(parse_format("AUTO"), Ok(FormatChoice::Auto));
    }
}
