// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// Memory mapping for kernel image
/// Placement of kernel image, stack and heap in physical memory.
pub struct MemoryMap {
    /// First byte of the kernel image.
    pub kernel_start: u64,
    /// Size of the kernel image in bytes.
    pub kernel_size: u64,
    /// Start of the boot stack.
    pub stack_start: u64,
    /// Start of the kernel heap.
    pub heap_start: u64,
}

impl Default for MemoryMap {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryMap {
    /// Default layout used by the kernel build.
    pub fn new() -> Self {
        Self {
            kernel_start: 0x100000,
            kernel_size: 0x200000,
            stack_start: 0x80000,
            heap_start: 0x300000,
        }
    }
    /// Returns true when `addr` falls inside the kernel image.
    pub fn is_in_kernel(&self, addr: u64) -> bool {
        addr >= self.kernel_start && addr < self.kernel_start + self.kernel_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_mmap() {
        let m = MemoryMap::new();
        assert!(m.is_in_kernel(0x100000));
        assert!(!m.is_in_kernel(0x300000));
    }
}
