//! GlobusOS settings core.
//!
//! Settings are treated as policy-controlled configuration, not direct hardware
//! access. Callers must first obtain the capability required for a mutation.

use core::fmt;

/// Supported settings domains.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Domain {
    /// Core system behaviour.
    System,
    /// Security policy.
    Security,
    /// Privacy defaults.
    Privacy,
    /// Network configuration.
    Network,
    /// Display and graphics.
    Display,
    /// Audio devices and levels.
    Audio,
    /// Storage and volumes.
    Storage,
    /// Installed applications.
    Applications,
    /// Identity and accounts.
    Identity,
    /// AI runtime and model settings.
    Ai,
    /// Chain and wallet settings.
    Blockchain,
    /// Developer options and debug switches.
    Developer,
    /// Update channel and schedule.
    Updates,
}

/// Value types accepted by the settings store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// Boolean switch.
    Bool(bool),
    /// Signed integer value.
    Integer(i64),
    /// Free text value.
    Text(String),
}

/// Authorization capability required for a setting mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capability {
    /// Read any setting.
    ReadSettings,
    /// Write settings in the system domain.
    WriteSystem,
    /// Write security policy settings.
    WriteSecurity,
    /// Write privacy settings.
    WritePrivacy,
    /// Write network settings.
    WriteNetwork,
    /// Write device settings: display, audio, storage.
    WriteDevice,
    /// Write identity and account settings.
    WriteIdentity,
    /// Write AI runtime settings.
    WriteAi,
    /// Write chain and wallet settings.
    WriteBlockchain,
    /// Write developer options.
    WriteDeveloper,
    /// Write update settings.
    WriteUpdates,
}

/// Errors returned by the settings core.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// The setting key is empty.
    EmptyKey,
    /// The domain does not match the key.
    InvalidDomain,
    /// The caller lacks the required capability.
    PermissionDenied,
    /// No such setting.
    NotFound,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::EmptyKey => "setting key is empty",
            Self::InvalidDomain => "setting domain is invalid",
            Self::PermissionDenied => "settings capability denied",
            Self::NotFound => "setting not found",
        };
        f.write_str(text)
    }
}

/// A validated setting mutation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mutation {
    /// Domain the setting belongs to.
    pub domain: Domain,
    /// Setting key within the domain.
    pub key: String,
    /// New value.
    pub value: Value,
}

/// Stateless policy evaluator used by the settings service boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct Policy;

impl Policy {
    /// Returns whether a capability can mutate the supplied domain.
    pub const fn allows(self, capability: Capability, domain: Domain) -> bool {
        matches!(
            (capability, domain),
            (Capability::WriteSystem, Domain::System)
                | (Capability::WriteSecurity, Domain::Security)
                | (Capability::WritePrivacy, Domain::Privacy)
                | (Capability::WriteNetwork, Domain::Network)
                | (Capability::WriteDevice, Domain::Display)
                | (Capability::WriteDevice, Domain::Audio)
                | (Capability::WriteDevice, Domain::Storage)
                | (Capability::WriteDevice, Domain::Applications)
                | (Capability::WriteIdentity, Domain::Identity)
                | (Capability::WriteAi, Domain::Ai)
                | (Capability::WriteBlockchain, Domain::Blockchain)
                | (Capability::WriteDeveloper, Domain::Developer)
                | (Capability::WriteUpdates, Domain::Updates)
        )
    }
}

/// Validate and authorize a mutation before it reaches an OS service.
pub fn authorize(policy: Policy, capability: Capability, mutation: &Mutation) -> Result<(), Error> {
    if mutation.key.trim().is_empty() {
        return Err(Error::EmptyKey);
    }
    if !policy.allows(capability, mutation.domain) {
        return Err(Error::PermissionDenied);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies_cross_domain_writes() {
        let mutation = Mutation {
            domain: Domain::Security,
            key: "secure_boot.enabled".into(),
            value: Value::Bool(true),
        };
        assert_eq!(
            authorize(Policy, Capability::WriteSystem, &mutation),
            Err(Error::PermissionDenied)
        );
    }

    #[test]
    fn allows_scoped_write() {
        let mutation = Mutation {
            domain: Domain::Privacy,
            key: "telemetry.enabled".into(),
            value: Value::Bool(false),
        };
        assert!(authorize(Policy, Capability::WritePrivacy, &mutation).is_ok());
    }

    #[test]
    fn rejects_empty_keys() {
        let mutation = Mutation {
            domain: Domain::System,
            key: "  ".into(),
            value: Value::Text("x".into()),
        };
        assert_eq!(
            authorize(Policy, Capability::WriteSystem, &mutation),
            Err(Error::EmptyKey)
        );
    }
}
