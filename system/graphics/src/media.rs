//! Desktop integration for media surfaces and media-window lifecycle.

use globus_media::{MediaSurface, MediaWindow, MediaWindowMode, SurfaceKind};

use crate::{compositor::Rect, desktop::{DesktopError, DesktopSession}, SurfaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaWindowBinding { pub surface: SurfaceId, pub media_surface_id: u64, pub media_window_id: u64 }

#[derive(Debug)]
pub struct MediaDesktop {
    bindings: Vec<(MediaWindowBinding, MediaSurface, MediaWindow)>,
    next_media_id: u64,
}

impl Default for MediaDesktop { fn default() -> Self { Self::new() } }

impl MediaDesktop {
    pub fn new() -> Self { Self { bindings: Vec::new(), next_media_id: 1 } }

    pub fn open_video(&mut self, desktop: &mut DesktopSession, title: impl Into<String>, bounds: Rect) -> Result<SurfaceId, DesktopError> {
        self.open(desktop, title, bounds, SurfaceKind::Video)
    }

    pub fn open_photo(&mut self, desktop: &mut DesktopSession, title: impl Into<String>, bounds: Rect) -> Result<SurfaceId, DesktopError> {
        self.open(desktop, title, bounds, SurfaceKind::Photo)
    }

    fn open(&mut self, desktop: &mut DesktopSession, title: impl Into<String>, bounds: Rect, kind: SurfaceKind) -> Result<SurfaceId, DesktopError> {
        let surface = desktop.open_window(title, bounds)?;
        let media_id = self.next_media_id;
        self.next_media_id = self.next_media_id.saturating_add(1);
        let mut media_surface = MediaSurface::new(media_id, kind, bounds.width as u32, bounds.height as u32);
        media_surface.mark_ready();
        let mut window = MediaWindow::new(media_id, media_id);
        window.attach();
        self.bindings.push((MediaWindowBinding { surface, media_surface_id: media_id, media_window_id: media_id }, media_surface, window));
        Ok(surface)
    }

    pub fn input(&mut self, surface: SurfaceId, input: globus_media::MediaInput) -> bool {
        if let Some((_, _, window)) = self.bindings.iter_mut().find(|(binding, _, _)| binding.surface == surface) {
            window.apply(input);
            true
        } else { false }
    }

    pub fn set_mode(&mut self, surface: SurfaceId, mode: MediaWindowMode) -> bool {
        if let Some((_, media_surface, window)) = self.bindings.iter_mut().find(|(binding, _, _)| binding.surface == surface) {
            window.mode = mode;
            media_surface.set_fullscreen(mode == MediaWindowMode::Fullscreen);
            media_surface.set_picture_in_picture(mode == MediaWindowMode::PictureInPicture);
            true
        } else { false }
    }

    pub fn present(&mut self, surface: SurfaceId, pts_us: u64) -> bool {
        self.bindings.iter_mut().find(|(binding, _, _)| binding.surface == surface).map(|(_, media_surface, _)| media_surface.present(pts_us)).unwrap_or(false)
    }

    pub fn close(&mut self, desktop: &mut DesktopSession, surface: SurfaceId) -> Result<(), DesktopError> {
        if let Some(pos) = self.bindings.iter().position(|(binding, _, _)| binding.surface == surface) {
            let (_, _, mut window) = self.bindings.remove(pos);
            window.close();
        }
        desktop.close_window(surface)
    }

    pub fn binding(&self, surface: SurfaceId) -> Option<MediaWindowBinding> { self.bindings.iter().find(|(binding, _, _)| binding.surface == surface).map(|x| x.0) }
}
