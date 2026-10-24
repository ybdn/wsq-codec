//! Two-channel wavelet filter bank with symmetric boundary extension.
//!
//! The filtering walks each line with an explicit position and step, the
//! way the NIST reference implementation does, instead of computing
//! reflected indices directly. Reproducing that walk, including the order
//! in which products are accumulated in single precision, is what lets the
//! output match the reference implementation bit for bit. The walk is
//! recorded once per line length as a [`Plan`], which then filters many
//! rows or columns side by side.

use crate::tree::{W_TREE_LEN, WNode};

/// Analysis low-pass filter of the standard WSQ filter bank (9 taps).
///
/// The coefficients are the reference implementation's double-precision
/// literals, rounded to single precision as its float arrays round them.
#[allow(clippy::excessive_precision)]
pub(crate) const LOFILT: [f32; 9] = [
    0.037_828_455_506_995_46_f64 as f32,
    -0.023_849_465_019_380_00_f64 as f32,
    -0.110_624_404_418_423_42_f64 as f32,
    0.377_402_855_612_653_80_f64 as f32,
    0.852_698_679_009_403_44_f64 as f32,
    0.377_402_855_612_653_80_f64 as f32,
    -0.110_624_404_418_423_42_f64 as f32,
    -0.023_849_465_019_380_00_f64 as f32,
    0.037_828_455_506_995_46_f64 as f32,
];

/// Analysis high-pass filter of the standard WSQ filter bank (7 taps).
#[allow(clippy::excessive_precision)]
pub(crate) const HIFILT: [f32; 7] = [
    0.064_538_882_628_938_45_f64 as f32,
    -0.040_689_417_609_558_44_f64 as f32,
    -0.418_092_273_222_212_21_f64 as f32,
    0.788_485_616_405_664_39_f64 as f32,
    -0.418_092_273_222_212_21_f64 as f32,
    -0.040_689_417_609_558_44_f64 as f32,
    0.064_538_882_628_938_45_f64 as f32,
];

/// A position moving along a line of `len` samples that bounces off its
/// ends. Each end may make the walk repeat the end sample once
/// (half-sample symmetry) before turning around; otherwise it turns around
/// immediately (whole-sample symmetry).
#[derive(Clone, Copy)]
struct Walk {
    pos: isize,
    step: isize,
    repeat_left: bool,
    repeat_right: bool,
}

impl Walk {
    /// Updates the step for a walk standing on the left end.
    fn turn_left(&mut self) -> bool {
        if self.pos != 0 {
            return false;
        }
        if self.repeat_left {
            self.step = 0;
            self.repeat_left = false;
        } else {
            self.step = 1;
        }
        true
    }

    /// Updates the step for a walk standing on the right end.
    fn turn_right(&mut self, last: isize) -> bool {
        if self.pos != last {
            return false;
        }
        if self.repeat_right {
            self.step = 0;
            self.repeat_right = false;
        } else {
            self.step = -1;
        }
        true
    }

    /// Moves by the current step. The clamp never fires for lines long
    /// enough to be well formed; it only keeps degenerate input in bounds.
    fn advance(&mut self, last: isize) {
        self.pos = (self.pos + self.step).clamp(0, last.max(0));
    }

    fn idx(&self) -> usize {
        self.pos as usize
    }
}

/// One product of a filter: input sample `src` times `coef` times `sfac`.
#[derive(Clone, Copy)]
struct Term {
    src: usize,
    coef: f32,
    sfac: f32,
}

/// Computes output sample `at` from `terms`, either from scratch or by
/// adding them to what the output already holds.
struct Op {
    at: usize,
    accumulate: bool,
    terms: std::ops::Range<usize>,
}

/// The operations filtering a line of a given length performs, in order.
///
/// Which input samples each output reads depends only on the line length,
/// the filters and the band order, so a plan is worked out once and then
/// applied to every row of a region, or to all of its columns at once.
/// Each output sample goes through the same products and sums in the same
/// order as the walk that built the plan.
struct Plan {
    len: usize,
    ops: Vec<Op>,
    terms: Vec<Term>,
}

impl Plan {
    fn new(len: usize) -> Self {
        Plan {
            len,
            ops: Vec::new(),
            terms: Vec::new(),
        }
    }

    /// Records an operation. Outputs past the end of the line, which only
    /// degenerate filter banks produce, are dropped.
    fn push(&mut self, at: usize, accumulate: bool, terms: impl IntoIterator<Item = Term>) {
        if at >= self.len {
            return;
        }
        let start = self.terms.len();
        self.terms.extend(terms);
        let end = self.terms.len();
        self.ops.push(Op {
            at,
            accumulate,
            terms: start..end,
        });
    }

