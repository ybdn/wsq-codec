# FBI certification of a WSQ implementation

This note records what certifying an encoder or decoder involves, so the maintainer can decide
whether to pursue it. It is not a claim that this crate is certified: it is not.

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
- The certification images and NIST's reference files are not in this repository and must be
  requested from NIST. Never commit them.

## Running the tests locally

The `wsq` example carries a kit that produces a submission and checks it against NIST's
references before anything is sent. NIST's images and reference files stay outside the
repository.

```sh
# Encoder tests: every .pgm under nist/images, at 0.75 and 2.25 bits per pixel,
# into out/0.75/ and out/2.25/; also writes out/submission.txt, the platform
# description to complete by hand. The compiler is never guessed: --compiler
# records what you pass, which must come from the toolchain that built the
# binary under test (the `rustc` on the PATH may be another one).
cargo run --release --example wsq -- cert encode nist/images out --software N \
  --compiler "$(rustc -vV)"

# Decoder tests: every .wsq under nist/streams, into binary PGM (P5).
cargo run --release --example wsq -- cert decode nist/streams out/decoded

# Comparison with NIST's reference streams and reconstructions, at the same
# relative paths.
cargo run --release --example wsq -- cert compare out nist/reference-streams
cargo run --release --example wsq -- cert compare out/decoded nist/reference-images
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
specification itself. They are in `examples/wsq/accuracy.rs`, where each limit cites its clause
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
questions for NIST, to settle with the answers to the certification questions (issue #17)
before a submission; no threshold depends on them, only what is counted:

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

The same values are available to other tools through `wsq_codec::inspect::read_quantized`.

## Certified and optimized releases

The plan is to certify a frozen, conformant encoder and decoder while the rest of the crate
keeps improving:

- a conformant profile, the current encoder, whose output stays aligned with the reference
  implementation and, since it computes its own logarithms and powers, does not depend on the
  platform; only it may write the implementation number the FBI assigns;
- an optimized profile, free to depart from the reference as long as every WSQ decoder reads
  its output, and never certified;
- the conformant code in its own crate, versioned with the certificate, so that releases of the
  optimized encoder, bindings and tools leave the certified code untouched.

## Open points

- **Implementation number.** It is assigned through the certification process; there is nothing to
  pick in advance. Until then, keep `software = 0`.
- **Per-configuration scope.** The certificate still names one configuration, but the encoder's
  output no longer depends on the platform's math library: logarithms and powers are computed
  in the crate and correctly rounded (`src/math.rs`), and the rest is IEEE 754 arithmetic that
  rounds alike everywhere. Every platform should therefore produce the same files, which `cert
  compare` can confirm between platforms before each submission.
- **Images below 81 pixels, and the flat-image widening** (README, "Differences from NBIS") depart
  from NBIS. Check that no certification image triggers them.
- **Certificate and releases.** Decided: the project ships certified releases (profile
  `Conformant`) and optimized ones (profile `Optimized`), as described above. The certificate
  is tied to a version, so it is pursued once the API is frozen for 1.0; the order of the work
  is in the roadmap, issue #17 of the repository.
