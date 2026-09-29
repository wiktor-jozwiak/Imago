//! WebP encoding with libwebp (lossy and lossless), including animations.

use image::{DynamicImage, RgbaImage};
use img_parts::webp::WebP;
use img_parts::{Bytes, ImageEXIF, ImageICC};
use webp::{Encoder, PixelLayout, WebPConfig};

use crate::cancel::CancelToken;
use crate::codecs::webp_mux::{AnimationMuxer, FramePlacement};
use crate::codecs::{
    AnimationFrame, AnimationInfo, EncodeInput, EncodeOptions, changed_rect, effort_index,
    encode_error,
};
use crate::errors::Result;
use crate::settings::{Effort, ImageFormat};

const FORMAT: ImageFormat = ImageFormat::Webp;

pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    let image = input.image;
    let encoded = if image.color().has_alpha() {
        let rgba = to_rgba8(image);
        encode_pixels(
            rgba.as_raw(),
            PixelLayout::Rgba,
            image.width(),
            image.height(),
            options,
        )?
    } else {
        let rgb = match image {
            DynamicImage::ImageRgb8(buffer) => std::borrow::Cow::Borrowed(buffer),
            other => std::borrow::Cow::Owned(other.to_rgb8()),
        };
        encode_pixels(
            rgb.as_raw(),
            PixelLayout::Rgb,
            image.width(),
            image.height(),
            options,
        )?
    };
    attach_metadata(encoded, input.icc_profile, input.exif)
}

fn to_rgba8(image: &DynamicImage) -> std::borrow::Cow<'_, RgbaImage> {
    match image {
        DynamicImage::ImageRgba8(buffer) => std::borrow::Cow::Borrowed(buffer),
        other => std::borrow::Cow::Owned(other.to_rgba8()),
    }
}

fn config(options: EncodeOptions) -> Result<WebPConfig> {
    let mut config =
        WebPConfig::new().map_err(|()| encode_error(FORMAT, "cannot initialize libwebp"))?;
    let effort = effort_index(options.effort);
    if options.lossless {
        config.lossless = 1;
        // In lossless mode `quality` is the compression effort.
        config.quality = [25.0, 75.0, 100.0][effort];
        config.method = [1, 4, 6][effort];
        // Keep RGB values under fully transparent pixels: lossless means lossless.
        config.exact = 1;
    } else {
        config.quality = f32::from(options.quality.get());
        config.method = [2, 4, 6][effort];
        config.use_sharp_yuv = i32::from(options.effort == Effort::Max);
        config.alpha_quality = 100;
    }
    config.thread_level = 1;
    Ok(config)
}

fn encode_pixels(
    pixels: &[u8],
    layout: PixelLayout,
    width: u32,
    height: u32,
    options: EncodeOptions,
) -> Result<Vec<u8>> {
    let config = config(options)?;
    let encoder = Encoder::new(pixels, layout, width, height);
    let memory = encoder
        .encode_advanced(&config)
        .map_err(|error| encode_error(FORMAT, format!("libwebp error: {error:?}")))?;
    Ok(memory.to_vec())
}

fn attach_metadata(encoded: Vec<u8>, icc: Option<&[u8]>, exif: Option<&[u8]>) -> Result<Vec<u8>> {
    if icc.is_none() && exif.is_none() {
        return Ok(encoded);
    }
    let mut webp = WebP::from_bytes(Bytes::from(encoded)).map_err(|e| encode_error(FORMAT, e))?;
    if let Some(icc) = icc {
        webp.set_icc_profile(Some(Bytes::copy_from_slice(icc)));
    }
    if let Some(exif) = exif {
        webp.set_exif(Some(Bytes::copy_from_slice(exif)));
    }
    let mut out = Vec::new();
    webp.encoder()
        .write_to(&mut out)
        .map_err(|e| encode_error(FORMAT, e))?;
    Ok(out)
}

/// Streams an animation into an animated WebP file.
///
/// Every frame is compared with the previous canvas; only the changed
/// rectangle is encoded, and identical frames extend the previous duration.
pub(crate) fn encode_animation(
    info: AnimationInfo,
    frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
    options: EncodeOptions,
    cancel: &CancelToken,
) -> Result<Vec<u8>> {
    let max = crate::codecs::capability(FORMAT)
        .max_dimension
        .unwrap_or(u32::MAX);
    if info.width > max || info.height > max {
        return Err(encode_error(
            FORMAT,
            format!(
                "WebP supports at most {max}×{max} px, the animation is {}×{} px",
                info.width, info.height
            ),
        ));
    }
    let mut muxer = AnimationMuxer::new(info.width, info.height, info.loop_count)?;
    let mut previous: Option<RgbaImage> = None;
    let mut pending: Option<(Vec<u8>, FramePlacement)> = None;

    for frame in frames {
        cancel.check()?;
        let frame = frame?;
        if frame.image.dimensions() != (info.width, info.height) {
            return Err(encode_error(
                FORMAT,
                "animation frame does not match the canvas size",
            ));
        }
        let rect = match &previous {
            None => Some((0, 0, info.width, info.height)),
            Some(previous) => changed_rect(previous, &frame.image),
        };
        let Some(rect) = rect else {
            // Identical to the previous frame: just show that one longer.
            if let Some((_, placement)) = pending.as_mut() {
                placement.duration_ms = placement.duration_ms.saturating_add(frame.delay_ms);
            }
            continue;
        };
        if let Some((bytes, placement)) = pending.take() {
            muxer.add_frame(&bytes, placement)?;
        }
        let (x, y, width, height) = align_to_even(rect);
        let region = image::imageops::crop_imm(&frame.image, x, y, width, height).to_image();
        let encoded = encode_pixels(region.as_raw(), PixelLayout::Rgba, width, height, options)?;
        pending = Some((
            encoded,
            FramePlacement {
                x,
                y,
                width,
                height,
                duration_ms: frame.delay_ms,
                blend: false,
                dispose_to_background: false,
            },
        ));
        previous = Some(frame.image);
    }
    cancel.check()?;
    if let Some((bytes, placement)) = pending.take() {
        muxer.add_frame(&bytes, placement)?;
    }
    muxer.finish()
}

/// WebP frame offsets must be even: grow the rectangle left/up if needed.
fn align_to_even((x, y, width, height): (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let aligned_x = x & !1;
    let aligned_y = y & !1;
    (
        aligned_x,
        aligned_y,
        width + (x - aligned_x),
        height + (y - aligned_y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_alignment_keeps_the_rectangle_covered() {
        assert_eq!(align_to_even((3, 5, 2, 2)), (2, 4, 3, 3));
        assert_eq!(align_to_even((4, 6, 2, 2)), (4, 6, 2, 2));
    }
}
