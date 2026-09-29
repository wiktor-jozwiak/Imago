//! Deterministic benchmark inputs.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageBuffer, Rgb, Rgba};

/// Photo-like content: gradients plus noise.
pub fn photo(width: u32, height: u32) -> DynamicImage {
    let mut state = 0x1234_5678u32;
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let noise = (state >> 28) as i32 - 8;
        let fx = x as f32 / width as f32;
        let fy = y as f32 / height as f32;
        let wave = ((fx * 14.0).sin() * (fy * 11.0).cos() * 45.0) as i32;
        let clamp = |v: i32| v.clamp(0, 255) as u8;
        Rgb([
            clamp((fx * 210.0) as i32 + wave + noise + 20),
            clamp((fy * 190.0) as i32 - wave / 2 + noise + 30),
            clamp(((1.0 - fx) * 170.0) as i32 + wave / 3 + noise + 50),
        ])
    }))
}

/// Logo with transparency and soft edges.
pub fn logo(width: u32, height: u32) -> DynamicImage {
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let radius = width.min(height) as f32 * 0.4;
    DynamicImage::ImageRgba8(ImageBuffer::from_fn(width, height, |x, y| {
        let distance = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        let alpha = ((radius - distance) / 2.0).clamp(0.0, 1.0);
        Rgba([40, (x % 256) as u8, 200, (alpha * 255.0) as u8])
    }))
}

/// Encodes with the `image` crate (benchmark inputs).
pub fn encoded(image: &DynamicImage, format: image::ImageFormat) -> Vec<u8> {
    let image = if format == image::ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(image.to_rgb8())
    } else {
        image.clone()
    };
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), format)
        .unwrap();
    bytes
}

/// Writes `count` photos of `width`×`height` as JPEG files into `dir`.
pub fn photo_folder(dir: &Path, count: usize, width: u32, height: u32) -> Vec<PathBuf> {
    let bytes = encoded(&photo(width, height), image::ImageFormat::Jpeg);
    (0..count)
        .map(|index| {
            let path = dir.join(format!("photo-{index:03}.jpg"));
            std::fs::write(&path, &bytes).unwrap();
            path
        })
        .collect()
}
