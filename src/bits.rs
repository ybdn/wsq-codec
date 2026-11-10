//! Byte- and bit-level reading and writing.
//!
//! Entropy-coded data is a big-endian bit stream in which every `0xFF`
//! byte is followed by a stuffed `0x00`, so that a `0xFF` followed by
//! anything else can only be a marker.

use crate::error::DecodeError;
use crate::mem::{self, OutOfMemory};

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
    /// The bytes not read yet.
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

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
///
/// The output grows fallibly: when the allocator refuses, the writer drops
/// what it is given from then on and [`flush`](Self::flush) reports it, so
/// that the coding loop stays free of error handling.
pub(crate) struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    /// Bits not yet written, in the low `nbits` bits.
    acc: u64,
    nbits: u32,
    /// The output could not grow.
    failed: bool,
}

impl<'a> BitWriter<'a> {
    pub fn new(out: &'a mut Vec<u8>) -> Self {
        BitWriter {
            out,
            acc: 0,
            nbits: 0,
            failed: false,
        }
    }

    /// Room needed for one byte of code: the byte and its stuffing zero.
    const STEP: usize = 2;

    /// Makes room for another [`STEP`](Self::STEP) bytes, doubling the
    /// output as `push` would. Returns whether there is room.
    #[cold]
    fn grow(&mut self) -> bool {
        if mem::grow_output(self.out, Self::STEP.max(self.out.len())).is_err() {
            self.failed = true;
        }
        !self.failed
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
            if self.out.capacity() - self.out.len() < Self::STEP && !self.grow() {
                return;
            }
            self.out.push(byte);
            if byte == 0xFF {
                self.out.push(0);
            }
        }
    }

    /// Pads the last byte with one bits. Fails if the output could not
    /// grow at some point.
    pub fn flush(mut self) -> Result<(), OutOfMemory> {
        if self.nbits > 0 {
            let pad = 8 - self.nbits as u8;
            self.put((1 << pad) - 1, pad);
        }
        if self.failed {
            Err(OutOfMemory)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(f: impl FnOnce(&mut BitWriter)) -> Vec<u8> {
        let mut out = Vec::new();
        let mut w = BitWriter::new(&mut out);
        f(&mut w);
        w.flush().unwrap();
        out
    }

    #[test]
    fn byte_reader_is_big_endian_and_reports_the_end() {
        let mut r = ByteReader::new(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(r.u8().unwrap(), 1);
        assert_eq!(r.u16().unwrap(), 0x0203);
        assert_eq!(r.u32().unwrap(), 0x0405_0607);
        assert_eq!(r.rest(), [8, 9]);
        assert_eq!(r.bytes(2).unwrap(), [8, 9]);
        assert_eq!(r.u8(), Err(DecodeError::UnexpectedEof));
        assert_eq!(r.rest(), []);
    }

    #[test]
    fn byte_reader_refuses_to_read_past_the_end() {
        let mut r = ByteReader::new(&[1, 2, 3]);
        assert_eq!(r.u32(), Err(DecodeError::UnexpectedEof));
        assert_eq!(r.bytes(4), Err(DecodeError::UnexpectedEof));
        assert_eq!(r.bytes(usize::MAX), Err(DecodeError::UnexpectedEof));
        let mut r = ByteReader::new(&[1, 2, 3]);
        assert_eq!(r.bytes(3).unwrap(), [1, 2, 3]);
    }

    #[test]
    fn writer_pads_with_ones_and_stuffs_every_ff() {
        assert_eq!(written(|w| w.put(0, 1)), [0b0111_1111]);
        assert_eq!(written(|_| {}), []);
        assert_eq!(written(|w| w.put(0xFFFF, 16)), [0xFF, 0, 0xFF, 0]);
        // Only the low `size` bits count.
        assert_eq!(written(|w| w.put(0xFFF5, 4)), [0b0101_1111]);
        // A byte made of the end of one code and the start of the next.
        assert_eq!(
            written(|w| {
                w.put(0b1111, 4);
                w.put(0b1111, 4);
            }),
            [0xFF, 0]
        );
    }

    #[test]
    fn codes_of_any_size_round_trip() {
        let codes: Vec<(u16, u8)> = (1..=16u8)
            .cycle()
            .take(100)
            .enumerate()
            .map(|(i, n)| ((i as u16).wrapping_mul(40503) & (u16::MAX >> (16 - n)), n))
            .collect();
        let out = written(|w| codes.iter().for_each(|&(c, n)| w.put(c, n)));
        let mut bytes = ByteReader::new(&out);
        let mut r = bytes.bits();
        for &(c, n) in &codes {
            assert_eq!(r.bits(n).unwrap(), c);
        }
    }

    #[test]
    fn reading_zero_bits_takes_nothing() {
        let mut bytes = ByteReader::new(&[]);
        assert_eq!(bytes.bits().bits(0).unwrap(), 0);
    }

    #[test]
    fn bit_by_bit_matches_whole_values() {
        let mut bytes = ByteReader::new(&[0b1010_0000, 0xFF, 0x00, 0x01]);
        let mut r = bytes.bits();
        let mut got = Vec::new();
        for _ in 0..8 {
            let Bit::Bit(b) = r.bit().unwrap() else {
                panic!("marker")
            };
            got.push(b);
        }
        assert_eq!(got, [1, 0, 1, 0, 0, 0, 0, 0]);
        assert_eq!(r.bits(8).unwrap(), 0xFF);
        assert_eq!(r.bits(8).unwrap(), 0x01);
    }

    #[test]
    fn marker_ends_the_data_on_a_byte_boundary() {
        let mut bytes = ByteReader::new(&[0xAB, 0xFF, 0xD9, 0x12]);
        let mut r = bytes.bits();
        assert_eq!(r.bits(8).unwrap(), 0xAB);
        // The marker is reported again and again, never as data.
        for _ in 0..3 {
            assert!(matches!(r.bit().unwrap(), Bit::Marker(0xFFD9)));
        }
        assert_eq!(r.peek(1), None);
        // The byte reader sits just past the marker.
        assert_eq!(bytes.rest(), [0x12]);
    }

    #[test]
    fn marker_inside_a_value_is_corrupt_data() {
        let mut bytes = ByteReader::new(&[0xAB, 0xFF, 0xD9]);
        let mut r = bytes.bits();
        assert_eq!(
            r.bits(12),
            Err(DecodeError::CorruptData("marker inside a value"))
        );
    }

    #[test]
    fn truncated_data_is_an_unexpected_end() {
        let mut bytes = ByteReader::new(&[0xAB]);
        let mut r = bytes.bits();
        assert_eq!(r.bits(9), Err(DecodeError::UnexpectedEof));
        let mut bytes = ByteReader::new(&[]);
        assert_eq!(bytes.bits().bit().err(), Some(DecodeError::UnexpectedEof));
        // A lone 0xFF has no second byte to tell stuffing from a marker.
        let mut bytes = ByteReader::new(&[0xFF]);
        assert_eq!(bytes.bits().bit().err(), Some(DecodeError::UnexpectedEof));
        let mut bytes = ByteReader::new(&[0xFF]);
        assert_eq!(bytes.bits().bits(8), Err(DecodeError::UnexpectedEof));
    }

    #[test]
    fn peek_leaves_the_bits_to_be_consumed() {
        let mut bytes = ByteReader::new(&[0b1100_1010, 0x0F]);
        let mut r = bytes.bits();
        assert_eq!(r.peek(4), Some(0b1100));
        assert_eq!(r.peek(12), Some(0b1100_1010_0000));
        r.consume(4);
        assert_eq!(r.peek(4), Some(0b1010));
        assert_eq!(r.peek(13), None);
    }

    #[test]
    fn stuffing_round_trips() {
        let mut out = Vec::new();
        let mut w = BitWriter::new(&mut out);
        w.put(0xFF, 8);
        w.put(0b101, 3);
        w.flush().unwrap();
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
