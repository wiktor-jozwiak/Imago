//! Encoders for every output format, behind one function: [`encode`].
//!
//! Each encoder receives pixels that the pipeline has already oriented,
//! resized and color-converted. Encoders never see the original file and
//! never touch the file system.

mod apng;
mod avif;
pub mod capabilities;
mod gif;
mod jpeg;
mod jxl;
mod png;
mod quantize;
mod tiff;
mod webp;
pub mod webp_mux;

use std::panic::{AssertUnwindSafe, catch_unwind};

use image::DynamicImage;

pub use capabilities::{FormatCapability, capabilities, capability};

use crate::cancel::CancelToken;
use crate::errors::{ImagoError, Result};
use crate::settings::{Effort, ImageFormat, Quality};

/// Encoder parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    /// Quality for lossy encoders.
    pub quality: Quality,
    /// Lossless encoding.
    pub lossless: bool,
    /// CPU effort.
    pub effort: Effort,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions {
            quality: Quality::DEFAULT,
            lossless: false,
            effort: Effort::Balanced,
        }
    }
}

/// Pixels and metadata to encode.
#[derive(Debug, Clone, Copy)]
pub struct EncodeInput<'a> {
    /// Final pixels (oriented, resized, color-converted).
    pub image: &'a DynamicImage,
    /// ICC profile to embed (only for formats that support it).
    pub icc_profile: Option<&'a [u8]>,
    /// EXIF block to embed (TIFF structure without the `Exif\0\0` prefix).
    pub exif: Option<&'a [u8]>,
    /// Lets slow encoders (AVIF, JPEG XL) stop in the middle of an image.
    pub cancel: Option<&'a CancelToken>,
}

impl<'a> EncodeInput<'a> {
    /// Pixels without any metadata.
    pub fn pixels(image: &'a DynamicImage) -> Self {
        EncodeInput {
            image,
            icc_profile: None,
            exif: None,
            cancel: None,
        }
    }
}

/// Encodes an image into `format`.
///
/// Checks that the codec is available and supports the requested mode, and
/// isolates panics raised inside codec libraries (mozjpeg reports its errors by
/// unwinding) so they surface as [`ImagoError::Encode`].
pub fn encode(
    format: ImageFormat,
    input: EncodeInput<'_>,
    options: EncodeOptions,
) -> Result<Vec<u8>> {
    let capability = capability(format);
    if !capability.encode {
        return Err(unavailable(format));
    }
    if options.lossless && !capability.lossless {
        return Err(ImagoError::Encode {
            format: format.display_name().into(),
            message: format!("{} has no lossless mode", format.display_name()),
        });
    }
    // Formats without a lossy mode (TIFF, JPEG XL) are simply written losslessly;
    // the pipeline reports that the quality setting was not used.
    if let Some(max) = capability.max_dimension {
        let (width, height) = (input.image.width(), input.image.height());
        if width > max || height > max {
            return Err(ImagoError::Encode {
                format: format.display_name().into(),
                message: format!(
                    "{} supports at most {max}×{max} px, the image is {width}×{height} px",
                    format.display_name()
                ),
            });
        }
    }

    let result = catch_unwind(AssertUnwindSafe(|| match format {
        ImageFormat::Jpeg => jpeg::encode(input, options),
        ImageFormat::Png => png::encode(input, options),
        ImageFormat::Webp => webp::encode(input, options),
        ImageFormat::Avif => avif::encode(input, options),
        ImageFormat::Tiff => tiff::encode(input, options),
        ImageFormat::Gif => gif::encode_still(input, options),
        ImageFormat::Jxl => jxl::encode(input, options),
        ImageFormat::Bmp | ImageFormat::Heic => Err(unavailable(format)),
    }));
    match result {
        Ok(result) => result,
        Err(payload) => Err(ImagoError::Encode {
            format: format.display_name().into(),
            message: panic_message(payload.as_ref()),
        }),
    }
}

/// One fully composited frame of an animation.
#[derive(Debug, Clone)]
pub struct AnimationFrame {
    /// The whole canvas as it should look while this frame is shown.
    pub image: image::RgbaImage,
    /// Display time in milliseconds.
    pub delay_ms: u32,
}

