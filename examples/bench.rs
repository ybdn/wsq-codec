//! Times encoding and decoding on generated fingerprint-like images.
//!
//! ```text
//! cargo run --release --example bench -- [THREADS] [RUNS]
//! ```
//!
//! Each figure is the fastest of RUNS runs (5 by default), so that it
//! tracks the code rather than the machine's noise. THREADS (1 by default)
//! is passed to the encoder and decoder options; 0 uses every core.

use std::error::Error;
use std::time::{Duration, Instant};
use std::{env, process};

use wsq_codec::{DecodeOptions, EncodeOptions};

/// Curved ridges with some noise, as in the tests.
fn ridges(w: usize, h: usize) -> Vec<u8> {
    let mut state = 12345u32;
    (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let r = ((x - w as f32 / 2.0).powi(2) + (y - h as f32 * 0.6).powi(2)).sqrt();
            let v =
                128.0 + 90.0 * (r / 3.0 + (x / 41.0).sin()).sin() + f32::from((state >> 28) as u8);
            v.clamp(0.0, 255.0) as u8
        })
        .collect()
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
    let threads = args.first().map(|a| a.parse()).transpose()?.unwrap_or(1);
    let runs = args.get(1).map(|a| a.parse()).transpose()?.unwrap_or(5);
    if runs == 0 {
        return Err("RUNS must be at least 1".into());
    }
    let encode = EncodeOptions::default().with_threads(threads);
    let decode = DecodeOptions::default().with_threads(threads);

    println!("| image | encode | decode |");
    println!("|---|---|---|");
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
        println!(
            "| {name}, {w}×{h} | {:.1} ms | {:.1} ms |",
            enc.as_secs_f64() * 1e3,
            dec.as_secs_f64() * 1e3
        );
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
