//! Large-image mode: JPEG and PNG processed row by row.
//!
//! Images above the pixel limit (or too big for the memory budget) would need
//! gigabytes as a decoded buffer. Here pixels flow through a pipeline of rows
//! instead: decoder → optional sRGB conversion → optional area-averaging
//! downscale → optional flattening of transparency → encoder. Memory depends
//! on the image *width*, not its area, so a 30 000 × 20 000 JPEG needs a few
//! megabytes of working memory instead of 1.8 GB.
//!
//! Only formats that can be read and written as a stream take part: JPEG
//! (baseline output, standard Huffman tables — optimizing them would need the
//! whole image) and non-interlaced PNG. Rotation needs the whole image, so the
//! EXIF orientation is kept as a tag instead of being applied.

use std::io::Cursor;

use moxcms::{ColorProfile, Layout, TransformOptions};

use crate::cancel::CancelToken;
use crate::codecs::encode_error;
use crate::errors::{ImagoError, Result};
use crate::image_engine::color::{IccKind, classify_rgb_icc};
use crate::settings::ImageFormat;

/// Hard upper bound for the large-image mode: JPEG's own maximum side.
pub const MAX_STREAM_SIDE: u32 = 65_535;

/// Whether a format can be read row by row.
pub fn can_stream_input(format: ImageFormat) -> bool {
    matches!(format, ImageFormat::Jpeg | ImageFormat::Png)
}

/// Whether a format can be written row by row.
pub fn can_stream_output(format: ImageFormat) -> bool {
    matches!(format, ImageFormat::Jpeg | ImageFormat::Png)
}

/// What the streaming job should do.
#[derive(Debug, Clone)]
pub struct StreamPlan {
    /// Output format (JPEG or PNG).
    pub format: ImageFormat,
    /// Output size; equal to the input size when not resizing.
    pub output_size: (u32, u32),
    /// JPEG quality.
    pub quality: u8,
    /// Convert pixels to sRGB (the profile is not kept).
    pub convert_to_srgb: bool,
    /// Embed the source ICC profile.
    pub keep_icc: bool,
    /// EXIF block to write (already filtered by the metadata settings).
    pub exif: Option<Vec<u8>>,
    /// Composite transparency onto white (the output has no alpha channel).
    pub flatten: bool,
}

/// Header facts of a streamable source.
#[derive(Debug, Clone)]
pub struct StreamSource {
    /// Pixel size.
    pub size: (u32, u32),
    /// 3 (RGB) or 4 (RGBA) channels after normalization.
    pub channels: usize,
    /// Embedded ICC profile.
    pub icc: Option<Vec<u8>>,
    /// EXIF block (TIFF structure).
    pub exif: Option<Vec<u8>>,
}

/// A row decoder.
trait RowReader {
    fn read_row(&mut self, row: &mut [u8]) -> Result<()>;
}

// ------------------------------------------------------------------ input

const EXIF_PREFIX: &[u8] = b"Exif\0\0";
const ICC_PREFIX: &[u8] = b"ICC_PROFILE\0";

/// Reads the header of a streamable file (no pixels are decoded).
pub fn inspect(bytes: &[u8], format: ImageFormat) -> Result<StreamSource> {
    match format {
        ImageFormat::Jpeg => inspect_jpeg(bytes),
        ImageFormat::Png => inspect_png(bytes),
        other => Err(not_streamable(other)),
    }
}

fn not_streamable(format: ImageFormat) -> ImagoError {
    ImagoError::InvalidSettings(format!(
        "the large-image mode reads and writes only JPEG and PNG, not {format}"
    ))
}

fn jpeg_error(error: impl std::fmt::Display) -> ImagoError {
    ImagoError::Decode {
        format: ImageFormat::Jpeg.display_name().into(),
        message: error.to_string(),
    }
}

fn png_error(error: impl std::fmt::Display) -> ImagoError {
    ImagoError::Decode {
        format: ImageFormat::Png.display_name().into(),
        message: error.to_string(),
    }
}

