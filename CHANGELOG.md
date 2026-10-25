# Changelog

All notable changes to this crate are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate
follows [Semantic Versioning](https://semver.org/).

## Unreleased

First release.

### Decoding

- `decode`, `decode_with` and `read_info` for WSQ streams, giving the same
  pixels as NIST's NBIS, with the standard 9/7 filter bank and NBIS's
  alternative 8/8 one.
- `DecodeOptions::max_pixels` refuses oversized images before allocating
  anything for them (100 million pixels by default); corrupt streams fail
  with a `DecodeError`, never a panic.
- Resolution, encoder implementation number and comments from the stream.

### Encoding

- `encode` with `EncodeOptions` (bit rate, resolution, comments, FBI
  implementation number), giving the same bytes as NBIS except where NBIS's
  own output is broken (images under 81 pixels wide or high, almost
  featureless images).
- The NISTCOM comment NBIS writes, merged with a caller's NISTCOM comment.

### Integration

- `decode_from` and `encode_to` for `io::Read` and `io::Write`, with
  `ReadError` and `WriteError`.
- `image` feature, off by default: conversion to `image::GrayImage`,
  `image::encode_gray_image`, and a `WsqDecoder` that `image::register_hooks`
  lets the `image` crate use for `.wsq` files.
- `threads` in `EncodeOptions` and `DecodeOptions` splits the wavelet
  transform over threads without changing the output.

### Tools

- The `wsq` example converts between PGM and WSQ, and checks the decoder on
  a corpus, streams inside ANSI/NIST-ITL transactions included; the `bench`
  example times encoding and decoding.
