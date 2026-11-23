//! What the data store objects scripts hold stand for: store objects
//! (one per name, scope and kind, as Roblox returns the same object for
//! the same request), key infos' user IDs and metadata, options'
//! metadata, and pages' listings.

use rustc_hash::FxHashMap;
use serde_json::{Map, Value as Json};

use rook_dom::InstanceId;

use super::{KeyInfo, Listing, Page, StoreError, StoreId};

/// A Pages object's listing and the page it shows.
#[derive(Debug)]
pub(crate) struct PageState {
    pub(crate) listing: Listing,
    pub(crate) page: Page,
}

#[derive(Debug, Default)]
pub(crate) struct Objects {
    stores: FxHashMap<StoreId, InstanceId>,
    handles: FxHashMap<InstanceId, StoreId>,
    pub(crate) key_infos: FxHashMap<InstanceId, KeyInfo>,
    /// `SetMetadata` on DataStoreSetOptions and DataStoreIncrementOptions:
    /// the metadata, or why it can't be stored, raised when it's used.
    pub(crate) metadata: FxHashMap<InstanceId, Result<Map<String, Json>, StoreError>>,
    pub(crate) pages: FxHashMap<InstanceId, PageState>,
}

impl Objects {
    pub(crate) fn store(&self, id: &StoreId) -> Option<InstanceId> {
        self.stores.get(id).copied()
    }

    pub(crate) fn add_store(&mut self, id: StoreId, object: InstanceId) {
        self.stores.insert(id.clone(), object);
        self.handles.insert(object, id);
    }

    /// What a store object reads and writes.
    pub(crate) fn store_of(&self, object: InstanceId) -> Option<&StoreId> {
        self.handles.get(&object)
    }

    /// Store objects stay for the next request of the same store.
    pub(crate) fn held(&self) -> impl Iterator<Item = InstanceId> + '_ {
        self.stores.values().copied()
    }

    pub(crate) fn forget(&mut self, id: InstanceId) {
        self.key_infos.remove(&id);
        self.metadata.remove(&id);
        self.pages.remove(&id);
    }
}
