"""Records hash-only references from the NIST reference implementation for
large images (see README.md, "Large images").

Usage: python3 make_large_references.py NBIS [--huge]

NBIS is a build of `harness.c` against NBIS (see README.md). For each image
of the table, the script generates the pixels (the same as `pattern` in
tests/nbis.rs), has NBIS encode them at each bit rate and decode the stream,
and prints one Rust table row per image: stream length, FNV-1a hash of the
stream and FNV-1a hash of NBIS's decode. Nothing is written to the
repository; the images and streams live in a temporary directory.

--huge adds the 5500x8000 images, which take a few minutes in this script
(pure Python generates and hashes 44 million pixels).
"""

import os
import subprocess
import sys
import tempfile

from make_fixtures import fnv1a, pattern

# kind, width, height, bit rates. Keep in sync with LARGE and HUGE in tests/nbis.rs.
LARGE = [
    ("ridges", 800, 750, (0.75, 2.25)),
    ("ridges", 1600, 1500, (0.75, 2.25)),
    ("rings", 800, 750, (0.75, 2.25)),
    ("noise", 1600, 1500, (0.75, 2.25)),
]
HUGE = [
    ("dark_ridges", 5500, 8000, (0.75, 2.25)),
]


def main(nbis, huge):
    with tempfile.TemporaryDirectory() as tmp:
        raw = os.path.join(tmp, "in.raw")
        wsq = os.path.join(tmp, "out.wsq")
        out = os.path.join(tmp, "out.raw")
        for kind, w, h, rates in LARGE + (HUGE if huge else []):
            with open(raw, "wb") as f:
                f.write(pattern(kind, w, h, w * h))
            for rate in rates:
                subprocess.run([nbis, "e", raw, str(w), str(h), str(rate), "500", wsq], check=True)
                subprocess.run([nbis, "d", wsq, out], check=True, capture_output=True)
                with open(wsq, "rb") as f:
                    stream = f.read()
                with open(out, "rb") as f:
                    decoded = fnv1a(f.read())
                print(
                    f'{kind} {w}x{h} {rate}: len {len(stream)}, '
                    f'stream 0x{fnv1a(stream):016x}, decoded 0x{decoded:016x}',
                    flush=True,
                )


if __name__ == "__main__":
    main(sys.argv[1], "--huge" in sys.argv[2:])
