// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Fixed x86-64 exception frame shared by the kernel trampoline and host-side tests.

use core::mem::MaybeUninit;

use crate::user_sched::SavedContext;

/// CPU frame for exceptions with one normalized error-code slot.
///
/// The frame contains 15 saved general-purpose registers, an error-code slot,
/// and the CPU return frame. For CPL0-origin exceptions the CPU does not push
/// RSP/SS; those fields therefore remain uninitialized and must only be read
/// after verifying a CPL3 origin.
#[repr(C)]
pub struct ExceptionTrapFrame {
    /// Accumulator.
    pub rax: u64,
    /// Base register.
    pub rbx: u64,
    /// Counter register.
    pub rcx: u64,
    /// Data register.
    pub rdx: u64,
    /// Source index.
    pub rsi: u64,
    /// Destination index.
    pub rdi: u64,
    /// Frame pointer.
    pub rbp: u64,
    /// Extended register 8.
    pub r8: u64,
    /// Extended register 9.
    pub r9: u64,
    /// Extended register 10.
    pub r10: u64,
    /// Extended register 11.
    pub r11: u64,
    /// Extended register 12.
    pub r12: u64,
    /// Extended register 13.
    pub r13: u64,
    /// Extended register 14.
    pub r14: u64,
    /// Extended register 15.
    pub r15: u64,
    /// CPU-supplied or synthetic (#UD) error code.
    pub error_code: u64,
    /// Instruction pointer at exception time.
    pub rip: u64,
    /// Code segment, including privilege level.
    pub cs: u64,
    /// Processor flags.
    pub rflags: u64,
    /// User stack pointer; initialized by the CPU only for CPL3-origin exceptions.
    pub rsp: MaybeUninit<u64>,
    /// User stack segment; initialized by the CPU only for CPL3-origin exceptions.
    pub ss: MaybeUninit<u64>,
}

impl ExceptionTrapFrame {
    /// Replaces the faulting context with the next Ring-3 context.
    pub fn apply_saved_context(&mut self, ctx: &SavedContext) {
        self.rax = ctx.regs.rax;
        self.rbx = ctx.regs.rbx;
        self.rcx = ctx.regs.rcx;
        self.rdx = ctx.regs.rdx;
        self.rsi = ctx.regs.rsi;
        self.rdi = ctx.regs.rdi;
        self.rbp = ctx.regs.rbp;
        self.r8 = ctx.regs.r8;
        self.r9 = ctx.regs.r9;
        self.r10 = ctx.regs.r10;
        self.r11 = ctx.regs.r11;
        self.r12 = ctx.regs.r12;
        self.r13 = ctx.regs.r13;
        self.r14 = ctx.regs.r14;
        self.r15 = ctx.regs.r15;
        self.rip = ctx.iret.rip;
        self.cs = u64::from(ctx.iret.cs);
        self.rflags = ctx.iret.rflags;
        self.rsp.write(ctx.iret.rsp);
        self.ss.write(u64::from(ctx.iret.ss));
    }
}

#[cfg(test)]
mod tests {
    use super::ExceptionTrapFrame;
    use crate::user_sched::{IretFrame, SavedContext, SavedRegisters};

    #[test]
    fn exception_frame_matches_hardware_error_code_layout() {
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, error_code), 120);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, rip), 128);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, cs), 136);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, rflags), 144);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, rsp), 152);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, ss), 160);
        assert_eq!(core::mem::size_of::<ExceptionTrapFrame>(), 168);
    }

    #[test]
    fn exception_frame_restores_every_general_register_and_iret_field() {
        let saved = SavedContext {
            regs: SavedRegisters {
                rax: 1,
                rbx: 2,
                rcx: 3,
                rdx: 4,
                rsi: 5,
                rdi: 6,
                rbp: 7,
                rsp: 0x8000,
                r8: 8,
                r9: 9,
                r10: 10,
                r11: 11,
                r12: 12,
                r13: 13,
                r14: 14,
                r15: 15,
            },
            iret: IretFrame {
                rip: 0x400000,
                cs: 0x1B,
                rflags: 0x202,
                rsp: 0x8000,
                ss: 0x23,
            },
        };
        let mut frame = ExceptionTrapFrame {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            error_code: 0x5,
            rip: 0,
            cs: 0,
            rflags: 0,
            rsp: core::mem::MaybeUninit::uninit(),
            ss: core::mem::MaybeUninit::uninit(),
        };
        frame.apply_saved_context(&saved);
        assert_eq!((frame.rax, frame.rbx, frame.rcx, frame.rdx), (1, 2, 3, 4));
        assert_eq!((frame.rsi, frame.rdi, frame.rbp), (5, 6, 7));
        assert_eq!((frame.r8, frame.r9, frame.r10, frame.r11), (8, 9, 10, 11));
        assert_eq!(
            (frame.r12, frame.r13, frame.r14, frame.r15),
            (12, 13, 14, 15)
        );
        assert_eq!(
            (
                frame.rip,
                frame.cs,
                frame.rflags,
                unsafe { frame.rsp.assume_init() },
                unsafe { frame.ss.assume_init() }
            ),
            (0x400000, 0x1B, 0x202, 0x8000, 0x23)
        );
        assert_eq!(frame.error_code, 0x5, "error code remains diagnostic data");
    }
}
