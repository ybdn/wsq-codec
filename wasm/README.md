# wsq-codec (WebAssembly)

Decode and encode [WSQ](https://en.wikipedia.org/wiki/Wavelet_scalar_quantization) (Wavelet
Scalar Quantization) fingerprint images in the browser or in Node.js. It is the pure-Rust
[`wsq-codec`](https://github.com/ybdn-ei/wsq-codec) crate compiled to WebAssembly, with a thin
[wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) wrapper: the same code, the same
bytes out (the default encoder reproduces NIST's reference encoder byte for byte), no
dependency, no server.

**Try it without installing anything:** the [demo page](https://ybdn-ei.github.io/wsq-codec/)
opens `.wsq` files and converts PNG/JPEG to WSQ entirely in your browser. No file is uploaded,
and a Content-Security-Policy makes sure the page cannot send one.

## Install

```sh
npm install wsq-codec
```

The package is named `wsq-codec`, like the Rust crate. It holds three builds of the module,
picked by `exports`: a bundler build (the default import), a CommonJS build for Node.js (the
`node` condition) and a `web` build for use without a bundler.

## Use

### With a bundler (webpack 5, Vite, ...) or in Node.js

```js
import { decode, readInfo, encode } from "wsq-codec";

const wsq = new Uint8Array(await (await fetch("finger.wsq")).arrayBuffer());

const info = readInfo(wsq);            // headers only, nothing is decoded
console.log(info.width, info.height, info.ppi, info.comments);

const image = decode(wsq);             // { width, height, ppi, software, comments, pixels }
const gray = image.pixels;             // Uint8Array, width * height bytes, 0 is black

const out = encode(gray, image.width, image.height, {
  bitrate: 0.75,                       // bits per pixel (the default)
  ppi: 500,                            // recorded in the stream
  comment: "scanned on site",
  profile: "conformant",               // or "optimized"
});                                    // Uint8Array: the .wsq file
```

Bundlers need to support importing `.wasm` modules: webpack 5 does out of the box
(`experiments.asyncWebAssembly`), Vite needs `vite-plugin-wasm`. If yours does not, use the
`web` build below.

### In a page without a bundler

```html
<script type="module">
  import init, { decode } from "./node_modules/wsq-codec/web/wsq_codec.js";
  await init();                        // fetches wsq_codec_bg.wasm next to the module
  // ... decode, readInfo and encode as above
</script>
```

or `import init, { decode } from "wsq-codec/web"` through an import map or a bundler. Serve the
`.wasm` file as `application/wasm` (every static host does).

## API

TypeScript declarations are included.

| Function | Returns |
| --- | --- |
| `decode(data: Uint8Array)` | `WsqImage`: `width`, `height`, `ppi` (a number, or `undefined` when the stream does not say), `software`, `comments` (`string[]`), `pixels` (`Uint8Array`, copied on each access: read it once) |
| `readInfo(data: Uint8Array)` | `WsqInfo`: the same, without `pixels`; reads only the headers |
| `encode(pixels: Uint8Array, width: number, height: number, options?: EncodeOptions)` | `Uint8Array`, the WSQ stream |
| `version()` | the package version |

`EncodeOptions` (all optional):

| Field | Meaning |
| --- | --- |
| `bitrate` | target bits per pixel, default `0.75` (about 15:1); `2.25` gives about 5:1 |
| `ppi` | resolution to record, a whole number of pixels per inch; none by default |
| `comment` | a text comment to store in the stream (UTF-8) |
| `profile` | `"conformant"` (default): the encoder aligned with NIST's reference, the one to use for certification; `"optimized"`: meets the requested size more closely, never certified |

Both dimensions must be between 81 and 65535. Images are 8-bit grayscale, row-major, 0 for
black; to encode a canvas, convert `getImageData` to grayscale first (the demo shows how).

Errors are thrown as JavaScript `Error`s whose message is the Rust error's, for example
`not a WSQ stream (no start-of-image marker)` or `cannot encode a 10x10 image`. `WsqImage` and
`WsqInfo` objects own WebAssembly memory that the garbage collector frees; call `.free()` to
release it sooner.

WebAssembly has no threads, so everything runs on the calling thread: decode or encode large
images in a Web Worker to keep a page responsive. A 500 ppi fingerprint takes a fraction of a
second. Images are limited to 100 million pixels, and the WebAssembly memory of 4 GiB bounds
the largest image a browser can handle.

## Fingerprints are biometric data

Nothing here sends data anywhere, but be careful with real fingerprints, in tests and in bug
reports: use synthetic images when you can.

## Building from source

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
wasm/build.sh                          # writes wasm/pkg (the npm package) and wasm/demo/wsq
wasm-pack test --node wasm             # the tests, run in Node.js
python3 -m http.server -d wasm/demo    # serve the demo at http://localhost:8000
```

## License

MIT.
