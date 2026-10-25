//! Converts between binary PGM (P5) and WSQ files, and checks the decoder
//! on a collection of real files.
//!
//! ```text
//! cargo run --release --example wsq -- encode in.pgm out.wsq [bitrate] [ppi]
//! cargo run --release --example wsq -- decode in.wsq out.pgm
//! cargo run --release --example wsq -- info in.wsq
//! cargo run --release --example wsq -- check DIR
//! cargo run --release --example wsq -- extract FILE
//! ```
//!
//! `check` decodes every WSQ stream under DIR, including streams inside
//! other files such as ANSI/NIST-ITL transactions, and compares them with
//! reference decodes stored next to them (see `corpus.rs`). `extract`
//! writes the WSQ streams inside a file next to it.

mod corpus;

use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::{env, fs, process};

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    match args {
        [cmd, input, output, rest @ ..] if cmd == "encode" && rest.len() <= 2 => {
            let data = fs::read(input)?;
            let (width, height, pixels) = corpus::read_pgm(&data)?;
            let mut options = wsq_codec::EncodeOptions::default();
            if let Some(bitrate) = rest.first() {
                options.bitrate = bitrate.parse()?;
            }
            options.ppi = rest.get(1).map(|p| p.parse()).transpose()?;
            let file = BufWriter::new(File::create(output)?);
            wsq_codec::encode_to(file, pixels, width, height, &options)?;
        }
        [cmd, input, output] if cmd == "decode" => {
            let options = wsq_codec::DecodeOptions::default();
            let image = wsq_codec::decode_from(File::open(input)?, &options)?;
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
                println!(
                    "comment:\n{}",
                    String::from_utf8_lossy(comment).trim_end_matches('\0')
                );
            }
        }
        [cmd, dir] if cmd == "check" => corpus::check(Path::new(dir))?,
        [cmd, file] if cmd == "extract" => corpus::extract(Path::new(file))?,
        _ => {
            return Err(
                "usage: wsq encode IN.pgm OUT.wsq [BITRATE] [PPI] | decode IN.wsq OUT.pgm \
                        | info IN.wsq | check DIR | extract FILE"
                    .into(),
            );
        }
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
