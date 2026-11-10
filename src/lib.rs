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

/// Default for [`DecodeOptions::max_pixels`]: 100 million pixels, which
/// covers a 1000 ppi palm print (about 44 million) with room to spare.
pub const DEFAULT_MAX_PIXELS: usize = 100_000_000;

/// A decoded 8-bit grayscale image, with what its stream says about it, so
/// that [`decode`] alone gives the pixels and the metadata (no second
/// [`read_info`] call is needed).
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
    /// address space of a 32-bit target, decoding fails with
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

/// Which encoder writes the stream: the one aligned with the reference
/// implementation, or one that departs from it to meet the requested size.
///
/// Both write streams that any WSQ decoder reads. They differ in what they
/// promise: see [`Conformant`](Self::Conformant) and
/// [`Optimized`](Self::Optimized). The profile is an
/// [`EncodeOptions`] field, so [`encode`], [`Encoder`] and [`encode_to`]
/// all honour it.
///
/// ```
/// use wsq_codec::{EncodeOptions, Profile};
///
/// let options = EncodeOptions::default()
///     .with_profile(Profile::Optimized)
///     .with_bitrate(0.75);
/// # let _ = options;
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Profile {
    /// The encoder aligned with the NIST reference implementation (NBIS):
    /// the bit allocation of the specification, byte-identical output for
    /// the same image and bit rate (see the crate documentation for the
    /// exceptions), the same on every platform. This is the default, and
    /// the only profile meant for FBI certification: only it may write an
    /// implementation number in [`EncodeOptions::software`].
    ///
    /// The actual size lands near the requested [`bitrate`] (the
    /// reference's allocation is approximate), not on it.
    ///
    /// [`bitrate`]: EncodeOptions::bitrate
    #[default]
    Conformant,
    /// An encoder free to depart from the reference, never certified: it
    /// meets the requested size.
    ///
    /// The stream has the conformant layout and tables; only the quantizer
    /// bin widths differ: those of the specification's bit allocation, all
    /// multiplied by one factor that a search chooses on the length of the
    /// whole stream (headers, tables, comments and entropy-coded data). The
    /// search stops on the first stream at most
    /// [`OPTIMIZED_TOLERANCE`](Self::OPTIMIZED_TOLERANCE) below the size
    /// [`bitrate`](EncodeOptions::bitrate) asks for (`bitrate * width *
    /// height / 8` bytes) and never returns a longer stream while one fits;
    /// where no stream can come that close (a target above what the image
    /// can spend, or below what the quantization table allows), it returns
    /// the nearest one it wrote. The output depends neither on the platform
    /// nor on [`threads`](EncodeOptions::threads).
    ///
    /// The target is the one setting: a size in bytes is `bitrate` times
    /// the pixel count over 8, so a second knob would say nothing more.
    ///
    /// Any WSQ decoder reads the result, but it is not what the reference
    /// writes for the same rate, so it is never conformant: encoding with a
    /// non-zero [`software`](EncodeOptions::software) is refused with
    /// [`EncodeError::SoftwareRequiresConformant`]. The NISTCOM comment
    /// records the requested rate. It costs one quantization and one
    /// Huffman coding per stream tried, and keeps the transformed image (4
    /// bytes per pixel) until the end.
    Optimized,
}

impl Profile {
    /// Share of the requested size that [`Optimized`](Self::Optimized) may
    /// fall short by when it stops: 0.005, half a percent.
    pub const OPTIMIZED_TOLERANCE: f64 = encode::HIT_TOLERANCE;
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
    ///
    /// Only the [`Conformant`](Profile::Conformant) profile may write a
    /// non-zero number: with [`Optimized`](Profile::Optimized) it is
    /// refused ([`EncodeError::SoftwareRequiresConformant`]).
    pub software: u16,
    /// Number of threads to split the work over: 1, the default, keeps it
    /// all on the calling thread, and 0 uses one thread per core. The
    /// output does not depend on it.
    pub threads: usize,
    /// Which encoder to use: [`Profile::Conformant`], the default, or
    /// [`Profile::Optimized`], which meets the size [`bitrate`](Self::bitrate)
    /// asks for.
    pub profile: Profile,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions {
            bitrate: 0.75,
            ppi: None,
            comments: Vec::new(),
            software: 0,
            threads: 1,
            profile: Profile::Conformant,
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

    /// Sets [`profile`](Self::profile).
    #[must_use]
    pub fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = profile;
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
/// them again. The output is the same either way. Its peak is then about
/// 10 bytes per pixel, since the buffers stay while the quantized
/// coefficients are read, against 8 for [`decode_with`] (see
/// [`DecodeOptions::max_pixels`]); a refused allocation gives
/// [`DecodeError::OutOfMemory`] as there.
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
/// The [`profile`](EncodeOptions::profile) chooses the encoder:
/// [`Conformant`](Profile::Conformant) by default, which reproduces the
/// reference, or [`Optimized`](Profile::Optimized), which meets the
/// requested size.
///
/// # Errors
///
/// Returns an [`EncodeError`]:
/// [`InvalidDimensions`](EncodeError::InvalidDimensions) if a dimension is outside that range,
/// [`PixelCountMismatch`](EncodeError::PixelCountMismatch) if `pixels` does not hold
/// `width * height` bytes, [`InvalidBitrate`](EncodeError::InvalidBitrate) or
/// [`BitrateTooLow`](EncodeError::BitrateTooLow) for an unusable bit rate,
/// [`CommentTooLong`](EncodeError::CommentTooLong) for a comment that does not fit in one
/// segment, [`InvalidPpi`](EncodeError::InvalidPpi) if the resolution is zero, and
/// [`SoftwareRequiresConformant`](EncodeError::SoftwareRequiresConformant) for an implementation
/// number with the [`Optimized`](Profile::Optimized) profile.
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

/// Experimental entry points outside the reference implementation, kept
/// for the benchmarks.
///
/// Nothing here is covered by semantic versioning: it may change or go in
/// any release. Use [`Profile::Optimized`] instead.
#[doc(hidden)]
pub mod experimental {
    use crate::{EncodeError, EncodeOptions, Profile, encode};

    /// Same as [`Profile::OPTIMIZED_TOLERANCE`].
    pub const RATE_TOLERANCE: f64 = Profile::OPTIMIZED_TOLERANCE;

    /// Encodes an image with [`Profile::Optimized`], whatever
    /// `options.profile` says, and with implementation number 0 whatever
    /// `options.software` says (the prototype of #42 ignored it).
    ///
    /// # Errors
    ///
    /// Returns the [`EncodeError`] that [`encode`] gives.
    pub fn encode_at_rate(
        pixels: &[u8],
        width: usize,
        height: usize,
        options: &EncodeOptions,
    ) -> Result<Vec<u8>, EncodeError> {
        let options = options
            .clone()
            .with_profile(Profile::Optimized)
            .with_software(0);
        encode(pixels, width, height, &options)
    }
}
