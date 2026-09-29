//! Metadata layer.
//!
//! * lossless metadata removal at the container level (JPEG, PNG, WebP): the
//!   compressed pixel data is copied byte for byte, only metadata blocks go;
//! * EXIF helpers (orientation reset after the pixels were rotated).

use img_parts::jpeg::Jpeg;
use img_parts::png::Png;
use img_parts::webp::WebP;
use img_parts::{Bytes, ImageEXIF, ImageICC};

use crate::errors::{ImagoError, Result};
use crate::settings::ImageFormat;

/// JPEG APPn markers.
const APP0: u8 = 0xE0;
const APP2: u8 = 0xE2;
const APP14: u8 = 0xEE;
const COM: u8 = 0xFE;
const ICC_SIGNATURE: &[u8] = b"ICC_PROFILE\0";

/// Removes EXIF, XMP, IPTC, comments and similar blocks without touching the
/// compressed pixels. The ICC profile is kept when `keep_icc` is set.
///
/// Returns `Ok(None)` for formats where Imago cannot do this losslessly.
pub fn strip_container_metadata(
    bytes: &[u8],
    format: ImageFormat,
    keep_icc: bool,
) -> Result<Option<Vec<u8>>> {
    let data = Bytes::copy_from_slice(bytes);
    let stripped = match format {
        ImageFormat::Jpeg => {
            let mut jpeg = Jpeg::from_bytes(data).map_err(container_error(format))?;
            jpeg.segments_mut().retain(|segment| {
                let marker = segment.marker();
                match marker {
                    // JFIF header and the Adobe marker (it defines the color transform).
                    APP0 | APP14 => true,
                    APP2 => keep_icc && segment.contents().starts_with(ICC_SIGNATURE),
                    0xE1..=0xEF | COM => false,
                    _ => true,
                }
            });
            let mut out = Vec::new();
            jpeg.encoder().write_to(&mut out).map_err(rewrite_error)?;
            out
        }
        ImageFormat::Png => {
            let mut png = Png::from_bytes(data).map_err(container_error(format))?;
            png.chunks_mut().retain(|chunk| {
                !matches!(
                    &chunk.kind(),
                    b"tEXt" | b"zTXt" | b"iTXt" | b"eXIf" | b"tIME"
                ) && (keep_icc || &chunk.kind() != b"iCCP")
            });
            let mut out = Vec::new();
            png.encoder().write_to(&mut out).map_err(rewrite_error)?;
            out
        }
        ImageFormat::Webp => {
            let mut webp = WebP::from_bytes(data).map_err(container_error(format))?;
            webp.remove_chunks_by_id(img_parts::webp::CHUNK_XMP);
            if !keep_icc {
                webp.set_icc_profile(None);
            }
            // Also recomputes the VP8X feature flags.
            webp.set_exif(None);
            let mut out = Vec::new();
            webp.encoder().write_to(&mut out).map_err(rewrite_error)?;
            out
        }
        _ => return Ok(None),
    };
    Ok(Some(stripped))
}

fn rewrite_error(error: std::io::Error) -> ImagoError {
    ImagoError::Internal(format!("cannot rewrite container: {error}"))
}

fn container_error(format: ImageFormat) -> impl Fn(img_parts::Error) -> ImagoError {
    move |error| ImagoError::Decode {
        format: format.display_name().into(),
        message: format!("invalid container: {error}"),
    }
}

/// Sets the EXIF orientation tag (0x0112 in IFD0) to 1 ("normal").
///
/// Needed when EXIF is kept but the pixels were already rotated, otherwise
/// viewers would rotate the image a second time. Returns whether the tag was found.
pub fn reset_exif_orientation(exif: &mut [u8]) -> bool {
    let exif = strip_exif_prefix_mut(exif);
    let Some(little_endian) = byte_order(exif) else {
        return false;
    };
    let read_u16 = |data: &[u8], at: usize| -> Option<u16> {
        let bytes = data.get(at..at + 2)?;
        Some(if little_endian {
            u16::from_le_bytes([bytes[0], bytes[1]])
        } else {
            u16::from_be_bytes([bytes[0], bytes[1]])
        })
    };
    let read_u32 = |data: &[u8], at: usize| -> Option<u32> {
        let bytes = data.get(at..at + 4)?;
        Some(if little_endian {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        })
    };
    let Some(ifd) = read_u32(exif, 4).map(|offset| offset as usize) else {
        return false;
    };
    let Some(count) = read_u16(exif, ifd) else {
        return false;
    };
    for index in 0..usize::from(count) {
        let entry = ifd + 2 + index * 12;
        let (Some(tag), Some(kind)) = (read_u16(exif, entry), read_u16(exif, entry + 2)) else {
            return false;
        };
        // Orientation: tag 0x0112, type SHORT (3), value stored inline.
        if tag == 0x0112 && kind == 3 {
            let value = if little_endian {
                1u16.to_le_bytes()
            } else {
                1u16.to_be_bytes()
            };
            if let Some(slot) = exif.get_mut(entry + 8..entry + 10) {
                slot.copy_from_slice(&value);
                return true;
            }
            return false;
        }
    }
    false
}

