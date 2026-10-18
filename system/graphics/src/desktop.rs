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
            self.input.set_pointer_focus(None);
        }
    }
    pub fn suspend(&mut self) {
        if self.mode == DesktopMode::Active {
            self.mode = DesktopMode::Suspended;
            self.input.set_keyboard_focus(None);
            self.input.set_pointer_focus(None);
        }
    }
    pub fn resume(&mut self) {
        if self.mode == DesktopMode::Suspended {
            self.mode = DesktopMode::Active;
        }
    }
    pub fn shutdown(&mut self) {
        self.mode = DesktopMode::Shutdown;
        self.input.set_keyboard_focus(None);
        self.input.set_pointer_focus(None);
    }
    pub fn mode(&self) -> DesktopMode {
        self.mode
    }
    pub fn viewport(&self) -> DesktopViewport {
        self.viewport
    }

    fn ensure_active(&self) -> Result<(), DesktopError> {
        match self.mode {
            DesktopMode::Active => Ok(()),
            DesktopMode::Shutdown => Err(DesktopError::Shutdown),
            DesktopMode::Locked | DesktopMode::Suspended => Err(DesktopError::Locked),
        }
    }

    fn bounds_for(&self, surface: SurfaceId) -> Result<Rect, DesktopError> {
        self.windows
            .windows()
            .iter()
            .find(|window| window.surface == surface)
            .map(|window| window.bounds)
            .ok_or(DesktopError::Window(WindowError::Missing))
    }

    fn sync_bounds(&mut self, surface: SurfaceId) -> Result<(), DesktopError> {
        let bounds = self.bounds_for(surface)?;
        self.compositor
            .set_bounds(surface, bounds)
            .map_err(|_| DesktopError::Window(WindowError::Missing))
    }

    fn focus_window_inner(&mut self, surface: SurfaceId) -> Result<(), DesktopError> {
        self.windows.focus(surface)?;
        self.compositor
            .focus(surface)
            .map_err(|_| DesktopError::Window(WindowError::Missing))?;
        self.input.set_keyboard_focus(Some(surface));
        self.input.set_pointer_focus(Some(surface));
        Ok(())
    }

    pub fn open_window(
        &mut self,
        title: impl Into<String>,
        bounds: Rect,
    ) -> Result<SurfaceId, DesktopError> {
        self.ensure_active()?;
        let id = SurfaceId(self.next_surface);
        self.next_surface = self
            .next_surface
            .checked_add(1)
            .ok_or(DesktopError::Shutdown)?;
        self.compositor
            .create(id, bounds)
            .map_err(|_| DesktopError::Window(WindowError::InvalidSize))?;
        self.windows.open(id, title, bounds)?;
        self.focus_window_inner(id)?;
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
        if self.input.pointer_focus() == Some(surface) {
            self.input.set_pointer_focus(None);
        }
        Ok(())
    }

    pub fn focus_window(&mut self, surface: SurfaceId) -> Result<(), DesktopError> {
        self.ensure_active()?;
        self.focus_window_inner(surface)
    }

    pub fn move_window(
        &mut self,
        surface: SurfaceId,
        x: i32,
        y: i32,
    ) -> Result<(), DesktopError> {
        self.ensure_active()?;
        self.windows.move_window(surface, x, y)?;
        self.sync_bounds(surface)
    }

    pub fn resize_window(
        &mut self,
        surface: SurfaceId,
        width: u32,
        height: u32,
    ) -> Result<(), DesktopError> {
        self.ensure_active()?;
        self.windows.resize_window(surface, width, height)?;
        self.sync_bounds(surface)
    }

    pub fn minimize_window(&mut self, surface: SurfaceId) -> Result<(), DesktopError> {
        self.ensure_active()?;
        self.windows.minimize(surface)?;
        self.compositor
            .set_visible(surface, false)
            .map_err(|_| DesktopError::Window(WindowError::Missing))?;
        if self.input.keyboard_focus() == Some(surface) {
            self.input.set_keyboard_focus(None);
        }
        if self.input.pointer_focus() == Some(surface) {
            self.input.set_pointer_focus(None);
        }
        Ok(())
    }

    pub fn maximize_window(&mut self, surface: SurfaceId) -> Result<(), DesktopError> {
        self.ensure_active()?;
        let work_area = Rect::new(0, 0, self.viewport.width, self.viewport.height);
        self.windows.maximize(surface, work_area)?;
        self.sync_bounds(surface)?;
        self.compositor
            .set_visible(surface, true)
            .map_err(|_| DesktopError::Window(WindowError::Missing))?;
        self.focus_window_inner(surface)
    }

    pub fn restore_window(&mut self, surface: SurfaceId) -> Result<(), DesktopError> {
        self.ensure_active()?;
        self.windows.restore(surface)?;
        self.sync_bounds(surface)?;
        self.compositor
            .set_visible(surface, true)
            .map_err(|_| DesktopError::Window(WindowError::Missing))?;
        self.focus_window_inner(surface)
    }

    pub fn set_window_state(
        &mut self,
        surface: SurfaceId,
        state: WindowState,
    ) -> Result<(), DesktopError> {
        match state {
            WindowState::Closed => self.close_window(surface),
            WindowState::Minimized => self.minimize_window(surface),
            WindowState::Maximized => self.maximize_window(surface),
            WindowState::Normal => self.restore_window(surface),
            WindowState::Fullscreen => {
                self.ensure_active()?;
                self.windows.fullscreen(
                    surface,
                    Rect::new(0, 0, self.viewport.width, self.viewport.height),
                )?;
                self.sync_bounds(surface)?;
                self.compositor
                    .set_visible(surface, true)
                    .map_err(|_| DesktopError::Window(WindowError::Missing))?;
                self.focus_window_inner(surface)
            }
        }
    }

    pub fn route_input(&mut self, event: InputEvent) -> Result<RoutedEvent, DesktopError> {
        self.ensure_active()?;
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

    fn active_desktop() -> DesktopSession {
        let mut desktop = DesktopSession::new(DesktopViewport {
            width: 1920,
            height: 1080,
        });
        desktop.unlock();
        desktop
    }

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
        let mut desktop = active_desktop();
        let surface = desktop
            .open_window("Shell", Rect::new(0, 0, 500, 500))
            .unwrap();
        let routed = desktop
            .route_input(InputEvent::PointerMove { x: 10, y: 10 })
            .unwrap();
        assert_eq!(routed.target, Some(surface));
    }

    #[test]
    fn moving_and_resizing_window_updates_compositor_hit_testing() {
        let mut desktop = active_desktop();
        let surface = desktop
            .open_window("Editor", Rect::new(0, 0, 100, 100))
            .unwrap();
        desktop.move_window(surface, 200, 200).unwrap();
        assert_eq!(desktop.hit_test(10, 10), None);
        assert_eq!(desktop.hit_test(210, 210), Some(surface));
        desktop.resize_window(surface, 20, 20).unwrap();
        assert_eq!(desktop.hit_test(225, 225), None);
    }

    #[test]
    fn minimize_hides_surface_and_restore_shows_it() {
        let mut desktop = active_desktop();
        let surface = desktop
            .open_window("Editor", Rect::new(0, 0, 100, 100))
            .unwrap();
        desktop.minimize_window(surface).unwrap();
        assert_eq!(desktop.hit_test(10, 10), None);
        desktop.restore_window(surface).unwrap();
        assert_eq!(desktop.hit_test(10, 10), Some(surface));
    }

    #[test]
    fn maximize_uses_viewport_and_restore_recovers_geometry() {
        let mut desktop = active_desktop();
        let surface = desktop
            .open_window("Editor", Rect::new(10, 20, 300, 200))
            .unwrap();
        desktop.maximize_window(surface).unwrap();
        assert_eq!(
            desktop.windows().iter().find(|w| w.surface == surface).unwrap().bounds,
            Rect::new(0, 0, 1920, 1080)
        );
        desktop.restore_window(surface).unwrap();
        assert_eq!(
            desktop.windows().iter().find(|w| w.surface == surface).unwrap().bounds,
            Rect::new(10, 20, 300, 200)
        );
    }
}
