//! The wavelet tree and the quantization tree.
//!
//! WSQ splits the image with a fixed sequence of 20 two-dimensional
//! analysis steps (the wavelet tree), which leaves 64 rectangular subbands
//! (the quantization tree). Only the first 60 of them carry coefficients.
//! Both trees depend on nothing but the image dimensions, and the way odd
//! lengths are split is part of the format, so encoder and decoder must
//! build them identically.

/// Number of nodes in the wavelet tree.
pub(crate) const W_TREE_LEN: usize = 20;
/// Number of nodes in the quantization tree.
pub(crate) const Q_TREE_LEN: usize = 64;
/// Number of subbands that carry coefficients.
pub(crate) const NUM_SUBBANDS: usize = 60;
/// First subband of the second compressed block.
pub(crate) const START_SUBBAND_2: usize = 19;
/// First subband of the third compressed block.
pub(crate) const START_SUBBAND_3: usize = 52;

/// One step of the wavelet decomposition: the region it splits and
/// whether the high-pass half comes first along rows or columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WNode {
    pub x: usize,
    pub y: usize,
    pub lenx: usize,
    pub leny: usize,
    pub inv_rw: bool,
    pub inv_cl: bool,
}

/// One subband: a rectangle of the transformed image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QNode {
    pub x: usize,
    pub y: usize,
    pub lenx: usize,
    pub leny: usize,
}

impl QNode {
    pub fn area(&self) -> usize {
        self.lenx * self.leny
    }
}

/// Both trees for one image size.
#[derive(Clone, Debug)]
pub(crate) struct Trees {
    pub w: [WNode; W_TREE_LEN],
    pub q: [QNode; Q_TREE_LEN],
}

impl Trees {
    pub fn new(width: usize, height: usize) -> Self {
        let w = build_w_tree(width, height);
        let q = build_q_tree(&w);
        Trees { w, q }
    }
}

fn build_w_tree(width: usize, height: usize) -> [WNode; W_TREE_LEN] {
    let mut t = [WNode::default(); W_TREE_LEN];
    for i in [2, 4, 7, 9, 11, 13, 16, 18] {
        t[i].inv_rw = true;
    }
    for i in [3, 5, 8, 9, 12, 13, 17, 18] {
        t[i].inv_cl = true;
    }

    w_tree4(&mut t, 0, 1, width, height, 0, 0, true);

    let (lenx, lenx2) = if t[1].lenx % 2 == 0 {
        (t[1].lenx / 2, t[1].lenx / 2)
    } else {
        let l = t[1].lenx.div_ceil(2);
        (l, l - 1)
    };
    let (leny, leny2) = if t[1].leny % 2 == 0 {
        (t[1].leny / 2, t[1].leny / 2)
    } else {
        let l = t[1].leny.div_ceil(2);
        (l, l - 1)
    };

    w_tree4(&mut t, 4, 6, lenx2, leny, lenx, 0, false);
    w_tree4(&mut t, 5, 10, lenx, leny2, 0, leny, false);
    w_tree4(&mut t, 14, 15, lenx, leny, 0, 0, false);

    t[19].x = 0;
    t[19].y = 0;
    t[19].lenx = t[15].lenx.div_ceil(2);
    t[19].leny = t[15].leny.div_ceil(2);
    t
}

