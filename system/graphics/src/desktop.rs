//! GlobusOS desktop session orchestration.

use crate::{
    SurfaceId,
    compositor::{Compositor, Rect},
    input::{InputEvent, InputRouter, RoutedEvent},
    wm::{WindowError, WindowManager, WindowState},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopViewport {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopMode {
    Locked,
    Active,
    Suspended,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopError {
    Locked,
    Shutdown,
    Window(WindowError),
}

impl From<WindowError> for DesktopError {
    fn from(value: WindowError) -> Self {
        Self::Window(value)
    }
}

#[derive(Debug)]
pub struct DesktopSession {
    mode: DesktopMode,
    viewport: DesktopViewport,
    compositor: Compositor,
    windows: WindowManager,
    input: InputRouter,
    next_surface: u64,
}

impl DesktopSession {
    pub fn new(viewport: DesktopViewport) -> Self {
        Self {
            mode: DesktopMode::Locked,
            viewport,
            compositor: Compositor::new(),
            windows: WindowManager::new(),
            input: InputRouter::new(),
            next_surface: 1,
        }
    }
    pub fn unlock(&mut self) {
        if self.mode == DesktopMode::Locked {
            self.mode = DesktopMode::Active;
        }
    }
    pub fn lock(&mut self) {
        if self.mode != DesktopMode::Shutdown {
            self.mode = DesktopMode::Locked;
            self.input.set_keyboard_focus(None);
        }
    }
    pub fn suspend(&mut self) {
        if self.mode == DesktopMode::Active {
            self.mode = DesktopMode::Suspended;
        }
    }
    pub fn resume(&mut self) {
        if self.mode == DesktopMode::Suspended {
            self.mode = DesktopMode::Active;
        }
    }
    pub fn shutdown(&mut self) {
        self.mode = DesktopMode::Shutdown;
    }
    pub fn mode(&self) -> DesktopMode {
        self.mode
    }
    pub fn viewport(&self) -> DesktopViewport {
        self.viewport
    }

    pub fn open_window(
        &mut self,
        title: impl Into<String>,
        bounds: Rect,
    ) -> Result<SurfaceId, DesktopError> {
        if self.mode != DesktopMode::Active {
            return Err(DesktopError::Locked);
        }
        let id = SurfaceId(self.next_surface);
        self.next_surface = self
            .next_surface
            .checked_add(1)
            .ok_or(DesktopError::Shutdown)?;
        self.compositor
            .create(id, bounds)
            .map_err(|_| DesktopError::Window(WindowError::InvalidSize))?;
        self.windows.open(id, title, bounds)?;
        self.windows.focus(id)?;
        self.compositor
            .focus(id)
            .map_err(|_| DesktopError::Window(WindowError::Missing))?;
        self.input.set_pointer_focus(Some(id));
        self.input.set_keyboard_focus(Some(id));
        Ok(id)
    }

    pub fn close_window(&mut self, surface: SurfaceId) -> Result<(), DesktopError> {
        if self.mode == DesktopMode::Shutdown {
            return Err(DesktopError::Shutdown);
        }
        self.windows.close(surface)?;
        self.compositor
            .remove(surface)
            .map_err(|_| DesktopError::Window(WindowError::Missing))?;
        if self.input.keyboard_focus() == Some(surface) {
            self.input.set_keyboard_focus(None);
        }
        Ok(())
    }

    pub fn set_window_state(
        &mut self,
        surface: SurfaceId,
        state: WindowState,
    ) -> Result<(), DesktopError> {
        self.windows.set_state(surface, state)?;
        Ok(())
    }

    pub fn route_input(&mut self, event: InputEvent) -> Result<RoutedEvent, DesktopError> {
        if self.mode != DesktopMode::Active {
            return Err(DesktopError::Locked);
        }
        Ok(self.input.route(event))
    }

    pub fn hit_test(&self, x: i32, y: i32) -> Option<SurfaceId> {
        self.compositor.hit_test(x, y)
    }
    pub fn windows(&self) -> &[crate::wm::Window] {
        self.windows.windows()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locked_desktop_cannot_open_windows() {
        let mut desktop = DesktopSession::new(DesktopViewport {
            width: 1920,
            height: 1080,
        });
        assert!(matches!(
            desktop.open_window("Shell", Rect::new(0, 0, 100, 100)),
            Err(DesktopError::Locked)
        ));
    }
    #[test]
    fn active_session_routes_input_to_open_window() {
        let mut desktop = DesktopSession::new(DesktopViewport {
            width: 1920,
            height: 1080,
        });
        desktop.unlock();
        let surface = desktop
            .open_window("Shell", Rect::new(0, 0, 500, 500))
            .unwrap();
        let routed = desktop
            .route_input(InputEvent::PointerMove { x: 10, y: 10 })
            .unwrap();
        assert_eq!(routed.target, Some(surface));
    }
}
