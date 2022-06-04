//! Allocates buffers near a given address
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

use lazy_static::lazy_static;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use self::proximity::ProximityError;

pub mod proximity;
pub mod search;

/// A thread-safe memory pool for allocating chunks close to addresses.
pub struct ThreadAllocator(Arc<Mutex<proximity::ProximityAllocator>>);

// TODO: Decrease use of mutexes
impl ThreadAllocator {
    /// Creates a new proximity memory allocator.
    pub fn new(max_distance: usize) -> Self {
        ThreadAllocator(Arc::new(Mutex::new(proximity::ProximityAllocator {
            max_distance,
            pools: Vec::new(),
        })))
    }

    /// Allocates read-, write- & executable memory close to `origin`.
    pub fn allocate(&self, origin: usize, size: usize) -> Result<ExecutableMemory, ProximityError> {
        let mut allocator = self.0.lock().unwrap();
        allocator
            .allocate(origin, size)
            .map(|data| ExecutableMemory {
                allocator: self.0.clone(),
                data,
            })
    }
}

/// A handle for allocated proximity memory.
pub struct ExecutableMemory {
    /// Proximity allocator for the executable code to reside
    allocator: Arc<Mutex<proximity::ProximityAllocator>>,
    /// Actual allocation where the executable code resides
    data: proximity::Allocation,
}

impl Drop for ExecutableMemory {
    fn drop(&mut self) {
        // Release the associated memory map (if unique)
        self.allocator.lock().unwrap().release(&self.data);
    }
}

impl Deref for ExecutableMemory {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.data.deref()
    }
}

impl DerefMut for ExecutableMemory {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.data.deref_mut()
    }
}

/// The furthest distance between a target and its detour (2 GiB).
// TODO: multi-arch support?
pub const DETOUR_RANGE: usize = 0x8000_0000;

lazy_static! {
    static ref POOL: ThreadAllocator = ThreadAllocator::new(DETOUR_RANGE);
}

/// Allocates an executable buffer
///
/// Note: When the executable buffer returns, the buffer's data is undefined, but valid u8 values
pub fn allocate_executable(origin: usize, size: usize) -> Result<ExecutableMemory, ProximityError> {
    POOL.allocate(origin, size)
}
