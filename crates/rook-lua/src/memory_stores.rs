//! MemoryStoreService's sorted maps, queues and hash maps for one
//! session, with Roblox's per-structure limits
//! (`docs/behavior/scripting/memory-stores.md`); scripts reach them
//! through `api/methods/web/memory_stores.rs`.
//!
//! Roblox keeps them on its servers for every server of a game; a Rook
//! session is one server, so they live here, in memory, for the run
//! (Approximation). Values are kept as the JSON Roblox turns them into.
//!
//! Items expire on the session's clock (`time()`). Nothing looks for that
//! each frame: each structure keeps its deadlines in order, and an
//! operation on it first drops the items whose time has passed, so the
//! cost follows what expires, never what is stored (Redis expires keys
//! the same way, on access).

mod hash_map;
mod queue;
mod sorted_map;
#[cfg(test)]
mod tests;

pub(crate) use hash_map::HashMap;
pub(crate) use queue::Queue;
pub(crate) use sorted_map::{Bound, SortKey, SortedMap};

use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::BTreeSet;

use rook_dom::InstanceId;

/// The longest an item may live, in seconds: 45 days (creator-docs
/// MemoryStoreSortedMap:SetAsync).
pub(crate) const MAX_EXPIRATION: i64 = 3_888_000;
/// The longest key and sort key, in bytes (creator-docs "Memory store
/// sorted map" and "hash map" limits: 128 characters).
pub(crate) const MAX_KEY_LEN: usize = 128;
/// The largest value, as JSON (creator-docs: 32 KB).
pub(crate) const MAX_VALUE_SIZE: usize = 32 * 1024;
/// A sorted map's or queue's most items and bytes, keys included
/// (creator-docs "Data structure size limits").
pub(crate) const MAX_ITEMS: usize = 1_000_000;
pub(crate) const MAX_STRUCTURE_SIZE: usize = 100 * 1024 * 1024;
/// The most items one `GetRangeAsync` returns, one `ReadAsync` reads and
/// one `ListItemsAsync` page holds (creator-docs).
pub(crate) const MAX_RANGE_COUNT: i64 = 200;
pub(crate) const MAX_READ_COUNT: i64 = 100;
pub(crate) const MAX_PAGE_COUNT: i64 = 200;

/// What a memory store call fails with: the status code creator-docs
/// "Memory stores" lists, then its description there (Inference: how
/// Roblox words the error a script sees isn't documented).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum MemoryStoreError {
    #[error("InvalidClientAccess: MemoryStoreService must be called from server.")]
    Client,
    #[error("InvalidExpirationTime: The field 'expiration' time must be between 0 and 3,888,000.")]
    Expiration,
    #[error("InvalidRequest: The key must not be empty.")]
    EmptyKey,
    #[error("InvalidRequest: The key must be at most 128 characters.")]
    KeyTooLong,
    #[error("InvalidRequest: Unable to convert value to json.")]
    NotJson,
    #[error("InvalidRequest: Unable to convert sortKey to a valid number or string.")]
    SortKeyType,
    #[error("InvalidRequest: The sort key must be at most 128 characters.")]
    SortKeyTooLong,
    #[error("InvalidRequest: The field 'count' must be between 1 and {0}.")]
    Count(i64),
    #[error("ItemValueSizeTooLarge: Value size exceeds limit (32 KB).")]
    ValueTooLarge,
    #[error("DataStructureItemsOverLimit: Exceeds data structure level item count limit (1M).")]
    TooManyItems,
    #[error(
        "DataStructureMemoryOverLimit: Exceeds data structure level memory size limit (100 MB)."
    )]
    TooLarge,
    #[error("TransformCallbackFailed: Failed to invoke transformation callback function.")]
    Transform,
    /// A structure's method called on an object that isn't that kind of
    /// structure (Inference: the wording is Rook's).
    #[error("This MemoryStore object is not available")]
    Unavailable,
}

/// Seconds on the session's clock. Ordered by `total_cmp`: the clock is
/// never NaN.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Time(pub(crate) f64);

impl Eq for Time {}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Time {
    /// `seconds` from now.
    pub(crate) fn after(self, seconds: f64) -> Self {
        Self(self.0 + seconds)
    }
}

