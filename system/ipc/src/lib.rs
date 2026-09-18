//! Capability-aware IPC primitives used by GlobusOS.

pub mod channel;
pub use channel::{ChannelRegistry, IpcError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Endpoint(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageHeader {
    pub endpoint: Endpoint,
    pub opcode: u32,
    pub payload_len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub header: MessageHeader,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn new(endpoint: Endpoint, opcode: u32, payload: Vec<u8>) -> Self {
        Self {
            header: MessageHeader {
                endpoint,
                opcode,
                payload_len: payload.len() as u32,
            },
            payload,
        }
    }
    pub fn validate(&self) -> Result<(), IpcError> {
        if self.payload.len() > MAX_IPC_PAYLOAD
            || self.header.payload_len as usize != self.payload.len()
        {
            return Err(IpcError::PayloadTooLarge);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IdentityOpcode {
    Register = 0x0900,
    Login = 0x0901,
    Logout = 0x0902,
    Lock = 0x0903,
    Recover = 0x0904,
    LoadKey = 0x0905,
}
impl IdentityOpcode {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdentityKeyHandle(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityKeyLoadRequest {
    pub key_handle: IdentityKeyHandle,
    pub capability_token: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityKeyLoadResponse {
    pub operation_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRegisterMessage {
    pub user_id: String,
    pub display_name: String,
    pub chain_id: u64,
    pub network: String,
    pub session_ttl_seconds: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRegisterResponse {
    pub user_id: String,
    pub wallet_address: String,
    pub chain_id: u64,
    pub network: String,
    pub recovery_confirmation_required: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuthenticationHandle(pub u128);
impl core::fmt::Debug for AuthenticationHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AuthenticationHandle(REDACTED)")
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityLoginMessage {
    pub user_id: String,
    pub authentication_handle: AuthenticationHandle,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentitySessionResponse {
    pub user_id: String,
    pub wallet_address: String,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum WalletOpcode {
    GetPublicIdentity = 0x1000,
    Provision = 0x1001,
    Sign = 0x1002,
    Lock = 0x1003,
    Delete = 0x1004,
}
impl WalletOpcode {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletIdentityMessage {
    pub user_id: String,
    pub wallet_address: String,
    pub chain_id: u64,
    pub network: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletSignMessage {
    pub user_id: String,
    pub key_id: String,
    pub domain: String,
    pub message: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletSignResponse {
    pub algorithm: String,
    pub signature: Vec<u8>,
    pub message_digest: [u8; 32],
}

pub const MAX_IPC_PAYLOAD: usize = 1024 * 1024;
pub fn validate_payload(payload: &[u8]) -> bool {
    payload.len() <= MAX_IPC_PAYLOAD
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opcode_values_are_stable() {
        assert_eq!(IdentityOpcode::Register.as_u32(), 0x0900);
        assert_eq!(IdentityOpcode::LoadKey.as_u32(), 0x0905);
        assert_eq!(WalletOpcode::Sign.as_u32(), 0x1002);
    }
    #[test]
    fn key_request_contains_only_opaque_handles() {
        let r = IdentityKeyLoadRequest {
            key_handle: IdentityKeyHandle(7),
            capability_token: 9,
        };
        assert_eq!(r.key_handle, IdentityKeyHandle(7));
        assert_eq!(r.capability_token, 9);
    }
    #[test]
    fn registration_excludes_recovery_phrase() {
        let r = IdentityRegisterResponse {
            user_id: "user".into(),
            wallet_address: "ATC00000000000000000000000000000000000".into(),
            chain_id: 1,
            network: "devnet".into(),
            recovery_confirmation_required: true,
        };
        assert!(r.recovery_confirmation_required);
    }
    #[test]
    fn oversized_ipc_is_rejected() {
        assert!(!validate_payload(&vec![0; MAX_IPC_PAYLOAD + 1]));
    }
    #[test]
    fn message_header_cannot_lie_about_payload_size() {
        let mut m = Message::new(Endpoint(1), 7, vec![1, 2, 3]);
        m.header.payload_len = 2;
        assert_eq!(m.validate(), Err(IpcError::PayloadTooLarge));
    }
    #[test]
    fn authentication_handle_debug_is_redacted() {
        assert_eq!(
            format!("{:?}", AuthenticationHandle(42)),
            "AuthenticationHandle(REDACTED)"
        );
    }
}
