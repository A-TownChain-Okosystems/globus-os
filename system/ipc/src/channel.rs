//! Bounded deterministic IPC channels.
use crate::{Endpoint, MAX_IPC_PAYLOAD, Message, validate_payload};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    PayloadTooLarge,
    QueueFull,
    EndpointNotFound,
}

#[derive(Debug)]
pub struct ChannelRegistry {
    capacity: usize,
    queues: HashMap<Endpoint, VecDeque<Message>>,
}

impl ChannelRegistry {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            queues: HashMap::new(),
        }
    }
    pub fn register(&mut self, endpoint: Endpoint) -> bool {
        self.queues.entry(endpoint).or_default();
        true
    }
    pub fn send(&mut self, message: Message) -> Result<(), IpcError> {
        if message.payload.len() > MAX_IPC_PAYLOAD || !validate_payload(&message.payload) {
            return Err(IpcError::PayloadTooLarge);
        }
        let queue = self
            .queues
            .get_mut(&message.header.endpoint)
            .ok_or(IpcError::EndpointNotFound)?;
        if queue.len() >= self.capacity {
            return Err(IpcError::QueueFull);
        }
        queue.push_back(message);
        Ok(())
    }
    pub fn receive(&mut self, endpoint: Endpoint) -> Result<Option<Message>, IpcError> {
        self.queues
            .get_mut(&endpoint)
            .map(|q| q.pop_front())
            .ok_or(IpcError::EndpointNotFound)
    }
    pub fn len(&self, endpoint: Endpoint) -> Result<usize, IpcError> {
        self.queues
            .get(&endpoint)
            .map(VecDeque::len)
            .ok_or(IpcError::EndpointNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fifo_is_deterministic() {
        let mut r = ChannelRegistry::new(2);
        let e = Endpoint(1);
        r.register(e);
        r.send(Message::new(e, 1, vec![1])).unwrap();
        r.send(Message::new(e, 2, vec![2])).unwrap();
        assert_eq!(r.receive(e).unwrap().unwrap().header.opcode, 1);
        assert_eq!(r.receive(e).unwrap().unwrap().header.opcode, 2);
    }
    #[test]
    fn capacity_is_enforced() {
        let mut r = ChannelRegistry::new(1);
        let e = Endpoint(2);
        r.register(e);
        r.send(Message::new(e, 1, vec![])).unwrap();
        assert_eq!(r.send(Message::new(e, 2, vec![])), Err(IpcError::QueueFull));
    }
}
