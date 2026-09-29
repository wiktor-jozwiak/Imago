//! Automatic format selection.
//!
//! The image is classified from its pixels; each class gets a short list of
//! candidate encodings. The pipeline encodes every candidate, keeps the
//! smallest, and falls back to the original file when nothing is smaller.
//!
//! Rules (from the specification):
//! * photos → WebP, AVIF, JPEG (AVIF skipped in the fastest mode);
//! * transparency → WebP or PNG (alpha is never removed);
//! * simple graphics and icons → PNG or lossless WebP (exact pixels);
//! * animations → animated WebP or the original animation;
//! * JPEG XL is never picked automatically (limited viewer support).

use image::DynamicImage;
use serde::Serialize;

use crate::codecs::capability;
use crate::image_engine::color::{AlphaUsage, alpha_usage, count_colors_up_to};
use crate::settings::{Effort, ImageFormat};

/// Largest side of an image treated as an icon.
pub const ICON_MAX_SIDE: u32 = 256;
/// Images with at most this many colors are treated as flat graphics.
pub const GRAPHIC_MAX_COLORS: usize = 256;

/// What kind of picture an image is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageKind {
    /// Continuous-tone content (many colors).
    Photo,
    /// Flat graphic with few colors (logos, charts, screenshots of UI).
    Graphic,
    /// Small image (≤ 256 px).
    Icon,
}

/// Result of pixel analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Analysis {
    /// Picture kind.
    pub kind: ImageKind,
    /// Alpha channel usage.
    pub alpha: AlphaUsage,
    /// Distinct colors, capped at `GRAPHIC_MAX_COLORS + 1`.
    pub colors: usize,
}

/// Classifies an (already oriented and resized) image.
pub fn analyze(image: &DynamicImage) -> Analysis {
    let alpha = alpha_usage(image);
    let colors = count_colors_up_to(image, GRAPHIC_MAX_COLORS);
    let kind = if image.width() <= ICON_MAX_SIDE && image.height() <= ICON_MAX_SIDE {
        ImageKind::Icon
    } else if colors <= GRAPHIC_MAX_COLORS {
        ImageKind::Graphic
    } else {
        ImageKind::Photo
    };
    Analysis {
        kind,
        alpha,
        colors,
    }
}

/// One encoding to try.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// Output format.
    pub format: ImageFormat,
    /// Encode losslessly (overrides the quality setting).
    pub lossless: bool,
}

impl Candidate {
    const fn lossy(format: ImageFormat) -> Self {
        Candidate {
            format,
            lossless: false,
        }
    }

    const fn exact(format: ImageFormat) -> Self {
        Candidate {
            format,
            lossless: true,
        }
    }
}

/// Candidate encodings for a still image in auto mode.
///
/// `lossless` is the user's lossless setting: only lossless encodings are
/// proposed then.
pub fn candidates(analysis: &Analysis, lossless: bool, effort: Effort) -> Vec<Candidate> {
    let transparent = analysis.alpha.is_transparent();
    let list: Vec<Candidate> = if lossless {
        vec![
            Candidate::exact(ImageFormat::Png),
            Candidate::exact(ImageFormat::Webp),
        ]
    } else {
        match (analysis.kind, transparent) {
            // Few colors: exact palettes beat lossy codecs and keep edges crisp.
            (ImageKind::Icon | ImageKind::Graphic, _) if analysis.colors <= GRAPHIC_MAX_COLORS => {
                vec![
                    Candidate::lossy(ImageFormat::Png),
                    Candidate::exact(ImageFormat::Webp),
                ]
            }
            (_, true) => vec![
                Candidate::lossy(ImageFormat::Webp),
                Candidate::lossy(ImageFormat::Png),
            ],
            (ImageKind::Icon, false) => {
                vec![
                    Candidate::lossy(ImageFormat::Webp),
                    Candidate::lossy(ImageFormat::Png),
                ]
            }
            (_, false) => {
                let mut photo = vec![Candidate::lossy(ImageFormat::Webp)];
                if effort != Effort::Fast {
                    photo.push(Candidate::lossy(ImageFormat::Avif));
                }
                photo.push(Candidate::lossy(ImageFormat::Jpeg));
                photo
            }
        }
    };
    list.into_iter()
        .filter(|candidate| capability(candidate.format).encode)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};

    fn photo(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([
                (x * 7 % 256) as u8,
                (y * 13 % 256) as u8,
                ((x + y) % 256) as u8,
            ])
        }))
    }

    fn formats(list: &[Candidate]) -> Vec<ImageFormat> {
        list.iter().map(|candidate| candidate.format).collect()
    }

    #[test]
    fn photos_consider_webp_avif_and_jpeg() {
        let analysis = analyze(&photo(400, 300));
        assert_eq!(analysis.kind, ImageKind::Photo);
        let list = candidates(&analysis, false, Effort::Balanced);
        assert!(formats(&list).contains(&ImageFormat::Webp));
        assert!(formats(&list).contains(&ImageFormat::Jpeg));
        let fast = candidates(&analysis, false, Effort::Fast);
        assert!(!formats(&fast).contains(&ImageFormat::Avif));
    }

    #[test]
    fn transparent_images_never_get_jpeg() {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(400, 300, |x, y| {
            Rgba([
                (x % 256) as u8,
                (y % 256) as u8,
                9,
                if x < 10 { 0 } else { 255 },
            ])
        }));
        let analysis = analyze(&image);
        assert!(analysis.alpha.is_transparent());
        let list = candidates(&analysis, false, Effort::Balanced);
        assert_eq!(formats(&list), vec![ImageFormat::Webp, ImageFormat::Png]);
    }

    #[test]
    fn flat_graphics_get_exact_encodings() {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_fn(400, 300, |x, _| {
            if x < 200 {
                Rgb([255, 0, 0])
            } else {
                Rgb([0, 0, 255])
            }
        }));
        let analysis = analyze(&image);
        assert_eq!(analysis.kind, ImageKind::Graphic);
        let list = candidates(&analysis, false, Effort::Balanced);
        assert_eq!(list[0].format, ImageFormat::Png);
        assert!(
            list.iter()
                .any(|c| c.format == ImageFormat::Webp && c.lossless)
        );
    }

    #[test]
    fn small_images_are_icons() {
        assert_eq!(analyze(&photo(64, 64)).kind, ImageKind::Icon);
    }

    #[test]
    fn lossless_mode_only_proposes_lossless_candidates() {
        let list = candidates(&analyze(&photo(400, 300)), true, Effort::Balanced);
        assert!(list.iter().all(|candidate| candidate.lossless));
        assert!(!formats(&list).contains(&ImageFormat::Jpeg));
    }

    #[test]
    fn jpeg_xl_is_never_chosen_automatically() {
        for lossless in [false, true] {
            for image in [photo(400, 300), photo(32, 32)] {
                let list = candidates(&analyze(&image), lossless, Effort::Max);
                assert!(!formats(&list).contains(&ImageFormat::Jxl));
            }
        }
    }
}
