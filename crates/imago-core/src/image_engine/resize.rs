//! Resizing: target-size computation and SIMD resampling.

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::DynamicImage;

use crate::errors::{ImagoError, Result};
use crate::settings::{ResizeMode, ResizeSettings};

/// Resampling quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeQuality {
    /// Lanczos3 — for output images.
    High,
    /// Bilinear — for thumbnails and previews.
    Fast,
}

/// Computes the output size for an image of `width`×`height`.
///
/// Returns `None` when the image should keep its size (no resize requested,
/// already within the limits, or upscaling would be required but is not allowed).
pub fn target_dimensions(width: u32, height: u32, settings: &ResizeSettings) -> Option<(u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let (w, h) = (f64::from(width), f64::from(height));
    let target = match settings.mode {
        ResizeMode::None => return None,
        ResizeMode::Dimensions {
            width: target_w,
            height: target_h,
            keep_aspect: true,
        } => {
            let scale = match (target_w, target_h) {
                (Some(tw), Some(th)) => (f64::from(tw) / w).min(f64::from(th) / h),
                (Some(tw), None) => f64::from(tw) / w,
                (None, Some(th)) => f64::from(th) / h,
                (None, None) => return None,
            };
            scaled(width, height, limit_scale(scale, settings.allow_upscale))
        }
        ResizeMode::Dimensions {
            width: target_w,
            height: target_h,
            keep_aspect: false,
        } => {
            let clamp = |target: Option<u32>, original: u32| match target {
                Some(value) if settings.allow_upscale => value,
                Some(value) => value.min(original),
                None => original,
            };
            (clamp(target_w, width), clamp(target_h, height))
        }
        ResizeMode::Fit {
            max_width,
            max_height,
        } => {
            let scale_w = max_width.map_or(1.0, |mw| f64::from(mw) / w);
            let scale_h = max_height.map_or(1.0, |mh| f64::from(mh) / h);
            // Fit only ever shrinks: images within the bounds are left alone.
            scaled(width, height, scale_w.min(scale_h).min(1.0))
        }
        ResizeMode::Percent { percent } => {
            let scale = f64::from(percent) / 100.0;
            if !scale.is_finite() || scale <= 0.0 {
                return None;
            }
            scaled(width, height, limit_scale(scale, settings.allow_upscale))
        }
    };
    let target = (target.0.max(1), target.1.max(1));
    (target != (width, height)).then_some(target)
}

fn limit_scale(scale: f64, allow_upscale: bool) -> f64 {
    if allow_upscale { scale } else { scale.min(1.0) }
}

fn scaled(width: u32, height: u32, scale: f64) -> (u32, u32) {
    let round = |value: u32| {
        let result = (f64::from(value) * scale).round();
        if result >= f64::from(u32::MAX) {
            u32::MAX
        } else {
            result.max(1.0) as u32
        }
    };
    (round(width), round(height))
}

/// Resamples `image` to exactly `width`×`height` pixels.
///
/// Alpha is premultiplied during filtering so transparent edges do not bleed.
pub fn resize(
    image: &DynamicImage,
    width: u32,
    height: u32,
    quality: ResizeQuality,
) -> Result<DynamicImage> {
    if width == 0 || height == 0 {
        return Err(ImagoError::InvalidSettings(format!(
            "cannot resize to {width}x{height}"
        )));
    }
    if (image.width(), image.height()) == (width, height) {
        return Ok(image.clone());
    }
    let source = normalize_for_resize(image);
    let source = source.as_ref().unwrap_or(image);
    let mut destination = DynamicImage::new(width, height, source.color());
    let algorithm = match quality {
        ResizeQuality::High => ResizeAlg::Convolution(FilterType::Lanczos3),
        ResizeQuality::Fast => ResizeAlg::Convolution(FilterType::Bilinear),
    };
    let options = ResizeOptions::new().resize_alg(algorithm);
    Resizer::new()
        .resize(source, &mut destination, &options)
        .map_err(|error| ImagoError::Internal(format!("resize failed: {error}")))?;
    Ok(destination)
}

