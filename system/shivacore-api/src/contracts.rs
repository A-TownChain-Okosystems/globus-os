//! Phase-2 stable contracts for kernel services.
//!
//! These are boundary contracts only: implementations remain in the kernel or
//! in service space. No kernel-owned tables or concrete subsystem state is exposed.
use crate::{AbiError, CapabilityHandle, CapabilityRights, KernelBootPhase};

/// Resource class identifiers used in capability queries across the ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CapabilityResourceType {
    Memory = 1,
    IpcChannel = 2,
    Accelerator = 3,
    FileSystem = 4,
    Network = 5,
}

/// Read-only request to check an opaque capability handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityCheckRequest {
    pub handle: CapabilityHandle,
    pub required: CapabilityRights,
}

/// Result of a capability check; never includes kernel table pointers or IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityCheckResult {
    pub granted: bool,
    pub effective_rights: CapabilityRights,
}

/// Service-facing capability authority. Implementations must fail closed.
pub trait CapabilityAuthority {
    fn check(
        &self,
        request: CapabilityCheckRequest,
    ) -> Result<CapabilityCheckResult, AbiError>;
}

/// Stable, bounded network boundary. recv returns the number of bytes written.
/// Concrete transport and connection state are intentionally not part of this ABI.
pub trait NetworkStack {
    fn connect(&mut self, peer_node_id: &[u8; 32]) -> Result<u64, AbiError>;
    fn send(&mut self, connection: u64, message: &[u8]) -> Result<(), AbiError>;
    fn recv(&mut self, connection: u64, buffer: &mut [u8]) -> Result<usize, AbiError>;
}

/// Immutable token-bucket parameters. One token is the smallest charge unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenBucketConfig {
    pub capacity: u64,
    pub refill_per_tick: u64,
}

/// Deterministic token bucket with caller-supplied monotonic ticks.
///
/// This primitive is a policy mechanism, not an authorization mechanism. Callers
/// must check capabilities independently. Time going backwards never adds tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenBucket {
    config: TokenBucketConfig,
    tokens: u64,
    last_tick: u64,
}

impl TokenBucket {
    pub const fn new(config: TokenBucketConfig, now_tick: u64) -> Self {
        Self {
            config,
            tokens: config.capacity,
            last_tick: now_tick,
        }
    }

    pub const fn available(&self) -> u64 {
        self.tokens
    }

    pub fn try_consume(&mut self, cost: u64, now_tick: u64) -> Result<(), AbiError> {
        self.refill(now_tick);
        if cost > self.tokens {
            return Err(AbiError::ResourceExhausted);
        }
        self.tokens -= cost;
        Ok(())
    }

    fn refill(&mut self, now_tick: u64) {
        if now_tick <= self.last_tick {
            return;
        }
        let elapsed = now_tick - self.last_tick;
        let added = elapsed.saturating_mul(self.config.refill_per_tick);
        self.tokens = self.tokens.saturating_add(added).min(self.config.capacity);
        self.last_tick = now_tick;
    }
}

/// Boot lifecycle state. phase is informational; it grants no authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KernelBootState {
    Initializing = 0,
    Ready = 1,
    Failed = 2,
}

/// Stable status snapshot without exposing KernelState or invoking boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelBootStatus {
    pub phase: KernelBootPhase,
    pub state: KernelBootState,
    pub error: Option<AbiError>,
}

/// Services may observe boot status but cannot initialize or mutate the kernel.
pub trait KernelBootStatusProvider {
    fn status(&self) -> KernelBootStatus;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_contract_preserves_opaque_handle_and_rights() {
        let request = CapabilityCheckRequest {
            handle: CapabilityHandle(42),
            required: CapabilityRights::READ,
        };
        assert_eq!(request.handle.0, 42);
        assert_eq!(request.required.bits(), CapabilityRights::READ.bits());
    }

    #[test]
    fn token_bucket_consumes_and_refills_deterministically() {
        let mut bucket = TokenBucket::new(
            TokenBucketConfig { capacity: 5, refill_per_tick: 2 },
            10,
        );
        assert_eq!(bucket.try_consume(4, 10), Ok(()));
        assert_eq!(bucket.available(), 1);
        assert_eq!(bucket.try_consume(1, 12), Ok(()));
        assert_eq!(bucket.available(), 4);
    }

    #[test]
    fn token_bucket_fails_closed_and_caps_refill() {
        let mut bucket = TokenBucket::new(
            TokenBucketConfig { capacity: 3, refill_per_tick: 1 },
            5,
        );
        assert_eq!(bucket.try_consume(4, 5), Err(AbiError::ResourceExhausted));
        assert_eq!(bucket.available(), 3);
        assert_eq!(bucket.try_consume(3, 100), Ok(()));
        assert_eq!(bucket.available(), 0);
    }

    #[test]
    fn backwards_time_does_not_refill() {
        let mut bucket = TokenBucket::new(
            TokenBucketConfig { capacity: 4, refill_per_tick: 1 },
            10,
        );
        bucket.try_consume(3, 10).unwrap();
        assert_eq!(bucket.try_consume(2, 9), Err(AbiError::ResourceExhausted));
        assert_eq!(bucket.available(), 1);
    }

    #[test]
    fn boot_status_is_observational_only() {
        let status = KernelBootStatus {
            phase: KernelBootPhase::Ready,
            state: KernelBootState::Ready,
            error: None,
        };
        assert_eq!(status.state, KernelBootState::Ready);
        assert_eq!(status.phase as u8, KernelBootPhase::Ready as u8);
    }
}
