// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// Kernel networking boundary: hardware/link primitives only.
// Protocol stacks remain in ShivaCore Service Space.
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore Service Space — network protocols above the kernel link boundary.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;
use shivacore::net::{EthernetFrame, Ipv4Address, MacAddress, NetworkDevice, NetworkError, ETH_TYPE_IPV4};



#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    FrameTooShort, PacketTooShort, InvalidChecksum, NoFrameAvailable, DeviceDown,
    SendFailed(String), RecvFailed(String), ArpResolutionFailed, UnsupportedProtocol,
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn ethernet_roundtrip(){let f=EthernetFrame::new(MacAddress::broadcast(),MacAddress::new(1,2,3,4,5,6),ETH_TYPE_IPV4,vec![1]);assert_eq!(EthernetFrame::from_bytes(&f.to_bytes()).unwrap(),f);}
 #[test] fn loopback_roundtrip(){let d=LoopbackDevice::new("test");d.send_frame(&[1,2]).unwrap();assert_eq!(d.recv_frame().unwrap(),vec![1,2]);}
}
