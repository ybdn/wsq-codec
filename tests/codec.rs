use wsq_codec::{DecodeError, EncodeError, EncodeOptions, decode, encode, read_info};

/// A fingerprint-like image: curved ridges with some noise.
fn ridges(w: usize, h: usize) -> Vec<u8> {
    let mut state = 12345u32;
    (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let r = ((x - w as f32 / 2.0).powi(2) + (y - h as f32 * 0.6).powi(2)).sqrt();
            let v =
                128.0 + 90.0 * (r / 3.0 + (x / 41.0).sin()).sin() + f32::from((state >> 28) as u8);
            v.clamp(0.0, 255.0) as u8
        })
        .collect()
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mse = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| (f64::from(x) - f64::from(y)).powi(2))
        .sum::<f64>()
        / a.len() as f64;
    10.0 * (255.0f64 * 255.0 / mse).log10()
}

fn round_trip(pixels: &[u8], w: usize, h: usize, bitrate: f32) -> (usize, f64) {
    let options = EncodeOptions {
        bitrate,
        ..Default::default()
    };
    let wsq = encode(pixels, w, h, &options).unwrap();
    let image = decode(&wsq).unwrap();
    assert_eq!((image.width, image.height), (w, h));
    (wsq.len(), psnr(pixels, &image.pixels))
}

#[test]
fn quality_follows_the_bit_rate() {
    let (w, h) = (416, 389);
    let pixels = ridges(w, h);
    let (low_len, low_psnr) = round_trip(&pixels, w, h, 0.75);
    let (high_len, high_psnr) = round_trip(&pixels, w, h, 2.25);
    // The actual rate lands near the target.
    let rate = |len: usize| (len * 8) as f64 / (w * h) as f64;
    assert!((0.5..1.0).contains(&rate(low_len)), "{}", rate(low_len));
    assert!((1.8..2.7).contains(&rate(high_len)), "{}", rate(high_len));
    assert!(low_psnr > 30.0, "{low_psnr}");
    assert!(high_psnr > low_psnr + 3.0, "{low_psnr} {high_psnr}");
}

#[test]
fn smallest_and_odd_sizes() {
    for (w, h) in [(81, 81), (81, 300), (300, 81), (255, 257), (1023, 97)] {
        let (_, quality) = round_trip(&ridges(w, h), w, h, 2.25);
        assert!(quality > 28.0, "{w}x{h}: {quality}");
    }
}

#[test]
fn flat_and_smooth_images() {
    let (w, h) = (200, 150);
    for value in [0, 77, 255] {
        let wsq = encode(&vec![value; w * h], w, h, &EncodeOptions::default()).unwrap();
        assert!(
            decode(&wsq).unwrap().pixels.iter().all(|&p| p == value),
            "{value}"
        );
    }
    // Gradients leave almost every subband empty, which the bit
    // allocation of the specification handles poorly.
    let gradient: Vec<u8> = (0..w * h)
        .map(|i| ((i % w) * 200 / w + (i / w) * 50 / h) as u8)
        .collect();
    let (_, quality) = round_trip(&gradient, w, h, 0.75);
    assert!(quality > 40.0, "{quality}");
}

#[test]
fn metadata() {
    let (w, h) = (128, 96);
    let options = EncodeOptions {
        bitrate: 1.5,
        ppi: Some(1000),
        comments: vec![b"first".to_vec(), vec![0xFF, 0xA1, 0xFF]],
    };
    let wsq = encode(&ridges(w, h), w, h, &options).unwrap();
    let info = read_info(&wsq).unwrap();
    assert_eq!((info.width, info.height, info.ppi), (w, h, Some(1000)));
    assert_eq!(info.comments.len(), 3);
    assert!(info.comments[0].starts_with(b"NIST_COM "));
    assert_eq!(info.comments[1..], options.comments[..]);
    assert_eq!(decode(&wsq).unwrap().ppi, Some(1000));
}

#[test]
fn encoder_rejects_bad_input() {
    let opts = EncodeOptions::default();
    let px = vec![0u8; 100 * 100];
    assert_eq!(
        encode(&px[..80 * 100], 80, 100, &opts),
        Err(EncodeError::InvalidDimensions {
            width: 80,
            height: 100
        })
    );
    assert_eq!(
        encode(&px, 100, 99, &opts),
        Err(EncodeError::PixelCountMismatch {
            expected: 9900,
            actual: 10000
        })
    );
    assert!(encode(&vec![0; 70000 * 81], 70000, 81, &opts).is_err());
    for bitrate in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let opts = EncodeOptions {
            bitrate,
            ..Default::default()
        };
        assert!(matches!(
            encode(&px, 100, 100, &opts),
            Err(EncodeError::InvalidBitrate(_))
        ));
    }
    let opts = EncodeOptions {
        comments: vec![vec![b'x'; 65534]],
        ..Default::default()
    };
    assert_eq!(
        encode(&px, 100, 100, &opts),
        Err(EncodeError::CommentTooLong(65534))
    );
}

#[test]
fn decoder_rejects_non_wsq() {
    assert_eq!(decode(b""), Err(DecodeError::NotWsq));
    assert_eq!(decode(b"\xFF"), Err(DecodeError::NotWsq));
    assert_eq!(decode(b"\xFF\xD8\xFF\xE0"), Err(DecodeError::NotWsq));
    assert_eq!(read_info(b"\xFF\xA0"), Err(DecodeError::UnexpectedEof));
}

fn sample() -> Vec<u8> {
    let (w, h) = (90, 85);
    encode(
        &ridges(w, h),
        w,
        h,
        &EncodeOptions {
            ppi: Some(500),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn truncated_streams_fail_cleanly() {
    let wsq = sample();
    for len in 0..wsq.len() {
        assert!(decode(&wsq[..len]).is_err(), "truncated to {len}");
    }
    assert!(decode(&wsq).is_ok());
}

#[test]
fn corrupted_streams_do_not_panic() {
    let wsq = sample();
    let mut state = 1u32;
    let mut rand = |n: usize| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as usize % n
    };
    for _ in 0..3000 {
        let mut data = wsq.clone();
        for _ in 0..1 + rand(4) {
            let at = rand(data.len());
            data[at] = rand(256) as u8;
        }
        if let Ok(image) = decode(&data) {
            assert_eq!(image.pixels.len(), image.width * image.height);
        }
        let _ = read_info(&data);
    }
}
