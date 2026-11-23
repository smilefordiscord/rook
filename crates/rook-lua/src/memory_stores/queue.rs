//! A queue: items read highest priority first, then in the order they
//! were added, hidden for a while once read until removed
//! (creator-docs MemoryStoreQueue).

use rustc_hash::FxHashMap;
use serde_json::Value as Json;
use std::cmp::Ordering;
use std::collections::BTreeSet;

use super::{Deadlines, MAX_ITEMS, MAX_STRUCTURE_SIZE, MemoryStoreError, Time, value_size};

/// An item's place: higher priority first, then the order of adding.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Place {
    priority: f64,
    seq: u64,
}

impl Eq for Place {}

impl Ord for Place {
    fn cmp(&self, other: &Self) -> Ordering {
        (other.priority.total_cmp(&self.priority)).then(self.seq.cmp(&other.seq))
    }
}

impl PartialOrd for Place {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
struct Item {
    value: Json,
    priority: f64,
    expires: Time,
    /// The read that hid it, and until when.
    hidden: Option<(u64, Time)>,
    size: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Queue {
    items: FxHashMap<u64, Item>,
    /// The items no read is hiding, in reading order.
    visible: BTreeSet<Place>,
    /// When hidden items show again.
    hidden: Deadlines<u64>,
    deadlines: Deadlines<u64>,
    /// The items each read hid, by the id `ReadAsync` returned.
    reads: FxHashMap<u64, Vec<u64>>,
    next_item: u64,
    next_read: u64,
    bytes: usize,
}

impl Queue {
    /// Drop expired items and show again those whose hiding ran out.
    fn update(&mut self, now: Time) {
        for seq in self.deadlines.due(now) {
            self.take(seq, false);
        }
        for seq in self.hidden.due(now) {
            if let Some(item) = self.items.get_mut(&seq) {
                if let Some((read, _)) = item.hidden.take() {
                    forget_read(&mut self.reads, read, seq);
                }
                self.visible.insert(Place {
                    priority: item.priority,
                    seq,
                });
            }
        }
    }

    /// Remove an item, and its deadline unless that was just taken.
    fn take(&mut self, seq: u64, deadline: bool) {
        let Some(item) = self.items.remove(&seq) else {
            return;
        };
        match item.hidden {
            Some((read, until)) => {
                self.hidden.remove(until, seq);
                forget_read(&mut self.reads, read, seq);
            }
            None => {
                self.visible.remove(&Place {
                    priority: item.priority,
                    seq,
                });
            }
        }
        if deadline {
            self.deadlines.remove(item.expires, seq);
        }
        self.bytes -= item.size;
    }

    /// `AddAsync`.
    pub(crate) fn add(
        &mut self,
        now: Time,
        value: Json,
        expiration: f64,
        priority: f64,
    ) -> Result<(), MemoryStoreError> {
        let size = value_size(&value)? + 8;
        self.update(now);
        if self.items.len() >= MAX_ITEMS {
            return Err(MemoryStoreError::TooManyItems);
        }
        if self.bytes + size > MAX_STRUCTURE_SIZE {
            return Err(MemoryStoreError::TooLarge);
        }
        self.next_item += 1;
        let seq = self.next_item;
        let expires = now.after(expiration);
        self.visible.insert(Place { priority, seq });
        self.deadlines.add(expires, seq);
        self.bytes += size;
        let item = Item {
            value,
            priority,
            expires,
            hidden: None,
            size,
        };
        self.items.insert(seq, item);
        Ok(())
    }

    /// `ReadAsync`'s attempt: up to `count` items and the read's id,
    /// hidden for `invisibility` seconds; `None` when there is nothing to
    /// read, or fewer than `count` items and `all_or_nothing`.
    pub(crate) fn read(
        &mut self,
        now: Time,
        count: usize,
        all_or_nothing: bool,
        invisibility: f64,
    ) -> Option<(Vec<Json>, String)> {
        self.update(now);
        if self.visible.is_empty() || (all_or_nothing && self.visible.len() < count) {
            return None;
        }
        self.next_read += 1;
        let read = self.next_read;
        let until = now.after(invisibility);
        let mut values = Vec::with_capacity(count.min(self.visible.len()));
        let mut seqs = Vec::with_capacity(values.capacity());
        while seqs.len() < count {
            let Some(place) = self.visible.pop_first() else {
                break;
            };
            let item = self.items.get_mut(&place.seq).expect("visible items exist");
            item.hidden = Some((read, until));
            self.hidden.add(until, place.seq);
            values.push(item.value.clone());
            seqs.push(place.seq);
        }
        self.reads.insert(read, seqs);
        Some((values, read_id(read)))
    }

    /// `RemoveAsync(id)`: the items that read hid and still hides; an id
    /// whose hiding ran out removes nothing (creator-docs).
    pub(crate) fn remove(&mut self, now: Time, id: &str) {
        self.update(now);
        let Some(seqs) = parse_read_id(id).and_then(|read| self.reads.remove(&read)) else {
            return;
        };
        for seq in seqs {
            self.take(seq, true);
        }
    }

    /// `GetSizeAsync`: every item, or only those no read is hiding.
    pub(crate) fn len(&mut self, now: Time, exclude_invisible: bool) -> usize {
        self.update(now);
        if exclude_invisible {
            self.visible.len()
        } else {
            self.items.len()
        }
    }
}

/// An item is no longer hidden by `read`.
fn forget_read(reads: &mut FxHashMap<u64, Vec<u64>>, read: u64, seq: u64) {
    if let Some(seqs) = reads.get_mut(&read) {
        seqs.retain(|s| *s != seq);
        if seqs.is_empty() {
            reads.remove(&read);
        }
    }
}

/// The string `ReadAsync` returns for a read (Inference: Roblox's is an
/// opaque string that scripts only pass back).
fn read_id(read: u64) -> String {
    format!("{read:016x}")
}

fn parse_read_id(id: &str) -> Option<u64> {
    u64::from_str_radix(id, 16).ok()
}
