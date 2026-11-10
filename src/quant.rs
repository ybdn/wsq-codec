//! Subband variances, bit allocation, quantization and its inverse.
//!
//! Intermediate values keep the single and double precision mix of the
//! NIST reference implementation, so quantization decisions, and thus the
//! encoded bytes, come out the same. Logarithms and powers come from
//! [`math`], not from the platform's math library, so they also come out
//! the same on every platform.

use crate::mem::{self, OutOfMemory};
use crate::tree::{NUM_SUBBANDS, Q_TREE_LEN, QNode, START_SUBBAND_3};
use crate::{math, par};

/// Subbands whose variance is below this carry no information and are
/// dropped.
const VARIANCE_THRESH: f32 = 1.01;
/// First subband of the second and third size regions used by the bit
/// allocation.
const START_SIZE_REGION_2: usize = 4;
const START_SIZE_REGION_3: usize = 51;

/// Quantizer bin widths chosen by the encoder.
pub(crate) struct QuantParams {
    /// Bin width per subband; zero means the subband is dropped.
    pub qbss: [f32; Q_TREE_LEN],
    /// Zero-bin width per subband.
    pub qzbs: [f32; Q_TREE_LEN],
}

/// Variance of a subband, over the whole subband or over the central
/// window the specification prescribes, and the largest magnitude of its
/// coefficients, over the whole subband, in one pass. The window is empty
/// for subbands of images below the encoder's minimum size.
///
/// The sums run row by row, left to right, over the window only, as in
/// the reference implementation.
fn subband_stats(data: &[&[f32]], node: &QNode, cropped: bool) -> (f32, f32) {
    let (skipx, skipy, lenx, leny) = if cropped {
        (
            node.lenx / 8,
            (9 * node.leny) / 32,
            (3 * node.lenx) / 4,
            (7 * node.leny) / 16,
        )
    } else {
        (0, 0, node.lenx, node.leny)
    };
    let mut sum = 0.0f32;
    let mut ssq = 0.0f32;
    let mut largest = 0.0f32;
    for (i, row) in (node.y..node.y + node.leny).enumerate() {
        let line = &data[row][node.x..node.x + node.lenx];
        largest = line.iter().fold(largest, |m, &v| m.max(v.abs()));
        if (skipy..skipy + leny).contains(&i) {
            for &v in &line[skipx..skipx + lenx] {
                sum += v;
                ssq += v * v;
            }
        }
    }
    let n = lenx * leny;
    let sum2 = (sum * sum) / n as f32;
    let var = (f64::from(ssq - sum2) / (n as f64 - 1.0)) as f32;
    (var, largest)
}

/// Statistics of the 60 subbands of a transformed image, as
/// [`variances`] gives them.
pub(crate) struct Stats {
    /// Variance per subband, as the bit allocation uses it.
    pub var: [f32; Q_TREE_LEN],
    /// Largest coefficient magnitude per subband.
    pub largest: [f32; Q_TREE_LEN],
}

/// Variances and largest magnitudes of the 60 subbands of a transformed
/// image, on up to `threads` threads. Each subband is computed on one
/// thread exactly as on a single one, so the result does not depend on
/// the threads.
pub(crate) fn variances(data: &[&[f32]], q_tree: &[QNode; Q_TREE_LEN], threads: usize) -> Stats {
    let mut var = [0.0f32; Q_TREE_LEN];
    let mut largest = [0.0f32; Q_TREE_LEN];
    let mut vsum = 0.0f32;
    for (cnt, node) in q_tree.iter().enumerate().take(4) {
        (var[cnt], largest[cnt]) = subband_stats(data, node, true);
        vsum += var[cnt];
    }
    // Images with little energy in the lowest bands use whole subbands.
    let (from, cropped) = if vsum < 20000.0 {
        (0, false)
    } else {
        (4, true)
    };
    // Largest subbands first, so that the threads finish together.
    let mut order: Vec<usize> = (0..NUM_SUBBANDS - from).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(q_tree[from + i].area()));
    let total = q_tree[from..NUM_SUBBANDS].iter().map(QNode::area).sum();
    let stats = par::map_dynamic(&order, par::parts(threads, total), |i| {
        subband_stats(data, &q_tree[from + i], cropped)
    });
    for (i, (v, m)) in stats.into_iter().enumerate() {
        (var[from + i], largest[from + i]) = (v, m);
    }
    Stats { var, largest }
}

