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
//! let options = wsq_codec::EncodeOptions::default().with_ppi(Some(500));
//! let wsq = wsq_codec::encode(&pixels, width, height, &options)?;
//!
//! let image = wsq_codec::decode(&wsq)?;
//! assert_eq!((image.width, image.height), (width, height));
//! assert_eq!(image.ppi, Some(500));
//! assert_eq!(image.pixels.len(), pixels.len());
//! # Ok(())
//! # }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bits;
mod decode;
mod encode;
mod error;
mod huffman;
#[cfg(feature = "image")]
pub mod image;
pub mod inspect;
mod math;
mod nistcom;
mod par;
mod quant;
mod tables;
mod tree;
mod wavelet;

pub use error::{DecodeError, EncodeError, ReadError, WriteError};

/// Default for [`DecodeOptions::max_pixels`]: 100 million pixels, which
/// covers a 1000 ppi palm print (about 44 million) with room to spare.
pub const DEFAULT_MAX_PIXELS: usize = 100_000_000;

/// A decoded 8-bit grayscale image.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Image {
    /// Width in pixels, at most 65535 as the format allows.
    pub width: usize,
    /// Height in pixels, at most 65535 as the format allows.
    pub height: usize,
    /// Scanning resolution in pixels per inch, when the stream records it
    /// in a NISTCOM comment.
    pub ppi: Option<u32>,
    /// Pixels in row-major order, `width * height` of them, 0 for black.
    pub pixels: Vec<u8>,
}

/// What a WSQ stream says about its image, read without decoding it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Info {
    /// Width in pixels, at most 65535 as the format allows.
    pub width: usize,
    /// Height in pixels, at most 65535 as the format allows.
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

/// Decoder settings.
///
/// ```
/// let options = wsq_codec::DecodeOptions::default().with_max_pixels(Some(20_000_000));
/// # let _ = options;
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DecodeOptions {
    /// Largest `width * height` the decoder accepts, or `None` for no
    /// limit. Decoding needs about 11 bytes per pixel at its peak, and the
    /// frame header alone decides the size, so a small crafted stream can
    /// otherwise claim an image of several gigapixels.
    pub max_pixels: Option<usize>,
    /// Number of threads to split the work over: 1, the default, keeps it
    /// all on the calling thread, and 0 uses one thread per core. The
    /// output does not depend on it.
    pub threads: usize,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        DecodeOptions {
            max_pixels: Some(DEFAULT_MAX_PIXELS),
            threads: 1,
        }
    }
}

impl DecodeOptions {
    /// Sets [`max_pixels`](Self::max_pixels).
    #[must_use]
    pub fn with_max_pixels(mut self, max_pixels: Option<usize>) -> Self {
        self.max_pixels = max_pixels;
        self
    }

    /// Sets [`threads`](Self::threads).
    #[must_use]
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }
}

/// Encoder settings.
///
/// ```
/// let options = wsq_codec::EncodeOptions::default()
///     .with_bitrate(2.25)
///     .with_ppi(Some(1000))
///     .with_comment(b"scanned on site".to_vec());
/// # let _ = options;
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct EncodeOptions {
    /// Target bit rate in bits per pixel. 0.75, the usual choice for 500 ppi
    /// fingerprints, gives about 15:1 compression; 2.25 gives about 5:1.
    pub bitrate: f32,
    /// Scanning resolution to record, or `None` for unknown. Zero is
    /// rejected.
    pub ppi: Option<u32>,
    /// Extra comment segments. The encoder always writes a NISTCOM comment
    /// describing the image; the first of these comments that is itself a
    /// NISTCOM comment (it starts with `NIST_COM`) is merged into it, as
    /// the reference implementation does: its other fields are kept, and
    /// the encoder's own fields (dimensions, depth, `PPI`, `LOSSY`,
    /// colorspace, compression, bit rate) replace any it gives. The others
    /// are written as they are, after it.
    pub comments: Vec<Vec<u8>>,
    /// Implementation number the FBI assigned to the encoder, written in
    /// the frame header. 0, the default, means unassigned.
    pub software: u16,
    /// Number of threads to split the work over: 1, the default, keeps it
    /// all on the calling thread, and 0 uses one thread per core. The
    /// output does not depend on it.
    pub threads: usize,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions {
            bitrate: 0.75,
            ppi: None,
            comments: Vec::new(),
            software: 0,
            threads: 1,
        }
    }
}

impl EncodeOptions {
    /// Sets [`bitrate`](Self::bitrate).
    #[must_use]
    pub fn with_bitrate(mut self, bitrate: f32) -> Self {
        self.bitrate = bitrate;
        self
    }

