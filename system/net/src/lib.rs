//! Network policy boundary. Protocol implementations remain behind explicit services.

/// Typed IP and endpoint addresses.
pub mod address;
/// Deterministic socket lifecycle and policy enforcement.
pub mod socket;

pub use address::{AddressError, EndpointAddress, IpAddress, Ipv4Address, validate_endpoint};
pub use socket::{Socket, SocketState, SocketTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Protocol families the boundary knows; implementations live in services.
pub enum Protocol {
    /// IPv4.
    Ipv4,
    /// IPv6.
    Ipv6,
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
    /// DNS resolution.
    Dns,
    /// DHCP configuration.
    Dhcp,
    /// TLS transport.
    Tls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How much networking the system allows.
pub enum NetworkPolicy {
    /// No socket can be created.
    Disabled,
    /// Sockets only on explicit permission.
    Restricted,
    /// Unrestricted operation for trusted services.
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Monotonic socket identifier.
pub struct SocketId(pub u64);
