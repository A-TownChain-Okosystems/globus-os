//! Atomic update state machine with rollback semantics.

pub mod health;
pub mod transition;

pub use health::{BootHealth, HealthError};
pub use transition::TransitionError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot { A, B }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateState { Idle, Downloaded, Verified, Activated, RolledBack }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdatePlan { pub current: Slot, pub target: Slot, pub state: UpdateState }

impl UpdatePlan {
    pub fn new(current: Slot) -> Self { Self { current, target: match current { Slot::A => Slot::B, Slot::B => Slot::A }, state: UpdateState::Idle } }
}
