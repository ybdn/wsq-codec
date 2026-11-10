#!/usr/bin/env python3
"""Summarizes the output of run.sh as Markdown tables (docs/nbis-comparison.md).

    summarize.py results.csv            distribution tables (min, median, p90, max)
    summarize.py results.csv --compact  the compact tables of the 4 November 2026 page
    summarize.py results.csv --ci       bootstrap intervals, thread scaling, memory (version 3)
    summarize.py calls.csv checks.csv --corpus
                                        aggregates over a local corpus (WSQ_CORPUS_DIR mode):
                                        counts, ratios and intervals, never a file name

Standard library only. The bootstrap uses a fixed seed (SEED) and RESAMPLES resamples, so the
same input always gives the same intervals.
"""

import csv
import math
import random
import sys
from collections import defaultdict

SEED = 68  # the issue number; printed with the tables
RESAMPLES = 10000
LEVEL = 0.95


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


def interval(values):
    """The 95 % percentile interval of a list of bootstrap statistics (nearest rank)."""
    v = sorted(x for x in values if x == x)
    lo = v[max(0, math.floor((1 - LEVEL) / 2 * len(v)))]
    hi = v[min(len(v) - 1, math.ceil((1 + LEVEL) / 2 * len(v)) - 1)]
    return lo, hi


def resamples(n, rng):
    """RESAMPLES lists of n indices drawn with replacement, shared by every statistic of a group."""
    return [[rng.randrange(n) for _ in range(n)] for _ in range(RESAMPLES)]


def verdict(lo, hi):
    """Reads a ratio t_reference / t_other (above 1: the other took less time)."""
    if lo > 1:
        return "faster"
    if hi < 1:
        return "slower"
    return "not established"


def impl_order(impl, threads):
    return (impl != "nbis", impl, int(threads))


def short(impl, threads):
    return label(impl, threads).replace("wsq-codec (this checkout)", "wsq-codec")


def point_ci(point, boots, digits):
    lo, hi = interval(boots)
    return f"{point:.{digits}f} [{lo:.{digits}f}, {hi:.{digits}f}]"


def ratio_ci(num, den, digits=2):
    """(median, bootstrap medians) of a numerator series over a denominator series."""
    return point_ci(num[0] / den[0], [a / b for a, b in zip(num[1], den[1])], digits)


def table(head, rows):
    print("| " + " | ".join(head) + " |")
    print("|" + "---|" * len(head))
    for row in rows:
        print("| " + " | ".join(row) + " |")
    print()


def ci_tables(path):
    """Median and 95 % percentile bootstrap interval per series, ratios, thread scaling, memory.

    The unit of resampling is the round: a round holds one call of every implementation, close in
    time, so a resample draws the same rounds for all of them (a paired bootstrap, which keeps
    the drift of the machine between rounds common to the series that are compared).
    """
    cols = defaultdict(lambda: defaultdict(dict))  # (op, image, rate) -> (impl, threads) -> round -> row
    with open(path, newline="") as f:
        for row in csv.DictReader(f):
            g = (row["op"], row["image"], row["rate"])
            cols[g][(row["impl"], row["threads"])][int(row["round"])] = (
                float(row["ms"]), int(row["extra_rss_kb"]) / 1024)
    print(f"Percentile bootstrap of the median, {RESAMPLES} resamples of the rounds, {LEVEL:.0%} interval, "
          f"seed {SEED}.\n")
    rng = random.Random(SEED)
    stats = {}  # group -> (n, keys, {("t"|"m", key): (median, bootstrap medians)})
    for g in sorted(cols, key=lambda g: (g[0], g[1], float(g[2]))):
        keys = sorted(cols[g], key=lambda k: impl_order(*k))
        rounds = sorted(set.intersection(*(set(cols[g][k]) for k in keys)))
        idx = resamples(len(rounds), rng)
        res = {}
        for which, name in ((0, "t"), (1, "m")):
            for k in keys:
                col = [cols[g][k][r][which] for r in rounds]
                res[(name, k)] = (median(col), [median([col[i] for i in s]) for s in idx])
        stats[g] = (len(rounds), keys, res)

    nbis = ("nbis", "0")
    one = ("current", "1")
    for op in ("encode", "decode"):
        groups = [g for g in stats if g[0] == op]
        keys = stats[groups[0]][1]
        others = [k for k in keys if k[0] != "nbis"]
        scaled = [k for k in others if k[0] == "current" and k[1] != "1"]
        print(f"### {op}: median time in ms [95 % interval]\n")
        table(["image", "bpp", "n"] + [short(*k) for k in keys],
              [[g[1], g[2], str(stats[g][0])]
               + [point_ci(stats[g][2][("t", k)][0], stats[g][2][("t", k)][1], 1) for k in keys]
               for g in groups])
        print(f"### {op}: ratio of medians, wsq-codec over NBIS (below 1: the crate took less time) [95 % interval]\n")
        table(["image", "bpp"] + [short(*k) for k in others],
              [[g[1], g[2]] + [ratio_ci(stats[g][2][("t", k)], stats[g][2][("t", nbis)]) for k in others]
               for g in groups])
        print(f"### {op}: thread scaling, median at 1 thread over median at k threads "
              "(above 1: k threads took less time) [95 % interval], verdict\n")
        table(["image", "bpp"] + [f"{k[1]} threads" for k in scaled],
              [[g[1], g[2]] + [
                  ratio_ci(stats[g][2][("t", one)], stats[g][2][("t", k)]) + " "
                  + verdict(*interval([a / b for a, b in zip(stats[g][2][("t", one)][1], stats[g][2][("t", k)][1])]))
                  for k in scaled] for g in groups])
    rate0 = min({g[2] for g in stats}, key=float)
    print(f"### Extra peak resident memory of the call, MiB [95 % interval] and ratio to NBIS ({rate0} bpp)\n")
    print("Same reading for every implementation: VmHWM minus VmRSS just before the call, from "
          "`/proc/self/status`.\n")
    for op in ("encode", "decode"):
        groups = [g for g in stats if g[0] == op and g[2] == rate0]
        keys = stats[groups[0]][1]
        others = [k for k in keys if k[0] != "nbis"]
        table(["image", "operation"] + [short(*k) for k in keys] + [f"{short(*k)} / NBIS" for k in others],
              [[g[1], op]
               + [point_ci(stats[g][2][("m", k)][0], stats[g][2][("m", k)][1], 1) for k in keys]
               + [ratio_ci(stats[g][2][("m", k)], stats[g][2][("m", nbis)]) for k in others]
               for g in groups])