/// Fills node `p1` with the given region and nodes `p2..p2+4` with its
/// four quadrants (only three when `stop` is set).
#[allow(clippy::too_many_arguments)]
fn w_tree4(
    t: &mut [WNode; W_TREE_LEN],
    p1: usize,
    p2: usize,
    lenx: usize,
    leny: usize,
    x: usize,
    y: usize,
    stop: bool,
) {
    t[p1].x = x;
    t[p1].y = y;
    t[p1].lenx = lenx;
    t[p1].leny = leny;

    t[p2].x = x;
    t[p2 + 2].x = x;
    t[p2].y = y;
    t[p2 + 1].y = y;

    if lenx % 2 == 0 {
        t[p2].lenx = lenx / 2;
        t[p2 + 1].lenx = t[p2].lenx;
    } else if p1 == 4 {
        t[p2].lenx = (lenx - 1) / 2;
        t[p2 + 1].lenx = t[p2].lenx + 1;
    } else {
        t[p2].lenx = lenx.div_ceil(2);
        t[p2 + 1].lenx = t[p2].lenx - 1;
    }
    t[p2 + 1].x = t[p2].lenx + x;
    if !stop {
        t[p2 + 3].lenx = t[p2 + 1].lenx;
        t[p2 + 3].x = t[p2 + 1].x;
    }
    t[p2 + 2].lenx = t[p2].lenx;

    if leny % 2 == 0 {
        t[p2].leny = leny / 2;
        t[p2 + 2].leny = t[p2].leny;
    } else if p1 == 5 {
        t[p2].leny = (leny - 1) / 2;
        t[p2 + 2].leny = t[p2].leny + 1;
    } else {
        t[p2].leny = leny.div_ceil(2);
        t[p2 + 2].leny = t[p2].leny - 1;
    }
    t[p2 + 2].y = t[p2].leny + y;
    if !stop {
        t[p2 + 3].leny = t[p2 + 2].leny;
        t[p2 + 3].y = t[p2 + 2].y;
    }
    t[p2 + 1].leny = t[p2].leny;
}

fn build_q_tree(w: &[WNode; W_TREE_LEN]) -> [QNode; Q_TREE_LEN] {
    let mut q = [QNode::default(); Q_TREE_LEN];
    q_tree16(&mut q, 3, &w[14], false, false);
    q_tree16(&mut q, 19, &w[4], false, true);
    q_tree16(&mut q, 48, &w[0], false, false);
    q_tree16(&mut q, 35, &w[5], true, false);
    q_tree4(&mut q, 0, w[19].lenx, w[19].leny, w[19].x, w[19].y);
    q
}

/// Splits a region in two along one axis. `low_first` decides which half
/// takes the extra sample of an odd length.
fn split(len: usize, low_first: bool) -> (usize, usize) {
    if len % 2 == 0 {
        (len / 2, len / 2)
    } else if low_first {
        let first = len.div_ceil(2);
        (first, first - 1)
    } else {
        let second = len.div_ceil(2);
        (second - 1, second)
    }
}

/// Fills `q[p..p+16]` with the 16 subbands of one wavelet region.
fn q_tree16(q: &mut [QNode; Q_TREE_LEN], p: usize, node: &WNode, rw: bool, cl: bool) {
    let (x, y) = (node.x, node.y);
    let (tempx, temp2x) = split(node.lenx, !cl);
    let (tempy, temp2y) = split(node.leny, !rw);

    // Upper-left quadrant: subbands p..p+4.
    let (ax, bx) = split(tempx, true);
    let (ay, by) = split(tempy, true);
    q[p] = QNode {
        x,
        y,
        lenx: ax,
        leny: ay,
    };
    q[p + 1] = QNode {
        x: x + ax,
        y,
        lenx: bx,
        leny: ay,
    };
    q[p + 2] = QNode {
        x,
        y: y + ay,
        lenx: ax,
        leny: by,
    };
    q[p + 3] = QNode {
        x: x + ax,
        y: y + ay,
        lenx: bx,
        leny: by,
    };

    // Upper-right quadrant: subbands p+4..p+8.
    let (cx, dx) = split(temp2x, false);
    let x2 = x + tempx;
    q[p + 4] = QNode {
        x: x2,
        y,
        lenx: cx,
        leny: ay,
    };
    q[p + 5] = QNode {
        x: x2 + cx,
        y,
        lenx: dx,
        leny: ay,
    };
    q[p + 6] = QNode {
        x: x2,
        y: y + ay,
        lenx: cx,
        leny: by,
    };
    q[p + 7] = QNode {
        x: x2 + cx,
        y: y + ay,
        lenx: dx,
        leny: by,
    };

    // Lower-left quadrant: subbands p+8..p+12.
    let (cy, dy) = split(temp2y, false);
    let y2 = y + tempy;
    q[p + 8] = QNode {
        x,
        y: y2,
        lenx: ax,
        leny: cy,
    };
    q[p + 9] = QNode {
        x: x + ax,
        y: y2,
        lenx: bx,
        leny: cy,
    };
    q[p + 10] = QNode {
        x,
        y: y2 + cy,
        lenx: ax,
        leny: dy,
    };
    q[p + 11] = QNode {
        x: x + ax,
        y: y2 + cy,
        lenx: bx,
        leny: dy,
    };

    // Lower-right quadrant: subbands p+12..p+16.
    q[p + 12] = QNode {
        x: x2,
        y: y2,
        lenx: cx,
        leny: cy,
    };
    q[p + 13] = QNode {
        x: x2 + cx,
        y: y2,
        lenx: dx,
        leny: cy,
    };
    q[p + 14] = QNode {
        x: x2,
        y: y2 + cy,
        lenx: cx,
        leny: dy,
    };
    q[p + 15] = QNode {
        x: x2 + cx,
        y: y2 + cy,
        lenx: dx,
        leny: dy,
    };
}

