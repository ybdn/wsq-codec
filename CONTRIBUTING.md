# Contributing

Thanks for your interest in `wsq-codec`.

**Never attach or commit real fingerprints, or any file from NIST's certification test set.**
They are biometric data (a special category under GDPR article 9) or data NIST supplies on
request. Use synthetic images, or describe a stream with the output of
`wsq info FILE`.

Report vulnerabilities privately, as [SECURITY.md](SECURITY.md) describes, not in an issue.

## Branches

Work goes into `develop` through pull requests, from branches named `feat/...`, `fix/...`,
`docs/...` or `ci/...`. `main` only receives releases, from `develop`.

## Before a commit

The Minimum Supported Rust Version is 1.85; CI also runs the tests on it.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --all-targets --features image -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo test --all-targets --features image
cargo test --doc --features image
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features image --workspace
```

## The certified crate

`certified/` is the `wsq-certified` crate: the decoder, the conformant encoder and `inspect`,
which an FBI certificate covers (`docs/certification.md`, `certified/README.md`). A certificate
names one version of that code and one compiler, so:

- **Any change under `certified/` is a new version to certify**, even a patch or a comment in
  the code. Do not touch it in a pull request about something else, and discuss a change before
  writing it. If it is needed, raise `version` in `certified/Cargo.toml`, add a section to
  `certified/CHANGELOG.md`, and update the exact pins (`wsq-certified = { version = "=X.Y.Z" }`)
  in `Cargo.toml` and `cli/Cargo.toml`. CI (`certified-version`, pull requests only) fails a
  change to a file under `certified/` (Markdown files excepted) that does not raise the version.
- The optimized encoder, bindings, `image` support and anything not certified belong outside
  `certified/` (`wsq-codec` and the other crates); they use the certified crate's public API,
  plus the hidden `for_wsq_codec` hook for the optimized rate search.
- The compiler is pinned in `certified/rust-toolchain.toml` and rustup applies it to commands run
  in `certified/`; run the certified tests from there (`cd certified && cargo test --release`)
  to use it. It is not at the repository root so that the other CI jobs, the MSRV one included,
  keep their own toolchains. Raise the pin only with a new certification.

From 1.0 the public API is frozen (`docs/api-1.0.md`): CI runs `cargo semver-checks check-release
-p wsq-codec --all-features` against the latest release on crates.io (install with `cargo install
--locked cargo-semver-checks`), and the same for `wsq-certified` once it is published, since the
tool cannot see through the items `wsq-codec` re-exports from it. A change it reports as breaking
needs a discussion first, and a major version. Adding to the API is fine; mark new public structs and enums `#[non_exhaustive]`
and document the errors a new function returns (clippy enforces it).

The WebAssembly package in `wasm/` (see [wasm/README.md](wasm/README.md)) is checked apart; it needs
`rustup target add wasm32-unknown-unknown` and `cargo install wasm-pack --locked`, and Node.js:

```sh
cargo clippy -p wsq-wasm --target wasm32-unknown-unknown --all-targets -- -D warnings
cargo test -p wsq-wasm
wasm-pack test --node wasm
wasm/build.sh && node wasm/scripts/check-package.mjs wasm/pkg wasm/demo/sample.wsq
python3 -m http.server -d wasm/demo   # the demo, at http://localhost:8000
```

The demo must keep sending nothing: no external resource, and a Content-Security-Policy that
only allows the page's own origin. Its samples are synthetic (`wasm/demo/make_sample.py`);
never add a real fingerprint there.

