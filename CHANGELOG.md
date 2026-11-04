# Changelog

## Unreleased

- `wsq cert encode` no longer runs the `rustc` found on the `PATH` for `submission.txt`: the
  compiler is the one given with `--compiler "$(rustc -vV)"`, or a line to fill in.
- CI also runs on pushes to `develop`. Releases are published by a workflow triggered by a `v*`
  tag on `main`, through crates.io Trusted Publishing; the steps are in `CONTRIBUTING.md`.
- Public-crate showcase: `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, issue and pull
  request templates, README badges and a notice (not affiliated with the FBI or NIST, not
  certified), `homepage` in the package metadata and an `include` list in place of `exclude`.
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
