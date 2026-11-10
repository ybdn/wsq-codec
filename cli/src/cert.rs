//! Running the FBI certification tests locally (see docs/certification.md).
//!
//! NIST supplies the test images and its reference streams and
//! reconstructions on request; they must never be committed. These
//! commands produce the files a submission holds and compare them with
//! NIST's references before anything is sent:
//!
//! - `cert encode IN OUT [--software N] [--ppi N] [--compiler TEXT]` encodes
//!   every `.pgm` in `IN` at both certification rates, into `OUT/0.75/` and
//!   `OUT/2.25/`, and writes `OUT/submission.txt`, the description of the
//!   platform the submission asks for, to be completed by hand. The
//!   compiler is never guessed: pass the output of `rustc -vV` run with the
//!   toolchain that built this binary, or fill the line in by hand.
//! - `cert decode IN OUT` decodes every `.wsq` under `IN` into a binary
//!   PGM (P5) at the same relative path under `OUT`.
//! - `cert compare OURS REF` compares, for every `.wsq` under `REF`, the
//!   stream at the same relative path under `OURS`: file size, header
//!   parameters, bin widths and bin indices; and for every `.pgm` or
//!   `.raw` under `REF`, the `.pgm` under `OURS`, pixel by pixel. It prints
//!   the differences, then a verdict per file and a summary, judged by the
//!   accuracy requirements of the specification (IAFIS-IC-0110 v3.1, Annex
//!   AA; see `accuracy.rs`). The exit status is 0 when every file passes, 2
//!   when a reference file has no readable counterpart and 3 when a
//!   requirement fails (3 wins over 2).
//!
//! The file names NIST uses are in its instructions, to which the output
//! directories may need renaming.

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use wsq_codec::inspect::{Quantized, read_quantized};
use wsq_codec::{DecodeOptions, EncodeOptions, Profile};

use crate::accuracy::{self, Thresholds};
use crate::corpus::read_pgm;

/// The bit rates the certification encodes at.
const RATES: [(f32, &str); 2] = [(0.75, "0.75"), (2.25, "2.25")];

/// How a `cert` command ended; the process exits with `code`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Done,
    /// A reference file has no readable counterpart.
    Incomplete,
    /// A file does not meet the accuracy requirements.
    Failed,
}

impl Outcome {
    pub fn code(self) -> i32 {
        match self {
            Outcome::Done => 0,
            Outcome::Incomplete => 2,
            Outcome::Failed => 3,
        }
    }
}

pub fn run(args: &[String]) -> Result<Outcome, Box<dyn Error>> {
    match args {
        [cmd, input, output, flags @ ..] if cmd == "encode" => {
            let mut software = 0;
            let mut ppi = None;
            let mut compiler = None;
            let mut flags = flags.iter();
            while let Some(flag) = flags.next() {
                let value = flags
                    .next()
                    .ok_or_else(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--software" => software = value.parse()?,
                    "--ppi" => ppi = Some(value.parse()?),
                    "--compiler" => compiler = Some(value.clone()),
                    _ => return Err(format!("unknown option {flag}").into()),
                }
            }
            encode(
                Path::new(input),
                Path::new(output),
                software,
                ppi,
                compiler.as_deref(),
            )?;
            Ok(Outcome::Done)
        }
        [cmd, input, output] if cmd == "decode" => {
            decode(Path::new(input), Path::new(output))?;
            Ok(Outcome::Done)
        }
        [cmd, ours, reference] if cmd == "compare" => {
            compare(Path::new(ours), Path::new(reference))
        }
        _ => Err("usage: wsq cert encode IN OUT [--software N] [--ppi N] \
                  [--compiler TEXT] | cert decode IN OUT | cert compare OURS REF"
            .into()),
    }
}

/// Files under `dir` with extension `ext`, recursively, sorted.
fn files(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn write_pgm(path: &Path, width: usize, height: usize, pixels: &[u8]) -> std::io::Result<()> {
    let mut pgm = format!("P5\n{width} {height}\n255\n").into_bytes();
    pgm.extend_from_slice(pixels);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, pgm)
}

