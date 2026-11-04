//! Decoding arbitrary bytes: no panic, a consistent image when it succeeds,
//! and the same outcome whatever the number of threads.

#![no_main]

use libfuzzer_sys::fuzz_target;
use wsq_codec::{DecodeOptions, decode_with};

fuzz_target!(|data: &[u8]| {
    // A small limit keeps a crafted header from costing seconds per input.
    let options = DecodeOptions::default().with_max_pixels(Some(1 << 20));
    let serial = decode_with(data, &options);
    if let Ok(image) = &serial {
        assert_eq!(image.pixels.len(), image.width * image.height);
    }
    let threaded = decode_with(data, &options.with_threads(3));
    assert_eq!(serial, threaded, "the output depends on the thread count");
});
