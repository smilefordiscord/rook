//! Standard data stores: each key a log of versions, at most one for each
//! UTC hour, so older versions stay readable for 30 days after a newer one
//! replaces them (creator-docs "Versioning, listing, and caching"). Expired
//! versions leave the front of the log when the key is next written, and
//! reads skip any still there, so a key holds about a month of hours and
//! a write costs what it drops. A store holds every scope's keys as
//! `scope/key`.

use std::collections::{BTreeMap, VecDeque};

use serde_json::{Map, Value as Json};

use super::{Millis, StoreName};

const HOUR: Millis = 60 * 60 * 1000;

/// How long a version stays after a newer one replaces it (creator-docs:
/// "Versioned backups expire 30 days after a new write overwrites them").
const RETENTION: Millis = 30 * 24 * HOUR;

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
    /// When the key was first written, which outlives its first version.
    created: Millis,
    /// Versions expired and dropped from the front of `versions`: the
    /// version numbered `n` sits at `n - 1 - dropped`.
    dropped: usize,
    /// Oldest first; times never decrease, since they follow the game's
    /// clock.
    versions: VecDeque<Version>,
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
            self.versions.back().map(|v| &v.content),
            Some(Content::Value(_))
        )
    }

    fn info(&self, v: &Version) -> KeyInfo {
        KeyInfo {
            created: self.created,
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

    /// Where in `versions` the first version not yet expired at `now` is.
    /// A version expires once the one after it is `RETENTION` old; the
    /// latest never does (creator-docs).
    fn live(&self, now: Millis) -> usize {
        let replaced = (self.versions).partition_point(|v| v.created <= now - RETENTION);
        replaced.saturating_sub(1)
    }

    /// Drop the versions expired at `now`.
    fn expire(&mut self, now: Millis) {
        let n = self.live(now);
        self.versions.drain(..n);
        self.dropped += n;
    }

    /// Where the version a version string names is, if it hasn't expired
    /// at `now`. The string carries the version's number, so finding it
    /// costs nothing.
    fn index_of(&self, id: &str, now: Millis) -> Option<usize> {
        let n: usize = id.split('.').nth(1)?.parse().ok()?;
        let i = n.checked_sub(1 + self.dropped)?;
        (i >= self.live(now) && self.versions.get(i)?.id == id).then_some(i)
    }

    /// The versions a listing may show at `now`, written from `min` to
    /// `max` (inclusive), as places in the whole log: a place stays the
    /// same version while older ones expire.
    pub(crate) fn places(
        &self,
        min: Option<Millis>,
        max: Option<Millis>,
        now: Millis,
    ) -> std::ops::Range<usize> {
        let low = min.map_or(0, |m| self.versions.partition_point(|v| v.created < m));
        let high = max.map_or(self.versions.len(), |m| {
            self.versions.partition_point(|v| v.created <= m)
        });
        let low = low.max(self.live(now));
        self.dropped + low..self.dropped + high.max(low)
    }

    /// The version at a place [`Key::places`] gave.
    pub(crate) fn at(&self, place: usize) -> &Version {
        &self.versions[place - self.dropped]
    }
}

impl Store {
    pub(crate) fn latest(&self, key: &str) -> Option<Found> {
        let k = self.keys.get(key)?;
        k.found(k.versions.back()?)
    }

    /// Write a version, returning its key info. The key's first write in a
    /// UTC hour makes a version, and later ones in that hour overwrite it
    /// (creator-docs); a tombstone is always a version of its own, keeping
    /// the value before it (creator-docs `RemoveAsync`).
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
        if k.versions.is_empty() {
            k.created = now;
        }
        let count = k.dropped + k.versions.len();
        let same_hour = k.versions.back().is_some_and(|last| {
            matches!(
                (&last.content, &content),
                (Content::Value(_), Content::Value(_))
            ) && last.created.div_euclid(HOUR) == now.div_euclid(HOUR)
        });
        let version = Version {
            // Inference: the hour's version takes a new string with the
            // same number, so the overwritten data's string finds nothing.
            id: version_id(now, if same_hour { count } else { count + 1 }),
            created: now,
            content,
            user_ids,
            metadata,
        };
        if same_hour {
            k.versions.pop_back();
        }
        k.versions.push_back(version);
        k.expire(now);
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

    pub(crate) fn version(&self, key: &str, id: &str, now: Millis) -> Option<Found> {
        let k = self.keys.get(key)?;
        k.found(&k.versions[k.index_of(id, now)?])
    }

    /// The version that was current at `time`, if it hasn't expired at
    /// `now`.
    pub(crate) fn version_at(&self, key: &str, time: Millis, now: Millis) -> Option<Found> {
        let k = self.keys.get(key)?;
        let i = k
            .versions
            .partition_point(|v| v.created <= time)
            .checked_sub(1)?;
        if i < k.live(now) {
            return None;
        }
        k.found(&k.versions[i])
    }

    /// Remove a version for good. Inference: a version string that names
    /// no version does nothing.
    pub(crate) fn purge(&mut self, key: &str, id: &str, now: Millis) {
        let Some(k) = self.keys.get_mut(key) else {
            return;
        };
        if let Some(i) = k.index_of(id, now) {
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

    fn write(s: &mut Store, n: i64, now: Millis) -> String {
        s.write("k", value(n), vec![], Map::new(), now).version
    }

    #[test]
    fn versions_are_found_by_string_and_time() {
        let mut s = Store::default();
        let now = 2 * HOUR;
        let a = write(&mut s, 1, 1_000);
        let b = s.write("k", value(2), vec![7], Map::new(), HOUR).version;
        assert_eq!(s.version("k", &a, now).map(|f| f.0), Some(Json::from(1)));
        assert_eq!(s.version("k", &b, now).map(|f| f.1.user_ids), Some(vec![7]));
        assert_eq!(
            s.version_at("k", 1_500, now).map(|f| f.0),
            Some(Json::from(1))
        );
        assert_eq!(s.version_at("k", 999, now), None);
        let info = s.latest("k").map(|f| f.1);
        assert_eq!(
            info.as_ref().map(|i| (i.created, i.updated)),
            Some((1_000, HOUR))
        );
    }

    #[test]
    fn writes_in_one_utc_hour_make_one_version() {
        let mut s = Store::default();
        let first = write(&mut s, 1, 5 * HOUR);
        write(&mut s, 2, 5 * HOUR + 1_000);
        let last = write(&mut s, 3, 6 * HOUR - 1);
        let k = &s.keys["k"];
        assert_eq!(k.versions.len(), 1);
        // The overwritten data is gone, its string with it.
        assert_eq!(s.version("k", &first, 6 * HOUR), None);
        assert_eq!(
            s.version("k", &last, 6 * HOUR).map(|f| f.0),
            Some(Json::from(3))
        );
        assert_eq!(first.split('.').nth(1), last.split('.').nth(1));
        let info = s.latest("k").map(|f| f.1);
        assert_eq!(
            info.map(|i| (i.created, i.updated)),
            Some((5 * HOUR, 6 * HOUR - 1))
        );
    }

    #[test]
    fn writes_across_utc_hours_make_a_version_each() {
        let mut s = Store::default();
        let ids: Vec<_> = (0..4)
            .map(|h| write(&mut s, h, h * HOUR + 30_000))
            .collect();
        write(&mut s, 9, 3 * HOUR + 40_000);
        assert_eq!(s.keys["k"].versions.len(), 4);
        let now = 4 * HOUR;
        let read: Vec<_> = ids
            .iter()
            .map(|id| s.version("k", id, now).map(|f| f.0))
            .collect();
        assert_eq!(read[..3], [0, 1, 2].map(|n| Some(Json::from(n))));
        assert_eq!(read[3], None);
    }

    #[test]
    fn versions_expire_30_days_after_being_replaced() {
        let mut s = Store::default();
        let a = write(&mut s, 1, 0);
        let b = write(&mut s, 2, HOUR);
        assert!(s.version("k", &a, HOUR + RETENTION - 1).is_some());
        assert_eq!(s.version("k", &a, HOUR + RETENTION), None);
        assert_eq!(s.version_at("k", 10, HOUR + RETENTION), None);
        // The latest never expires.
        let later = 100 * RETENTION;
        assert_eq!(s.version("k", &b, later).map(|f| f.0), Some(Json::from(2)));
        assert_eq!(
            s.version_at("k", later, later).map(|f| f.0),
            Some(Json::from(2))
        );
        let c = write(&mut s, 3, later);
        let k = &s.keys["k"];
        assert_eq!((k.versions.len(), k.dropped), (2, 1));
        assert_eq!(c.split('.').nth(1), Some("0000000003"));
        assert_eq!(k.places(None, None, later), 1..3);
        assert_eq!(s.version("k", &b, later).map(|f| f.0), Some(Json::from(2)));
        assert_eq!(s.latest("k").map(|f| f.1.created), Some(0));
    }

    #[test]
    fn removing_keeps_older_versions() {
        let mut s = Store::default();
        let a = write(&mut s, 1, 1_000);
        // A tombstone is a version of its own, in the same hour too.
        assert!(s.remove("k", 2_000).is_some());
        assert!(s.remove("k", 3_000).is_none());
        assert_eq!(s.latest("k"), None);
        assert_eq!(s.keys["k"].versions.len(), 2);
        assert_eq!(s.version("k", &a, 3_000).map(|f| f.0), Some(Json::from(1)));
        s.purge("k", &a, 3_000);
        assert_eq!(s.version("k", &a, 3_000), None);
    }
}
