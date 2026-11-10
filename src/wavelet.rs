//! Two-channel wavelet filter bank with symmetric boundary extension.
//!
//! The filtering walks each line with an explicit position and step, the
//! way the NIST reference implementation does, instead of computing
//! reflected indices directly. Reproducing that walk, including the order
//! in which products are accumulated in single precision, is what lets the
//! output match the reference implementation bit for bit. The walk is
//! recorded once per line length as a [`Plan`], which then filters many
//! rows or columns side by side.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, RwLock, mpsc};

use crate::mem::{self, OutOfMemory};
use crate::par;
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

    /// Filters rows: `src[r]` is input row `r` and `dst[r]` output row
    /// `r`, both [`len`](Self::len) samples long.
    ///
    /// Rows go through [`apply_columns`](Self::apply_columns) in blocks,
    /// transposed, which filters them side by side. The two blocks, of
    /// `len` by 64 samples each, are allocated fallibly.
    fn apply_rows(&self, src: &[&[f32]], dst: &mut [&mut [f32]]) -> Result<(), OutOfMemory> {
        const BLOCK: usize = 64;
        let n = self.len;
        let mut tin = mem::transposition_block(n * BLOCK)?;
        let mut tout = mem::transposition_block(n * BLOCK)?;
        for (src, dst) in src.chunks(BLOCK).zip(dst.chunks_mut(BLOCK)) {
            let count = src.len();
            // Transposing a sample of every row at a time keeps the rows
            // being read in cache.
            for (k, line) in tin.chunks_exact_mut(BLOCK).enumerate() {
                for (v, row) in line.iter_mut().zip(src) {
                    *v = row[k];
                }
            }
            let tin_rows: Vec<&[f32]> = tin.chunks_exact(BLOCK).map(|c| &c[..count]).collect();
            let mut tout_rows: Vec<&mut [f32]> = tout
                .chunks_exact_mut(BLOCK)
                .map(|c| &mut c[..count])
                .collect();
            self.apply_columns(&tin_rows, &mut tout_rows);
            for (k, line) in tout.chunks_exact(BLOCK).enumerate() {
                for (&v, row) in line.iter().zip(dst.iter_mut()) {
                    row[k] = v;
                }
            }
        }
        Ok(())
    }

    /// Filters columns side by side: `src[k]` holds sample `k` of every
    /// input column, and `dst[k]` sample `k` of every output column, for
    /// `k` below [`len`](Self::len).
    fn apply_columns(&self, src: &[&[f32]], dst: &mut [&mut [f32]]) {
        self.apply_columns_range(src, dst, 0..self.len);
    }

    /// [`apply_columns`](Self::apply_columns) restricted to the output
    /// samples in `range`: `dst[k]` holds output sample `range.start + k`.
    ///
    /// Each output sample goes through the operations that compute it in
    /// plan order, wherever the range starts, so splitting the outputs
    /// between threads does not change them.
    fn apply_columns_range(
        &self,
        src: &[&[f32]],
        dst: &mut [&mut [f32]],
        range: std::ops::Range<usize>,
    ) {
        for out in dst.iter_mut() {
            out.fill(0.0);
        }
        for op in self.ops.iter().filter(|op| range.contains(&op.at)) {
            let out = &mut *dst[op.at - range.start];
            let mut terms = self.terms[op.terms.clone()].iter();
            if !op.accumulate {
                match terms.next() {
                    Some(t) => {
                        for (o, &x) in out.iter_mut().zip(src[t.src]) {
                            *o = x * t.coef * t.sfac;
                        }
                    }
                    None => out.fill(0.0),
                }
            }
            for t in terms {
                for (o, &x) in out.iter_mut().zip(src[t.src]) {
                    *o += x * t.coef * t.sfac;
                }
            }
        }
    }
}

/// How many threads to split `lines` lines of `len` samples over, given
/// up to `threads` of them. Each gets enough work to pay for starting it.
fn shares(threads: usize, lines: usize, len: usize) -> usize {
    const MIN_LINES: usize = 32;
    const MIN_SAMPLES: usize = 1 << 16;
    let by_lines = lines / MIN_LINES;
    let by_samples = lines * len / MIN_SAMPLES;
    threads.min(by_lines).min(by_samples).max(1)
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

/// The transformed image and the transform's scratch space, which
/// [`crate::Decoder`] and [`crate::Encoder`] keep from one image to the
/// next.
#[derive(Default)]
pub(crate) struct Scratch {
    pub image: Bands,
    pub tmp: Bands,
    /// Keeps the buffers once an image is done. Without it each buffer is
    /// freed as soon as it is no longer needed, which keeps the peak
    /// memory of a single image down.
    pub keep: bool,
}

impl Scratch {
    /// Buffers kept from one image to the next.
    pub fn kept() -> Self {
        Scratch {
            keep: true,
            ..Scratch::default()
        }
    }
}

impl std::fmt::Debug for Scratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scratch")
            .field("samples", &self.image.len())
            .finish()
    }
}

