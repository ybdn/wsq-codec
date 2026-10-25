use crate::error::EncodeError;
use crate::huffman::{self, EncodeTable, HuffmanSpec};
use crate::quant;
use crate::tables::{self, EOI, FrameHeader, SOI};
use crate::tree::{NUM_SUBBANDS, START_SUBBAND_2, START_SUBBAND_3, Trees};
use crate::wavelet::{self, HIFILT, LOFILT};
use crate::{EncodeOptions, nistcom, par};

/// Smallest width and height the encoder accepts. Below it some subbands
/// are too small for the variance estimate that drives quantization.
pub(crate) const MIN_DIMENSION: usize = 81;

/// Converts pixels to floats centered on zero and scaled to about
/// +/-128, on up to `threads` threads. Returns the shift and scale that
/// undo it.
fn normalize(pixels: &[u8], threads: usize) -> (Vec<f32>, f32, f32) {
    // A histogram gives the sum, the minimum and the maximum in one pass.
    let mut counts = [0u64; 256];
    for &p in pixels {
        counts[usize::from(p)] += 1;
    }
    let sum: u64 = (0..256).map(|v| v as u64 * counts[v]).sum();
    let low = counts.iter().position(|&c| c > 0).unwrap_or(0) as u8;
    let high = counts.iter().rposition(|&c| c > 0).unwrap_or(0) as u8;
    let m_shift = sum as f32 / pixels.len() as f32;
    let low_diff = m_shift - f32::from(low);
    let high_diff = f32::from(high) - m_shift;
    let mut r_scale = if low_diff >= high_diff {
        low_diff
    } else {
        high_diff
    };
    r_scale /= 128.0;
    if r_scale == 0.0 {
        // A flat image: every coefficient is zero whatever the scale.
        r_scale = 1.0;
    }
    let mut data = vec![0.0f32; pixels.len()];
    par::zip_chunks(pixels, &mut data, threads, |pixels, data| {
        for (d, &p) in data.iter_mut().zip(pixels) {
            *d = (f32::from(p) - m_shift) / r_scale;
        }
    });
    (data, m_shift, r_scale)
}

pub(crate) fn encode(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
) -> Result<Vec<u8>, EncodeError> {
    let range = MIN_DIMENSION..=usize::from(u16::MAX);
    if !range.contains(&width) || !range.contains(&height) {
        return Err(EncodeError::InvalidDimensions { width, height });
    }
    if pixels.len() != width * height {
        return Err(EncodeError::PixelCountMismatch {
            expected: width * height,
            actual: pixels.len(),
        });
    }
    if options.ppi == Some(0) {
        return Err(EncodeError::InvalidPpi);
    }
    let bitrate = options.bitrate;
    if !(bitrate.is_finite() && bitrate > 0.0) {
        return Err(EncodeError::InvalidBitrate(bitrate));
    }

    let threads = crate::thread_count(options.threads);
    let (mut data, m_shift, r_scale) = normalize(pixels, threads);
    let trees = Trees::new(width, height);
    wavelet::decompose(&mut data, width, &trees.w, threads);

    let var = quant::variances(&data, width, &trees.q);
    let mut params = quant::bin_widths(&var, bitrate);
    quant::limit_bin_widths(&mut params, &var, &data, width, &trees.q);
    let coeffs = quant::quantize(&data, width, &trees.q, &params, threads);
    drop(data);

    // The coefficients of each block, in subband order.
    let block_len = |subbands: std::ops::Range<usize>| -> usize {
        subbands
            .filter(|&c| params.qbss[c] != 0.0)
            .map(|c| trees.q[c].area())
            .sum()
    };
    let len1 = block_len(0..START_SUBBAND_2);
    let len2 = block_len(START_SUBBAND_2..START_SUBBAND_3);
    let len3 = block_len(START_SUBBAND_3..NUM_SUBBANDS);
    debug_assert_eq!(len1 + len2 + len3, coeffs.len());
    let (block1, rest) = coeffs.split_at(len1);
    let (block2, block3) = rest.split_at(len2);

    let mut out = Vec::new();
    out.extend_from_slice(&SOI.to_be_bytes());
    // The first NISTCOM comment the caller gives is merged into ours.
    let merged = options.comments.iter().position(|c| nistcom::is_nistcom(c));
    let base = merged.map(|i| options.comments[i].as_slice());
    let nist = nistcom::build(width, height, options.ppi, bitrate, base);
    tables::write_comment(&mut out, &nist)?;
    for (i, comment) in options.comments.iter().enumerate() {
        if Some(i) != merged {
            tables::write_comment(&mut out, comment)?;
        }
    }
    tables::write_transform_table(&mut out, &LOFILT, &HIFILT);
    tables::write_quant_table(&mut out, &params, bitrate)?;
    let frame = FrameHeader {
        width: width as u16,
        height: height as u16,
        m_shift,
        r_scale,
        software: options.software,
    };
    tables::write_frame_header(&mut out, &frame);

    // The first block has its own Huffman table; the other two share one.
    let mut counts = [0; 256];
    huffman::count_symbols(block1, &mut counts);
    let spec = HuffmanSpec::from_counts(&counts);
    tables::write_huffman_table(&mut out, 0, &spec);
    tables::write_block_header(&mut out, 0);
    EncodeTable::new(&spec).compress(block1, &mut out);

    let mut counts = [0; 256];
    huffman::count_symbols(block2, &mut counts);
    huffman::count_symbols(block3, &mut counts);
    let spec = HuffmanSpec::from_counts(&counts);
    let table = EncodeTable::new(&spec);
    tables::write_huffman_table(&mut out, 1, &spec);
    tables::write_block_header(&mut out, 1);
    table.compress(block2, &mut out);
    tables::write_block_header(&mut out, 1);
    table.compress(block3, &mut out);

    out.extend_from_slice(&EOI.to_be_bytes());
    Ok(out)
}
