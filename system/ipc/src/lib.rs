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
            header: MessageHeader {
                endpoint,
                opcode,
                payload_len: payload.len() as u32,
            },
            payload,
        }
    }
}
