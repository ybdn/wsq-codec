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
//! From 1.0 the public API follows semantic versioning: the items this
//! documentation lists, their signatures, the variants and fields of the
//! types, the output of the conformant encoder (byte for byte), the decoded
//! pixels, and the `image` feature's API. Four things are not covered and may
//! change in a minor release: the exact bytes of the
//! [`Optimized`](Profile::Optimized) profile's streams (its promises about
//! size are covered), the text of error messages and the `&'static str`
//! reasons of [`DecodeError`], the `Debug` output of any type, and speed and
//! memory use. Minimum Supported Rust Version bumps are minor releases. The
//! `image` feature exposes the `image` 0.25 crate's types: a new `image`
//! major version will come as a new feature, not as a change to this one. See
//! `docs/api-1.0.md` in the repository for the review behind this API.
//!
//! # The certified part
//!
//! The decoder, the conformant encoder and [`inspect`] come from the
//! [`wsq-certified`](https://docs.rs/wsq-certified) crate, which this crate
//! re-exports and depends on at one exact version: that crate is the code
//! the FBI certification covers, and it only changes when a new
//! certification is intended. This crate adds what is not certified: the
//! [`Optimized`](Profile::Optimized) profile and the `image` integration.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::missing_errors_doc, clippy::missing_panics_doc)]

#[cfg(feature = "image")]
pub mod image;
mod optimized;

#[doc(inline)]
pub use wsq_certified::{
    DEFAULT_MAX_PIXELS, DecodeError, DecodeOptions, EncodeError, Info, MAX_DIMENSION,
    MIN_ENCODE_DIMENSION, ReadError, WriteError, decode_into, inspect, read_info,
};

use wsq_certified::for_wsq_codec::Scratch;

/// A decoded 8-bit grayscale image, with what its stream says about it, so
/// that [`decode`] alone gives the pixels and the metadata (no second
/// [`read_info`] call is needed).
///
/// The metadata fields repeat those of [`Info`] one for one, and stay flat
/// rather than nested in an `info` field, so that `image.width` reads as it
/// does on the image of any other crate.
///
/// This is a type of `wsq-codec`, not of `wsq-certified`, which has one of its
/// own: the `image` feature implements `TryFrom<Image> for GrayImage`, and
/// Rust only allows that for a type of the crate that writes the impl.
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

impl Image {
    /// The same image as the certified crate decoded it.
    fn from_certified(image: wsq_certified::Image) -> Self {
        Image {
            width: image.width,
            height: image.height,
            ppi: image.ppi,
            software: image.software,
            comments: image.comments,
            pixels: image.pixels,
        }
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
    wsq_certified::decode_with(data, options).map(Image::from_certified)
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
    inner: wsq_certified::Decoder,
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
            inner: wsq_certified::Decoder::new(options),
        }
    }

    /// The options every image is decoded with.
    #[must_use]
    pub fn options(&self) -> &DecodeOptions {
        self.inner.options()
    }

    /// Decodes a WSQ stream, as [`decode_with`] does.
    ///
    /// # Errors
    ///
    /// Returns the [`DecodeError`] that [`decode_with`] gives.
    pub fn decode(&mut self, data: &[u8]) -> Result<Image, DecodeError> {
        self.inner.decode(data).map(Image::from_certified)
    }

    /// Decodes a WSQ stream into `out`, as [`decode_into`] does.
    ///
    /// # Errors
    ///
    /// Returns the [`DecodeError`] that [`decode_into`] gives.
    pub fn decode_into(&mut self, data: &[u8], out: &mut [u8]) -> Result<Info, DecodeError> {
        self.inner.decode_into(data, out)
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
    pub const OPTIMIZED_TOLERANCE: f64 = optimized::HIT_TOLERANCE;
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

/// The certified encoder's settings for `options`: everything but the profile.
fn certified_options(options: &EncodeOptions) -> wsq_certified::EncodeOptions {
    let mut certified = wsq_certified::EncodeOptions::default()
        .with_bitrate(options.bitrate)
        .with_ppi(options.ppi)
        .with_software(options.software)
        .with_threads(options.threads);
    for comment in &options.comments {
        certified = certified.with_comment(comment.clone());
    }
    certified
}

/// Encodes with the profile `options` asks for: the certified encoder for
/// [`Profile::Conformant`], the rate search of this crate on top of its
/// analysis for [`Profile::Optimized`].
fn encode_with(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
    scratch: &mut Scratch,
) -> Result<Vec<u8>, EncodeError> {
    // Only a conformant stream may carry the FBI's implementation number.
    if options.profile == Profile::Optimized && options.software != 0 {
        return Err(EncodeError::SoftwareRequiresConformant(options.software));
    }
    let certified = certified_options(options);
    match options.profile {
        Profile::Conformant => {
            wsq_certified::for_wsq_codec::encode(pixels, width, height, &certified, scratch)
        }
        Profile::Optimized => {
            optimized::encode(pixels, width, height, options.bitrate, &certified, scratch)
        }
    }
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
    encode_with(pixels, width, height, options, &mut Scratch::default())
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
        encode_with(pixels, width, height, &self.options, &mut self.scratch)
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
