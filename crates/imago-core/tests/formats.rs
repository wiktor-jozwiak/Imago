//! Formats added after the first release: animated PNG/AVIF/JPEG XL output,
//! animated AVIF/JPEG XL input and HEIC input.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use imago_core::cancel::CancelToken;
use imago_core::codecs::{self, AnimationFrame, AnimationInfo, EncodeOptions};
use imago_core::filesystem::InputFile;
use imago_core::image_engine::animation::open_animation;
use imago_core::image_engine::probe_bytes;
use imago_core::jobs::{FileReport, FileTask, JobEvent, JobSpec};
use imago_core::settings::{FormatChoice, ImageFormat, Limits, OptimizeSettings, Quality};
use imago_core::workers::{JobControl, run_job};

fn available(format: ImageFormat) -> bool {
    let capability = codecs::capability(format);
    capability.encode && capability.animation
}

fn frames(images: &[image::RgbaImage], delay_ms: u32) -> Vec<imago_core::Result<AnimationFrame>> {
    images
        .iter()
        .map(|image| {
            Ok(AnimationFrame {
                image: image.clone(),
                delay_ms,
            })
        })
        .collect()
}

fn encode_animation(
    format: ImageFormat,
    images: &[image::RgbaImage],
    options: EncodeOptions,
) -> Vec<u8> {
    let (width, height) = images[0].dimensions();
    let info = AnimationInfo {
        width,
        height,
        loop_count: 0,
    };
    codecs::encode_animation(
        format,
        info,
        &mut frames(images, 80).into_iter(),
        options,
        &CancelToken::new(),
    )
    .unwrap()
}

fn lossless() -> EncodeOptions {
    EncodeOptions {
        lossless: true,
        ..EncodeOptions::default()
    }
}

fn lossy(quality: u8) -> EncodeOptions {
    EncodeOptions {
        quality: Quality::new(quality).unwrap(),
        ..EncodeOptions::default()
    }
}

/// Decodes an animation with Imago's own frame reader.
fn decode_animation(bytes: &[u8], format: ImageFormat) -> Vec<AnimationFrame> {
    open_animation(bytes, format, &Limits::default(), None)
        .unwrap()
        .collect::<imago_core::Result<Vec<_>>>()
        .unwrap()
}

#[test]
fn lossless_animations_round_trip_exactly_in_apng_and_jpeg_xl() {
    let images = common::animation_frames(48, 16, 4, true);
    for format in [ImageFormat::Png, ImageFormat::Jxl] {
        if !available(format) {
            continue;
        }
        let bytes = encode_animation(format, &images, lossless());
        let probe = probe_bytes(&bytes, &Limits::default()).unwrap();
        assert_eq!(probe.format, format);
        assert!(probe.animated, "{format}");
        let decoded = decode_animation(&bytes, format);
        assert_eq!(decoded.len(), images.len(), "{format}");
        for (original, frame) in images.iter().zip(&decoded) {
            assert_eq!(frame.image.as_raw(), original.as_raw(), "{format}");
            assert!(
                (70..=90).contains(&frame.delay_ms),
                "{format}: {}",
                frame.delay_ms
            );
        }
    }
}

#[test]
fn animated_avif_round_trips_with_high_fidelity() {
    if !available(ImageFormat::Avif) {
        return;
    }
    let images = common::animation_frames(64, 32, 3, false);
    let bytes = encode_animation(ImageFormat::Avif, &images, lossy(90));
    let probe = probe_bytes(&bytes, &Limits::default()).unwrap();
    assert!(probe.animated);
    let decoded = decode_animation(&bytes, ImageFormat::Avif);
    assert_eq!(decoded.len(), 3);
    for (original, frame) in images.iter().zip(&decoded) {
        let psnr = common::psnr(
            &image::DynamicImage::ImageRgba8(original.clone()),
            &image::DynamicImage::ImageRgba8(frame.image.clone()),
        );
        assert!(psnr > 30.0, "PSNR {psnr}");
        assert!((70..=90).contains(&frame.delay_ms), "{}", frame.delay_ms);
    }
}

