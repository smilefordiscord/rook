//! Data store requests: what each spends from the server's budgets, the
//! queue it waits in once they're spent, and running it. The methods are
//! Luau (`api/luau/datastore.luau`) over three helpers: `admit` spends a
//! request's budget or queues it, `poll` asks again for a queued one, and
//! `run` does the request.

use mlua::{Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;

use super::{RawMethod, clock, entries, pages, players, service, target, versions};
use crate::data_stores::{Admission, Cost, Listing, Pool, Queue, StoreError, StoreId};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, api_ret, with_world};
use crate::runtime::world::MessageType;

/// A request a method makes, and the helper that does it once admitted.
struct Op {
    /// The method's name, for argument errors.
    method: &'static str,
    run: RawMethod,
}

/// The requests by the name the Luau side passes. `UpdateAsync` is two:
/// its read, then the write of what its transform returned.
fn op(name: &str) -> Option<Op> {
    let (method, run): (&'static str, RawMethod) = match name {
        "GetAsync" => ("GetAsync", entries::get),
        "SetAsync" => ("SetAsync", entries::set),
        "IncrementAsync" => ("IncrementAsync", entries::increment),
        "RemoveAsync" => ("RemoveAsync", entries::remove),
        "UpdateAsync" => ("UpdateAsync", entries::update_read),
        "UpdateWrite" => ("UpdateAsync", entries::update_write),
        "BatchGetAsync" => ("BatchGetAsync", entries::batch_get),
        "GetSortedAsync" => ("GetSortedAsync", entries::get_sorted),
        "GetVersionAsync" => ("GetVersionAsync", versions::get_version),
        "GetVersionAtTimeAsync" => ("GetVersionAtTimeAsync", versions::get_version_at_time),
        "RemoveVersionAsync" => ("RemoveVersionAsync", versions::remove_version),
        "ListKeysAsync" => ("ListKeysAsync", versions::list_keys),
        "ListVersionsAsync" => ("ListVersionsAsync", versions::list_versions),
        "ListDataStoresAsync" => ("ListDataStoresAsync", service::list_data_stores),
        "AdvanceToNextPageAsync" => ("AdvanceToNextPageAsync", pages::advance),
        _ => return None,
    };
    Some(Op { method, run })
}

/// The methods, each a Luau function over the helpers.
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    macro_rules! request {
        ($name:literal) => {
            concat!(
                "local std = ...\nreturn std.datastore.method(\"",
                $name,
                "\")"
            )
        };
    }
    Some(match (owner, name) {
        ("GlobalDataStore", "GetAsync") => request!("GetAsync"),
        ("GlobalDataStore", "SetAsync") => request!("SetAsync"),
        ("GlobalDataStore", "IncrementAsync") => request!("IncrementAsync"),
        ("GlobalDataStore", "RemoveAsync") => request!("RemoveAsync"),
        ("GlobalDataStore", "BatchGetAsync") => request!("BatchGetAsync"),
        ("GlobalDataStore", "UpdateAsync") => "local std = ...\nreturn std.datastore.update",
        ("OrderedDataStore", "GetSortedAsync") => request!("GetSortedAsync"),
        ("DataStore", "GetVersionAsync") => request!("GetVersionAsync"),
        ("DataStore", "GetVersionAtTimeAsync") => request!("GetVersionAtTimeAsync"),
        ("DataStore", "RemoveVersionAsync") => request!("RemoveVersionAsync"),
        ("DataStore", "ListKeysAsync") => request!("ListKeysAsync"),
        ("DataStore", "ListVersionsAsync") => request!("ListVersionsAsync"),
        ("DataStoreService", "ListDataStoresAsync") => request!("ListDataStoresAsync"),
        ("Pages", "AdvanceToNextPageAsync") => request!("AdvanceToNextPageAsync"),
        _ => return None,
    })
}

/// The helpers the Luau methods use (`std.datastore`).
pub(crate) fn helpers(lua: &Lua, std: &Table) -> mlua::Result<Table> {
    let task: Table = std.get("task")?;
    let coroutine: Table = std.get("coroutine")?;
    lua.load(include_str!("../../luau/datastore.luau"))
        .set_name("=[rook] DataStore")
        .call((
            api_fn(lua, admit)?,
            api_ret(lua, poll)?,
            api_fn(lua, run)?,
            task.get::<LuaValue>("wait")?,
            task.get::<LuaValue>("cancel")?,
            coroutine.get::<LuaValue>("create")?,
            coroutine.get::<LuaValue>("resume")?,
            coroutine.get::<LuaValue>("status")?,
            std.get::<LuaValue>("error")?,
        ))
}

/// The request's op, its object and its arguments, from what the Luau side
/// passes: the request's name, then the method's own arguments.
fn parse(args: MultiValue) -> ApiResult<(Op, InstanceId, Args)> {
    let mut args = args.into_vec();
    let name = match args.first() {
        Some(LuaValue::String(s)) => s.to_string_lossy(),
        _ => String::new(),
    };
    let op = op(&name).ok_or_else(|| ScriptError(format!("no data store request {name}")))?;
    args.remove(0);
    let (this, a) = Args::method(args.into_iter().collect(), op.method)?;
    Ok((op, this, a))
}

/// What a request spends, the queue it waits in and how it's named when
/// dropped.
struct Request {
    cost: Cost,
    /// The key it's for, which the queue's warning names.
    key: Option<String>,
    code: Option<u16>,
    dropped_as: &'static str,
}

