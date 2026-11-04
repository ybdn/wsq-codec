use crate::error::EncodeError;
use crate::huffman::{self, EncodeTable, HuffmanSpec};
use crate::quant::{self, QuantParams};
use crate::tables::{self, EOI, FrameHeader, SOI};
use crate::tree::{NUM_SUBBANDS, START_SUBBAND_2, START_SUBBAND_3, Trees};
use crate::wavelet::{self, HIFILT, LOFILT};
use crate::{EncodeOptions, math, nistcom, par};

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

/// How the encoder chooses the quantizer bin widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rate {
    /// The specification's bit allocation, as the reference implementation
    /// does it: the actual rate lands near the requested one, not on it.
    Allocated,
    /// The bit allocation's widths, all scaled by one factor searched so
    /// that the whole stream fits the requested size (#42, experimental).
    Hit,
}

/// Share of the target size the [`Rate::Hit`] search may fall short by: it
/// stops on the first stream between `1 - HIT_TOLERANCE` times the target
/// and the target.
pub(crate) const HIT_TOLERANCE: f64 = 0.005;
/// Most streams the [`Rate::Hit`] search writes before keeping the best.
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
    rate: Rate,
    scratch: &mut wavelet::Scratch,
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
    let mut data = scratch.image.rows(width, height, threads)?;
    let (m_shift, r_scale) = normalize(pixels, &mut data, threads);
    let trees = Trees::new(width, height);
    let mut tmp = scratch.tmp.rows(width, height, threads)?;
    wavelet::decompose(&mut data, &mut tmp, &trees.w, threads);
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
        // Only the conformant encoder may carry the FBI's number.
        software: match rate {
            Rate::Allocated => options.software,
            Rate::Hit => 0,
        },
    };
    let stream = Stream {
        trees: &trees,
        frame: &frame,
        bitrate,
        threads,
    };
    match rate {
        Rate::Allocated => {
            let mut params = allocated;
            quant::limit_bin_widths(&mut params, &stats);
            let coeffs = quant::quantize(&data, &trees.q, &params, threads)?;
            drop(data);
            scratch.image.release(scratch.keep);
            let mut out = header(width, height, options)?;
            stream.write(&mut out, &coeffs, &params)?;
            Ok(out)
        }
        Rate::Hit => {
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
    let mut out = Vec::new();
    out.extend_from_slice(&SOI.to_be_bytes());
    // The first NISTCOM comment the caller gives is merged into ours.
    let merged = options.comments.iter().position(|c| nistcom::is_nistcom(c));
    let base = merged.map(|i| options.comments[i].as_slice());
    let nist = nistcom::build(width, height, options.ppi, options.bitrate, base);
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
        let compress = |table: &EncodeTable, block: &[i16]| {
            let mut data = Vec::new();
            table.compress(block, &mut data);
            data
        };
        let (data1, data2, data3) = par::run3(
            parallel,
            || compress(&table1, block1),
            || compress(&table2, block2),
            || compress(&table2, block3),
        );

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

/// State of the [`Rate::Hit`] search.
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
        let mut out = self.head.to_vec();
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

/// The [`Rate::Hit`] search: the stream whose total length comes closest to
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
