# Changelog

## Unreleased

- `tests/nbis.rs` compares the encoder with NBIS 5.0.0 on large images without committing any
  stream: the length and the FNV-1a hash of NBIS's stream and of its decode, at 0.75 and 2.25
  bits per pixel, for 800x750 and 1600x1500 images (ridges, rings, noise) in the default tests
  and a 5500x8000 image in an `#[ignore]`d test (`cargo test --release --test nbis --
  --ignored`). Measured against NBIS's own `cwsq` and `dwsq`, built with `-ffp-contract=off`:
  all ten streams are identical to the byte (zero differing bytes) and decode to the same
  pixels. `tests/data/README.md` has the procedure and `make_large_references.py` the
  generator. NBIS cannot encode a 5500x8000 image whose mean is above 97.6 (its pixel sum
  overflows 32 bits); this crate can, and the README lists it among the differences.
- `wsq cert encode` no longer runs the `rustc` found on the `PATH` for `submission.txt`: the
  compiler is the one given with `--compiler "$(rustc -vV)"`, or a line to fill in.
- CI also runs on pushes to `develop`. Releases are published by a workflow triggered by a `v*`
  tag on `main`, through crates.io Trusted Publishing; the steps are in `CONTRIBUTING.md`.
- Public-crate showcase: `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, issue and pull
  request templates, README badges and a notice (not affiliated with the FBI or NIST, not
  certified), `homepage` in the package metadata and an `include` list in place of `exclude`.
- A structured test (`generated_streams_decode_or_fail_cleanly`) feeds the decoder valid streams
  with random dimensions, filter banks, quantization tables and coefficients, and copies with a
  few bytes changed. Four `cargo-fuzz` targets (`decode_with`, `read_info`, `read_quantized` and
  an encode-decode round trip) live in `fuzz/`, outside the published package, and run in CI
  through ClusterFuzzLite (see `fuzz/README.md`).
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
