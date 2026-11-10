# Fuzzing wsq-codec

Four [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) targets, in their own
workspace. This directory is not part of the published package.

| target | what it checks |
|---|---|
| `decode_with` | `decode_with` on arbitrary bytes (image limited to 1 Mpx): no panic, `pixels.len() == width * height`, and the same result with 1 and 3 threads |
| `read_info` | `read_info` on arbitrary bytes: no panic, and the dimensions and resolution agree with a full decode |
| `read_quantized` | `inspect::read_quantized` on arbitrary bytes: no panic, 60 subbands, each holding `width * height` coefficients or none when its bin width is zero |
| `roundtrip` | `encode` then `decode` of arbitrary pixels and options: no panic, decoded size equal to the input size |

`roundtrip` reads its input as 7 bytes of options and dimensions (width and height from 81 to
160, bit rate, resolution, threads), then pixels, repeated to fill the image.

## Running

```sh
cargo install cargo-fuzz
rustup toolchain install nightly --profile minimal
mkdir -p fuzz/corpus/decode_with
cargo +nightly fuzz run decode_with fuzz/corpus/decode_with fuzz/seeds/streams -- -max_total_time=60
```

libFuzzer writes the new inputs it finds to the first corpus directory: give it one of its own
(`fuzz/corpus/<target>`, ignored by git) before the seeds, as above, or it fills the seeds up.
`fuzz/seeds/streams` seeds `decode_with`, `read_info` and `read_quantized`, and
`fuzz/seeds/roundtrip` seeds `roundtrip`. The fixtures of `certified/tests/data` make good extra seeds for
the first three. Crashes go to `fuzz/artifacts/<target>`, also ignored by git. Turn a crash into
a regression test and fix it; do not commit it as a seed.

## Seeds

The seeds are a handful of synthetic streams, about 5 KB in all: valid streams with random
dimensions, filter banks, quantization tables and coefficients, one mutated copy, and one
encoder output. They come from the generator of `certified/src/testgen.rs`, which the unit test
`generated_streams_decode_or_fail_cleanly` also uses. To write them again:

```sh
WSQ_SEED_DIR=$PWD/fuzz/seeds cargo test -p wsq-certified --lib write_fuzz_seeds -- --ignored
```

Never add real fingerprints, or data from a real transaction, as seeds.

## Continuous fuzzing

[ClusterFuzzLite](https://google.github.io/clusterfuzzlite/) builds the targets with
`.clusterfuzzlite/build.sh`. `.github/workflows/cflite_pr.yml` runs them for 5 minutes on each
pull request that changes code, and `.github/workflows/scheduled.yml` for an hour every
week. Neither keeps the corpus between runs yet (that needs a storage repository).
