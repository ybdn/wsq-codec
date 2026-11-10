# wsq-codec for Python

Decoder and encoder for WSQ (Wavelet Scalar Quantization) fingerprint images, the format of
FBI and ANSI/NIST-ITL fingerprint transactions. These are the Python bindings of the Rust crate
[`wsq-codec`](https://github.com/ybdn-ei/wsq-codec): the conformant encoder writes byte for
byte what NIST's NBIS writes, and the decoder returns NBIS's pixels. No system library is
needed.

```sh
pip install wsq-codec
```

The wheels (Linux, macOS, Windows) are built for CPython 3.9 and later (one `abi3` wheel per
platform) and depend on NumPy.

```python
import numpy as np
import wsq_codec

# 8-bit grayscale pixels, shape (height, width), each side from 81 to 65535.
pixels = np.full((389, 416), 128, dtype=np.uint8)

wsq = wsq_codec.encode(pixels, bitrate=0.75, ppi=500)   # -> bytes
image = wsq_codec.decode(wsq)                            # -> wsq_codec.Image
assert image.pixels.shape == (389, 416) and image.ppi == 500
```

## API

- `decode(data, *, max_pixels=None, threads=0) -> Image`. `image.pixels` is a `uint8` array of
  shape `(height, width)`; `width`, `height`, `ppi` (`None` if unknown), `software` (the
  implementation number of the frame header) and `comments` (a list of `bytes`) describe the
  stream. `max_pixels` bounds `width * height` (`None`: 100 million); a stream is cheap to
  forge and costly to decode, so raise it only for streams you trust.
- `read_info(data) -> Info`: the same metadata, without decoding the image.
- `encode(pixels, *, bitrate=0.75, ppi=None, comment=None, software=0, profile="conformant",
  threads=0) -> bytes`. `pixels` is a 2-D `uint8` array (any strides; it is copied).
  `profile="conformant"` is the NBIS-aligned encoder; `"optimized"` meets the requested size
  within half a percent and is never certifiable (a non-zero `software` is refused). `comment`
  is `bytes` or `str`.
- `threads=0` uses one thread per core; the output never depends on it. The GIL is released
  while encoding and decoding.

Errors derive from `wsq_codec.WsqError`, a `ValueError`: `DecodeError` for a stream that cannot
be decoded or is over `max_pixels`, `EncodeError` for an image or an option that cannot be
encoded. The message is the Rust error's. Running out of memory raises `MemoryError`; a `pixels`
argument that is not a 2-D `uint8` array raises `TypeError`, an unknown `profile` `ValueError`.
Type stubs are included (`py.typed`).

Never put real fingerprints in bug reports or tests: they are biometric data.

## Licence and source

MIT. Source, issues, the format notes and the certification status are in the
[repository](https://github.com/ybdn-ei/wsq-codec).
