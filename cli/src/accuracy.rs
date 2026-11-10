//! The accuracy requirements of the FBI's WSQ specification, as `cert
//! compare` applies them (see docs/certification.md).
//!
//! Source: IAFIS-IC-0110 v3.1 (October 4, 2010), Part 2, "Compliance Tests",
//! Annex AA, "Procedures for Determining Compliance", printed page 37:
//!
//! - AA.2 item 1: the compressed file size, excluding comments, is within
//!   0.4% of the reference's;
//! - AA.2 item 2: every quantization bin width `Q` and zero bin width `Z`,
//!   for the subbands `0 <= k <= 59`, is within 0.051% of the reference's;
//! - AA.2 item 3: at least 99.99% of the bin indices are the reference's,
//!   and none differs from it by more than 1;
//! - AA.3 item 1 (decoders): at least 99.9% of the reconstructed values are
//!   the reference's, and none differs from it by more than 1.
//!
//! Every number below is one of these. The rest is how this module reads
//! them where the text leaves a choice; each such choice is marked
//! "Reading" and is listed as a question for NIST in docs/certification.md.
//! Nothing here is an estimate: a requirement the text does not give is not
//! checked.

use wsq_codec::inspect::Quantized;

/// The limits of Annex AA, in parts per million, so that every comparison on
/// counts is exact integer arithmetic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thresholds {
    /// AA.2 item 1: largest relative difference of the file sizes (0.4%).
    pub file_size_ppm: u64,
    /// AA.2 item 2: largest relative difference of a bin width (0.051%).
    pub bin_width_ppm: u64,
    /// AA.2 item 3: smallest share of identical bin indices (99.99%).
    pub bin_index_agreement_ppm: u64,
    /// AA.2 item 3: largest difference of one bin index (1).
    pub bin_index_max_difference: u32,
    /// AA.3 item 1: smallest share of identical reconstructed values (99.9%).
    pub pixel_agreement_ppm: u64,
    /// AA.3 item 1: largest difference of one reconstructed value (1).
    pub pixel_max_difference: u32,
}

impl Default for Thresholds {
    /// The values of IAFIS-IC-0110 v3.1, Annex AA.
    fn default() -> Self {
        Thresholds {
            file_size_ppm: 4_000,
            bin_width_ppm: 510,
            bin_index_agreement_ppm: 999_900,
            bin_index_max_difference: 1,
            pixel_agreement_ppm: 999_000,
            pixel_max_difference: 1,
        }
    }
}

/// The outcome of one requirement.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    /// Where the requirement is in the specification, e.g. "AA.2 item 1".
    pub clause: &'static str,
    /// What it asks, e.g. "file size within 0.4%".
    pub name: String,
    pub passed: bool,
    /// The measured value, for the report.
    pub detail: String,
}

/// Parts per million as a percentage, for display.
fn percent(ppm: u64) -> String {
    format!("{}", ppm as f64 / 10_000.0)
}

/// AA.2 item 1. `ours` and `reference` are the file sizes without comments.
pub fn file_size(ours: u64, reference: u64, t: &Thresholds) -> Check {
    // |ours - reference| / reference <= ppm / 10^6, on integers.
    let diff = u128::from(ours.abs_diff(reference));
    let passed = diff * 1_000_000 <= u128::from(t.file_size_ppm) * u128::from(reference);
    let signed = (ours as f64 / reference as f64 - 1.0) * 100.0;
    Check {
        clause: "AA.2 item 1",
        name: format!("file size within {}%", percent(t.file_size_ppm)),
        passed,
        detail: format!("{signed:+.4}%"),
    }
}

/// Size of a stream without its comment segments: each comment is a COM
/// marker (2 bytes), a length (2 bytes) and its payload.
///
/// Reading: the whole segment is a comment, marker and length included. The
/// specification says "excluding comments" and does not define the term.
pub fn size_without_comments(len: usize, comments: &[Vec<u8>]) -> u64 {
    let comment_bytes: usize = comments.iter().map(|c| c.len() + 4).sum();
    len.saturating_sub(comment_bytes) as u64
}

/// Relative difference of a bin width to the reference's, in the sense of
/// AA.2 item 2: `|test - reference| / reference`. Infinite when the reference
/// width is zero and the test's is not.
pub fn relative_difference(test: f32, reference: f32) -> f64 {
    let (t, r) = (f64::from(test), f64::from(reference));
    if r == 0.0 {
        if t == 0.0 { 0.0 } else { f64::INFINITY }
    } else {
        (t - r).abs() / r.abs()
    }
}

