//! Deterministic socket lifecycle and policy enforcement.

use crate::{NetworkPolicy, Protocol, SocketId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Lifecycle state of a socket.
pub enum SocketState {
    /// Allocated, not yet bound.
    Created,
    /// Bound to a local endpoint.
    Bound,
    /// Accepting connections.
    Listening,
    /// Talking to a peer.
    Connected,
    /// Closed; only reachable as a terminal state.
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// One socket: identifier, protocol and state.
pub struct Socket {
    /// Socket identifier.
    pub id: SocketId,
    /// Protocol this socket speaks.
    pub protocol: Protocol,
    /// Current lifecycle state.
    pub state: SocketState,
}

#[derive(Debug, Default)]
/// All sockets of the system; identifiers are handed out monotonically.
pub struct SocketTable {
    next: u64,
    sockets: Vec<Socket>,
}

impl SocketTable {
    /// Creates an empty table; identifiers start at 1.
    pub fn new() -> Self {
        Self {
            next: 1,
            sockets: Vec::new(),
        }
    }
    /// Allocates a socket, or `None` when the policy disables networking.
    pub fn create(&mut self, protocol: Protocol, policy: NetworkPolicy) -> Option<SocketId> {
        if policy == NetworkPolicy::Disabled {
            return None;
        }
        let id = SocketId(self.next);
        self.next = self.next.checked_add(1)?;
        self.sockets.push(Socket {
            id,
            protocol,
            state: SocketState::Created,
        });
        Some(id)
    }
    /// Moves a socket to `next` when that transition is allowed.
    pub fn transition(&mut self, id: SocketId, next: SocketState) -> bool {
        let Some(socket) = self.sockets.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        let valid = matches!(
            (socket.state, next),
            (SocketState::Created, SocketState::Bound)
                | (SocketState::Bound, SocketState::Listening)
                | (SocketState::Bound, SocketState::Connected)
                | (SocketState::Created, SocketState::Connected)
                | (_, SocketState::Closed)
        );
        if valid {
            socket.state = next;
        }
        valid
    }
    /// Returns the socket with that identifier.
    pub fn get(&self, id: SocketId) -> Option<&Socket> {
        self.sockets.iter().find(|s| s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_policy_denies_creation() {
        let mut t = SocketTable::new();
        assert!(t.create(Protocol::Tcp, NetworkPolicy::Disabled).is_none());
    }
    #[test]
    fn lifecycle_is_checked() {
        let mut t = SocketTable::new();
        let id = t.create(Protocol::Tcp, NetworkPolicy::Normal).unwrap();
        assert!(!t.transition(id, SocketState::Listening));
        assert!(t.transition(id, SocketState::Bound));
        assert!(t.transition(id, SocketState::Listening));
    }
}
