//! Bounded in-memory thumbnail cache.

use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbnailSize { Small, Medium, Large }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbnailKey { pub uri: String, pub modified_ns: u64, pub size: ThumbnailSize }

#[derive(Debug)]
pub struct ThumbnailCache { capacity: usize, map: HashMap<ThumbnailKey, Vec<u8>>, order: VecDeque<ThumbnailKey> }

impl ThumbnailCache {
    pub fn new(capacity: usize) -> Self { Self { capacity, map: HashMap::new(), order: VecDeque::new() } }
    pub fn len(&self) -> usize { self.map.len() }
    pub fn insert(&mut self, key: ThumbnailKey, data: Vec<u8>) {
        if self.capacity == 0 { return; }
        self.map.insert(key.clone(), data); self.order.retain(|k| k != &key); self.order.push_back(key);
        while self.order.len() > self.capacity { if let Some(old) = self.order.pop_front() { self.map.remove(&old); } }
    }
    pub fn get(&mut self, key: &ThumbnailKey) -> Option<&[u8]> {
        if self.map.contains_key(key) { self.order.retain(|k| k != key); self.order.push_back(key.clone()); }
        self.map.get(key).map(Vec::as_slice)
    }
    pub fn invalidate_uri(&mut self, uri: &str) { let keys: Vec<_> = self.order.iter().filter(|k| k.uri == uri).cloned().collect(); for k in keys { self.map.remove(&k); } self.order.retain(|k| k.uri != uri); }
}