/// A request's arguments checked, and what it spends; `None` for a read
/// the cache answers. Advancing pages is a request of the method they came
/// from.
fn request(
    lua: &Lua,
    method: &'static str,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Option<Request>> {
    let (endpoint, standard, key, count) = match method {
        "AdvanceToNextPageAsync" => {
            let (listing, finished) = pages::listing_of(lua, this)?;
            if finished {
                return Err(StoreError::NoMorePages.into());
            }
            let standard = !matches!(listing, Listing::Sorted { .. });
            (listing.method(), standard, None, 1)
        }
        "ListDataStoresAsync" => (method, true, None, 1),
        _ => {
            let store = target(lua, this)?;
            let standard = matches!(store, StoreId::Standard { .. });
            match method {
                "GetSortedAsync" => {
                    entries::sorted_listing(lua, this, a)?;
                    (method, false, None, 1)
                }
                "BatchGetAsync" => (method, false, None, entries::batch_keys(&store, a)?.len()),
                "ListKeysAsync" => (method, true, None, 1),
                _ => {
                    let key = a.string(0)?;
                    entries::cache_key(&store, &key)?;
                    if method == "GetVersionAtTimeAsync" {
                        versions::timestamp(lua, a)?;
                    }
                    if method == "GetAsync" && entries::from_cache(lua, &store, a)? {
                        return Ok(None);
                    }
                    (method, standard, Some(key), 1)
                }
            }
        }
    };
    let (pools, queue) = spends(endpoint, standard);
    let (code, dropped_as) = dropped(endpoint);
    Ok(Some(Request {
        cost: Cost {
            pools: pools.iter().map(|&p| (p, count as f64)).collect(),
            queue,
        },
        key,
        code,
        dropped_as,
    }))
}

/// The budgets a request spends (creator-docs server limits; `UpdateAsync`
/// spends from both its read and write budgets) and the queue it waits in
/// (creator-docs: set, ordered set, get and ordered get; Inference: which
/// requests wait in which).
fn spends(method: &str, standard: bool) -> (&'static [Pool], Queue) {
    use Pool::*;
    match (method, standard) {
        ("GetAsync" | "GetVersionAsync" | "GetVersionAtTimeAsync", true) => {
            (&[StandardRead], Queue::Get)
        }
        ("SetAsync" | "IncrementAsync", true) => (&[StandardWrite], Queue::Set),
        ("UpdateAsync", true) => (&[StandardRead, StandardWrite], Queue::Set),
        ("RemoveAsync", true) => (&[StandardRemove], Queue::Set),
        ("RemoveVersionAsync", _) => (&[RemoveVersion], Queue::Set),
        ("GetAsync" | "BatchGetAsync", false) => (&[OrderedRead], Queue::OrderedGet),
        ("SetAsync" | "IncrementAsync", false) => (&[OrderedWrite], Queue::OrderedSet),
        ("UpdateAsync", false) => (&[OrderedRead, OrderedWrite], Queue::OrderedSet),
        ("RemoveAsync", false) => (&[OrderedRemove], Queue::OrderedSet),
        ("GetSortedAsync", _) => (&[OrderedList], Queue::OrderedGet),
        // ListKeysAsync, ListVersionsAsync and ListDataStoresAsync.
        _ => (&[StandardList], Queue::Get),
    }
}

/// How a dropped request is named: the methods the docs number, and the
/// rest by name (creator-docs error codes).
fn dropped(method: &'static str) -> (Option<u16>, &'static str) {
    match method {
        "GetAsync" => (Some(301), method),
        "SetAsync" => (Some(302), method),
        "IncrementAsync" => (Some(303), method),
        "UpdateAsync" => (Some(304), method),
        "GetSortedAsync" => (Some(305), "GetSorted"),
        "RemoveAsync" => (Some(306), method),
        _ => (None, method),
    }
}

/// `admit(name, object, ...)`: nothing when the request may go ahead, or
/// a ticket and the seconds to wait before asking again. A queued request
/// warns, as Roblox's do (Inference: the warning scripts report seeing).
fn admit(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let (op, this, a) = parse(args)?;
    let Some(request) = request(lua, op.method, this, &a)? else {
        return Outcome::ret(lua, ());
    };
    let admission = with_world(lua, |w| {
        let (now, _) = clock(w);
        let players = players(w);
        let admission = w.data_stores.budgets.admit(&request.cost, now, players);
        if let Some(Admission::Queued { .. }) = admission {
            let key = (request.key.as_deref())
                .map(|k| format!("Key = {k}"))
                .unwrap_or_default();
            w.push_log(
                MessageType::Warning,
                format!(
                    "DataStore request was added to queue. If request queue fills, further requests will be dropped. Try sending fewer requests.{key}"
                ),
            );
        }
        admission
    });
    match admission {
        Some(Admission::Now) => Outcome::ret(lua, ()),
        Some(Admission::Queued { ticket, wait }) => Outcome::ret(lua, (ticket, wait)),
        None => Err(StoreError::Dropped {
            code: request.code,
            method: request.dropped_as,
        }
        .into()),
    }
}

/// `poll(ticket)`: nil once the queued request may go ahead, else the
/// seconds to wait again.
fn poll(lua: &Lua, args: MultiValue) -> ApiResult<Option<f64>> {
    let ticket = Args::new(args, "poll").number(0)? as u64;
    Ok(with_world(lua, |w| {
        let (now, _) = clock(w);
        let players = players(w);
        w.data_stores.budgets.poll(ticket, now, players)
    }))
}

/// `run(name, object, ...)`: do the request.
fn run(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let (op, this, a) = parse(args)?;
    (op.run)(lua, this, &a)
}
