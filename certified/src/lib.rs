//! The decoder, the conformant encoder and the inspection tools of
//! `wsq-codec`, in a crate of their own that is frozen and versioned with
//! the FBI certificate.
//!
//! Most users want [`wsq-codec`](https://docs.rs/wsq-codec), which depends on
//! this crate, re-exports all of it, and adds an encoding profile that
//! departs from the reference to meet a size, and the `image` integration.
//! This crate is the part that is certified: WSQ (Wavelet Scalar
//! Quantization) decoding, and encoding that reproduces the NIST reference
//! implementation (NBIS), as the FBI specifies it (IAFIS-IC-0110, version
//! 3.1). Any change to it needs a new certification, so it only changes when
//! that is intended: see `certified/README.md` and `docs/certification.md`
//! in the repository.
//!
//! The decoder and the encoder reproduce NBIS: decoding gives the same
//! pixels, and encoding gives the same bytes for the same image and bit
//! rate, except on degenerate images where NBIS's own output is broken
//! (very small or almost featureless ones).
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A 256x256 test pattern.
//! let (width, height) = (256, 256);
//! let pixels: Vec<u8> = (0..width * height)
//!     .map(|i| (((i % width) * 7 + (i / width) * 3) % 200 + 20) as u8)
//!     .collect();
//!
//! let options = wsq_certified::EncodeOptions::default().with_ppi(Some(500));
//! let wsq = wsq_certified::encode(&pixels, width, height, &options)?;
//!
//! let image = wsq_certified::decode(&wsq)?;
//! assert_eq!((image.width, image.height), (width, height));
//! assert_eq!(image.ppi, Some(500));
//! assert_eq!(image.pixels.len(), pixels.len());
//! # Ok(())
//! # }
//! ```
//!
//! # Dimensions
//!
//! Widths and heights are `usize` throughout, to match slice lengths and the
//! `width * height` arithmetic, but the format stores them in 16 bits: no
//! stream and no image this crate handles exceeds [`MAX_DIMENSION`] (65535)
//! on a side. The encoder also refuses images under
//! [`MIN_ENCODE_DIMENSION`] (81) on a side. Bindings and converters can
//! therefore narrow a dimension to `u16` after a successful call, and must
//! check it before an encoding.
//!
//! # Stability
//!
//! The version of this crate names the certified code. Its documented API
//! follows semantic versioning, and so do the decoded pixels and the bytes
//! of the encoder for a given image and options; the text of error
//! messages, the `&'static str` reasons of [`DecodeError`], the `Debug`
//! output of any type, and speed and memory use are not covered. Any
//! release, even a patch one, is a new version to certify. The hidden
//! `for_wsq_codec` module is not part of the API: it is the seam
//! `wsq-codec` builds its optimized profile on, and `wsq-codec` depends on
//! one exact version of this crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::missing_errors_doc, clippy::missing_panics_doc)]

mod bits;
mod decode;
mod encode;
mod error;
mod huffman;
pub mod inspect;
mod math;
mod mem;
mod nistcom;
mod par;
mod quant;
mod tables;
#[cfg(test)]
mod testgen;
mod tree;
mod wavelet;

pub use error::{DecodeError, EncodeError, ReadError, WriteError};

use wavelet::Scratch;

/// The version of this crate, as the certification names it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What `wsq-codec` builds its optimized profile on. Not part of the
/// documented API, not covered by semantic versioning, and not certified
/// behaviour: it only exposes the conformant pipeline's steps (the
/// analysis of an image, and the stream written for bin widths scaled by a
/// factor, where a factor of 1 is the conformant stream) and the
/// deterministic arithmetic of the bit allocation, so that the rate search
/// of the optimized profile does not have to copy them.
#[doc(hidden)]
pub mod for_wsq_codec {
    use crate::{EncodeError, EncodeOptions};

    pub use crate::encode::Scaler;
    pub use crate::wavelet::Scratch;