fn encode(
    input: &Path,
    output: &Path,
    software: u16,
    ppi: Option<u32>,
    compiler: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let images = files(input, "pgm")?;
    if images.is_empty() {
        return Err(format!("no .pgm file under {}", input.display()).into());
    }
    for (_, name) in RATES {
        fs::create_dir_all(output.join(name))?;
    }
    for path in &images {
        let data = fs::read(path)?;
        let (width, height, pixels) =
            read_pgm(&data).map_err(|e| format!("{}: {e}", path.display()))?;
        let rel = path.strip_prefix(input)?.with_extension("wsq");
        for (rate, name) in RATES {
            let options = EncodeOptions::default()
                .with_profile(Profile::Conformant)
                .with_bitrate(rate)
                .with_ppi(ppi)
                .with_software(software);
            let wsq = wsq_codec::encode(pixels, width, height, &options)
                .map_err(|e| format!("{} at {name}: {e}", path.display()))?;
            let out = output.join(name).join(&rel);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out, &wsq)?;
        }
        println!("{}: {width}x{height}", rel.display());
    }
    let sheet = output.join("submission.txt");
    fs::write(&sheet, submission_sheet(software, compiler))?;
    println!(
        "{} images encoded at {} rates; complete {}",
        images.len(),
        RATES.len(),
        sheet.display()
    );
    Ok(())
}

