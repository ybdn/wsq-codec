#!/usr/bin/env python3
"""Generates the demo's synthetic samples: a made-up ridge pattern, NOT a fingerprint.

    python3 make_sample.py OUT_DIR
    cargo run --release -p wsq-cli -- encode OUT_DIR/sample.pgm sample.wsq 0.75 500

writes OUT_DIR/sample.pgm (to give to `wsq encode`) and OUT_DIR/sample.png. The committed
`sample.wsq` is `wsq encode sample.pgm sample.wsq 0.75 500` of the PGM this script writes.
Only the standard library is used, and the output is the same on every run.
"""
import math
import struct
import sys
import zlib

W, H = 288, 320
PERIOD = 9.0  # ridge period in pixels, as at 500 ppi


def pixel(x, y, seed):
    # A whorl-like flow field: ridges follow circles around a centre that drifts a little,
    # with a slow angular warp so the lines bend like skin ridges do.
    cx, cy = W * 0.5 + 10 * math.sin(y / 60.0), H * 0.52
    dx, dy = x - cx, (y - cy) * 0.9
    r = math.hypot(dx, dy)
    theta = math.atan2(dy, dx)
    phase = r / PERIOD + 0.9 * math.sin(2 * theta) + 0.35 * math.sin(r / 23.0)
    ridge = 0.5 + 0.5 * math.cos(2 * math.pi * phase)
    # Fade to a plain background outside an elliptical "finger" and add a little noise.
    e = math.hypot((x - W * 0.5) / (W * 0.46), (y - H * 0.5) / (H * 0.48))
    mask = min(1.0, max(0.0, (1.0 - e) / 0.12))
    seed[0] = (seed[0] * 1103515245 + 12345) & 0x7FFFFFFF
    noise = ((seed[0] >> 16) % 17) - 8
    value = 235 - mask * (150 * ridge + 10) + noise * 0.6 * mask
    return max(0, min(255, int(round(value))))


def main(out):
    seed = [20240607]
    rows = [bytes(pixel(x, y, seed) for x in range(W)) for y in range(H)]

    with open(f"{out}/sample.pgm", "wb") as f:
        f.write(f"P5\n{W} {H}\n255\n".encode())
        f.write(b"".join(rows))

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    raw = b"".join(b"\x00" + row for row in rows)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 0, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(f"{out}/sample.png", "wb") as f:
        f.write(png)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
