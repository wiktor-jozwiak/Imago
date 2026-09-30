//! Engine tests: every input format, corrupted files, EXIF orientation,
//! lossless metadata removal and Unicode paths.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use image::DynamicImage;
use imago_core::codecs::{self, EncodeInput, EncodeOptions};
use imago_core::image_engine::{self, Detected, probe_bytes, probe_file};
use imago_core::metadata;
use imago_core::settings::{ImageFormat, Limits};

/// One encoded sample per supported input format.
fn samples() -> Vec<(ImageFormat, Vec<u8>)> {
    let photo = common::photo(48, 32);
    let lossless = EncodeOptions {
        lossless: true,
        ..EncodeOptions::default()
    };
    let mut samples: Vec<(ImageFormat, Vec<u8>)> = [
        (ImageFormat::Jpeg, image::ImageFormat::Jpeg),
        (ImageFormat::Png, image::ImageFormat::Png),
        (ImageFormat::Webp, image::ImageFormat::WebP),
        (ImageFormat::Tiff, image::ImageFormat::Tiff),
        (ImageFormat::Bmp, image::ImageFormat::Bmp),
        (ImageFormat::Gif, image::ImageFormat::Gif),
    ]
    .into_iter()
    .map(|(format, encoder)| (format, common::encode_with_image(&photo, encoder)))
    .collect();
    // AVIF and JPEG XL inputs are produced with Imago's own encoders, when compiled in.
    for (format, options) in [
        (ImageFormat::Avif, EncodeOptions::default()),
        (ImageFormat::Jxl, lossless),
    ] {
        let capability = codecs::capability(format);
        if capability.encode && capability.decode {
            samples.push((
                format,
                codecs::encode(format, EncodeInput::pixels(&photo), options).unwrap(),
            ));
        }
    }
    samples
}

#[test]
fn every_input_format_is_detected_probed_and_decoded() {
    let limits = Limits::default();
    for (format, bytes) in samples() {
        assert_eq!(
            image_engine::sniff(&bytes[..bytes.len().min(64)]),
            Detected::Supported(format)
        );
        let probe = probe_bytes(&bytes, &limits).unwrap();
        assert_eq!(probe.format, format);
        assert_eq!((probe.width, probe.height), (48, 32), "{format}");
        assert!(!probe.animated, "{format}");
        let decoded = image_engine::decode(&bytes, &probe, &limits).unwrap();
        assert_eq!(
            (decoded.image.width(), decoded.image.height()),
            (48, 32),
            "{format}"
        );
    }
}

#[test]
fn truncated_and_garbled_files_fail_cleanly() {
    let limits = Limits::default();
    for (format, bytes) in samples() {
        for cut in [bytes.len() / 2, 40, 12] {
            let truncated = &bytes[..cut.min(bytes.len())];
            let outcome = probe_bytes(truncated, &limits)
                .and_then(|probe| image_engine::decode(truncated, &probe, &limits).map(|_| ()));
            if let Err(error) = outcome {
                assert!(
                    matches!(
                        error.code(),
                        "decode_failed" | "unsupported_format" | "dimensions_too_large"
                    ),
                    "{format} cut at {cut}: {error}"
                );
            }
        }
        // Keep the header, destroy the rest.
        let mut garbled = bytes.clone();
        let keep = 64.min(garbled.len());
        for (index, byte) in garbled.iter_mut().enumerate().skip(keep) {
            *byte = (index * 7 % 251) as u8;
        }
        let _ = probe_bytes(&garbled, &limits)
            .and_then(|probe| image_engine::decode(&garbled, &probe, &limits));
    }
}

#[test]
fn animated_gif_is_detected_and_first_frame_decodes() {
    let frames = common::animation_frames(40, 16, 3, false);
    let bytes = common::animated_gif(&frames, 100);
    let limits = Limits::default();
    let probe = probe_bytes(&bytes, &limits).unwrap();
    assert!(probe.animated);
    let decoded = image_engine::decode(&bytes, &probe, &limits).unwrap();
    assert_eq!(decoded.image.width(), 40);
}

