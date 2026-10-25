#![cfg(feature = "image")]

use std::io::Cursor;

use image::{GrayImage, ImageDecoder, ImageReader, Luma};
use wsq_codec::image::{WsqDecoder, encode_gray_image, register_hooks};
use wsq_codec::{DecodeOptions, EncodeOptions, decode};

fn sample() -> GrayImage {
    GrayImage::from_fn(150, 120, |x, y| Luma([((x * 7 + y * 3) % 200 + 20) as u8]))
}

#[test]
fn gray_images_round_trip() {
    let gray = sample();
    let wsq = encode_gray_image(&gray, &EncodeOptions::default().with_bitrate(2.25)).unwrap();
    let image = decode(&wsq).unwrap();
    let pixels = image.pixels.clone();
    let decoded: GrayImage = image.into();
    assert_eq!(decoded.dimensions(), gray.dimensions());
    assert_eq!(decoded.as_raw(), &pixels);
}

#[test]
fn decoder_reads_dimensions_first() {
    let wsq = encode_gray_image(&sample(), &EncodeOptions::default()).unwrap();
    let decoder = WsqDecoder::new(&wsq[..]).unwrap();
    assert_eq!(decoder.dimensions(), (150, 120));
    assert_eq!(decoder.color_type(), image::ColorType::L8);
    let mut buf = vec![0; decoder.total_bytes() as usize];
    decoder.read_image(&mut buf).unwrap();
    assert_eq!(buf, decode(&wsq).unwrap().pixels);

    let strict = DecodeOptions::default().with_max_pixels(Some(100));
    let decoder = WsqDecoder::with_options(&wsq[..], strict).unwrap();
    let mut buf = vec![0; decoder.total_bytes() as usize];
    assert!(decoder.read_image(&mut buf).is_err());
    assert!(WsqDecoder::new(&b"not wsq"[..]).is_err());
}

#[test]
fn image_opens_wsq_once_hooked() {
    let wsq = encode_gray_image(&sample(), &EncodeOptions::default()).unwrap();
    let expected = decode(&wsq).unwrap().pixels;
    register_hooks();
    // A second registration leaves the first in place.
    assert!(!register_hooks());

    // Recognized by its content...
    let reader = ImageReader::new(Cursor::new(&wsq))
        .with_guessed_format()
        .unwrap();
    let opened = reader.decode().unwrap();
    assert_eq!(opened.to_luma8().as_raw(), &expected);

    // ...and by its extension.
    let path = std::env::temp_dir().join(format!("wsq-codec-{}.wsq", std::process::id()));
    std::fs::write(&path, &wsq).unwrap();
    let opened = image::open(&path);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(opened.unwrap().to_luma8().as_raw(), &expected);
}
