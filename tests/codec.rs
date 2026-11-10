use wsq_codec::{
    DecodeError, DecodeOptions, Decoder, EncodeError, EncodeOptions, Encoder, decode, decode_into,
    decode_with, encode, read_info,
};

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
    let options = EncodeOptions::default().with_bitrate(bitrate);
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
    let options = EncodeOptions::default()
        .with_bitrate(1.5)
        .with_ppi(Some(1000))
        .with_comment(b"first".to_vec())
        .with_comment(vec![0xFF, 0xA1, 0xFF])
        .with_software(0x1234);
    let wsq = encode(&ridges(w, h), w, h, &options).unwrap();
    let info = read_info(&wsq).unwrap();
    assert_eq!((info.width, info.height, info.ppi), (w, h, Some(1000)));
    assert_eq!(info.software, 0x1234);
    assert_eq!(info.comments.len(), 3);
    assert!(info.comments[0].starts_with(b"NIST_COM "));
    assert_eq!(info.comments[1..], options.comments[..]);
    // One `decode` call gives the pixels and all the metadata.
    let image = decode(&wsq).unwrap();
    assert_eq!(image.ppi, info.ppi);
    assert_eq!(image.software, 0x1234);
    assert_eq!(image.comments, info.comments);
}

#[test]
fn merges_a_caller_nistcom_comment() {
    let (w, h) = (128, 96);
    let options = EncodeOptions::default()
        .with_ppi(Some(500))
        .with_comment(b"plain".to_vec())
        .with_comment(b"NIST_COM 3\nFING_CLASS W\nPPI 1000".to_vec())
        .with_comment(b"NIST_COM 2\nSEX f".to_vec());
    let info = read_info(&encode(&ridges(w, h), w, h, &options).unwrap()).unwrap();
    // The first caller NISTCOM is merged; the second one is kept as given.
    assert_eq!(info.comments.len(), 3);
    let nist = String::from_utf8(info.comments[0].clone()).unwrap();
    assert!(
        nist.starts_with("NIST_COM 10\nFING_CLASS W\nPPI 500\nPIX_WIDTH 128\n"),
        "{nist}"
    );
    assert_eq!(info.comments[1], b"plain");
    assert_eq!(info.comments[2], b"NIST_COM 2\nSEX f");
    assert_eq!(info.ppi, Some(500));
}

#[test]
fn threads_do_not_change_the_output() {
    // Large enough for the wavelet to split its first levels over threads.
    let (w, h) = (1031, 777);
    let pixels = ridges(w, h);
    let wsq = encode(&pixels, w, h, &EncodeOptions::default()).unwrap();
    let image = decode(&wsq).unwrap();
    for threads in [0, 2, 3, 8] {
        let options = EncodeOptions::default().with_threads(threads);
        assert_eq!(encode(&pixels, w, h, &options).unwrap(), wsq, "{threads}");
        let options = DecodeOptions::default().with_threads(threads);
        assert_eq!(decode_with(&wsq, &options).unwrap(), image, "{threads}");
    }
}

#[test]
fn encoder_rejects_bad_input() {
    let opts = EncodeOptions::default();
    let px = vec![0u8; 100 * 100];
    assert!(matches!(
        encode(&px[..80 * 100], 80, 100, &opts),
        Err(EncodeError::InvalidDimensions {
            width: 80,
            height: 100,
            ..
        })
    ));
    assert!(matches!(
        encode(&px, 100, 99, &opts),
        Err(EncodeError::PixelCountMismatch {
            expected: 9900,
            actual: 10000,
            ..
        })
    ));
    assert!(encode(&vec![0; 70000 * 81], 70000, 81, &opts).is_err());
    for bitrate in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let opts = EncodeOptions::default().with_bitrate(bitrate);
        assert!(matches!(
            encode(&px, 100, 100, &opts),
            Err(EncodeError::InvalidBitrate(_))
        ));
    }
    assert_eq!(
        encode(&px, 100, 100, &EncodeOptions::default().with_ppi(Some(0))),
        Err(EncodeError::InvalidPpi)
    );
    let opts = EncodeOptions::default().with_comment(vec![b'x'; 65534]);
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
        &EncodeOptions::default().with_ppi(Some(500)),
    )
    .unwrap()
}

/// `wsq` with the dimensions in its frame header replaced.
fn with_dimensions(wsq: &[u8], width: u16, height: u16) -> Vec<u8> {
    let sof = wsq.windows(2).position(|m| m == [0xFF, 0xA2]).unwrap();
    let mut out = wsq.to_vec();
    out[sof + 6..sof + 8].copy_from_slice(&height.to_be_bytes());
    out[sof + 8..sof + 10].copy_from_slice(&width.to_be_bytes());
    out
}

