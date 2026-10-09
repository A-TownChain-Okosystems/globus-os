//! Userspace security and capability boundary.

pub mod capability_registry;
pub mod identity;

pub use capability_registry::{CapabilityError, CapabilityRegistry, SystemCapabilities};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub capability: Capability,
    pub right: Right,
}

/// The outcome of an authorization check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    /// The requested action is permitted.
    Allowed,
    /// The requested action is denied.
    Denied,
}

pub fn authorize(grants: &[Grant], target: Capability, required: Right) -> Authorization {
    if grants.iter().any(|g| {
        g.capability == target && (g.right == required || g.right == Right::Admin)
    }) {
        Authorization::Allowed
    } else {
        Authorization::Denied
    }
}
