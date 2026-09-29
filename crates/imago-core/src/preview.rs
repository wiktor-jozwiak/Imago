//! Thumbnails and before/after previews.
//!
//! Any supported format (including TIFF, AVIF and JPEG XL, which webviews may
//! not display) is decoded with the usual limits, oriented, converted to sRGB,
//! downscaled and re-encoded as PNG (with transparency) or JPEG.

use std::io::Cursor;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, ImageEncoder};
use serde::Serialize;

use crate::errors::{ImagoError, Result};
use crate::image_engine::color::{self, IccKind};
use crate::image_engine::resize::{ResizeQuality, resize};
use crate::image_engine::{decode, probe};
use crate::settings::Limits;

/// Largest preview side accepted from callers.
pub const MAX_PREVIEW_SIDE: u32 = 4096;

/// A rendered preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// MIME type of `data` (`image/png` or `image/jpeg`).
    pub mime: &'static str,
    /// Encoded preview.
    #[serde(skip)]
    pub data: Vec<u8>,
    /// Preview width.
    pub width: u32,
    /// Preview height.
    pub height: u32,
    /// Width of the full image (after orientation).
    pub source_width: u32,
    /// Height of the full image (after orientation).
    pub source_height: u32,
}

/// Renders a preview of `path` that fits into `max_side`×`max_side`.
pub fn render_preview(path: &Path, max_side: u32, limits: &Limits) -> Result<Preview> {
    let max_side = max_side.clamp(16, MAX_PREVIEW_SIDE);
    let metadata = std::fs::metadata(path).map_err(|error| ImagoError::io(path, error))?;
    probe::check_file_size(metadata.len(), limits)?;
    let bytes = std::fs::read(path).map_err(|error| ImagoError::io(path, error))?;
    let image_probe = probe::probe_bytes(&bytes, limits)?;
    let decoded = decode::decode(&bytes, &image_probe, limits)?;
    let mut image = decoded.image;
    image.apply_orientation(decoded.orientation);
    let (source_width, source_height) = (image.width(), image.height());

    let scale = (f64::from(max_side) / f64::from(source_width.max(source_height))).min(1.0);
    if scale < 1.0 {
        let width = ((f64::from(source_width) * scale).round() as u32).max(1);
        let height = ((f64::from(source_height) * scale).round() as u32).max(1);
        image = resize(&image, width, height, ResizeQuality::Fast)?;
    }
    if let Some(icc) = decoded.icc_profile.as_deref()
        && color::classify_icc(icc, &image) == IccKind::Other
    {
        image = color::convert_to_srgb(&image, icc)?;
    }
    encode_preview(image, source_width, source_height)
}

fn encode_preview(image: DynamicImage, source_width: u32, source_height: u32) -> Result<Preview> {
    let (width, height) = (image.width(), image.height());
    let mut data = Vec::new();
    let failed =
        |error: image::ImageError| ImagoError::Internal(format!("cannot encode preview: {error}"));
    let mime = if image.color().has_alpha() {
        let rgba = image.to_rgba8();
        PngEncoder::new_with_quality(
            Cursor::new(&mut data),
            CompressionType::Fast,
            FilterType::Adaptive,
        )
        .write_image(
            rgba.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(failed)?;
        "image/png"
    } else {
        let rgb = image.to_rgb8();
        JpegEncoder::new_with_quality(Cursor::new(&mut data), 85)
            .write_image(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)
            .map_err(failed)?;
        "image/jpeg"
    };
    Ok(Preview {
        mime,
        data,
        width,
        height,
        source_width,
        source_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};

    fn write(
        dir: &Path,
        name: &str,
        image: &DynamicImage,
        format: image::ImageFormat,
    ) -> std::path::PathBuf {
        let path = dir.join(name);
        image.save_with_format(&path, format).unwrap();
        path
    }

    #[test]
    fn large_images_are_scaled_down_to_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(800, 400, Rgb([10, 200, 30])));
        let path = write(dir.path(), "a.png", &image, image::ImageFormat::Png);
        let preview = render_preview(&path, 200, &Limits::default()).unwrap();
        assert_eq!((preview.width, preview.height), (200, 100));
        assert_eq!((preview.source_width, preview.source_height), (800, 400));
        assert_eq!(preview.mime, "image/jpeg");
        assert!(preview.data.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn transparent_images_become_png_and_small_ones_keep_size() {
        let dir = tempfile::tempdir().unwrap();
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(30, 20, Rgba([1, 2, 3, 100])));
        let path = write(dir.path(), "t.png", &image, image::ImageFormat::Png);
        let preview = render_preview(&path, 256, &Limits::default()).unwrap();
        assert_eq!(preview.mime, "image/png");
        assert_eq!((preview.width, preview.height), (30, 20));
    }

    #[test]
    fn broken_files_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.png");
        std::fs::write(&path, b"nope").unwrap();
        assert_eq!(
            render_preview(&path, 100, &Limits::default())
                .unwrap_err()
                .code(),
            "unsupported_format"
        );
    }
}
