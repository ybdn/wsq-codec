use crate::error::EncodeError;
use crate::huffman::{self, EncodeTable, HuffmanSpec};
use crate::mem::{self, OutOfMemory};
use crate::quant::{self, QuantParams};
use crate::tables::{self, EOI, FrameHeader, SOI};
use crate::tree::{NUM_SUBBANDS, START_SUBBAND_2, START_SUBBAND_3, Trees};
use crate::wavelet::{self, HIFILT, LOFILT, Scratch};
use crate::{EncodeOptions, nistcom, par};

/// More than the bytes of the tables of a stream beside its comments and
/// entropy-coded blocks: the transform, quantization and Huffman tables, the
/// frame header, the block headers and the markers add up to about 1 KB.
const SLACK: usize = 2048;

/// Smallest width and height the encoder accepts. Below it some subbands
/// are too small for the variance estimate that drives quantization.
pub(crate) const MIN_DIMENSION: usize = 81;

/// Converts pixels to floats centered on zero and scaled to about
/// +/-128, on up to `threads` threads. Returns the shift and scale that
/// undo it. `data`, the rows of the image, receives the floats.
fn normalize(pixels: &[u8], data: &mut [&mut [f32]], threads: usize) -> (f32, f32) {
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
    let width = data.first().map_or(1, |row| row.len()).max(1);
    let mut rows: Vec<_> = data.iter_mut().zip(pixels.chunks(width)).collect();
    par::row_groups(&mut rows, pixels.len(), threads, |_, rows| {
        for (data, pixels) in rows.iter_mut() {
            for (d, &p) in data.iter_mut().zip(*pixels) {
                *d = (f32::from(p) - m_shift) / r_scale;
            }
        }
    });
    (m_shift, r_scale)
}

/// What the stream depends on besides the bin widths: the image, transformed
/// and with its variances, and the bit allocation. `data` borrows the
/// scratch buffer that holds the transformed image.
struct Analysis<'a> {
    data: Vec<&'a [f32]>,
    stats: quant::Stats,
    /// The widths of the specification's bit allocation, before they are
    /// held to the quantization table.
    allocated: QuantParams,
    stream: Stream,
}

/// Checks the arguments, transforms the image and allocates the bits.
fn analyze<'a>(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
    scratch: &'a mut Scratch,
) -> Result<Analysis<'a>, EncodeError> {
    let range = MIN_DIMENSION..=crate::MAX_DIMENSION;
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
    let Scratch { image, tmp, keep } = scratch;
    let mut data = image.rows(width, height, threads)?;
    let (m_shift, r_scale) = normalize(pixels, &mut data, threads);
    let trees = Trees::new(width, height);
    let mut rows = tmp.rows(width, height, threads)?;
    wavelet::decompose(&mut data, &mut rows, &trees.w, threads)?;
    drop(rows);
    tmp.release(*keep);

    let data: Vec<&[f32]> = data.into_iter().map(|row| &*row).collect();
    let stats = quant::variances(&data, &trees.q, threads);
    let allocated = quant::bin_widths(&stats.var, bitrate);
    let frame = FrameHeader {
        width: width as u16,
        height: height as u16,
        m_shift,
        r_scale,
        software: options.software,
    };
    Ok(Analysis {
        data,
        stats,
        allocated,
        stream: Stream {
            trees,
            frame,
            bitrate,
            threads,
        },
    })
}

/// Encodes an image with the bit allocation of the specification: the
/// conformant stream.
pub(crate) fn encode(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
    scratch: &mut Scratch,
) -> Result<Vec<u8>, EncodeError> {
    let Analysis {
        data,
        stats,
        allocated,
        stream,
    } = analyze(pixels, width, height, options, scratch)?;
    let mut params = allocated;
    quant::limit_bin_widths(&mut params, &stats);
    let coeffs = quant::quantize(&data, &stream.trees.q, &params, stream.threads)?;
    drop(data);
    scratch.image.release(scratch.keep);
    let mut out = header(width, height, options)?;
    stream.write(&mut out, &coeffs, &params)?;
    Ok(out)
}

