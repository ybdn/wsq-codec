//! Timing harness for `wsq-codec`, the counterpart of `nbis_time.c`
//! (docs/nbis-comparison.md).
//!
//! ```text
//! wsq-nbis gen DIR
//! wsq-nbis encode RAW WIDTH HEIGHT BITRATE PPI THREADS [OUT.wsq]
//! wsq-nbis decode STREAM.wsq THREADS [OUT.raw]
//! wsq-nbis pgm2raw IN.pgm OUT.raw
//! ```
//!
//! `gen` writes the synthetic images as 8-bit raw files. `pgm2raw` is for the `WSQ_CORPUS_DIR`
//! mode of `run.sh`: it writes the pixels of a binary 8-bit PGM and prints `WIDTH HEIGHT`
//! (and nothing else: the harness never prints a path or a file name from the corpus). `encode` and `decode` do one call,
//! in memory, and print one line: `ms=… out_bytes=… fnv=… extra_rss_kb=…`. The time is that of
//! the call alone (file reading and writing excluded). `extra_rss_kb` is the peak resident set
//! of the process (`VmHWM`) minus the resident set just before the call, so the input
//! buffer is not counted. One call per process, so every call starts from the same state.
//! `nbis_time.c` prints the same line for NBIS.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use wsq_codec::{DecodeOptions, EncodeOptions, decode_with, encode};

/// Synthetic images, the same as `pattern` in wsq-codec's `tests/nbis.rs` and
/// `tests/data/make_fixtures.py`: integer arithmetic only, so identical on every platform.
fn pattern(kind: &str, w: usize, h: usize, seed: u32) -> Vec<u8> {
    let mut state = seed;
    let mut rand = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state >> 24
    };
    let (cx, cy) = (w / 3, h / 2);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let v = match kind {
                "rings" => {
                    let (dx, dy) = (x.abs_diff(cx), y.abs_diff(cy));
                    let r2 = dx * dx + dy * dy * 3 / 2 + x * y / 5;
                    let t = (r2 / 37) % 32;
                    30 + t.abs_diff(16) as u32 * 12 + (rand() & 15)
                }
                "noise" => rand(),
                "ridges" | "dark_ridges" => {
                    // "dark_ridges" has a mean of about 75 instead of 143: NBIS stops when the
                    // sum of the pixels passes 2^32 - 1, which a bright 5500x8000 image does.
                    let (base, gain) = if kind == "ridges" { (40, 6) } else { (20, 3) };
                    let triangle = |p: usize, period: usize| (p % period).abs_diff(period / 2);
                    let (cx, cy) = (w / 2, h * 6 / 10);
                    let (dx, dy) = (x.abs_diff(cx) as u64, y.abs_diff(cy) as u64);
                    let phase = (dx * dx + dy * dy).isqrt() as usize / 3 + 4 * triangle(x / 5, 16);
                    base + gain * triangle(phase, 64) as u32 + (rand() >> 4)
                }
                _ => (if (x / 7 + y / 5) % 2 == 1 { 200 } else { 40 }) + (rand() & 7),
            };
            out.push(v as u8);
        }
    }
    out
}

/// The header of a binary 8-bit PGM (`P5`, comments allowed): width, height and the offset of
/// the pixels.
fn parse_pgm(data: &[u8]) -> Result<(usize, usize, usize), Box<dyn Error>> {
    let mut pos = 0;
    let mut token = || -> Result<String, Box<dyn Error>> {
        loop {
            match data.get(pos) {
                Some(b'#') => {
                    while data.get(pos).is_some_and(|&b| b != b'\n') {
                        pos += 1;
                    }
                }
                Some(b) if b.is_ascii_whitespace() => pos += 1,
                Some(_) => break,
                None => return Err("truncated PGM header".into()),
            }
        }
        let start = pos;
        while data.get(pos).is_some_and(|b| !b.is_ascii_whitespace()) {
            pos += 1;
        }
        Ok(String::from_utf8_lossy(&data[start..pos]).into_owned())
    };
    if token()? != "P5" {
        return Err("not a binary PGM".into());
    }
    let (w, h, max): (usize, usize, usize) =
        (token()?.parse()?, token()?.parse()?, token()?.parse()?);
    if max != 255 {
        return Err("not an 8-bit PGM".into());
    }
    // A single whitespace byte separates the header from the pixels.
    let offset = pos + 1;
    if w == 0 || h == 0 || data.len() < offset + w * h {
        return Err("bad PGM size".into());
    }
    Ok((w, h, offset))
}

fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xCBF2_9CE4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3)
    })
}

/// A `kB` figure of `/proc/self/status`, such as `VmRSS` or `VmHWM`.
fn status_kb(key: &str) -> Result<u64, Box<dyn Error>> {
    let status = fs::read_to_string("/proc/self/status")?;
    let line = status
        .lines()
        .find(|l| l.starts_with(key))
        .ok_or("missing key in /proc/self/status")?;
    Ok(line.split_whitespace().nth(1).ok_or("bad line")?.parse()?)
}

fn report(ms: f64, out: &[u8], rss_before: u64) -> Result<(), Box<dyn Error>> {
    let extra = status_kb("VmHWM:")?.saturating_sub(rss_before);
    println!(
        "ms={ms:.3} out_bytes={} fnv={:016x} extra_rss_kb={extra}",
        out.len(),
        fnv1a(out)
    );
    Ok(())
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    match args.first().map(String::as_str) {
        Some("gen") if args.len() == 2 => {
            let dir = Path::new(&args[1]);
            fs::create_dir_all(dir)?;
            for (kind, w, h) in [
                ("ridges", 800, 750),
                ("ridges", 1600, 1500),
                ("noise", 1600, 1500),
                ("dark_ridges", 5500, 8000),
            ] {
                let pixels = pattern(kind, w, h, (w * h) as u32);
                fs::write(dir.join(format!("{kind}_{w}x{h}.raw")), pixels)?;
            }
            Ok(())
        }
        Some("pgm2raw") if args.len() == 3 => {
            let data = fs::read(&args[1])?;
            let (w, h, offset) = parse_pgm(&data)?;
            fs::write(&args[2], &data[offset..offset + w * h])?;
            println!("{w} {h}");
            Ok(())
        }
        Some("encode") if args.len() == 7 || args.len() == 8 => {
            let pixels = fs::read(&args[1])?;
            let (w, h): (usize, usize) = (args[2].parse()?, args[3].parse()?);
            let options = EncodeOptions::default()
                .with_bitrate(args[4].parse()?)
                .with_ppi(Some(args[5].parse()?))
                .with_threads(args[6].parse()?);
            let before = status_kb("VmRSS:")?;
            let start = Instant::now();
            let wsq = encode(&pixels, w, h, &options)?;
            let ms = start.elapsed().as_secs_f64() * 1e3;
            report(ms, &wsq, before)?;
            if let Some(out) = args.get(7) {
                fs::write(out, &wsq)?;
            }
            Ok(())
        }
        Some("decode") if args.len() == 3 || args.len() == 4 => {
            let wsq = fs::read(&args[1])?;
            let options = DecodeOptions::default().with_threads(args[2].parse()?);
            let before = status_kb("VmRSS:")?;
            let start = Instant::now();
            let image = decode_with(&wsq, &options)?;
            let ms = start.elapsed().as_secs_f64() * 1e3;
            report(ms, &image.pixels, before)?;
            if let Some(out) = args.get(3) {
                fs::write(out, &image.pixels)?;
            }
            Ok(())
        }
        _ => Err("usage: wsq-nbis gen DIR | encode RAW W H BITRATE PPI THREADS [OUT] | decode WSQ THREADS [OUT] | pgm2raw IN OUT".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wsq-nbis: {e}");
            ExitCode::FAILURE
        }
    }
}
