#!/bin/bash -eu
# Builds the cargo-fuzz targets of `fuzz/` for ClusterFuzzLite and packs
# their seeds. The targets and the seeds are described in fuzz/README.md.

cd "$SRC/wsq-codec"
cargo fuzz build -O --debug-assertions

target_dir=fuzz/target/x86_64-unknown-linux-gnu/release
for file in fuzz/fuzz_targets/*.rs; do
    name=$(basename "${file%.*}")
    cp "$target_dir/$name" "$OUT/"
    if [ "$name" = roundtrip ]; then
        zip -jq "$OUT/${name}_seed_corpus.zip" fuzz/seeds/roundtrip/*
    else
        zip -jq "$OUT/${name}_seed_corpus.zip" fuzz/seeds/streams/* certified/tests/data/*.wsq
    fi
done
