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
gcc -O2 -w -D__NBISLE__ $INC -o nbis harness.c $SRCS -lm
gcc -O2 -w -D__NBISLE__ -DFILTBANK_EVEN_8X8_1 $INC -o nbis_even harness.c $SRCS -lm
python3 make_fixtures.py ./nbis ./nbis_even
```

The script prints the hash of each image the reference decoder
produces; `tests/nbis.rs` holds the same values.
