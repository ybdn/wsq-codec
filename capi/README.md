# wsq C API

A C interface to [`wsq-codec`](../README.md), the pure-Rust WSQ fingerprint codec: decode and
encode WSQ from C or C++ (or any language that can call C) without NBIS and without writing any
Rust. It builds `libwsq` as a static and a shared library, and the header
[`include/wsq.h`](include/wsq.h).

```c
#include "wsq.h"

wsq_image image;
if (wsq_decode(data, size, NULL, &image) != WSQ_OK) {
    fprintf(stderr, "wsq: %s\n", wsq_last_error_message());
    return 1;
}
/* image.info.width, image.info.height, image.info.ppi, image.pixels (width * height bytes) */
wsq_image_free(&image);
```

The library decodes to the same pixels as NBIS and its conformant encoder writes the same bytes
(see the [main README](../README.md)). It is not certified by the FBI.

## Getting it

- **Prebuilt**: each [GitHub release](https://github.com/ybdn/wsq-codec/releases) attaches
  `wsq-capi-vX.Y.Z-TARGET.tar.gz` (`.zip` for Windows) for `x86_64-unknown-linux-gnu`,
  `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin` (Apple silicon) and
  `x86_64-pc-windows-msvc`, where `vX.Y.Z` is the release tag. Each holds `include/wsq.h`, the
  static and shared libraries in `lib/`, and `LICENSE`. The version of the interface itself is
  `wsq_version()`, that of the `wsq-capi` crate.
- **From source**, with Rust 1.85 or later:

  ```sh
  cargo build --release -p wsq-capi
  ```

  Linux gives `target/release/libwsq.a` and `libwsq.so`, macOS `libwsq.a` and `libwsq.dylib`,
  Windows (MSVC) `wsq.lib` (static), `wsq.dll` and `wsq.dll.lib` (the import library of the
  DLL). The header is committed, so a C project needs neither Rust nor cbindgen to read it.

The crate is not published on crates.io (`publish = false`): a C program cannot depend on a
crate, and the header is generated, so a source package would add nothing. The releases are the
distribution.

## Linking

Shared library:

```sh
cc -Iinclude prog.c -Llib -lwsq -Wl,-rpath,/path/to/lib      # Linux, macOS
cl /I include prog.c wsq.dll.lib                              # Windows; wsq.dll next to prog.exe
```

Static library: besides `libwsq.a` / `wsq.lib`, the program needs the system libraries that
Rust's standard library uses. `rustc` prints the list for your platform:

```sh
rustc --print native-static-libs --crate-type staticlib -o /dev/null - <<< ''
# Linux (glibc):  -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc
# macOS:          typically -lSystem -lc -lm
# Windows (MSVC): a list of .lib files; compile the program with /MD (the dynamic C runtime)
```

```sh
cc -Iinclude prog.c lib/libwsq.a -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc -o prog
```

(The test `tests/c_roundtrip.rs` does exactly this on every platform.) No `pkg-config` file is
shipped: the paths depend on where you unpack the archive, and a build system that wants one can
write a three-line `wsq.pc` for its layout.

The header works from C99 (an enum fixed to `int32_t` is used from C23 and in C++) and from C++
(it has `extern "C"` guards). On Windows the functions are exported from the DLL without
`__declspec(dllimport)` decoration, which is correct for functions.

## Functions

| Function | What it does |
| --- | --- |
| `wsq_decode(data, len, options, &image)` | decode a stream into a new `wsq_image` |
| `wsq_decode_into(data, len, options, pixels, pixels_len, info)` | decode into your own buffer |
| `wsq_read_info(data, len, &info)` | dimensions, resolution and comments, without decoding the image |
| `wsq_encode(pixels, pixels_len, width, height, options, &out, &out_len)` | encode an 8-bit image |
| `wsq_decode_options_default()`, `wsq_encode_options_default()` | the default settings |
| `wsq_image_free`, `wsq_info_free`, `wsq_free_bytes` | release what the library returned |
| `wsq_status_message(status)`, `wsq_last_error_message()` | text for a status; detail of the last failure |
| `wsq_version()` | version of the library |

`wsq.h` documents each one. Images are 8-bit grayscale, row-major, one byte per pixel (0 is
black), from 1 to 65535 pixels on a side; the encoder needs at least 81 on each side.

## Ownership

The rule is one allocator per direction: **whatever the library returns, you give back to the
library with the matching function; whatever you pass in, you keep.**

| You get | From | Release with |
| --- | --- | --- |
| the stream `*out` (`out_len` bytes) | `wsq_encode` | `wsq_free_bytes(*out, *out_len)` |
| `wsq_image` (its `pixels`, `info.comments` and every comment payload) | `wsq_decode` | `wsq_image_free(&image)` |
| `wsq_info` (its `comments` and the payloads) | `wsq_read_info`, `wsq_decode_into` | `wsq_info_free(&info)` |
| a `const char *` | `wsq_status_message`, `wsq_version`, `wsq_last_error_message` | nothing, never |

- Never pass library memory to `free()`, nor your `malloc`'d memory to a `wsq_*_free`: the two
  may use different allocators (always, on Windows with a different C runtime).
- Everything inside a `wsq_image` or `wsq_info` belongs to it. Do not free, resize or keep
  pointers into it after `wsq_image_free` / `wsq_info_free`. You may modify `pixels` in place.
- The `*_free` functions accept NULL, reset the structure to empty (so freeing twice is
  harmless), and need a structure that holds either what the library filled in or zeros:
  initialize one you might free without having decoded (`wsq_image image = {0};`).
- A failed call leaves nothing to free: `wsq_decode` and `wsq_read_info` set their output to an
  empty structure first, `wsq_encode` sets `*out` to NULL and `*out_len` to 0. So the
  outputs are never read: an image that is still allocated must be freed before it is reused.
- The library only reads what you pass (`data`, `pixels`, options, comments) during the call and
  keeps no pointer to it. The memory behind `data` and `pixels` may be a read-only mapping.
- An empty comment or an empty `wsq_bytes` is a NULL `data` with `len` 0.
- Zero-length results are NULL: a stream with no comments has `comments == NULL`.

Under valgrind or AddressSanitizer a program is clean, with one exception: the first time a
thread calls the library with `threads` other than 1, Rust's standard library allocates a small
(48 bytes) handle for it that is released when the thread exits. valgrind reports it as "possibly
lost" for the main thread of a C program, which ends without running thread-exit code. It is not
a leak and does not grow.

## Thread safety

The library has no global state besides a per-thread error message. Every function can be called
from any number of threads at once, on different or the same read-only input, and none holds a
lock or blocks other threads. `wsq_status_message` and `wsq_version` return static strings.
`wsq_last_error_message` is per thread: it describes the last failed call of the *calling* thread,
and its string is valid until that thread's next `wsq_decode`, `wsq_decode_into`,
`wsq_read_info` or `wsq_encode`. The `wsq_*_free` functions do not touch it.

A single `wsq_image`, `wsq_info` or output buffer must not be used from two threads at once (the
usual rule for any C structure). The `threads` option makes one call use several threads
internally; the output does not depend on it. All functions are reentrant.

## Errors

Every function that can fail returns a `wsq_status`; `WSQ_OK` is 0. The values are explicit and
**never change**; new failures get new values, so treat an unknown non-zero value as a failure
(`wsq_status_message` accepts any `int`).

| Code | Value | Meaning |
| --- | --- | --- |
| `WSQ_OK` | 0 | success |
| `WSQ_ERR_INVALID_ARGUMENT` | 1 | NULL pointer, unusable length, malformed or unknown option (`struct_size`, `profile`) |
| `WSQ_ERR_INVALID_STREAM` | 2 | not a WSQ stream, truncated, or corrupt |
| `WSQ_ERR_TOO_LARGE` | 3 | the image has more than `max_pixels` pixels (decoding) |
| `WSQ_ERR_OUT_OF_MEMORY` | 4 | the image or its work buffers could not be allocated |
| `WSQ_ERR_SOFTWARE_REQUIRES_CONFORMANT` | 5 | a non-zero `software` with the optimized profile |
| `WSQ_ERR_PANIC` | 6 | a bug panicked inside the library; caught, nothing to free |
| `WSQ_ERR_INVALID_DIMENSIONS` | 7 | width or height outside 81 to 65535 (encoding) |
| `WSQ_ERR_BUFFER_SIZE` | 8 | `pixels_len` is not `width * height`, or the output buffer of `wsq_decode_into` has the wrong size |
| `WSQ_ERR_INVALID_BITRATE` | 9 | the bit rate is not positive and finite, or too low |
| `WSQ_ERR_COMMENT_TOO_LONG` | 10 | a comment does not fit in one segment |
| `WSQ_ERR_UNKNOWN` | 99 | an error a later version of the Rust library may add |

Each `DecodeError` and `EncodeError` of the Rust crate maps to one of these (an unknown future
variant to `WSQ_ERR_UNKNOWN`): `INVALID_STREAM` covers every way a stream can be malformed
(`NotWsq`, `UnexpectedEof`, `CorruptData`, ...). For the detail, `wsq_last_error_message()` gives
the Rust error's text (for instance "invalid segment 0xFFA4: ..."); it is for people and may be
reworded, so do not parse it.

**No panic crosses the boundary.** Every entry point runs inside `catch_unwind`; a panic gives
`WSQ_ERR_PANIC` (and the panic message on standard error, as Rust prints it). This needs unwinding,
so the library is built with `panic = "unwind"`, the default: do not set `panic = "abort"` when
you build it yourself. Corrupt streams fail with `WSQ_ERR_INVALID_STREAM`, they do not panic; the
codec is fuzzed for that.

## Options and ABI evolution

`wsq_decode_options` and `wsq_encode_options` start with `size_t struct_size`. Create them with
`wsq_decode_options_default()` / `wsq_encode_options_default()` (which set it to the `sizeof` of
the header you compiled against), change fields, and pass a pointer; NULL means defaults.

```c
wsq_encode_options options = wsq_encode_options_default();
options.bitrate = 0.75f;
options.ppi = 500;
options.profile = WSQ_PROFILE_OPTIMIZED;   /* or WSQ_PROFILE_CONFORMANT, the default */
```

This is what lets the structures grow: later versions append fields at the end, and

- a newer library reads only the `struct_size` bytes you declare and uses the default for the
  fields it knows beyond them, so a program built against an older header keeps working;
- an older library ignores the bytes beyond what it knows, so a program built against a newer
  header can still call it;
- a `struct_size` smaller than a `size_t` is `WSQ_ERR_INVALID_ARGUMENT`.

Always use the `_default()` functions: a zeroed structure is refused (its `struct_size` is 0).
`profile` is an `int32_t`, not the `wsq_profile` enum, so that a wrong value is an error code and
not undefined behaviour.

`max_pixels` defaults to 100 million. A few bytes of stream can claim an image of billions of
pixels, and decoding needs about 8 bytes per pixel: keep a limit for data you do not trust
(`WSQ_NO_PIXEL_LIMIT` removes it).

## ABI stability

- The C API is versioned with the `wsq-capi` crate (`wsq_version()`), by semantic versioning:
  while the version is 0.x a minor release may change the API, and from 1.0 only a major release
  may. A release that touches the interface says so in `CHANGELOG.md`.
- Within a version, and always: function names and signatures, the values of the status codes and
  of `wsq_profile`, and the layout of `wsq_bytes`, `wsq_info` and `wsq_image` (fields are never
  added to or reordered in these *output* structures; new information comes with new functions)
  do not change. Options structures only grow at the end, as above.
- The text of `wsq_status_message` and `wsq_last_error_message` may change in any release.
- The shared library has no SONAME or install name versioning (Cargo does not provide one). If
  you ship it next to your program, ship the one you tested; or link the static library.
- `wsq.h` is generated from `src/lib.rs` and checked in CI: a change to the interface that is
  not reflected in the header fails the build.

The codec itself follows the semantic versioning of the `wsq-codec` crate; its guarantees about
pixels and about the conformant encoder's bytes hold through this interface.

## Developing

The only `unsafe` code in the project is in `src/lib.rs` of this crate (the core crate has
`#![forbid(unsafe_code)]` and no dependencies). Every `unsafe` block has a `// SAFETY:` comment,
and clippy checks it (`undocumented_unsafe_blocks`).

```sh
cargo test -p wsq-capi          # unit tests, and the C program tests/roundtrip.c built with
                                # the system C compiler (cc, clang, or MSVC's cl) and run, linked
                                # to the shared library and to the static one
WSQ_C_SANITIZE=1 cargo test -p wsq-capi   # the same under AddressSanitizer and UBSan (GCC, Clang)

cargo install cbindgen --locked
cbindgen --config capi/cbindgen.toml --crate wsq-capi --output capi/include/wsq.h
```

After changing the interface, regenerate `include/wsq.h` as above and commit it; CI regenerates
it and fails on any difference. Keep the documentation of the `extern "C"` items written for C
programmers: it becomes the header's comments.
