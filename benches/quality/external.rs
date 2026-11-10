//! The external tools that measure what PSNR does not: NFIQ 2 (NIST's fingerprint quality
//! score), and NBIS's `mindtct` (minutiae) and `bozorth3` (matcher). They are C and C++
//! programs that never link to this crate; they run as processes, and each is optional.
//!
//! Images go to them as files in a work directory: an 8-bit grayscale PNG with its resolution
//! for NFIQ 2, and NBIS's "IHead" format (a 288-byte header, then raw pixels) for `mindtct`,
//! which reads ANSI/NIST, WSQ, JPEG and IHead files but not raw ones.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::{fs, process};

/// Where the tools are. A tool left out is "not run".
#[derive(Default, Clone)]
pub struct Tools {
    pub nfiq2: Option<PathBuf>,
    /// NFIQ 2's model information file (`nist_plain_tir-ink.txt`); by default the one in the
    /// `share` directory next to the tool's `bin`, or else the tool's own search (the current
    /// directory).
    pub nfiq2_model: Option<PathBuf>,
    pub mindtct: Option<PathBuf>,
    pub bozorth3: Option<PathBuf>,
}

impl Tools {
    pub fn any(&self) -> bool {
        self.nfiq2.is_some() || self.mindtct.is_some()
    }
}

/// What the tools say about one image.
#[derive(Default, Clone)]
pub struct Reading {
    /// NFIQ 2 score, 0 (worst) to 100 (best).
    pub nfiq2: Option<u32>,
    /// Minutiae `mindtct` found, with the template's path for the matcher.
    pub minutiae: Option<usize>,
    pub template: Option<PathBuf>,
}

/// One image's reading, and its match against the original's template.
#[derive(Default, Clone)]
pub struct Fp {
    pub reading: Reading,
    /// `bozorth3` score of this image's minutiae against the original's.
    pub matched: Option<u32>,
}

/// An 8-bit grayscale PNG with a pHYs chunk (`ppi` pixels per inch), deflate-stored.
pub fn png(width: usize, height: usize, pixels: &[u8], ppi: u32) -> Vec<u8> {
    fn crc32(parts: &[&[u8]]) -> u32 {
        let mut crc = !0u32;
        for part in parts {
            for &byte in *part {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xedb8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
    }
    let mut raw = Vec::with_capacity((width + 1) * height);
    for row in pixels.chunks(width) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(i + 1 == blocks.len()));
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]); // 8 bits, grayscale, no interlace
    let per_metre = (u64::from(ppi) * 10_000 / 254) as u32; // 500 ppi is 19685 pixels/m
    let mut phys = Vec::new();
    phys.extend_from_slice(&per_metre.to_be_bytes());
    phys.extend_from_slice(&per_metre.to_be_bytes());
    phys.push(1); // metre
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"pHYs", &phys);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// An NBIS "IHead" image: the 8-byte header length ("288"), the 288-byte header (fixed-size
/// ASCII fields, `ihead.h`), then the uncompressed pixels.
pub fn ihead(width: usize, height: usize, pixels: &[u8], ppi: u32) -> Vec<u8> {
    fn field(out: &mut Vec<u8>, text: &str, size: usize) {
        let mut bytes = text.as_bytes().to_vec();
        bytes.resize(size, 0);
        out.extend_from_slice(&bytes);
    }
    let mut out = Vec::new();
    field(&mut out, "288", 8);
    field(&mut out, "wsq-codec quality benchmark", 80); // id
    field(&mut out, "", 26); // created
    for value in [
        width.to_string(),
        height.to_string(),
        "8".into(),      // depth
        ppi.to_string(), // density
        "0".into(),      // compress: none
        "0".into(),      // complen
        "8".into(),      // align
        "8".into(),      // unitsize
    ] {
        field(&mut out, &value, 8);
    }
    out.extend_from_slice(b"00"); // sigbit, byte_order
    field(&mut out, "0", 8); // pix_offset
    field(&mut out, "255", 8); // whitepix
    out.extend_from_slice(b"0000"); // issigned, rm_cm, tb_bt, lr_rl
    field(&mut out, "", 80); // parent
    field(&mut out, "0", 8); // par_x
    field(&mut out, "0", 8); // par_y
    debug_assert_eq!(out.len(), 8 + 288);
    out.extend_from_slice(pixels);
    out
}

