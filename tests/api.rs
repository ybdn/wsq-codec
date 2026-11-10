//! The shape of the public API, frozen for 1.0 (see docs/api-1.0.md): which
//! traits the public types implement, that they can cross threads, and how the
//! errors chain. A change that fails to compile here is a breaking change.

use std::error::Error;
use std::io;

use wsq_codec::inspect::{Quantized, Subband};
use wsq_codec::{
    DecodeError, DecodeOptions, Decoder, EncodeError, EncodeOptions, Encoder, Image, Info, Profile,
    ReadError, WriteError,
};

fn send_sync<T: Send + Sync>() {}
fn common<T: Clone + std::fmt::Debug + PartialEq>() {}
fn eq_hash<T: Eq + std::hash::Hash>() {}
fn error<T: Error + Send + Sync + 'static>() {}

#[test]
fn public_types_cross_threads() {
    send_sync::<Image>();
    send_sync::<Info>();
    send_sync::<DecodeOptions>();
    send_sync::<EncodeOptions>();
    send_sync::<Profile>();
    send_sync::<Decoder>();
    send_sync::<Encoder>();
    send_sync::<Quantized>();
    send_sync::<Subband>();
    send_sync::<DecodeError>();
    send_sync::<EncodeError>();
    send_sync::<ReadError>();
    send_sync::<WriteError>();
}

#[test]
fn public_types_have_the_common_traits() {
    common::<Image>();
    common::<Info>();
    common::<DecodeOptions>();
    common::<EncodeOptions>();
    common::<Profile>();
    common::<Quantized>();
    common::<Subband>();
    common::<DecodeError>();
    common::<EncodeError>();
    eq_hash::<Image>();
    eq_hash::<Info>();
    eq_hash::<DecodeOptions>();
    eq_hash::<Profile>();
    eq_hash::<DecodeError>();
    error::<DecodeError>();
    error::<EncodeError>();
    error::<ReadError>();
    error::<WriteError>();

    assert_eq!(DecodeOptions::default(), DecodeOptions::default());
    assert_eq!(EncodeOptions::default().profile, Profile::default());
    assert_eq!(Profile::default(), Profile::Conformant);
    // `Debug` and `Default` on the buffer-keeping types.
    assert_eq!(Decoder::default().options(), &DecodeOptions::default());
    assert_eq!(Encoder::default().options(), &EncodeOptions::default());
    assert!(format!("{:?}", Decoder::default()).starts_with("Decoder"));
    assert!(format!("{:?}", Encoder::default()).starts_with("Encoder"));
}

#[test]
fn dimension_constants_are_those_of_the_format() {
    assert_eq!(wsq_codec::MAX_DIMENSION, usize::from(u16::MAX));
    let options = EncodeOptions::default();
    let n = wsq_codec::MIN_ENCODE_DIMENSION;
    assert!(wsq_codec::encode(&vec![9; n * n], n, n, &options).is_ok());
    let m = n - 1;
    assert!(matches!(
        wsq_codec::encode(&vec![9; m * n], m, n, &options),
        Err(EncodeError::InvalidDimensions { .. })
    ));
    let big = wsq_codec::MAX_DIMENSION + 1;
    assert!(matches!(
        wsq_codec::encode(&[], big, n, &options),
        Err(EncodeError::InvalidDimensions { .. })
    ));
}

/// The chain of messages a reporter prints: each error once.
fn chain(e: &dyn Error) -> Vec<String> {
    let mut out = vec![e.to_string()];
    let mut source = e.source();
    while let Some(s) = source {
        out.push(s.to_string());
        source = s.source();
    }
    out
}

#[test]
fn wrapped_errors_are_reported_once() {
    let io_error = || io::Error::new(io::ErrorKind::UnexpectedEof, "short read");
    let read = ReadError::from(io_error());
    assert_eq!(chain(&read), ["cannot read WSQ data", "short read"]);
    let write = WriteError::from(io_error());
    assert_eq!(chain(&write), ["cannot write WSQ data", "short read"]);
    // The io error is reachable, as the downcast of the source.
    let source = read.source().and_then(|s| s.downcast_ref::<io::Error>());
    assert_eq!(
        source.map(io::Error::kind),
        Some(io::ErrorKind::UnexpectedEof)
    );

    // A wrapped codec error displays as itself and adds nothing to the chain.
    let read = ReadError::from(DecodeError::NotWsq);
    assert_eq!(chain(&read), [DecodeError::NotWsq.to_string()]);
    let write = WriteError::from(EncodeError::InvalidPpi);
    assert_eq!(chain(&write), [EncodeError::InvalidPpi.to_string()]);
    // The leaves have no source.
    assert!(DecodeError::OutOfMemory.source().is_none());
    assert!(EncodeError::OutOfMemory.source().is_none());
}
