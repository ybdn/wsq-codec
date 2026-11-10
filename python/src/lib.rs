//! Python bindings for `wsq-codec`: the native module `wsq_codec._wsq_codec`, re-exported by the
//! `wsq_codec` package (see `python/wsq_codec/__init__.py` and the stubs next to it).
//!
//! The bindings add no codec logic: they convert arguments, call the Rust API with the GIL
//! released, and convert the result and the errors back.

use numpy::ndarray::Array2;
use numpy::{IntoPyArray, PyArray2, PyReadonlyArray2, PyUntypedArrayMethods};
use pyo3::create_exception;
use pyo3::exceptions::{PyMemoryError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyList, PyMemoryView, PyString};
use wsq_codec as wsq;

create_exception!(
    _wsq_codec,
    WsqError,
    PyValueError,
    "Base class of the errors of this package; a `ValueError`."
);
create_exception!(
    _wsq_codec,
    DecodeError,
    WsqError,
    "The data is not a WSQ stream the decoder accepts, or the image is over the limit."
);
create_exception!(
    _wsq_codec,
    EncodeError,
    WsqError,
    "The image or an option cannot be encoded."
);

/// Maps a core error to its Python exception, with the Rust `Display` message. Out of memory is
/// the built-in `MemoryError`, as for any Python allocation.
fn decode_error(e: &wsq::DecodeError) -> PyErr {
    match e {
        wsq::DecodeError::OutOfMemory => PyMemoryError::new_err(e.to_string()),
        _ => DecodeError::new_err(e.to_string()),
    }
}

fn encode_error(e: &wsq::EncodeError) -> PyErr {
    match e {
        wsq::EncodeError::OutOfMemory => PyMemoryError::new_err(e.to_string()),
        _ => EncodeError::new_err(e.to_string()),
    }
}

fn comments_list<'py>(py: Python<'py>, comments: &[Vec<u8>]) -> PyResult<Bound<'py, PyList>> {
    PyList::new(py, comments.iter().map(|c| PyBytes::new(py, c)))
}

fn repr_ppi(ppi: Option<u32>) -> String {
    ppi.map_or_else(|| "None".to_owned(), |p| p.to_string())
}

/// What a WSQ stream says about its image, read without decoding it.
#[pyclass(frozen, module = "wsq_codec")]
struct Info {
    #[pyo3(get)]
    width: usize,
    #[pyo3(get)]
    height: usize,
    #[pyo3(get)]
    ppi: Option<u32>,
    #[pyo3(get)]
    software: u16,
    comments: Vec<Vec<u8>>,
}

#[pymethods]
impl Info {
    /// Comment segments of the stream, as bytes (a comment need not be text).
    #[getter]
    fn comments<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        comments_list(py, &self.comments)
    }

    fn __repr__(&self) -> String {
        format!(
            "Info(width={}, height={}, ppi={}, software={}, comments=<{}>)",
            self.width,
            self.height,
            repr_ppi(self.ppi),
            self.software,
            self.comments.len()
        )
    }
}

/// A decoded image: `pixels` is a `(height, width)` NumPy `uint8` array.
#[pyclass(frozen, module = "wsq_codec")]
struct Image {
    pixels: Py<PyArray2<u8>>,
    #[pyo3(get)]
    width: usize,
    #[pyo3(get)]
    height: usize,
    #[pyo3(get)]
    ppi: Option<u32>,
    #[pyo3(get)]
    software: u16,
    comments: Vec<Vec<u8>>,
}

#[pymethods]
impl Image {
    /// The pixels, row by row, one byte each (0 is black).
    #[getter]
    fn pixels<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<u8>> {
        self.pixels.bind(py).clone()
    }

    /// Comment segments of the stream, as bytes (a comment need not be text).
    #[getter]
    fn comments<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        comments_list(py, &self.comments)
    }

    fn __repr__(&self) -> String {
        format!(
            "Image(width={}, height={}, ppi={}, software={}, comments=<{}>)",
            self.width,
            self.height,
            repr_ppi(self.ppi),
            self.software,
            self.comments.len()
        )
    }
}

/// The bytes of a `bytes`, `bytearray` or `memoryview`, copied: the GIL is released while the
/// stream is parsed, and another Python thread could change a mutable buffer meanwhile. (The
/// buffer protocol is not available to the `abi3-py39` build before Python 3.11.)
fn bytes_of(data: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(b) = data.cast::<PyBytes>() {
        Ok(b.as_bytes().to_vec())
    } else if let Ok(b) = data.cast::<PyByteArray>() {
        Ok(b.to_vec())
    } else if data.cast::<PyMemoryView>().is_ok() {
        data.call_method0("tobytes")?.extract()
    } else {
        Err(PyTypeError::new_err(
            "data must be bytes, bytearray or memoryview",
        ))
    }
}

