//! Header analysis without full decoding.
//!
//! [`probe`] reads only what each format needs to report dimensions, alpha,
//! animation and metadata presence. Limits are enforced here, *before* any
//! pixel buffer is allocated, so decompression bombs are rejected cheaply.

use std::io::{BufRead, BufReader, Cursor, Seek, SeekFrom};
use std::path::Path;

use image::codecs::bmp::BmpDecoder;
use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::tiff::TiffDecoder;
use image::codecs::webp::WebPDecoder;
use image::metadata::Orientation;
use image::{ColorType, ImageDecoder};
use serde::Serialize;

use crate::errors::{ImagoError, Result};
use crate::image_engine::detection::{self, Detected};
use crate::settings::{ImageFormat, Limits};

/// What the header of an image reveals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageProbe {
    /// Real (content-detected) format.
    pub format: ImageFormat,
    /// Width in pixels as stored (before EXIF orientation).
    pub width: u32,
    /// Height in pixels as stored (before EXIF orientation).
    pub height: u32,
    /// The pixel format has an alpha channel (it may still be fully opaque).
    pub has_alpha: bool,
    /// The image is an animation with more than one frame.
    pub animated: bool,
    /// Bits per channel (8, 16, ...).
    pub bit_depth: u8,
    /// The image is grayscale.
    pub grayscale: bool,
    /// An ICC profile is embedded.
    pub has_icc: bool,
    /// EXIF metadata is embedded.
    pub has_exif: bool,
    /// EXIF orientation (1 = normal ... 8).
    pub orientation: u8,
    /// Size of the file in bytes.
    pub file_size: u64,
}

