/// Trait for hooks
///
/// TODO: explain the precise definition of a hook
pub trait Hook {
    /// Hook guard to allow functions to be automatcially unhooked when the guard goes out of scope
    type Guard<'a>: HookGuard + 'a
    where
        Self: 'a;

    /// Creates a new generic hook moving execution from `location` to some destination
    fn create(location: *const u8) -> Self;
    /// Hooks the `location` provided by the `create` function and redirects execution to the `location` of this function
    fn hook(&mut self, location: *const u8) -> Self::Guard<'_>;
    /// Returns where the destination location should return to after executing
    fn ret(&self) -> usize;
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
