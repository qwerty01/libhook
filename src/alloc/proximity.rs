//! Proximity allocator
//!
//! Taken from detour-rs with slight modifications: https://github.com/darfink/detour-rs

// detour-rs - A cross-platform detour library written in Rust
// Copyright (C) 2017 Elliott Linder.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
//
//  1. Redistributions of source code must retain the above copyright
//     notice, this list of conditions and the following disclaimer.
//  2. Redistributions in binary form must reproduce the above copyright
//     notice, this list of conditions and the following disclaimer in the
//     documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER
// OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
// EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
// PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
// LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
// NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// ===============================================================================
//
// minhook-rs - A minimalist x86/x86-64 hooking library for Rust
// Copyright (C) 2015 Jascha Neutelings.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
//
//  1. Redistributions of source code must retain the above copyright
//     notice, this list of conditions and the following disclaimer.
//  2. Redistributions in binary form must reproduce the above copyright
//     notice, this list of conditions and the following disclaimer in the
//     documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER
// OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
// EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
// PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
// LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
// NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::error::Error;
use std::slice;
use std::{fmt::Display, ops::Range};

use slice_pool::sync::{SliceBox, SlicePool};

use super::search as region_search;

/// Defines the allocation type.
pub type Allocation = SliceBox<u8>;

#[derive(Debug)]
/// Errors that occur while creating proximity allocations
pub enum ProximityError {
    /// Ran out of memory within an acceptable proximity to the allocation location
    OutOfMemory,
    /// Error while memmapping a region
    MmapError(mmap::MapError),
    /// Error while querying a memory region
    RegionError(region::Error),
}
impl Display for ProximityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfMemory => write!(
                f,
                "Ran out of memory within an acceptable proximity to the allocation location"
            ),
            Self::MmapError(e) => write!(f, "{e}"),
            Self::RegionError(e) => write!(f, "{e}"),
        }
    }
}
impl Error for ProximityError {}

/// Shared instance containing all pools
pub struct ProximityAllocator {
    /// Max distance away from the origin that the pool can be
    pub max_distance: usize,
    /// Memory pools used for allocations
    pub pools: Vec<SlicePool<u8>>,
}

impl ProximityAllocator {
    /// Allocates a slice in an eligible memory map.
    pub fn allocate(&mut self, origin: usize, size: usize) -> Result<Allocation, ProximityError> {
        let memory_range =
            (origin.saturating_sub(self.max_distance))..(origin.saturating_add(self.max_distance));

        // Check if an existing pool can handle the allocation request
        self.allocate_memory(&memory_range, size).or_else(|e| {
            if !matches!(e, ProximityError::OutOfMemory) {
                // make sure the error is that the pool is out of memory
                return Err(e);
            }
            // ... otherwise allocate a pool within the memory range
            self.allocate_pool(&memory_range, origin, size)
                .and_then(|pool| {
                    // Use the newly allocated pool for the request
                    let allocation = pool.alloc(size).ok_or(ProximityError::OutOfMemory)?;
                    self.pools.push(pool);
                    Ok(allocation)
                })
        })
    }

    /// Releases the memory pool associated with an allocation.
    pub fn release(&mut self, value: &Allocation) {
        // Find the associated memory pool
        let index = self
            .pools
            .iter()
            .position(|pool| {
                let lower = pool.as_ptr() as usize;
                let upper = lower + pool.len();

                // Determine if this is the associated memory pool
                (lower..upper).contains(&(value.as_ptr() as usize))
            })
            .expect("retrieving associated memory pool");

        // Release the pool if the associated allocation is unique
        if self.pools[index].len() == 1 {
            self.pools.remove(index);
        }
    }

    /// Allocates a chunk using any of the existing pools.
    fn allocate_memory(
        &mut self,
        range: &Range<usize>,
        size: usize,
    ) -> Result<Allocation, ProximityError> {
        // Returns true if the pool's memory is within the range
        let is_pool_in_range = |pool: &SlicePool<u8>| {
            let lower = pool.as_ptr() as usize;
            let upper = lower + pool.len();
            range.contains(&lower) && range.contains(&(upper - 1))
        };

        // Tries to allocate a slice within any eligible pool
        self.pools
            .iter_mut()
            .filter_map(|pool| {
                if is_pool_in_range(pool) {
                    pool.alloc(size)
                } else {
                    None
                }
            })
            .next()
            .ok_or(ProximityError::OutOfMemory)
    }

    /// Allocates a new pool close to `origin`.
    fn allocate_pool(
        &mut self,
        range: &Range<usize>,
        origin: usize,
        size: usize,
    ) -> Result<SlicePool<u8>, ProximityError> {
        let before = region_search::before(origin, Some(range.clone()));
        let after = region_search::after(origin, Some(range.clone()));

        // TODO: Part of the pool can be out of range
        // Try to allocate after the specified address first (mostly because
        // macOS cannot allocate memory before the process's address).
        after
            .chain(before)
            .find_map(|result| match result {
                Ok(address) => Self::allocate_fixed_pool(address, size).ok().map(Ok),
                Err(error) => Some(Err(ProximityError::RegionError(error))),
            })
            .unwrap_or(Err(ProximityError::OutOfMemory))
    }

    /// Tries to allocate fixed memory at the specified address.
    fn allocate_fixed_pool(
        address: *const (),
        size: usize,
    ) -> Result<SlicePool<u8>, ProximityError> {
        // Try to allocate memory at the specified address
        mmap::MemoryMap::new(
            size,
            &[
                mmap::MapOption::MapReadable,
                mmap::MapOption::MapWritable,
                mmap::MapOption::MapExecutable,
                mmap::MapOption::MapAddr(address as *const _),
            ],
        )
        .map_err(|e| match e {
            mmap::MapError::ErrNoMem => ProximityError::OutOfMemory,
            e => ProximityError::MmapError(e),
        })
        .map(SliceableMemoryMap)
        .map(SlicePool::new)
    }
}

// TODO: Use memmap-rs instead
/// A wrapper for making a memory map compatible with `SlicePool`.
struct SliceableMemoryMap(mmap::MemoryMap);

impl SliceableMemoryMap {
    /// Get a slice of the memory map
    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.0.data(), self.0.len()) }
    }

    /// Get a mutable slice of the memory map
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.0.data(), self.0.len()) }
    }
}

impl AsRef<[u8]> for SliceableMemoryMap {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for SliceableMemoryMap {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

unsafe impl Send for SliceableMemoryMap {}
unsafe impl Sync for SliceableMemoryMap {}
