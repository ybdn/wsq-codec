//! Runs the `wsq` binary: round trip on a synthetic image, exit codes, `--help`.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::{env, fs};

fn wsq(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wsq"))
        .args(args)
        .output()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("wsq-cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn pgm(w: usize, h: usize) -> Vec<u8> {
    let mut out = format!("P5\n{w} {h}\n255\n").into_bytes();
    out.extend((0..w * h).map(|i| (((i % w) * 5 + (i / w) * 3) % 200 + 20) as u8));
    out
}

#[test]
fn encode_decode_round_trip() {
    let dir = scratch("roundtrip");
    let (src, wsq_file, back) = (dir.join("a.pgm"), dir.join("a.wsq"), dir.join("b.pgm"));
    let original = pgm(128, 96);
    fs::write(&src, &original).unwrap();

    let out = wsq(&[
        "encode",
        src.to_str().unwrap(),
        wsq_file.to_str().unwrap(),
        "2.25",
        "500",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = wsq(&["info", wsq_file.to_str().unwrap()]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("128x96") && text.contains("500 ppi"),
        "{text}"
    );

    let out = wsq(&["decode", wsq_file.to_str().unwrap(), back.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let decoded = fs::read(&back).unwrap();
    let header = b"P5\n128 96\n255\n";
    assert!(decoded.starts_with(header));
    let (a, b) = (&decoded[header.len()..], &original[header.len()..]);
    assert_eq!(a.len(), b.len());
    let mean: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| (*x as f64 - *y as f64).abs())
        .sum::<f64>()
        / a.len() as f64;
    assert!(mean < 8.0, "mean absolute error {mean}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn help_and_version_succeed() {
    for flag in ["--help", "-h"] {
        let out = wsq(&[flag]);
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("Usage:"));
    }
    let out = wsq(&["--version"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("wsq "));
}

#[test]
fn exit_codes() {
    let out = wsq(&[]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage:"));
    assert_eq!(wsq(&["frobnicate"]).status.code(), Some(64));

    let dir = scratch("errors");
    let missing = dir.join("missing.wsq");
    let out = wsq(&["info", missing.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("wsq: "));

    let junk = dir.join("junk.wsq");
    fs::write(&junk, b"not a wsq stream").unwrap();
    let target = dir.join("out.pgm");
    let out = wsq(&["decode", junk.to_str().unwrap(), target.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!target.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn encode_profile_optimized_meets_the_size() {
    let dir = scratch("profile");
    let (src, conformant, optimized) = (dir.join("a.pgm"), dir.join("c.wsq"), dir.join("o.wsq"));
    fs::write(&src, pgm(256, 256)).unwrap();
    let encode = |args: &[&str], dst: &PathBuf| {
        let mut full = vec!["encode"];
        full.extend_from_slice(args);
        full.extend([src.to_str().unwrap(), dst.to_str().unwrap(), "1.5"]);
        wsq(&full)
    };
    assert!(encode(&[], &conformant).status.success());
    assert!(
        encode(&["--profile", "conformant"], &conformant)
            .status
            .success()
    );
    let out = encode(&["--profile=optimized"], &optimized);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let target = 1.5 * 256.0 * 256.0 / 8.0;
    let len = fs::metadata(&optimized).unwrap().len() as f64;
    assert!(
        len <= target && len >= target * 0.995,
        "{len} against {target}"
    );
    assert_ne!(
        fs::read(&conformant).unwrap(),
        fs::read(&optimized).unwrap()
    );
    let back = dir.join("b.pgm");
    let out = wsq(&[
        "decode",
        optimized.to_str().unwrap(),
        back.to_str().unwrap(),
    ]);
    assert!(out.status.success());
}

#[test]
fn encode_rejects_an_unknown_profile() {
    for args in [&["--profile", "fast"][..], &["--profile"], &["--profile=x"]] {
        let mut full = vec!["encode"];
        full.extend_from_slice(args);
        full.extend(["in.pgm", "out.wsq"]);
        let out = wsq(&full);
        assert_eq!(out.status.code(), Some(64), "{args:?}");
    }
}
