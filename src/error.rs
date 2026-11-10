use std::error::Error;
use std::fmt;

/// Why a WSQ stream could not be decoded.
///
/// The enum and its struct variants are `#[non_exhaustive]`: match with a
/// wildcard arm and, in the pattern of a struct variant, `..`, so that later
/// versions can add variants and context to them. A tuple variant holds the
/// one value that identifies the error and does not change.
///
/// The text of [`Display`](fmt::Display), and the `&'static str` reasons the
/// variants carry, describe the error for people and may be reworded in any
/// release: match on the variants, not on the text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DecodeError {
    /// The data does not start with the WSQ start-of-image marker.
    NotWsq,
    /// The data ended in the middle of the stream.
    UnexpectedEof,
    /// A marker appeared where the format does not allow it.
    UnexpectedMarker(u16),
    /// A marker segment is malformed.
    #[non_exhaustive]
    InvalidSegment {
        /// The marker that introduced the segment.
        marker: u16,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// The stream needs a table that it never defines.
    MissingTable(&'static str),
    /// A block refers to a Huffman table that was not defined.
    UndefinedHuffmanTable(u8),
    /// The frame header gives a zero width or height.
    #[non_exhaustive]
    InvalidDimensions {
        /// Width from the frame header (at most
        /// [`MAX_DIMENSION`](crate::MAX_DIMENSION), as the header field has
        /// 16 bits).
        width: usize,
        /// Height from the frame header (at most
        /// [`MAX_DIMENSION`](crate::MAX_DIMENSION)).
        height: usize,
    },
    /// The entropy-coded data is corrupt.
    CorruptData(&'static str),
    /// The image has more pixels than [`DecodeOptions::max_pixels`]
    /// allows.
    ///
    /// [`DecodeOptions::max_pixels`]: crate::DecodeOptions::max_pixels
    #[non_exhaustive]
    TooLarge {
        /// Width from the frame header.
        width: usize,
        /// Height from the frame header.
        height: usize,
        /// The limit in force.
        max_pixels: usize,
    },
    /// The buffer given to [`decode_into`] does not hold exactly one byte
    /// per pixel.
    ///
    /// [`decode_into`]: crate::decode_into
    #[non_exhaustive]
    OutputSize {
        /// `width * height` of the image.
        expected: usize,
        /// Length of the buffer.
        actual: usize,
    },
    /// The memory the image needs could not be allocated.
    ///
    /// The frame header alone decides how much a stream asks for, so this
    /// is what a crafted stream gets when the machine, or the address space
    /// of a 32-bit target, cannot hold its image. The allocations that can
    /// fail this way are the image-sized ones; see
    /// [`DecodeOptions::max_pixels`] for what a decode needs.
    ///
    /// [`DecodeOptions::max_pixels`]: crate::DecodeOptions::max_pixels
    OutOfMemory,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::NotWsq => f.write_str("not a WSQ stream (no start-of-image marker)"),
            DecodeError::UnexpectedEof => f.write_str("unexpected end of WSQ data"),
            DecodeError::UnexpectedMarker(m) => write!(f, "unexpected marker 0x{m:04X}"),
            DecodeError::InvalidSegment { marker, reason } => {
                write!(f, "invalid segment 0x{marker:04X}: {reason}")
            }
            DecodeError::MissingTable(table) => write!(f, "missing {table}"),
            DecodeError::UndefinedHuffmanTable(id) => write!(f, "undefined Huffman table {id}"),
            DecodeError::InvalidDimensions { width, height } => {
                write!(f, "invalid image dimensions {width}x{height}")
            }
            DecodeError::CorruptData(reason) => write!(f, "corrupt WSQ data: {reason}"),
            DecodeError::TooLarge {
                width,
                height,
                max_pixels,
            } => write!(
                f,
                "a {width}x{height} image exceeds the limit of {max_pixels} pixels"
            ),
            DecodeError::OutputSize { expected, actual } => write!(
                f,
                "the output buffer holds {actual} bytes instead of {expected}"
            ),
            DecodeError::OutOfMemory => f.write_str("not enough memory to decode the image"),
        }
    }
}

impl Error for DecodeError {}

