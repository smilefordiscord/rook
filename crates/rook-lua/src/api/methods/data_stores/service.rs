//! DataStoreService's own members: handing out stores, the request
//! budgets, and listing the data stores.

use mlua::Lua;
use rook_dom::InstanceId;

use super::{RawMethod, clock, players};
use crate::api::instance::push_instance;
use crate::data_stores::{
    Listing, Reported, Scope, StoreError, StoreId, StoreName, check_name, check_scope,
    configurable, reported,
};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("DataStoreService", "GetDataStore") => get_data_store,
        ("DataStoreService", "GetGlobalDataStore") => get_global_data_store,
        ("DataStoreService", "GetOrderedDataStore") => get_ordered_data_store,
        ("DataStoreService", "GetRequestBudgetForRequestType") => request_budget,
        ("DataStoreService", "SetRateLimitForRequestType") => set_rate_limit,
        _ => return None,
    })
}

/// The object for a store: the same one each time it's asked for
/// (creator-docs GetDataStore: "Subsequent calls to this method with the
/// same name/scope will return the same object").
fn store_object(lua: &Lua, class: &str, name: &str, id: StoreId) -> ApiResult<Outcome> {
    let object: ApiResult<InstanceId> = with_world(lua, |w| {
        if w.is_client {
            return Err(StoreError::Client.into());
        }
        if let Some(object) = w.data_stores.objects.store(&id) {
            return Ok(object);
        }
        let object = w.dom.create_by_name(class)?;
        w.dom.set_name(object, name.to_owned())?;
        w.data_stores.objects.add_store(id, object);
        Ok(object)
    });
    Outcome::ret(lua, push_instance(lua, object?)?)
}

/// `GetDataStore(name, scope = "global", options)`. With
/// `DataStoreOptions.AllScopes` the scope must be empty and keys name
/// their scope (creator-docs DataStoreOptions.AllScopes).
fn get_data_store(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let scope = a.opt_string(1)?.unwrap_or_else(|| "global".into());
    let all_scopes = match a.opt_instance(2)? {
        Some(options) => {
            with_world(lua, |w| w.dom.bool_property(options, "AllScopes")).unwrap_or(false)
        }
        None => false,
    };
    check_name(&name)?;
    let scope = if all_scopes {
        if !scope.is_empty() {
            return Err(StoreError::ScopeWithAllScopes.into());
        }
        Scope::All
    } else {
        check_scope(&scope)?;
        Scope::One(scope)
    };
    let id = StoreId::Standard {
        name: StoreName::Named(name.clone()),
        scope,
    };
    store_object(lua, "DataStore", &name, id)
}

/// The default store, which always uses the scope `u` (creator-docs). Its
/// class follows the API dump's return type, `DataStore` (Inference).
fn get_global_data_store(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let id = StoreId::Standard {
        name: StoreName::Global,
        scope: Scope::One("u".into()),
    };
    store_object(lua, "DataStore", "GlobalDataStore", id)
}

fn get_ordered_data_store(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let scope = a.opt_string(1)?.unwrap_or_else(|| "global".into());
    check_name(&name)?;
    check_scope(&scope)?;
    let id = StoreId::Ordered {
        name: name.clone(),
        scope,
    };
    store_object(lua, "OrderedDataStore", &name, id)
}

fn request_budget(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let request_type = a.enum_value(lua, 0, "DataStoreRequestType")?;
    let budget = with_world(lua, |w| {
        let (now, _) = clock(w);
        let players = players(w);
        let mut remaining = |pool| w.data_stores.budgets.remaining(pool, now, players);
        match reported(request_type) {
            Some(Reported::Pool(pool)) => remaining(pool),
            Some(Reported::Both(a, b)) => remaining(a).min(remaining(b)),
            Some(Reported::Zero) | None => 0,
        }
    });
    Outcome::ret(lua, budget)
}

/// `SetRateLimitForRequestType(requestType, baseLimit, perPlayerLimit)`.
/// Inference: a limit outside the docs' constraints raises an error.
fn set_rate_limit(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let request_type = a.enum_value(lua, 0, "DataStoreRequestType")?;
    let base = a.number(1)?;
    let per_player = a.number(2)?;
    let name = crate::types::enums::item_name(lua, a.get(0), "DataStoreRequestType")
        .unwrap_or("DataStoreRequestType");
    let (pool, max_base, max_per_player) = configurable(request_type, name)?;
    let within = |n: f64, max: u32| (0.0..=f64::from(max)).contains(&n);
    if !within(base, max_base) {
        return Err(StoreError::LimitRange {
            name,
            argument: "baseLimit",
            max: max_base,
        }
        .into());
    }
    if !within(per_player, max_per_player) {
        return Err(StoreError::LimitRange {
            name,
            argument: "perPlayerLimit",
            max: max_per_player,
        }
        .into());
    }
    with_world(lua, |w| {
        (w.data_stores.budgets).set_limit(pool, base as u32, per_player as u32);
    });
    Outcome::ret(lua, ())
}

/// `ListDataStoresAsync(prefix, pageSize, cursor)`: the data stores
/// holding a key, by name.
pub(super) fn list_data_stores(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let prefix = a.opt_string(0)?.unwrap_or_default();
    let size = super::pages::page_size(a, 1, crate::data_stores::STORES_PAGE)?;
    let cursor = a.opt_string(2)?.filter(|c| !c.is_empty());
    let listing = Listing::Stores {
        prefix,
        size,
        after: cursor,
    };
    super::pages::open(lua, "DataStoreListingPages", listing)
}
