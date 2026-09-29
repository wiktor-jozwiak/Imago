//! Pipeline benchmarks: one file end to end, and a batch with different
//! numbers of workers.
//!
//! Run with `cargo bench -p imago-core --bench pipeline`.

#[path = "support/mod.rs"]
mod support;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use imago_core::filesystem::InputFile;
use imago_core::jobs::{FileTask, JobEvent, JobSpec};
use imago_core::settings::{
    ConflictPolicy, FormatChoice, ImageFormat, OptimizeSettings, OutputLocation,
};
use imago_core::workers::{JobControl, run_job};

fn spec(
    files: &[std::path::PathBuf],
    out: &std::path::Path,
    format: FormatChoice,
    workers: usize,
) -> JobSpec {
    let mut settings = OptimizeSettings {
        format,
        workers: Some(workers),
        ..OptimizeSettings::default()
    };
    settings.output.location = OutputLocation::Directory {
        path: out.to_path_buf(),
    };
    settings.output.conflict = ConflictPolicy::Overwrite;
    settings.output.fsync = false;
    JobSpec {
        files: files
            .iter()
            .enumerate()
            .filter_map(|(id, path)| {
                InputFile::from_path(path).ok().map(|input| FileTask {
                    id: id as u64,
                    input,
                })
            })
            .collect(),
        settings,
    }
}

fn run(spec: &JobSpec) {
    let sink = |_: JobEvent| {};
    let _ = run_job(spec, &JobControl::new(), &sink);
}

fn bench_single_file(c: &mut Criterion) {
    let dir = tempfile::tempdir().ok();
    let Some(dir) = dir else { return };
    let files = support::photo_folder(dir.path(), 1, 1600, 1200);
    let out = dir.path().join("out");
    let mut group = c.benchmark_group("single_file_2mp");
    group.sample_size(10);
    for (name, format) in [
        ("webp", FormatChoice::Fixed(ImageFormat::Webp)),
        ("jpeg", FormatChoice::Fixed(ImageFormat::Jpeg)),
        ("auto", FormatChoice::Auto),
    ] {
        let spec = spec(&files, &out, format, 1);
        group.bench_function(name, |b| b.iter(|| run(&spec)));
    }
    group.finish();
}

fn bench_batch_workers(c: &mut Criterion) {
    let dir = tempfile::tempdir().ok();
    let Some(dir) = dir else { return };
    let files = support::photo_folder(dir.path(), 32, 800, 600);
    let out = dir.path().join("out");
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let mut group = c.benchmark_group("batch_32_files_webp");
    group.sample_size(10);
    group.throughput(Throughput::Elements(files.len() as u64));
    for workers in [1usize, 2, 4, 8, 16]
        .into_iter()
        .filter(|&workers| workers <= cores.max(1))
    {
        let spec = spec(
            &files,
            &out,
            FormatChoice::Fixed(ImageFormat::Webp),
            workers,
        );
        group.bench_with_input(BenchmarkId::from_parameter(workers), &spec, |b, spec| {
            b.iter(|| run(spec))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_single_file, bench_batch_workers);
criterion_main!(benches);
