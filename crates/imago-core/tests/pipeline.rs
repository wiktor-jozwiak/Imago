//! End-to-end tests of jobs: scanning, the pipeline, workers and safe output.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use imago_core::cancel::CancelToken;
use imago_core::filesystem::{InputFile, ScanOptions, scan_inputs};
use imago_core::jobs::{
    DecisionReason, FileReport, FileStatus, FileTask, JobEvent, JobSpec, JobSummary,
};
use imago_core::settings::{
    AnimationPolicy, ConflictPolicy, FormatChoice, ImageFormat, MetadataSettings, OptimizeSettings,
    OutputLocation, ResizeMode, TransparencyPolicy, WhenLarger,
};
use imago_core::workers::{JobControl, run_job};

struct Run {
    events: Vec<JobEvent>,
    summary: JobSummary,
}

impl Run {
    fn reports(&self) -> Vec<FileReport> {
        let mut reports: Vec<FileReport> = self
            .events
            .iter()
            .filter_map(|event| match event {
                JobEvent::FileFinished { report } => Some((**report).clone()),
                _ => None,
            })
            .collect();
        reports.sort_by_key(|report| report.id);
        reports
    }

    fn report(&self, name: &str) -> FileReport {
        self.reports()
            .into_iter()
            .find(|report| report.input_path.file_name().unwrap() == name)
            .unwrap_or_else(|| panic!("no report for {name}"))
    }
}

fn tasks(files: &[PathBuf]) -> Vec<FileTask> {
    files
        .iter()
        .enumerate()
        .map(|(index, path)| FileTask {
            id: index as u64,
            input: InputFile::from_path(path).unwrap(),
        })
        .collect()
}

fn run_with(files: &[PathBuf], settings: OptimizeSettings, control: &JobControl) -> Run {
    let events = Mutex::new(Vec::new());
    let sink = |event: JobEvent| events.lock().unwrap().push(event);
    let spec = JobSpec {
        files: tasks(files),
        settings,
    };
    let summary = run_job(&spec, control, &sink).unwrap();
    Run {
        events: events.into_inner().unwrap(),
        summary,
    }
}

fn run(files: &[PathBuf], settings: OptimizeSettings) -> Run {
    run_with(files, settings, &JobControl::new())
}

fn fixed(format: ImageFormat) -> OptimizeSettings {
    OptimizeSettings {
        format: FormatChoice::Fixed(format),
        ..OptimizeSettings::default()
    }
}

