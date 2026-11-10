//! C API for `wsq-codec`: the library behind `include/wsq.h`.
//!
//! This crate is the only place of the project that uses `unsafe`: raw pointers cannot cross
//! the C boundary any other way. The rules it follows, which `capi/README.md` documents for
//! users:
//!
//! - every entry point runs inside [`std::panic::catch_unwind`] and returns
//!   [`wsq_status::WSQ_ERR_PANIC`] instead of unwinding into C (so the crate must be built
//!   with `panic = "unwind"`, the default);
//! - every pointer argument is checked for null before use, and a `len` that no slice can
//!   have is refused;
//! - what the library hands out was allocated by Rust and is only ever freed by the matching
//!   `wsq_*_free` function, which reconstructs the exact `Box<[T]>` it came from;
//! - outputs are written only after the work is done, and are reset to "empty" first, so that
//!   a failed call leaves nothing to free.
//!
//! Doc comments on the `extern "C"` items are copied into the generated header by `cbindgen`,
//! so they are written for C programmers.
//!
//! The doc comments describe the contract in C terms rather than under a `# Safety` heading,
//! which would end up in the header.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]
// The items are named as C names them (`wsq_status`, `WSQ_OK`).
#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use wsq_codec::{DecodeError, DecodeOptions, EncodeError, EncodeOptions, Profile};

// ---------------------------------------------------------------------------------------------
// Status codes
// ---------------------------------------------------------------------------------------------

/// Result of a call. `WSQ_OK` (0) is success, every other value is a failure.
///
/// The values are part of the ABI and never change; a new failure gets a new value. Treat a
/// value you do not know as a failure, and print it with `wsq_status_message`. Which codes a
/// function can return is listed with it. Several specific errors of the Rust library share a
/// code (`WSQ_ERR_INVALID_STREAM` stands for every way a stream can be malformed): for the
/// detail, read `wsq_last_error_message` after the call.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum wsq_status {
    /// Success.
    WSQ_OK = 0,
    /// A pointer argument is null, a length is not usable, an options structure is malformed
    /// (`struct_size` too small, unknown `profile`), or an option has a value the library
    /// refuses and no other code names.
    WSQ_ERR_INVALID_ARGUMENT = 1,
    /// The data is not a valid WSQ stream: not WSQ at all, truncated, or corrupt.
    WSQ_ERR_INVALID_STREAM = 2,
    /// The image has more pixels than `max_pixels` allows (decoding).
    WSQ_ERR_TOO_LARGE = 3,
    /// Memory for the image or the work buffers could not be allocated.
    WSQ_ERR_OUT_OF_MEMORY = 4,
    /// A non-zero `software` (implementation number) was given with `WSQ_PROFILE_OPTIMIZED`:
    /// only the conformant profile may write one.
    WSQ_ERR_SOFTWARE_REQUIRES_CONFORMANT = 5,
    /// A bug in the library panicked; it was caught at the boundary. Nothing was returned and
    /// nothing needs freeing. Please report it.
    WSQ_ERR_PANIC = 6,
    /// The image dimensions cannot be encoded: each must be from 81 to 65535.
    WSQ_ERR_INVALID_DIMENSIONS = 7,
    /// A buffer does not have the size the image needs: `pixels_len` is not `width * height`
    /// (encoding), or the output buffer is not one byte per pixel (`wsq_decode_into`).
    WSQ_ERR_BUFFER_SIZE = 8,
    /// The bit rate is not a positive finite number, or is too low for the quantization table.
    WSQ_ERR_INVALID_BITRATE = 9,
    /// A comment is longer than one comment segment can hold (65533 bytes).
    WSQ_ERR_COMMENT_TOO_LONG = 10,
    /// An error this version of the header does not know. Not returned by version 1 of the
    /// library; it exists for errors that later versions of the Rust library may add.
    WSQ_ERR_UNKNOWN = 99,
}

use wsq_status::*;

/// Which encoder writes the stream. Store a value in `wsq_encode_options.profile`.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum wsq_profile {
    /// The encoder aligned with the NIST reference implementation (NBIS): byte-identical
    /// output, the only profile meant for FBI certification, and the only one that may write
    /// an implementation number (`software`). The default.
    WSQ_PROFILE_CONFORMANT = 0,
    /// An encoder free to depart from the reference so that the stream meets the size
    /// `bitrate` asks for. Never certified; refuses a non-zero `software`.
    WSQ_PROFILE_OPTIMIZED = 1,
}

/// A decoding limit meaning "no limit": `wsq_decode_options.max_pixels` set to this lets a
/// stream claim an image of any size the format can describe (at most 65535 by 65535 pixels).
/// cbindgen:ignore
pub const WSQ_NO_PIXEL_LIMIT: usize = usize::MAX;

// ---------------------------------------------------------------------------------------------
// Error plumbing
// ---------------------------------------------------------------------------------------------

