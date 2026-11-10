//! Compares against fixtures written by the NIST reference implementation
//! (see tests/data/README.md).
//!
//! Large images have no committed stream: their reference is the length and
//! the FNV-1a hash of NBIS's stream and of NBIS's decode, and the test makes
//! the image again.

use wsq_certified::{EncodeOptions, decode, encode, read_info};

/// Synthetic test images, the same as `pattern` in make_fixtures.py.
fn pattern(kind: &str, w: usize, h: usize, seed: u32) -> Vec<u8> {
    let mut state = seed;
    let mut rand = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state >> 24
    };
    let (cx, cy) = (w / 3, h / 2);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let v = match kind {
                "rings" => {
                    let (dx, dy) = (x.abs_diff(cx), y.abs_diff(cy));
                    let r2 = dx * dx + dy * dy * 3 / 2 + x * y / 5;
                    let t = (r2 / 37) % 32;
                    30 + t.abs_diff(16) as u32 * 12 + (rand() & 15)
                }
                "noise" => rand(),
                "ridges" | "dark_ridges" => {
                    // "dark_ridges" has a mean of about 75 instead of 143, for the images
                    // NBIS refuses when bright (see tests/data/README.md).
                    let (base, gain) = if kind == "ridges" { (40, 6) } else { (20, 3) };
                    // Triangle wave of the given period (a multiple of 2).
                    let triangle = |p: usize, period: usize| (p % period).abs_diff(period / 2);
                    let (cx, cy) = (w / 2, h * 6 / 10);
                    let (dx, dy) = (x.abs_diff(cx) as u64, y.abs_diff(cy) as u64);
                    let phase = (dx * dx + dy * dy).isqrt() as usize / 3 + 4 * triangle(x / 5, 16);
                    base + gain * triangle(phase, 64) as u32 + (rand() >> 4)
                }
                _ => (if (x / 7 + y / 5) % 2 == 1 { 200 } else { 40 }) + (rand() & 7),
            };
            out.push(v as u8);
        }
    }
    out
}

fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xCBF2_9CE4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3)
    })
}

struct Fixture {
    file: &'static str,
    wsq: &'static [u8],
    kind: &'static str,
    width: usize,
    height: usize,
    bitrate: f32,
    ppi: Option<u32>,
    /// Comment handed to the encoder.
    comment: Option<&'static [u8]>,
    /// Hash of the pixels the reference decoder produces.
    decoded: u64,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        file: "rings_320x288.wsq",
        wsq: include_bytes!("data/rings_320x288.wsq"),
        kind: "rings",
        width: 320,
        height: 288,
        bitrate: 0.75,
        ppi: Some(500),
        comment: None,
        decoded: 0x6085_0ff0_f7c8_5e04,
    },
    Fixture {
        file: "noise_257x255.wsq",
        wsq: include_bytes!("data/noise_257x255.wsq"),
        kind: "noise",
        width: 257,
        height: 255,
        bitrate: 2.25,
        ppi: Some(1000),
        comment: None,
        decoded: 0xf646_cf9b_04f0_4c45,
    },
    Fixture {
        file: "checker_101x97.wsq",
        wsq: include_bytes!("data/checker_101x97.wsq"),
        kind: "checker",
        width: 101,
        height: 97,
        bitrate: 0.75,
        ppi: None,
        comment: None,
        decoded: 0x3381_3f51_f197_5ea8,
    },
    Fixture {
        file: "rings_200x180_nistcom.wsq",
        wsq: include_bytes!("data/rings_200x180_nistcom.wsq"),
        kind: "rings",
        width: 200,
        height: 180,
        bitrate: 0.75,
        ppi: Some(500),
        comment: Some(NISTCOM),
        decoded: 0x735e_939c_b100_51d9,
    },
    Fixture {
        file: "checker_150x140_comment.wsq",
        wsq: include_bytes!("data/checker_150x140_comment.wsq"),
        kind: "checker",
        width: 150,
        height: 140,
        bitrate: 2.25,
        ppi: None,
        comment: Some(PLAIN),
        decoded: 0x5524_cbd8_7390_da7b,
    },
];

/// Comments handed to the encoder, the same as in make_fixtures.py.
const NISTCOM: &[u8] = b"NIST_COM 4\nSEX m\nPPI 1000\nFING_CLASS W";
const PLAIN: &[u8] = b"Scanned by the test suite";

const EVEN_FILTERS: Fixture = Fixture {
    file: "rings_300x211_even.wsq",
    wsq: include_bytes!("data/rings_300x211_even.wsq"),
    kind: "rings",
    width: 300,
    height: 211,
    bitrate: 0.75,
    ppi: Some(500),
    comment: None,
    decoded: 0xb497_af5e_c6f5_cf5c,
};

#[test]
fn encodes_like_the_reference() {
    for f in FIXTURES {
        let pixels = pattern(f.kind, f.width, f.height, (f.width * f.height) as u32);
        let mut options = EncodeOptions::default()
            .with_bitrate(f.bitrate)
            .with_ppi(f.ppi);
        if let Some(comment) = f.comment {
            options = options.with_comment(comment.to_vec());
        }
        let wsq = encode(&pixels, f.width, f.height, &options).unwrap();
        assert!(wsq == f.wsq, "{}: encoded bytes differ", f.file);
    }
}

#[test]
fn decodes_like_the_reference() {
    for f in FIXTURES.iter().chain([&EVEN_FILTERS]) {
        let image = decode(f.wsq).unwrap();
        assert_eq!(
            (image.width, image.height),
            (f.width, f.height),
            "{}",
            f.file
        );
        assert_eq!(image.ppi, f.ppi, "{}", f.file);
        assert_eq!(
            fnv1a(&image.pixels),
            f.decoded,
            "{}: decoded pixels differ",
            f.file
        );
    }
}

