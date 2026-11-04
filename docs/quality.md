# Quality benchmark

What the encoder delivers: the file size and actual bit rate it lands on for a requested bit
rate, and the quality of the reconstruction. The tool is `benches/quality.rs`
(`cargo bench --bench quality`, see the README); this page holds the figures it printed on the
date below and the conditions of measurement. They are measurements, not a verdict.

## Protocol

- Images: generated with integer arithmetic only, the same generator as `tests/nbis.rs`
  (seed 1), so they are identical on every platform and nothing is committed: `ridges`,
  `rings`, `noise` (800x750), `checker` (320x288) and, with `--large`, `ridges`
  (1600x1500). These are synthetic patterns, not fingerprints.
- Each image is encoded at 0.5, 0.75, 1.0 and 2.25 bits per pixel, at 500 ppi, with the default
  options (one thread), then decoded by this crate.
- Actual bit rate: the whole file (frame header, tables, NISTCOM comment, entropy-coded data)
  in bits, divided by the pixels. File size is the same figure in bytes.
- PSNR: 10 log10(255^2 / MSE) over every pixel of the image, in dB.
- SSIM: mean over 8x8 uniform windows at stride 4, with C1 = (0.01 * 255)^2 and
  C2 = (0.03 * 255)^2. It is not the Gaussian-window SSIM of the literature: compare it with
  other runs of this tool only.
- NBIS: `cwsq` and `dwsq` of NBIS 5.0.0 (`--nbis BIN_DIR`), built with
  `-ffp-contract=off -fno-fast-math` (`tests/data/README.md`). `cwsq` encodes the raw image
  with the same bit rate and 500 ppi, `dwsq` decodes its stream, and both are measured as
  this crate's are. The last column says whether the stream and the reconstruction are
  byte-identical to this crate's. The tool fails if one is not.

## Conditions

| | |
|---|---|
| Date | 2026-11-04 |
| Revision | `feat/qualite-bench`, on top of `origin/develop` at `7366f65` (workspace of PR #55) |
| Machine | 4 vCPUs, Intel Xeon @ 2.10 GHz, Linux 6.18 (x86-64) |
| Rust | 1.97.0, `cargo bench` (release profile) |
| NBIS | 5.0.0, gcc 13.3.0, `-O2 -w -ffp-contract=off -fno-fast-math -D__NBISLE__` |
| Command | `cargo bench --bench quality -- --large --nbis /path/to/nbis/bin` |

Times are in the tool's output but not here: they depend on the machine and the run.

## Conformant encoder, per image and bit rate

| image | requested bpp | file size, bytes | actual bpp | PSNR dB | SSIM | NBIS bytes | NBIS actual bpp | NBIS PSNR dB | identical to NBIS |
|---|---|---|---|---|---|---|---|---|---|
| ridges 800×750 | 0.5 | 39011 | 0.5201 | 34.17 | 0.9330 | 39011 | 0.5201 | 34.17 | yes |
| ridges 800×750 | 0.75 | 56126 | 0.7483 | 34.69 | 0.9415 | 56126 | 0.7483 | 34.69 | yes |
| ridges 800×750 | 1.0 | 78633 | 1.0484 | 35.37 | 0.9506 | 78633 | 1.0484 | 35.37 | yes |
| ridges 800×750 | 2.25 | 184145 | 2.4553 | 39.35 | 0.9800 | 184145 | 2.4553 | 39.35 | yes |
| rings 800×750 | 0.5 | 17067 | 0.2276 | 16.84 | 0.7052 | 17067 | 0.2276 | 16.84 | yes |
| rings 800×750 | 0.75 | 21558 | 0.2874 | 17.37 | 0.7315 | 21558 | 0.2874 | 17.37 | yes |
| rings 800×750 | 1.0 | 26207 | 0.3494 | 17.81 | 0.7495 | 26207 | 0.3494 | 17.81 | yes |
| rings 800×750 | 2.25 | 73660 | 0.9821 | 18.80 | 0.7870 | 73660 | 0.9821 | 18.80 | yes |
| noise 800×750 | 0.5 | 29185 | 0.3891 | 11.36 | 0.4333 | 29185 | 0.3891 | 11.36 | yes |
| noise 800×750 | 0.75 | 51038 | 0.6805 | 12.17 | 0.5865 | 51038 | 0.6805 | 12.17 | yes |
| noise 800×750 | 1.0 | 72653 | 0.9687 | 13.17 | 0.6871 | 72653 | 0.9687 | 13.17 | yes |
| noise 800×750 | 2.25 | 159510 | 2.1268 | 16.27 | 0.8396 | 159510 | 2.1268 | 16.27 | yes |
| checker 320×288 | 0.5 | 5117 | 0.4442 | 18.08 | 0.9181 | 5117 | 0.4442 | 18.08 | yes |
| checker 320×288 | 0.75 | 7692 | 0.6677 | 19.38 | 0.9405 | 7692 | 0.6677 | 19.38 | yes |
| checker 320×288 | 1.0 | 11454 | 0.9943 | 20.75 | 0.9577 | 11454 | 0.9943 | 20.75 | yes |
| checker 320×288 | 2.25 | 27342 | 2.3734 | 26.78 | 0.9893 | 27342 | 2.3734 | 26.78 | yes |
| ridges 1600×1500 | 0.5 | 152398 | 0.5080 | 34.17 | 0.9324 | 152398 | 0.5080 | 34.17 | yes |
| ridges 1600×1500 | 0.75 | 220900 | 0.7363 | 34.69 | 0.9409 | 220900 | 0.7363 | 34.69 | yes |
| ridges 1600×1500 | 1.0 | 311028 | 1.0368 | 35.37 | 0.9502 | 311028 | 1.0368 | 35.37 | yes |
| ridges 1600×1500 | 2.25 | 733608 | 2.4454 | 39.34 | 0.9798 | 733608 | 2.4454 | 39.34 | yes |

## Mean per requested bit rate

Over the five images, a plain mean of each column (the deviation is actual minus requested, over
requested):

| target bpp | images | mean actual bpp | mean deviation | mean PSNR dB | mean SSIM |
|---|---|---|---|---|---|
| 0.5 | 5 | 0.4178 | -16.4% | 22.92 | 0.7844 |
| 0.75 | 5 | 0.6241 | -16.8% | 23.66 | 0.8282 |
| 1 | 5 | 0.8795 | -12.0% | 24.49 | 0.8590 |
| 2.25 | 5 | 2.0766 | -7.7% | 28.11 | 0.9151 |

## Reproducing

```sh
cargo bench --bench quality -- --large --csv quality.csv                      # this crate only
cargo bench --bench quality -- --large --nbis /path/to/nbis/bin --csv quality.csv   # with NBIS
```

Streams written earlier by NBIS, on another machine for instance, are compared with
`--nbis-streams DIR` (files `IMAGE_RATE.wsq`); `--export DIR` writes the images as PGM for
that. `WSQ_QUALITY_DIR` names a local directory of 8-bit PGM files to measure as well; it stays
outside the repository, and no real fingerprint is ever committed. The optimized encoder of
phase 3 of #17 is to be measured with the same command and compared with this table.

Speed and memory against NBIS are in [nbis-comparison.md](nbis-comparison.md).
