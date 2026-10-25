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

## Open points

- **Implementation number.** It is assigned through the certification process; there is nothing to
  pick in advance. Until then, keep `software = 0`.
- **Per-configuration scope.** Byte-identical encoding depends on the platform's `pow` and `log`
  (see the README). Each OS/compiler to be certified needs its own run, and a toolchain update
  could in principle require a new one.
- **Images below 81 pixels, and the flat-image widening** (README, "Differences from NBIS") depart
  from NBIS. Check that no certification image triggers them.
- **Decision (maintainer).** Whether the crate's users need the certificate. Without it the crate
  remains usable for decoding, and for encoding where no certified encoder is required. Pursue it
  only once the API is frozen (issue #10), since the certificate is tied to a version.
