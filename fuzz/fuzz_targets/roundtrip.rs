//! Encoding arbitrary pixels with arbitrary options, then decoding the
//! result: no panic, and the decoded size is the input size.
//!
//! The first 7 bytes of the input choose the options and the dimensions;
//! the rest, repeated as needed, are the pixels.

#![no_main]

use libfuzzer_sys::fuzz_target;
use wsq_codec::{EncodeOptions, decode, encode};

fuzz_target!(|data: &[u8]| {
    let Some((head, rest)) = data.split_first_chunk::<7>() else {
        return;
    };
    // 81 is the smallest dimension the encoder accepts.
    let width = 81 + usize::from(u16::from_be_bytes([head[0], head[1]]) % 80);
    let height = 81 + usize::from(u16::from_be_bytes([head[2], head[3]]) % 80);
    let bitrate = 0.02 + f32::from(head[4]) / 32.0;
    let options = EncodeOptions::default()
        .with_bitrate(bitrate)
        .with_ppi(match head[5] & 3 {
            0 => None,
            1 => Some(500),
            2 => Some(1000),
            _ => Some(1),
        })
        .with_threads(usize::from(head[6] % 4));
    let pixels: Vec<u8> = if rest.is_empty() {
        vec![0; width * height]
    } else {
        rest.iter().copied().cycle().take(width * height).collect()
    };
    // A bit rate that is too low is a clean error, not a failure.
    let Ok(stream) = encode(&pixels, width, height, &options) else {
        return;
    };
    let image = decode(&stream).expect("the encoder's output decodes");
    assert_eq!((image.width, image.height), (width, height));
    assert_eq!(image.pixels.len(), pixels.len());
});