    /// Sets [`ppi`](Self::ppi).
    #[must_use]
    pub fn with_ppi(mut self, ppi: Option<u32>) -> Self {
        self.ppi = ppi;
        self
    }

    /// Appends a comment to [`comments`](Self::comments).
    #[must_use]
    pub fn with_comment(mut self, comment: Vec<u8>) -> Self {
        self.comments.push(comment);
        self
    }

    /// Sets [`software`](Self::software).
    #[must_use]
    pub fn with_software(mut self, software: u16) -> Self {
        self.software = software;
        self
    }

    /// Sets [`threads`](Self::threads).
    #[must_use]
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }
}

/// Resolves a `threads` option: 0 means one thread per core.
fn thread_count(threads: usize) -> usize {
    match threads {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get()),
        n => n,
    }
}

/// Decodes a WSQ stream with the default [`DecodeOptions`], which reject
/// images of more than [`DEFAULT_MAX_PIXELS`] pixels.
///
/// # Errors
///
/// Returns a [`DecodeError`] if the data is not a valid WSQ stream (for
/// instance [`NotWsq`](DecodeError::NotWsq), [`UnexpectedEof`](DecodeError::UnexpectedEof) or
/// [`CorruptData`](DecodeError::CorruptData)), or [`TooLarge`](DecodeError::TooLarge) if the
/// image has more than [`DEFAULT_MAX_PIXELS`] pixels.
pub fn decode(data: &[u8]) -> Result<Image, DecodeError> {
    decode_with(data, &DecodeOptions::default())
}

/// Decodes a WSQ stream.
///
/// # Errors
///
/// Returns a [`DecodeError`] if the data is not a valid WSQ stream, as for [`decode`], or
/// [`TooLarge`](DecodeError::TooLarge) if the image has more than
/// [`max_pixels`](DecodeOptions::max_pixels) pixels.
pub fn decode_with(data: &[u8], options: &DecodeOptions) -> Result<Image, DecodeError> {
    decode::decode(data, options, &mut wavelet::Scratch::default())
}

/// Decodes a WSQ stream into `out`, which must hold exactly one byte per
/// pixel (see [`read_info`] for the dimensions), and returns what the
/// stream says about its image.
///
/// # Errors
///
/// Returns the [`DecodeError`] that [`decode_with`] gives, or
/// [`OutputSize`](DecodeError::OutputSize) if `out` does not hold `width * height` bytes.
pub fn decode_into(
    data: &[u8],
    options: &DecodeOptions,
    out: &mut [u8],
) -> Result<Info, DecodeError> {
    decode::decode_into(data, options, &mut wavelet::Scratch::default(), out)
}

/// A decoder that keeps its working buffers from one image to the next.
///
/// Decoding needs two buffers of 4 bytes per pixel besides the output.
/// [`decode_with`] allocates them for every image; a `Decoder` allocates
/// them once and keeps them, as large as the largest image so far, which
/// spares a batch of images the cost of allocating and first touching
/// them again. The output is the same either way.
///
/// ```
/// # let wsq = wsq_codec::encode(&[128; 100 * 100], 100, 100, &Default::default()).unwrap();
/// let mut decoder = wsq_codec::Decoder::new(wsq_codec::DecodeOptions::default());
/// let mut pixels = vec![0; 100 * 100];
/// for stream in [&wsq, &wsq] {
///     let info = decoder.decode_into(stream, &mut pixels)?;
///     assert_eq!((info.width, info.height), (100, 100));
/// }
/// # Ok::<(), wsq_codec::DecodeError>(())
/// ```
#[derive(Debug)]
pub struct Decoder {
    options: DecodeOptions,
    scratch: wavelet::Scratch,
}

impl Decoder {
    /// A decoder using `options` for every image.
    pub fn new(options: DecodeOptions) -> Self {
        Decoder {
            options,
            scratch: wavelet::Scratch::kept(),
        }
    }

    /// Decodes a WSQ stream, as [`decode_with`] does.
    ///
    /// # Errors
    ///
    /// Returns the [`DecodeError`] that [`decode_with`] gives.
    pub fn decode(&mut self, data: &[u8]) -> Result<Image, DecodeError> {
        decode::decode(data, &self.options, &mut self.scratch)
    }