def corpus_tables(calls_path, checks_path):
    """Aggregates over a corpus. The unit is the file, known only by an index; no name is read."""
    per = defaultdict(lambda: defaultdict(list))  # (op, impl, threads) -> file -> [(ms, MiB)]
    pixels = {}
    with open(calls_path, newline="") as f:
        for row in csv.DictReader(f):
            per[(row["op"], row["impl"], row["threads"])][int(row["file"])].append(
                (float(row["ms"]), int(row["extra_rss_kb"]) / 1024))
            if row["op"] == "decode":
                pixels[int(row["file"])] = int(row["out_bytes"])
    checks = defaultdict(lambda: [0, 0])
    with open(checks_path, newline="") as f:
        for row in csv.DictReader(f):
            checks[row["check"]][0] += 1
            checks[row["check"]][1] += row["ok"] == "1"
    rng = random.Random(SEED)
    print(f"Percentile bootstrap, {RESAMPLES} resamples of the files, {LEVEL:.0%} interval, seed {SEED}. "
          "Per file, the median of its repeated calls; the statistic is then taken over the files.\n")
    if pixels:
        mp = sorted(pixels.values())
        print(f"Images decoded: {len(mp)}, {sum(mp) / 1e6:.1f} Mpixel in all, median {median(mp) / 1e6:.2f} Mpixel.\n")
    print("### Identity with NBIS\n")
    table(["check", "checks", "identical"], [[n, str(c[0]), str(c[1])] for n, c in sorted(checks.items())])

    def per_file(k, which):
        return {f: median([v[which] for v in vals]) for f, vals in per[k].items()}

    for op in ("encode", "decode"):
        ks = sorted((k for k in per if k[0] == op), key=lambda k: impl_order(k[1], k[2]))
        if ("nbis" not in {k[1] for k in ks}) or not any(k[1] == "current" for k in ks):
            continue
        files = sorted(set.intersection(*(set(per[k]) for k in ks)))
        idx = resamples(len(files), rng)
        nb_t, nb_m = per_file((op, "nbis", "0"), 0), per_file((op, "nbis", "0"), 1)
        t1 = per_file((op, "current", "1"), 0)
        print(f"### {op}: {len(files)} files\n")
        rows, scaling = [], []
        for k in ks:
            if k[1] != "current":
                continue
            t, m = per_file(k, 0), per_file(k, 1)
            r = [t[f] / nb_t[f] for f in files]
            rm = [m[f] / nb_m[f] if nb_m[f] else float("nan") for f in files]
            med = lambda xs, s: median([xs[i] for i in s if xs[i] == xs[i]] or [float("nan")])
            tot = [sum(t[files[i]] for i in s) / sum(nb_t[files[i]] for i in s) for s in idx]
            rows.append([k[2], point_ci(median(r), [med(r, s) for s in idx], 2),
                         point_ci(sum(t[f] for f in files) / sum(nb_t[f] for f in files), tot, 2),
                         f"{sum(x < 1 for x in r)} of {len(files)}",
                         point_ci(med(rm, range(len(rm))), [med(rm, s) for s in idx], 2)])
            if k[2] != "1":
                sp = [t1[f] / t[f] for f in files]
                boots = [median([sp[i] for i in s]) for s in idx]
                lo, hi = interval(boots)
                scaling.append([k[2], f"{median(sp):.2f} [{lo:.2f}, {hi:.2f}]", verdict(lo, hi),
                                f"{sum(x > 1 for x in sp)} of {len(files)}"])
        table(["wsq-codec threads", "time: median over files of the ratio to NBIS [95 %]",
               "time: total over total of NBIS [95 %]", "files where the crate took less time",
               "extra memory: median of the ratio to NBIS [95 %]"], rows)
        print("Median extra peak memory of the call, MiB: " + ", ".join(
            f"{short(k[1], k[2])} {median(list(per_file(k, 1).values())):.1f}" for k in ks) + ".\n")
        print(f"Thread scaling, {op}: per file, time at 1 thread over time at k threads.\n")
        table(["threads", "median over files [95 %]", "verdict", "files faster than at 1 thread"], scaling)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[2] == "--ci":
        ci_tables(sys.argv[1])
    elif len(sys.argv) == 4 and sys.argv[3] == "--corpus":
        corpus_tables(sys.argv[1], sys.argv[2])
    elif len(sys.argv) == 3 and sys.argv[2] == "--compact":
        compact(sys.argv[1])
    else:
        main(sys.argv[1])
