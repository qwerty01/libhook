//! This module contains a byte patcher

use std::ptr;

use super::{PatchGuard, Patcher};

/// Patcher for patching memory locations with byte arrays.
/// This patcher never fails.
#[derive(Default)]
pub struct BytePatcher;
impl BytePatcher {
    /// Creates a new [`BytePatcher`]
    pub fn new() -> Self {
        Self::default()
    }
}
unsafe impl Patcher for BytePatcher {
    type Error = ();
    type Guard<'a> = BytePatchGuard;

    unsafe fn patch<'a>(
        &'a self,
        location: *mut u8,
        patch: &[u8],
    ) -> Result<Self::Guard<'a>, Self::Error> {
        Ok(BytePatchGuard::patch(location, patch))
    }
}
/// Guard for byte-patches
///
/// See [`BytePatcher`].
pub struct BytePatchGuard {
    /// Original data from `location`
    original: Vec<u8>,
    /// Location of the patch
    location: *mut u8,
}
impl BytePatchGuard {
    /// Patches a location, returning a guard for unpatching
    ///
    /// # Safety
    ///
    /// `location` must be a valid pointer
    unsafe fn patch(location: *mut u8, patch: &[u8]) -> Self {
        let mut original = Vec::with_capacity(patch.len());

        // Safety: caller must pass in a `location` pointer that is valid for the full length of the patch
        ptr::copy(location, original.as_mut_ptr(), patch.len());

        // Safety: We initialized the vec to patch.len(), so fix the length
        original.set_len(patch.len());

        let guard = Self { original, location };

        // Safety: caller must ensure that `location` is writable
        ptr::copy(patch.as_ptr(), location, patch.len());

        guard
    }
}
unsafe impl PatchGuard for BytePatchGuard {}
impl Drop for BytePatchGuard {
    fn drop(&mut self) {
        // Safety: creator must pass in a `location` pointer that is valid and writable for the full length of the patch
        unsafe {
            ptr::copy(self.original.as_ptr(), self.location, self.original.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use crate::patcher::byte::BytePatcher;
    use crate::patcher::{PatchGuard, Patcher};

    #[test]
    /// Test patch and revert functionality
    fn test_patch() {
        let vec = vec![1u8, 2, 3, 4];
        let (ptr, size, capacity) = vec.into_raw_parts();

        // sanity check
        assert_eq!(unsafe { slice::from_raw_parts(ptr, size) }, [1, 2, 3, 4]);

        // get our patcher to test
        let patcher = BytePatcher::new();

        // patch the vec's data
        let patch = unsafe { patcher.patch(ptr, &[4, 3, 2, 1]).unwrap() };

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
    /// Tests a partial patch of a block to ensure we're not overwriting outside the patch area
    fn test_partial_patch() {
        let vec = vec![1u8, 2, 3, 4];
        let (ptr, size, capacity) = vec.into_raw_parts();

        // sanity check
        assert_eq!(unsafe { slice::from_raw_parts(ptr, size) }, [1, 2, 3, 4]);

        // get our patcher to test
        let patcher = BytePatcher::new();

        // patch the vec's data
        let patch = unsafe { patcher.patch((ptr as usize + 1) as _, &[5, 5]).unwrap() };

        // make sure the data was actually changed
        assert_eq!(unsafe { slice::from_raw_parts(ptr, size) }, [1, 5, 5, 4]);

        // restore the patch
        patch.restore();

        // make sure the patch was restored
        assert_eq!(unsafe { slice::from_raw_parts(ptr, size) }, [1, 2, 3, 4]);

        // clean up
        let _ = unsafe { Vec::from_raw_parts(ptr, size, capacity) };
    }
}
