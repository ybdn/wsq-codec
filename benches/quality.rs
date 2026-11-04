//! Measures what the encoder delivers: the actual bit rate against the requested one, and the
//! quality of the reconstruction (PSNR and SSIM), with the time taken.
//!
//! ```text
//! cargo bench --bench quality -- [--rates 0.5,0.75,1.0,2.25] [--runs N] [--threads N]
//!                                [--large] [--csv FILE] [--export DIR]
//!                                [--nbis BIN_DIR | --nbis-streams DIR]
//! ```
//!
//! The images are generated with integer arithmetic only (the same generator as
//! `tests/nbis.rs`, whose images have NBIS references), so the figures do not depend on the
//! platform. `--large` adds a 1600x1500 image. Setting `WSQ_QUALITY_DIR` to a directory of 8-bit
//! binary PGM files (P5, at least 81x81) adds them under their file names; the directory stays
//! outside the repository and nothing in it is ever printed beyond the figures.
//!
//! Every image is encoded at every rate at 500 ppi, decoded again, and compared with the
//! original on the whole image. The figures are:
//!
//! - bit rate: stream length in bits over pixels, and the file size;
//! - PSNR: 10 log10(255^2 / MSE) over all pixels, `inf` if the reconstruction is exact;
//! - SSIM: mean over 8x8 uniform windows at stride 4 (C1 = (0.01 * 255)^2, C2 = (0.03 * 255)^2).
//!   It is not the Gaussian-window SSIM of the literature: compare it only with itself;
//! - encoding and decoding time, the fastest of RUNS runs (3 by default).
//!
//! Comparison with NBIS, which never links to this crate, in one of two ways:
//!
//! - `--nbis BIN_DIR`: `BIN_DIR` holds NBIS 5.0.0's `cwsq` and `dwsq` (build procedure in
//!   `tests/data/README.md`). Each image is written as a raw file in a temporary directory,
//!   encoded by `cwsq` and decoded by `dwsq`, and the stream and the reconstruction are measured
//!   like this crate's own.
//! - `--nbis-streams DIR`: `DIR` holds streams NBIS wrote earlier, named `IMAGE_RATE.wsq` (for
//!   example `ridges_0.75.wsq`, RATE as spelled in `--rates`). They are decoded with this crate.
//!   `--export DIR` writes the images as `IMAGE.pgm` for that purpose.
//!
//! In both, the conformant encoder is expected to match NBIS byte for byte: a difference in the
//! stream or in the reconstruction is reported and makes the run fail. Without `--nbis*`
//! nothing is judged.
//!
//! The table goes to the standard output; `--csv FILE` also writes every measurement as CSV.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{env, fs, process};

use wsq_codec::{DecodeOptions, EncodeOptions};

const PPI: u32 = 500;

/// Synthetic images, the same as `pattern` in `tests/nbis.rs` and `make_fixtures.py`.
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
                "ridges" => {
                    let triangle = |p: usize, period: usize| (p % period).abs_diff(period / 2);
                    let (cx, cy) = (w / 2, h * 6 / 10);
                    let (dx, dy) = (x.abs_diff(cx) as u64, y.abs_diff(cy) as u64);
                    let phase = (dx * dx + dy * dy).isqrt() as usize / 3 + 4 * triangle(x / 5, 16);
                    40 + 6 * triangle(phase, 64) as u32 + (rand() >> 4)
                }
                _ => (if (x / 7 + y / 5) % 2 == 1 { 200 } else { 40 }) + (rand() & 7),
            };
            out.push(v as u8);
        }
    }
    out
}