/// A failure on its way out: the code for C, the text for `wsq_last_error_message`.
struct CError {
    status: wsq_status,
    message: String,
}

impl CError {
    fn new(status: wsq_status, message: impl Into<String>) -> Self {
        CError {
            status,
            message: message.into(),
        }
    }
}

fn invalid(message: impl Into<String>) -> CError {
    CError::new(WSQ_ERR_INVALID_ARGUMENT, message)
}

impl From<DecodeError> for CError {
    fn from(e: DecodeError) -> Self {
        let status = match e {
            DecodeError::NotWsq
            | DecodeError::UnexpectedEof
            | DecodeError::UnexpectedMarker(_)
            | DecodeError::InvalidSegment { .. }
            | DecodeError::MissingTable(_)
            | DecodeError::UndefinedHuffmanTable(_)
            | DecodeError::InvalidDimensions { .. }
            | DecodeError::CorruptData(_) => WSQ_ERR_INVALID_STREAM,
            DecodeError::TooLarge { .. } => WSQ_ERR_TOO_LARGE,
            DecodeError::OutputSize { .. } => WSQ_ERR_BUFFER_SIZE,
            DecodeError::OutOfMemory => WSQ_ERR_OUT_OF_MEMORY,
            // `DecodeError` is non-exhaustive.
            _ => WSQ_ERR_UNKNOWN,
        };
        CError::new(status, e.to_string())
    }
}

impl From<EncodeError> for CError {
    fn from(e: EncodeError) -> Self {
        let status = match e {
            EncodeError::InvalidDimensions { .. } => WSQ_ERR_INVALID_DIMENSIONS,
            EncodeError::PixelCountMismatch { .. } => WSQ_ERR_BUFFER_SIZE,
            EncodeError::InvalidBitrate(_) | EncodeError::BitrateTooLow(_) => {
                WSQ_ERR_INVALID_BITRATE
            }
            EncodeError::CommentTooLong(_) => WSQ_ERR_COMMENT_TOO_LONG,
            // The C options cannot express a zero resolution (0 means "unknown").
            EncodeError::InvalidPpi => WSQ_ERR_INVALID_ARGUMENT,
            EncodeError::SoftwareRequiresConformant(_) => WSQ_ERR_SOFTWARE_REQUIRES_CONFORMANT,
            EncodeError::OutOfMemory => WSQ_ERR_OUT_OF_MEMORY,
            // `EncodeError` is non-exhaustive.
            _ => WSQ_ERR_UNKNOWN,
        };
        CError::new(status, e.to_string())
    }
}

thread_local! {
    /// Text of the last failure of a `wsq_*` call on this thread; `None` after a success.
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(message: Option<&str>) {
    // `try_with` and `try_borrow_mut`: this must not panic, not even while the thread exits.
    let _ = LAST_ERROR.try_with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut() {
            *slot = message.map(|m| {
                // A NUL would end the C string early: the messages have none, but be sure.
                CString::new(m.replace('\0', "?")).unwrap_or_default()
            });
        }
    });
}