impl ImageProbe {
    /// Dimensions after applying the EXIF orientation.
    pub fn display_dimensions(&self) -> (u32, u32) {
        if matches!(self.orientation, 5..=8) {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// Number of pixels.
    pub fn pixel_count(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// Probes a file on disk. Only headers are read for most formats.
pub fn probe_file(path: &Path, limits: &Limits) -> Result<ImageProbe> {
    let metadata = std::fs::metadata(path).map_err(|error| ImagoError::io(path, error))?;
    if !metadata.is_file() {
        return Err(ImagoError::NotAFile {
            path: path.to_path_buf(),
        });
    }
    check_file_size(metadata.len(), limits)?;
    let format = detection::detect_file(path)?.require_supported()?;
    let file = std::fs::File::open(path).map_err(|error| ImagoError::io(path, error))?;
    probe_reader(BufReader::new(file), format, metadata.len(), limits)
}

/// Probes an in-memory file.
pub fn probe_bytes(bytes: &[u8], limits: &Limits) -> Result<ImageProbe> {
    check_file_size(bytes.len() as u64, limits)?;
    let format = sniff_format(bytes)?;
    probe_reader(Cursor::new(bytes), format, bytes.len() as u64, limits)
}

/// Detects the format of in-memory data.
pub fn sniff_format(bytes: &[u8]) -> Result<ImageFormat> {
    let head = &bytes[..bytes.len().min(detection::SNIFF_LEN)];
    match detection::sniff(head) {
        Detected::Supported(format) => Ok(format),
        other => other.require_supported(),
    }
}

/// Rejects files above the configured size limit.
pub fn check_file_size(size: u64, limits: &Limits) -> Result<()> {
    if size > limits.max_file_bytes {
        return Err(ImagoError::FileTooLarge {
            size,
            limit: limits.max_file_bytes,
        });
    }
    Ok(())
}

/// Rejects dimensions above the configured limits.
pub fn check_dimensions(width: u32, height: u32, limits: &Limits) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(ImagoError::Decode {
            format: "image".into(),
            message: format!("invalid dimensions {width}x{height}"),
        });
    }
    if width > limits.max_dimension || height > limits.max_dimension {
        return Err(ImagoError::DimensionsTooLarge {
            width,
            height,
            reason: format!("maximum side is {} px", limits.max_dimension),
        });
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.max_pixels {
        return Err(ImagoError::DimensionsTooLarge {
            width,
            height,
            reason: format!(
                "maximum is {:.1} megapixels",
                limits.max_pixels as f64 / 1_000_000.0
            ),
        });
    }
    Ok(())
}

/// Probes a seekable reader whose format is already known.
pub fn probe_reader<R: BufRead + Seek>(
    reader: R,
    format: ImageFormat,
    file_size: u64,
    limits: &Limits,
) -> Result<ImageProbe> {
    let probe = super::guard_decoder(format, || probe_format(reader, format, file_size))?;
    check_dimensions(probe.width, probe.height, limits)?;
    Ok(probe)
}

fn probe_format<R: BufRead + Seek>(
    reader: R,
    format: ImageFormat,
    file_size: u64,
) -> Result<ImageProbe> {
    Ok(match format {
        ImageFormat::Jpeg => {
            // The JPEG decoder buffers the whole file anyway; keep the bytes to
            // read metadata blocks it does not report.
            let mut bytes = Vec::new();
            let mut reader = reader;
            reader
                .read_to_end(&mut bytes)
                .map_err(|e| ImagoError::Decode {
                    format: format.display_name().into(),
                    message: e.to_string(),
                })?;
            let decoder =
                JpegDecoder::new(Cursor::new(&bytes)).map_err(|e| decode_error(format, e))?;
            let mut probe = from_decoder(decoder, format, file_size, false)?;
            probe.has_icc |= crate::metadata::container_icc(&bytes, format).is_some();
            probe
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::with_limits(reader, image::Limits::no_limits())
                .map_err(|e| decode_error(format, e))?;
            let animated = decoder.is_apng().map_err(|e| decode_error(format, e))?;
            from_decoder(decoder, format, file_size, animated)?
        }
        ImageFormat::Webp => {
            let decoder = WebPDecoder::new(reader).map_err(|e| decode_error(format, e))?;
            let animated = decoder.has_animation();
            from_decoder(decoder, format, file_size, animated)?
        }
        ImageFormat::Gif => probe_gif(reader, file_size)?,
        ImageFormat::Tiff => {
            let decoder = TiffDecoder::new(reader).map_err(|e| decode_error(format, e))?;
            from_decoder(decoder, format, file_size, false)?
        }
        ImageFormat::Bmp => {
            let decoder = BmpDecoder::new(reader).map_err(|e| decode_error(format, e))?;
            from_decoder(decoder, format, file_size, false)?
        }
        ImageFormat::Avif => probe_avif(reader, file_size)?,
        ImageFormat::Jxl => probe_jxl(reader, file_size)?,
        ImageFormat::Heic => probe_heic(reader, file_size)?,
    })
}

fn from_decoder(
    mut decoder: impl ImageDecoder,
    format: ImageFormat,
    file_size: u64,
    animated: bool,
) -> Result<ImageProbe> {
    let (width, height) = decoder.dimensions();
    let color = decoder.color_type();
    let has_icc = decoder
        .icc_profile()
        .map_err(|e| decode_error(format, e))?
        .is_some_and(|icc| !icc.is_empty());
    let exif = decoder
        .exif_metadata()
        .map_err(|e| decode_error(format, e))?;
    let has_exif = exif.as_ref().is_some_and(|exif| !exif.is_empty());
    let orientation = exif
        .as_deref()
        .and_then(Orientation::from_exif_chunk)
        .or_else(|| decoder.orientation().ok())
        .map(orientation_to_exif)
        .unwrap_or(1);
    Ok(ImageProbe {
        format,
        width,
        height,
        has_alpha: color.has_alpha(),
        animated,
        bit_depth: bits_per_channel(color),
        grayscale: !color.has_color(),
        has_icc,
        has_exif,
        orientation,
        file_size,
    })
}

fn probe_gif<R: BufRead + Seek>(mut reader: R, file_size: u64) -> Result<ImageProbe> {
    let format = ImageFormat::Gif;
    // The image crate gives dimensions; the gif crate lets us count frames
    // without decoding pixels.
    let (width, height) = {
        let decoder = GifDecoder::new(&mut reader).map_err(|e| decode_error(format, e))?;
        decoder.dimensions()
    };
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|e| ImagoError::Decode {
            format: format.display_name().into(),
            message: e.to_string(),
        })?;
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = options.read_info(reader).map_err(|e| ImagoError::Decode {
        format: format.display_name().into(),
        message: e.to_string(),
    })?;
    let mut frames = 0u32;
    let mut transparent = false;
    while frames < 2 {
        match decoder.next_frame_info() {
            Ok(Some(frame)) => {
                transparent |= frame.transparent.is_some();
                frames += 1;
            }
            Ok(None) => break,
            Err(error) => {
                // A broken trailing frame still leaves a usable first frame.
                if frames == 0 {
                    return Err(ImagoError::Decode {
                        format: format.display_name().into(),
                        message: error.to_string(),
                    });
                }
                break;
            }
        }
    }
    Ok(ImageProbe {
        format,
        width,
        height,
        // GIF frames are decoded to RGBA; transparency is only known per frame.
        has_alpha: transparent,
        animated: frames > 1,
        bit_depth: 8,
        grayscale: false,
        has_icc: false,
        has_exif: false,
        orientation: 1,
        file_size,
    })
}

#[cfg(feature = "avif")]
fn probe_avif<R: BufRead + Seek>(mut reader: R, file_size: u64) -> Result<ImageProbe> {
    use zenavif_parse::{AvifParser, ColorInformation, DecodeConfig};

    let format = ImageFormat::Avif;
    let error = |message: String| ImagoError::Decode {
        format: format.display_name().into(),
        message,
    };
    let parser = AvifParser::from_reader_with_config(
        &mut reader,
        &DecodeConfig::default().lenient(true),
        &enough::Unstoppable,
    )
    .map_err(|e| error(e.to_string()))?;
    let meta = parser
        .primary_metadata()
        .map_err(|e| error(e.to_string()))?;
    let (width, height) = match parser.grid_config() {
        Some(grid) => (grid.output_width, grid.output_height),
        None => (meta.max_frame_width.get(), meta.max_frame_height.get()),
    };
    let animated = parser
        .animation_info()
        .is_some_and(|animation| animation.frame_count > 1);
    let orientation = crate::image_engine::decode::orientation_from_irot_imir(
        parser.rotation().map_or(0, |rotation| rotation.angle),
        parser.mirror().map(|mirror| mirror.axis),
    );
    Ok(ImageProbe {
        format,
        width,
        height,
        has_alpha: parser.alpha_data().is_some()
            || parser
                .animation_info()
                .is_some_and(|animation| animation.has_alpha),
        animated,
        bit_depth: meta.bit_depth,
        grayscale: meta.monochrome,
        has_icc: matches!(parser.color_info(), Some(ColorInformation::IccProfile(_))),
        has_exif: parser.exif().is_some(),
        orientation: orientation_to_exif(orientation),
        file_size,
    })
}

#[cfg(not(feature = "avif"))]
fn probe_avif<R: BufRead + Seek>(_reader: R, _file_size: u64) -> Result<ImageProbe> {
    Err(crate::codecs::unavailable(ImageFormat::Avif))
}

/// HEIC: dimensions are reported after `irot`/`imir`, which the decoder
/// applies itself, so the probe never asks for an EXIF rotation.
#[cfg(feature = "heic")]
fn probe_heic<R: BufRead + Seek>(mut reader: R, file_size: u64) -> Result<ImageProbe> {
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| ImagoError::Decode {
            format: ImageFormat::Heic.display_name().into(),
            message: e.to_string(),
        })?;
    let info = heic_rs::probe(&bytes).map_err(crate::image_engine::decode::heic_error)?;
    Ok(ImageProbe {
        format: ImageFormat::Heic,
        width: info.width,
        height: info.height,
        has_alpha: info.has_alpha,
        animated: false,
        bit_depth: info.bit_depth,
        grayscale: matches!(info.chroma, heic_rs::ChromaFormat::Monochrome),
        has_icc: info.has_icc,
        has_exif: info.has_exif,
        orientation: 1,
        file_size,
    })
}

#[cfg(not(feature = "heic"))]
fn probe_heic<R: BufRead + Seek>(_reader: R, _file_size: u64) -> Result<ImageProbe> {
    Err(crate::codecs::unavailable(ImageFormat::Heic))
}

#[cfg(feature = "jxl")]
fn probe_jxl<R: BufRead + Seek>(reader: R, file_size: u64) -> Result<ImageProbe> {
    let format = ImageFormat::Jxl;
    let image = jxl_oxide::JxlImage::builder()
        .read(reader)
        .map_err(|e| ImagoError::Decode {
            format: format.display_name().into(),
            message: e.to_string(),
        })?;
    let metadata = &image.image_header().metadata;
    let bit_depth = u8::try_from(metadata.bit_depth.bits_per_sample()).unwrap_or(16);
    Ok(ImageProbe {
        format,
        // jxl-oxide reports (and renders) dimensions with orientation applied.
        width: image.width(),
        height: image.height(),
        has_alpha: metadata.alpha().is_some(),
        animated: metadata.animation.is_some(),
        bit_depth,
        grayscale: metadata.grayscale(),
        has_icc: image.original_icc().is_some(),
        has_exif: false,
        orientation: 1,
        file_size,
    })
}

#[cfg(not(feature = "jxl"))]
fn probe_jxl<R: BufRead + Seek>(_reader: R, _file_size: u64) -> Result<ImageProbe> {
    Err(crate::codecs::unavailable(ImageFormat::Jxl))
}

/// Converts an [`Orientation`] into the EXIF tag value (1..=8).
pub fn orientation_to_exif(orientation: Orientation) -> u8 {
    match orientation {
        Orientation::NoTransforms => 1,
        Orientation::FlipHorizontal => 2,
        Orientation::Rotate180 => 3,
        Orientation::FlipVertical => 4,
        Orientation::Rotate90FlipH => 5,
        Orientation::Rotate90 => 6,
        Orientation::Rotate270FlipH => 7,
        Orientation::Rotate270 => 8,
    }
}

/// Converts an EXIF orientation value into an [`Orientation`].
pub fn orientation_from_exif(value: u8) -> Orientation {
    Orientation::from_exif(value).unwrap_or(Orientation::NoTransforms)
}

fn bits_per_channel(color: ColorType) -> u8 {
    let channels = color.channel_count().max(1);
    color.bytes_per_pixel() / channels * 8
}

pub(crate) fn decode_error(format: ImageFormat, error: image::ImageError) -> ImagoError {
    match error {
        image::ImageError::Limits(limit) => ImagoError::DimensionsTooLarge {
            width: 0,
            height: 0,
            reason: limit.to_string(),
        },
        other => ImagoError::Decode {
            format: format.display_name().into(),
            message: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgba};

    fn encode(image: &DynamicImage, format: image::ImageFormat) -> Vec<u8> {
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), format)
            .unwrap();
        bytes
    }

