//! Pixel analysis and color-space handling.
//!
//! * alpha usage (is the alpha channel really needed?),
//! * color counting (photo vs. flat graphic),
//! * flattening transparency onto a background (only with user consent),
//! * ICC profiles: sRGB detection and conversion to sRGB via `moxcms`.

use std::collections::HashSet;

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgb, Rgba};
use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};

use crate::errors::{ImagoError, Result};

/// How an image uses its alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaUsage {
    /// The pixel format has no alpha channel.
    None,
    /// There is an alpha channel but every pixel is fully opaque.
    Opaque,
    /// Pixels are either fully transparent or fully opaque.
    Binary,
    /// Some pixels are partially transparent.
    Partial,
}

impl AlphaUsage {
    /// Whether any pixel is (partially) transparent.
    pub fn is_transparent(self) -> bool {
        matches!(self, AlphaUsage::Binary | AlphaUsage::Partial)
    }
}

/// Inspects the alpha channel.
pub fn alpha_usage(image: &DynamicImage) -> AlphaUsage {
    if !image.color().has_alpha() {
        return AlphaUsage::None;
    }
    let mut usage = AlphaUsage::Opaque;
    match image {
        DynamicImage::ImageLumaA8(buffer) => {
            for pixel in buffer.pixels() {
                usage = combine(usage, u16::from(pixel.0[1]), 255);
                if usage == AlphaUsage::Partial {
                    break;
                }
            }
        }
        DynamicImage::ImageRgba8(buffer) => {
            for pixel in buffer.pixels() {
                usage = combine(usage, u16::from(pixel.0[3]), 255);
                if usage == AlphaUsage::Partial {
                    break;
                }
            }
        }
        DynamicImage::ImageLumaA16(buffer) => {
            for pixel in buffer.pixels() {
                usage = combine(usage, pixel.0[1], u16::MAX);
                if usage == AlphaUsage::Partial {
                    break;
                }
            }
        }
        DynamicImage::ImageRgba16(buffer) => {
            for pixel in buffer.pixels() {
                usage = combine(usage, pixel.0[3], u16::MAX);
                if usage == AlphaUsage::Partial {
                    break;
                }
            }
        }
        other => {
            for (_, _, pixel) in other.pixels() {
                usage = combine(usage, u16::from(pixel.0[3]), 255);
                if usage == AlphaUsage::Partial {
                    break;
                }
            }
        }
    }
    usage
}

fn combine(current: AlphaUsage, alpha: u16, max: u16) -> AlphaUsage {
    if alpha == max {
        current
    } else if alpha == 0 {
        match current {
            AlphaUsage::Partial => AlphaUsage::Partial,
            _ => AlphaUsage::Binary,
        }
    } else {
        AlphaUsage::Partial
    }
}

/// Counts distinct RGBA colors, stopping once `limit` is exceeded.
///
/// Returns a value in `0..=limit + 1`; `limit + 1` means "more than `limit`".
pub fn count_colors_up_to(image: &DynamicImage, limit: usize) -> usize {
    let rgba = match image {
        DynamicImage::ImageRgba8(buffer) => std::borrow::Cow::Borrowed(buffer),
        other => std::borrow::Cow::Owned(other.to_rgba8()),
    };
    let mut colors: HashSet<u32> = HashSet::with_capacity(limit.min(4096) + 1);
    for pixel in rgba.pixels() {
        colors.insert(u32::from_le_bytes(pixel.0));
        if colors.len() > limit {
            return limit + 1;
        }
    }
    colors.len()
}

/// Removes an alpha channel that is fully opaque (lossless).
pub fn drop_opaque_alpha(image: DynamicImage) -> DynamicImage {
    match image {
        DynamicImage::ImageLumaA8(_) => DynamicImage::ImageLuma8(image.to_luma8()),
        DynamicImage::ImageRgba8(_) => DynamicImage::ImageRgb8(image.to_rgb8()),
        DynamicImage::ImageLumaA16(_) => DynamicImage::ImageLuma16(image.to_luma16()),
        DynamicImage::ImageRgba16(_) => DynamicImage::ImageRgb16(image.to_rgb16()),
        DynamicImage::ImageRgba32F(_) => DynamicImage::ImageRgb32F(image.to_rgb32f()),
        other => other,
    }
}

