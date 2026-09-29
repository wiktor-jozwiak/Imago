//! End-to-end tests of the `imago` binary.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use image::{DynamicImage, ImageBuffer, Rgb, Rgba};
use predicates::prelude::*;

fn imago() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_imago"));
    command.env_remove("IMAGO_LOG");
    command
}

fn photo(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([
            (x * 3 % 256) as u8,
            (y * 5 % 256) as u8,
            ((x ^ y) % 256) as u8,
        ])
    }))
}

fn write_image(
    dir: &Path,
    name: &str,
    image: &DynamicImage,
    format: image::ImageFormat,
) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), format)
        .unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn decode(path: &Path) -> DynamicImage {
    image::ImageReader::open(path)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
}

#[test]
fn help_and_version_work() {
    imago()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Fast. Local. Smaller images."))
        .stdout(predicate::str::contains("--quality"))
        .stdout(predicate::str::contains("Exit codes"));
    imago()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn lists_formats() {
    imago()
        .arg("--list-formats")
        .assert()
        .success()
        .stdout(predicate::str::contains("WebP"))
        .stdout(predicate::str::contains("BMP"));
}

#[test]
fn converts_a_single_file_to_the_named_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_image(
        dir.path(),
        "input.png",
        &photo(64, 48),
        image::ImageFormat::Png,
    );
    let output = dir.path().join("out").join("result.webp");
    imago()
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .assert()
        .success();
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[8..12], b"WEBP");
    assert_eq!(decode(&output).width(), 64);
}

#[test]
fn mismatched_output_extension_is_corrected() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_image(
        dir.path(),
        "input.png",
        &photo(32, 32),
        image::ImageFormat::Png,
    );
    imago()
        .arg(&input)
        .args(["--format", "webp", "--quality", "82", "--output"])
        .arg(dir.path().join("optimized.jpg"))
        .assert()
        .success()
        .stderr(predicate::str::contains("optimized.webp"));
    assert!(dir.path().join("optimized.webp").exists());
    assert!(!dir.path().join("optimized.jpg").exists());
}

#[test]
fn folders_are_recursive_only_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let photos = dir.path().join("photos");
    write_image(&photos, "a.png", &photo(40, 30), image::ImageFormat::Png);
    write_image(
        &photos,
        "sub/b.png",
        &photo(40, 30),
        image::ImageFormat::Png,
    );
    let out = dir.path().join("out");

    imago()
        .arg(&photos)
        .arg("--output")
        .arg(&out)
        .args(["--format", "webp"])
        .assert()
        .success();
    assert!(out.join("a.webp").exists());
    assert!(!out.join("sub").join("b.webp").exists());

    imago()
        .arg(&photos)
        .arg("--output")
        .arg(&out)
        .args([
            "--format",
            "webp",
            "--recursive",
            "--skip-existing",
            "--workers",
            "2",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("skipped"));
    assert!(out.join("sub").join("b.webp").exists());
}

#[test]
fn default_output_is_the_optimized_folder() {
    let dir = tempfile::tempdir().unwrap();
    write_image(
        dir.path(),
        "a.jpg",
        &photo(80, 60),
        image::ImageFormat::Jpeg,
    );
    imago().arg(dir.path()).assert().success();
    let produced: Vec<_> = std::fs::read_dir(dir.path().join("optimized"))
        .unwrap()
        .collect();
    assert_eq!(produced.len(), 1);
}

#[test]
fn resize_and_lossless_options_are_applied() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_image(
        dir.path(),
        "big.png",
        &photo(200, 100),
        image::ImageFormat::Png,
    );
    let out = dir.path().join("small.png");
    imago()
        .arg(&input)
        .arg("-o")
        .arg(&out)
        .args(["--width", "50"])
        .assert()
        .success();
    assert_eq!((decode(&out).width(), decode(&out).height()), (50, 25));

    let logo = DynamicImage::ImageRgba8(ImageBuffer::from_fn(30, 20, |x, y| {
        Rgba([x as u8, y as u8, 9, 128])
    }));
    let input = write_image(dir.path(), "logo.png", &logo, image::ImageFormat::Png);
    let exact = dir.path().join("exact.png");
    imago()
        .arg(&input)
        .arg("-o")
        .arg(&exact)
        .arg("--lossless")
        .assert()
        .success();
    assert_eq!(decode(&exact).to_rgba8().as_raw(), logo.to_rgba8().as_raw());
}

#[test]
fn zip_and_json_report() {
    let dir = tempfile::tempdir().unwrap();
    let photos = dir.path().join("photos");
    write_image(&photos, "a.png", &photo(40, 30), image::ImageFormat::Png);
    write_image(&photos, "b.png", &photo(41, 30), image::ImageFormat::Png);
    let zip = dir.path().join("pack.zip");
    let output = imago()
        .arg(&photos)
        .args(["--format", "webp", "--json", "--zip"])
        .arg(&zip)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["done"], 2);
    assert_eq!(report["files"].as_array().unwrap().len(), 2);
    assert_eq!(report["archive"]["files"], 2);
    let bytes = std::fs::read(&zip).unwrap();
    assert!(bytes.starts_with(b"PK"));
}

#[test]
fn failures_and_fatal_errors_have_distinct_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    // Invalid arguments → 2.
    imago().args(["x.png", "--quality", "0"]).assert().code(2);
    // Missing input → 3.
    imago()
        .arg(dir.path().join("missing.png"))
        .assert()
        .code(3)
        .stderr(predicate::str::contains("not found"));
    // Folder without images → 3.
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    std::fs::write(empty.join("notes.txt"), b"hello").unwrap();
    imago()
        .arg(&empty)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no supported images"));
    // A broken image among good ones → 1, good ones still written.
    let mixed = dir.path().join("mixed");
    write_image(&mixed, "good.png", &photo(20, 20), image::ImageFormat::Png);
    std::fs::write(mixed.join("broken.png"), b"\x89PNG\r\n\x1a\n broken").unwrap();
    imago()
        .arg(&mixed)
        .args(["--format", "webp"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("broken.png"));
    assert!(mixed.join("optimized").join("good.webp").exists());
}

#[test]
fn quiet_mode_prints_only_errors() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_image(dir.path(), "a.png", &photo(20, 20), image::ImageFormat::Png);
    imago()
        .arg(&input)
        .arg("--quiet")
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::is_empty());
}

#[test]
fn unicode_paths_are_supported() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("zdjęcia żółwia 🐢");
    write_image(&folder, "Łódź.png", &photo(24, 24), image::ImageFormat::Png);
    imago()
        .arg(&folder)
        .args(["--format", "avif"])
        .assert()
        .success();
    assert!(folder.join("optimized").join("Łódź.avif").exists());
}
