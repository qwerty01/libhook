//! This module contains a code patcher which can disassemble the target location and patch enough bytes to jump back
//!
//! Note that [`CodePatcher`] does *not* implement [`Patcher`].
//! This is because future pipelines may need to get the pointer to the original function *without* patching the location
//!
//! When you finally want to patch, use [`CodePatcher::patch`].

use std::marker::PhantomData;
use std::{iter, ptr, slice};

use iced_x86::{
    BlockEncoder, BlockEncoderOptions, Code, Decoder, DecoderOptions, IcedError, Instruction,
    InstructionBlock,
};
use thiserror::Error;

use crate::alloc::{allocate_executable, proximity::ProximityError, ExecutableMemory};

use super::byte::BytePatcher;
use super::mem::{to_mut, PermissionError, PermissionWrapper};
use super::Patcher;

#[derive(Debug, Error)]
/// Error types for `CodePatcher`
pub enum CodeError<E> {
    /// Error while writing data to memory
    #[error("{0}")]
    PermissionError(#[from] PermissionError<E>),
    /// Error while disassembling target location
    #[error("{0}")]
    IcedError(#[from] IcedError),
    /// Error while setting permissions of executable buffer
    #[error("{0}")]
    BufferError(#[from] region::Error),
    /// Error allocating an executable buffer
    #[error("{0}")]
    ProximityError(#[from] ProximityError),
    /// Buffer size that was allocated was too small.
    /// If you encounter this error, open an issue and include the full patch bytes, [allocated] bytes from the original function, and the location of the target and location value from this error.
    #[error("Buffer size was too small (allocated: {0}, needed: {1}, location: {2:?})")]
    BufferTooSmall(usize, usize, *const ()),
}

/// Wrapper for patching code sections that may need to patch more bytes than what's provided
///
/// Because code is often read-only, this patcher wraps the main patcher with a `PermissionWrapper` automatically
///
/// # Safety
///
/// `CodePatcher` disassembles the target to determine how many bytes to patch.
/// The caller must therefore ensure that `location` is valid for the patch size + 14
/// (enough space to ensure that if `location` ends on the first byte of the largest instruction size (15),
/// we can still disassemble the full instruction)
pub struct CodePatcher<P: Patcher, A: Architecture> {
    /// Internal patcher that will actually write the data we create
    patcher: PermissionWrapper<P>,
    /// Original data that was patched. Created such that `original` contains safely moved code that can be executed as if you were executing the original code.
    original: ExecutableMemory,
    /// Data to patch to the location
    patch: Vec<u8>,
    /// location to patch
    location: *const u8,
    /// Placeholder for architecture
    _arch: PhantomData<A>,
}

impl<P, A> CodePatcher<P, A>
where
    P: Patcher,
    A: Architecture,
    PermissionError<P::Error>: From<P::Error>,
{
    /// Creates a new CodePatcher
    ///
    /// Note: The patcher will be wrapped in a [`PermissionWrapper`], so there is no need to wrap it yourself
    ///
    /// # Safety
    ///
    /// `location` must point to valid executable code, valid for the length of `patch` + the max architecture
    pub unsafe fn new<B: AsRef<[u8]>>(
        patcher: P,
        location: *const u8,
        patch: B,
    ) -> Result<Self, CodeError<P::Error>> {
        let patch = patch.as_ref();
        let patcher = PermissionWrapper::new(patcher);

        // Length of patch + max instruction size
        let patch_size = patch.len();
        let max_size = patch_size + A::max_instr_len();

        // Actual patch data
        // Safety: the caller is required to ensure that `location` is valid
        let data = slice::from_raw_parts(location, max_size);

        // Create a decoder to figure out what length we need to patch
        let decoder = Decoder::with_ip(A::bitness(), data, location as u64, DecoderOptions::NONE);

        // Get the full patch length. This might be larger than the passed in patch if the location being patched has more instructions than the patch, but never smaller.
        let mut size = 0usize;
        let mut instructions: Vec<_> = decoder
            .into_iter()
            .take_while(|v| {
                let ret = size < patch_size; // include this instruction if it would go past the end
                size += v.len();
                ret
            })
            .collect();

        // Now that we have the list of instructions, get the actual size
        // Note: The old size will be 1 instruction too long, so we need to recalculate it here
        let size = instructions.iter().fold(0, |c, i| c + i.len());

        // Add a jmp to the previous location
        instructions.push(Instruction::with_branch(
            Code::Jmp_rel32_64,
            // Jump to the end of the patched block
            (location as usize + size) as u64,
        )?);

        // Allocate the place we'll be putting the old code
        // Note: the original code may have some fixed up relative instructions, so we need to allocate a size larger than what we're moving in case the final code is larger
        // doubling the size + max instruction length was chosen arbitrarilly (size * 2 isn't big enough for very small patches since we add an extra jmp)
        let mut original = allocate_executable(location as _, size * 2 + A::max_instr_len())?;

        // Create a block for the new location
        let block = InstructionBlock::new(&instructions, original.as_ptr() as _);

        // This is where the magic happens. [`BlockEncoder`] re-encodes the instructions for the new location and fixes up all the relative instructions
        // BlockEncoder requires a buffer be allocated *close* to where the original data came from, and our [`allocate_executable`] function handles that.
        let encoded = BlockEncoder::encode(A::bitness(), block, BlockEncoderOptions::NONE)?;
        let bytes = encoded.code_buffer;

        // Sanity check in case our allocation is too small
        if bytes.len() > original.len() {
            // This is a bug. Check [CodeError::BufferTooSmall] for what info to include in your issue
            return Err(CodeError::BufferTooSmall(
                original.len(),
                bytes.len(),
                original.as_ptr() as _,
            ));
        }

        // Finally, copy the fixed up buffer to its destination
        ptr::copy(bytes.as_ptr(), original.as_mut_ptr(), bytes.len());

        // Re-generate the patch, filling the rest of the space with NOPs
        let patch = patch
            .iter()
            .copied()
            .chain(iter::repeat(b'\x90')) // Fill extra space with nops
            .take(size)
            .collect();

        Ok(Self {
            patcher,
            original,
            patch,
            location,
            _arch: Default::default(),
        })
    }
    /// Returns a pointer to the original function.
    ///
    /// This pointer is directly callable regardless of patch status and will act as if you're calling the original unpatched function
    pub fn original(&self) -> *const u8 {
        self.original.as_ptr()
    }
    /// Patches the original location, returning a guard for the patch
    pub fn patch(
        &self,
    ) -> Result<
        <PermissionWrapper<P> as Patcher>::Guard<'_>,
        <PermissionWrapper<P> as Patcher>::Error,
    > {
        // Safety: `self.location` is validated in [`CodePatcher::new`]
        unsafe { self.patcher.patch(to_mut(self.location), &self.patch) }
    }
}

/// Helper functions for an architecture
pub trait Architecture {
    /// Gets the maximum instruction length for this architecture
    fn max_instr_len() -> usize;
    /// Gets the bitness of this architecture
    fn bitness() -> u32;
}

/// x86_64 architecture
pub struct X86_64;
impl Architecture for X86_64 {
    fn max_instr_len() -> usize {
        16
    }
    fn bitness() -> u32 {
        64
    }
}

/// Patcher for patching x86_64 code
pub type X64Patcher = CodePatcher<BytePatcher, X86_64>;

// TODO: figure out how to test this
