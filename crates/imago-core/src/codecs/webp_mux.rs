//! Minimal WebP container (RIFF) handling for animations.
//!
//! libwebp encodes every frame as an ordinary still WebP; this module lifts
//! the frame bitstream (`ALPH` + `VP8 `/`VP8L` chunks) out of each still file
//! and wraps it in `ANMF` chunks behind a `VP8X` + `ANIM` header, following the
//! WebP container specification. It is container muxing only — no codec code.
//!
//! Frames are appended as they are produced, so an animation never has to be
//! held in memory as raw pixels.

use crate::errors::{ImagoError, Result};

/// Largest canvas side a WebP container can describe (24-bit field, minus one).
pub const MAX_CANVAS_SIDE: u32 = 1 << 24;
/// Largest frame duration in milliseconds (24-bit field).
pub const MAX_DURATION_MS: u32 = (1 << 24) - 1;

const FLAG_ALPHA: u8 = 0x10;
const FLAG_ANIMATION: u8 = 0x02;

/// A chunk of a RIFF container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk<'a> {
    /// Four-character code.
    pub fourcc: [u8; 4],
    /// Payload without header and padding.
    pub payload: &'a [u8],
}

/// Splits a WebP file into its top-level chunks.
pub fn parse_chunks(data: &[u8]) -> Result<Vec<Chunk<'_>>> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return Err(malformed("not a RIFF/WEBP file"));
    }
    let declared = read_u32(&data[4..8]) as usize;
    let end = declared.saturating_add(8).min(data.len());
    let mut chunks = Vec::new();
    let mut offset = 12;
    while offset + 8 <= end {
        let fourcc = [
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ];
        let size = read_u32(&data[offset + 4..offset + 8]) as usize;
        let start = offset + 8;
        let stop = start
            .checked_add(size)
            .filter(|stop| *stop <= end)
            .ok_or_else(|| malformed("chunk extends past the end of the file"))?;
        chunks.push(Chunk {
            fourcc,
            payload: &data[start..stop],
        });
        offset = stop + (size & 1);
    }
    Ok(chunks)
}

/// Builds an animated WebP file frame by frame.
#[derive(Debug)]
pub struct AnimationMuxer {
    canvas_width: u32,
    canvas_height: u32,
    loop_count: u16,
    background_bgra: [u8; 4],
    has_alpha: bool,
    frames: Vec<u8>,
    frame_count: usize,
}

/// Placement and timing of one frame on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePlacement {
    /// Left offset in pixels (must be even).
    pub x: u32,
    /// Top offset in pixels (must be even).
    pub y: u32,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Display time in milliseconds.
    pub duration_ms: u32,
    /// Alpha-blend the frame onto the canvas (`false` replaces the rectangle).
    pub blend: bool,
    /// Clear the frame rectangle to the background after display.
    pub dispose_to_background: bool,
}

impl AnimationMuxer {
    /// Starts an animation. `loop_count` 0 means "loop forever".
    pub fn new(canvas_width: u32, canvas_height: u32, loop_count: u16) -> Result<Self> {
        if canvas_width == 0
            || canvas_height == 0
            || canvas_width > MAX_CANVAS_SIDE
            || canvas_height > MAX_CANVAS_SIDE
        {
            return Err(malformed("invalid animation canvas size"));
        }
        Ok(AnimationMuxer {
            canvas_width,
            canvas_height,
            loop_count,
            background_bgra: [0, 0, 0, 0],
            has_alpha: false,
            frames: Vec::new(),
            frame_count: 0,
        })
    }

