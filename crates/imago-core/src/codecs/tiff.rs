//! TIFF encoding: always lossless, Deflate with the horizontal predictor.

use std::io::Cursor;

use image::DynamicImage;
use tiff::encoder::{Compression, DeflateLevel, Predictor, TiffEncoder, colortype};
use tiff::tags::Tag;

use crate::codecs::{EncodeInput, EncodeOptions, encode_error};
use crate::errors::{ImagoError, Result};
use crate::settings::{Effort, ImageFormat};

const FORMAT: ImageFormat = ImageFormat::Tiff;

pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    let level = match options.effort {
        Effort::Fast => DeflateLevel::Fast,
        Effort::Balanced => DeflateLevel::Balanced,
        Effort::Max => DeflateLevel::Best,
    };
    let mut output = Cursor::new(Vec::new());
    {
        let mut encoder = TiffEncoder::new(&mut output)
            .map_err(tiff_error)?
            .with_compression(Compression::Deflate(level))
            .with_predictor(Predictor::Horizontal);
        let image = input.image;
        let (width, height) = (image.width(), image.height());
        let icc = input.icc_profile;
        match image {
            DynamicImage::ImageLuma8(buffer) => {
                write::<colortype::Gray8>(&mut encoder, width, height, buffer.as_raw(), icc)?
            }
            DynamicImage::ImageRgb8(buffer) => {
                write::<colortype::RGB8>(&mut encoder, width, height, buffer.as_raw(), icc)?
            }
            DynamicImage::ImageRgba8(buffer) => {
                write::<colortype::RGBA8>(&mut encoder, width, height, buffer.as_raw(), icc)?
            }
            DynamicImage::ImageLuma16(buffer) => {
                write::<colortype::Gray16>(&mut encoder, width, height, buffer.as_raw(), icc)?
            }
            DynamicImage::ImageRgb16(buffer) => {
                write::<colortype::RGB16>(&mut encoder, width, height, buffer.as_raw(), icc)?
            }
            DynamicImage::ImageRgba16(buffer) => {
                write::<colortype::RGBA16>(&mut encoder, width, height, buffer.as_raw(), icc)?
            }
            // TIFF has no gray+alpha color type in the encoder; widen to RGBA.
            DynamicImage::ImageLumaA8(_) => {
                let rgba = image.to_rgba8();
                write::<colortype::RGBA8>(&mut encoder, width, height, rgba.as_raw(), icc)?
            }
            other if other.color().has_alpha() => {
                let rgba = other.to_rgba16();
                write::<colortype::RGBA16>(&mut encoder, width, height, rgba.as_raw(), icc)?
            }
            other => {
                let rgb = other.to_rgb16();
                write::<colortype::RGB16>(&mut encoder, width, height, rgb.as_raw(), icc)?
            }
        }
    }
    Ok(output.into_inner())
}

fn write<C>(
    encoder: &mut TiffEncoder<&mut Cursor<Vec<u8>>>,
    width: u32,
    height: u32,
    data: &[C::Inner],
    icc: Option<&[u8]>,
) -> Result<()>
where
    C: colortype::ColorType,
    [C::Inner]: tiff::encoder::TiffValue,
{
    let mut image = encoder.new_image::<C>(width, height).map_err(tiff_error)?;
    if let Some(icc) = icc {
        image
            .encoder()
            .write_tag(Tag::IccProfile, icc)
            .map_err(tiff_error)?;
    }
    image.write_data(data).map_err(tiff_error)
}

fn tiff_error(error: tiff::TiffError) -> ImagoError {
    encode_error(FORMAT, error)
}