/// Runs the body of an entry point: no panic escapes, the status is returned, and the message
/// is kept for `wsq_last_error_message`.
fn guard<F: FnOnce() -> Result<(), CError>>(body: F) -> wsq_status {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => {
            set_last_error(None);
            WSQ_OK
        }
        Ok(Err(e)) => {
            set_last_error(Some(&e.message));
            e.status
        }
        Err(_) => {
            set_last_error(Some("internal error: the library panicked"));
            WSQ_ERR_PANIC
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------------------------

/// A byte string: `len` bytes at `data`. Used for comments, which are arbitrary bytes (not
/// necessarily text, and not NUL-terminated). `data` may be NULL only when `len` is 0.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct wsq_bytes {
    /// First byte.
    pub data: *const u8,
    /// Number of bytes.
    pub len: usize,
}

/// What a WSQ stream says about its image. Filled in by `wsq_read_info`, `wsq_decode_into` and
/// (as `wsq_image.info`) `wsq_decode`; it owns the `comments` array, released by
/// `wsq_info_free`.
///
/// The layout of this structure is frozen for version 1 of the library: fields are never added
/// or reordered. (Input structures grow through `struct_size` instead.)
#[repr(C)]
#[derive(Debug)]
pub struct wsq_info {
    /// Width in pixels, 1 to 65535.
    pub width: u32,
    /// Height in pixels, 1 to 65535.
    pub height: u32,
    /// Scanning resolution in pixels per inch, or 0 when the stream does not record it.
    pub ppi: u32,
    /// Implementation number of the encoder that wrote the stream, 0 when unassigned.
    pub software: u16,
    /// The comment segments of the stream, in order, `comment_count` of them. The first is
    /// normally the NISTCOM comment (`NIST_COM ...`). NULL when `comment_count` is 0. Owned by
    /// this structure; do not free the array or the payloads yourself.
    pub comments: *const wsq_bytes,
    /// Number of entries of `comments`.
    pub comment_count: usize,
}

/// cbindgen:ignore
impl wsq_info {
    const EMPTY: wsq_info = wsq_info {
        width: 0,
        height: 0,
        ppi: 0,
        software: 0,
        comments: ptr::null(),
        comment_count: 0,
    };
}

/// A decoded 8-bit grayscale image. Filled in by `wsq_decode`, released by `wsq_image_free`.
///
/// The layout of this structure is frozen for version 1 of the library.
#[repr(C)]
#[derive(Debug)]
pub struct wsq_image {
    /// Dimensions, resolution, implementation number and comments.
    pub info: wsq_info,
    /// `info.width * info.height` bytes, row-major, one byte per pixel (0 is black). Owned by
    /// this structure.
    pub pixels: *mut u8,
    /// Number of bytes at `pixels`.
    pub pixels_len: usize,
}

/// cbindgen:ignore
impl wsq_image {
    const EMPTY: wsq_image = wsq_image {
        info: wsq_info::EMPTY,
        pixels: ptr::null_mut(),
        pixels_len: 0,
    };
}

/// Decoder settings. Start from `wsq_decode_options_default()` and change what you need; a
/// function that takes a `const wsq_decode_options *` also accepts NULL for the defaults.
///
/// The first member, `struct_size`, lets the structure grow without breaking programs built
/// against an older header: set it to `sizeof(wsq_decode_options)` (the default function does).
/// A library that knows more fields than your `struct_size` covers uses the defaults for them;
/// one that knows fewer ignores yours. A `struct_size` smaller than a `size_t` is refused.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct wsq_decode_options {
    /// `sizeof(wsq_decode_options)` as your program was compiled.
    pub struct_size: usize,
    /// Largest `width * height` accepted; a stream whose header claims more fails with
    /// `WSQ_ERR_TOO_LARGE` before anything is allocated for it. The default is 100 million;
    /// `WSQ_NO_PIXEL_LIMIT` removes the limit. Decoding needs about 8 bytes per pixel at its
    /// peak, and a few bytes of stream can claim billions of pixels: keep a limit for data you
    /// do not trust.
    pub max_pixels: usize,
    /// Number of threads: 1 (the default) keeps the work on the calling thread, 0 uses one per
    /// core. The output does not depend on it.
    pub threads: usize,
}

/// Encoder settings. Start from `wsq_encode_options_default()` and change what you need; a
/// function that takes a `const wsq_encode_options *` also accepts NULL for the defaults.
///
/// `struct_size` works as in `wsq_decode_options`: set it to `sizeof(wsq_encode_options)`.
/// The pointers in the structure (`comments`) are read during the call only.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct wsq_encode_options {
    /// `sizeof(wsq_encode_options)` as your program was compiled.
    pub struct_size: usize,
    /// Target bit rate in bits per pixel: 0.75 (the default) is the usual choice for 500 ppi
    /// fingerprints, about 15:1; 2.25 gives about 5:1.
    pub bitrate: f32,
    /// Scanning resolution to record in the stream, in pixels per inch; 0 (the default) records
    /// none.
    pub ppi: u32,
    /// A `wsq_profile` value: `WSQ_PROFILE_CONFORMANT` (0, the default) or
    /// `WSQ_PROFILE_OPTIMIZED` (1). Any other value is refused with `WSQ_ERR_INVALID_ARGUMENT`.
    /// (An `int32_t`, not the enum, so that a bad value cannot be undefined behaviour.)
    pub profile: i32,
    /// Implementation number the FBI assigned to the encoder, written in the frame header; 0
    /// (the default) means unassigned. Only with `WSQ_PROFILE_CONFORMANT`.
    pub software: u16,
    /// Number of threads: 1 (the default) keeps the work on the calling thread, 0 uses one per
    /// core. The output does not depend on it.
    pub threads: usize,
    /// Extra comment segments, `comment_count` of them (NULL when 0). The encoder always writes
    /// a NISTCOM comment; the first of these that starts with `NIST_COM` is merged into it, the
    /// others are written as they are, after it.
    pub comments: *const wsq_bytes,
    /// Number of entries of `comments`.
    pub comment_count: usize,
}

