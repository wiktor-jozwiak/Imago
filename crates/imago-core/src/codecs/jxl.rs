//! JPEG XL encoding (lossless only) with zune-jpegxl.
//!
//! The encoder is young, so every result is decoded again with jxl-oxide and
//! compared pixel by pixel before it is accepted. A mismatch is reported as an
//! encoding error instead of producing a wrong file.

use crate::codecs::{EncodeInput, EncodeOptions};
use crate::errors::Result;

#[cfg(feature = "jxl")]
pub(crate) fn encode(input: EncodeInput<'_>, _options: EncodeOptions) -> Result<Vec<u8>> {
    use std::io::Cursor;

    use image::{DynamicImage, ImageDecoder};
    use zune_core::bit_depth::BitDepth;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::EncoderOptions;

    use crate::codecs::encode_error;
    use crate::settings::ImageFormat;

    let format = ImageFormat::Jxl;
    let image = input.image;
    let (width, height) = (image.width(), image.height());
    if width < 2 || height < 2 {
        return Err(encode_error(
            format,
            "the JPEG XL encoder needs at least 2×2 pixels",
        ));
    }

    let native =
        |samples: &[u16]| -> Vec<u8> { samples.iter().flat_map(|s| s.to_ne_bytes()).collect() };
    let widened;
    let source = match image {
        DynamicImage::ImageLuma8(_)
        | DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_)
        | DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_) => image,
        other => {
            widened = if other.color().has_alpha() {
                DynamicImage::ImageRgba16(other.to_rgba16())
            } else {
                DynamicImage::ImageRgb16(other.to_rgb16())
            };
            &widened
        }
    };
    let (color_space, depth, data): (ColorSpace, BitDepth, std::borrow::Cow<'_, [u8]>) =
        match source {
            DynamicImage::ImageLuma8(b) => (ColorSpace::Luma, BitDepth::Eight, b.as_raw().into()),
            DynamicImage::ImageLumaA8(b) => (ColorSpace::LumaA, BitDepth::Eight, b.as_raw().into()),
            DynamicImage::ImageRgb8(b) => (ColorSpace::RGB, BitDepth::Eight, b.as_raw().into()),
            DynamicImage::ImageRgba8(b) => (ColorSpace::RGBA, BitDepth::Eight, b.as_raw().into()),
            DynamicImage::ImageLuma16(b) => (
                ColorSpace::Luma,
                BitDepth::Sixteen,
                native(b.as_raw()).into(),
            ),
            DynamicImage::ImageLumaA16(b) => (
                ColorSpace::LumaA,
                BitDepth::Sixteen,
                native(b.as_raw()).into(),
            ),
            DynamicImage::ImageRgb16(b) => (
                ColorSpace::RGB,
                BitDepth::Sixteen,
                native(b.as_raw()).into(),
            ),
            DynamicImage::ImageRgba16(b) => (
                ColorSpace::RGBA,
                BitDepth::Sixteen,
                native(b.as_raw()).into(),
            ),
            _ => return Err(encode_error(format, "unexpected pixel layout")),
        };

    let options = EncoderOptions::new(width as usize, height as usize, color_space, depth);
    let mut output = Vec::new();
    zune_jpegxl::JxlSimpleEncoder::new(&data, options)
        .encode(&mut output)
        .map_err(|error| encode_error(format, format!("{error:?}")))?;

    // Round-trip verification.
    let decoder = jxl_oxide::integration::JxlDecoder::new(Cursor::new(&output))
        .map_err(|error| encode_error(format, format!("verification decode failed: {error}")))?;
    if decoder.dimensions() != (width, height) {
        return Err(encode_error(
            format,
            "verification failed: dimensions differ",
        ));
    }
    let decoded = DynamicImage::from_decoder(decoder)
        .map_err(|error| encode_error(format, format!("verification decode failed: {error}")))?;
    if decoded.to_rgba16().as_raw() != source.to_rgba16().as_raw() {
        return Err(encode_error(
            format,
            "verification failed: decoded pixels differ",
        ));
    }
    Ok(output)
}

#[cfg(not(feature = "jxl"))]
pub(crate) fn encode(_input: EncodeInput<'_>, _options: EncodeOptions) -> Result<Vec<u8>> {
    Err(crate::codecs::unavailable(
        crate::settings::ImageFormat::Jxl,
    ))
}
