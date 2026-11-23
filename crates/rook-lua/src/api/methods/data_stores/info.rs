//! `DataStoreKeyInfo`, which reads return beside a value, and the
//! metadata `DataStoreSetOptions` and `DataStoreIncrementOptions` carry.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::Value;
use serde_json::{Map, Value as Json};

use super::RawMethod;
use crate::api::instance::push_instance;
use crate::data_stores::{KeyInfo, StoreError, metadata_of};
use crate::json;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("DataStoreKeyInfo", "GetMetadata") => key_metadata,
        ("DataStoreKeyInfo", "GetUserIds") => user_ids,
        ("DataStoreSetOptions" | "DataStoreIncrementOptions", "SetMetadata") => set_metadata,
        ("DataStoreSetOptions" | "DataStoreIncrementOptions", "GetMetadata") => options_metadata,
        // Creator-docs: "Calling this method has no observable effect".
        ("DataStoreOptions", "SetExperimentalFeatures") => super::super::inert,
        _ => return None,
    })
}

/// A new `DataStoreKeyInfo` for a version, or nil for none.
pub(super) fn key_info(lua: &Lua, info: Option<KeyInfo>) -> ApiResult<LuaValue> {
    let Some(info) = info else {
        return Ok(LuaValue::Nil);
    };
    let id: ApiResult<InstanceId> = with_world(lua, |w| {
        let id = w.dom.create_by_name("DataStoreKeyInfo")?;
        w.dom
            .set_property_by_name(id, "CreatedTime", Value::Int64(info.created))?;
        w.dom
            .set_property_by_name(id, "UpdatedTime", Value::Int64(info.updated))?;
        w.dom
            .set_property_by_name(id, "Version", Value::String(info.version.clone()))?;
        w.data_stores.objects.key_infos.insert(id, info);
        Ok(id)
    });
    Ok(LuaValue::UserData(push_instance(lua, id?)?))
}

fn table_of(lua: &Lua, metadata: Map<String, Json>) -> ApiResult<Outcome> {
    Outcome::ret(lua, json::to_lua(lua, &Json::Object(metadata))?)
}

fn key_metadata(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let metadata = with_world(lua, |w| {
        (w.data_stores.objects.key_infos.get(&this)).map(|i| i.metadata.clone())
    });
    table_of(lua, metadata.unwrap_or_default())
}

fn user_ids(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let ids = with_world(lua, |w| {
        (w.data_stores.objects.key_infos.get(&this)).map(|i| i.user_ids.clone())
    });
    let t = lua.create_sequence_from(ids.unwrap_or_default())?;
    Outcome::ret(lua, t)
}

/// `SetMetadata(attributes)`. Metadata that can't be stored raises its
/// error when a request uses it (Inference: the docs list the metadata
/// errors among the requests').
fn set_metadata(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let metadata = match a.get(0) {
        LuaValue::Table(t) => metadata_of(t),
        LuaValue::Nil => return Err(a.missing(0)),
        _ => Err(StoreError::BadMetadata),
    };
    with_world(lua, |w| {
        w.data_stores.objects.metadata.insert(this, metadata);
    });
    Outcome::ret(lua, ())
}

fn options_metadata(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let metadata = with_world(lua, |w| {
        (w.data_stores.objects.metadata.get(&this)).and_then(|m| m.clone().ok())
    });
    table_of(lua, metadata.unwrap_or_default())
}

/// The metadata an options object passed to a request carries.
pub(super) fn options_of(lua: &Lua, options: Option<InstanceId>) -> ApiResult<Map<String, Json>> {
    let Some(options) = options else {
        return Ok(Map::new());
    };
    let metadata = with_world(lua, |w| {
        w.data_stores.objects.metadata.get(&options).cloned()
    });
    metadata
        .unwrap_or_else(|| Ok(Map::new()))
        .map_err(ScriptError::from)
}
