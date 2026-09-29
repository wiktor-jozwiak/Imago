//! Image processing: detection, header probing, decoding, color handling,
//! resizing, automatic format selection, animations and the per-file
//! [`pipeline`] that ties them together.

pub mod animation;
pub mod auto;
pub mod color;
pub mod decode;
pub mod detection;
pub mod pipeline;
pub mod probe;
pub mod resize;

pub use decode::{DecodedImage, decode};
pub use detection::{Detected, detect_file, sniff};
pub use probe::{ImageProbe, probe_bytes, probe_file};
pub use resize::{ResizeQuality, resize, target_dimensions};

use crate::errors::{ImagoError, Result};
use crate::settings::ImageFormat;

/// Runs third-party parsing/decoding code and turns a panic into a
/// [`ImagoError::Decode`], so a malicious or broken file fails alone instead
/// of taking the whole process down.
pub(crate) fn guard_decoder<T>(format: ImageFormat, work: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(result) => result,
        Err(payload) => Err(ImagoError::Decode {
            format: format.display_name().into(),
            message: format!(
                "the decoder crashed on this file: {}",
                crate::codecs::panic_message(payload.as_ref())
            ),
        }),
    }
}
