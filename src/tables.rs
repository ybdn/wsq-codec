//! Marker segments: reading them from a stream and writing them.

use crate::bits::ByteReader;
use crate::error::{DecodeError, EncodeError};
use crate::huffman::{HuffmanSpec, MAX_HUFFBITS};
use crate::quant::{QuantParams, QuantTable};
use crate::tree::{NUM_SUBBANDS, Q_TREE_LEN};

pub(crate) const SOI: u16 = 0xFFA0;
pub(crate) const EOI: u16 = 0xFFA1;
pub(crate) const SOF: u16 = 0xFFA2;
pub(crate) const SOB: u16 = 0xFFA3;
pub(crate) const DTT: u16 = 0xFFA4;
pub(crate) const DQT: u16 = 0xFFA5;
pub(crate) const DHT: u16 = 0xFFA6;
pub(crate) const COM: u16 = 0xFFA8;

/// Number of Huffman table slots.
pub(crate) const MAX_DHT_TABLES: usize = 8;
/// Most symbols a Huffman table may list.
const MAX_HUFFVALUES: usize = 257;

fn invalid(marker: u16, reason: &'static str) -> DecodeError {
    DecodeError::InvalidSegment { marker, reason }
}

/// Divides `v` by ten `scale` times, rounding to single precision after
/// each step.
fn descale(v: f32, scale: u8) -> f32 {
    (0..scale).fold(v, |v, _| (f64::from(v) / 10.0) as f32)
}

fn scaled_u16(r: &mut ByteReader<'_>) -> Result<f32, DecodeError> {
    let scale = r.u8()?;
    Ok(descale(f32::from(r.u16()?), scale))
}

/// Inverse of [`scaled_u16`]: `v` as a decimal exponent and the 16-bit
/// mantissa with the most digits. `None` if `v` does not fit.
fn to_scaled_u16(v: f32) -> Option<(u8, u16)> {
    if v == 0.0 {
        return Some((0, 0));
    }
    if v.is_nan() || v >= 65535.0 {
        return None;
    }
    let mut t = v;
    let mut scale = 0u8;
    while t < 65535.0 {
        scale += 1;
        t *= 10.0;
    }
    Some((scale - 1, (f64::from(t) / 10.0 + 0.5) as u16))
}

/// Synthesis filters read from a transform table.
#[derive(Clone, Debug)]
pub(crate) struct TransformTable {
    pub lo: Vec<f32>,
    pub hi: Vec<f32>,
}

/// Reads a transform table segment (after its marker).
///
/// The table lists half of each symmetric analysis filter. The synthesis
/// high-pass filter is the analysis low-pass one with every other sign
/// flipped, and the synthesis low-pass filter likewise comes from the
/// analysis high-pass one.
pub(crate) fn read_transform_table(r: &mut ByteReader<'_>) -> Result<TransformTable, DecodeError> {
    // The length is not checked: some encoders write a wrong one.
    let _len = r.u16()?;
    let hisz = usize::from(r.u8()?);
    let losz = usize::from(r.u8()?);
    if !(2..=64).contains(&hisz) || !(2..=64).contains(&losz) {
        return Err(invalid(DTT, "unsupported filter length"));
    }
    let mut coef = || -> Result<f32, DecodeError> {
        let sign = r.u8()?;
        let scale = r.u8()?;
        let v = descale(r.u32()? as f32, scale);
        Ok(if sign != 0 { -v } else { v })
    };
    let alternate = |i: usize| if i % 2 == 0 { 1.0f32 } else { -1.0 };

    let mut hi = vec![0.0f32; hisz];
    let a = hisz.div_ceil(2) - 1;
    for cnt in 0..=a {
        let v = alternate(cnt) * coef()?;
        if hisz % 2 == 1 {
            hi[cnt + a] = v;
            hi[a - cnt] = v;
        } else {
            hi[cnt + a + 1] = v;
            hi[a - cnt] = -v;
        }
    }
    let mut lo = vec![0.0f32; losz];
    let a = losz.div_ceil(2) - 1;
    for cnt in 0..=a {
        let v = coef()?;
        if losz % 2 == 1 {
            lo[cnt + a] = alternate(cnt) * v;
            lo[a - cnt] = lo[cnt + a];
        } else {
            lo[cnt + a + 1] = alternate(cnt + 1) * v;
            lo[a - cnt] = lo[cnt + a + 1];
        }
    }
    Ok(TransformTable { lo, hi })
}