fn stdout_of(program: &Path, args: &[&std::ffi::OsStr]) -> Result<String, Box<dyn Error>> {
    let out = process::Command::new(program)
        .args(args)
        .stdin(process::Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", program.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{} failed with {}: {}",
            program.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .next()
                .unwrap_or("")
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// NFIQ 2 score of the PNG at `path`: the `QualityScore` column of the tool's CSV output.
/// `None` when the tool reports an error for this image (it prints `NA` or an error code).
fn nfiq2_score(
    tool: &Path,
    model: Option<&Path>,
    path: &Path,
) -> Result<Option<u32>, Box<dyn Error>> {
    let mut args: Vec<&std::ffi::OsStr> = vec!["-F".as_ref()];
    let model = model.map(Path::to_path_buf).or_else(|| {
        let share = tool
            .parent()?
            .parent()?
            .join("share/nist_plain_tir-ink.txt");
        share.is_file().then_some(share)
    });
    if let Some(model) = &model {
        args.extend(["-m".as_ref(), model.as_os_str()]);
    }
    args.push(path.as_os_str());
    let text = stdout_of(tool, &args)?;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or("nfiq2 printed nothing")?;
    // For one image the tool prints the score alone; for several it prints a CSV with a header.
    if let Ok(score) = header.trim().parse::<u32>() {
        return Ok(Some(score).filter(|&v| v <= 100));
    }
    let column = header
        .split(',')
        .position(|c| c.trim_matches('"') == "QualityScore")
        .ok_or_else(|| {
            format!(
                "unexpected nfiq2 output: {}",
                header.chars().take(80).collect::<String>()
            )
        })?;
    let row = lines.next().ok_or("nfiq2 printed no score")?;
    Ok(row
        .split(',')
        .nth(column)
        .and_then(|v| v.trim_matches('"').parse::<u32>().ok())
        .filter(|&v| v <= 100))
}

/// Measures one image with the tools that are present. `key` names the files in `work`.
pub fn read(
    tools: &Tools,
    work: &Path,
    key: &str,
    width: usize,
    height: usize,
    pixels: &[u8],
    ppi: u32,
) -> Result<Reading, Box<dyn Error>> {
    fs::create_dir_all(work)?;
    let mut reading = Reading::default();
    if let Some(tool) = &tools.nfiq2 {
        let file = work.join(format!("{key}.png"));
        fs::write(&file, png(width, height, pixels, ppi))?;
        reading.nfiq2 = nfiq2_score(tool, tools.nfiq2_model.as_deref(), &file)?;
        fs::remove_file(&file)?;
    }
    if let Some(tool) = &tools.mindtct {
        let file = work.join(format!("{key}.ihead"));
        fs::write(&file, ihead(width, height, pixels, ppi))?;
        let root = work.join(key);
        stdout_of(tool, &[file.as_os_str(), root.as_os_str()])?;
        fs::remove_file(&file)?;
        let template = work.join(format!("{key}.xyt"));
        reading.minutiae = Some(fs::read_to_string(&template)?.lines().count());
        reading.template = Some(template);
        // mindtct also writes maps and the binarized image: only the template is kept.
        for ext in ["brw", "dm", "hcm", "lcm", "lfm", "min", "qm"] {
            let _ = fs::remove_file(work.join(format!("{key}.{ext}")));
        }
    }
    Ok(reading)
}

/// `bozorth3` score of `probe` against `gallery` (default options: at most 150 minutiae each,
/// no score below 10 minutiae).
pub fn bozorth3(tool: &Path, probe: &Path, gallery: &Path) -> Result<u32, Box<dyn Error>> {
    let text = stdout_of(tool, &[probe.as_os_str(), gallery.as_os_str()])?;
    Ok(text
        .split_whitespace()
        .next()
        .ok_or("bozorth3 printed nothing")?
        .parse()?)
}