/// Fills `q[p..p+4]` with the four quadrants of a region.
fn q_tree4(q: &mut [QNode; Q_TREE_LEN], p: usize, lenx: usize, leny: usize, x: usize, y: usize) {
    let (ax, bx) = split(lenx, true);
    let (ay, by) = split(leny, true);
    q[p] = QNode {
        x,
        y,
        lenx: ax,
        leny: ay,
    };
    q[p + 1] = QNode {
        x: x + ax,
        y,
        lenx: bx,
        leny: ay,
    };
    q[p + 2] = QNode {
        x,
        y: y + ay,
        lenx: ax,
        leny: by,
    };
    q[p + 3] = QNode {
        x: x + ax,
        y: y + ay,
        lenx: bx,
        leny: by,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 60 subbands tile the transformed image, except for the finest
    /// diagonal band, which WSQ drops.
    fn assert_tiles(width: usize, height: usize) {
        let trees = Trees::new(width, height);
        let mut seen = vec![0u8; width * height];
        for node in &trees.q[..NUM_SUBBANDS] {
            for row in node.y..node.y + node.leny {
                for col in node.x..node.x + node.lenx {
                    seen[row * width + col] += 1;
                }
            }
        }
        let (lx, ly) = (trees.w[1].lenx, trees.w[1].leny);
        for (i, &n) in seen.iter().enumerate() {
            let dropped = i % width >= lx && i / width >= ly;
            assert_eq!(n, u8::from(!dropped), "{width}x{height}: pixel {i}");
        }
    }

    #[test]
    fn subbands_tile_the_image() {
        for (w, h) in [
            (256, 256),
            (500, 500),
            (257, 255),
            (545, 622),
            (81, 81),
            (33, 97),
        ] {
            assert_tiles(w, h);
        }
    }

    /// From 81 pixels on, every subband is large enough for the window
    /// the encoder estimates its variance over. Lengths along one axis
    /// depend on that dimension only.
    #[test]
    fn variance_windows_from_81_pixels() {
        let window = |n: &QNode| ((3 * n.lenx) / 4, (7 * n.leny) / 16);
        for d in 81..=usize::from(u16::MAX) {
            let trees = Trees::new(d, d);
            assert!(
                trees.q[..NUM_SUBBANDS]
                    .iter()
                    .all(|n| window(n).0 >= 1 && window(n).1 >= 1),
                "{d}"
            );
        }
        for w in 81..=200 {
            for h in 81..=200 {
                let trees = Trees::new(w, h);
                assert!(
                    trees.q[..NUM_SUBBANDS]
                        .iter()
                        .all(|n| window(n).0 * window(n).1 >= 2),
                    "{w}x{h}"
                );
            }
        }
        let trees = Trees::new(80, 80);
        assert!(trees.q[..NUM_SUBBANDS].iter().any(|n| window(n).1 == 0));
    }

    #[test]
    fn corner_nodes_of_a_square_image() {
        let trees = Trees::new(512, 512);
        assert_eq!(
            trees.w[19],
            WNode {
                x: 0,
                y: 0,
                lenx: 32,
                leny: 32,
                ..WNode::default()
            }
        );
        assert_eq!(
            trees.q[0],
            QNode {
                x: 0,
                y: 0,
                lenx: 16,
                leny: 16
            }
        );
        assert_eq!(
            trees.q[59],
            QNode {
                x: 128,
                y: 384,
                lenx: 128,
                leny: 128
            }
        );
    }
}