    fn rgba(width: u32, height: u32, alpha: u8) -> DynamicImage {
        DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            width,
            height,
            Rgba([10, 20, 30, alpha]),
        ))
    }

    #[test]
    fn probes_png_dimensions_and_alpha() {
        let bytes = encode(&rgba(40, 30, 128), image::ImageFormat::Png);
        let probe = probe_bytes(&bytes, &Limits::default()).unwrap();
        assert_eq!(probe.format, ImageFormat::Png);
        assert_eq!((probe.width, probe.height), (40, 30));
        assert!(probe.has_alpha);
        assert!(!probe.animated);
        assert_eq!(probe.bit_depth, 8);
        assert_eq!(probe.file_size, bytes.len() as u64);
    }

    #[test]
    fn probes_jpeg_without_alpha() {
        let image = DynamicImage::ImageRgb8(image::RgbImage::new(16, 8));
        let bytes = encode(&image, image::ImageFormat::Jpeg);
        let probe = probe_bytes(&bytes, &Limits::default()).unwrap();
        assert_eq!(probe.format, ImageFormat::Jpeg);
        assert_eq!((probe.width, probe.height), (16, 8));
        assert!(!probe.has_alpha);
        assert_eq!(probe.orientation, 1);
    }

    #[test]
    fn rejects_oversized_dimensions_before_decoding() {
        let bytes = encode(&rgba(64, 64, 255), image::ImageFormat::Png);
        let limits = Limits {
            max_dimension: 32,
            ..Limits::default()
        };
        let error = probe_bytes(&bytes, &limits).unwrap_err();
        assert_eq!(error.code(), "dimensions_too_large");

        let limits = Limits {
            max_pixels: 1000,
            ..Limits::default()
        };
        let error = probe_bytes(&bytes, &limits).unwrap_err();
        assert_eq!(error.code(), "dimensions_too_large");
    }

    #[test]
    fn rejects_files_above_size_limit() {
        let bytes = encode(&rgba(8, 8, 255), image::ImageFormat::Png);
        let limits = Limits {
            max_file_bytes: 10,
            ..Limits::default()
        };
        assert_eq!(
            probe_bytes(&bytes, &limits).unwrap_err().code(),
            "file_too_large"
        );
    }

    #[test]
    fn truncated_file_is_a_controlled_error() {
        let bytes = encode(&rgba(32, 32, 255), image::ImageFormat::Png);
        let error = probe_bytes(&bytes[..20], &Limits::default()).unwrap_err();
        assert_eq!(error.code(), "decode_failed");
    }

    #[test]
    fn unknown_content_is_rejected() {
        let error = probe_bytes(b"definitely not an image", &Limits::default()).unwrap_err();
        assert_eq!(error.code(), "unsupported_format");
    }

    #[test]
    fn display_dimensions_swap_for_rotated_images() {
        let probe = ImageProbe {
            format: ImageFormat::Jpeg,
            width: 400,
            height: 300,
            has_alpha: false,
            animated: false,
            bit_depth: 8,
            grayscale: false,
            has_icc: false,
            has_exif: true,
            orientation: 6,
            file_size: 1,
        };
        assert_eq!(probe.display_dimensions(), (300, 400));
    }

    #[test]
    fn orientation_round_trips() {
        for value in 1..=8u8 {
            assert_eq!(orientation_to_exif(orientation_from_exif(value)), value);
        }
        assert_eq!(orientation_to_exif(orientation_from_exif(0)), 1);
    }
}
