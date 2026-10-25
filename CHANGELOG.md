# Changelog

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
