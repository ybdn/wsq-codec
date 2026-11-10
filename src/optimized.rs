//! The rate search of [`Profile::Optimized`](crate::Profile::Optimized).
//!
//! Everything it needs from the conformant pipeline (the transform, the bit
//! allocation, the quantization and the stream writer) comes from
//! `wsq-certified`, through its hidden `for_wsq_codec` module: this crate
//! only decides which scale factor to write a stream for. A factor of 1 is
//! the conformant stream.

use wsq_certified::EncodeError;
use wsq_certified::for_wsq_codec::{Scaler, Scratch, ln, pow, with_scaler};

/// Share of the target size the search may fall short by: it stops on the
/// first stream between `1 - HIT_TOLERANCE` times the target and the target.
pub(crate) const HIT_TOLERANCE: f64 = 0.005;
/// Most streams the search writes before keeping the best.
const HIT_MAX_TRIES: usize = 40;
/// Bound on the base-2 logarithm of the scale factor.
const HIT_LOG2_RANGE: f64 = 40.0;
/// Bracket width, on that logarithm, below which the search stops: about
/// the resolution of an `f32` factor.
const HIT_MIN_GAP: f64 = 1e-7;

/// Encodes with the search: the stream whose total length comes closest to
/// `bitrate * width * height / 8` bytes without exceeding it.
pub(crate) fn encode(
    pixels: &[u8],
    width: usize,
    height: usize,
    bitrate: f32,
    options: &wsq_certified::EncodeOptions,
    scratch: &mut Scratch,
) -> Result<Vec<u8>, EncodeError> {
    let target = f64::from(bitrate) * (width * height) as f64 / 8.0;
    with_scaler(pixels, width, height, options, scratch, |scaler| {
        hit_rate(scaler, bitrate, target)
    })
}

/// State of the search.
struct Search<'a, 'b> {
    scaler: &'a Scaler<'b>,
    target: f64,
    /// Longest stream that fits the target.
    fits: Option<Vec<u8>>,
    /// Shortest stream that does not.
    over: Option<Vec<u8>>,
    /// Why widths did not fit the quantization table, the first time.
    error: Option<EncodeError>,
    tries: usize,
}

impl Search<'_, '_> {
    /// Writes the stream for `2^x` times the bit allocation's widths and
    /// returns how far its length is from the middle of the accepted range,
    /// as a difference of logarithms: positive if too long, negative if too
    /// short, minus infinity if the widths do not fit the quantization
    /// table (they are then too wide).
    fn try_scale(&mut self, x: f64) -> Result<f64, EncodeError> {
        self.tries += 1;
        let out = match self.scaler.write(pow(2.0, x) as f32) {
            Ok(out) => out,
            Err(e @ EncodeError::BitrateTooLow(_)) => {
                self.error.get_or_insert(e);
                return Ok(f64::NEG_INFINITY);
            }
            Err(e) => return Err(e),
        };
        let len = out.len() as f64;
        if len <= self.target {
            if self.fits.as_ref().is_none_or(|f| f.len() < out.len()) {
                self.fits = Some(out);
            }
        } else if self.over.as_ref().is_none_or(|o| o.len() > out.len()) {
            self.over = Some(out);
        }
        Ok(ln(len) - ln(self.target * (1.0 - HIT_TOLERANCE / 2.0)))
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

/// The search: the stream whose total length comes closest to `target`
/// bytes without exceeding it, found by scaling every bin width of the bit
/// allocation by one factor.
///
/// The length shrinks, in steps, as the factor grows. The search runs on
/// the base-2 logarithm of the factor, starting from the conformant widths
/// (factor 1): it doubles its step until it brackets the target, then
/// narrows the bracket by regula falsi on the logarithm of the length
/// (Illinois variant), or by halving next to widths too wide for the
/// quantization table. It stops on the first stream within
/// [`HIT_TOLERANCE`] below the target, or after [`HIT_MAX_TRIES`] streams,
/// and returns the longest stream that fits, or else the shortest one. The
/// steps are plain arithmetic on lengths with the certified crate's own
/// logarithm and power, so the result depends neither on the platform nor
/// on the number of threads.
fn hit_rate(scaler: &Scaler<'_>, bitrate: f32, target: f64) -> Result<Vec<u8>, EncodeError> {
    let mut search = Search {
        scaler,
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
    search
        .fits
        .or(search.over)
        .ok_or_else(|| search.error.unwrap_or(EncodeError::BitrateTooLow(bitrate)))
}