fn no_temp_files_left(dir: &Path) -> bool {
    walk(dir).iter().all(|path| {
        let name = path.file_name().unwrap().to_string_lossy();
        !(name.starts_with(".imago-") && name.ends_with(".tmp"))
    })
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// A folder with a realistic mix of inputs.
fn mixed_folder(dir: &Path) -> Vec<PathBuf> {
    vec![
        common::write_file(
            dir,
            "photo.jpg",
            &common::encode_with_image(&common::photo(320, 240), image::ImageFormat::Jpeg),
        ),
        common::write_file(
            dir,
            "logo.png",
            &common::encode_with_image(
                &common::transparent_logo(200, 200),
                image::ImageFormat::Png,
            ),
        ),
        common::write_file(
            dir,
            "chart.png",
            &common::encode_with_image(&common::graphic(300, 200), image::ImageFormat::Png),
        ),
        common::write_file(
            dir,
            "scan.bmp",
            &common::encode_with_image(&common::photo(120, 90), image::ImageFormat::Bmp),
        ),
        common::write_file(
            dir,
            "anim.gif",
            &common::animated_gif(&common::animation_frames(48, 16, 4, false), 100),
        ),
    ]
}

#[test]
fn auto_mode_processes_a_mixed_batch_without_growing_files() {
    let dir = tempfile::tempdir().unwrap();
    let files = mixed_folder(dir.path());
    let originals: Vec<Vec<u8>> = files
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect();

    let result = run(&files, OptimizeSettings::default());
    assert_eq!(result.summary.done, files.len(), "{:#?}", result.reports());
    for report in result.reports() {
        let output = report.output_path.clone().unwrap();
        assert!(
            output.starts_with(dir.path().join("optimized")),
            "{}",
            output.display()
        );
        assert!(output.exists());
        assert!(
            report.output_size.unwrap() <= report.input_size,
            "{report:#?}"
        );
        let decision = report.decision.clone().unwrap();
        assert!(matches!(
            decision.reason,
            DecisionReason::Smallest | DecisionReason::KeptOriginal
        ));
        assert!(report.timings.total_ms > 0.0);
    }
    // Originals are untouched.
    for (path, original) in files.iter().zip(&originals) {
        assert_eq!(&std::fs::read(path).unwrap(), original);
    }
    assert!(no_temp_files_left(dir.path()));

    // Transparency and animation survive auto mode.
    let logo = result.report("logo.png");
    assert!(logo.decision.as_ref().unwrap().kept_transparency);
    let decoded = common::decode(&std::fs::read(logo.output_path.unwrap()).unwrap()).image;
    assert!(imago_core::image_engine::color::alpha_usage(&decoded).is_transparent());
    let anim = result.report("anim.gif");
    assert!(anim.decision.as_ref().unwrap().kept_animation);
    let anim_bytes = std::fs::read(anim.output_path.unwrap()).unwrap();
    assert!(
        imago_core::image_engine::probe_bytes(&anim_bytes, &Default::default())
            .unwrap()
            .animated
    );
}

#[test]
fn events_are_ordered_and_complete_with_many_workers() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<PathBuf> = (0..24)
        .map(|index| {
            common::write_file(
                dir.path(),
                &format!("img-{index:02}.png"),
                &common::encode_with_image(&common::photo(64 + index, 48), image::ImageFormat::Png),
            )
        })
        .collect();
    let settings = OptimizeSettings {
        workers: Some(4),
        ..fixed(ImageFormat::Webp)
    };
    let result = run(&files, settings);
    assert!(matches!(
        result.events.first(),
        Some(JobEvent::Started {
            total: 24,
            workers: 4
        })
    ));
    assert!(matches!(
        result.events.last(),
        Some(JobEvent::Finished { .. })
    ));
    let finished = result
        .events
        .iter()
        .filter(|e| matches!(e, JobEvent::FileFinished { .. }))
        .count();
    assert_eq!(finished, 24);
    assert_eq!(result.summary.done, 24);
    for report in result.reports() {
        assert_eq!(report.output_format, Some(ImageFormat::Webp));
        assert_eq!(report.output_path.unwrap().extension().unwrap(), "webp");
    }
}

#[test]
fn fixed_format_conversion_keeps_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "p.png",
        &common::encode_with_image(&common::photo(150, 100), image::ImageFormat::Png),
    )];
    let formats = [
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Avif,
        ImageFormat::Tiff,
        ImageFormat::Png,
    ];
    // Formats compiled out of this build (e.g. `--no-default-features`) are skipped.
    for format in formats
        .into_iter()
        .filter(|format| imago_core::codecs::capability(*format).encode)
    {
        let result = run(
            &files,
            OptimizeSettings {
                output: imago_core::settings::OutputSettings {
                    conflict: ConflictPolicy::Overwrite,
                    ..Default::default()
                },
                ..fixed(format)
            },
        );
        let report = result.report("p.png");
        assert_eq!(report.status, FileStatus::Done, "{format}: {report:#?}");
        assert_eq!(report.output_format, Some(format));
        let decoded = common::decode(&std::fs::read(report.output_path.unwrap()).unwrap());
        assert_eq!(
            (decoded.image.width(), decoded.image.height()),
            (150, 100),
            "{format}"
        );
    }
}

