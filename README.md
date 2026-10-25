# wsq-codec

Rust decoder and encoder for **WSQ** (Wavelet Scalar Quantization) fingerprint images, the
compression the FBI specifies (IAFIS-IC-0110, version 3.1) and that ANSI/NIST-ITL transactions
carry in Types 4 and 14.

- Pure Rust, no dependencies (an optional `image` integration aside), no `unsafe`.
- Matches NIST's reference implementation (NBIS): decoding gives the same pixels, and encoding
  gives the same bytes for the same image and bit rate, except where NBIS fails (see below).
  The tests check both against fixtures that NBIS produced.
- Decodes the standard 9/7 filter bank and NBIS's alternative 8/8 one; encodes with the
  standard bank.
- Reads the resolution and comments a stream carries, and writes the NISTCOM comment NBIS
  writes, merged with a NISTCOM comment of the caller's as NBIS merges it.
- Safe on untrusted input: corrupt streams fail with an error, never a panic, and the decoder
  refuses images larger than a configurable limit (100 million pixels by default) before
  allocating anything for them.

## Usage

```rust
use wsq_codec::{EncodeOptions, decode, encode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 8-bit grayscale pixels, row-major.
    let (width, height) = (416, 389);
    let pixels = vec![128u8; width * height];

    let options = EncodeOptions::default().with_bitrate(0.75).with_ppi(Some(500));
    let wsq: Vec<u8> = encode(&pixels, width, height, &options)?;

    let image = decode(&wsq)?;
    assert_eq!((image.width, image.height, image.ppi), (width, height, Some(500)));
    Ok(())
}
```

`decode_from` and `encode_to` read from an `io::Read` and write to an `io::Write`. Decoding
needs the whole stream before it can produce a pixel, so `decode_from` reads to the end: wrap
the reader in `Read::take` when the stream sits inside a larger file.

`read_info` returns the dimensions, resolution and comments without decoding the image.
`decode_with` takes `DecodeOptions`, whose `max_pixels` bounds the image size; decoding needs
about 11 bytes per pixel at its peak.

The bit rate is a target in bits per pixel. NBIS documents 0.75, the usual choice for 500 ppi
fingerprints, as about 15:1 compression and 2.25 as about 5:1.

With the `image` feature (off by default, the only dependency the crate can pull), decoded
images convert into `image::GrayImage`, `wsq_codec::image::encode_gray_image` encodes one, and
`wsq_codec::image::register_hooks()` lets the `image` crate open `.wsq` files itself:

```rust,ignore
wsq_codec::image::register_hooks();
let gray = image::open("finger.wsq")?.to_luma8();
```

An example converts between binary PGM and WSQ:

```sh
cargo run --release --example wsq -- encode finger.pgm finger.wsq 0.75 500
cargo run --release --example wsq -- decode finger.wsq finger.pgm
cargo run --release --example wsq -- info finger.wsq
```

## ANSI/NIST-ITL transactions

This crate handles the WSQ stream only. Parsing the Type-4 and Type-14 records of ANSI/NIST-ITL
transactions (EBTS and national profiles) belongs in a separate crate, which would hand the
bytes of the image field to `decode` or `read_info`. In a transaction the authoritative
resolution is the one the record gives (fields 14.009 and 14.010 in Type-14; for Type-4, its
ISR byte with the resolution fields of the Type-1 record), not `Image::ppi`, which only repeats
what the NISTCOM comment in the stream says, when it says anything. Since transactions come
from other parties, keep the decoder's pixel limit.

## Checking real files

The same example checks the decoder on a collection of files, locally:

```sh
cargo run --release --example wsq -- check /path/to/corpus
```

It decodes every WSQ stream under the directory, including streams inside other files such as
ANSI/NIST-ITL transactions (found by their start-of-image marker), and reports dimensions,
resolution, encoder number and decoding time. When a reference decode sits next to a stream, it
compares the pixels: `name.raw` or `name.pgm` for `name.wsq`, and `name.N.raw` or `name.N.pgm`
for the `N`th stream inside another file `name.ext`. `.raw` files hold the bare pixels, as NBIS's
`dwsq` writes them with raw output. To produce references for streams inside transactions,
`extract` writes them out as `name.N.wsq`:

```sh
cargo run --release --example wsq -- extract case.eft   # case.1.wsq, case.2.wsq, ...
```

## Differences from NBIS

The encoder departs from NBIS only where NBIS's output is broken:

- Images must be at least 81 pixels wide and high. Below that, some subbands are too small for
  the variance estimate that drives quantization, and NBIS computes with NaN.
- On images with almost no detail, such as smooth gradients, the bit allocation of the
  specification asks for bins too fine for 16-bit coefficients. NBIS then wraps coefficients
  around or drops every subband, and decodes a flat image. This encoder widens those bins
  just enough for the coefficients to fit.

The decoder rejects streams whose blocks hold fewer coefficients than the subbands need, where
NBIS would read uninitialized memory.

Byte-identical encoding relies on the platform's `pow` and `log` rounding like glibc's. CI
checks it on Linux, macOS and Windows against the NBIS fixtures; an image whose bit allocation
hits a rounding difference could still get a quantizer bin that differs in its last digit.

## Performance

The fastest of 5 runs on generated images, in memory, on a 4-core cloud x86-64 machine
(`cargo run --release --example bench -- THREADS`):

| image | encode, 1 thread | decode, 1 thread | encode, 4 threads | decode, 4 threads |
|---|---|---|---|---|
| 500 ppi rolled print, 800×750 | 12 ms | 11 ms | 12 ms | 12 ms |
| 1000 ppi, 1600×1500 | 50 ms | 45 ms | 45 ms | 39 ms |
| 1000 ppi palm, 5500×8000 | 1.17 s | 1.07 s | 0.70 s | 0.69 s |

The work stays on the calling thread unless `EncodeOptions::threads` or
`DecodeOptions::threads` asks for more (0 means one per core). Only the wavelet transform is
split, by rows and by columns, which leaves the result unchanged; it pays off on large images
only.

## License

MIT, see [LICENSE](LICENSE).
