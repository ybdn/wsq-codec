# Contributing

Thanks for your interest in `wsq-codec`.

**Never attach or commit real fingerprints, or any file from NIST's certification test set.**
They are biometric data (a special category under GDPR article 9) or data NIST supplies on
request. Use synthetic images, or describe a stream with the output of
`cargo run --release --example wsq -- info FILE`.

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

1. On `develop`, set `version` in `Cargo.toml`, rename the `Unreleased` section of
   `CHANGELOG.md` to the version, and open a pull request to `main`.
2. Once it is merged, tag the merge commit and push the tag:
   `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The workflow checks that the tag matches `Cargo.toml` and that `CHANGELOG.md` has a section
   for it, runs the checks (Rust 1.85 included), publishes to crates.io through Trusted
   Publishing (OIDC, no stored token), and creates the GitHub release from the changelog
   section.
4. Merge `main` back into `develop`.

The workflow can also be started by hand with `dry_run` set (the default for a manual run): it
runs every check and `cargo publish --dry-run` without publishing anything.
