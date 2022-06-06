//! # Hook
//!
//! This module covers hooks, which redirect execution from one location to another

pub mod jmphook;

/// Trait for hooks
///
/// # Safety
///
/// Hooks are inherently unsafe; it is up to the hook
pub unsafe trait Hook {
    /// Errors that could happen during a hook
    type Error;
    /// Hook guard to allow functions to be automatcially unhooked when the guard goes out of scope
    type Guard<'a>: HookGuard + 'a
    where
        Self: 'a;

    /// Hooks the `location` provided by the `create` function and redirects execution to the `location` of this function
    /// Creates a hook which redirects `source` to `destination`.
    ///
    /// # Safety
    ///
    /// - Both `source` and `destination` must be valid pointers
    /// - `destination` must be valid executable code
    unsafe fn hook(
        &self,
        source: *const u8,
        destination: *const u8,
    ) -> Result<Self::Guard<'_>, Self::Error>;
}

/// Guard for a currently active hook
///
/// # Safety
///
/// Must ensure that the guard fully unhooks whether dropped or unhooked via `unhook`
pub unsafe trait HookGuard: Sized {
    /// Manually unhooks the hook rather than letting the guard go out of scope
    fn unhook(self) {
        // most guards will implement all functionality in [`Drop::drop`]
    }
}
