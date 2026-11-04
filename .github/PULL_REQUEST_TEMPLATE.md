<!--
Never attach or commit real fingerprints or NIST's certification data. They are biometric
data (GDPR article 9). Fixtures are synthetic.
-->

## What and why

Closes #

## Checks

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`, with and without `--features image`
- [ ] `cargo test --all-targets` and `cargo test --doc`, with and without `--features image`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features image`
- [ ] `tests/nbis.rs` still passes unchanged (the conformant encoder stays byte-identical)
- [ ] `CHANGELOG.md` updated for user-visible changes
- [ ] No real fingerprint or NIST certification file added
