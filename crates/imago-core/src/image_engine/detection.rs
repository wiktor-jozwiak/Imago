//! Format detection from file content ("magic numbers").
//!
//! The file extension is never trusted: [`sniff`] looks at the first bytes of
//! the file. The extension is only used to warn about mismatches.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::errors::{ImagoError, Result};
use crate::settings::ImageFormat;

/// Number of leading bytes needed by [`sniff`].
pub const SNIFF_LEN: usize = 64;

/// Result of content-based format detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "format", rename_all = "camelCase")]
pub enum Detected {
    /// A format Imago can process.
    Supported(ImageFormat),
    /// A recognized format that Imago does not process (HEIC, PDF, ...).
    Unsupported(&'static str),
    /// Not a recognized image format.
    Unknown,
}

impl Detected {
    /// The supported format, if any.
    pub fn format(self) -> Option<ImageFormat> {
        match self {
            Detected::Supported(format) => Some(format),
            _ => None,
        }
    }

    /// Converts the detection into a format or a descriptive error.
    pub fn require_supported(self) -> Result<ImageFormat> {
        match self {
            Detected::Supported(format) => Ok(format),
            Detected::Unsupported(name) => Err(ImagoError::UnsupportedFormat {
                detected: Some(name.to_owned()),
            }),
            Detected::Unknown => Err(ImagoError::UnsupportedFormat { detected: None }),
        }
    }
}

/// Detects the image format from the leading bytes of a file.
pub fn sniff(header: &[u8]) -> Detected {
    if header.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Detected::Supported(ImageFormat::Jpeg);
    }
    if header.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Detected::Supported(ImageFormat::Png);
    }
    if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        return Detected::Supported(ImageFormat::Gif);
    }
    if header.len() >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        return Detected::Supported(ImageFormat::Webp);
    }
    if header.starts_with(b"II*\0")
        || header.starts_with(b"MM\0*")
        || header.starts_with(b"II+\0")
        || header.starts_with(b"MM\0+")
    {
        return Detected::Supported(ImageFormat::Tiff);
    }
    if is_bmp(header) {
        return Detected::Supported(ImageFormat::Bmp);
    }
    if header.starts_with(&[0xFF, 0x0A])
        || header.starts_with(&[
            0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
        ])
    {
        return Detected::Supported(ImageFormat::Jxl);
    }
    if let Some(detected) = sniff_iso_bmff(header) {
        return detected;
    }
    sniff_unsupported(header)
}

/// Reads the beginning of a file and detects its format.
pub fn detect_file(path: &Path) -> Result<Detected> {
    let mut file = File::open(path).map_err(|error| ImagoError::io(path, error))?;
    let mut header = [0u8; SNIFF_LEN];
    let mut filled = 0;
    while filled < header.len() {
        let read = file
            .read(&mut header[filled..])
            .map_err(|error| ImagoError::io(path, error))?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(sniff(&header[..filled]))
}

/// Format implied by the file extension, if it is a known image extension.
pub fn format_from_extension(path: &Path) -> Option<ImageFormat> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(ImageFormat::from_extension)
}

/// Whether the extension looks like an image Imago can read (used for quick
/// folder filtering before the content is sniffed).
pub fn has_image_extension(path: &Path) -> bool {
    format_from_extension(path).is_some()
}

fn is_bmp(header: &[u8]) -> bool {
    if header.len() < 18 || !header.starts_with(b"BM") {
        return false;
    }
    // The DIB header size identifies the BMP variant; anything else is not a bitmap.
    let dib_size = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);
    matches!(dib_size, 12 | 16 | 40 | 52 | 56 | 64 | 108 | 124)
}

/// ISO base media file format (`ftyp` box): AVIF, HEIC and friends.
fn sniff_iso_bmff(header: &[u8]) -> Option<Detected> {
    if header.len() < 16 || &header[4..8] != b"ftyp" {
        return None;
    }
    let box_size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let box_end = box_size.clamp(16, header.len());
    let major = &header[8..12];
    // Compatible brands follow the major brand and the minor version.
    let brands = std::iter::once(major).chain(
        header[16..box_end]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|brand| brand.as_slice()),
    );

    let mut is_avif = false;
    let mut is_heic = false;
    for brand in brands {
        match brand {
            b"avif" | b"avis" => is_avif = true,
            b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"hevm" | b"hevs" => {
                is_heic = true
            }
            _ => {}
        }
    }
    if is_avif {
        Some(Detected::Supported(ImageFormat::Avif))
    } else if is_heic {
        Some(Detected::Unsupported("HEIC"))
    } else if major == b"mif1" || major == b"msf1" {
        Some(Detected::Unsupported("HEIF"))
    } else {
        None
    }
}

