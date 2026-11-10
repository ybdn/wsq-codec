//! Natural logarithm and power, the same on every IEEE 754 binary64 target.
//!
//! The bit allocation takes logarithms and powers, and its results decide
//! the bin widths written to the stream, so they must not depend on the
//! platform's math library: `f64::ln` and `f64::powf` call it, and its
//! last bit differs between C libraries, between their versions, and even
//! between the code paths a library picks for the CPU at run time.
//!
//! These functions compute in double-double arithmetic, about 106 bits,
//! and round to the nearest `f64`. They use nothing but IEEE 754 addition,
//! subtraction, multiplication and division, which round exactly the same
//! way everywhere (Rust never fuses a multiplication and an addition
//! unless asked), so their results are identical on every target that computes
//! `f64` in IEEE 754 binary64 (not on the x87 unit, such as `i586`). With 50
//! bits to spare, the rounding is the correct one except for inputs whose
//! exact result lies within about 2^-100 of the midpoint between two
//! `f64`s; glibc, whose `log` and `pow` stay within 0.52 units in the last
//! place, gives the same results on all but such rare inputs, which is
//! what keeps the encoder's output identical to NBIS's.

/// A double-double: the unevaluated sum `hi + lo` with `|lo|` at most half
/// a unit in the last place of `hi`.
#[derive(Clone, Copy, Debug)]
struct Dd {
    hi: f64,
    lo: f64,
}

/// ln 2 to 107 bits.
const LN2: Dd = Dd {
    hi: f64::from_bits(0x3FE6_2E42_FEFA_39EF),
    lo: f64::from_bits(0x3C7A_BC9E_3B39_803F),
};

/// `a + b` and its rounding error.
fn two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    let bb = s - a;
    Dd {
        hi: s,
        lo: (a - (s - bb)) + (b - bb),
    }
}

/// `a + b` and its rounding error, for `|a| >= |b|`.
fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd {
        hi: s,
        lo: b - (s - a),
    }
}

/// Splits `a` into two halves of 26 significant bits (Veltkamp), for
/// `|a|` well below 2^996.
fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a; // 2^27 + 1
    let hi = c - (c - a);
    (hi, a - hi)
}

/// `a * b` and its rounding error (Dekker), without a fused multiply-add,
/// whose availability varies.
fn two_prod(a: f64, b: f64) -> Dd {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    Dd {
        hi: p,
        lo: ((ah * bh - p) + ah * bl + al * bh) + al * bl,
    }
}

impl Dd {
    fn new(x: f64) -> Self {
        Dd { hi: x, lo: 0.0 }
    }

    fn add(self, y: Dd) -> Dd {
        let s = two_sum(self.hi, y.hi);
        let t = two_sum(self.lo, y.lo);
        let s = quick_two_sum(s.hi, s.lo + t.hi);
        quick_two_sum(s.hi, s.lo + t.lo)
    }

    fn neg(self) -> Dd {
        Dd {
            hi: -self.hi,
            lo: -self.lo,
        }
    }

    fn sub(self, y: Dd) -> Dd {
        self.add(y.neg())
    }

    fn mul(self, y: Dd) -> Dd {
        let p = two_prod(self.hi, y.hi);
        quick_two_sum(p.hi, p.lo + (self.hi * y.lo + self.lo * y.hi))
    }

    fn div(self, y: Dd) -> Dd {
        let q1 = self.hi / y.hi;
        let r = self.sub(y.mul(Dd::new(q1)));
        let q2 = r.hi / y.hi;
        let r = r.sub(y.mul(Dd::new(q2)));
        let q3 = r.hi / y.hi;
        quick_two_sum(q1, q2).add(Dd::new(q3))
    }

    /// Multiplies by a power of two, exactly while the result stays
    /// normal.
    fn scale(self, k: i32) -> Dd {
        let f = f64::from_bits(((1023 + k) as u64) << 52);
        Dd {
            hi: self.hi * f,
            lo: self.lo * f,
        }
    }
}

