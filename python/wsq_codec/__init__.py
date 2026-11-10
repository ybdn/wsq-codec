"""Decoder and encoder for WSQ (Wavelet Scalar Quantization) fingerprint images.

Python bindings for the Rust crate ``wsq-codec``: the conformant encoder writes byte for byte
what NIST's NBIS writes, and the decoder returns NBIS's pixels.
"""

from ._wsq_codec import (
    DEFAULT_MAX_PIXELS,
    MAX_DIMENSION,
    MIN_ENCODE_DIMENSION,
    DecodeError,
    EncodeError,
    Image,
    Info,
    WsqError,
    decode,
    encode,
    read_info,
)

__all__ = [
    "DEFAULT_MAX_PIXELS",
    "MAX_DIMENSION",
    "MIN_ENCODE_DIMENSION",
    "DecodeError",
    "EncodeError",
    "Image",
    "Info",
    "WsqError",
    "decode",
    "encode",
    "read_info",
]
