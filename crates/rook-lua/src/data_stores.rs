//! DataStoreService's side of Roblox's servers, for one server's run: the
//! stores' data, the server's request budgets and read cache, and what the
//! objects scripts hold stand for. Methods are in `api::methods::data_stores`;
//! findings are in docs/behavior/scripting/data-stores.md.
//!
//! Each standard key is an append-only log of versions: a write appends, a
//! remove appends a tombstone, and an old version is found by number or by
//! time without a scan. Ordered stores keep a value index beside their keys,
//! and every listing is a cursor (an exclusive start) read again from the
//! live data, so a request costs what it touches. Values are kept as the
//! JSON they serialize to, as Roblox stores and measures them, in plain Rust
//! types so the data can be saved between runs without the Luau state.

mod budget;
mod cache;
mod errors;
mod objects;
mod ordered;
mod pages;
mod standard;
mod values;

pub(crate) use budget::{Admission, Cost, Pool, Queue, Reported, configurable, reported};
pub(crate) use errors::StoreError;
pub(crate) use objects::{Objects, PageState};
pub(crate) use ordered::OrderedStores;
pub(crate) use pages::{Item, KEYS_PAGE, Listing, Page, STORES_PAGE, VERSIONS_PAGE};
pub(crate) use standard::{Content, Found, KeyInfo, StandardStores};
pub(crate) use values::{
    check_key, check_name, check_scope, metadata_of, ordered_value, stored_value, user_ids_of,
};

/// Milliseconds since the Unix epoch, the unit of every time data stores
/// report (creator-docs `DataStoreKeyInfo.CreatedTime`).
pub(crate) type Millis = i64;

/// Which standard store: a named one, or the one
/// `DataStoreService:GetGlobalDataStore` returns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StoreName {
    Named(String),
    Global,
}

/// The scope a store object reads in. Roblox keeps every scope's keys in
/// one store as `scope/key`, the form an `AllScopes` store reads and
/// lists them in (creator-docs `DataStoreOptions.AllScopes`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Scope {
    One(String),
    All,
}

impl Scope {
    /// The stored key for the key a script passed, checked against the
    /// key limits.
    pub(crate) fn full_key(&self, key: &str) -> Result<String, StoreError> {
        match self {
            Self::One(scope) => {
                values::check_key(key)?;
                Ok(format!("{scope}/{key}"))
            }
            Self::All => {
                let (scope, rest) = key.split_once('/').ok_or(StoreError::NoScopeInKey)?;
                if scope.is_empty() {
                    return Err(StoreError::NoScopeInKey);
                }
                values::check_key(rest)?;
                Ok(key.to_owned())
            }
        }
    }

    /// Where this scope's keys start among the stored keys.
    pub(crate) fn prefix(&self) -> String {
        match self {
            Self::One(scope) => format!("{scope}/"),
            Self::All => String::new(),
        }
    }

    /// A stored key as this scope shows it to scripts.
    pub(crate) fn shown<'a>(&self, full: &'a str) -> &'a str {
        match self {
            Self::One(scope) => full
                .strip_prefix(scope.as_str())
                .and_then(|k| k.strip_prefix('/'))
                .unwrap_or(full),
            Self::All => full,
        }
    }
}

/// What a store object scripts hold reads and writes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StoreId {
    Standard { name: StoreName, scope: Scope },
    Ordered { name: String, scope: String },
}

/// Everything DataStoreService keeps for one server.
#[derive(Debug, Default)]
pub(crate) struct DataStores {
    pub(crate) standard: StandardStores,
    pub(crate) ordered: OrderedStores,
    pub(crate) budgets: budget::Budgets,
    pub(crate) cache: cache::ReadCache,
    pub(crate) objects: Objects,
}
