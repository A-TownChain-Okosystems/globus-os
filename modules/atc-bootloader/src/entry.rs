// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// 32-bit/64-bit entry point
/// Address the bootloader hands control to: `0x100000`.
pub fn entry_point() -> u64 {
    0x100000
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_entry() {
        assert_eq!(entry_point(), 0x100000);
    }
}
