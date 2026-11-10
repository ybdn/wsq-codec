# nbis-compare

Timing and memory harness of [`docs/nbis-comparison.md`](../../docs/nbis-comparison.md): this
crate against NBIS 5.0.0's `wsq_encode_mem` and `wsq_decode_mem`, one call per process.

| File | Role |
|---|---|
| `run.sh` | the whole run: builds, identity checks, interleaved timing rounds, summaries |
| `corpus.sh` | the `WSQ_CORPUS_DIR` mode of `run.sh` (real fingerprints, aggregates only) |
| `src/main.rs` | the crate's side of one call (`encode`, `decode`), the synthetic images (`gen`), `pgm2raw` |
| `nbis_time.c` | NBIS's side of one call, printing the same line |
| `summarize.py` | tables from the CSV: distributions, bootstrap intervals, corpus aggregates (stdlib only) |
| `results/` | dated runs; each directory is one run, never rewritten |

Both sides measure the same way: the time of the call alone, and the memory it adds, read from
`VmHWM` minus `VmRSS` in `/proc/self/status` just before the call (the same reading for NBIS and
the crate).

## Synthetic run

```sh
unzip nbis_v5_0_0.zip     # SHA-256 in docs/nbis-comparison.md; gives Rel_5.0.0/
NBIS=$PWD/Rel_5.0.0 NBIS_ARCHIVE=$PWD/nbis_v5_0_0.zip \
  bench/nbis-compare/run.sh /tmp/nbis-compare 31 11
```

`THREADS="1 2 4 8"` (the default) sets the thread counts measured for the crate; `WITH_RELEASED=1`
adds the crates.io release pinned in `Cargo.released.lock` at 1 and 4 threads. The work directory
receives `machine.txt` (CPU, load before and during, versions), `identity.txt`, `results.csv`,
`summary.md` and `ci.md`. `ci.md` is the version 3 report: per series, the median with a 95 %
percentile bootstrap interval (10,000 resamples of the rounds, seed 68, the same rounds drawn for all
implementations), the ratio to NBIS, the thread scaling with a verdict ("faster", "slower" or
"not established" according to whether the interval of `median(1 thread) / median(k threads)`
lies above 1, below 1 or contains 1) and the memory.

## Real fingerprints, locally: `WSQ_CORPUS_DIR`

The corpus never leaves the machine that holds it, and the output carries no trace of it.

```sh
WSQ_CORPUS_DIR=/path/to/corpus NBIS=$PWD/Rel_5.0.0 NBIS_ARCHIVE=$PWD/nbis_v5_0_0.zip \
  bench/nbis-compare/run.sh /tmp/nbis-corpus 5
```

The second argument is the number of timed calls per file, implementation and operation
(default 5); `CORPUS_RATE` (default 0.75) is the bit rate of the encoding series; `THREADS` is as
above. Every `*.pgm` (binary, 8 bits) and `*.wsq` under the directory is used: a PGM is encoded by
both implementations, then the stream NBIS wrote is decoded by both; a WSQ stream is decoded by both.
Files that are not readable are counted as skipped.

What is written to the work directory:

- `corpus-summary.md`: counts (files, identity checks, files where the crate took less time),
  the median over files of the per-file ratio to NBIS with its 95 % bootstrap interval (the unit of
  resampling is the file), the ratio of total times, the memory ratio, and the thread scaling.
  No file name, no path, no per-file row.
- `machine.txt`: CPU, versions and load; it does not mention the corpus.

What is not: the per-call rows (they carry the rank of the file in the sorted list, nothing else) live
in a private directory that `mktemp -d` creates under `$TMPDIR` (mode 700, not under `$WORKDIR`),
which is deleted when the script ends, even on failure, together with the list of corpus files and
the raw copy of the pixels; everything the script writes is owner-only (`umask 077`). The tools' error messages are discarded. The work directory also holds
the binaries and the build directory: **share `corpus-summary.md` and `machine.txt` only**. Do not
commit anything else, and never a fingerprint, a stream or a name from the corpus (see
[CONTRIBUTING.md](../../CONTRIBUTING.md)). Check `corpus-summary.md` once before putting it
anywhere: it is the only file that should leave the machine, and a corpus of a few files has intervals
too wide to mean anything.
