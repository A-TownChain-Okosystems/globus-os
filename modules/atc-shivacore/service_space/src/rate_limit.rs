//! Userspace rate-limiting primitives for Service Space.
//!
//! This module intentionally depends only on `core`, `alloc`, and `spin`.
//! It must not import kernel implementation types. Kernel-enforced resource
//! limits remain a separate security boundary; this token bucket is protocol
//! policy and is not a substitute for kernel-side DoS protection.

use spin::Mutex;

/// A bounded token bucket measured against a monotonic nanosecond clock.
pub struct TokenBucket {
    capacity: u32,
    tokens: Mutex<u32>,
    refill_rate: u32,
    last_refill: Mutex<u64>,
}

impl TokenBucket {
    /// Create a full bucket. `now` must be from a monotonic clock in ns.
    pub fn new(capacity: u32, refill_rate: u32, now: u64) -> Self {
        Self {
            capacity,
            tokens: Mutex::new(capacity),
            refill_rate,
            last_refill: Mutex::new(now),
        }
    }

    fn refill(&self, tokens: &mut u32, last: &mut u64, now: u64) {
        let elapsed_ns = now.saturating_sub(*last);
        let elapsed_secs = elapsed_ns / 1_000_000_000;
        if elapsed_secs == 0 {
            return;
        }

        let refill = elapsed_secs
            .saturating_mul(self.refill_rate as u64)
            .min(u32::MAX as u64) as u32;
        *tokens = tokens.saturating_add(refill).min(self.capacity);
        // Keep the fractional-second remainder rather than losing it.
        *last = last.saturating_add(elapsed_secs.saturating_mul(1_000_000_000));
    }

    /// Consume `count` tokens, returning false if the bucket is exhausted.
    pub fn try_consume(&self, now: u64, count: u32) -> bool {
        let mut tokens = self.tokens.lock();
        let mut last = self.last_refill.lock();
        self.refill(&mut tokens, &mut last, now);
        if *tokens < count {
            return false;
        }
        *tokens -= count;
        true
    }

    /// Report available tokens at `now` without mutating bucket state.
    pub fn available(&self, now: u64) -> u32 {
        let tokens = self.tokens.lock();
        let last = self.last_refill.lock();
        let elapsed_secs = now.saturating_sub(*last) / 1_000_000_000;
        let refill = elapsed_secs
            .saturating_mul(self.refill_rate as u64)
            .min(u32::MAX as u64) as u32;
        tokens.saturating_add(refill).min(self.capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::TokenBucket;

    const SECOND: u64 = 1_000_000_000;

    #[test]
    fn starts_full_and_rejects_overconsumption() {
        let bucket = TokenBucket::new(3, 1, 0);
        assert_eq!(bucket.available(0), 3);
        assert!(bucket.try_consume(0, 3));
        assert!(!bucket.try_consume(0, 1));
    }

    #[test]
    fn refills_after_elapsed_seconds_and_caps_at_capacity() {
        let bucket = TokenBucket::new(2, 1, 0);
        assert!(bucket.try_consume(0, 2));
        assert_eq!(bucket.available(2 * SECOND), 2);
        assert!(bucket.try_consume(2 * SECOND, 2));
        assert!(!bucket.try_consume(2 * SECOND, 1));
    }

    #[test]
    fn backwards_clock_does_not_create_tokens() {
        let bucket = TokenBucket::new(2, 1, SECOND);
        assert!(bucket.try_consume(SECOND, 2));
        assert_eq!(bucket.available(0), 0);
        assert!(!bucket.try_consume(0, 1));
    }

    #[test]
    fn fractional_elapsed_time_is_preserved_across_refills() {
        let bucket = TokenBucket::new(2, 1, 0);
        assert!(bucket.try_consume(0, 2));
        assert!(!bucket.try_consume(SECOND / 2, 1));
        assert!(bucket.try_consume(SECOND, 1));
    }

    #[test]
    fn zero_count_is_a_noop_even_when_empty() {
        let bucket = TokenBucket::new(1, 0, 0);
        assert!(bucket.try_consume(0, 1));
        assert!(bucket.try_consume(0, 0));
        assert!(!bucket.try_consume(0, 1));
    }
}