/// Chooses the bin widths that spend about `bitrate` bits per pixel.
pub(crate) fn bin_widths(var: &[f32; Q_TREE_LEN], bitrate: f32) -> QuantParams {
    let mut weight = [1.0f32; NUM_SUBBANDS];
    weight[START_SUBBAND_3..].copy_from_slice(&[1.32, 1.08, 1.42, 1.08, 1.32, 1.42, 1.08, 1.08]);

    let mut qbss = [0.0f32; Q_TREE_LEN];
    let mut qzbs = [0.0f32; Q_TREE_LEN];
    for cnt in 0..NUM_SUBBANDS {
        if var[cnt] < VARIANCE_THRESH {
            qbss[cnt] = 0.0;
        } else if cnt < START_SIZE_REGION_2 {
            qbss[cnt] = 1.0;
        } else {
            qbss[cnt] =
                (10.0 / f64::from(weight[cnt] * math::ln(f64::from(var[cnt])) as f32)) as f32;
        }
    }

    // Reciprocals of each subband's share of the image.
    let m = |cnt: usize| -> f32 {
        if cnt < START_SIZE_REGION_2 {
            (1.0 / 1024.0) as f32
        } else if cnt < START_SIZE_REGION_3 {
            (1.0 / 256.0) as f32
        } else {
            (1.0 / 16.0) as f32
        }
    };

    let mut sigma = [0.0f32; NUM_SUBBANDS];
    let k0: Vec<usize> = (0..NUM_SUBBANDS)
        .filter(|&c| var[c] >= VARIANCE_THRESH)
        .collect();
    for &c in &k0 {
        sigma[c] = f64::from(var[c]).sqrt() as f32;
    }

    // Drop subbands that would get a non-positive bit rate until none do.
    let mut k = k0.clone();
    let q = loop {
        let mut s = 0.0f32;
        for &c in &k {
            s += m(c);
        }
        let mut p = 1.0f32;
        for &c in &k {
            p = (f64::from(p) * math::pow(f64::from(sigma[c] / qbss[c]), f64::from(m(c)))) as f32;
        }
        let q = ((math::pow(2.0, f64::from(bitrate / s) - 1.0) / 2.5)
            / math::pow(f64::from(p), 1.0 / f64::from(s))) as f32;
        let before = k.len();
        #[allow(clippy::neg_cmp_op_on_partial_ord)] // keeps NaN handling of the reference
        k.retain(|&c| !(f64::from(qbss[c] / q) >= 5.0 * f64::from(sigma[c])));
        if k.len() == before {
            break q;
        }
    };

    for cnt in 0..NUM_SUBBANDS {
        if k0.contains(&cnt) {
            qbss[cnt] /= q;
        } else {
            qbss[cnt] = 0.0;
        }
        qzbs[cnt] = (1.2 * f64::from(qbss[cnt])) as f32;
    }
    QuantParams { qbss, qzbs }
}

/// Largest magnitude [`limit_bin_widths`] lets a quantized coefficient
/// reach, just short of what 16 bits hold.
const MAX_QUANTIZED: f32 = 32000.0;

/// Widens the bins of subbands whose coefficients would not fit in 16
/// bits once quantized.
///
/// The bit allocation only asks for such bins on images with almost no
/// detail, such as smooth gradients: the few subbands with any variance
/// cannot absorb the requested rate, and their bins shrink without bound,
/// down to zero when the proportionality constant overflows. The
/// reference implementation then wraps coefficients around or drops every
/// subband and decodes a flat image. On other images this changes nothing.
pub(crate) fn limit_bin_widths(params: &mut QuantParams, stats: &Stats) {
    for cnt in 0..NUM_SUBBANDS {
        if stats.var[cnt] < VARIANCE_THRESH {
            continue;
        }
        let narrowest = stats.largest[cnt] / MAX_QUANTIZED;
        #[allow(clippy::neg_cmp_op_on_partial_ord)] // also catches NaN
        if !(params.qbss[cnt] >= narrowest) {
            params.qbss[cnt] = narrowest;
            params.qzbs[cnt] = (1.2 * f64::from(narrowest)) as f32;
        }
    }
}

