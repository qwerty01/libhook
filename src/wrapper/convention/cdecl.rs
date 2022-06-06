//! # CDecl
//!
//! This module provides utilites for generating wrappers for the cdecl calling convention

use crate::code::x64::jmp_abs;

use super::WrapperGenerator;

/// Generator to convert from cdecl to the standardized calling convention
pub struct CDeclWrapperGenerator;
unsafe impl WrapperGenerator for CDeclWrapperGenerator {
    unsafe fn generate(target: usize) -> Vec<u8> {
        // cdecl is the standardized calling convention, so just jump straight to the target
        jmp_abs(target).into()
    }
}