    /// Number of frames added so far.
    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// Appends a frame whose pixels are an encoded still WebP file.
    pub fn add_frame(&mut self, still_webp: &[u8], placement: FramePlacement) -> Result<()> {
        let FramePlacement {
            x,
            y,
            width,
            height,
            ..
        } = placement;
        if x % 2 != 0 || y % 2 != 0 {
            return Err(malformed("frame offsets must be even"));
        }
        if width == 0
            || height == 0
            || x.checked_add(width)
                .is_none_or(|right| right > self.canvas_width)
            || y.checked_add(height)
                .is_none_or(|bottom| bottom > self.canvas_height)
        {
            return Err(malformed("frame does not fit on the canvas"));
        }

        let mut alpha_chunk = None;
        let mut image_chunk = None;
        for chunk in parse_chunks(still_webp)? {
            match &chunk.fourcc {
                b"ALPH" => alpha_chunk = Some(chunk),
                b"VP8 " | b"VP8L" => image_chunk = Some(chunk),
                _ => {}
            }
        }
        let image_chunk = image_chunk.ok_or_else(|| malformed("frame has no image data"))?;
        let (frame_width, frame_height, lossless_alpha) = bitstream_info(&image_chunk)?;
        if (frame_width, frame_height) != (width, height) {
            return Err(malformed(
                "frame bitstream size does not match its placement",
            ));
        }
        self.has_alpha |= alpha_chunk.is_some() || lossless_alpha;

        let mut payload = Vec::with_capacity(16 + image_chunk.payload.len() + 16);
        write_u24(&mut payload, x / 2);
        write_u24(&mut payload, y / 2);
        write_u24(&mut payload, width - 1);
        write_u24(&mut payload, height - 1);
        write_u24(&mut payload, placement.duration_ms.min(MAX_DURATION_MS));
        let flags = (u8::from(!placement.blend) << 1) | u8::from(placement.dispose_to_background);
        payload.push(flags);
        if let Some(alpha) = alpha_chunk {
            write_chunk(&mut payload, &alpha.fourcc, alpha.payload);
        }
        write_chunk(&mut payload, &image_chunk.fourcc, image_chunk.payload);
        write_chunk(&mut self.frames, b"ANMF", &payload);
        self.frame_count += 1;
        Ok(())
    }

    /// Writes the complete file.
    pub fn finish(self) -> Result<Vec<u8>> {
        if self.frame_count == 0 {
            return Err(malformed("an animation needs at least one frame"));
        }
        let mut body = Vec::with_capacity(self.frames.len() + 64);
        body.extend_from_slice(b"WEBP");

        let mut vp8x = Vec::with_capacity(10);
        let mut flags = FLAG_ANIMATION;
        if self.has_alpha {
            flags |= FLAG_ALPHA;
        }
        vp8x.push(flags);
        vp8x.extend_from_slice(&[0, 0, 0]);
        write_u24(&mut vp8x, self.canvas_width - 1);
        write_u24(&mut vp8x, self.canvas_height - 1);
        write_chunk(&mut body, b"VP8X", &vp8x);

        let mut anim = Vec::with_capacity(6);
        anim.extend_from_slice(&self.background_bgra);
        anim.extend_from_slice(&self.loop_count.to_le_bytes());
        write_chunk(&mut body, b"ANIM", &anim);

        body.extend_from_slice(&self.frames);

        let size =
            u32::try_from(body.len()).map_err(|_| malformed("animation is larger than 4 GiB"))?;
        let mut file = Vec::with_capacity(body.len() + 8);
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&size.to_le_bytes());
        file.extend_from_slice(&body);
        Ok(file)
    }
}

/// Width, height and "alpha used" flag of a `VP8 ` or `VP8L` bitstream.
fn bitstream_info(chunk: &Chunk<'_>) -> Result<(u32, u32, bool)> {
    let data = chunk.payload;
    match &chunk.fourcc {
        b"VP8 " => {
            // 3-byte frame tag, start code 9D 01 2A, then 14-bit width and height.
            if data.len() < 10 || data[3..6] != [0x9D, 0x01, 0x2A] {
                return Err(malformed("invalid VP8 bitstream"));
            }
            let width = u32::from(u16::from_le_bytes([data[6], data[7]]) & 0x3FFF);
            let height = u32::from(u16::from_le_bytes([data[8], data[9]]) & 0x3FFF);
            Ok((width, height, false))
        }
        b"VP8L" => {
            // Signature 0x2F, then 14 bits width-1, 14 bits height-1, 1 bit alpha.
            if data.len() < 5 || data[0] != 0x2F {
                return Err(malformed("invalid VP8L bitstream"));
            }
            let bits = read_u32(&data[1..5]);
            let width = (bits & 0x3FFF) + 1;
            let height = ((bits >> 14) & 0x3FFF) + 1;
            let alpha = (bits >> 28) & 1 == 1;
            Ok((width, height, alpha))
        }
        _ => Err(malformed("unexpected bitstream chunk")),
    }
}