#[test]
fn resizing_follows_the_settings() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "big.jpg",
        &common::encode_with_image(&common::photo(800, 600), image::ImageFormat::Jpeg),
    )];
    let settings = OptimizeSettings {
        resize: imago_core::settings::ResizeSettings {
            mode: ResizeMode::Fit {
                max_width: Some(400),
                max_height: None,
            },
            allow_upscale: false,
        },
        ..fixed(ImageFormat::Jpeg)
    };
    let report = run(&files, settings).report("big.jpg");
    assert_eq!(report.output_dimensions, Some((400, 300)));
    let decoded = common::decode(&std::fs::read(report.output_path.unwrap()).unwrap());
    assert_eq!((decoded.image.width(), decoded.image.height()), (400, 300));
}

#[test]
fn lossless_png_output_is_pixel_exact() {
    let dir = tempfile::tempdir().unwrap();
    let image = common::transparent_logo(90, 70);
    let files = vec![common::write_file(
        dir.path(),
        "l.png",
        &common::encode_with_image(&image, image::ImageFormat::Png),
    )];
    let mut settings = fixed(ImageFormat::Png);
    settings.compression.lossless = true;
    let report = run(&files, settings).report("l.png");
    let decoded = common::decode(&std::fs::read(report.output_path.unwrap()).unwrap());
    assert_eq!(decoded.image.to_rgba8().as_raw(), image.to_rgba8().as_raw());
}

#[test]
fn original_format_mode_turns_bmp_into_png() {
    let dir = tempfile::tempdir().unwrap();
    let files = mixed_folder(dir.path());
    let settings = OptimizeSettings {
        format: FormatChoice::Original,
        ..OptimizeSettings::default()
    };
    let result = run(&files, settings);
    assert_eq!(
        result.report("scan.bmp").output_format,
        Some(ImageFormat::Png)
    );
    assert_eq!(
        result.report("photo.jpg").output_format,
        Some(ImageFormat::Jpeg)
    );
    assert_eq!(
        result.report("anim.gif").output_format,
        Some(ImageFormat::Gif)
    );
}

#[test]
fn transparency_is_never_dropped_without_consent() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "logo.png",
        &common::encode_with_image(&common::transparent_logo(80, 80), image::ImageFormat::Png),
    )];
    let report = run(&files, fixed(ImageFormat::Jpeg)).report("logo.png");
    assert_eq!(report.output_format, Some(ImageFormat::Png));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == "transparency_kept_in_other_format")
    );

    let consented = OptimizeSettings {
        transparency: TransparencyPolicy::FlattenWhite,
        output: imago_core::settings::OutputSettings {
            conflict: ConflictPolicy::Overwrite,
            ..Default::default()
        },
        ..fixed(ImageFormat::Jpeg)
    };
    let report = run(&files, consented).report("logo.png");
    assert_eq!(report.output_format, Some(ImageFormat::Jpeg));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == "transparency_flattened")
    );
}

#[test]
fn animations_are_preserved_unless_first_frame_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "anim.gif",
        &common::animated_gif(&common::animation_frames(48, 16, 4, false), 100),
    )];
    let report = run(&files, fixed(ImageFormat::Jpeg)).report("anim.gif");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == "animation_kept_in_input_format")
    );
    assert_eq!(report.output_format, Some(ImageFormat::Gif));

    let webp = run(&files, fixed(ImageFormat::Webp)).report("anim.gif");
    let bytes = std::fs::read(webp.output_path.unwrap()).unwrap();
    assert!(
        imago_core::image_engine::probe_bytes(&bytes, &Default::default())
            .unwrap()
            .animated
    );

    let first_frame = OptimizeSettings {
        animation: AnimationPolicy::FirstFrame,
        output: imago_core::settings::OutputSettings {
            conflict: ConflictPolicy::Overwrite,
            ..Default::default()
        },
        ..fixed(ImageFormat::Png)
    };
    let report = run(&files, first_frame).report("anim.gif");
    assert_eq!(report.output_format, Some(ImageFormat::Png));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == "animation_first_frame")
    );
}

