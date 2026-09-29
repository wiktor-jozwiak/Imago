//! Encoder tests for every output format: round trips, quality, lossless,
//! transparency, metadata and animations.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use image::{AnimationDecoder, DynamicImage};
use imago_core::cancel::CancelToken;
use imago_core::codecs::{self, AnimationFrame, AnimationInfo, EncodeInput, EncodeOptions};
use imago_core::image_engine::color::{AlphaUsage, alpha_usage, display_p3_icc};
use imago_core::settings::{Effort, ImageFormat, Quality};

fn options(quality: u8) -> EncodeOptions {
    EncodeOptions {
        quality: Quality::new(quality).unwrap(),
        lossless: false,
        effort: Effort::Balanced,
    }
}

fn lossless() -> EncodeOptions {
    EncodeOptions {
        lossless: true,
        ..EncodeOptions::default()
    }
}

/// Formats whose encoder is compiled into this build.
fn available(formats: &[ImageFormat]) -> Vec<ImageFormat> {
    formats
        .iter()
        .copied()
        .filter(|format| codecs::capability(*format).encode)
        .collect()
}

fn encode(format: ImageFormat, image: &DynamicImage, options: EncodeOptions) -> Vec<u8> {
    codecs::encode(format, EncodeInput::pixels(image), options).unwrap()
}

#[test]
fn lossy_formats_round_trip_with_good_quality() {
    let photo = common::photo(96, 64);
    for format in available(&[ImageFormat::Jpeg, ImageFormat::Webp, ImageFormat::Avif]) {
        let bytes = encode(format, &photo, options(82));
        let decoded = common::decode(&bytes);
        assert_eq!(decoded.image.width(), 96, "{format}");
        assert_eq!(decoded.image.height(), 64, "{format}");
        let quality = common::psnr(&photo, &decoded.image);
        assert!(quality > 30.0, "{format}: PSNR {quality:.1} dB");
    }
}

#[test]
fn lower_quality_gives_smaller_files() {
    let photo = common::photo(128, 96);
    for format in available(&[
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Avif,
        ImageFormat::Png,
    ]) {
        let low = encode(format, &photo, options(30)).len();
        let high = encode(format, &photo, options(95)).len();
        assert!(low < high, "{format}: q30 = {low} B, q95 = {high} B");
    }
}

#[test]
fn lossless_formats_preserve_pixels_exactly() {
    let images = [
        common::photo(40, 30),
        common::transparent_logo(40, 30),
        common::gray(40, 30),
    ];
    for format in available(&[
        ImageFormat::Png,
        ImageFormat::Webp,
        ImageFormat::Tiff,
        ImageFormat::Jxl,
    ]) {
        for image in &images {
            let bytes = encode(format, image, lossless());
            let decoded = common::decode(&bytes).image;
            assert_eq!(
                decoded.to_rgba8().as_raw(),
                image.to_rgba8().as_raw(),
                "{format} {:?}",
                image.color()
            );
        }
    }
}

#[test]
fn sixteen_bit_images_stay_exact_in_png_and_tiff() {
    let image = common::rgb16(33, 17);
    for format in available(&[ImageFormat::Png, ImageFormat::Tiff, ImageFormat::Jxl]) {
        let decoded = common::decode(&encode(format, &image, lossless())).image;
        assert_eq!(
            decoded.to_rgb16().as_raw(),
            image.to_rgb16().as_raw(),
            "{format}"
        );
    }
}

#[test]
fn lossy_png_is_smaller_than_lossless_for_photos() {
    let photo = common::photo(128, 128);
    let lossy = encode(ImageFormat::Png, &photo, options(75));
    let exact = encode(ImageFormat::Png, &photo, lossless());
    assert!(
        lossy.len() < exact.len(),
        "lossy {} vs lossless {}",
        lossy.len(),
        exact.len()
    );
    let decoded = common::decode(&lossy).image;
    assert!(common::psnr(&photo, &decoded) > 28.0);
}

