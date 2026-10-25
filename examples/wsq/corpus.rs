//! Checking the decoder against a collection of real files.
//!
//! Files are either WSQ streams or containers, such as ANSI/NIST-ITL
//! transactions, with WSQ streams inside them; streams are found by
//! scanning for their start-of-image marker. A reference decode next to a
//! stream is compared pixel for pixel:
//!
//! - `name.raw` or `name.pgm` for a WSQ file `name.wsq`;
//! - `name.N.raw` or `name.N.pgm` for the stream numbered `N` (from 1) in
//!   any other file `name.ext`, numbered as `extract` writes them.
//!
//! `.raw` files hold `width * height` bytes, as NBIS's `dwsq` writes them
//! with its raw output option. `.pgm` files are binary 8-bit PGM (P5),
//! which `read_pgm` also parses for the PGM conversions.

use std::error::Error;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Parses a binary PGM with a maximum value of 255.
pub fn read_pgm(data: &[u8]) -> Result<(usize, usize, &[u8]), Box<dyn Error>> {
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
    Ok((width, height, pixels))
}

/// Whether a WSQ start-of-image marker followed by another WSQ marker
/// starts at `i`.
fn starts_stream(data: &[u8], i: usize) -> bool {
    matches!(data.get(i..i + 4), Some([0xFF, 0xA0, 0xFF, 0xA2..=0xA8]))
}

/// End of the WSQ stream starting at `start`, found by walking its marker
/// segments, or `None` if they do not lead to an end-of-image marker.
fn stream_end(data: &[u8], start: usize) -> Option<usize> {
    let u16_at = |i: usize| {
        Some(usize::from(u16::from_be_bytes([
            *data.get(i)?,
            *data.get(i + 1)?,
        ])))
    };
    let mut i = start + 2;
    loop {
        if *data.get(i)? != 0xFF {
            return None;
        }
        let marker = *data.get(i + 1)?;
        i += 2;
        match marker {
            0xA1 => return Some(i),
            0xA2..=0xA8 => {
                i += u16_at(i)?;
                if marker == 0xA3 {
                    // Entropy-coded data runs to the next marker.
                    while !(*data.get(i)? == 0xFF && *data.get(i + 1)? != 0) {
                        i += if data[i] == 0xFF { 2 } else { 1 };
                    }
                }
            }
            _ => return None,
        }
    }
}

/// Byte ranges of the WSQ streams in `data`. A stream whose end cannot be
/// found runs to the end of `data`; the decoder stops at its own end.
pub fn find_streams(data: &[u8]) -> Vec<Range<usize>> {
    let mut streams = Vec::new();
    // A WSQ file: decode it whatever follows its start-of-image marker.
    if data.starts_with(&[0xFF, 0xA0]) {
        streams.push(0..stream_end(data, 0).unwrap_or(data.len()));
        return streams;
    }
    let mut i = 0;
    while i + 4 <= data.len() {
        if starts_stream(data, i) {
            let end = stream_end(data, i).unwrap_or(data.len());
            streams.push(i..end);
            i = end;
        } else {
            i += 1;
        }
    }
    streams
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out)?;
        } else if !matches!(extension(&path).as_str(), "raw" | "pgm") {
            out.push(path);
        }
    }
    Ok(())
}

fn extension(path: &Path) -> String {
    path.extension()
        .map_or_else(String::new, |e| e.to_string_lossy().to_lowercase())
}

/// Path of the stream file or reference decode for stream `n` of `path`.
fn sibling(path: &Path, n: usize, ext: &str) -> PathBuf {
    if extension(path) == "wsq" {
        path.with_extension(ext)
    } else {
        path.with_extension(format!("{n}.{ext}"))
    }
}

/// The reference pixels for stream `n` of `path`, if any.
fn reference(path: &Path, n: usize) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
    if let Ok(raw) = fs::read(sibling(path, n, "raw")) {
        return Ok(Some(raw));
    }
    match fs::read(sibling(path, n, "pgm")) {
        Ok(pgm) => Ok(Some(read_pgm(&pgm)?.2.to_vec())),
        Err(_) => Ok(None),
    }
}

/// Decodes every stream under `dir` and prints a report. Fails if a
/// stream does not decode or differs from its reference.
pub fn check(dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut paths = Vec::new();
    files(dir, &mut paths)?;
    let (mut streams, mut failed, mut compared, mut differing) = (0, 0, 0, 0);
    for path in &paths {
        let data = fs::read(path)?;
        for (n, range) in find_streams(&data).into_iter().enumerate() {
            let n = n + 1;
            streams += 1;
            let label = format!("{}#{n}", path.display());
            let options = wsq_codec::DecodeOptions::default().with_max_pixels(None);
            let start = Instant::now();
            let image = match wsq_codec::decode_with(&data[range.clone()], &options) {
                Ok(image) => image,
                Err(e) => {
                    failed += 1;
                    println!("FAIL  {label}: {e}");
                    continue;
                }
            };
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            let info = wsq_codec::read_info(&data[range])?;
            let ppi = image.ppi.map_or_else(|| "?".to_owned(), |p| p.to_string());
            let verdict = match reference(path, n)? {
                None => "no reference".to_owned(),
                Some(expected) => {
                    compared += 1;
                    if expected.len() != image.pixels.len() {
                        differing += 1;
                        format!("DIFFERS: reference has {} pixels", expected.len())
                    } else {
                        let diffs: Vec<u8> = expected
                            .iter()
                            .zip(&image.pixels)
                            .map(|(&a, &b)| a.abs_diff(b))
                            .filter(|&d| d > 0)
                            .collect();
                        if diffs.is_empty() {
                            "identical to reference".to_owned()
                        } else {
                            differing += 1;
                            let max = diffs.iter().max().unwrap_or(&0);
                            format!("DIFFERS: {} pixels, by up to {max}", diffs.len())
                        }
                    }
                }
            };
            println!(
                "ok    {label}: {}x{} {ppi} ppi, software {}, {ms:.1} ms, {verdict}",
                image.width, image.height, info.software
            );
        }
    }
    println!(
        "\n{} files, {streams} WSQ streams: {failed} failed to decode, \
         {compared} compared with a reference, {differing} differ",
        paths.len()
    );
    if failed + differing > 0 {
        return Err("some streams failed".into());
    }
    Ok(())
}

/// Writes each WSQ stream of `path` next to it, as `name.N.wsq`.
pub fn extract(path: &Path) -> Result<(), Box<dyn Error>> {
    let data = fs::read(path)?;
    let streams = find_streams(&data);
    for (n, range) in streams.iter().enumerate() {
        let out = sibling(path, n + 1, "wsq");
        fs::write(&out, &data[range.clone()])?;
        println!("{}", out.display());
    }
    if streams.is_empty() {
        return Err(format!("no WSQ stream in {}", path.display()).into());
    }
    Ok(())
}
