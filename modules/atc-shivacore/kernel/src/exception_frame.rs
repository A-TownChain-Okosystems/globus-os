// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Fixed x86-64 exception frame shared by the kernel trampoline and host-side tests.

use core::mem::{MaybeUninit, size_of};

use crate::user_sched::SavedContext;

/// CPU frame present for every normalized x86-64 exception.
///
/// The frame contains 15 saved general-purpose registers, an error-code slot,
/// and the CPU return frame through RFLAGS. This is exactly 152 bytes. The
/// privilege-dependent RSP/SS extension is deliberately a separate type:
/// CPL0 exceptions do not push those words, so a reference to this base frame
/// must not extend into memory above the interrupted kernel RSP.
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
}

/// Additional CPU frame words pushed only on a CPL3-to-CPL0 transition.
#[repr(C)]
pub struct ExceptionUserStackFrame {
    /// User stack pointer; initialized by the CPU only for CPL3-origin exceptions.
    pub rsp: MaybeUninit<u64>,
    /// User stack segment; initialized by the CPU only for CPL3-origin exceptions.
    pub ss: MaybeUninit<u64>,
}

impl ExceptionTrapFrame {
    /// Returns the privilege-transition extension following this base frame.
    ///
    /// # Safety
    ///
    /// The interrupted context must have originated in CPL3. The CPU only
    /// pushes the RSP/SS words in that case; for CPL0 they are not part of the
    /// hardware frame and the memory may not be mapped or initialized.
    pub unsafe fn user_stack_frame(&self) -> &ExceptionUserStackFrame {
        let extension = unsafe {
            (self as *const Self)
                .cast::<u8>()
                .add(size_of::<Self>())
                .cast::<ExceptionUserStackFrame>()
        };
        // SAFETY: guaranteed by the caller's CPL3-origin precondition.
        unsafe { &*extension }
    }

    /// Returns the mutable privilege-transition extension following this frame.
    ///
    /// # Safety
    ///
    /// The interrupted context must have originated in CPL3. The CPU only
    /// pushes the RSP/SS words in that case.
    pub unsafe fn user_stack_frame_mut(&mut self) -> &mut ExceptionUserStackFrame {
        let extension = unsafe {
            (self as *mut Self)
                .cast::<u8>()
                .add(size_of::<Self>())
                .cast::<ExceptionUserStackFrame>()
        };
        // SAFETY: guaranteed by the caller's CPL3-origin precondition.
        unsafe { &mut *extension }
    }

    /// Replaces the faulting Ring-3 context with the next saved context.
    ///
    /// # Safety
    ///
    /// The frame must originate from CPL3 so the CPU-pushed RSP/SS extension
    /// exists immediately after the base exception frame.
    pub unsafe fn apply_saved_context(&mut self, ctx: &SavedContext) {
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
        // SAFETY: this method's caller guarantees a CPL3-origin frame.
        let user_stack = unsafe { self.user_stack_frame_mut() };
        user_stack.rsp.write(ctx.iret.rsp);
        user_stack.ss.write(u64::from(ctx.iret.ss));
    }
}

#[cfg(test)]
mod tests {
    use super::{ExceptionTrapFrame, ExceptionUserStackFrame};
    use crate::user_sched::{IretFrame, SavedContext, SavedRegisters};
    use core::mem::MaybeUninit;

    #[repr(C)]
    struct CompleteUserExceptionFrame {
        base: ExceptionTrapFrame,
        user_stack: ExceptionUserStackFrame,
    }

    #[test]
    fn exception_frame_matches_hardware_privilege_layout() {
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, error_code), 120);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, rip), 128);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, cs), 136);
        assert_eq!(core::mem::offset_of!(ExceptionTrapFrame, rflags), 144);
        assert_eq!(core::mem::size_of::<ExceptionTrapFrame>(), 152);
        assert_eq!(
            core::mem::offset_of!(CompleteUserExceptionFrame, user_stack),
            152
        );
        assert_eq!(core::mem::offset_of!(ExceptionUserStackFrame, rsp), 0);
        assert_eq!(core::mem::offset_of!(ExceptionUserStackFrame, ss), 8);
        assert_eq!(core::mem::size_of::<ExceptionUserStackFrame>(), 16);
        assert_eq!(core::mem::size_of::<CompleteUserExceptionFrame>(), 168);
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
        let mut frame = CompleteUserExceptionFrame {
            base: ExceptionTrapFrame {
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
            },
            user_stack: ExceptionUserStackFrame {
                rsp: MaybeUninit::uninit(),
                ss: MaybeUninit::uninit(),
            },
        };
        // SAFETY: the wrapper provides the CPU-pushed CPL3 extension immediately
        // after the base frame, exactly as the hardware does on a privilege change.
        unsafe { frame.base.apply_saved_context(&saved) };
        assert_eq!(
            (
                frame.base.rax,
                frame.base.rbx,
                frame.base.rcx,
                frame.base.rdx
            ),
            (1, 2, 3, 4)
        );
        assert_eq!((frame.base.rsi, frame.base.rdi, frame.base.rbp), (5, 6, 7));
        assert_eq!(
            (frame.base.r8, frame.base.r9, frame.base.r10, frame.base.r11),
            (8, 9, 10, 11)
        );
        assert_eq!(
            (
                frame.base.r12,
                frame.base.r13,
                frame.base.r14,
                frame.base.r15
            ),
            (12, 13, 14, 15)
        );
        assert_eq!(
            (
                frame.base.rip,
                frame.base.cs,
                frame.base.rflags,
                unsafe { frame.user_stack.rsp.assume_init() },
                unsafe { frame.user_stack.ss.assume_init() }
            ),
            (0x400000, 0x1B, 0x202, 0x8000, 0x23)
        );
        assert_eq!(
            frame.base.error_code, 0x5,
            "error code remains diagnostic data"
        );
    }
}