/// Appends the quantized coefficients of subband `cnt` to `out`, in
/// row-major order.
fn quantize_subband(
    data: &[&[f32]],
    node: &QNode,
    params: &QuantParams,
    cnt: usize,
    out: &mut Vec<i16>,
) {
    let qbin = params.qbss[cnt];
    let zbin = (f64::from(params.qzbs[cnt]) / 2.0) as f32;
    for row in &data[node.y..node.y + node.leny] {
        for &v in &row[node.x..node.x + node.lenx] {
            let q = if -zbin <= v && v <= zbin {
                0.0
            } else if v > 0.0 {
                f64::from((v - zbin) / qbin) + 1.0
            } else {
                f64::from((v + zbin) / qbin) - 1.0
            };
            out.push(q as i16);
        }
    }
}

/// Quantizes the transformed image, subband by subband in row-major
/// order, skipping dropped subbands. Runs of consecutive subbands go to
/// up to `threads` threads.
pub(crate) fn quantize(
    data: &[&[f32]],
    q_tree: &[QNode; Q_TREE_LEN],
    params: &QuantParams,
    threads: usize,
) -> Result<Vec<i16>, OutOfMemory> {
    let kept: Vec<usize> = (0..NUM_SUBBANDS)
        .filter(|&c| params.qbss[c] != 0.0)
        .collect();
    let total: usize = kept.iter().map(|&c| q_tree[c].area()).sum();
    let run = |subbands: &[usize]| {
        let len = subbands.iter().map(|&c| q_tree[c].area()).sum();
        let mut out = mem::with_capacity(len)?;
        for &c in subbands {
            quantize_subband(data, &q_tree[c], params, c, &mut out);
        }
        Ok(out)
    };
    let parts = par::parts(threads, total);
    if parts == 1 {
        return run(&kept);
    }
    // Cut the subbands into runs of about equal area.
    let mut runs = Vec::new();
    let (mut start, mut area) = (0, 0);
    for (i, &c) in kept.iter().enumerate() {
        area += q_tree[c].area();
        if area * parts >= total * (runs.len() + 1) && runs.len() + 1 < parts {
            runs.push(&kept[start..=i]);
            start = i + 1;
        }
    }
    runs.push(&kept[start..]);
    std::thread::scope(|scope| {
        let handles: Vec<_> = runs
            .iter()
            .map(|subbands| par::spawn(scope, || run(subbands)))
            .collect();
        let mut out = mem::with_capacity(total)?;
        for handle in handles {
            out.extend(handle.join()?);
        }
        Ok(out)
    })
}

/// Quantization table as read from a file.
#[derive(Clone, Debug)]
pub(crate) struct QuantTable {
    pub bin_center: f32,
    pub q_bin: [f32; Q_TREE_LEN],
    pub z_bin: [f32; Q_TREE_LEN],
}

impl QuantTable {
    /// Number of coefficients the subbands of `q_tree` hold.
    pub fn coefficient_count(&self, q_tree: &[QNode; Q_TREE_LEN]) -> usize {
        (0..NUM_SUBBANDS)
            .filter(|&c| self.q_bin[c] != 0.0)
            .map(|c| q_tree[c].area())
            .sum()
    }
}

