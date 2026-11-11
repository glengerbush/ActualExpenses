//! Router-owned revision cache, bounded by both entry count and serialized bytes.
use serde_json::Value;
use std::collections::VecDeque;
#[derive(Default)]
pub(crate) struct Cache {
    entries: VecDeque<(String, Value, usize)>,
    bytes: usize,
}
impl Cache {
    pub fn get(&mut self, key: &str) -> Option<Value> {
        let index = self.entries.iter().position(|entry| entry.0 == key)?;
        let entry = self.entries.remove(index)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }
    pub fn put(&mut self, key: String, value: Value) {
        let size = key.len() + value.to_string().len();
        while self.entries.len() >= 8 || self.bytes + size > 16 * 1024 * 1024 {
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= entry.2;
        }
        if size <= 16 * 1024 * 1024 {
            self.bytes += size;
            self.entries.push_back((key, value, size));
        }
    }
}