fn inspect_jpeg(bytes: &[u8]) -> Result<StreamSource> {
    use mozjpeg::{Decompress, Marker};
    let decompress = Decompress::with_markers(&[Marker::APP(1), Marker::APP(2)])
        .from_mem(bytes)
        .map_err(jpeg_error)?;
    if !matches!(
        decompress.color_space(),
        mozjpeg::ColorSpace::JCS_YCbCr
            | mozjpeg::ColorSpace::JCS_RGB
            | mozjpeg::ColorSpace::JCS_GRAYSCALE
    ) {
        return Err(jpeg_error(
            "CMYK JPEGs cannot be processed in the large-image mode",
        ));
    }
    let mut exif = None;
    let mut icc_parts: Vec<(u8, Vec<u8>)> = Vec::new();
    for marker in decompress.markers() {
        match marker.marker {
            Marker::APP(1) if exif.is_none() => {
                if let Some(body) = marker.data.strip_prefix(EXIF_PREFIX) {
                    exif = Some(body.to_vec());
                }
            }
            Marker::APP(2) => {
                if let Some(body) = marker.data.strip_prefix(ICC_PREFIX)
                    && body.len() > 2
                {
                    icc_parts.push((body[0], body[2..].to_vec()));
                }
            }
            _ => {}
        }
    }
    icc_parts.sort_by_key(|(sequence, _)| *sequence);
    let icc: Vec<u8> = icc_parts.into_iter().flat_map(|(_, part)| part).collect();
    Ok(StreamSource {
        size: (decompress.width() as u32, decompress.height() as u32),
        channels: 3,
        icc: (!icc.is_empty()).then_some(icc),
        exif,
    })
}

struct JpegRows<'a> {
    started: mozjpeg::decompress::DecompressStarted<&'a [u8]>,
}

impl RowReader for JpegRows<'_> {
    fn read_row(&mut self, row: &mut [u8]) -> Result<()> {
        let read = self
            .started
            .read_scanlines_into::<[u8; 3]>(bytemuck_rows(row))
            .map_err(jpeg_error)?;
        if read.is_empty() {
            return Err(jpeg_error("the file ended before the last row"));
        }
        Ok(())
    }
}

/// Views a byte row as RGB triples for mozjpeg.
fn bytemuck_rows(row: &mut [u8]) -> &mut [[u8; 3]] {
    row.as_chunks_mut::<3>().0
}

fn inspect_png(bytes: &[u8]) -> Result<StreamSource> {
    let reader = png_reader(bytes)?;
    let info = reader.info();
    if info.interlaced {
        return Err(png_error(
            "interlaced PNGs cannot be processed in the large-image mode",
        ));
    }
    let (color, _) = reader.output_color_type();
    Ok(StreamSource {
        size: (info.width, info.height),
        channels: if color_has_alpha(color) { 4 } else { 3 },
        icc: info.icc_profile.as_ref().map(|icc| icc.to_vec()),
        exif: info.exif_metadata.as_ref().map(|exif| exif.to_vec()),
    })
}

fn color_has_alpha(color: png::ColorType) -> bool {
    matches!(color, png::ColorType::Rgba | png::ColorType::GrayscaleAlpha)
}

fn png_reader(bytes: &[u8]) -> Result<png::Reader<Cursor<&[u8]>>> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    // Rows are read one at a time, so the decoder's whole-image limit does not apply.
    decoder.set_limits(png::Limits { bytes: usize::MAX });
    decoder.read_info().map_err(png_error)
}

struct PngRows<'a> {
    reader: png::Reader<Cursor<&'a [u8]>>,
    color: png::ColorType,
    channels: usize,
}

