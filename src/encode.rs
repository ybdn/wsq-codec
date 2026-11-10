use crate::error::EncodeError;
use crate::huffman::{self, EncodeTable, HuffmanSpec};
use crate::mem::{self, OutOfMemory};
use crate::quant::{self, QuantParams};
use crate::tables::{self, EOI, FrameHeader, SOI};
use crate::tree::{NUM_SUBBANDS, START_SUBBAND_2, START_SUBBAND_3, Trees};
use crate::wavelet::{self, HIFILT, LOFILT};
use crate::{EncodeOptions, Profile, math, nistcom, par};

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

/// Share of the target size the [`Profile::Optimized`] search may fall short
/// by: it stops on the first stream between `1 - HIT_TOLERANCE` times the
/// target and the target.
pub(crate) const HIT_TOLERANCE: f64 = 0.005;
/// Most streams the [`Profile::Optimized`] search writes before keeping the best.
const HIT_MAX_TRIES: usize = 40;
/// Bound on the base-2 logarithm of the scale factor.
const HIT_LOG2_RANGE: f64 = 40.0;
/// Bracket width, on that logarithm, below which the search stops: about
/// the resolution of an `f32` factor.
const HIT_MIN_GAP: f64 = 1e-7;

pub(crate) fn encode(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: &EncodeOptions,
    scratch: &mut wavelet::Scratch,
) -> Result<Vec<u8>, EncodeError> {
    // Only a conformant stream may carry the FBI's implementation number.
    if options.profile == Profile::Optimized && options.software != 0 {
        return Err(EncodeError::SoftwareRequiresConformant(options.software));
    }
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
    let mut data = scratch.image.rows(width, height, threads)?;
    let (m_shift, r_scale) = normalize(pixels, &mut data, threads);
    let trees = Trees::new(width, height);
    let mut tmp = scratch.tmp.rows(width, height, threads)?;
    wavelet::decompose(&mut data, &mut tmp, &trees.w, threads)?;
    drop(tmp);
    scratch.tmp.release(scratch.keep);

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
    let stream = Stream {
        trees: &trees,
        frame: &frame,
        bitrate,
        threads,
    };
    match options.profile {
        Profile::Conformant => {
            let mut params = allocated;
            quant::limit_bin_widths(&mut params, &stats);
            let coeffs = quant::quantize(&data, &trees.q, &params, threads)?;
            drop(data);
            scratch.image.release(scratch.keep);
            let mut out = header(width, height, options)?;
            stream.write(&mut out, &coeffs, &params)?;
            Ok(out)
        }
        Profile::Optimized => {
            let head = header(width, height, options)?;
            let target = f64::from(bitrate) * (width * height) as f64 / 8.0;
            let out = hit_rate(&data, &stats, &allocated, &head, &stream, target);
            drop(data);
            scratch.image.release(scratch.keep);
            out
        }
    }
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
struct Stream<'a> {
    trees: &'a Trees,
    frame: &'a FrameHeader,
    bitrate: f32,
    threads: usize,
}

impl Stream<'_> {
    /// Appends the quantization table, the frame header, the Huffman tables
    /// and blocks of `coeffs`, and the end of image to `out`.
    fn write(
        &self,
        out: &mut Vec<u8>,
        coeffs: &[i16],
        params: &QuantParams,
    ) -> Result<(), EncodeError> {
        let trees = self.trees;
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
        tables::write_frame_header(out, self.frame);

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

/// State of the [`Profile::Optimized`] search.
struct Search<'a> {
    data: &'a [&'a [f32]],
    stats: &'a quant::Stats,
    allocated: &'a QuantParams,
    head: &'a [u8],
    stream: &'a Stream<'a>,
    target: f64,
    /// Longest stream that fits the target.
    fits: Option<Vec<u8>>,
    /// Shortest stream that does not.
    over: Option<Vec<u8>>,
    /// Why widths did not fit the quantization table, the first time.
    error: Option<EncodeError>,
    tries: usize,
}

impl Search<'_> {
    /// Writes the stream for `2^x` times the bit allocation's widths and
    /// returns how far its length is from the middle of the accepted range,
    /// as a difference of logarithms: positive if too long, negative if too
    /// short, minus infinity if the widths do not fit the quantization
    /// table (they are then too wide).
    fn try_scale(&mut self, x: f64) -> Result<f64, EncodeError> {
        self.tries += 1;
        let params = scaled(self.allocated, self.stats, math::pow(2.0, x) as f32);
        let coeffs = quant::quantize(
            self.data,
            &self.stream.trees.q,
            &params,
            self.stream.threads,
        )?;
        let mut out = mem::with_capacity(self.head.len())?;
        out.extend_from_slice(self.head);
        match self.stream.write(&mut out, &coeffs, &params) {
            Ok(()) => {}
            Err(e @ EncodeError::BitrateTooLow(_)) => {
                self.error.get_or_insert(e);
                return Ok(f64::NEG_INFINITY);
            }
            Err(e) => return Err(e),
        }
        let len = out.len() as f64;
        if len <= self.target {
            if self.fits.as_ref().is_none_or(|f| f.len() < out.len()) {
                self.fits = Some(out);
            }
        } else if self.over.as_ref().is_none_or(|o| o.len() > out.len()) {
            self.over = Some(out);
        }
        Ok(math::ln(len) - math::ln(self.target * (1.0 - HIT_TOLERANCE / 2.0)))
    }

    /// Whether a stream within the accepted range has been written, or the
    /// search has run out of tries.
    fn finished(&self) -> bool {
        self.tries >= HIT_MAX_TRIES
            || self
                .fits
                .as_ref()
                .is_some_and(|f| f.len() as f64 >= self.target * (1.0 - HIT_TOLERANCE))
    }
}