/// Turns quantized coefficients back into a transformed image, written
/// into `data` whatever it holds, on up to `threads` threads. `coeffs`
/// holds [`QuantTable::coefficient_count`] coefficients, in consecutive
/// runs; missing ones count as zero.
///
/// Each sample depends on its own coefficient only, so the image is cut
/// into bands of rows, one per thread, each filled row by row.
pub(crate) fn unquantize(
    coeffs: &[Vec<i16>],
    table: &QuantTable,
    q_tree: &[QNode; Q_TREE_LEN],
    data: &mut [&mut [f32]],
    threads: usize,
) {
    /// A kept subband and where its coefficients start.
    struct Kept<'a> {
        node: &'a QNode,
        start: usize,
        qbin: f32,
        half_z: f64,
    }
    let mut kept = Vec::new();
    let mut start = 0;
    for (cnt, node) in q_tree.iter().enumerate().take(NUM_SUBBANDS) {
        let qbin = table.q_bin[cnt];
        if qbin == 0.0 {
            continue;
        }
        kept.push(Kept {
            node,
            start,
            qbin,
            half_z: f64::from(table.z_bin[cnt]) / 2.0,
        });
        start += node.area();
    }
    let c = table.bin_center;
    let coeffs = Runs::new(coeffs);
    let fill = |first_row: usize, rows: &mut [&mut [f32]]| {
        let mut joined = Vec::new();
        for (i, line) in rows.iter_mut().enumerate() {
            let row = first_row + i;
            line.fill(0.0);
            for k in kept
                .iter()
                .filter(|k| (k.node.y..k.node.y + k.node.leny).contains(&row))
            {
                let (node, qbin, half_z) = (k.node, k.qbin, k.half_z);
                let from = k.start + (row - node.y) * node.lenx;
                let out = &mut line[node.x..node.x + node.lenx];
                for (out, &s) in out.iter_mut().zip(coeffs.get(from, node.lenx, &mut joined)) {
                    *out = match s.signum() {
                        0 => 0.0,
                        1 => (f64::from(qbin * (f32::from(s) - c)) + half_z) as f32,
                        _ => (f64::from(qbin * (f32::from(s) + c)) - half_z) as f32,
                    };
                }
            }
        }
    };
    let items = data.len() * data.first().map_or(0, |row| row.len());
    par::row_groups(data, items, threads, fill);
}

/// Coefficients held in consecutive runs, read as one sequence.
struct Runs<'a> {
    runs: &'a [Vec<i16>],
    /// Where each run ends in the sequence.
    ends: Vec<usize>,
}

impl<'a> Runs<'a> {
    fn new(runs: &'a [Vec<i16>]) -> Self {
        let ends = runs
            .iter()
            .scan(0, |end, run| {
                *end += run.len();
                Some(*end)
            })
            .collect();
        Runs { runs, ends }
    }

