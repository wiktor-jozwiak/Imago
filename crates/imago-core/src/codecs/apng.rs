//! Animated PNG (APNG) output.
//!
//! APNG needs one color type, bit depth and palette for all frames, so the
//! animation is collected first (bounded by `Limits::max_animation_pixels`):
//!
//! * ≤256 distinct colors in the whole animation → indexed, exact palette;
//! * lossy settings → one palette for all frames from libimagequant;
//! * otherwise → 8-bit RGBA.
//!
//! Every frame after the first stores only its changed rectangle. Two variants
//! are compressed with oxipng and the smaller one is kept: the rectangle as is
//! ("source" blending), or with unchanged pixels made transparent and blended
//! over the previous frame ("over" blending), which compresses much better
//! when only scattered pixels change.

use image::RgbaImage;
use imagequant::RGBA;
use oxipng::{BitDepth, ColorType, Options, RawImage, StripChunks};
use rgb::bytemuck;

use crate::cancel::CancelToken;
use crate::codecs::quantize::png_quality;
use crate::codecs::{
    AnimationFrame, AnimationInfo, EncodeOptions, changed_rect_raw, effort_index, encode_error,
};
use crate::errors::Result;
use crate::settings::{Effort, ImageFormat};

const FORMAT: ImageFormat = ImageFormat::Png;
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// How pixels are stored in every frame.
enum Mode {
    Indexed {
        palette: Vec<RGBA>,
        /// Palette entry that is fully transparent (needed for "over" blending).
        transparent: Option<u8>,
    },
    Rgba,
}

impl Mode {
    fn bytes_per_pixel(&self) -> usize {
        match self {
            Mode::Indexed { .. } => 1,
            Mode::Rgba => 4,
        }
    }

    /// Whether the pixel at `offset` of `canvas` is fully opaque.
    fn opaque(&self, canvas: &[u8], offset: usize) -> bool {
        match self {
            Mode::Indexed { palette, .. } => palette
                .get(usize::from(canvas[offset]))
                .is_some_and(|color| color.a == 255),
            Mode::Rgba => canvas[offset + 3] == 255,
        }
    }

    /// The byte pattern of a fully transparent pixel.
    fn transparent_pixel(&self) -> Option<Vec<u8>> {
        match self {
            Mode::Indexed { transparent, .. } => transparent.map(|index| vec![index]),
            Mode::Rgba => Some(vec![0, 0, 0, 0]),
        }
    }
}

/// One frame ready to be written.
struct EncodedFrame {
    rect: (u32, u32, u32, u32),
    blend_over: bool,
    delay_ms: u32,
    /// Compressed image data (the content of IDAT/fdAT chunks).
    data: Vec<u8>,
}

/// Encodes an animation as APNG.
pub(crate) fn encode_animation(
    info: AnimationInfo,
    frames: &mut dyn Iterator<Item = Result<AnimationFrame>>,
    options: EncodeOptions,
    cancel: &CancelToken,
) -> Result<Vec<u8>> {
    let mut collected: Vec<(RgbaImage, u32)> = Vec::new();
    for frame in frames {
        cancel.check()?;
        let frame = frame?;
        if frame.image.dimensions() != (info.width, info.height) {
            return Err(encode_error(
                FORMAT,
                "animation frame does not match the canvas size",
            ));
        }
        match collected.last_mut() {
            Some((previous, delay)) if *previous == frame.image => {
                *delay = delay.saturating_add(frame.delay_ms);
            }
            _ => collected.push((frame.image, frame.delay_ms)),
        }
    }
    if collected.is_empty() {
        return Err(encode_error(FORMAT, "the animation has no frames"));
    }

    let lossless = options.lossless || options.quality.get() >= 100;
    let delays: Vec<u32> = collected.iter().map(|(_, delay)| *delay).collect();
    let encode = |(mode, canvases): (Mode, Vec<Vec<u8>>)| -> Result<Vec<u8>> {
        let encoded = encode_frames(&mode, &canvases, &delays, info, options.effort, cancel)?;
        Ok(write_apng(&mode, info, &encoded))
    };
    if let Some(exact) = exact_palette(&collected) {
        return encode(exact);
    }
    if lossless {
        return encode(rgba_canvases(&collected));
    }
    let Some(quantized) = global_quantize(&collected, options)? else {
        return encode(rgba_canvases(&collected));
    };
    let quantized = encode(quantized)?;
    if options.effort == Effort::Fast {
        return Ok(quantized);
    }
    // Dithering noise can outweigh the palette on smooth content: keep the
    // smaller of the quantized and the exact animation.
    let exact = encode(rgba_canvases(&collected))?;
    Ok(if exact.len() < quantized.len() {
        exact
    } else {
        quantized
    })
}

