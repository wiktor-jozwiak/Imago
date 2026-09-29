//! Full decoding of still images with memory limits.
//!
//! Animated inputs are decoded frame by frame in `image_engine::animation`;
//! here an animation yields its first frame (used only with explicit consent).

use std::io::Cursor;

use image::codecs::bmp::BmpDecoder;
use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::tiff::TiffDecoder;
use image::codecs::webp::WebPDecoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder};

use crate::errors::Result;
use crate::image_engine::probe::{ImageProbe, check_dimensions, decode_error};
use crate::settings::{ImageFormat, Limits};

/// A decoded image together with the metadata needed to write it back.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    /// Pixels exactly as stored (EXIF orientation not applied yet).
    pub image: DynamicImage,
    /// Embedded ICC profile.
    pub icc_profile: Option<Vec<u8>>,
    /// Raw EXIF block (TIFF structure, without the `Exif\0\0` prefix).
    pub exif: Option<Vec<u8>>,
    /// EXIF orientation to apply.
    pub orientation: Orientation,
}

/// Upper bound on bytes per pixel of any decoded buffer (RGBA, 32-bit float).
const MAX_BYTES_PER_PIXEL: u64 = 16;

/// Decoder allocation limits derived from Imago's limits.
pub fn decoder_limits(limits: &Limits) -> image::Limits {
    let mut decoder_limits = image::Limits::default();
    decoder_limits.max_image_width = Some(limits.max_dimension);
    decoder_limits.max_image_height = Some(limits.max_dimension);
    decoder_limits.max_alloc = Some(limits.max_pixels.saturating_mul(MAX_BYTES_PER_PIXEL));
    decoder_limits
}

/// Decodes an in-memory file whose header was already probed.
pub fn decode(bytes: &[u8], probe: &ImageProbe, limits: &Limits) -> Result<DecodedImage> {
    check_dimensions(probe.width, probe.height, limits)?;
    let format = probe.format;
    super::guard_decoder(format, || decode_format(bytes, format, limits))
}

fn decode_format(bytes: &[u8], format: ImageFormat, limits: &Limits) -> Result<DecodedImage> {
    let decoded = match format {
        ImageFormat::Jpeg => {
            let decoder =
                JpegDecoder::new(Cursor::new(bytes)).map_err(|e| decode_error(format, e))?;
            decode_with(decoder, format, limits)?
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::with_limits(Cursor::new(bytes), decoder_limits(limits))
                .map_err(|e| decode_error(format, e))?;
            decode_with(decoder, format, limits)?
        }
        ImageFormat::Webp => {
            let decoder =
                WebPDecoder::new(Cursor::new(bytes)).map_err(|e| decode_error(format, e))?;
            decode_with(decoder, format, limits)?
        }
        ImageFormat::Gif => {
            let decoder =
                GifDecoder::new(Cursor::new(bytes)).map_err(|e| decode_error(format, e))?;
            decode_with(decoder, format, limits)?
        }
        ImageFormat::Tiff => {
            let decoder =
                TiffDecoder::new(Cursor::new(bytes)).map_err(|e| decode_error(format, e))?;
            decode_with(decoder, format, limits)?
        }
        ImageFormat::Bmp => {
            let decoder =
                BmpDecoder::new(Cursor::new(bytes)).map_err(|e| decode_error(format, e))?;
            decode_with(decoder, format, limits)?
        }
        ImageFormat::Avif => decode_avif(bytes)?,
        ImageFormat::Jxl => decode_jxl(bytes, limits)?,
    };
    let (width, height) = (decoded.image.width(), decoded.image.height());
    check_dimensions(width, height, limits)?;
    let mut decoded = decoded;
    if decoded.icc_profile.is_none() {
        // Some decoders skip multi-part or late ICC blocks; read the container directly.
        decoded.icc_profile = crate::metadata::container_icc(bytes, format);
    }
    Ok(decoded)
}

fn decode_with(
    mut decoder: impl ImageDecoder,
    format: ImageFormat,
    limits: &Limits,
) -> Result<DecodedImage> {
    // Metadata first: some decoders (TIFF) rebuild their state in `set_limits`.
    let icc_profile = decoder
        .icc_profile()
        .map_err(|e| decode_error(format, e))?
        .filter(|icc| !icc.is_empty());
    let exif = decoder
        .exif_metadata()
        .map_err(|e| decode_error(format, e))?
        .filter(|exif| !exif.is_empty());
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    decoder
        .set_limits(decoder_limits(limits))
        .map_err(|e| decode_error(format, e))?;
    let image = DynamicImage::from_decoder(decoder).map_err(|e| decode_error(format, e))?;
    Ok(DecodedImage {
        image,
        icc_profile,
        exif,
        orientation,
    })
}

