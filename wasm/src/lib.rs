//! WebAssembly bindings for [`wsq_codec`]: a thin wrapper that moves bytes and metadata
//! between JavaScript and the codec. All the work is the codec's.
//!
//! The target has no threads, so every call runs on the calling thread (`threads = 1` is
//! set explicitly). Errors become JavaScript exceptions (`Error`) carrying the `Display`
//! message of the codec's error.

use wasm_bindgen::prelude::*;
use wsq_codec::{DecodeOptions, EncodeOptions, Profile};

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** Options of `encode`; every field is optional. */
export interface EncodeOptions {
  /** Target bit rate in bits per pixel (default 0.75, about 15:1 for 500 ppi fingerprints). */
  bitrate?: number;
  /** Scanning resolution to record, in pixels per inch (default: not recorded). */
  ppi?: number | null;
  /** A text comment to store in the stream, in UTF-8. */
  comment?: string;
  /**
   * "conformant" (default): the encoder aligned with NIST's reference, the only one meant for
   * certification; "optimized": meets the requested size more closely, never certified.
   */
  profile?: "conformant" | "optimized";
}
"#;

#[wasm_bindgen]
extern "C" {
    /// The plain JavaScript object `encode` takes as options.
    #[wasm_bindgen(typescript_type = "EncodeOptions")]
    pub type JsEncodeOptions;

    #[wasm_bindgen(method, getter)]
    fn bitrate(this: &JsEncodeOptions) -> Option<f64>;
    #[wasm_bindgen(method, getter)]
    fn ppi(this: &JsEncodeOptions) -> Option<f64>;
    #[wasm_bindgen(method, getter)]
    fn comment(this: &JsEncodeOptions) -> Option<String>;
    #[wasm_bindgen(method, getter)]
    fn profile(this: &JsEncodeOptions) -> Option<String>;
}

/// What a WSQ stream says about its image (`readInfo`), read without decoding it.
#[wasm_bindgen]
pub struct WsqInfo {
    width: u32,
    height: u32,
    ppi: Option<u32>,
    software: u16,
    comments: Vec<String>,
}

#[wasm_bindgen]
impl WsqInfo {
    /// Width in pixels.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Scanning resolution in pixels per inch, or `undefined` when the stream does not say.
    #[wasm_bindgen(getter)]
    pub fn ppi(&self) -> Option<u32> {
        self.ppi
    }

    /// Implementation number of the encoder (0 when unassigned).
    #[wasm_bindgen(getter)]
    pub fn software(&self) -> u16 {
        self.software
    }

    /// The comment segments, as text (bytes that are not UTF-8 become U+FFFD).
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.comments.clone()
    }
}

/// A decoded image (`decode`): 8-bit grayscale pixels and what the stream says about them.
#[wasm_bindgen]
pub struct WsqImage {
    info: WsqInfo,
    pixels: Vec<u8>,
}

#[wasm_bindgen]
impl WsqImage {
    /// Width in pixels.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.info.width
    }

    /// Height in pixels.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.info.height
    }

    /// Scanning resolution in pixels per inch, or `undefined` when the stream does not say.
    #[wasm_bindgen(getter)]
    pub fn ppi(&self) -> Option<u32> {
        self.info.ppi
    }

    /// Implementation number of the encoder (0 when unassigned).
    #[wasm_bindgen(getter)]
    pub fn software(&self) -> u16 {
        self.info.software
    }

    /// The comment segments, as text (bytes that are not UTF-8 become U+FFFD).
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.info.comments.clone()
    }

    /// The pixels in row-major order, `width * height` bytes, 0 for black. Each access copies
    /// them into a new `Uint8Array`: read it once.
    #[wasm_bindgen(getter)]
    pub fn pixels(&self) -> Vec<u8> {
        self.pixels.clone()
    }
}

fn comments_text(comments: &[Vec<u8>]) -> Vec<String> {
    comments
        .iter()
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect()
}

