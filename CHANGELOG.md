# Changelog

## Unreleased

- The decoder and the encoder allocate their image-sized buffers fallibly (coefficients,
  transformed image, wavelet scratch, output pixels, normalized input, quantized coefficients,
  and the copies `inspect::read_quantized` returns). A refused allocation, or a size beyond the
  address space of a 32-bit target, gives the new `DecodeError::OutOfMemory` or
  `EncodeError::OutOfMemory` instead of aborting the process or panicking with "capacity
  overflow"; output is unchanged. Two kinds of allocation are still infallible: the compressed
  output of the encoder, which grows as it is written, and the wavelet's transposition blocks,
  which follow the longest dimension, not the area (at most 2 x 64 x 65535 floats per thread).
- The documented decoding peak is about 8 bytes per pixel, not 11: 787 MiB measured for a
  10000 by 10000 image (Linux x86-64, one thread). `DecodeOptions::max_pixels` and the README
  advise checking `read_info` against the dimensions of an ANSI/NIST record. CI tests a 32-bit
  target (`i686-unknown-linux-gnu`).
- The benchmark images are generated with integer arithmetic only, and `benches/hashes.txt` is
  recorded again from them (checked on Linux x86-64 with 1, 0 and 3 threads). CI runs
  `--check` on Linux, macOS and Windows.
- A thread that cannot be created (no thread support on the target, thread limits) no longer
  panics: that share of the work runs on the calling thread, and the output is unchanged.
- The encoder computes the logarithms and powers of its bit allocation itself, correctly
  rounded, instead of calling the platform's math library: its output is now the same on every
  platform. It is unchanged wherever glibc rounded correctly, which covers the NBIS fixtures.
- The `with_*` methods of `DecodeOptions` and `EncodeOptions` are `#[must_use]`, and the
  public functions that return a `Result` document their errors.
- `inspect::read_quantized` returns the header parameters, bin widths and bin indices of a
  stream, the values the FBI's certification compares.
- The `wsq` example's `cert` commands run the certification tests locally and compare the
  output with NIST's reference files (see `docs/certification.md`).

## 0.1.1

No code change. The repository history was rewritten after 0.1.0, so this release records the
new commit in the package metadata.

## 0.1.0

First release.

- WSQ decoder matching NBIS 5.0.0 pixel for pixel, including the 8/8 filter bank, with the same
  accepted and rejected inputs; corrupt streams fail with a typed error and a pixel limit bounds
  memory.
- WSQ encoder whose output is byte-identical to NBIS 5.0.0, with a PPI check, a configurable
  software implementation number and NISTCOM output.
- Opt-in multithreading (`threads` option); the output does not depend on it.
- `io::Read`/`io::Write` helpers and an optional `image` integration (feature `image`).
- Not supported: the DRT marker, which NBIS rejects as well.