/// Writes the transform table for the given analysis filters.
pub(crate) fn write_transform_table(out: &mut Vec<u8>, lo: &[f32], hi: &[f32]) {
    let put = |out: &mut Vec<u8>, c: f32| {
        let (sign, mut t) = if c >= 0.0 { (0u8, c) } else { (1u8, -c) };
        let mut scale = 0u8;
        let mantissa = if t == 0.0 {
            0
        } else {
            while f64::from(t) < 4_294_967_295.0 {
                scale += 1;
                t *= 10.0;
            }
            scale -= 1;
            (f64::from(t) / 10.0 + 0.5) as u32
        };
        out.push(sign);
        out.push(scale);
        out.extend_from_slice(&mantissa.to_be_bytes());
    };
    let start = out.len();
    out.extend_from_slice(&DTT.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.push(lo.len() as u8);
    out.push(hi.len() as u8);
    for &c in &lo[lo.len() / 2..] {
        put(out, c);
    }
    for &c in &hi[hi.len() / 2..] {
        put(out, c);
    }
    let len = (out.len() - start - 2) as u16;
    out[start + 2..start + 4].copy_from_slice(&len.to_be_bytes());
}

/// Reads a quantization table segment (after its marker).
pub(crate) fn read_quant_table(r: &mut ByteReader<'_>) -> Result<QuantTable, DecodeError> {
    let _len = r.u16()?;
    let bin_center = scaled_u16(r)?;
    let mut table = QuantTable {
        bin_center,
        q_bin: [0.0; Q_TREE_LEN],
        z_bin: [0.0; Q_TREE_LEN],
    };
    for cnt in 0..Q_TREE_LEN {
        table.q_bin[cnt] = scaled_u16(r)?;
        table.z_bin[cnt] = scaled_u16(r)?;
    }
    Ok(table)
}

/// Writes the quantization table for the given bin widths.
pub(crate) fn write_quant_table(
    out: &mut Vec<u8>,
    params: &QuantParams,
    bitrate: f32,
) -> Result<(), EncodeError> {
    out.extend_from_slice(&DQT.to_be_bytes());
    out.extend_from_slice(&389u16.to_be_bytes());
    // Bin center 0.44.
    out.push(2);
    out.extend_from_slice(&44u16.to_be_bytes());
    for sub in 0..Q_TREE_LEN {
        let (q, z) = if sub < NUM_SUBBANDS && params.qbss[sub] != 0.0 {
            let q = to_scaled_u16(params.qbss[sub]).ok_or(EncodeError::BitrateTooLow(bitrate))?;
            let z = to_scaled_u16(params.qzbs[sub]).ok_or(EncodeError::BitrateTooLow(bitrate))?;
            (q, z)
        } else {
            ((0, 0), (0, 0))
        };
        for (scale, v) in [q, z] {
            out.push(scale);
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
    Ok(())
}

/// Reads a Huffman table segment (after its marker), which may define
/// several tables.
pub(crate) fn read_huffman_tables(
    r: &mut ByteReader<'_>,
    tables: &mut [Option<HuffmanSpec>; MAX_DHT_TABLES],
) -> Result<(), DecodeError> {
    let len = usize::from(r.u16()?);
    let mut left = len.checked_sub(2).ok_or(invalid(DHT, "bad length"))?;
    let mut first = true;
    while first || left > 0 {
        if left < 1 + MAX_HUFFBITS {
            return Err(invalid(DHT, "bad length"));
        }
        let id = usize::from(r.u8()?);
        let slot = tables.get_mut(id).ok_or(invalid(DHT, "bad table id"))?;
        let mut bits = [0u8; MAX_HUFFBITS];
        for b in &mut bits {
            *b = r.u8()?;
        }
        let count: usize = bits.iter().map(|&b| usize::from(b)).sum();
        if count > MAX_HUFFVALUES || 1 + MAX_HUFFBITS + count > left {
            return Err(invalid(DHT, "too many values"));
        }
        let values = r.bytes(count)?.to_vec();
        *slot = Some(HuffmanSpec { bits, values });
        left -= 1 + MAX_HUFFBITS + count;
        first = false;
    }
    Ok(())
}

/// Writes one Huffman table.
pub(crate) fn write_huffman_table(out: &mut Vec<u8>, id: u8, spec: &HuffmanSpec) {
    out.extend_from_slice(&DHT.to_be_bytes());
    let len = 3 + MAX_HUFFBITS + spec.values.len();
    out.extend_from_slice(&(len as u16).to_be_bytes());
    out.push(id);
    out.extend_from_slice(&spec.bits);
    out.extend_from_slice(&spec.values);
}

/// The frame header.
#[derive(Clone, Debug)]
pub(crate) struct FrameHeader {
    pub width: u16,
    pub height: u16,
    pub m_shift: f32,
    pub r_scale: f32,
    pub software: u16,
}

pub(crate) fn read_frame_header(r: &mut ByteReader<'_>) -> Result<FrameHeader, DecodeError> {
    let _len = r.u16()?;
    let _black = r.u8()?;
    let _white = r.u8()?;
    let height = r.u16()?;
    let width = r.u16()?;
    let m_shift = scaled_u16(r)?;
    let r_scale = scaled_u16(r)?;
    let _encoder = r.u8()?;
    let software = r.u16()?;
    Ok(FrameHeader {
        width,
        height,
        m_shift,
        r_scale,
        software,
    })
}

pub(crate) fn write_frame_header(out: &mut Vec<u8>, header: &FrameHeader) {
    out.extend_from_slice(&SOF.to_be_bytes());
    out.extend_from_slice(&17u16.to_be_bytes());
    // Black and white levels.
    out.extend_from_slice(&[0, 255]);
    out.extend_from_slice(&header.height.to_be_bytes());
    out.extend_from_slice(&header.width.to_be_bytes());
    for v in [header.m_shift, header.r_scale] {
        // Both are at most 255, so they always fit.
        let (scale, mantissa) = to_scaled_u16(v).unwrap_or((0, 0));
        out.push(scale);
        out.extend_from_slice(&mantissa.to_be_bytes());
    }
    // Encoder number 2: the encoder the specification describes.
    out.push(2);
    out.extend_from_slice(&header.software.to_be_bytes());
}

/// Reads a block header (after its marker) and returns its Huffman
/// table id.
pub(crate) fn read_block_header(r: &mut ByteReader<'_>) -> Result<u8, DecodeError> {
    let _len = r.u16()?;
    r.u8()
}

pub(crate) fn write_block_header(out: &mut Vec<u8>, table: u8) {
    out.extend_from_slice(&SOB.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes());
    out.push(table);
}

/// Reads a comment segment (after its marker) and returns its payload.
pub(crate) fn read_comment<'a>(r: &mut ByteReader<'a>) -> Result<&'a [u8], DecodeError> {
    let len = usize::from(r.u16()?);
    let len = len.checked_sub(2).ok_or(invalid(COM, "bad length"))?;
    r.bytes(len)
}

pub(crate) fn write_comment(out: &mut Vec<u8>, payload: &[u8]) -> Result<(), EncodeError> {
    let len =
        u16::try_from(payload.len() + 2).map_err(|_| EncodeError::CommentTooLong(payload.len()))?;
    out.extend_from_slice(&COM.to_be_bytes());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wavelet::{HIFILT, LOFILT};

    #[test]
    fn standard_transform_table() {
        let mut out = Vec::new();
        write_transform_table(&mut out, &LOFILT, &HIFILT);
        assert_eq!(out.len(), 2 + 58);
        assert_eq!(&out[..6], &[0xFF, 0xA4, 0, 58, 9, 7]);

        let table = read_transform_table(&mut ByteReader::new(&out[2..])).unwrap();
        // Synthesis filters are the modulated analysis filters.
        assert_eq!(table.lo.len(), 7);
        assert_eq!(table.hi.len(), 9);
        for (i, (&s, &a)) in table.lo.iter().zip(&HIFILT).enumerate() {
            let sign = if (i + 1) % 2 == 0 { 1.0 } else { -1.0 };
            assert!((s - sign * a).abs() < 1e-6, "lo[{i}]");
        }
        for (i, (&s, &a)) in table.hi.iter().zip(&LOFILT).enumerate() {
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            assert!((s - sign * a).abs() < 1e-6, "hi[{i}]");
        }
    }

    #[test]
    fn scaled_values() {
        assert_eq!(to_scaled_u16(0.0), Some((0, 0)));
        assert_eq!(to_scaled_u16(1.5), Some((4, 15000)));
        assert_eq!(to_scaled_u16(123.25), Some((2, 12325)));
        assert_eq!(to_scaled_u16(70000.0), None);
        assert_eq!(descale(12325.0, 2), 123.25);
    }
}
