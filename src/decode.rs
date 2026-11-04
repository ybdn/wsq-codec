use crate::bits::ByteReader;
use crate::error::DecodeError;
use crate::huffman::{DecodeTable, HuffmanSpec, Item};
use crate::quant::{self, QuantTable};
use crate::tables::{
    self, COM, DHT, DQT, DTT, EOI, FrameHeader, MAX_DHT_TABLES, SOB, SOF, SOI, TransformTable,
};
use crate::tree::Trees;
use crate::{DecodeOptions, Image, Info, nistcom, par, wavelet};

/// Tables defined so far in a stream.
#[derive(Default)]
struct Tables {
    transform: Option<TransformTable>,
    quant: Option<QuantTable>,
    huffman: [Option<HuffmanSpec>; MAX_DHT_TABLES],
}

impl Tables {
    /// Reads the table or comment segment introduced by `marker`, which
    /// must be one of them.
    fn read<'a>(
        &mut self,
        marker: u16,
        r: &mut ByteReader<'a>,
    ) -> Result<Option<&'a [u8]>, DecodeError> {
        match marker {
            DTT => self.transform = Some(tables::read_transform_table(r)?),
            DQT => self.quant = Some(tables::read_quant_table(r)?),
            DHT => tables::read_huffman_tables(r, &mut self.huffman)?,
            COM => return Ok(Some(tables::read_comment(r)?)),
            m => return Err(DecodeError::UnexpectedMarker(m)),
        }
        Ok(None)
    }
}

/// Everything before the first block.
struct Header<'a> {
    frame: FrameHeader,
    tables: Tables,
    comments: Vec<&'a [u8]>,
    /// The marker that ended the header: the first block's, or EOI.
    next: u16,
}

impl Header<'_> {
    fn info(&self) -> Info {
        Info {
            width: usize::from(self.frame.width),
            height: usize::from(self.frame.height),
            ppi: self
                .comments
                .iter()
                .find(|c| nistcom::is_nistcom(c))
                .and_then(|c| nistcom::ppi(c)),
            software: self.frame.software,
            comments: self.comments.iter().map(|c| c.to_vec()).collect(),
        }
    }
}

fn read_header<'a>(r: &mut ByteReader<'a>) -> Result<Header<'a>, DecodeError> {
    if r.u16().ok() != Some(SOI) {
        return Err(DecodeError::NotWsq);
    }
    let mut tables = Tables::default();
    let mut comments = Vec::new();
    let frame = loop {
        match r.u16()? {
            SOF => break tables::read_frame_header(r)?,
            m => comments.extend(tables.read(m, r)?),
        }
    };
    if frame.width == 0 || frame.height == 0 {
        return Err(DecodeError::InvalidDimensions {
            width: usize::from(frame.width),
            height: usize::from(frame.height),
        });
    }
    let next = loop {
        match r.u16()? {
            m @ (SOB | EOI) => break m,
            m => comments.extend(tables.read(m, r)?),
        }
    };
    Ok(Header {
        frame,
        tables,
        comments,
        next,
    })
}

pub(crate) fn read_info(data: &[u8]) -> Result<Info, DecodeError> {
    Ok(read_header(&mut ByteReader::new(data))?.info())
}

/// Decodes the entropy-coded data of one block into `coeffs`, which may
/// grow to at most `max` coefficients. Returns the marker that ends it.
fn read_block(
    r: &mut ByteReader<'_>,
    table: &DecodeTable,
    coeffs: &mut Vec<i16>,
    max: usize,
) -> Result<u16, DecodeError> {
    let too_many = DecodeError::CorruptData("more coefficients than the subbands hold");
    let mut bits = r.bits();
    loop {
        match table.item(&mut bits)? {
            Item::Coeff(c) => {
                if coeffs.len() >= max {
                    return Err(too_many);
                }
                coeffs.push(c);
            }
            Item::Zeros(n) => {
                let n = n as usize;
                if coeffs.len() + n > max {
                    return Err(too_many);
                }
                coeffs.resize(coeffs.len() + n, 0);
            }
            Item::End(marker) => return Ok(marker),
        }
    }
}