/// An image analyzed once, from which streams are written with the bin
/// widths of the bit allocation multiplied by a factor. This is the one
/// seam through which `wsq-codec` builds its optimized profile on this
/// crate's code (see `docs/certification.md`); a factor of 1 gives the
/// conformant stream, byte for byte.
pub struct Scaler<'a> {
    analysis: Analysis<'a>,
    /// The start of every stream: start of image, comments and transform
    /// table.
    head: Vec<u8>,
}

impl Scaler<'_> {
    /// Writes the stream whose bin widths are those of the bit allocation
    /// multiplied by `factor` (zero bins in the same ratio), held to the
    /// range of the quantization table.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::BitrateTooLow`] if the widths do not fit the
    /// quantization table, and [`EncodeError::OutOfMemory`] if a buffer
    /// cannot be allocated.
    pub fn write(&self, factor: f32) -> Result<Vec<u8>, EncodeError> {
        let analysis = &self.analysis;
        let params = scaled(&analysis.allocated, &analysis.stats, factor);
        let coeffs = quant::quantize(
            &analysis.data,
            &analysis.stream.trees.q,
            &params,
            analysis.stream.threads,
        )?;
        let mut out = mem::with_capacity(self.head.len())?;
        out.extend_from_slice(&self.head);
        analysis.stream.write(&mut out, &coeffs, &params)?;
        Ok(out)
    }
}