#[cfg(feature = "avif")]
fn decode_avif(bytes: &[u8]) -> Result<DecodedImage> {
    use avif_decode::Image;
    use image::{ImageBuffer, Luma, Rgb, Rgba};
    use rgb::bytemuck;

    let error = |message: String| crate::errors::ImagoError::Decode {
        format: ImageFormat::Avif.display_name().into(),
        message,
    };
    let decoded = avif_decode::Decoder::from_avif(bytes)
        .and_then(|decoder| decoder.to_image())
        .map_err(|e| error(e.to_string()))?;

    macro_rules! convert {
        ($img:expr, $pixel:ty, $variant:ident) => {{
            let (pixels, width, height) = $img.into_contiguous_buf();
            let samples = bytemuck::cast_slice(&pixels).to_vec();
            let buffer = ImageBuffer::<$pixel, _>::from_raw(width as u32, height as u32, samples)
                .ok_or_else(|| error("decoded buffer has an unexpected size".into()))?;
            DynamicImage::$variant(buffer)
        }};
    }

    let image = match decoded {
        Image::Rgb8(img) => convert!(img, Rgb<u8>, ImageRgb8),
        Image::Rgba8(img) => convert!(img, Rgba<u8>, ImageRgba8),
        Image::Gray8(img) => convert!(img, Luma<u8>, ImageLuma8),
        Image::Rgb16(img) => convert!(img, Rgb<u16>, ImageRgb16),
        Image::Rgba16(img) => convert!(img, Rgba<u16>, ImageRgba16),
        Image::Gray16(img) => convert!(img, Luma<u16>, ImageLuma16),
    };
    Ok(DecodedImage {
        image,
        icc_profile: None,
        exif: None,
        orientation: Orientation::NoTransforms,
    })
}

#[cfg(not(feature = "avif"))]
fn decode_avif(_bytes: &[u8]) -> Result<DecodedImage> {
    Err(crate::codecs::unavailable(ImageFormat::Avif))
}

#[cfg(feature = "jxl")]
fn decode_jxl(bytes: &[u8], limits: &Limits) -> Result<DecodedImage> {
    let format = ImageFormat::Jxl;
    let mut decoder = jxl_oxide::integration::JxlDecoder::new(Cursor::new(bytes))
        .map_err(|e| decode_error(format, e))?;
    decoder
        .set_limits(decoder_limits(limits))
        .map_err(|e| decode_error(format, e))?;
    let icc_profile = decoder
        .icc_profile()
        .map_err(|e| decode_error(format, e))?
        .filter(|icc| !icc.is_empty());
    let image = DynamicImage::from_decoder(decoder).map_err(|e| decode_error(format, e))?;
    // jxl-oxide renders pixels with the orientation already applied.
    Ok(DecodedImage {
        image,
        icc_profile,
        exif: None,
        orientation: Orientation::NoTransforms,
    })
}

#[cfg(not(feature = "jxl"))]
fn decode_jxl(_bytes: &[u8], _limits: &Limits) -> Result<DecodedImage> {
    Err(crate::codecs::unavailable(ImageFormat::Jxl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::probe::probe_bytes;
    use image::{ImageBuffer, Rgba};

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(width, height, |x, y| {
            Rgba([x as u8, y as u8, 7, 200])
        }));
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn decodes_png_pixels_exactly() {
        let bytes = png_bytes(20, 10);
        let limits = Limits::default();
        let probe = probe_bytes(&bytes, &limits).unwrap();
        let decoded = decode(&bytes, &probe, &limits).unwrap();
        let rgba = decoded.image.to_rgba8();
        assert_eq!(rgba.dimensions(), (20, 10));
        assert_eq!(rgba.get_pixel(5, 3).0, [5, 3, 7, 200]);
        assert_eq!(decoded.orientation, Orientation::NoTransforms);
    }

    #[test]
    fn corrupted_data_is_a_decode_error() {
        let mut bytes = png_bytes(64, 64);
        let limits = Limits::default();
        let probe = probe_bytes(&bytes, &limits).unwrap();
        // Keep the header intact but destroy the compressed pixel data.
        let len = bytes.len();
        for byte in &mut bytes[60..len - 12] {
            *byte = 0xAB;
        }
        let error = decode(&bytes, &probe, &limits).unwrap_err();
        assert_eq!(error.code(), "decode_failed");
    }

    #[test]
    fn decoder_limits_follow_imago_limits() {
        let limits = Limits {
            max_dimension: 1000,
            max_pixels: 10,
            ..Limits::default()
        };
        let decoder_limits = decoder_limits(&limits);
        assert_eq!(decoder_limits.max_image_width, Some(1000));
        assert_eq!(decoder_limits.max_alloc, Some(160));
    }
}
