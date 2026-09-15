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

/// Stable IPC operation numbers for the userspace wallet service.
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

/// Public wallet identity carried over generic IPC. No secret material is permitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletIdentityMessage {
    pub user_id: String,
    pub wallet_address: String,
    pub chain_id: u64,
    pub network: String,
}

/// Signing request envelope. The generic IPC layer carries only a message to be signed;
/// private keys and recovery phrases are never valid IPC payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletSignMessage {
    pub user_id: String,
    pub key_id: String,
    pub domain: String,
    pub message: Vec<u8>,
}

/// Result envelope returned by the wallet service.
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
    fn wallet_sign_opcode_is_stable() {
        assert_eq!(WalletOpcode::Sign.as_u32(), 0x1002);
    }

    #[test]
    fn wallet_ipc_does_not_model_private_keys() {
        let request = WalletSignMessage {
            user_id: "user".into(),
            key_id: "wallet-primary".into(),
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        };
        assert_eq!(request.domain, "ATC-TX-V1");
    }
}
