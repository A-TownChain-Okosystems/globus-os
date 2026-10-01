// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// Kernel networking boundary: hardware/link primitives only.
// Protocol stacks (ARP/IPv4/UDP/TCP/P2P) live in ShivaCore Service Space.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

// ─── MAC-Adresse ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        MacAddress([a, b, c, d, e, f])
    }

    pub fn broadcast() -> Self {
        MacAddress([0xFF; 6])
    }

    pub fn zero() -> Self {
        MacAddress([0; 6])
    }

    pub fn is_broadcast(&self) -> bool {
        self.0 == [0xFF; 6]
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0; 6]
    }

    pub fn to_string(&self) -> String {
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

// ─── IPv4-Adresse ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Ipv4Address([a, b, c, d])
    }

    pub fn zero() -> Self {
        Ipv4Address([0; 4])
    }
    pub fn broadcast() -> Self {
        Ipv4Address([0xFF; 4])
    }

    pub fn is_broadcast(&self) -> bool {
        self.0 == [0xFF; 4]
    }
    pub fn is_zero(&self) -> bool {
        self.0 == [0; 4]
    }

    pub fn to_string(&self) -> String {
        format!("{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}


// ─── Ethernet-Frame ────────────────────────────────────────────────────────

pub const ETH_TYPE_ARP: u16 = 0x0806;
pub const ETH_TYPE_IPV4: u16 = 0x0800;

#[derive(Clone, Debug, PartialEq)]
pub struct EthernetFrame {
    pub dst_mac: MacAddress,
    pub src_mac: MacAddress,
    pub ethertype: u16,
    pub payload: Vec<u8>,
}

impl EthernetFrame {
    pub fn new(dst: MacAddress, src: MacAddress, ethertype: u16, payload: Vec<u8>) -> Self {
        EthernetFrame {
            dst_mac: dst,
            src_mac: src,
            ethertype,
            payload,
        }
    }

    /// Serialisiert das Frame zu Bytes (dst[6] + src[6] + type[2] + payload).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(14 + self.payload.len());
        buf.extend_from_slice(&self.dst_mac.0);
        buf.extend_from_slice(&self.src_mac.0);
        buf.extend_from_slice(&self.ethertype.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Deserialisiert ein Frame aus Bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, NetworkError> {
        if data.len() < 14 {
            return Err(NetworkError::FrameTooShort);
        }
        let mut dst = [0u8; 6];
        dst.copy_from_slice(&data[0..6]);
        let mut src = [0u8; 6];
        src.copy_from_slice(&data[6..12]);
        let ethertype = u16::from_be_bytes([data[12], data[13]]);
        let payload = data[14..].to_vec();

        Ok(EthernetFrame {
            dst_mac: MacAddress(dst),
            src_mac: MacAddress(src),
            ethertype,
            payload,
        })
    }
}


// ─── NetworkDevice Trait ────────────────────────────────────────────────────

pub trait NetworkDevice: Send + Sync {
    /// Sendet ein Ethernet-Frame.
    fn send_frame(&self, frame: &[u8]) -> Result<(), NetworkError>;

    /// Empfängt ein Frame (blockierend oder nicht, je nach Implementierung).
    fn recv_frame(&self) -> Result<Vec<u8>, NetworkError>;

    /// MAC-Adresse des Geräts.
    fn mac_address(&self) -> MacAddress;

    /// MTU (Maximum Transmission Unit) in Bytes.
    fn mtu(&self) -> usize {
        1500
    }

    /// Ob das Gerät "up" ist (verlinkt).
    fn is_up(&self) -> bool {
        true
    }

    /// Gerätename.
    fn name(&self) -> &str {
        "net-device"
    }
}

// ─── LoopbackDevice (für Tests) ─────────────────────────────────────────────

pub struct LoopbackDevice {
    mac: MacAddress,
    queue: Mutex<Vec<Vec<u8>>>,
    dev_name: String,
}

impl LoopbackDevice {
    pub fn new(name: &str) -> Self {
        LoopbackDevice {
            mac: MacAddress::new(0x02, 0x00, 0x00, 0x00, 0x00, 0x01),
            queue: Mutex::new(Vec::new()),
            dev_name: name.to_string(),
        }
    }

    pub fn queue_len(&self) -> usize {
        self.queue.lock().len()
    }
}

impl NetworkDevice for LoopbackDevice {
    fn send_frame(&self, frame: &[u8]) -> Result<(), NetworkError> {
        // Loopback: Frame wird in die Empfangs-Queue gesteckt
        self.queue.lock().push(frame.to_vec());
        Ok(())
    }

    fn recv_frame(&self) -> Result<Vec<u8>, NetworkError> {
        let mut queue = self.queue.lock();
        queue.pop().ok_or(NetworkError::NoFrameAvailable)
    }

    fn mac_address(&self) -> MacAddress {
        self.mac
    }
    fn name(&self) -> &str {
        &self.dev_name
    }
}



#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    FrameTooShort,
    PacketTooShort,
    InvalidChecksum,
    NoFrameAvailable,
    DeviceDown,
    SendFailed(String),
    RecvFailed(String),
    ArpResolutionFailed,
    UnsupportedProtocol,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ethernet_roundtrip() {
        let frame = EthernetFrame::new(MacAddress::broadcast(), MacAddress::new(1,2,3,4,5,6), ETH_TYPE_IPV4, vec![1,2,3]);
        assert_eq!(EthernetFrame::from_bytes(&frame.to_bytes()).unwrap(), frame);
    }
    #[test]
    fn ethernet_rejects_short_frame() {
        assert_eq!(EthernetFrame::from_bytes(&[0u8; 13]), Err(NetworkError::FrameTooShort));
    }
    #[test]
    fn loopback_device_roundtrip() {
        let dev = LoopbackDevice::new("test");
        dev.send_frame(&[1,2,3]).unwrap();
        assert_eq!(dev.recv_frame().unwrap(), vec![1,2,3]);
    }
}