impl RowReader for PngRows<'_> {
    fn read_row(&mut self, row: &mut [u8]) -> Result<()> {
        let decoded = self
            .reader
            .next_row()
            .map_err(png_error)?
            .ok_or_else(|| png_error("the file ended before the last row"))?;
        let data = decoded.data();
        match (self.color, self.channels) {
            (png::ColorType::Rgb, 3) | (png::ColorType::Rgba, 4) => row.copy_from_slice(data),
            (png::ColorType::Grayscale, 3) => {
                for (out, &gray) in row.as_chunks_mut::<3>().0.iter_mut().zip(data) {
                    *out = [gray, gray, gray];
                }
            }
            (png::ColorType::GrayscaleAlpha, 4) => {
                for (out, pair) in row
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(data.as_chunks::<2>().0)
                {
                    *out = [pair[0], pair[0], pair[0], pair[1]];
                }
            }
            _ => return Err(png_error("unexpected pixel layout")),
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- resizing

/// Streaming area-average downscaler: every output pixel is the mean of the
/// input area it covers (exact for any ratio ≥ 1). Works on premultiplied
/// values so transparent pixels do not darken edges.
struct Downscaler {
    in_height: usize,
    out_width: usize,
    out_height: usize,
    channels: usize,
    /// For every output column: `(first input column, weights)`.
    columns: Vec<(usize, Vec<f32>)>,
    /// Horizontally resampled current input row.
    scratch: Vec<f32>,
    /// Accumulators of the output row being built and of the next one.
    current: Vec<f32>,
    next: Vec<f32>,
    /// Index of the output row in `current`.
    out_row: usize,
    /// Input rows consumed.
    in_row: usize,
}

impl Downscaler {
    fn new(input: (u32, u32), output: (u32, u32), channels: usize) -> Self {
        let (in_width, in_height) = (input.0 as usize, input.1 as usize);
        let (out_width, out_height) = (output.0 as usize, output.1 as usize);
        let scale = in_width as f64 / out_width as f64;
        let columns = (0..out_width)
            .map(|x| {
                let start = x as f64 * scale;
                let end = start + scale;
                let first = start.floor() as usize;
                let last = (end.ceil() as usize).min(in_width);
                let weights = (first..last)
                    .map(|column| {
                        let overlap =
                            (end.min(column as f64 + 1.0) - start.max(column as f64)).max(0.0);
                        (overlap / scale) as f32
                    })
                    .collect();
                (first, weights)
            })
            .collect();
        Downscaler {
            in_height,
            out_width,
            out_height,
            channels,
            columns,
            scratch: vec![0.0; out_width * channels],
            current: vec![0.0; out_width * channels],
            next: vec![0.0; out_width * channels],
            out_row: 0,
            in_row: 0,
        }
    }

    /// Feeds one input row; calls `emit` for every finished output row.
    fn push(&mut self, row: &[u8], emit: &mut dyn FnMut(&[u8]) -> Result<()>) -> Result<()> {
        let channels = self.channels;
        let alpha = channels == 4;
        // Horizontal pass.
        for (x, (first, weights)) in self.columns.iter().enumerate() {
            let out = &mut self.scratch[x * channels..(x + 1) * channels];
            out.fill(0.0);
            for (offset, weight) in weights.iter().enumerate() {
                let pixel = &row[(first + offset) * channels..(first + offset + 1) * channels];
                let a = if alpha {
                    f32::from(pixel[3]) / 255.0
                } else {
                    1.0
                };
                for channel in 0..channels {
                    let value = f32::from(pixel[channel]);
                    out[channel] += weight
                        * if alpha && channel < 3 {
                            value * a
                        } else {
                            value
                        };
                }
            }
        }
        // Vertical pass: this input row covers [in_row, in_row + 1).
        let scale = self.in_height as f64 / self.out_height as f64;
        let top = self.in_row as f64;
        let bottom = top + 1.0;
        let boundary = (self.out_row + 1) as f64 * scale;
        let (into_current, into_next) = if bottom <= boundary {
            ((1.0 / scale) as f32, 0.0)
        } else {
            let current_part = ((boundary - top).max(0.0) / scale) as f32;
            (current_part, ((bottom - boundary) / scale) as f32)
        };
        for (index, value) in self.scratch.iter().enumerate() {
            self.current[index] += value * into_current;
            self.next[index] += value * into_next;
        }
        self.in_row += 1;

        let finished = bottom >= boundary || self.in_row == self.in_height;
        if finished && self.out_row < self.out_height {
            let mut pixels = vec![0u8; self.out_width * channels];
            for (x, out) in pixels.chunks_exact_mut(channels).enumerate() {
                let sums = &self.current[x * channels..(x + 1) * channels];
                let a = if alpha { sums[3] / 255.0 } else { 1.0 };
                for channel in 0..channels {
                    let value = if alpha && channel < 3 {
                        if a > 0.0 { sums[channel] / a } else { 0.0 }
                    } else {
                        sums[channel]
                    };
                    out[channel] = value.round().clamp(0.0, 255.0) as u8;
                }
            }
            emit(&pixels)?;
            self.out_row += 1;
            std::mem::swap(&mut self.current, &mut self.next);
            self.next.fill(0.0);
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ output

/// A `Write` target the PNG stream writer can own while we keep access to it.
#[derive(Clone, Default)]
struct SharedBuffer(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

impl std::io::Write for SharedBuffer {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

enum RowWriter {
    Jpeg(Box<mozjpeg::compress::CompressStarted<Vec<u8>>>),
    Png(Box<png::StreamWriter<'static, SharedBuffer>>, SharedBuffer),
}

impl RowWriter {
    fn new(plan: &StreamPlan, channels: usize, icc: Option<&[u8]>) -> Result<Self> {
        let (width, height) = plan.output_size;
        match plan.format {
            ImageFormat::Jpeg => {
                let mut compress = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
                compress.set_size(width as usize, height as usize);
                // Baseline with standard tables: anything else buffers the whole image.
                compress.set_fastest_defaults();
                compress.set_quality(f32::from(plan.quality));
                compress.set_optimize_coding(false);
                let mut started = compress
                    .start_compress(Vec::new())
                    .map_err(|e| encode_error(ImageFormat::Jpeg, e))?;
                if let Some(exif) = &plan.exif
                    && exif.len() + EXIF_PREFIX.len() <= 65_533
                {
                    let mut segment = EXIF_PREFIX.to_vec();
                    segment.extend_from_slice(exif);
                    started.write_marker(mozjpeg::Marker::APP(1), &segment);
                }
                if let Some(icc) = icc {
                    started.write_icc_profile(icc);
                }
                Ok(RowWriter::Jpeg(Box::new(started)))
            }
            ImageFormat::Png => {
                let mut info = png::Info::with_size(width, height);
                info.color_type = if channels == 4 {
                    png::ColorType::Rgba
                } else {
                    png::ColorType::Rgb
                };
                info.bit_depth = png::BitDepth::Eight;
                info.icc_profile = icc.map(|icc| std::borrow::Cow::Owned(icc.to_vec()));
                info.exif_metadata = plan
                    .exif
                    .as_ref()
                    .map(|exif| std::borrow::Cow::Owned(exif.clone()));
                let buffer = SharedBuffer::default();
                let mut encoder = png::Encoder::with_info(buffer.clone(), info)
                    .map_err(|e| encode_error(ImageFormat::Png, e))?;
                encoder.set_compression(png::Compression::High);
                let writer = encoder
                    .write_header()
                    .map_err(|e| encode_error(ImageFormat::Png, e))?;
                let stream = writer
                    .into_stream_writer()
                    .map_err(|e| encode_error(ImageFormat::Png, e))?;
                Ok(RowWriter::Png(Box::new(stream), buffer))
            }
            other => Err(not_streamable(other)),
        }
    }

    fn write_row(&mut self, row: &[u8]) -> Result<()> {
        match self {
            RowWriter::Jpeg(started) => started
                .write_scanlines(row)
                .map_err(|e| encode_error(ImageFormat::Jpeg, e)),
            RowWriter::Png(stream, _) => {
                use std::io::Write;
                stream
                    .write_all(row)
                    .map_err(|e| encode_error(ImageFormat::Png, e))
            }
        }
    }

    fn finish(self) -> Result<Vec<u8>> {
        match self {
            RowWriter::Jpeg(started) => started
                .finish()
                .map_err(|e| encode_error(ImageFormat::Jpeg, e)),
            RowWriter::Png(stream, buffer) => {
                // Finishing the stream writes the IEND chunk into the buffer.
                stream
                    .finish()
                    .map_err(|e| encode_error(ImageFormat::Png, e))?;
                Ok(buffer.0.take())
            }
        }
    }
}

// ----------------------------------------------------------------- the run

/// Converts `bytes` according to `plan`, one row at a time.
pub fn run(
    bytes: &[u8],
    format: ImageFormat,
    source: &StreamSource,
    plan: &StreamPlan,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(f32),
) -> Result<Vec<u8>> {
    let (width, height) = source.size;
    let channels = source.channels;
    let mut reader: Box<dyn RowReader + '_> = match format {
        ImageFormat::Jpeg => {
            let started = mozjpeg::Decompress::new_mem(bytes)
                .map_err(jpeg_error)?
                .rgb()
                .map_err(jpeg_error)?;
            Box::new(JpegRows { started })
        }
        ImageFormat::Png => {
            let reader = png_reader(bytes)?;
            let (color, _) = reader.output_color_type();
            Box::new(PngRows {
                reader,
                color,
                channels,
            })
        }
        other => return Err(not_streamable(other)),
    };

    // sRGB conversion of each row (when the profile is not kept).
    let transform = match (&source.icc, plan.convert_to_srgb) {
        (Some(icc), true) => match classify_rgb_icc(icc) {
            IccKind::Other => {
                let profile = ColorProfile::new_from_slice(icc)
                    .map_err(|e| ImagoError::Internal(format!("invalid ICC profile: {e}")))?;
                let layout = if channels == 4 {
                    Layout::Rgba
                } else {
                    Layout::Rgb
                };
                Some(
                    profile
                        .create_transform_8bit(
                            layout,
                            &ColorProfile::new_srgb(),
                            layout,
                            TransformOptions::default(),
                        )
                        .map_err(|e| {
                            ImagoError::Internal(format!("color conversion failed: {e}"))
                        })?,
                )
            }
            _ => None,
        },
        _ => None,
    };
    let icc_out = if plan.keep_icc {
        source.icc.as_deref()
    } else {
        None
    };
    let out_channels = if plan.flatten { 3 } else { channels };
    let mut writer = RowWriter::new(plan, out_channels, icc_out)?;
    let mut downscaler = (plan.output_size != source.size)
        .then(|| Downscaler::new(source.size, plan.output_size, channels));

    let mut row = vec![0u8; width as usize * channels];
    let mut converted = vec![0u8; width as usize * channels];
    let mut emit = |pixels: &[u8]| -> Result<()> {
        if plan.flatten && channels == 4 {
            let opaque: Vec<u8> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|[r, g, b, a]| {
                    let blend = |value: u8| {
                        let alpha = u16::from(*a);
                        ((u16::from(value) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
                    };
                    [blend(*r), blend(*g), blend(*b)]
                })
                .collect();
            writer.write_row(&opaque)
        } else {
            writer.write_row(pixels)
        }
    };
    for index in 0..height {
        if index % 64 == 0 {
            cancel.check()?;
            progress(index as f32 / height as f32);
        }
        reader.read_row(&mut row)?;
        let pixels: &[u8] = match &transform {
            Some(transform) => {
                transform
                    .transform(&row, &mut converted)
                    .map_err(|e| ImagoError::Internal(format!("color conversion failed: {e}")))?;
                &converted
            }
            None => &row,
        };
        match downscaler.as_mut() {
            Some(downscaler) => downscaler.push(pixels, &mut emit)?,
            None => emit(pixels)?,
        }
    }
    drop(reader);
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32) -> image::RgbImage {
        image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x * 255 / width) as u8, (y * 255 / height) as u8, 90])
        })
    }

    fn png_bytes(image: &image::DynamicImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    fn plan(format: ImageFormat, size: (u32, u32)) -> StreamPlan {
        StreamPlan {
            format,
            output_size: size,
            quality: 90,
            convert_to_srgb: false,
            keep_icc: false,
            exif: None,
            flatten: false,
        }
    }

    #[test]
    fn png_to_png_is_exact() {
        let image = image::DynamicImage::ImageRgb8(gradient(97, 41));
        let bytes = png_bytes(&image);
        let source = inspect(&bytes, ImageFormat::Png).unwrap();
        let out = run(
            &bytes,
            ImageFormat::Png,
            &source,
            &plan(ImageFormat::Png, (97, 41)),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.to_rgb8().as_raw(), image.to_rgb8().as_raw());
    }

    #[test]
    fn downscaling_averages_areas() {
        // 4×2 → 2×1: every output pixel averages a 2×2 block.
        let image = image::RgbImage::from_fn(4, 2, |x, _| {
            if x < 2 {
                image::Rgb([0, 100, 200])
            } else {
                image::Rgb([200, 100, 0])
            }
        });
        let bytes = png_bytes(&image::DynamicImage::ImageRgb8(image));
        let source = inspect(&bytes, ImageFormat::Png).unwrap();
        let out = run(
            &bytes,
            ImageFormat::Png,
            &source,
            &plan(ImageFormat::Png, (2, 1)),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap();
        let decoded = image::load_from_memory(&out).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.get_pixel(0, 0).0, [0, 100, 200]);
        assert_eq!(decoded.get_pixel(1, 0).0, [200, 100, 0]);
    }

    #[test]
    fn jpeg_streams_to_jpeg_with_odd_ratios() {
        let image = image::DynamicImage::ImageRgb8(gradient(300, 170));
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
            .unwrap();
        let source = inspect(&bytes, ImageFormat::Jpeg).unwrap();
        assert_eq!(source.size, (300, 170));
        let out = run(
            &bytes,
            ImageFormat::Jpeg,
            &source,
            &plan(ImageFormat::Jpeg, (113, 64)),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (113, 64));
        // The gradient survives: left is dark red, right is bright red.
        let rgb = decoded.to_rgb8();
        assert!(rgb.get_pixel(2, 32).0[0] < 30);
        assert!(rgb.get_pixel(110, 32).0[0] > 220);
    }

    #[test]
    fn transparency_is_flattened_onto_white_for_jpeg() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([0, 0, 0, 0]),
        ));
        let bytes = png_bytes(&image);
        let source = inspect(&bytes, ImageFormat::Png).unwrap();
        assert_eq!(source.channels, 4);
        let mut flat = plan(ImageFormat::Jpeg, (8, 8));
        flat.flatten = true;
        let out = run(
            &bytes,
            ImageFormat::Png,
            &source,
            &flat,
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap();
        let decoded = image::load_from_memory(&out).unwrap().to_rgb8();
        assert!(decoded.get_pixel(4, 4).0.iter().all(|&c| c > 250));
    }

    #[test]
    fn cancellation_stops_the_stream() {
        let bytes = png_bytes(&image::DynamicImage::ImageRgb8(gradient(16, 16)));
        let source = inspect(&bytes, ImageFormat::Png).unwrap();
        let token = CancelToken::new();
        token.cancel();
        let error = run(
            &bytes,
            ImageFormat::Png,
            &source,
            &plan(ImageFormat::Png, (16, 16)),
            &token,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(error.is_cancelled());
    }
}
