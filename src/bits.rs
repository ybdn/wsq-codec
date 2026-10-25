//! Byte- and bit-level reading and writing.
//!
//! Entropy-coded data is a big-endian bit stream in which every `0xFF`
//! byte is followed by a stuffed `0x00`, so that a `0xFF` followed by
//! anything else can only be a marker.

use crate::error::DecodeError;

/// Big-endian reader over marker segments.
pub(crate) struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        ByteReader { data, pos: 0 }
    }

    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        let b = *self.data.get(self.pos).ok_or(DecodeError::UnexpectedEof)?;
        self.pos += 1;
        Ok(b)
    }

    pub fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_be_bytes([self.u8()?, self.u8()?]))
    }

    pub fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_be_bytes([
            self.u8()?,
            self.u8()?,
            self.u8()?,
            self.u8()?,
        ]))
    }

    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(DecodeError::UnexpectedEof)?;
        let bytes = self
            .data
            .get(self.pos..end)
            .ok_or(DecodeError::UnexpectedEof)?;
        self.pos = end;
        Ok(bytes)
    }

    /// Starts reading entropy-coded data at the current position.
    pub fn bits(&mut self) -> BitReader<'_, 'a> {
        BitReader {
            bytes: self,
            acc: 0,
            avail: 0,
            end: None,
        }
    }
}

/// What reading the next bit of entropy-coded data found.
pub(crate) enum Bit {
    Bit(u16),
    /// The data ended with this marker, which has been consumed.
    Marker(u16),
}

/// Why the reader stopped taking bytes in.
#[derive(Clone, Copy)]
enum End {
    /// A marker, already consumed from the byte reader.
    Marker(u16),
    /// The end of the stream.
    Eof,
}

/// Reader of entropy-coded data. It advances the underlying byte reader,
/// which ends up just past the marker that ends the data.
///
/// Bytes are taken in ahead of use, up to the marker that ends the data,
/// so that codes can be looked up several bits at a time. The bits come
/// out exactly as one byte at a time would give them.
pub(crate) struct BitReader<'r, 'a> {
    bytes: &'r mut ByteReader<'a>,
    /// Bits not yet read, aligned to the most significant end.
    acc: u64,
    avail: u32,
    end: Option<End>,
}

impl BitReader<'_, '_> {
    /// Takes in whole bytes while they fit and the data goes on.
    fn fill(&mut self) {
        while self.avail <= 56 && self.end.is_none() {
            let Ok(b) = self.bytes.u8() else {
                self.end = Some(End::Eof);
                break;
            };
            if b == 0xFF {
                match self.bytes.u8() {
                    Ok(0x00) => {}
                    Ok(m) => {
                        self.end = Some(End::Marker(0xFF00 | u16::from(m)));
                        break;
                    }
                    Err(_) => {
                        self.end = Some(End::Eof);
                        break;
                    }
                }
            }
            self.acc |= u64::from(b) << (56 - self.avail);
            self.avail += 8;
        }
    }

    /// Returns the next `n` bits (1 to 16) without reading them, if the
    /// data holds that many before its end.
    pub fn peek(&mut self, n: u32) -> Option<u16> {
        if self.avail < n {
            self.fill();
            if self.avail < n {
                return None;
            }
        }
        Some((self.acc >> (64 - n)) as u16)
    }

    /// Skips `n` bits, which [`peek`](Self::peek) has returned.
    pub fn consume(&mut self, n: u32) {
        debug_assert!(n <= self.avail);
        self.acc <<= n;
        self.avail -= n;
    }

    /// Reads one bit. A marker can only end the data on a byte boundary,
    /// and only where a new code starts or continues.
    pub fn bit(&mut self) -> Result<Bit, DecodeError> {
        match self.peek(1) {
            Some(b) => {
                self.consume(1);
                Ok(Bit::Bit(b))
            }
            None => match self.end {
                Some(End::Marker(m)) => Ok(Bit::Marker(m)),
                _ => Err(DecodeError::UnexpectedEof),
            },
        }
    }

    /// Reads `n` bits (at most 16) of raw data, which a marker may not
    /// interrupt.
    pub fn bits(&mut self, n: u8) -> Result<u16, DecodeError> {
        if n == 0 {
            return Ok(0);
        }
        if let Some(v) = self.peek(u32::from(n)) {
            self.consume(u32::from(n));
            return Ok(v);
        }
        match self.end {
            Some(End::Marker(_)) => Err(DecodeError::CorruptData("marker inside a value")),
            _ => Err(DecodeError::UnexpectedEof),
        }
    }
}

/// Big-endian writer of entropy-coded data with byte stuffing.
pub(crate) struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    /// Bits not yet written, in the low `nbits` bits.
    acc: u64,
    nbits: u32,
}

impl<'a> BitWriter<'a> {
    pub fn new(out: &'a mut Vec<u8>) -> Self {
        BitWriter {
            out,
            acc: 0,
            nbits: 0,
        }
    }

    /// Writes the low `size` bits of `code` (at most 16), most significant
    /// first.
    pub fn put(&mut self, code: u16, size: u8) {
        let size = u32::from(size);
        let bits = u64::from(code) & ((1 << size) - 1);
        self.acc = (self.acc << size) | bits;
        self.nbits += size;
        while self.nbits >= 8 {
            self.nbits -= 8;
            let byte = (self.acc >> self.nbits) as u8;
            self.out.push(byte);
            if byte == 0xFF {
                self.out.push(0);
            }
        }
    }

    /// Pads the last byte with one bits.
    pub fn flush(mut self) {
        if self.nbits > 0 {
            let pad = 8 - self.nbits as u8;
            self.put((1 << pad) - 1, pad);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stuffing_round_trips() {
        let mut out = Vec::new();
        let mut w = BitWriter::new(&mut out);
        w.put(0xFF, 8);
        w.put(0b101, 3);
        w.flush();
        out.extend_from_slice(&[0xFF, 0xA1]);
        assert_eq!(out, [0xFF, 0x00, 0b1011_1111, 0xFF, 0xA1]);

        let mut bytes = ByteReader::new(&out);
        let mut r = bytes.bits();
        assert_eq!(r.bits(8).unwrap(), 0xFF);
        assert_eq!(r.bits(3).unwrap(), 0b101);
        assert_eq!(r.bits(5).unwrap(), 0b11111);
        assert!(matches!(r.bit().unwrap(), Bit::Marker(0xFFA1)));
    }
}
