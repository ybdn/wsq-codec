# Public API review for 1.0

Issue #45. The certified crate and the bindings (Python, C, WASM) build on the public API, so
changing it afterwards would mean a new certificate and a change in every binding. This is the
record of what was reviewed and decided before 1.0.0. The changes against 0.2.0 are in
`CHANGELOG.md` under 1.0.0.

## What semantic versioning covers

Covered (a change needs a major release):

- every public item of the crate and of the `image` feature: names, signatures, the variants and
  public fields of the types, trait implementations;
- the decoded pixels for a given stream, and the bytes of the `Conformant` encoder for a given
  image and options (they are what the certification compares);
- the promises of `Profile::Optimized` about size (never longer than the target while a stream
  fits, at most `Profile::OPTIMIZED_TOLERANCE` below it) and that it is deterministic.

Not covered (may change in a minor or patch release):

- the exact bytes of `Optimized` streams;
- the text of `Display` for errors, and the `&'static str` reasons in `InvalidSegment`,
  `MissingTable` and `CorruptData`;
- the output of `Debug`;
- speed, memory use (the documented peak is an estimate) and the number of threads started;
- anything private, and the examples and benchmarks.

MSRV (1.85): raised in a minor release at most, never in a patch release, and noted in the
changelog. CI tests it. The `image` feature is tied to `image` 0.25: a new major version of
`image` would change the types of the public API of that feature, so it comes as a new feature
(for instance `image-0_26`) in a minor release, while `image` keeps meaning 0.25, or as a major
release of this crate if the old one is dropped.

`cargo-semver-checks` runs in CI (job `semver`) against the latest crates.io release. Cargo.toml
holds 1.0.0, so against 0.2.0 it expects (and allows) breaking changes; once 1.0.0 is
published, a pull request that breaks the API fails unless it also raises the major version. The
release itself (tag) is the maintainer's.

## Review items

### `inspect`

`inspect::read_quantized`, `Quantized` and `Subband` stay public and become part of the frozen
API: the certification tooling and tools that compare two encoders need the bin widths and bin
indices, and `cli` uses them. Both structs are `#[non_exhaustive]` (fields can be added), `Clone`,
`Debug` and `PartialEq` (no `Eq` or `Hash`: they hold `f32`), `Send + Sync`. Field names are kept
(`m_shift`, `r_scale`, `bin_center`, `q_bin`, `z_bin`, `coefficients`, `x`, `y`, `width`,
`height`), which follow the specification's symbols. `Quantized::info` reuses `Info`, which
stays at the crate root. No `Default`: an empty `Quantized` means nothing. **Decision: keep.**

### `Image` and `Info` metadata

`Image` repeats the fields of `Info` (`width`, `height`, `ppi`, `software`, `comments`) and adds
`pixels`. Nesting an `info: Info` field would remove the duplication but break every caller of
`image.width` and read worse; a flat `Image` is what `decode` users expect. Both are
`#[non_exhaustive]`, so a field (bit depth, for instance) can be added. `Image` gets a note that
the fields mirror `Info`'s. Added `Hash` to both. `ppi: Option<u32>` and `software: u16` stay.
`comments: Vec<Vec<u8>>` stays (comments are bytes, not necessarily text). **Decision: keep the
layout, add `Hash`.**

### Dimensions as `usize`

The format limits both to 65535, and the encoder to at least 81. `usize` is kept: it matches
slice lengths and the `width * height` arithmetic (which fits `usize` even on 32-bit targets:
65535 x 65535 < 2^32), and the 0.2.0 API. Bindings narrow to `u16` after a successful call.
New public constants `MAX_DIMENSION` (65535) and `MIN_ENCODE_DIMENSION` (81) give the limits
a name; the crate documentation has a "Dimensions" section; `Image`, `Info`, `DecodeError::
InvalidDimensions`, `EncodeError::InvalidDimensions`, `encode` and the `image` integration
point to them. `tests/api.rs` checks the constants against the behaviour. **Decision: keep
`usize`, document, add constants (additive).**

### Profiles (#44)

`Profile` (`Conformant` default, `Optimized`) is `#[non_exhaustive]`, `Copy`, `Eq`, `Hash`,
`Default`. `EncodeOptions::profile` and `with_profile` are kept, as is the associated constant
`Profile::OPTIMIZED_TOLERANCE`. A target-quality profile could join later without a break.
`EncodeOptions` gains no knob for a size in bytes: `bitrate` already says it.

The hidden `experimental` module (`encode_at_rate`, `RATE_TOLERANCE`) is **removed**: it was
declared outside semver, was a prototype that `Profile::Optimized` replaced, and only two tests
and the internal test of `src/encode.rs` used it (rewritten with the profile; benchmarks
already used the profile). It is the one removal and is listed as breaking. **Decision:
remove.**

### Errors: one type or several

Four types are kept: `DecodeError`, `EncodeError`, `ReadError`, `WriteError`.

- `decode`, `read_info`, `encode`, `Decoder` and `Encoder` work on memory and can fail in no
  I/O way. A single error enum would make every caller of a pure function handle `Io`, and
  every binding map variants that cannot occur. It would also mix decode causes with encode
  causes (`SoftwareRequiresConformant` has no meaning when decoding).
- `DecodeError` and `EncodeError` are therefore plain data: `Clone`, `PartialEq` (tests compare
  them), `DecodeError` also `Eq` and `Hash`. `EncodeError` cannot be `Eq` (it holds `f32`).
- `ReadError` and `WriteError` exist only for the two functions that touch a reader or writer
  (`decode_from`, `encode_to`); they wrap the plain error beside `io::Error`, which is neither
  `Clone` nor `PartialEq`, so they are neither either.

