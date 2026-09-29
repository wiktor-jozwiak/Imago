//! Security and robustness tests: hostile inputs must fail cleanly (never
//! panic, never hang, never allocate unbounded memory) and outputs must never
//! escape their folder or touch the original.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use imago_core::archive::{ArchiveEntry, create_zip, sanitize_entry_name};
use imago_core::cancel::CancelToken;
use imago_core::filesystem::paths::{ensure_within, output_path};
use imago_core::filesystem::{InputFile, ScanOptions, scan_inputs};
use imago_core::image_engine::{self, probe_bytes};
use imago_core::jobs::{FileStatus, FileTask, JobEvent, JobSpec};
use imago_core::settings::{ImageFormat, Limits, OptimizeSettings, OutputLocation, OutputSettings};
use imago_core::workers::{JobControl, run_job};

/// Every signature followed by random bytes: probing and decoding must return
/// errors, never panic.
#[test]
fn random_data_behind_real_signatures_never_panics() {
    let signatures: [&[u8]; 9] = [
        &[0xFF, 0xD8, 0xFF, 0xE0],
        b"\x89PNG\r\n\x1a\n",
        b"GIF89a",
        b"RIFF\0\0\0\0WEBPVP8 ",
        b"II*\0",
        b"BM\0\0\0\0\0\0\0\0\0\0\0\0\x28\0\0\0",
        &[0xFF, 0x0A],
        b"\0\0\0\x1cftypavif\0\0\0\0mif1avif",
        b"\0\0\0\x0cJXL \r\n\x87\n",
    ];
    let limits = Limits::default();
    let mut noise = common::Noise::new(7);
    for signature in signatures {
        for length in [0usize, 1, 7, 64, 500, 5000] {
            let mut bytes = signature.to_vec();
            bytes.extend((0..length).map(|_| noise.byte()));
            if let Ok(probe) = probe_bytes(&bytes, &limits) {
                let _ = image_engine::decode(&bytes, &probe, &limits);
            }
        }
    }
}

/// Real files with every single byte in the header flipped: no panics.
#[test]
fn bit_flipped_headers_fail_cleanly() {
    let limits = Limits::default();
    let photo = common::photo(32, 24);
    for format in [
        image::ImageFormat::Png,
        image::ImageFormat::Jpeg,
        image::ImageFormat::Gif,
        image::ImageFormat::Bmp,
        image::ImageFormat::Tiff,
        image::ImageFormat::WebP,
    ] {
        let original = common::encode_with_image(&photo, format);
        for position in 0..original.len().min(96) {
            let mut bytes = original.clone();
            bytes[position] ^= 0xFF;
            if let Ok(probe) = probe_bytes(&bytes, &limits) {
                let _ = image_engine::decode(&bytes, &probe, &limits);
            }
        }
    }
}

#[test]
fn hostile_zip_entry_names_are_rejected() {
    for name in [
        "../escape.png",
        "..\\escape.png",
        "C:\\windows\\x.png",
        "a/../../b.png",
        "",
        "\u{0}.png",
    ] {
        assert!(sanitize_entry_name(name).is_err(), "{name:?} was accepted");
    }
    // Leading separators are dropped: entries are always relative.
    assert_eq!(
        sanitize_entry_name("/abs/path.png").unwrap(),
        "abs/path.png"
    );
    assert_eq!(
        sanitize_entry_name("\\\\server\\share\\x.png").unwrap(),
        "server/share/x.png"
    );
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.png");
    std::fs::write(&source, b"x").unwrap();
    let entries = vec![ArchiveEntry {
        source,
        name: "../../outside.png".into(),
    }];
    let error = create_zip(
        &entries,
        &dir.path().join("x.zip"),
        false,
        &CancelToken::new(),
        |_, _| {},
    )
    .unwrap_err();
    assert_eq!(error.code(), "path_traversal");
}