    /// Filters `rows` rows. Row `r` of the input starts in `src` at
    /// `src_row(r)`, and row `r` of the output in `dst` at `dst_row(r)`.
    ///
    /// Rows go through [`apply_columns`](Self::apply_columns) in blocks,
    /// transposed, which filters them side by side.
    fn apply_rows(
        &self,
        src: &[f32],
        src_row: impl Fn(usize) -> usize,
        dst: &mut [f32],
        dst_row: impl Fn(usize) -> usize,
        rows: usize,
    ) {
        const BLOCK: usize = 64;
        let n = self.len;
        let mut tin = vec![0.0f32; n * BLOCK];
        let mut tout = vec![0.0f32; n * BLOCK];
        let mut starts = [0usize; BLOCK];
        for first in (0..rows).step_by(BLOCK) {
            let count = BLOCK.min(rows - first);
            // Transposing a sample of every row at a time keeps the rows
            // being read in cache.
            for (b, start) in starts[..count].iter_mut().enumerate() {
                *start = src_row(first + b);
            }
            for (k, line) in tin.chunks_exact_mut(BLOCK).enumerate() {
                for (v, &start) in line.iter_mut().zip(&starts[..count]) {
                    *v = src[start + k];
                }
            }
            self.apply_columns(&tin, |k| k * BLOCK, &mut tout, |k| k * BLOCK, count);
            for (b, start) in starts[..count].iter_mut().enumerate() {
                *start = dst_row(first + b);
            }
            for (k, line) in tout.chunks_exact(BLOCK).enumerate() {
                for (&v, &start) in line.iter().zip(&starts[..count]) {
                    dst[start + k] = v;
                }
            }
        }
    }

    /// Filters `cols` columns at once. Sample `k` of every input column
    /// lies in `src` from `src_row(k)` on, and sample `k` of every output
    /// column in `dst` from `dst_row(k)` on.
    fn apply_columns(
        &self,
        src: &[f32],
        src_row: impl Fn(usize) -> usize,
        dst: &mut [f32],
        dst_row: impl Fn(usize) -> usize,
        cols: usize,
    ) {
        for k in 0..self.len {
            let d = dst_row(k);
            dst[d..d + cols].fill(0.0);
        }
        for op in &self.ops {
            let d = dst_row(op.at);
            let out = &mut dst[d..d + cols];
            let mut terms = self.terms[op.terms.clone()].iter();
            if !op.accumulate {
                match terms.next() {
                    Some(t) => {
                        let s = src_row(t.src);
                        for (o, &x) in out.iter_mut().zip(&src[s..s + cols]) {
                            *o = x * t.coef * t.sfac;
                        }
                    }
                    None => out.fill(0.0),
                }
            }
            for t in terms {
                let s = src_row(t.src);
                for (o, &x) in out.iter_mut().zip(&src[s..s + cols]) {
                    *o += x * t.coef * t.sfac;
                }
            }
        }
    }
}

/// Plan for splitting a line of `n` samples into its low-pass half
/// followed by its high-pass half (or the reverse when `inv` is set).
fn analysis_plan(n: usize, lo: &[f32], hi: &[f32], inv: bool) -> Plan {
    let mut plan = Plan::new(n);
    if n == 0 {
        return plan;
    }
    let odd_len = n % 2 == 1;
    let odd_filters = lo.len() % 2 == 1;

    let (loc, hoc, ext) = if odd_filters {
        (
            (lo.len() as isize - 1) / 2,
            (hi.len() as isize - 1) / 2 - 1,
            false,
        )
    } else {
        (lo.len() as isize / 2 - 2, hi.len() as isize / 2 - 2, true)
    };
    let (mut lle, mut hle) = (ext, ext);
    let (loc, hoc) = (
        if loc == -1 {
            lle = false;
            0
        } else {
            loc
        },
        if hoc == -1 {
            hle = false;
            0
        } else {
            hoc
        },
    );
    // Even-length filter banks run the high-pass filter negated.
    let hi_even: Vec<f32>;
    let hi = if odd_filters {
        hi
    } else {
        hi_even = hi.iter().map(|&c| -c).collect();
        &hi_even
    };

    let llen = n.div_ceil(2);
    let hlen = n / 2;
    let (lo_out, hi_out) = if inv { (hlen, 0) } else { (0, llen) };
    let last = n as isize - 1;

    let filter = |plan: &mut Plan, at: usize, start: Walk, taps: &[f32]| {
        let mut w = start;
        let first = w.idx();
        let rest = taps[1..].iter().map(move |&coef| {
            w.turn_left();
            w.turn_right(last);
            w.advance(last);
            (w.idx(), coef)
        });
        let terms = std::iter::once((first, taps[0])).chain(rest);
        plan.push(
            at,
            false,
            terms.map(|(src, coef)| Term {
                src,
                coef,
                sfac: 1.0,
            }),
        );
    };

    let clamp = |p: isize| p.clamp(0, last);
    let mut lstart = Walk {
        pos: clamp(loc),
        step: -1,
        repeat_left: lle,
        repeat_right: ext,
    };
    let mut hstart = Walk {
        pos: clamp(hoc),
        step: -1,
        repeat_left: hle,
        repeat_right: ext,
    };

    for pix in 0..hlen {
        filter(&mut plan, lo_out + pix, lstart, lo);
        filter(&mut plan, hi_out + pix, hstart, hi);
        for _ in 0..2 {
            lstart.turn_left();
            lstart.advance(last);
            hstart.turn_left();
            hstart.advance(last);
        }
    }
    if odd_len {
        filter(&mut plan, lo_out + hlen, lstart, lo);
    }
    plan
}

