//! Compares against fixtures written by the NIST reference implementation
//! (see tests/data/README.md).

use wsq_codec::{EncodeOptions, decode, encode, read_info};

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
        decoded: 0x3381_3f51_f197_5ea8,
    },
];

const EVEN_FILTERS: Fixture = Fixture {
    file: "rings_300x211_even.wsq",
    wsq: include_bytes!("data/rings_300x211_even.wsq"),
    kind: "rings",
    width: 300,
    height: 211,
    bitrate: 0.75,
    ppi: Some(500),
    decoded: 0xb497_af5e_c6f5_cf5c,
};

#[test]
fn encodes_like_the_reference() {
    for f in FIXTURES {
        let pixels = pattern(f.kind, f.width, f.height, (f.width * f.height) as u32);
        let options = EncodeOptions {
            bitrate: f.bitrate,
            ppi: f.ppi,
            ..Default::default()
        };
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