#[test]
fn outputs_never_escape_their_folder() {
    let settings = OutputSettings {
        location: OutputLocation::Directory {
            path: PathBuf::from("/safe/out"),
        },
        suffix: "/../../etc".into(),
        ..OutputSettings::default()
    };
    let input = InputFile {
        path: PathBuf::from("/photos/a.png"),
        root: Some(PathBuf::from("/photos")),
        size: 1,
    };
    assert!(output_path(&input, ImageFormat::Png, &settings).is_err());
    assert!(ensure_within(Path::new("/safe/out"), Path::new("/safe/out/../../etc/x")).is_err());

    // A file that claims a root it is not inside of.
    let liar = InputFile {
        path: PathBuf::from("/etc/passwd.png"),
        root: Some(PathBuf::from("/photos")),
        size: 1,
    };
    assert!(output_path(&liar, ImageFormat::Png, &OutputSettings::default()).is_err());

    // Invalid settings are refused before any job starts.
    let mut bad = OptimizeSettings::default();
    bad.output.suffix = "../x".into();
    assert!(bad.validate().is_err());
    bad = OptimizeSettings::default();
    bad.output.location = OutputLocation::Subfolder {
        name: "../up".into(),
    };
    assert!(bad.validate().is_err());
}

#[test]
fn oversized_inputs_are_refused_before_decoding() {
    let dir = tempfile::tempdir().unwrap();
    let big = common::encode_with_image(&common::photo(300, 200), image::ImageFormat::Png);
    let path = common::write_file(dir.path(), "big.png", &big);
    let mut settings = OptimizeSettings::default();
    settings.limits.max_file_bytes = 1000;
    let spec = JobSpec {
        files: vec![FileTask {
            id: 1,
            input: InputFile::from_path(&path).unwrap(),
        }],
        settings: settings.clone(),
    };
    let reports = std::sync::Mutex::new(Vec::new());
    let sink = |event: JobEvent| {
        if let JobEvent::FileFinished { report } = event {
            reports.lock().unwrap().push(*report);
        }
    };
    run_job(&spec, &JobControl::new(), &sink).unwrap();
    let report = reports.into_inner().unwrap().pop().unwrap();
    assert_eq!(report.status, FileStatus::Failed);
    assert_eq!(report.error.unwrap().code, "file_too_large");

    settings.limits.max_file_bytes = u64::MAX / 2;
    settings.limits.max_pixels = 1000;
    let error = probe_bytes(&big, &settings.limits).unwrap_err();
    assert_eq!(error.code(), "dimensions_too_large");
}

#[test]
fn huge_gif_logical_screen_is_rejected_from_the_header() {
    // GIF header claiming 65535×65535 with no image data at all.
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&u16::MAX.to_le_bytes());
    gif.extend_from_slice(&u16::MAX.to_le_bytes());
    gif.extend_from_slice(&[0, 0, 0, 0x3B]);
    let limits = Limits {
        max_pixels: 100_000_000,
        ..Limits::default()
    };
    assert!(probe_bytes(&gif, &limits).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_loops_do_not_hang_the_scanner() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("a").join("loop")).unwrap();
    common::write_file(
        dir.path(),
        "a/x.png",
        &common::encode_with_image(&common::graphic(4, 4), image::ImageFormat::Png),
    );
    let report = scan_inputs(
        &[dir.path().to_path_buf()],
        &ScanOptions::default(),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(report.files.len(), 1);
}

#[test]
fn scanning_never_reports_temporary_or_output_files() {
    let dir = tempfile::tempdir().unwrap();
    let png = common::encode_with_image(&common::graphic(4, 4), image::ImageFormat::Png);
    common::write_file(dir.path(), "in.png", &png);
    common::write_file(dir.path(), "optimized/in.png", &png);
    common::write_file(dir.path(), ".imago-abc.tmp", &png);
    let options =
        imago_core::app::scan_options_for(&OptimizeSettings::default(), ScanOptions::default());
    let report = scan_inputs(&[dir.path().to_path_buf()], &options, &CancelToken::new()).unwrap();
    assert_eq!(report.files.len(), 1);
    assert!(report.files[0].path.ends_with("in.png"));
}
