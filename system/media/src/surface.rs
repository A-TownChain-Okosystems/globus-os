//! Graphics-facing media surface contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind { Video, Photo }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaSurface { pub id: u64, pub kind: SurfaceKind, pub width: u32, pub height: u32, pub visible: bool, pub fullscreen: bool, pub picture_in_picture: bool }

impl MediaSurface {
    pub fn new(id: u64, kind: SurfaceKind, width: u32, height: u32) -> Self { Self { id, kind, width, height, visible: true, fullscreen: false, picture_in_picture: false } }
    pub fn resize(&mut self, width: u32, height: u32) { self.width = width; self.height = height; }
    pub fn set_fullscreen(&mut self, value: bool) { self.fullscreen = value; }
    pub fn set_picture_in_picture(&mut self, value: bool) { self.picture_in_picture = value; }
    pub fn set_visible(&mut self, value: bool) { self.visible = value; }
}
