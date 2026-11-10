"""Round-trip and conformance tests of the Python bindings.

The fixtures in ``certified/tests/data`` were written by NIST's NBIS 5.0.0 from synthetic images; the
hashes below are the ones ``tests/nbis.rs`` checks (the FNV-1a of the pixels NBIS decodes).
"""

import threading
from pathlib import Path

import numpy as np
import pytest

import wsq_codec

DATA = Path(__file__).resolve().parents[2] / "certified" / "tests" / "data"

NISTCOM = b"NIST_COM 4\nSEX m\nPPI 1000\nFING_CLASS W"
PLAIN = b"Scanned by the test suite"

# file, kind, width, height, bitrate, ppi, comment, FNV-1a of the decoded pixels
FIXTURES = [
    ("rings_320x288.wsq", "rings", 320, 288, 0.75, 500, None, 0x60850FF0F7C85E04),
    ("noise_257x255.wsq", "noise", 257, 255, 2.25, 1000, None, 0xF646CF9B04F04C45),
    ("checker_101x97.wsq", "checker", 101, 97, 0.75, None, None, 0x33813F51F1975EA8),
    ("rings_200x180_nistcom.wsq", "rings", 200, 180, 0.75, 500, NISTCOM, 0x735E939CB10051D9),
    ("checker_150x140_comment.wsq", "checker", 150, 140, 2.25, None, PLAIN, 0x5524CBD87390DA7B),
]
EVEN = ("rings_300x211_even.wsq", 300, 211, 500, 0xB497AF5EC6F5CF5C)