The C API (`capi/`, #50) has its own checks: `cargo clippy -p wsq-capi --all-targets -- -D warnings`
and `cargo test -p wsq-capi` (which compiles and runs `capi/tests/roundtrip.c` with the system C
compiler). After changing an `extern "C"` item, regenerate the committed header with `cargo
install cbindgen --version 0.29.4 --locked` and `cbindgen --config capi/cbindgen.toml --crate
wsq-capi --output capi/include/wsq.h`: CI fails on any difference. See `capi/README.md`.

The Python bindings (`python/`, PyO3 and maturin; the distribution is `wsq-codec` on PyPI, the
import `wsq_codec`) are a workspace member that the commands above leave out (select it with
`-p wsq-python`). To check a change to them, in a virtual environment:

```sh
cargo clippy -p wsq-python --all-targets -- -D warnings
pip install maturin numpy pytest
cd python && maturin develop --release && pytest
```

`python/tests` holds the round-trip tests; they compare the conformant encoder with the NBIS
fixtures in `tests/data`. The core crate takes no dependency for the bindings: PyO3 and `numpy`
are dependencies of `wsq-python` only.

Optionally, `cargo deny check` (install with `cargo install --locked cargo-deny`) runs the
supply-chain checks CI runs.

The conformant encoder must keep `certified/tests/nbis.rs` passing: its output stays
byte-identical to NBIS's on the fixtures. `certified/src/math.rs` uses no libm and no FMA, so that
the output is the same on every platform. A change that alters an encoded byte needs a discussion
first (and is a new certification).

Add a line to `CHANGELOG.md` for any user-visible change. Claims about speed, quality or
compatibility must come with a measurement or a test.

## Commit messages

One short line in the imperative, prefixed by its kind and followed by the issue number when
there is one:

```
fix: a failed thread spawn no longer panics (#22)
docs: ...    ci: ...    feat: ...    chore: ...
```

## Releasing

Maintainers only. A release is published by the `Release` workflow when a `v*` tag is pushed
on `main`; nobody publishes from a personal machine.

1. On `develop`, set `version` in `Cargo.toml` (`wsq-codec`; the version of `wsq-certified`
   changes only with a certification, see above), in `wasm/Cargo.toml` (the npm package takes
   its version from it) and in `python/Cargo.toml` (the workflow refuses a tag when they differ;
   the Python package takes the crate's version), and the `version` of the `path` dependencies on
   `wsq-codec` in `cli/Cargo.toml` and `wasm/Cargo.toml`; rename the `Unreleased` section of
   `CHANGELOG.md` to the version (a line `## X.Y.Z`, as the workflow looks for it exactly) and
   leave an empty `## Unreleased` above it, and open a pull request to `main`. A minimum Rust
   version bump is a minor release, noted in the changelog.
2. Once it is merged, tag the merge commit and push the tag:
   `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The workflow checks that the tag matches `Cargo.toml`, that `CHANGELOG.md` has a section
   for it and that `wsq-codec` and `wsq-cli` pin the version of `wsq-certified`, runs the checks
   (Rust 1.85 included, and the certified crate on its pinned compiler), publishes to crates.io
   through Trusted Publishing (OIDC, no stored token), and creates the GitHub release from the
   changelog section. It publishes `wsq-certified` first, when that version is not on crates.io
   yet, then `wsq-codec`. (The workflow does not publish `wsq-cli`.) In parallel, the `npm` job builds the WebAssembly package (`wasm/`) and
   publishes it to npm as `wsq-codec`, with provenance, after checking that `wasm/Cargo.toml` has
   the same version; the Python wheels (Linux x86-64 and AArch64, macOS Apple silicon and Intel,
   Windows x64) and the source distribution are built, checked with `twine check` and published to
   PyPI through Trusted Publishing from the `pypi` environment; and the C library is attached to
   the GitHub release. The `Pages` workflow redeploys the demo when `main` is updated.
4. Merge `main` back into `develop`.

**Trusted Publishing for `wsq-certified`.** Each crate has its own Trusted Publishing
configuration on crates.io. Before the first release that includes `wsq-certified`, the owner
has to publish the crate (crates.io lets one configure Trusted Publishing for an existing crate;
if it does not offer a pending configuration for a new one, the first `wsq-certified` upload is a
manual `cargo publish -p wsq-certified` with a temporary token) and then add the publisher in
the crate's settings: repository `ybdn-ei/wsq-codec`, workflow `release.yml`, the same settings as
for `wsq-codec`. Until then the `Publish wsq-certified` step fails and nothing is published.

The workflow can also be started by hand with `dry_run` set (the default for a manual run): it
runs every check and `cargo publish --dry-run` without publishing anything.

### One-time setup of npm and GitHub Pages

1. **npm.** The package is `wsq-codec` (unscoped, like the crate; the name was free when this was
   written). npm Trusted Publishing can only be configured on a package that exists, so the
   first release is published with a token: create a granular access token on npmjs.com (publish
   rights on new packages, short expiry) and store it as the repository secret `NPM_TOKEN`; the
   `npm` job of `release.yml` uses it when it is set. Right after the first publish, open the
   package's Settings on npmjs.com, add a Trusted Publisher (GitHub Actions, repository
   `ybdn-ei/wsq-codec`, workflow `release.yml`, no environment), then delete the `NPM_TOKEN`
   secret and revoke the token. Later releases authenticate through OIDC and carry a provenance
   statement. (An organization scope such as `@ybdn-ei/wsq-codec` would need the organization on
   npm; change `name` in `wasm/package.template.json` and the README if you prefer it.)
2. **GitHub Pages.** In the repository's Settings > Pages, set Source to "GitHub Actions". The
   `Pages` workflow then deploys `wasm/demo` on every push to `main` (or when started by hand),
   at <https://ybdn-ei.github.io/wsq-codec/>.

### One-time setup for PyPI

Maintainers do this once, before the first tag that publishes the Python package. Nothing is
stored as a secret: PyPI trusts the workflow through OIDC, as crates.io does.

1. On GitHub, in the repository's Settings, Environments, create an environment named `pypi`
   (optionally with required reviewers, so that a tag waits for approval before publishing, and
   with deployment restricted to `v*` tags).
2. On PyPI, log in with the account that will own the project and, in Account settings,
   Publishing, add a *pending publisher* (the project does not exist until the first upload):
   project name `wsq-codec`, owner `ybdn-ei`, repository `wsq-codec`, workflow name
   `release.yml`, environment name `pypi`. After the first release the pending publisher
   becomes a normal Trusted Publisher of the project.
3. Check that the name is still free (`curl -s -o /dev/null -w '%{http_code}\n'
   https://pypi.org/pypi/wsq-codec/json` prints 404 while it is) and that the project can be
   created: a pending publisher does not reserve the name.

A manual run of the `Release` workflow builds the wheels and the source distribution and checks
them, but never publishes.
