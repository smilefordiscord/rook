//! The server's read cache: creator-docs GlobalDataStore:GetAsync says
//! keys are cached for 4 seconds after the first read, a read within them
//! comes from the cache, and `SetAsync` and `UpdateAsync` write through it
//! and restart the 4 seconds. One server's stores hold nothing another
//! server changed, so a cached read is always the stored value; what the
//! cache changes is that such a read sends no request, so it spends no
//! budget (Inference from "without a backend request",
//! creator-docs DataStoreGetOptions.UseCache).

use std::collections::VecDeque;

use rustc_hash::FxHashMap;

/// How long a read or write keeps a key cached, in seconds.
const CACHE_SECONDS: f64 = 4.0;

#[derive(Debug, Default)]
pub(crate) struct ReadCache {
    /// When each cached key expires, in game seconds.
    expires: FxHashMap<String, f64>,
    /// Keys in the order they were cached, to forget expired ones in
    /// the time it takes to reach them; a key cached again has a later
    /// entry, and an entry older than its key's time is skipped.
    order: VecDeque<(f64, String)>,
}

impl ReadCache {
    pub(crate) fn is_fresh(&mut self, key: &str, now: f64) -> bool {
        self.forget_expired(now);
        self.expires.contains_key(key)
    }

    /// Cache a key from now.
    pub(crate) fn touch(&mut self, key: &str, now: f64) {
        self.forget_expired(now);
        let until = now + CACHE_SECONDS;
        self.expires.insert(key.to_owned(), until);
        self.order.push_back((until, key.to_owned()));
    }

    fn forget_expired(&mut self, now: f64) {
        while let Some((until, _)) = self.order.front()
            && *until <= now
        {
            let Some((until, key)) = self.order.pop_front() else {
                break;
            };
            if self.expires.get(&key) == Some(&until) {
                self.expires.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_stay_cached_for_four_seconds_from_the_last_touch() {
        let mut c = ReadCache::default();
        c.touch("k", 0.0);
        assert!(c.is_fresh("k", 3.9));
        c.touch("k", 3.0);
        assert!(c.is_fresh("k", 6.9));
        assert!(!c.is_fresh("k", 7.0));
    }
}