/// Indexed canvases with an exact palette, if the animation has ≤256 colors.
fn exact_palette(frames: &[(RgbaImage, u32)]) -> Option<(Mode, Vec<Vec<u8>>)> {
    let mut lookup: std::collections::HashMap<[u8; 4], u8> = std::collections::HashMap::new();
    let mut palette: Vec<RGBA> = Vec::new();
    let mut canvases = Vec::with_capacity(frames.len());
    for (image, _) in frames {
        let mut indices = Vec::with_capacity(image.as_raw().len() / 4);
        for pixel in image.as_raw().as_chunks::<4>().0 {
            // All fully transparent pixels are the same color.
            let key = if pixel[3] == 0 { [0; 4] } else { *pixel };
            let index = match lookup.get(&key) {
                Some(index) => *index,
                None => {
                    let index = u8::try_from(palette.len()).ok()?;
                    palette.push(RGBA::new(key[0], key[1], key[2], key[3]));
                    lookup.insert(key, index);
                    index
                }
            };
            indices.push(index);
        }
        canvases.push(indices);
    }
    let transparent = transparent_entry(&mut palette);
    Some((
        Mode::Indexed {
            palette,
            transparent,
        },
        canvases,
    ))
}

/// Finds (or adds, when there is room) a fully transparent palette entry.
fn transparent_entry(palette: &mut Vec<RGBA>) -> Option<u8> {
    if let Some(index) = palette.iter().position(|color| color.a == 0) {
        return u8::try_from(index).ok();
    }
    let index = u8::try_from(palette.len()).ok()?;
    palette.push(RGBA::new(0, 0, 0, 0));
    Some(index)
}

/// One libimagequant palette for all frames. `None` if the quality target
/// cannot be reached or the frames did not end up with the same palette.
fn global_quantize(
    frames: &[(RgbaImage, u32)],
    options: EncodeOptions,
) -> Result<Option<(Mode, Vec<Vec<u8>>)>> {
    let fail =
        |error: imagequant::Error| encode_error(FORMAT, format!("quantization failed: {error}"));
    let quality = png_quality(options.quality.get());
    let mut attributes = imagequant::new();
    attributes
        .set_speed([8, 4, 1][effort_index(options.effort)])
        .map_err(fail)?;
    attributes
        .set_quality(quality.minimum.min(quality.target), quality.target.min(100))
        .map_err(fail)?;
    let (width, height) = frames[0].0.dimensions();
    let (width, height) = (width as usize, height as usize);

    let mut histogram = imagequant::Histogram::new(&attributes);
    for (image, _) in frames {
        let pixels: &[RGBA] = bytemuck::cast_slice(image.as_raw());
        let mut liq_image = attributes
            .new_image_borrowed(pixels, width, height, 0.0)
            .map_err(fail)?;
        histogram
            .add_image(&attributes, &mut liq_image)
            .map_err(fail)?;
    }
    let mut result = match histogram.quantize(&attributes) {
        Ok(result) => result,
        Err(imagequant::Error::QualityTooLow) => return Ok(None),
        Err(error) => return Err(fail(error)),
    };
    // Mild dithering: strong dithering makes static areas "shimmer" between frames.
    result.set_dithering_level(0.5).map_err(fail)?;

    let mut palette: Option<Vec<RGBA>> = None;
    let mut canvases = Vec::with_capacity(frames.len());
    for (image, _) in frames {
        let pixels: &[RGBA] = bytemuck::cast_slice(image.as_raw());
        let mut liq_image = attributes
            .new_image_borrowed(pixels, width, height, 0.0)
            .map_err(fail)?;
        let (frame_palette, indices) = result.remapped(&mut liq_image).map_err(fail)?;
        match &palette {
            None => palette = Some(frame_palette),
            Some(first) if *first != frame_palette => return Ok(None),
            Some(_) => {}
        }
        canvases.push(indices);
    }
    let Some(mut palette) = palette else {
        return Ok(None);
    };
    let transparent = transparent_entry(&mut palette);
    Ok(Some((
        Mode::Indexed {
            palette,
            transparent,
        },
        canvases,
    )))
}

fn rgba_canvases(frames: &[(RgbaImage, u32)]) -> (Mode, Vec<Vec<u8>>) {
    (
        Mode::Rgba,
        frames
            .iter()
            .map(|(image, _)| image.as_raw().clone())
            .collect(),
    )
}