#[test]
fn lossy_jpeg_xl_is_smaller_than_lossless_for_photos() {
    if !codecs::capability(ImageFormat::Jxl).encode {
        return;
    }
    let photo = common::photo(96, 64);
    let input = codecs::EncodeInput::pixels(&photo);
    let exact = codecs::encode(ImageFormat::Jxl, input, lossless()).unwrap();
    let small = codecs::encode(ImageFormat::Jxl, input, lossy(75)).unwrap();
    assert!(
        small.len() < exact.len(),
        "{} vs {}",
        small.len(),
        exact.len()
    );
    let decoded = common::decode(&small).image;
    assert!(common::psnr(&photo, &decoded) > 28.0);
}

fn run(files: &[PathBuf], settings: OptimizeSettings) -> Vec<FileReport> {
    let events = Mutex::new(Vec::new());
    let sink = |event: JobEvent| events.lock().unwrap().push(event);
    let spec = JobSpec {
        files: files
            .iter()
            .enumerate()
            .map(|(index, path)| FileTask {
                id: index as u64,
                input: InputFile::from_path(path).unwrap(),
            })
            .collect(),
        settings,
    };
    run_job(&spec, &JobControl::new(), &sink).unwrap();
    let mut reports: Vec<FileReport> = events
        .into_inner()
        .unwrap()
        .into_iter()
        .filter_map(|event| match event {
            JobEvent::FileFinished { report } => Some(*report),
            _ => None,
        })
        .collect();
    reports.sort_by_key(|report| report.id);
    reports
}

fn fixed(format: ImageFormat) -> OptimizeSettings {
    OptimizeSettings {
        format: FormatChoice::Fixed(format),
        ..OptimizeSettings::default()
    }
}

#[test]
fn animated_gifs_can_become_apng_avif_or_jpeg_xl() {
    let dir = tempfile::tempdir().unwrap();
    let gif = common::write_file(
        dir.path(),
        "anim.gif",
        &common::animated_gif(&common::animation_frames(48, 16, 4, false), 100),
    );
    for format in [ImageFormat::Png, ImageFormat::Avif, ImageFormat::Jxl] {
        if !available(format) {
            continue;
        }
        let reports = run(std::slice::from_ref(&gif), fixed(format));
        let report = &reports[0];
        assert_eq!(report.output_format, Some(format), "{format}: {report:?}");
        let bytes = std::fs::read(report.output_path.as_ref().unwrap()).unwrap();
        let probe = probe_bytes(&bytes, &Limits::default()).unwrap();
        assert!(probe.animated, "{format}");
        assert!(report.decision.as_ref().unwrap().kept_animation);
    }
}

fn heic_fixture() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/flat_red_64.heic"
    ))
}

#[test]
fn heic_photos_are_decoded() {
    if !codecs::capability(ImageFormat::Heic).decode {
        return;
    }
    let bytes = std::fs::read(heic_fixture()).unwrap();
    let probe = probe_bytes(&bytes, &Limits::default()).unwrap();
    assert_eq!(probe.format, ImageFormat::Heic);
    assert_eq!((probe.width, probe.height), (64, 64));
    let decoded = common::decode(&bytes).image.to_rgb8();
    let [r, g, b] = decoded.get_pixel(32, 32).0;
    assert!(r > 200 && g < 50 && b < 50, "{r} {g} {b}");
}

