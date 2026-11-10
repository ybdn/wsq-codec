# Comparison with NBIS: speed and memory

How long this crate takes to encode and decode, and how much memory it needs, next to NBIS 5.0.0's
own `wsq_encode_mem` and `wsq_decode_mem`, on synthetic images. The tool is
[`bench/nbis-compare`](../bench/nbis-compare/); this page holds the protocol, the figures it
printed on the dates below, and what they do not show. They are measurements, not a verdict, and
they claim no superiority beyond the conditions listed here. No real fingerprint is used in any
run recorded here.

The page has had three versions. **Version 3, 10 November 2026 (current)** adds 95 % bootstrap
intervals on the medians, thread scaling at 1, 2, 4 and 8 threads for encoding and decoding, memory
read the same way for NBIS and the crate, a second run to see whether the first repeats, and a mode
for a local corpus of real fingerprints (not run yet: see [Real fingerprints](#real-fingerprints-not-executed)).
**Version 2, 4 November 2026**, measured 31 calls per series with no interval, at 1 and 4 threads;
its results are kept below, labelled as such, and are superseded where the two disagree. Version 1
is `benches/codec.rs` (README), one process, no NBIS.

Anyone can repeat the measurement: the protocol, the harness and the raw results are in this
public repository, and [Reproducing](#reproducing) names the NBIS archive with its SHA-256.

## What is where

| Subject | Where |
|---|---|
| Speed and memory against NBIS, intervals, thread scaling (1, 2, 4, 8) | this page |
| Real fingerprints, locally | [`bench/nbis-compare/README.md`](../bench/nbis-compare/README.md); not executed yet, see below |
| Byte-for-byte identity with NBIS, streams and pixels | [`tests/nbis.rs`](../tests/nbis.rs) and [`tests/data/README.md`](../tests/data/README.md): ten streams up to 5500x8000, built with `-ffp-contract=off`. Checked again on eight of them here, in both directions |
| Quality: actual bit rate, PSNR, SSIM | [`docs/quality.md`](quality.md), `benches/quality.rs`: not repeated here |
| Platform independence, FBI certification | [`docs/certification.md`](certification.md): the crate is not certified |
| Deliberate differences from NBIS | [README](../README.md#differences-from-nbis) |

`benches/codec.rs` (README, "Performance") reports the fastest of five repeated calls in one
process and does not compare with NBIS. The tool described here measures something else, one call
per process and the whole distribution, so the two series do not replace each other.

## Protocol

**Images.** Four 8-bit images generated with integer arithmetic only, so that they are the same
on every platform, by the generator of `tests/nbis.rs` (`pattern`, seed = number of pixels):
`ridges` 800x750 and 1600x1500, `noise` 1600x1500 (white noise, the worst case for compression)
and `dark_ridges` 5500x8000 (a palm at 1000 ppi). `dark_ridges` is dark (mean 75) because NBIS
stops on a bright image of that size: it sums the pixels in 32 bits (`tests/data/README.md`).
These are synthetic patterns, not fingerprints. `wsq-nbis gen` writes them as raw files.

**Parameters.** Requested bit rates 0.75 and 2.25 bits per pixel, 500 ppi declared, no comment
given (both write their NISTCOM comment). Decoding reads the stream NBIS wrote, at the same rate.

**Implementations.**

| Label | Version | Build |
|---|---|---|
| NBIS 5.0.0 | NIST sources (`Rel_5.0.0`), libraries `wsq`, `jpegl`, `fet`, `ioutil`, `util`, called in memory; single-threaded (NBIS has no threads) | gcc 13.3.0, `-O2 -w -ffp-contract=off -fno-fast-math -D__NBISLE__`, no `-march` |
| `wsq-codec`, this checkout | the revision of the repository, `threads` option at 1 and 4 (4 November) or 1, 2, 4 and 8 (10 November) | rustc 1.97.0, Cargo's `release` profile (`opt-level = 3`, no LTO, 16 codegen units), no `target-cpu` |
| `wsq-codec`, release | 0.1.1 from crates.io, pinned by `Cargo.released.lock`, 1 and 4 threads; **measured on 4 November only** | same |

The checkout measured on 4 November was 0.1.1 (revision `1a1a34e`); the one measured on 10 November
is 0.2.0 (revision `c21a942` of `develop`, then `8959180`, which adds only the benchmark tooling).
The codec is not the same code, and a difference between the two dates is not attributable to
the machine alone.

`-ffp-contract=off` is the build this repository uses for its NBIS references: without it gcc may
fuse multiplications and additions and NBIS rounds differently. There is no "fastest possible"
build of NBIS (`-O3`, `-march=native`, profile-guided) in this protocol.

**Measurement.** One call per process, in memory. The process reads its input, notes its
resident set, times the call alone (`CLOCK_MONOTONIC` in C, `Instant` in Rust; reading and writing
files excluded), then notes its peak resident set (`VmHWM`). The memory reported is that peak minus
the resident set before the call, so the input buffer is not counted: it is the cost of the call,
allocations and page faults included. Each call starts from the same state (a fresh process, the
input file in the page cache). NBIS and the crate read the same two numbers from
`/proc/self/status` (`nbis_time.c` and `src/main.rs`), so the memory comparison uses one tool for
both; it does not measure what a whole program using either library would need.

**Repetitions.** Per image, rate, operation and implementation: 31 calls (11 for 5500x8000). The
implementations are **interleaved**: in each round all the calls of all the implementations follow
one another, in an order that rotates from round to round, so that a drift of the machine does not
favour one of them. No call is discarded and there is no warm-up round. The report gives the
minimum, the median, the 90th percentile (nearest rank) and the maximum, never only the best time.
Every run recorded here has 2080 raw measurements, in its `results.csv`.

**Identity, before any timing.** For each image and rate, and for each configuration of the crate
(4 November: this checkout and the release, 1 and 4 threads; 10 November: this checkout, 1, 2, 4 and
8 threads; 96 checks in both cases): (1) the crate's stream is compared byte for byte
with NBIS's; (2) the crate decodes NBIS's stream and the pixels are compared with those NBIS
decodes from its own stream; (3) NBIS decodes the crate's stream and the pixels are compared in the
same way. If one of the 96 checks differs, the script stops without timing anything. During the
timing, the size and FNV-1a hash of every output are recorded and must be the same for all the
implementations.

**Statistics (version 3).** For each series, the median of the timed calls, and its 95 % interval by
the percentile bootstrap: 10,000 resamples of the rounds with replacement, a fixed seed (68, the
issue number), so that the same `results.csv` always gives the same intervals
(`summarize.py --ci`, standard library only). One round holds one call of every implementation, close
in time, so a resample draws **the same rounds for all the implementations** (a paired bootstrap):
the ratio of two medians, and the interval of that ratio, then keep the drift of the machine that
the two series share. Memory and ratios to NBIS get their intervals the same way. The interval of a
median of 11 calls (the 5500x8000 image) is coarse: it can only be bounded by values among the 11
(the extreme ones are the limits), and reads as a range of plausible medians for this machine and
this load, not as a guarantee for another.

**Thread scaling.** The ratio `median(1 thread) / median(k threads)` for the crate, with its
interval. The verdict is "faster" when the whole interval is above 1, "slower" when it is below 1,
"n.e." (not established) when it contains 1. A verdict describes one run on one loaded machine; a
gain is claimed here only where the verdict repeats in the two runs.

**Load.** The machine's load is not controlled. The script records the load average and the CPU
busy and steal shares over 5 seconds **before it starts any work**, and again over the whole
timing phase. The latter includes the benchmark's own processes (one core, or up to four with
threads), so only the former says what else was running.

## Results of 10 November 2026 (version 3)

**Conditions.** Two runs of the same command on the same virtual machine, a cloud sandbox,
recorded in [`results/2026-11-10`](../bench/nbis-compare/results/2026-11-10/) (run 1) and
[`results/2026-11-10-run2`](../bench/nbis-compare/results/2026-11-10-run2/) (run 2):
Intel Xeon at 2.10 GHz, 4 vCPUs, 16 GB, Linux 6.18 x86-64, rustc 1.97.0, gcc 13.3.0, NBIS 5.0.0 from
the archive of the SHA-256 given under Reproducing. **The machine was shared with other jobs, and
the figures are noisy.** Before the script did any work, run 1 saw a load average of 2.59 and the
CPUs 54.0 % busy (steal 0.4 %); over its timing phase they were 75.0 % busy, benchmark included.
Run 2, started about ten minutes later, saw a load average of 6.43 and the CPUs 99.5 % busy (steal 1.0 %)
before it began, and 88.0 % busy over its timing phase: it is the same measurement under heavy
contention, and is kept for that reason. No core was reserved or pinned, and 4 vCPUs mean that the
8-thread series run more threads than cores. The checkout is 0.2.0 (see Protocol); the release
comparison of 4 November was not repeated, and NBIS was measured again in every run. Run 1 was taken at revision `c21a942` with the benchmark changes of this issue
not yet committed (`machine.txt` says "clean" because the script ignores `bench/`); run 2 at
`8959180`, which holds them. The codec (`src/`) is the same.

**Identity with NBIS.** All 96 checks are identical in both runs
([`identity.txt`](../bench/nbis-compare/results/2026-11-10/identity.txt)): streams equal to NBIS's
byte for byte, pixels equal in both directions, for the 8 pairs of image and rate (streams from
56,089 to 12,518,245 bytes), at 1, 2, 4 and 8 threads. The sizes and hashes of all the timed outputs
are also the same for every implementation.

### Speed, one thread

Median in ms with the 95 % interval, 0.75 bpp; the last column is the ratio of the medians with its
interval (below 1: the crate took less time). The tables for 2.25 bpp and for the other thread counts
are in `ci.md` of each run.

**Run 1, encoding**

| image | NBIS 5.0.0 | wsq-codec, 1 thread | wsq-codec / NBIS |
|---|---|---|---|
| `dark_ridges` 5500x8000 | 2774.1 [2673.8, 2930.5] | 1420.9 [1280.9, 1500.6] | 0.51 [0.46, 0.54] |
| `noise` 1600x1500 | 138.2 [118.9, 152.8] | 62.9 [60.0, 71.3] | 0.46 [0.40, 0.58] |
| `ridges` 1600x1500 | 135.2 [122.8, 147.2] | 58.4 [55.1, 59.8] | 0.43 [0.39, 0.47] |
| `ridges` 800x750 | 24.9 [24.0, 29.0] | 14.5 [13.5, 17.4] | 0.58 [0.54, 0.67] |

**Run 1, decoding**

| image | NBIS 5.0.0 | wsq-codec, 1 thread | wsq-codec / NBIS |
|---|---|---|---|
| `dark_ridges` 5500x8000 | 3899.8 [3757.1, 4027.0] | 1281.9 [1236.7, 1342.1] | 0.33 [0.31, 0.35] |
| `noise` 1600x1500 | 165.4 [157.5, 177.2] | 59.3 [55.0, 68.0] | 0.36 [0.32, 0.42] |
| `ridges` 1600x1500 | 167.0 [157.0, 187.8] | 55.4 [52.3, 64.4] | 0.33 [0.29, 0.40] |
| `ridges` 800x750 | 36.7 [35.1, 43.0] | 13.8 [12.5, 16.0] | 0.38 [0.32, 0.41] |

**Run 2, encoding** (heavier load)

| image | NBIS 5.0.0 | wsq-codec, 1 thread | wsq-codec / NBIS |
|---|---|---|---|
| `dark_ridges` 5500x8000 | 2699.9 [2592.5, 2789.9] | 1326.3 [1217.7, 1404.7] | 0.49 [0.45, 0.51] |
| `noise` 1600x1500 | 142.0 [135.2, 148.5] | 62.7 [56.7, 74.7] | 0.44 [0.40, 0.53] |
| `ridges` 1600x1500 | 153.2 [140.4, 183.7] | 79.4 [66.9, 119.7] | 0.52 [0.38, 0.78] |
| `ridges` 800x750 | 48.6 [39.7, 64.7] | 22.2 [17.9, 31.2] | 0.46 [0.32, 0.68] |

**Run 2, decoding** (heavier load)

| image | NBIS 5.0.0 | wsq-codec, 1 thread | wsq-codec / NBIS |
|---|---|---|---|
| `dark_ridges` 5500x8000 | 3682.8 [3531.5, 3829.2] | 1197.1 [1106.3, 1212.4] | 0.33 [0.30, 0.34] |
| `noise` 1600x1500 | 178.0 [169.0, 201.4] | 56.2 [51.3, 64.3] | 0.32 [0.27, 0.36] |
| `ridges` 1600x1500 | 218.6 [188.6, 233.2] | 60.4 [57.6, 63.7] | 0.28 [0.25, 0.31] |
| `ridges` 800x750 | 69.7 [51.3, 89.5] | 24.1 [16.7, 26.3] | 0.35 [0.24, 0.49] |

What these show, and no more. Across both runs, both rates and the four images, the crate's median at
one thread is 0.38 to 0.58 times NBIS's when encoding and 0.28 to 0.39 times when decoding. In every
one of the 32 ratios the whole 95 % interval is below 1; the upper ends reach 0.78 (encoding) and 0.49
(decoding), under the heaviest load. The ratio of an image varies from one run to the other by more
than its interval in some cases (the 800x750 encoding at 0.75 bpp: 0.58 [0.54, 0.67] in run 1 and 0.46
[0.32, 0.68] in run 2, whose median lies outside the first interval), which says that the intervals of
one run understate the variation between runs on this machine. NBIS's own time on the same image
varied by almost a factor of two between the two runs (800x750 encoding, 24.9 ms and 48.6 ms).

### Thread scaling

`median(1 thread) / median(k threads)` with its interval, both rates, for the crate. Above 1: k
threads took less time. n.e.: not established (the interval contains 1).

**Encoding**

| image | bpp | run 1, 2 threads | run 1, 4 threads | run 1, 8 threads | run 2, 2 threads | run 2, 4 threads | run 2, 8 threads |
|---|---|---|---|---|---|---|---|
| `dark_ridges` 5500x8000 | 0.75 | 1.53 [1.08, 1.63] faster | 1.35 [1.14, 1.97] faster | 1.54 [1.31, 2.01] faster | 1.38 [1.13, 1.63] faster | 1.54 [1.17, 1.63] faster | 1.43 [1.30, 1.71] faster |
| `dark_ridges` 5500x8000 | 2.25 | 1.50 [1.00, 1.67] n.e. | 1.51 [1.12, 1.83] faster | 1.83 [1.23, 2.12] faster | 1.66 [1.51, 1.79] faster | 1.62 [1.41, 1.79] faster | 1.66 [1.50, 1.73] faster |
| `noise` 1600x1500 | 0.75 | 1.26 [1.17, 1.48] faster | 1.33 [1.23, 1.60] faster | 1.25 [1.18, 1.43] faster | 0.85 [0.73, 1.05] n.e. | 0.97 [0.88, 1.13] n.e. | 0.97 [0.86, 1.15] n.e. |
| `noise` 1600x1500 | 2.25 | 1.53 [1.34, 1.78] faster | 1.49 [1.32, 1.76] faster | 1.47 [1.25, 1.71] faster | 0.96 [0.87, 1.07] n.e. | 0.95 [0.91, 1.14] n.e. | 1.05 [0.98, 1.20] n.e. |
| `ridges` 1600x1500 | 0.75 | 1.19 [1.03, 1.25] faster | 1.16 [1.03, 1.27] faster | 1.07 [0.90, 1.17] n.e. | 0.93 [0.73, 1.44] n.e. | 1.00 [0.82, 1.46] n.e. | 1.04 [0.81, 1.58] n.e. |
| `ridges` 1600x1500 | 2.25 | 1.35 [1.20, 1.45] faster | 1.28 [1.16, 1.44] faster | 1.25 [1.16, 1.32] faster | 0.81 [0.75, 0.96] slower | 0.99 [0.84, 1.11] n.e. | 0.95 [0.87, 1.04] n.e. |
| `ridges` 800x750 | 0.75 | 1.03 [0.91, 1.19] n.e. | 0.93 [0.85, 1.08] n.e. | 0.78 [0.73, 0.96] slower | 0.79 [0.62, 1.25] n.e. | 0.82 [0.59, 1.27] n.e. | 0.81 [0.63, 1.13] n.e. |
| `ridges` 800x750 | 2.25 | 0.99 [0.89, 1.12] n.e. | 0.85 [0.83, 1.00] n.e. | 0.86 [0.83, 0.98] slower | 0.87 [0.63, 1.03] n.e. | 0.81 [0.68, 1.02] n.e. | 0.77 [0.65, 0.99] slower |

**Decoding**

| image | bpp | run 1, 2 threads | run 1, 4 threads | run 1, 8 threads | run 2, 2 threads | run 2, 4 threads | run 2, 8 threads |
|---|---|---|---|---|---|---|---|
| `dark_ridges` 5500x8000 | 0.75 | 1.58 [1.12, 1.66] faster | 1.36 [1.19, 2.00] faster | 1.48 [1.32, 2.23] faster | 1.45 [1.17, 1.56] faster | 1.53 [1.18, 1.71] faster | 1.63 [1.35, 1.80] faster |
| `dark_ridges` 5500x8000 | 2.25 | 1.24 [1.10, 1.64] faster | 1.79 [1.21, 2.04] faster | 1.31 [1.21, 2.08] faster | 1.54 [1.46, 1.64] faster | 1.67 [1.54, 1.77] faster | 1.70 [1.50, 1.81] faster |
| `noise` 1600x1500 | 0.75 | 1.39 [1.22, 1.65] faster | 1.45 [1.34, 1.67] faster | 1.39 [1.26, 1.60] faster | 1.00 [0.91, 1.14] n.e. | 1.00 [0.90, 1.17] n.e. | 0.97 [0.86, 1.15] n.e. |
| `noise` 1600x1500 | 2.25 | 1.29 [1.21, 1.40] faster | 1.49 [1.33, 1.63] faster | 1.38 [1.27, 1.51] faster | 0.97 [0.90, 1.09] n.e. | 0.97 [0.90, 1.13] n.e. | 1.05 [0.97, 1.19] n.e. |
| `ridges` 1600x1500 | 0.75 | 1.24 [1.15, 1.51] faster | 1.23 [1.12, 1.55] faster | 1.21 [1.07, 1.43] faster | 1.02 [0.91, 1.09] n.e. | 0.87 [0.78, 1.03] n.e. | 0.85 [0.80, 0.96] slower |
| `ridges` 1600x1500 | 2.25 | 1.39 [1.28, 1.59] faster | 1.40 [1.25, 1.60] faster | 1.32 [1.15, 1.56] faster | 0.85 [0.79, 1.15] n.e. | 0.83 [0.74, 1.11] n.e. | 0.93 [0.82, 1.26] n.e. |
| `ridges` 800x750 | 0.75 | 0.98 [0.87, 1.13] n.e. | 0.94 [0.83, 1.06] n.e. | 0.72 [0.64, 0.86] slower | 0.87 [0.56, 1.07] n.e. | 0.72 [0.47, 0.97] slower | 0.80 [0.52, 0.94] slower |
| `ridges` 800x750 | 2.25 | 1.05 [0.97, 1.19] n.e. | 1.00 [0.89, 1.13] n.e. | 0.79 [0.74, 0.92] slower | 0.73 [0.64, 1.02] n.e. | 0.65 [0.54, 1.01] n.e. | 0.72 [0.51, 1.01] n.e. |

What these show, and no more:

- **5500x8000 (a palm at 1000 ppi).** A gain is established in both runs: the interval is above 1 in 23
  of the 24 series (2 runs, encoding and decoding, 2 rates, 2, 4 and 8 threads), with medians 1.2 to
  1.8 times faster than at one thread. It is far from linear (four cores), and it does not grow
  clearly from 2 to 4 to 8 threads. Whether it holds on an unloaded machine, or at another size, is not
  shown.
- **1600x1500.** A gain in run 1 (23 of 24 series faster, 1.07 to 1.53), none in run 2 (22 of 24 n.e.,
  two slower). A result that does not repeat from one run to the next on this machine is **not
  established**.
- **800x750.** No gain in either run (medians 0.65 to 1.05); 7 of the 24 series are measurably slower,
  all at 4 or 8 threads. On a small image the threads cost.
- **Against 4 November.** Read again with the same bootstrap (no new measurement), the 4 November data
  at 4 threads give 0 series faster, 7 not established and 9 slower out of 16, including the 5500x8000
  image (encoding at 0.75 bpp slower, the other three not established). The gain on the largest
  image was therefore seen in the two runs of 10 November (codec 0.2.0) and not in the run of
  4 November (codec 0.1.1, a more loaded machine): it is a measured result of two runs, not a
  settled one.
- NBIS has no threads: a thread count here compares the crate on k threads with NBIS on one core. The
  throughput of several NBIS processes in parallel was not measured.

### Memory

Peak resident memory added by the call, MiB, median with its interval and ratio to NBIS (0.75 bpp,
run 1; run 2 differs by 0.1 MiB at most for NBIS and by 1 MiB at most for the crate).
The same reading for NBIS and the crate: `VmHWM` minus `VmRSS` before the call, from
`/proc/self/status`.

| image | operation | NBIS 5.0.0 | wsq-codec, 1 thread | wsq-codec, 4 threads | wsq-codec, 8 threads | 1 thread / NBIS | 4 threads / NBIS | 8 threads / NBIS |
|---|---|---|---|---|---|---|---|---|
| `dark_ridges` 5500x8000 | encode | 335.6 [335.6, 335.6] | 350.2 [350.2, 350.3] | 359.1 [359.0, 359.1] | 370.4 [370.4, 370.5] | 1.04 [1.04, 1.04] | 1.07 [1.07, 1.07] | 1.10 [1.10, 1.10] |
| `noise` 1600x1500 | encode | 18.2 [18.2, 18.2] | 22.1 [22.1, 22.1] | 23.9 [23.8, 24.0] | 25.6 [25.2, 25.8] | 1.22 [1.22, 1.22] | 1.32 [1.31, 1.32] | 1.41 [1.39, 1.42] |
| `ridges` 1600x1500 | encode | 18.2 [18.2, 18.2] | 22.1 [22.1, 22.1] | 23.9 [23.8, 24.0] | 25.7 [24.9, 26.4] | 1.22 [1.22, 1.22] | 1.32 [1.31, 1.32] | 1.41 [1.37, 1.46] |
| `ridges` 800x750 | encode | 4.5 [4.5, 4.5] | 6.7 [6.7, 6.7] | 7.2 [7.2, 7.3] | 7.9 [7.8, 8.0] | 1.49 [1.49, 1.49] | 1.61 [1.60, 1.61] | 1.75 [1.73, 1.77] |
| `dark_ridges` 5500x8000 | decode | 335.8 [335.8, 335.8] | 352.1 [352.1, 352.1] | 360.8 [360.7, 360.9] | 372.3 [372.2, 372.3] | 1.05 [1.05, 1.05] | 1.07 [1.07, 1.07] | 1.11 [1.11, 1.11] |
| `noise` 1600x1500 | decode | 18.5 [18.5, 18.5] | 22.8 [22.7, 22.8] | 25.2 [25.2, 25.4] | 28.8 [28.8, 28.9] | 1.23 [1.23, 1.23] | 1.37 [1.36, 1.38] | 1.56 [1.56, 1.56] |
| `ridges` 1600x1500 | decode | 18.5 [18.5, 18.5] | 22.6 [22.6, 22.6] | 25.5 [25.5, 25.6] | 28.9 [28.8, 28.9] | 1.22 [1.22, 1.22] | 1.38 [1.38, 1.38] | 1.56 [1.56, 1.56] |
| `ridges` 800x750 | decode | 4.8 [4.8, 4.8] | 6.9 [6.8, 6.9] | 8.3 [8.3, 8.3] | 10.0 [10.0, 10.0] | 1.44 [1.43, 1.45] | 1.73 [1.73, 1.74] | 2.10 [2.09, 2.10] |

The crate needs more memory than NBIS, not less: at one thread 4 % to 49 % more (the smaller the image,
the larger the share), more with more threads, up to 2.1 times NBIS's on the 800x750 image at 8
threads. No memory advantage is shown. The intervals are tight because memory is nearly
deterministic, which does not make the comparison wider than this machine, these images and this
build. The crate's figures are higher than on 4 November (for instance 22.8 MiB against 20.0 MiB for
decoding 1600x1500 at one thread) for a codec that is not the same (0.2.0 against 0.1.1); the
cause was not investigated, and NBIS's figures did not change.

## Real fingerprints: not executed

The `WSQ_CORPUS_DIR` mode of `bench/nbis-compare` measures a local directory of real fingerprints
(8-bit binary `.pgm` and `.wsq` files) against NBIS and writes aggregates only: counts, ratios and 95 %
intervals (the unit of resampling is the file), never a file name, a path or a per-file row. It is
described in [`bench/nbis-compare/README.md`](../bench/nbis-compare/README.md). It was tested here only
on three generated images, to see that it runs and writes no name.

**No run on a real corpus has been executed.** No real fingerprint was available or used in the sandbox that
produced the results above, and none may be put in this repository. The maintainer, on the machine that holds
the corpus, runs:

```sh
WSQ_CORPUS_DIR=/path/to/corpus NBIS=$PWD/Rel_5.0.0 NBIS_ARCHIVE=$PWD/nbis_v5_0_0.zip \
  bench/nbis-compare/run.sh /tmp/nbis-corpus 5
```

and checks `/tmp/nbis-corpus/corpus-summary.md` (the only file to share, with `machine.txt`) before
adding its content to this page under a dated heading: the date, the machine, the number of files and
the aggregates, with the corpus described in general terms only (kind, size, scanner class if
useful). Until then no statement about real fingerprints is made anywhere in this repository.

## Results of 4 November 2026 (version 2, kept as recorded)

These are the results of the first version of this comparison: 31 calls per series, no interval, the crate at 1 and 4 threads, release 0.1.1 beside the checkout of the day. They are unchanged. Where they disagree with the section above, the 10 November section governs; the figures below carry no interval and must not be quoted as measurements with a margin.


**Conditions** ([`machine.txt`](../bench/nbis-compare/results/2026-11-04/machine.txt)). Cloud
virtual machine: Intel Xeon at 2.10 GHz, 4 vCPUs, 16 GB, Linux 6.18 x86-64, rustc 1.97.0, gcc
13.3.0. Revision `1a1a34e` of this repository (clean working tree) and the 0.1.1 release. **Other
processes were running**: before the script did anything, the load average was 2.97 (1 minute) and
the CPUs were 89.3 % busy (steal 1.2 %); over the timing phase they were 87.3 % busy (the
benchmark included), with a load average of 4.8 at the end. No core was reserved or pinned.

### Identity with NBIS

All 96 checks are identical ([`identity.txt`](../bench/nbis-compare/results/2026-11-04/identity.txt)):
streams equal to NBIS's byte for byte and pixels equal in both directions, for the 8 pairs of
image and rate (streams from 56,089 to 12,518,245 bytes), for this checkout and the release, with
1 and 4 threads. The lengths are those of `tests/data/README.md`. It is a subset of that page's ten
streams (`rings` is not used) and says nothing more than it does, only that the result repeats here,
on this machine.

### Speed

Median in milliseconds, 90th percentile in parentheses; "median / NBIS" is the ratio of the
medians (below 1, the crate took less time). The last column is the median of the release over the
median of this checkout, at one thread.

**Encoding**, median ms (90th percentile)

| image | bpp | NBIS 5.0.0 | wsq-codec, 1 thread | median / NBIS | wsq-codec, 4 threads | median / NBIS | released / current, 1 thread |
|---|---|---|---|---|---|---|---|
| `dark_ridges` 5500x8000 | 0.75 | 3926.2 (4556.9) | 1393.1 (1678.7) | 0.35 | 1658.5 (1996.5) | 0.42 | 0.95 |
| `dark_ridges` 5500x8000 | 2.25 | 4307.1 (5167.6) | 1739.5 (1901.0) | 0.40 | 1583.6 (2175.6) | 0.37 | 1.00 |
| `noise` 1600x1500 | 0.75 | 120.9 (158.9) | 58.0 (87.7) | 0.48 | 57.2 (98.6) | 0.47 | 0.96 |
| `noise` 1600x1500 | 2.25 | 154.5 (189.1) | 82.0 (106.3) | 0.53 | 68.8 (111.8) | 0.44 | 0.94 |
| `ridges` 1600x1500 | 0.75 | 138.8 (194.4) | 61.9 (86.0) | 0.45 | 91.5 (119.5) | 0.66 | 1.05 |
| `ridges` 1600x1500 | 2.25 | 163.2 (214.1) | 85.8 (101.2) | 0.53 | 106.7 (141.4) | 0.65 | 0.94 |
| `ridges` 800x750 | 0.75 | 26.4 (39.2) | 13.1 (20.7) | 0.50 | 28.7 (43.9) | 1.09 | 1.00 |
| `ridges` 800x750 | 2.25 | 36.6 (48.1) | 19.6 (27.7) | 0.54 | 29.8 (53.4) | 0.82 | 1.05 |

**Decoding**, median ms (90th percentile)

| image | bpp | NBIS 5.0.0 | wsq-codec, 1 thread | median / NBIS | wsq-codec, 4 threads | median / NBIS | released / current, 1 thread |
|---|---|---|---|---|---|---|---|
| `dark_ridges` 5500x8000 | 0.75 | 4704.4 (5218.7) | 1290.2 (1567.1) | 0.27 | 1322.4 (1790.8) | 0.28 | 0.93 |
| `dark_ridges` 5500x8000 | 2.25 | 5504.1 (5910.7) | 1487.2 (1657.0) | 0.27 | 1687.7 (1922.4) | 0.31 | 0.96 |
| `noise` 1600x1500 | 0.75 | 157.1 (226.5) | 51.3 (71.2) | 0.33 | 50.8 (75.7) | 0.32 | 0.99 |
| `noise` 1600x1500 | 2.25 | 204.0 (295.4) | 61.4 (73.6) | 0.30 | 68.2 (94.0) | 0.33 | 1.01 |
| `ridges` 1600x1500 | 0.75 | 202.0 (268.0) | 52.8 (98.6) | 0.26 | 80.1 (94.7) | 0.40 | 1.16 |
| `ridges` 1600x1500 | 2.25 | 241.9 (317.0) | 68.1 (84.4) | 0.28 | 94.5 (113.4) | 0.39 | 1.25 |
| `ridges` 800x750 | 0.75 | 40.4 (56.7) | 12.8 (23.2) | 0.32 | 22.9 (43.1) | 0.57 | 0.94 |
| `ridges` 800x750 | 2.25 | 54.0 (70.5) | 15.7 (22.9) | 0.29 | 27.3 (47.3) | 0.51 | 1.08 |


What these figures show, and no more:

- **One thread.** In these sixteen series (eight pairs of image and rate, encoding and decoding),
  the crate's median is 0.35 to 0.54 times NBIS's when encoding and 0.26 to 0.33 times when decoding.
  In every series the crate's 90th percentile is below NBIS's minimum; its maximum is below NBIS's
  minimum in nine of the sixteen, and the other seven overlap in their extreme values, under the
  load described above.
- **Four threads.** No gain is established, and the gain is irregular: against one thread, the
  median of the crate with four threads ranges from 119 % longer (`ridges` 800x750, encoding at
  0.75 bpp) to 16 % shorter (`noise` 1600x1500, encoding at 2.25 bpp). Since other processes were
  using the cores, these differences cannot be told from contention on the machine, and they must not
  be turned into a speed-up. NBIS has no multi-threaded mode: the comparison at four threads sets the
  crate on four threads against NBIS on one core, and the throughput of four NBIS processes in
  parallel was not measured.
- **Versions.** The release and this checkout differ by -7 % to +25 % in their medians, in both
  directions, which is within the dispersion measured (NBIS's 90th percentile exceeds its median by
  7 % to 49 % in these series). No difference between the two versions is established.

### Memory

Peak resident memory added by the call, median in MiB, at 0.75 bpp (the medians at 2.25 bpp differ
by 0.1 MiB at most; see `summary.md`).

| image | operation | NBIS 5.0.0 | wsq-codec, 1 thread | wsq-codec, 4 threads |
|---|---|---|---|---|
| `dark_ridges` 5500x8000 | decode (0.75 bpp) | 335.8 | 341.6 | 350.7 |
| `dark_ridges` 5500x8000 | encode (0.75 bpp) | 335.6 | 339.7 | 348.5 |
| `noise` 1600x1500 | decode (0.75 bpp) | 18.5 | 20.0 | 22.5 |
| `noise` 1600x1500 | encode (0.75 bpp) | 18.2 | 19.5 | 21.9 |
| `ridges` 1600x1500 | decode (0.75 bpp) | 18.5 | 20.0 | 21.9 |
| `ridges` 1600x1500 | encode (0.75 bpp) | 18.2 | 19.5 | 21.3 |
| `ridges` 800x750 | decode (0.75 bpp) | 4.8 | 5.4 | 6.2 |
| `ridges` 800x750 | encode (0.75 bpp) | 4.5 | 5.2 | 5.9 |

At one thread the crate needs a little **more** memory than NBIS (1 % to 16 %, depending on the
image), not less; with four threads, more again (up to 31 % more on the smallest image). No memory
advantage is shown. Both cost about 8 bytes per pixel at the peak. The peak memory of a whole
program using the crate was not measured.

## Limits

- **Memory figures predate #67.** They were measured before the encoder's output and the wavelet
  transposition blocks became fallible allocations. The change adds no buffer (the output is
  reserved from its expected size, a fraction of a byte per pixel, instead of growing by
  doubling), but the figures were not measured again.
- **Synthetic images.** A codec's time depends on the content (quantization, Huffman coding). No
  real fingerprint was measured (the corpus run is [not executed](#real-fingerprints-not-executed)),
  and there are only three kinds of pattern.
- **One machine, uncontrolled load.** A 4-vCPU x86-64 virtual machine on which other processes were
  running (on 10 November a shared sandbox, 54 % to 99.5 % busy before the runs began), with no reserved
  core and no fixed frequency. The two runs of 10 November disagree on the thread scaling of the
  1600x1500 images, which is what the load does; a quiet, dedicated machine could give other
  figures in either direction. The ratios may change on another
  processor (vector instructions, cache); nothing was measured on macOS, Windows or ARM. A second
  run of the same day, with an earlier version of the script and a lower load, gave similar ratios
  at one thread (0.35 to 0.61 when encoding, 0.27 to 0.37 when decoding) but a different picture
  with four threads (26 % to 34 % shorter medians on 5500x8000); its raw data were not kept, so it
  is mentioned and not relied on.
- **Precise builds.** NBIS built with a portable `-O2` and the crate with a portable `release`
  profile. Another compiler, `-O3`, `-march=native` or profile-guided optimization for NBIS, or
  `target-cpu=native` for the crate, would give other ratios: not measured. Nothing is said about
  other WSQ implementations, commercial or not, which were not measured.
- **Cold call.** A call per process includes its page faults. It is not the steady state of a batch
  in one process, which `benches/codec.rs` measures; the two series differ.
- **Statistics.** 31 calls (11 for 5500x8000). From 10 November the medians carry a percentile
  bootstrap interval, which assumes the calls of a run are exchangeable draws from one distribution;
  on a loaded machine the load drifts, and the intervals of one run understate the difference
  between two runs (see the 800x750 ratios). Two runs is little evidence of repeatability. Do not read
  the ratios beyond their order of magnitude, and a difference under 10 % between two medians cannot
  be interpreted. The 4 November figures have no interval.
- **Rates and formats.** Two rates (0.75 and 2.25), 8 bits, 500 ppi declared. The deliberate
  differences from NBIS (images under 81 pixels, nearly flat images, the 32-bit sum overflowing on a
  bright 5500x8000 image) are not in the corpus; the README describes them.
- **Identity.** Shown for the 8 streams and reconstructions above and, in `tests/nbis.rs`, for the
  cases it lists; not proved for every image. It is not a certification: see
  [`docs/certification.md`](certification.md).

## Prior art

A search made on 4 November 2026 in public sources, before deciding whether "first WSQ codec in
Rust" could be written.

- **crates.io** (API search for `wsq`, `wavelet scalar quantization`, `wsq decoder`, `wsq encoder`,
  `biometric wsq`, `nbis`, and the keyword `wsq`): one WSQ codec crate, this one. `nbis-rs` 0.1.0
  (15 July 2025) is a **binding** to the C NBIS library; `wsq` 0.9.1 is an unrelated WebSocket queue.
- **GitHub** (repository search for `wsq`, `wsq fingerprint`, `wsq decoder`, `wsq encoder`,
  `wsq codec`, `WSQ compression`, `wavelet scalar quantization rust`):
  `lesjon/rust_wsq`, created on 8 November 2023 (created and last pushed the same day) and titled "Rust WSQ
  Encoder/Decoder". Files read on 4 November 2026: modules `swt` (265 lines), `entropy` (74 lines, its
  `encoder` module empty) and `quantization` (6 lines). A WSQ codec in Rust was therefore **begun
  before** this crate (repository created on 21 October 2026); from those files it is unfinished, but
  its state was judged from those three files only, without building it. `Nbeleski/wsq-rs`
  (13 January 2025) is a Rust library that compiles NBIS's C sources: a binding. The other results are
  in C, C++, C#, Java, Kotlin, Go or Python.
- **Limits of the search**: public APIs only, two hosts, no private repository or other forge, no
  search in the code of unindexed repositories, English terms. Not finding a project does not prove
  there is none.

Conclusion: **"first WSQ codec in Rust" must not be used**, since at least one earlier attempt exists
and the absence of others cannot be shown. A narrower statement is verifiable, "on 4 November 2026,
the only codec crate for WSQ in pure Rust on crates.io, bindings to NBIS excepted"; it ages quickly
and should only be repeated after a new dated search. This repository contains no "first" claim.

## Wording

Allowed, with the date, the machine, the interval and a link to this page:

- "On synthetic images, on one core, this crate's median time was 0.38 to 0.58 times that of NBIS 5.0.0
  (gcc 13.3, `-O2`) when encoding and 0.28 to 0.39 times when decoding (95 % bootstrap intervals
  within 0.29 to 0.78 and 0.23 to 0.49), in two runs on 10 November 2026 on a shared 4-vCPU Intel Xeon
  2.10 GHz cloud virtual machine with uncontrolled load."
- "On the 96 checks of each of the two runs of 10 November 2026 (8 pairs of image and rate, 1, 2, 4 and 8
  threads), the streams are identical byte for byte to NBIS 5.0.0's and the decoded pixels are
  identical in both directions."
- "On the 5500x8000 synthetic image, 2, 4 and 8 threads were 1.2 to 1.8 times faster than one thread
  (95 % interval above 1 in 23 of 24 series, two runs of 11 calls, 10 November 2026, shared 4-vCPU
  machine); the run of 4 November 2026 did not show it. On 1600x1500 images the gain was not
  reproduced between the two runs of 10 November; on 800x750 images there was none."
- "The memory a call adds is 4 % to 49 % above NBIS's at one thread, more with more threads (same
  `/proc/self/status` reading for both, 10 November 2026)."

Not allowed without new evidence: "faster than NBIS" with no condition; "faster than other WSQ
implementations"; "uses less memory" (the opposite was measured); "scales with threads", or a thread
gain on small or medium images; any statement about real fingerprints (no corpus run is recorded);
"first WSQ codec in Rust"; "certified" or "compliant" with the FBI or NIST (no official notification
and no cited proof: see [`docs/certification.md`](certification.md)). Any new release of the crate
means running the tool again before a figure is repeated. The 4 November statement ("0.35 to 0.54
times when encoding and 0.26 to 0.33 times when decoding", without interval) is superseded by the
first item above.

## Reproducing

Requirements: Linux (the tool reads `/proc`), gcc, Rust 1.85 or later, python3, and the NBIS 5.0.0
sources, public domain, from NIST:

- URL: `https://nigos.nist.gov/nist/nbis/nbis_v5_0_0.zip` (downloaded on 4 November 2026)
- SHA-256: `0adf8ab0f6b0e4208de50ca00ba21d3d77112ecd66288757ddfed21f6bee92c3`

```sh
unzip nbis_v5_0_0.zip                                   # gives Rel_5.0.0/
NBIS=$PWD/Rel_5.0.0 NBIS_ARCHIVE=$PWD/nbis_v5_0_0.zip \
  bench/nbis-compare/run.sh /tmp/nbis-compare 31 11     # THREADS="1 2 4 8" by default
python3 -I bench/nbis-compare/summarize.py /tmp/nbis-compare/results.csv --ci   # the tables of 10 November
```

The 4 November run used `WITH_RELEASED=1` and its tables came from `summarize.py results.csv --compact`
(still available). `run.sh` records the load, builds NBIS and the timer (`nbis_time.c`) and the Rust timer
(`src/main.rs`) with `--locked` (`Cargo.lock` for this checkout, `Cargo.released.lock` for the
release), checks the archive's SHA-256, generates the images, makes the 96 identity checks (it stops
at the first difference), times, then writes `machine.txt`, `identity.txt`, `results.csv` and
`summary.md` in the work directory. `machine.txt` also records the revision of this repository and
whether its working tree was modified. A full run took about twelve minutes on 4 November and about eight on 10 November (without the
release). Check the load before starting and keep the machine otherwise idle. When the crate's
version changes, `cargo update -p wsq-codec --manifest-path bench/nbis-compare/Cargo.toml` refreshes
`Cargo.lock`. The results of this page are in
[`bench/nbis-compare/results/2026-11-04`](../bench/nbis-compare/results/2026-11-04/),
[`2026-11-10`](../bench/nbis-compare/results/2026-11-10/) and
[`2026-11-10-run2`](../bench/nbis-compare/results/2026-11-10-run2/).
