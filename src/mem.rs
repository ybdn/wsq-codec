//! Fallible allocation of the buffers whose size follows the image.
//!
//! `vec![0; n]` and `Vec::with_capacity` abort the process when the
//! allocator refuses, and panic when `n` bytes exceed `isize::MAX`, which
//! a 65535 by 65535 image does on 32-bit targets. These helpers turn both
//! into a value the caller reports as an error.
//!
//! No allocation whose size follows the pixel count (the image, the
//! coefficients, the encoder's output, the wavelet transform's blocks)
//! goes around these helpers. This search, run from the repository root,
//! finds the infallible allocations:
//!
//! ```sh
//! grep -nE 'vec!\[|Vec::with_capacity|\.to_vec\(\)|Box::new|\.resize\(' src/*.rs
//! ```
//!
//! What it still lists is test code, or sized by something other than the
//! pixel count: tables of fixed size, the comments the caller supplies,
//! vectors with one entry per row, subband or wavelet tree node (at most
//! 65535 rows), and the per-line plans of the wavelet filters (some MiB
//! at the largest dimension). The encoder's growing `Vec<u8>` buffers
//! only grow through [`grow_output`]. Reading a stream from a `Read`
//! (`decode_from`, the `image` decoder) grows with the input's size, not
//! the pixel count, and `Read::take` bounds it.

use crate::error::{DecodeError, EncodeError};

/// An allocation was refused, or its size does not fit the address space.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OutOfMemory;

impl From<OutOfMemory> for DecodeError {
    fn from(OutOfMemory: OutOfMemory) -> Self {
        DecodeError::OutOfMemory
    }
}

impl From<OutOfMemory> for EncodeError {
    fn from(OutOfMemory: OutOfMemory) -> Self {
        EncodeError::OutOfMemory
    }
}

/// An empty vector with room for exactly `len` elements.
pub(crate) fn with_capacity<T>(len: usize) -> Result<Vec<T>, OutOfMemory> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).map_err(|_| OutOfMemory)?;
    Ok(v)
}

/// A vector of `len` copies of `value`.
pub(crate) fn filled<T: Clone>(len: usize, value: T) -> Result<Vec<T>, OutOfMemory> {
    let mut v = with_capacity(len)?;
    v.resize(len, value);
    Ok(v)
}

/// Makes room for `additional` more elements, and no more than that.
pub(crate) fn reserve<T>(v: &mut Vec<T>, additional: usize) -> Result<(), OutOfMemory> {
    v.try_reserve_exact(additional).map_err(|_| OutOfMemory)
}

/// Makes room for `additional` more elements, growing by doubling as
/// `push` does, but failing instead of aborting.
pub(crate) fn grow<T>(v: &mut Vec<T>, additional: usize) -> Result<(), OutOfMemory> {
    v.try_reserve(additional).map_err(|_| OutOfMemory)
}

/// [`grow`] for the encoder's output buffers.
pub(crate) fn grow_output(v: &mut Vec<u8>, additional: usize) -> Result<(), OutOfMemory> {
    #[cfg(test)]
    if inject::refused(inject::Site::EncodeOutput) {
        return Err(OutOfMemory);
    }
    grow(v, additional)
}

/// A zeroed block of `len` samples for the wavelet transform to transpose
/// lines through.
pub(crate) fn transposition_block(len: usize) -> Result<Vec<f32>, OutOfMemory> {
    #[cfg(test)]
    if inject::refused(inject::Site::Transposition) {
        return Err(OutOfMemory);
    }
    filled(len, 0.0)
}

/// Makes allocations fail in tests, on the threads that [`crate::par`]
/// starts as well as on the one that asks.
#[cfg(test)]
pub(crate) mod inject {
    use std::cell::RefCell;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Where an allocation is refused.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(crate) enum Site {
        /// The encoder's output buffers ([`super::grow_output`]).
        EncodeOutput,
        /// The wavelet transform's transposition blocks.
        Transposition,
    }

    /// Allocations at `site` are refused once `allowed` of them were made.
    #[derive(Clone)]
    pub(crate) struct Injection {
        site: Site,
        allowed: Arc<AtomicUsize>,
    }

    thread_local! {
        static CURRENT: RefCell<Option<Injection>> = const { RefCell::new(None) };
    }

    /// What the calling thread is set to refuse, for a new thread to inherit.
    pub(crate) fn current() -> Option<Injection> {
        CURRENT.with(|c| c.borrow().clone())
    }

    /// Sets what the calling thread refuses.
    pub(crate) fn set(injection: Option<Injection>) {
        CURRENT.with(|c| *c.borrow_mut() = injection);
    }

    /// Whether the allocation at `site` that is being made is refused.
    pub(crate) fn refused(site: Site) -> bool {
        current().is_some_and(|i| {
            i.site == site
                && i.allowed
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                    .is_err()
        })
    }

    /// Runs `f` with the allocations at `site` refused after `allowed` of
    /// them succeeded, on this thread and on those it starts.
    pub(crate) fn refusing<R>(site: Site, allowed: usize, f: impl FnOnce() -> R) -> R {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                set(None);
            }
        }
        set(Some(Injection {
            site,
            allowed: Arc::new(AtomicUsize::new(allowed)),
        }));
        let _reset = Reset;
        f()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_beyond_the_address_space_are_refused() {
        // More than `isize::MAX` bytes: refused on every target, whatever
        // memory the machine has.
        let len = isize::MAX as usize / 4 + 1;
        assert!(with_capacity::<f32>(len).is_err());
        assert!(filled(len, 0.0f32).is_err());
        let mut v = vec![1u8];
        assert!(reserve(&mut v, isize::MAX as usize).is_err());
        assert_eq!(v, [1]);
    }

    #[test]
    fn small_sizes_work() {
        assert_eq!(filled(3, 7i16).unwrap(), [7, 7, 7]);
        assert_eq!(with_capacity::<u8>(0).unwrap().len(), 0);
        assert_eq!(transposition_block(2).unwrap(), [0.0, 0.0]);
        let mut v = vec![1u8];
        reserve(&mut v, 9).unwrap();
        assert!(v.capacity() >= 10);
    }

    #[test]
    fn injection_refuses_after_the_allowed_allocations() {
        use inject::{Site, refusing};
        let mut v = Vec::new();
        refusing(Site::EncodeOutput, 2, || {
            assert!(grow_output(&mut v, 1).is_ok());
            assert!(grow_output(&mut v, 1).is_ok());
            assert!(grow_output(&mut v, 1).is_err());
            // Other sites and plain helpers are not affected.
            assert!(transposition_block(1).is_ok());
            assert!(grow(&mut v, 1).is_ok());
        });
        assert!(grow_output(&mut v, 1).is_ok());
    }
}