/// Finds where the entropy-coded data at the start of `data` ends, as the
/// bit reader does: at the first `0xFF` not followed by a stuffed `0x00`.
/// Returns the length up to and including that marker, and the marker, or
/// `None` if the data ends first.
fn find_marker(data: &[u8]) -> Option<(usize, u16)> {
    let mut i = 0;
    loop {
        i += data.get(i..)?.iter().position(|&b| b == 0xFF)?;
        match *data.get(i + 1)? {
            0x00 => i += 2,
            m => return Some((i + 2, 0xFF00 | u16::from(m))),
        }
    }
}

/// A block found but not decoded yet.
struct Block<'a> {
    /// Its entropy-coded data, up to and including the marker that ends it.
    data: &'a [u8],
    marker: u16,
    table: DecodeTable,
    /// Most coefficients the blocks up to this one may hold together.
    max: usize,
}

/// Decodes `blocks` on up to `threads` threads and joins their
/// coefficients, or returns `None` if the serial decoder would not
/// produce the same coefficients, in which case it decides the error.
fn decode_blocks(blocks: &[Block<'_>], threads: usize) -> Option<Vec<i16>> {
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(blocks[i].data.len()));
    let decoded = par::map_dynamic(&order, threads.min(blocks.len()), |i| {
        let block = &blocks[i];
        let mut coeffs = Vec::new();
        let mut r = ByteReader::new(block.data);
        let marker = read_block(&mut r, &block.table, &mut coeffs, block.max).ok()?;
        (marker == block.marker).then_some(coeffs)
    });
    let mut total = 0;
    for (block, coeffs) in blocks.iter().zip(&decoded) {
        total += coeffs.as_ref()?.len();
        if total > block.max {
            return None;
        }
    }
    let mut decoded = decoded.into_iter().flatten();
    let mut coeffs = decoded.next().unwrap_or_default();
    coeffs.reserve_exact(total - coeffs.len());
    for block in decoded {
        coeffs.extend_from_slice(&block);
    }
    Some(coeffs)
}

/// A stream read up to its quantized coefficients.
pub(crate) struct Quantized {
    pub info: Info,
    pub frame: FrameHeader,
    pub transform: TransformTable,
    pub quant: QuantTable,
    pub trees: Trees,
    /// The coefficients of the subbands with a nonzero bin width, in
    /// subband order, each subband row by row.
    pub coeffs: Vec<i16>,
}

/// Reads every segment of a stream and decodes its entropy-coded data,
/// stopping short of dequantization.
///
/// With several threads, the blocks are first located by their markers
/// and then decoded side by side. Any failure on that path reads the
/// stream again on one thread, so that errors are the ones the serial
/// decoder reports, the earliest in the stream.
pub(crate) fn read_quantized(
    data: &[u8],
    options: &DecodeOptions,
) -> Result<Quantized, DecodeError> {
    let threads = crate::thread_count(options.threads);
    if threads > 1 {
        if let Ok(Some(quantized)) = read_quantized_with(data, options, threads) {
            return Ok(quantized);
        }
    }
    read_quantized_with(data, options, 1)
        .map(|quantized| quantized.expect("the serial decoder always concludes"))
}

/// [`read_quantized`] on up to `threads` threads. Returns `None` when the
/// parallel decoding cannot tell what the serial decoder would give.
fn read_quantized_with(
    data: &[u8],
    options: &DecodeOptions,
    threads: usize,
) -> Result<Option<Quantized>, DecodeError> {
    let mut r = ByteReader::new(data);
    let header = read_header(&mut r)?;
    let info = header.info();
    if let Some(max_pixels) = options.max_pixels {
        if info.width * info.height > max_pixels {
            return Err(DecodeError::TooLarge {
                width: info.width,
                height: info.height,
                max_pixels,
            });
        }
    }
    let Header {
        frame,
        mut tables,
        next,
        ..
    } = header;
    let trees = Trees::new(info.width, info.height);
    let parallel = threads > 1
        && tables
            .quant
            .as_ref()
            .is_some_and(|q| par::parts(threads, q.coefficient_count(&trees.q)) > 1);

    let mut coeffs = Vec::new();
    let mut blocks = Vec::new();
    let mut marker = next;
    while marker != EOI {
        if marker != SOB {
            tables.read(marker, &mut r)?;
            marker = r.u16()?;
            continue;
        }
        let id = tables::read_block_header(&mut r)?;
        let spec = tables
            .huffman
            .get(usize::from(id))
            .and_then(Option::as_ref)
            .ok_or(DecodeError::UndefinedHuffmanTable(id))?;
        let table = DecodeTable::new(spec).ok_or(DecodeError::InvalidSegment {
            marker: DHT,
            reason: "not a valid Huffman code",
        })?;
        let quant = tables
            .quant
            .as_ref()
            .ok_or(DecodeError::MissingTable("quantization table"))?;
        let max = quant.coefficient_count(&trees.q);
        if parallel {
            let Some((len, end)) = find_marker(r.rest()) else {
                return Ok(None);
            };
            let data = r.bytes(len)?;
            blocks.push(Block {
                data,
                marker: end,
                table,
                max,
            });
            marker = end;
        } else {
            marker = read_block(&mut r, &table, &mut coeffs, max)?;
        }
    }
    if parallel {
        let Some(decoded) = decode_blocks(&blocks, threads) else {
            return Ok(None);
        };
        coeffs = decoded;
    }

    let transform = tables
        .transform
        .ok_or(DecodeError::MissingTable("transform table"))?;
    let quant = tables
        .quant
        .ok_or(DecodeError::MissingTable("quantization table"))?;
    if coeffs.len() != quant.coefficient_count(&trees.q) {
        return Err(DecodeError::CorruptData(
            "fewer coefficients than the subbands hold",
        ));
    }
    Ok(Some(Quantized {
        info,
        frame,
        transform,
        quant,
        trees,
        coeffs,
    }))
}

