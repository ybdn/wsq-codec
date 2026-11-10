# Changelog of wsq-certified

Every version of this crate is a version to certify (see README.md). The changes of `wsq-codec`
itself are in the `CHANGELOG.md` at the repository root.

## 1.0.0

First release, split out of `wsq-codec` 1.0.0 (#46): the decoder, the conformant encoder and
`inspect`, with the same behaviour and the same API as in `wsq-codec` 1.0.0 (decoded pixels and
encoder bytes unchanged, checked by `tests/nbis.rs` and the benchmark stream hashes). Differences
in the crate itself:

- `EncodeOptions` here has no `profile` field: this encoder is always the conformant one.
  `wsq-codec` keeps its own `EncodeOptions` with the `Profile` for its users.
- `VERSION`, the version of this crate, for tools that name it (`wsq cert` writes it in
  `submission.txt`).
- The hidden module `for_wsq_codec`, the seam `wsq-codec` builds its optimized profile on (not
  API, not certified behaviour).
