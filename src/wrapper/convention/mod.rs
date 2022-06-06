//! # Convention
//!
//! This module handles converting specific calling conventions to a standardized version
//!
//! ## Standardized calling convention
//!
//! Right now I'm not entirely sure what I want it to be, so we'll just keep it simple and use the C calling convention
//! - args: rdx, rcx, r8, r9, stack
//! - return: rax, xmm0
//! - 16-byte aligned
//! - TODO: floating point: xmm0, xmm1, xmm2, xmm3, stack
//! - volatile registers: rax, rcx, rdx, r8, r9, r10, r11
//! - nonvolatile registers: rbx, rbp, rdi, rsi, rsp, r12, r13, r14, r15

pub mod cdecl;

/// Generates a wrapper for the specified calling convention
///
/// # Safety
///
/// The implementor must ensure the generated code correctly follows both the calling convention
/// that it's wrapping and the target calling convention
pub unsafe trait WrapperGenerator {
    /// Generates the code needed to convert the given calling convention to the standardized calling convention
    ///
    /// # Safety
    ///
    /// `target` must be a pointer to code that expects the standardized calling convention
    unsafe fn generate(target: usize) -> Vec<u8>;
}