/// AA.2 item 2 on the worst relative differences found over the subbands
/// `0 <= k <= 59` that both streams keep, `q` for the bin widths and `z` for
/// the zero bin widths, and on the number of subbands one stream drops (`Q`
/// of zero) and the other keeps.
///
/// Reading: a subband dropped on one side only has a width of zero against a
/// width that is not, which is no relative difference within 0.051%, so it
/// fails the requirement. A subband both streams drop has nothing to compare.
pub fn bin_widths(q: f64, z: f64, dropped: usize, t: &Thresholds) -> Check {
    let limit = t.bin_width_ppm as f64 / 1e6;
    Check {
        clause: "AA.2 item 2",
        name: format!("bin widths Q and Z within {}%", percent(t.bin_width_ppm)),
        passed: dropped == 0 && q <= limit && z <= limit,
        detail: format!(
            "worst Q {:.4}%, worst Z {:.4}%, {dropped} subband(s) dropped on one side only",
            q * 100.0,
            z * 100.0
        ),
    }
}

/// AA.2 item 3 on `differing` bin indices out of `total`, the largest
/// difference being `max_difference`.
pub fn bin_indices(differing: u64, max_difference: u32, total: u64, t: &Thresholds) -> Check {
    Check {
        clause: "AA.2 item 3",
        name: format!(
            "{}% of bin indices identical, none off by more than {}",
            percent(t.bin_index_agreement_ppm),
            t.bin_index_max_difference
        ),
        passed: agreement(differing, total, t.bin_index_agreement_ppm)
            && max_difference <= t.bin_index_max_difference,
        detail: format!("{differing}/{total} differ, largest difference {max_difference}"),
    }
}

/// AA.3 item 1 on `differing` reconstructed values out of `total`.
pub fn pixels(differing: u64, max_difference: u32, total: u64, t: &Thresholds) -> Check {
    Check {
        clause: "AA.3 item 1",
        name: format!(
            "{}% of reconstructed values identical, none off by more than {}",
            percent(t.pixel_agreement_ppm),
            t.pixel_max_difference
        ),
        passed: agreement(differing, total, t.pixel_agreement_ppm)
            && max_difference <= t.pixel_max_difference,
        detail: format!("{differing}/{total} differ, largest difference {max_difference}"),
    }
}

/// Whether the share of values that are not `differing` is at least `ppm`.
fn agreement(differing: u64, total: u64, ppm: u64) -> bool {
    let identical = u128::from(total.saturating_sub(differing));
    identical * 1_000_000 >= u128::from(ppm) * u128::from(total)
}

/// What differs between two streams, from the reference's point of view.
#[derive(Debug, Default, PartialEq)]
pub struct StreamMeasures {
    /// Worst relative difference of `Q` and of `Z` (see
    /// [`relative_difference`]), over the subbands `0 <= k <= 59` that both
    /// streams keep.
    pub worst_q: f64,
    pub worst_z: f64,
    /// Bin indices that differ, the largest difference, and the number of bin
    /// indices of the reference.
    pub differing: u64,
    pub max_difference: u32,
    pub total: u64,
    /// Subbands one stream drops and the other keeps.
    pub dropped: usize,
}

/// Measures the differences of the quantized data of `ours` against
/// `reference`.
///
/// Reading: the bin indices are those of every coefficient of the 60
/// subbands, so a subband the stream drops counts its coefficients, all
/// zero. A subband dropped on one side only compares the other side's
/// indices with zero.
pub fn measure(ours: &Quantized, reference: &Quantized) -> StreamMeasures {
    let mut m = StreamMeasures::default();
    for (o, r) in ours.subbands.iter().zip(&reference.subbands).take(60) {
        m.total += (r.width * r.height) as u64;
        match (o.q_bin == 0.0, r.q_bin == 0.0) {
            (true, true) => {}
            (false, false) => {
                m.worst_q = m.worst_q.max(relative_difference(o.q_bin, r.q_bin));
                m.worst_z = m.worst_z.max(relative_difference(o.z_bin, r.z_bin));
                for (&a, &b) in o.coefficients.iter().zip(&r.coefficients) {
                    if a != b {
                        m.differing += 1;
                        m.max_difference = m.max_difference.max(u32::from(a.abs_diff(b)));
                    }
                }
            }
            (dropped_ours, _) => {
                m.dropped += 1;
                let kept = if dropped_ours {
                    &r.coefficients
                } else {
                    &o.coefficients
                };
                for &c in kept.iter().filter(|&&c| c != 0) {
                    m.differing += 1;
                    m.max_difference = m.max_difference.max(u32::from(c.unsigned_abs()));
                }
            }
        }
    }
    m
}

