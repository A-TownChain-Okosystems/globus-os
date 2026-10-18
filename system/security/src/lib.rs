//! GlobusOS security primitives. Authority is explicit and deny-by-default.

pub mod capability_registry;
pub mod identity;
pub use capability_registry::{CapabilityObject, CapabilityRegistry};

/// A capability identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Capability(pub u128);

/// A permission right for accessing a resource or capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Right {
    /// Permission to read.
    Read,
    /// Permission to write.
    Write,
    /// Permission to execute.
    Execute,
    /// Permission to map memory or resources.
    Map,
    /// Permission to perform cryptographic signing.
    Sign,
    /// Administrative permission with full access.
    Admin,
}

/// The outcome of an authorization check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    /// The requested action is permitted.
    Allowed,
    /// The requested action is denied.
    Denied,
}

/// Authorizes only a capability that exists in the trusted registry, is bound to
/// the requested object, has the requested right, and has not been revoked.
/// Callers must never treat a caller-constructed permission claim as authority.
pub fn authorize(
    registry: &CapabilityRegistry,
    capability: Capability,
    object_id: u64,
    requested: Right,
) -> Authorization {
    if registry.authorize(capability, object_id, requested) {
        Authorization::Allowed
    } else {
        Authorization::Denied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_capability_is_denied() {
        let registry = CapabilityRegistry::new();
        assert_eq!(
            authorize(&registry, Capability(99), 7, Right::Sign),
            Authorization::Denied
        );
    }

    #[test]
    fn registry_enforces_object_binding_and_revocation() {
        let mut registry = CapabilityRegistry::new();
        let capability = Capability(7);
        assert!(registry.grant(capability, 42, Right::Sign));
        assert_eq!(
            authorize(&registry, capability, 42, Right::Sign),
            Authorization::Allowed
        );
        assert_eq!(
            authorize(&registry, capability, 99, Right::Sign),
            Authorization::Denied
        );
        assert!(registry.revoke(capability));
        assert_eq!(
            authorize(&registry, capability, 42, Right::Sign),
            Authorization::Denied
        );
    }

    #[test]
    fn wrong_right_is_denied() {
        let mut registry = CapabilityRegistry::new();
        let capability = Capability(8);
        assert!(registry.grant(capability, 42, Right::Execute));
        assert_eq!(
            authorize(&registry, capability, 42, Right::Sign),
            Authorization::Denied
        );
    }
}
