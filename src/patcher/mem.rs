//! This module contains a patcher which adjusts memory permissions to patch read-only data

use region::Protection;
use thiserror::Error;

use super::{PatchGuard, Patcher};

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
    /// Underlying patcher.
    patcher: P,
}
impl<P: Patcher> PermissionWrapper<P> {
    /// Creates a new PermissionWrapper
    pub fn new(patcher: P) -> Self {
        Self { patcher }
    }
}

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

unsafe impl<P> Patcher for PermissionWrapper<P>
where
    P: Patcher,
    PermissionError<P::Error>: From<P::Error>,
{
    type Error = PermissionError<P::Error>;
    type Guard<'a> = PermissionWrapperGuard<P::Guard<'a>> where Self: 'a;

    unsafe fn patch<'a>(
        &'a self,
        location: *mut u8,
        patch: &[u8],
    ) -> Result<Self::Guard<'a>, Self::Error> {
        let _guard = region::protect_with_handle(location, patch.len(), Protection::all())?;
        self.patcher
            .patch(location, patch)
            .map(|g| PermissionWrapperGuard::guard(g, location, patch.len()))
            .map_err(Into::into)
    }
}

/// Permission guard for the underlying patch guard
pub struct PermissionWrapperGuard<G: PatchGuard> {
    /// Underlying patch guard for the wrapped patcher. `Option` so that we can drop it in our [`Drop::drop`] impl
    guard: Option<G>,
    /// Location of the patch
    location: *const u8,
    /// Length of the patch
    len: usize,
}
impl<G: PatchGuard> PermissionWrapperGuard<G> {
    /// Wrap a patcher's guard. When this guard is dropped, the underlying guard will also be dropped with its target location made writable
    fn guard(guard: G, location: *const u8, len: usize) -> Self {
        let guard = Some(guard);
        Self {
            guard,
            location,
            len,
        }
    }
}
unsafe impl<G: PatchGuard> PatchGuard for PermissionWrapperGuard<G> {}

impl<P: PatchGuard> Drop for PermissionWrapperGuard<P> {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: We already changed memory permissions to construct the wrapper, so we shouldn't run into errors here
            let _guard =
                region::protect_with_handle(self.location, self.len, Protection::all()).unwrap();
            // `self.patcher` should never be `None` while we are alive
            self.guard.take().unwrap().restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use region::Protection;

    use crate::patcher::byte::BytePatcher;
    use crate::patcher::mem::{to_mut, PermissionWrapper};
    use crate::patcher::PatchGuard;
    use crate::patcher::Patcher;

    #[test]
    /// Test patch and revert functionality
    fn test_patch() {
        let vec = vec![1u8, 2, 3, 4];
        let (ptr, size, capacity) = vec.into_raw_parts();

        // sanity check
        assert_eq!(unsafe { slice::from_raw_parts(ptr, size) }, [1, 2, 3, 4]);

        // create the patcher and wrapper
        let patcher = BytePatcher::new();
        let wrapper = PermissionWrapper::new(patcher);

        // patch the vec's data
        let patch = unsafe { wrapper.patch(ptr, &[4, 3, 2, 1]).unwrap() };

        // make sure the data was actually changed
        assert_eq!(unsafe { slice::from_raw_parts(ptr, size) }, [4, 3, 2, 1]);

        // restore the patch
        patch.restore();

        // make sure the patch was restored
        assert_eq!(unsafe { slice::from_raw_parts(ptr, size) }, [1, 2, 3, 4]);

        // clean up
        let _ = unsafe { Vec::from_raw_parts(ptr, size, capacity) };
    }

    #[test]
    /// Tests to ensure permissions are actually set
    fn test_perms() {
        // Global immutables are stored in a read-only section in the binary.
        // Normally, writing to this global would result in a segfault, but PermissionWrapper changes the permissions to be writable so that no fault occurs
        let data = b"1234";

        let ptr = data.as_ptr();
        let size = data.len();

        // sanity check
        // make sure the data is what we expect and that the data is definitely read-only
        // Note: we can't do `b"1234"` because it might get compiled to the same global const that we're modifying
        assert_eq!(
            unsafe { slice::from_raw_parts(ptr, size) },
            [b'1', b'2', b'3', b'4']
        );
        for region in region::query_range(ptr, size).unwrap() {
            let region = region.unwrap();
            assert!(!region.is_guarded());
            assert_eq!(region.protection(), Protection::READ);
        }

        // create the patcher and wrapper
        let patcher = BytePatcher::new();
        let wrapper = PermissionWrapper::new(patcher);

        // patch the vec's data
        let patch = unsafe { wrapper.patch(to_mut(ptr), &[4, 3, 2, 1]).unwrap() };

        // make sure the data was actually changed
        assert_eq!(unsafe { slice::from_raw_parts(ptr, size) }, [4, 3, 2, 1]);

        // make sure permissions reverted correctly after the patch
        for region in region::query_range(ptr, size).unwrap() {
            let region = region.unwrap();
            assert!(!region.is_guarded());
            assert_eq!(region.protection(), Protection::READ);
        }

        // restore the patch
        patch.restore();

        // make sure the patch was restored
        assert_eq!(
            unsafe { slice::from_raw_parts(ptr, size) },
            [b'1', b'2', b'3', b'4']
        );

        // make sure permissions were restored
        for region in region::query_range(ptr, size).unwrap() {
            let region = region.unwrap();
            assert!(!region.is_guarded());
            assert_eq!(region.protection(), Protection::READ);
        }
    }
}
