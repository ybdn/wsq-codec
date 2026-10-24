//! Huffman coding of quantized coefficients.
//!
//! Coefficients become symbols the way the specification lays out:
//!
//! | symbol     | meaning                                    |
//! |------------|--------------------------------------------|
//! | 1..=100    | run of that many zero coefficients         |
//! | 101, 102   | positive, negative coefficient in 8 bits   |
//! | 103, 104   | positive, negative coefficient in 16 bits  |
//! | 105, 106   | run of zeros whose length follows in 8, 16 bits |
//! | 107..=254  | coefficient equal to `symbol - 180`        |
//!
//! Tables are built from symbol counts as in Annex K of the JPEG standard,
//! with code lengths limited to 16 bits and the all-ones code left unused.

use crate::bits::{Bit, BitReader, BitWriter};
use crate::error::DecodeError;

/// Longest code length.
pub(crate) const MAX_HUFFBITS: usize = 16;
/// Number of symbols, not counting the reserved one.
const MAX_HUFFCOUNTS: usize = 256;
/// Largest coefficient coded by its symbol alone. The smallest is
/// `1 - MAX_COEFF`.
const MAX_COEFF: i32 = 74;
/// Longest zero run coded by its symbol alone.
const MAX_ZRUN: u32 = 100;

/// Calls `f(symbol, extra, extra_bits)` for each symbol of `coeffs`.
fn for_each_symbol(coeffs: &[i16], mut f: impl FnMut(u8, u16, u8)) {
    let coeff = |f: &mut dyn FnMut(u8, u16, u8), pix: i32| {
        if pix > MAX_COEFF {
            if pix > 255 {
                f(103, pix as u16, 16)
            } else {
                f(101, pix as u16, 8)
            }
        } else if pix < 1 - MAX_COEFF {
            if pix < -255 {
                f(104, (-pix) as u16, 16)
            } else {
                f(102, (-pix) as u16, 8)
            }
        } else {
            f((pix + 180) as u8, 0, 0)
        }
    };
    let run = |f: &mut dyn FnMut(u8, u16, u8), len: u32| {
        if len <= MAX_ZRUN {
            f(len as u8, 0, 0)
        } else if len <= 0xFF {
            f(105, len as u16, 8)
        } else {
            f(106, len as u16, 16)
        }
    };

    let mut zeros = 0u32;
    for &pix in coeffs {
        if pix == 0 && zeros < 0xFFFF {
            zeros += 1;
            continue;
        }
        if zeros > 0 {
            run(&mut f, zeros);
            zeros = 0;
        }
        if pix == 0 {
            zeros = 1;
        } else {
            coeff(&mut f, i32::from(pix));
        }
    }
    if zeros > 0 {
        run(&mut f, zeros);
    }
}

/// Adds the symbol counts of `coeffs` to `counts`.
pub(crate) fn count_symbols(coeffs: &[i16], counts: &mut [u32; MAX_HUFFCOUNTS]) {
    for_each_symbol(coeffs, |sym, _, _| counts[usize::from(sym)] += 1);
}

/// A Huffman table as stored in a DHT segment: how many codes have each
/// length, and the symbols in code order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HuffmanSpec {
    pub bits: [u8; MAX_HUFFBITS],
    pub values: Vec<u8>,
}

/// Lengths of the codes of the canonical code for `bits`, in code order.
fn code_sizes(bits: &[u8; MAX_HUFFBITS]) -> Vec<u8> {
    (1..=MAX_HUFFBITS as u8)
        .flat_map(|size| std::iter::repeat_n(size, usize::from(bits[usize::from(size) - 1])))
        .collect()
}

/// The canonical codes for the given lengths, or `None` if the lengths do
/// not form a prefix code.
fn canonical_codes(sizes: &[u8]) -> Option<Vec<u16>> {
    let mut codes = Vec::with_capacity(sizes.len());
    let mut code = 0u32;
    let mut size = sizes.first().copied().unwrap_or(0);
    for &s in sizes {
        while size < s {
            code <<= 1;
            size += 1;
        }
        if code >> s != 0 {
            return None;
        }
        codes.push(code as u16);
        code += 1;
    }
    Some(codes)
}

