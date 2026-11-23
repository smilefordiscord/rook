//! A sorted map: keys with values and optional sort keys, kept in order
//! (creator-docs MemoryStoreSortedMap).

use rustc_hash::FxHashMap;
use serde_json::Value as Json;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::Bound as Edge;

use super::{
    Deadlines, MAX_ITEMS, MAX_KEY_LEN, MAX_STRUCTURE_SIZE, MemoryStoreError, Time, check_key,
    value_size,
};

/// An item's sort key: a number or a string.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SortKey {
    Number(f64),
    String(String),
}

impl SortKey {
    /// A string sort key, checked against the length limit.
    pub(crate) fn string(s: String) -> Result<Self, MemoryStoreError> {
        if s.len() > MAX_KEY_LEN {
            return Err(MemoryStoreError::SortKeyTooLong);
        }
        Ok(Self::String(s))
    }

    /// A number sort key; NaN isn't a number Roblox can sort.
    pub(crate) fn number(n: f64) -> Result<Self, MemoryStoreError> {
        if n.is_nan() {
            return Err(MemoryStoreError::SortKeyType);
        }
        Ok(Self::Number(n))
    }
}

/// Numeric sort keys sort first, then string ones, then items with none
/// (Confirmed: creator-docs MemoryStoreSortedMap).
fn cmp_sort(a: &Option<SortKey>, b: &Option<SortKey>) -> Ordering {
    use SortKey::{Number, String};
    match (a, b) {
        (Some(Number(x)), Some(Number(y))) => x.total_cmp(y),
        (Some(String(x)), Some(String(y))) => x.cmp(y),
        (Some(Number(_)), _) | (Some(String(_)), None) => Ordering::Less,
        (_, Some(Number(_))) | (None, Some(String(_))) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// An item's place in the map: by sort key, then key (Confirmed:
/// creator-docs "Memory store sorted map", "Add or overwrite data").
#[derive(Debug, Clone, PartialEq)]
struct Place {
    sort: Option<SortKey>,
    key: String,
}

impl Eq for Place {}

impl Ord for Place {
    fn cmp(&self, other: &Self) -> Ordering {
        cmp_sort(&self.sort, &other.sort).then_with(|| self.key.cmp(&other.key))
    }
}

impl PartialOrd for Place {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// One end of `GetRangeAsync`'s range, exclusive: `{key, sortKey}` with
/// either or both given. A bound with both is that item's place; with a
/// key alone, the place of an item with that key and no sort key; with a
/// sort key alone, past every item with that sort key (Inference: the
/// creator-docs describe the table, not how a half-given one compares).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Bound {
    pub(crate) key: Option<String>,
    pub(crate) sort: Option<SortKey>,
}

impl Bound {
    /// How `place` compares with the bound: Less when it lies before it.
    fn compare(&self, place: &Place) -> Ordering {
        match &self.key {
            Some(key) => place.cmp(&Place {
                sort: self.sort.clone(),
                key: key.clone(),
            }),
            None => cmp_sort(&place.sort, &self.sort),
        }
    }

    fn is_empty(&self) -> bool {
        self.key.is_none() && self.sort.is_none()
    }
}

#[derive(Debug)]
struct Entry {
    value: Json,
    sort: Option<SortKey>,
    expires: Time,
    size: usize,
}

/// An item `GetRangeAsync` returns.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RangeItem {
    pub(crate) key: String,
    pub(crate) value: Json,
    pub(crate) sort: Option<SortKey>,
}

#[derive(Debug, Default)]
pub(crate) struct SortedMap {
    entries: FxHashMap<String, Entry>,
    order: BTreeSet<Place>,
    deadlines: Deadlines<String>,
    /// Keys, values and sort keys, against the 100 MB limit.
    bytes: usize,
}

impl SortedMap {
    /// Drop the items whose time has come.
    fn expire(&mut self, now: Time) {
        for key in self.deadlines.due(now) {
            self.take(&key, false);
        }
    }

    /// Remove `key`, and its deadline unless that was just taken.
    fn take(&mut self, key: &str, deadline: bool) -> Option<Entry> {
        let entry = self.entries.remove(key)?;
        self.order.remove(&Place {
            sort: entry.sort.clone(),
            key: key.to_owned(),
        });
        if deadline {
            self.deadlines.remove(entry.expires, key.to_owned());
        }
        self.bytes -= entry.size;
        Some(entry)
    }

    /// `GetAsync`: the value and sort key.
    pub(crate) fn get(
        &mut self,
        now: Time,
        key: &str,
    ) -> Result<Option<(Json, Option<SortKey>)>, MemoryStoreError> {
        check_key(key)?;
        self.expire(now);
        Ok(self
            .entries
            .get(key)
            .map(|e| (e.value.clone(), e.sort.clone())))
    }

    /// `SetAsync`: whether the key was already there.
    pub(crate) fn set(
        &mut self,
        now: Time,
        key: &str,
        value: Json,
        expiration: f64,
        sort: Option<SortKey>,
    ) -> Result<bool, MemoryStoreError> {
        check_key(key)?;
        let size = key.len()
            + value_size(&value)?
            + match &sort {
                Some(SortKey::String(s)) => s.len(),
                Some(SortKey::Number(_)) => 8,
                None => 0,
            };
        self.expire(now);
        let old = self.entries.get(key).map(|e| e.size);
        if old.is_none() && self.entries.len() >= MAX_ITEMS {
            return Err(MemoryStoreError::TooManyItems);
        }
        if self.bytes - old.unwrap_or(0) + size > MAX_STRUCTURE_SIZE {
            return Err(MemoryStoreError::TooLarge);
        }
        let existed = self.take(key, true).is_some();
        let expires = now.after(expiration);
        self.order.insert(Place {
            sort: sort.clone(),
            key: key.to_owned(),
        });
        self.deadlines.add(expires, key.to_owned());
        self.bytes += size;
        let entry = Entry {
            value,
            sort,
            expires,
            size,
        };
        self.entries.insert(key.to_owned(), entry);
        Ok(existed)
    }

    /// `RemoveAsync`.
    pub(crate) fn remove(&mut self, now: Time, key: &str) -> Result<(), MemoryStoreError> {
        check_key(key)?;
        self.expire(now);
        self.take(key, true);
        Ok(())
    }

    /// `GetSizeAsync`.
    pub(crate) fn len(&mut self, now: Time) -> usize {
        self.expire(now);
        self.entries.len()
    }

    /// `GetRangeAsync`: up to `count` items strictly between the bounds,
    /// from the low end when `ascending`, else from the high end.
    pub(crate) fn range(
        &mut self,
        now: Time,
        ascending: bool,
        count: usize,
        lower: &Bound,
        upper: &Bound,
    ) -> Vec<RangeItem> {
        self.expire(now);
        let lower = (!lower.is_empty()).then_some(lower);
        let upper = (!upper.is_empty()).then_some(upper);
        let above = |p: &Place| lower.is_none_or(|b| b.compare(p) == Ordering::Greater);
        let below = |p: &Place| upper.is_none_or(|b| b.compare(p) == Ordering::Less);
        let item = |p: &Place| {
            let e = &self.entries[&p.key];
            RangeItem {
                key: p.key.clone(),
                value: e.value.clone(),
                sort: e.sort.clone(),
            }
        };
        // Start the walk at the near bound's place (a sort key alone
        // starts at the first key it could have, "" being the least).
        let start = |b: Option<&Bound>| {
            b.map(|b| Place {
                sort: b.sort.clone(),
                key: b.key.clone().unwrap_or_default(),
            })
        };
        if ascending {
            let from = start(lower).map_or(Edge::Unbounded, Edge::Included);
            (self.order.range((from, Edge::Unbounded)))
                .skip_while(|p| !above(p))
                .take_while(|p| below(p))
                .take(count)
                .map(item)
                .collect()
        } else {
            let to = start(upper).map_or(Edge::Unbounded, Edge::Included);
            (self.order.range((Edge::Unbounded, to)).rev())
                .skip_while(|p| !below(p))
                .take_while(|p| above(p))
                .take(count)
                .map(item)
                .collect()
        }
    }
}