    /// Analyzes an image as the conformant encoder does and hands the
    /// result to `f`, which writes streams from it with
    /// [`Scaler::write`]. The buffers of `scratch` are released when `f`
    /// returns, unless it was made with [`Scratch::kept`].
    ///
    /// # Errors
    ///
    /// Returns the errors [`encode`](crate::encode) gives for the same
    /// arguments (except those only the stream can raise, which `f`
    /// returns), and whatever `f` returns.
    pub fn with_scaler<T>(
        pixels: &[u8],
        width: usize,
        height: usize,
        options: &EncodeOptions,
        scratch: &mut Scratch,
        f: impl FnOnce(&Scaler<'_>) -> Result<T, EncodeError>,
    ) -> Result<T, EncodeError> {
        crate::encode::with_scaler(pixels, width, height, options, scratch, f)
    }

    /// The conformant encoder, on buffers the caller keeps.
    ///
    /// # Errors
    ///
    /// Returns the [`EncodeError`] that [`encode`](crate::encode) gives.
    pub fn encode(
        pixels: &[u8],
        width: usize,
        height: usize,
        options: &EncodeOptions,
        scratch: &mut Scratch,
    ) -> Result<Vec<u8>, EncodeError> {
        crate::encode::encode(pixels, width, height, options, scratch)
    }

    /// Natural logarithm, computed without the platform's math library.
    #[must_use]
    pub fn ln(x: f64) -> f64 {
        crate::math::ln(x)
    }

    /// `x` to the power `y`, computed without the platform's math library.
    #[must_use]
    pub fn pow(x: f64, y: f64) -> f64 {
        crate::math::pow(x, y)
    }

    /// Resolves a `threads` option the way the encoder does: 0 means one
    /// thread per core.
    #[must_use]
    pub fn thread_count(threads: usize) -> usize {
        crate::thread_count(threads)
    }
}

/// Default for [`DecodeOptions::max_pixels`]: 100 million pixels, which
/// covers a 1000 ppi palm print (about 44 million) with room to spare.
pub const DEFAULT_MAX_PIXELS: usize = 100_000_000;

/// Largest width or height of a WSQ image, 65535: the frame header stores
/// each in 16 bits. Dimensions are `usize` in this API, but never exceed this.
pub const MAX_DIMENSION: usize = 65535;

/// Smallest width and height [`encode`] accepts, 81. Below it some subbands
/// are too small for the variance estimate that drives quantization. The
/// decoder has no such limit.
pub const MIN_ENCODE_DIMENSION: usize = encode::MIN_DIMENSION;

/// A decoded 8-bit grayscale image, with what its stream says about it, so
/// that [`decode`] alone gives the pixels and the metadata (no second
/// [`read_info`] call is needed).
///
/// The metadata fields repeat those of [`Info`] one for one, and stay flat
/// rather than nested in an `info` field, so that `image.width` reads as it
/// does on the image of any other crate.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Image {
    /// Width in pixels, from 1 to [`MAX_DIMENSION`] (65535).
    pub width: usize,
    /// Height in pixels, from 1 to [`MAX_DIMENSION`] (65535).
    pub height: usize,
    /// Scanning resolution in pixels per inch, when the stream records it
    /// in a NISTCOM comment.
    pub ppi: Option<u32>,
    /// Implementation number of the encoder that wrote the stream, as
    /// assigned by the FBI; 0 when unassigned. Same as [`Info::software`].
    pub software: u16,
    /// Payloads of the comment segments before the image data. Same as
    /// [`Info::comments`].
    pub comments: Vec<Vec<u8>>,
    /// Pixels in row-major order, `width * height` of them, 0 for black.
    pub pixels: Vec<u8>,
}

/// What a WSQ stream says about its image, read without decoding it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Info {
    /// Width in pixels, from 1 to [`MAX_DIMENSION`] (65535).
    pub width: usize,
    /// Height in pixels, from 1 to [`MAX_DIMENSION`] (65535).
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
/// let options = wsq_certified::DecodeOptions::default().with_max_pixels(Some(20_000_000));
/// # let _ = options;
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct DecodeOptions {
    /// Largest `width * height` the decoder accepts, or `None` for no
    /// limit. The frame header alone decides the size, so a small crafted
    /// stream can otherwise claim an image of several gigapixels.
    ///
    /// Decoding needs about 8 bytes per pixel at its peak: 4 for the
    /// transformed image and 4 for the wavelet scratch buffer. The quantized
    /// coefficients (at most 2 bytes per pixel) are dropped before the
    /// scratch buffer is allocated, and the output pixels (1 byte) are
    /// allocated after it is gone. A stream of under 1 KB that declares
    /// 10000 by 10000 pixels with every bin at zero, which a flat image
    /// legitimately produces and so cannot be rejected, peaked at 787 MiB
    /// (measured on Linux x86-64, one thread). The limit is therefore the
    /// only defense against a stream that is cheap to send and costly to
    /// decode. When the buffers cannot be allocated, or are larger than the
    /// address space of a 32-bit target, or the wavelet transform cannot
    /// allocate its working blocks, decoding fails with
    /// [`DecodeError::OutOfMemory`] instead of aborting or panicking.
    ///
    /// For ANSI/NIST transactions, call [`read_info`] first and compare the
    /// dimensions with the record's own fields, or derive the limit from
    /// them.
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

/// Encoder settings. The encoder is always the conformant one.
///
/// ```
/// let options = wsq_certified::EncodeOptions::default()
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
    decode::decode(data, options, &mut Scratch::default())
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
    decode::decode_into(data, options, &mut Scratch::default(), out)
}