struct Source {
    name: String,
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

fn synthetic(large: bool) -> Vec<Source> {
    let mut sets = vec![
        ("ridges_800x750", "ridges", 800, 750),
        ("rings_800x750", "rings", 800, 750),
        ("noise_800x750", "noise", 800, 750),
        ("checker_320x288", "checker", 320, 288),
    ];
    if large {
        sets.push(("ridges_1600x1500", "ridges", 1600, 1500));
    }
    sets.into_iter()
        .map(|(name, kind, width, height)| Source {
            name: name.to_string(),
            width,
            height,
            pixels: pattern(kind, width, height, 1),
        })
        .collect()
}

/// Parses a binary PGM with a maximum value of 255.
fn read_pgm(data: &[u8]) -> Result<(usize, usize, Vec<u8>), Box<dyn Error>> {
    let mut fields = Vec::new();
    let mut pos = 0;
    while fields.len() < 4 {
        match data.get(pos) {
            None => return Err("truncated PGM header".into()),
            Some(b'#') => {
                while data.get(pos).is_some_and(|&b| b != b'\n') {
                    pos += 1;
                }
            }
            Some(b) if b.is_ascii_whitespace() => pos += 1,
            Some(_) => {
                let start = pos;
                while data.get(pos).is_some_and(|b| !b.is_ascii_whitespace()) {
                    pos += 1;
                }
                fields.push(std::str::from_utf8(&data[start..pos])?);
            }
        }
    }
    if fields[0] != "P5" || fields[3] != "255" {
        return Err("only 8-bit binary PGM (P5) is supported".into());
    }
    let (width, height): (usize, usize) = (fields[1].parse()?, fields[2].parse()?);
    let pixels = data
        .get(pos + 1..pos + 1 + width * height)
        .ok_or("truncated PGM data")?;
    Ok((width, height, pixels.to_vec()))
}

fn local_corpus() -> Result<Vec<Source>, Box<dyn Error>> {
    let Some(dir) = env::var_os("WSQ_QUALITY_DIR") else {
        return Ok(Vec::new());
    };
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pgm")))
        .collect();
    paths.sort();
    let mut sources = Vec::new();
    for path in paths {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let (width, height, pixels) = read_pgm(&fs::read(&path)?)?;
        sources.push(Source {
            name: stem.into_owned(),
            width,
            height,
            pixels,
        });
    }
    Ok(sources)
}

/// PSNR in dB over all pixels, infinite if the images are equal.
fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| u64::from(x.abs_diff(y)).pow(2))
        .sum();
    if sum == 0 {
        return f64::INFINITY;
    }
    10.0 * (255.0 * 255.0 * a.len() as f64 / sum as f64).log10()
}

