#!/usr/bin/env python3
"""Summarizes results.csv of run.sh as Markdown tables (docs/nbis-comparison.md)."""

import csv
import math
import sys
from collections import defaultdict


def rank(sorted_values, q):
    """Nearest-rank quantile of an ascending list."""
    return sorted_values[max(0, math.ceil(q * len(sorted_values)) - 1)]


def label(impl, threads):
    if impl == "nbis":
        return "NBIS 5.0.0"
    name = {"current": "wsq-codec (this checkout)", "released": "wsq-codec (crates.io release)"}[impl]
    return f"{name}, {threads} thread" + ("s" if threads != "1" else "")


def median(values):
    s = sorted(values)
    return s[len(s) // 2] if len(s) % 2 else (s[len(s) // 2 - 1] + s[len(s) // 2]) / 2


def compact(path):
    """One row per image and rate: medians (p90) of NBIS and of this checkout's wsq-codec."""
    groups = defaultdict(list)
    rss = defaultdict(list)
    with open(path, newline="") as f:
        for row in csv.DictReader(f):
            key = (row["op"], row["image"], row["rate"], row["impl"], row["threads"])
            groups[key].append(float(row["ms"]))
            rss[key].append(int(row["extra_rss_kb"]))

    def cell(key):
        ms = sorted(groups[key])
        return median(ms), f"{median(ms):.1f} ({rank(ms, 0.9):.1f})"

    for op in ("encode", "decode"):
        print(f"### {op}, median ms (p90)\n")
        print("| image | bpp | NBIS 5.0.0 | wsq-codec, 1 thread | median / NBIS | wsq-codec, 4 threads | median / NBIS | released / current, 1 thread |")
        print("|---|---|---|---|---|---|---|---|")
        keys = sorted({k[:3] for k in groups if k[0] == op}, key=lambda k: (k[1], float(k[2])))
        for k in keys:
            n_med, n_txt = cell(k + ("nbis", "0"))
            p1_med, p1_txt = cell(k + ("current", "1"))
            p4_med, p4_txt = cell(k + ("current", "4"))
            dev = ""
            if k + ("released", "1") in groups:
                dev = f"{cell(k + ('released', '1'))[0] / p1_med:.2f}"
            print(f"| {k[1]} | {k[2]} | {n_txt} | {p1_txt} | {p1_med / n_med:.2f} | {p4_txt} | {p4_med / n_med:.2f} | {dev} |")
        print()
    print("### Extra peak resident memory, median MiB\n")
    print("| image | operation | NBIS 5.0.0 | wsq-codec, 1 thread | wsq-codec, 4 threads |")
    print("|---|---|---|---|---|")
    seen = sorted({(k[1], k[0]) for k in rss})
    for image, op in seen:
        r = lambda impl, th: median(rss[(op, image, "0.75", impl, th)]) / 1024
        print(f"| {image} | {op} (0.75 bpp) | {r('nbis', '0'):.1f} | {r('current', '1'):.1f} | {r('current', '4'):.1f} |")


def main(path):
    groups = defaultdict(list)
    outputs = defaultdict(set)
    with open(path, newline="") as f:
        for row in csv.DictReader(f):
            key = (row["op"], row["image"], row["rate"])
            groups[key + (row["impl"], row["threads"])].append(
                (float(row["ms"]), int(row["extra_rss_kb"]))
            )
            outputs[key].add((row["out_bytes"], row["fnv"]))
    identical = all(len(v) == 1 for v in outputs.values())
    print(f"Output size and hash identical across every timed call and implementation: "
          f"{'yes' if identical else 'NO'}\n")
    for op in ("encode", "decode"):
        print(f"### {op}\n")
        print("| image | bpp | implementation | n | min ms | median ms | p90 ms | max ms | median / NBIS | median extra RSS MiB |")
        print("|---|---|---|---|---|---|---|---|---|---|")
        keys = sorted({k[:3] for k in groups if k[0] == op}, key=lambda k: (k[1], float(k[2])))
        for key in keys:
            base = None
            impls = [k for k in groups if k[:3] == key]
            impls.sort(key=lambda k: (k[3] != "nbis", k[3] != "current", k[3], int(k[4])))
            for k in impls:
                ms = sorted(m for m, _ in groups[k])
                rss = sorted(r for _, r in groups[k])
                med = rank(ms, 0.5) if len(ms) % 2 else (ms[len(ms) // 2 - 1] + ms[len(ms) // 2]) / 2
                if k[3] == "nbis":
                    base = med
                ratio = f"{med / base:.2f}" if base else ""
                rss_med = rss[len(rss) // 2] / 1024
                print(f"| {k[1]} | {k[2]} | {label(k[3], k[4])} | {len(ms)} | {ms[0]:.1f} | {med:.1f} "
                      f"| {rank(ms, 0.9):.1f} | {ms[-1]:.1f} | {ratio} | {rss_med:.1f} |")
        print()


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[2] == "--compact":
        compact(sys.argv[1])
    else:
        main(sys.argv[1])
