//! JPEG encoding with mozjpeg (trellis quantization, progressive scans,
//! optimized Huffman tables).
//!
//! mozjpeg reports libjpeg errors by unwinding; `codecs::encode` catches that.

use image::DynamicImage;
use mozjpeg::{ColorSpace, Compress, Marker};

use crate::codecs::{EncodeInput, EncodeOptions, encode_error};
use crate::errors::Result;
use crate::image_engine::color::flatten_onto;
use crate::settings::{Effort, ImageFormat};

/// Largest payload of a single JPEG marker segment.
const MAX_MARKER_PAYLOAD: usize = 65_533;
const EXIF_PREFIX: &[u8] = b"Exif\0\0";

pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    let format = ImageFormat::Jpeg;
    // JPEG has no alpha channel. The pipeline only lets transparent images get
    // here with the user's consent; opaque alpha channels are dropped exactly.
    let pixels = to_jpeg_pixels(input.image);
    let (color_space, samples) = match &pixels {
        DynamicImage::ImageLuma8(buffer) => (ColorSpace::JCS_GRAYSCALE, buffer.as_raw()),
        DynamicImage::ImageRgb8(buffer) => (ColorSpace::JCS_RGB, buffer.as_raw()),
        _ => return Err(encode_error(format, "unexpected pixel layout")),
    };
    let (width, height) = (pixels.width() as usize, pixels.height() as usize);
    let quality = options.quality.get();

    let mut compress = Compress::new(color_space);
    compress.set_size(width, height);
    if options.effort == Effort::Fast {
        compress.set_fastest_defaults();
    }
    compress.set_quality(f32::from(quality));
    compress.set_optimize_coding(true);
    if options.effort != Effort::Fast {
        compress.set_progressive_mode();
        compress.set_optimize_scans(true);
    }
    if color_space == ColorSpace::JCS_RGB && quality >= 90 {
        // Full-resolution chroma for high quality settings (4:4:4).
        compress.set_chroma_sampling_pixel_sizes((1, 1), (1, 1));
    }

    let estimate = width * height / 4 + 4096;
    let mut started = compress
        .start_compress(Vec::with_capacity(estimate))
        .map_err(|e| encode_error(format, e))?;
    if let Some(exif) = input.exif
        && exif.len() + EXIF_PREFIX.len() <= MAX_MARKER_PAYLOAD
    {
        let mut segment = Vec::with_capacity(EXIF_PREFIX.len() + exif.len());
        segment.extend_from_slice(EXIF_PREFIX);
        segment.extend_from_slice(exif);
        started.write_marker(Marker::APP(1), &segment);
    }
    if let Some(icc) = input.icc_profile {
        started.write_icc_profile(icc);
    }
    started
        .write_scanlines(samples)
        .map_err(|e| encode_error(format, e))?;
    started.finish().map_err(|e| encode_error(format, e))
}

fn to_jpeg_pixels(image: &DynamicImage) -> DynamicImage {
    let opaque = if image.color().has_alpha() {
        flatten_onto(image, [255, 255, 255])
    } else {
        image.clone()
    };
    if opaque.color().has_color() {
        match opaque {
            DynamicImage::ImageRgb8(_) => opaque,
            other => DynamicImage::ImageRgb8(other.to_rgb8()),
        }
    } else {
        match opaque {
            DynamicImage::ImageLuma8(_) => opaque,
            other => DynamicImage::ImageLuma8(other.to_luma8()),
        }
    }
}
