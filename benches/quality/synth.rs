//! A synthetic fingerprint-like image, with integer arithmetic only (so the pixels are the same
//! on every platform). It is not a fingerprint and models none: it is a ridge pattern with
//! known minutiae, there so that NFIQ 2 and a minutiae extractor have something to find.
//!
//! The ridges are concentric ellipses (a whorl-like pattern) around a centre. A fingerprint's
//! ridge endings and bifurcations are dislocations of that periodic pattern, so some points are
//! given one: the phase of the pattern gains `+-atan2(y - y0, x - x0)` around each of them, a
//! full period once round the point, which ends or splits one ridge there. The ridges are dark on
//! a light ground, with a grain of noise, a slow variation of the lighting, and a ground that
//! takes over outside an elliptical foreground, as on a rolled or flat print.

/// One turn of phase, in fixed-point units.
const TURN: i64 = 1 << 24;

/// `atan(2^-i)` in turns of `TURN`, the angles of the CORDIC rotations.
const ATAN: [i64; 16] = [
    2_097_152, 1_238_021, 654_136, 332_050, 166_669, 83_416, 41_718, 20_860, 10_430, 5_215, 2_608,
    1_304, 652, 326, 163, 81,
];

/// The angle of the vector (x, y) in [0, TURN) turns, by CORDIC (error below 2^-15 turn).
fn atan2_turn(y: i64, x: i64) -> i64 {
    if x == 0 && y == 0 {
        return 0;
    }
    let (mut vx, mut vy, base) = if x < 0 { (-x, -y, TURN / 2) } else { (x, y, 0) };
    vx <<= 16;
    vy <<= 16;
    let mut z = 0;
    for (i, &step) in ATAN.iter().enumerate() {
        let (dx, dy) = (vy >> i, vx >> i);
        if vy > 0 {
            vx += dx;
            vy -= dy;
            z += step;
        } else {
            vx -= dx;
            vy += dy;
            z -= step;
        }
    }
    (base + z).rem_euclid(TURN)
}

/// cos(2 pi phase / TURN) in Q15 (a parabola with the usual correction, error under 0.002).
fn cos_q15(phase: i64) -> i64 {
    let p = (phase + TURN / 4).rem_euclid(TURN) >> 9; // one turn is 32768
    let half = p & 0x3fff; // position in the half turn
    let mut y = (4 * half * (0x4000 - half)) >> 13; // 4t(1-t) in Q15
    y += (7373 * (((y * y) >> 15) - y)) >> 15;
    if p & 0x4000 != 0 { -y } else { y }
}

/// The shape of one image.
pub struct Pattern {
    pub width: usize,
    pub height: usize,
    pub seed: u32,
    /// Centre of the ellipses, in per-mille of the width and the height.
    pub centre: (i64, i64),
    /// Ridge period in 1/16 pixel (152 is 9.5 pixels, about 0.48 mm at 500 ppi).
    pub period16: i64,
    /// Stretch of the ellipses along y, in 1/16 (16 is circular).
    pub stretch16: i64,
    /// Number of dislocations (minutiae of the pattern).
    pub dislocations: usize,
}

pub fn ridge_pattern(p: &Pattern) -> Vec<u8> {
    let (w, h) = (p.width as i64, p.height as i64);
    let mut state = p.seed;
    let mut rand = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        i64::from(state >> 16)
    };
    let (cx, cy) = (w * p.centre.0 / 1000, h * p.centre.1 / 1000);
    // Foreground ellipse: semi-axes in pixels. `q` is 1024 on its edge.
    let (ea, eb) = (w * 46 / 100, h * 47 / 100);
    let (ex, ey) = (w / 2, h / 2);
    let q =
        |x: i64, y: i64| (x - ex).pow(2) * 1024 / (ea * ea) + (y - ey).pow(2) * 1024 / (eb * eb);

    // Dislocations, inside the foreground and at least 40 pixels apart.
    let mut points: Vec<(i64, i64, i64)> = Vec::new();
    let mut tries = 0;
    while points.len() < p.dislocations && tries < 10_000 {
        tries += 1;
        let (x, y) = (rand() % w, rand() % h);
        if q(x, y) > 600 {
            continue;
        }
        if points
            .iter()
            .any(|&(px, py, _)| (px - x).pow(2) + (py - y).pow(2) < 40 * 40)
        {
            continue;
        }
        points.push((x, y, if rand() & 1 == 0 { 1 } else { -1 }));
    }

    let mut out = Vec::with_capacity(p.width * p.height);
    for y in 0..h {
        for x in 0..w {
            // Distance to the centre in 1/16 pixel, along stretched axes.
            let (dx, dy) = ((x - cx) * 16, (y - cy) * p.stretch16);
            let r16 = (dx * dx + dy * dy).isqrt();
            let mut phase = r16 * TURN / p.period16;
            for &(px, py, sign) in &points {
                phase += sign * atan2_turn(y - py, x - px);
            }
            let ridge = cos_q15(phase); // -1 (ridge) .. 1 (valley), Q15

            // 256 inside the foreground, 0 outside, linear across the edge.
            let weight = (1024 - q(x, y)).clamp(0, 324) * 256 / 324;
            let light = 128 + (x * 24 / w) - (y * 16 / h); // slow lighting variation
            let contrast = 60 + 36 * weight / 256;
            let fore = light + contrast * ridge / 32768;
            let ground = 236;
            let grain = (rand() & 15) - 8;
            let v = (ground * (256 - weight) + fore * weight) / 256 + grain * (64 + weight) / 320;
            out.push(v.clamp(0, 255) as u8);
        }
    }
    out
}

/// The images of the benchmark: different centres, ridge periods and minutiae counts.
pub fn images() -> Vec<(&'static str, Pattern)> {
    let (width, height) = (640, 704);
    let pattern = |seed, centre, period16, stretch16, dislocations| Pattern {
        width,
        height,
        seed,
        centre,
        period16,
        stretch16,
        dislocations,
    };
    vec![
        ("ridgefp_a_640x704", pattern(11, (500, 480), 152, 20, 28)),
        ("ridgefp_b_640x704", pattern(22, (420, 620), 136, 26, 34)),
        ("ridgefp_c_640x704", pattern(33, (560, 400), 168, 16, 22)),
    ]
}