/// Natural logarithm of a positive, finite `x`, in double-double.
fn ln_dd(x: f64) -> Dd {
    let mut bits = x.to_bits();
    let mut e = 0i32;
    if bits >> 52 == 0 {
        // Subnormal: scale it into the normal range first.
        bits = (x * f64::from_bits(0x4350_0000_0000_0000)).to_bits(); // 2^54
        e = -54;
    }
    e += ((bits >> 52) & 0x7FF) as i32 - 1023;
    let mut m = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000);
    if m > core::f64::consts::SQRT_2 {
        m /= 2.0;
        e += 1;
    }
    // ln m = 2 atanh(f) with f = (m - 1) / (m + 1), |f| <= 0.1716; m - 1
    // is exact, m + 1 is kept exactly as a double-double.
    let f = Dd::new(m - 1.0).div(two_sum(m, 1.0));
    let f2 = f.mul(f);
    // atanh(f) / f = sum of f^2k / (2k + 1); f^2 <= 0.0295, so 24 terms
    // leave a truncation error below 2^-120.
    let mut s = Dd::new(1.0).div(Dd::new(49.0));
    for k in (0..24).rev() {
        s = Dd::new(1.0)
            .div(Dd::new(f64::from(2 * k + 1)))
            .add(f2.mul(s));
    }
    let ln_m = f.mul(s).scale(1);
    Dd::new(f64::from(e)).mul(LN2).add(ln_m)
}

/// `e^y` for a finite `y`, rounded to the nearest `f64`.
fn exp_dd(y: Dd) -> f64 {
    if y.hi > 710.0 {
        return f64::INFINITY;
    }
    if y.hi < -746.0 {
        return 0.0;
    }
    // y = k ln 2 + r with |r| <= ln 2 / 2, then e^r = (e^(r / 1024))^1024.
    let k = (y.hi / LN2.hi).round_ties_even();
    let r = y.sub(Dd::new(k).mul(LN2)).scale(-10);
    // e^r - 1 for |r| < 3.4e-4: 10 terms leave an error below 2^-120.
    let mut t = Dd::new(1.0);
    for n in (2..=10).rev() {
        t = Dd::new(1.0).add(r.mul(t).div(Dd::new(f64::from(n))));
    }
    let mut t = r.mul(t);
    // (1 + t)^2 = 1 + (2t + t^2), keeping the small part apart.
    for _ in 0..10 {
        t = t.scale(1).add(t.mul(t));
    }
    let m = Dd::new(1.0).add(t);
    // m is within [0.7, 1.42] and k within [-1077, 1025].
    let k = k as i32;
    if k > -1020 {
        // A normal result, or an overflow: rounding m and then scaling it
        // rounds once. Scale in two steps so that neither factor
        // overflows on its own.
        let k1 = k / 2;
        return m.hi
            * f64::from_bits(((1023 + k1) as u64) << 52)
            * f64::from_bits(((1023 + k - k1) as u64) << 52);
    }
    // Possibly below the normal range, where the result is a multiple of
    // 2^-1074: count those multiples, exactly, and round that count once.
    let v = m.scale(k + 1074);
    if v.hi >= 4_503_599_627_370_496.0 {
        // 2^52 or more multiples: a normal result after all.
        return m.hi * f64::from_bits(((1023 + k + 1074) as u64) << 52) * f64::from_bits(1);
    }
    let n = v.hi.round_ties_even();
    let d = (v.hi - n) + v.lo;
    let n = if d > 0.5 || (d == 0.5 && n % 2.0 != 0.0) {
        n + 1.0
    } else if d < -0.5 || (d == -0.5 && n % 2.0 != 0.0) {
        n - 1.0
    } else {
        n
    };
    n * f64::from_bits(1)
}

/// Natural logarithm, with the special values of IEEE 754's `log`.
pub(crate) fn ln(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        f64::NAN
    } else if x == 0.0 {
        f64::NEG_INFINITY
    } else if x == f64::INFINITY {
        x
    } else {
        ln_dd(x).hi
    }
}

/// Whether `y`, an integer, is odd.
fn is_odd(y: f64) -> bool {
    // Every double from 2^53 up is even.
    y.abs() < 9_007_199_254_740_992.0 && (y as i64) % 2 != 0
}