/// Chooses the rectangle and blending of every frame and compresses it.
fn encode_frames(
    mode: &Mode,
    canvases: &[Vec<u8>],
    delays: &[u32],
    info: AnimationInfo,
    effort: Effort,
    cancel: &CancelToken,
) -> Result<Vec<EncodedFrame>> {
    let bpp = mode.bytes_per_pixel();
    let full = (0, 0, info.width, info.height);
    let mut encoded: Vec<EncodedFrame> = Vec::with_capacity(canvases.len());
    for (index, canvas) in canvases.iter().enumerate() {
        cancel.check()?;
        let delay_ms = delays.get(index).copied().unwrap_or(0);
        let Some(previous) = index.checked_sub(1).and_then(|i| canvases.get(i)) else {
            encoded.push(EncodedFrame {
                rect: full,
                blend_over: false,
                delay_ms,
                data: compress(mode, canvas, info.width, info.height, effort)?,
            });
            continue;
        };
        let Some(rect) = changed_rect_raw(previous, canvas, info.width, bpp) else {
            // Became identical after quantization: show the previous frame longer.
            if let Some(last) = encoded.last_mut() {
                last.delay_ms = last.delay_ms.saturating_add(delay_ms);
            }
            continue;
        };
        let source = crop(canvas, info.width, rect, bpp);
        let mut best = EncodedFrame {
            rect,
            blend_over: false,
            delay_ms,
            data: compress(mode, &source, rect.2, rect.3, effort)?,
        };
        if effort != Effort::Fast
            && let Some(over) = blended_region(mode, previous, canvas, info.width, rect)
        {
            let data = compress(mode, &over, rect.2, rect.3, effort)?;
            if data.len() < best.data.len() {
                best.data = data;
                best.blend_over = true;
            }
        }
        encoded.push(best);
    }
    Ok(encoded)
}

/// The changed rectangle with unchanged pixels made transparent, for
/// "over" blending. `None` when that would be wrong (a changed pixel is not
/// fully opaque, so blending would mix it with the previous frame) or
/// impossible (no transparent palette entry).
fn blended_region(
    mode: &Mode,
    previous: &[u8],
    current: &[u8],
    width: u32,
    (x, y, w, h): (u32, u32, u32, u32),
) -> Option<Vec<u8>> {
    let bpp = mode.bytes_per_pixel();
    let transparent = mode.transparent_pixel()?;
    let mut region = Vec::with_capacity(w as usize * h as usize * bpp);
    for row in y..y + h {
        for column in x..x + w {
            let offset = (row as usize * width as usize + column as usize) * bpp;
            let pixel = &current[offset..offset + bpp];
            if pixel == &previous[offset..offset + bpp] {
                region.extend_from_slice(&transparent);
            } else if mode.opaque(current, offset) {
                region.extend_from_slice(pixel);
            } else {
                return None;
            }
        }
    }
    Some(region)
}

fn crop(canvas: &[u8], width: u32, (x, y, w, h): (u32, u32, u32, u32), bpp: usize) -> Vec<u8> {
    let mut region = Vec::with_capacity(w as usize * h as usize * bpp);
    for row in y..y + h {
        let start = (row as usize * width as usize + x as usize) * bpp;
        region.extend_from_slice(&canvas[start..start + w as usize * bpp]);
    }
    region
}

/// Compresses one frame with oxipng, keeping the shared color type and bit
/// depth. Returns the concatenated IDAT payload.
fn compress(
    mode: &Mode,
    pixels: &[u8],
    width: u32,
    height: u32,
    effort: Effort,
) -> Result<Vec<u8>> {
    let color_type = match mode {
        Mode::Indexed { palette, .. } => ColorType::Indexed {
            palette: palette.clone(),
        },
        Mode::Rgba => ColorType::RGBA,
    };
    let raw = RawImage::new(width, height, color_type, BitDepth::Eight, pixels.to_vec())
        .map_err(|error| encode_error(FORMAT, error))?;
    let mut options = Options::from_preset([1, 2, 4][effort_index(effort)]);
    options.strip = StripChunks::All;
    options.interlace = Some(false);
    options.optimize_alpha = false;
    options.bit_depth_reduction = false;
    options.color_type_reduction = false;
    options.palette_reduction = false;
    options.grayscale_reduction = false;
    let png = raw
        .create_optimized_png(&options)
        .map_err(|error| encode_error(FORMAT, error))?;
    let expected_type = match mode {
        Mode::Indexed { .. } => 3,
        Mode::Rgba => 6,
    };
    let mut data = Vec::new();
    for (name, body) in png_chunks(&png)? {
        match &name {
            b"IHDR" if body.len() >= 10 && (body[8] != 8 || body[9] != expected_type) => {
                return Err(encode_error(
                    FORMAT,
                    "the PNG encoder changed the pixel format of a frame",
                ));
            }
            b"IDAT" => data.extend_from_slice(body),
            _ => {}
        }
    }
    Ok(data)
}