fn write_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(fourcc);
    // Chunk payloads here are bounded by the 4 GiB file check in `finish`.
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
}

fn write_u24(out: &mut Vec<u8>, value: u32) {
    let bytes = value.to_le_bytes();
    out.extend_from_slice(&bytes[..3]);
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn malformed(message: &str) -> ImagoError {
    ImagoError::Encode {
        format: "WebP".into(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_still(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let pixels: Vec<u8> = rgba
            .iter()
            .copied()
            .cycle()
            .take((width * height * 4) as usize)
            .collect();
        let encoder = webp::Encoder::from_rgba(&pixels, width, height);
        let mut config = webp::WebPConfig::new().unwrap();
        config.lossless = 1;
        encoder.encode_advanced(&config).unwrap().to_vec()
    }

    #[test]
    fn parses_chunks_of_a_still_file() {
        let still = tiny_still(4, 2, [10, 20, 30, 255]);
        let chunks = parse_chunks(&still).unwrap();
        assert!(chunks.iter().any(|c| &c.fourcc == b"VP8L"));
    }

    #[test]
    fn rejects_truncated_files() {
        let still = tiny_still(4, 2, [10, 20, 30, 255]);
        assert!(parse_chunks(&still[..still.len() - 3]).is_err());
        assert!(parse_chunks(b"RIFF").is_err());
    }

    #[test]
    fn builds_a_decodable_animation() {
        let mut muxer = AnimationMuxer::new(8, 8, 0).unwrap();
        let full = FramePlacement {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
            duration_ms: 100,
            blend: false,
            dispose_to_background: false,
        };
        muxer
            .add_frame(&tiny_still(8, 8, [255, 0, 0, 255]), full)
            .unwrap();
        let patch = FramePlacement {
            x: 2,
            y: 4,
            width: 4,
            height: 2,
            duration_ms: 250,
            ..full
        };
        muxer
            .add_frame(&tiny_still(4, 2, [0, 0, 255, 128]), patch)
            .unwrap();
        let file = muxer.finish().unwrap();

        use image::AnimationDecoder;
        let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&file)).unwrap();
        assert!(decoder.has_animation());
        let frames: Vec<_> = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 2);
        let second = frames[1].buffer();
        assert_eq!(second.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(second.get_pixel(3, 4).0, [0, 0, 255, 128]);
        let (numerator, denominator) = frames[1].delay().numer_denom_ms();
        assert_eq!(numerator / denominator, 250);
    }

    #[test]
    fn rejects_bad_placements() {
        let still = tiny_still(4, 4, [1, 2, 3, 255]);
        let mut muxer = AnimationMuxer::new(8, 8, 0).unwrap();
        let odd = FramePlacement {
            x: 1,
            y: 0,
            width: 4,
            height: 4,
            duration_ms: 10,
            blend: false,
            dispose_to_background: false,
        };
        assert!(muxer.add_frame(&still, odd).is_err());
        let outside = FramePlacement { x: 6, ..odd };
        assert!(muxer.add_frame(&still, outside).is_err());
        let wrong_size = FramePlacement {
            x: 0,
            width: 5,
            ..odd
        };
        assert!(muxer.add_frame(&still, wrong_size).is_err());
        assert!(AnimationMuxer::new(8, 8, 0).unwrap().finish().is_err());
    }
}
