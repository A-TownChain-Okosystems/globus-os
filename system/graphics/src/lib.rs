//! Display, compositor and GPU policy boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsBackend {
    Framebuffer,
    Vulkan,
    WebGpu,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceId(pub u64);
