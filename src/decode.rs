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

pub(crate) fn decode(data: &[u8], options: &DecodeOptions) -> Result<Image, DecodeError> {
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
    let (width, height) = (info.width, info.height);
    let trees = Trees::new(width, height);

    let mut coeffs = Vec::new();
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
        marker = read_block(&mut r, &table, &mut coeffs, max)?;
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
    let mut data = quant::unquantize(&coeffs, &quant, width, height, &trees.q);
    drop(coeffs);
    let threads = crate::thread_count(options.threads);
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
