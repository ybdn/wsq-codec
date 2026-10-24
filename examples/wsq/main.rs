//! Converts between binary PGM (P5) and WSQ files.
//!
//! ```text
//! cargo run --example wsq -- encode in.pgm out.wsq [bitrate] [ppi]
//! cargo run --example wsq -- decode in.wsq out.pgm
//! cargo run --example wsq -- info in.wsq
//! ```

use std::error::Error;
use std::{env, fs, process};

/// Parses a binary PGM with a maximum value of 255.
fn read_pgm(data: &[u8]) -> Result<(usize, usize, &[u8]), Box<dyn Error>> {
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

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    match args {
        [cmd, input, output, rest @ ..] if cmd == "encode" && rest.len() <= 2 => {
            let data = fs::read(input)?;
            let (width, height, pixels) = read_pgm(&data)?;
            let mut options = wsq_codec::EncodeOptions::default();
            if let Some(bitrate) = rest.first() {
                options.bitrate = bitrate.parse()?;
            }
            options.ppi = rest.get(1).map(|p| p.parse()).transpose()?;
            fs::write(output, wsq_codec::encode(pixels, width, height, &options)?)?;
        }
        [cmd, input, output] if cmd == "decode" => {
            let image = wsq_codec::decode(&fs::read(input)?)?;
            let mut pgm = format!("P5\n{} {}\n255\n", image.width, image.height).into_bytes();
            pgm.extend_from_slice(&image.pixels);
            fs::write(output, pgm)?;
        }
        [cmd, input] if cmd == "info" => {
            let info = wsq_codec::read_info(&fs::read(input)?)?;
            println!("{}x{}", info.width, info.height);
            match info.ppi {
                Some(ppi) => println!("{ppi} ppi"),
                None => println!("resolution unknown"),
            }
            println!("software {}", info.software);
            for comment in &info.comments {
                println!("comment:\n{}", String::from_utf8_lossy(comment).trim_end_matches('\0'));
            }
        }
        _ => return Err("usage: wsq encode IN.pgm OUT.wsq [BITRATE] [PPI] | decode IN.wsq OUT.pgm | info IN.wsq".into()),
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("wsq: {e}");
        process::exit(1);
    }
}
