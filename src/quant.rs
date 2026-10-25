//! Subband variances, bit allocation, quantization and its inverse.
//!
//! Intermediate values keep the single and double precision mix of the
//! NIST reference implementation, so quantization decisions, and thus the
//! encoded bytes, come out the same.

use crate::tree::{NUM_SUBBANDS, Q_TREE_LEN, QNode, START_SUBBAND_3};

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

/// Variance of a `lenx` by `leny` window of the image at (`x`, `y`).
fn window_variance(
    data: &[f32],
    width: usize,
    x: usize,
    y: usize,
    lenx: usize,
    leny: usize,
) -> f32 {
    let mut sum = 0.0f32;
    let mut ssq = 0.0f32;
    for row in y..y + leny {
        for &v in &data[row * width + x..row * width + x + lenx] {
            sum += v;
            ssq += v * v;
        }
    }
    let n = lenx * leny;
    let sum2 = (sum * sum) / n as f32;
    (f64::from(ssq - sum2) / (n as f64 - 1.0)) as f32
}

/// Variance of a subband, either over the whole subband or over the
/// central window the specification prescribes. The window is empty for
/// subbands of images below the encoder's minimum size.
fn subband_variance(data: &[f32], width: usize, node: &QNode, cropped: bool) -> f32 {
    if cropped {
        let skipx = node.lenx / 8;
        let skipy = (9 * node.leny) / 32;
        let lenx = (3 * node.lenx) / 4;
        let leny = (7 * node.leny) / 16;
        window_variance(data, width, node.x + skipx, node.y + skipy, lenx, leny)
    } else {
        window_variance(data, width, node.x, node.y, node.lenx, node.leny)
    }
}

/// Variances of the 60 subbands of a transformed image.
pub(crate) fn variances(
    data: &[f32],
    width: usize,
    q_tree: &[QNode; Q_TREE_LEN],
) -> [f32; Q_TREE_LEN] {
    let mut var = [0.0f32; Q_TREE_LEN];
    let mut vsum = 0.0f32;
    for (v, node) in var.iter_mut().zip(q_tree).take(4) {
        *v = subband_variance(data, width, node, true);
        vsum += *v;
    }
    // Images with little energy in the lowest bands use whole subbands.
    let (from, cropped) = if vsum < 20000.0 {
        (0, false)
    } else {
        (4, true)
    };
    for (v, node) in var.iter_mut().zip(q_tree).take(NUM_SUBBANDS).skip(from) {
        *v = subband_variance(data, width, node, cropped);
    }
    var
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
            qbss[cnt] = (10.0 / f64::from(weight[cnt] * f64::from(var[cnt]).ln() as f32)) as f32;
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
            p = (f64::from(p) * f64::from(sigma[c] / qbss[c]).powf(f64::from(m(c)))) as f32;
        }
        let q = ((2.0f64.powf(f64::from(bitrate / s) - 1.0) / 2.5)
            / f64::from(p).powf(1.0 / f64::from(s))) as f32;
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
pub(crate) fn limit_bin_widths(
    params: &mut QuantParams,
    var: &[f32; Q_TREE_LEN],
    data: &[f32],
    width: usize,
    q_tree: &[QNode; Q_TREE_LEN],
) {
    for (cnt, node) in q_tree.iter().enumerate().take(NUM_SUBBANDS) {
        if var[cnt] < VARIANCE_THRESH {
            continue;
        }
        let largest = (node.y..node.y + node.leny)
            .flat_map(|row| &data[row * width + node.x..row * width + node.x + node.lenx])
            .fold(0.0f32, |m, &v| m.max(v.abs()));
        let narrowest = largest / MAX_QUANTIZED;
        #[allow(clippy::neg_cmp_op_on_partial_ord)] // also catches NaN
        if !(params.qbss[cnt] >= narrowest) {
            params.qbss[cnt] = narrowest;
            params.qzbs[cnt] = (1.2 * f64::from(narrowest)) as f32;
        }
    }
}

/// Quantizes the transformed image, subband by subband in row-major
/// order, skipping dropped subbands.
pub(crate) fn quantize(
    data: &[f32],
    width: usize,
    q_tree: &[QNode; Q_TREE_LEN],
    params: &QuantParams,
) -> Vec<i16> {
    let mut out = Vec::with_capacity(data.len());
    for (cnt, node) in q_tree.iter().enumerate().take(NUM_SUBBANDS) {
        let qbin = params.qbss[cnt];
        if qbin == 0.0 {
            continue;
        }
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
    out
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

/// Turns quantized coefficients back into a transformed image. `coeffs`
/// holds [`QuantTable::coefficient_count`] coefficients.
pub(crate) fn unquantize(
    coeffs: &[i16],
    table: &QuantTable,
    width: usize,
    height: usize,
    q_tree: &[QNode; Q_TREE_LEN],
) -> Vec<f32> {
    let mut data = vec![0.0f32; width * height];
    let mut coeffs = coeffs;
    let c = table.bin_center;
    for (cnt, node) in q_tree.iter().enumerate().take(NUM_SUBBANDS) {
        let qbin = table.q_bin[cnt];
        if qbin == 0.0 {
            continue;
        }
        let half_z = f64::from(table.z_bin[cnt]) / 2.0;
        for row in node.y..node.y + node.leny {
            let (line, rest) = coeffs.split_at(node.lenx.min(coeffs.len()));
            coeffs = rest;
            let out = &mut data[row * width + node.x..row * width + node.x + node.lenx];
            for (out, &s) in out.iter_mut().zip(line) {
                *out = match s.signum() {
                    0 => 0.0,
                    1 => (f64::from(qbin * (f32::from(s) - c)) + half_z) as f32,
                    _ => (f64::from(qbin * (f32::from(s) + c)) - half_z) as f32,
                };
            }
        }
    }
    data
}
