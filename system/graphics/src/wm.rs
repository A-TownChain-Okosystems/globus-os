//! Capability-neutral window manager state machine.

use crate::SurfaceId;
use crate::compositor::{Rect, SurfacePlacement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
    Fullscreen,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub surface: SurfaceId,
    pub title: String,
    pub bounds: Rect,
    pub state: WindowState,
    pub focused: bool,
    pub resizable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowError {
    Exists,
    Missing,
    Closed,
    InvalidSize,
}

#[derive(Debug, Default)]
pub struct WindowManager {
    windows: Vec<Window>,
}

impl WindowManager {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn open(
        &mut self,
        surface: SurfaceId,
        title: impl Into<String>,
        bounds: Rect,
    ) -> Result<(), WindowError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(WindowError::InvalidSize);
        }
        if self
            .windows
            .iter()
            .any(|w| w.surface == surface && w.state != WindowState::Closed)
        {
            return Err(WindowError::Exists);
        }
        self.windows.push(Window {
            surface,
            title: title.into(),
            bounds,
            state: WindowState::Normal,
            focused: false,
            resizable: true,
        });
        self.focus(surface)?;
        Ok(())
    }
    pub fn focus(&mut self, surface: SurfaceId) -> Result<(), WindowError> {
        let exists = self
            .windows
            .iter()
            .any(|w| w.surface == surface && w.state != WindowState::Closed);
        if !exists {
            return Err(WindowError::Missing);
        }
        for window in &mut self.windows {
            window.focused = window.surface == surface;
        }
        Ok(())
    }
    pub fn set_state(&mut self, surface: SurfaceId, state: WindowState) -> Result<(), WindowError> {
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.surface == surface)
            .ok_or(WindowError::Missing)?;
        if window.state == WindowState::Closed {
            return Err(WindowError::Closed);
        }
        window.state = state;
        if state == WindowState::Closed {
            window.focused = false;
        }
        Ok(())
    }
    pub fn close(&mut self, surface: SurfaceId) -> Result<(), WindowError> {
        self.set_state(surface, WindowState::Closed)
    }
    pub fn focused(&self) -> Option<&Window> {
        self.windows.iter().find(|w| w.focused)
    }
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    pub fn placements(&self) -> impl Iterator<Item = SurfacePlacement> + '_ {
        self.windows.iter().enumerate().filter_map(|(i, w)| {
            if w.state == WindowState::Closed || w.state == WindowState::Minimized {
                return None;
            }
            Some(SurfacePlacement {
                surface: w.surface,
                bounds: w.bounds,
                z: i as u32 + 1,
                visible: true,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_is_exclusive() {
        let mut wm = WindowManager::new();
        wm.open(SurfaceId(1), "one", Rect::new(0, 0, 100, 100))
            .unwrap();
        wm.open(SurfaceId(2), "two", Rect::new(10, 10, 100, 100))
            .unwrap();
        assert_eq!(wm.focused().unwrap().surface, SurfaceId(2));
        wm.focus(SurfaceId(1)).unwrap();
        assert_eq!(wm.focused().unwrap().surface, SurfaceId(1));
    }
}