/// Composites the image onto an opaque background color (8-bit output).
///
/// Only called when the user explicitly allowed removing transparency.
pub fn flatten_onto(image: &DynamicImage, background: [u8; 3]) -> DynamicImage {
    if !image.color().has_alpha() {
        return image.clone();
    }
    let grayscale = !image.color().has_color();
    let rgba = image.to_rgba8();
    let blend = |value: u8, background: u8, alpha: u8| -> u8 {
        let alpha = u32::from(alpha);
        let mixed = u32::from(value) * alpha + u32::from(background) * (255 - alpha);
        ((mixed + 127) / 255) as u8
    };
    let flattened: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_fn(rgba.width(), rgba.height(), |x, y| {
            let [r, g, b, a] = rgba.get_pixel(x, y).0;
            Rgb([
                blend(r, background[0], a),
                blend(g, background[1], a),
                blend(b, background[2], a),
            ])
        });
    let flattened = DynamicImage::ImageRgb8(flattened);
    if grayscale {
        DynamicImage::ImageLuma8(flattened.to_luma8())
    } else {
        flattened
    }
}

/// Result of inspecting an embedded ICC profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IccKind {
    /// Equivalent to sRGB: safe to drop without changing colors.
    Srgb,
    /// A different RGB or gray space (Display P3, Adobe RGB, ...).
    Other,
    /// Unusable for the decoded pixels (CMYK profile on converted RGB data,
    /// broken profile, unsupported space): must be dropped.
    Unusable,
}

/// Classifies an ICC profile relative to the image it belongs to.
pub fn classify_icc(icc: &[u8], image: &DynamicImage) -> IccKind {
    let Ok(profile) = ColorProfile::new_from_slice(icc) else {
        return IccKind::Unusable;
    };
    let color = image.color();
    let matches_pixels = match profile.color_space {
        DataColorSpace::Rgb => color.has_color(),
        DataColorSpace::Gray => !color.has_color(),
        _ => false,
    };
    if !matches_pixels {
        return IccKind::Unusable;
    }
    if profile.color_space == DataColorSpace::Rgb && is_srgb_equivalent(&profile) {
        IccKind::Srgb
    } else {
        IccKind::Other
    }
}

/// Pushes a grid of colors through `profile → sRGB`; if nothing moves by more
/// than one code value, the profile is visually identical to sRGB.
fn is_srgb_equivalent(profile: &ColorProfile) -> bool {
    let srgb = ColorProfile::new_srgb();
    let Ok(transform) =
        profile.create_transform_8bit(Layout::Rgb, &srgb, Layout::Rgb, TransformOptions::default())
    else {
        return false;
    };
    const STEPS: [u8; 6] = [0, 51, 102, 153, 204, 255];
    let mut source = Vec::with_capacity(STEPS.len().pow(3) * 3);
    for r in STEPS {
        for g in STEPS {
            for b in STEPS {
                source.extend_from_slice(&[r, g, b]);
            }
        }
    }
    let mut converted = vec![0u8; source.len()];
    if transform.transform(&source, &mut converted).is_err() {
        return false;
    }
    source
        .iter()
        .zip(&converted)
        .all(|(before, after)| before.abs_diff(*after) <= 1)
}

