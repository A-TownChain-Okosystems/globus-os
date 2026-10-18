// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! Pure privilege-selector checks shared by trap entry and host-side tests.

/// x86-64 ring-3 code/data selectors must have RPL=3.
#[inline]
pub const fn is_user_mode_selector(selector: u64) -> bool {
    selector & 0b11 == 0b11
}

/// Validate the privilege selectors captured for a CPL3 -> CPL0 trap.
///
/// The assembly entry must check CS before constructing/reading the full
/// five-word privilege-transition frame: a CPL0-origin interrupt has no saved
/// RSP/SS pair. This helper validates both selectors once that shape is known.
#[inline]
pub const fn is_user_trap_frame(cs: u64, ss: u64) -> bool {
    is_user_mode_selector(cs) && is_user_mode_selector(ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_ring3_code_and_stack_selectors() {
        assert!(is_user_trap_frame(0x23, 0x1b));
        assert!(!is_user_trap_frame(0x08, 0x10));
        assert!(!is_user_trap_frame(0x23, 0x10));
        assert!(!is_user_trap_frame(0x08, 0x1b));
        assert!(!is_user_trap_frame(0x21, 0x19));
    }
}
