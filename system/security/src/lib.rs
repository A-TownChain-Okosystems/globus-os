//! GlobusOS security primitives. Authority is explicit and deny-by-default.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Capability(pub u128);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Right { Read, Write, Execute, Map, Admin }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub capability: Capability,
    pub right: Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization { Allowed, Denied }

pub fn authorize(grant: Option<Grant>, requested: Right) -> Authorization {
    match grant {
        Some(g) if g.right == requested || g.right == Right::Admin => Authorization::Allowed,
        _ => Authorization::Denied,
    }
}
