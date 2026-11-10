//! Measures what the encoder delivers: the actual bit rate against the requested one, and the
//! quality of the reconstruction (PSNR and SSIM), with the time taken.
//!
//! ```text
//! cargo bench --bench quality -- [--rates 0.5,0.75,1.0,2.25] [--runs N] [--threads N]
//!                                [--large] [--csv FILE] [--export DIR]
//!                                [--nbis BIN_DIR | --nbis-streams DIR] [--optimized]
//!                                [--fingerprint-like] [--nfiq2 PATH [--nfiq2-model FILE]]
//!                                [--mindtct PATH [--bozorth3 PATH]]
//! ```
//!
//! The images are generated with integer arithmetic only (the same generator as
//! `tests/nbis.rs`, whose images have NBIS references), so the figures do not depend on the
//! platform. `--large` adds a 1600x1500 image. Setting `WSQ_QUALITY_DIR` to a directory of 8-bit
//! binary PGM files (P5, at least 81x81) adds them, reported in aggregate only (see below); the
//! directory stays outside the repository.
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
//! `--optimized` also measures the `Optimized` profile (`wsq_codec::Profile`, #42 and #44), which scales the bin widths to hit the requested
//! rate, and the conformant encoder at equal size: the requested rate that makes the conformant
//! stream closest in length to the optimized one, found by bisection. With `--nbis`, NBIS's
//! `dwsq` decodes every optimized stream too, and a reconstruction that differs from this
//! crate's makes the run fail.
//!
//! Fingerprint-specific measures (#43), which PSNR does not give: whether the decoded image
//! still looks like a good fingerprint to NIST's quality score, and still matches the original.
//! They come from external programs that never link to this crate, each optional:
//!
//! - `--nfiq2 PATH`: NIST's NFIQ 2 (`nfiq2`), on the original and the decoded image, and the
//!   change between them (`--nfiq2-model FILE` names its `nist_plain_tir-ink.txt` model file when
//!   it is not in the `share` directory next to the tool);
//! - `--mindtct PATH`: NBIS 5.0.0's `mindtct`, which extracts the minutiae of both images (their
//!   number, and its change);
//! - `--bozorth3 PATH` (with `--mindtct`): NBIS's matcher, the score of the decoded image's
//!   minutiae against the original's, with the original against itself as the reference.
//!
//! The paths can also come from `WSQ_NFIQ2`, `WSQ_NFIQ2_MODEL`, `WSQ_MINDTCT` and
//! `WSQ_BOZORTH3`. A tool left out shows as "not run". They measure the synthetic ridge patterns
//! of `benches/quality/synth.rs` (three images, added by `--fingerprint-like` or by any of these
//! flags; they are not fingerprints, only patterns that have minutiae) and the images of
//! `WSQ_QUALITY_DIR`, not the other synthetic images, on which these tools say nothing. The
//! decoded images are those of the conformant encoder, the experimental one (`--optimized`) and
//! the conformant encoder at equal size, at the same requested rates.
//!
//! Images from `WSQ_QUALITY_DIR` are never listed: no file name and no per-file row is printed or
//! written to the CSV, only the means, minima and maxima over the directory.
//!
//! The table goes to the standard output; `--csv FILE` also writes every measurement as CSV.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{env, fs, process};

use wsq_codec::{DecodeOptions, EncodeOptions};

#[path = "quality/external.rs"]
mod external;
#[path = "quality/synth.rs"]
mod synth;

use external::{Fp, Reading, Tools};

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
    /// Measured with NFIQ 2 and minutiae (a ridge pattern, or an image of the local directory).
    fingerprint: bool,
    /// From `WSQ_QUALITY_DIR`: reported in aggregate only, never by name.
    corpus: bool,
}

