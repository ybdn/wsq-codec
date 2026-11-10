//! Runs the exported functions as JavaScript sees them, on WebAssembly:
//! `wasm-pack test --node wasm` (or `--headless --firefox` for a browser).
#![cfg(target_arch = "wasm32")]

use js_sys::{Error, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;
use wsq_wasm::{JsEncodeOptions, decode, encode, read_info};

/// A synthetic NBIS-encoded stream of the codec's own fixtures: 101 by 97 pixels.
const FIXTURE: &[u8] = include_bytes!("../../certified/tests/data/checker_101x97.wsq");

/// Diagonal ridges, 150 by 130 pixels.
fn ridges(width: u32, height: u32) -> Vec<u8> {
    (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let phase = (x + 2 * y) as f32 * 0.9;
                (128.0 + 100.0 * phase.sin()) as u8
            })
        })
        .collect()
}

fn options(fields: &[(&str, JsValue)]) -> JsEncodeOptions {
    let object = Object::new();
    for (key, value) in fields {
        Reflect::set(&object, &JsValue::from_str(key), value).unwrap();
    }
    object.unchecked_into()
}

fn message(error: impl Into<JsValue>) -> String {
    error.into().dyn_into::<Error>().unwrap().message().into()
}

#[wasm_bindgen_test]
fn decodes_a_fixture() {
    let image = decode(FIXTURE).unwrap_or_else(|_| panic!("decode failed"));
    assert_eq!((image.width(), image.height()), (101, 97));
    assert_eq!(image.pixels().len(), 101 * 97);
    let info = read_info(FIXTURE).unwrap_or_else(|_| panic!("read_info failed"));
    assert_eq!((info.width(), info.height()), (101, 97));
    assert_eq!(info.ppi(), image.ppi());
    assert_eq!(info.comments(), image.comments());
}

#[wasm_bindgen_test]
fn round_trips_both_profiles() {
    let (width, height) = (150, 130);
    let pixels = ridges(width, height);
    for profile in ["conformant", "optimized"] {
        let opts = options(&[
            ("bitrate", JsValue::from_f64(2.25)),
            ("ppi", JsValue::from_f64(500.0)),
            ("comment", JsValue::from_str("made in a test")),
            ("profile", JsValue::from_str(profile)),
        ]);
        let wsq = encode(&pixels, width, height, Some(opts)).unwrap_or_else(|_| panic!("encode"));
        assert!(
            wsq.len() < pixels.len() / 2,
            "{profile}: {} bytes",
            wsq.len()
        );

        let info = read_info(&wsq).unwrap_or_else(|_| panic!("read_info"));
        assert_eq!(
            (info.width(), info.height(), info.ppi()),
            (width, height, Some(500))
        );
        assert!(info.comments().iter().any(|c| c.contains("made in a test")));

        let image = decode(&wsq).unwrap_or_else(|_| panic!("decode"));
        assert_eq!((image.width(), image.height()), (width, height));
        let decoded = image.pixels();
        let error: u64 = pixels
            .iter()
            .zip(&decoded)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = error as f64 / pixels.len() as f64;
        assert!(mean < 12.0, "{profile}: mean error {mean}");
    }
}

#[wasm_bindgen_test]
fn encodes_with_default_options() {
    let pixels = ridges(100, 100);
    let wsq = encode(&pixels, 100, 100, None).unwrap_or_else(|_| panic!("encode"));
    let image = decode(&wsq).unwrap_or_else(|_| panic!("decode"));
    assert_eq!(image.ppi(), None);
    // The same bytes the codec gives natively: the output does not depend on the target.
    let native = wsq_codec::encode(&pixels, 100, 100, &Default::default()).unwrap();
    assert_eq!(wsq, native);
}

#[wasm_bindgen_test]
fn errors_are_exceptions_with_the_codec_message() {
    let Err(error) = decode(b"definitely not a WSQ file") else {
        panic!("garbage decoded");
    };
    let expected = wsq_codec::decode(b"definitely not a WSQ file").unwrap_err();
    assert_eq!(message(error), expected.to_string());

    let Err(error) = encode(&[0; 10], 10, 10, None) else {
        panic!("a 10 by 10 image encoded");
    };
    assert!(!message(error).is_empty());

    let Err(error) = encode(
        &ridges(100, 100),
        100,
        100,
        Some(options(&[("profile", "fast".into())])),
    ) else {
        panic!("unknown profile accepted");
    };
    assert!(message(error).contains("unknown profile"));

    let Err(error) = encode(
        &ridges(100, 100),
        100,
        100,
        Some(options(&[("bitrate", (-1.0).into())])),
    ) else {
        panic!("negative bit rate accepted");
    };
    assert!(!message(error).is_empty());
}
