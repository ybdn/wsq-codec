use std::io::{self, Cursor, Read, Write};

use wsq_codec::{
    DecodeError, DecodeOptions, EncodeError, EncodeOptions, ReadError, WriteError, decode_from,
    encode, encode_to,
};

fn pattern(w: usize, h: usize) -> Vec<u8> {
    (0..w * h)
        .map(|i| (((i % w) * 7 + (i / w) * 3) % 200 + 20) as u8)
        .collect()
}

#[test]
fn streams_round_trip() {
    let (w, h) = (120, 100);
    let pixels = pattern(w, h);
    let options = EncodeOptions::default().with_ppi(Some(500));
    let mut out = Vec::new();
    encode_to(&mut out, &pixels, w, h, &options).unwrap();
    assert_eq!(out, encode(&pixels, w, h, &options).unwrap());

    // Followed by other data, which `take` keeps out.
    let len = out.len() as u64;
    out.extend_from_slice(b"trailing record");
    let mut reader = Cursor::new(&out);
    let image = decode_from(Read::take(&mut reader, len), &DecodeOptions::default()).unwrap();
    assert_eq!((image.width, image.height, image.ppi), (w, h, Some(500)));
    assert_eq!(reader.position(), len);
}

struct Failing;

impl Read for Failing {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("broken"))
    }
}

impl Write for Failing {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("broken"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn errors_are_kept_apart() {
    let options = DecodeOptions::default();
    assert!(matches!(
        decode_from(Failing, &options),
        Err(ReadError::Io(_))
    ));
    assert!(matches!(
        decode_from(&b"not wsq"[..], &options),
        Err(ReadError::Decode(DecodeError::NotWsq))
    ));

    let pixels = pattern(100, 100);
    let options = EncodeOptions::default();
    assert!(matches!(
        encode_to(Failing, &pixels, 100, 100, &options),
        Err(WriteError::Io(_))
    ));
    assert!(matches!(
        encode_to(Vec::new(), &pixels, 50, 200, &options),
        Err(WriteError::Encode(EncodeError::InvalidDimensions { .. }))
    ));
    let e = encode_to(Failing, &pixels, 100, 100, &options).unwrap_err();
    assert!(std::error::Error::source(&e).is_some());
}
