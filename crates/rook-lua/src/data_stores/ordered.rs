//! Ordered data stores: integer values by key, without versions or
//! metadata (creator-docs OrderedDataStore), with an index by value so a
//! sorted page reads only its own entries.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use rustc_hash::FxHashMap;

/// The stores, by name and scope.
#[derive(Debug, Default)]
pub(crate) struct OrderedStores {
    stores: FxHashMap<(String, String), Ordered>,
}

impl OrderedStores {
    pub(crate) fn get(&self, name: &str, scope: &str) -> Option<&Ordered> {
        self.stores.get(&(name.to_owned(), scope.to_owned()))
    }

    pub(crate) fn get_mut(&mut self, name: &str, scope: &str) -> &mut Ordered {
        self.stores
            .entry((name.to_owned(), scope.to_owned()))
            .or_default()
    }
}

#[derive(Debug, Default)]
pub(crate) struct Ordered {
    values: BTreeMap<String, i64>,
    /// Every entry as (value, key), the order sorted pages read in.
    /// Inference: entries with equal values come in key order, reversed
    /// when descending.
    by_value: BTreeSet<(i64, String)>,
}

impl Ordered {
    pub(crate) fn get(&self, key: &str) -> Option<i64> {
        self.values.get(key).copied()
    }

    pub(crate) fn set(&mut self, key: &str, value: i64) {
        if let Some(old) = self.values.insert(key.to_owned(), value) {
            self.by_value.remove(&(old, key.to_owned()));
        }
        self.by_value.insert((value, key.to_owned()));
    }

    /// Delete a key for good (no versions), returning its value.
    pub(crate) fn remove(&mut self, key: &str) -> Option<i64> {
        let old = self.values.remove(key)?;
        self.by_value.remove(&(old, key.to_owned()));
        Some(old)
    }

    /// Up to `size` entries with values from `min` to `max`, after the
    /// entry `after` in the order asked for, and whether more follow.
    pub(crate) fn sorted(
        &self,
        ascending: bool,
        min: Option<i64>,
        max: Option<i64>,
        after: Option<&(i64, String)>,
        size: usize,
    ) -> (Vec<(i64, String)>, bool) {
        let low = min.map_or(Bound::Unbounded, |m| Bound::Included((m, String::new())));
        let high = match max {
            Some(m) if m < i64::MAX => Bound::Excluded((m + 1, String::new())),
            _ => Bound::Unbounded,
        };
        let (low, high) = match (after, ascending) {
            (Some(a), true) => (Bound::Excluded(a.clone()), high),
            (Some(a), false) => (low, Bound::Excluded(a.clone())),
            (None, _) => (low, high),
        };
        if is_empty(&low, &high) {
            return (Vec::new(), false);
        }
        let range = self.by_value.range((low, high));
        let mut entries: Box<dyn Iterator<Item = &(i64, String)>> = if ascending {
            Box::new(range)
        } else {
            Box::new(range.rev())
        };
        let page: Vec<_> = entries.by_ref().take(size).cloned().collect();
        let more = entries.next().is_some();
        (page, more)
    }
}

/// Whether a range holds nothing, which `range` would panic on when
/// crossed (min above max, a cursor past the end).
fn is_empty<T: Ord>(low: &Bound<T>, high: &Bound<T>) -> bool {
    match (low, high) {
        (Bound::Included(l), Bound::Included(h)) => l > h,
        (Bound::Included(l) | Bound::Excluded(l), Bound::Included(h) | Bound::Excluded(h)) => {
            l >= h
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Ordered {
        let mut s = Ordered::default();
        for (k, v) in [("a", 5), ("b", 1), ("c", 9), ("d", 5)] {
            s.set(k, v);
        }
        s
    }

    fn keys(page: &[(i64, String)]) -> Vec<&str> {
        page.iter().map(|(_, k)| k.as_str()).collect()
    }

    #[test]
    fn pages_follow_their_cursor() {
        let s = store();
        let (first, more) = s.sorted(false, None, None, None, 2);
        assert_eq!((keys(&first), more), (vec!["c", "d"], true));
        let (second, more) = s.sorted(false, None, None, first.last(), 2);
        assert_eq!((keys(&second), more), (vec!["a", "b"], false));
        let (asc, more) = s.sorted(true, Some(2), Some(5), None, 10);
        assert_eq!((keys(&asc), more), (vec!["a", "d"], false));
    }

    #[test]
    fn setting_again_moves_the_entry() {
        let mut s = store();
        s.set("b", 10);
        let (page, _) = s.sorted(false, None, None, None, 1);
        assert_eq!(keys(&page), vec!["b"]);
        assert_eq!(s.remove("b"), Some(10));
        let (page, _) = s.sorted(false, None, None, None, 1);
        assert_eq!(keys(&page), vec!["c"]);
    }
}