/// Copies an options structure from C, honoring the `struct_size` it declares.
///
/// Bytes beyond `defaults`' size are ignored; fields beyond the declared size keep their
/// default.
///
/// The caller promises that `p` is readable for the `usize` it starts with, and that this many
/// bytes are readable from `p`. `T` must be a plain structure of integers, floats and
/// pointers, for which every bit pattern is valid.
unsafe fn load_options<T: Copy>(p: *const T, defaults: T) -> Result<T, CError> {
    if p.is_null() {
        return Ok(defaults);
    }
    // SAFETY: `p` is non-null and, by the caller's promise, starts with a readable `usize`
    // (`struct_size`, the first member of every options structure); the read is unaligned in
    // case the caller built the structure in a byte buffer.
    let declared = unsafe { p.cast::<usize>().read_unaligned() };
    if declared < size_of::<usize>() {
        return Err(invalid(format!(
            "struct_size {declared} is smaller than {}",
            size_of::<usize>()
        )));
    }
    let n = declared.min(size_of::<T>());
    let mut options = defaults;
    // SAFETY: the caller promises `declared >= n` readable bytes at `p`; `options` is a `T`,
    // which has `size_of::<T>() >= n` writable bytes; the regions are distinct objects, and any
    // bit pattern is valid for `T`.
    unsafe {
        ptr::copy_nonoverlapping(p.cast::<u8>(), ptr::from_mut(&mut options).cast::<u8>(), n);
    }
    Ok(options)
}

/// A slice of the caller's memory.
///
/// The caller promises that `p` is readable for `len` elements (when non-null).
unsafe fn slice_in<'a, T>(p: *const T, len: usize, what: &str) -> Result<&'a [T], CError> {
    if p.is_null() {
        return Err(invalid(format!("{what} is NULL")));
    }
    if len
        .checked_mul(size_of::<T>())
        .is_none_or(|n| n > isize::MAX as usize)
    {
        return Err(invalid(format!("{what}: length {len} is too large")));
    }
    // SAFETY: `p` is non-null, the byte size is at most `isize::MAX`, and the caller promises
    // that `len` elements are readable and stay unchanged during the call. Alignment is that
    // of `T`; the only `T`s used are `u8` and `wsq_bytes` (a C structure the caller built).
    Ok(unsafe { std::slice::from_raw_parts(p, len) })
}

/// Checks an output pointer for null. The pointer is only ever written through [`put`], never
/// turned into a reference or read: the memory behind it may be uninitialized.
fn non_null<T>(p: *mut T, what: &str) -> Result<*mut T, CError> {
    if p.is_null() {
        Err(invalid(format!("{what} is NULL")))
    } else {
        Ok(p)
    }
}

/// Writes `value` to an output of the caller's, without reading or dropping what was there.
///
/// The caller promises that `p` is non-null, aligned and writable for a `T`.
unsafe fn put<T>(p: *mut T, value: T) {
    // SAFETY: by the caller's promise.
    unsafe { p.write(value) }
}

// ---------------------------------------------------------------------------------------------
// Ownership: Rust allocations handed to C and taken back
// ---------------------------------------------------------------------------------------------

/// Hands a vector to C as a pointer and a length. An empty vector becomes NULL.
fn export_bytes(v: Vec<u8>) -> (*mut u8, usize) {
    let boxed = v.into_boxed_slice();
    let len = boxed.len();
    if len == 0 {
        (ptr::null_mut(), 0)
    } else {
        (Box::into_raw(boxed).cast::<u8>(), len)
    }
}

/// Takes back what [`export_bytes`] gave out.
///
/// The caller promises that `(p, len)` is a pair returned by `export_bytes`, not freed yet.
unsafe fn release_bytes(p: *mut u8, len: usize) {
    if p.is_null() || len == 0 {
        return;
    }
    // SAFETY: by the caller's promise `p` came from `Box::into_raw` of a `Box<[u8]>` of exactly
    // `len` bytes, so this rebuilds that box, whose drop frees it once.
    drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(p, len)) });
}

/// Builds the `wsq_info` for decoded metadata, moving the comments into C-owned form.
fn export_info(
    width: usize,
    height: usize,
    ppi: Option<u32>,
    software: u16,
    comments: Vec<Vec<u8>>,
) -> wsq_info {
    let entries: Vec<wsq_bytes> = comments
        .into_iter()
        .map(|c| {
            let (data, len) = export_bytes(c);
            wsq_bytes { data, len }
        })
        .collect();
    let comment_count = entries.len();
    let comments = if comment_count == 0 {
        ptr::null()
    } else {
        Box::into_raw(entries.into_boxed_slice())
            .cast::<wsq_bytes>()
            .cast_const()
    };
    wsq_info {
        // The format stores 16 bits; the saturation is never reached.
        width: u32::try_from(width).unwrap_or(u32::MAX),
        height: u32::try_from(height).unwrap_or(u32::MAX),
        ppi: ppi.unwrap_or(0),
        software,
        comments,
        comment_count,
    }
}

/// Frees what [`export_info`] built, and resets `info` to empty.
///
/// The caller promises that `info` is a `wsq_info` that [`export_info`] built, or an empty one.
unsafe fn release_info(info: &mut wsq_info) {
    if !info.comments.is_null() && info.comment_count != 0 {
        let array = ptr::slice_from_raw_parts_mut(info.comments.cast_mut(), info.comment_count);
        // SAFETY: by the caller's promise the array came from `Box::into_raw` of a
        // `Box<[wsq_bytes]>` of `comment_count` entries, and is freed once (the structure is
        // reset below).
        let entries = unsafe { Box::from_raw(array) };
        for entry in entries.iter() {
            // SAFETY: each payload came from `export_bytes` with this length, and is freed once.
            unsafe { release_bytes(entry.data.cast_mut(), entry.len) };
        }
    }
    *info = wsq_info::EMPTY;
}