/// The requirements for an encoder's stream.
pub fn judge_stream(
    measures: &StreamMeasures,
    our_size: u64,
    reference_size: u64,
    t: &Thresholds,
) -> Vec<Check> {
    vec![
        file_size(our_size, reference_size, t),
        bin_widths(measures.worst_q, measures.worst_z, measures.dropped, t),
        bin_indices(
            measures.differing,
            measures.max_difference,
            measures.total,
            t,
        ),
    ]
}

/// How many values of `ours` differ from `reference` (of the same length),
/// and the largest difference.
pub fn pixel_differences(ours: &[u8], reference: &[u8]) -> (u64, u32) {
    let (mut differing, mut max) = (0u64, 0u8);
    for (&a, &b) in ours.iter().zip(reference) {
        if a != b {
            differing += 1;
            max = max.max(a.abs_diff(b));
        }
    }
    (differing, u32::from(max))
}

/// Whether every check passed.
pub fn passed(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.passed)
}

/// The failed checks, one per line, for the report.
pub fn failures(checks: &[Check]) -> String {
    checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| format!("{} ({}): {}", c.clause, c.name, c.detail))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Thresholds = Thresholds {
        file_size_ppm: 4_000,
        bin_width_ppm: 510,
        bin_index_agreement_ppm: 999_900,
        bin_index_max_difference: 1,
        pixel_agreement_ppm: 999_000,
        pixel_max_difference: 1,
    };

    #[test]
    fn default_thresholds_are_the_specifications() {
        assert_eq!(Thresholds::default(), T);
    }

    #[test]
    fn file_size_limit_is_0_4_percent() {
        // 0.4% of 100 000 bytes is 400 bytes: the limit itself passes.
        assert!(file_size(100_400, 100_000, &T).passed);
        assert!(file_size(99_600, 100_000, &T).passed);
        // One byte beyond fails, in both directions.
        assert!(!file_size(100_401, 100_000, &T).passed);
        assert!(!file_size(99_599, 100_000, &T).passed);
        assert!(file_size(100_000, 100_000, &T).passed);
    }

    #[test]
    fn comments_are_not_counted_in_the_size() {
        // A 10-byte comment is a 14-byte segment.
        assert_eq!(size_without_comments(1_000, &[vec![0; 10]]), 986);
        assert_eq!(size_without_comments(1_000, &[]), 1_000);
        assert_eq!(size_without_comments(1_000, &[vec![0; 10], vec![]]), 982);
    }

    #[test]
    fn bin_width_limit_is_0_051_percent() {
        let within = |test: f32, reference: f32| {
            let r = relative_difference(test, reference);
            bin_widths(r, r, 0, &T).passed
        };
        // 0.050% of 1 000 000 is 500; 0.052% is 520 (both exact in f32).
        assert!(within(1_000_500.0, 1_000_000.0));
        assert!(within(999_500.0, 1_000_000.0));
        assert!(!within(1_000_520.0, 1_000_000.0));
        assert!(!within(999_480.0, 1_000_000.0));
        assert!(within(3.5, 3.5));
    }

    #[test]
    fn bin_width_limit_is_exact_on_the_limit() {
        // 0.5% (5 000 ppm) of 1000 is exactly 5.
        let t = Thresholds {
            bin_width_ppm: 5_000,
            ..T
        };
        assert!(bin_widths(relative_difference(1_005.0, 1_000.0), 0.0, 0, &t).passed);
        assert!(!bin_widths(relative_difference(1_006.0, 1_000.0), 0.0, 0, &t).passed);
    }

    #[test]
    fn bin_widths_judge_q_and_z_separately() {
        let r = relative_difference(1_000_520.0, 1_000_000.0);
        assert!(!bin_widths(r, 0.0, 0, &T).passed);
        assert!(!bin_widths(0.0, r, 0, &T).passed);
        assert!(bin_widths(0.0, 0.0, 0, &T).passed);
    }

    #[test]
    fn a_subband_dropped_on_one_side_fails_the_widths() {
        assert_eq!(relative_difference(0.0, 0.0), 0.0);
        assert!(relative_difference(1.0, 0.0).is_infinite());
        assert_eq!(relative_difference(0.0, 2.0), 1.0);
        assert!(!bin_widths(0.0, 0.0, 1, &T).passed);
    }

    #[test]
    fn bin_index_agreement_limit_is_99_99_percent() {
        // 1 in 10 000 is exactly 99.99%: it passes; 2 do not.
        assert!(bin_indices(1, 1, 10_000, &T).passed);
        assert!(!bin_indices(2, 1, 10_000, &T).passed);
        assert!(bin_indices(0, 0, 10_000, &T).passed);
        assert!(bin_indices(0, 0, 0, &T).passed);
    }

    #[test]
    fn bin_index_difference_limit_is_one() {
        assert!(bin_indices(1, 1, 1_000_000, &T).passed);
        assert!(!bin_indices(1, 2, 1_000_000, &T).passed);
        // A single wide difference fails even among a million identical ones.
        assert!(!bin_indices(1, 3, 1_000_000_000, &T).passed);
    }

    #[test]
    fn pixel_agreement_limit_is_99_9_percent() {
        // 1 in 1 000 is exactly 99.9%: it passes; 2 do not.
        assert!(pixels(1, 1, 1_000, &T).passed);
        assert!(!pixels(2, 1, 1_000, &T).passed);
    }

    #[test]
    fn pixel_difference_limit_is_one() {
        assert!(pixels(1, 1, 1_000_000, &T).passed);
        assert!(!pixels(1, 2, 1_000_000, &T).passed);
    }

    #[test]
    fn judges_an_image_pixel_by_pixel() {
        let judge = |ours: &[u8], reference: &[u8]| {
            let (differing, max) = pixel_differences(ours, reference);
            pixels(differing, max, reference.len() as u64, &T).passed
        };
        let reference = vec![100u8; 2_000];
        let mut ours = reference.clone();
        ours[0] = 101; // 1 in 2 000 differs by 1: inside
        assert!(judge(&ours, &reference));
        ours[1] = 99;
        ours[2] = 101; // 3 in 2 000 (0.15%): outside the 99.9%
        assert!(!judge(&ours, &reference));
        let mut ours = reference.clone();
        ours[0] = 102; // one value off by 2: outside
        assert!(!judge(&ours, &reference));
    }

    #[test]
    fn failures_name_the_clause() {
        let checks = [file_size(101_000, 100_000, &T), pixels(0, 0, 10, &T)];
        let text = failures(&checks);
        assert!(text.starts_with("AA.2 item 1"), "{text}");
        assert!(!text.contains("AA.3"), "{text}");
    }

    /// A stream of a synthetic image at the given bit rate.
    fn stream(bitrate: f32) -> (Vec<u8>, Quantized) {
        let pixels: Vec<u8> = (0..120 * 100)
            .map(|i| ((i % 120) * 3 + (i / 120) * 2 + (i * i) % 7) as u8)
            .collect();
        let options = wsq_codec::EncodeOptions::default().with_bitrate(bitrate);
        let data = wsq_codec::encode(&pixels, 120, 100, &options).unwrap();
        let q = wsq_codec::inspect::read_quantized(&data, &wsq_codec::DecodeOptions::default())
            .unwrap();
        (data, q)
    }

    #[test]
    fn identical_streams_pass_every_requirement() {
        let (data, q) = stream(0.75);
        let m = measure(&q, &q);
        assert_eq!((m.differing, m.max_difference, m.dropped), (0, 0, 0));
        assert!(m.total > 0);
        let size = size_without_comments(data.len(), &q.info.comments);
        assert!(size < data.len() as u64, "the NISTCOM comment is excluded");
        assert!(passed(&judge_stream(&m, size, size, &T)));
    }

    #[test]
    fn streams_at_other_bit_rates_fail() {
        let (low_data, low) = stream(0.75);
        let (high_data, high) = stream(2.25);
        let m = measure(&high, &low);
        assert!(m.worst_q > 0.0 || m.dropped > 0);
        let checks = judge_stream(
            &m,
            size_without_comments(high_data.len(), &high.info.comments),
            size_without_comments(low_data.len(), &low.info.comments),
            &T,
        );
        assert!(!checks[0].passed, "{checks:?}");
        assert!(!checks[1].passed, "{checks:?}");
        assert!(!checks[2].passed, "{checks:?}");
    }
}
