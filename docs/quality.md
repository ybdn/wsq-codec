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

## Experimental encoder that hits the rate (#42)

`wsq_codec::experimental::encode_at_rate`, a prototype for phase 3 of #17, hidden from the
documentation and outside semantic versioning. It keeps the conformant stream layout and tables
and changes only the quantizer bin widths: those of the specification's bit allocation, all
multiplied by one factor that a search chooses on the length of the whole stream. It stops on the
first stream at most 0.5% below the target size (target: requested bpp × pixels / 8 bytes) and
never returns one above it while one fits. Its streams carry implementation number 0. They are
not what NBIS writes for the requested rate, so they are never conformant.

### Conditions

| | |
|---|---|
| Date | 2026-11-04 |
| Revision | `feat/encodeur-optimise` at `a0d1c57`, on top of `origin/develop` at `673eab3` |
| Machine | as above, with other jobs running (load average 2.5 to 3.5 on 4 vCPUs) |
| NBIS | `dwsq` as above |
| Command | `cargo bench --bench quality -- --large --optimized --nbis /path/to/nbis/bin` |

The images and the rates are those of the tables above, one thread, fastest of three runs.
"Equal size" is the conformant encoder at the requested rate, found by bisection, whose stream is
closest in length to the experimental one; its PSNR and SSIM are those of that stream. The last
column compares NBIS's `dwsq` reconstruction of the experimental stream with this crate's.

### Per image and bit rate

| image | requested bpp | bytes | actual bpp | deviation | PSNR dB | SSIM | equal size: requested bpp | its bytes | its PSNR dB | its SSIM | NBIS `dwsq` |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ridges 800×750 | 0.5 | 37410 | 0.4988 | -0.24% | 34.11 | 0.9319 | 0.4727 | 37408 | 34.11 | 0.9319 | same pixels |
| ridges 800×750 | 0.75 | 56126 | 0.7483 | -0.22% | 34.69 | 0.9415 | 0.7501 | 56125 | 34.69 | 0.9415 | same pixels |
| ridges 800×750 | 1.0 | 74726 | 0.9963 | -0.37% | 35.24 | 0.9490 | 0.9599 | 74727 | 35.24 | 0.9490 | same pixels |
| ridges 800×750 | 2.25 | 168713 | 2.2495 | -0.02% | 38.93 | 0.9781 | 2.0477 | 168713 | 38.93 | 0.9781 | same pixels |
| rings 800×750 | 0.5 | 37384 | 0.4985 | -0.31% | 18.29 | 0.7682 | 1.4225 | 37384 | 18.29 | 0.7682 | same pixels |
| rings 800×750 | 0.75 | 56123 | 0.7483 | -0.23% | 18.63 | 0.7813 | 1.9027 | 56122 | 18.63 | 0.7813 | same pixels |
| rings 800×750 | 1.0 | 74884 | 0.9985 | -0.15% | 18.81 | 0.7873 | 2.2734 | 74884 | 18.81 | 0.7873 | same pixels |
| rings 800×750 | 2.25 | 168358 | 2.2448 | -0.23% | 19.08 | 0.7965 | 3.4021 | 168364 | 19.08 | 0.7965 | same pixels |
| noise 800×750 | 0.5 | 37420 | 0.4989 | -0.21% | 11.64 | 0.4998 | 0.5981 | 37420 | 11.64 | 0.4997 | same pixels |
| noise 800×750 | 0.75 | 56110 | 0.7481 | -0.25% | 12.39 | 0.6133 | 0.8063 | 56107 | 12.39 | 0.6134 | same pixels |
| noise 800×750 | 1.0 | 74757 | 0.9968 | -0.32% | 13.28 | 0.6958 | 1.0271 | 74757 | 13.28 | 0.6957 | same pixels |
| noise 800×750 | 2.25 | 168582 | 2.2478 | -0.10% | 16.37 | 0.8428 | 2.3732 | 168583 | 16.37 | 0.8428 | same pixels |
| checker 320×288 | 0.5 | 5740 | 0.4983 | -0.35% | 18.49 | 0.9252 | 0.5900 | 5740 | 18.49 | 0.9252 | same pixels |
| checker 320×288 | 0.75 | 8630 | 0.7491 | -0.12% | 19.72 | 0.9445 | 0.8250 | 8630 | 19.72 | 0.9445 | same pixels |
| checker 320×288 | 1.0 | 11512 | 0.9993 | -0.07% | 20.78 | 0.9580 | 1.0035 | 11512 | 20.78 | 0.9580 | same pixels |
| checker 320×288 | 2.25 | 25818 | 2.2411 | -0.39% | 26.05 | 0.9873 | 2.0918 | 25818 | 26.05 | 0.9873 | same pixels |
| ridges 1600×1500 | 0.5 | 149635 | 0.4988 | -0.24% | 34.14 | 0.9319 | 0.4878 | 149636 | 34.14 | 0.9319 | same pixels |
| ridges 1600×1500 | 0.75 | 224504 | 0.7483 | -0.22% | 34.71 | 0.9412 | 0.7607 | 224506 | 34.71 | 0.9413 | same pixels |
| ridges 1600×1500 | 1.0 | 299365 | 0.9979 | -0.21% | 35.27 | 0.9490 | 0.9695 | 299368 | 35.27 | 0.9490 | same pixels |
| ridges 1600×1500 | 2.25 | 672086 | 2.2403 | -0.43% | 38.94 | 0.9779 | 2.0496 | 672091 | 38.94 | 0.9779 | same pixels |

