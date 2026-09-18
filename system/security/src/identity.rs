//! Hardware-backed identity and key-use policy contracts.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPurpose {
    DeviceIdentity,
    UserIdentity,
    StorageEncryption,
    Network,
    Signing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBackend {
    Tpm2,
    SecureElement,
    SoftwareSealed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyHandle(pub u128);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    Unavailable,
    Unauthorized,
    InvalidPurpose,
    InvalidHandle,
}

pub trait IdentityProvider {
    fn backend(&self) -> KeyBackend;
    fn attest(&self, nonce: &[u8], output: &mut [u8]) -> Result<usize, KeyError>;
    fn sign(
        &mut self,
        key: KeyHandle,
        purpose: KeyPurpose,
        message: &[u8],
        signature: &mut [u8],
    ) -> Result<usize, KeyError>;
}

pub fn purpose_allowed(requested: KeyPurpose, provisioned: KeyPurpose) -> bool {
    requested == provisioned
}
