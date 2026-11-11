# FBI certification of a WSQ implementation

This note records what certifying an encoder or decoder involves, so the maintainer can decide
whether to pursue it. It is not a claim that this crate is certified: it is not.

**This plan may change.** NIST has not yet answered the maintainer's questions about the
certification (issue #34). The technical groundwork below (the `wsq-certified` crate, the pinned
compiler, the submission procedure) was done in advance and is written to be easy to adjust to
the answers: which platforms, which configuration, whether a vendor-supplied tool such as the
`wsq` CLI is part of what is certified, and what the submission must contain. The NIST
submission itself is the maintainer's and is not part of this repository's work.

## Procedure

Source: NIST, [WSQ certification procedure](https://www.nist.gov/programs-projects/wsq-certification-procedure),
and the specification itself,
[IAFIS-IC-0110 v3.1](https://fbibiospecs.fbi.gov/file-repository/wsq_gray-scale_specification_version_3_1_final.pdf).

**Encoder tests.** 40 images, each compressed at two bit allocation targets, 2.25 bits per pixel
(about 5:1) and 0.75 (about 15:1): 80 tests. The output is compared with NIST's reference on file
size, header parameter values and quantized wavelet bin indices, within the accuracy requirements
of the v3.1 specification. The encoder must write `Ev = 2` and its own implementation number in
`Sf` in the frame header.

**Decoder tests.** The same 40 images, compressed by NIST at both rates, plus 6 images compressed
with non-9/7 filter tap sets at 0.75: 86 tests. Each reconstruction is compared with NIST's.

**Submission.** The vendor runs the tests, on every platform to certify, and returns the output
files (`.wsq`, and `.pgm` in binary P5 form) as an archive whose URL is emailed to NIST's WSQ
certification liaison, or on discs. A plain ASCII text file gives the vendor's contact, the
implementation number, the platform (hardware, OS, compiler) and an internal version identifier.
NIST forwards the report to the FBI.

**Outcome.** The FBI may ask for more information or run supplemental tests. If the results are
satisfactory, the FBI CJIS Division issues a certification letter, and the implementation number
and description join the FBI's list of approved implementations. NIST's page mentions no fee.

Certification covers one configuration: software version, hardware, OS and compiler. Any change
requires recertification.

## Where this crate stands

- The encoder already writes `Ev = 2` and takes the implementation number from
  `EncodeOptions::software` (0 by default, meaning unassigned).
- Its output is byte-identical to NBIS 5.0.0 on the fixtures and on the comparison run in the
  test suite, and NBIS is the reference the tests were derived from. That is the strongest basis
  one could ask for, but it is not a substitute for running the official test set.
- The code a certificate would cover is the `wsq-certified` crate (`certified/`), built with the
  compiler pinned in `certified/rust-toolchain.toml`; see "The certified crate" below.
- The certification images and NIST's reference files are not in this repository and must be
  requested from NIST. Never commit them.

## Running the tests locally

The `wsq` example carries a kit that produces a submission and checks it against NIST's
references before anything is sent. NIST's images and reference files stay outside the
repository. The kit runs `wsq-certified` directly. For a trial run, any build will do; for a
submission, build it from `certified/` with the pinned compiler and follow "Submission
procedure" below.

```sh
# Encoder tests: every .pgm under nist/images, at 0.75 and 2.25 bits per pixel,
# into out/0.75/ and out/2.25/; also writes out/submission.txt, the platform
# description to complete by hand. The compiler is never guessed: --compiler
# records what you pass, which must come from the toolchain that built the
# binary under test (the `rustc` on the PATH may be another one).
wsq cert encode nist/images out --software N \
  --compiler "$(rustc -vV)"

# Decoder tests: every .wsq under nist/streams, into binary PGM (P5).
wsq cert decode nist/streams out/decoded

# Comparison with NIST's reference streams and reconstructions, at the same
# relative paths.
wsq cert compare out nist/reference-streams
wsq cert compare out/decoded nist/reference-images
```

For each stream, `compare` reports the file size difference, the header parameters (`M`, `R`),
the largest relative difference of the bin widths (`Q`, `Z`), the subbands one side drops and
the other keeps, and how many bin indices differ and by how much. For each reconstruction, it
reports how many pixels differ and by how much. Then, after each file's numbers, it prints
`PASS` or `FAIL` with the failed requirements and their clause, and ends with a summary
(`verdict (IAFIS-IC-0110 v3.1, Annex AA): streams 79/80 pass; ...`). The numbers stay as they
were; the verdict is the accuracy requirements below. The exit status is 0 when every file
passes, 2 when a reference file has no readable counterpart (it gets no verdict), and 3 when a
requirement fails (3 wins when both happen). A `PASS` is a local pre-check against the
specification's text, not NIST's verdict and not a certificate. The file names and directory
layout NIST expects come with the test data, and the output may need renaming to match them.

## Accuracy requirements

Source: IAFIS-IC-0110 v3.1 (October 4, 2010), Part 2, "Compliance Tests", Annex AA,
"Procedures for Determining Compliance" (printed page 37, page 45 of the PDF), read from the
specification itself. They are in `cli/src/accuracy.rs`, where each limit cites its clause
and has unit tests just inside and just outside it.

| clause | applies to | requirement |
|---|---|---|
| AA.2 item 1 | encoder | the compressed file size, excluding comments, is within 0.4% of the reference's |
| AA.2 item 2 | encoder | every quantization bin width `Q` and zero bin width `Z`, for the subbands `0 <= k <= 59`, is within 0.051% of the reference's |
| AA.2 item 3 | encoder | at least 99.99% of the bin indices are the reference's, and none differs from it by more than 1 |
| AA.3 item 1 | decoder | at least 99.9% of the reconstructed values are the reference's, and none differs from it by more than 1 |

The limits are compared on integers (parts per million) wherever the quantity is a count, so
that a value exactly on a limit (1 pixel in 1000 at 99.9%) passes, as "at least" says.

The specification leaves some points open. `cert compare` reads them as below, and they are
questions for NIST, to settle with the answers to the certification questions (issues #17
and #34) before a submission; no threshold depends on them, only what is counted:

1. **Absolute value.** The formulas of AA.2 items 1 and 2 are printed without absolute-value
   bars, while the text says "within". They are read as the absolute relative difference.
2. **"Excluding comments".** Taken as the whole COM segments: the marker, the length and the
   payload. The specification does not define the term.
3. **Population of the bin indices.** Taken as the coefficients of all 60 subbands, so a subband
   a stream drops (`Q = 0`) counts its coefficients as zeros; a subband dropped on one side only
   compares the other side's nonzero indices with zero.
4. **A subband dropped on one side only.** Its width is zero against a width that is not, which
   is within no relative limit: the bin-width requirement fails. A subband both streams drop has
   nothing to compare.
5. **"Reconstructed values".** Taken as the 8-bit pixels of the `.pgm` files the submission
   holds, not the floating-point values before rounding.

Not checked, because Annex AA gives no accuracy limit for it: the syntax tests of the
interchange and abbreviated formats (Part 2, section 3), `Ev = 2` and the implementation number
(Part 3).

The same values are available to other tools through `wsq_certified::inspect::read_quantized`
(re-exported as `wsq_codec::inspect::read_quantized`).

## The certified crate

A certificate covers one version of the code and one configuration, so the code to certify lives
in a crate of its own, `wsq-certified` (`certified/`), versioned with the certificate. Releases
of `wsq-codec`, of the optimized encoder, of the bindings and of the tools then leave it
untouched. (Issue #46.)

**What is in it.** The decoder, the conformant encoder, and `inspect` (the header parameters and
bin indices the certification tools compare), with `math.rs` that computes logarithms and powers
without libm or FMA. It is `#![forbid(unsafe_code)]` and has no dependency, not even optional:
the `image` integration stays in `wsq-codec`, because it is not part of what is certified.

**What is not.** The `Optimized` profile (below), the `image` feature, the bindings and the CLI
as a whole. The `wsq cert` commands of the CLI are the test kit; they call `wsq-certified`
directly and `submission.txt` names its version. Whether the kit itself needs to be part of a
certificate is a question for NIST (#34).

**How `wsq-codec` uses it.** It depends on `wsq-certified = "=1.0.0"` (an exact version) and
re-exports the public API of 1.0.0 from it, so that users of `wsq-codec` see the same items at
the same paths (`wsq_codec::decode`, `wsq_codec::DecodeError`, `wsq_codec::inspect`, ...). Three
things are not plain re-exports:

- `wsq_codec::EncodeOptions` and `Encoder` are `wsq-codec`'s own types: they carry the
  `Profile`, which the certified crate does not know. `wsq_certified::EncodeOptions` has no
  `profile` field, because its encoder is always the conformant one. `wsq-codec` converts one to
  the other (same bit rate, resolution, comments, implementation number and threads).
- `wsq_codec::Image` and `Decoder` are `wsq-codec`'s own types too, wrapping the certified ones
  (the fields are moved, not copied). Rust's orphan rule only lets the crate that owns `Image`
  implement `TryFrom<Image> for GrayImage` for the `image` feature, and the certified crate must
  not depend on `image`.
- `EncodeError` is shared and keeps `SoftwareRequiresConformant`, which only `wsq-codec` ever
  returns (the certified encoder is always conformant): removing a variant from the public
  `wsq-codec` API would break it.

**The seam for the optimized profile.** `Profile::Optimized` scales the bin widths of the bit
allocation by one factor, chosen by a rate search, and so needs the conformant pipeline's
internals: the transformed image with its variances, the bit allocation, the quantizer and the
stream writer. Options considered:

1. *The whole encoder in `wsq-certified`, with a scale-factor parameter that is 1.0 in the
   conformant path.* The certified entry point would carry a parameter that has no meaning for
   the certificate, and the search would have to live there too.
2. *Copy the pipeline into `wsq-codec`.* Two copies of the transform, the quantizer and the
   Huffman writer, one of which the certificate covers; a divergence would go unseen.
3. *A narrow, hidden hook in `wsq-certified`.* Chosen. The module `for_wsq_codec`
   (`#[doc(hidden)]`, not API, not covered by semver) exposes `with_scaler`, which analyzes an
   image exactly as `encode` does and hands a `Scaler` to a closure, whose `Scaler::write(factor)`
   writes the stream for bin widths multiplied by `factor`; `encode` on buffers the caller keeps
   (`Scratch`, for `wsq-codec`'s `Encoder`); and the crate's own `ln` and `pow`, so that the search
   is as platform independent as the encoder. No tree, quantizer or table type leaves the crate.
   The rate search itself (`src/optimized.rs` of `wsq-codec`) only decides which factor to write.

Option 3 keeps the certified surface minimal and the conformant output byte-identical: the
public `encode` of the certified crate never calls `Scaler`, and `Scaler::write(1.0)` is the
conformant stream byte for byte (a unit test of `certified/src/encode.rs` checks it, and the
benchmark stream hashes pass through `wsq-codec`). The `Scaler` code is compiled into the
certified crate and is part of what is hashed and versioned with it, but no conformant call
reaches it. If NIST asks for a certified crate with nothing but the conformant path, the hook
moves behind a feature or out of the crate and `wsq-codec` takes a new exact version; that is a
decision for after #34.

**The version.** `wsq-certified` starts at 1.0.0, the version of `wsq-codec` it was split from:
the API it exposes is the one frozen for 1.0 (`docs/api-1.0.md`), and the decoded pixels and the
encoder's bytes are what semantic versioning covers, so the number can name a certified
behaviour. A `0.x` number would suggest an unstable API, which is not the case. It is a number
that can be changed before anything is published. From then on, any change under `certified/`
(code, tests, manifest, toolchain; not Markdown files) is a new version to certify, even a
patch: CI (`certified-version`) fails a pull request that changes it without raising the version
and adding a section to `certified/CHANGELOG.md`.

**The compiler.** The certificate names the compiler, so `certified/rust-toolchain.toml` pins it
(1.97.0, `profile = "minimal"`, with `clippy` and `rustfmt`). Options considered:

1. *A `rust-toolchain.toml` at the repository root.* Rejected. rustup applies it to every command
   in the repository and overrides the toolchain that CI sets up for `cargo` invocations without
   `+toolchain`: the MSRV job (1.85) and the stable jobs would silently build with the pinned
   version instead, so the MSRV would no longer be tested. It would also pin every contributor's
   compiler.
2. *A file in `certified/` only.* Chosen. rustup reads it from the working directory upward, so
   commands run in `certified/` use the pinned compiler (and `cargo` in a workspace member
   directory builds that member), while the rest of the repository keeps using whatever the CI
   jobs install. `--manifest-path` does not change the toolchain, only the working directory
   does: the kit for a submission must be built from `certified/`
   (`cd certified && cargo build --release -p wsq-cli`).
3. *Pinning in CI only (`dtolnay/rust-toolchain@1.97.0`).* Not enough alone: the file is what a
   maintainer or NIST's reviewers see, and what a local build uses.

The `certified` job of CI installs the toolchain from the file (`rustup toolchain install`),
fails if `rustc --version` differs from the pin, prints `rustc -vV`, and builds, lints and tests
`wsq-certified` and the kit with it, and runs the benchmark stream hashes. Releases of the
compiler do not change the pin; raise it, together with a new certification, on purpose. The
workspace has no dependency besides the optional `image` (not built there), so there is no lock
file to pin for the certified build.

**Publishing.** `release.yml` publishes `wsq-certified` first, only if its version is not on
crates.io yet (it changes with a certification, not with every release), then `wsq-codec`. The
maintainer configures Trusted Publishing for the new crate; see `CONTRIBUTING.md`.

**Checking the API.** `cargo-semver-checks` cannot see through a re-export from another crate: run
on `wsq-codec` it reports the items re-exported from `wsq-certified` as removed. Verified for the
split: against `65519d0` (`wsq-codec` 1.0.0 as one crate), every item `wsq-codec` defines passes,
and run on `wsq-certified` against that same baseline renamed, the only reports are the items
that are `wsq-codec`'s alone (`Profile`, `EncodeOptions::profile` and `with_profile`, the `image`
feature and module). CI checks `wsq-codec` as before, and `wsq-certified` too once it is on
crates.io. The tests of `wsq-codec` (`tests/api.rs`, `codec.rs`, `io.rs`, `image*.rs`) are
unchanged and compile and pass against the split.

## Optimized releases

The other half of the plan, unchanged:

- the optimized profile (`Profile::Optimized`), free to depart from the reference as long as
  every WSQ decoder reads its output, and never certified: it meets the requested size by
  scaling the bin widths, so its streams fail the bin-width and size requirements of Annex AA
  by design. The encoder enforces the rule: a non-zero `EncodeOptions::software` (field `Sf` of
  the frame header) with `Optimized` is refused with `EncodeError::SoftwareRequiresConformant`,
  so a stream that carries a certified implementation number is always a conformant one. The
  `wsq cert encode` kit does not know the profile at all: it runs the certified encoder.

## Submission procedure

Before each submission, on every platform to certify (hardware, OS and compiler as NIST's
instructions define a platform; to settle with #34). NIST's images and reference files stay
outside the repository.

1. **Fix the version.** `certified/Cargo.toml` holds the version to certify, `CHANGELOG.md` has its
   section, the tree is clean, and the commit is tagged (the tag and commit go in the "internal
   version" line of `submission.txt`). `wsq-codec` and `wsq-cli` pin that exact version.
2. **Build the kit with the pinned compiler, on each platform.**
   ```sh
   git checkout <tag>
   cd certified
   rustup toolchain install          # the version in rust-toolchain.toml
   rustc -vV                         # must name the pinned version; keep the output
   cargo build --release -p wsq-cli  # target/release/wsq, at the workspace root
   cargo test --release                            # the tests, NBIS identity included
   cargo test --release --test nbis -- --ignored   # the same on 5500x8000 images (slow)
   ```
3. **Run the tests.** From the same directory, so that `rustc -vV` is the compiler of the binary:
   ```sh
   ../target/release/wsq cert encode nist/images out --software N --compiler "$(rustc -vV)"
   ../target/release/wsq cert decode nist/streams out/decoded
   ../target/release/wsq cert compare out nist/reference-streams
   ../target/release/wsq cert compare out/decoded nist/reference-images
   ```
   Every file must pass (exit status 0). Complete `out/submission.txt` (contact, hardware, OS
   version, commit); it already names `wsq-certified`'s version.
4. **Compare the platforms with each other.** Pick one platform as the reference and copy its
   `out/` next to the others'. For every other platform, compare its output with the
   reference's, streams and reconstructions together (`compare` reads every `.wsq` and `.pgm`
   under the reference directory, `out/decoded` included):
   ```sh
   wsq cert compare out-platform-b out-reference
   ```
   The encoder and the decoder are deterministic (see "Platform independence"), so the summary
   must read `streams: N/N identical; reconstructions: M/M identical`, and the exit status
   must be 0. A difference between two platforms is not a rounding detail: stop, do not
   submit, and find it (it would contradict the statement below, and the certificate names one
   configuration).
5. **Submit** as NIST's instructions say (the maintainer's step): the archive with the `.wsq`
   and `.pgm` files, and `submission.txt` for each platform.
6. **After the certificate**: record the version, the compiler and the platforms here and in the
   README, and do not touch `certified/` again without planning a recertification.

## Open points

- **Implementation number.** It is assigned through the certification process; there is nothing to
  pick in advance. Until then, keep `software = 0`.
- **Per-configuration scope.** The certificate still names one configuration. The encoder's
  output does not depend on the platform within the scope stated in the next section, which
  `cert compare` can confirm between platforms before each submission.
- **Images below 81 pixels, and the flat-image widening** (README, "Differences from NBIS") depart
  from NBIS. Check that no certification image triggers them.
- **Certificate and releases.** Decided: the project ships certified releases (the
  `wsq-certified` crate, which `wsq-codec` re-exports) and optimized ones (profile `Optimized`,
  in `wsq-codec`), as described above. The certificate is tied to a version, so it is pursued
  once the API is frozen for 1.0 (#45, decisions in `docs/api-1.0.md`) and the certified code
  is split out (#46): the output of the conformant encoder and the decoded pixels are part of
  what semantic versioning covers, so a certified version can be named by its number and any
  change to them is a major release. The order of the work is in the roadmap, issue #17.

## Platform independence

The statement, and what supports it (issue #19).

**Statement.** On targets whose `f64` and `f32` arithmetic follows IEEE 754 binary64 and
binary32, the encoder's output is deterministic: the same image and options give the same bytes,
whatever the operating system, the C library, the CPU model or the number of threads. It is not
"correctly rounded": the logarithms and powers of the bit allocation, computed in `certified/src/math.rs`
from addition, subtraction, multiplication and division alone, are correctly rounded except when
the exact result lies within about 2^-100 of the midpoint between two `f64`s, and the rest of the
encoder is basic arithmetic in a fixed order, without fused multiply-add (Rust does not fuse
unless asked).

**Scope.** Rust's usual targets. Targets that compute `f64` on the x87 unit, such as
`i586-unknown-linux-gnu`, round intermediate results differently and are excluded; so is any
build that enables fast-math or fused operations by other means.

**Verified.** The benchmark's three integer-generated images (800x750, 1600x1500, 5500x8000)
give the same stream hashes, `benches/hashes.txt`, on the three GitHub-hosted runners, with `cargo
bench --bench codec -- 1 1 --check` (one thread) and `-- 0 1 --check` (one thread per core):

| run | commit | runners | result |
|---|---|---|---|
| [41882431956](https://github.com/ybdn/wsq-codec/actions/runs/41882431956), PR #51 | `fa82067` | ubuntu-latest, macos-latest, windows-latest | both `--check` steps pass; the logs show `5ce22c9ec2bea4ee`, `9a25b2791f2f4f27` and `a3949f99443cd4ba` on each |
| [41886526469](https://github.com/ybdn/wsq-codec/actions/runs/41886526469), `develop` | `15cd780` | same | both `--check` steps pass; the logs read (Windows in full, Linux and macOS for the larger images) show the same hashes |
| [41889317306](https://github.com/ybdn/wsq-codec/actions/runs/41889317306), `develop` | `34fbc49` | same, plus a 32-bit x86 job (`i686-unknown-linux-gnu`, release tests) | all 8 jobs succeed; the job logs were not read |

Those runs also pass `tests/nbis.rs` (now `certified/tests/nbis.rs`), the byte-for-byte comparison with NBIS 5.0.0 on the
fixtures, on the three operating systems. The hashes of the three runners are equal to each
other and to the ones recorded on Linux x86-64; the macOS runner's `/opt/homebrew` paths in
its log suggest Apple silicon (AArch64), which would make it a second architecture, but the
logs do not state it, and neither do they state the Windows runner's architecture.

**Verified, other targets (issue #65).** The tests, `certified/tests/nbis.rs` included, and the
benchmark's stream hashes were also executed on:

| target | how | date | result |
|---|---|---|---|
| `aarch64-unknown-linux-gnu` | CI job `aarch64`, `ubuntu-24.04-arm`, `cargo test --release --lib --tests -p wsq-codec -p wsq-certified` and both `bench --check` | 10 November 2026 | pass: run [42881844032](https://github.com/ybdn/wsq-codec/actions/runs/42881844032), commit `effbbc5`, runner label `ubuntu-24.04-arm`; the three hashes of `benches/hashes.txt` with 1 and 0 threads |
| `wasm32-wasip1` | CI job `wasi`, wasmtime 50.0.1, same commands. Run by hand on 10 November 2026 (wasmtime 50.0.1, Rust 1.97.0, x86-64 host): all test binaries pass, and both `bench --check` print the three hashes of `benches/hashes.txt` | 10 November 2026 (by hand and in CI) | pass; CI run [42881844032](https://github.com/ybdn/wsq-codec/actions/runs/42881844032), commit `effbbc5` |
| `s390x-unknown-linux-gnu` (big-endian) | CI job `s390x` of `scheduled.yml` (`cross`, QEMU), weekly and on demand: tests only. Run by hand on 10 November 2026 without `cross`, with `qemu-s390x` 8.2 user emulation: tests pass, and `bench --check` (one thread) prints the three hashes | 10 November 2026 (by hand and in CI) | pass; tests in CI run [42880561801](https://github.com/ybdn/wsq-codec/actions/runs/42880561801) (`cross`, commit `93533b8`) |

WASI has no threads: `certified/src/par.rs` falls back to the calling thread, so the `threads` option
has no effect there, and the output is the same. Every job prints `uname -m` and `rustc -vV`
at the start, so the architecture is in the log.

**Not verified.** RISC-V and other big-endian targets than `s390x`; the architecture of the
macOS and Windows runners (the logs do not state it); a compiler other than the stable toolchain
and 1.85 of the matrix. Run `cert compare` between two platforms' outputs on NIST's images
before each submission, as planned above.
