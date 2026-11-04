# Test fixtures

The `.wsq` files here were written by the WSQ encoder of NIST's NBIS 5.0.0
(public domain), from synthetic images that `tests/nbis.rs` and
`make_fixtures.py` generate identically. `tests/nbis.rs` checks that this
crate encodes those images to the same bytes and decodes the files to the
same pixels as NBIS.

`rings_300x211_even.wsq` uses NBIS's alternative 8/8-tap filter bank
(`FILTBANK_EVEN_8X8_1`), which the encoder here does not offer, to exercise
the decoder's support for even-length filters.

## Regenerating

With the NBIS sources unpacked in `$NBIS` (the `csrc` directory of the
`wsq` Python package carries the needed subset):

```sh
SRCS="$NBIS/imgtools/src/lib/wsq/*.c $NBIS/imgtools/src/lib/jpegl/*.c \
      $NBIS/commonnbis/src/lib/fet/*.c $NBIS/commonnbis/src/lib/ioutil/*.c \
      $NBIS/commonnbis/src/lib/util/*.c"
INC="-I$NBIS/commonnbis/include -I$NBIS/imgtools/include"
gcc -O2 -w -ffp-contract=off -D__NBISLE__ $INC -o nbis harness.c $SRCS -lm
gcc -O2 -w -ffp-contract=off -D__NBISLE__ -DFILTBANK_EVEN_8X8_1 $INC -o nbis_even harness.c $SRCS -lm
python3 make_fixtures.py ./nbis ./nbis_even
```

The script prints the hash of each image the reference decoder
produces; `tests/nbis.rs` holds the same values.

`-ffp-contract=off` keeps the compiler from fusing multiplications and additions into FMA
instructions: GCC does so by default on targets that have them (AArch64, x86-64 with
`-march=native`), which would make NBIS round differently from the strict IEEE 754 arithmetic
this crate uses. On a baseline x86-64 build it changes nothing (checked on the 1600x1500 noise
image: the same streams with and without); it is there so that the references do not depend on
the machine that made them.

## Large images

The fixtures above are at most 320x288. `tests/nbis.rs` also checks larger images against
NBIS 5.0.0 without any committed stream: for each image and bit rate it holds the length and
the FNV-1a hash of NBIS's stream and the hash of the pixels NBIS decodes from it. The test
generates the image (integer arithmetic only, the same as `pattern` in `make_fixtures.py`),
encodes it with this crate at 500 ppi, compares length and hash, then decodes the stream and
compares the pixel hash. As the streams are identical, that also checks the decoder against
NBIS's on large images.

| image | size | bit rates | run by |
|---|---|---|---|
| `ridges`, `rings` | 800x750 | 0.75, 2.25 | `cargo test` |
| `ridges` | 1600x1500 | 0.75, 2.25 | `cargo test` |
| `noise` | 1600x1500 | 0.75, 2.25 | `cargo test` |
| `dark_ridges` | 5500x8000 | 0.75, 2.25 | `cargo test --release --test nbis -- --ignored` (about 12 s) |

To make the references again, build `harness` as above and run:

```sh
python3 make_large_references.py ./nbis          # 800x750 and 1600x1500
python3 make_large_references.py ./nbis --huge   # also 5500x8000 (a few minutes in Python)
```

It prints one line per image and bit rate (stream length, stream hash, decoded hash), to copy
into `LARGE` and `HUGE` in `tests/nbis.rs`. If a test fails after a change, first rule out a
generator that drifted from `make_fixtures.py`, then compare the streams byte by byte
(`cwsq`/`dwsq`, `cmp -l`) before suspecting the codec; if the difference is one quantizer bin
width, check that value with a high-precision evaluation, because `src/math.rs` rounds
correctly where glibc, which NBIS used, very rarely does not.

`cwsq` and `dwsq`, NBIS's own command-line tools, build from the same sources (the image library
needs `ihead` and everything but `imgtype.c` and `imgdecod.c`, which need `libjpeg`; `dwsq`
needs a `print_usage` that its image library expects the program to define):

