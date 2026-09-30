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
        ImageFormat::Avif => decode_avif(bytes, limits)?,
        ImageFormat::Jxl => decode_jxl(bytes, limits)?,
        ImageFormat::Heic => decode_heic(bytes, limits)?,
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

/// EXIF orientation equivalent to an ISOBMFF `irot` (counter-clockwise
/// degrees) followed by an `imir` (0: left-right flip, 1: top-bottom flip).
#[cfg(feature = "avif")]
pub(crate) fn orientation_from_irot_imir(rotation_ccw: u16, mirror: Option<u8>) -> Orientation {
    let clockwise = (360 - u32::from(rotation_ccw % 360)) % 360;
    match (clockwise, mirror) {
        (0, None) => Orientation::NoTransforms,
        (90, None) => Orientation::Rotate90,
        (180, None) => Orientation::Rotate180,
        (270, None) => Orientation::Rotate270,
        (0, Some(0)) | (180, Some(1)) => Orientation::FlipHorizontal,
        (0, Some(_)) | (180, Some(_)) => Orientation::FlipVertical,
        (90, Some(0)) | (270, Some(1)) => Orientation::Rotate90FlipH,
        (90, Some(_)) | (270, Some(_)) => Orientation::Rotate270FlipH,
        _ => Orientation::NoTransforms,
    }
}

/// An ICC profile for CICP-signalled wide-gamut images (Display P3 with the
/// sRGB curve, the iPhone default), so colors survive conversion or embedding.
/// sRGB/BT.709 and unspecified need no profile; other spaces are left alone.
#[cfg(any(feature = "avif", feature = "heic"))]
fn icc_for_cicp(primaries: u16, transfer: u16) -> Option<Vec<u8>> {
    // Primaries 12 = SMPTE EG 432 (P3-D65); transfer 13 = sRGB, 1/6 = BT.709-like.
    if primaries == 12 && matches!(transfer, 1 | 6 | 13) {
        moxcms::ColorProfile::new_display_p3().encode().ok()
    } else {
        None
    }
}

/// Converts zenavif's pixel buffer (8- or 16-bit, gray or color) into an image.
#[cfg(feature = "avif")]
pub(crate) fn avif_pixels(buffer: &zenavif::PixelBuffer) -> Result<DynamicImage> {
    use image::{ImageBuffer, Luma, LumaA, Rgb, Rgba};
    use zenpixels::PixelDescriptor;

    let failed = || crate::errors::ImagoError::Decode {
        format: ImageFormat::Avif.display_name().into(),
        message: "decoded buffer has an unexpected layout".into(),
    };
    let (width, height) = (buffer.width(), buffer.height());
    let descriptor = buffer.descriptor();
    let bytes = buffer.copy_to_contiguous_bytes();
    let wide = |bytes: &[u8]| -> Vec<u16> {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_ne_bytes(*pair))
            .collect()
    };
    let image = if descriptor.layout_compatible(PixelDescriptor::RGB8) {
        DynamicImage::ImageRgb8(
            ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, bytes).ok_or_else(failed)?,
        )
    } else if descriptor.layout_compatible(PixelDescriptor::RGBA8) {
        DynamicImage::ImageRgba8(
            ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, bytes).ok_or_else(failed)?,
        )
    } else if descriptor.layout_compatible(PixelDescriptor::GRAY8) {
        DynamicImage::ImageLuma8(
            ImageBuffer::<Luma<u8>, _>::from_raw(width, height, bytes).ok_or_else(failed)?,
        )
    } else if descriptor.layout_compatible(PixelDescriptor::GRAYA8) {
        DynamicImage::ImageLumaA8(
            ImageBuffer::<LumaA<u8>, _>::from_raw(width, height, bytes).ok_or_else(failed)?,
        )
    } else if descriptor.layout_compatible(PixelDescriptor::RGB16) {
        DynamicImage::ImageRgb16(
            ImageBuffer::<Rgb<u16>, _>::from_raw(width, height, wide(&bytes)).ok_or_else(failed)?,
        )
    } else if descriptor.layout_compatible(PixelDescriptor::RGBA16) {
        DynamicImage::ImageRgba16(
            ImageBuffer::<Rgba<u16>, _>::from_raw(width, height, wide(&bytes))
                .ok_or_else(failed)?,
        )
    } else if descriptor.layout_compatible(PixelDescriptor::GRAY16) {
        DynamicImage::ImageLuma16(
            ImageBuffer::<Luma<u16>, _>::from_raw(width, height, wide(&bytes))
                .ok_or_else(failed)?,
        )
    } else if descriptor.layout_compatible(PixelDescriptor::GRAYA16) {
        DynamicImage::ImageLumaA16(
            ImageBuffer::<LumaA<u16>, _>::from_raw(width, height, wide(&bytes))
                .ok_or_else(failed)?,
        )
    } else {
        return Err(failed());
    };
    Ok(image)
}