#[test]
fn heic_keeps_its_look_as_jpeg_in_original_mode() {
    if !codecs::capability(ImageFormat::Heic).decode {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("photo.heic");
    std::fs::copy(heic_fixture(), &input).unwrap();
    let settings = OptimizeSettings {
        format: FormatChoice::Original,
        ..OptimizeSettings::default()
    };
    let report = &run(&[input], settings)[0];
    assert_eq!(report.output_format, Some(ImageFormat::Jpeg), "{report:?}");
    let output = report.output_path.as_ref().unwrap();
    assert_eq!(output.extension().and_then(|e| e.to_str()), Some("jpg"));
}

/// Two sprites in opposite corners: the changed rectangle covers almost the
/// whole canvas although only a few pixels change.
fn two_sprites(count: u32) -> Vec<image::RgbaImage> {
    (0..count)
        .map(|index| {
            image::RgbaImage::from_fn(160, 96, |x, y| {
                let a = index * 3;
                let b = 150 - index * 3;
                if (a..a + 8).contains(&x) && (4..12).contains(&y) {
                    image::Rgba([230, 40, 40, 255])
                } else if (b.saturating_sub(8)..b).contains(&x) && (84..92).contains(&y) {
                    image::Rgba([40, 40, 230, 255])
                } else {
                    image::Rgba([(x + y) as u8, (2 * y) as u8, 120, 255])
                }
            })
        })
        .collect()
}

#[test]
fn webp_animations_use_blending_and_mixed_frames() {
    let images = two_sprites(8);
    let fast = encode_animation(
        ImageFormat::Webp,
        &images,
        EncodeOptions {
            effort: imago_core::settings::Effort::Fast,
            ..lossy(80)
        },
    );
    let balanced = encode_animation(ImageFormat::Webp, &images, lossy(80));
    assert!(
        balanced.len() * 10 < fast.len() * 8,
        "{} vs {}",
        balanced.len(),
        fast.len()
    );

    // Lossless mode never blends, so every frame stays exact.
    let exact = encode_animation(ImageFormat::Webp, &images, lossless());
    let decoded = decode_animation(&exact, ImageFormat::Webp);
    assert_eq!(decoded.len(), images.len());
    for (original, frame) in images.iter().zip(&decoded) {
        assert_eq!(frame.image.as_raw(), original.as_raw());
    }
}

fn tiny_limits(settings: OptimizeSettings) -> OptimizeSettings {
    OptimizeSettings {
        limits: Limits {
            max_pixels: 5_000,
            ..Limits::default()
        },
        ..settings
    }
}

#[test]
fn images_above_the_limit_use_the_large_image_mode() {
    let dir = tempfile::tempdir().unwrap();
    let photo = common::photo(200, 120);
    let jpeg = common::write_file(
        dir.path(),
        "big.jpg",
        &common::encode_with_image(&photo, image::ImageFormat::Jpeg),
    );
    let png = common::write_file(
        dir.path(),
        "big.png",
        &common::encode_with_image(&common::transparent_logo(200, 120), image::ImageFormat::Png),
    );
    let settings = OptimizeSettings {
        resize: imago_core::settings::ResizeSettings {
            mode: imago_core::settings::ResizeMode::Percent { percent: 50.0 },
            allow_upscale: false,
        },
        ..tiny_limits(OptimizeSettings::default())
    };
    let reports = run(&[jpeg, png], settings);
    for report in &reports {
        assert!(
            report.warnings.iter().any(|w| w.code == "large_image_mode"),
            "{report:?}"
        );
        assert_eq!(report.output_dimensions, Some((100, 60)), "{report:?}");
    }
    // Auto mode keeps transparency: JPEG for the photo, PNG for the logo.
    assert_eq!(reports[0].output_format, Some(ImageFormat::Jpeg));
    assert_eq!(reports[1].output_format, Some(ImageFormat::Png));
    let logo = std::fs::read(reports[1].output_path.as_ref().unwrap()).unwrap();
    assert!(common::decode(&logo).image.color().has_alpha());
}

#[test]
fn the_large_image_mode_can_be_turned_off_and_skips_other_formats() {
    let dir = tempfile::tempdir().unwrap();
    let photo = common::photo(200, 120);
    let jpeg = common::write_file(
        dir.path(),
        "big.jpg",
        &common::encode_with_image(&photo, image::ImageFormat::Jpeg),
    );
    let webp = common::write_file(
        dir.path(),
        "big.webp",
        &common::encode_with_image(&photo, image::ImageFormat::WebP),
    );
    let off = OptimizeSettings {
        large_images: imago_core::settings::LargeImages::Reject,
        ..tiny_limits(OptimizeSettings::default())
    };
    let reports = run(std::slice::from_ref(&jpeg), off);
    assert_eq!(
        reports[0].error.as_ref().unwrap().code,
        "dimensions_too_large"
    );

    let reports = run(&[webp], tiny_limits(OptimizeSettings::default()));
    let error = reports[0].error.as_ref().unwrap();
    assert_eq!(error.code, "dimensions_too_large");
    assert!(error.message.contains("JPEG and PNG"), "{}", error.message);
}
