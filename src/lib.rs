#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(rustdoc::missing_doc_code_examples)]
#![feature(vec_into_raw_parts)]
#![doc = include_str!("../README.md")]

use region::{ProtectGuard, Protection};
use std::slice;

pub mod patch;

/// Stores data as executable memory
pub struct ExecutableBuffer {
    /// Old memory protections of the data location
    // Note: guard declared first so that it's dropped first
    #[allow(unused)]
    guard: Option<ProtectGuard>,
    /// Data that is stored in an executable location
    data: *mut u8,
    /// Size of the buffer at `data`
    size: usize,
    /// Capacity of the allocated block that `data` resides in
    capacity: usize,
}

impl ExecutableBuffer {
    /// Create a new buffer with the given data
    pub fn new(value: Vec<u8>) -> Result<Self, region::Error> {
        let (data, size, capacity) = value.into_raw_parts();
        let guard = unsafe { region::protect_with_handle(data, size, Protection::all())? };
        Ok(Self {
            guard: Some(guard),
            data,
            size,
            capacity,
        })
    }

    /// Create a new empty buffer with the given size.
    ///
    /// # Safety
    ///
    /// Data starts uninitialized, but must be initialized before this object is dropped.
    pub fn new_uninit(size: usize) -> Result<Self, region::Error> {
        let value = Vec::with_capacity(size);
        let (data, _, capacity) = value.into_raw_parts();
        let guard = unsafe { region::protect_with_handle(data, size, Protection::all())? };
        Ok(Self {
            guard: Some(guard),
            data,
            size,
            capacity,
        })
    }

    /// Get a pointer to the executable data
    ///
    /// # Safety
    ///
    /// It is undefined behavior to dereference this pointer while a mutable reference from [`Self::as_mut`] is active
    pub fn as_ptr(&self) -> *const u8 {
        self.data as _
    }

    /// Get a mutable pointer to the executable data
    ///
    /// # Safety
    ///
    /// It is undefined behavior to dereference this pointer while a reference to the internal buffer is active
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data
    }

    /// Get the size of the buffer
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.size
    }
}

impl AsRef<[u8]> for ExecutableBuffer {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: self.data is properly aligned because of how we create it
        unsafe { slice::from_raw_parts(self.data, self.size) }
    }
}

impl AsMut<[u8]> for ExecutableBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: self.data is properly aligned because of how we create it
        unsafe { slice::from_raw_parts_mut(self.data, self.size) }
    }
}

impl Drop for ExecutableBuffer {
    fn drop(&mut self) {
        // `self.guard` should always be `Some` while we are alive. If this panics, we somehow double freed.
        // Drop the guard immediately to restore the original protections
        let _ = self.guard.take().unwrap();

        // Drop the vec immediately since we're done using it
        let _ = unsafe { Vec::from_raw_parts(self.data, self.size, self.capacity) };
    }
}
