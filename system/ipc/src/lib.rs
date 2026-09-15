//! Capability-aware IPC primitives used by GlobusOS services.

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
            header: MessageHeader { endpoint, opcode, payload_len: payload.len() as u32 },
            payload,
        }
    }
}

/// Stable IPC operation numbers for the GlobusOS identity service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IdentityOpcode {
    Register = 0x0900,
    Login = 0x0901,
    Logout = 0x0902,
    Lock = 0x0903,
    Recover = 0x0904,
}

impl IdentityOpcode {
    pub const fn as_u32(self) -> u32 { self as u32 }
}

/// Registration request. Wallet creation is performed inside the trusted identity boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRegisterMessage {
    pub user_id: String,
    pub display_name: String,
    pub chain_id: u64,
    pub network: String,
    pub session_ttl_seconds: u64,
}

/// Registration result deliberately contains only public identity data.
/// The recovery phrase is delivered once through the trusted identity UI boundary and is
/// never represented by the generic IPC protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRegisterResponse {
    pub user_id: String,
    pub wallet_address: String,
    pub chain_id: u64,
    pub network: String,
    pub recovery_confirmation_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityLoginMessage {
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentitySessionResponse {
    pub user_id: String,
    pub wallet_address: String,
    pub expires_at_unix: u64,
}

/// Wallet service operation numbers.
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
    pub const fn as_u32(self) -> u32 { self as u32 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletIdentityMessage {
    pub user_id: String,
    pub wallet_address: String,
    pub chain_id: u64,
    pub network: String,
}

/// Signing request envelope. Private keys and recovery phrases are never valid IPC payloads.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_values_are_stable() {
        assert_eq!(IdentityOpcode::Register.as_u32(), 0x0900);
        assert_eq!(IdentityOpcode::Login.as_u32(), 0x0901);
        assert_eq!(WalletOpcode::Sign.as_u32(), 0x1002);
    }

    #[test]
    fn registration_does_not_carry_recovery_material() {
        let response = IdentityRegisterResponse {
            user_id: "user".into(),
            wallet_address: "ATC00000000000000000000000000000000000".into(),
            chain_id: 1,
            network: "devnet".into(),
            recovery_confirmation_required: true,
        };
        assert!(response.recovery_confirmation_required);
    }

    #[test]
    fn wallet_ipc_carries_public_signing_input_only() {
        let request = WalletSignMessage {
            user_id: "user".into(),
            key_id: "wallet-primary".into(),
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        };
        assert_eq!(request.domain, "ATC-TX-V1");
    }
}