/// An image-sized buffer held in bands of whole rows, each band its own
/// allocation.
///
/// Filling a fresh image-sized allocation costs mostly the first touch of
/// its pages, and safe code can only fill a vector from one thread. Bands
/// let each thread allocate, fallibly, and zero its own share, side by
/// side.
#[derive(Default)]
pub(crate) struct Bands {
    bands: Vec<Vec<f32>>,
}

impl Bands {
    /// Samples held.
    pub fn len(&self) -> usize {
        self.bands.iter().map(Vec::len).sum()
    }

    /// `height` rows of `width` samples. They are carved from the bands
    /// already held when those hold enough whole rows, and keep whatever
    /// they hold; otherwise new bands are allocated and zeroed on up to
    /// `threads` threads.
    pub fn rows(
        &mut self,
        width: usize,
        height: usize,
        threads: usize,
    ) -> Result<Vec<&mut [f32]>, OutOfMemory> {
        let width = width.max(1);
        let held: usize = self.bands.iter().map(|band| band.len() / width).sum();
        if held < height {
            // Freed first, so that the old and new bands never coexist.
            self.bands = Vec::new();
            self.bands = allocate(width, height, threads)?;
        }
        Ok(self
            .bands
            .iter_mut()
            .flat_map(|band| band.chunks_exact_mut(width))
            .take(height)
            .collect())
    }

    /// Frees the bands unless `keep` is set.
    pub fn release(&mut self, keep: bool) {
        if !keep {
            self.bands = Vec::new();
        }
    }
}

/// Zeroed bands for `height` rows of `width` samples, one per thread.
fn allocate(width: usize, height: usize, threads: usize) -> Result<Vec<Vec<f32>>, OutOfMemory> {
    let parts = par::parts(threads, width * height);
    let rows = height.div_ceil(parts).max(1);
    let sizes: Vec<usize> = (0..height)
        .step_by(rows)
        .map(|y| rows.min(height - y) * width)
        .collect();
    if sizes.len() <= 1 {
        return sizes.iter().map(|&n| mem::filled(n, 0.0)).collect();
    }
    std::thread::scope(|scope| {
        let tasks: Vec<_> = sizes
            .iter()
            .map(|&n| par::spawn(scope, move || mem::filled(n, 0.0)))
            .collect();
        tasks.into_iter().map(par::Task::join).collect()
    })
}

/// One filtering pass over a `lenx` by `leny` region: between the region
/// of the image at (`x`, `y`) and the top left corner of the scratch
/// buffer, in the direction `to_tmp` gives.
struct Pass {
    plan: Plan,
    /// Filters columns rather than rows.
    columns: bool,
    to_tmp: bool,
    x: usize,
    y: usize,
    lenx: usize,
    leny: usize,
}

/// Rows of a buffer, each behind its own lock so that threads can share
/// the buffer out row by row, differently in each pass.
fn lock_rows<'a>(rows: &'a mut [&mut [f32]]) -> Vec<RwLock<&'a mut [f32]>> {
    rows.iter_mut().map(|row| RwLock::new(&mut **row)).collect()
}

/// Runs `passes` in order over the rows of `data` and `tmp`, on up to
/// `threads` threads started once for all of them.
///
/// Every pass splits its output rows between the threads, as many as the
/// pass is worth, and the threads wait for each other between passes. A
/// row pass filters its share of the rows, and a column pass computes its
/// share of the output rows from all the input rows. Either way each
/// output sample goes through the same products and sums as on a single
/// thread.
///
/// Fails if a thread could not allocate its transposition blocks. The
/// threads then skip the passes left, but still wait for each other
/// between them, so that none is left waiting; `data` and `tmp` hold
/// nothing meaningful after that.
fn run(
    passes: &[Pass],
    data: &mut [&mut [f32]],
    tmp: &mut [&mut [f32]],
    threads: usize,
) -> Result<(), OutOfMemory> {
    let wanted = passes
        .iter()
        .map(|p| shares(threads, p.leny, p.lenx))
        .max()
        .unwrap_or(1);
    let image = lock_rows(data);
    let scratch = lock_rows(tmp);
    let failed = AtomicBool::new(false);
    let work = |id: usize, workers: usize, barrier: Option<&Barrier>| {
        for pass in passes {
            if !failed.load(Ordering::Relaxed) {
                let parts = workers.min(shares(threads, pass.leny, pass.lenx));
                if run_share(pass, id, parts, &image, &scratch).is_err() {
                    failed.store(true, Ordering::Relaxed);
                }
            }
            if let Some(barrier) = barrier {
                barrier.wait();
            }
        }
    };
    if wanted <= 1 {
        work(0, 1, None);
        return outcome(&failed);
    }
    std::thread::scope(|scope| {
        // Threads that could be created learn how many there are, and
        // which one they are, before the first pass.
        let mut starts = Vec::new();
        for _ in 1..wanted {
            let (start, started) = mpsc::channel::<(usize, usize, Arc<Barrier>)>();
            let job = move || {
                if let Ok((id, workers, barrier)) = started.recv() {
                    work(id, workers, Some(&barrier));
                }
            };
            match par::try_spawn(scope, job) {
                Ok(_) => starts.push(start),
                Err(_) => break,
            }
        }
        let workers = starts.len() + 1;
        let barrier = Arc::new(Barrier::new(workers));
        for (i, start) in starts.iter().enumerate() {
            start
                .send((i + 1, workers, Arc::clone(&barrier)))
                .expect("a started thread waits for its share");
        }
        work(0, workers, Some(&barrier));
    });
    outcome(&failed)
}