/// Inverse of [`analysis_plan`]: plan for interleaving a low-pass and a
/// high-pass half back into one line of `n` samples. `lo` and `hi` are the
/// synthesis filters.
fn synthesis_plan(n: usize, lo: &[f32], hi: &[f32], inv: bool) -> Plan {
    let mut plan = Plan::new(n);
    if n == 0 {
        return plan;
    }
    let odd_len = n % 2 == 1;
    let llen = n.div_ceil(2);
    let hlen = n / 2;
    let (lsz, hsz) = (lo.len() as isize, hi.len() as isize);

    let asym;
    let ssfac: f32;
    let ofhre: i32;
    let (mut loc, mut hoc, lotap, hotap);
    let (mut olle, olre, mut ohle, ohre);
    if lsz % 2 == 1 {
        asym = false;
        ssfac = 1.0;
        ofhre = 0;
        loc = (lsz - 1) / 4;
        hoc = (hsz + 1) / 4 - 1;
        lotap = ((lsz - 1) / 2) % 2 == 1;
        hotap = ((hsz + 1) / 2) % 2 == 1;
        (olle, olre, ohle, ohre) = if odd_len {
            (false, false, true, true)
        } else {
            (false, true, true, false)
        };
    } else {
        asym = true;
        ssfac = -1.0;
        ofhre = 2;
        loc = lsz / 4 - 1;
        hoc = hsz / 4 - 1;
        lotap = (lsz / 2) % 2 == 1;
        hotap = (hsz / 2) % 2 == 1;
        (olle, olre, ohle, ohre) = if odd_len {
            (true, false, true, true)
        } else {
            (true, true, true, true)
        };
        if loc == -1 {
            loc = 0;
            olle = false;
        }
        if hoc == -1 {
            hoc = 0;
            ohle = false;
        }
    }
    let hi_even: Vec<f32>;
    let hi = if asym {
        hi_even = hi.iter().map(|&c| -c).collect();
        &hi_even[..]
    } else {
        hi
    };

    let (lo_src, hi_src) = if inv { (hlen, 0) } else { (0, llen) };
    let llast = llen as isize - 1;
    let hlast = hlen as isize - 1;

    // Low-pass taps assign each output sample...
    let low = |plan: &mut Plan, at: usize, start: Walk, tap: usize| {
        let mut w = start;
        let first = w.idx();
        let rest = lo.iter().skip(tap + 2).step_by(2).map(move |&coef| {
            w.turn_left();
            w.turn_right(llast);
            w.advance(llast);
            (w.idx(), coef)
        });
        let terms = std::iter::once((first, lo[tap])).chain(rest);
        plan.push(
            at,
            false,
            terms.map(|(i, coef)| Term {
                src: lo_src + i,
                coef,
                sfac: 1.0,
            }),
        );
    };
    // ...and high-pass taps then add to it, one product at a time.
    // Returns the updated right-end counter, which only even-length
    // filter banks use.
    let high = |plan: &mut Plan, at: usize, start: Walk, tap: usize, sfac0: f32, fhre0: i32| {
        if hlen == 0 {
            return fhre0;
        }
        let mut w = start;
        let mut sfac = sfac0;
        let mut fhre = fhre0;
        let mut terms = Vec::new();
        for &coef in hi.iter().skip(tap).step_by(2) {
            if w.turn_left() && w.step == 1 {
                sfac = 1.0;
            }
            if w.pos == hlast {
                if w.repeat_right {
                    w.step = 0;
                    w.repeat_right = false;
                    if asym && odd_len {
                        w.repeat_right = true;
                        fhre -= 1;
                        sfac = fhre as f32;
                        if sfac == 0.0 {
                            w.repeat_right = false;
                        }
                    }
                } else {
                    w.step = -1;
                    if asym {
                        sfac = -1.0;
                    }
                }
            }
            terms.push(Term {
                src: hi_src + w.idx(),
                coef,
                sfac,
            });
            w.advance(hlast);
        }
        plan.push(at, true, terms);
        fhre
    };

    let clamp = |p: isize, last: isize| p.clamp(0, last.max(0));
    let mut lstart = Walk {
        pos: clamp(loc, llast),
        step: -1,
        repeat_left: olle,
        repeat_right: olre,
    };
    let mut hstart = Walk {
        pos: clamp(hoc, hlast),
        step: -1,
        repeat_left: ohle,
        repeat_right: ohre,
    };
    let mut lstap = usize::from(lotap);
    let mut hstap = usize::from(hotap);
    let mut osfac = ssfac;
    let (mut limg, mut himg) = (0usize, 0usize);
    let mut fhre = 0;

    for _ in 0..hlen {
        for tap in (0..=lstap).rev() {
            low(&mut plan, limg, lstart, tap);
            limg += 1;
        }
        lstart.turn_left();
        lstart.advance(llast);
        lstap = 1;

        for tap in (0..=hstap).rev() {
            fhre = high(&mut plan, himg, hstart, tap, osfac, ofhre);
            himg += 1;
        }
        if hstart.turn_left() && hstart.step == 1 {
            osfac = 1.0;
        }
        hstart.advance(hlast);
        hstap = 1;
    }

    let lstap = match (odd_len, lotap) {
        (true, true) => 1,
        (true, false) => 0,
        (false, true) => 2,
        (false, false) => 1,
    };
    for tap in (lstap..=1).rev() {
        low(&mut plan, limg, lstart, tap);
        limg += 1;
    }

    let hstap = match (odd_len, hotap) {
        (true, true) => 1,
        (true, false) => 0,
        (false, true) => 2,
        (false, false) => 1,
    };
    if odd_len && hsz == 2 {
        hstart.pos = clamp(hstart.pos - hstart.step, hlast);
        fhre = 1;
    }
    for tap in (hstap..=1).rev() {
        if hsz != 2 {
            fhre = ofhre;
        }
        fhre = high(&mut plan, himg, hstart, tap, osfac, fhre);
        himg += 1;
    }
    plan
}

