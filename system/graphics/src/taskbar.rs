//! Deterministic visual-state contracts for the Globus glass/neon desktop taskbar.
use crate::AppId;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] #[repr(u8)] pub enum TaskbarMode { Glass = 0, GlassNeon = 1, Hud = 2 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] #[repr(u8)] pub enum ActivityState { Idle = 0, Thinking = 1, Processing = 2, Generating = 3 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] #[repr(u8)] pub enum GlowState { Normal = 0, AppActive = 1, Download = 2, Notification = 3, AiWorking = 4, SystemProblem = 5, Critical = 6, Gaming = 7, Blockchain = 8 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct GlassStyle { pub opacity_percent: u8, pub blur_radius: u16, pub border_width: u8, pub glow_intensity: u8, pub corner_radius: u16, pub reflection: bool, pub parallax: bool }
impl GlassStyle { pub const fn glass_neon() -> Self { Self { opacity_percent: 72, blur_radius: 24, border_width: 1, glow_intensity: 36, corner_radius: 18, reflection: true, parallax: true } } }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct TaskbarItem { pub app: AppId, pub active: bool, pub minimized: bool, pub glow: GlowState }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct ActivityIndicator { pub state: ActivityState, pub visible: bool }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct TaskbarState { pub mode: TaskbarMode, pub style: GlassStyle, pub activity: ActivityIndicator }
impl Default for TaskbarState { fn default() -> Self { Self { mode: TaskbarMode::GlassNeon, style: GlassStyle::glass_neon(), activity: ActivityIndicator { state: ActivityState::Idle, visible: true } } } }
impl TaskbarState {
    pub fn set_mode(&mut self, mode: TaskbarMode) { self.mode = mode; }
    pub fn set_activity(&mut self, state: ActivityState) { self.activity = ActivityIndicator { state, visible: true }; }
    pub const fn glow_for(&self, active: bool, downloading: bool, notification: bool, system_problem: bool, critical: bool, gaming: bool, blockchain: bool) -> GlowState {
        if critical { GlowState::Critical } else if system_problem { GlowState::SystemProblem } else if downloading { GlowState::Download } else if notification { GlowState::Notification } else if gaming { GlowState::Gaming } else if blockchain { GlowState::Blockchain } else if self.activity.state != ActivityState::Idle { GlowState::AiWorking } else if active { GlowState::AppActive } else { GlowState::Normal }
    }
}
#[cfg(test)] mod tests { use super::*; #[test] fn glass_neon_is_the_default_mode() { let state = TaskbarState::default(); assert_eq!(state.mode, TaskbarMode::GlassNeon); assert!(state.style.reflection); assert!(state.style.parallax); } #[test] fn critical_state_has_priority() { let state = TaskbarState::default(); assert_eq!(state.glow_for(true,true,true,true,true,true,true), GlowState::Critical); } #[test] fn ai_activity_changes_glow() { let mut state = TaskbarState::default(); state.set_activity(ActivityState::Processing); assert_eq!(state.glow_for(false,false,false,false,false,false,false), GlowState::AiWorking); } }