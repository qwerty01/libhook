//! # Jump Hook
//!
//! This hook type uses a basic `jmp` instruction to redirect execution

use crate::{
    code::x64::jmp_abs,
    patcher::{PatchGuard, Patcher},
};

use super::{Hook, HookGuard};

/// Simple jmp hook
pub struct JmpHook<P> {
    /// Underlying patcher to be used to hook
    patcher: P,
}
impl<P: Patcher> JmpHook<P> {
    /// Creates a new jmp hook
    pub fn new(patcher: P) -> Self {
        Self { patcher }
    }
}
unsafe impl<P: Patcher> Hook for JmpHook<P> {
    type Error = P::Error;
    type Guard<'a> = JmpHookGuard<P::Guard<'a>>
    where
        Self: 'a;

    unsafe fn hook(
        &self,
        source: *const u8,
        destination: *const u8,
    ) -> Result<Self::Guard<'_>, Self::Error> {
        // patch with an absolute jmp to the destination
        let patch = self
            .patcher
            .patch(source as _, &jmp_abs(destination as _))?;

        Ok(JmpHookGuard::new(patch))
    }
}

/// Guard for jmp hooks
#[allow(dead_code)]
pub struct JmpHookGuard<G: PatchGuard> {
    /// Underlying patch guard that we're wrapping
    guard: G,
}
impl<G: PatchGuard> JmpHookGuard<G> {
    /// Creates a new jmp hook guard that wraps `guard`
    fn new(guard: G) -> Self {
        Self { guard }
    }
    /// Get the underlying patch guard in case info is needed
    pub fn patch(&self) -> &G {
        &self.guard
    }
}
unsafe impl<G: PatchGuard> HookGuard for JmpHookGuard<G> {}
