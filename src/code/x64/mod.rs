use std::mem;

#[repr(packed)]
#[allow(dead_code)]
/// Struct helper for generating an absolute jump
struct JmpAbs {
    /// Absolute jmp instruction (jmp [rip + 6])
    jmp: [u8; 6],
    /// Absolute address to jump to
    target: usize,
}

/// Generates an absolute jump to a specified address and returns bytecode
pub fn jmp_abs(target: usize) -> [u8; mem::size_of::<JmpAbs>()] {
    unsafe {
        mem::transmute(JmpAbs {
            jmp: [0xff, 0x25, 0x00, 0x00, 0x00, 0x00],
            target,
        })
    }
}