// ---------------------------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------------------------

/// Reads the decoder options (NULL for the defaults) into the Rust type.
unsafe fn decode_options(p: *const wsq_decode_options) -> Result<DecodeOptions, CError> {
    // SAFETY: forwarded from the caller of the entry point: `p` is NULL or points to a
    // `wsq_decode_options` of the `struct_size` it declares.
    let o = unsafe { load_options(p, wsq_decode_options_default()) }?;
    Ok(DecodeOptions::default()
        .with_max_pixels(if o.max_pixels == WSQ_NO_PIXEL_LIMIT {
            None
        } else {
            Some(o.max_pixels)
        })
        .with_threads(o.threads))
}

/// Reads the encoder options (NULL for the defaults) into the Rust type, comments included.
unsafe fn encode_options(p: *const wsq_encode_options) -> Result<EncodeOptions, CError> {
    // SAFETY: forwarded from the caller of the entry point, as in `decode_options`.
    let o = unsafe { load_options(p, wsq_encode_options_default()) }?;
    let profile = match o.profile {
        x if x == wsq_profile::WSQ_PROFILE_CONFORMANT as i32 => Profile::Conformant,
        x if x == wsq_profile::WSQ_PROFILE_OPTIMIZED as i32 => Profile::Optimized,
        x => return Err(invalid(format!("unknown profile {x}"))),
    };
    let mut options = EncodeOptions::default()
        .with_bitrate(o.bitrate)
        .with_ppi((o.ppi != 0).then_some(o.ppi))
        .with_software(o.software)
        .with_threads(o.threads)
        .with_profile(profile);
    if o.comment_count != 0 {
        // SAFETY: the caller promises that `comments` points to `comment_count` readable
        // `wsq_bytes` when the count is not 0.
        let entries = unsafe { slice_in(o.comments, o.comment_count, "options.comments") }?;
        for (i, entry) in entries.iter().enumerate() {
            let comment = if entry.len == 0 {
                &[][..]
            } else {
                // SAFETY: the caller promises that each entry's `data` points to `len` readable
                // bytes.
                unsafe {
                    slice_in(
                        entry.data,
                        entry.len,
                        &format!("options.comments[{i}].data"),
                    )
                }?
            };
            options = options.with_comment(comment.to_vec());
        }
    }
    Ok(options)
}

// ---------------------------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------------------------

/// Returns the default decoder settings (`struct_size` set, `max_pixels` 100 million, one
/// thread).
#[unsafe(no_mangle)]
pub extern "C" fn wsq_decode_options_default() -> wsq_decode_options {
    let defaults = DecodeOptions::default();
    wsq_decode_options {
        struct_size: size_of::<wsq_decode_options>(),
        max_pixels: defaults.max_pixels.unwrap_or(WSQ_NO_PIXEL_LIMIT),
        threads: defaults.threads,
    }
}

/// Returns the default encoder settings (`struct_size` set, bit rate 0.75, no resolution,
/// conformant profile, no implementation number, one thread, no comments).
#[unsafe(no_mangle)]
pub extern "C" fn wsq_encode_options_default() -> wsq_encode_options {
    let defaults = EncodeOptions::default();
    wsq_encode_options {
        struct_size: size_of::<wsq_encode_options>(),
        bitrate: defaults.bitrate,
        ppi: defaults.ppi.unwrap_or(0),
        profile: wsq_profile::WSQ_PROFILE_CONFORMANT as i32,
        software: defaults.software,
        threads: defaults.threads,
        comments: ptr::null(),
        comment_count: 0,
    }
}

