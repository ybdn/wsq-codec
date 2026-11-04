# Changelog

## Unreleased

- With several threads, dequantization and the encoder's subband variances run in parallel; the
  largest-magnitude scan that bounds the bin widths shares the variance pass. Output unchanged.
- The wavelet transform starts its threads once per image instead of once per filtering pass,
  and splits column passes by output rows: several threads no longer slow small images down.
- With several threads, the decoder finds the Huffman blocks by their markers and decodes them
  side by side, each block's coefficients growing fallibly. A stream that fails on that path is
  read again on one thread, so errors are unchanged.
- `decode_into` decodes into a buffer the caller provides, and `WsqDecoder::read_image` uses it
  instead of copying the pixels. `Decoder` and `Encoder` keep their working buffers from one
  image to the next (the peak memory of `decode` and `encode` is unchanged; a `Decoder` keeps
  8 bytes per pixel of its largest image). New `DecodeError::OutputSize` for a buffer of the
  wrong size.
- A comparison with NBIS 5.0.0 for speed and memory: `bench/nbis-compare` (outside the published
  package and the workspace, with its own lock files) times NBIS's `wsq_encode_mem` and
  `wsq_decode_mem` and this crate in separate processes, one call each, interleaved, with
  distributions (minimum, median, 90th percentile, maximum), peak resident memory, one and four
  threads, and optionally the crates.io release next to the checkout. It first checks that streams
  and pixels are identical to NBIS's in both directions (96 checks) and records the load, the
  revision and the NBIS archive's SHA-256. `docs/nbis-comparison.md` has the protocol, the first
  results of 4 November 2026 and their limits (synthetic images, one machine with uncontrolled
  load, builds named), the prior-art search, and the wording the figures allow.
- A quality benchmark, `cargo bench --bench quality` (`benches/quality.rs`): it encodes
  integer-generated images at 0.5, 0.75, 1.0 and 2.25 bits per pixel (`--rates` changes them) and
  reports, per image and rate, the file size, the actual bit rate against the requested one, the
  PSNR and SSIM of the reconstruction and the encoding and decoding time, as a table and as CSV
  (`--csv`). `--nbis BIN_DIR` also runs NBIS's `cwsq` and `dwsq` on the same images and fails if
  a stream or a reconstruction differs from this crate's; `--nbis-streams DIR` reads streams NBIS
  wrote elsewhere; `WSQ_QUALITY_DIR` adds a local directory of PGM files. CI runs it on the
  synthetic set and judges nothing. `docs/quality.md` has the first table for the conformant
  encoder, dated and with the conditions of measurement: on the 20 measurements (five images,
  four rates) NBIS's streams and reconstructions are identical to this crate's to the byte.
- The `wsq` command line is now an installable crate, `wsq-cli` (directory `cli/`, binary `wsq`),
  in a Cargo workspace with the library: `cargo install wsq-cli`. It has the example's commands
  (`encode`, `decode`, `info`, `check`, `extract`, `cert`), plus `--help` and `--version`, and
  exits with 64 on a usage error. It adds no dependency: argument parsing is by hand, and
  `wsq-codec` still has no dependency for its users. `examples/wsq` is gone; `cargo run --example
  wsq --` becomes `cargo run -p wsq-cli --` or `wsq`. The binary has tests (round trip, exit codes,
  `--help`). `tests/corpus.rs` moved to `cli/tests/corpus.rs`. Prebuilt binaries wait for the
  release workflow (#40). (#47)
- `wsq cert compare` judges each stream and each reconstruction by the accuracy requirements of
  IAFIS-IC-0110 v3.1, Annex AA (file size within 0.4%, bin widths within 0.051%, 99.99% of the
  bin indices identical and none off by more than 1; for decoders, 99.9% of the pixels identical
  and none off by more than 1), prints `PASS` or `FAIL` with the clause next to the numbers it
  already gave, and ends with a summary. It exits with status 3 when a requirement fails (2 stays
  for a reference file without a readable counterpart). The limits are in
  `cli/src/accuracy.rs`, with unit tests on each side of every limit; the points the
  specification leaves open are listed in `docs/certification.md` as questions for NIST.
- The platform-independence claim is worded as determinism, not exact rounding, and scoped to
  targets with IEEE 754 `f64` (not x87-only ones such as `i586`). `docs/certification.md` cites
  the CI runs that show the benchmark's stream hashes on Linux, macOS and Windows, and says what
  has not been verified.
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