/// Mean SSIM over 8x8 windows at stride 4 (images smaller than a window count as one).
fn ssim(a: &[u8], b: &[u8], width: usize, height: usize) -> f64 {
    const WIN: usize = 8;
    const STRIDE: usize = 4;
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let n = (WIN * WIN) as f64;
    let (mut total, mut count) = (0.0, 0u64);
    for y in (0..=height.saturating_sub(WIN)).step_by(STRIDE) {
        for x in (0..=width.saturating_sub(WIN)).step_by(STRIDE) {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for j in y..y + WIN {
                let row = j * width + x;
                for (&p, &q) in a[row..row + WIN].iter().zip(&b[row..row + WIN]) {
                    let (p, q) = (u64::from(p), u64::from(q));
                    sa += p;
                    sb += q;
                    saa += p * p;
                    sbb += q * q;
                    sab += p * q;
                }
            }
            let (ma, mb) = (sa as f64 / n, sb as f64 / n);
            let va = saa as f64 / n - ma * ma;
            let vb = sbb as f64 / n - mb * mb;
            let cov = sab as f64 / n - ma * mb;
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            count += 1;
        }
    }
    total / count as f64
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

/// What one stream and its reconstruction measure.
struct Measure {
    bytes: usize,
    bpp: f64,
    psnr: f64,
    ssim: f64,
    /// Encoding and decoding time in milliseconds, for this crate's own measurements.
    times: Option<(f64, f64)>,
}

fn measure(src: &Source, stream_len: usize, recon: &[u8], times: Option<(f64, f64)>) -> Measure {
    Measure {
        bytes: stream_len,
        bpp: stream_len as f64 * 8.0 / (src.width * src.height) as f64,
        psnr: psnr(&src.pixels, recon),
        ssim: ssim(&src.pixels, recon, src.width, src.height),
        times,
    }
}

struct Nbis {
    measure: Measure,
    same_stream: bool,
    same_pixels: bool,
}

struct Row {
    image: String,
    width: usize,
    height: usize,
    rate: String,
    target: f64,
    ours: Measure,
    nbis: Option<Nbis>,
}

enum Mode {
    None,
    /// Directory holding `cwsq` and `dwsq`.
    Run(PathBuf),
    /// Directory of streams NBIS wrote earlier.
    Streams(PathBuf),
}

struct Args {
    rates: Vec<String>,
    runs: usize,
    threads: usize,
    large: bool,
    csv: Option<PathBuf>,
    export: Option<PathBuf>,
    mode: Mode,
}

fn parse_args(args: &[String]) -> Result<Args, Box<dyn Error>> {
    let mut out = Args {
        rates: ["0.5", "0.75", "1.0", "2.25"].map(String::from).to_vec(),
        runs: 3,
        threads: 1,
        large: false,
        csv: None,
        export: None,
        mode: Mode::None,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| -> Result<&String, Box<dyn Error>> {
            it.next()
                .ok_or_else(|| format!("{name} needs a value").into())
        };
        match arg.as_str() {
            "--bench" => {} // added by cargo
            "--large" => out.large = true,
            "--rates" => {
                out.rates = value("--rates")?.split(',').map(String::from).collect();
            }
            "--runs" => out.runs = value("--runs")?.parse()?,
            "--threads" => out.threads = value("--threads")?.parse()?,
            "--csv" => out.csv = Some(value("--csv")?.into()),
            "--export" => out.export = Some(value("--export")?.into()),
            "--nbis" => out.mode = Mode::Run(value("--nbis")?.into()),
            "--nbis-streams" => out.mode = Mode::Streams(value("--nbis-streams")?.into()),
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    if out.runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    for rate in &out.rates {
        let r: f32 = rate.parse().map_err(|_| format!("bad rate {rate}"))?;
        if !(r > 0.0 && r.is_finite()) {
            return Err(format!("bad rate {rate}").into());
        }
    }
    Ok(out)
}

/// Runs an NBIS tool, failing with its status and name.
fn nbis_tool(bin: &Path, tool: &str, args: &[&std::ffi::OsStr]) -> Result<(), Box<dyn Error>> {
    let status = process::Command::new(bin.join(tool))
        .args(args)
        .stdout(process::Stdio::null())
        .status()
        .map_err(|e| format!("cannot run {}: {e}", bin.join(tool).display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{tool} failed with {status}").into())
    }
}

/// Stream and reconstruction from NBIS's own `cwsq` and `dwsq`.
fn nbis_run(
    bin: &Path,
    work: &Path,
    src: &Source,
    rate: &str,
) -> Result<(Vec<u8>, Vec<u8>), Box<dyn Error>> {
    let dir = work.join(format!("r{rate}"));
    fs::create_dir_all(&dir)?;
    let raw = dir.join(format!("{}.raw", src.name));
    fs::write(&raw, &src.pixels)?;
    let geometry = format!("{},{},8,{PPI}", src.width, src.height);
    nbis_tool(
        bin,
        "cwsq",
        &[
            rate.as_ref(),
            "wsq".as_ref(),
            raw.as_os_str(),
            "-raw_in".as_ref(),
            geometry.as_ref(),
        ],
    )?;
    let wsq = dir.join(format!("{}.wsq", src.name));
    let stream = fs::read(&wsq)?;
    fs::remove_file(&raw)?;
    nbis_tool(
        bin,
        "dwsq",
        &["raw".as_ref(), wsq.as_os_str(), "-raw_out".as_ref()],
    )?;
    let recon = fs::read(dir.join(format!("{}.raw", src.name)))?;
    Ok((stream, recon))
}

fn pgm(src: &Source) -> Vec<u8> {
    let mut out = format!("P5\n{} {}\n255\n", src.width, src.height).into_bytes();
    out.extend_from_slice(&src.pixels);
    out
}

fn fmt_psnr(v: f64) -> String {
    if v.is_infinite() {
        "inf".into()
    } else {
        format!("{v:.2}")
    }
}

fn csv_line(row: &Row, source: &str, m: &Measure, identical: Option<bool>) -> String {
    let ms = |t: Option<f64>| t.map_or(String::new(), |t| format!("{t:.1}"));
    let times = m.times;
    format!(
        "{source},{},{},{},{},{},{},{:.4},{},{:.5},{},{},{}",
        row.image,
        row.width,
        row.height,
        row.rate,
        row.target,
        m.bytes,
        m.bpp,
        fmt_psnr(m.psnr),
        m.ssim,
        ms(times.map(|t| t.0)),
        ms(times.map(|t| t.1)),
        identical.map_or(String::new(), |i| i.to_string()),
    )
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let args = parse_args(args)?;
    let mut sources = synthetic(args.large);
    sources.extend(local_corpus()?);
    if let Some(dir) = &args.export {
        fs::create_dir_all(dir)?;
        for src in &sources {
            fs::write(dir.join(format!("{}.pgm", src.name)), pgm(src))?;
        }
    }
    let work = env::temp_dir().join(format!("wsq-quality-{}", process::id()));
    let encode_options = |rate: f32| {
        EncodeOptions::default()
            .with_bitrate(rate)
            .with_ppi(Some(PPI))
            .with_threads(args.threads)
    };
    let decode_options = DecodeOptions::default().with_threads(args.threads);

    let mut rows = Vec::new();
    let result = (|| -> Result<(), Box<dyn Error>> {
        for src in &sources {
            for rate in &args.rates {
                let target: f32 = rate.parse()?;
                let options = encode_options(target);
                let (enc, wsq) = fastest(args.runs, || {
                    wsq_codec::encode(&src.pixels, src.width, src.height, &options)
                });
                let wsq = wsq?;
                let (dec, image) =
                    fastest(args.runs, || wsq_codec::decode_with(&wsq, &decode_options));
                let image = image?;
                if (image.width, image.height) != (src.width, src.height) {
                    return Err(format!("{}: decoded size differs", src.name).into());
                }
                let times = Some((enc.as_secs_f64() * 1e3, dec.as_secs_f64() * 1e3));
                let ours = measure(src, wsq.len(), &image.pixels, times);

                let nbis = match &args.mode {
                    Mode::None => None,
                    Mode::Run(bin) => {
                        let (stream, recon) = nbis_run(bin, &work, src, rate)?;
                        Some((stream, recon))
                    }
                    Mode::Streams(dir) => {
                        let stream = fs::read(dir.join(format!("{}_{rate}.wsq", src.name)))?;
                        let recon = wsq_codec::decode_with(&stream, &decode_options)?.pixels;
                        Some((stream, recon))
                    }
                }
                .map(|(stream, recon)| Nbis {
                    same_stream: stream == wsq,
                    same_pixels: recon == image.pixels,
                    measure: measure(src, stream.len(), &recon, None),
                });
                rows.push(Row {
                    image: src.name.clone(),
                    width: src.width,
                    height: src.height,
                    rate: rate.clone(),
                    target: f64::from(target),
                    ours,
                    nbis,
                });
            }
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&work);
    result?;

    print_tables(&rows, &args);
    if let Some(path) = &args.csv {
        let mut csv = String::from(
            "source,image,width,height,rate,target_bpp,bytes,bpp,psnr_db,ssim,encode_ms,decode_ms,identical_to_crate\n",
        );
        for row in &rows {
            writeln!(csv, "{}", csv_line(row, "wsq-codec", &row.ours, None))?;
            if let Some(n) = &row.nbis {
                writeln!(
                    csv,
                    "{}",
                    csv_line(
                        row,
                        "nbis",
                        &n.measure,
                        Some(n.same_stream && n.same_pixels)
                    )
                )?;
            }
        }
        fs::write(path, csv)?;
    }
    let differing = rows
        .iter()
        .filter(|r| {
            r.nbis
                .as_ref()
                .is_some_and(|n| !(n.same_stream && n.same_pixels))
        })
        .count();
    if differing > 0 {
        return Err(format!("{differing} measurement(s) differ from NBIS").into());
    }
    Ok(())
}

fn print_tables(rows: &[Row], args: &Args) {
    let compare = !matches!(args.mode, Mode::None);
    println!(
        "Threads {}, fastest of {} run(s), {PPI} ppi, whole-image PSNR, SSIM 8x8/stride 4.\n",
        args.threads, args.runs
    );
    print!("| image | target bpp | bytes | actual bpp | PSNR dB | SSIM | encode ms | decode ms |");
    println!(
        "{}",
        if compare {
            " NBIS bytes | NBIS bpp | NBIS PSNR dB | stream | pixels |"
        } else {
            ""
        }
    );
    print!("|---|---|---|---|---|---|---|---|");
    println!("{}", if compare { "---|---|---|---|---|" } else { "" });
    for r in rows {
        let (e, d) = r.ours.times.unwrap_or_default();
        print!(
            "| {}, {}×{} | {} | {} | {:.4} | {} | {:.4} | {e:.1} | {d:.1} |",
            r.image,
            r.width,
            r.height,
            r.rate,
            r.ours.bytes,
            r.ours.bpp,
            fmt_psnr(r.ours.psnr),
            r.ours.ssim
        );
        if let Some(n) = &r.nbis {
            let word = |same| if same { "identical" } else { "DIFFERENT" };
            print!(
                " {} | {:.4} | {} | {} | {} |",
                n.measure.bytes,
                n.measure.bpp,
                fmt_psnr(n.measure.psnr),
                word(n.same_stream),
                word(n.same_pixels)
            );
        }
        println!();
    }

    // Per requested rate, over the images that have a finite PSNR.
    let mut by_rate: BTreeMap<u32, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by_rate.entry((r.target * 1e4) as u32).or_default().push(r);
    }
    println!("\nPer requested rate, mean over the images:\n");
    println!(
        "| target bpp | images | mean actual bpp | mean deviation | mean PSNR dB | mean SSIM |"
    );
    println!("|---|---|---|---|---|---|");
    for group in by_rate.values() {
        let n = group.len() as f64;
        let target = group[0].target;
        let bpp = group.iter().map(|r| r.ours.bpp).sum::<f64>() / n;
        let dev = group
            .iter()
            .map(|r| (r.ours.bpp - target) / target)
            .sum::<f64>()
            / n
            * 100.0;
        let finite: Vec<f64> = group
            .iter()
            .map(|r| r.ours.psnr)
            .filter(|p| p.is_finite())
            .collect();
        let mean_psnr = finite.iter().sum::<f64>() / finite.len().max(1) as f64;
        let mean_ssim = group.iter().map(|r| r.ours.ssim).sum::<f64>() / n;
        println!(
            "| {target} | {} | {bpp:.4} | {dev:+.1}% | {mean_psnr:.2} | {mean_ssim:.4} |",
            group.len()
        );
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("quality: {e}");
        process::exit(1);
    }
}
