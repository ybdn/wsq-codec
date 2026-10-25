//! Integration with the [`image`] crate, behind the `image`
//! feature.
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use image::GrayImage;
//! use wsq_codec::EncodeOptions;
//!
//! let gray = GrayImage::from_fn(256, 200, |x, y| image::Luma([((x * 7 + y * 3) % 200) as u8]));
//! let wsq = wsq_codec::image::encode_gray_image(&gray, &EncodeOptions::default())?;
//!
//! let decoded: GrayImage = wsq_codec::decode(&wsq)?.into();
//! assert_eq!(decoded.dimensions(), (256, 200));
//!
//! // Or let `image` open WSQ files itself.
//! wsq_codec::image::register_hooks();
//! # Ok(())
//! # }
//! ```

use std::io::Read;

use ::image::error::{DecodingError, ImageFormatHint};
use ::image::{ColorType, GrayImage, ImageDecoder, ImageError, ImageResult};

use crate::{DecodeError, DecodeOptions, EncodeError, EncodeOptions, Image, Info};

impl From<Image> for GrayImage {
    fn from(image: Image) -> Self {
        // The format limits both dimensions to 65535.
        GrayImage::from_raw(image.width as u32, image.height as u32, image.pixels)
            .expect("a decoded image holds width * height pixels")
    }
}

/// Encodes a grayscale image, whose dimensions must be between 81 and
/// 65535 as for [`encode`](crate::encode).
pub fn encode_gray_image(
    image: &GrayImage,
    options: &EncodeOptions,
) -> Result<Vec<u8>, EncodeError> {
    let (width, height) = image.dimensions();
    crate::encode(image.as_raw(), width as usize, height as usize, options)
}

fn decoding_error(e: DecodeError) -> ImageError {
    ImageError::Decoding(DecodingError::new(ImageFormatHint::Name("WSQ".into()), e))
}

/// An [`ImageDecoder`] for WSQ streams, which `image` uses once
/// [`register_hooks`] has run, and which can also be used directly.
///
/// It reads the stream and its header when created, and decodes the image
/// when asked for its pixels.
pub struct WsqDecoder {
    data: Vec<u8>,
    info: Info,
    options: DecodeOptions,
}

impl WsqDecoder {
    /// Reads a whole WSQ stream from `reader`, which should end with it
    /// (see [`decode_from`](crate::decode_from)), to decode it with the
    /// default [`DecodeOptions`].
    pub fn new<R: Read>(reader: R) -> ImageResult<Self> {
        Self::with_options(reader, DecodeOptions::default())
    }

    /// Like [`new`](Self::new), with the given [`DecodeOptions`].
    pub fn with_options<R: Read>(mut reader: R, options: DecodeOptions) -> ImageResult<Self> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        let info = crate::read_info(&data).map_err(decoding_error)?;
        Ok(WsqDecoder {
            data,
            info,
            options,
        })
    }

    /// What the stream says about its image.
    pub fn info(&self) -> &Info {
        &self.info
    }
}

impl ImageDecoder for WsqDecoder {
    fn dimensions(&self) -> (u32, u32) {
        (self.info.width as u32, self.info.height as u32)
    }

    fn color_type(&self) -> ColorType {
        ColorType::L8
    }

    fn read_image(self, buf: &mut [u8]) -> ImageResult<()> {
        let image = crate::decode_with(&self.data, &self.options).map_err(decoding_error)?;
        buf.copy_from_slice(&image.pixels);
        Ok(())
    }

    fn read_image_boxed(self: Box<Self>, buf: &mut [u8]) -> ImageResult<()> {
        (*self).read_image(buf)
    }
}

/// Lets `image` open WSQ streams, recognized by their `wsq` extension or
/// by their start-of-image marker, with [`WsqDecoder`] and the default
/// [`DecodeOptions`]. Returns `false` if a decoder for the `wsq` extension
/// was already registered, in which case it stays in place.
pub fn register_hooks() -> bool {
    let registered = ::image::hooks::register_decoding_hook(
        "wsq".into(),
        Box::new(|reader| Ok(Box::new(WsqDecoder::new(reader)?))),
    );
    if registered {
        ::image::hooks::register_format_detection_hook("wsq".into(), &[0xFF, 0xA0], None);
    }
    registered
}