#[test]
fn reads_the_reference_comment() {
    let info = read_info(FIXTURES[0].wsq).unwrap();
    assert_eq!(
        (info.width, info.height, info.ppi, info.software),
        (320, 288, Some(500), 0)
    );
    assert_eq!(info.comments.len(), 1);
    let comment = String::from_utf8(info.comments[0].clone()).unwrap();
    assert!(
        comment.starts_with("NIST_COM 9\nPIX_WIDTH 320\nPIX_HEIGHT 288\n"),
        "{comment}"
    );
    assert!(comment.ends_with("\nWSQ_BITRATE 0.750000"), "{comment}");
}

/// Reference for one bit rate of a large image, from NBIS 5.0.0.
struct LargeRun {
    bitrate: f32,
    /// Length of NBIS's stream.
    len: usize,
    /// Hash of NBIS's stream.
    stream: u64,
    /// Hash of the pixels NBIS decodes from its stream.
    decoded: u64,
}

struct Large {
    kind: &'static str,
    width: usize,
    height: usize,
    runs: &'static [LargeRun],
}

/// References made by
/// `tests/data/make_large_references.py`, which also lists the images.
const LARGE: &[Large] = &[
    Large {
        kind: "ridges",
        width: 800,
        height: 750,
        runs: &[
            LargeRun {
                bitrate: 0.75,
                len: 56_089,
                stream: 0x75e8_bccc_2958_f2ac,
                decoded: 0x8a36_ce97_e760_4ab0,
            },
            LargeRun {
                bitrate: 2.25,
                len: 184_038,
                stream: 0x2934_d838_1fdc_c548,
                decoded: 0x25d6_e066_c937_fd98,
            },
        ],
    },
    Large {
        kind: "ridges",
        width: 1600,
        height: 1500,
        runs: &[
            LargeRun {
                bitrate: 0.75,
                len: 220_487,
                stream: 0x8b14_c9b7_3a36_2dcd,
                decoded: 0x57ab_cdb7_1915_34b8,
            },
            LargeRun {
                bitrate: 2.25,
                len: 733_681,
                stream: 0x56a6_d9b8_a071_d59f,
                decoded: 0x7049_e299_f962_fb7a,
            },
        ],
    },
    Large {
        kind: "rings",
        width: 800,
        height: 750,
        runs: &[
            LargeRun {
                bitrate: 0.75,
                len: 21_599,
                stream: 0x9a90_6b29_65c8_48e3,
                decoded: 0x4a0a_4e85_5842_1234,
            },
            LargeRun {
                bitrate: 2.25,
                len: 73_641,
                stream: 0xaeae_48d2_7ab6_95e9,
                decoded: 0x118d_067d_249c_838d,
            },
        ],
    },
    Large {
        kind: "noise",
        width: 1600,
        height: 1500,
        runs: &[
            LargeRun {
                bitrate: 0.75,
                len: 201_447,
                stream: 0x04ce_ebf9_f8b3_02ba,
                decoded: 0xc2bd_700d_8ed0_5269,
            },
            LargeRun {
                bitrate: 2.25,
                len: 635_548,
                stream: 0x2d5a_7593_3c6b_70cb,
                decoded: 0x7c8a_23c3_c487_69e4,
            },
        ],
    },
];

/// Encodes the image of each entry at each of its bit rates (500 ppi, as the
/// references were made) and compares length, stream hash and decoded hash
/// with NBIS's.
fn check_large(images: &[Large]) {
    for image in images {
        let (w, h) = (image.width, image.height);
        let pixels = pattern(image.kind, w, h, (w * h) as u32);
        for run in image.runs {
            let name = format!("{} {w}x{h} at {}", image.kind, run.bitrate);
            let options = EncodeOptions::default()
                .with_bitrate(run.bitrate)
                .with_ppi(Some(500));
            let wsq = encode(&pixels, w, h, &options).unwrap();
            assert_eq!(wsq.len(), run.len, "{name}: stream length differs");
            assert_eq!(fnv1a(&wsq), run.stream, "{name}: encoded bytes differ");
            let decoded = decode(&wsq).unwrap();
            assert_eq!((decoded.width, decoded.height), (w, h), "{name}");
            assert_eq!(
                fnv1a(&decoded.pixels),
                run.decoded,
                "{name}: decoded pixels differ"
            );
        }
    }
}

#[test]
fn encodes_large_images_like_the_reference() {
    check_large(LARGE);
}

/// The 5500x8000 print of 1000 ppi palm size, too slow for a debug build:
/// `cargo test --release --test nbis -- --ignored`. About 44 million
/// pixels at 1 s per encode in release. Dark, because NBIS stops when the
/// sum of the pixel values overflows 32 bits (tests/data/README.md).
const HUGE: &[Large] = &[Large {
    kind: "dark_ridges",
    width: 5500,
    height: 8000,
    runs: &[
        LargeRun {
            bitrate: 0.75,
            len: 3_932_440,
            stream: 0x086f_15c6_e188_ef88,
            decoded: 0xbe7f_4906_ecbb_e36e,
        },
        LargeRun {
            bitrate: 2.25,
            len: 12_518_245,
            stream: 0x986e_6cfe_50d6_9e33,
            decoded: 0x1887_2de8_672c_4dcd,
        },
    ],
}];

#[test]
#[ignore = "slow: run with --release -- --ignored"]
fn encodes_huge_images_like_the_reference() {
    check_large(HUGE);
}