/// Forward transform of the whole image in place, one wavelet tree node
/// at a time.
pub(crate) fn decompose(data: &mut [f32], width: usize, tree: &[WNode; W_TREE_LEN]) {
    let mut tmp = vec![0.0f32; data.len()];
    for node in tree {
        let (x, y, lenx, leny) = (node.x, node.y, node.lenx, node.leny);
        // Rows: image -> tmp.
        let rows = analysis_plan(lenx, &LOFILT, &HIFILT, node.inv_rw);
        rows.apply_rows(data, |r| (y + r) * width + x, &mut tmp, |r| r * width, leny);
        // Columns: tmp -> image.
        let cols = analysis_plan(leny, &LOFILT, &HIFILT, node.inv_cl);
        cols.apply_columns(&tmp, |r| r * width, data, |r| (y + r) * width + x, lenx);
    }
}

/// Inverse transform of the whole image in place. `lo` and `hi` are the
/// synthesis filters as read from the transform table.
pub(crate) fn reconstruct(
    data: &mut [f32],
    width: usize,
    tree: &[WNode; W_TREE_LEN],
    lo: &[f32],
    hi: &[f32],
) {
    let mut tmp = vec![0.0f32; data.len()];
    for node in tree.iter().rev() {
        let (x, y, lenx, leny) = (node.x, node.y, node.lenx, node.leny);
        // Columns: image -> tmp.
        let cols = synthesis_plan(leny, lo, hi, node.inv_cl);
        cols.apply_columns(data, |r| (y + r) * width + x, &mut tmp, |r| r * width, lenx);
        // Rows: tmp -> image.
        let rows = synthesis_plan(lenx, lo, hi, node.inv_rw);
        rows.apply_rows(&tmp, |r| r * width, data, |r| (y + r) * width + x, leny);
    }
}