### Mean per requested bit rate

| target bpp | images | mean actual bpp | deviation, min / max | mean PSNR dB | mean PSNR difference at equal size, dB | mean SSIM difference | encoding time / conformant |
|---|---|---|---|---|---|---|---|
| 0.5 | 5 | 0.4986 | -0.35% / -0.21% | 23.33 | -0.000 | +0.0000 | 1.85 |
| 0.75 | 5 | 0.7485 | -0.25% / -0.12% | 24.03 | -0.000 | -0.0000 | 2.47 |
| 1 | 5 | 0.9977 | -0.37% / -0.07% | 24.67 | +0.000 | +0.0000 | 1.76 |
| 2.25 | 5 | 2.2447 | -0.43% / -0.02% | 27.87 | +0.000 | +0.0000 | 3.02 |

The time ratio is the sum of the experimental encoding times over the sum of the conformant ones
at the same requested rate, on a loaded machine: it says the order of magnitude of the cost of the
search (one quantization and one Huffman coding per stream tried), not a figure to quote.

### What these figures say, and what they do not

- On these 20 measurements the actual rate is between 0.43% and 0.02% below the requested one;
  the conformant encoder is 7.7% to 16.8% below on average (table above), and up to 65% below on
  `rings`. The stated margin of the prototype is therefore 0.5% below the target, none above.
- At equal size (streams within 6 bytes of each other), PSNR is the same as the conformant
  encoder's to the 0.01 dB printed, and SSIM within 0.0001. This follows from the code rather than from the
  images: the conformant bit allocation already gives every subband its width divided by one
  constant that depends on the requested rate (`src/quant.rs`, `bin_widths`), so scaling all the
  widths by one factor stays within the family of conformant encodings at other requested rates.
  The gain is in hitting the size, not in quality at a given size.
- NBIS's `dwsq` decodes the 20 experimental streams to the same pixels as this crate. JNBIS was
  not run for this table.
- The images are synthetic patterns, not fingerprints; one machine, under load.

## Reproducing

```sh
cargo bench --bench quality -- --large --csv quality.csv                      # this crate only
cargo bench --bench quality -- --large --nbis /path/to/nbis/bin --csv quality.csv   # with NBIS
```

Streams written earlier by NBIS, on another machine for instance, are compared with
`--nbis-streams DIR` (files `IMAGE_RATE.wsq`); `--export DIR` writes the images as PGM for
that. `WSQ_QUALITY_DIR` names a local directory of 8-bit PGM files to measure as well; it stays
outside the repository, and no real fingerprint is ever committed. `--optimized` adds the
experimental encoder of #42 and the conformant encoder at equal size:

```sh
cargo bench --bench quality -- --large --optimized --nbis /path/to/nbis/bin --csv quality.csv
```

Speed and memory against NBIS are in [nbis-comparison.md](nbis-comparison.md).
