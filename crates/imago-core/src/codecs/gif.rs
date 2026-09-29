//! GIF encoding (palette via libimagequant). Used to keep animated GIFs
//! animated when the output must stay GIF, and for static GIF output.
//!
//! GIF transparency is 1-bit: pixels with alpha < 128 become transparent.

use gif::{DisposalMethod, Encoder, Frame, Repeat};
use image::RgbaImage;

use crate::cancel::CancelToken;
use crate::codecs::quantize::{QuantizeQuality, quantize};
use crate::codecs::{
    AnimationFrame, AnimationInfo, EncodeInput, EncodeOptions, changed_rect, encode_error,
};
use crate::errors::{ImagoError, Result};
use crate::settings::ImageFormat;

const FORMAT: ImageFormat = ImageFormat::Gif;

pub(crate) fn encode_still(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    let canvas = binarize_alpha(input.image.to_rgba8());
    let (width, height) = dimensions_u16(canvas.width(), canvas.height())?;
    let mut output = Vec::new();
    {
        let mut encoder = Encoder::new(&mut output, width, height, &[]).map_err(gif_error)?;
        let frame = quantized_frame(&canvas, (0, 0, canvas.width(), canvas.height()), options)?;
        encoder.write_frame(&frame).map_err(gif_error)?;
    }
    Ok(output)
}

struct Pending {
    canvas: RgbaImage,
    rect: (u32, u32, u32, u32),
    delay_ms: u32,
}

/// Streams an animation into a GIF file.
///
/// Each frame is drawn over the previous one ("keep" disposal) using only its
/// changed rectangle. When a frame needs pixels to become transparent again,
/// the previous frame is written full-size with "restore to background"
/// disposal, because GIF cannot draw transparency over opaque pixels.
pub(crate) fn encode_animation(
    info: AnimationInfo,
    frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
    options: EncodeOptions,
    cancel: &CancelToken,
) -> Result<Vec<u8>> {
    let (width, height) = dimensions_u16(info.width, info.height)?;
    let full = (0, 0, info.width, info.height);
    let mut output = Vec::new();
    {
        let mut encoder = Encoder::new(&mut output, width, height, &[]).map_err(gif_error)?;
        let repeat = match info.loop_count {
            0 => Repeat::Infinite,
            count => Repeat::Finite(count),
        };
        encoder.set_repeat(repeat).map_err(gif_error)?;

        let mut pending: Option<Pending> = None;
        for frame in frames {
            cancel.check()?;
            let frame = frame?;
            if frame.image.dimensions() != (info.width, info.height) {
                return Err(encode_error(
                    FORMAT,
                    "animation frame does not match the canvas size",
                ));
            }
            let canvas = binarize_alpha(frame.image);
            let Some(previous) = pending.take() else {
                pending = Some(Pending {
                    canvas,
                    rect: full,
                    delay_ms: frame.delay_ms,
                });
                continue;
            };
            let Some(changed) = changed_rect(&previous.canvas, &canvas) else {
                pending = Some(Pending {
                    delay_ms: previous.delay_ms.saturating_add(frame.delay_ms),
                    ..previous
                });
                continue;
            };
            let next_rect = if needs_clear(&previous.canvas, &canvas) {
                write_frame(
                    &mut encoder,
                    &previous.canvas,
                    full,
                    previous.delay_ms,
                    DisposalMethod::Background,
                    options,
                )?;
                full
            } else {
                write_frame(
                    &mut encoder,
                    &previous.canvas,
                    previous.rect,
                    previous.delay_ms,
                    DisposalMethod::Keep,
                    options,
                )?;
                changed
            };
            pending = Some(Pending {
                canvas,
                rect: next_rect,
                delay_ms: frame.delay_ms,
            });
        }
        cancel.check()?;
        match pending {
            Some(last) => {
                write_frame(
                    &mut encoder,
                    &last.canvas,
                    last.rect,
                    last.delay_ms,
                    DisposalMethod::Keep,
                    options,
                )?;
            }
            None => {
                return Err(encode_error(
                    FORMAT,
                    "an animation needs at least one frame",
                ));
            }
        }
    }
    Ok(output)
}