/// Decodes a WSQ stream.
#[pyfunction]
#[pyo3(signature = (data, *, max_pixels=None, threads=0))]
fn decode(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    max_pixels: Option<usize>,
    threads: usize,
) -> PyResult<Image> {
    let data = bytes_of(data)?;
    let options = wsq::DecodeOptions::default()
        .with_max_pixels(max_pixels.or(wsq::DecodeOptions::default().max_pixels))
        .with_threads(threads);
    let image = py
        .detach(|| wsq::decode_with(&data, &options))
        .map_err(|e| decode_error(&e))?;
    let pixels = Array2::from_shape_vec((image.height, image.width), image.pixels)
        .map_err(|e| PyValueError::new_err(e.to_string()))?
        .into_pyarray(py)
        .unbind();
    Ok(Image {
        pixels,
        width: image.width,
        height: image.height,
        ppi: image.ppi,
        software: image.software,
        comments: image.comments,
    })
}

/// Reads the dimensions, resolution and comments of a WSQ stream without decoding its image.
#[pyfunction]
fn read_info(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Info> {
    let data = bytes_of(data)?;
    let info = py
        .detach(|| wsq::read_info(&data))
        .map_err(|e| decode_error(&e))?;
    Ok(Info {
        width: info.width,
        height: info.height,
        ppi: info.ppi,
        software: info.software,
        comments: info.comments,
    })
}

/// The pixels of a 2-D `uint8` array in row-major order, copied (so that the GIL can be released
/// while encoding without another thread changing them), whatever the strides.
fn pixels_of(pixels: &Bound<'_, PyAny>) -> PyResult<(Vec<u8>, usize, usize)> {
    let array: PyReadonlyArray2<'_, u8> = pixels.extract().map_err(|_| {
        PyTypeError::new_err("pixels must be a 2-D numpy.ndarray of dtype uint8 (height, width)")
    })?;
    let [height, width] = *array.shape() else {
        unreachable!("a PyArray2 has two dimensions");
    };
    // `as_slice` also succeeds on a Fortran-ordered array, whose memory is not in row order:
    // only a C-contiguous one is copied as it is.
    let data = match array.as_slice() {
        Ok(slice) if array.is_c_contiguous() => slice.to_vec(),
        // A slice, a transpose, a Fortran-ordered array: copy in row order.
        _ => array.as_array().iter().copied().collect(),
    };
    Ok((data, width, height))
}

/// Encodes a 2-D `uint8` array as a WSQ stream.
#[pyfunction]
#[pyo3(signature = (pixels, *, bitrate=0.75, ppi=None, comment=None, software=0, profile="conformant", threads=0))]
#[allow(clippy::too_many_arguments)]
fn encode<'py>(
    py: Python<'py>,
    pixels: &Bound<'py, PyAny>,
    bitrate: f32,
    ppi: Option<u32>,
    comment: Option<&Bound<'py, PyAny>>,
    software: u16,
    profile: &str,
    threads: usize,
) -> PyResult<Bound<'py, PyBytes>> {
    let profile = match profile {
        "conformant" => wsq::Profile::Conformant,
        "optimized" => wsq::Profile::Optimized,
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown profile {other:?}: expected \"conformant\" or \"optimized\""
            )));
        }
    };
    let mut options = wsq::EncodeOptions::default()
        .with_bitrate(bitrate)
        .with_ppi(ppi)
        .with_software(software)
        .with_profile(profile)
        .with_threads(threads);
    if let Some(comment) = comment.filter(|c| !c.is_none()) {
        let bytes = if let Ok(b) = comment.cast::<PyBytes>() {
            b.as_bytes().to_vec()
        } else if let Ok(s) = comment.cast::<PyString>() {
            s.to_cow()?.as_bytes().to_vec()
        } else {
            return Err(PyTypeError::new_err("comment must be bytes or str"));
        };
        options = options.with_comment(bytes);
    }
    let (data, width, height) = pixels_of(pixels)?;
    let wsq = py
        .detach(|| wsq::encode(&data, width, height, &options))
        .map_err(|e| encode_error(&e))?;
    Ok(PyBytes::new(py, &wsq))
}

#[pymodule]
fn _wsq_codec(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_function(wrap_pyfunction!(decode, m)?)?;
    m.add_function(wrap_pyfunction!(read_info, m)?)?;
    m.add_function(wrap_pyfunction!(encode, m)?)?;
    m.add_class::<Image>()?;
    m.add_class::<Info>()?;
    m.add("WsqError", py.get_type::<WsqError>())?;
    m.add("DecodeError", py.get_type::<DecodeError>())?;
    m.add("EncodeError", py.get_type::<EncodeError>())?;
    m.add("MAX_DIMENSION", wsq::MAX_DIMENSION)?;
    m.add("MIN_ENCODE_DIMENSION", wsq::MIN_ENCODE_DIMENSION)?;
    m.add("DEFAULT_MAX_PIXELS", wsq::DEFAULT_MAX_PIXELS)?;
    Ok(())
}