    /// Up to `len` coefficients from position `from`, fewer at the end of
    /// the sequence. Copied into `joined` when they straddle two runs.
    fn get<'s>(&'s self, from: usize, len: usize, joined: &'s mut Vec<i16>) -> &'s [i16] {
        let total = self.ends.last().copied().unwrap_or(0);
        let (from, to) = (from.min(total), (from + len).min(total));
        let i = self.ends.partition_point(|&end| end <= from);
        let Some(run) = self.runs.get(i) else {
            return &[];
        };
        let start = self.ends[i] - run.len();
        if to <= self.ends[i] {
            return &run[from - start..to - start];
        }
        joined.clear();
        let mut at = from;
        for (run, &end) in self.runs[i..].iter().zip(&self.ends[i..]) {
            let start = end - run.len();
            let stop = to.min(end);
            joined.extend_from_slice(&run[at - start..stop - start]);
            at = stop;
            if at == to {
                break;
            }
        }
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tree::Trees;

    const SIDE: usize = 81;

    /// Rows of a `SIDE` x `SIDE` image filled by `f(x, y)`.
    fn image(f: impl Fn(usize, usize) -> f32) -> Vec<Vec<f32>> {
        (0..SIDE)
            .map(|y| (0..SIDE).map(|x| f(x, y)).collect())
            .collect()
    }

    fn rows(image: &[Vec<f32>]) -> Vec<&[f32]> {
        image.iter().map(Vec::as_slice).collect()
    }

    /// Deterministic values in -100..100.
    fn noise(x: usize, y: usize) -> f32 {
        let h = (x * 7919 + y * 104_729 + x * y * 31) % 2003;
        h as f32 / 10.0 - 100.0
    }

    #[test]
    fn flat_image_has_zero_variance_and_every_subband_is_dropped() {
        let trees = Trees::new(SIDE, SIDE);
        let flat = image(|_, _| 2.0);
        let stats = variances(&rows(&flat), &trees.q, 1);
        assert!(stats.var[..NUM_SUBBANDS].iter().all(|&v| v == 0.0));
        assert!(stats.largest[..NUM_SUBBANDS].iter().all(|&m| m == 2.0));
        let params = bin_widths(&stats.var, 0.75);
        assert!(params.qbss[..NUM_SUBBANDS].iter().all(|&w| w == 0.0));
        assert!(params.qzbs[..NUM_SUBBANDS].iter().all(|&w| w == 0.0));
    }

    #[test]
    fn nearly_flat_subband_stays_below_the_variance_threshold() {
        let trees = Trees::new(SIDE, SIDE);
        // One pixel off by one: the variance is far under 1.01.
        let flat = image(|x, y| if (x, y) == (40, 40) { 3.0 } else { 2.0 });
        let stats = variances(&rows(&flat), &trees.q, 1);
        assert!(
            stats.var[..NUM_SUBBANDS]
                .iter()
                .all(|&v| v < VARIANCE_THRESH)
        );
        let params = bin_widths(&stats.var, 0.75);
        assert!(params.qbss[..NUM_SUBBANDS].iter().all(|&w| w == 0.0));
    }

    #[test]
    fn variances_do_not_depend_on_the_threads() {
        let trees = Trees::new(SIDE, SIDE);
        let data = image(noise);
        let (one, many) = (
            variances(&rows(&data), &trees.q, 1),
            variances(&rows(&data), &trees.q, 4),
        );
        assert_eq!(one.var, many.var);
        assert_eq!(one.largest, many.largest);
    }

    #[test]
    fn bin_widths_follow_the_variance() {
        let mut var = [0.0f32; Q_TREE_LEN];
        var[..NUM_SUBBANDS].fill(500.0);
        var[10] = VARIANCE_THRESH - 0.01; // dropped
        let params = bin_widths(&var, 0.75);
        for cnt in 0..NUM_SUBBANDS {
            if cnt == 10 {
                assert_eq!((params.qbss[cnt], params.qzbs[cnt]), (0.0, 0.0));
            } else {
                assert!(
                    params.qbss[cnt] > 0.0 && params.qbss[cnt].is_finite(),
                    "{cnt}"
                );
                assert_eq!(params.qzbs[cnt], (1.2 * f64::from(params.qbss[cnt])) as f32);
            }
        }
        // A higher rate gives narrower bins.
        let finer = bin_widths(&var, 2.0);
        assert!(finer.qbss[20] < params.qbss[20]);
    }

    #[test]
    fn limit_widens_only_bins_too_narrow_for_16_bits() {
        let mut stats = Stats {
            var: [0.0; Q_TREE_LEN],
            largest: [0.0; Q_TREE_LEN],
        };
        let mut params = QuantParams {
            qbss: [0.0; Q_TREE_LEN],
            qzbs: [0.0; Q_TREE_LEN],
        };
        for cnt in 0..5 {
            stats.var[cnt] = 100.0;
            stats.largest[cnt] = 64000.0; // narrowest bin: 2.0
        }
        stats.var[5] = 0.5; // below the threshold: left alone
        stats.largest[5] = 64000.0;
        params.qbss[..5].copy_from_slice(&[0.5, 2.0, 5.0, 0.0, f32::NAN]);
        limit_bin_widths(&mut params, &stats);
        assert_eq!(params.qbss[..5], [2.0, 2.0, 5.0, 2.0, 2.0]);
        assert_eq!(params.qzbs[0], (1.2 * 2.0f64) as f32);
        assert_eq!(params.qzbs[2], 0.0); // untouched
        assert_eq!(params.qbss[5], 0.0);
    }

    /// Parameters that keep only the lowest subband.
    fn one_subband(qbin: f32, zbin: f32) -> QuantParams {
        let mut params = QuantParams {
            qbss: [0.0; Q_TREE_LEN],
            qzbs: [0.0; Q_TREE_LEN],
        };
        params.qbss[0] = qbin;
        params.qzbs[0] = zbin;
        params
    }

    #[test]
    fn quantize_uses_the_zero_bin_then_uniform_steps() {
        let trees = Trees::new(SIDE, SIDE);
        let node = trees.q[0];
        // Zero bin of total width 4 (half width 2), bins of width 3.
        let params = one_subband(3.0, 4.0);
        let values = [0.0, 2.0, -2.5, 2.5, 5.5, 1e9, -1e9, -5.0, -2.0];
        let want: [i16; 9] = [0, 0, -1, 1, 2, i16::MAX, i16::MIN, -2, 0];
        let data = image(|x, y| {
            let i = (y - node.y) * node.lenx + (x - node.x);
            if (node.x..node.x + node.lenx).contains(&x)
                && (node.y..node.y + node.leny).contains(&y)
                && i < values.len()
            {
                values[i]
            } else {
                0.0
            }
        });
        let out = quantize(&rows(&data), &trees.q, &params, 1).unwrap();
        assert_eq!(node.area(), values.len());
        assert_eq!(out, want);
    }

    #[test]
    fn quantize_skips_dropped_subbands_and_ignores_the_threads() {
        let trees = Trees::new(SIDE, SIDE);
        let data = image(noise);
        let mut params = one_subband(1.0, 1.2);
        for cnt in [7, 30, 55] {
            params.qbss[cnt] = 0.8;
            params.qzbs[cnt] = 0.96;
        }
        let one = quantize(&rows(&data), &trees.q, &params, 1).unwrap();
        let area: usize = [0, 7, 30, 55].iter().map(|&c| trees.q[c].area()).sum();
        assert_eq!(one.len(), area);
        for threads in [2, 3, 8] {
            assert_eq!(
                quantize(&rows(&data), &trees.q, &params, threads).unwrap(),
                one
            );
        }
    }

    #[test]
    fn dequantization_lands_within_one_bin_of_the_original() {
        let trees = Trees::new(SIDE, SIDE);
        let data = image(noise);
        let (qbin, zbin, center) = (3.0f32, 4.0f32, 0.44f32);
        let params = one_subband(qbin, zbin);
        let coeffs = quantize(&rows(&data), &trees.q, &params, 1).unwrap();
        let mut table = QuantTable {
            bin_center: center,
            q_bin: [0.0; Q_TREE_LEN],
            z_bin: [0.0; Q_TREE_LEN],
        };
        table.q_bin[0] = qbin;
        table.z_bin[0] = zbin;
        assert_eq!(table.coefficient_count(&trees.q), coeffs.len());

        let mut back = vec![vec![99.0f32; SIDE]; SIDE];
        let node = trees.q[0];
        for threads in [1, 3] {
            let mut view: Vec<&mut [f32]> = back.iter_mut().map(Vec::as_mut_slice).collect();
            unquantize(
                std::slice::from_ref(&coeffs),
                &table,
                &trees.q,
                &mut view,
                threads,
            );
            for y in 0..SIDE {
                for x in 0..SIDE {
                    let inside = (node.x..node.x + node.lenx).contains(&x)
                        && (node.y..node.y + node.leny).contains(&y);
                    if inside {
                        let err = (back[y][x] - data[y][x]).abs();
                        assert!(err < qbin, "({x},{y}): {err}");
                        if data[y][x].abs() <= zbin / 2.0 {
                            assert_eq!(back[y][x], 0.0);
                        }
                    } else {
                        assert_eq!(back[y][x], 0.0, "dropped area ({x},{y})");
                    }
                }
            }
        }
    }

    #[test]
    fn missing_coefficients_count_as_zero() {
        let trees = Trees::new(SIDE, SIDE);
        let mut table = QuantTable {
            bin_center: 0.44,
            q_bin: [0.0; Q_TREE_LEN],
            z_bin: [0.0; Q_TREE_LEN],
        };
        table.q_bin[0] = 1.0;
        table.z_bin[0] = 1.2;
        let mut back = vec![vec![7.0f32; SIDE]; SIDE];
        let mut view: Vec<&mut [f32]> = back.iter_mut().map(Vec::as_mut_slice).collect();
        unquantize(&[vec![5]], &table, &trees.q, &mut view, 1);
        let node = trees.q[0];
        assert!(back[node.y][node.x] > 4.0);
        assert!(back.iter().flatten().skip(1).all(|&v| v != 7.0));
        assert_eq!(back[node.y][node.x + 1], 0.0);
    }

    #[test]
    fn runs_read_as_one_sequence() {
        let runs = vec![vec![0, 1, 2], vec![], vec![3], vec![4, 5, 6, 7]];
        let all: Vec<i16> = (0..8).collect();
        let seq = Runs::new(&runs);
        let mut joined = Vec::new();
        for from in 0..10 {
            for len in 0..10 {
                let want = &all[from.min(8)..(from + len).min(8)];
                assert_eq!(seq.get(from, len, &mut joined), want, "{from} {len}");
            }
        }
    }
}
