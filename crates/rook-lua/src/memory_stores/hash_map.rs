//! A hash map: keys with values, listed in pages (creator-docs
//! MemoryStoreHashMap). Roblox promises no order; Rook lists keys in
//! byte order, so a listing resumes after the last key it gave
//! (Approximation: Roblox's order follows its partitions).

use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::ops::Bound as Edge;

use super::{Deadlines, MemoryStoreError, Time, check_key, value_size};

#[derive(Debug)]
struct Entry {
    value: Json,
    expires: Time,
}

#[derive(Debug, Default)]
pub(crate) struct HashMap {
    entries: BTreeMap<String, Entry>,
    deadlines: Deadlines<String>,
}

impl HashMap {
    fn expire(&mut self, now: Time) {
        for key in self.deadlines.due(now) {
            self.entries.remove(&key);
        }
    }

    /// `GetAsync`.
    pub(crate) fn get(&mut self, now: Time, key: &str) -> Result<Option<Json>, MemoryStoreError> {
        check_key(key)?;
        self.expire(now);
        Ok(self.entries.get(key).map(|e| e.value.clone()))
    }

    /// `SetAsync`: whether the key was already there. Hash maps have no
    /// item count or size limit of their own (creator-docs "Memory store
    /// best practices").
    pub(crate) fn set(
        &mut self,
        now: Time,
        key: &str,
        value: Json,
        expiration: f64,
    ) -> Result<bool, MemoryStoreError> {
        check_key(key)?;
        value_size(&value)?;
        self.expire(now);
        let expires = now.after(expiration);
        let old = self
            .entries
            .insert(key.to_owned(), Entry { value, expires });
        if let Some(old) = &old {
            self.deadlines.remove(old.expires, key.to_owned());
        }
        self.deadlines.add(expires, key.to_owned());
        Ok(old.is_some())
    }

    /// `RemoveAsync`.
    pub(crate) fn remove(&mut self, now: Time, key: &str) -> Result<(), MemoryStoreError> {
        check_key(key)?;
        self.expire(now);
        if let Some(old) = self.entries.remove(key) {
            self.deadlines.remove(old.expires, key.to_owned());
        }
        Ok(())
    }

    /// A `ListItemsAsync` page: up to `count` items after the key `after`
    /// (from the start without one), and whether any come after them.
    pub(crate) fn page(
        &mut self,
        now: Time,
        after: Option<&str>,
        count: usize,
    ) -> (Vec<(String, Json)>, bool) {
        self.expire(now);
        let from = after.map_or(Edge::Unbounded, |k| Edge::Excluded(k.to_owned()));
        let mut items = (self.entries.range((from, Edge::Unbounded)))
            .take(count + 1)
            .map(|(k, e)| (k.clone(), e.value.clone()))
            .collect::<Vec<_>>();
        let more = items.len() > count;
        items.truncate(count);
        (items, more)
    }
}