/// The [`Profile::Optimized`] search: the stream whose total length comes closest to
/// `target` bytes without exceeding it, found by scaling every bin width of
/// the bit allocation by one factor.
///
/// The length shrinks, in steps, as the factor grows. The search runs on
/// the base-2 logarithm of the factor, starting from the conformant widths
/// (factor 1): it doubles its step until it brackets the target, then
/// narrows the bracket by regula falsi on the logarithm of the length
/// (Illinois variant), or by halving next to widths too wide for the
/// quantization table. It stops on the first stream within
/// [`HIT_TOLERANCE`] below the target, or after [`HIT_MAX_TRIES`] streams,
/// and returns the longest stream that fits, or else the shortest one. The
/// steps are plain arithmetic on lengths with the crate's own logarithm and
/// power, so the result depends neither on the platform nor on the number
/// of threads.
fn hit_rate(
    data: &[&[f32]],
    stats: &quant::Stats,
    allocated: &QuantParams,
    head: &[u8],
    stream: &Stream<'_>,
    target: f64,
) -> Result<Vec<u8>, EncodeError> {
    let mut search = Search {
        data,
        stats,
        allocated,
        head,
        stream,
        target,
        fits: None,
        over: None,
        error: None,
        tries: 0,
    };
    let mut prev = (0.0f64, search.try_scale(0.0)?);
    // Too long: widen (x up); too short: narrow (x down).
    let mut step = if prev.1 > 0.0 { 1.0 } else { -1.0 };
    let mut bracket = None;
    while !search.finished() {
        let x = prev.0 + step;
        if x.abs() > HIT_LOG2_RANGE {
            break;
        }
        let g = search.try_scale(x)?;
        if (g > 0.0) != (prev.1 > 0.0) {
            bracket = Some((prev, (x, g)));
            break;
        }
        prev = (x, g);
        step *= 2.0;
    }
    if let Some((p, q)) = bracket {
        // `long` has g > 0, `short` g <= 0.
        let (mut long, mut short) = if p.1 > 0.0 { (p, q) } else { (q, p) };
        let mut kept = 0i8;
        while !search.finished() && (long.0 - short.0).abs() > HIT_MIN_GAP {
            let mut x = if short.1.is_finite() {
                (long.0 * short.1 - short.0 * long.1) / (short.1 - long.1)
            } else {
                f64::NAN
            };
            let (lo, hi) = (long.0.min(short.0), long.0.max(short.0));
            #[allow(clippy::neg_cmp_op_on_partial_ord)] // also catches NaN
            if !(lo < x && x < hi) {
                x = (long.0 + short.0) / 2.0;
            }
            let g = search.try_scale(x)?;
            if g > 0.0 {
                long = (x, g);
                if kept == 1 {
                    short.1 /= 2.0;
                }
                kept = 1;
            } else {
                short = (x, g);
                if kept == -1 {
                    long.1 /= 2.0;
                }
                kept = -1;
            }
        }
    }
    search.fits.or(search.over).ok_or_else(|| {
        search
            .error
            .unwrap_or(EncodeError::BitrateTooLow(stream.bitrate))
    })
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
    fn optimized_profile_refuses_the_software_number() {
        let (w, h) = (101, 97);
        let mut scratch = wavelet::Scratch::default();
        let options = EncodeOptions::default()
            .with_profile(crate::Profile::Optimized)
            .with_software(0x1234);
        assert_eq!(
            encode(&checker(w, h), w, h, &options, &mut scratch).unwrap_err(),
            EncodeError::SoftwareRequiresConformant(0x1234)
        );
        let options = options.with_software(0);
        let wsq = encode(&checker(w, h), w, h, &options, &mut scratch).unwrap();
        assert_eq!(read_info(&wsq).unwrap().software, 0);
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
    use crate::mem::inject::{Site, refusing};
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
    fn rate_search_output_allocations_are_fallible() {
        let (w, h) = (96, 90);
        let px = pixels(w, h);
        let options = EncodeOptions::default();
        let expected = crate::experimental::encode_at_rate(&px, w, h, &options).unwrap();
        for allowed in [0, 1, 4, 5, 9] {
            let out = refusing(Site::EncodeOutput, allowed, || {
                crate::experimental::encode_at_rate(&px, w, h, &options)
            });
            assert_eq!(out, Err(EncodeError::OutOfMemory), "after {allowed}");
        }
        assert_eq!(
            crate::experimental::encode_at_rate(&px, w, h, &options).unwrap(),
            expected
        );
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
        for allowed in [0, 1, 2, 7, 20, 41] {
            let out = refusing(Site::Transposition, allowed, || encode(&px, w, h, &options));
            assert_eq!(out, Err(EncodeError::OutOfMemory), "encode after {allowed}");
            let out = refusing(Site::Transposition, allowed, || {
                decode_with(&wsq, &decode_options)
            });
            assert_eq!(out, Err(DecodeError::OutOfMemory), "decode after {allowed}");
        }
        assert_eq!(encode(&px, w, h, &options).unwrap(), wsq);
        assert_eq!(decode_with(&wsq, &decode_options).unwrap(), image);
    }
}
