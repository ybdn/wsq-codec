//! The hooks are global to the process, so their options get a test binary of
//! their own, where nothing else registers them first.

#![cfg(feature = "image")]

use std::io::Cursor;

use image::ImageReader;
use wsq_codec::image::{encode_gray_image, register_hooks, register_hooks_with};
use wsq_codec::{DecodeOptions, EncodeOptions};

#[test]
fn hook_options_apply_and_the_first_registration_wins() {
    let gray = image::GrayImage::from_fn(150, 120, |x, y| image::Luma([((x + y) % 250) as u8]));
    let wsq = encode_gray_image(&gray, &EncodeOptions::default()).unwrap();
    let open = || {
        ImageReader::new(Cursor::new(&wsq))
            .with_guessed_format()
            .unwrap()
            .decode()
    };

    assert!(register_hooks_with(
        DecodeOptions::default().with_max_pixels(Some(1000))
    ));
    // 18000 pixels are more than the options allow.
    assert!(open().is_err());
    // Neither a later registration nor the default hooks replace them.
    assert!(!register_hooks_with(DecodeOptions::default()));
    assert!(!register_hooks());
    assert!(open().is_err());
}
