//! The data store Pages objects: `DataStorePages`, `DataStoreKeyPages`,
//! `DataStoreVersionPages` and `DataStoreListingPages`, each a listing
//! read a page at a time. `Pages:AdvanceToNextPageAsync` is a request of
//! the endpoint the pages came from (creator-docs Pages: "The request
//! limit is the same of the endpoint originally called").

use mlua::{Lua, Table};
use rook_dom::InstanceId;
use rook_types::Value;

use super::{RawMethod, clock};
use crate::api::instance::push_instance;
use crate::data_stores::{Item, Listing, PageState, StoreError};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::world::World;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Pages", "GetCurrentPage") => current_page,
        _ => return None,
    })
}

/// A page size argument: 0 or none is the endpoint's default.
pub(super) fn page_size(a: &Args, i: usize, default: usize) -> ApiResult<usize> {
    let n = a.number_or(i, 0.0)?;
    Ok(if n >= 1.0 { n as usize } else { default })
}

/// A Pages object of `class` showing the listing's first page.
pub(super) fn open(lua: &Lua, class: &str, mut listing: Listing) -> ApiResult<Outcome> {
    let id: ApiResult<InstanceId> = with_world(lua, |w| {
        let id = w.dom.create_by_name(class)?;
        let page = listing.next_page(&w.data_stores, clock(w).1);
        show(w, id, &listing, page.finished)?;
        (w.data_stores.objects.pages).insert(id, PageState { listing, page });
        Ok(id)
    });
    Outcome::ret(lua, push_instance(lua, id?)?)
}

/// What a Pages object shows of where its listing is.
fn show(w: &mut World, id: InstanceId, listing: &Listing, finished: bool) -> ApiResult<()> {
    w.dom
        .set_property_by_name(id, "IsFinished", Value::Bool(finished))?;
    // Only key and listing pages have a cursor; the others ignore it.
    w.dom
        .set_property_by_name(id, "Cursor", Value::String(listing.cursor(finished)))?;
    Ok(())
}

/// The listing behind a data store Pages object.
pub(super) fn listing_of(lua: &Lua, this: InstanceId) -> ApiResult<(Listing, bool)> {
    with_world(lua, |w| {
        let state = w.data_stores.objects.pages.get(&this);
        match state {
            Some(s) => Ok((s.listing.clone(), s.page.finished)),
            None => Err(ScriptError(format!(
                "{}:AdvanceToNextPageAsync is not implemented in Rook yet",
                w.dom.class_name(this)
            ))),
        }
    })
}

/// `AdvanceToNextPageAsync`, once its request is let through.
pub(super) fn advance(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    with_world(lua, |w| {
        let Some(mut state) = w.data_stores.objects.pages.remove(&this) else {
            return Err(StoreError::NoMorePages.into());
        };
        if state.page.finished {
            w.data_stores.objects.pages.insert(this, state);
            return Err(StoreError::NoMorePages.into());
        }
        state.page = state.listing.next_page(&w.data_stores, clock(w).1);
        let shown = show(w, this, &state.listing, state.page.finished);
        w.data_stores.objects.pages.insert(this, state);
        shown
    })?;
    Outcome::ret(lua, ())
}

/// `GetCurrentPage()`: a new table of the page's items each call.
fn current_page(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let items = with_world(lua, |w| {
        (w.data_stores.objects.pages.get(&this)).map(|s| s.page.items.clone())
    });
    let Some(items) = items else {
        let class = with_world(lua, |w| w.dom.class_name(this).to_owned());
        return Err(ScriptError(format!(
            "{class}:GetCurrentPage is not implemented in Rook yet"
        )));
    };
    let page = lua.create_table_with_capacity(items.len(), 0)?;
    for (i, item) in items.into_iter().enumerate() {
        match shown(item) {
            Shown::Entry(key, value) => {
                let entry: Table = lua.create_table_with_capacity(0, 2)?;
                entry.raw_set("key", key)?;
                entry.raw_set("value", value)?;
                page.raw_set(i + 1, entry)?;
            }
            Shown::Object(class, properties) => {
                let id: ApiResult<InstanceId> = with_world(lua, |w| {
                    let id = w.dom.create_by_name(class)?;
                    for (name, value) in properties {
                        w.dom.set_property_by_name(id, name, value)?;
                    }
                    Ok(id)
                });
                page.raw_set(i + 1, push_instance(lua, id?)?)?;
            }
        }
    }
    Outcome::ret(lua, page)
}

/// How a page shows an item: an ordered store's entry as a table of
/// `key` and `value` (creator-docs DataStorePages), the rest as objects.
enum Shown {
    Entry(String, i64),
    Object(&'static str, Vec<(&'static str, Value)>),
}

fn shown(item: Item) -> Shown {
    match item {
        Item::Entry { key, value } => Shown::Entry(key, value),
        Item::Key(name) => Shown::Object("DataStoreKey", vec![("KeyName", Value::String(name))]),
        Item::Version {
            id,
            created,
            deleted,
        } => Shown::Object(
            "DataStoreObjectVersionInfo",
            vec![
                ("CreatedTime", Value::Int64(created)),
                ("IsDeleted", Value::Bool(deleted)),
                ("Version", Value::String(id)),
            ],
        ),
        Item::Store {
            name,
            created,
            updated,
        } => Shown::Object(
            "DataStoreInfo",
            vec![
                ("CreatedTime", Value::Int64(created)),
                ("DataStoreName", Value::String(name)),
                ("UpdatedTime", Value::Int64(updated)),
            ],
        ),
    }
}
