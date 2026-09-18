//! GlobusOS security primitives. Authority is explicit and deny-by-default.

pub mod capability_registry;
pub mod identity;
pub use capability_registry::{CapabilityObject, CapabilityRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Capability(pub u128);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Right {
    Read,
    Write,
    Execute,
    Map,
    Sign,
    Admin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub capability: Capability,
    pub right: Right,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    Allowed,
    Denied,
}
pub fn authorize(grant: Option<Grant>, requested: Right) -> Authorization {
    match grant {
        Some(g) if g.right == requested || g.right == Right::Admin => Authorization::Allowed,
        _ => Authorization::Denied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signing_is_distinct_from_execute() {
        let grant = Grant {
            capability: Capability(1),
            right: Right::Execute,
        };
        assert_eq!(authorize(Some(grant), Right::Sign), Authorization::Denied);
    }
    #[test]
    fn admin_can_sign() {
        let grant = Grant {
            capability: Capability(1),
            right: Right::Admin,
        };
        assert_eq!(authorize(Some(grant), Right::Sign), Authorization::Allowed);
    }
}
