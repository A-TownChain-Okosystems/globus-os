//! Network interface contracts for isolated NIC services.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacAddress(pub [u8; 6]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Address(pub [u8; 4]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkInterface {
    pub id: InterfaceId,
    pub mac: MacAddress,
    pub ipv4: Option<Ipv4Address>,
    pub mtu: u16,
}

impl NetworkInterface {
    pub const DEFAULT_MTU: u16 = 1500;
}
