//! # Patch
//!
//! This module covers patchers, which are used to overwrite and restore locations in memory

pub mod byte;
pub mod code;
pub mod mem;

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
