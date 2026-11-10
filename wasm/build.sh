#!/usr/bin/env bash
# Builds the npm package `wsq-codec` into wasm/pkg, and the demo's copy of the web build into
# wasm/demo/wsq. Needs wasm-pack (`cargo install wasm-pack --locked`) and the
# wasm32-unknown-unknown target (`rustup target add wasm32-unknown-unknown`).
#
# The package holds three builds of the same module, picked by its `exports`:
#   bundler/  ES module that imports the .wasm file (webpack 5, Vite with a wasm plugin, ...)
#   nodejs/   CommonJS for Node.js (the "node" condition)
#   web/      ES module with an `init()` that fetches the .wasm file: no bundler needed
#
# Usage: wasm/build.sh [--dev]
# Set WSQ_NO_OPT=1 to skip wasm-opt (it is downloaded from GitHub on first use).
set -euo pipefail

cd "$(dirname "$0")"

mode=--release
[ "${1:-}" = "--dev" ] && mode=--dev

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
[ -n "$version" ] || { echo "no version in wasm/Cargo.toml" >&2; exit 1; }

opt=()
[ -n "${WSQ_NO_OPT:-}" ] && opt=(--no-opt)

rm -rf pkg
for target in web bundler nodejs; do
  wasm-pack build . "$mode" --target "$target" --out-dir "pkg/$target" --out-name wsq_codec \
    --no-pack --no-gitignore ${opt[@]+"${opt[@]}"}
done

# Each build is a module of its own kind, whatever the package says.
echo '{ "type": "module" }' > pkg/web/package.json
echo '{ "type": "module" }' > pkg/bundler/package.json
echo '{ "type": "commonjs" }' > pkg/nodejs/package.json

sed "s/@VERSION@/$version/" package.template.json > pkg/package.json
cp README.md pkg/README.md
cp ../LICENSE pkg/LICENSE

# The demo loads the web build from its own directory (same origin, no CDN).
rm -rf demo/wsq
mkdir -p demo/wsq
cp pkg/web/wsq_codec.js pkg/web/wsq_codec_bg.wasm demo/wsq/

echo "built wsq-codec $version in wasm/pkg"