/// Returns the two least frequent symbols, preferring higher symbols on
/// ties, or `None` for the second one when only one symbol is left.
fn least_frequent(freq: &[u64]) -> (Option<usize>, Option<usize>) {
    let mut v1: Option<usize> = None;
    let mut v2: Option<usize> = None;
    let (mut c1, mut c2) = (0u64, 0u64);
    for (i, &f) in freq.iter().enumerate() {
        if f == 0 {
            continue;
        }
        if v1.is_none() {
            (v1, c1) = (Some(i), f);
            continue;
        }
        if v2.is_none() {
            (v2, c2) = (Some(i), f);
        }
        if c1 < f && c2 < f {
            continue;
        }
        if f < c1 || (f == c1 && Some(i) > v1) {
            (v2, c2) = (v1, c1);
            (v1, c1) = (Some(i), f);
            continue;
        }
        if f < c2 || (f == c2 && Some(i) > v2) {
            (v2, c2) = (Some(i), f);
        }
    }
    (v1, v2)
}

impl HuffmanSpec {
    /// Builds the table for the given symbol counts.
    pub fn from_counts(counts: &[u32; MAX_HUFFCOUNTS]) -> Self {
        // One extra symbol with the lowest count takes the all-ones code
        // of the longest length, which is then left unused.
        let reserved = MAX_HUFFCOUNTS;
        let mut freq: Vec<u64> = counts.iter().map(|&c| u64::from(c)).collect();
        freq.push(1);
        let mut codesize = vec![0usize; MAX_HUFFCOUNTS + 1];
        let mut others: Vec<Option<usize>> = vec![None; MAX_HUFFCOUNTS + 1];
        while let (Some(mut v1), Some(mut v2)) = least_frequent(&freq) {
            freq[v1] += freq[v2];
            freq[v2] = 0;
            codesize[v1] += 1;
            while let Some(next) = others[v1] {
                v1 = next;
                codesize[v1] += 1;
            }
            others[v1] = Some(v2);
            codesize[v2] += 1;
            while let Some(next) = others[v2] {
                v2 = next;
                codesize[v2] += 1;
            }
        }

        let max_size = codesize.iter().copied().max().unwrap_or(0);
        let mut bits = vec![0i64; max_size.max(2 * MAX_HUFFBITS)];
        for &size in &codesize[..MAX_HUFFCOUNTS] {
            if size != 0 {
                bits[size - 1] += 1;
            }
        }
        if codesize[..MAX_HUFFCOUNTS].iter().any(|&s| s > MAX_HUFFBITS) {
            // Annex K.3: shorten codes longer than 16 bits, then drop the
            // reserved code from the longest remaining length.
            bits[codesize[reserved] - 1] += 1;
            for i in (MAX_HUFFBITS..bits.len()).rev() {
                while bits[i] > 0 {
                    let mut j = i - 2;
                    while bits[j] == 0 {
                        j -= 1;
                    }
                    bits[i] -= 2;
                    bits[i - 1] += 1;
                    bits[j + 1] += 2;
                    bits[j] -= 1;
                }
                bits[i] = 0;
            }
            if let Some(last) = bits[..MAX_HUFFBITS].iter().rposition(|&b| b > 0) {
                bits[last] -= 1;
            }
        }

        let mut values = Vec::new();
        for size in 1..=max_size {
            values.extend(
                (0..MAX_HUFFCOUNTS)
                    .filter(|&s| codesize[s] == size)
                    .map(|s| s as u8),
            );
        }
        let mut spec = HuffmanSpec {
            bits: [0; MAX_HUFFBITS],
            values,
        };
        for (dst, &src) in spec.bits.iter_mut().zip(&bits) {
            *dst = src as u8;
        }
        spec
    }
}

