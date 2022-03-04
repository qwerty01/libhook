//! # Patch
//!
//! This module covers patchers, which are used to overwrite and restore locations in memory

use std::ptr;

use iced_x86::IcedError;
use region::Protection;
use thiserror::Error;

use crate::ExecutableBuffer;

/// All patchers save state from where they patched and are able to revert on-command
pub trait Patcher
where
    Self: Sized,
{
    /// Error type that can occur when patching. If patching always succeeds, use `()`.
    type Error;
    /// Patches a given location.
    ///
    /// # Safety
    ///
    /// This function is intended to be used on arbitrary memory addresses.
    /// The caller must guarantee that `location` is [https://doc.rust-lang.org/stable/std/ptr/index.html#safety](valid) for the full size of the patch
    unsafe fn patch(location: *mut u8, patch: &[u8]) -> Result<Self, Self::Error>;
    /// Restores the original value of a patch
    ///
    /// # Safety
    ///
    /// There is no way to guarantee that the original value is still valid for the patch location.
    /// The caller must ensure that reverting to a pre-patched state is safe for the location that has been patched
    unsafe fn restore(self);
}

/// Errors when using permission patching
#[derive(Debug, Error)]
pub enum PermissionError<E> {
    /// Error when setting memory protections
    #[error("Error setting memory protections")]
    ProtectionError(#[from] region::Error),
    /// Custom error type from the underlying patcher
    #[error("{0}")]
    CustomError(E),
}

impl From<()> for PermissionError<()> {
    fn from(e: ()) -> Self {
        Self::CustomError(e)
    }
}

/// This struct wraps patchers to allow them to write to memory that's normally unwritable.
/// It achieves this result by changing the memory permissions of the target memory, triggering the patch, and then reverting the permissions.
///
/// # Safety
///
/// `PermissionWrapper` relies on the size of the patch value to determine how many pages to change write permissions,
/// pairing `PermissionWrapper` with a patcher that writes more memory than the size of the patch is undefined behavior.
///
/// As always, casting a `&T` or `&mut T` to a `*mut u8` for use with `PermissionWrapper` can result in  undefined behavior because rust assumes `&T` will never change and `&mut T` will only be changed via that reference.
/// The `*mut u8` **MUST** be memory not tracked by Rust, or ensured that reading from and writing to data tracked by Rust will not trigger undefined behavior.
pub struct PermissionWrapper<P: Patcher> {
    /// Underlying patcher. Option used so that we can drop `patcher` in our [`Drop::drop`] implementation
    patcher: Option<P>,
    /// Location that was patched
    location: *mut u8,
    /// Length of the patch
    len: usize,
}
impl<P: Patcher> PermissionWrapper<P> {
    /// Converts a const pointer to a mutable pointer to be passed into our [`Patcher::patch`] implementation.
    ///
    /// # Safety
    ///
    /// **THIS FUNCTION DOES NOT CHANGE MEMORY PERMISSIONS.**
    ///
    /// It is **NOT** safe to treat the returned value as mutable, as this function does not change memory permissions.
    ///
    /// This function should **ONLY** be called in conjunction with our [`Patcher::patch`] implementation, which properly changes the memory permissions.
    pub unsafe fn to_mut<T>(ptr: *const T) -> *mut T {
        ptr as _
    }
}

impl<P> Patcher for PermissionWrapper<P>
where
    P: Patcher,
    PermissionError<P::Error>: From<P::Error>,
{
    type Error = PermissionError<P::Error>;

    unsafe fn patch(location: *mut u8, patch: &[u8]) -> Result<Self, Self::Error> {
        let _guard = region::protect_with_handle(location, patch.len(), Protection::all())?;
        let patcher = P::patch(location, patch)?;
        Ok(Self {
            patcher: Some(patcher),
            location,
            len: patch.len(),
        })
    }

    unsafe fn restore(self) {
        // Implemented in [`Drop::drop`]
    }
}
impl<P: Patcher> Drop for PermissionWrapper<P> {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: We already changed memory permissions to construct the wrapper, so we shouldn't run into errors here
            let _guard =
                region::protect_with_handle(self.location, self.len, Protection::all()).unwrap();
            // `self.patcher` should never be `None` while we are alive
            self.patcher.take().unwrap().restore();
        }
    }
}

/// Patcher for patching memory locations with byte arrays.
/// This patcher never fails.
pub struct BytePatcher {
    /// Original data from `location`
    original: Vec<u8>,
    /// Location of the patch
    location: *mut u8,
}

impl Patcher for BytePatcher {
    type Error = ();

    unsafe fn patch(location: *mut u8, patch: &[u8]) -> Result<Self, Self::Error> {
        let mut original = Vec::with_capacity(patch.len());
        // Safety: caller must pass in a `location` pointer that is valid for the full length of the patch
        ptr::copy(location, original.as_mut_ptr(), patch.len());
        let patcher = Self { original, location };
        // Safety: caller must ensure that `location` is writable
        ptr::copy(patch.as_ptr(), location, patch.len());
        Ok(patcher)
    }

    unsafe fn restore(self) {
        // implemented in `drop`
    }
}
impl Drop for BytePatcher {
    fn drop(&mut self) {
        // Safety: creator must pass in a `location` pointer that is valid and writable for the full length of the patch
        unsafe {
            ptr::copy(self.original.as_ptr(), self.location, self.original.len());
        }
    }
}

#[derive(Debug, Error)]
/// Error types for `DisasmPatcher`
pub enum DisasmError<E> {
    /// Error while writing data to memory
    #[error("{0}")]
    PermissionError(#[from] PermissionError<E>),
    /// Error while disassembling target location
    #[error("{0}")]
    IcedError(#[from] IcedError),
    /// Error while setting permissions of executable buffer
    #[error("{0}")]
    BufferError(#[from] region::Error),
}

/// Wrapper for patching code sections that may need to patch more bytes than what's provided
///
/// Because code is often read-only, this patcher wraps the main patcher with a `PermissionWrapper` automatically
///
/// # Safety
///
/// `DisasmPatcher` disassembles the target to determine how many bytes to patch.
/// The caller must therefore ensure that `location` is valid for the patch size + 14
/// (enough space to ensure that if `location` ends on the first byte of the largest instruction size (15),
/// we can still disassemble the full instruction)
pub struct DisasmPatcher<P: Patcher> {
    /// Internal patcher that will actually write the data we create
    #[allow(unused)]
    patcher: PermissionWrapper<P>,
    /// Original data that was patched. Created such that `original` contains safely moved code that can be executed as if you were executing the original code.
    original: ExecutableBuffer,
}
impl<P: Patcher> DisasmPatcher<P> {
    /// Returns a pointer to the original function.
    ///
    /// This pointer is directly callable and will act as if you're calling the original unpatched function
    pub fn original(&self) -> *const u8 {
        self.original.as_ptr()
    }
}

impl<P> Patcher for DisasmPatcher<P>
where
    P: Patcher,
    PermissionError<P::Error>: From<P::Error>,
{
    type Error = DisasmError<P::Error>;

    unsafe fn patch(location: *mut u8, patch: &[u8]) -> Result<Self, Self::Error> {
        // TODO: use `BlockEncoder` to generate the actual patch
        let patcher = PermissionWrapper::patch(location, patch)?;
        let original = ExecutableBuffer::new_uninit(0)?;
        Ok(Self { patcher, original })
    }

    unsafe fn restore(self) {
        // Implemented in `drop`
    }
}
impl<P: Patcher> Drop for DisasmPatcher<P> {
    fn drop(&mut self) {
        // Handled by PermissionWrapper's drop
    }
}
