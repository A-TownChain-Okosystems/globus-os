//! Network policy boundary. Protocol implementations remain behind explicit services.

pub mod address;
pub mod ethernet;
pub mod socket;

pub use address::{AddressError, EndpointAddress, IpAddress, Ipv4Address, validate_endpoint};
pub use socket::{Socket, SocketState, SocketTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Ipv4,
    Ipv6,
    Tcp,
    Udp,
}
