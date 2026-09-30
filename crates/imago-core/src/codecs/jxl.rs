//! JPEG XL encoding with jxl-encoder: lossy (VarDCT), lossless (modular) and
//! animations.
//!
//! The encoder is young, so every result is decoded again with jxl-oxide
//! before it is accepted: lossless output must reproduce the input pixels
//! exactly, lossy output must decode to the right dimensions. A failure is
//! reported as an encoding error instead of producing a wrong file.

use crate::cancel::CancelToken;
use crate::codecs::{AnimationFrame, AnimationInfo, EncodeInput, EncodeOptions};
use crate::errors::Result;

#[cfg(feature = "jxl")]
mod imp {
    use std::io::Cursor;

    use image::{DynamicImage, ImageDecoder, RgbaImage};
    use jxl_encoder::{
        AnimationFrame as JxlFrame, AnimationParams, ImageMetadata, LosslessConfig, LossyConfig,
        PixelLayout, calibrated_jxl_quality, quality_to_distance,
    };

    use crate::cancel::CancelToken;
    use crate::codecs::{
        AnimationFrame, AnimationInfo, EncodeInput, EncodeOptions, effort_index, encode_error,
    };
    use crate::errors::{ImagoError, Result};
    use crate::settings::ImageFormat;

    const FORMAT: ImageFormat = ImageFormat::Jxl;

    /// Lossy or lossless configuration derived from Imago's options.
    enum Config {
        Lossy(LossyConfig),
        Lossless(LosslessConfig),
    }

    fn config(options: EncodeOptions) -> Config {
        let effort = effort_index(options.effort);
        if options.lossless {
            Config::Lossless(LosslessConfig::new().with_effort([3, 6, 8][effort]))
        } else {
            // Imago's quality follows the JPEG scale; map it onto the JPEG XL
            // butteraugli distance calibrated for the same visual quality.
            let quality = calibrated_jxl_quality(f32::from(options.quality.get()));
            let distance = quality_to_distance(quality).max(0.05);
            Config::Lossy(LossyConfig::new(distance).with_effort([3, 6, 8][effort]))
        }
    }