#[test]
fn exif_orientation_is_applied_and_metadata_stripped() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "rotated.jpg",
        &common::jpeg_with_orientation(&common::photo(120, 60), 6),
    )];
    let report = run(&files, fixed(ImageFormat::Jpeg)).report("rotated.jpg");
    assert_eq!(report.input_dimensions, Some((60, 120)));
    assert_eq!(report.output_dimensions, Some((60, 120)));
    let bytes = std::fs::read(report.output_path.unwrap()).unwrap();
    assert!(!imago_core::metadata::contains_exif(
        &bytes,
        ImageFormat::Jpeg
    ));
    let decoded = common::decode(&bytes);
    assert_eq!((decoded.image.width(), decoded.image.height()), (60, 120));
}

#[test]
fn kept_exif_gets_its_orientation_reset() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "rotated.jpg",
        &common::jpeg_with_orientation(&common::photo(120, 60), 6),
    )];
    let settings = OptimizeSettings {
        metadata: MetadataSettings {
            strip_metadata: false,
            ..MetadataSettings::default()
        },
        output: imago_core::settings::OutputSettings {
            when_larger: WhenLarger::WriteAnyway,
            ..Default::default()
        },
        ..fixed(ImageFormat::Jpeg)
    };
    let report = run(&files, settings).report("rotated.jpg");
    let bytes = std::fs::read(report.output_path.unwrap()).unwrap();
    let probe = imago_core::image_engine::probe_bytes(&bytes, &Default::default()).unwrap();
    assert!(probe.has_exif);
    assert_eq!(probe.orientation, 1);
    assert_eq!((probe.width, probe.height), (60, 120));
}

#[test]
fn name_conflicts_follow_the_policy() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "a.png",
        &common::encode_with_image(&common::photo(40, 40), image::ImageFormat::Png),
    )];
    let first = run(&files, fixed(ImageFormat::Webp)).report("a.png");
    let first_path = first.output_path.unwrap();
    assert!(first_path.ends_with(Path::new("optimized").join("a.webp")));

    let renamed = run(&files, fixed(ImageFormat::Webp)).report("a.png");
    assert!(
        renamed
            .output_path
            .unwrap()
            .ends_with(Path::new("optimized").join("a (1).webp"))
    );

    let mut skip = fixed(ImageFormat::Webp);
    skip.output.conflict = ConflictPolicy::Skip;
    let skipped = run(&files, skip).report("a.png");
    assert_eq!(skipped.status, FileStatus::Skipped);
    assert_eq!(skipped.error.unwrap().code, "output_exists");

    std::fs::write(&first_path, b"stale").unwrap();
    let mut overwrite = fixed(ImageFormat::Webp);
    overwrite.output.conflict = ConflictPolicy::Overwrite;
    let replaced = run(&files, overwrite).report("a.png");
    assert_eq!(replaced.output_path.unwrap(), first_path);
    assert_ne!(std::fs::read(&first_path).unwrap(), b"stale");
}

#[test]
fn inputs_mapping_to_the_same_name_do_not_clobber_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![
        common::write_file(
            dir.path(),
            "x.png",
            &common::encode_with_image(&common::photo(30, 30), image::ImageFormat::Png),
        ),
        common::write_file(
            dir.path(),
            "x.jpg",
            &common::encode_with_image(&common::photo(31, 31), image::ImageFormat::Jpeg),
        ),
    ];
    let mut settings = fixed(ImageFormat::Webp);
    settings.output.conflict = ConflictPolicy::Overwrite;
    let result = run(&files, settings);
    let outputs: Vec<PathBuf> = result
        .reports()
        .into_iter()
        .map(|r| r.output_path.unwrap())
        .collect();
    assert_ne!(outputs[0], outputs[1]);
    assert!(outputs.iter().all(|path| path.exists()));
}

