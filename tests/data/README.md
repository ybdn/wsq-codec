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

## Real-world files

Results of `cargo run --release --example wsq -- check`, with reference decodes from NBIS 5.0.0's
decoder (`harness d`, built as above). None of these files is committed here.

| source | encoder | streams | outcome |
|---|---|---|---|
| NIST's ANSI/NIST-ITL 2011 sample transactions, as shipped in the test resources of [JNBIS](https://github.com/mhshams/jnbis) (commit `4905b61`): 34 `.an2` files with Type-4, Type-13, Type-14 and Type-15 records | unknown, NBIS-style NISTCOM comments, implementation number 0 | 63 | all decode, identical to NBIS |
| `sample.wsq` from the same test resources | Cognaxon (its comment names it); NISTCOM with `PIX_DEPTH 24` | 1 | decodes, identical to NBIS |

Several of the NIST streams, 1608×1000 slaps among them, carry `PPI 255` in their NISTCOM
comment, which is the comment's own value: the resolution to trust is the record's.
