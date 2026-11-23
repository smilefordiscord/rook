//! Pages objects for the members that list in pages:
//! `Pages:GetCurrentPage`, `AdvanceToNextPageAsync` and `IsFinished`
//! (creator-docs Pages). Each object keeps its current page and where the
//! next one comes from, fetched when the script advances.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::Value;
use serde_json::{Value as Json, json};

use super::RawMethod;
use super::http::from_json;
use crate::api::instance::push_instance;
use crate::api::methods::mutate;
use crate::memory_stores::Time;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::world::World;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Pages", "GetCurrentPage") => current_page,
        ("Pages", "AdvanceToNextPageAsync") => advance,
        _ => return None,
    })
}

/// Where a page comes from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Source {
    /// `MemoryStoreHashMap:ListItemsAsync`: `count` items after the key
    /// `after`, read when the page is fetched.
    HashMap {
        name: String,
        count: usize,
        after: Option<String>,
    },
}

/// A Pages object's state.
#[derive(Debug)]
pub(crate) struct Pages {
    current: Vec<Json>,
    /// None once the current page is the last (`IsFinished`).
    next: Option<Source>,
}

fn fetch(w: &mut World, source: Source) -> Pages {
    match source {
        Source::HashMap { name, count, after } => {
            let now = Time(w.scheduler.now());
            let map = w.memory_stores.hash_map(&name);
            let (items, more) = map.page(now, after.as_deref(), count);
            let last = items.last().map(|(k, _)| k.clone());
            let current = (items.into_iter())
                .map(|(key, value)| json!({ "key": key, "value": value }))
                .collect();
            let next = more.then_some(Source::HashMap {
                name,
                count,
                after: last,
            });
            Pages { current, next }
        }
    }
}

/// Store `pages` as `id`'s and set its `IsFinished`.
fn keep(w: &mut World, id: InstanceId, pages: Pages) -> Result<(), rook_dom::DomError> {
    let finished = pages.next.is_none();
    w.pages.insert(id, pages);
    w.dom
        .set_property_by_name(id, "IsFinished", Value::Bool(finished))
}

/// A new Pages object of `class` on the first page from `source`.
pub(crate) fn create(lua: &Lua, class: &str, source: Source) -> ApiResult<Outcome> {
    let id = mutate(lua, |w| {
        let id = w.dom.create_by_name(class)?;
        let pages = fetch(w, source);
        keep(w, id, pages)?;
        Ok(id)
    })?;
    Outcome::ret(lua, push_instance(lua, id)?)
}

fn current_page(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let page = with_world(lua, |w| w.pages.get(&this).map(|p| p.current.clone()));
    let list = lua.create_table()?;
    for entry in page.unwrap_or_default() {
        list.raw_push(from_json(lua, &entry)?)?;
    }
    Outcome::ret(lua, list)
}

/// The error past the last page is Inference.
fn advance(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let next = with_world(lua, |w| w.pages.get_mut(&this).and_then(|p| p.next.take()));
    let Some(source) = next else {
        return Err(ScriptError("No pages to advance to".into()));
    };
    mutate(lua, |w| {
        let pages = fetch(w, source);
        keep(w, this, pages)
    })?;
    Outcome::ret(lua, ())
}