/// Converts pixels from the embedded profile to sRGB.
///
/// The result has the same layout (gray images become RGB, since sRGB is an
/// RGB space); 16-bit images keep 16 bits.
pub fn convert_to_srgb(image: &DynamicImage, icc: &[u8]) -> Result<DynamicImage> {
    let profile = ColorProfile::new_from_slice(icc)
        .map_err(|error| ImagoError::Internal(format!("invalid ICC profile: {error}")))?;
    let srgb = ColorProfile::new_srgb();
    let options = TransformOptions::default();
    let fail =
        |error: moxcms::CmsError| ImagoError::Internal(format!("color conversion failed: {error}"));
    let (width, height) = image.dimensions();

    let converted = match image {
        DynamicImage::ImageRgb8(buffer) => {
            let transform = profile
                .create_transform_8bit(Layout::Rgb, &srgb, Layout::Rgb, options)
                .map_err(fail)?;
            let mut out = vec![0u8; buffer.as_raw().len()];
            transform
                .transform(buffer.as_raw(), &mut out)
                .map_err(fail)?;
            ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, out).map(DynamicImage::ImageRgb8)
        }
        DynamicImage::ImageRgba8(buffer) => {
            let transform = profile
                .create_transform_8bit(Layout::Rgba, &srgb, Layout::Rgba, options)
                .map_err(fail)?;
            let mut out = vec![0u8; buffer.as_raw().len()];
            transform
                .transform(buffer.as_raw(), &mut out)
                .map_err(fail)?;
            ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, out).map(DynamicImage::ImageRgba8)
        }
        DynamicImage::ImageLuma8(buffer) => {
            let transform = profile
                .create_transform_8bit(Layout::Gray, &srgb, Layout::Rgb, options)
                .map_err(fail)?;
            let mut out = vec![0u8; buffer.as_raw().len() * 3];
            transform
                .transform(buffer.as_raw(), &mut out)
                .map_err(fail)?;
            ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, out).map(DynamicImage::ImageRgb8)
        }
        DynamicImage::ImageLumaA8(buffer) => {
            let transform = profile
                .create_transform_8bit(Layout::GrayAlpha, &srgb, Layout::Rgba, options)
                .map_err(fail)?;
            let mut out = vec![0u8; buffer.as_raw().len() * 2];
            transform
                .transform(buffer.as_raw(), &mut out)
                .map_err(fail)?;
            ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, out).map(DynamicImage::ImageRgba8)
        }
        DynamicImage::ImageRgb16(buffer) => {
            let transform = profile
                .create_transform_16bit(Layout::Rgb, &srgb, Layout::Rgb, options)
                .map_err(fail)?;
            let mut out = vec![0u16; buffer.as_raw().len()];
            transform
                .transform(buffer.as_raw(), &mut out)
                .map_err(fail)?;
            ImageBuffer::<Rgb<u16>, _>::from_raw(width, height, out).map(DynamicImage::ImageRgb16)
        }
        DynamicImage::ImageRgba16(buffer) => {
            let transform = profile
                .create_transform_16bit(Layout::Rgba, &srgb, Layout::Rgba, options)
                .map_err(fail)?;
            let mut out = vec![0u16; buffer.as_raw().len()];
            transform
                .transform(buffer.as_raw(), &mut out)
                .map_err(fail)?;
            ImageBuffer::<Rgba<u16>, _>::from_raw(width, height, out).map(DynamicImage::ImageRgba16)
        }
        DynamicImage::ImageLuma16(buffer) => {
            let transform = profile
                .create_transform_16bit(Layout::Gray, &srgb, Layout::Rgb, options)
                .map_err(fail)?;
            let mut out = vec![0u16; buffer.as_raw().len() * 3];
            transform
                .transform(buffer.as_raw(), &mut out)
                .map_err(fail)?;
            ImageBuffer::<Rgb<u16>, _>::from_raw(width, height, out).map(DynamicImage::ImageRgb16)
        }
        DynamicImage::ImageLumaA16(buffer) => {
            let transform = profile
                .create_transform_16bit(Layout::GrayAlpha, &srgb, Layout::Rgba, options)
                .map_err(fail)?;
            let mut out = vec![0u16; buffer.as_raw().len() * 2];
            transform
                .transform(buffer.as_raw(), &mut out)
                .map_err(fail)?;
            ImageBuffer::<Rgba<u16>, _>::from_raw(width, height, out).map(DynamicImage::ImageRgba16)
        }
        other => {
            // Float images: convert through 16-bit, which keeps plenty of precision.
            let widened = if other.color().has_alpha() {
                DynamicImage::ImageRgba16(other.to_rgba16())
            } else {
                DynamicImage::ImageRgb16(other.to_rgb16())
            };
            return convert_to_srgb(&widened, icc);
        }
    };
    converted
        .ok_or_else(|| ImagoError::Internal("color conversion produced a wrong buffer size".into()))
}

/// The sRGB profile encoded as ICC bytes (used by tests and previews).
pub fn srgb_icc() -> Result<Vec<u8>> {
    ColorProfile::new_srgb()
        .encode()
        .map_err(|error| ImagoError::Internal(format!("cannot encode sRGB profile: {error}")))
}

/// The Display P3 profile encoded as ICC bytes.
pub fn display_p3_icc() -> Result<Vec<u8>> {
    ColorProfile::new_display_p3()
        .encode()
        .map_err(|error| ImagoError::Internal(format!("cannot encode Display P3 profile: {error}")))
}

