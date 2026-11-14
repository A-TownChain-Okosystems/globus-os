//! Boot health tracking for fail-closed A/B updates.

use crate::Slot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// In-memory boot health for the fail-closed A/B switch.
pub struct BootHealth {
    /// Slot this tracker observes.
    pub slot: Slot,
    /// Attempts counted so far.
    pub attempts: u32,
    /// Attempts allowed before a rollback is due.
    pub max_attempts: u32,
    /// True once the boot was confirmed healthy.
    pub confirmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Misuse of the health tracker.
pub enum HealthError {
    /// `max_attempts` was 0.
    InvalidLimit,
    /// The boot had already been confirmed.
    AlreadyConfirmed,
}

impl BootHealth {
    /// Creates a tracker for a slot with a non-zero attempt limit.
    pub fn new(slot: Slot, max_attempts: u32) -> Result<Self, HealthError> {
        if max_attempts == 0 {
            return Err(HealthError::InvalidLimit);
        }
        Ok(Self {
            slot,
            attempts: 0,
            max_attempts,
            confirmed: false,
        })
    }

    /// Counts an attempt and reports whether a rollback is due.
    pub fn record_attempt(&mut self) -> Result<bool, HealthError> {
        if self.confirmed {
            return Err(HealthError::AlreadyConfirmed);
        }
        self.attempts = self.attempts.saturating_add(1);
        Ok(self.should_rollback())
    }

    /// Marks the boot as healthy.
    pub fn confirm(&mut self) {
        self.confirmed = true;
    }

    /// True when unconfirmed attempts reached the limit.
    pub fn should_rollback(&self) -> bool {
        !self.confirmed && self.attempts >= self.max_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rollback_after_failed_attempt_limit() {
        let mut health = BootHealth::new(Slot::B, 2).unwrap();
        assert!(!health.record_attempt().unwrap());
        assert!(health.record_attempt().unwrap());
    }
    #[test]
    fn confirmation_prevents_retry_counter() {
        let mut health = BootHealth::new(Slot::A, 2).unwrap();
        health.confirm();
        assert_eq!(health.record_attempt(), Err(HealthError::AlreadyConfirmed));
        assert!(!health.should_rollback());
    }
}
