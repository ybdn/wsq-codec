//! The parameters and quantized coefficients a WSQ stream carries.
//!
//! [`read_quantized`] stops decoding before dequantization and the inverse
//! wavelet transform, and returns what the encoder decided: the header
//! parameters, the bin widths of each subband and the bin index of every
//! coefficient. These are the values the FBI's certification of an
//! encoder compares with NIST's reference streams, besides the file size,
//! and what tools comparing two encoders need.

use crate::decode::{self, Quantized as Raw};
use crate::mem;
use crate::tree::NUM_SUBBANDS;
use crate::{DecodeError, DecodeOptions, Info};

/// The header parameters and quantized coefficients of a stream.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Quantized {
    /// Dimensions, resolution, encoder number and comments.
    pub info: Info,
    /// Pixel shift `M` of the frame header, subtracted from each pixel
    /// before the transform.
    pub m_shift: f32,
    /// Pixel scale `R` of the frame header, by which shifted pixels are
    /// divided before the transform.
    pub r_scale: f32,
    /// Bin center `C` of the quantization table.
    pub bin_center: f32,
    /// Synthesis low-pass filter, as the decoder derives it from the
    /// transform table.
    pub synthesis_lowpass: Vec<f32>,
    /// Synthesis high-pass filter, as the decoder derives it from the
    /// transform table.
    pub synthesis_highpass: Vec<f32>,
    /// The 60 subbands, in the order of the specification.
    pub subbands: Vec<Subband>,
}

/// One subband: its place in the transformed image, its bin widths and the
/// bin indices of its coefficients.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Subband {
    /// Column of the subband's top left corner in the transformed image.
    pub x: usize,
    /// Row of the subband's top left corner in the transformed image.
    pub y: usize,
    /// Width in coefficients.
    pub width: usize,
    /// Height in coefficients.
    pub height: usize,
    /// Quantizer bin width `Q`; zero when the encoder dropped the subband.
    pub q_bin: f32,
    /// Zero bin width `Z`.
    pub z_bin: f32,
    /// Bin indices, row by row; empty when the subband was dropped.
    pub coefficients: Vec<i16>,
}

/// Reads a stream's parameters and quantized coefficients, with the same
/// checks and pixel limit as [`decode_with`](crate::decode_with).
///
/// # Errors
///
/// Returns the [`DecodeError`] that [`decode_with`](crate::decode_with) gives for the same data.
pub fn read_quantized(data: &[u8], options: &DecodeOptions) -> Result<Quantized, DecodeError> {
    let Raw {
        info,
        frame,
        transform,
        quant,
        trees,
        coeffs,
    } = decode::read_quantized(data, options)?;
    let mut rest = coeffs.as_slice();
    let subbands = trees.q[..NUM_SUBBANDS]
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let q_bin = quant.q_bin[i];
            let n = if q_bin == 0.0 { 0 } else { node.area() };
            let (coefficients, tail) = rest.split_at(n);
            rest = tail;
            let mut copy = mem::with_capacity(n)?;
            copy.extend_from_slice(coefficients);
            Ok(Subband {
                x: node.x,
                y: node.y,
                width: node.lenx,
                height: node.leny,
                q_bin,
                z_bin: quant.z_bin[i],
                coefficients: copy,
            })
        })
        .collect::<Result<_, DecodeError>>()?;
    Ok(Quantized {
        info,
        m_shift: frame.m_shift,
        r_scale: frame.r_scale,
        bin_center: quant.bin_center,
        synthesis_lowpass: transform.lo,
        synthesis_highpass: transform.hi,
        subbands,
    })
}
