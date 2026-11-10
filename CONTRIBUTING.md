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
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features image -- -D warnings
cargo test --all-targets
cargo test --doc
cargo test --all-targets --features image
cargo test --doc --features image
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features image
```

From 1.0 the public API is frozen (`docs/api-1.0.md`): CI runs `cargo semver-checks check-release
-p wsq-codec --all-features` against the latest release on crates.io (install with `cargo install
--locked cargo-semver-checks`). A change it reports as breaking needs a discussion first, and a
major version. Adding to the API is fine; mark new public structs and enums `#[non_exhaustive]`
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

Optionally, `cargo deny check` (install with `cargo install --locked cargo-deny`) runs the
supply-chain checks CI runs.

The conformant encoder must keep `tests/nbis.rs` passing: its output stays byte-identical to
NBIS's on the fixtures. `src/math.rs` uses no libm and no FMA, so that the output is the same
on every platform. A change that alters an encoded byte needs a discussion first.

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

1. On `develop`, set `version` in `Cargo.toml` (and in `wasm/Cargo.toml`, which the npm package
   takes its version from, and the `version` of the `path` dependencies on `wsq-codec` in
   `cli/Cargo.toml` and `wasm/Cargo.toml`), rename the `Unreleased` section of `CHANGELOG.md` to
   the version (a line `## X.Y.Z`, as the workflow looks for it exactly) and leave an empty
   `## Unreleased` above it, and open a pull request to `main`. A minimum Rust version bump is a
   minor release, noted in the changelog.
2. Once it is merged, tag the merge commit and push the tag:
   `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The workflow checks that the tag matches `Cargo.toml` and that `CHANGELOG.md` has a section
   for it, runs the checks (Rust 1.85 included), publishes to crates.io through Trusted
   Publishing (OIDC, no stored token), and creates the GitHub release from the changelog
   section. In parallel, the `npm` job builds the WebAssembly package (`wasm/`) and publishes it
   to npm as `wsq-codec`, with provenance, after checking that `wasm/Cargo.toml` has the same
   version. The `Pages` workflow redeploys the demo when `main` is updated.
4. Merge `main` back into `develop`.

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
