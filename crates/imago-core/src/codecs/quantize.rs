//! Palette quantization with libimagequant (lossy PNG, GIF).

use image::RgbaImage;
use imagequant::RGBA;
use rgb::bytemuck;

use crate::codecs::{effort_index, encode_error};
use crate::errors::Result;
use crate::settings::{Effort, ImageFormat};

/// An image reduced to at most 256 palette entries.
#[derive(Debug, Clone)]
pub(crate) struct Quantized {
    /// Palette entries (RGBA, straight alpha).
    pub palette: Vec<RGBA>,
    /// One palette index per pixel, row by row.
    pub indices: Vec<u8>,
}

/// Quality bounds for the quantizer (0..=100, like pngquant's `--quality`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct QuantizeQuality {
    /// Give up (return `None`) if this quality cannot be reached.
    pub minimum: u8,
    /// Use as few colors as possible while reaching this quality.
    pub target: u8,
}

/// Quantizes `image`. Returns `Ok(None)` when the minimum quality is not
/// reachable with 256 colors, so the caller can fall back to lossless output.
pub(crate) fn quantize(
    image: &RgbaImage,
    quality: QuantizeQuality,
    effort: Effort,
    format: ImageFormat,
) -> Result<Option<Quantized>> {
    let fail =
        |error: imagequant::Error| encode_error(format, format!("quantization failed: {error}"));
    let mut attributes = imagequant::new();
    let speed = [8, 4, 1][effort_index(effort)];
    attributes.set_speed(speed).map_err(fail)?;
    attributes
        .set_quality(quality.minimum.min(quality.target), quality.target.min(100))
        .map_err(fail)?;

    let pixels: &[RGBA] = bytemuck::cast_slice(image.as_raw());
    let (width, height) = (image.width() as usize, image.height() as usize);
    let mut liq_image = attributes
        .new_image_borrowed(pixels, width, height, 0.0)
        .map_err(fail)?;
    let mut result = match attributes.quantize(&mut liq_image) {
        Ok(result) => result,
        Err(imagequant::Error::QualityTooLow) => return Ok(None),
        Err(error) => return Err(fail(error)),
    };
    result.set_dithering_level(1.0).map_err(fail)?;
    let (palette, indices) = result.remapped(&mut liq_image).map_err(fail)?;
    Ok(Some(Quantized { palette, indices }))
}

/// Quality bounds used for lossy PNG at a given Imago quality.
pub(crate) fn png_quality(quality: u8) -> QuantizeQuality {
    QuantizeQuality {
        minimum: quality.saturating_sub(30),
        target: quality,
    }
}
