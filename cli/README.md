# wsq-cli

The `wsq` command-line tool of [wsq-codec](https://github.com/ybdn/wsq-codec): encode and decode
WSQ fingerprint images (binary PGM on the other side), print a file's header, check a corpus of
real files and run the FBI certification tests locally.

```sh
cargo install wsq-cli
wsq --help
wsq encode finger.pgm finger.wsq 0.75 500
wsq decode finger.wsq finger.pgm
wsq info finger.wsq
```

See the [main README](https://github.com/ybdn/wsq-codec#readme) and `docs/certification.md`.
Licensed under MIT.