/// Analyzes an image as [`encode`] does and hands the result to `f`, which
/// writes streams from it; the buffers are released when `f` returns.
pub(crate) fn with_scaler<T>(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
    scratch: &mut Scratch,
    f: impl FnOnce(&Scaler<'_>) -> Result<T, EncodeError>,
) -> Result<T, EncodeError> {
    let analysis = analyze(pixels, width, height, options, scratch)?;
    let head = header(width, height, options)?;
    let scaler = Scaler { analysis, head };
    let out = f(&scaler);
    drop(scaler);
    scratch.image.release(scratch.keep);
    out
}

/// The start of a stream, the same whatever the bin widths: start of image,
/// comments and transform table.
fn header(width: usize, height: usize, options: &EncodeOptions) -> Result<Vec<u8>, EncodeError> {
    // The first NISTCOM comment the caller gives is merged into ours.
    let merged = options.comments.iter().position(|c| nistcom::is_nistcom(c));
    let base = merged.map(|i| options.comments[i].as_slice());
    let nist = nistcom::build(width, height, options.ppi, options.bitrate, base);
    // Start of image, a comment segment per comment, the transform table.
    let comments: usize = options.comments.iter().map(|c| c.len() + 4).sum();
    let mut out = Vec::new();
    mem::reserve(&mut out, SLACK + nist.len() + comments)?;
    out.extend_from_slice(&SOI.to_be_bytes());
    tables::write_comment(&mut out, &nist)?;
    for (i, comment) in options.comments.iter().enumerate() {
        if Some(i) != merged {
            tables::write_comment(&mut out, comment)?;
        }
    }
    tables::write_transform_table(&mut out, &LOFILT, &HIFILT);
    Ok(out)
}

/// What the rest of a stream depends on besides the bin widths.
struct Stream {
    trees: Trees,
    frame: FrameHeader,
    bitrate: f32,
    threads: usize,
}

impl Stream {
    /// Appends the quantization table, the frame header, the Huffman tables
    /// and blocks of `coeffs`, and the end of image to `out`.
    fn write(
        &self,
        out: &mut Vec<u8>,
        coeffs: &[i16],
        params: &QuantParams,
    ) -> Result<(), EncodeError> {
        let trees = &self.trees;
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

        tables::write_quant_table(out, params, self.bitrate)?;
        tables::write_frame_header(out, &self.frame);

        // The first block has its own Huffman table; the other two share one.
        // Counting the symbols and coding the blocks are independent per block,
        // so they can go to separate threads: each block ends on a byte
        // boundary, and the streams join up unchanged.
        let parallel = par::parts(self.threads, coeffs.len()) > 1;
        let count = |block: &[i16]| {
            let mut counts = [0; 256];
            huffman::count_symbols(block, &mut counts);
            counts
        };
        let (counts1, counts2, counts3) = par::run3(
            parallel,
            || count(block1),
            || count(block2),
            || count(block3),
        );
        let spec1 = HuffmanSpec::from_counts(&counts1);
        let mut counts = counts2;
        for (n, m) in counts.iter_mut().zip(counts3) {
            *n += m;
        }
        let spec2 = HuffmanSpec::from_counts(&counts);
        let (table1, table2) = (EncodeTable::new(&spec1), EncodeTable::new(&spec2));
        // Each block gets its share of the stream's expected size, which is
        // the target rate, but never more than a byte per coefficient. The
        // blocks grow past it if they must, fallibly.
        let pixels = usize::from(self.frame.width) * usize::from(self.frame.height);
        let expected = (f64::from(self.bitrate) / 8.0 * pixels as f64).min(coeffs.len() as f64);
        let compress = |table: &EncodeTable, block: &[i16]| -> Result<Vec<u8>, OutOfMemory> {
            let share = expected * block.len() as f64 / coeffs.len().max(1) as f64;
            let mut data = Vec::new();
            mem::grow_output(&mut data, share as usize)?;
            table.compress(block, &mut data)?;
            Ok(data)
        };
        let (data1, data2, data3) = par::run3(
            parallel,
            || compress(&table1, block1),
            || compress(&table2, block2),
            || compress(&table2, block3),
        );
        let (data1, data2, data3) = (data1?, data2?, data3?);
        mem::grow_output(out, SLACK + data1.len() + data2.len() + data3.len())?;

        tables::write_huffman_table(out, 0, &spec1);
        tables::write_block_header(out, 0);
        out.extend_from_slice(&data1);
        tables::write_huffman_table(out, 1, &spec2);
        tables::write_block_header(out, 1);
        out.extend_from_slice(&data2);
        tables::write_block_header(out, 1);
        out.extend_from_slice(&data3);

        out.extend_from_slice(&EOI.to_be_bytes());
        Ok(())
    }
}

/// The bit allocation's bin widths, each multiplied by `factor`, with zero
/// bins in the same ratio, then held to 16-bit indices by
/// [`quant::limit_bin_widths`]. A factor of 1 gives the conformant widths.
fn scaled(allocated: &QuantParams, stats: &quant::Stats, factor: f32) -> QuantParams {
    let mut params = QuantParams {
        qbss: allocated.qbss,
        qzbs: allocated.qzbs,
    };
    if factor != 1.0 {
        for cnt in 0..NUM_SUBBANDS {
            params.qbss[cnt] = allocated.qbss[cnt] * factor;
            params.qzbs[cnt] = (1.2 * f64::from(params.qbss[cnt])) as f32;
        }
    }
    quant::limit_bin_widths(&mut params, stats);
    params
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, read_info};

    /// The `checker` test image of tests/nbis.rs, seeded by its size.
    fn checker(w: usize, h: usize) -> Vec<u8> {
        let mut state = (w * h) as u32;
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let base = if (x / 7 + y / 5) % 2 == 1 { 200 } else { 40 };
                out.push((base + (state >> 24 & 7)) as u8);
            }
        }
        out
    }

    fn enc(w: usize, h: usize, options: &EncodeOptions) -> Result<Vec<u8>, EncodeError> {
        let mut scratch = wavelet::Scratch::default();
        encode(&checker(w, h), w, h, options, &mut scratch)
    }

    #[test]
    fn default_options_give_the_reference_stream() {
        let wsq = enc(101, 97, &EncodeOptions::default()).unwrap();
        assert!(wsq == include_bytes!("../tests/data/checker_101x97.wsq"));
    }

    #[test]
    fn stream_starts_with_the_header_and_ends_with_the_end_of_image() {
        let wsq = enc(MIN_DIMENSION, MIN_DIMENSION, &EncodeOptions::default()).unwrap();
        assert_eq!(wsq[..2], SOI.to_be_bytes());
        assert_eq!(wsq[wsq.len() - 2..], EOI.to_be_bytes());
        let info = read_info(&wsq).unwrap();
        assert_eq!((info.width, info.height), (MIN_DIMENSION, MIN_DIMENSION));
        let image = decode(&wsq).unwrap();
        assert_eq!(image.pixels.len(), MIN_DIMENSION * MIN_DIMENSION);
    }

    #[test]
    fn ppi_software_and_comments_reach_the_stream() {
        let options = EncodeOptions::default()
            .with_ppi(Some(1000))
            .with_software(0x1234)
            .with_comment(b"hello".to_vec());
        let wsq = enc(101, 97, &options).unwrap();
        let info = read_info(&wsq).unwrap();
        assert_eq!(info.ppi, Some(1000));
        assert_eq!(info.software, 0x1234);
        assert!(info.comments.iter().any(|c| c == b"hello"));
        assert_eq!(
            read_info(&enc(101, 97, &EncodeOptions::default()).unwrap())
                .unwrap()
                .ppi,
            None
        );
    }

    #[test]
    fn invalid_arguments_are_refused() {
        let options = EncodeOptions::default();
        let err = |w, h, pixels: usize, o: &EncodeOptions| {
            let mut scratch = wavelet::Scratch::default();
            encode(&vec![0; pixels], w, h, o, &mut scratch).unwrap_err()
        };
        let m = MIN_DIMENSION;
        assert_eq!(
            err(m - 1, m, (m - 1) * m, &options),
            EncodeError::InvalidDimensions {
                width: m - 1,
                height: m
            }
        );
        assert_eq!(
            err(m, m - 1, m * (m - 1), &options),
            EncodeError::InvalidDimensions {
                width: m,
                height: m - 1
            }
        );
        assert_eq!(
            err(m, m, m * m - 1, &options),
            EncodeError::PixelCountMismatch {
                expected: m * m,
                actual: m * m - 1
            }
        );
        assert_eq!(
            err(m, m, m * m, &options.clone().with_ppi(Some(0))),
            EncodeError::InvalidPpi
        );
        for rate in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let got = err(m, m, m * m, &options.clone().with_bitrate(rate));
            assert!(matches!(got, EncodeError::InvalidBitrate(_)), "{rate}");
        }
    }

    #[test]
    fn flat_image_encodes_and_decodes_flat() {
        let (w, h) = (MIN_DIMENSION, MIN_DIMENSION);
        let mut scratch = wavelet::Scratch::default();
        let flat = vec![77u8; w * h];
        let options = EncodeOptions::default();
        let wsq = encode(&flat, w, h, &options, &mut scratch).unwrap();
        assert_eq!(decode(&wsq).unwrap().pixels, flat);
    }

    #[test]
    fn threads_do_not_change_the_stream() {
        let one = enc(101, 97, &EncodeOptions::default()).unwrap();
        let four = enc(101, 97, &EncodeOptions::default().with_threads(4)).unwrap();
        assert!(one == four);
    }
}