/// Code and length for each symbol.
pub(crate) struct EncodeTable {
    codes: [(u16, u8); MAX_HUFFCOUNTS],
}

impl EncodeTable {
    pub fn new(spec: &HuffmanSpec) -> Self {
        let sizes = code_sizes(&spec.bits);
        let codes = canonical_codes(&sizes).expect("tables built from counts are prefix codes");
        let mut table = EncodeTable {
            codes: [(0, 0); MAX_HUFFCOUNTS],
        };
        for ((&sym, &code), &size) in spec.values.iter().zip(&codes).zip(&sizes) {
            table.codes[usize::from(sym)] = (code, size);
        }
        table
    }

    /// Appends the entropy-coded `coeffs`, padded to a byte, to `out`.
    pub fn compress(&self, coeffs: &[i16], out: &mut Vec<u8>) {
        let mut w = BitWriter::new(out);
        for_each_symbol(coeffs, |sym, extra, extra_bits| {
            let (code, size) = self.codes[usize::from(sym)];
            debug_assert!(size > 0, "symbol {sym} has no code");
            w.put(code, size);
            w.put(extra, extra_bits);
        });
        w.flush();
    }
}

/// Lookup tables for decoding, as in Annex F.2.2.3 of the JPEG standard.
pub(crate) struct DecodeTable {
    maxcode: [i32; MAX_HUFFBITS + 1],
    mincode: [i32; MAX_HUFFBITS + 1],
    valptr: [usize; MAX_HUFFBITS + 1],
    values: Vec<u8>,
}

/// One decoded item of entropy-coded data.
pub(crate) enum Item {
    Coeff(i16),
    Zeros(u32),
    /// The data of this block ended with the given marker.
    End(u16),
}

impl DecodeTable {
    /// Checks the table and prepares it for decoding. `None` means the
    /// table is not a usable prefix code.
    pub fn new(spec: &HuffmanSpec) -> Option<Self> {
        let sizes = code_sizes(&spec.bits);
        if sizes.len() > spec.values.len() {
            return None;
        }
        let codes = canonical_codes(&sizes)?;
        // The all-ones codes are reserved.
        if codes
            .iter()
            .zip(&sizes)
            .any(|(&c, &s)| u32::from(c) == (1u32 << s) - 1)
        {
            return None;
        }
        let mut table = DecodeTable {
            maxcode: [-1; MAX_HUFFBITS + 1],
            mincode: [0; MAX_HUFFBITS + 1],
            valptr: [0; MAX_HUFFBITS + 1],
            values: spec.values.clone(),
        };
        let mut k = 0;
        for len in 1..=MAX_HUFFBITS {
            let n = usize::from(spec.bits[len - 1]);
            if n == 0 {
                continue;
            }
            table.valptr[len] = k;
            table.mincode[len] = i32::from(codes[k]);
            table.maxcode[len] = i32::from(codes[k + n - 1]);
            k += n;
        }
        Some(table)
    }

    /// Decodes the next symbol, or returns `Err` with the marker that
    /// ends the block.
    fn symbol(&self, r: &mut BitReader<'_, '_>) -> Result<Result<u8, u16>, DecodeError> {
        let mut code = match r.bit()? {
            Bit::Bit(b) => i32::from(b),
            Bit::Marker(m) => return Ok(Err(m)),
        };
        let mut len = 1;
        while code > self.maxcode[len] {
            len += 1;
            if len > MAX_HUFFBITS {
                return Err(DecodeError::CorruptData("invalid Huffman code"));
            }
            match r.bit()? {
                Bit::Bit(b) => code = (code << 1) | i32::from(b),
                Bit::Marker(m) => return Ok(Err(m)),
            }
        }
        let offset = usize::try_from(code - self.mincode[len])
            .map_err(|_| DecodeError::CorruptData("invalid Huffman code"))?;
        let value = self.values.get(self.valptr[len] + offset);
        Ok(Ok(
            *value.ok_or(DecodeError::CorruptData("invalid Huffman code"))?
        ))
    }