/// Converts any image to 8 bits per channel, keeping gray/color and alpha.
pub fn to_8bit(image: DynamicImage) -> DynamicImage {
    match image {
        DynamicImage::ImageLuma8(_)
        | DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_) => image,
        DynamicImage::ImageLuma16(_) => DynamicImage::ImageLuma8(image.to_luma8()),
        DynamicImage::ImageLumaA16(_) => DynamicImage::ImageLumaA8(image.to_luma_alpha8()),
        other if other.color().has_alpha() => DynamicImage::ImageRgba8(other.to_rgba8()),
        other => DynamicImage::ImageRgb8(other.to_rgb8()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, LumaA};

    fn gray_image(width: u32, height: u32, value: u8) -> DynamicImage {
        DynamicImage::ImageLuma8(ImageBuffer::from_pixel(width, height, Luma([value])))
    }

    fn gray_alpha_image(width: u32, height: u32, value: u8, alpha: u8) -> DynamicImage {
        DynamicImage::ImageLumaA8(ImageBuffer::from_pixel(
            width,
            height,
            LumaA([value, alpha]),
        ))
    }

    fn rgba_from(pixels: &[[u8; 4]]) -> DynamicImage {
        let width = pixels.len() as u32;
        DynamicImage::ImageRgba8(ImageBuffer::from_fn(width, 1, |x, _| {
            Rgba(pixels[x as usize])
        }))
    }

    #[test]
    fn alpha_usage_is_classified() {
        assert_eq!(alpha_usage(&gray_image(2, 2, 9)), AlphaUsage::None);
        assert_eq!(
            alpha_usage(&rgba_from(&[[1, 2, 3, 255]; 4])),
            AlphaUsage::Opaque
        );
        assert_eq!(
            alpha_usage(&rgba_from(&[[1, 2, 3, 255], [0, 0, 0, 0]])),
            AlphaUsage::Binary
        );
        assert_eq!(
            alpha_usage(&rgba_from(&[[1, 2, 3, 255], [0, 0, 0, 128]])),
            AlphaUsage::Partial
        );
        assert_eq!(
            alpha_usage(&gray_alpha_image(3, 3, 1, 10)),
            AlphaUsage::Partial
        );
    }

    #[test]
    fn opaque_alpha_is_dropped_losslessly() {
        let image = rgba_from(&[[10, 20, 30, 255], [40, 50, 60, 255]]);
        let dropped = drop_opaque_alpha(image);
        assert_eq!(dropped.color(), image::ColorType::Rgb8);
        assert_eq!(dropped.to_rgb8().get_pixel(1, 0).0, [40, 50, 60]);
    }

    #[test]
    fn colors_are_counted_with_early_exit() {
        let image = rgba_from(&[[1, 1, 1, 255], [2, 2, 2, 255], [1, 1, 1, 255]]);
        assert_eq!(count_colors_up_to(&image, 256), 2);
        let many = DynamicImage::ImageRgb8(ImageBuffer::from_fn(64, 64, |x, y| {
            Rgb([x as u8, y as u8, (x ^ y) as u8])
        }));
        assert_eq!(count_colors_up_to(&many, 256), 257);
    }

    #[test]
    fn flattening_blends_onto_white() {
        let image = rgba_from(&[[0, 0, 0, 0], [0, 0, 0, 255], [0, 0, 0, 128]]);
        let flat = flatten_onto(&image, [255, 255, 255]).to_rgb8();
        assert_eq!(flat.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(flat.get_pixel(1, 0).0, [0, 0, 0]);
        assert_eq!(flat.get_pixel(2, 0).0, [127, 127, 127]);
    }

    #[test]
    fn flattening_keeps_grayscale() {
        let image = gray_alpha_image(2, 2, 0, 0);
        let flat = flatten_onto(&image, [255, 255, 255]);
        assert_eq!(flat.color(), image::ColorType::L8);
        assert_eq!(flat.to_luma8().get_pixel(0, 0).0, [255]);
    }

    #[test]
    fn srgb_profile_is_recognized_and_p3_is_not() {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(2, 2, Rgb([1, 2, 3])));
        assert_eq!(classify_icc(&srgb_icc().unwrap(), &image), IccKind::Srgb);
        assert_eq!(
            classify_icc(&display_p3_icc().unwrap(), &image),
            IccKind::Other
        );
        assert_eq!(classify_icc(b"garbage", &image), IccKind::Unusable);
        // An RGB profile does not describe gray pixels.
        assert_eq!(
            classify_icc(&srgb_icc().unwrap(), &gray_image(2, 2, 5)),
            IccKind::Unusable
        );
    }

    #[test]
    fn p3_red_becomes_a_different_srgb_value() {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(1, 1, Rgb([200, 30, 30])));
        let converted = convert_to_srgb(&image, &display_p3_icc().unwrap()).unwrap();
        let pixel = converted.to_rgb8().get_pixel(0, 0).0;
        // P3 red is more saturated than sRGB red: the red channel grows (clipped) and
        // the other channels shrink.
        assert!(pixel[0] >= 200, "{pixel:?}");
        assert!(pixel[1] <= 30, "{pixel:?}");
    }

    #[test]
    fn srgb_to_srgb_conversion_is_identity() {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(8, 8, |x, y| {
            Rgba([x as u8 * 30, y as u8 * 30, 99, 200])
        }));
        let converted = convert_to_srgb(&image, &srgb_icc().unwrap())
            .unwrap()
            .to_rgba8();
        for (a, b) in image.to_rgba8().pixels().zip(converted.pixels()) {
            for channel in 0..4 {
                assert!(a.0[channel].abs_diff(b.0[channel]) <= 1);
            }
        }
    }

    #[test]
    fn sixteen_bit_images_become_8bit() {
        let image = DynamicImage::ImageRgba16(ImageBuffer::from_pixel(
            1,
            1,
            Rgba([65535, 0, 32896, 65535]),
        ));
        let converted = to_8bit(image);
        assert_eq!(converted.color(), image::ColorType::Rgba8);
        assert_eq!(converted.to_rgba8().get_pixel(0, 0).0, [255, 0, 128, 255]);
    }
}
