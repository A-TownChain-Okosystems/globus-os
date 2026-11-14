// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// Boot info structure and handoff
/// Boot information passed from the bootloader to the kernel.
pub struct BootInfo {
    /// Where the memory map sits in low memory.
    pub memory_map_addr: u64,
    /// Kernel command line.
    pub cmdline: String,
    /// Load address of the initrd, when one was loaded.
    pub initrd_addr: Option<u64>,
}

impl Default for BootInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl BootInfo {
    /// Default layout: memory map at `0x8000`, empty command line, no initrd.
    pub fn new() -> Self {
        Self {
            memory_map_addr: 0x8000,
            cmdline: String::new(),
            initrd_addr: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_bootinfo() {
        let bi = BootInfo::new();
        assert_eq!(bi.memory_map_addr, 0x8000);
        assert!(bi.initrd_addr.is_none());
    }
}