/// Decodes a WSQ stream of `len` bytes at `data` into a new image.
///
/// `options` may be NULL for the defaults. On success `*out` holds the image, which you release
/// with `wsq_image_free`. On failure `*out` is set to an empty image (all zero), there is
/// nothing to free, and the status says why: `WSQ_ERR_INVALID_ARGUMENT` (NULL pointer, bad
/// options), `WSQ_ERR_INVALID_STREAM`, `WSQ_ERR_TOO_LARGE`, `WSQ_ERR_OUT_OF_MEMORY` or
/// `WSQ_ERR_PANIC`.
///
/// The previous contents of `*out` are not read, so an image that is still allocated must be
/// freed first. `data` is read during the call only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsq_decode(
    data: *const u8,
    len: usize,
    options: *const wsq_decode_options,
    out: *mut wsq_image,
) -> wsq_status {
    guard(|| {
        let out = non_null(out, "out")?;
        // SAFETY: `out` is non-null and, per the contract above, writable for a `wsq_image`.
        unsafe { put(out, wsq_image::EMPTY) };
        // SAFETY: `data` is NULL or readable for `len` bytes, per the contract above.
        let data = unsafe { slice_in(data, len, "data") }?;
        // SAFETY: `options` is NULL or a `wsq_decode_options` of its declared size.
        let options = unsafe { decode_options(options) }?;
        let image = wsq_codec::decode_with(data, &options)?;
        let (pixels, pixels_len) = export_bytes(image.pixels);
        let image = wsq_image {
            info: export_info(
                image.width,
                image.height,
                image.ppi,
                image.software,
                image.comments,
            ),
            pixels,
            pixels_len,
        };
        // SAFETY: as for the first write to `out`.
        unsafe { put(out, image) };
        Ok(())
    })
}

/// Decodes a WSQ stream into a buffer you own: `pixels_len` bytes at `pixels`, which must be
/// exactly `width * height` (read the size with `wsq_read_info` first).
///
/// `options` may be NULL. `info` may be NULL if you do not need the metadata; otherwise it is
/// filled in on success (release it with `wsq_info_free`) and set to empty on failure. Nothing
/// else is allocated for you, so this is the call for a program that decodes many images into
/// the same buffer.
///
/// Errors as for `wsq_decode`, plus `WSQ_ERR_BUFFER_SIZE` when `pixels_len` is not the size of
/// the image. The content of `pixels` is unspecified after a failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsq_decode_into(
    data: *const u8,
    len: usize,
    options: *const wsq_decode_options,
    pixels: *mut u8,
    pixels_len: usize,
    info: *mut wsq_info,
) -> wsq_status {
    guard(|| {
        // `info` is optional: NULL is allowed.
        if !info.is_null() {
            // SAFETY: `info` is non-null and, per the contract above, writable for a `wsq_info`.
            unsafe { put(info, wsq_info::EMPTY) };
        }
        // SAFETY: `data` is NULL or readable for `len` bytes.
        let data = unsafe { slice_in(data, len, "data") }?;
        // SAFETY: `options` is NULL or a `wsq_decode_options` of its declared size.
        let options = unsafe { decode_options(options) }?;
        if pixels.is_null() {
            return Err(invalid("pixels is NULL"));
        }
        if pixels_len > isize::MAX as usize {
            return Err(invalid("pixels_len is too large"));
        }
        // SAFETY: `pixels` is non-null and the caller promises it is writable for `pixels_len`
        // bytes, not aliased by `data` or `info` during the call.
        let pixels = unsafe { std::slice::from_raw_parts_mut(pixels, pixels_len) };
        let decoded = wsq_codec::decode_into(data, &options, pixels)?;
        if !info.is_null() {
            let meta = export_info(
                decoded.width,
                decoded.height,
                decoded.ppi,
                decoded.software,
                decoded.comments,
            );
            // SAFETY: as for the first write to `info`.
            unsafe { put(info, meta) };
        }
        Ok(())
    })
}

/// Reads the dimensions, resolution and comments of a WSQ stream without decoding its image
/// (fast; corruption in the image data is not reported). There is no size limit.
///
/// On success `*out` is filled in and released with `wsq_info_free`; on failure it is set to
/// empty. Errors: `WSQ_ERR_INVALID_ARGUMENT`, `WSQ_ERR_INVALID_STREAM`, `WSQ_ERR_PANIC`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsq_read_info(
    data: *const u8,
    len: usize,
    out: *mut wsq_info,
) -> wsq_status {
    guard(|| {
        let out = non_null(out, "out")?;
        // SAFETY: `out` is non-null and writable for a `wsq_info`.
        unsafe { put(out, wsq_info::EMPTY) };
        // SAFETY: `data` is NULL or readable for `len` bytes.
        let data = unsafe { slice_in(data, len, "data") }?;
        let info = wsq_codec::read_info(data)?;
        let meta = export_info(
            info.width,
            info.height,
            info.ppi,
            info.software,
            info.comments,
        );
        // SAFETY: as for the first write to `out`.
        unsafe { put(out, meta) };
        Ok(())
    })
}

