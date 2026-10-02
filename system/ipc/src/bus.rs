//! Capability-scoped IPCBus facade.
//!
//! This layer reuses the existing bounded ChannelRegistry instead of
//! replacing it. Access is explicit: a principal presents the capability
//! assigned to the endpoint for the requested operation. Missing or
//! mismatched credentials fail closed.

use std::collections::BTreeMap;

use crate::{ChannelRegistry, Endpoint, IpcError, Message};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IpcPrincipal(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IpcCapability(pub u128);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcAccess {
    pub principal: IpcPrincipal,
    pub capability: IpcCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcOperation {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointPolicy {
    pub sender: IpcAccess,
    pub receiver: IpcAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcBusError {
    EndpointAlreadyRegistered,
    EndpointNotRegistered,
    Unauthorized(IpcOperation),
    ProtocolVersionMismatch,
    Channel(IpcError),
}

impl From<IpcError> for IpcBusError {
    fn from(value: IpcError) -> Self {
        Self::Channel(value)
    }
}

/// Capability-scoped IPC bus built on the canonical bounded channel primitive.
#[derive(Debug)]
pub struct IpcBus {
    channels: ChannelRegistry,
    policies: BTreeMap<Endpoint, EndpointPolicy>,
    protocol_version: u16,
}

impl IpcBus {
    pub fn new(capacity: usize) -> Self {
        Self::new_with_protocol_version(capacity, 1)
    }

    pub fn new_with_protocol_version(capacity: usize, protocol_version: u16) -> Self {
        assert!(protocol_version != 0);
        Self {
            channels: ChannelRegistry::new(capacity),
            policies: BTreeMap::new(),
            protocol_version,
        }
    }

    pub fn register_endpoint(
        &mut self,
        endpoint: Endpoint,
        policy: EndpointPolicy,
    ) -> Result<(), IpcBusError> {
        if self.policies.contains_key(&endpoint) {
            return Err(IpcBusError::EndpointAlreadyRegistered);
        }
        self.channels.register(endpoint);
        self.policies.insert(endpoint, policy);
        Ok(())
    }

    pub fn policy(&self, endpoint: Endpoint) -> Result<EndpointPolicy, IpcBusError> {
        self.policies
            .get(&endpoint)
            .copied()
            .ok_or(IpcBusError::EndpointNotRegistered)
    }

    pub fn send(
        &mut self,
        access: IpcAccess,
        message: Message,
    ) -> Result<(), IpcBusError> {
        let endpoint = message.header.endpoint;
        let policy = self.policy(endpoint)?;
        if policy.sender != access {
            return Err(IpcBusError::Unauthorized(IpcOperation::Send));
        }
        if message.header.protocol_version != self.protocol_version {
            return Err(IpcBusError::ProtocolVersionMismatch);
        }
        message.validate()?;
        self.channels.send(message)?;
        Ok(())
    }

    pub fn receive(
        &mut self,
        access: IpcAccess,
        endpoint: Endpoint,
    ) -> Result<Option<Message>, IpcBusError> {
        let policy = self.policy(endpoint)?;
        if policy.receiver != access {
            return Err(IpcBusError::Unauthorized(IpcOperation::Receive));
        }
        Ok(self.channels.receive(endpoint)?)
    }

    pub fn pending(&self, endpoint: Endpoint) -> Result<usize, IpcBusError> {
        self.policy(endpoint)?;
        Ok(self.channels.len(endpoint)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> EndpointPolicy {
        EndpointPolicy {
            sender: IpcAccess {
                principal: IpcPrincipal(10),
                capability: IpcCapability(100),
            },
            receiver: IpcAccess {
                principal: IpcPrincipal(20),
                capability: IpcCapability(200),
            },
        }
    }

    #[test]
    fn send_and_receive_require_explicit_capabilities() {
        let mut bus = IpcBus::new(4);
        let endpoint = Endpoint(7);
        let p = policy();
        bus.register_endpoint(endpoint, p).unwrap();

        bus.send(p.sender, Message::new(endpoint, 1, vec![9]))
            .unwrap();
        assert_eq!(
            bus.receive(p.receiver, endpoint)
                .unwrap()
                .unwrap()
                .payload,
            vec![9]
        );
    }

    #[test]
    fn wrong_sender_is_denied() {
        let mut bus = IpcBus::new(4);
        let endpoint = Endpoint(7);
        let p = policy();
        bus.register_endpoint(endpoint, p).unwrap();

        let wrong = IpcAccess {
            principal: IpcPrincipal(10),
            capability: IpcCapability(999),
        };
        assert_eq!(
            bus.send(wrong, Message::new(endpoint, 1, vec![])),
            Err(IpcBusError::Unauthorized(IpcOperation::Send))
        );
    }

    #[test]
    fn wrong_receiver_is_denied() {
        let mut bus = IpcBus::new(4);
        let endpoint = Endpoint(7);
        let p = policy();
        bus.register_endpoint(endpoint, p).unwrap();

        let wrong = IpcAccess {
            principal: IpcPrincipal(20),
            capability: IpcCapability(999),
        };
        assert_eq!(
            bus.receive(wrong, endpoint),
            Err(IpcBusError::Unauthorized(IpcOperation::Receive))
        );
    }

    #[test]
    fn fifo_is_preserved() {
        let mut bus = IpcBus::new(4);
        let endpoint = Endpoint(8);
        let p = policy();
        bus.register_endpoint(endpoint, p).unwrap();

        bus.send(p.sender, Message::new(endpoint, 1, vec![])).unwrap();
        bus.send(p.sender, Message::new(endpoint, 2, vec![])).unwrap();

        assert_eq!(
            bus.receive(p.receiver, endpoint)
                .unwrap()
                .unwrap()
                .header
                .opcode,
            1
        );
        assert_eq!(
            bus.receive(p.receiver, endpoint)
                .unwrap()
                .unwrap()
                .header
                .opcode,
            2
        );
    }

    #[test]
    fn queue_overflow_fails_closed() {
        let mut bus = IpcBus::new(1);
        let endpoint = Endpoint(9);
        let p = policy();
        bus.register_endpoint(endpoint, p).unwrap();

        bus.send(p.sender, Message::new(endpoint, 1, vec![])).unwrap();
        assert_eq!(
            bus.send(p.sender, Message::new(endpoint, 2, vec![])),
            Err(IpcBusError::Channel(IpcError::QueueFull))
        );
    }
}
