//! `wsq-codec` is the certified crate plus the optimized profile and the `image` feature
//! (docs/certification.md): its conformant path, its decoder and `inspect` must give exactly
//! what `wsq-certified` gives, byte for byte, whatever the options.

use wsq_codec::{DecodeOptions, EncodeOptions, Encoder, Profile};

/// A synthetic image of integer arithmetic only.
fn pixels(w: usize, h: usize) -> Vec<u8> {
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            (((x * 7 + y * 3) % 61) * 4 + (x * y) % 13) as u8
        })
        .collect()
}

/// The certified crate's settings for the same request.
fn certified(options: &EncodeOptions) -> wsq_certified::EncodeOptions {
    let mut certified = wsq_certified::EncodeOptions::default()
        .with_bitrate(options.bitrate)
        .with_ppi(options.ppi)
        .with_software(options.software)
        .with_threads(options.threads);
    for comment in &options.comments {
        certified = certified.with_comment(comment.clone());
    }
    certified
}

#[test]
fn the_conformant_profile_is_the_certified_encoder() {
    let (w, h) = (130, 110);
    let px = pixels(w, h);
    let requests = [
        EncodeOptions::default(),
        EncodeOptions::default()
            .with_bitrate(2.25)
            .with_ppi(Some(500)),
        EncodeOptions::default()
            .with_software(0x1234)
            .with_comment(b"a comment".to_vec())
            .with_comment(b"NIST_COM 1\nPPI 1000\n".to_vec()),
        EncodeOptions::default()
            .with_threads(3)
            .with_profile(Profile::Conformant),
    ];
    for options in &requests {
        let ours = wsq_codec::encode(&px, w, h, options).unwrap();
        let theirs = wsq_certified::encode(&px, w, h, &certified(options)).unwrap();
        assert!(ours == theirs, "{options:?}");
        // The encoder that keeps its buffers is the same.
        let mut kept = Encoder::new(options.clone());
        assert!(kept.encode(&px, w, h).unwrap() == theirs);
        assert!(kept.encode(&px, w, h).unwrap() == theirs);
    }
}

#[test]
fn the_errors_are_the_certified_ones() {
    let px = pixels(100, 100);
    let options = EncodeOptions::default().with_ppi(Some(0));
    assert_eq!(
        wsq_codec::encode(&px, 100, 100, &options).unwrap_err(),
        wsq_certified::encode(&px, 100, 100, &certified(&options)).unwrap_err()
    );
    // The same type, not a copy: a certified error is a `wsq-codec` error.
    let _: wsq_codec::EncodeError = wsq_certified::EncodeError::InvalidPpi;
    let _: wsq_codec::DecodeError = wsq_certified::DecodeError::NotWsq;
}

#[test]
fn the_decoder_and_inspect_are_the_certified_ones() {
    let (w, h) = (130, 110);
    let wsq = wsq_codec::encode(&pixels(w, h), w, h, &EncodeOptions::default()).unwrap();
    let ours = wsq_codec::decode(&wsq).unwrap();
    let theirs = wsq_certified::decode(&wsq).unwrap();
    assert_eq!(
        (
            ours.width,
            ours.height,
            ours.ppi,
            ours.software,
            &ours.comments,
            &ours.pixels
        ),
        (
            theirs.width,
            theirs.height,
            theirs.ppi,
            theirs.software,
            &theirs.comments,
            &theirs.pixels
        )
    );
    let mut decoder = wsq_codec::Decoder::default();
    assert_eq!(decoder.decode(&wsq).unwrap(), ours);
    assert_eq!(
        wsq_codec::read_info(&wsq).unwrap(),
        wsq_certified::read_info(&wsq).unwrap()
    );
    let options = DecodeOptions::default();
    assert_eq!(
        wsq_codec::inspect::read_quantized(&wsq, &options).unwrap(),
        wsq_certified::inspect::read_quantized(&wsq, &options).unwrap()
    );
}

#[test]
fn the_optimized_profile_is_built_on_the_certified_pipeline() {
    // The optimized stream has the conformant stream's headers (dimensions, implementation
    // number) and decodes with the certified decoder.
    let (w, h) = (130, 110);
    let px = pixels(w, h);
    let conformant = wsq_codec::encode(&px, w, h, &EncodeOptions::default()).unwrap();
    let optimized = wsq_codec::encode(
        &px,
        w,
        h,
        &EncodeOptions::default().with_profile(Profile::Optimized),
    )
    .unwrap();
    let (a, b) = (
        wsq_codec::read_info(&conformant).unwrap(),
        wsq_codec::read_info(&optimized).unwrap(),
    );
    assert_eq!(
        (a.width, a.height, a.software),
        (b.width, b.height, b.software)
    );
    assert!(wsq_codec::decode(&optimized).is_ok());
}
