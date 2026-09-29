//! Frame-by-frame decoding of animated GIF, WebP and APNG.
//!
//! Frames are produced lazily by an iterator, fully composited onto the
//! canvas, so encoders can stream them without holding the animation in memory.

use std::io::Cursor;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::metadata::LoopCount;
use image::{AnimationDecoder, DynamicImage, Frames, ImageDecoder, RgbaImage};

use crate::codecs::{AnimationFrame, AnimationInfo};
use crate::errors::{ImagoError, Result};
use crate::image_engine::decode::decoder_limits;
use crate::image_engine::probe::{check_dimensions, decode_error};
use crate::image_engine::resize::{ResizeQuality, resize};
use crate::settings::{ImageFormat, Limits};

/// Formats whose animations Imago can decode.
pub fn can_decode_animation(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Gif | ImageFormat::Webp | ImageFormat::Png
    )
}

/// An open animation: global info plus a lazy frame iterator.
pub struct AnimationSource<'a> {
    /// Canvas size (after resizing) and loop count.
    pub info: AnimationInfo,
    frames: Frames<'a>,
    canvas: (u32, u32),
    target: Option<(u32, u32)>,
    pixels_left: u64,
    format: ImageFormat,
}

impl std::fmt::Debug for AnimationSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnimationSource")
            .field("info", &self.info)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

/// Opens an animation; frames are resized to `target` when given.
pub fn open_animation<'a>(
    bytes: &'a [u8],
    format: ImageFormat,
    limits: &Limits,
    target: Option<(u32, u32)>,
) -> Result<AnimationSource<'a>> {
    let reader = Cursor::new(bytes);
    let (canvas, loop_count, frames) = match format {
        ImageFormat::Gif => {
            let mut decoder = GifDecoder::new(reader).map_err(|e| decode_error(format, e))?;
            decoder
                .set_limits(decoder_limits(limits))
                .map_err(|e| decode_error(format, e))?;
            let canvas = decoder.dimensions();
            let loops = decoder.loop_count();
            (canvas, loops, decoder.into_frames())
        }
        ImageFormat::Webp => {
            let decoder = WebPDecoder::new(reader).map_err(|e| decode_error(format, e))?;
            let canvas = decoder.dimensions();
            let loops = decoder.loop_count();
            (canvas, loops, decoder.into_frames())
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::with_limits(reader, decoder_limits(limits))
                .map_err(|e| decode_error(format, e))?;
            let canvas = decoder.dimensions();
            let apng = decoder.apng().map_err(|e| decode_error(format, e))?;
            let loops = apng.loop_count();
            (canvas, loops, apng.into_frames())
        }
        other => {
            return Err(ImagoError::Decode {
                format: other.display_name().into(),
                message: "animations in this format cannot be decoded".into(),
            });
        }
    };
    check_dimensions(canvas.0, canvas.1, limits)?;
    let (width, height) = target.unwrap_or(canvas);
    Ok(AnimationSource {
        info: AnimationInfo {
            width,
            height,
            loop_count: match loop_count {
                LoopCount::Infinite => 0,
                LoopCount::Finite(count) => u16::try_from(count.get()).unwrap_or(u16::MAX),
            },
        },
        frames,
        canvas,
        target,
        pixels_left: limits.max_animation_pixels,
        format,
    })
}

impl Iterator for AnimationSource<'_> {
    type Item = Result<AnimationFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        let frame = match self.frames.next()? {
            Ok(frame) => frame,
            Err(error) => return Some(Err(decode_error(self.format, error))),
        };
        let pixels = u64::from(self.canvas.0) * u64::from(self.canvas.1);
        if pixels > self.pixels_left {
            return Some(Err(ImagoError::DimensionsTooLarge {
                width: self.canvas.0,
                height: self.canvas.1,
                reason: "the animation has too many frames for the configured limit".into(),
            }));
        }
        self.pixels_left -= pixels;

        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay_ms = numerator.checked_div(denominator).unwrap_or(0);
        let (left, top) = (frame.left(), frame.top());
        let buffer = frame.into_buffer();
        let canvas = if buffer.dimensions() == self.canvas && (left, top) == (0, 0) {
            buffer
        } else {
            // Defensive: decoders normally return full canvases already.
            let mut canvas = RgbaImage::new(self.canvas.0, self.canvas.1);
            image::imageops::overlay(&mut canvas, &buffer, i64::from(left), i64::from(top));
            canvas
        };
        let image = match self.target {
            Some((width, height)) if (width, height) != self.canvas => {
                match resize(
                    &DynamicImage::ImageRgba8(canvas),
                    width,
                    height,
                    ResizeQuality::High,
                ) {
                    Ok(resized) => resized.to_rgba8(),
                    Err(error) => return Some(Err(error)),
                }
            }
            _ => canvas,
        };
        Some(Ok(AnimationFrame { image, delay_ms }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, Rgba};

    fn gif(frames: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for index in 0..frames {
                let image = RgbaImage::from_pixel(20, 10, Rgba([index as u8 * 50, 0, 0, 255]));
                encoder
                    .encode_frame(Frame::from_parts(
                        image,
                        0,
                        0,
                        Delay::from_numer_denom_ms(120, 1),
                    ))
                    .unwrap();
            }
        }
        bytes
    }

    #[test]
    fn decodes_frames_with_delays_and_loop_count() {
        let bytes = gif(3);
        let source = open_animation(&bytes, ImageFormat::Gif, &Limits::default(), None).unwrap();
        assert_eq!(source.info.loop_count, 0);
        let frames: Vec<_> = source.collect::<Result<_>>().unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[2].image.get_pixel(0, 0).0, [100, 0, 0, 255]);
        assert_eq!(frames[0].delay_ms, 120);
    }

    #[test]
    fn frames_are_resized_to_the_target() {
        let bytes = gif(2);
        let source =
            open_animation(&bytes, ImageFormat::Gif, &Limits::default(), Some((10, 5))).unwrap();
        assert_eq!((source.info.width, source.info.height), (10, 5));
        for frame in source {
            assert_eq!(frame.unwrap().image.dimensions(), (10, 5));
        }
    }

    #[test]
    fn too_many_frames_hit_the_limit() {
        let bytes = gif(5);
        let limits = Limits {
            max_animation_pixels: 20 * 10 * 3,
            ..Limits::default()
        };
        let source = open_animation(&bytes, ImageFormat::Gif, &limits, None).unwrap();
        let results: Vec<_> = source.collect();
        assert!(results.iter().take(3).all(|r| r.is_ok()));
        assert_eq!(
            results[3].as_ref().unwrap_err().code(),
            "dimensions_too_large"
        );
    }
}