/// Why an image could not be encoded.
///
/// `Eq` is not implemented, since two variants hold an `f32`. The enum and
/// its struct variants are `#[non_exhaustive]`, and the text of
/// [`Display`](fmt::Display) may change, as for [`DecodeError`], which
/// explains how to match.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum EncodeError {
    /// A dimension is outside
    /// [`MIN_ENCODE_DIMENSION`](crate::MIN_ENCODE_DIMENSION) to
    /// [`MAX_DIMENSION`](crate::MAX_DIMENSION): the image is smaller than
    /// the wavelet decomposition needs, or larger than the format can
    /// describe.
    #[non_exhaustive]
    InvalidDimensions {
        /// Requested width.
        width: usize,
        /// Requested height.
        height: usize,
    },
    /// The pixel buffer does not hold `width * height` bytes.
    #[non_exhaustive]
    PixelCountMismatch {
        /// `width * height`.
        expected: usize,
        /// Length of the buffer.
        actual: usize,
    },
    /// The bit rate is not a positive finite number.
    InvalidBitrate(f32),
    /// The bit rate is so low that a quantizer bin width does not fit in
    /// the quantization table.
    BitrateTooLow(f32),
    /// A comment does not fit in one marker segment. Holds its length.
    CommentTooLong(usize),
    /// The resolution is zero.
    InvalidPpi,
    /// An implementation number other than 0 was given with the
    /// [`Optimized`](crate::Profile::Optimized) profile. Only a conformant
    /// stream may carry an implementation number the FBI assigned.
    SoftwareRequiresConformant(u16),
    /// The memory the encoder needs for the image could not be allocated:
    /// its buffers, the wavelet transform's blocks or the compressed output.
    OutOfMemory,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::InvalidDimensions { width, height } => {
                write!(f, "cannot encode a {width}x{height} image")
            }
            EncodeError::PixelCountMismatch { expected, actual } => {
                write!(f, "expected {expected} pixels, got {actual}")
            }
            EncodeError::InvalidBitrate(r) => write!(f, "invalid bit rate {r}"),
            EncodeError::BitrateTooLow(r) => write!(f, "bit rate {r} is too low"),
            EncodeError::InvalidPpi => f.write_str("the resolution cannot be zero"),
            EncodeError::SoftwareRequiresConformant(n) => write!(
                f,
                "implementation number {n} can only be written by the conformant profile"
            ),
            EncodeError::OutOfMemory => f.write_str("not enough memory to encode the image"),
            EncodeError::CommentTooLong(len) => {
                write!(f, "comment of {len} bytes does not fit in a segment")
            }
        }
    }
}

impl Error for EncodeError {}

/// Why a WSQ stream could not be read and decoded.
///
/// [`Io`](Self::Io) describes itself and has the I/O error as its
/// [`source`](Error::source). [`Decode`](Self::Decode) is transparent: it
/// displays as the [`DecodeError`] it holds and reports that error's own
/// source (none), so that a report which walks the chain of sources does not
/// print the same message twice. It is neither `Clone` nor `PartialEq`, since
/// [`std::io::Error`] is neither.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReadError {
    /// Reading failed.
    Io(std::io::Error),
    /// The data read is not a valid WSQ stream.
    Decode(DecodeError),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Io(_) => f.write_str("cannot read WSQ data"),
            ReadError::Decode(e) => e.fmt(f),
        }
    }
}

impl Error for ReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ReadError::Io(e) => Some(e),
            ReadError::Decode(e) => e.source(),
        }
    }
}

impl From<std::io::Error> for ReadError {
    fn from(e: std::io::Error) -> Self {
        ReadError::Io(e)
    }
}

impl From<DecodeError> for ReadError {
    fn from(e: DecodeError) -> Self {
        ReadError::Decode(e)
    }
}

/// Why an image could not be encoded and written.
///
/// Like [`ReadError`]: [`Io`](Self::Io) describes itself and has the I/O
/// error as its source, and [`Encode`](Self::Encode) is transparent.
#[derive(Debug)]
#[non_exhaustive]
pub enum WriteError {
    /// Writing failed.
    Io(std::io::Error),
    /// The image could not be encoded.
    Encode(EncodeError),
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::Io(_) => f.write_str("cannot write WSQ data"),
            WriteError::Encode(e) => e.fmt(f),
        }
    }
}

impl Error for WriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            WriteError::Io(e) => Some(e),
            WriteError::Encode(e) => e.source(),
        }
    }
}

impl From<std::io::Error> for WriteError {
    fn from(e: std::io::Error) -> Self {
        WriteError::Io(e)
    }
}

impl From<EncodeError> for WriteError {
    fn from(e: EncodeError) -> Self {
        WriteError::Encode(e)
    }
}
