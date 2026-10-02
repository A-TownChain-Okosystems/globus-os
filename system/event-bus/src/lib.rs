//! Deterministic, bounded in-process EventBus for GlobusOS.
//!
//! The bus is transport-free. It provides ordered pub/sub, bounded
//! subscriber queues and optional bounded history. It does not authorize
//! operations and never represents canonical state or finality. Timestamps
//! are supplied by the caller so replay/tests do not depend on wall-clock state.

use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubscriptionId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl SchemaVersion {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub event_type: String,
    pub schema_version: SchemaVersion,
    pub source: u64,
    pub domain: u16,
    pub correlation_id: Option<u64>,
    pub causation_id: Option<EventId>,
    pub sequence: u64,
    pub timestamp: u64,
    pub authoritative: bool,
    pub payload: Vec<u8>,
}

impl EventEnvelope {
    pub fn new(
        event_type: impl Into<String>,
        schema_version: SchemaVersion,
        source: u64,
        domain: u16,
        timestamp: u64,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            event_id: EventId(0),
            event_type: event_type.into(),
            schema_version,
            source,
            domain,
            correlation_id: None,
            causation_id: None,
            sequence: 0,
            timestamp,
            authoritative: false,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventFilter {
    Exact(String),
    All,
}

impl EventFilter {
    fn matches(&self, event_type: &str) -> bool {
        match self {
            Self::Exact(expected) => expected == event_type,
            Self::All => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventError {
    InvalidCapacity,
    SubscriptionNotFound,
    SubscriberBackpressure,
}

#[derive(Debug)]
pub struct EventBus {
    subscriber_capacity: usize,
    history_capacity: usize,
    next_event_id: u64,
    next_subscription_id: u64,
    sequence: u64,
    subscribers: BTreeMap<SubscriptionId, Subscriber>,
    history: VecDeque<EventEnvelope>,
}

#[derive(Debug)]
struct Subscriber {
    filter: EventFilter,
    queue: VecDeque<EventEnvelope>,
}

impl EventBus {
    pub fn new(subscriber_capacity: usize, history_capacity: usize) -> Result<Self, EventError> {
        if subscriber_capacity == 0 {
            return Err(EventError::InvalidCapacity);
        }

        Ok(Self {
            subscriber_capacity,
            history_capacity,
            next_event_id: 1,
            next_subscription_id: 1,
            sequence: 0,
            subscribers: BTreeMap::new(),
            history: VecDeque::with_capacity(history_capacity),
        })
    }

    pub fn subscribe(&mut self, filter: EventFilter) -> SubscriptionId {
        let id = SubscriptionId(self.next_subscription_id);
        self.next_subscription_id = self.next_subscription_id.saturating_add(1);
        self.subscribers.insert(
            id,
            Subscriber {
                filter,
                queue: VecDeque::with_capacity(self.subscriber_capacity),
            },
        );
        id
    }

    pub fn unsubscribe(&mut self, id: SubscriptionId) -> Result<(), EventError> {
        self.subscribers
            .remove(&id)
            .map(|_| ())
            .ok_or(EventError::SubscriptionNotFound)
    }

    pub fn publish(&mut self, mut event: EventEnvelope) -> Result<EventId, EventError> {
        let matching: Vec<SubscriptionId> = self
            .subscribers
            .iter()
            .filter_map(|(id, subscriber)| {
                subscriber.filter.matches(&event.event_type).then_some(*id)
            })
            .collect();

        if matching.iter().any(|id| {
            self.subscribers
                .get(id)
                .map(|subscriber| subscriber.queue.len() >= self.subscriber_capacity)
                .unwrap_or(false)
        }) {
            return Err(EventError::SubscriberBackpressure);
        }

        let id = EventId(self.next_event_id);
        self.next_event_id = self.next_event_id.saturating_add(1);
        let sequence = self.sequence;
        self.sequence = self.sequence.saturating_add(1);

        event.event_id = id;
        event.sequence = sequence;

        for id in matching {
            if let Some(subscriber) = self.subscribers.get_mut(&id) {
                subscriber.queue.push_back(event.clone());
            }
        }

        if self.history_capacity != 0 {
            if self.history.len() == self.history_capacity {
                self.history.pop_front();
            }
            self.history.push_back(event);
        }

        Ok(id)
    }

    pub fn receive(&mut self, id: SubscriptionId) -> Result<Option<EventEnvelope>, EventError> {
        self.subscribers
            .get_mut(&id)
            .map(|subscriber| subscriber.queue.pop_front())
            .ok_or(EventError::SubscriptionNotFound)
    }

    pub fn pending(&self, id: SubscriptionId) -> Result<usize, EventError> {
        self.subscribers
            .get(&id)
            .map(|subscriber| subscriber.queue.len())
            .ok_or(EventError::SubscriptionNotFound)
    }

    pub fn history(&self) -> Vec<EventEnvelope> {
        self.history.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: &str, timestamp: u64) -> EventEnvelope {
        EventEnvelope::new(
            kind,
            SchemaVersion::new(1, 0, 0),
            7,
            1,
            timestamp,
            vec![1, 2, 3],
        )
    }

    #[test]
    fn publish_and_receive_are_fifo_and_ordered() {
        let mut bus = EventBus::new(4, 4).unwrap();
        let sub = bus.subscribe(EventFilter::Exact("system.boot".into()));

        let first = bus.publish(event("system.boot", 10)).unwrap();
        let second = bus.publish(event("system.boot", 11)).unwrap();

        assert_eq!(first, EventId(1));
        assert_eq!(second, EventId(2));
        assert_eq!(bus.receive(sub).unwrap().unwrap().event_id, first);
        assert_eq!(bus.receive(sub).unwrap().unwrap().event_id, second);
    }

    #[test]
    fn unrelated_events_are_not_delivered() {
        let mut bus = EventBus::new(2, 2).unwrap();
        let sub = bus.subscribe(EventFilter::Exact("a".into()));
        bus.publish(event("b", 1)).unwrap();
        assert_eq!(bus.receive(sub).unwrap(), None);
    }

    #[test]
    fn backpressure_is_atomic() {
        let mut bus = EventBus::new(1, 4).unwrap();
        let sub = bus.subscribe(EventFilter::All);
        bus.publish(event("a", 1)).unwrap();
        assert_eq!(
            bus.publish(event("b", 2)),
            Err(EventError::SubscriberBackpressure)
        );
        assert_eq!(bus.history().len(), 1);
        assert_eq!(bus.receive(sub).unwrap().unwrap().event_type, "a");
    }

    #[test]
    fn history_is_bounded() {
        let mut bus = EventBus::new(4, 2).unwrap();
        bus.publish(event("a", 1)).unwrap();
        bus.publish(event("b", 2)).unwrap();
        bus.publish(event("c", 3)).unwrap();

        let history = bus.history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].event_type, "b");
        assert_eq!(history[1].event_type, "c");
    }

    #[test]
    fn timestamps_are_caller_supplied() {
        let mut bus = EventBus::new(1, 1).unwrap();
        let sub = bus.subscribe(EventFilter::All);
        bus.publish(event("clock.test", 1234)).unwrap();
        assert_eq!(bus.receive(sub).unwrap().unwrap().timestamp, 1234);
    }

    #[test]
    fn unsubscribe_is_explicit() {
        let mut bus = EventBus::new(1, 1).unwrap();
        let sub = bus.subscribe(EventFilter::All);
        bus.unsubscribe(sub).unwrap();
        assert_eq!(bus.receive(sub), Err(EventError::SubscriptionNotFound));
    }
}