#[test]
fn few_color_graphics_stay_exact_even_in_lossy_png() {
    let graphic = common::graphic(64, 48);
    let decoded = common::decode(&encode(ImageFormat::Png, &graphic, options(60))).image;
    assert_eq!(decoded.to_rgb8().as_raw(), graphic.to_rgb8().as_raw());
}

#[test]
fn transparency_is_preserved_where_the_format_supports_it() {
    let logo = common::transparent_logo(64, 64);
    for format in available(&[
        ImageFormat::Png,
        ImageFormat::Webp,
        ImageFormat::Avif,
        ImageFormat::Tiff,
    ]) {
        let decoded = common::decode(&encode(format, &logo, options(82))).image;
        assert!(
            alpha_usage(&decoded).is_transparent(),
            "{format} lost transparency"
        );
        // A fully transparent corner stays fully transparent.
        assert_eq!(decoded.to_rgba8().get_pixel(0, 0).0[3], 0, "{format}");
    }
    let gif = common::decode(&encode(ImageFormat::Gif, &logo, options(82))).image;
    assert_eq!(alpha_usage(&gif), AlphaUsage::Binary);
}

#[test]
fn jpeg_flattens_transparency_onto_white() {
    let logo = common::transparent_logo(32, 32);
    let decoded = common::decode(&encode(ImageFormat::Jpeg, &logo, options(95)))
        .image
        .to_rgb8();
    let corner = decoded.get_pixel(0, 0).0;
    assert!(corner.iter().all(|&c| c > 245), "corner {corner:?}");
}

#[test]
fn grayscale_jpeg_stays_grayscale() {
    let gray = common::gray(32, 32);
    let decoded = common::decode(&encode(ImageFormat::Jpeg, &gray, options(90))).image;
    assert!(!decoded.color().has_color());
}

#[test]
fn icc_profiles_are_embedded() {
    let photo = common::photo(24, 24);
    let p3 = display_p3_icc().unwrap();
    for format in available(&[
        ImageFormat::Jpeg,
        ImageFormat::Png,
        ImageFormat::Webp,
        ImageFormat::Tiff,
    ]) {
        let input = EncodeInput {
            image: &photo,
            icc_profile: Some(&p3),
            exif: None,
        };
        let bytes = codecs::encode(format, input, options(90)).unwrap();
        let decoded = common::decode(&bytes);
        assert_eq!(
            decoded.icc_profile.as_deref(),
            Some(p3.as_slice()),
            "{format}"
        );
    }
}

#[test]
fn exif_is_embedded_when_requested() {
    let photo = common::photo(24, 24);
    let exif = imago_core::metadata::orientation_only_exif(1);
    for format in available(&[ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::Webp]) {
        let input = EncodeInput {
            image: &photo,
            icc_profile: None,
            exif: Some(&exif),
        };
        let bytes = codecs::encode(format, input, options(90)).unwrap();
        assert!(
            imago_core::metadata::contains_exif(&bytes, format),
            "{format}"
        );
        let plain = encode(format, &photo, options(90));
        assert!(
            !imago_core::metadata::contains_exif(&plain, format),
            "{format} leaks EXIF"
        );
    }
}

#[test]
fn every_effort_level_produces_valid_output() {
    let photo = common::photo(48, 32);
    for effort in [Effort::Fast, Effort::Balanced, Effort::Max] {
        for format in available(&[
            ImageFormat::Jpeg,
            ImageFormat::Png,
            ImageFormat::Webp,
            ImageFormat::Avif,
            ImageFormat::Tiff,
        ]) {
            let bytes = encode(
                format,
                &photo,
                EncodeOptions {
                    effort,
                    ..options(80)
                },
            );
            assert_eq!(
                common::decode(&bytes).image.width(),
                48,
                "{format} {effort:?}"
            );
        }
    }
}

#[test]
fn unsupported_modes_are_reported_not_faked() {
    let photo = common::photo(16, 16);
    for format in available(&[ImageFormat::Jpeg, ImageFormat::Avif]) {
        let error = codecs::encode(format, EncodeInput::pixels(&photo), lossless()).unwrap_err();
        assert_eq!(error.code(), "encode_failed", "{format}");
    }
    let error =
        codecs::encode(ImageFormat::Bmp, EncodeInput::pixels(&photo), options(80)).unwrap_err();
    assert_eq!(error.code(), "format_unavailable");
    let info = error.to_info();
    assert_eq!(info.alternative.as_deref(), Some("PNG"));
}

