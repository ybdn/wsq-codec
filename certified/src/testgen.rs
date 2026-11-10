//! Generator of structurally valid WSQ streams for tests: random
//! dimensions, filter banks, quantization tables and coefficients, so that
//! the wavelet and quantization code sees inputs that real encoders never
//! write. Also the source of the seeds of the `fuzz` targets.

use crate::tree::{NUM_SUBBANDS, Trees};

/// xorshift64: small, deterministic and good enough to vary streams.
pub(crate) struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A value in `0..n`, `n > 0`.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A value in `a..=b`.
    pub fn range(&mut self, a: u64, b: u64) -> u64 {
        a + self.below(b - a + 1)
    }
}

fn scaled(out: &mut Vec<u8>, scale: u8, v: u16) {
    out.push(scale);
    out.extend_from_slice(&v.to_be_bytes());
}

/// A valid stream of random dimensions from 1 to 48.
pub(crate) fn random_stream(r: &mut Rng) -> Vec<u8> {
    let w = r.range(1, 48) as u16;
    let h = r.range(1, 48) as u16;
    stream(r, w, h, false)
}

/// A valid stream declaring `width` by `height` pixels in which every
/// quantizer bin is zero, so that it carries no coefficient. A flat image
/// legitimately produces such a stream, whatever its size.
pub(crate) fn empty_stream(r: &mut Rng, width: u16, height: u16) -> Vec<u8> {
    stream(r, width, height, true)
}

/// A structurally valid stream with random filters, tables and, unless
/// `empty`, exactly as many coefficients as the subbands with a nonzero bin
/// width hold.
fn stream(r: &mut Rng, w: u16, h: u16, empty: bool) -> Vec<u8> {
    let mut out = vec![0xFF, 0xA0];
    // Transform table.
    let hisz = r.range(2, 64) as u8;
    let losz = r.range(2, 64) as u8;
    let n = usize::from(hisz).div_ceil(2) + usize::from(losz).div_ceil(2);
    out.extend_from_slice(&[0xFF, 0xA4]);
    out.extend_from_slice(&((4 + 6 * n) as u16).to_be_bytes());
    out.push(hisz);
    out.push(losz);
    for _ in 0..n {
        out.push(r.below(2) as u8);
        out.push(r.range(0, 12) as u8);
        out.extend_from_slice(&((r.next() as u32) >> r.below(32)).to_be_bytes());
    }
    // Quantization table.
    let mut q = [0u16; 64];
    out.extend_from_slice(&[0xFF, 0xA5, 1, 133]);
    scaled(&mut out, r.range(0, 3) as u8, r.below(100) as u16);
    for q in &mut q {
        *q = if empty || r.below(4) == 0 {
            0
        } else {
            r.range(1, 65535) as u16
        };
        scaled(&mut out, r.range(0, 6) as u8, *q);
        scaled(&mut out, r.range(0, 6) as u8, r.below(65536) as u16);
    }
    // Frame header.
    out.extend_from_slice(&[0xFF, 0xA2, 0, 17, 0, 255]);
    out.extend_from_slice(&h.to_be_bytes());
    out.extend_from_slice(&w.to_be_bytes());
    scaled(&mut out, r.range(0, 3) as u8, r.below(65536) as u16);
    scaled(&mut out, r.range(0, 5) as u8, r.below(65536) as u16);
    out.push(2);
    out.extend_from_slice(&[0, 0]);
    // Huffman table: 254 eight-bit codes, symbol s has code s - 1.
    out.extend_from_slice(&[0xFF, 0xA6]);
    out.extend_from_slice(&((3 + 16 + 254) as u16).to_be_bytes());
    out.push(0);
    let mut bits = [0u8; 16];
    bits[7] = 254;
    out.extend_from_slice(&bits);
    out.extend(1..=254u8);
    // One block with the coefficients.
    let trees = Trees::new(usize::from(w), usize::from(h));
    let count: usize = (0..NUM_SUBBANDS)
        .filter(|&c| q[c] != 0)
        .map(|c| trees.q[c].area())
        .sum();
    out.extend_from_slice(&[0xFF, 0xA3, 0, 3, 0]);
    let put = |out: &mut Vec<u8>, b: u8| {
        out.push(b);
        if b == 0xFF {
            out.push(0);
        }
    };
    let mut left = count;
    while left > 0 {
        match r.below(6) {
            0 => {
                let z = r.range(1, 100.min(left as u64)) as u8;
                put(&mut out, z - 1);
                left -= usize::from(z);
            }
            1 => {
                put(&mut out, 105 - 1);
                let z = r.range(1, 255.min(left as u64)) as u8;
                put(&mut out, z);
                left -= usize::from(z);
            }
            2 => {
                put(&mut out, 103 + r.below(2) as u8 - 1);
                let v = r.next() as u16;
                put(&mut out, (v >> 8) as u8);
                put(&mut out, v as u8);
                left -= 1;
            }
            3 => {
                put(&mut out, 101 + r.below(2) as u8 - 1);
                put(&mut out, r.next() as u8);
                left -= 1;
            }
            _ => {
                put(&mut out, r.range(107, 254) as u8 - 1);
                left -= 1;
            }
        }
    }
    out.extend_from_slice(&[0xFF, 0xA1]);
    out
}

