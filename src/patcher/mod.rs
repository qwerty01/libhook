//! # Patch
//!
//! This module covers patchers, which are used to overwrite and restore locations in memory

pub mod byte;
pub mod code;
pub mod mem;

/// All patchers save state from where they patched and are able to revert on-command
///
/// # Safety
///
/// Patchers are inherently unsafe. The implementor must ensure that the implementation of `patch` works correctly and is properly documented for avoiding undefined behavior
pub unsafe trait Patcher {
    /// Error type that can occur when patching. If patching always succeeds, use `()`.
    type Error;
    /// Guard type for the patcher. When this guard is dropped, the location should be restored.
    type Guard<'a>: PatchGuard + 'a
    where
        Self: 'a;

    /// Patches a given location.
    ///
    /// # Safety
    ///
    /// This function is intended to be used on arbitrary memory addresses, but must be valid for the supplied patcher
    unsafe fn patch<'a>(
        &'a self,
        target: *mut u8,
        patch: &[u8],
    ) -> Result<Self::Guard<'a>, Self::Error>;
}

/// Guard for a patch
///
/// # Safety
///
/// Guard must fully unpatch the location when dropped, even if `restore` is not called
pub unsafe trait PatchGuard: Sized {
    /// Restores the original value of a patch
    fn restore(self) {
        // most implementations have their functionality in their [`Drop::drop`] implementation
    }
}