pub(crate) fn decode(data: &[u8], options: &DecodeOptions) -> Result<Image, DecodeError> {
    let Quantized {
        info,
        frame,
        transform,
        quant,
        trees,
        coeffs,
    } = read_quantized(data, options)?;
    let (width, height) = (info.width, info.height);
    let threads = crate::thread_count(options.threads);
    let mut data = quant::unquantize(&coeffs, &quant, width, height, &trees.q, threads);
    drop(coeffs);
    wavelet::reconstruct(
        &mut data,
        width,
        &trees.w,
        &transform.lo,
        &transform.hi,
        threads,
    );

    let mut pixels = vec![0u8; data.len()];
    par::zip_chunks(&data, &mut pixels, threads, |data, pixels| {
        for (p, &v) in pixels.iter_mut().zip(data) {
            let v = (f64::from(v * frame.r_scale + frame.m_shift) + 0.5) as f32;
            // Also maps NaN, from absurd tables, to 0.
            *p = v.clamp(0.0, 255.0) as u8;
        }
    });
    Ok(Image {
        width,
        height,
        ppi: info.ppi,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EncodeOptions, encode};

    fn coefficients(data: &[u8], threads: usize) -> Result<Vec<i16>, DecodeError> {
        let options = DecodeOptions::default().with_threads(threads);
        read_quantized(data, &options).map(|q| q.coeffs)
    }

    #[test]
    fn finds_the_marker_that_ends_a_block() {
        assert_eq!(
            find_marker(&[1, 0xFF, 0, 2, 0xFF, 0xA3, 9]),
            Some((6, 0xFFA3))
        );
        assert_eq!(find_marker(&[0xFF, 0xFF, 0]), Some((2, 0xFFFF)));
        assert_eq!(find_marker(&[1, 0xFF, 0, 2]), None);
        assert_eq!(find_marker(&[1, 0xFF]), None);
    }

    #[test]
    fn parallel_blocks_give_the_serial_coefficients_and_errors() {
        // Large enough for the blocks to be decoded in parallel.
        let (w, h) = (1031usize, 777usize);
        let pixels: Vec<u8> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                (((x * 7 + y * 3) % 61) * 4 + (x * y) % 13) as u8
            })
            .collect();
        let options = EncodeOptions::default().with_bitrate(2.25);
        let wsq = encode(&pixels, w, h, &options).unwrap();
        let options = DecodeOptions::default().with_threads(4);
        assert!(matches!(
            read_quantized_with(&wsq, &options, 4),
            Ok(Some(_))
        ));
        assert_eq!(coefficients(&wsq, 4), coefficients(&wsq, 1));

        let mut state = 7u32;
        let mut rand = |n: usize| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as usize % n
        };
        for _ in 0..200 {
            let mut data = wsq.clone();
            for _ in 0..1 + rand(3) {
                let at = rand(data.len());
                // Stray 0xFF bytes make markers appear and disappear.
                data[at] = if rand(2) == 0 { 0xFF } else { rand(256) as u8 };
            }
            assert_eq!(coefficients(&data, 4), coefficients(&data, 1));
        }
    }
}