```sh
SRCS="$NBIS/imgtools/src/lib/wsq/*.c $NBIS/imgtools/src/lib/jpegl/*.c \
      $NBIS/imgtools/src/lib/ihead/*.c \
      $(ls $NBIS/imgtools/src/lib/image/*.c | grep -v -e imgtype.c -e imgdecod.c) \
      $NBIS/commonnbis/src/lib/fet/*.c $NBIS/commonnbis/src/lib/ioutil/*.c \
      $NBIS/commonnbis/src/lib/util/*.c"
echo 'void print_usage(char *a) {}' > usage.c
gcc -O2 -w -ffp-contract=off -D__NBISLE__ $INC -o cwsq $NBIS/imgtools/src/bin/cwsq/cwsq.c $SRCS -lm
gcc -O2 -w -ffp-contract=off -D__NBISLE__ $INC -o dwsq $NBIS/imgtools/src/bin/dwsq/dwsq.c usage.c $SRCS -lm
./cwsq 0.75 wsq image.raw -raw_in 1600,1500,8,500     # writes image.wsq
./dwsq raw image.wsq -raw_out                         # writes image.raw
```

The same images were also encoded with NBIS's own `cwsq` and decoded with `dwsq` (built with
the flags above, plus the `ihead` and `image` libraries of `imgtools` without `libjpeg`) and
compared with `cmp -l` against this crate's `wsq encode` and `decode`, at 500 ppi: the ten
streams of the table have the same length as NBIS's and zero differing bytes, and both
decoders give the same pixels.

| image | stream at 0.75 | stream at 2.25 |
|---|---|---|
| `ridges` 800x750 | 56 089 bytes | 184 038 bytes |
| `ridges` 1600x1500 | 220 487 bytes | 733 681 bytes |
| `rings` 800x750 | 21 599 bytes | 73 641 bytes |
| `noise` 1600x1500 | 201 447 bytes | 635 548 bytes |
| `dark_ridges` 5500x8000 | 3 932 440 bytes | 12 518 245 bytes |

### Why the 5500x8000 image is dark

NBIS sums the pixel values in an `unsigned int` (`conv_img_2_flt` in `wsq/util.c`) and gives up
with "overflow" when the sum passes 2^32 - 1. At 5500x8000, that is any image whose mean is
above 97.6; `ridges`, whose mean is 143, fails (`cwsq` exits with 165), as does a random one.
This crate sums in 64 bits and encodes them, so there is no NBIS reference for such an image:
`dark_ridges` has a mean of about 75 and fits. The benchmark's 5500x8000 image (mean 143, in
`benches/hashes.txt`) is therefore checked against itself across platforms, not against NBIS.

## Real-world files

Results of `cargo run --release --example wsq -- check`, with reference decodes from NBIS 5.0.0's
decoder (`harness d`, built as above). None of these files is committed here.

| source | encoder | streams | outcome |
|---|---|---|---|
| NIST's ANSI/NIST-ITL 2011 sample transactions, as shipped in the test resources of [JNBIS](https://github.com/mhshams/jnbis) (commit `4905b61`): 34 `.an2` files with Type-4, Type-13, Type-14 and Type-15 records | unknown, NBIS-style NISTCOM comments, implementation number 0 | 63 | all decode, identical to NBIS |
| `sample.wsq` from the same test resources | Cognaxon (its comment names it); NISTCOM with `PIX_DEPTH 24` | 1 | decodes, identical to NBIS |
| 96 images generated locally: 8 sizes from 81×81 to 1608×1000, ridges, noise and gradients, at 0.3, 0.75, 2.25 and 4.0 bits per pixel | this crate | 96 | NBIS and JNBIS (its Java port, same commit) decode them to the same pixels as this crate |

JNBIS also decodes the 64 streams above to the same pixels as NBIS. Being a port of NBIS, it
is not an independent check of the format, only of the implementations.

Several of the NIST streams, 1608×1000 slaps among them, carry `PPI 255` in their NISTCOM
comment, which is the comment's own value: the resolution to trust is the record's.
