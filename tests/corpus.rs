//! Decodes a collection of real-world WSQ files, out of tree.
//!
//! Set `WSQ_CORPUS_DIR` to a directory of WSQ files, or of files with WSQ
//! streams inside them such as ANSI/NIST-ITL transactions, with reference
//! decodes next to them as the `check` command of the `wsq` example reads
//! them. The test then fails if a stream does not decode or differs from
//! its reference. Without the variable it does nothing, so that CI needs
//! no corpus.
//!
//! ```sh
//! WSQ_CORPUS_DIR=/path/to/corpus cargo test --release --test corpus -- --nocapture
//! ```

#[path = "../examples/wsq/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use std::fs;
use std::path::PathBuf;

use wsq_codec::{EncodeOptions, decode, encode};

#[test]
fn real_world_corpus() {
    let Some(dir) = std::env::var_os("WSQ_CORPUS_DIR") else {
        eprintln!("WSQ_CORPUS_DIR is not set; skipping the corpus");
        return;
    };
    corpus::check(&PathBuf::from(dir)).unwrap();
}

fn pattern(w: usize, h: usize, seed: usize) -> Vec<u8> {
    (0..w * h)
        .map(|i| (((i % w) * 7 + (i / w) * 3 + seed * 11) % 200 + 20) as u8)
        .collect()
}

/// A scratch directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("wsq-codec-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn harness_checks_files_and_embedded_streams() {
    let scratch = Scratch::new("corpus");
    let dir = &scratch.0;
    let streams: Vec<Vec<u8>> = (0..3)
        .map(|seed| {
            encode(
                &pattern(100 + seed, 90, seed),
                100 + seed,
                90,
                &EncodeOptions::default(),
            )
            .unwrap()
        })
        .collect();
    let pixels = |wsq: &[u8]| decode(wsq).unwrap().pixels;

    // A WSQ file with a raw reference.
    fs::write(dir.join("a.wsq"), &streams[0]).unwrap();
    fs::write(dir.join("a.raw"), pixels(&streams[0])).unwrap();
    // Two streams in a container, the second with a PGM reference.
    let mut eft = b"1.001:header\x1d".to_vec();
    eft.extend_from_slice(&streams[1]);
    eft.extend_from_slice(b"\x1cbetween records");
    eft.extend_from_slice(&streams[2]);
    eft.extend_from_slice(b"\x1c");
    fs::write(dir.join("b.eft"), &eft).unwrap();
    let mut pgm = b"P5\n102 90\n255\n".to_vec();
    pgm.extend_from_slice(&pixels(&streams[2]));
    fs::write(dir.join("b.2.pgm"), pgm).unwrap();

    let found = corpus::find_streams(&eft);
    assert_eq!(found.len(), 2);
    assert_eq!(&eft[found[0].clone()], &streams[1][..]);
    assert_eq!(&eft[found[1].clone()], &streams[2][..]);
    corpus::check(dir).unwrap();

    // A reference that differs by one pixel fails the check.
    let mut wrong = pixels(&streams[0]);
    wrong[17] ^= 1;
    fs::write(dir.join("a.raw"), wrong).unwrap();
    assert!(corpus::check(dir).is_err());
}
