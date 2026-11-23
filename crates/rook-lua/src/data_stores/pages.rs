//! Listings behind the data store Pages objects: each keeps a cursor
//! (an exclusive start, as creator-docs `DataStoreKeyPages.Cursor`
//! describes) and reads its next page from the live stores, so a page
//! costs what it returns.

use super::{DataStores, Millis, Scope, StoreName};

/// Default page sizes, for a page size of 0 (creator-docs
/// `ListKeysAsync`, `ListVersionsAsync`, `ListDataStoresAsync`).
pub(crate) const KEYS_PAGE: usize = 50;
pub(crate) const VERSIONS_PAGE: usize = 1024;
pub(crate) const STORES_PAGE: usize = 32;
/// Keys `ListKeysAsync` checks per page when it leaves out deleted keys
/// (creator-docs: "If all checked keys are deleted then it will return an
/// empty list with a cursor to continue iteration").
const KEYS_CHECKED: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Listing {
    /// `OrderedDataStore:GetSortedAsync`.
    Sorted {
        name: String,
        scope: String,
        ascending: bool,
        min: Option<i64>,
        max: Option<i64>,
        size: usize,
        after: Option<(i64, String)>,
    },
    /// `DataStore:ListKeysAsync`; `after` is a stored key.
    Keys {
        store: StoreName,
        scope: Scope,
        prefix: String,
        size: usize,
        exclude_deleted: bool,
        after: Option<String>,
    },
    /// `DataStore:ListVersionsAsync`; `after` is a version's place in its
    /// key's log, which only grows.
    Versions {
        store: StoreName,
        key: String,
        descending: bool,
        min: Option<Millis>,
        max: Option<Millis>,
        size: usize,
        after: Option<usize>,
    },
    /// `DataStoreService:ListDataStoresAsync`.
    Stores {
        prefix: String,
        size: usize,
        after: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Item {
    Entry {
        key: String,
        value: i64,
    },
    Key(String),
    Version {
        id: String,
        created: Millis,
        deleted: bool,
    },
    Store {
        name: String,
        created: Millis,
        updated: Millis,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Page {
    pub(crate) items: Vec<Item>,
    /// No page follows (`Pages.IsFinished`).
    pub(crate) finished: bool,
}

impl Listing {
    /// The method the listing came from, whose budget its pages spend.
    pub(crate) fn method(&self) -> &'static str {
        match self {
            Self::Sorted { .. } => "GetSortedAsync",
            Self::Keys { .. } => "ListKeysAsync",
            Self::Versions { .. } => "ListVersionsAsync",
            Self::Stores { .. } => "ListDataStoresAsync",
        }
    }

    /// Read the next page and move the cursor past it.
    pub(crate) fn next_page(&mut self, stores: &DataStores) -> Page {
        match self {
            Self::Sorted {
                name,
                scope,
                ascending,
                min,
                max,
                size,
                after,
            } => {
                let Some(store) = stores.ordered.get(name, scope) else {
                    return Page::last(Vec::new());
                };
                let (entries, more) = store.sorted(*ascending, *min, *max, after.as_ref(), *size);
                if let Some(last) = entries.last() {
                    *after = Some(last.clone());
                }
                let items = entries
                    .into_iter()
                    .map(|(value, key)| Item::Entry { key, value })
                    .collect();
                Page {
                    items,
                    finished: !more,
                }
            }
            Self::Keys {
                store,
                scope,
                prefix,
                size,
                exclude_deleted,
                after,
            } => keys_page(stores, store, scope, prefix, *size, *exclude_deleted, after),
            Self::Versions {
                store,
                key,
                descending,
                min,
                max,
                size,
                after,
            } => {
                let versions = stores
                    .standard
                    .get(store)
                    .and_then(|s| s.keys.get(key.as_str()))
                    .map_or(&[][..], |k| &k.versions[..]);
                // Times never decrease along the log, so the dates bound a
                // range found by halving.
                let low = min.map_or(0, |m| versions.partition_point(|v| v.created < m));
                let high = max.map_or(versions.len(), |m| {
                    versions.partition_point(|v| v.created <= m)
                });
                let (low, high) = match (*after, *descending) {
                    (Some(a), false) => (low.max(a + 1), high),
                    (Some(a), true) => (low, high.min(a)),
                    (None, _) => (low, high),
                };
                let mut places: Box<dyn Iterator<Item = usize>> = if *descending {
                    Box::new((low..high.max(low)).rev())
                } else {
                    Box::new(low..high.max(low))
                };
                let mut items = Vec::new();
                for i in places.by_ref() {
                    let v = &versions[i];
                    if matches!(v.content, super::standard::Content::Purged) {
                        continue;
                    }
                    items.push(Item::Version {
                        id: v.id.clone(),
                        created: v.created,
                        deleted: matches!(v.content, super::standard::Content::Deleted),
                    });
                    *after = Some(i);
                    if items.len() == *size {
                        break;
                    }
                }
                let more = places
                    .any(|i| !matches!(versions[i].content, super::standard::Content::Purged));
                Page {
                    items,
                    finished: !more,
                }
            }
            Self::Stores {
                prefix,
                size,
                after,
            } => {
                let start = after.clone().unwrap_or_else(|| prefix.clone());
                let mut names = stores
                    .standard
                    .named
                    .range(start..)
                    .filter(|(name, _)| Some(*name) != after.as_ref())
                    .take_while(|(name, _)| name.starts_with(prefix.as_str()))
                    // A store is listed once it holds a key (creator-docs).
                    .filter(|(_, s)| !s.keys.is_empty());
                let items: Vec<_> = names
                    .by_ref()
                    .take(*size)
                    .map(|(name, s)| Item::Store {
                        name: name.clone(),
                        created: s.created,
                        updated: s.updated,
                    })
                    .collect();
                let more = names.next().is_some();
                if let Some(Item::Store { name, .. }) = items.last() {
                    *after = Some(name.clone());
                }
                Page {
                    items,
                    finished: !more,
                }
            }
        }
    }

    /// The cursor `DataStoreKeyPages` and `DataStoreListingPages` show:
    /// where the next page starts, empty once there is none. Rook's
    /// cursors are the last key or store name read, as a script would
    /// pass it back (Approximation: Roblox's are opaque strings).
    pub(crate) fn cursor(&self, finished: bool) -> String {
        if finished {
            return String::new();
        }
        match self {
            Self::Keys { scope, after, .. } => after.as_deref().map(|k| scope.shown(k).to_owned()),
            Self::Stores { after, .. } => after.clone(),
            Self::Sorted { .. } | Self::Versions { .. } => None,
        }
        .unwrap_or_default()
    }
}

impl Page {
    fn last(items: Vec<Item>) -> Self {
        Self {
            items,
            finished: true,
        }
    }
}

fn keys_page(
    stores: &DataStores,
    store: &StoreName,
    scope: &Scope,
    prefix: &str,
    size: usize,
    exclude_deleted: bool,
    after: &mut Option<String>,
) -> Page {
    let Some(s) = stores.standard.get(store) else {
        return Page::last(Vec::new());
    };
    let start = format!("{}{prefix}", scope.prefix());
    let skip = after.clone();
    let from = skip.clone().unwrap_or_else(|| start.clone());
    let mut keys = s
        .keys
        .range(from..)
        .filter(|(k, _)| Some(*k) != skip.as_ref())
        .take_while(|(k, _)| k.starts_with(start.as_str()));
    let mut items = Vec::new();
    let mut checked = 0;
    for (k, key) in keys.by_ref() {
        checked += 1;
        *after = Some(k.clone());
        if !(exclude_deleted && key.is_deleted()) {
            items.push(Item::Key(scope.shown(k).to_owned()));
        }
        if items.len() == size || (exclude_deleted && checked == KEYS_CHECKED) {
            break;
        }
    }
    let more = keys.next().is_some();
    Page {
        items,
        finished: !more,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_stores::standard::Content;
    use serde_json::{Map, Value as Json};

    fn stores() -> DataStores {
        let mut d = DataStores::default();
        let s = d.standard.get_mut(&StoreName::Named("S".into()));
        for (i, k) in ["global/a1", "global/a2", "global/b1", "houses/a3"]
            .into_iter()
            .enumerate()
        {
            s.write(
                k,
                Content::Value(Json::from(1)),
                vec![],
                Map::new(),
                i as i64,
            );
        }
        s.remove("global/a2", 10);
        d
    }

    fn keys(
        store: &DataStores,
        scope: Scope,
        prefix: &str,
        size: usize,
        exclude: bool,
    ) -> Vec<Vec<Item>> {
        let mut listing = Listing::Keys {
            store: StoreName::Named("S".into()),
            scope,
            prefix: prefix.into(),
            size,
            exclude_deleted: exclude,
            after: None,
        };
        let mut pages = Vec::new();
        loop {
            let page = listing.next_page(store);
            pages.push(page.items);
            if page.finished {
                return pages;
            }
        }
    }

    #[test]
    fn keys_list_by_scope_and_prefix_a_page_at_a_time() {
        let d = stores();
        let key = |k: &str| Item::Key(k.into());
        assert_eq!(
            keys(&d, Scope::One("global".into()), "a", 1, false),
            vec![vec![key("a1")], vec![key("a2")]]
        );
        assert_eq!(
            keys(&d, Scope::One("global".into()), "", 10, true),
            vec![vec![key("a1"), key("b1")]]
        );
        assert_eq!(
            keys(&d, Scope::All, "", 3, false),
            vec![
                vec![key("global/a1"), key("global/a2"), key("global/b1")],
                vec![key("houses/a3")]
            ]
        );
    }
}