/// The synthetic ridge patterns that have minutiae, for the fingerprint measures.
fn ridge_patterns() -> Vec<Source> {
    synth::images()
        .into_iter()
        .map(|(name, pattern)| Source {
            name: name.to_string(),
            width: pattern.width,
            height: pattern.height,
            pixels: synth::ridge_pattern(&pattern),
            fingerprint: true,
            corpus: false,
        })
        .collect()
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
            fingerprint: false,
            corpus: false,
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
    for (i, path) in paths.iter().enumerate() {
        let (width, height, pixels) =
            read_pgm(&fs::read(path)?).map_err(|e| format!("corpus image {}: {e}", i + 1))?;
        sources.push(Source {
            // A number, not the file's name: nothing identifies a file in the output.
            name: format!("corpus{:04}", i + 1),
            width,
            height,
            pixels,
            fingerprint: true,
            corpus: true,
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
    /// NFIQ 2 and minutiae of the reconstruction, when the tools ran on this source.
    fp: Option<Fp>,
}

fn measure(src: &Source, stream_len: usize, recon: &[u8], times: Option<(f64, f64)>) -> Measure {
    Measure {
        bytes: stream_len,
        bpp: stream_len as f64 * 8.0 / (src.width * src.height) as f64,
        psnr: psnr(&src.pixels, recon),
        ssim: ssim(&src.pixels, recon, src.width, src.height),
        times,
        fp: None,
    }
}

/// The original image as the external tools see it.
#[derive(Clone)]
struct FpRef {
    reading: Reading,
    /// `bozorth3` score of the original's minutiae against themselves.
    self_score: Option<u32>,
}

/// Runs the tools on `pixels` (a reconstruction of `src`) and matches its minutiae against the
/// original's.
fn fp_measure(
    tools: &Tools,
    work: &Path,
    src: &Source,
    reference: &FpRef,
    key: &str,
    pixels: &[u8],
) -> Result<Fp, Box<dyn Error>> {
    let reading = external::read(tools, work, key, src.width, src.height, pixels, PPI)?;
    let matched = match (
        &tools.bozorth3,
        &reading.template,
        &reference.reading.template,
    ) {
        (Some(bin), Some(probe), Some(gallery)) => Some(external::bozorth3(bin, probe, gallery)?),
        _ => None,
    };
    Ok(Fp { reading, matched })
}

fn fp_reference(tools: &Tools, work: &Path, src: &Source) -> Result<FpRef, Box<dyn Error>> {
    let reading = external::read(
        tools,
        work,
        "original",
        src.width,
        src.height,
        &src.pixels,
        PPI,
    )?;
    let self_score = match (&tools.bozorth3, &reading.template) {
        (Some(bin), Some(t)) => Some(external::bozorth3(bin, t, t)?),
        _ => None,
    };
    Ok(FpRef {
        reading,
        self_score,
    })
}

struct Nbis {
    measure: Measure,
    same_stream: bool,
    same_pixels: bool,
}

/// The experimental encoder against the conformant one at equal size.
struct Optimized {
    measure: Measure,
    /// Requested rate that gives the conformant stream closest in length.
    equal_rate: f32,
    equal: Measure,
    /// Whether NBIS's `dwsq` gives the same pixels, when it ran.
    nbis_pixels: Option<bool>,
}

struct Row {
    image: String,
    width: usize,
    height: usize,
    rate: String,
    target: f64,
    corpus: bool,
    /// Whether the fingerprint measures apply to this image.
    fingerprint: bool,
    reference: Option<FpRef>,
    ours: Measure,
    nbis: Option<Nbis>,
    optimized: Option<Optimized>,
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
    optimized: bool,
    /// Add the synthetic ridge patterns, for the fingerprint measures.
    fingerprint_like: bool,
    tools: Tools,
}

fn parse_args(args: &[String]) -> Result<Args, Box<dyn Error>> {
    let from_env = |name: &str| {
        env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let mut out = Args {
        rates: ["0.5", "0.75", "1.0", "2.25"].map(String::from).to_vec(),
        runs: 3,
        threads: 1,
        large: false,
        csv: None,
        export: None,
        mode: Mode::None,
        optimized: false,
        fingerprint_like: false,
        tools: Tools {
            nfiq2: from_env("WSQ_NFIQ2"),
            nfiq2_model: from_env("WSQ_NFIQ2_MODEL"),
            mindtct: from_env("WSQ_MINDTCT"),
            bozorth3: from_env("WSQ_BOZORTH3"),
        },
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
            "--optimized" => out.optimized = true,
            "--fingerprint-like" => out.fingerprint_like = true,
            "--nfiq2" => out.tools.nfiq2 = Some(value("--nfiq2")?.into()),
            "--nfiq2-model" => out.tools.nfiq2_model = Some(value("--nfiq2-model")?.into()),
            "--mindtct" => out.tools.mindtct = Some(value("--mindtct")?.into()),
            "--bozorth3" => out.tools.bozorth3 = Some(value("--bozorth3")?.into()),
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
    if out.tools.bozorth3.is_some() && out.tools.mindtct.is_none() {
        return Err("--bozorth3 needs --mindtct: it matches the minutiae mindtct extracts".into());
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

/// Reconstruction of `stream` by NBIS's `dwsq`.
fn nbis_decode(
    bin: &Path,
    work: &Path,
    name: &str,
    stream: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let dir = work.join("optimized");
    fs::create_dir_all(&dir)?;
    let wsq = dir.join(format!("{name}.wsq"));
    fs::write(&wsq, stream)?;
    nbis_tool(
        bin,
        "dwsq",
        &["raw".as_ref(), wsq.as_os_str(), "-raw_out".as_ref()],
    )?;
    let raw = dir.join(format!("{name}.raw"));
    let recon = fs::read(&raw)?;
    fs::remove_file(&raw)?;
    fs::remove_file(&wsq)?;
    Ok(recon)
}

/// The requested rate whose conformant stream is closest in length to `len` bytes, by bisection
/// on the logarithm of the rate, and that stream.
fn equal_size(
    src: &Source,
    len: usize,
    options: impl Fn(f32) -> EncodeOptions,
) -> Result<(f32, Vec<u8>), Box<dyn Error>> {
    let encode = |rate: f32| wsq_codec::encode(&src.pixels, src.width, src.height, &options(rate));
    let target = len as f64 * 8.0 / (src.width * src.height) as f64;
    let (mut lo, mut hi) = ((target / 64.0).ln(), (target * 64.0).ln());
    let mut best: Option<(f32, Vec<u8>)> = None;
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        let rate = mid.exp() as f32;
        let wsq = encode(rate)?;
        if best
            .as_ref()
            .is_none_or(|b| wsq.len().abs_diff(len) < b.1.len().abs_diff(len))
        {
            best = Some((rate, wsq.clone()));
        }
        match wsq.len().cmp(&len) {
            std::cmp::Ordering::Less => lo = mid,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => break,
        }
    }
    Ok(best.expect("at least one try"))
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

fn csv_line(
    row: &Row,
    source: &str,
    m: &Measure,
    identical: Option<bool>,
    requested: Option<f32>,
) -> String {
    let ms = |t: Option<f64>| t.map_or(String::new(), |t| format!("{t:.1}"));
    let times = m.times;
    let cell = |v: Option<String>| v.unwrap_or_default();
    // The original's figures go with the reconstruction's, on the rows that have one.
    let fp = m.fp.as_ref();
    let reference = row.reference.as_ref().filter(|_| fp.is_some());
    let fingerprint = format!(
        "{},{},{},{},{},{}",
        cell(
            reference
                .and_then(|r| r.reading.nfiq2)
                .map(|v| v.to_string())
        ),
        cell(fp.and_then(|f| f.reading.nfiq2).map(|v| v.to_string())),
        cell(
            reference
                .and_then(|r| r.reading.minutiae)
                .map(|v| v.to_string())
        ),
        cell(fp.and_then(|f| f.reading.minutiae).map(|v| v.to_string())),
        cell(fp.and_then(|f| f.matched).map(|v| v.to_string())),
        cell(reference.and_then(|r| r.self_score).map(|v| v.to_string())),
    );
    format!(
        "{source},{},{},{},{},{},{},{:.4},{},{:.4},{},{},{},{fingerprint},{}",
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
        requested.map_or(String::new(), |r| r.to_string()),
    )
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let args = parse_args(args)?;
    let mut sources = synthetic(args.large);
    if args.fingerprint_like || args.tools.any() {
        sources.extend(ridge_patterns());
    }
    sources.extend(local_corpus()?);
    if let Some(dir) = &args.export {
        fs::create_dir_all(dir)?;
        for src in &sources {
            fs::write(dir.join(format!("{}.pgm", src.name)), pgm(src))?;
        }
    }
    let work = env::temp_dir().join(format!("wsq-quality-{}", process::id()));
    let fp_work = work.join("fingerprint");
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
            let measured = src.fingerprint && args.tools.any();
            let reference = if measured {
                Some(fp_reference(&args.tools, &fp_work.join(&src.name), src)?)
            } else {
                None
            };
            // Runs the tools on a reconstruction of this source, keyed by rate and encoder.
            let fp_of =
                |rate: &str, encoder: &str, pixels: &[u8]| -> Result<Option<Fp>, Box<dyn Error>> {
                    let Some(reference) = &reference else {
                        return Ok(None);
                    };
                    let key = format!("{}_{encoder}", rate.replace('.', "p"));
                    let work = fp_work.join(&src.name);
                    let fp = fp_measure(&args.tools, &work, src, reference, &key, pixels)?;
                    Ok(Some(fp))
                };
            for rate in &args.rates {
                let target: f32 = rate.parse()?;
                let options = encode_options(target);
                let optimized_options = options.clone().with_profile(wsq_codec::Profile::Optimized);
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
                let mut ours = measure(src, wsq.len(), &image.pixels, times);
                ours.fp = fp_of(rate, "conformant", &image.pixels)?;

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
                let optimized = if args.optimized {
                    let (enc, wsq) = fastest(args.runs, || {
                        wsq_codec::encode(&src.pixels, src.width, src.height, &optimized_options)
                    });
                    let wsq = wsq?;
                    let (dec, image) =
                        fastest(args.runs, || wsq_codec::decode_with(&wsq, &decode_options));
                    let image = image?;
                    let times = Some((enc.as_secs_f64() * 1e3, dec.as_secs_f64() * 1e3));
                    let mut opt = measure(src, wsq.len(), &image.pixels, times);
                    opt.fp = fp_of(rate, "optimized", &image.pixels)?;
                    let (equal_rate, equal_wsq) = equal_size(src, wsq.len(), encode_options)?;
                    let equal_pixels = wsq_codec::decode_with(&equal_wsq, &decode_options)?.pixels;
                    let mut equal = measure(src, equal_wsq.len(), &equal_pixels, None);
                    equal.fp = fp_of(rate, "equal", &equal_pixels)?;
                    let nbis_pixels = match &args.mode {
                        Mode::Run(bin) => Some(
                            nbis_decode(bin, &work, &format!("{}_{rate}", src.name), &wsq)?
                                == image.pixels,
                        ),
                        _ => None,
                    };
                    Some(Optimized {
                        measure: opt,
                        equal_rate,
                        equal,
                        nbis_pixels,
                    })
                } else {
                    None
                };
                rows.push(Row {
                    image: src.name.clone(),
                    width: src.width,
                    height: src.height,
                    rate: rate.clone(),
                    target: f64::from(target),
                    corpus: src.corpus,
                    fingerprint: src.fingerprint,
                    reference: reference.clone(),
                    ours,
                    nbis,
                    optimized,
                });
            }
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&work);
    result?;

    print_tables(&rows, &args);
    print_optimized(&rows);
    print_fingerprint(&rows, &args);
    if let Some(path) = &args.csv {
        let mut csv = String::from(
            "source,image,width,height,rate,target_bpp,bytes,bpp,psnr_db,ssim,encode_ms,decode_ms,identical_to_crate,nfiq2_original,nfiq2_decoded,minutiae_original,minutiae_decoded,bozorth3_match,bozorth3_self,requested_bpp\n",
        );
        // Images of the local directory are not listed, even here.
        for row in rows.iter().filter(|r| !r.corpus) {
            writeln!(csv, "{}", csv_line(row, "wsq-codec", &row.ours, None, None))?;
            if let Some(o) = &row.optimized {
                writeln!(
                    csv,
                    "{}",
                    csv_line(row, "wsq-codec-optimized", &o.measure, o.nbis_pixels, None)
                )?;
                let equal = csv_line(
                    row,
                    "wsq-codec-equal-size",
                    &o.equal,
                    None,
                    Some(o.equal_rate),
                );
                writeln!(csv, "{equal}")?;
            }
            if let Some(n) = &row.nbis {
                writeln!(
                    csv,
                    "{}",
                    csv_line(
                        row,
                        "nbis",
                        &n.measure,
                        Some(n.same_stream && n.same_pixels),
                        None
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
    let unreadable = rows
        .iter()
        .filter(|r| {
            r.optimized
                .as_ref()
                .is_some_and(|o| o.nbis_pixels == Some(false))
        })
        .count();
    if differing > 0 {
        return Err(format!("{differing} measurement(s) differ from NBIS").into());
    }
    if unreadable > 0 {
        return Err(format!("NBIS decodes {unreadable} optimized stream(s) differently").into());
    }
    Ok(())
}

/// How a group of images is called in the aggregate tables.
fn set_name(corpus: bool) -> &'static str {
    if corpus {
        "local directory"
    } else {
        "synthetic"
    }
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
    for r in rows.iter().filter(|r| !r.corpus) {
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
    let mut by_rate: BTreeMap<(bool, u32), Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by_rate
            .entry((r.corpus, (r.target * 1e4) as u32))
            .or_default()
            .push(r);
    }
    println!("\nPer requested rate, mean over the images:\n");
    println!(
        "| set | target bpp | images | mean actual bpp | mean deviation | mean PSNR dB | mean SSIM |"
    );
    println!("|---|---|---|---|---|---|---|");
    for (&(corpus, _), group) in &by_rate {
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
            "| {} | {target} | {} | {bpp:.4} | {dev:+.1}% | {mean_psnr:.2} | {mean_ssim:.4} |",
            set_name(corpus),
            group.len()
        );
    }
}

fn print_optimized(rows: &[Row]) {
    let rows: Vec<(&Row, &Optimized)> = rows
        .iter()
        .filter_map(|r| r.optimized.as_ref().map(|o| (r, o)))
        .collect();
    if rows.is_empty() {
        return;
    }
    let nbis = rows.iter().any(|(_, o)| o.nbis_pixels.is_some());
    println!(
        "\nExperimental encoder (#42), and the conformant encoder at the requested rate that gives \
         the closest size:\n"
    );
    print!(
        "| image | target bpp | bytes | actual bpp | deviation | PSNR dB | SSIM | encode ms | \
         equal-size requested bpp | its bytes | its PSNR dB | its SSIM | PSNR diff dB |"
    );
    println!("{}", if nbis { " NBIS dwsq |" } else { "" });
    print!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    println!("{}", if nbis { "---|" } else { "" });
    for (r, o) in rows.iter().filter(|(r, _)| !r.corpus) {
        let m = &o.measure;
        print!(
            "| {}, {}×{} | {} | {} | {:.4} | {:+.2}% | {} | {:.4} | {:.1} | {:.4} | {} | {} | {:.4} | {:+.2} |",
            r.image,
            r.width,
            r.height,
            r.rate,
            m.bytes,
            m.bpp,
            (m.bpp - r.target) / r.target * 100.0,
            fmt_psnr(m.psnr),
            m.ssim,
            m.times.unwrap_or_default().0,
            o.equal_rate,
            o.equal.bytes,
            fmt_psnr(o.equal.psnr),
            o.equal.ssim,
            m.psnr - o.equal.psnr,
        );
        if let Some(same) = o.nbis_pixels {
            print!(" {} |", if same { "identical" } else { "DIFFERENT" });
        }
        println!();
    }

    let mut by_rate: BTreeMap<(bool, u32), Vec<(&Row, &Optimized)>> = BTreeMap::new();
    for &(r, o) in &rows {
        by_rate
            .entry((r.corpus, (r.target * 1e4) as u32))
            .or_default()
            .push((r, o));
    }
    println!("\nPer requested rate, over the images:\n");
    println!(
        "| set | target bpp | images | mean actual bpp | deviation, min / max | mean PSNR dB | \
         mean PSNR diff at equal size dB | mean SSIM diff | encode time / conformant |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    for (&(corpus, _), group) in &by_rate {
        let n = group.len() as f64;
        let target = group[0].0.target;
        let mean = |f: &dyn Fn(&Row, &Optimized) -> f64| {
            group.iter().map(|(r, o)| f(r, o)).sum::<f64>() / n
        };
        let devs: Vec<f64> = group
            .iter()
            .map(|(r, o)| (o.measure.bpp - r.target) / r.target * 100.0)
            .collect();
        let min = devs.iter().copied().fold(f64::INFINITY, f64::min);
        let max = devs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let time = |m: &Measure| m.times.unwrap_or_default().0;
        println!(
            "| {} | {target} | {} | {:.4} | {min:+.2}% / {max:+.2}% | {:.2} | {:+.3} | {:+.4} | {:.2} |",
            set_name(corpus),
            group.len(),
            mean(&|_, o| o.measure.bpp),
            mean(&|_, o| o.measure.psnr),
            mean(&|_, o| o.measure.psnr - o.equal.psnr),
            mean(&|_, o| o.measure.ssim - o.equal.ssim),
            group.iter().map(|(_, o)| time(&o.measure)).sum::<f64>()
                / group.iter().map(|(r, _)| time(&r.ours)).sum::<f64>(),
        );
    }
}

/// A table cell for a figure of an external tool: "not run" when the tool was not given, "n/a"
/// when it ran and gave nothing for this image.
fn cell<T: Display>(ran: bool, value: Option<T>) -> String {
    if !ran {
        "not run".into()
    } else {
        value.map_or("n/a".into(), |v| v.to_string())
    }
}

/// The reconstructions of one row that the fingerprint measures cover, by encoder.
fn encoders(row: &Row) -> Vec<(&'static str, &Measure)> {
    let mut out = vec![("conformant", &row.ours)];
    if let Some(o) = &row.optimized {
        out.push(("optimized", &o.measure));
        out.push(("conformant, equal size", &o.equal));
    }
    out
}

/// Figures of one reconstruction, from the original's reading and its own.
struct Figures {
    nfiq2: Option<u32>,
    nfiq2_original: Option<u32>,
    minutiae: Option<usize>,
    minutiae_original: Option<usize>,
    matched: Option<u32>,
    self_score: Option<u32>,
}

impl Figures {
    fn of(row: &Row, m: &Measure) -> Figures {
        let reference = row.reference.as_ref();
        let fp = m.fp.as_ref();
        Figures {
            nfiq2: fp.and_then(|f| f.reading.nfiq2),
            nfiq2_original: reference.and_then(|r| r.reading.nfiq2),
            minutiae: fp.and_then(|f| f.reading.minutiae),
            minutiae_original: reference.and_then(|r| r.reading.minutiae),
            matched: fp.and_then(|f| f.matched),
            self_score: reference.and_then(|r| r.self_score),
        }
    }

    fn nfiq2_change(&self) -> Option<f64> {
        Some(f64::from(self.nfiq2?) - f64::from(self.nfiq2_original?))
    }

    fn minutiae_change(&self) -> Option<f64> {
        Some(self.minutiae? as f64 - self.minutiae_original? as f64)
    }

    /// Match score over the original's score against itself, in per cent.
    fn match_ratio(&self) -> Option<f64> {
        let (m, s) = (self.matched?, self.self_score?);
        (s > 0).then(|| f64::from(m) / f64::from(s) * 100.0)
    }
}

/// Mean and minimum of the figures that exist.
fn mean_min(values: impl Iterator<Item = Option<f64>>) -> Option<(f64, f64)> {
    let v: Vec<f64> = values.flatten().collect();
    (!v.is_empty()).then(|| {
        (
            v.iter().sum::<f64>() / v.len() as f64,
            v.iter().copied().fold(f64::INFINITY, f64::min),
        )
    })
}

fn print_fingerprint(rows: &[Row], args: &Args) {
    let rows: Vec<&Row> = rows.iter().filter(|r| r.fingerprint).collect();
    if rows.is_empty() {
        return;
    }
    let tools = &args.tools;
    let (nfiq2, minutiae, matcher) = (
        tools.nfiq2.is_some(),
        tools.mindtct.is_some(),
        tools.bozorth3.is_some(),
    );
    println!(
        "\nFingerprint measures (#43): NFIQ 2 (0 to 100, higher is better), minutiae of NBIS's \
         `mindtct`, and the `bozorth3` score of the reconstruction's minutiae against the \
         original's (the original against itself is the reference). Synthetic ridge patterns \
         are not fingerprints. A tool that was not given shows as \"not run\", one that gave \
         nothing for an image as \"n/a\".\n"
    );
    println!(
        "| image | requested bpp | encoder | bytes | NFIQ 2 original | NFIQ 2 decoded | change | \
         minutiae original | minutiae decoded | change | bozorth3 match | bozorth3 self | \
         match / self |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    let signed = |ran: bool, v: Option<f64>| cell(ran, v.map(|v| format!("{v:+.0}")));
    for r in rows.iter().filter(|r| !r.corpus) {
        for (name, m) in encoders(r) {
            let f = Figures::of(r, m);
            println!(
                "| {} | {} | {name} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                r.image,
                r.rate,
                m.bytes,
                cell(nfiq2, f.nfiq2_original),
                cell(nfiq2, f.nfiq2),
                signed(nfiq2, f.nfiq2_change()),
                cell(minutiae, f.minutiae_original),
                cell(minutiae, f.minutiae),
                signed(minutiae, f.minutiae_change()),
                cell(matcher, f.matched),
                cell(matcher, f.self_score),
                cell(matcher, f.match_ratio().map(|v| format!("{v:.0}%"))),
            );
        }
    }

    // Aggregates, which are all there is for the local directory.
    let mut groups: BTreeMap<(bool, u32, usize), Vec<Figures>> = BTreeMap::new();
    let mut names = Vec::new();
    for r in &rows {
        for (i, (name, m)) in encoders(r).into_iter().enumerate() {
            if !names.contains(&(i, name)) {
                names.push((i, name));
            }
            groups
                .entry((r.corpus, (r.target * 1e4) as u32, i))
                .or_default()
                .push(Figures::of(r, m));
        }
    }
    println!(
        "\nPer requested rate and encoder, over the images (means; \"worst\" is the minimum):\n"
    );
    println!(
        "| set | requested bpp | encoder | images | NFIQ 2 original | NFIQ 2 decoded | \
         change, mean / worst | minutiae change, mean | bozorth3 match | bozorth3 self | \
         match / self, mean / worst |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    let pair = |ran: bool, v: Option<(f64, f64)>, precision: usize| {
        cell(
            ran,
            v.map(|(mean, min)| format!("{mean:+.precision$} / {min:+.precision$}")),
        )
    };
    let plain =
        |ran: bool, v: Option<(f64, f64)>| cell(ran, v.map(|(mean, _)| format!("{mean:.1}")));
    let mut missing = 0;
    for (&(corpus, rate, encoder), group) in &groups {
        let name = names
            .iter()
            .find(|(i, _)| *i == encoder)
            .map_or("", |n| n.1);
        let target = f64::from(rate) / 1e4;
        let col = |f: fn(&Figures) -> Option<f64>| mean_min(group.iter().map(f));
        let ratio = col(Figures::match_ratio);
        println!(
            "| {} | {target} | {name} | {} | {} | {} | {} | {} | {} | {} | {} |",
            set_name(corpus),
            group.len(),
            plain(nfiq2, col(|f| f.nfiq2_original.map(f64::from))),
            plain(nfiq2, col(|f| f.nfiq2.map(f64::from))),
            pair(nfiq2, col(Figures::nfiq2_change), 1),
            plain(minutiae, col(Figures::minutiae_change)),
            plain(matcher, col(|f| f.matched.map(f64::from))),
            plain(matcher, col(|f| f.self_score.map(f64::from))),
            cell(
                matcher,
                ratio.map(|(mean, min)| format!("{mean:.0}% / {min:.0}%"))
            ),
        );
        missing += group
            .iter()
            .filter(|f| nfiq2 && (f.nfiq2.is_none() || f.nfiq2_original.is_none()))
            .count();
    }
    if missing > 0 {
        println!("\nNFIQ 2 gave no score for {missing} reconstruction(s) or original(s) above.");
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("quality: {e}");
        process::exit(1);
    }
}
