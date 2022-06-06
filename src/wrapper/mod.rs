//! # Wrapper
//!
//! This takes code execution from a given calling convention specification and standardizes it

pub mod cdecl;
pub mod convention;

/// Call wrappers take execution and standardize the calling convention to call some other function
///
/// # Safety
///
/// Call wrappers need to correctly handle the old calling convention and standardize the calling convention correctly
pub unsafe trait CallWrapper {
    /// Errors that could happen when wrapping calls
    type Error;
    /// Wrapper guard to allow cleanup automatically when the guard goes out of scope
    type Guard<'a>: CallWrapperGuard + 'a
    where
        Self: 'a;

    /// Activates the wrapper
    ///
    /// - `src`: the source location where execution is coming from
    /// - `dst`: the destination where execution is being sent to
    ///
    /// # Safety
    ///
    /// - `src` must be compatible with the calling convention specified by this wrapper
    /// - `dst` must follow the standardized calling convention used by this wrapper
    unsafe fn activate(
        &self,
        src: *const u8,
        dst: *const u8,
    ) -> Result<Self::Guard<'_>, Self::Error>;
}

/// Guard for callwrappers to clean up correctly
///
/// # Safety
///
/// Guards must clean up fully even if [`drop`] is not called
pub unsafe trait CallWrapperGuard: Sized {
    /// Drops the guard
    fn drop(self) {
        // most guards will cleanup in [`Drop::drop`]
    }
}
