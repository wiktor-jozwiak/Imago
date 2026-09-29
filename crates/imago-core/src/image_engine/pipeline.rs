//! The per-file pipeline (the 23 steps of the specification).
//!
//! ```text
//!  1 path validation      2 permissions        3 file size
//!  4 signature            5 real format        6 dimensions from header
//!  7 animation            8 alpha              9 metadata
//! 10 plan (format, mode) 11 decode            12 EXIF orientation
//! 13 resize              14 color space       15 metadata out
//! 16 encode candidates   17 compare original  18 pick the best
//! 19 temp file           20 verify            21 atomic rename
//! 22 progress            23 report
//! ```
//!
//! Every step returns a [`Result`], checks the cancellation token, is timed
//! and logged. A panic anywhere in here fails only the current file.

use std::path::Path;
use std::time::Instant;

use image::DynamicImage;
use image::metadata::Orientation;

use crate::cancel::CancelToken;
use crate::codecs::{self, EncodeInput, EncodeOptions, capability};
use crate::errors::{ImagoError, Result};
use crate::filesystem::paths::{ClaimedPaths, Resolution, destination_dir, output_path};
use crate::filesystem::safe_write::{self, Placement, TempCleanup, WriteOptions, write_atomic};
use crate::image_engine::animation::{can_decode_animation, open_animation};
use crate::image_engine::auto::{self, Candidate};
use crate::image_engine::color::{self, AlphaUsage, IccKind};
use crate::image_engine::detection;
use crate::image_engine::probe::{self, ImageProbe};
use crate::image_engine::resize::{ResizeQuality, resize, target_dimensions};
use crate::image_engine::{decode, guard_decoder};
use crate::jobs::{
    CandidateSize, Decision, DecisionReason, FileReport, FileStatus, FileTask, Stage, Warning,
    millis,
};
use crate::metadata;
use crate::settings::{
    AnimationPolicy, ConflictPolicy, FormatChoice, ImageFormat, OptimizeSettings,
    TransparencyPolicy, WhenLarger,
};
use crate::workers::MemoryBudget;

/// Shared state a worker needs to process one file.
pub struct PipelineContext<'a> {
    /// Job settings.
    pub settings: &'a OptimizeSettings,
    /// Memory budget shared by all workers.
    pub budget: &'a MemoryBudget,
    /// Output names claimed by this job.
    pub claims: &'a ClaimedPaths,
    /// Stale temporary file cleanup.
    pub cleanup: &'a TempCleanup,
    /// This file's cancellation token (child of the job token).
    pub cancel: &'a CancelToken,
    /// The job's token (distinguishes "cancel all" from "skip this file").
    pub job_cancel: &'a CancelToken,
    /// Stage notifications.
    pub progress: &'a dyn Fn(Stage),
}

/// Processes one file. Never panics and never returns an error: the outcome
/// is described by the report's status.
pub fn process_file(task: &FileTask, context: &PipelineContext<'_>) -> FileReport {
    let span = tracing::info_span!("file", id = task.id, path = %task.input.path.display());
    let _entered = span.enter();
    let started = Instant::now();
    let mut report = FileReport::pending(task);
    report.status = FileStatus::Processing;

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(task, context, &mut report)
    }));
    match outcome {
        Ok(Ok(Outcome::Written)) => report.status = FileStatus::Done,
        Ok(Ok(Outcome::Skipped(code, message))) => {
            report.status = FileStatus::Skipped;
            report.error = Some(crate::errors::ErrorInfo {
                code,
                message,
                alternative: None,
                hint: None,
            });
        }
        Ok(Err(error)) if error.is_cancelled() => {
            report.status = if context.job_cancel.is_cancelled() {
                FileStatus::Cancelled
            } else {
                FileStatus::Skipped
            };
            if report.status == FileStatus::Skipped {
                report.error = Some(crate::errors::ErrorInfo {
                    code: "skipped_by_user",
                    message: "skipped by the user".into(),
                    alternative: None,
                    hint: None,
                });
            }
        }
        Ok(Err(error)) => {
            tracing::warn!(code = error.code(), %error, "file failed");
            report.status = FileStatus::Failed;
            report.error = Some(error.to_info());
        }
        Err(payload) => {
            let message = codecs::panic_message(payload.as_ref());
            tracing::error!(%message, "file processing panicked");
            report.status = FileStatus::Failed;
            report.error = Some(ImagoError::Internal(message).to_info());
        }
    }
    report.timings.total_ms = millis(started.elapsed());
    tracing::debug!(status = ?report.status, total_ms = report.timings.total_ms, "file finished");
    report
}

