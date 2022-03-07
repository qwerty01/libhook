//! This module contains a code patcher which can disassemble the target location and

use std::marker::PhantomData;
use std::slice;

use iced_x86::{
    BlockEncoderOptions, BlockEncoderResult, Decoder, DecoderOptions, FlowControl, IcedError,
    Instruction, InstructionBlock,
};
use thiserror::Error;

use crate::ExecutableBuffer;

use super::mem::{PermissionError, PermissionWrapper};
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
    #[allow(unused)]
    patcher: PermissionWrapper<P>,
    /// Original data that was patched. Created such that `original` contains safely moved code that can be executed as if you were executing the original code.
    original: ExecutableBuffer,
    /// Placeholder for architecture
    _arch: PhantomData<A>,
}
impl<P: Patcher, A: Architecture> CodePatcher<P, A> {
    /// Returns a pointer to the original function.
    ///
    /// This pointer is directly callable and will act as if you're calling the original unpatched function
    pub fn original(&self) -> *const u8 {
        self.original.as_ptr()
    }
}

/// Helper functions for an architecture
pub trait Architecture {
    /// Error type for the architecture
    type Error;

    /// Creates an instruction decoder from the given slice
    fn decoder(data: &[u8], options: u32) -> Decoder;
    /// Creates an encoder for the given instruction block
    fn encode(block: InstructionBlock, options: u32) -> Result<BlockEncoderResult, Self::Error>;
    /// Gets the maximum instruction length for this architecture
    fn max_instr_len() -> usize;
    /// Trims the provided size to the nearest instruction
    ///
    /// # Safety
    ///
    /// - `data` must be [https://doc.rust-lang.org/stable/std/ptr/index.html#safety](valid) for `size` + `max_instr_len() - 1` bytes
    unsafe fn trim_size(data: *const u8, size: usize) -> (Vec<Instruction>, usize) {
        // Add max_instr_len() - 1 in case we have the first byte of the longest instruction
        let max_size = size + Self::max_instr_len() - 1;
        let buf = slice::from_raw_parts(data, max_size);
        let mut new_size = 0;
        let decoder = Self::decoder(buf, DecoderOptions::NONE);
        let inst: Vec<_> = decoder
            .into_iter()
            .take_while(|v| {
                if new_size + v.len() >= size {
                    false
                } else {
                    new_size += v.len();
                    true
                }
            })
            .collect();
        (inst, new_size)
    }
    /// Expands the provded size to a full instruction
    ///
    /// # Safety
    ///
    /// - `data` must be [https://doc.rust-lang.org/stable/std/ptr/index.html#safety](valid) for `size` + `max_instr_len() - 1` bytes
    unsafe fn expand_size(data: *const u8, size: usize) -> (Vec<Instruction>, usize) {
        // Add max_instr_len() - 1 in case we have the first byte of the longest instruction
        let max_size = size + Self::max_instr_len() - 1;
        let buf = slice::from_raw_parts(data, max_size);
        let mut new_size = 0;
        let decoder = Self::decoder(buf, DecoderOptions::NONE);
        let inst: Vec<_> = decoder
            .into_iter()
            .take_while(|v| {
                new_size += v.len();
                new_size + v.len() <= size
            })
            .collect();
        (inst, new_size)
    }
    /// Copies a block of code from `src`, expanding to the nearest instruction
    ///
    /// # Safety
    ///
    /// - `src` must be [https://doc.rust-lang.org/stable/std/ptr/index.html#safety](valid) for `size` + `max_instr_len() - 1` bytes
    unsafe fn copy_instr(src: *const u8, size: usize) -> Result<ExecutableBuffer, Self::Error>
    where
        Self::Error: From<region::Error>,
    {
        // Get the new length and disassembled instructions
        let (mut instr, mut new_size) = Self::expand_size(src, size);

        if let Some(i) = instr.last() {
            if i.flow_control() != FlowControl::Return {
                // TODO: How do we do a non-rip-relative branch?
                //instr.push(Instruction::with_far_branch(code, selector, offset));
                new_size += instr.last().unwrap().len();
            }
        }

        // Create the executable buffer from the instruction buffer
        // /!\ /!\ MAJOR HACK /!\ /!\
        // There is no way to know what size we'll need before we encode the new instructions, but we need the size to make the buffer it'll be moved to
        // It probably won't need more than twice the total size, so we'll just double the size and add 10 in case it's small
        // TODO: is there *any* way to do this? The instruction sizes could change depending on the value of RIP, so we can't just make up a spot and then fix it later
        let buffer = ExecutableBuffer::new_uninit(new_size * 2 + 10)?;

        // Create an instruction block for the destination
        let block = InstructionBlock::new(&instr, buffer.data as _);

        // Encode the instructions in the new location
        let block = Self::encode(block, BlockEncoderOptions::NONE)?;

        Ok(buffer)
    }
}

impl<P, A> Patcher for CodePatcher<P, A>
where
    P: Patcher,
    A: Architecture,
    PermissionError<P::Error>: From<P::Error>,
{
    type Error = CodeError<P::Error>;

    unsafe fn patch(location: *mut u8, patch: &[u8]) -> Result<Self, Self::Error> {
        // TODO: use `BlockEncoder` to generate the actual patch
        // Length of patch + max instruction size
        let patch_size = patch.len();
        let max_size = patch_size + 14;
        let data = slice::from_raw_parts(location, max_size);
        let decoder = Decoder::with_ip(32, data, location as u64, DecoderOptions::NONE);
        let mut size = 0usize;
        let _instructions: Vec<_> = decoder
            .into_iter()
            .take_while(|v| {
                size += v.len();
                size < patch_size
            })
            .collect();
        let size = size;
        let patcher = PermissionWrapper::patch(location, &patch[..size])?;
        let original = ExecutableBuffer::new_uninit(0)?;
        Ok(Self {
            patcher,
            original,
            _arch: Default::default(),
        })
    }

    unsafe fn restore(self) {
        // Implemented in `drop`
    }
}