def fnv1a(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def pattern(kind: str, w: int, h: int, seed: int) -> np.ndarray:
    """The synthetic images of ``pattern`` in tests/nbis.rs, as an (h, w) array."""
    state = seed
    out = bytearray()
    cx, cy = w // 3, h // 2
    for y in range(h):
        for x in range(w):
            state = (state * 1664525 + 1013904223) & 0xFFFFFFFF
            r = state >> 24
            if kind == "rings":
                dx, dy = abs(x - cx), abs(y - cy)
                r2 = dx * dx + dy * dy * 3 // 2 + x * y // 5
                v = 30 + abs((r2 // 37) % 32 - 16) * 12 + (r & 15)
            elif kind == "noise":
                v = r
            else:  # checker
                v = (200 if (x // 7 + y // 5) % 2 else 40) + (r & 7)
            out.append(v)
    return np.frombuffer(bytes(out), dtype=np.uint8).reshape(h, w)


def synthetic(w: int = 200, h: int = 160) -> np.ndarray:
    """A smooth image with some texture, without real data."""
    yy, xx = np.mgrid[0:h, 0:w]
    v = 128 + 90 * np.sin(xx / 7.0) * np.cos(yy / 9.0) + 20 * np.sin((xx + yy) / 3.0)
    return np.clip(v, 0, 255).astype(np.uint8)


@pytest.mark.parametrize("fixture", FIXTURES, ids=lambda f: f[0])
def test_decodes_like_the_reference(fixture):
    name, _, w, h, _, ppi, _, digest = fixture
    image = wsq_codec.decode((DATA / name).read_bytes())
    assert (image.width, image.height, image.ppi) == (w, h, ppi)
    assert image.pixels.shape == (h, w)
    assert image.pixels.dtype == np.uint8
    assert fnv1a(image.pixels.tobytes()) == digest


def test_decodes_even_length_filters():
    name, w, h, ppi, digest = EVEN
    image = wsq_codec.decode((DATA / name).read_bytes())
    assert (image.width, image.height, image.ppi) == (w, h, ppi)
    assert fnv1a(image.pixels.tobytes()) == digest


@pytest.mark.parametrize("fixture", FIXTURES, ids=lambda f: f[0])
def test_conformant_encoder_is_byte_identical_to_the_reference(fixture):
    name, kind, w, h, bitrate, ppi, comment, _ = fixture
    pixels = pattern(kind, w, h, w * h)
    wsq = wsq_codec.encode(pixels, bitrate=bitrate, ppi=ppi, comment=comment)
    assert wsq == (DATA / name).read_bytes()


def test_read_info_matches_decode():
    data = (DATA / "rings_200x180_nistcom.wsq").read_bytes()
    info = wsq_codec.read_info(data)
    image = wsq_codec.decode(data)
    assert (info.width, info.height, info.ppi, info.software) == (200, 180, 500, 0)
    assert info.comments == image.comments
    assert all(isinstance(c, bytes) for c in info.comments)
    assert any(c.startswith(b"NIST_COM") for c in info.comments)
    assert "200" in repr(info) and "200" in repr(image)


def test_round_trip_metadata_and_shape():
    pixels = synthetic(200, 160)
    wsq = wsq_codec.encode(pixels, bitrate=2.25, ppi=500, comment=b"hello", software=0)
    assert isinstance(wsq, bytes)
    image = wsq_codec.decode(wsq)
    assert image.pixels.shape == (160, 200)
    assert (image.width, image.height, image.ppi, image.software) == (200, 160, 500, 0)
    assert b"hello" in image.comments
    # Lossy, but close at 2.25 bits per pixel.
    error = np.abs(image.pixels.astype(int) - pixels.astype(int))
    assert error.mean() < 4.0


def test_software_number_is_kept_by_the_conformant_profile():
    wsq = wsq_codec.encode(synthetic(), software=0x1234)
    assert wsq_codec.read_info(wsq).software == 0x1234


def test_str_comment():
    wsq = wsq_codec.encode(synthetic(), comment="café")
    assert "café".encode() in wsq_codec.decode(wsq).comments


def test_default_ppi_is_unknown():
    assert wsq_codec.decode(wsq_codec.encode(synthetic())).ppi is None


def test_non_contiguous_input_is_encoded_like_its_copy():
    big = synthetic(300, 240)
    views = [
        big[10:170, 20:220],  # a sub-window: rows are not contiguous
        np.ascontiguousarray(big.T)[:, ::-1][:200, :160],
        np.asfortranarray(big[:160, :200]),
    ]
    for view in views:
        assert not view.flags.c_contiguous
        assert wsq_codec.encode(view) == wsq_codec.encode(np.ascontiguousarray(view))


def test_optimized_profile_meets_the_size():
    pixels = pattern("rings", 320, 288, 320 * 288)
    for bitrate in (0.75, 2.25):
        wsq = wsq_codec.encode(pixels, bitrate=bitrate, profile="optimized")
        target = bitrate * 320 * 288 / 8
        assert len(wsq) <= target
        assert len(wsq) >= target * (1 - 0.005 - 0.0005)
        assert wsq_codec.decode(wsq).pixels.shape == (288, 320)


def test_optimized_profile_refuses_a_software_number():
    with pytest.raises(wsq_codec.EncodeError, match="software|implementation"):
        wsq_codec.encode(synthetic(), profile="optimized", software=1)


def test_optimized_round_trip_is_close():
    pixels = synthetic()
    image = wsq_codec.decode(wsq_codec.encode(pixels, bitrate=2.25, profile="optimized"))
    assert np.abs(image.pixels.astype(int) - pixels.astype(int)).mean() < 4.0


def test_threads_do_not_change_the_output():
    pixels = synthetic(300, 260)
    reference = wsq_codec.encode(pixels, threads=1)
    for threads in (0, 2, 4):
        assert wsq_codec.encode(pixels, threads=threads) == reference
        image = wsq_codec.decode(reference, threads=threads)
        assert np.array_equal(image.pixels, wsq_codec.decode(reference, threads=1).pixels)


def test_concurrent_calls_from_python_threads():
    pixels = synthetic(250, 220)
    reference = wsq_codec.encode(pixels)
    results = []

    def work():
        results.append(wsq_codec.decode(wsq_codec.encode(pixels)).pixels.tobytes())
        results.append(wsq_codec.encode(pixels))

    workers = [threading.Thread(target=work) for _ in range(4)]
    for w in workers:
        w.start()
    for w in workers:
        w.join()
    assert len(results) == 8
    assert results.count(reference) == 4


def test_accepts_bytes_like_input():
    wsq = wsq_codec.encode(synthetic())
    expected = wsq_codec.decode(wsq).pixels
    assert np.array_equal(wsq_codec.decode(bytearray(wsq)).pixels, expected)
    assert np.array_equal(wsq_codec.decode(memoryview(wsq)).pixels, expected)


def test_decoded_pixels_are_writable_and_owned():
    image = wsq_codec.decode(wsq_codec.encode(synthetic()))
    pixels = image.pixels
    pixels[0, 0] = 7
    assert image.pixels[0, 0] == 7
    assert pixels.flags.writeable


# Errors.


def test_exception_hierarchy():
    assert issubclass(wsq_codec.WsqError, ValueError)
    assert issubclass(wsq_codec.DecodeError, wsq_codec.WsqError)
    assert issubclass(wsq_codec.EncodeError, wsq_codec.WsqError)


@pytest.mark.parametrize("func", [wsq_codec.decode, wsq_codec.read_info])
def test_garbage_is_a_decode_error(func):
    for data in (b"", b"not a wsq stream at all", bytes(1000)):
        with pytest.raises(wsq_codec.DecodeError) as excinfo:
            func(data)
        assert str(excinfo.value)
        assert isinstance(excinfo.value, ValueError)


def test_truncated_stream_is_a_decode_error():
    wsq = (DATA / "rings_320x288.wsq").read_bytes()
    with pytest.raises(wsq_codec.DecodeError):
        wsq_codec.decode(wsq[: len(wsq) // 2])


def test_max_pixels():
    wsq = (DATA / "rings_320x288.wsq").read_bytes()
    with pytest.raises(wsq_codec.DecodeError, match="pixels"):
        wsq_codec.decode(wsq, max_pixels=1000)
    assert wsq_codec.decode(wsq, max_pixels=320 * 288).pixels.shape == (288, 320)
    assert wsq_codec.decode(wsq, max_pixels=None).pixels.shape == (288, 320)
    assert wsq_codec.DEFAULT_MAX_PIXELS == 100_000_000


def test_wrong_argument_types():
    wsq = wsq_codec.encode(synthetic())
    with pytest.raises(TypeError):
        wsq_codec.decode("text")
    with pytest.raises(TypeError):
        wsq_codec.decode(wsq, 5)  # keyword-only options
    with pytest.raises(TypeError):
        wsq_codec.encode(synthetic(), 0.75)  # keyword-only options


def test_encode_rejects_a_small_image():
    assert wsq_codec.MIN_ENCODE_DIMENSION == 81
    with pytest.raises(wsq_codec.EncodeError, match="80"):
        wsq_codec.encode(np.zeros((80, 200), dtype=np.uint8))
    with pytest.raises(wsq_codec.EncodeError):
        wsq_codec.encode(np.zeros((200, 80), dtype=np.uint8))
    wsq_codec.encode(np.zeros((81, 81), dtype=np.uint8))


@pytest.mark.parametrize("bitrate", [0.0, -1.0, float("nan"), float("inf")])
def test_encode_rejects_a_bad_bitrate(bitrate):
    with pytest.raises(wsq_codec.EncodeError):
        wsq_codec.encode(synthetic(), bitrate=bitrate)


def test_encode_rejects_a_zero_ppi():
    with pytest.raises(wsq_codec.EncodeError):
        wsq_codec.encode(synthetic(), ppi=0)


def test_encode_rejects_a_long_comment():
    with pytest.raises(wsq_codec.EncodeError):
        wsq_codec.encode(synthetic(), comment=bytes(70000))


@pytest.mark.parametrize(
    "bad",
    [
        np.zeros((100, 100), dtype=np.float32),
        np.zeros((100, 100), dtype=np.uint16),
        np.zeros((100, 100, 3), dtype=np.uint8),
        np.zeros(10000, dtype=np.uint8),
        [[0] * 100] * 100,
        b"\0" * 10000,
        None,
    ],
    ids=["float32", "uint16", "3-D", "1-D", "list", "bytes", "None"],
)
def test_encode_rejects_what_is_not_a_2d_uint8_array(bad):
    with pytest.raises(TypeError, match="uint8"):
        wsq_codec.encode(bad)


def test_encode_rejects_an_unknown_profile():
    with pytest.raises(ValueError, match="profile"):
        wsq_codec.encode(synthetic(), profile="fast")


def test_encode_rejects_a_comment_that_is_not_text():
    with pytest.raises(TypeError, match="comment"):
        wsq_codec.encode(synthetic(), comment=5)
