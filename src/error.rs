use std::fmt;

/// Why a WSQ stream could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The data does not start with the WSQ start-of-image marker.
    NotWsq,
    /// The data ended in the middle of the stream.
    UnexpectedEof,
    /// A marker appeared where the format does not allow it.
    UnexpectedMarker(u16),
    /// A marker segment is malformed.
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
    InvalidDimensions {
        /// Width from the frame header.
        width: usize,
        /// Height from the frame header.
        height: usize,
    },
    /// The entropy-coded data is corrupt.
    CorruptData(&'static str),
    /// The image has more pixels than [`DecodeOptions::max_pixels`]
    /// allows.
    ///
    /// [`DecodeOptions::max_pixels`]: crate::DecodeOptions::max_pixels
    TooLarge {
        /// Width from the frame header.
        width: usize,
        /// Height from the frame header.
        height: usize,
        /// The limit in force.
        max_pixels: usize,
    },
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
        }
    }
}

impl std::error::Error for DecodeError {}

/// Why an image could not be encoded.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum EncodeError {
    /// The image is smaller than the wavelet decomposition needs, or
    /// larger than the format can describe.
    InvalidDimensions {
        /// Requested width.
        width: usize,
        /// Requested height.
        height: usize,
    },
    /// The pixel buffer does not hold `width * height` bytes.
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
    /// A comment does not fit in one marker segment.
    CommentTooLong(usize),
    /// The resolution is zero.
    InvalidPpi,
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
            EncodeError::CommentTooLong(len) => {
                write!(f, "comment of {len} bytes does not fit in a segment")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// Why a WSQ stream could not be read and decoded.
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
            ReadError::Io(e) => write!(f, "cannot read WSQ data: {e}"),
            ReadError::Decode(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadError::Io(e) => Some(e),
            ReadError::Decode(e) => Some(e),
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
            WriteError::Io(e) => write!(f, "cannot write WSQ data: {e}"),
            WriteError::Encode(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WriteError::Io(e) => Some(e),
            WriteError::Encode(e) => Some(e),
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