#[test]
fn originals_are_protected_unless_explicitly_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let original_bytes = common::encode_with_image(&common::photo(64, 64), image::ImageFormat::Png);
    let files = vec![common::write_file(
        dir.path(),
        "in-place.png",
        &original_bytes,
    )];
    let mut settings = OptimizeSettings {
        format: FormatChoice::Original,
        ..OptimizeSettings::default()
    };
    settings.output.location = OutputLocation::SameFolder;
    settings.output.conflict = ConflictPolicy::Overwrite;
    settings.output.when_larger = WhenLarger::WriteAnyway;

    let report = run(&files, settings.clone()).report("in-place.png");
    assert_eq!(report.status, FileStatus::Failed);
    assert_eq!(report.error.unwrap().code, "would_overwrite_original");
    assert_eq!(std::fs::read(&files[0]).unwrap(), original_bytes);

    settings.output.allow_overwrite_originals = true;
    let report = run(&files, settings).report("in-place.png");
    assert_eq!(report.status, FileStatus::Done);
    assert_eq!(report.output_path.unwrap(), files[0]);
    // Still a valid image of the same size.
    let decoded = common::decode(&std::fs::read(&files[0]).unwrap());
    assert_eq!(decoded.image.width(), 64);
    assert!(no_temp_files_left(dir.path()));
}

#[test]
fn a_corrupted_file_fails_alone() {
    let dir = tempfile::tempdir().unwrap();
    let good = common::encode_with_image(&common::photo(40, 40), image::ImageFormat::Png);
    let files = vec![
        common::write_file(dir.path(), "good.png", &good),
        common::write_file(dir.path(), "broken.png", &good[..good.len() / 2]),
        common::write_file(dir.path(), "text.png", b"not an image at all"),
    ];
    let result = run(&files, OptimizeSettings::default());
    assert_eq!(result.report("good.png").status, FileStatus::Done);
    let broken = result.report("broken.png");
    assert_eq!(broken.status, FileStatus::Failed);
    assert_eq!(broken.error.unwrap().code, "decode_failed");
    assert_eq!(
        result.report("text.png").error.unwrap().code,
        "unsupported_format"
    );
    assert_eq!(result.summary.failed, 2);
}

#[test]
fn cancelling_the_job_stops_every_file() {
    let dir = tempfile::tempdir().unwrap();
    let files = mixed_folder(dir.path());
    let control = JobControl::new();
    control.cancel();
    let result = run_with(&files, OptimizeSettings::default(), &control);
    assert_eq!(result.summary.cancelled, files.len());
    assert!(result.summary.was_cancelled);
    assert!(
        !dir.path().join("optimized").exists() || walk(&dir.path().join("optimized")).is_empty()
    );
}

#[test]
fn skipping_one_file_keeps_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let files = mixed_folder(dir.path());
    let control = JobControl::new();
    control.skip(1);
    let result = run_with(&files, OptimizeSettings::default(), &control);
    let skipped = &result.reports()[1];
    assert_eq!(skipped.status, FileStatus::Skipped);
    assert_eq!(skipped.error.as_ref().unwrap().code, "skipped_by_user");
    assert_eq!(result.summary.done, files.len() - 1);
}

#[test]
fn unicode_names_and_structure_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = common::encode_with_image(&common::photo(32, 32), image::ImageFormat::Png);
    common::write_file(dir.path(), "wakacje 2024/Łódź — plaża 🌊.png", &bytes);
    common::write_file(dir.path(), "wakacje 2024/zwierzęta/żółw.png", &bytes);
    let report = scan_inputs(
        &[dir.path().to_path_buf()],
        &ScanOptions::default(),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(report.files.len(), 2);
    let spec = JobSpec {
        files: report
            .files
            .into_iter()
            .enumerate()
            .map(|(id, input)| FileTask {
                id: id as u64,
                input,
            })
            .collect(),
        settings: fixed(ImageFormat::Webp),
    };
    let events = Mutex::new(Vec::new());
    let sink = |event: JobEvent| events.lock().unwrap().push(event);
    let summary = run_job(&spec, &JobControl::new(), &sink).unwrap();
    assert_eq!(summary.done, 2);
    let out = dir.path().join("optimized");
    assert!(
        out.join("wakacje 2024")
            .join("Łódź — plaża 🌊.webp")
            .exists()
    );
    assert!(
        out.join("wakacje 2024")
            .join("zwierzęta")
            .join("żółw.webp")
            .exists()
    );
}