/// Maps a zenavif error onto Imago's errors.
#[cfg(feature = "avif")]
pub(crate) fn avif_error(error: zenavif::Error) -> crate::errors::ImagoError {
    match error {
        zenavif::Error::ImageTooLarge { width, height } => {
            crate::errors::ImagoError::DimensionsTooLarge {
                width,
                height,
                reason: "the image exceeds the configured pixel limit".into(),
            }
        }
        zenavif::Error::Cancelled(_) => crate::errors::ImagoError::Cancelled,
        other => crate::errors::ImagoError::Decode {
            format: ImageFormat::Avif.display_name().into(),
            message: other.to_string(),
        },
    }
}

/// zenavif decoder settings enforcing Imago's pixel limit.
#[cfg(feature = "avif")]
pub(crate) fn avif_config(limits: &Limits) -> zenavif::DecoderConfig {
    zenavif::DecoderConfig::new()
        .frame_size_limit(u32::try_from(limits.max_pixels).unwrap_or(u32::MAX))
}

/// Decodes a still AVIF. Pixels come from avif-decode (rav1d), which handles
/// every matrix including identity (RGB-coded) AVIF; orientation, color and
/// EXIF come from the container.
#[cfg(feature = "avif")]
fn decode_avif(bytes: &[u8], _limits: &Limits) -> Result<DecodedImage> {
    use avif_decode::Image;
    use image::{ImageBuffer, Luma, Rgb, Rgba};
    use rgb::bytemuck;
    use zenavif_parse::{AvifParser, ColorInformation};

    let error = |message: String| crate::errors::ImagoError::Decode {
        format: ImageFormat::Avif.display_name().into(),
        message,
    };
    let parser = AvifParser::from_bytes(bytes).map_err(|e| error(e.to_string()))?;
    let orientation = orientation_from_irot_imir(
        parser.rotation().map_or(0, |rotation| rotation.angle),
        parser.mirror().map(|mirror| mirror.axis),
    );
    let icc_profile = match parser.color_info() {
        Some(ColorInformation::IccProfile(icc)) if !icc.is_empty() => Some(icc.clone()),
        Some(ColorInformation::Nclx {
            color_primaries,
            transfer_characteristics,
            ..
        }) => icc_for_cicp(*color_primaries, *transfer_characteristics),
        _ => None,
    };
    let exif = parser
        .exif()
        .and_then(|exif| exif.ok())
        .filter(|exif| !exif.is_empty())
        .map(|exif| exif.into_owned());
    drop(parser);

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
        icc_profile,
        exif,
        orientation,
    })
}

#[cfg(not(feature = "avif"))]
fn decode_avif(_bytes: &[u8], _limits: &Limits) -> Result<DecodedImage> {
    Err(crate::codecs::unavailable(ImageFormat::Avif))
}

/// Maps a heic-rs error onto Imago's errors.
#[cfg(feature = "heic")]
pub(crate) fn heic_error(error: heic_rs::Error) -> crate::errors::ImagoError {
    crate::errors::ImagoError::Decode {
        format: ImageFormat::Heic.display_name().into(),
        message: error.to_string(),
    }
}

