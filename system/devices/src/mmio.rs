//! Register-level MMIO primitives. The caller must provide a kernel-mapped MMIO window.

use core::{marker::PhantomData, ptr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioWindow {
    base: usize,
    len: usize,
}

impl MmioWindow {
    /// Creates a window over an already-mapped MMIO virtual address range.
    ///
    /// # Safety
    /// `base..base+len` must be a valid, uncached/device-memory mapping for the
    /// target device and remain mapped for the lifetime of this value.
    pub const unsafe fn new(base: usize, len: usize) -> Self {
        Self { base, len }
    }

    fn addr(&self, offset: usize, width: usize) -> *mut u8 {
        assert!(offset.checked_add(width).is_some_and(|end| end <= self.len));
        (self.base + offset) as *mut u8
    }

    pub fn read16(&self, offset: usize) -> u16 {
        unsafe { ptr::read_volatile(self.addr(offset, 2).cast()) }
    }

    pub fn write16(&self, offset: usize, value: u16) {
        unsafe { ptr::write_volatile(self.addr(offset, 2).cast(), value) }
    }

    pub fn read32(&self, offset: usize) -> u32 {
        unsafe { ptr::read_volatile(self.addr(offset, 4).cast()) }
    }

    pub fn write32(&self, offset: usize, value: u32) {
        unsafe { ptr::write_volatile(self.addr(offset, 4).cast(), value) }
    }

    pub fn read64(&self, offset: usize) -> u64 {
        unsafe { ptr::read_volatile(self.addr(offset, 8).cast()) }
    }

    pub fn write64(&self, offset: usize, value: u64) {
        unsafe { ptr::write_volatile(self.addr(offset, 8).cast(), value) }
    }

    pub fn fence(&self) {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MmioRegister<T> {
    pub offset: usize,
    _marker: PhantomData<T>,
}

impl<T> MmioRegister<T> {
    pub const fn new(offset: usize) -> Self {
        Self {
            offset,
            _marker: PhantomData,
        }
    }
}