#[test]
fn timestamps_can_be_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "old.png",
        &common::encode_with_image(&common::photo(20, 20), image::ImageFormat::Png),
    )];
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600 * 24 * 365);
    std::fs::File::options()
        .write(true)
        .open(&files[0])
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
    let mut settings = fixed(ImageFormat::Webp);
    settings.metadata.preserve_timestamps = true;
    let report = run(&files, settings).report("old.png");
    let modified = std::fs::metadata(report.output_path.unwrap())
        .unwrap()
        .modified()
        .unwrap();
    let difference = modified
        .duration_since(old)
        .unwrap_or_else(|error| error.duration());
    assert!(difference.as_secs() < 2);
}

#[test]
fn invalid_settings_are_rejected_before_the_job_starts() {
    let settings = OptimizeSettings {
        workers: Some(0),
        ..OptimizeSettings::default()
    };
    let spec = JobSpec {
        files: Vec::new(),
        settings,
    };
    let sink = |_: JobEvent| {};
    assert_eq!(
        run_job(&spec, &JobControl::new(), &sink)
            .unwrap_err()
            .code(),
        "invalid_settings"
    );
}

#[test]
fn lossless_jpeg_request_keeps_the_jpeg_data() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "photo.jpg",
        &common::jpeg_with_orientation(&common::photo(64, 48), 1),
    )];
    let mut settings = fixed(ImageFormat::Jpeg);
    settings.compression.lossless = true;
    let report = run(&files, settings).report("photo.jpg");
    assert_eq!(report.status, FileStatus::Done, "{report:#?}");
    let output = std::fs::read(report.output_path.unwrap()).unwrap();
    assert!(!imago_core::metadata::contains_exif(
        &output,
        ImageFormat::Jpeg
    ));
    let before = common::decode(&std::fs::read(&files[0]).unwrap()).image;
    let after = common::decode(&output).image;
    assert_eq!(before.to_rgb8().as_raw(), after.to_rgb8().as_raw());

    let png = common::write_file(
        dir.path(),
        "p.png",
        &common::encode_with_image(&common::photo(8, 8), image::ImageFormat::Png),
    );
    let mut to_jpeg = fixed(ImageFormat::Jpeg);
    to_jpeg.compression.lossless = true;
    let failed = run(&[png], to_jpeg).report("p.png");
    assert_eq!(failed.error.unwrap().code, "invalid_settings");
}

#[cfg(unix)]
#[test]
fn unreadable_files_report_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = common::write_file(
        dir.path(),
        "secret.png",
        &common::encode_with_image(&common::photo(8, 8), image::ImageFormat::Png),
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&path).is_ok() {
        return; // running as root: permissions are not enforced
    }
    let report = run(&[path], OptimizeSettings::default()).report("secret.png");
    assert_eq!(report.error.unwrap().code, "permission_denied");
}

#[cfg(windows)]
#[test]
fn read_only_output_reports_permission_denied() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![common::write_file(
        dir.path(),
        "a.png",
        &common::encode_with_image(&common::photo(8, 8), image::ImageFormat::Png),
    )];
    let target = common::write_file(&dir.path().join("optimized"), "a.webp", b"locked");
    let mut permissions = std::fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&target, permissions).unwrap();
    let mut settings = fixed(ImageFormat::Webp);
    settings.output.conflict = ConflictPolicy::Overwrite;
    let report = run(&files, settings).report("a.png");
    assert_eq!(report.status, FileStatus::Failed);
    assert_eq!(report.error.unwrap().code, "permission_denied");
    assert_eq!(std::fs::read(&target).unwrap(), b"locked");
    assert!(no_temp_files_left(dir.path()));
}
