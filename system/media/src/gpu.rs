//! Hardware-neutral GPU/media buffer boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferFormat { Rgba8, Bgra8, Nv12, Yuv420p }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaBuffer {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub format: BufferFormat,
    pub stride: u32,
    pub shared: bool,
}

pub trait MediaGpuBackend {
    fn allocate(&mut self, width: u32, height: u32, format: BufferFormat) -> Option<MediaBuffer>;
    fn release(&mut self, id: u64) -> bool;
    fn present(&mut self, buffer: MediaBuffer, timestamp_us: u64) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTiming { pub refresh_hz: u32, pub vsync_enabled: bool }

impl FrameTiming {
    pub const fn new(refresh_hz: u32) -> Self { Self { refresh_hz, vsync_enabled: true } }
    pub const fn frame_interval_us(self) -> Option<u64> {
        if self.refresh_hz == 0 { None } else { Some(1_000_000 / self.refresh_hz as u64) }
    }
    pub const fn deadline_us(self, vsync_us: u64) -> Option<u64> {
        if !self.vsync_enabled { None } else { self.frame_interval_us().map(|interval| vsync_us.saturating_add(interval)) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedBufferHandle { pub id: u64, pub read_only: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorConversion { pub source: BufferFormat, pub destination: BufferFormat }

impl ColorConversion {
    pub const fn required(source: BufferFormat, destination: BufferFormat) -> bool { source != destination }
    pub const fn is_yuv_to_rgb(self) -> bool {
        matches!(self.source, BufferFormat::Nv12 | BufferFormat::Yuv420p)
            && matches!(self.destination, BufferFormat::Rgba8 | BufferFormat::Bgra8)
    }
}
