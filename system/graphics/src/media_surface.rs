//! Desktop/compositor bridge for media surfaces.

use crate::{compositor::Rect, SurfaceId};
use globus_media::{MediaKind, MediaState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaSurface { pub surface: SurfaceId, pub kind: MediaKind, pub bounds: Rect, pub state: MediaState, pub fullscreen: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaSurfaceError { InvalidBounds }

impl MediaSurface {
    pub fn new(surface: SurfaceId, kind: MediaKind, bounds: Rect) -> Result<Self, MediaSurfaceError> {
        if bounds.width == 0 || bounds.height == 0 { return Err(MediaSurfaceError::InvalidBounds); }
        Ok(Self { surface, kind, bounds, state: MediaState::Stopped, fullscreen: false })
    }
    pub fn set_state(&mut self, state: MediaState) { self.state = state; }
    pub fn set_fullscreen(&mut self, fullscreen: bool) { self.fullscreen = fullscreen; }
}

#[derive(Debug, Default)]
pub struct MediaSurfaceRegistry { surfaces: Vec<MediaSurface> }

impl MediaSurfaceRegistry {
    pub fn register(&mut self, surface: MediaSurface) { self.surfaces.retain(|x| x.surface != surface.surface); self.surfaces.push(surface); }
    pub fn remove(&mut self, surface: SurfaceId) { self.surfaces.retain(|x| x.surface != surface); }
    pub fn get(&self, surface: SurfaceId) -> Option<&MediaSurface> { self.surfaces.iter().find(|x| x.surface == surface) }
    pub fn get_mut(&mut self, surface: SurfaceId) -> Option<&mut MediaSurface> { self.surfaces.iter_mut().find(|x| x.surface == surface) }
    pub fn surfaces(&self) -> &[MediaSurface] { &self.surfaces }
}

#[cfg(test)]
mod tests { use super::*;
    #[test] fn media_surface_registry_is_deterministic() { let mut r=MediaSurfaceRegistry::default(); let s=MediaSurface::new(SurfaceId(1),MediaKind::Video,Rect::new(0,0,100,100)).unwrap(); r.register(s); assert_eq!(r.get(SurfaceId(1)).unwrap().kind,MediaKind::Video); }
}