/// Global properties of an animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationInfo {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Number of loops; 0 means "forever".
    pub loop_count: u16,
}

/// Encodes an animation, consuming frames one at a time.
///
/// WebP and GIF stream: only the previous canvas and one encoded frame are
/// kept in memory. AVIF, JPEG XL and APNG need all frames at once (inter-frame
/// coding, one shared palette); `Limits::max_animation_pixels` bounds them.
pub fn encode_animation(
    format: ImageFormat,
    info: AnimationInfo,
    frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
    options: EncodeOptions,
    cancel: &CancelToken,
) -> Result<Vec<u8>> {
    let result = catch_unwind(AssertUnwindSafe(|| match format {
        ImageFormat::Webp => webp::encode_animation(info, frames, options, cancel),
        ImageFormat::Gif => gif::encode_animation(info, frames, options, cancel),
        ImageFormat::Png => apng::encode_animation(info, frames, options, cancel),
        ImageFormat::Avif => avif::encode_animation(info, frames, options, cancel),
        ImageFormat::Jxl => jxl::encode_animation(info, frames, options, cancel),
        other => Err(encode_error(
            other,
            format!("{} cannot store animations", other.display_name()),
        )),
    }));
    match result {
        Ok(result) => result,
        Err(payload) => Err(encode_error(format, panic_message(payload.as_ref()))),
    }
}

/// Smallest rectangle `(x, y, width, height)` containing every pixel that
/// differs between two canvases of equal size, or `None` if they are identical.
pub(crate) fn changed_rect(
    previous: &image::RgbaImage,
    current: &image::RgbaImage,
) -> Option<(u32, u32, u32, u32)> {
    changed_rect_raw(previous.as_raw(), current.as_raw(), current.width(), 4)
}

/// [`changed_rect`] for raw canvases of `bytes_per_pixel` bytes per pixel
/// (4 for RGBA, 1 for palette indices).
pub(crate) fn changed_rect_raw(
    previous: &[u8],
    current: &[u8],
    width: u32,
    bytes_per_pixel: usize,
) -> Option<(u32, u32, u32, u32)> {
    let row_len = width as usize * bytes_per_pixel;
    if row_len == 0 || previous.len() != current.len() {
        return None;
    }
    let height = current.len() / row_len;
    let rows_differ = |y: usize| {
        previous[y * row_len..(y + 1) * row_len] != current[y * row_len..(y + 1) * row_len]
    };

    let top = (0..height).find(|&y| rows_differ(y))?;
    let bottom = (top..height).rev().find(|&y| rows_differ(y)).unwrap_or(top);
    let mut left = width as usize;
    let mut right = 0usize;
    for y in top..=bottom {
        let row = y * row_len;
        for x in 0..width as usize {
            let at = row + x * bytes_per_pixel;
            if previous[at..at + bytes_per_pixel] != current[at..at + bytes_per_pixel] {
                left = left.min(x);
                right = right.max(x);
            }
        }
    }
    Some((
        left as u32,
        top as u32,
        (right - left + 1) as u32,
        (bottom - top + 1) as u32,
    ))
}

/// Extracts a readable message from a panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "the codec panicked".to_owned()
    }
}

/// Error for a format whose codec is not compiled into this build.
pub fn unavailable(format: ImageFormat) -> ImagoError {
    let capability = capability(format);
    ImagoError::FormatUnavailable {
        format: format.display_name().into(),
        reason: capability
            .unavailable_reason
            .clone()
            .unwrap_or_else(|| "this format cannot be written".into()),
        alternative: capability
            .alternative
            .map(|alt| alt.display_name().to_owned()),
        how_to_enable: capability.how_to_enable.clone(),
    }
}

/// Maps an encoder error message into an [`ImagoError`].
pub(crate) fn encode_error(format: ImageFormat, message: impl std::fmt::Display) -> ImagoError {
    ImagoError::Encode {
        format: format.display_name().into(),
        message: message.to_string(),
    }
}

/// CPU effort mapped onto a 0..=2 index (fast, balanced, max).
pub(crate) fn effort_index(effort: Effort) -> usize {
    match effort {
        Effort::Fast => 0,
        Effort::Balanced => 1,
        Effort::Max => 2,
    }
}
