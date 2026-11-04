//! Subband variances, bit allocation, quantization and its inverse.
//!
//! Intermediate values keep the single and double precision mix of the
//! NIST reference implementation, so quantization decisions, and thus the
//! encoded bytes, come out the same. Logarithms and powers come from
//! [`math`], not from the platform's math library, so they also come out
//! the same on every platform.

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
fn subband_stats(data: &[f32], width: usize, node: &QNode, cropped: bool) -> (f32, f32) {
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
        let line = &data[row * width + node.x..row * width + node.x + node.lenx];
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
pub(crate) fn variances(
    data: &[f32],
    width: usize,
    q_tree: &[QNode; Q_TREE_LEN],
    threads: usize,
) -> Stats {
    let mut var = [0.0f32; Q_TREE_LEN];
    let mut largest = [0.0f32; Q_TREE_LEN];
    let mut vsum = 0.0f32;
    for (cnt, node) in q_tree.iter().enumerate().take(4) {
        (var[cnt], largest[cnt]) = subband_stats(data, width, node, true);
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
        subband_stats(data, width, &q_tree[from + i], cropped)
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
    data: &[f32],
    width: usize,
    node: &QNode,
    params: &QuantParams,
    cnt: usize,
    out: &mut Vec<i16>,
) {
    let qbin = params.qbss[cnt];
    let zbin = (f64::from(params.qzbs[cnt]) / 2.0) as f32;
    for row in node.y..node.y + node.leny {
        for &v in &data[row * width + node.x..row * width + node.x + node.lenx] {
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
    data: &[f32],
    width: usize,
    q_tree: &[QNode; Q_TREE_LEN],
    params: &QuantParams,
    threads: usize,
) -> Vec<i16> {
    let kept: Vec<usize> = (0..NUM_SUBBANDS)
        .filter(|&c| params.qbss[c] != 0.0)
        .collect();
    let total: usize = kept.iter().map(|&c| q_tree[c].area()).sum();
    let run = |subbands: &[usize]| {
        let len = subbands.iter().map(|&c| q_tree[c].area()).sum();
        let mut out = Vec::with_capacity(len);
        for &c in subbands {
            quantize_subband(data, width, &q_tree[c], params, c, &mut out);
        }
        out
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
        let mut out = Vec::with_capacity(total);
        for handle in handles {
            out.extend(handle.join());
        }
        out
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
/// holds [`QuantTable::coefficient_count`] coefficients; missing ones
/// count as zero.
///
/// Each sample depends on its own coefficient only, so the image is cut
/// into bands of rows, one per thread, each filled row by row.
pub(crate) fn unquantize(
    coeffs: &[i16],
    table: &QuantTable,
    width: usize,
    q_tree: &[QNode; Q_TREE_LEN],
    data: &mut [f32],
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
    let fill = |first_row: usize, rows: &mut [f32]| {
        for (i, line) in rows.chunks_mut(width).enumerate() {
            let row = first_row + i;
            line.fill(0.0);
            for k in kept
                .iter()
                .filter(|k| (k.node.y..k.node.y + k.node.leny).contains(&row))
            {
                let (node, qbin, half_z) = (k.node, k.qbin, k.half_z);
                let from = (k.start + (row - node.y) * node.lenx).min(coeffs.len());
                let to = (from + node.lenx).min(coeffs.len());
                let out = &mut line[node.x..node.x + node.lenx];
                for (out, &s) in out.iter_mut().zip(&coeffs[from..to]) {
                    *out = match s.signum() {
                        0 => 0.0,
                        1 => (f64::from(qbin * (f32::from(s) - c)) + half_z) as f32,
                        _ => (f64::from(qbin * (f32::from(s) + c)) - half_z) as f32,
                    };
                }
            }
        }
    };
    let height = data.len().checked_div(width).unwrap_or(0);
    let parts = par::parts(threads, data.len());
    if parts == 1 {
        return fill(0, data);
    }
    let band = height.div_ceil(parts);
    let fill = &fill;
    std::thread::scope(|scope| {
        for (i, rows) in data.chunks_mut(band * width).enumerate() {
            drop(par::spawn(scope, move || fill(i * band, rows)));
        }
    });
}
