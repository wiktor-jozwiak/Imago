//! AVIF encoding with zenavif (zenravif / rav1e AV1 encoder): still images and
//! animations. AVIF is lossy only: the encoder's quantizer-0 mode is not
//! bit-exact, so Imago does not offer it as lossless.

use crate::cancel::CancelToken;
use crate::codecs::{AnimationFrame, AnimationInfo, EncodeInput, EncodeOptions};
use crate::errors::Result;

#[cfg(feature = "avif")]
mod imp {
    use std::borrow::Cow;

    use almost_enough::StopToken;
    use image::{DynamicImage, RgbaImage};
    use imgref::{Img, ImgVec};
    use rgb::bytemuck;
    use zenavif::{
        AnimationFrameRgba, EncodeBitDepth, EncoderConfig, encode_animation_rgba8, encode_rgb8,
        encode_rgb16, encode_rgba8, encode_rgba16,
    };

    use crate::cancel::CancelToken;
    use crate::codecs::{
        AnimationFrame, AnimationInfo, EncodeInput, EncodeOptions, effort_index, encode_error,
    };
    use crate::errors::{ImagoError, Result};
    use crate::settings::ImageFormat;

    const FORMAT: ImageFormat = ImageFormat::Avif;

    fn config(options: EncodeOptions) -> EncoderConfig {
        let effort = effort_index(options.effort);
        // rav1e speed: 10 is fastest. Still images get the tuning presets of the encoder.
        let speed = [9, 6, 3][effort];
        let quality = f32::from(options.quality.get());
        EncoderConfig::new()
            .speed(speed)
            .threads(None)
            .still_image_preset()
            .quality(quality)
            .alpha_quality(quality)
    }

    fn stop_token(cancel: Option<&CancelToken>) -> StopToken {
        match cancel {
            Some(token) => StopToken::new(token.clone()),
            None => StopToken::new(CancelToken::new()),
        }
    }

    fn map_error(error: zenavif::Error) -> ImagoError {
        match error {
            zenavif::Error::Cancelled(_) => ImagoError::Cancelled,
            other => encode_error(FORMAT, other),
        }
    }

    pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
        let mut config = config(options);
        if let Some(exif) = input.exif {
            config = config.exif(exif.to_vec());
        }
        if let Some(icc) = input.icc_profile {
            config = config.icc_profile(icc.to_vec());
        }
        let stop = stop_token(input.cancel);
        let image = input.image;
        let (width, height) = (image.width() as usize, image.height() as usize);
        let sixteen_bit = matches!(
            image,
            DynamicImage::ImageLuma16(_)
                | DynamicImage::ImageLumaA16(_)
                | DynamicImage::ImageRgb16(_)
                | DynamicImage::ImageRgba16(_)
                | DynamicImage::ImageRgb32F(_)
                | DynamicImage::ImageRgba32F(_)
        );
        let alpha = image.color().has_alpha();
        let encoded = match (sixteen_bit, alpha) {
            (false, false) => {
                let rgb = match image {
                    DynamicImage::ImageRgb8(buffer) => Cow::Borrowed(buffer),
                    other => Cow::Owned(other.to_rgb8()),
                };
                encode_rgb8(
                    Img::new(bytemuck::cast_slice(rgb.as_raw()), width, height),
                    &config,
                    stop,
                )
            }
            (false, true) => {
                let rgba = match image {
                    DynamicImage::ImageRgba8(buffer) => Cow::Borrowed(buffer),
                    other => Cow::Owned(other.to_rgba8()),
                };
                encode_rgba8(
                    Img::new(bytemuck::cast_slice(rgba.as_raw()), width, height),
                    &config,
                    stop,
                )
            }
            // AVIF stores at most 12 bits; 16-bit sources keep their precision as 10 bits.
            (true, false) => {
                let rgb = image.to_rgb16();
                encode_rgb16(
                    Img::new(bytemuck::cast_slice(rgb.as_raw()), width, height),
                    &config.bit_depth(EncodeBitDepth::Ten),
                    stop,
                )
            }
            (true, true) => {
                let rgba = image.to_rgba16();
                encode_rgba16(
                    Img::new(bytemuck::cast_slice(rgba.as_raw()), width, height),
                    &config.bit_depth(EncodeBitDepth::Ten),
                    stop,
                )
            }
        }
        .map_err(|error| map_error(error.decompose().0))?;
        Ok(encoded.avif_file)
    }

    /// Encodes an animation into an AVIF image sequence (inter-frame AV1).
    ///
    /// The AV1 encoder needs the whole sequence at once, so frames are
    /// collected first; `Limits::max_animation_pixels` bounds that memory.
    pub(crate) fn encode_animation(
        info: AnimationInfo,
        frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
        options: EncodeOptions,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>> {
        let (width, height) = (info.width as usize, info.height as usize);
        let mut collected: Vec<AnimationFrameRgba> = Vec::new();
        let mut previous: Option<RgbaImage> = None;
        for frame in frames {
            cancel.check()?;
            let frame = frame?;
            if frame.image.dimensions() != (info.width, info.height) {
                return Err(encode_error(
                    FORMAT,
                    "animation frame does not match the canvas size",
                ));
            }
            // Identical consecutive frames only extend the previous duration.
            if previous.as_ref() == Some(&frame.image)
                && let Some(last) = collected.last_mut()
            {
                last.duration_ms = last.duration_ms.saturating_add(frame.delay_ms);
                continue;
            }
            let pixels: Vec<rgb::RGBA8> = bytemuck::cast_slice(frame.image.as_raw()).to_vec();
            collected.push(AnimationFrameRgba {
                pixels: ImgVec::new(pixels, width, height),
                // A zero delay means "as fast as possible" in GIF; browsers show ~100 ms.
                duration_ms: frame.delay_ms.max(10),
            });
            previous = Some(frame.image);
        }
        drop(previous);
        if collected.is_empty() {
            return Err(encode_error(FORMAT, "the animation has no frames"));
        }
        cancel.check()?;
        let config = config(options);
        let encoded = encode_animation_rgba8(&collected, &config, stop_token(Some(cancel)))
            .map_err(|error| map_error(error.decompose().0))?;
        Ok(encoded.avif_file)
    }
}

#[cfg(feature = "avif")]
pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    imp::encode(input, options)
}

#[cfg(feature = "avif")]
pub(crate) fn encode_animation(
    info: AnimationInfo,
    frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
    options: EncodeOptions,
    cancel: &CancelToken,
) -> Result<Vec<u8>> {
    imp::encode_animation(info, frames, options, cancel)
}

#[cfg(not(feature = "avif"))]
pub(crate) fn encode(_input: EncodeInput<'_>, _options: EncodeOptions) -> Result<Vec<u8>> {
    Err(crate::codecs::unavailable(
        crate::settings::ImageFormat::Avif,
    ))
}

#[cfg(not(feature = "avif"))]
pub(crate) fn encode_animation(
    _info: AnimationInfo,
    _frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
    _options: EncodeOptions,
    _cancel: &CancelToken,
) -> Result<Vec<u8>> {
    Err(crate::codecs::unavailable(
        crate::settings::ImageFormat::Avif,
    ))
}