#[test]
fn exif_orientation_is_reported_and_applied() {
    let photo = common::photo(60, 20);
    let bytes = common::jpeg_with_orientation(&photo, 6);
    let limits = Limits::default();
    let probe = probe_bytes(&bytes, &limits).unwrap();
    assert_eq!(probe.orientation, 6);
    assert!(probe.has_exif);
    assert_eq!(probe.display_dimensions(), (20, 60));

    let mut decoded = image_engine::decode(&bytes, &probe, &limits).unwrap();
    decoded.image.apply_orientation(decoded.orientation);
    assert_eq!((decoded.image.width(), decoded.image.height()), (20, 60));
}

#[test]
fn metadata_is_removed_without_touching_pixels() {
    let photo = common::photo(64, 48);
    let with_exif = common::jpeg_with_orientation(&photo, 1);
    assert!(metadata::contains_exif(&with_exif, ImageFormat::Jpeg));
    let stripped = metadata::strip_container_metadata(&with_exif, ImageFormat::Jpeg, true)
        .unwrap()
        .unwrap();
    assert!(!metadata::contains_exif(&stripped, ImageFormat::Jpeg));
    assert!(stripped.len() < with_exif.len());
    let before = common::decode(&with_exif).image;
    let after = common::decode(&stripped).image;
    assert_eq!(before.to_rgb8().as_raw(), after.to_rgb8().as_raw());
}

#[test]
fn icc_survives_lossless_stripping_when_kept() {
    let photo = common::photo(16, 16);
    let p3 = imago_core::image_engine::color::display_p3_icc().unwrap();
    let exif = metadata::orientation_only_exif(1);
    for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::Webp] {
        let input = EncodeInput {
            image: &photo,
            icc_profile: Some(&p3),
            exif: Some(&exif),
            cancel: None,
        };
        let bytes = codecs::encode(format, input, EncodeOptions::default()).unwrap();
        let kept = metadata::strip_container_metadata(&bytes, format, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            metadata::container_icc(&kept, format).as_deref(),
            Some(p3.as_slice()),
            "{format}"
        );
        assert!(!metadata::contains_exif(&kept, format), "{format}");
        let dropped = metadata::strip_container_metadata(&bytes, format, false)
            .unwrap()
            .unwrap();
        assert!(
            metadata::container_icc(&dropped, format).is_none(),
            "{format}"
        );
        // Still a valid image.
        assert_eq!(common::decode(&dropped).image.width(), 16, "{format}");
    }
}

#[test]
fn format_is_detected_from_content_not_extension() {
    let dir = tempfile::tempdir().unwrap();
    let png = common::encode_with_image(&common::graphic(10, 10), image::ImageFormat::Png);
    let path = common::write_file(dir.path(), "actually-a-png.jpg", &png);
    let probe = probe_file(&path, &Limits::default()).unwrap();
    assert_eq!(probe.format, ImageFormat::Png);
}

#[test]
fn unicode_paths_work() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = common::encode_with_image(&common::photo(12, 9), image::ImageFormat::Png);
    let path = common::write_file(dir.path(), "zdjęcie ąęśćż — 日本語 😀/zażółć.png", &bytes);
    let probe = probe_file(&path, &Limits::default()).unwrap();
    assert_eq!((probe.width, probe.height), (12, 9));
}

#[test]
fn non_images_and_folders_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let text = common::write_file(dir.path(), "notes.png", b"these are not pixels");
    assert_eq!(
        probe_file(&text, &Limits::default()).unwrap_err().code(),
        "unsupported_format"
    );
    assert_eq!(
        probe_file(dir.path(), &Limits::default())
            .unwrap_err()
            .code(),
        "not_a_file"
    );
    let missing = dir.path().join("missing.png");
    assert_eq!(
        probe_file(&missing, &Limits::default()).unwrap_err().code(),
        "not_found"
    );
}

#[test]
fn decompression_bomb_is_rejected_from_the_header() {
    // A 1×1 PNG whose IHDR claims 60000×60000 pixels: the probe rejects it
    // before any pixel buffer exists.
    let mut bytes =
        common::encode_with_image(&DynamicImage::new_rgb8(1, 1), image::ImageFormat::Png);
    bytes[16..20].copy_from_slice(&60_000u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&60_000u32.to_be_bytes());
    let crc = common::crc32(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&crc.to_be_bytes());
    let error = probe_bytes(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), "dimensions_too_large", "{error}");
}
