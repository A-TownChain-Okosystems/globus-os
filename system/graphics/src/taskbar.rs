//! GlobusOS Glass Neon Taskbar design and interaction contract.
//!
//! Rendering backends consume this model. The model contains no platform GUI
//! dependency so it can be used by native, egui, compositor or future clients.

use crate::SurfaceId;

/// Taskbar presentation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarMode {
    Glass,
    GlassNeon,
    Hud,
}

/// Semantic state driving the visual glow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowState {
    Normal,
    AppActive,
    Download,
    Notification,
    AiWorking,
    SystemProblem,
    CriticalError,
    Gaming,
    Blockchain,
}

/// ShivaCore/Aurora activity indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    Idle,
    Thinking,
    Processing,
    Generating,
}

/// Declarative glass surface parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlassStyle {
    pub opacity_percent: u8,
    pub blur_radius: u16,
    pub border_width: u8,
    pub corner_radius: u16,
    pub reflection_percent: u8,
}

impl GlassStyle {
    pub const fn for_mode(mode: TaskbarMode) -> Self {
        match mode {
            TaskbarMode::Glass => Self {
                opacity_percent: 18,
                blur_radius: 28,
                border_width: 1,
                corner_radius: 18,
                reflection_percent: 8,
            },
            TaskbarMode::GlassNeon => Self {
                opacity_percent: 28,
                blur_radius: 32,
                border_width: 1,
                corner_radius: 20,
                reflection_percent: 12,
            },
            TaskbarMode::Hud => Self {
                opacity_percent: 42,
                blur_radius: 20,
                border_width: 1,
                corner_radius: 12,
                reflection_percent: 5,
            },
        }
    }
}

/// Taskbar geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskbarGeometry {
    pub width: u32,
    pub height: u32,
    pub bottom_margin: u16,
}

/// A taskbar item. The renderer decides how its icon is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskbarItem {
    pub app_id: u64,
    pub surface: Option<SurfaceId>,
    pub active: bool,
    pub minimized: bool,
    pub pinned: bool,
}

/// Complete declarative taskbar state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskbarState {
    pub mode: TaskbarMode,
    pub geometry: TaskbarGeometry,
    pub glow: GlowState,
    pub activity: ActivityState,
    pub items: alloc::vec::Vec<TaskbarItem>,
}

impl TaskbarState {
    pub fn new(width: u32, mode: TaskbarMode) -> Self {
        Self {
            mode,
            geometry: TaskbarGeometry {
                width,
                height: 64,
                bottom_margin: 16,
            },
            glow: GlowState::Normal,
            activity: ActivityState::Idle,
            items: alloc::vec::Vec::new(),
        }
    }

    pub fn glass_style(&self) -> GlassStyle {
        GlassStyle::for_mode(self.mode)
    }

    pub fn set_glow(&mut self, glow: GlowState) {
        self.glow = glow;
    }

    pub fn set_activity(&mut self, activity: ActivityState) {
        self.activity = activity;
    }

    pub fn set_mode(&mut self, mode: TaskbarMode) {
        self.mode = mode;
    }

    pub fn add_item(&mut self, item: TaskbarItem) {
        self.items.retain(|existing| existing.app_id != item.app_id);
        self.items.push(item);
    }

    pub fn activate(&mut self, app_id: u64) {
        for item in &mut self.items {
            item.active = item.app_id == app_id;
            if item.active {
                item.minimized = false;
            }
        }
    }

    pub fn minimize(&mut self, app_id: u64) {
        if let Some(item) = self.items.iter_mut().find(|item| item.app_id == app_id) {
            item.minimized = true;
            item.active = false;
        }
    }

    pub fn remove_surface(&mut self, surface: SurfaceId) {
        for item in &mut self.items {
            if item.surface == Some(surface) {
                item.surface = None;
                item.active = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_neon_profile_is_deterministic() {
        let state = TaskbarState::new(1920, TaskbarMode::GlassNeon);
        assert_eq!(state.glass_style().blur_radius, 32);
        assert_eq!(state.glass_style().corner_radius, 20);
    }

    #[test]
    fn activation_is_exclusive() {
        let mut state = TaskbarState::new(1920, TaskbarMode::GlassNeon);
        state.add_item(TaskbarItem {
            app_id: 1,
            surface: Some(SurfaceId(1)),
            active: false,
            minimized: false,
            pinned: true,
        });
        state.add_item(TaskbarItem {
            app_id: 2,
            surface: Some(SurfaceId(2)),
            active: false,
            minimized: false,
            pinned: false,
        });
        state.activate(2);
        assert!(!state.items[0].active);
        assert!(state.items[1].active);
    }

    #[test]
    fn critical_error_is_explicit_visual_state() {
        let mut state = TaskbarState::new(1920, TaskbarMode::GlassNeon);
        state.set_glow(GlowState::CriticalError);
        assert_eq!(state.glow, GlowState::CriticalError);
    }
}
