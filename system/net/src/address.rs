//! Typed IP/socket endpoint addresses. Parsing is intentionally strict and allocation-free for IPv4.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// IPv4 address in network byte order.
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    /// `0.0.0.0`.
    pub const UNSPECIFIED: Self = Self([0, 0, 0, 0]);
    /// `127.0.0.1`.
    pub const LOOPBACK: Self = Self([127, 0, 0, 1]);
    /// Builds an address from four octets.
    pub const fn new(octets: [u8; 4]) -> Self {
        Self(octets)
    }
    /// True for `0.0.0.0`.
    pub fn is_unspecified(self) -> bool {
        self == Self::UNSPECIFIED
    }
    /// True for any `127.x.x.x` address.
    pub const fn is_loopback(self) -> bool {
        self.0[0] == 127
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// IP address; only IPv4 exists so far.
pub enum IpAddress {
    /// IPv4 address.
    V4(Ipv4Address),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// IP address plus port.
pub struct EndpointAddress {
    /// Address part.
    pub ip: IpAddress,
    /// Port part; zero is rejected by `validate_endpoint`.
    pub port: u16,
}

impl EndpointAddress {
    /// Builds an endpoint from an IPv4 address and a port.
    pub const fn ipv4(ip: Ipv4Address, port: u16) -> Self {
        Self {
            ip: IpAddress::V4(ip),
            port,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Address validation failures.
pub enum AddressError {
    /// Port 0 is not a usable endpoint.
    ZeroPort,
}

/// Rejects an endpoint with port 0.
pub fn validate_endpoint(endpoint: EndpointAddress) -> Result<(), AddressError> {
    if endpoint.port == 0 {
        Err(AddressError::ZeroPort)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_port() {
        assert!(validate_endpoint(EndpointAddress::ipv4(Ipv4Address::LOOPBACK, 443)).is_ok());
        assert_eq!(
            validate_endpoint(EndpointAddress::ipv4(Ipv4Address::LOOPBACK, 0)),
            Err(AddressError::ZeroPort)
        );
    }
}