/// Encodes an 8-bit grayscale image of `width` by `height` pixels, given as `pixels_len` bytes
/// at `pixels` in row-major order (0 is black); `pixels_len` must be `width * height`. Both
/// dimensions must be from 81 to 65535.
///
/// `options` may be NULL for the defaults. On success `*out` points to the WSQ stream and
/// `*out_len` is its length in bytes; release it with `wsq_free_bytes(*out, *out_len)`. On
/// failure `*out` is NULL and `*out_len` is 0. Errors: `WSQ_ERR_INVALID_ARGUMENT`,
/// `WSQ_ERR_INVALID_DIMENSIONS`, `WSQ_ERR_BUFFER_SIZE`, `WSQ_ERR_INVALID_BITRATE`,
/// `WSQ_ERR_COMMENT_TOO_LONG`, `WSQ_ERR_SOFTWARE_REQUIRES_CONFORMANT`,
/// `WSQ_ERR_OUT_OF_MEMORY`, `WSQ_ERR_PANIC`.
///
/// `pixels` and the options' comments are read during the call only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsq_encode(
    pixels: *const u8,
    pixels_len: usize,
    width: usize,
    height: usize,
    options: *const wsq_encode_options,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> wsq_status {
    guard(|| {
        let out = non_null(out, "out")?;
        let out_len = non_null(out_len, "out_len")?;
        // SAFETY: both are non-null and, per the contract above, writable.
        unsafe {
            put(out, ptr::null_mut());
            put(out_len, 0);
        }
        // SAFETY: `pixels` is NULL or readable for `pixels_len` bytes.
        let pixels = unsafe { slice_in(pixels, pixels_len, "pixels") }?;
        // SAFETY: `options` is NULL or a `wsq_encode_options` of its declared size whose
        // `comments` array is valid.
        let options = unsafe { encode_options(options) }?;
        let stream = wsq_codec::encode(pixels, width, height, &options)?;
        let (data, len) = export_bytes(stream);
        // SAFETY: as for the first writes.
        unsafe {
            put(out, data);
            put(out_len, len);
        }
        Ok(())
    })
}

/// Frees a buffer returned by `wsq_encode`, given the pointer and the length it returned.
/// NULL is accepted and does nothing. Do not free it with `free`, and do not free it twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsq_free_bytes(ptr: *mut u8, len: usize) {
    // Freeing runs no user code, but a panic must still not cross the boundary.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the caller passes a pair returned by `wsq_encode` and not freed yet (or NULL).
        unsafe { release_bytes(ptr, len) };
    }));
}

/// Frees what `wsq_read_info` or `wsq_decode_into` put in `*info` (the comments), and resets
/// it to empty, so that freeing twice is harmless. NULL is accepted and does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsq_info_free(info: *mut wsq_info) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `info` is NULL or points to a `wsq_info` filled in by this library (or
        // zeroed), not modified by the caller since.
        if let Some(info) = unsafe { info.as_mut() } {
            // SAFETY: as above, the structure holds what `export_info` built, or is empty.
            unsafe { release_info(info) };
        }
    }));
}

/// Frees what `wsq_decode` put in `*image` (pixels and comments), and resets it to empty, so
/// that freeing twice is harmless. NULL is accepted and does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsq_image_free(image: *mut wsq_image) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `image` is NULL or points to a `wsq_image` filled in by `wsq_decode` (or
        // zeroed), not modified by the caller since.
        if let Some(image) = unsafe { image.as_mut() } {
            // SAFETY: `pixels` and `pixels_len` are the pair `export_bytes` returned.
            unsafe { release_bytes(image.pixels, image.pixels_len) };
            // SAFETY: `info` holds what `export_info` built, or is empty.
            unsafe { release_info(&mut image.info) };
            *image = wsq_image::EMPTY;
        }
    }));
}

/// A static, NUL-terminated English description of a status (any `int`, so also of a value
/// from a later version). Never NULL, never to be freed, valid for the life of the program, and
/// safe to call from any thread.
#[unsafe(no_mangle)]
pub extern "C" fn wsq_status_message(status: i32) -> *const c_char {
    let message: &'static std::ffi::CStr = match status {
        x if x == WSQ_OK as i32 => c"success",
        x if x == WSQ_ERR_INVALID_ARGUMENT as i32 => {
            c"invalid argument (NULL pointer or bad option)"
        }
        x if x == WSQ_ERR_INVALID_STREAM as i32 => c"not a valid WSQ stream",
        x if x == WSQ_ERR_TOO_LARGE as i32 => c"the image exceeds max_pixels",
        x if x == WSQ_ERR_OUT_OF_MEMORY as i32 => c"out of memory",
        x if x == WSQ_ERR_SOFTWARE_REQUIRES_CONFORMANT as i32 => {
            c"an implementation number requires the conformant profile"
        }
        x if x == WSQ_ERR_PANIC as i32 => c"internal error (panic caught)",
        x if x == WSQ_ERR_INVALID_DIMENSIONS as i32 => c"image dimensions must be from 81 to 65535",
        x if x == WSQ_ERR_BUFFER_SIZE as i32 => c"buffer size does not match the image",
        x if x == WSQ_ERR_INVALID_BITRATE as i32 => c"invalid bit rate",
        x if x == WSQ_ERR_COMMENT_TOO_LONG as i32 => c"comment too long",
        x if x == WSQ_ERR_UNKNOWN as i32 => c"unknown error",
        _ => c"unknown status",
    };
    message.as_ptr()
}

