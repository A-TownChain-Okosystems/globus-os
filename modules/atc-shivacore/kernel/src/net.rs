// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// Kernel networking boundary: hardware/link primitives only.
// Protocol stacks remain in ShivaCore Service Space.

use alloc::string::String;
use alloc::vec::Vec;

pub const ETH_TYPE_IPV4: u16 = 0x0800;
pub const ETH_TYPE_ARP: u16 = 0x0806;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    pub const fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        Self([a, b, c, d, e, f])
    }

    pub const fn zero() -> Self {
        Self([0; 6])
    }

    pub const fn broadcast() -> Self {
        Self([0xff; 6])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self([a, b, c, d])
    }

    pub const fn is_broadcast(self) -> bool {
        self.0 == [0xff; 4]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    DeviceError,
    NoPacket,
    FrameTooShort,
    InvalidFrame,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthernetFrame {
    pub dst: MacAddress,
    pub src: MacAddress,
    pub ethertype: u16,
    pub payload: Vec<u8>,
}

impl EthernetFrame {
    pub fn new(dst: MacAddress, src: MacAddress, ethertype: u16, payload: Vec<u8>) -> Self {
        Self {
            dst,
            src,
            ethertype,
            payload,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(14 + self.payload.len());
        bytes.extend_from_slice(&self.dst.0);
        bytes.extend_from_slice(&self.src.0);
        bytes.extend_from_slice(&self.ethertype.to_be_bytes());
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, NetworkError> {
        if data.len() < 14 {
            return Err(NetworkError::FrameTooShort);
        }

        let mut dst = [0u8; 6];
        let mut src = [0u8; 6];
        dst.copy_from_slice(&data[0..6]);
        src.copy_from_slice(&data[6..12]);

        Ok(Self {
            dst: MacAddress(dst),
            src: MacAddress(src),
            ethertype: u16::from_be_bytes([data[12], data[13]]),
            payload: data[14..].to_vec(),
        })
    }
}

pub trait NetworkDevice: Send + Sync {
    fn mac_address(&self) -> MacAddress;
    fn send(&self, data: &[u8]) -> Result<(), NetworkError>;
    fn receive(&self) -> Result<Vec<u8>, NetworkError>;
    fn has_packets(&self) -> bool;
}

pub struct LoopbackDevice {
    mac: MacAddress,
    queue: spin::Mutex<Vec<Vec<u8>>>,
    name: String,
}

impl LoopbackDevice {
    pub fn new(name: &str) -> Self {
        Self {
            mac: MacAddress::new(0x02, 0x00, 0x00, 0x00, 0x00, 0x01),
            queue: spin::Mutex::new(Vec::new()),
            name: name.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl NetworkDevice for LoopbackDevice {
    fn mac_address(&self) -> MacAddress {
        self.mac
    }

    fn send(&self, data: &[u8]) -> Result<(), NetworkError> {
        self.queue.lock().push(data.to_vec());
        Ok(())
    }

    fn receive(&self) -> Result<Vec<u8>, NetworkError> {
        self.queue.lock().pop().ok_or(NetworkError::NoPacket)
    }

    fn has_packets(&self) -> bool {
        !self.queue.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ethernet_roundtrip() {
        let frame = EthernetFrame::new(
            MacAddress::broadcast(),
            MacAddress::new(1, 2, 3, 4, 5, 6),
            ETH_TYPE_IPV4,
            vec![1],
        );
        assert_eq!(EthernetFrame::from_bytes(&frame.to_bytes()).unwrap(), frame);
    }

    #[test]
    fn loopback_roundtrip() {
        let device = LoopbackDevice::new("test");
        device.send(&[1, 2]).unwrap();
        assert_eq!(device.receive().unwrap(), vec![1, 2]);
    }
}
