//! Object capabilities with explicit revocation.
use super::{Capability, Right};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityObject {
    pub capability: Capability,
    pub right: Right,
    pub object_id: u64,
    pub revoked: bool,
}

#[derive(Debug, Default)]
pub struct CapabilityRegistry {
    objects: HashMap<Capability, CapabilityObject>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn grant(&mut self, capability: Capability, object_id: u64, right: Right) -> bool {
        if self.objects.contains_key(&capability) {
            return false;
        }
        self.objects.insert(
            capability,
            CapabilityObject {
                capability,
                right,
                object_id,
                revoked: false,
            },
        );
        true
    }
    pub fn revoke(&mut self, capability: Capability) -> bool {
        let Some(object) = self.objects.get_mut(&capability) else {
            return false;
        };
        if object.revoked {
            return false;
        }
        object.revoked = true;
        true
    }
    pub fn authorize(&self, capability: Capability, object_id: u64, requested: Right) -> bool {
        let Some(object) = self.objects.get(&capability) else {
            return false;
        };
        !object.revoked
            && object.object_id == object_id
            && (object.right == requested || object.right == Right::Admin)
    }
    pub fn get(&self, capability: Capability) -> Option<&CapabilityObject> {
        self.objects.get(&capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revoked_capability_is_denied() {
        let mut r = CapabilityRegistry::new();
        let c = Capability(7);
        assert!(r.grant(c, 42, Right::Write));
        assert!(r.authorize(c, 42, Right::Write));
        assert!(r.revoke(c));
        assert!(!r.authorize(c, 42, Right::Write));
    }
    #[test]
    fn object_binding_is_enforced() {
        let mut r = CapabilityRegistry::new();
        let c = Capability(8);
        r.grant(c, 1, Right::Read);
        assert!(!r.authorize(c, 2, Right::Read));
    }
}
