"""Regenerates the WSQ fixtures with the NIST reference implementation.

Usage: python3 make_fixtures.py NBIS NBIS_EVEN

NBIS and NBIS_EVEN are builds of `harness.c` against NBIS, the second one
with -DFILTBANK_EVEN_8X8_1 (see README.md). Prints the FNV-1a hash of each
image the reference decoder produces, for tests/nbis.rs.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))


def pattern(kind, w, h, seed):
    """Same images as `pattern` in tests/nbis.rs."""
    state = seed

    def rand():
        nonlocal state
        state = (state * 1664525 + 1013904223) & 0xFFFFFFFF
        return state >> 24

    cx, cy = w // 3, h // 2
    out = bytearray()
    for y in range(h):
        for x in range(w):
            if kind == "rings":
                dx, dy = abs(x - cx), abs(y - cy)
                r2 = dx * dx + dy * dy * 3 // 2 + x * y // 5
                t = (r2 // 37) % 32
                v = 30 + abs(t - 16) * 12 + (rand() & 15)
            elif kind == "noise":
                v = rand()
            else:
                v = (200 if (x // 7 + y // 5) % 2 else 40) + (rand() & 7)
            out.append(v)
    return bytes(out)


def fnv1a(data):
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


FIXTURES = [
    # name, generator, width, height, bit rate, ppi, even filter bank
    ("rings_320x288.wsq", "rings", 320, 288, 0.75, 500, False),
    ("noise_257x255.wsq", "noise", 257, 255, 2.25, 1000, False),
    ("checker_101x97.wsq", "checker", 101, 97, 0.75, -1, False),
    ("rings_300x211_even.wsq", "rings", 300, 211, 0.75, 500, True),
]


def main(nbis, nbis_even):
    with tempfile.TemporaryDirectory() as tmp:
        raw, out = os.path.join(tmp, "in.raw"), os.path.join(tmp, "out.raw")
        for name, kind, w, h, rate, ppi, even in FIXTURES:
            tool = nbis_even if even else nbis
            with open(raw, "wb") as f:
                f.write(pattern(kind, w, h, w * h))
            wsq = os.path.join(HERE, name)
            subprocess.run([tool, "e", raw, str(w), str(h), str(rate), str(ppi), wsq], check=True)
            subprocess.run([tool, "d", wsq, out], check=True, capture_output=True)
            with open(out, "rb") as f:
                print(f"{name}: 0x{fnv1a(f.read()):016x}")


if __name__ == "__main__":
    main(*sys.argv[1:])