fn sniff_unsupported(header: &[u8]) -> Detected {
    const SIGNATURES: &[(&[u8], &str)] = &[
        (b"%PDF", "PDF"),
        (b"8BPS", "PSD"),
        (b"\x00\x00\x01\x00", "ICO"),
        (b"\x00\x00\x02\x00", "CUR"),
        (b"qoif", "QOI"),
        (b"DDS ", "DDS"),
        (b"#?RADIANCE", "HDR"),
        (b"\x76\x2F\x31\x01", "OpenEXR"),
        (b"<svg", "SVG"),
    ];
    for (signature, name) in SIGNATURES {
        if header.starts_with(signature) {
            return Detected::Unsupported(name);
        }
    }
    let text_start = header
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map(|start| &header[start..])
        .unwrap_or_default();
    if text_start.starts_with(b"<?xml") || text_start.starts_with(b"<svg") {
        return Detected::Unsupported("SVG");
    }
    Detected::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded(prefix: &[u8]) -> Vec<u8> {
        let mut bytes = prefix.to_vec();
        bytes.resize(SNIFF_LEN, 0);
        bytes
    }

    #[test]
    fn detects_supported_signatures() {
        let cases: Vec<(Vec<u8>, ImageFormat)> = vec![
            (padded(&[0xFF, 0xD8, 0xFF, 0xE0]), ImageFormat::Jpeg),
            (padded(b"\x89PNG\r\n\x1a\n"), ImageFormat::Png),
            (padded(b"GIF89a"), ImageFormat::Gif),
            (padded(b"GIF87a"), ImageFormat::Gif),
            (padded(b"RIFF\x10\x00\x00\x00WEBPVP8 "), ImageFormat::Webp),
            (padded(b"II*\0"), ImageFormat::Tiff),
            (padded(b"MM\0*"), ImageFormat::Tiff),
            (padded(&[0xFF, 0x0A]), ImageFormat::Jxl),
            (
                padded(&[
                    0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
                ]),
                ImageFormat::Jxl,
            ),
        ];
        for (bytes, expected) in cases {
            assert_eq!(sniff(&bytes), Detected::Supported(expected), "{expected:?}");
        }
    }

    #[test]
    fn detects_bmp_only_with_valid_dib_header() {
        let mut bmp = padded(b"BM");
        bmp[14] = 40;
        assert_eq!(sniff(&bmp), Detected::Supported(ImageFormat::Bmp));

        let mut text = padded(b"BMW is a car brand");
        text[14] = b'a';
        assert_eq!(sniff(&text), Detected::Unknown);
    }

    #[test]
    fn detects_avif_and_heic_by_brand() {
        let mut avif = vec![0, 0, 0, 0x20];
        avif.extend_from_slice(b"ftypavif\0\0\0\0mif1miafMA1B");
        avif.resize(SNIFF_LEN, 0);
        assert_eq!(sniff(&avif), Detected::Supported(ImageFormat::Avif));

        let mut avif_compat = vec![0, 0, 0, 0x1C];
        avif_compat.extend_from_slice(b"ftypmif1\0\0\0\0mif1avifmiaf");
        avif_compat.resize(SNIFF_LEN, 0);
        assert_eq!(sniff(&avif_compat), Detected::Supported(ImageFormat::Avif));

        let mut heic = vec![0, 0, 0, 0x18];
        heic.extend_from_slice(b"ftypheic\0\0\0\0mif1heic");
        heic.resize(SNIFF_LEN, 0);
        assert_eq!(sniff(&heic), Detected::Unsupported("HEIC"));
    }

    #[test]
    fn recognizes_unsupported_and_unknown_content() {
        assert_eq!(sniff(&padded(b"%PDF-1.7")), Detected::Unsupported("PDF"));
        assert_eq!(
            sniff(&padded(b"  <?xml version")),
            Detected::Unsupported("SVG")
        );
        assert_eq!(sniff(b"hello world"), Detected::Unknown);
        assert_eq!(sniff(&[]), Detected::Unknown);
        assert_eq!(sniff(&[0xFF]), Detected::Unknown);
    }

    #[test]
    fn require_supported_reports_detected_name() {
        let error = Detected::Unsupported("HEIC")
            .require_supported()
            .unwrap_err();
        assert_eq!(error.code(), "unsupported_format");
        assert!(error.to_string().contains("HEIC"));
    }

    #[test]
    fn extension_helpers() {
        assert_eq!(
            format_from_extension(Path::new("a/b/photo.JPEG")),
            Some(ImageFormat::Jpeg)
        );
        assert!(has_image_extension(Path::new("x.webp")));
        assert!(!has_image_extension(Path::new("notes.txt")));
        assert!(!has_image_extension(Path::new("no_extension")));
    }
}
