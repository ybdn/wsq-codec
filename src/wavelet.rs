//! Two-channel wavelet filter bank with symmetric boundary extension.
//!
//! The filtering walks each line with an explicit position and step, the
//! way the NIST reference implementation does, instead of computing
//! reflected indices directly. Reproducing that walk, including the order
//! in which products are accumulated in single precision, is what lets the
//! output match the reference implementation bit for bit.

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

/// Splits one line into its low-pass half followed by its high-pass half
/// (or the reverse when `inv` is set). `dst` must be as long as `src`.
fn analyze_line(src: &[f32], dst: &mut [f32], lo: &[f32], hi: &[f32], inv: bool) {
    let n = src.len();
    if n == 0 {
        return;
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

    let filter = |start: Walk, taps: &[f32]| -> f32 {
        let mut w = start;
        let mut acc = src[w.idx()] * taps[0];
        for &tap in &taps[1..] {
            w.turn_left();
            w.turn_right(last);
            w.advance(last);
            acc += src[w.idx()] * tap;
        }
        acc
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
        dst[lo_out + pix] = filter(lstart, lo);
        dst[hi_out + pix] = filter(hstart, hi);
        for _ in 0..2 {
            lstart.turn_left();
            lstart.advance(last);
            hstart.turn_left();
            hstart.advance(last);
        }
    }
    if odd_len {
        dst[lo_out + hlen] = filter(lstart, lo);
    }
}

/// Inverse of [`analyze_line`]: interleaves a low-pass and a high-pass half
/// back into one line. `lo` and `hi` are the synthesis filters.
fn synthesize_line(src: &[f32], dst: &mut [f32], lo: &[f32], hi: &[f32], inv: bool) {
    let n = src.len();
    if n == 0 {
        return;
    }
    dst.fill(0.0);
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

    let (lo_src, hi_src) = if inv {
        (&src[hlen..], &src[..hlen])
    } else {
        (&src[..llen], &src[llen..])
    };
    let llast = llen as isize - 1;
    let hlast = hlen as isize - 1;

    // Low-pass taps assign each output sample...
    let low = |dst: &mut [f32], at: usize, start: Walk, tap: usize| {
        let mut w = start;
        let mut acc = lo_src[w.idx()] * lo[tap];
        for &coef in lo.iter().skip(tap + 2).step_by(2) {
            w.turn_left();
            w.turn_right(llast);
            w.advance(llast);
            acc += lo_src[w.idx()] * coef;
        }
        if let Some(out) = dst.get_mut(at) {
            *out = acc;
        }
    };
    // ...and high-pass taps then add to it, one product at a time.
    // Returns the updated right-end counter, which only even-length
    // filter banks use.
    let high = |dst: &mut [f32], at: usize, start: Walk, tap: usize, sfac0: f32, fhre0: i32| {
        if hlen == 0 {
            return fhre0;
        }
        let mut w = start;
        let mut sfac = sfac0;
        let mut fhre = fhre0;
        let mut acc = dst.get(at).copied().unwrap_or(0.0);
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
            acc += hi_src[w.idx()] * coef * sfac;
            w.advance(hlast);
        }
        if let Some(out) = dst.get_mut(at) {
            *out = acc;
        }
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
            low(dst, limg, lstart, tap);
            limg += 1;
        }
        lstart.turn_left();
        lstart.advance(llast);
        lstap = 1;

        for tap in (0..=hstap).rev() {
            fhre = high(dst, himg, hstart, tap, osfac, ofhre);
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
        low(dst, limg, lstart, tap);
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
        fhre = high(dst, himg, hstart, tap, osfac, fhre);
        himg += 1;
    }
}

/// Forward transform of the whole image in place, one wavelet tree node
/// at a time.
pub(crate) fn decompose(data: &mut [f32], width: usize, tree: &[WNode; W_TREE_LEN]) {
    let mut tmp = vec![0.0f32; data.len()];
    let mut line = Vec::new();
    let mut out = Vec::new();
    for node in tree {
        let (x, y, lenx, leny) = (node.x, node.y, node.lenx, node.leny);
        // Rows: image -> tmp.
        for r in 0..leny {
            let start = (y + r) * width + x;
            let dst = &mut tmp[r * width..r * width + lenx];
            analyze_line(
                &data[start..start + lenx],
                dst,
                &LOFILT,
                &HIFILT,
                node.inv_rw,
            );
        }
        // Columns: tmp -> image.
        for c in 0..lenx {
            line.clear();
            line.extend((0..leny).map(|r| tmp[r * width + c]));
            out.resize(leny, 0.0);
            analyze_line(&line, &mut out, &LOFILT, &HIFILT, node.inv_cl);
            for (r, &v) in out.iter().enumerate() {
                data[(y + r) * width + x + c] = v;
            }
        }
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
    let mut line = Vec::new();
    let mut out = Vec::new();
    for node in tree.iter().rev() {
        let (x, y, lenx, leny) = (node.x, node.y, node.lenx, node.leny);
        // Columns: image -> tmp.
        for c in 0..lenx {
            line.clear();
            line.extend((0..leny).map(|r| data[(y + r) * width + x + c]));
            out.resize(leny, 0.0);
            synthesize_line(&line, &mut out, lo, hi, node.inv_cl);
            for (r, &v) in out.iter().enumerate() {
                tmp[r * width + c] = v;
            }
        }
        // Rows: tmp -> image.
        for r in 0..leny {
            let start = (y + r) * width + x;
            let src = &tmp[r * width..r * width + lenx];
            synthesize_line(src, &mut data[start..start + lenx], lo, hi, node.inv_rw);
        }
    }
}