fn write_frame(
    encoder: &mut Encoder<&mut Vec<u8>>,
    canvas: &RgbaImage,
    rect: (u32, u32, u32, u32),
    delay_ms: u32,
    dispose: DisposalMethod,
    options: EncodeOptions,
) -> Result<()> {
    let mut frame = quantized_frame(canvas, rect, options)?;
    frame.delay = u16::try_from(delay_ms.saturating_add(5) / 10).unwrap_or(u16::MAX);
    frame.dispose = dispose;
    encoder.write_frame(&frame).map_err(gif_error)
}

/// Quantizes the `rect` part of `canvas` into a frame with a local palette.
fn quantized_frame(
    canvas: &RgbaImage,
    rect: (u32, u32, u32, u32),
    options: EncodeOptions,
) -> Result<Frame<'static>> {
    let (x, y, width, height) = rect;
    let region = image::imageops::crop_imm(canvas, x, y, width, height).to_image();
    let target = if options.lossless {
        100
    } else {
        options.quality.get()
    };
    // Minimum quality 0: GIF output must always succeed.
    let quality = QuantizeQuality { minimum: 0, target };
    let quantized = quantize(&region, quality, options.effort, FORMAT)?
        .ok_or_else(|| encode_error(FORMAT, "quantization produced no palette"))?;
    let transparent = quantized
        .palette
        .iter()
        .position(|color| color.a == 0)
        .and_then(|index| u8::try_from(index).ok());
    let palette: Vec<u8> = quantized
        .palette
        .iter()
        .flat_map(|color| [color.r, color.g, color.b])
        .collect();
    let (frame_width, frame_height) = dimensions_u16(width, height)?;
    let mut frame = Frame::from_palette_pixels(
        frame_width,
        frame_height,
        quantized.indices,
        palette,
        transparent,
    );
    frame.left = u16::try_from(x).map_err(|_| encode_error(FORMAT, "frame offset too large"))?;
    frame.top = u16::try_from(y).map_err(|_| encode_error(FORMAT, "frame offset too large"))?;
    Ok(frame)
}

/// Whether some pixel must turn transparent where the previous frame was opaque.
fn needs_clear(previous: &RgbaImage, current: &RgbaImage) -> bool {
    previous
        .pixels()
        .zip(current.pixels())
        .any(|(before, after)| after.0[3] == 0 && before.0[3] != 0)
}

/// GIF has 1-bit transparency: make every pixel fully opaque or fully transparent.
fn binarize_alpha(mut image: RgbaImage) -> RgbaImage {
    for pixel in image.pixels_mut() {
        if pixel.0[3] < 128 {
            pixel.0 = [0, 0, 0, 0];
        } else {
            pixel.0[3] = 255;
        }
    }
    image
}

fn dimensions_u16(width: u32, height: u32) -> Result<(u16, u16)> {
    match (u16::try_from(width), u16::try_from(height)) {
        (Ok(w), Ok(h)) => Ok((w, h)),
        _ => Err(encode_error(
            FORMAT,
            format!("GIF supports at most 65535×65535 px, the image is {width}×{height} px"),
        )),
    }
}

fn gif_error(error: gif::EncodingError) -> ImagoError {
    encode_error(FORMAT, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn alpha_is_binarized() {
        let image = RgbaImage::from_fn(3, 1, |x, _| Rgba([9, 9, 9, [0, 127, 200][x as usize]]));
        let binary = binarize_alpha(image);
        assert_eq!(binary.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(binary.get_pixel(1, 0).0, [0, 0, 0, 0]);
        assert_eq!(binary.get_pixel(2, 0).0, [9, 9, 9, 255]);
    }

    #[test]
    fn clearing_is_needed_only_for_newly_transparent_pixels() {
        let opaque = RgbaImage::from_pixel(2, 2, Rgba([1, 1, 1, 255]));
        let mut hole = opaque.clone();
        hole.put_pixel(1, 1, Rgba([0, 0, 0, 0]));
        assert!(needs_clear(&opaque, &hole));
        assert!(!needs_clear(&hole, &opaque));
    }
}