    /// Decodes a WSQ stream into `out`, as [`decode_into`] does.
    ///
    /// # Errors
    ///
    /// Returns the [`DecodeError`] that [`decode_into`] gives.
    pub fn decode_into(&mut self, data: &[u8], out: &mut [u8]) -> Result<Info, DecodeError> {
        decode::decode_into(data, &self.options, &mut self.scratch, out)
    }
}

/// Reads a whole WSQ stream from `reader` and decodes it.
///
/// The wavelet transform needs every coefficient before it can produce a
/// single pixel, so this reads `reader` to its end first: give it a reader
/// that ends with the stream, for instance through [`Read::take`] when the
/// stream sits inside a larger file, which also bounds the memory a
/// stream from an untrusted source can claim.
///
/// # Errors
///
/// Returns [`ReadError::Io`] if reading fails, and [`ReadError::Decode`] with the
/// [`DecodeError`] that [`decode_with`] gives if the data read cannot be decoded.
///
/// [`Read::take`]: std::io::Read::take
pub fn decode_from<R: std::io::Read>(
    mut reader: R,
    options: &DecodeOptions,
) -> Result<Image, ReadError> {
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    Ok(decode_with(&data, options)?)
}

/// Reads the dimensions, resolution and comments of a WSQ stream without
/// decoding its image.
///
/// # Errors
///
/// Returns a [`DecodeError`] if the headers are not valid, as for [`decode`]. The entropy-coded
/// data is not read, so corruption there is not reported. The image size is not limited.
pub fn read_info(data: &[u8]) -> Result<Info, DecodeError> {
    decode::read_info(data)
}

/// Encodes an 8-bit grayscale image, given as `width * height` pixels in
/// row-major order.
///
/// Both dimensions must be between 81 and 65535; smaller images leave
/// some subbands too small to quantize.
///
/// # Errors
///
/// Returns an [`EncodeError`]:
/// [`InvalidDimensions`](EncodeError::InvalidDimensions) if a dimension is outside that range,
/// [`PixelCountMismatch`](EncodeError::PixelCountMismatch) if `pixels` does not hold
/// `width * height` bytes, [`InvalidBitrate`](EncodeError::InvalidBitrate) or
/// [`BitrateTooLow`](EncodeError::BitrateTooLow) for an unusable bit rate,
/// [`CommentTooLong`](EncodeError::CommentTooLong) for a comment that does not fit in one
/// segment, and [`InvalidPpi`](EncodeError::InvalidPpi) if the resolution is zero.
pub fn encode(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
) -> Result<Vec<u8>, EncodeError> {
    encode::encode(
        pixels,
        width,
        height,
        options,
        &mut wavelet::Scratch::default(),
    )
}

/// An encoder that keeps its working buffers from one image to the next.
///
/// Encoding needs two buffers of 4 bytes per pixel besides the stream.
/// [`encode`] allocates them for every image; an `Encoder` allocates them
/// once and keeps them, as large as the largest image so far, as a
/// [`Decoder`] does. The output is the same either way.
///
/// ```
/// let mut encoder = wsq_codec::Encoder::new(wsq_codec::EncodeOptions::default());
/// for value in [96, 160] {
///     let wsq = encoder.encode(&[value; 100 * 100], 100, 100)?;
///     assert_eq!(wsq, wsq_codec::encode(&[value; 100 * 100], 100, 100, &Default::default())?);
/// }
/// # Ok::<(), wsq_codec::EncodeError>(())
/// ```
#[derive(Debug)]
pub struct Encoder {
    options: EncodeOptions,
    scratch: wavelet::Scratch,
}

impl Encoder {
    /// An encoder using `options` for every image.
    pub fn new(options: EncodeOptions) -> Self {
        Encoder {
            options,
            scratch: wavelet::Scratch::kept(),
        }
    }

    /// Encodes an image, as [`encode`] does.
    ///
    /// # Errors
    ///
    /// Returns the [`EncodeError`] that [`encode`] gives.
    pub fn encode(
        &mut self,
        pixels: &[u8],
        width: usize,
        height: usize,
    ) -> Result<Vec<u8>, EncodeError> {
        encode::encode(pixels, width, height, &self.options, &mut self.scratch)
    }
}

/// Encodes an image as [`encode`] does and writes the stream to `writer`.
///
/// # Errors
///
/// Returns [`WriteError::Encode`] with the [`EncodeError`] that [`encode`] gives, and
/// [`WriteError::Io`] if writing or flushing fails.
pub fn encode_to<W: std::io::Write>(
    mut writer: W,
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
) -> Result<(), WriteError> {
    writer.write_all(&encode(pixels, width, height, options)?)?;
    writer.flush()?;
    Ok(())
}
