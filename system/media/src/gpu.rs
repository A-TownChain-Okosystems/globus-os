//! Hardware-neutral GPU/media buffer boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferFormat { Rgba8, Bgra8, Nv12, Yuv420p }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaBuffer { pub id: u64, pub width: u32, pub height: u32, pub format: BufferFormat, pub stride: u32 }

pub trait MediaGpuBackend {
    fn allocate(&mut self, width: u32, height: u32, format: BufferFormat) -> Option<MediaBuffer>;
    fn release(&mut self, id: u64) -> bool;
    fn present(&mut self, buffer: MediaBuffer, timestamp_us: u64) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTiming { pub refresh_hz: u32, pub vsync_enabled: bool }

impl FrameTiming { pub const fn new(refresh_hz: u32) -> Self { Self { refresh_hz, vsync_enabled: true } } }
