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
    NotResizable,
}

#[derive(Debug, Default)]
pub struct WindowManager {
    windows: Vec<Window>,
    restore_bounds: Vec<(SurfaceId, Rect)>,
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
        self.restore_bounds.retain(|(id, _)| *id != surface);
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

    /// Move a live window without changing its size or state.
    pub fn move_window(&mut self, surface: SurfaceId, x: i32, y: i32) -> Result<(), WindowError> {
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.surface == surface)
            .ok_or(WindowError::Missing)?;
        if window.state == WindowState::Closed {
            return Err(WindowError::Closed);
        }
        if window.state == WindowState::Maximized || window.state == WindowState::Fullscreen {
            return Err(WindowError::InvalidSize);
        }
        window.bounds.x = x;
        window.bounds.y = y;
        Ok(())
    }

    /// Resize a live, resizable window. Zero-sized geometry is rejected.
    pub fn resize_window(
        &mut self,
        surface: SurfaceId,
        width: u32,
        height: u32,
    ) -> Result<(), WindowError> {
        if width == 0 || height == 0 {
            return Err(WindowError::InvalidSize);
        }
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.surface == surface)
            .ok_or(WindowError::Missing)?;
        if window.state == WindowState::Closed {
            return Err(WindowError::Closed);
        }
        if !window.resizable {
            return Err(WindowError::NotResizable);
        }
        if window.state == WindowState::Maximized || window.state == WindowState::Fullscreen {
            return Err(WindowError::InvalidSize);
        }
        window.bounds.width = width;
        window.bounds.height = height;
        Ok(())
    }

    pub fn minimize(&mut self, surface: SurfaceId) -> Result<(), WindowError> {
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.surface == surface)
            .ok_or(WindowError::Missing)?;
        if window.state == WindowState::Closed {
            return Err(WindowError::Closed);
        }
        window.state = WindowState::Minimized;
        window.focused = false;
        Ok(())
    }

    /// Maximize into the supplied compositor work area while retaining the
    /// previous geometry for restore. The caller owns display/work-area policy.
    pub fn maximize(&mut self, surface: SurfaceId, work_area: Rect) -> Result<(), WindowError> {
        if work_area.width == 0 || work_area.height == 0 {
            return Err(WindowError::InvalidSize);
        }
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.surface == surface)
            .ok_or(WindowError::Missing)?;
        if window.state == WindowState::Closed {
            return Err(WindowError::Closed);
        }
        if window.state != WindowState::Maximized {
            if let Some((_, saved)) = self
                .restore_bounds
                .iter_mut()
                .find(|(id, _)| *id == surface)
            {
                *saved = window.bounds;
            } else {
                self.restore_bounds.push((surface, window.bounds));
            }
        }
        window.bounds = work_area;
        window.state = WindowState::Maximized;
        window.focused = true;
        for other in &mut self.windows {
            if other.surface != surface {
                other.focused = false;
            }
        }
        Ok(())
    }

    /// Restore the saved geometry after maximize, or return minimized/fullscreen
    /// windows to normal state when no saved geometry exists.
    pub fn restore(&mut self, surface: SurfaceId) -> Result<(), WindowError> {
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.surface == surface)
            .ok_or(WindowError::Missing)?;
        if window.state == WindowState::Closed {
            return Err(WindowError::Closed);
        }
        if let Some(index) = self
            .restore_bounds
            .iter()
            .position(|(id, _)| *id == surface)
        {
            let (_, bounds) = self.restore_bounds.remove(index);
            window.bounds = bounds;
        }
        window.state = WindowState::Normal;
        window.focused = true;
        for other in &mut self.windows {
            if other.surface != surface {
                other.focused = false;
            }
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
            self.restore_bounds.retain(|(id, _)| *id != surface);
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

    fn manager_with_window() -> WindowManager {
        let mut wm = WindowManager::new();
        wm.open(SurfaceId(1), "one", Rect::new(10, 20, 300, 200))
            .unwrap();
        wm
    }

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

    #[test]
    fn move_and_resize_update_geometry() {
        let mut wm = manager_with_window();
        wm.move_window(SurfaceId(1), -5, 12).unwrap();
        wm.resize_window(SurfaceId(1), 640, 480).unwrap();
        let bounds = wm.windows()[0].bounds;
        assert_eq!(bounds, Rect::new(-5, 12, 640, 480));
    }

    #[test]
    fn resize_rejects_zero_dimensions_without_mutating_geometry() {
        let mut wm = manager_with_window();
        assert_eq!(
            wm.resize_window(SurfaceId(1), 0, 480),
            Err(WindowError::InvalidSize)
        );
        assert_eq!(wm.windows()[0].bounds, Rect::new(10, 20, 300, 200));
    }

    #[test]
    fn maximize_and_restore_round_trip_geometry() {
        let mut wm = manager_with_window();
        wm.maximize(SurfaceId(1), Rect::new(0, 24, 1920, 1056))
            .unwrap();
        assert_eq!(wm.windows()[0].state, WindowState::Maximized);
        assert_eq!(wm.windows()[0].bounds, Rect::new(0, 24, 1920, 1056));
        wm.restore(SurfaceId(1)).unwrap();
        assert_eq!(wm.windows()[0].state, WindowState::Normal);
        assert_eq!(wm.windows()[0].bounds, Rect::new(10, 20, 300, 200));
    }

    #[test]
    fn minimize_removes_window_from_visible_placements() {
        let mut wm = manager_with_window();
        wm.minimize(SurfaceId(1)).unwrap();
        assert_eq!(wm.windows()[0].state, WindowState::Minimized);
        assert_eq!(wm.placements().count(), 0);
    }

    #[test]
    fn closed_windows_cannot_be_moved_or_restored() {
        let mut wm = manager_with_window();
        wm.close(SurfaceId(1)).unwrap();
        assert_eq!(wm.move_window(SurfaceId(1), 1, 2), Err(WindowError::Closed));
        assert_eq!(wm.restore(SurfaceId(1)), Err(WindowError::Closed));
    }
}