fn dimension(value: usize) -> u32 {
    // The format caps both dimensions at 65535.
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// The options `encode` reads from its JavaScript object, before validation.
#[derive(Default)]
struct RawOptions {
    bitrate: Option<f64>,
    ppi: Option<f64>,
    comment: Option<String>,
    profile: Option<String>,
}

impl RawOptions {
    fn from_js(options: &JsEncodeOptions) -> Self {
        RawOptions {
            bitrate: options.bitrate(),
            ppi: options.ppi(),
            comment: options.comment(),
            profile: options.profile(),
        }
    }

    /// The codec's options, or the message of the exception to throw.
    fn build(self) -> Result<EncodeOptions, String> {
        // A single-threaded target: say so rather than rely on the default.
        let mut options = EncodeOptions::default().with_threads(1);
        if let Some(bitrate) = self.bitrate {
            options = options.with_bitrate(bitrate as f32);
        }
        if let Some(ppi) = self.ppi {
            if !(ppi.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&ppi)) {
                return Err(format!(
                    "ppi must be a whole number of pixels per inch, not {ppi}"
                ));
            }
            options = options.with_ppi(Some(ppi as u32));
        }
        if let Some(comment) = self.comment {
            options = options.with_comment(comment.into_bytes());
        }
        match self.profile.as_deref() {
            None | Some("conformant") => {}
            Some("optimized") => options = options.with_profile(Profile::Optimized),
            Some(other) => {
                return Err(format!(
                    "unknown profile {other:?}: expected \"conformant\" or \"optimized\""
                ));
            }
        }
        Ok(options)
    }
}

fn decode_impl(data: &[u8]) -> Result<WsqImage, wsq_codec::DecodeError> {
    let image = wsq_codec::decode_with(data, &DecodeOptions::default().with_threads(1))?;
    Ok(WsqImage {
        info: WsqInfo {
            width: dimension(image.width),
            height: dimension(image.height),
            ppi: image.ppi,
            software: image.software,
            comments: comments_text(&image.comments),
        },
        pixels: image.pixels,
    })
}

/// Decodes a WSQ stream (the bytes of a `.wsq` file).
///
/// Throws an `Error` if the data is not a valid WSQ stream or the image is too large.
#[wasm_bindgen]
pub fn decode(data: &[u8]) -> Result<WsqImage, JsError> {
    decode_impl(data).map_err(|e| JsError::new(&e.to_string()))
}

/// Reads the dimensions, resolution and comments of a WSQ stream without decoding its image.
///
/// Throws an `Error` if the headers are not valid.
#[wasm_bindgen(js_name = readInfo)]
pub fn read_info(data: &[u8]) -> Result<WsqInfo, JsError> {
    let info = wsq_codec::read_info(data).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(WsqInfo {
        width: dimension(info.width),
        height: dimension(info.height),
        ppi: info.ppi,
        software: info.software,
        comments: comments_text(&info.comments),
    })
}

/// Encodes 8-bit grayscale `pixels` (`width * height` bytes, row-major, 0 for black) as a
/// WSQ stream.
///
/// Both dimensions must be between 81 and 65535. Throws an `Error` for invalid dimensions,
/// a pixel count that does not match them, or unusable options.
#[wasm_bindgen]
pub fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    options: Option<JsEncodeOptions>,
) -> Result<Vec<u8>, JsError> {
    let raw = options
        .as_ref()
        .map(RawOptions::from_js)
        .unwrap_or_default();
    let options = raw.build().map_err(|e| JsError::new(&e))?;
    wsq_codec::encode(pixels, width as usize, height as usize, &options)
        .map_err(|e| JsError::new(&e.to_string()))
}

/// The version of this package.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_default_to_the_codec_defaults() {
        let options = RawOptions::default().build().unwrap();
        assert_eq!(options, EncodeOptions::default());
    }

    #[test]
    fn options_are_carried_over() {
        let options = RawOptions {
            bitrate: Some(1.5),
            ppi: Some(500.0),
            comment: Some("hi".into()),
            profile: Some("optimized".into()),
        }
        .build()
        .unwrap();
        assert_eq!(options.bitrate, 1.5);
        assert_eq!(options.ppi, Some(500));
        assert_eq!(options.comments, vec![b"hi".to_vec()]);
        assert_eq!(options.profile, Profile::Optimized);
        assert_eq!(options.threads, 1);
    }

    #[test]
    fn bad_options_are_refused() {
        let profile = RawOptions {
            profile: Some("fast".into()),
            ..Default::default()
        };
        assert!(profile.build().unwrap_err().contains("unknown profile"));
        for ppi in [1.5, -1.0, f64::NAN, 1e12] {
            let raw = RawOptions {
                ppi: Some(ppi),
                ..Default::default()
            };
            assert!(raw.build().is_err(), "{ppi}");
        }
    }
}
