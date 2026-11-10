# wsq-codec

[![CI](https://github.com/ybdn-ei/wsq-codec/actions/workflows/ci.yml/badge.svg)](https://github.com/ybdn-ei/wsq-codec/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wsq-codec.svg)](https://crates.io/crates/wsq-codec)
[![docs.rs](https://img.shields.io/docsrs/wsq-codec)](https://docs.rs/wsq-codec)
[![license: MIT](https://img.shields.io/crates/l/wsq-codec.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-blue.svg)](Cargo.toml)

Rust decoder and encoder for **WSQ** (Wavelet Scalar Quantization) fingerprint images, the
compression the FBI specifies (IAFIS-IC-0110, version 3.1) and that ANSI/NIST-ITL transactions
carry in Types 4 and 14.

> **Not affiliated, not certified.** This project is not affiliated with or endorsed by the FBI
> or NIST, and the crate is not certified by the FBI. [docs/certification.md](docs/certification.md)
> gives the status.

> **No real fingerprints in issues.** Fingerprints are biometric data, a special category under
> GDPR article 9. Never attach them to an issue or a pull request: see
> [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

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

`read_info` returns the dimensions, resolution and comments without decoding the image; `decode`
returns the same metadata (`ppi`, `software`, `comments`) with the pixels.
`decode_with` takes `DecodeOptions`, whose `max_pixels` bounds the image size; decoding needs
about 8 bytes per pixel at its peak (787 MiB measured for a 10000 by 10000 image on Linux
x86-64). A stream of under 1 KB can declare such an image, so on untrusted input keep the limit
low. For ANSI/NIST transactions, call `read_info` first and compare the dimensions with the
record's own fields, or derive `max_pixels` from them. Every buffer whose size follows the
pixel count is allocated fallibly: the coefficients, the transformed image and its scratch
buffer, the output pixels, the encoder's compressed output (reserved from the expected size,
then grown with `try_reserve`) and the wavelet transform's transposition blocks (on every
thread). When one cannot be, `decode_with` and `encode` return `OutOfMemory` instead of
aborting, and `decode_with` does the same on a 32-bit target for a size beyond its address
space, even with `max_pixels: None`. What stays infallible is small and does not follow the
pixel count: tables of fixed size, the comments you give, vectors with one entry per row or
per subband (at most 65535 rows), and the wavelet filters' per-line plans (a few MiB at the
largest dimension). `src/mem.rs` documents the `grep` that lists them.

The bit rate is a target in bits per pixel. NBIS documents 0.75, the usual choice for 500 ppi
fingerprints, as about 15:1 compression and 2.25 as about 5:1.

### Profiles

`EncodeOptions::profile` chooses the encoder, and `encode`, `Encoder` and `encode_to` all honour
it:

- `Profile::Conformant`, the default: the encoder aligned with NBIS, byte for byte on the
  fixtures. The actual size lands near the requested bit rate, not on it. It is the only profile
  meant for FBI certification, and the only one that may write an implementation number
  (`EncodeOptions::software`).
- `Profile::Optimized`: the same layout and tables, but the quantizer bin widths are scaled by
  one factor searched on the length of the whole stream, so that the stream ends at most
  `Profile::OPTIMIZED_TOLERANCE` (0.5%) below the size `bitrate * width * height / 8` bytes, and
  never above it while a stream fits. Any WSQ decoder reads it, but it is not what the reference
  writes, so it is never conformant: a non-zero `software` is refused with
  `EncodeError::SoftwareRequiresConformant`. It costs one quantization and one Huffman coding
  per stream tried. `docs/quality.md` has the measurements.

```rust
use wsq_codec::{EncodeOptions, Profile};

let options = EncodeOptions::default()
    .with_profile(Profile::Optimized)
    .with_bitrate(0.75);
```

`Profile` is `#[non_exhaustive]`, so that a target quality can join later.

With the `image` feature (off by default, the only dependency the crate can pull), decoded
images convert into `image::GrayImage` (`TryFrom`), `wsq_codec::image::encode_gray_image` encodes one, and
`wsq_codec::image::register_hooks()` lets the `image` crate open `.wsq` files itself:

```rust,ignore
wsq_codec::image::register_hooks();
let gray = image::open("finger.wsq")?.to_luma8();
```

`register_hooks_with(DecodeOptions)` sets the options the hooks decode with (the hooks are global:
the first registration wins). `WsqDecoder` honours `image::Limits`: `max_alloc` must cover about
8 bytes per pixel, or the image is refused before decoding.

## Command line

The `wsq` tool (crate `wsq-cli`, in `cli/`) converts between binary PGM and WSQ. Install it with
`cargo install wsq-cli` (or `cargo install --git https://github.com/ybdn-ei/wsq-codec wsq-cli`);
`wsq --help` lists the commands:

```sh
wsq encode finger.pgm finger.wsq 0.75 500
wsq encode --profile optimized finger.pgm finger.wsq 0.75 500   # meets the size, not certifiable
wsq decode finger.wsq finger.pgm
wsq info finger.wsq
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

The same tool checks the decoder on a collection of files, locally:

```sh
wsq check /path/to/corpus
```

It decodes every WSQ stream under the directory, including streams inside other files such as
ANSI/NIST-ITL transactions (found by their start-of-image marker), and reports dimensions,
resolution, encoder number and decoding time. When a reference decode sits next to a stream, it
compares the pixels: `name.raw` or `name.pgm` for `name.wsq`, and `name.N.raw` or `name.N.pgm`
for the `N`th stream inside another file `name.ext`. `.raw` files hold the bare pixels, as NBIS's
`dwsq` writes them with raw output. To produce references for streams inside transactions,
`extract` writes them out as `name.N.wsq`:

```sh
wsq extract case.eft   # case.1.wsq, case.2.wsq, ...
```

`cli/tests/corpus.rs` runs the same check under `cargo test` when `WSQ_CORPUS_DIR` names a corpus,
and does nothing otherwise, so the corpus never has to leave the machine that holds it:

```sh
WSQ_CORPUS_DIR=/path/to/corpus cargo test --release --test corpus -- --nocapture
```

## Certification

The crate is not certified by the FBI. `docs/certification.md` describes the procedure, the
plan to pursue it, and the `cert` commands of the example, which run the certification tests
locally and compare the output with NIST's reference files.

## Differences from NBIS

The encoder departs from NBIS only where NBIS's output is broken:

- Images must be at least 81 pixels wide and high. Below that, some subbands are too small for
  the variance estimate that drives quantization, and NBIS computes with NaN.
- On images with almost no detail, such as smooth gradients, the bit allocation of the
  specification asks for bins too fine for 16-bit coefficients. NBIS then wraps coefficients
  around or drops every subband, and decodes a flat image. This encoder widens those bins
  just enough for the coefficients to fit.
- NBIS sums the pixel values in 32 bits and stops with an overflow error when the sum passes
  2^32 - 1, which for 5500x8000 pixels (44 million) means a mean above 97.6: it cannot encode a
  bright image of that size (`cwsq` exits with 165). This encoder sums in 64 bits and encodes
  it, so there is no NBIS output to compare with for such an image.

The decoder rejects streams whose blocks hold fewer coefficients than the subbands need, where
NBIS would read uninitialized memory.

The encoder's output is deterministic on every platform: the logarithms and powers of the bit
allocation are computed by the crate itself, in double-double arithmetic, from basic IEEE 754
operations, rather than by the platform's math library, whose last bit varies between systems,
and the rest is basic arithmetic in a fixed order without fused multiply-add. This holds on
targets whose `f64` and `f32` arithmetic follows IEEE 754, which is all of Rust's usual targets;
targets that compute `f64` on the x87 unit (`i586`) are excluded. The results of `ln` and `pow`
are correctly rounded except within about 2^-100 of the midpoint between two `f64`s, so the
claim is determinism, not exact rounding. glibc's `log` and `pow`, which NBIS used for the
fixtures, return the correctly rounded result on all but rare inputs (about 1 in 1000 for
`pow`); an image whose bit allocation hits one of those would get a quantizer bin differing from
NBIS's in its last digit, identically on every platform.

Targets where this has been executed, not only built (as of 10 November 2026; the test suite
includes the NBIS fixtures, the stream hashes are those of `benches/hashes.txt`;
`docs/certification.md` lists the runs and what has not been verified):

- x86-64 Linux, macOS and Windows, on every pull request: tests and stream hashes.
- AArch64 Linux (`ubuntu-24.04-arm`), on every pull request, job `aarch64`: tests and stream
  hashes.
- `wasm32-wasip1` under wasmtime 50.0.1, on every pull request, job `wasi`: tests and stream
  hashes, with no thread (the codec then runs every job on the calling thread).
- `s390x-unknown-linux-gnu`, big-endian, emulated by QEMU, weekly and on demand, job `s390x` of
  `scheduled.yml`: tests.

Not verified: RISC-V, other big-endian targets, and the architecture of the macOS and Windows
runners (their logs do not state it).

## Performance

The fastest of 5 runs on generated images, in memory, on a 4-core cloud x86-64 machine
(`cargo bench --bench codec -- THREADS`):

| image | encode, 1 thread | decode, 1 thread | encode, 4 threads | decode, 4 threads |
|---|---|---|---|---|
| 500 ppi rolled print, 800×750 | 11 ms | 10 ms | 10 ms | 9 ms |
| 1000 ppi, 1600×1500 | 43 ms | 38 ms | 23 ms | 19 ms |
| 1000 ppi palm, 5500×8000 | 1.13 s | 1.06 s | 0.45 s | 0.41 s |

`docs/nbis-comparison.md` compares speed and memory with NBIS 5.0.0 (distributions over repeated
calls, one process per call, one and four threads), with the protocol, the conditions and the
limits; `bench/nbis-compare` repeats it. It is a measurement on synthetic images on one machine, not
a claim for other cases.

The work stays on the calling thread unless `EncodeOptions::threads` or
`DecodeOptions::threads` asks for more (0 means one per core). The wavelet transform starts its
threads once per image and splits every pass by rows; quantization, dequantization and the
subband statistics are split by subbands or rows, the three Huffman blocks are coded and decoded
side by side, and the image-sized buffers are allocated in bands, each zeroed by its own thread.
Each value goes through the same arithmetic either way, so the result is unchanged. Threads pay
off on large images; on small ones they cost nothing. `Decoder` and `Encoder` keep their buffers
from one image to the next, which spares a batch of images their allocation.

## Quality

`benches/quality.rs` encodes generated images at several bit rates and reports, per image and
per requested rate, the actual bit rate and file size, the PSNR and SSIM of the reconstruction
and the encoding and decoding time (`cargo bench --bench quality`). It prints a table and, with
`--csv FILE`, every measurement as CSV. `--nbis BIN_DIR` runs NBIS 5.0.0's `cwsq` and `dwsq` on
the same images and compares streams and reconstructions, and `--nbis-streams DIR` reads
streams NBIS wrote elsewhere. `WSQ_QUALITY_DIR` names a local directory of PGM files to measure
too; it stays outside the repository. `--optimized` also measures the `Optimized` profile
(#42, #44), which hits the requested rate, against the conformant one at equal size. `docs/quality.md` holds the tables, with the conditions of measurement.

PSNR does not say whether a decoded image still matches, so the same tool can also run two
external programs (each optional, none a dependency of the crate): NIST's NFIQ 2 score
(`--nfiq2 PATH`) and NBIS's `mindtct` and `bozorth3` (`--mindtct PATH --bozorth3 PATH`), which
give the change in NFIQ 2, the change in the number of minutiae and the match score of the
decoded image against the original, with the original's own score as the reference. Without a
tool, its columns read "not run". `docs/quality.md` explains how to build them.

What was measured, on 10 November 2026: three **synthetic** ridge patterns (generated, not
fingerprints), 640x704 at 500 ppi, at 0.5 to 2.25 bits per pixel, with NFIQ 2 2.3.1 and NBIS
5.0.0 (36 reconstructions). NFIQ 2 changed by -9 to +2 points from the original to the decoded
image, the decoded image's minutiae matched the original's at 74% to 99% of the original's
score against itself, and the experimental encoder and the conformant encoder at equal size
were not distinguishable on these measures. Three patterns that are not fingerprints say
nothing about real prints, on which this was **not measured**: no real fingerprint may be
committed or published, so run it on a local directory (`WSQ_QUALITY_DIR`, reported in
aggregate only) and read the figures there.

```sh
cargo bench --bench quality -- --optimized --nfiq2 /path/to/nfiq2 \
  --mindtct /path/to/mindtct --bozorth3 /path/to/bozorth3
```

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) for the checks to run and the branch flow, and
[SECURITY.md](SECURITY.md) to report a vulnerability privately. Participation is governed by the
[Code of Conduct](CODE_OF_CONDUCT.md).

## License

MIT, see [LICENSE](LICENSE). The test fixtures in `tests/data` were written by NIST's NBIS,
which is in the public domain; see [tests/data/README.md](tests/data/README.md).
