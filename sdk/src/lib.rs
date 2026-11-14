//! GlobusOS application SDK facade.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Handle to a wallet that an application may use.
pub struct WalletHandle {
    /// Wallet address as the chain knows it.
    pub address: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Handle to the identity of the calling application.
pub struct IdentityHandle {
    /// Opaque identity identifier.
    pub id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Handle to a service reachable over the SDK.
pub struct ServiceHandle {
    /// Service name as used in the boot order.
    pub name: String,
}
/// Supplies the identity of the caller.
pub trait IdentityProvider {
    /// Returns the identity of this provider.
    fn identity(&self) -> IdentityHandle;
}
/// Supplies the wallet bound to the caller.
pub trait WalletProvider {
    /// Returns the wallet handle.
    fn wallet(&self) -> WalletHandle;
}
/// Looks up services by name.
pub trait ServiceProvider {
    /// Returns the named service, or `None` when it is not available.
    fn service(&self, name: &str) -> Option<ServiceHandle>;
}
