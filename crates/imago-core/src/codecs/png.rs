//! PNG encoding.
//!
//! * lossless: pixels go straight into oxipng (`RawImage`), which picks the
//!   best color type, bit depth, filters and deflate settings;
//! * lossy (like TinyPNG): libimagequant reduces the image to ≤256 colors with
//!   dithering, then oxipng writes the smallest indexed PNG. Images that already
//!   have ≤256 colors, or that cannot reach the minimum quality, stay lossless.

use image::DynamicImage;
use oxipng::{BitDepth, ColorType, Options, RawImage, StripChunks};

use crate::codecs::quantize::{png_quality, quantize};
use crate::codecs::{EncodeInput, EncodeOptions, effort_index, encode_error};
use crate::errors::Result;
use crate::image_engine::color::count_colors_up_to;
use crate::settings::{Effort, ImageFormat};

const FORMAT: ImageFormat = ImageFormat::Png;

pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    let image = input.image;
    let lossless = options.lossless || options.quality.get() >= 100;
    let raw = if lossless || count_colors_up_to(image, 256) <= 256 {
        lossless_raw(image)?
    } else {
        let rgba = image.to_rgba8();
        match quantize(
            &rgba,
            png_quality(options.quality.get()),
            options.effort,
            FORMAT,
        )? {
            Some(quantized) => {
                let palette = quantized.palette;
                RawImage::new(
                    image.width(),
                    image.height(),
                    ColorType::Indexed { palette },
                    BitDepth::Eight,
                    quantized.indices,
                )
                .map_err(|e| encode_error(FORMAT, e))?
            }
            None => lossless_raw(image)?,
        }
    };
    finish(raw, input, options.effort, lossless)
}

fn finish(
    mut raw: RawImage,
    input: EncodeInput<'_>,
    effort: Effort,
    lossless: bool,
) -> Result<Vec<u8>> {
    if let Some(icc) = input.icc_profile {
        raw.add_icc_profile(icc);
    }
    if let Some(exif) = input.exif {
        raw.add_png_chunk(*b"eXIf", exif.to_vec());
    }
    let preset = [1, 2, 5][effort_index(effort)];
    let mut options = Options::from_preset(preset);
    // We decide which ancillary chunks exist; oxipng must not drop them.
    options.strip = StripChunks::None;
    // Invisible color values under fully transparent pixels may be changed
    // only in lossy mode.
    options.optimize_alpha = !lossless;
    raw.create_optimized_png(&options)
        .map_err(|e| encode_error(FORMAT, e))
}

/// Builds an oxipng image that keeps every pixel value (and 16-bit depth).
fn lossless_raw(image: &DynamicImage) -> Result<RawImage> {
    let (width, height) = (image.width(), image.height());
    let (color_type, depth, data) = match image {
        DynamicImage::ImageLuma8(buffer) => (
            ColorType::Grayscale {
                transparent_shade: None,
            },
            BitDepth::Eight,
            buffer.as_raw().clone(),
        ),
        DynamicImage::ImageLumaA8(buffer) => (
            ColorType::GrayscaleAlpha,
            BitDepth::Eight,
            buffer.as_raw().clone(),
        ),
        DynamicImage::ImageRgb8(buffer) => (
            ColorType::RGB {
                transparent_color: None,
            },
            BitDepth::Eight,
            buffer.as_raw().clone(),
        ),
        DynamicImage::ImageRgba8(buffer) => {
            (ColorType::RGBA, BitDepth::Eight, buffer.as_raw().clone())
        }
        DynamicImage::ImageLuma16(buffer) => (
            ColorType::Grayscale {
                transparent_shade: None,
            },
            BitDepth::Sixteen,
            big_endian(buffer.as_raw()),
        ),
        DynamicImage::ImageLumaA16(buffer) => (
            ColorType::GrayscaleAlpha,
            BitDepth::Sixteen,
            big_endian(buffer.as_raw()),
        ),
        DynamicImage::ImageRgb16(buffer) => (
            ColorType::RGB {
                transparent_color: None,
            },
            BitDepth::Sixteen,
            big_endian(buffer.as_raw()),
        ),
        DynamicImage::ImageRgba16(buffer) => (
            ColorType::RGBA,
            BitDepth::Sixteen,
            big_endian(buffer.as_raw()),
        ),
        other => {
            // Floating point images: 16 bits per channel keeps the precision PNG can hold.
            let widened = if other.color().has_alpha() {
                DynamicImage::ImageRgba16(other.to_rgba16())
            } else {
                DynamicImage::ImageRgb16(other.to_rgb16())
            };
            return lossless_raw(&widened);
        }
    };
    RawImage::new(width, height, color_type, depth, data).map_err(|e| encode_error(FORMAT, e))
}

/// PNG stores 16-bit samples big-endian.
fn big_endian(samples: &[u16]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_be_bytes())
        .collect()
}
