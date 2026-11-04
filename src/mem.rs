//! Fallible allocation of the buffers whose size follows the image.
//!
//! `vec![0; n]` and `Vec::with_capacity` abort the process when the
//! allocator refuses, and panic when `n` bytes exceed `isize::MAX`, which
//! a 65535 by 65535 image does on 32-bit targets. These helpers turn both
//! into a value the caller reports as an error.

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
        let mut v = vec![1u8];
        reserve(&mut v, 9).unwrap();
        assert!(v.capacity() >= 10);
    }
}