/// The description, in English, of the last failed `wsq_*` call that returned a status, made
/// on the calling thread: more detailed than `wsq_status_message` (for instance, which
/// dimensions were refused). Empty after a call that succeeded or before any call. The text
/// may be reworded in any release: show it to people, do not parse it.
///
/// The string belongs to the library and stays valid until the next call of `wsq_decode`,
/// `wsq_decode_into`, `wsq_read_info` or `wsq_encode` on the same thread; copy it if you need
/// it longer. Never NULL.
#[unsafe(no_mangle)]
pub extern "C" fn wsq_last_error_message() -> *const c_char {
    LAST_ERROR
        .try_with(|cell| {
            cell.try_borrow()
                .ok()
                .and_then(|slot| slot.as_ref().map(|m| m.as_ptr()))
        })
        .ok()
        .flatten()
        .unwrap_or(c"".as_ptr())
}

/// The version of the library, a static NUL-terminated string such as "0.1.0" (the version of
/// the `wsq-capi` crate; it follows the semantic versioning of this C interface).
#[unsafe(no_mangle)]
pub extern "C" fn wsq_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0")
        .as_ptr()
        .cast::<c_char>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    fn text(p: *const c_char) -> String {
        // SAFETY: the library returns NUL-terminated strings.
        unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned()
    }

    #[test]
    fn a_panic_becomes_a_status() {
        let status = guard(|| panic!("boom"));
        assert_eq!(status, WSQ_ERR_PANIC);
        assert!(text(wsq_last_error_message()).contains("panicked"));
        // A success clears the message.
        assert_eq!(guard(|| Ok(())), WSQ_OK);
        assert_eq!(text(wsq_last_error_message()), "");
    }

    #[test]
    fn decode_errors_map_to_codes() {
        let code = |e: DecodeError| CError::from(e).status;
        assert_eq!(code(DecodeError::NotWsq), WSQ_ERR_INVALID_STREAM);
        assert_eq!(code(DecodeError::UnexpectedEof), WSQ_ERR_INVALID_STREAM);
        assert_eq!(code(DecodeError::CorruptData("x")), WSQ_ERR_INVALID_STREAM);
        assert_eq!(code(DecodeError::OutOfMemory), WSQ_ERR_OUT_OF_MEMORY);
        let too_large = wsq_codec::decode_with(
            &wsq_codec::encode(&[7; 81 * 81], 81, 81, &Default::default()).unwrap(),
            &DecodeOptions::default().with_max_pixels(Some(10)),
        )
        .unwrap_err();
        assert_eq!(code(too_large), WSQ_ERR_TOO_LARGE);
        let mut small = [0u8; 1];
        let stream = wsq_codec::encode(&[7; 81 * 81], 81, 81, &Default::default()).unwrap();
        let wrong_size =
            wsq_codec::decode_into(&stream, &DecodeOptions::default(), &mut small).unwrap_err();
        assert_eq!(code(wrong_size), WSQ_ERR_BUFFER_SIZE);
    }

    #[test]
    fn encode_errors_map_to_codes() {
        let code = |r: Result<Vec<u8>, EncodeError>| CError::from(r.unwrap_err()).status;
        let px = vec![9u8; 81 * 81];
        let o = EncodeOptions::default();
        assert_eq!(
            code(wsq_codec::encode(&px, 10, 10, &o)),
            WSQ_ERR_INVALID_DIMENSIONS
        );
        assert_eq!(
            code(wsq_codec::encode(&px[..100], 81, 81, &o)),
            WSQ_ERR_BUFFER_SIZE
        );
        assert_eq!(
            code(wsq_codec::encode(
                &px,
                81,
                81,
                &o.clone().with_bitrate(f32::NAN)
            )),
            WSQ_ERR_INVALID_BITRATE
        );
        assert_eq!(
            code(wsq_codec::encode(&px, 81, 81, &o.clone().with_bitrate(0.0))),
            WSQ_ERR_INVALID_BITRATE
        );
        assert_eq!(
            code(wsq_codec::encode(
                &px,
                81,
                81,
                &o.clone().with_comment(vec![0; 70_000])
            )),
            WSQ_ERR_COMMENT_TOO_LONG
        );
        assert_eq!(
            code(wsq_codec::encode(&px, 81, 81, &o.clone().with_ppi(Some(0)))),
            WSQ_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            code(wsq_codec::encode(
                &px,
                81,
                81,
                &o.with_profile(Profile::Optimized).with_software(7)
            )),
            WSQ_ERR_SOFTWARE_REQUIRES_CONFORMANT
        );
    }

    #[test]
    fn status_messages_are_never_null() {
        for status in -2..120 {
            assert!(!text(wsq_status_message(status)).is_empty());
        }
    }

    #[test]
    fn version_is_the_crate_version() {
        assert_eq!(text(wsq_version()), env!("CARGO_PKG_VERSION"));
    }
}
