//! Times encoding and decoding on generated fingerprint-like images.
//!
//! ```text
//! cargo bench --bench codec -- [THREADS] [RUNS] [--check]
//! ```
//!
//! Each figure is the fastest of RUNS runs (5 by default), so that it
//! tracks the code rather than the machine's noise. THREADS (1 by default)
//! is passed to the encoder and decoder options; 0 uses every core.
//!
//! The table also gives a hash of each encoded stream. Speeding the codec
//! up must not change it, whatever THREADS is: `--check` compares the
//! hashes with `benches/hashes.txt` and fails on a difference. The test
//! images use integer arithmetic only, so the hashes hold on every platform.

use std::error::Error;
use std::time::{Duration, Instant};
use std::{env, process};

use wsq_codec::{DecodeOptions, EncodeOptions};

/// Curved ridges with some noise, as in the tests. Integer arithmetic only,
/// so that the image is the same on every platform.
fn ridges(w: usize, h: usize) -> Vec<u8> {
    // Triangle wave of the given period (a multiple of 2), from 0 to period / 2.
    fn triangle(p: usize, period: usize) -> usize {
        (p % period).abs_diff(period / 2)
    }
    let mut state = 12345u32;
    let (cx, cy) = (w / 2, h * 6 / 10);
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let (dx, dy) = (x.abs_diff(cx) as u64, y.abs_diff(cy) as u64);
            let r = (dx * dx + dy * dy).isqrt() as usize;
            let phase = r / 3 + 4 * triangle(x / 5, 16);
            (40 + 6 * triangle(phase, 64) + (state >> 28) as usize) as u8
        })
        .collect()
}

/// FNV-1a, enough to tell two streams apart.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn fastest<T>(runs: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut best = None;
    let mut out = None;
    for _ in 0..runs {
        let start = Instant::now();
        let value = f();
        let elapsed = start.elapsed();
        if best.is_none_or(|b| elapsed < b) {
            best = Some(elapsed);
        }
        out = Some(value);
    }
    (best.unwrap(), out.unwrap())
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let check = args.iter().any(|a| a == "--check");
    // Cargo adds `--bench` when it runs this file.
    let args: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let threads = args.first().map(|a| a.parse()).transpose()?.unwrap_or(1);
    let runs = args.get(1).map(|a| a.parse()).transpose()?.unwrap_or(5);
    let expected = if check {
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/benches/hashes.txt"))?
    } else {
        String::new()
    };
    let mut mismatches = 0;
    if runs == 0 {
        return Err("RUNS must be at least 1".into());
    }
    let encode = EncodeOptions::default().with_threads(threads);
    let decode = DecodeOptions::default().with_threads(threads);

    println!("| image | encode | decode | stream hash |");
    println!("|---|---|---|---|");
    for (name, w, h) in [
        ("500 ppi rolled print", 800, 750),
        ("1000 ppi", 1600, 1500),
        ("1000 ppi palm", 5500, 8000),
    ] {
        let pixels = ridges(w, h);
        let (enc, wsq) = fastest(runs, || wsq_codec::encode(&pixels, w, h, &encode));
        let wsq = wsq?;
        let (dec, image) = fastest(runs, || wsq_codec::decode_with(&wsq, &decode));
        assert_eq!(image?.pixels.len(), pixels.len());
        let id = format!("{w}x{h} {:016x}", hash(&wsq));
        if check && !expected.lines().any(|l| l.trim() == id) {
            mismatches += 1;
        }
        println!(
            "| {name}, {w}×{h} | {:.1} ms | {:.1} ms | `{:016x}` |",
            enc.as_secs_f64() * 1e3,
            dec.as_secs_f64() * 1e3,
            hash(&wsq)
        );
    }
    if mismatches > 0 {
        return Err(format!("{mismatches} stream(s) differ from benches/hashes.txt").into());
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("bench: {e}");
        process::exit(1);
    }
}