/// The plain text description a submission includes, with what can be
/// detected filled in. The compiler is only what the caller states: the
/// `rustc` on the `PATH` is not necessarily the one that built this binary.
fn submission_sheet(software: u16, compiler: Option<&str>) -> String {
    let compiler = compiler.map_or_else(
        || {
            "(fill in: `rustc -vV` of the toolchain that built the tested binary, \
             or pass --compiler)"
                .to_string()
        },
        |c| c.trim().replace('\n', "\n                           "),
    );
    let mut s = String::new();
    let _ = writeln!(s, "WSQ certification submission");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Vendor contact:            (fill in: name, organization, email, phone)"
    );
    let _ = writeln!(
        s,
        "Implementation number:     {}",
        if software == 0 {
            "(not yet assigned)".to_string()
        } else {
            software.to_string()
        }
    );
    let _ = writeln!(
        s,
        "Implementation:            wsq-codec {}",
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(s, "Internal version:          (fill in: git commit)");
    let _ = writeln!(
        s,
        "Platform:                  {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(s, "Hardware:                  (fill in: CPU model)");
    let _ = writeln!(s, "Operating system version:  (fill in)");
    let _ = writeln!(s, "Compiler:                  {compiler}");
    s
}

fn decode(input: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let streams = files(input, "wsq")?;
    if streams.is_empty() {
        return Err(format!("no .wsq file under {}", input.display()).into());
    }
    let options = DecodeOptions::default();
    for path in &streams {
        let image = wsq_codec::decode_with(&fs::read(path)?, &options)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let rel = path.strip_prefix(input)?.with_extension("pgm");
        write_pgm(&output.join(&rel), image.width, image.height, &image.pixels)?;
        println!("{}: {}x{}", rel.display(), image.width, image.height);
    }
    println!("{} streams decoded", streams.len());
    Ok(())
}

/// The header parameters that differ between two streams.
fn header_diff(ours: &Quantized, reference: &Quantized) -> (f32, f32) {
    (
        (ours.m_shift - reference.m_shift).abs(),
        (ours.r_scale - reference.r_scale).abs(),
    )
}

/// Prints the verdict on a file, after the raw differences; returns whether
/// it passed.
fn verdict(rel: &Path, checks: &[accuracy::Check]) -> bool {
    let ok = accuracy::passed(checks);
    if ok {
        println!("{}: PASS", rel.display());
    } else {
        println!("{}: FAIL {}", rel.display(), accuracy::failures(checks));
    }
    ok
}

/// Compares the files under `ours` with NIST's under `reference`, and judges
/// each by the accuracy requirements of the specification.
fn compare(ours: &Path, reference: &Path) -> Result<Outcome, Box<dyn Error>> {
    let options = DecodeOptions::default();
    let thresholds = Thresholds::default();
    let mut complete = true;
    let (mut streams, mut identical_streams, mut passing_streams) = (0, 0, 0);
    let (mut images, mut identical_images, mut passing_images) = (0, 0, 0);

    for ref_path in files(reference, "wsq")? {
        let rel = ref_path.strip_prefix(reference)?;
        let our_path = ours.join(rel);
        let (ref_data, our_data) = match (fs::read(&ref_path), fs::read(&our_path)) {
            (Ok(r), Ok(o)) => (r, o),
            (_, Err(e)) => {
                println!("{}: MISSING ({e})", rel.display());
                complete = false;
                continue;
            }
            (Err(e), _) => return Err(format!("{}: {e}", ref_path.display()).into()),
        };
        let (r, o) = match (
            read_quantized(&ref_data, &options),
            read_quantized(&our_data, &options),
        ) {
            (Ok(r), Ok(o)) => (r, o),
            (r, o) => {
                println!(
                    "{}: UNREADABLE (reference: {:?}, ours: {:?})",
                    rel.display(),
                    r.err(),
                    o.err()
                );
                complete = false;
                continue;
            }
        };
        streams += 1;
        if (o.info.width, o.info.height) != (r.info.width, r.info.height) {
            println!(
                "{}: DIMENSIONS {}x{}, reference {}x{}",
                rel.display(),
                o.info.width,
                o.info.height,
                r.info.width,
                r.info.height
            );
            complete = false;
            continue;
        }
        let size = (our_data.len() as f64 / ref_data.len() as f64 - 1.0) * 100.0;
        let (m_shift, r_scale) = header_diff(&o, &r);
        let d = accuracy::measure(&o, &r);
        if our_data == ref_data {
            identical_streams += 1;
            println!("{}: identical bytes", rel.display());
        } else {
            println!(
                "{}: size {} vs {} ({size:+.3}%), M diff {:.3e}, R diff {:.3e}, \
                 Q rel diff {:.3e}, Z rel diff {:.3e}, subbands dropped differently {}, \
                 bin indices differing {}/{} (max {})",
                rel.display(),
                our_data.len(),
                ref_data.len(),
                m_shift,
                r_scale,
                d.worst_q,
                d.worst_z,
                d.dropped,
                d.differing,
                d.total,
                d.max_difference
            );
        }
        let checks = accuracy::judge_stream(
            &d,
            accuracy::size_without_comments(our_data.len(), &o.info.comments),
            accuracy::size_without_comments(ref_data.len(), &r.info.comments),
            &thresholds,
        );
        passing_streams += usize::from(verdict(rel, &checks));
    }

    let mut ref_images = files(reference, "pgm")?;
    ref_images.extend(files(reference, "raw")?);
    ref_images.sort();
    for ref_path in ref_images {
        let rel = ref_path.strip_prefix(reference)?;
        let our_path = ours.join(rel).with_extension("pgm");
        let our_data = match fs::read(&our_path) {
            Ok(d) => d,
            Err(e) => {
                println!("{}: MISSING ({e})", rel.display());
                complete = false;
                continue;
            }
        };
        let ref_data = fs::read(&ref_path)?;
        let (w, h, our_pixels) =
            read_pgm(&our_data).map_err(|e| format!("{}: {e}", our_path.display()))?;
        let ref_pixels = if ref_path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("raw"))
        {
            ref_data.as_slice()
        } else {
            let (rw, rh, p) =
                read_pgm(&ref_data).map_err(|e| format!("{}: {e}", ref_path.display()))?;
            if (rw, rh) != (w, h) {
                println!("{}: DIMENSIONS {w}x{h}, reference {rw}x{rh}", rel.display());
                complete = false;
                continue;
            }
            p
        };
        if ref_pixels.len() != our_pixels.len() {
            println!(
                "{}: SIZE {} pixels, reference {}",
                rel.display(),
                our_pixels.len(),
                ref_pixels.len()
            );
            complete = false;
            continue;
        }
        images += 1;
        let (differing, max) = accuracy::pixel_differences(our_pixels, ref_pixels);
        let checks = [accuracy::pixels(
            differing,
            max,
            ref_pixels.len() as u64,
            &thresholds,
        )];
        if differing == 0 {
            identical_images += 1;
            println!("{}: identical pixels", rel.display());
        } else {
            println!(
                "{}: pixels differing {differing}/{} (max {max})",
                rel.display(),
                our_pixels.len()
            );
        }
        passing_images += usize::from(verdict(rel, &checks));
    }

    println!(
        "streams: {identical_streams}/{streams} identical; \
         reconstructions: {identical_images}/{images} identical"
    );
    println!(
        "verdict (IAFIS-IC-0110 v3.1, Annex AA): streams {passing_streams}/{streams} pass; \
         reconstructions {passing_images}/{images} pass"
    );
    if streams + images == 0 {
        return Err(format!("no .wsq, .pgm or .raw file under {}", reference.display()).into());
    }
    if !complete {
        println!("some reference files have no readable counterpart, hence no verdict");
    }
    Ok(if passing_streams < streams || passing_images < images {
        Outcome::Failed
    } else if !complete {
        Outcome::Incomplete
    } else {
        Outcome::Done
    })
}
