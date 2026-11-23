//! Standard data stores: each key an append-only log of versions, so
//! older versions stay readable until removed (creator-docs "Versioning,
//! listing, and caching"). A store holds every scope's keys as
//! `scope/key`.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json};

use super::{Millis, StoreName};

/// The named stores, by name, and the global one.
#[derive(Debug, Default)]
pub(crate) struct StandardStores {
    pub(crate) named: BTreeMap<String, Store>,
    global: Store,
}

impl StandardStores {
    pub(crate) fn get(&self, name: &StoreName) -> Option<&Store> {
        match name {
            StoreName::Named(name) => self.named.get(name),
            StoreName::Global => Some(&self.global),
        }
    }

    pub(crate) fn get_mut(&mut self, name: &StoreName) -> &mut Store {
        match name {
            StoreName::Named(name) => self.named.entry(name.clone()).or_default(),
            StoreName::Global => &mut self.global,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Store {
    /// When the store was first and last written (`DataStoreInfo`).
    pub(crate) created: Millis,
    pub(crate) updated: Millis,
    pub(crate) keys: BTreeMap<String, Key>,
}

#[derive(Debug, Default)]
pub(crate) struct Key {
    /// Oldest first; times never decrease, since they follow the game's
    /// clock.
    pub(crate) versions: Vec<Version>,
}

#[derive(Debug)]
pub(crate) struct Version {
    pub(crate) id: String,
    pub(crate) created: Millis,
    pub(crate) content: Content,
    pub(crate) user_ids: Vec<i64>,
    pub(crate) metadata: Map<String, Json>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content {
    Value(Json),
    /// The tombstone `RemoveAsync` writes.
    Deleted,
    /// Gone for good (`RemoveVersionAsync`): listings skip it.
    Purged,
}

/// What a `DataStoreKeyInfo` reports about one version.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KeyInfo {
    /// When the key was first written.
    pub(crate) created: Millis,
    /// When this version was written.
    pub(crate) updated: Millis,
    pub(crate) version: String,
    pub(crate) user_ids: Vec<i64>,
    pub(crate) metadata: Map<String, Json>,
}

/// A version's value and what its key info says.
pub(crate) type Found = (Json, KeyInfo);

impl Key {
    pub(crate) fn is_deleted(&self) -> bool {
        !matches!(
            self.versions.last().map(|v| &v.content),
            Some(Content::Value(_))
        )
    }

    fn info(&self, v: &Version) -> KeyInfo {
        KeyInfo {
            created: self.versions.first().map_or(v.created, |f| f.created),
            updated: v.created,
            version: v.id.clone(),
            user_ids: v.user_ids.clone(),
            metadata: v.metadata.clone(),
        }
    }

    fn found(&self, v: &Version) -> Option<Found> {
        match &v.content {
            Content::Value(value) => Some((value.clone(), self.info(v))),
            Content::Deleted | Content::Purged => None,
        }
    }

    /// Where the version a version string names is. The string carries
    /// the version's number, so finding it costs nothing.
    fn index_of(&self, id: &str) -> Option<usize> {
        let n: usize = id.split('.').nth(1)?.parse().ok()?;
        let i = n.checked_sub(1)?;
        (self.versions.get(i)?.id == id).then_some(i)
    }

    fn by_id(&self, id: &str) -> Option<&Version> {
        self.versions.get(self.index_of(id)?)
    }
}

impl Store {
    pub(crate) fn latest(&self, key: &str) -> Option<Found> {
        let k = self.keys.get(key)?;
        k.found(k.versions.last()?)
    }

    /// Append a version, returning its key info.
    pub(crate) fn write(
        &mut self,
        key: &str,
        content: Content,
        user_ids: Vec<i64>,
        metadata: Map<String, Json>,
        now: Millis,
    ) -> KeyInfo {
        if self.keys.is_empty() {
            self.created = now;
        }
        self.updated = now;
        let k = self.keys.entry(key.to_owned()).or_default();
        k.versions.push(Version {
            id: version_id(now, k.versions.len() + 1),
            created: now,
            content,
            user_ids,
            metadata,
        });
        let v = &k.versions[k.versions.len() - 1];
        k.info(v)
    }

    /// Mark a key deleted with a tombstone, returning the version it
    /// replaced; a key already deleted gets no new version.
    pub(crate) fn remove(&mut self, key: &str, now: Millis) -> Option<Found> {
        let old = self.latest(key)?;
        self.write(key, Content::Deleted, Vec::new(), Map::new(), now);
        Some(old)
    }

    pub(crate) fn version(&self, key: &str, id: &str) -> Option<Found> {
        let k = self.keys.get(key)?;
        k.found(k.by_id(id)?)
    }

    /// The version that was current at `time`.
    pub(crate) fn version_at(&self, key: &str, time: Millis) -> Option<Found> {
        let k = self.keys.get(key)?;
        let after = k.versions.partition_point(|v| v.created <= time);
        k.found(k.versions.get(after.checked_sub(1)?)?)
    }

    /// Remove a version for good. Inference: a version string that names
    /// no version does nothing.
    pub(crate) fn purge(&mut self, key: &str, id: &str) {
        let Some(k) = self.keys.get_mut(key) else {
            return;
        };
        if let Some(i) = k.index_of(id) {
            let v = &mut k.versions[i];
            v.content = Content::Purged;
            v.user_ids.clear();
            v.metadata.clear();
        }
    }
}

/// A version string, in the shape Roblox's take (creator-docs samples):
/// the time as .NET ticks in hex, the version's number in the key, the
/// ticks again and `01`. Approximation: what Roblox's parts mean beyond
/// that shape is unknown, so only their order matters.
fn version_id(now: Millis, n: usize) -> String {
    const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
    let ticks = UNIX_EPOCH_TICKS + now * 10_000;
    format!("{ticks:016X}.{n:010}.{ticks:016X}.01")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(n: i64) -> Content {
        Content::Value(Json::from(n))
    }

    #[test]
    fn versions_are_found_by_string_and_time() {
        let mut s = Store::default();
        let a = s.write("k", value(1), vec![], Map::new(), 1_000).version;
        let b = s.write("k", value(2), vec![7], Map::new(), 2_000).version;
        assert_eq!(s.version("k", &a).map(|f| f.0), Some(Json::from(1)));
        assert_eq!(s.version("k", &b).map(|f| f.1.user_ids), Some(vec![7]));
        assert_eq!(s.version_at("k", 1_500).map(|f| f.0), Some(Json::from(1)));
        assert_eq!(s.version_at("k", 999), None);
        let info = s.latest("k").map(|f| f.1);
        assert_eq!(
            info.as_ref().map(|i| (i.created, i.updated)),
            Some((1_000, 2_000))
        );
    }

    #[test]
    fn removing_keeps_older_versions() {
        let mut s = Store::default();
        let a = s.write("k", value(1), vec![], Map::new(), 1_000).version;
        assert!(s.remove("k", 2_000).is_some());
        assert!(s.remove("k", 3_000).is_none());
        assert_eq!(s.latest("k"), None);
        assert_eq!(s.keys["k"].versions.len(), 2);
        assert_eq!(s.version("k", &a).map(|f| f.0), Some(Json::from(1)));
        s.purge("k", &a);
        assert_eq!(s.version("k", &a), None);
    }
}