#[cfg(test)]
mod alloc_tests {
    use super::{Scaler, with_scaler};
    use crate::mem::inject::{Site, refusing};
    use crate::wavelet::Scratch;
    use crate::{DecodeError, DecodeOptions, EncodeError, EncodeOptions, decode_with, encode};

    /// A synthetic image of integer arithmetic only.
    fn pixels(w: usize, h: usize) -> Vec<u8> {
        (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                (((x * 7 + y * 3) % 61) * 4 + (x * y) % 13) as u8
            })
            .collect()
    }

    /// Refuses the allocations at `site`, after letting 0, 1, 2... of them
    /// through, until `run` succeeds: every refusal must come out as an
    /// error, and the first success must be the unhindered result. Returns
    /// how many allocations `run` makes at `site`.
    fn every_refusal<T: PartialEq + std::fmt::Debug, E: std::fmt::Debug>(
        site: Site,
        expected: &T,
        is_oom: impl Fn(&E) -> bool,
        run: impl Fn() -> Result<T, E>,
    ) -> usize {
        for allowed in 0.. {
            match refusing(site, allowed, &run) {
                Err(e) => assert!(is_oom(&e), "{site:?} after {allowed}: {e:?}"),
                Ok(out) => {
                    assert_eq!(&out, expected);
                    return allowed;
                }
            }
        }
        unreachable!()
    }

    #[test]
    fn encoder_output_allocations_are_fallible() {
        let (w, h) = (96, 90);
        let px = pixels(w, h);
        let options = EncodeOptions::default().with_comment(b"a comment".to_vec());
        let expected = encode(&px, w, h, &options).unwrap();
        let count = every_refusal(
            Site::EncodeOutput,
            &expected,
            |e| *e == EncodeError::OutOfMemory,
            || encode(&px, w, h, &options),
        );
        // The stream, and each of its three blocks; more as they grow.
        assert!(count >= 4, "{count}");
    }

    #[test]
    fn scaled_output_allocations_are_fallible() {
        let (w, h) = (96, 90);
        let px = pixels(w, h);
        let options = EncodeOptions::default();
        let expected = encode(&px, w, h, &options).unwrap();
        let run = || {
            let write = |scaler: &Scaler<'_>| scaler.write(1.0);
            with_scaler(&px, w, h, &options, &mut Scratch::default(), write)
        };
        // A factor of 1 is the conformant stream.
        assert_eq!(run().unwrap(), expected);
        let count = every_refusal(
            Site::EncodeOutput,
            &expected,
            |e| *e == EncodeError::OutOfMemory,
            run,
        );
        assert!(count >= 4, "{count}");
    }

    #[test]
    fn transposition_allocations_are_fallible_on_one_thread() {
        let (w, h) = (96, 90);
        let px = pixels(w, h);
        let options = EncodeOptions::default();
        let wsq = encode(&px, w, h, &options).unwrap();
        let image = crate::decode(&wsq).unwrap();
        let count = every_refusal(
            Site::Transposition,
            &wsq,
            |e| *e == EncodeError::OutOfMemory,
            || encode(&px, w, h, &options),
        );
        assert!(count > 2, "{count}");
        every_refusal(
            Site::Transposition,
            &image,
            |e| *e == DecodeError::OutOfMemory,
            || decode_with(&wsq, &DecodeOptions::default()),
        );
    }

    #[test]
    fn transposition_allocations_are_fallible_on_several_threads() {
        // Large enough for the transform to split its passes over threads.
        let (w, h) = (512, 400);
        let px = pixels(w, h);
        let options = EncodeOptions::default().with_threads(4);
        let wsq = encode(&px, w, h, &options).unwrap();
        let decode_options = DecodeOptions::default().with_threads(4);
        let image = decode_with(&wsq, &decode_options).unwrap();
        // Refusing from the start, or after any number of blocks, in the
        // middle of a pass or between two: the threads must all get out.
        // Where threads cannot be created (WASI), the work runs on the calling
        // thread and makes fewer allocations: a refusal point past the last
        // one lets the call succeed, which must then give the usual output.
        let threads = !cfg!(target_os = "wasi");
        for allowed in [0, 1, 2, 7, 20, 41] {
            match refusing(Site::Transposition, allowed, || encode(&px, w, h, &options)) {
                Ok(out) if !threads && allowed > 2 => assert_eq!(out, wsq),
                out => assert_eq!(out, Err(EncodeError::OutOfMemory), "encode after {allowed}"),
            }
            match refusing(Site::Transposition, allowed, || {
                decode_with(&wsq, &decode_options)
            }) {
                Ok(out) if !threads && allowed > 2 => assert_eq!(out, image),
                out => assert_eq!(out, Err(DecodeError::OutOfMemory), "decode after {allowed}"),
            }
        }
        assert_eq!(encode(&px, w, h, &options).unwrap(), wsq);
        assert_eq!(decode_with(&wsq, &decode_options).unwrap(), image);
    }
}