/// Splits a PNG file into `(chunk type, chunk data)` pairs.
fn png_chunks(png: &[u8]) -> Result<Vec<([u8; 4], &[u8])>> {
    let broken = || encode_error(FORMAT, "the PNG encoder produced an invalid file");
    let mut chunks = Vec::new();
    let mut rest = png.strip_prefix(&SIGNATURE[..]).ok_or_else(broken)?;
    while !rest.is_empty() {
        let (header, after) = rest.split_first_chunk::<8>().ok_or_else(broken)?;
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let name = [header[4], header[5], header[6], header[7]];
        if after.len() < length + 4 {
            return Err(broken());
        }
        chunks.push((name, &after[..length]));
        rest = &after[length + 4..];
    }
    Ok(chunks)
}

fn write_chunk(output: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    let length = u32::try_from(data.len()).unwrap_or(u32::MAX);
    output.extend_from_slice(&length.to_be_bytes());
    let start = output.len();
    output.extend_from_slice(name);
    output.extend_from_slice(data);
    let crc = crc32fast::hash(&output[start..]);
    output.extend_from_slice(&crc.to_be_bytes());
}

/// Frame delay as a fraction of a second that fits APNG's 16-bit fields.
fn delay_fraction(delay_ms: u32) -> (u16, u16) {
    match u16::try_from(delay_ms) {
        Ok(ms) => (ms, 1000),
        Err(_) => (u16::try_from(delay_ms / 1000).unwrap_or(u16::MAX), 1),
    }
}

