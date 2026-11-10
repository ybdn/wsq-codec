# Comparison with NBIS: speed and memory

How long this crate takes to encode and decode, and how much memory it needs, next to NBIS 5.0.0's
own `wsq_encode_mem` and `wsq_decode_mem`, on synthetic images. The tool is
[`bench/nbis-compare`](../bench/nbis-compare/); this page holds the protocol, the figures it
printed on the date below, and what they do not show. They are measurements, not a verdict, and
they claim no superiority beyond the conditions listed here. No real fingerprint is used.

Anyone can repeat the measurement: the protocol, the harness and the raw results are in this
public repository, and [Reproducing](#reproducing) names the NBIS archive with its SHA-256.

## What is where

| Subject | Where |
|---|---|
| Speed and memory against NBIS, distributions, one and four threads | this page |
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
| `wsq-codec`, this checkout | the revision of the repository, 1 and 4 threads (`threads` option) | rustc 1.97.0, Cargo's `release` profile (`opt-level = 3`, no LTO, 16 codegen units), no `target-cpu` |
| `wsq-codec`, release | 0.1.1 from crates.io, pinned by `Cargo.released.lock`, 1 and 4 threads | same |

`-ffp-contract=off` is the build this repository uses for its NBIS references: without it gcc may
fuse multiplications and additions and NBIS rounds differently. There is no "fastest possible"
build of NBIS (`-O3`, `-march=native`, profile-guided) in this protocol.

**Measurement.** One call per process, in memory. The process reads its input, notes its
resident set, times the call alone (`CLOCK_MONOTONIC` in C, `Instant` in Rust; reading and writing
files excluded), then notes its peak resident set (`VmHWM`). The memory reported is that peak minus
the resident set before the call, so the input buffer is not counted: it is the cost of the call,
allocations and page faults included. Each call starts from the same state (a fresh process, the
input file in the page cache).

**Repetitions.** Per image, rate, operation and implementation: 31 calls (11 for 5500x8000). The
implementations are **interleaved**: in each round all the calls of all the implementations follow
one another, in an order that rotates from round to round, so that a drift of the machine does not
favour one of them. No call is discarded and there is no warm-up round. The report gives the
minimum, the median, the 90th percentile (nearest rank) and the maximum, never only the best time.
All 2080 raw measurements are in
[`results.csv`](../bench/nbis-compare/results/2026-11-04/results.csv).

**Identity, before any timing.** For each image and rate, and for each configuration of the crate
(this checkout and the release, 1 and 4 threads): (1) the crate's stream is compared byte for byte
with NBIS's; (2) the crate decodes NBIS's stream and the pixels are compared with those NBIS
decodes from its own stream; (3) NBIS decodes the crate's stream and the pixels are compared in the
same way. If one of the 96 checks differs, the script stops without timing anything. During the
timing, the size and FNV-1a hash of every output are recorded and must be the same for all the
implementations.

**Load.** The machine's load is not controlled. The script records the load average and the CPU
busy and steal shares over 5 seconds **before it starts any work**, and again over the whole
timing phase. The latter includes the benchmark's own processes (one core, or up to four with
threads), so only the former says what else was running.

## Results of 4 November 2026

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
  real fingerprint was measured, and there are only three kinds of pattern.
- **One machine, uncontrolled load.** A 4-vCPU x86-64 virtual machine on which other processes were
  running, with no reserved core and no fixed frequency. The ratios may change on another
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
- **Statistics.** 31 calls (11 for 5500x8000), no confidence interval. Do not read the ratios
  beyond their order of magnitude: a difference under 10 % between two medians cannot be
  interpreted.
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

Allowed, with the date, the machine and a link to this page:

- "On synthetic images, on one core, this crate took 0.35 to 0.54 times the median time of NBIS 5.0.0
  (gcc 13.3, `-O2`) when encoding and 0.26 to 0.33 times when decoding, on a 4-vCPU x86-64 Linux
  virtual machine with uncontrolled load, on 4 November 2026."
- "On the 96 checks repeated on that date (8 pairs of image and rate, this revision and release 0.1.1,
  1 and 4 threads), the streams are identical byte for byte to NBIS 5.0.0's and the decoded pixels
  are identical in both directions."

Not allowed without new evidence: "faster than NBIS" with no condition; "faster than other WSQ
implementations"; "uses less memory" (the opposite was measured); "scales with threads"; "first WSQ
codec in Rust"; "certified" or "compliant" with the FBI or NIST (no official notification and no cited
proof: see [`docs/certification.md`](certification.md)). Any new release of the crate means running
the tool again before a figure is repeated.

## Reproducing

Requirements: Linux (the tool reads `/proc`), gcc, Rust 1.85 or later, python3, and the NBIS 5.0.0
sources, public domain, from NIST:

- URL: `https://nigos.nist.gov/nist/nbis/nbis_v5_0_0.zip` (downloaded on 4 November 2026)
- SHA-256: `0adf8ab0f6b0e4208de50ca00ba21d3d77112ecd66288757ddfed21f6bee92c3`

```sh
unzip nbis_v5_0_0.zip                                   # gives Rel_5.0.0/
NBIS=$PWD/Rel_5.0.0 NBIS_ARCHIVE=$PWD/nbis_v5_0_0.zip WITH_RELEASED=1 \
  bench/nbis-compare/run.sh /tmp/nbis-compare 31 11
python3 -I bench/nbis-compare/summarize.py /tmp/nbis-compare/results.csv --compact   # the tables above
```

`run.sh` records the load, builds NBIS and the timer (`nbis_time.c`) and the Rust timer
(`src/main.rs`) with `--locked` (`Cargo.lock` for this checkout, `Cargo.released.lock` for the
release), checks the archive's SHA-256, generates the images, makes the 96 identity checks (it stops
at the first difference), times, then writes `machine.txt`, `identity.txt`, `results.csv` and
`summary.md` in the work directory. `machine.txt` also records the revision of this repository and
whether its working tree was modified. A full run takes about twelve minutes on the machine
described. Check the load before starting and keep the machine otherwise idle. When the crate's
version changes, `cargo update -p wsq-codec --manifest-path bench/nbis-compare/Cargo.toml` refreshes
`Cargo.lock`. The results of this page are in
[`bench/nbis-compare/results/2026-11-04`](../bench/nbis-compare/results/2026-11-04/).
