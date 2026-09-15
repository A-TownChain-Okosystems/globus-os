//! Package metadata and verification policy. Installation is deny-by-default.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub digest: String,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Trusted,
    Untrusted,
}

pub fn verify_metadata(pkg: &Package) -> Verification {
    if pkg.name.is_empty()
        || pkg.version.is_empty()
        || pkg.digest.is_empty()
        || pkg.signature.is_empty()
    {
        Verification::Untrusted
    } else {
        Verification::Trusted
    }
}