/// `x^y`, with the special values of IEEE 754's `pow`.
pub(crate) fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 || x == 1.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let integer = y == y.trunc();
    let negative = x.is_sign_negative() && integer && is_odd(y);
    let signed = |v: f64| if negative { -v } else { v };
    if y.is_infinite() {
        return match (x.abs() == 1.0, x.abs() < 1.0, y > 0.0) {
            (true, _, _) => 1.0,
            (false, true, true) | (false, false, false) => 0.0,
            _ => f64::INFINITY,
        };
    }
    if x == 0.0 {
        return signed(if y > 0.0 { 0.0 } else { f64::INFINITY });
    }
    if x.is_infinite() {
        return signed(if y > 0.0 { f64::INFINITY } else { 0.0 });
    }
    if x < 0.0 && !integer {
        return f64::NAN;
    }
    signed(exp_dd(Dd::new(y).mul(ln_dd(x.abs()))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_results() {
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(pow(2.0, 10.0), 1024.0);
        assert_eq!(pow(2.0, -3.0), 0.125);
        assert_eq!(pow(9.0, 0.5), 3.0);
        assert_eq!(pow(10.0, 15.0), 1e15);
        assert_eq!(pow(-2.0, 3.0), -8.0);
        assert_eq!(pow(1024.0, 1.0 / 1024.0), pow(2.0, 10.0 / 1024.0));
    }

    #[test]
    fn special_values() {
        assert!(ln(-1.0).is_nan());
        assert!(ln(f64::NAN).is_nan());
        assert_eq!(ln(0.0), f64::NEG_INFINITY);
        assert_eq!(ln(-0.0), f64::NEG_INFINITY);
        assert_eq!(ln(f64::INFINITY), f64::INFINITY);
        assert_eq!(pow(f64::NAN, 0.0), 1.0);
        assert_eq!(pow(1.0, f64::NAN), 1.0);
        assert!(pow(f64::NAN, 1.0).is_nan());
        assert!(pow(-2.0, 0.5).is_nan());
        assert_eq!(pow(0.0, 2.0), 0.0);
        assert_eq!(pow(0.0, -2.0), f64::INFINITY);
        assert_eq!(pow(-0.0, 3.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(pow(-0.0, -3.0), f64::NEG_INFINITY);
        assert_eq!(pow(f64::INFINITY, 0.5), f64::INFINITY);
        assert_eq!(pow(f64::INFINITY, -0.5), 0.0);
        assert_eq!(pow(f64::NEG_INFINITY, 3.0), f64::NEG_INFINITY);
        assert_eq!(pow(0.5, f64::INFINITY), 0.0);
        assert_eq!(pow(0.5, f64::NEG_INFINITY), f64::INFINITY);
        assert_eq!(pow(2.0, f64::INFINITY), f64::INFINITY);
        assert_eq!(pow(-1.0, f64::INFINITY), 1.0);
        assert_eq!(pow(2.0, 2000.0), f64::INFINITY);
        assert_eq!(pow(2.0, -2000.0), 0.0);
        assert_eq!(pow(2.0, -1074.0), f64::from_bits(1));
        assert_eq!(pow(2.0, 1023.0), f64::from_bits(0x7FE0_0000_0000_0000));
    }

    #[test]
    fn subnormal_input() {
        let x = f64::from_bits(1); // 2^-1074
        let expected = -1074.0 * core::f64::consts::LN_2;
        assert!((ln(x) - expected).abs() <= expected.abs() * f64::EPSILON);
    }

    /// Inputs on which glibc 2.39 returns the other neighbor of the exact
    /// result, with the correctly rounded result (checked with 300-bit
    /// arithmetic): kind 0 is `ln(x)`, kind 1 to 4 `pow(x, y)` over the
    /// ranges the bit allocation uses and beyond.
    const CORRECTLY_ROUNDED: [(u8, u64, u64, u64); 30] = [
        (
            0,
            0x4013be3620000000,
            0x0000000000000000,
            0x3ff98b492831570b,
        ),
        (
            0,
            0x4018463e80000000,
            0x0000000000000000,
            0x3ffcd99c4c1e91b9,
        ),
        (
            0,
            0x400b62d280000000,
            0x0000000000000000,
            0x3ff3b0807cfd8dae,
        ),
        (
            0,
            0x3ff1406920000000,
            0x0000000000000000,
            0x3fb347efa138a8d7,
        ),
        (
            0,
            0x3ff1e39200000000,
            0x0000000000000000,
            0x3fbc917178555dfe,
        ),
        (
            0,
            0x4006fd94e0000000,
            0x0000000000000000,
            0x3ff0e3e8b46f61c0,
        ),
        (
            1,
            0x4041455480000000,
            0x3f50000000000000,
            0x3ff00e31755140a2,
        ),
        (
            1,
            0x400185f2e0000000,
            0x3f50000000000000,
            0x3ff00323363b802d,
        ),
        (
            1,
            0x3fa2f41fc0000000,
            0x3f70000000000000,
            0x3fef97319252f5a2,
        ),
        (
            1,
            0x4021fdfac0000000,
            0x3f70000000000000,
            0x3ff0234cc0ca4661,
        ),
        (
            1,
            0x40e3845e00000000,
            0x3fb0000000000000,
            0x3fff06b6fd819ae5,
        ),
        (
            1,
            0x4104472400000000,
            0x3f70000000000000,
            0x3ff0c4e9f9f9164e,
        ),
        (
            2,
            0x4000000000000000,
            0x405baad620000000,
            0x46d971f5c4c7ec32,
        ),
        (
            2,
            0x4000000000000000,
            0x402298b740000000,
            0x4083acbc23061601,
        ),
        (
            2,
            0x4000000000000000,
            0x40407d5d00000000,
            0x41ff8bdccdc27be9,
        ),
        (
            2,
            0x4000000000000000,
            0x404a842dc0000000,
            0x43405dbe50813ea1,
        ),
        (
            2,
            0x4000000000000000,
            0x405c72b9e0000000,
            0x470bb709ff494b12,
        ),
        (
            2,
            0x4000000000000000,
            0x404cfca840000000,
            0x438f6d0b3ea5ad66,
        ),
        (
            3,
            0x397b442820000000,
            0x4010410410410410,
            0x25b6f8ede82657b1,
        ),
        (
            3,
            0x3c09537e00000000,
            0x3ff7e225515a4f1d,
            0x3a1edf041d195bf9,
        ),
        (
            3,
            0x38550ebd60000000,
            0x3ff0410410410410,
            0x383619e5006f30c8,
        ),
        (
            3,
            0x4562242b00000000,
            0x400999999999999a,
            0x515f8e66dfa3a8d0,
        ),
        (
            3,
            0x4642a69060000000,
            0x4003bd60d9232955,
            0x4f8b193e67680f52,
        ),
        (
            3,
            0x38ad64cc00000000,
            0x3ff7e225515a4f1d,
            0x351953608304f73f,
        ),
        (
            4,
            0x381ce0434d1cc4e0,
            0x3fffd54739baa584,
            0x306019611f214989,
        ),
        (
            4,
            0x2260dfe28aabfe13,
            0xbff4433a540e6888,
            0x655e3165a205b0de,
        ),
        (
            4,
            0x508c79eeeca40bde,
            0x3ff30e560c72df6a,
            0x53b8584efc1389a9,
        ),
        (
            4,
            0x480098578cc84763,
            0x3ffc80e4b566ac0a,
            0x4e4dec99d6a76ebf,
        ),
        (
            4,
            0x45ff0bd392f829a1,
            0x3fbb2a3b9fc695c0,
            0x40938a31bc360550,
        ),
        (
            4,
            0x2dfc1049e137b651,
            0xbff10d61de52a022,
            0x5310e08dbaeca349,
        ),
    ];

    #[test]
    fn correctly_rounded_where_glibc_is_not() {
        for &(kind, x, y, expected) in &CORRECTLY_ROUNDED {
            let (x, y) = (f64::from_bits(x), f64::from_bits(y));
            let got = if kind == 0 { ln(x) } else { pow(x, y) };
            assert_eq!(got.to_bits(), expected, "kind {kind}, x = {x:e}, y = {y:e}");
        }
    }

    #[test]
    fn subnormal_results() {
        // 2^-1075 lies halfway between 0 and 2^-1074: ties go to even.
        assert_eq!(pow(2.0, -1075.0), 0.0);
        assert_eq!(pow(2.0, -1074.5), f64::from_bits(1));
        assert_eq!(pow(2.0, -1073.4), f64::from_bits(2));
        assert_eq!(pow(2.0, -1030.0), f64::from_bits(1 << 44));
        assert_eq!(pow(2.0, -1022.0), f64::MIN_POSITIVE);
    }
}