/// Decodes HEIC. Rotation, mirroring and the clean aperture are applied by
/// the decoder, so no EXIF orientation is applied afterwards.
#[cfg(feature = "heic")]
fn decode_heic(bytes: &[u8], limits: &Limits) -> Result<DecodedImage> {
    use heic_rs::{DecodeOptions, PixelLayout};
    use image::{ImageBuffer, Rgb, Rgba};

    let info = heic_rs::probe(bytes).map_err(heic_error)?;
    let layout = match (info.bit_depth > 8, info.has_alpha) {
        (false, false) => PixelLayout::Rgb8,
        (false, true) => PixelLayout::Rgba8,
        (true, false) => PixelLayout::Rgb16,
        (true, true) => PixelLayout::Rgba16,
    };
    let options = DecodeOptions {
        layout,
        max_pixels: Some(limits.max_pixels),
        apply_transforms: true,
        decode_alpha: info.has_alpha,
        ..DecodeOptions::default()
    };
    let decoded = heic_rs::decode(bytes, &options).map_err(heic_error)?;
    let failed = || crate::errors::ImagoError::Decode {
        format: ImageFormat::Heic.display_name().into(),
        message: "decoded buffer has an unexpected size".into(),
    };
    let (width, height) = (decoded.width, decoded.height);
    let wide = |data: &[u8]| -> Vec<u16> {
        data.as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_ne_bytes(*pair))
            .collect()
    };
    let image = match decoded.layout {
        PixelLayout::Rgba8 => DynamicImage::ImageRgba8(
            ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, decoded.data).ok_or_else(failed)?,
        ),
        PixelLayout::Rgb16 => DynamicImage::ImageRgb16(
            ImageBuffer::<Rgb<u16>, _>::from_raw(width, height, wide(&decoded.data))
                .ok_or_else(failed)?,
        ),
        PixelLayout::Rgba16 => DynamicImage::ImageRgba16(
            ImageBuffer::<Rgba<u16>, _>::from_raw(width, height, wide(&decoded.data))
                .ok_or_else(failed)?,
        ),
        _ => DynamicImage::ImageRgb8(
            ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, decoded.data).ok_or_else(failed)?,
        ),
    };

    // Color profile and EXIF of the primary item.
    let context = heic_rs::context::Context::open(bytes).map_err(heic_error)?;
    let primary = context.meta.primary;
    let props = context.props(primary).map_err(heic_error)?;
    let icc_profile = props
        .icc
        .filter(|icc| !icc.is_empty())
        .map(<[u8]>::to_vec)
        .or_else(|| {
            props
                .nclx
                .as_ref()
                .and_then(|nclx| icc_for_cicp(nclx.primaries, nclx.transfer))
        });
    let exif = context
        .exif(primary)
        .ok()
        .flatten()
        .filter(|exif| !exif.is_empty())
        .map(<[u8]>::to_vec);
    Ok(DecodedImage {
        image,
        icc_profile,
        exif,
        orientation: Orientation::NoTransforms,
    })
}

#[cfg(not(feature = "heic"))]
fn decode_heic(_bytes: &[u8], _limits: &Limits) -> Result<DecodedImage> {
    Err(crate::codecs::unavailable(ImageFormat::Heic))
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

    #[cfg(feature = "avif")]
    #[test]
    fn irot_and_imir_map_onto_exif_orientations() {
        assert_eq!(
            orientation_from_irot_imir(0, None),
            Orientation::NoTransforms
        );
        // `irot` turns counter-clockwise; EXIF 6 ("Rotate90") turns clockwise.
        assert_eq!(orientation_from_irot_imir(270, None), Orientation::Rotate90);
        assert_eq!(orientation_from_irot_imir(90, None), Orientation::Rotate270);
        assert_eq!(
            orientation_from_irot_imir(180, None),
            Orientation::Rotate180
        );
        assert_eq!(
            orientation_from_irot_imir(0, Some(0)),
            Orientation::FlipHorizontal
        );
        assert_eq!(
            orientation_from_irot_imir(0, Some(1)),
            Orientation::FlipVertical
        );
        assert_eq!(
            orientation_from_irot_imir(180, Some(0)),
            Orientation::FlipVertical
        );
        assert_eq!(
            orientation_from_irot_imir(270, Some(0)),
            Orientation::Rotate90FlipH
        );
    }
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
