//! Reading the headers of arbitrary bytes: no panic, and what is read
//! agrees with a full decode.

#![no_main]

use libfuzzer_sys::fuzz_target;
use wsq_codec::{DecodeOptions, decode_with, read_info};

fuzz_target!(|data: &[u8]| {
    let info = read_info(data);
    let options = DecodeOptions::default().with_max_pixels(Some(1 << 20));
    if let (Ok(info), Ok(image)) = (&info, decode_with(data, &options)) {
        assert_eq!((info.width, info.height), (image.width, image.height));
        assert_eq!(info.ppi, image.ppi);
    }
});
