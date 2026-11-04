# Security policy

`wsq-codec` parses WSQ streams that usually come from other parties, so a malformed stream
that panics, hangs, exhausts memory or reads out of bounds is a vulnerability.

## Supported versions

Only the latest 0.x release receives fixes.

## Reporting a vulnerability

Do not open a public issue. Report it privately, in either way:

- through GitHub's private vulnerability reporting: the "Report a vulnerability" button on the
  [Security tab](https://github.com/ybdn/wsq-codec/security/advisories/new) of the repository;
- or by email to **security@ybdn.io**.

Please include:

- the version of `wsq-codec` (or the commit) and the Rust toolchain;
- what happens (panic message, memory use, running time) and what you expected;
- the smallest input that triggers it, or the steps to build one.

**Never send real fingerprints.** They are biometric data, a special category of personal data
under GDPR article 9. Build the input synthetically, or describe the stream with the output of
`cargo run --release --example wsq -- info FILE`, which prints the header values and no pixel.

We aim to acknowledge a report within a few days and to publish a fix and an advisory once a
release is ready. Reporters are credited unless they prefer not to be.