fn outcome(failed: &AtomicBool) -> Result<(), OutOfMemory> {
    if failed.load(Ordering::Relaxed) {
        Err(OutOfMemory)
    } else {
        Ok(())
    }
}

/// Thread `id`'s share of `pass`, split between `parts` threads.
fn run_share<'a>(
    pass: &Pass,
    id: usize,
    parts: usize,
    image: &[RwLock<&'a mut [f32]>],
    scratch: &[RwLock<&'a mut [f32]>],
) -> Result<(), OutOfMemory> {
    let size = pass.leny.div_ceil(parts.max(1));
    let first = id * size;
    let last = (first + size).min(pass.leny);
    if first >= last {
        return Ok(());
    }
    let ((src, sx, sy), (dst, dx, dy)) = if pass.to_tmp {
        ((image, pass.x, pass.y), (scratch, 0, 0))
    } else {
        ((scratch, 0, 0), (image, pass.x, pass.y))
    };
    let lenx = pass.lenx;
    // Column passes read every input row; row passes read their own.
    let inputs = if pass.columns {
        0..pass.leny
    } else {
        first..last
    };
    let src: Vec<_> = src[sy + inputs.start..sy + inputs.end]
        .iter()
        .map(|row| row.read().expect("no thread panics while holding a row"))
        .collect();
    let mut dst: Vec<_> = dst[dy + first..dy + last]
        .iter()
        .map(|row| row.write().expect("no thread panics while holding a row"))
        .collect();
    let src: Vec<&[f32]> = src.iter().map(|row| &row[sx..sx + lenx]).collect();
    let mut dst: Vec<&mut [f32]> = dst.iter_mut().map(|row| &mut row[dx..dx + lenx]).collect();
    if pass.columns {
        pass.plan.apply_columns_range(&src, &mut dst, first..last);
        Ok(())
    } else {
        pass.plan.apply_rows(&src, &mut dst)
    }
}

/// Forward transform of the whole image in place, one wavelet tree node
/// at a time, on up to `threads` threads.
///
/// `tmp`, with as many rows as `data`, is scratch space, whatever it holds.
/// Fails, leaving `data` unusable, if the transform could not allocate its
/// transposition blocks.
pub(crate) fn decompose(
    data: &mut [&mut [f32]],
    tmp: &mut [&mut [f32]],
    tree: &[WNode; W_TREE_LEN],
    threads: usize,
) -> Result<(), OutOfMemory> {
    let mut passes = Vec::with_capacity(2 * tree.len());
    for node in tree {
        let (x, y, lenx, leny) = (node.x, node.y, node.lenx, node.leny);
        // Rows: image -> tmp, then columns: tmp -> image.
        for (columns, len, inv) in [(false, lenx, node.inv_rw), (true, leny, node.inv_cl)] {
            passes.push(Pass {
                plan: analysis_plan(len, &LOFILT, &HIFILT, inv),
                columns,
                to_tmp: !columns,
                x,
                y,
                lenx,
                leny,
            });
        }
    }
    run(&passes, data, tmp, threads)
}

/// Inverse transform of the whole image in place, on up to `threads`
/// threads. `lo` and `hi` are the synthesis filters as read from the
/// transform table. `tmp`, with as many rows as `data`, is scratch space,
/// whatever it holds. Fails, like [`decompose`], if it could not allocate.
pub(crate) fn reconstruct(
    data: &mut [&mut [f32]],
    tmp: &mut [&mut [f32]],
    tree: &[WNode; W_TREE_LEN],
    lo: &[f32],
    hi: &[f32],
    threads: usize,
) -> Result<(), OutOfMemory> {
    let mut passes = Vec::with_capacity(2 * tree.len());
    for node in tree.iter().rev() {
        let (x, y, lenx, leny) = (node.x, node.y, node.lenx, node.leny);
        // Columns: image -> tmp, then rows: tmp -> image.
        for (columns, len, inv) in [(true, leny, node.inv_cl), (false, lenx, node.inv_rw)] {
            passes.push(Pass {
                plan: synthesis_plan(len, lo, hi, inv),
                columns,
                to_tmp: columns,
                x,
                y,
                lenx,
                leny,
            });
        }
    }
    run(&passes, data, tmp, threads)
}