/// A decoder that keeps its working buffers from one image to the next.
///
/// `Decoder::default()` uses the default [`DecodeOptions`].
///
/// Decoding needs two buffers of 4 bytes per pixel besides the output.
/// [`decode_with`] allocates them for every image; a `Decoder` allocates
/// them once and keeps them, as large as the largest image so far, which
/// spares a batch of images the cost of allocating and first touching
/// them again. The output is the same either way. Its peak is then about
/// 10 bytes per pixel, since the buffers stay while the quantized
/// coefficients are read, against 8 for [`decode_with`] (see
/// [`DecodeOptions::max_pixels`]); a refused allocation gives
/// [`DecodeError::OutOfMemory`] as there.
///
/// ```
/// # let wsq = wsq_certified::encode(&[128; 100 * 100], 100, 100, &Default::default()).unwrap();
/// let mut decoder = wsq_certified::Decoder::new(wsq_certified::DecodeOptions::default());
/// let mut pixels = vec![0; 100 * 100];
/// for stream in [&wsq, &wsq] {
///     let info = decoder.decode_into(stream, &mut pixels)?;
///     assert_eq!((info.width, info.height), (100, 100));
/// }
/// # Ok::<(), wsq_certified::DecodeError>(())
/// ```
#[derive(Debug)]
pub struct Decoder {
    options: DecodeOptions,
    scratch: Scratch,
}

impl Default for Decoder {
    /// A decoder using the default [`DecodeOptions`].
    fn default() -> Self {
        Decoder::new(DecodeOptions::default())
    }
}

impl Decoder {
    /// A decoder using `options` for every image.
    #[must_use]
    pub fn new(options: DecodeOptions) -> Self {
        Decoder {
            options,
            scratch: Scratch::kept(),
        }
    }

    /// The options every image is decoded with.
    #[must_use]
    pub fn options(&self) -> &DecodeOptions {
        &self.options
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
/// The stream is the one the reference implementation writes for the
/// same image and bit rate (see the crate documentation for the
/// exceptions).
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
    encode::encode(pixels, width, height, options, &mut Scratch::default())
}

/// An encoder that keeps its working buffers from one image to the next.
///
/// Encoding needs two buffers of 4 bytes per pixel besides the stream.
/// [`encode`] allocates them for every image; an `Encoder` allocates them
/// once and keeps them, as large as the largest image so far, as a
/// [`Decoder`] does. The output is the same either way.
///
/// ```
/// let mut encoder = wsq_certified::Encoder::new(wsq_certified::EncodeOptions::default());
/// for value in [96, 160] {
///     let wsq = encoder.encode(&[value; 100 * 100], 100, 100)?;
///     assert_eq!(wsq, wsq_certified::encode(&[value; 100 * 100], 100, 100, &Default::default())?);
/// }
/// # Ok::<(), wsq_certified::EncodeError>(())
/// ```
#[derive(Debug)]
pub struct Encoder {
    options: EncodeOptions,
    scratch: Scratch,
}

impl Default for Encoder {
    /// An encoder using the default [`EncodeOptions`].
    fn default() -> Self {
        Encoder::new(EncodeOptions::default())
    }
}

impl Encoder {
    /// An encoder using `options` for every image.
    #[must_use]
    pub fn new(options: EncodeOptions) -> Self {
        Encoder {
            options,
            scratch: Scratch::kept(),
        }
    }

    /// The options every image is encoded with.
    #[must_use]
    pub fn options(&self) -> &EncodeOptions {
        &self.options
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