All four are `#[non_exhaustive]`, `Send + Sync + 'static` (checked in `tests/api.rs`) and
implement `std::error::Error` and `From` for their parts.

`source()`: 0.2.0 both displayed the wrapped error and returned it as the source, so a reporter
that walks the chain printed it twice. Now `Io` displays "cannot read WSQ data" / "cannot write
WSQ data" and returns the `io::Error` as its source, and `Decode` / `Encode` are transparent
(same text as the inner error, `source()` delegated to it, so none). `DecodeError` and
`EncodeError` are leaves (no source). The CLI prints the chain. **Breaking (message texts,
`source()` of the wrappers); tested.**

Variants: the struct variants (`InvalidSegment`, `InvalidDimensions`, `TooLarge`, `OutputSize`,
`PixelCountMismatch`) become `#[non_exhaustive]`. A `#[non_exhaustive]` enum does not protect its
variants' fields, and a byte offset in `InvalidSegment` or `CorruptData` is a plausible
addition. Cost: patterns need `..` and code outside the crate cannot build these variants (only
useful for comparing in tests; `matches!` does it). Tuple variants (`UnexpectedMarker(u16)`,
`UndefinedHuffmanTable(u8)`, `InvalidBitrate(f32)`, `BitrateTooLow(f32)`, `CommentTooLong(usize)`,
`SoftwareRequiresConformant(u16)`, `MissingTable`, `CorruptData`) hold the one value that
identifies the error and are not marked: more context would be a new variant. **Breaking;
justified because it is the one moment to do it.**

`OutOfMemory` (in both `DecodeError` and `EncodeError`) and `SoftwareRequiresConformant`
(`EncodeError`) are kept as variants: bindings map the first to an out-of-memory status without
parsing text, and the second is the enforcement of the certification rule that only a conformant
stream carries an implementation number. **Decision: keep.**

### `#[must_use]` and `# Errors`

Every public function and method that returns a `Result` has an `# Errors` section, now enforced
by `#![warn(clippy::missing_errors_doc, clippy::missing_panics_doc)]` (CI runs clippy with
`-D warnings`). `Result` is already `#[must_use]`. Added `#[must_use]` to `Decoder::new`,
`Encoder::new`, the new `options()` accessors and `WsqDecoder::info`; the `with_*` builders
already had it. `image::register_hooks` returns a `bool` that callers may ignore, so it is not
`#[must_use]` (documented). `missing_panics_doc` finds no public function that documents or needs a `# Panics` section.

### `image` integration: limits and hook options

Kept as in 0.2.0: `TryFrom<Image> for GrayImage`, `encode_gray_image`, `WsqDecoder::{new,
with_options, info}` and its `ImageDecoder` implementation (`set_limits` checks dimensions and
reserves 8 bytes per pixel of the decoder's peak from `max_alloc`), `register_hooks` and
`register_hooks_with(DecodeOptions)`. The hooks are global to the process, registered once, the
first registration wins: this is how `image::hooks` works and is documented. `WsqDecoder::new`
documented what it really does: the `max_pixels` limit is checked when the image is read (the
test in `tests/image.rs` relies on it), not on creation. `WsqDecoder` implements `Debug` (header
and options, not the stream). Not added: a `TryFrom<&Image>`, a way to unregister hooks (not
possible in `image`), `Limits` in `DecodeOptions` (the `image` crate passes its own through
`set_limits`). The `image` 0.25 dependency is part of the feature's public API; see the policy
above. **Decision: keep, document.**

### General guidelines (Rust API guidelines)

- **C-COMMON-TRAITS.** `Image`, `Info`, `DecodeOptions`: `Clone, Debug, PartialEq, Eq, Hash`
  (`Default` on `DecodeOptions`; an empty `Image` means nothing). `EncodeOptions`: `Clone, Debug,
  PartialEq, Default` (`f32` forbids `Eq` and `Hash`). `Profile`: `Clone, Copy, Debug, Default,
  PartialEq, Eq, Hash`. `Decoder` and `Encoder`: `Debug` and, new, `Default` (default options)
  and an `options()` accessor; they are not `Clone` (they own working buffers). `Quantized` and
  `Subband`: `Clone, Debug, PartialEq`.
- **C-SEND-SYNC.** Every public type is `Send + Sync` (compile-time check in `tests/api.rs`,
  `WsqDecoder` in `tests/image.rs`).
- **C-STRUCT-PRIVATE / evolution.** Public fields exist only on `#[non_exhaustive]` structs,
  built by `Default` and `with_*`, and on `non_exhaustive` enums. The two buffer-owning structs
  (`Decoder`, `Encoder`, `WsqDecoder`) have private fields.
- **Naming.** `with_*` builders, `read_*` for header readers, `decode_into`/`decode_from`/
  `encode_to` for the I/O forms, constants in upper case. Kept. `Info` is short but unambiguous
  at the crate root; renaming (`StreamInfo`) would break for no gain.
- **`threads: usize` with 0 meaning one per core.** An enum would be clearer but the convention
  is documented, and it matches the CLI and the bindings' integer arguments. Kept.
- **`with_comment(Vec<u8>)`.** Kept (taking `impl Into<Vec<u8>>` would be a compatible change
  later).

## Left for later (additive, not blocking 1.0)

- `From<ReadError> for io::Error` and `From<WriteError> for io::Error`.
- `Display` / `FromStr` for `Profile` (the CLI parses it itself).
- A streaming decoder or encoder (the wavelet transform needs the whole image).
