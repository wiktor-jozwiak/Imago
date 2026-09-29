//! Codec benchmarks: encoding per format and quality, decoding, resizing and
//! whole conversions (decode + encode).
//!
//! Run with `cargo bench -p imago-core --bench codecs`.

#[path = "support/mod.rs"]
mod support;

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use imago_core::codecs::{self, EncodeInput, EncodeOptions};
use imago_core::image_engine::{ResizeQuality, decode, probe_bytes, resize};
use imago_core::settings::{Effort, ImageFormat, Limits, Quality};

fn options(quality: u8, lossless: bool) -> EncodeOptions {
    EncodeOptions {
        quality: Quality::new(quality).unwrap_or_default(),
        lossless,
        effort: Effort::Balanced,
    }
}

fn bench_encode(c: &mut Criterion) {
    let photo = support::photo(1024, 768);
    let pixels = u64::from(photo.width() * photo.height());
    let mut group = c.benchmark_group("encode_1mp");
    group.throughput(Throughput::Elements(pixels));
    group.sample_size(10);
    for format in [
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Avif,
        ImageFormat::Png,
        ImageFormat::Tiff,
    ] {
        group.bench_function(format.extension(), |b| {
            b.iter(|| {
                codecs::encode(
                    format,
                    EncodeInput::pixels(black_box(&photo)),
                    options(82, false),
                )
            })
        });
    }
    group.bench_function("png_lossless", |b| {
        b.iter(|| {
            codecs::encode(
                ImageFormat::Png,
                EncodeInput::pixels(black_box(&photo)),
                options(82, true),
            )
        })
    });
    group.finish();
}

fn bench_quality(c: &mut Criterion) {
    let photo = support::photo(800, 600);
    let mut group = c.benchmark_group("quality");
    group.sample_size(10);
    for format in [ImageFormat::Jpeg, ImageFormat::Webp, ImageFormat::Avif] {
        for quality in [50u8, 75, 90] {
            group.bench_with_input(
                BenchmarkId::new(format.extension(), quality),
                &quality,
                |b, &quality| {
                    b.iter(|| {
                        codecs::encode(
                            format,
                            EncodeInput::pixels(black_box(&photo)),
                            options(quality, false),
                        )
                    })
                },
            );
        }
    }
    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let photo = support::photo(1024, 768);
    let limits = Limits::default();
    let inputs = [
        ("jpeg", support::encoded(&photo, image::ImageFormat::Jpeg)),
        ("png", support::encoded(&photo, image::ImageFormat::Png)),
        (
            "webp_lossless",
            support::encoded(&photo, image::ImageFormat::WebP),
        ),
        (
            "avif",
            codecs::encode(
                ImageFormat::Avif,
                EncodeInput::pixels(&photo),
                options(82, false),
            )
            .unwrap_or_default(),
        ),
    ];
    let mut group = c.benchmark_group("decode_1mp");
    group.throughput(Throughput::Elements(u64::from(
        photo.width() * photo.height(),
    )));
    for (name, bytes) in &inputs {
        let Ok(probe) = probe_bytes(bytes, &limits) else {
            continue;
        };
        group.bench_function(*name, |b| {
            b.iter(|| decode(black_box(bytes), &probe, &limits))
        });
    }
    group.finish();
}

fn bench_resize(c: &mut Criterion) {
    let mut group = c.benchmark_group("resize");
    for (width, height) in [(1920u32, 1080u32), (4000, 3000)] {
        let photo = support::photo(width, height);
        group.throughput(Throughput::Elements(u64::from(width * height)));
        group.bench_with_input(
            BenchmarkId::new("lanczos3_half", format!("{width}x{height}")),
            &photo,
            |b, photo| {
                b.iter(|| resize(black_box(photo), width / 2, height / 2, ResizeQuality::High))
            },
        );
        group.bench_with_input(
            BenchmarkId::new("bilinear_thumb", format!("{width}x{height}")),
            &photo,
            |b, photo| {
                b.iter(|| {
                    resize(
                        black_box(photo),
                        256,
                        256 * height / width,
                        ResizeQuality::Fast,
                    )
                })
            },
        );
    }
    let logo = support::logo(2048, 2048);
    group.bench_function("lanczos3_alpha_2048_to_512", |b| {
        b.iter(|| resize(black_box(&logo), 512, 512, ResizeQuality::High))
    });
    group.finish();
}

/// Whole conversions as a user sees them: decode the file, encode the target.
fn bench_conversions(c: &mut Criterion) {
    let photo = support::photo(1024, 768);
    let logo = support::logo(512, 512);
    let limits = Limits::default();
    let jpeg = support::encoded(&photo, image::ImageFormat::Jpeg);
    let png_photo = support::encoded(&photo, image::ImageFormat::Png);
    let png_logo = support::encoded(&logo, image::ImageFormat::Png);
    let webp = codecs::encode(
        ImageFormat::Webp,
        EncodeInput::pixels(&photo),
        options(82, false),
    )
    .unwrap_or_default();

    let cases: [(&str, &[u8], ImageFormat); 5] = [
        ("jpeg_to_webp", &jpeg, ImageFormat::Webp),
        ("jpeg_to_avif", &jpeg, ImageFormat::Avif),
        ("png_to_webp", &png_photo, ImageFormat::Webp),
        ("png_to_png", &png_logo, ImageFormat::Png),
        ("webp_to_avif", &webp, ImageFormat::Avif),
    ];
    let mut group = c.benchmark_group("convert");
    group.sample_size(10);
    for (name, bytes, target) in cases {
        let Ok(probe) = probe_bytes(bytes, &limits) else {
            continue;
        };
        group.bench_function(name, |b| {
            b.iter(|| {
                let decoded =
                    decode(black_box(bytes), &probe, &limits).map(|decoded| decoded.image);
                decoded.and_then(|image| {
                    codecs::encode(target, EncodeInput::pixels(&image), options(82, false))
                })
            })
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_encode,
    bench_quality,
    bench_decode,
    bench_resize,
    bench_conversions
);
criterion_main!(benches);
