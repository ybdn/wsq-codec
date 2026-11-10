//! Converts between binary PGM (P5) and WSQ files, and checks the decoder
//! on a collection of real files.
//!
//! ```text
//! wsq --help | --version
//! wsq encode [--profile conformant|optimized] in.pgm out.wsq [bitrate] [ppi]
//! wsq decode in.wsq out.pgm
//! wsq info in.wsq
//! wsq check DIR
//! wsq extract FILE
//! wsq cert encode|decode|compare ...
//! ```
//!
//! Exit status: 0 on success, 1 on an error (unreadable or invalid file),
//! 64 on a usage error; `cert` also uses 2 and 3 (see `cert.rs`).
//!
//! `check` decodes every WSQ stream under DIR, including streams inside
//! other files such as ANSI/NIST-ITL transactions, and compares them with
//! reference decodes stored next to them (see `corpus.rs`). `extract`
//! writes the WSQ streams inside a file next to it. `cert` runs the FBI
//! certification tests locally (see `cert.rs` and docs/certification.md).

mod accuracy;
mod cert;
mod corpus;

use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::{env, fs, process};

const USAGE: &str = "\
Usage:
  wsq encode [--profile P] IN.pgm OUT.wsq [BITRATE] [PPI]
                                              compress a binary PGM (P5); P is `conformant`
                                              (default, as the reference) or `optimized`
                                              (meets the requested size, never certified)
  wsq decode IN.wsq OUT.pgm                   decompress to a binary PGM
  wsq info IN.wsq                             print the header, without decoding
  wsq check DIR                               decode every WSQ stream under DIR
  wsq extract FILE                            write the WSQ streams inside FILE
  wsq cert encode|decode|compare ...          local FBI certification tests
  wsq --help | --version

Exit status: 0 success, 1 error, 64 usage error; `cert` also uses 2 and 3.";

/// Takes `--profile P` or `--profile=P` out of the arguments of `encode`, wherever it stands.
/// `Err` carries a usage error message.
fn take_profile(args: &[String]) -> Result<(wsq_codec::Profile, Vec<String>), String> {
    let mut profile = wsq_codec::Profile::Conformant;
    let mut rest = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let value = if arg == "--profile" {
            args.next()
                .map(String::as_str)
                .ok_or("--profile needs a value: conformant or optimized")?
        } else if let Some(value) = arg.strip_prefix("--profile=") {
            value
        } else {
            rest.push(arg.clone());
            continue;
        };
        profile = match value {
            "conformant" => wsq_codec::Profile::Conformant,
            "optimized" => wsq_codec::Profile::Optimized,
            other => {
                return Err(format!(
                    "unknown profile `{other}`: use conformant or optimized"
                ));
            }
        };
    }
    Ok((profile, rest))
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let (profile, args) = match args.first().map(String::as_str) {
        Some("encode") => match take_profile(&args[1..]) {
            Ok((profile, rest)) => (profile, [&args[..1], &rest].concat()),
            Err(message) => {
                eprintln!("wsq: {message}\n\n{USAGE}");
                process::exit(64);
            }
        },
        _ => (wsq_codec::Profile::Conformant, args.to_vec()),
    };
    match &args[..] {
        [cmd, input, output, rest @ ..] if cmd == "encode" && rest.len() <= 2 => {
            let data = fs::read(input)?;
            let (width, height, pixels) = corpus::read_pgm(&data)?;
            let mut options = wsq_codec::EncodeOptions::default().with_profile(profile);
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
        [cmd, rest @ ..] if cmd == "cert" => {
            let outcome = cert::run(rest)?;
            if outcome != cert::Outcome::Done {
                process::exit(outcome.code());
            }
        }
        _ => {
            eprintln!("wsq: invalid arguments\n\n{USAGE}");
            process::exit(64);
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") => {
            println!(
                "wsq {}: WSQ fingerprint image tool\n\n{USAGE}",
                env!("CARGO_PKG_VERSION")
            );
            return;
        }
        Some("-V" | "--version") => {
            println!("wsq {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        _ => {}
    }
    if let Err(e) = run(&args) {
        // The error and its causes, as `ReadError::Io` keeps the I/O error as its source.
        let mut message = format!("wsq: {e}");
        let mut source = e.source();
        while let Some(cause) = source {
            message.push_str(&format!(": {cause}"));
            source = cause.source();
        }
        eprintln!("{message}");
        process::exit(1);
    }
}
