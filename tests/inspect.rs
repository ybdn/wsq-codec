//! Reading the parameters and bin indices of a stream.

use wsq_codec::inspect::read_quantized;
use wsq_codec::{DecodeError, DecodeOptions, EncodeOptions, encode};

fn image(w: usize, h: usize) -> Vec<u8> {
    (0..w * h)
        .map(|i| (((i % w) * 7 + (i / w) * 3) % 200 + 20) as u8)
        .collect()
}

#[test]
fn subbands_cover_the_coefficients() {
    let (w, h) = (256, 200);
    let options = EncodeOptions::default()
        .with_ppi(Some(500))
        .with_software(7);
    let wsq = encode(&image(w, h), w, h, &options).unwrap();
    let q = read_quantized(&wsq, &DecodeOptions::default()).unwrap();

    assert_eq!((q.info.width, q.info.height, q.info.software), (w, h, 7));
    assert_eq!(q.subbands.len(), 60);
    assert!((q.bin_center - 0.44).abs() < 1e-6);
    assert_eq!(q.synthesis_lowpass.len(), 7);
    assert_eq!(q.synthesis_highpass.len(), 9);
    for s in &q.subbands {
        if s.q_bin == 0.0 {
            assert!(s.coefficients.is_empty());
        } else {
            assert_eq!(s.coefficients.len(), s.width * s.height);
            assert!(s.x + s.width <= w && s.y + s.height <= h);
        }
    }
    assert!(q.subbands.iter().any(|s| s.q_bin != 0.0));
}

#[test]
fn reads_every_fixture() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "wsq") {
            let data = std::fs::read(&path).unwrap();
            let q = read_quantized(&data, &DecodeOptions::default()).unwrap();
            let info = wsq_codec::read_info(&data).unwrap();
            assert_eq!(q.info, info, "{}", path.display());
            count += 1;
        }
    }
    assert!(count > 0);
}

#[test]
fn keeps_the_pixel_limit() {
    let (w, h) = (100, 100);
    let wsq = encode(&image(w, h), w, h, &EncodeOptions::default()).unwrap();
    let options = DecodeOptions::default().with_max_pixels(Some(w * h - 1));
    assert!(matches!(
        read_quantized(&wsq, &options),
        Err(DecodeError::TooLarge { .. })
    ));
}