/// Replaces 1 to 3 random bytes of `data`.
pub(crate) fn mutate(r: &mut Rng, data: &mut [u8]) {
    for _ in 0..r.range(1, 3) {
        let at = r.below(data.len() as u64) as usize;
        data[at] = r.next() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecodeOptions, decode_with, inspect, read_info};

    /// Valid streams of random shapes, and copies with a few bytes changed,
    /// reach the wavelet and quantization code with odd dimensions and
    /// every filter length. Nothing may panic, whatever the build.
    #[test]
    fn generated_streams_decode_or_fail_cleanly() {
        let options = DecodeOptions::default();
        let mut r = Rng(0x9E37_79B9_7F4A_7C15);
        let mut decoded = 0;
        for _ in 0..ITERATIONS {
            let valid = random_stream(&mut r);
            let mut corrupt = valid.clone();
            mutate(&mut r, &mut corrupt);
            for (i, data) in [valid, corrupt].iter().enumerate() {
                let result = decode_with(data, &options);
                if let Ok(image) = &result {
                    assert_eq!(image.pixels.len(), image.width * image.height);
                    decoded += usize::from(i == 0);
                }
                let _ = read_info(data);
                let _ = inspect::read_quantized(data, &options);
            }
        }
        // The generator must reach the decoder's last stage.
        assert!(decoded > ITERATIONS / 2, "{decoded} of {ITERATIONS}");
    }

    #[cfg(not(debug_assertions))]
    const ITERATIONS: usize = 4000;
    #[cfg(debug_assertions)]
    const ITERATIONS: usize = 600;

    /// Writes the seeds of the `fuzz` targets into `$WSQ_SEED_DIR`: a few
    /// generated streams and a tiny encoder output for the stream targets,
    /// in `streams/`, and two inputs for `roundtrip`, in `roundtrip/`.
    ///
    /// `WSQ_SEED_DIR=$PWD/fuzz/seeds cargo test -p wsq-certified --lib write_fuzz_seeds -- --ignored`
    #[test]
    #[ignore = "writes files; run by hand to regenerate fuzz/seeds"]
    fn write_fuzz_seeds() {
        let dir = std::path::PathBuf::from(std::env::var_os("WSQ_SEED_DIR").unwrap());
        let (streams, roundtrip) = (dir.join("streams"), dir.join("roundtrip"));
        std::fs::create_dir_all(&streams).unwrap();
        std::fs::create_dir_all(&roundtrip).unwrap();
        let mut r = Rng(0xC0FF_EE00_1234_5678);
        let mut seeds = Vec::new();
        // Fixed shapes: 1 pixel, narrow, odd, and one that is mutated.
        for (w, h) in [(1, 1), (7, 40), (33, 21)] {
            seeds.push(stream(&mut r, w, h, false));
        }
        let mut mutated = random_stream(&mut r);
        mutate(&mut r, &mut mutated);
        seeds.push(mutated);
        // A real encoder output, from integer-only pixels.
        let pixels: Vec<u8> = (0..81 * 81)
            .map(|i| (((i % 81) * 5 + (i / 81) * 3) % 251) as u8)
            .collect();
        let encoded = crate::encode(&pixels, 81, 81, &crate::EncodeOptions::default()).unwrap();
        seeds.push(encoded);
        for (i, seed) in seeds.iter().enumerate() {
            std::fs::write(streams.join(format!("seed{i}.wsq")), seed).unwrap();
        }
        // Width 81 + 0, height 81 + 0, bit rate 0.02 + 24 / 32, 500 ppi, 1 thread.
        std::fs::write(
            roundtrip.join("seed0"),
            [0, 0, 0, 0, 24, 1, 1, 10, 200, 30, 255],
        )
        .unwrap();
        std::fs::write(roundtrip.join("seed1"), [0, 9, 0, 3, 64, 0, 2, 0]).unwrap();
    }

    /// A stream without coefficients is a flat image, however large it
    /// declares itself.
    #[test]
    fn empty_streams_decode_to_flat_images() {
        let data = empty_stream(&mut Rng(5), 600, 500);
        let image = decode_with(&data, &DecodeOptions::default()).unwrap();
        assert_eq!((image.width, image.height), (600, 500));
        assert!(image.pixels.iter().all(|&p| p == image.pixels[0]));
        let quantized = inspect::read_quantized(&data, &DecodeOptions::default()).unwrap();
        assert!(quantized.subbands.iter().all(|s| s.coefficients.is_empty()));
    }

    /// On a 32-bit target the transformed image of the largest header does
    /// not fit in the address space: an error, not a "capacity overflow"
    /// panic. Run by the 32-bit CI job.
    #[cfg(target_pointer_width = "32")]
    #[test]
    fn a_header_beyond_the_address_space_is_an_error() {
        let data = empty_stream(&mut Rng(3), u16::MAX, u16::MAX);
        let options = DecodeOptions::default().with_max_pixels(None);
        assert_eq!(
            decode_with(&data, &options).unwrap_err(),
            crate::DecodeError::OutOfMemory
        );
    }
}