/// Reads the EXIF orientation tag (1..=8), if present.
pub fn exif_orientation(exif: &[u8]) -> Option<u8> {
    image::metadata::Orientation::from_exif_chunk(exif)
        .map(crate::image_engine::probe::orientation_to_exif)
}

fn strip_exif_prefix_mut(exif: &mut [u8]) -> &mut [u8] {
    if exif.starts_with(b"Exif\0\0") {
        &mut exif[6..]
    } else {
        exif
    }
}

fn byte_order(tiff: &[u8]) -> Option<bool> {
    match tiff.get(0..4)? {
        [b'I', b'I', 42, 0] => Some(true),
        [b'M', b'M', 0, 42] => Some(false),
        _ => None,
    }
}

/// Builds a minimal EXIF block containing only an orientation tag (for tests
/// and for synthesizing orientation-only metadata).
pub fn orientation_only_exif(orientation: u8) -> Vec<u8> {
    let mut exif = Vec::with_capacity(26);
    exif.extend_from_slice(b"II");
    exif.extend_from_slice(&42u16.to_le_bytes());
    exif.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
    exif.extend_from_slice(&1u16.to_le_bytes()); // one entry
    exif.extend_from_slice(&0x0112u16.to_le_bytes());
    exif.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    exif.extend_from_slice(&1u32.to_le_bytes()); // count
    exif.extend_from_slice(&u16::from(orientation).to_le_bytes());
    exif.extend_from_slice(&[0, 0]); // padding of the value field
    exif.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    exif
}

/// Whether a JPEG, PNG or WebP file contains an EXIF block.
pub fn contains_exif(bytes: &[u8], format: ImageFormat) -> bool {
    let data = Bytes::copy_from_slice(bytes);
    match format {
        ImageFormat::Jpeg => Jpeg::from_bytes(data)
            .ok()
            .and_then(|jpeg| jpeg.exif())
            .is_some(),
        ImageFormat::Png => Png::from_bytes(data)
            .ok()
            .and_then(|png| png.exif())
            .is_some(),
        ImageFormat::Webp => WebP::from_bytes(data)
            .ok()
            .and_then(|webp| webp.exif())
            .is_some(),
        _ => false,
    }
}

/// Reads the embedded ICC profile from JPEG, PNG or WebP containers.
pub fn container_icc(bytes: &[u8], format: ImageFormat) -> Option<Vec<u8>> {
    let data = Bytes::copy_from_slice(bytes);
    let icc = match format {
        ImageFormat::Jpeg => Jpeg::from_bytes(data).ok()?.icc_profile(),
        ImageFormat::Png => Png::from_bytes(data).ok()?.icc_profile(),
        ImageFormat::Webp => WebP::from_bytes(data).ok()?.icc_profile(),
        _ => None,
    };
    icc.map(|bytes| bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_can_be_read_and_reset() {
        let mut exif = orientation_only_exif(6);
        assert_eq!(exif_orientation(&exif), Some(6));
        assert!(reset_exif_orientation(&mut exif));
        assert_eq!(exif_orientation(&exif), Some(1));
    }

    #[test]
    fn reset_handles_prefix_and_garbage() {
        let mut prefixed = b"Exif\0\0".to_vec();
        prefixed.extend_from_slice(&orientation_only_exif(3));
        assert!(reset_exif_orientation(&mut prefixed));
        assert_eq!(exif_orientation(&prefixed[6..]), Some(1));

        let mut garbage = vec![1, 2, 3];
        assert!(!reset_exif_orientation(&mut garbage));
        let mut truncated = orientation_only_exif(6);
        truncated.truncate(12);
        assert!(!reset_exif_orientation(&mut truncated));
    }

    #[test]
    fn unsupported_formats_are_not_stripped() {
        assert!(
            strip_container_metadata(b"GIF89a", ImageFormat::Gif, true)
                .unwrap()
                .is_none()
        );
    }
}
