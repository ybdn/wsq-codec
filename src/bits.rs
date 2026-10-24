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
            cur: 0,
            avail: 0,
        }
    }
}

/// What reading the next bit of entropy-coded data found.
pub(crate) enum Bit {
    Bit(u16),
    /// The data ended with this marker, which has been consumed.
    Marker(u16),
}

/// Reader of entropy-coded data. It advances the underlying byte reader,
/// which ends up just past the marker that ends the data.
pub(crate) struct BitReader<'r, 'a> {
    bytes: &'r mut ByteReader<'a>,
    cur: u8,
    avail: u8,
}

impl BitReader<'_, '_> {
    /// Reads one bit. A marker can only end the data on a byte boundary,
    /// and only where a new code starts or continues.
    pub fn bit(&mut self) -> Result<Bit, DecodeError> {
        if self.avail == 0 {
            let b = self.bytes.u8()?;
            if b == 0xFF {
                match self.bytes.u8()? {
                    0x00 => {}
                    m => return Ok(Bit::Marker(0xFF00 | u16::from(m))),
                }
            }
            self.cur = b;
            self.avail = 8;
        }
        self.avail -= 1;
        Ok(Bit::Bit(u16::from((self.cur >> self.avail) & 1)))
    }

    /// Reads `n` bits (at most 16) of raw data, which a marker may not
    /// interrupt.
    pub fn bits(&mut self, n: u8) -> Result<u16, DecodeError> {
        let mut v = 0u16;
        for _ in 0..n {
            match self.bit()? {
                Bit::Bit(b) => v = (v << 1) | b,
                Bit::Marker(_) => return Err(DecodeError::CorruptData("marker inside a value")),
            }
        }
        Ok(v)
    }
}

/// Big-endian writer of entropy-coded data with byte stuffing.
pub(crate) struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    acc: u8,
    nbits: u8,
}

impl<'a> BitWriter<'a> {
    pub fn new(out: &'a mut Vec<u8>) -> Self {
        BitWriter {
            out,
            acc: 0,
            nbits: 0,
        }
    }

    /// Writes the low `size` bits of `code`, most significant first.
    pub fn put(&mut self, code: u16, size: u8) {
        for i in (0..size).rev() {
            self.acc = (self.acc << 1) | ((code >> i) & 1) as u8;
            self.nbits += 1;
            if self.nbits == 8 {
                self.emit();
            }
        }
    }

    /// Pads the last byte with one bits.
    pub fn flush(mut self) {
        if self.nbits > 0 {
            while self.nbits < 8 {
                self.acc = (self.acc << 1) | 1;
                self.nbits += 1;
            }
            self.emit();
        }
    }

    fn emit(&mut self) {
        self.out.push(self.acc);
        if self.acc == 0xFF {
            self.out.push(0);
        }
        self.acc = 0;
        self.nbits = 0;
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