fn write_apng(mode: &Mode, info: AnimationInfo, frames: &[EncodedFrame]) -> Vec<u8> {
    let payload: usize = frames.iter().map(|frame| frame.data.len() + 64).sum();
    let mut output = Vec::with_capacity(payload + 1024);
    output.extend_from_slice(&SIGNATURE);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&info.width.to_be_bytes());
    ihdr.extend_from_slice(&info.height.to_be_bytes());
    ihdr.push(8);
    ihdr.push(match mode {
        Mode::Indexed { .. } => 3,
        Mode::Rgba => 6,
    });
    ihdr.extend_from_slice(&[0, 0, 0]);
    write_chunk(&mut output, b"IHDR", &ihdr);

    if let Mode::Indexed { palette, .. } = mode {
        let rgb: Vec<u8> = palette.iter().flat_map(|c| [c.r, c.g, c.b]).collect();
        write_chunk(&mut output, b"PLTE", &rgb);
        let last_translucent = palette.iter().rposition(|c| c.a != 255);
        if let Some(last) = last_translucent {
            let alpha: Vec<u8> = palette[..=last].iter().map(|c| c.a).collect();
            write_chunk(&mut output, b"tRNS", &alpha);
        }
    }

    let frame_count = u32::try_from(frames.len()).unwrap_or(u32::MAX);
    let mut actl = Vec::with_capacity(8);
    actl.extend_from_slice(&frame_count.to_be_bytes());
    actl.extend_from_slice(&u32::from(info.loop_count).to_be_bytes());
    write_chunk(&mut output, b"acTL", &actl);

    let mut sequence: u32 = 0;
    for (index, frame) in frames.iter().enumerate() {
        let (x, y, width, height) = frame.rect;
        let (numerator, denominator) = delay_fraction(frame.delay_ms);
        let mut fctl = Vec::with_capacity(26);
        fctl.extend_from_slice(&sequence.to_be_bytes());
        fctl.extend_from_slice(&width.to_be_bytes());
        fctl.extend_from_slice(&height.to_be_bytes());
        fctl.extend_from_slice(&x.to_be_bytes());
        fctl.extend_from_slice(&y.to_be_bytes());
        fctl.extend_from_slice(&numerator.to_be_bytes());
        fctl.extend_from_slice(&denominator.to_be_bytes());
        fctl.push(0); // dispose: none
        fctl.push(u8::from(frame.blend_over)); // blend: source (0) or over (1)
        write_chunk(&mut output, b"fcTL", &fctl);
        sequence += 1;
        if index == 0 {
            // The first frame is also the default (static) image.
            write_chunk(&mut output, b"IDAT", &frame.data);
        } else {
            let mut fdat = Vec::with_capacity(frame.data.len() + 4);
            fdat.extend_from_slice(&sequence.to_be_bytes());
            fdat.extend_from_slice(&frame.data);
            write_chunk(&mut output, b"fdAT", &fdat);
            sequence += 1;
        }
    }
    write_chunk(&mut output, b"IEND", &[]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::png::PngDecoder;
    use image::{AnimationDecoder, Rgba};

    fn frames(images: Vec<RgbaImage>) -> Vec<Result<AnimationFrame>> {
        images
            .into_iter()
            .map(|image| {
                Ok(AnimationFrame {
                    image,
                    delay_ms: 80,
                })
            })
            .collect()
    }

    fn decode(bytes: &[u8]) -> Vec<RgbaImage> {
        let decoder = PngDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        decoder
            .apng()
            .unwrap()
            .into_frames()
            .map(|frame| frame.unwrap().into_buffer())
            .collect()
    }

    fn info() -> AnimationInfo {
        AnimationInfo {
            width: 40,
            height: 30,
            loop_count: 0,
        }
    }

    fn lossless() -> EncodeOptions {
        EncodeOptions {
            lossless: true,
            ..EncodeOptions::default()
        }
    }

    /// A moving square over a gradient: many colors, small changes.
    fn moving_square(count: u32) -> Vec<RgbaImage> {
        (0..count)
            .map(|index| {
                RgbaImage::from_fn(40, 30, |x, y| {
                    if (index * 3..index * 3 + 6).contains(&x) && (10..16).contains(&y) {
                        Rgba([255, 0, 0, 255])
                    } else {
                        Rgba([(x * 6) as u8, (y * 8) as u8, 90, 255])
                    }
                })
            })
            .collect()
    }

    #[test]
    fn round_trips_losslessly_with_many_colors() {
        let images = moving_square(5);
        let bytes = encode_animation(
            info(),
            &mut frames(images.clone()).into_iter(),
            lossless(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(decode(&bytes), images);
    }

    #[test]
    fn few_colors_use_an_indexed_palette_and_stay_exact() {
        let images: Vec<RgbaImage> = (0..4)
            .map(|index| {
                RgbaImage::from_fn(40, 30, |x, _| {
                    if x == index * 5 {
                        Rgba([0, 0, 0, 0])
                    } else {
                        Rgba([20, 200, 30, 255])
                    }
                })
            })
            .collect();
        let bytes = encode_animation(
            info(),
            &mut frames(images.clone()).into_iter(),
            lossless(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(bytes[8 + 8 + 9], 3, "IHDR color type is indexed");
        assert_eq!(decode(&bytes), images);
    }

    #[test]
    fn identical_frames_are_merged() {
        let mut images = moving_square(2);
        images.insert(1, images[0].clone());
        let bytes = encode_animation(
            info(),
            &mut frames(images).into_iter(),
            lossless(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(decode(&bytes).len(), 2);
    }

    /// Film-like frames: a gradient with grain, thousands of colors that
    /// compress badly as RGBA but quantize well.
    fn grainy(count: u32, width: u32, height: u32) -> Vec<RgbaImage> {
        (0..count)
            .map(|index| {
                RgbaImage::from_fn(width, height, |x, y| {
                    let hash = (x * 7919 + y * 104_729 + index * 31).wrapping_mul(2_654_435_761);
                    let grain = |shift: u32| ((hash >> shift) & 15) as u8;
                    Rgba([
                        (x * 200 / width) as u8 + grain(4),
                        (y * 200 / height) as u8 + grain(12),
                        (60 + index * 20) as u8 + grain(20),
                        255,
                    ])
                })
            })
            .collect()
    }

    #[test]
    fn lossy_settings_quantize_to_one_palette() {
        let info = AnimationInfo {
            width: 160,
            height: 120,
            loop_count: 0,
        };
        let images = grainy(4, 160, 120);
        let lossy = EncodeOptions {
            quality: crate::settings::Quality::new(70).unwrap(),
            ..EncodeOptions::default()
        };
        let exact = encode_animation(
            info,
            &mut frames(images.clone()).into_iter(),
            lossless(),
            &CancelToken::new(),
        )
        .unwrap();
        let smaller = encode_animation(
            info,
            &mut frames(images).into_iter(),
            lossy,
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(decode(&smaller).len(), 4);
        assert_eq!(smaller[8 + 8 + 9], 3, "IHDR color type is indexed");
        assert!(
            smaller.len() < exact.len(),
            "{} vs {}",
            smaller.len(),
            exact.len()
        );
    }
}
