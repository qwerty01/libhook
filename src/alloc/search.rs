//! Searches memory for a location close to a specified address to allocate
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

use std::ops::Range;

/// Returns an iterator for free after the specified address.
pub fn after(
    origin: usize,
    range: Option<Range<usize>>,
) -> impl Iterator<Item = Result<*const (), region::Error>> {
    FreeRegionIter::new(origin, range, SearchDirection::After)
}

/// Returns an iterator for free before the specified address.
pub fn before(
    origin: usize,
    range: Option<Range<usize>>,
) -> impl Iterator<Item = Result<*const (), region::Error>> {
    FreeRegionIter::new(origin, range, SearchDirection::Before)
}

#[allow(clippy::missing_docs_in_private_items)]
/// Direction for the region search.
enum SearchDirection {
    Before,
    After,
}

/// An iterator searching for free regions.
struct FreeRegionIter {
    /// Range we're iterating over
    range: Range<usize>,
    /// Direction we're searching
    search: SearchDirection,
    /// Current location in the search
    current: usize,
}

impl FreeRegionIter {
    /// Creates a new iterator for free regions.
    fn new(origin: usize, range: Option<Range<usize>>, search: SearchDirection) -> Self {
        FreeRegionIter {
            range: range.unwrap_or(0..usize::max_value()),
            current: origin as usize,
            search,
        }
    }
}

impl Iterator for FreeRegionIter {
    type Item = Result<*const (), region::Error>;

    /// Returns the closest free region for the current address.
    fn next(&mut self) -> Option<Self::Item> {
        let page_size = region::page::size();

        while self.current > 0 && self.range.contains(&self.current) {
            match region::query(self.current as *const ()) {
                Ok(region) => {
                    self.current = match self.search {
                        SearchDirection::Before => {
                            region.as_range().start.saturating_sub(page_size)
                        }
                        SearchDirection::After => region.as_range().end,
                    }
                }
                Err(error) => {
                    // Check whether the region is free, otherwise return the error
                    let result = Some(match error {
                        region::Error::UnmappedRegion => Ok(self.current as *const _),
                        inner => Err(inner),
                    });

                    // Adjust the offset for repeated calls.
                    self.current = match self.search {
                        SearchDirection::Before => self.current.saturating_sub(page_size),
                        SearchDirection::After => self.current + page_size,
                    };

                    return result;
                }
            }
        }

        None
    }
}
