//! AVIF encoding with ravif (rav1e AV1 encoder).

use crate::codecs::{EncodeInput, EncodeOptions};
use crate::errors::Result;

#[cfg(feature = "avif")]
pub(crate) fn encode(input: EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    use image::DynamicImage;
    use ravif::{Encoder, Img, RGB8, RGBA8};
    use rgb::bytemuck;

    use crate::codecs::{effort_index, encode_error};
    use crate::settings::ImageFormat;

    let format = ImageFormat::Avif;
    let quality = f32::from(options.quality.get());
    let speed = [9, 6, 3][effort_index(options.effort)];
    let mut encoder = Encoder::new()
        .with_quality(quality)
        .with_alpha_quality(quality)
        .with_speed(speed)
        .with_num_threads(None);
    if let Some(exif) = input.exif {
        encoder = encoder.with_exif(exif);
    }

    let image = input.image;
    let (width, height) = (image.width() as usize, image.height() as usize);
    let encoded = if image.color().has_alpha() {
        let rgba = match image {
            DynamicImage::ImageRgba8(buffer) => std::borrow::Cow::Borrowed(buffer),
            other => std::borrow::Cow::Owned(other.to_rgba8()),
        };
        let pixels: &[RGBA8] = bytemuck::cast_slice(rgba.as_raw());
        encoder.encode_rgba(Img::new(pixels, width, height))
    } else {
        let rgb = match image {
            DynamicImage::ImageRgb8(buffer) => std::borrow::Cow::Borrowed(buffer),
            other => std::borrow::Cow::Owned(other.to_rgb8()),
        };
        let pixels: &[RGB8] = bytemuck::cast_slice(rgb.as_raw());
        encoder.encode_rgb(Img::new(pixels, width, height))
    }
    .map_err(|e| encode_error(format, e))?;
    Ok(encoded.avif_file)
}

#[cfg(not(feature = "avif"))]
pub(crate) fn encode(_input: EncodeInput<'_>, _options: EncodeOptions) -> Result<Vec<u8>> {
    Err(crate::codecs::unavailable(
        crate::settings::ImageFormat::Avif,
    ))
}