enum Outcome {
    Written,
    Skipped(&'static str, String),
}

/// The bytes to write and what they are.
struct Chosen {
    bytes: Vec<u8>,
    format: ImageFormat,
    /// Dimensions as stored in the file (for verification).
    stored_dimensions: (u32, u32),
    /// Dimensions as displayed (after orientation).
    display_dimensions: (u32, u32),
    decision: Decision,
}

/// How a file will be handled after analysis (step 10).
enum Plan {
    /// Decode, transform and encode a still image.
    Still(StillTarget),
    /// Re-encode an animation into `target` (and/or keep the original).
    Animated {
        target: Option<ImageFormat>,
        include_original: bool,
    },
    /// Keep the original bytes (metadata removed losslessly where possible).
    Original,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StillTarget {
    Auto,
    Format(ImageFormat),
}

fn run(task: &FileTask, context: &PipelineContext<'_>, report: &mut FileReport) -> Result<Outcome> {
    let settings = context.settings;
    let cancel = context.cancel;
    let path = &task.input.path;

    // 1–3: path, permissions, size.
    (context.progress)(Stage::Reading);
    let read_started = Instant::now();
    let file_metadata = std::fs::metadata(path).map_err(|error| ImagoError::io(path, error))?;
    if !file_metadata.is_file() {
        return Err(ImagoError::NotAFile { path: path.clone() });
    }
    report.input_size = file_metadata.len();
    probe::check_file_size(file_metadata.len(), &settings.limits)?;
    let bytes = std::fs::read(path).map_err(|error| ImagoError::io(path, error))?;
    report.input_size = bytes.len() as u64;
    cancel.check()?;

    // 4–9: signature, real format, dimensions, animation, alpha, metadata.
    (context.progress)(Stage::Analyzing);
    let probe = probe::probe_bytes(&bytes, &settings.limits)?;
    report.input_format = Some(probe.format);
    report.input_dimensions = Some(probe.display_dimensions());
    if let Some(named) = detection::format_from_extension(path)
        && named != probe.format
    {
        report.warnings.push(Warning::new(
            "extension_mismatch",
            format!(
                "the file is named like {named} but contains {}; it was processed as {}",
                probe.format, probe.format
            ),
        ));
    }
    report.timings.read_ms = millis(read_started.elapsed());
    tracing::debug!(format = %probe.format, width = probe.width, height = probe.height, animated = probe.animated, "analyzed");
    cancel.check()?;

    // 10: plan.
    let plan = plan(&probe, settings, report)?;
    if let Some(existing) = existing_output(task, &probe, &plan, settings)? {
        report.output_path = Some(existing);
        return Ok(Outcome::Skipped(
            "output_exists",
            "the output file already exists".into(),
        ));
    }

    let memory = context.budget.acquire(estimate_memory(&probe), cancel)?;
    let chosen = match plan {
        Plan::Still(target) => process_still(&bytes, &probe, target, context, report)?,
        Plan::Animated {
            target,
            include_original,
        } => process_animation(&bytes, &probe, target, include_original, context, report)?,
        Plan::Original => {
            keep_original(&bytes, &probe, settings, report, DecisionReason::Requested)?.ok_or_else(
                || {
                    ImagoError::InvalidSettings(
                        "the original cannot be kept with these metadata settings".into(),
                    )
                },
            )?
        }
    };
    drop(memory);

    // 19–21: write, verify, move into place.
    write_output(task, chosen, context, report)
}

/// Step 10: decides between still/animated processing and the target format.
fn plan(probe: &ImageProbe, settings: &OptimizeSettings, report: &mut FileReport) -> Result<Plan> {
    let lossless = settings.compression.lossless;

    if probe.animated && settings.animation == AnimationPolicy::Preserve {
        if !can_decode_animation(probe.format) {
            report.warnings.push(Warning::new(
                "animation_kept_original",
                format!(
                    "animated {} cannot be re-encoded; the original file was kept",
                    probe.format
                ),
            ));
            return Ok(Plan::Original);
        }
        let original_target = match probe.format {
            ImageFormat::Gif => Some(ImageFormat::Gif),
            ImageFormat::Webp => Some(ImageFormat::Webp),
            _ => None, // APNG cannot be written; the original is kept.
        };
        let (mut target, include_original) = match settings.format {
            FormatChoice::Auto => (Some(ImageFormat::Webp), true),
            FormatChoice::Original => (original_target, true),
            FormatChoice::Fixed(format) if format.supports_animation() => {
                (Some(format), format == probe.format)
            }
            FormatChoice::Fixed(format) => {
                report.warnings.push(Warning::new(
                    "animation_kept_in_input_format",
                    format!(
                        "{format} cannot store animations; the animation was kept as {}",
                        probe.format
                    ),
                ));
                (original_target, true)
            }
        };
        let resizing = target_dimensions(probe.width, probe.height, &settings.resize).is_some();
        if target == Some(ImageFormat::Gif) && lossless && !resizing {
            // Re-encoding a GIF is never lossless; keeping it is.
            target = None;
        }
        if target.is_none() && resizing {
            report.warnings.push(Warning::new(
                "resize_not_applied",
                "the animation could not be re-encoded, so it was not resized",
            ));
        }
        return Ok(Plan::Animated {
            target,
            include_original,
        });
    }

    if probe.animated {
        report.warnings.push(Warning::new(
            "animation_first_frame",
            "only the first frame of the animation was used",
        ));
    }

    let target = match settings.format {
        FormatChoice::Auto => return Ok(Plan::Still(StillTarget::Auto)),
        FormatChoice::Original if probe.format == ImageFormat::Bmp => ImageFormat::Png,
        FormatChoice::Original => probe.format,
        FormatChoice::Fixed(format) => format,
    };
    let capability = capability(target);
    if !capability.encode {
        return Err(codecs::unavailable(target));
    }
    if lossless && !capability.lossless {
        let resizing = target_dimensions(probe.width, probe.height, &settings.resize).is_some();
        if target == probe.format && !resizing && probe.orientation == 1 {
            // Lossless "optimization" of e.g. a JPEG: keep the compressed data,
            // drop metadata only.
            return Ok(Plan::Original);
        }
        return Err(ImagoError::InvalidSettings(format!(
            "{target} has no lossless mode; choose PNG or WebP for lossless output"
        )));
    }
    if !lossless && !capability.lossy {
        report.warnings.push(Warning::new(
            "quality_not_used",
            format!("{target} is always written losslessly; the quality setting was not used"),
        ));
    }
    Ok(Plan::Still(StillTarget::Format(target)))
}

/// With the "skip existing" policy, detects an output from an earlier run
/// before doing any work.
fn existing_output(
    task: &FileTask,
    probe: &ImageProbe,
    plan: &Plan,
    settings: &OptimizeSettings,
) -> Result<Option<std::path::PathBuf>> {
    if settings.output.conflict != ConflictPolicy::Skip {
        return Ok(None);
    }
    let formats: Vec<ImageFormat> = match plan {
        Plan::Still(StillTarget::Format(format)) => vec![*format],
        Plan::Still(StillTarget::Auto) => vec![
            ImageFormat::Webp,
            ImageFormat::Avif,
            ImageFormat::Jpeg,
            ImageFormat::Png,
            probe.format,
        ],
        Plan::Animated { target, .. } => target.iter().copied().chain([probe.format]).collect(),
        Plan::Original => vec![probe.format],
    };
    for format in formats {
        let candidate = output_path(&task.input, format, &settings.output)?;
        let is_input = same_file::is_same_file(&task.input.path, &candidate).unwrap_or(false);
        if !is_input && candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// Rough peak memory for one file: decoded pixels, a resized copy, encoder
/// buffers and the file itself.
fn estimate_memory(probe: &ImageProbe) -> u64 {
    let bytes_per_pixel: u64 = if probe.bit_depth > 8 { 8 } else { 4 };
    let pixels = probe.pixel_count().saturating_mul(bytes_per_pixel);
    let factor = if probe.animated { 4 } else { 3 };
    pixels
        .saturating_mul(factor)
        .saturating_add(probe.file_size.saturating_mul(2))
}

/// Steps 11–18 for still images.
fn process_still(
    bytes: &[u8],
    probe: &ImageProbe,
    target: StillTarget,
    context: &PipelineContext<'_>,
    report: &mut FileReport,
) -> Result<Chosen> {
    let settings = context.settings;
    let cancel = context.cancel;

    // 11: decode.
    (context.progress)(Stage::Decoding);
    let decode_started = Instant::now();
    let decoded = decode::decode(bytes, probe, &settings.limits)?;
    report.timings.decode_ms = millis(decode_started.elapsed());
    cancel.check()?;

    // 12–14: orientation, resize, color space.
    (context.progress)(Stage::Transforming);
    let transform_started = Instant::now();
    let mut image = decoded.image;
    let rotated = settings.metadata.auto_orient && decoded.orientation != Orientation::NoTransforms;
    if rotated {
        image.apply_orientation(decoded.orientation);
    }
    let resized_to = target_dimensions(image.width(), image.height(), &settings.resize);
    if let Some((width, height)) = resized_to {
        image = resize(&image, width, height, ResizeQuality::High)?;
    }
    cancel.check()?;
    let alpha = color::alpha_usage(&image);
    if alpha == AlphaUsage::Opaque {
        image = color::drop_opaque_alpha(image);
    }
    let icc = decoded.icc_profile.as_deref();
    let icc_kind = icc.map(|profile| color::classify_icc(profile, &image));
    if icc_kind == Some(IccKind::Unusable) {
        report.warnings.push(Warning::new(
            "icc_ignored",
            "the embedded color profile does not match the pixels and was ignored",
        ));
    }
    report.timings.resize_ms = millis(transform_started.elapsed());

    // 15: metadata for the output.
    let orientation_value = probe::orientation_to_exif(decoded.orientation);
    let exif_out: Option<Vec<u8>> = if !settings.metadata.strip_metadata {
        decoded.exif.clone().map(|mut exif| {
            if rotated {
                metadata::reset_exif_orientation(&mut exif);
            }
            exif
        })
    } else if !rotated && orientation_value != 1 {
        // Pixels were not rotated: keep just the orientation so the image
        // still displays the right way up.
        Some(metadata::orientation_only_exif(orientation_value))
    } else {
        None
    };

    // Candidates.
    let lossless = settings.compression.lossless;
    let mut candidates: Vec<Candidate> = match target {
        StillTarget::Auto => auto::candidates(
            &auto::analyze(&image),
            lossless,
            settings.compression.effort,
        ),
        StillTarget::Format(format) => vec![Candidate { format, lossless }],
    };
    let mut flatten = false;
    if alpha.is_transparent()
        && candidates
            .iter()
            .any(|candidate| !candidate.format.supports_alpha())
    {
        match settings.transparency {
            TransparencyPolicy::FlattenWhite => {
                flatten = true;
                report.warnings.push(Warning::new(
                    "transparency_flattened",
                    "transparency was replaced with a white background",
                ));
            }
            TransparencyPolicy::Preserve => {
                let fallback = if probe.format.supports_alpha() && capability(probe.format).encode {
                    probe.format
                } else {
                    ImageFormat::Png
                };
                for candidate in &mut candidates {
                    if !candidate.format.supports_alpha() {
                        report.warnings.push(Warning::new(
                            "transparency_kept_in_other_format",
                            format!(
                                "{} cannot store transparency; the image was written as {fallback} instead",
                                candidate.format
                            ),
                        ));
                        candidate.format = fallback;
                    }
                }
                candidates.dedup();
            }
        }
    }

    // 16: encode candidates, keeping only the smallest.
    (context.progress)(Stage::Encoding);
    let encode_started = Instant::now();
    let mut srgb_image: Option<DynamicImage> = None;
    let mut best: Option<(Vec<u8>, Candidate)> = None;
    let mut sizes = Vec::new();
    let mut icc_warned = false;
    let total = candidates.len();
    for (index, candidate) in candidates.iter().enumerate() {
        cancel.check()?;
        let format_capability = capability(candidate.format);
        let (pixels, embed_icc): (&DynamicImage, Option<&[u8]>) = match icc_kind {
            Some(IccKind::Other) if settings.metadata.keep_icc && format_capability.icc => {
                (&image, icc)
            }
            Some(IccKind::Other) => {
                if settings.metadata.keep_icc && !icc_warned {
                    icc_warned = true;
                    report.warnings.push(Warning::new(
                        "icc_converted_to_srgb",
                        format!(
                            "{} cannot embed color profiles; colors were converted to sRGB",
                            candidate.format
                        ),
                    ));
                }
                if srgb_image.is_none() {
                    // `icc_kind` is only `Some` when a profile exists.
                    let profile = icc.unwrap_or_default();
                    srgb_image = Some(color::convert_to_srgb(&image, profile)?);
                }
                match &srgb_image {
                    Some(converted) => (converted, None),
                    None => (&image, None),
                }
            }
            _ => (&image, None),
        };
        let flattened;
        let pixels = if flatten && !candidate.format.supports_alpha() {
            flattened = color::flatten_onto(pixels, [255, 255, 255]);
            &flattened
        } else {
            pixels
        };
        let input = EncodeInput {
            image: pixels,
            icc_profile: embed_icc,
            exif: exif_out.as_deref().filter(|_| format_capability.exif),
        };
        let options = EncodeOptions {
            quality: settings.compression.quality,
            lossless: candidate.lossless || lossless,
            effort: settings.compression.effort,
        };
        let started = Instant::now();
        let encoded = match codecs::encode(candidate.format, input, options) {
            Ok(encoded) => encoded,
            Err(error) if target == StillTarget::Auto && index + 1 < total => {
                tracing::debug!(format = %candidate.format, %error, "auto candidate failed");
                continue;
            }
            Err(error) if target == StillTarget::Auto && best.is_some() => {
                tracing::debug!(format = %candidate.format, %error, "auto candidate failed");
                continue;
            }
            Err(error) => return Err(error),
        };
        tracing::debug!(
            format = %candidate.format,
            lossless = options.lossless,
            size = encoded.len(),
            ms = millis(started.elapsed()),
            "candidate encoded"
        );
        sizes.push(CandidateSize {
            format: candidate.format,
            lossless: options.lossless,
            size: encoded.len() as u64,
        });
        if best
            .as_ref()
            .is_none_or(|(current, _)| encoded.len() < current.len())
        {
            best = Some((encoded, *candidate));
        }
    }
    report.timings.encode_ms = millis(encode_started.elapsed());
    let (encoded, winner) =
        best.ok_or_else(|| ImagoError::Internal("no encoder produced a result".into()))?;

    // 17–18: compare with the original.
    let output_dimensions = (image.width(), image.height());
    let original_is_candidate = settings.output.when_larger == WhenLarger::KeepOriginal
        && resized_to.is_none()
        && (target == StillTarget::Auto || winner.format == probe.format)
        && !(icc_kind == Some(IccKind::Other) && !settings.metadata.keep_icc);
    if original_is_candidate
        && let Some(original) = original_bytes(bytes, probe, settings)?
        && original.len() <= encoded.len()
    {
        let mut chosen = kept_original(original, probe, report, DecisionReason::KeptOriginal);
        chosen.decision.candidates = sizes;
        return Ok(chosen);
    }

    let reason = match target {
        StillTarget::Auto => DecisionReason::Smallest,
        StillTarget::Format(_) => DecisionReason::Requested,
    };
    let saved = saved_percent(report.input_size, encoded.len() as u64);
    if saved < 0.0 {
        report.warnings.push(Warning::new(
            "larger_than_original",
            format!("the result is {:.0}% larger than the original", -saved),
        ));
    }
    Ok(Chosen {
        stored_dimensions: output_dimensions,
        display_dimensions: output_dimensions,
        format: winner.format,
        decision: Decision {
            format: winner.format,
            reason,
            saved_percent: saved,
            kept_transparency: alpha.is_transparent() && winner.format.supports_alpha() && !flatten,
            kept_animation: false,
            candidates: sizes,
        },
        bytes: encoded,
    })
}

/// Steps 11–18 for animations.
fn process_animation(
    bytes: &[u8],
    probe: &ImageProbe,
    target: Option<ImageFormat>,
    include_original: bool,
    context: &PipelineContext<'_>,
    report: &mut FileReport,
) -> Result<Chosen> {
    let settings = context.settings;
    let Some(target) = target else {
        return keep_original(bytes, probe, settings, report, DecisionReason::KeptOriginal)?
            .ok_or_else(|| {
                ImagoError::InvalidSettings(
                    "the animation cannot be kept with these metadata settings".into(),
                )
            });
    };

    (context.progress)(Stage::Decoding);
    let resized_to = target_dimensions(probe.width, probe.height, &settings.resize);
    let mut source = open_animation(bytes, probe.format, &settings.limits, resized_to)?;
    let info = source.info;

    (context.progress)(Stage::Encoding);
    let encode_started = Instant::now();
    let options = EncodeOptions {
        quality: settings.compression.quality,
        lossless: settings.compression.lossless,
        effort: settings.compression.effort,
    };
    let encoded = guard_decoder(probe.format, || {
        codecs::encode_animation(target, info, &mut source, options, context.cancel)
    })?;
    report.timings.encode_ms = millis(encode_started.elapsed());
    let sizes = vec![CandidateSize {
        format: target,
        lossless: options.lossless,
        size: encoded.len() as u64,
    }];

    if include_original
        && resized_to.is_none()
        && settings.output.when_larger == WhenLarger::KeepOriginal
        && let Some(original) = original_bytes(bytes, probe, settings)?
        && original.len() <= encoded.len()
    {
        let mut chosen = kept_original(original, probe, report, DecisionReason::KeptOriginal);
        chosen.decision.kept_animation = true;
        chosen.decision.candidates = sizes;
        return Ok(chosen);
    }

    let dimensions = (info.width, info.height);
    let explicit = matches!(settings.format, FormatChoice::Fixed(format) if format == target);
    Ok(Chosen {
        stored_dimensions: dimensions,
        display_dimensions: dimensions,
        format: target,
        decision: Decision {
            format: target,
            reason: if explicit {
                DecisionReason::Requested
            } else {
                DecisionReason::Smallest
            },
            saved_percent: saved_percent(report.input_size, encoded.len() as u64),
            kept_transparency: probe.has_alpha,
            kept_animation: true,
            candidates: sizes,
        },
        bytes: encoded,
    })
}

/// The original file, with metadata removed losslessly when requested.
/// `None` when the original cannot satisfy the metadata settings.
fn original_bytes(
    bytes: &[u8],
    probe: &ImageProbe,
    settings: &OptimizeSettings,
) -> Result<Option<Vec<u8>>> {
    if !capability(probe.format).encode {
        // BMP (or a format this build cannot write) is never kept as output.
        return Ok(None);
    }
    if !settings.metadata.strip_metadata {
        return Ok(Some(bytes.to_vec()));
    }
    if probe.orientation != 1 {
        // Removing EXIF would lose the orientation and show the image rotated.
        return Ok(None);
    }
    // Keep a non-sRGB profile: dropping it without converting would change colors.
    let keep_icc = probe.has_icc
        && metadata::container_icc(bytes, probe.format).is_some_and(|icc| !is_srgb_profile(&icc));
    match metadata::strip_container_metadata(bytes, probe.format, keep_icc)? {
        Some(stripped) => Ok(Some(stripped)),
        None if !probe.has_exif => Ok(Some(bytes.to_vec())),
        None => Ok(None),
    }
}

fn is_srgb_profile(icc: &[u8]) -> bool {
    // Classify against a tiny RGB image: only the profile matters here.
    let probe_image = DynamicImage::new_rgb8(1, 1);
    color::classify_icc(icc, &probe_image) == IccKind::Srgb
}

fn keep_original(
    bytes: &[u8],
    probe: &ImageProbe,
    settings: &OptimizeSettings,
    report: &mut FileReport,
    reason: DecisionReason,
) -> Result<Option<Chosen>> {
    Ok(original_bytes(bytes, probe, settings)?
        .map(|original| kept_original(original, probe, report, reason)))
}

fn kept_original(
    original: Vec<u8>,
    probe: &ImageProbe,
    report: &FileReport,
    reason: DecisionReason,
) -> Chosen {
    Chosen {
        stored_dimensions: (probe.width, probe.height),
        display_dimensions: probe.display_dimensions(),
        format: probe.format,
        decision: Decision {
            format: probe.format,
            reason,
            saved_percent: saved_percent(report.input_size, original.len() as u64),
            kept_transparency: probe.has_alpha,
            kept_animation: probe.animated,
            candidates: Vec::new(),
        },
        bytes: original,
    }
}

fn saved_percent(input: u64, output: u64) -> f64 {
    if input == 0 {
        0.0
    } else {
        (input as f64 - output as f64) / input as f64 * 100.0
    }
}

/// Steps 19–23.
fn write_output(
    task: &FileTask,
    chosen: Chosen,
    context: &PipelineContext<'_>,
    report: &mut FileReport,
) -> Result<Outcome> {
    let settings = context.settings;
    (context.progress)(Stage::Writing);
    let write_started = Instant::now();

    let wanted = output_path(&task.input, chosen.format, &settings.output)?;
    if let crate::settings::OutputLocation::File { path } = &settings.output.location
        && *path != wanted
    {
        report.warnings.push(Warning::new(
            "output_extension_changed",
            format!(
                "the output is {} data, so it was saved as {} instead of {}",
                chosen.format,
                wanted.display(),
                path.display()
            ),
        ));
    }
    context
        .cleanup
        .clean_once(&destination_dir(&task.input, &settings.output));
    if let Some(parent) = wanted.parent() {
        context.cleanup.clean_once(parent);
    }

    let mut resolution =
        context
            .claims
            .resolve(&task.input.path, wanted.clone(), &settings.output)?;
    let mut attempts = 0;
    let written = loop {
        attempts += 1;
        let (path, placement) = match &resolution {
            Resolution::Skip(path) => {
                report.output_path = Some(path.clone());
                return Ok(Outcome::Skipped(
                    "output_exists",
                    "the output file already exists".into(),
                ));
            }
            Resolution::Create(path) => (path.clone(), Placement::CreateNew),
            Resolution::Replace(path) => (path.clone(), Placement::Replace),
        };
        let options = WriteOptions {
            placement,
            fsync: settings.output.fsync,
        };
        let result = write_atomic(&path, &chosen.bytes, options, context.cancel, |temp| {
            verify_written(temp, &chosen.bytes, chosen.format)
        });
        match result {
            Ok(()) => break path,
            Err(ImagoError::OutputExists { .. }) if attempts < 5 => {
                // Another program created the file meanwhile: pick another name.
                context.claims.claim(&path);
                resolution = context
                    .claims
                    .resolve(&task.input.path, path, &settings.output)?;
            }
            Err(error) => {
                context.claims.release(&path);
                return Err(error);
            }
        }
    };

    if let Err(error) = safe_write::copy_permissions(&task.input.path, &written) {
        report
            .warnings
            .push(Warning::new("permissions_not_copied", error.to_string()));
    }
    if settings.metadata.preserve_timestamps
        && let Err(error) = safe_write::copy_file_times(&task.input.path, &written)
    {
        report
            .warnings
            .push(Warning::new("timestamps_not_copied", error.to_string()));
    }
    report.timings.write_ms = millis(write_started.elapsed());

    let output_size = chosen.bytes.len() as u64;
    report.output_path = Some(written);
    report.output_format = Some(chosen.format);
    report.output_size = Some(output_size);
    report.output_dimensions = Some(chosen.display_dimensions);
    report.reduction_percent = Some(saved_percent(report.input_size, output_size));
    tracing::info!(
        format = %chosen.format,
        input = report.input_size,
        output = output_size,
        stored = ?chosen.stored_dimensions,
        "written"
    );
    report.decision = Some(chosen.decision);
    Ok(Outcome::Written)
}

/// Step 20: the temporary file must read back byte-for-byte and still be
/// recognized as the expected format.
fn verify_written(temp: &Path, expected: &[u8], format: ImageFormat) -> Result<()> {
    let written = std::fs::read(temp).map_err(|error| ImagoError::io(temp, error))?;
    if written != expected {
        return Err(ImagoError::VerificationFailed {
            path: temp.to_path_buf(),
            reason: "the file on disk differs from the encoded data".into(),
        });
    }
    let head = &written[..written.len().min(detection::SNIFF_LEN)];
    if detection::sniff(head).format() != Some(format) {
        return Err(ImagoError::VerificationFailed {
            path: temp.to_path_buf(),
            reason: format!("the written file is not a valid {format} file"),
        });
    }
    Ok(())
}