    /// Decodes the next coefficient or run of zeros.
    pub fn item(&self, r: &mut BitReader<'_, '_>) -> Result<Item, DecodeError> {
        let sym = match self.symbol(r)? {
            Ok(sym) => sym,
            Err(marker) => return Ok(Item::End(marker)),
        };
        let item = match sym {
            1..=100 => Item::Zeros(u32::from(sym)),
            101 => Item::Coeff(r.bits(8)? as i16),
            102 => Item::Coeff(-(r.bits(8)? as i16)),
            // Values past `i16::MAX` cannot come from a quantizer; they wrap
            // as they do in the reference implementation.
            103 => Item::Coeff(r.bits(16)? as i16),
            104 => Item::Coeff((r.bits(16)? as i16).wrapping_neg()),
            105 => Item::Zeros(u32::from(r.bits(8)?)),
            106 => Item::Zeros(u32::from(r.bits(16)?)),
            107..=254 => Item::Coeff(i16::from(sym) - 180),
            _ => return Err(DecodeError::CorruptData("invalid Huffman symbol")),
        };
        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bits::ByteReader;

    fn round_trip(coeffs: &[i16]) {
        let mut counts = [0; MAX_HUFFCOUNTS];
        count_symbols(coeffs, &mut counts);
        let spec = HuffmanSpec::from_counts(&counts);
        let mut out = Vec::new();
        EncodeTable::new(&spec).compress(coeffs, &mut out);
        out.extend_from_slice(&[0xFF, 0xA1]);

        let table = DecodeTable::new(&spec).unwrap();
        let mut bytes = ByteReader::new(&out);
        let mut r = bytes.bits();
        let mut decoded = Vec::new();
        loop {
            match table.item(&mut r).unwrap() {
                Item::Coeff(c) => decoded.push(c),
                Item::Zeros(n) => decoded.extend(std::iter::repeat_n(0, n as usize)),
                Item::End(m) => {
                    assert_eq!(m, 0xFFA1);
                    break;
                }
            }
        }
        assert_eq!(decoded, coeffs);
    }

    #[test]
    fn coefficients_round_trip() {
        round_trip(&[]);
        round_trip(&[0]);
        round_trip(&[5]);
        let mut v: Vec<i16> = vec![
            1,
            -1,
            74,
            -73,
            75,
            -74,
            255,
            -255,
            256,
            -256,
            i16::MAX,
            -i16::MAX,
        ];
        v.extend(std::iter::repeat_n(0, 100));
        v.push(3);
        v.extend(std::iter::repeat_n(0, 101));
        v.push(-3);
        v.extend(std::iter::repeat_n(0, 256));
        v.push(9);
        v.extend(std::iter::repeat_n(0, 70_000));
        round_trip(&v);
    }

    #[test]
    fn long_codes_are_limited_to_16_bits() {
        // Fibonacci counts make a tree 30 levels deep.
        let mut counts = [0u32; MAX_HUFFCOUNTS];
        let (mut a, mut b) = (1u32, 1u32);
        for c in counts.iter_mut().skip(107).take(30) {
            *c = a;
            (a, b) = (b, a + b);
        }
        let spec = HuffmanSpec::from_counts(&counts);
        // The table the reference implementation builds for these counts.
        assert_eq!(spec.bits, [0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 1]);
        assert_eq!(
            spec.values,
            [
                135, 136, 133, 134, 131, 132, 129, 130, 127, 128, 125, 126, 123, 124, 121, 122,
                119, 120, 117, 118, 115, 116, 113, 114, 111, 112, 107, 109, 110, 108
            ]
        );
        assert!(DecodeTable::new(&spec).is_some());
        let coeffs: Vec<i16> = (0..30).map(|i| i - 73).collect();
        round_trip(&coeffs);
    }
}