#[test]
fn webp_rejects_images_beyond_its_limit() {
    let wide = DynamicImage::new_rgb8(16_384, 1);
    let error =
        codecs::encode(ImageFormat::Webp, EncodeInput::pixels(&wide), options(80)).unwrap_err();
    assert!(error.to_string().contains("16383"), "{error}");
}

#[cfg(feature = "jxl")]
#[test]
fn jpeg_xl_needs_at_least_two_pixels() {
    let dot = DynamicImage::new_rgb8(1, 5);
    assert!(codecs::encode(ImageFormat::Jxl, EncodeInput::pixels(&dot), lossless()).is_err());
}

fn frames_iter(
    frames: &[image::RgbaImage],
    delay_ms: u32,
) -> Vec<imago_core::Result<AnimationFrame>> {
    frames
        .iter()
        .map(|image| {
            Ok(AnimationFrame {
                image: image.clone(),
                delay_ms,
            })
        })
        .collect()
}

#[test]
fn animated_webp_keeps_every_frame() {
    let frames = common::animation_frames(48, 16, 5, false);
    let info = AnimationInfo {
        width: 48,
        height: 16,
        loop_count: 0,
    };
    let mut source = frames_iter(&frames, 80).into_iter();
    let bytes = codecs::encode_animation(
        ImageFormat::Webp,
        info,
        &mut source,
        lossless(),
        &CancelToken::new(),
    )
    .unwrap();
    let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
    assert!(decoder.has_animation());
    let decoded: Vec<_> = decoder.into_frames().collect_frames().unwrap();
    assert_eq!(decoded.len(), 5);
    for (original, frame) in frames.iter().zip(&decoded) {
        assert_eq!(frame.buffer().as_raw(), original.as_raw());
    }
}

#[test]
fn identical_frames_are_merged_into_longer_ones() {
    let mut frames = common::animation_frames(32, 16, 2, false);
    frames.insert(1, frames[0].clone());
    let info = AnimationInfo {
        width: 32,
        height: 16,
        loop_count: 0,
    };
    let mut source = frames_iter(&frames, 100).into_iter();
    let bytes = codecs::encode_animation(
        ImageFormat::Webp,
        info,
        &mut source,
        lossless(),
        &CancelToken::new(),
    )
    .unwrap();
    let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
    let decoded: Vec<_> = decoder.into_frames().collect_frames().unwrap();
    assert_eq!(decoded.len(), 2);
    let (numerator, denominator) = decoded[0].delay().numer_denom_ms();
    assert_eq!(numerator / denominator, 200);
}

#[test]
fn animated_gif_keeps_frames_and_transparency() {
    let frames = common::animation_frames(40, 16, 4, true);
    let info = AnimationInfo {
        width: 40,
        height: 16,
        loop_count: 0,
    };
    let mut source = frames_iter(&frames, 100).into_iter();
    let bytes = codecs::encode_animation(
        ImageFormat::Gif,
        info,
        &mut source,
        options(90),
        &CancelToken::new(),
    )
    .unwrap();
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
    let decoded: Vec<_> = decoder.into_frames().collect_frames().unwrap();
    assert_eq!(decoded.len(), 4);
    for (original, frame) in frames.iter().zip(&decoded) {
        for (a, b) in original.pixels().zip(frame.buffer().pixels()) {
            assert_eq!(a.0[3], b.0[3], "transparency differs");
        }
    }
}

#[test]
fn animation_encoding_can_be_cancelled() {
    let frames = common::animation_frames(32, 16, 3, false);
    let info = AnimationInfo {
        width: 32,
        height: 16,
        loop_count: 0,
    };
    let token = CancelToken::new();
    token.cancel();
    let mut source = frames_iter(&frames, 100).into_iter();
    let error = codecs::encode_animation(ImageFormat::Webp, info, &mut source, lossless(), &token)
        .unwrap_err();
    assert!(error.is_cancelled());
}
