from typing import Literal

import numpy as np
import numpy.typing as npt

MAX_DIMENSION: int
"""Largest width or height the format can describe (65535)."""
MIN_ENCODE_DIMENSION: int
"""Smallest width or height the encoder accepts (81)."""
DEFAULT_MAX_PIXELS: int
"""Default limit of `decode` on `width * height` (100 million)."""

class WsqError(ValueError):
    """Base class of the errors of this package."""

class DecodeError(WsqError):
    """The data is not a WSQ stream the decoder accepts, or the image is over the limit."""

class EncodeError(WsqError):
    """The image or an option cannot be encoded."""

class Info:
    """What a WSQ stream says about its image."""

    @property
    def width(self) -> int: ...
    @property
    def height(self) -> int: ...
    @property
    def ppi(self) -> int | None:
        """Scanning resolution, or None when the stream does not give it."""
    @property
    def software(self) -> int:
        """Implementation number in the frame header (0: unassigned)."""
    @property
    def comments(self) -> list[bytes]:
        """Comment segments, as bytes (a comment need not be text)."""

class Image:
    """A decoded image."""

    @property
    def pixels(self) -> npt.NDArray[np.uint8]:
        """The pixels, shape ``(height, width)``, dtype ``uint8``; 0 is black."""
    @property
    def width(self) -> int: ...
    @property
    def height(self) -> int: ...
    @property
    def ppi(self) -> int | None: ...
    @property
    def software(self) -> int: ...
    @property
    def comments(self) -> list[bytes]: ...

def decode(
    data: bytes | bytearray | memoryview,
    *,
    max_pixels: int | None = None,
    threads: int = 0,
) -> Image:
    """Decode a WSQ stream.

    ``max_pixels`` is the largest ``width * height`` accepted; None means
    ``DEFAULT_MAX_PIXELS``. ``threads`` is the number of threads to use; 0 means one per core.
    Raises ``DecodeError`` (a ``ValueError``), or ``MemoryError``.
    """

def read_info(data: bytes | bytearray | memoryview) -> Info:
    """Read the dimensions, resolution and comments without decoding the image.

    Raises ``DecodeError`` (a ``ValueError``).
    """

def encode(
    pixels: npt.NDArray[np.uint8],
    *,
    bitrate: float = 0.75,
    ppi: int | None = None,
    comment: bytes | str | None = None,
    software: int = 0,
    profile: Literal["conformant", "optimized"] = "conformant",
    threads: int = 0,
) -> bytes:
    """Encode a 2-D ``uint8`` array of shape ``(height, width)``, each side 81 to 65535.

    ``bitrate`` is the target in bits per pixel. ``profile="conformant"`` writes what NBIS
    writes; ``"optimized"`` meets the requested size more closely and cannot carry a non-zero
    ``software``. Raises ``EncodeError`` (a ``ValueError``), ``TypeError`` for an array that is
    not 2-D ``uint8``, ``ValueError`` for an unknown profile, or ``MemoryError``.
    """
