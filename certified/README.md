# wsq-certified

The certified part of [`wsq-codec`](https://github.com/ybdn-ei/wsq-codec): a WSQ (Wavelet Scalar
Quantization) fingerprint **decoder**, a **conformant encoder** that reproduces the NIST
reference implementation (NBIS) byte for byte, and the `inspect` tools the certification
compares streams with. Pure Rust, no `unsafe` (`#![forbid(unsafe_code)]`), no dependencies.

> **Not certified yet.** This crate is split out so that it can be. NIST has not answered the
> maintainer's questions about the certification (issue #34), so nothing has been submitted, and
> the plan in [`docs/certification.md`](https://github.com/ybdn-ei/wsq-codec/blob/main/docs/certification.md) may change once it does.

**Most users want [`wsq-codec`](https://crates.io/crates/wsq-codec)**, which depends on this
crate, re-exports all of it (`wsq_codec::decode`, `wsq_codec::encode`, `wsq_codec::inspect`, ...)
and adds what is not certified: the `Optimized` encoding profile and the `image` integration.
Depend on `wsq-certified` directly only to use the certified code alone, as `wsq cert` does.

```rust
let wsq = wsq_certified::encode(&pixels, width, height, &wsq_certified::EncodeOptions::default())?;
let image = wsq_certified::decode(&wsq)?;
```

## One version, one certificate

A certificate covers one version of the code and one configuration, so this crate is versioned
with the certificate:

- **Any change to a file under `certified/` is a new version to certify** (source, tests that
  define the expected output, `rust-toolchain.toml`, `Cargo.toml`), even a patch release or a
  comment, because the certificate names the version and the one it covers must be the one that
  was tested. Do not touch it casually. CI fails a pull request that changes `certified/`
  without raising `version` in `certified/Cargo.toml` (job `certified-version`).
- `wsq-codec` depends on `wsq-certified` at an exact version (`=1.0.0`), so a release of
  `wsq-codec`, of the bindings or of the tools never moves the certified code by itself. Raising
  the pin is a deliberate change, made together with the new certification.
- Semantic versioning applies to the documented API, to the decoded pixels and to the bytes of
  the encoder; the hidden `for_wsq_codec` module is not API (it is the seam described in
  `docs/certification.md`). Each release is listed in [CHANGELOG.md](CHANGELOG.md).
- The version starts at 1.0.0, like `wsq-codec`: the API it exposes is the one frozen for 1.0
  (`docs/api-1.0.md`), and the certificate will name that number.

## The compiler

The certificate names the compiler, so `certified/rust-toolchain.toml` pins it (currently
1.97.0). rustup applies it to every command run in `certified/` or below, whatever `rustc` is the
default. It is deliberately not at the repository root: there it would override the toolchain of
the other CI jobs, among them the one that tests the Minimum Supported Rust Version (1.85).

```sh
cd certified
rustup toolchain install     # installs the pinned toolchain; rustup 1.28 does not do it by itself
rustc -vV                    # must name the pinned version
cargo test --release
```

To build the certification kit with the same compiler, run cargo from this directory too:
`cd certified && cargo build --release -p wsq-cli`. Cargo's `--manifest-path` does not change the
toolchain; only the working directory does.

## Layout

| path | what |
|---|---|
| `src/` | the decoder, the conformant encoder, `inspect`; `math.rs` computes logarithms and powers without libm or FMA, so that the encoder's output does not depend on the platform |
| `tests/nbis.rs`, `tests/data/` | byte-identity with NBIS on fixtures and generated images (`tests/data/README.md` says how they were made) |
| `tests/inspect.rs`, `codec.rs`, `io.rs` | the inspection tools, the codec through its public API, readers and writers |
| `rust-toolchain.toml` | the pinned compiler |
| `CHANGELOG.md` | what changed in each version |

The benchmark stream hashes (`benches/hashes.txt` in the repository, checked in CI on several
platforms and architectures) exercise this crate through `wsq-codec`.