#[test]
fn oversized_images_are_refused_before_decoding() {
    let huge = with_dimensions(&sample(), 65535, 65535);
    assert!(matches!(
        decode(&huge),
        Err(DecodeError::TooLarge {
            width: 65535,
            height: 65535,
            max_pixels: wsq_codec::DEFAULT_MAX_PIXELS,
            ..
        })
    ));
    // Reading the header alone needs no limit.
    assert_eq!(read_info(&huge).unwrap().width, 65535);

    let wsq = sample();
    let strict = DecodeOptions::default().with_max_pixels(Some(90 * 85 - 1));
    assert!(matches!(
        decode_with(&wsq, &strict),
        Err(DecodeError::TooLarge { .. })
    ));
    let exact = DecodeOptions::default().with_max_pixels(Some(90 * 85));
    assert!(decode_with(&wsq, &exact).is_ok());
    let unlimited = DecodeOptions::default().with_max_pixels(None);
    assert_eq!(decode_with(&wsq, &unlimited), decode(&wsq));
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

#[test]
fn reused_buffers_do_not_change_the_output() {
    // Larger, then smaller, then larger again, so that the kept buffers
    // hold samples of an earlier image.
    let sizes = [(300, 280), (120, 97), (1031, 777), (300, 280)];
    for threads in [1, 3] {
        let mut encoder = Encoder::new(EncodeOptions::default().with_threads(threads));
        let mut decoder = Decoder::new(DecodeOptions::default().with_threads(threads));
        for (w, h) in sizes {
            let pixels = ridges(w, h);
            let wsq = encode(&pixels, w, h, &EncodeOptions::default()).unwrap();
            assert_eq!(encoder.encode(&pixels, w, h).unwrap(), wsq);
            let image = decode(&wsq).unwrap();
            assert_eq!(decoder.decode(&wsq).unwrap(), image);
            let mut out = vec![0x55; w * h];
            let info = decoder.decode_into(&wsq, &mut out).unwrap();
            assert_eq!((info.width, info.height), (w, h));
            assert_eq!(out, image.pixels);
            let mut out = vec![0; w * h];
            decode_into(&wsq, &DecodeOptions::default(), &mut out).unwrap();
            assert_eq!(out, image.pixels);
        }
    }
}

#[test]
fn decode_into_checks_the_buffer_size() {
    let wsq = sample();
    let info = read_info(&wsq).unwrap();
    let mut out = vec![0; info.width * info.height + 1];
    let expected = info.width * info.height;
    assert!(matches!(
        decode_into(&wsq, &DecodeOptions::default(), &mut out),
        Err(DecodeError::OutputSize { expected: e, actual: a, .. })
            if e == expected && a == expected + 1
    ));
}

/// The `Optimized` profile (#42, #44): the stream lands just below the requested size.
mod optimized {
    use super::*;
    use wsq_codec::{Profile, WriteError, encode_to};

    const TOLERANCE: f64 = Profile::OPTIMIZED_TOLERANCE;

    fn optimized() -> EncodeOptions {
        EncodeOptions::default().with_profile(Profile::Optimized)
    }

    fn target(w: usize, h: usize, bitrate: f32) -> f64 {
        f64::from(bitrate) * (w * h) as f64 / 8.0
    }

    #[test]
    fn conformant_is_the_default() {
        assert_eq!(EncodeOptions::default().profile, Profile::Conformant);
        assert_eq!(Profile::default(), Profile::Conformant);
        let (w, h) = (200, 180);
        let pixels = ridges(w, h);
        let default = encode(&pixels, w, h, &EncodeOptions::default()).unwrap();
        let explicit = EncodeOptions::default().with_profile(Profile::Conformant);
        assert_eq!(encode(&pixels, w, h, &explicit).unwrap(), default);
        assert_ne!(encode(&pixels, w, h, &optimized()).unwrap(), default);
    }

    #[test]
    fn hits_the_requested_rate() {
        for (w, h) in [(416, 389), (257, 301)] {
            let pixels = ridges(w, h);
            for bitrate in [0.75, 1.5, 2.25] {
                let options = optimized().with_bitrate(bitrate);
                let wsq = encode(&pixels, w, h, &options).unwrap();
                let target = target(w, h, bitrate);
                let len = wsq.len() as f64;
                assert!(len <= target, "{bitrate}: {len} > {target}");
                assert!(len >= target * (1.0 - TOLERANCE), "{bitrate}: {len}");
                let image = decode(&wsq).unwrap();
                assert_eq!((image.width, image.height), (w, h));
                assert!(psnr(&pixels, &image.pixels) > 28.0);
            }
        }
    }

    #[test]
    fn every_entry_point_honours_the_profile() {
        let (w, h) = (300, 250);
        let pixels = ridges(w, h);
        let options = optimized().with_bitrate(1.0).with_ppi(Some(500));
        let wsq = encode(&pixels, w, h, &options).unwrap();
        let mut encoder = Encoder::new(options.clone());
        for _ in 0..2 {
            assert_eq!(encoder.encode(&pixels, w, h).unwrap(), wsq);
        }
        let mut written = Vec::new();
        encode_to(&mut written, &pixels, w, h, &options).unwrap();
        assert_eq!(written, wsq);
        assert!((wsq.len() as f64) <= target(w, h, 1.0));
    }

    #[test]
    fn threads_do_not_change_the_output() {
        let (w, h) = (640, 480);
        let pixels = ridges(w, h);
        let options = optimized().with_bitrate(1.0);
        let one = encode(&pixels, w, h, &options).unwrap();
        for threads in [0, 2, 3, 8] {
            let many = encode(&pixels, w, h, &options.clone().with_threads(threads));
            assert_eq!(many.unwrap(), one, "{threads} threads");
        }
    }

    #[test]
    fn refuses_an_implementation_number() {
        let (w, h) = (200, 180);
        let pixels = ridges(w, h);
        let options = optimized().with_software(1234);
        let refused = Err(EncodeError::SoftwareRequiresConformant(1234));
        assert_eq!(encode(&pixels, w, h, &options), refused);
        assert_eq!(Encoder::new(options.clone()).encode(&pixels, w, h), refused);
        assert!(matches!(
            encode_to(Vec::new(), &pixels, w, h, &options),
            Err(WriteError::Encode(EncodeError::SoftwareRequiresConformant(
                1234
            )))
        ));
        assert_eq!(
            refused.clone().unwrap_err().to_string(),
            "implementation number 1234 can only be written by the conformant profile"
        );
        // Zero is no number, and the conformant profile still writes its own.
        let wsq = encode(&pixels, w, h, &optimized().with_ppi(Some(500))).unwrap();
        assert_eq!(read_info(&wsq).unwrap().software, 0);
        let conformant = options.clone().with_profile(Profile::Conformant);
        let wsq = encode(&pixels, w, h, &conformant).unwrap();
        assert_eq!(read_info(&wsq).unwrap().software, 1234);
    }

    #[test]
    fn unreachable_rates_give_the_nearest_stream() {
        let (w, h) = (200, 180);
        let pixels = ridges(w, h);
        // Far above what the image can spend: the longest stream, still under the target.
        let options = optimized().with_bitrate(40.0);
        let wsq = encode(&pixels, w, h, &options).unwrap();
        assert!((wsq.len() as f64) < target(w, h, 40.0));
        let conformant = options.clone().with_profile(Profile::Conformant);
        assert!(wsq.len() >= encode(&pixels, w, h, &conformant).unwrap().len());
        // Far below: the shortest stream the quantization table allows.
        let options = optimized().with_bitrate(0.001);
        let wsq = encode(&pixels, w, h, &options).unwrap();
        assert!((wsq.len() as f64) > target(w, h, 0.001));
        assert_eq!(decode(&wsq).unwrap().pixels.len(), w * h);
        // A flat image has nothing to spend bits on.
        let wsq = encode(&vec![77; w * h], w, h, &optimized()).unwrap();
        assert!(decode(&wsq).unwrap().pixels.iter().all(|&p| p == 77));
    }

    #[test]
    fn rejects_what_conformant_rejects() {
        let options = optimized().with_bitrate(-1.0);
        assert_eq!(
            encode(&[0; 100 * 100], 100, 100, &options),
            Err(EncodeError::InvalidBitrate(-1.0))
        );
        assert!(matches!(
            encode(&[0; 80 * 100], 80, 100, &optimized()),
            Err(EncodeError::InvalidDimensions {
                width: 80,
                height: 100,
                ..
            })
        ));
        assert_eq!(
            encode(&[0; 100 * 100], 100, 100, &optimized().with_ppi(Some(0))),
            Err(EncodeError::InvalidPpi)
        );
    }
}
