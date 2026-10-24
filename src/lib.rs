//! Decoder and encoder for WSQ (Wavelet Scalar Quantization) fingerprint
//! images, as the FBI specifies it (IAFIS-IC-0110, version 3.1).
//!
//! WSQ compresses 8-bit grayscale images with a 9/7 biorthogonal wavelet
//! decomposition into 60 subbands, scalar quantization tuned to a target
//! bit rate, and Huffman coding. ANSI/NIST-ITL transactions carry WSQ
//! images in their Type-4 and Type-14 records.
//!
//! The decoder and the encoder reproduce the NIST reference implementation
//! (NBIS): decoding gives the same pixels, and encoding gives the same
//! bytes for the same image and bit rate, except on degenerate images
//! where NBIS's own output is broken (very small or almost featureless
//! ones).
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A 256x256 test pattern.
//! let (width, height) = (256, 256);
//! let pixels: Vec<u8> = (0..width * height)
//!     .map(|i| (((i % width) * 7 + (i / width) * 3) % 200 + 20) as u8)
//!     .collect();
//!
//! let options = wsq_codec::EncodeOptions { ppi: Some(500), ..Default::default() };
//! let wsq = wsq_codec::encode(&pixels, width, height, &options)?;
//!
//! let image = wsq_codec::decode(&wsq)?;
//! assert_eq!((image.width, image.height), (width, height));
//! assert_eq!(image.ppi, Some(500));
//! assert_eq!(image.pixels.len(), pixels.len());
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bits;
mod decode;
mod encode;
mod error;
mod huffman;
mod nistcom;
mod quant;
mod tables;
mod tree;
mod wavelet;

pub use error::{DecodeError, EncodeError};

/// A decoded 8-bit grayscale image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Scanning resolution in pixels per inch, when the stream records it
    /// in a NISTCOM comment.
    pub ppi: Option<u32>,
    /// Pixels in row-major order, `width * height` of them, 0 for black.
    pub pixels: Vec<u8>,
}

/// What a WSQ stream says about its image, read without decoding it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Scanning resolution in pixels per inch, when the stream records it
    /// in a NISTCOM comment.
    pub ppi: Option<u32>,
    /// Implementation number of the encoder that wrote the stream, as
    /// assigned by the FBI; 0 when unassigned.
    pub software: u16,
    /// Payloads of the comment segments before the image data.
    pub comments: Vec<Vec<u8>>,
}

/// Encoder settings.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodeOptions {
    /// Target bit rate in bits per pixel. 0.75, the usual choice for 500 ppi
    /// fingerprints, gives about 15:1 compression; 2.25 gives about 5:1.
    pub bitrate: f32,
    /// Scanning resolution to record, or `None` for unknown.
    pub ppi: Option<u32>,
    /// Extra comment segments, written after the NISTCOM comment the
    /// encoder always writes.
    pub comments: Vec<Vec<u8>>,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions {
            bitrate: 0.75,
            ppi: None,
            comments: Vec::new(),
        }
    }
}

/// Decodes a WSQ stream.
pub fn decode(data: &[u8]) -> Result<Image, DecodeError> {
    decode::decode(data)
}

/// Reads the dimensions, resolution and comments of a WSQ stream without
/// decoding its image.
pub fn read_info(data: &[u8]) -> Result<Info, DecodeError> {
    decode::read_info(data)
}

/// Encodes an 8-bit grayscale image, given as `width * height` pixels in
/// row-major order.
///
/// Both dimensions must be between 81 and 65535; smaller images leave
/// some subbands too small to quantize.
pub fn encode(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
) -> Result<Vec<u8>, EncodeError> {
    encode::encode(pixels, width, height, options)
}