/// Converts pixel layouts the resizer cannot handle into ones it can.
fn normalize_for_resize(image: &DynamicImage) -> Option<DynamicImage> {
    match image {
        DynamicImage::ImageLuma8(_)
        | DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_)
        | DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_)
        | DynamicImage::ImageRgb32F(_)
        | DynamicImage::ImageRgba32F(_) => None,
        other if other.color().has_alpha() => Some(DynamicImage::ImageRgba8(other.to_rgba8())),
        other => Some(DynamicImage::ImageRgb8(other.to_rgb8())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn settings(mode: ResizeMode) -> ResizeSettings {
        ResizeSettings {
            mode,
            allow_upscale: false,
        }
    }

    #[test]
    fn no_resize_mode_keeps_size() {
        assert_eq!(
            target_dimensions(800, 600, &settings(ResizeMode::None)),
            None
        );
    }

    #[test]
    fn width_only_keeps_aspect_ratio() {
        let mode = ResizeMode::Dimensions {
            width: Some(400),
            height: None,
            keep_aspect: true,
        };
        assert_eq!(
            target_dimensions(800, 600, &settings(mode)),
            Some((400, 300))
        );
    }

    #[test]
    fn height_only_keeps_aspect_ratio() {
        let mode = ResizeMode::Dimensions {
            width: None,
            height: Some(150),
            keep_aspect: true,
        };
        assert_eq!(
            target_dimensions(800, 600, &settings(mode)),
            Some((200, 150))
        );
    }

    #[test]
    fn both_dimensions_with_aspect_fit_inside_the_box() {
        let mode = ResizeMode::Dimensions {
            width: Some(400),
            height: Some(400),
            keep_aspect: true,
        };
        assert_eq!(
            target_dimensions(800, 600, &settings(mode)),
            Some((400, 300))
        );
    }

    #[test]
    fn both_dimensions_without_aspect_stretch() {
        let mode = ResizeMode::Dimensions {
            width: Some(400),
            height: Some(100),
            keep_aspect: false,
        };
        assert_eq!(
            target_dimensions(800, 600, &settings(mode)),
            Some((400, 100))
        );
    }

    #[test]
    fn upscaling_is_blocked_unless_allowed() {
        let mode = ResizeMode::Dimensions {
            width: Some(1600),
            height: None,
            keep_aspect: true,
        };
        assert_eq!(target_dimensions(800, 600, &settings(mode)), None);
        let allowed = ResizeSettings {
            mode,
            allow_upscale: true,
        };
        assert_eq!(target_dimensions(800, 600, &allowed), Some((1600, 1200)));
    }

    #[test]
    fn fit_only_shrinks_images_over_the_limit() {
        let mode = ResizeMode::Fit {
            max_width: Some(2400),
            max_height: None,
        };
        assert_eq!(
            target_dimensions(4800, 3200, &settings(mode)),
            Some((2400, 1600))
        );
        assert_eq!(target_dimensions(1200, 800, &settings(mode)), None);

        let both = ResizeMode::Fit {
            max_width: Some(1000),
            max_height: Some(1000),
        };
        assert_eq!(
            target_dimensions(3000, 1500, &settings(both)),
            Some((1000, 500))
        );
        assert_eq!(
            target_dimensions(1500, 3000, &settings(both)),
            Some((500, 1000))
        );
    }

    #[test]
    fn percent_scales_both_dimensions() {
        let mode = ResizeMode::Percent { percent: 50.0 };
        assert_eq!(
            target_dimensions(801, 601, &settings(mode)),
            Some((401, 301))
        );
        let tiny = ResizeMode::Percent { percent: 0.01 };
        assert_eq!(target_dimensions(10, 10, &settings(tiny)), Some((1, 1)));
    }

    #[test]
    fn resize_produces_requested_size_and_keeps_color_type() {
        let image =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(64, 32, Rgba([200, 100, 50, 255])));
        let resized = resize(&image, 16, 8, ResizeQuality::High).unwrap();
        assert_eq!((resized.width(), resized.height()), (16, 8));
        assert_eq!(resized.color(), image.color());
        // A flat image stays flat after filtering.
        assert_eq!(resized.to_rgba8().get_pixel(8, 4).0, [200, 100, 50, 255]);
    }

    #[test]
    fn resize_does_not_bleed_color_from_transparent_pixels() {
        // Left half transparent black, right half opaque white.
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(64, 64, |x, _| {
            if x < 32 {
                Rgba([0, 0, 0, 0])
            } else {
                Rgba([255, 255, 255, 255])
            }
        }));
        let resized = resize(&image, 16, 16, ResizeQuality::High)
            .unwrap()
            .to_rgba8();
        for pixel in resized.pixels() {
            if pixel.0[3] > 16 {
                assert!(pixel.0[0] > 200, "dark fringe: {:?}", pixel.0);
            }
        }
    }
}
