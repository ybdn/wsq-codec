//! Reading the quantized coefficients of arbitrary bytes: no panic, and
//! the subbands are consistent with their own dimensions.

#![no_main]

use libfuzzer_sys::fuzz_target;
use wsq_codec::DecodeOptions;
use wsq_codec::inspect::read_quantized;

fuzz_target!(|data: &[u8]| {
    let options = DecodeOptions::default().with_max_pixels(Some(1 << 20));
    if let Ok(quantized) = read_quantized(data, &options) {
        assert_eq!(quantized.subbands.len(), 60);
        for band in &quantized.subbands {
            let expected = if band.q_bin == 0.0 {
                0
            } else {
                band.width * band.height
            };
            assert_eq!(band.coefficients.len(), expected);
        }
    }
});
