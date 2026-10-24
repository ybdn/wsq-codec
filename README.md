# wsq-codec

Rust decoder and encoder for **WSQ** (Wavelet Scalar Quantization) fingerprint images, the
compression the FBI specifies (IAFIS-IC-0110, version 3.1) and that ANSI/NIST-ITL transactions
carry in Types 4 and 14.

- Pure Rust, no dependencies, no `unsafe`.
- Matches NIST's reference implementation (NBIS): decoding gives the same pixels, and encoding
  gives the same bytes for the same image and bit rate, except where NBIS fails (see below).
  The tests check both against fixtures that NBIS produced.
- Decodes the standard 9/7 filter bank and NBIS's alternative 8/8 one; encodes with the
  standard bank.
- Reads the resolution and comments a stream carries, and writes the NISTCOM comment NBIS
  writes.

## Usage

```rust
use wsq_codec::{EncodeOptions, decode, encode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 8-bit grayscale pixels, row-major.
    let (width, height) = (416, 389);
    let pixels = vec![128u8; width * height];

    let options = EncodeOptions { bitrate: 0.75, ppi: Some(500), ..Default::default() };
    let wsq: Vec<u8> = encode(&pixels, width, height, &options)?;

    let image = decode(&wsq)?;
    assert_eq!((image.width, image.height, image.ppi), (width, height, Some(500)));
    Ok(())
}
```

`read_info` returns the dimensions, resolution and comments without decoding the image.

The bit rate is a target in bits per pixel. NBIS documents 0.75, the usual choice for 500 ppi
fingerprints, as about 15:1 compression and 2.25 as about 5:1.

An example converts between binary PGM and WSQ:

```sh
cargo run --release --example wsq -- encode finger.pgm finger.wsq 0.75 500
cargo run --release --example wsq -- decode finger.wsq finger.pgm
cargo run --release --example wsq -- info finger.wsq
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

Byte-identical encoding relies on the platform's `pow` and `log` agreeing with glibc's, which
holds on Linux; elsewhere an occasional quantizer bin may differ in its last digit.

## License

MIT, see [LICENSE](LICENSE).
