//! What each format can do in this build.
//!
//! The GUI disables unavailable options and the CLI reports them, with an
//! alternative and instructions for enabling the format.

use serde::Serialize;

use crate::settings::ImageFormat;

/// Capabilities of one format in the current build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapability {
    /// The format.
    pub format: ImageFormat,
    /// Can be read.
    pub decode: bool,
    /// Can be written.
    pub encode: bool,
    /// Has a lossy mode (otherwise it is always written losslessly).
    pub lossy: bool,
    /// Has a lossless mode.
    pub lossless: bool,
    /// Can store transparency.
    pub alpha: bool,
    /// Can store animations.
    pub animation: bool,
    /// Can embed an ICC profile (otherwise pixels are converted to sRGB).
    pub icc: bool,
    /// Can embed EXIF metadata.
    pub exif: bool,
    /// Largest width/height the encoder accepts.
    pub max_dimension: Option<u32>,
    /// The encoder is young and every result is verified by decoding it again.
    pub experimental: bool,
    /// Why the format is (partly) unavailable.
    pub unavailable_reason: Option<String>,
    /// Suggested alternative output format.
    pub alternative: Option<ImageFormat>,
    /// How to enable the format.
    pub how_to_enable: Option<String>,
}

/// Capabilities of every known format.
pub fn capabilities() -> Vec<FormatCapability> {
    ImageFormat::ALL.into_iter().map(capability).collect()
}

/// Capabilities of one format.
pub fn capability(format: ImageFormat) -> FormatCapability {
    let base = FormatCapability {
        format,
        decode: true,
        encode: true,
        lossy: true,
        lossless: false,
        alpha: true,
        animation: false,
        icc: true,
        exif: true,
        max_dimension: None,
        experimental: false,
        unavailable_reason: None,
        alternative: None,
        how_to_enable: None,
    };
    match format {
        ImageFormat::Jpeg => FormatCapability {
            alpha: false,
            max_dimension: Some(65_500),
            ..base
        },
        ImageFormat::Png => FormatCapability {
            lossless: true,
            ..base
        },
        ImageFormat::Webp => FormatCapability {
            lossless: true,
            animation: true,
            max_dimension: Some(16_383),
            ..base
        },
        ImageFormat::Avif => avif_capability(base),
        ImageFormat::Tiff => FormatCapability {
            lossy: false,
            lossless: true,
            exif: false,
            ..base
        },
        ImageFormat::Bmp => FormatCapability {
            encode: false,
            lossy: false,
            icc: false,
            exif: false,
            unavailable_reason: Some("BMP is supported as an input format only".into()),
            alternative: Some(ImageFormat::Png),
            how_to_enable: Some("Choose PNG to keep the image lossless.".into()),
            ..base
        },
        ImageFormat::Gif => FormatCapability {
            animation: true,
            icc: false,
            exif: false,
            max_dimension: Some(65_535),
            ..base
        },
        ImageFormat::Jxl => jxl_capability(base),
    }
}

#[cfg(feature = "avif")]
fn avif_capability(base: FormatCapability) -> FormatCapability {
    FormatCapability {
        icc: false,
        unavailable_reason: Some("lossless AVIF is not supported by the AV1 encoder".into()),
        alternative: Some(ImageFormat::Webp),
        ..base
    }
}

#[cfg(not(feature = "avif"))]
fn avif_capability(base: FormatCapability) -> FormatCapability {
    FormatCapability {
        decode: false,
        encode: false,
        lossy: false,
        icc: false,
        unavailable_reason: Some("this build of Imago was compiled without AVIF support".into()),
        alternative: Some(ImageFormat::Webp),
        how_to_enable: Some(
            "Rebuild with the `avif` feature of imago-core (NASM is required on x86/x86_64)."
                .into(),
        ),
        ..base
    }
}

#[cfg(feature = "jxl")]
fn jxl_capability(base: FormatCapability) -> FormatCapability {
    FormatCapability {
        lossy: false,
        lossless: true,
        icc: false,
        exif: false,
        experimental: true,
        unavailable_reason: Some(
            "lossy JPEG XL encoding is not available (no stable pure-Rust encoder); images are written losslessly"
                .into(),
        ),
        alternative: Some(ImageFormat::Avif),
        ..base
    }
}

#[cfg(not(feature = "jxl"))]
fn jxl_capability(base: FormatCapability) -> FormatCapability {
    FormatCapability {
        decode: false,
        encode: false,
        lossy: false,
        icc: false,
        exif: false,
        unavailable_reason: Some("this build of Imago was compiled without JPEG XL support".into()),
        alternative: Some(ImageFormat::Webp),
        how_to_enable: Some("Rebuild with the `jxl` feature of imago-core.".into()),
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_is_listed_once() {
        let all = capabilities();
        assert_eq!(all.len(), ImageFormat::ALL.len());
        for format in ImageFormat::ALL {
            assert_eq!(all.iter().filter(|c| c.format == format).count(), 1);
        }
    }

    #[test]
    fn core_formats_are_always_available() {
        for format in [
            ImageFormat::Jpeg,
            ImageFormat::Png,
            ImageFormat::Webp,
            ImageFormat::Tiff,
        ] {
            let capability = capability(format);
            assert!(capability.decode && capability.encode, "{format}");
        }
    }

    #[test]
    fn capabilities_agree_with_format_traits() {
        for capability in capabilities() {
            if capability.alpha {
                assert!(capability.format.supports_alpha());
            }
            if capability.animation {
                assert!(capability.format.supports_animation());
            }
        }
    }

    #[test]
    fn bmp_is_input_only_and_suggests_png() {
        let bmp = capability(ImageFormat::Bmp);
        assert!(bmp.decode);
        assert!(!bmp.encode);
        assert_eq!(bmp.alternative, Some(ImageFormat::Png));
    }
}
