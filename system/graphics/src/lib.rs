//! Display, compositor, desktop shell and GPU policy boundary.

pub mod compositor;
/// Desktop-Flaeche und ihre Ebenen.
pub mod desktop;
/// Framebuffer und Anzeigemodus.
pub mod display;
/// GPU-Grenze und Zeichenbefehle.
pub mod gpu;
/// Eingabeereignisse und ihre Zustellung.
pub mod input;
/// Registrierung von Video- und Medienflaechen.
pub mod media_surface;
/// Shell-Zustand und Befehle.
pub mod shell;
/// Fensterverwaltung mit Fokus und Platzierung.
pub mod wm;

pub use media_surface::{MediaSurface, MediaSurfaceError, MediaSurfaceRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Weg, auf dem gezeichnet wird.
pub enum GraphicsBackend {
    /// Direkt in den Bildpuffer.
    Framebuffer,
    /// Ueber Vulkan.
    Vulkan,
    /// Ueber WebGPU.
    WebGpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kennung einer Zeichen- oder Fensterflaeche.
pub struct SurfaceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Aufloesung und Bildwiederholrate.
pub struct DisplayMode {
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// Bildwiederholrate in Hertz.
    pub refresh_hz: u32,
}

impl DisplayMode {
    /// Nimmt nur Werte groesser 0 an, sonst None.
    pub const fn new(width: u32, height: u32, refresh_hz: u32) -> Option<Self> {
        if width == 0 || height == 0 || refresh_hz == 0 {
            None
        } else {
            Some(Self {
                width,
                height,
                refresh_hz,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_display_mode() {
        assert!(DisplayMode::new(0, 1080, 60).is_none());
    }
}
