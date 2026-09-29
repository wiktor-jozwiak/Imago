//! Folder scanning: recursion, hidden files, filtering, deduplication.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use imago_core::cancel::CancelToken;
use imago_core::filesystem::scan::SkipReason;
use imago_core::filesystem::{ScanOptions, scan_inputs};

fn png() -> Vec<u8> {
    common::encode_with_image(&common::graphic(8, 8), image::ImageFormat::Png)
}

fn tree(dir: &Path) {
    common::write_file(dir, "a.png", &png());
    common::write_file(dir, "sub/b.png", &png());
    common::write_file(dir, "sub/deeper/c.png", &png());
    common::write_file(dir, ".hidden.png", &png());
    common::write_file(dir, ".cache/d.png", &png());
    common::write_file(dir, "optimized/old.png", &png());
    common::write_file(dir, "notes.txt", b"hello");
    common::write_file(dir, "fake.png", b"not an image");
    common::write_file(dir, "no-extension", &png());
    common::write_file(dir, ".imago-12345.tmp", b"leftover");
}

fn names(files: &[imago_core::filesystem::InputFile]) -> Vec<String> {
    let mut names: Vec<String> = files
        .iter()
        .map(|file| {
            file.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn scan(paths: &[PathBuf], options: &ScanOptions) -> imago_core::filesystem::ScanReport {
    scan_inputs(paths, options, &CancelToken::new()).unwrap()
}

#[test]
fn recursive_scan_finds_nested_images() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let options = ScanOptions {
        exclude_dir_names: vec!["optimized".into()],
        ..ScanOptions::default()
    };
    let report = scan(&[dir.path().to_path_buf()], &options);
    // "fake.png" is listed by extension; its content is re-checked before processing.
    assert_eq!(
        names(&report.files),
        vec!["a.png", "b.png", "c.png", "fake.png", "no-extension"]
    );
    assert!(
        report
            .files
            .iter()
            .all(|file| file.root.as_deref() == Some(dir.path()))
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|s| s.path.ends_with("notes.txt") && s.reason == SkipReason::Unsupported)
    );
}

#[test]
fn non_recursive_scan_stays_in_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let options = ScanOptions {
        recursive: false,
        ..ScanOptions::default()
    };
    let report = scan(&[dir.path().to_path_buf()], &options);
    assert_eq!(
        names(&report.files),
        vec!["a.png", "fake.png", "no-extension"]
    );
}

#[test]
fn hidden_files_are_optional() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let options = ScanOptions {
        include_hidden: true,
        exclude_dir_names: vec!["optimized".into()],
        ..ScanOptions::default()
    };
    let report = scan(&[dir.path().to_path_buf()], &options);
    let found = names(&report.files);
    assert!(found.contains(&".hidden.png".to_string()));
    assert!(found.contains(&"d.png".to_string()));
    // Imago's temporary files are never inputs.
    assert!(!found.iter().any(|name| name.ends_with(".tmp")));
}

#[test]
fn explicit_files_are_checked_by_content() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let files = vec![
        dir.path().join("fake.png"),
        dir.path().join("no-extension"),
        dir.path().join("a.png"),
    ];
    let report = scan(&files, &ScanOptions::default());
    assert_eq!(names(&report.files), vec!["a.png", "no-extension"]);
    assert!(report.files.iter().all(|file| file.root.is_none()));
    assert_eq!(report.skipped.len(), 1);
}

#[test]
fn duplicates_are_reported_once() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let paths = vec![
        dir.path().join("a.png"),
        dir.path().to_path_buf(),
        dir.path().join("a.png"),
    ];
    let report = scan(&paths, &ScanOptions::default());
    let a_count = report
        .files
        .iter()
        .filter(|file| file.path.ends_with("a.png"))
        .count();
    assert_eq!(a_count, 1);
    assert!(
        report
            .skipped
            .iter()
            .any(|s| s.reason == SkipReason::Duplicate)
    );
}

#[test]
fn missing_paths_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let error = scan_inputs(
        &[dir.path().join("missing")],
        &ScanOptions::default(),
        &CancelToken::new(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "not_found");
}

#[test]
fn scanning_can_be_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let token = CancelToken::new();
    token.cancel();
    let error =
        scan_inputs(&[dir.path().to_path_buf()], &ScanOptions::default(), &token).unwrap_err();
    assert!(error.is_cancelled());
}

#[cfg(windows)]
#[test]
fn windows_hidden_attribute_is_respected() {
    let dir = tempfile::tempdir().unwrap();
    let path = common::write_file(dir.path(), "visible-name.png", &png());
    let status = std::process::Command::new("attrib")
        .arg("+h")
        .arg(&path)
        .status();
    if !status.is_ok_and(|status| status.success()) {
        return;
    }
    let report = scan(&[dir.path().to_path_buf()], &ScanOptions::default());
    assert!(report.files.is_empty());
}