/// When each item of a structure expires, earliest first. An item is gone
/// once the clock reaches its deadline (Inference: so an expiration of 0
/// is gone at once).
#[derive(Debug)]
pub(crate) struct Deadlines<K> {
    set: BTreeSet<(Time, K)>,
}

impl<K: Ord> Default for Deadlines<K> {
    fn default() -> Self {
        Self {
            set: BTreeSet::new(),
        }
    }
}

impl<K: Ord + Clone> Deadlines<K> {
    pub(crate) fn add(&mut self, at: Time, key: K) {
        self.set.insert((at, key));
    }

    pub(crate) fn remove(&mut self, at: Time, key: K) {
        self.set.remove(&(at, key));
    }

    /// Take the keys whose deadline is `now` or earlier.
    pub(crate) fn due(&mut self, now: Time) -> Vec<K> {
        let mut out = Vec::new();
        while self.set.first().is_some_and(|(at, _)| *at <= now) {
            if let Some((_, key)) = self.set.pop_first() {
                out.push(key);
            }
        }
        out
    }
}

/// An expiration a script gave, in whole seconds as Roblox's `int64`
/// takes it, checked against Roblox's range.
pub(crate) fn expiration(seconds: f64) -> Result<f64, MemoryStoreError> {
    let whole = seconds.trunc();
    if !(0.0..=MAX_EXPIRATION as f64).contains(&whole) {
        return Err(MemoryStoreError::Expiration);
    }
    Ok(whole)
}

/// A sorted map's or hash map's key, checked.
pub(crate) fn check_key(key: &str) -> Result<(), MemoryStoreError> {
    if key.is_empty() {
        return Err(MemoryStoreError::EmptyKey);
    }
    if key.len() > MAX_KEY_LEN {
        return Err(MemoryStoreError::KeyTooLong);
    }
    Ok(())
}

/// A value's size as stored (its JSON), checked against the limit.
pub(crate) fn value_size(value: &serde_json::Value) -> Result<usize, MemoryStoreError> {
    let size = value.to_string().len();
    if size > MAX_VALUE_SIZE {
        return Err(MemoryStoreError::ValueTooLarge);
    }
    Ok(size)
}

/// A count a script gave, checked to be from 1 to `max`.
pub(crate) fn count(n: f64, max: i64) -> Result<usize, MemoryStoreError> {
    let whole = n.trunc();
    if !(1.0..=max as f64).contains(&whole) {
        return Err(MemoryStoreError::Count(max));
    }
    Ok(whole as usize)
}

/// What one object a script got from MemoryStoreService stands for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Handle {
    SortedMap(String),
    /// A queue read through this object hides what it reads for
    /// `invisibility` seconds (`GetQueue`'s second argument).
    Queue {
        name: String,
        invisibility: f64,
    },
    HashMap(String),
}

/// Every structure in the session, by name: the same name reaches the
/// same structure from any script (creator-docs MemoryStoreService).
#[derive(Debug, Default)]
pub(crate) struct MemoryStores {
    pub(crate) sorted_maps: FxHashMap<String, SortedMap>,
    pub(crate) queues: FxHashMap<String, Queue>,
    pub(crate) hash_maps: FxHashMap<String, HashMap>,
    /// The structure each object scripts were given stands for.
    pub(crate) objects: FxHashMap<InstanceId, Handle>,
    reads: u64,
}

impl MemoryStores {
    pub(crate) fn sorted_map(&mut self, name: &str) -> &mut SortedMap {
        self.sorted_maps.entry(name.to_owned()).or_default()
    }

    /// A number for a queue read's id, unique in the session, so that one
    /// queue's id passed to another's `RemoveAsync` removes nothing.
    pub(crate) fn next_read(&mut self) -> u64 {
        self.reads += 1;
        self.reads
    }

    pub(crate) fn queue(&mut self, name: &str) -> &mut Queue {
        self.queues.entry(name.to_owned()).or_default()
    }

    pub(crate) fn hash_map(&mut self, name: &str) -> &mut HashMap {
        self.hash_maps.entry(name.to_owned()).or_default()
    }
}