    /// Pixels in a layout the encoder accepts, borrowed when possible.
    struct Pixels<'a> {
        data: std::borrow::Cow<'a, [u8]>,
        layout: PixelLayout,
        /// The image as it must decode again (for lossless verification).
        reference: std::borrow::Cow<'a, DynamicImage>,
    }

    fn native_bytes(samples: &[u16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_ne_bytes()).collect()
    }

    fn pixels(image: &DynamicImage) -> Pixels<'_> {
        use std::borrow::Cow;
        fn borrowed<'a>(
            data: &'a [u8],
            layout: PixelLayout,
            image: &'a DynamicImage,
        ) -> Pixels<'a> {
            Pixels {
                data: Cow::Borrowed(data),
                layout,
                reference: Cow::Borrowed(image),
            }
        }
        let owned = |data: Vec<u8>, layout, reference| Pixels {
            data: Cow::Owned(data),
            layout,
            reference,
        };
        match image {
            DynamicImage::ImageLuma8(b) => borrowed(b.as_raw(), PixelLayout::Gray8, image),
            DynamicImage::ImageLumaA8(b) => borrowed(b.as_raw(), PixelLayout::GrayAlpha8, image),
            DynamicImage::ImageRgb8(b) => borrowed(b.as_raw(), PixelLayout::Rgb8, image),
            DynamicImage::ImageRgba8(b) => borrowed(b.as_raw(), PixelLayout::Rgba8, image),
            DynamicImage::ImageLuma16(b) => owned(
                native_bytes(b.as_raw()),
                PixelLayout::Gray16,
                Cow::Borrowed(image),
            ),
            DynamicImage::ImageLumaA16(b) => owned(
                native_bytes(b.as_raw()),
                PixelLayout::GrayAlpha16,
                Cow::Borrowed(image),
            ),
            DynamicImage::ImageRgb16(b) => owned(
                native_bytes(b.as_raw()),
                PixelLayout::Rgb16,
                Cow::Borrowed(image),
            ),
            DynamicImage::ImageRgba16(b) => owned(
                native_bytes(b.as_raw()),
                PixelLayout::Rgba16,
                Cow::Borrowed(image),
            ),
            // Float and future layouts: keep 16 bits of precision.
            other if other.color().has_alpha() => {
                let widened = other.to_rgba16();
                owned(
                    native_bytes(widened.as_raw()),
                    PixelLayout::Rgba16,
                    Cow::Owned(DynamicImage::ImageRgba16(widened)),
                )
            }
            other => {
                let widened = other.to_rgb16();
                owned(
                    native_bytes(widened.as_raw()),
                    PixelLayout::Rgb16,
                    Cow::Owned(DynamicImage::ImageRgb16(widened)),
                )
            }
        }
    }

    fn error_message(error: impl std::fmt::Display) -> String {
        error.to_string()
    }

    pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
        let image = input.image;
        let (width, height) = (image.width(), image.height());
        let pixels = pixels(image);
        let mut metadata = ImageMetadata::new();
        if let Some(icc) = input.icc_profile {
            metadata = metadata.with_icc_profile(icc);
        }
        if let Some(exif) = input.exif {
            metadata = metadata.with_exif(exif);
        }
        let never = CancelToken::new();
        let stop: &dyn enough::Stop = match input.cancel {
            Some(token) => token,
            None => &never,
        };
        let config = config(options);
        let request = match &config {
            Config::Lossy(config) => config.encode_request(width, height, pixels.layout),
            Config::Lossless(config) => config.encode_request(width, height, pixels.layout),
        };
        let output = request
            .with_metadata(&metadata)
            .with_stop(stop)
            .encode(&pixels.data)
            .map_err(|error| {
                if input.cancel.is_some_and(CancelToken::is_cancelled) {
                    ImagoError::Cancelled
                } else {
                    encode_error(FORMAT, error_message(error))
                }
            })?;
        verify(
            &output,
            (width, height),
            options.lossless.then_some(&*pixels.reference),
        )?;
        Ok(output)
    }

    /// Decodes the result again; lossless output must match `reference` exactly.
    fn verify(
        output: &[u8],
        dimensions: (u32, u32),
        reference: Option<&DynamicImage>,
    ) -> Result<()> {
        let failed =
            |message: String| encode_error(FORMAT, format!("verification failed: {message}"));
        let decoder = jxl_oxide::integration::JxlDecoder::new(Cursor::new(output))
            .map_err(|error| failed(error.to_string()))?;
        if decoder.dimensions() != dimensions {
            return Err(failed("dimensions differ".into()));
        }
        let decoded =
            DynamicImage::from_decoder(decoder).map_err(|error| failed(error.to_string()))?;
        if let Some(reference) = reference
            && decoded.to_rgba16().as_raw() != reference.to_rgba16().as_raw()
        {
            return Err(failed("decoded pixels differ".into()));
        }
        Ok(())
    }

    /// Encodes an animation. The encoder needs every frame at once, so frames
    /// are collected first (bounded by `Limits::max_animation_pixels`).
    pub(crate) fn encode_animation(
        info: AnimationInfo,
        frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
        options: EncodeOptions,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>> {
        // Durations in milliseconds: 1000 ticks per second.
        let mut collected: Vec<(RgbaImage, u32)> = Vec::new();
        for frame in frames {
            cancel.check()?;
            let frame = frame?;
            if frame.image.dimensions() != (info.width, info.height) {
                return Err(encode_error(
                    FORMAT,
                    "animation frame does not match the canvas size",
                ));
            }
            match collected.last_mut() {
                Some((previous, duration)) if *previous == frame.image => {
                    *duration = duration.saturating_add(frame.delay_ms);
                }
                _ => collected.push((frame.image, frame.delay_ms.max(10))),
            }
        }
        if collected.is_empty() {
            return Err(encode_error(FORMAT, "the animation has no frames"));
        }
        cancel.check()?;
        let animation = AnimationParams {
            tps_numerator: 1000,
            tps_denominator: 1,
            num_loops: u32::from(info.loop_count),
        };
        let frames: Vec<JxlFrame<'_>> = collected
            .iter()
            .map(|(image, duration)| JxlFrame {
                pixels: image.as_raw(),
                duration: *duration,
            })
            .collect();
        let output = match config(options) {
            Config::Lossy(config) => config.encode_animation(
                info.width,
                info.height,
                PixelLayout::Rgba8,
                &animation,
                &frames,
            ),
            Config::Lossless(config) => config.encode_animation(
                info.width,
                info.height,
                PixelLayout::Rgba8,
                &animation,
                &frames,
            ),
        }
        .map_err(|error| encode_error(FORMAT, error_message(error)))?;
        verify(&output, (info.width, info.height), None)?;
        Ok(output)
    }
}

#[cfg(feature = "jxl")]
pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    imp::encode(input, options)
}

#[cfg(feature = "jxl")]
pub(crate) fn encode_animation(
    info: AnimationInfo,
    frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
    options: EncodeOptions,
    cancel: &CancelToken,
) -> Result<Vec<u8>> {
    imp::encode_animation(info, frames, options, cancel)
}

#[cfg(not(feature = "jxl"))]
pub(crate) fn encode(_input: EncodeInput<'_>, _options: EncodeOptions) -> Result<Vec<u8>> {
    Err(crate::codecs::unavailable(
        crate::settings::ImageFormat::Jxl,
    ))
}

#[cfg(not(feature = "jxl"))]
pub(crate) fn encode_animation(
    _info: AnimationInfo,
    _frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
    _options: EncodeOptions,
    _cancel: &CancelToken,
) -> Result<Vec<u8>> {
    Err(crate::codecs::unavailable(
        crate::settings::ImageFormat::Jxl,
    ))
}
