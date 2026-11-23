//! Members Roblox answers from its servers: HttpService, users and text filtering, through [`requests`] where Rook has an endpoint. DataStores have their own topic (`super::data_stores`).

use crate::api::methods::{RawMethod, first};

pub(crate) mod filtering;
pub(crate) mod http;
pub(crate) mod requests;
mod users;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[http::native, users::native, filtering::native],
        owner,
        name,
    )
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    http::luau_source(owner, name)
}
