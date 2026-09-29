//! Deterministic test images, generated in code so the repository needs no
//! binary fixtures.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageBuffer, Luma, Rgb, Rgba, RgbaImage};

/// Small deterministic pseudo-random generator (xorshift32).
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Self {
        Noise(seed.max(1))
    }

    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    pub fn byte(&mut self) -> u8 {
        (self.next() >> 24) as u8
    }
}

/// Photo-like content: smooth gradients plus fine noise (many colors).
pub fn photo(width: u32, height: u32) -> DynamicImage {
    let mut noise = Noise::new(width * 31 + height);
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
        let fx = x as f32 / width.max(1) as f32;
        let fy = y as f32 / height.max(1) as f32;
        let wave = ((fx * 12.0).sin() * (fy * 9.0).cos() * 40.0) as i32;
        let jitter = i32::from(noise.byte() % 12) - 6;
        let clamp = |v: i32| v.clamp(0, 255) as u8;
        Rgb([
            clamp((fx * 200.0) as i32 + wave + jitter + 20),
            clamp((fy * 180.0) as i32 - wave / 2 + jitter + 40),
            clamp(((1.0 - fx) * 160.0) as i32 + wave / 3 + jitter + 60),
        ])
    }))
}

/// Flat graphic with a handful of colors (like a chart or logo).
pub fn graphic(width: u32, height: u32) -> DynamicImage {
    let palette = [
        Rgb([255, 255, 255]),
        Rgb([30, 90, 160]),
        Rgb([230, 120, 40]),
        Rgb([60, 170, 90]),
        Rgb([20, 20, 20]),
    ];
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
        let band = ((x / 16 + y / 24) % palette.len() as u32) as usize;
        if (x as i32 - width as i32 / 2).pow(2) + (y as i32 - height as i32 / 2).pow(2)
            < (width.min(height) as i32 / 4).pow(2)
        {
            palette[4]
        } else {
            palette[band]
        }
    }))
}

/// Logo on a transparent background with soft (partially transparent) edges.
pub fn transparent_logo(width: u32, height: u32) -> DynamicImage {
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let radius = width.min(height) as f32 * 0.35;
    DynamicImage::ImageRgba8(ImageBuffer::from_fn(width, height, |x, y| {
        let distance = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        let alpha = ((radius + 1.5 - distance) / 3.0).clamp(0.0, 1.0);
        let shade = (x * 255 / width.max(1)) as u8;
        if alpha == 0.0 {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([shade, 80, 200, (alpha * 255.0).round() as u8])
        }
    }))
}

/// Photo with an alpha channel that is fully opaque.
pub fn opaque_rgba(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(photo(width, height).to_rgba8())
}

/// 8-bit grayscale gradient.
pub fn gray(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageLuma8(ImageBuffer::from_fn(width, height, |x, y| {
        Luma([((x + y) * 255 / (width + height).max(1)) as u8])
    }))
}

/// 16-bit RGB gradient using the full sample range.
pub fn rgb16(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb16(ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([
            (x * 65535 / width.max(1)) as u16,
            (y * 65535 / height.max(1)) as u16,
            ((x * y) % 65536) as u16,
        ])
    }))
}

/// Encodes with the `image` crate (used to create *input* files).
pub fn encode_with_image(image: &DynamicImage, format: image::ImageFormat) -> Vec<u8> {
    let mut bytes = Vec::new();
    let image = if format == image::ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(image.to_rgb8())
    } else {
        image.clone()
    };
    image
        .write_to(&mut Cursor::new(&mut bytes), format)
        .unwrap();
    bytes
}

/// A JPEG whose EXIF orientation tag has the given value.
pub fn jpeg_with_orientation(image: &DynamicImage, orientation: u8) -> Vec<u8> {
    use img_parts::jpeg::Jpeg;
    use img_parts::{Bytes, ImageEXIF};
    let plain = encode_with_image(image, image::ImageFormat::Jpeg);
    let mut jpeg = Jpeg::from_bytes(Bytes::from(plain)).unwrap();
    jpeg.set_exif(Some(Bytes::from(
        imago_core::metadata::orientation_only_exif(orientation),
    )));
    let mut out = Vec::new();
    jpeg.encoder().write_to(&mut out).unwrap();
    out
}

/// Frames of a small animation: a square moving over a background.
pub fn animation_frames(
    width: u32,
    height: u32,
    count: u32,
    transparent_background: bool,
) -> Vec<RgbaImage> {
    (0..count)
        .map(|index| {
            ImageBuffer::from_fn(width, height, |x, y| {
                let left = index * 4;
                if (left..left + 8).contains(&x) && (4..12).contains(&y) {
                    Rgba([220, 40, 40, 255])
                } else if transparent_background {
                    Rgba([0, 0, 0, 0])
                } else {
                    Rgba([240, 240, 255, 255])
                }
            })
        })
        .collect()
}

/// An animated GIF created with the `image` crate.
pub fn animated_gif(frames: &[RgbaImage], delay_ms: u32) -> Vec<u8> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for frame in frames {
            encoder
                .encode_frame(Frame::from_parts(
                    frame.clone(),
                    0,
                    0,
                    Delay::from_numer_denom_ms(delay_ms, 1),
                ))
                .unwrap();
        }
    }
    bytes
}

/// Peak signal-to-noise ratio between two RGBA renderings, in dB.
pub fn psnr(a: &DynamicImage, b: &DynamicImage) -> f64 {
    let a = a.to_rgba8();
    let b = b.to_rgba8();
    assert_eq!(a.dimensions(), b.dimensions());
    let mut sum = 0f64;
    for (pa, pb) in a.as_raw().iter().zip(b.as_raw()) {
        let diff = f64::from(*pa) - f64::from(*pb);
        sum += diff * diff;
    }
    let mse = sum / a.as_raw().len() as f64;
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0 * 255.0 / mse).log10()
    }
}

/// Decodes bytes with Imago's own probe + decode.
pub fn decode(bytes: &[u8]) -> imago_core::image_engine::DecodedImage {
    let limits = imago_core::settings::Limits::default();
    let probe = imago_core::image_engine::probe_bytes(bytes, &limits).unwrap();
    imago_core::image_engine::decode(bytes, &probe, &limits).unwrap()
}

/// Writes bytes into a file inside `dir` and returns its path.
pub fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, bytes).unwrap();
    path
}

/// CRC-32 (IEEE) used by PNG chunks, for building hand-crafted test files.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
