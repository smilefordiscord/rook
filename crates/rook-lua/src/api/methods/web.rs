//! Members Roblox answers from its servers: HttpService, users, text filtering, DataStores and MarketplaceService, through [`requests`] where Rook has an endpoint.

use crate::api::methods::{RawMethod, first};

pub(crate) mod data_stores;
pub(crate) mod filtering;
pub(crate) mod http;
pub(crate) mod marketplace;
pub(crate) mod requests;
mod users;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[
            marketplace::native,
            data_stores::native,
            http::native,
            users::native,
            filtering::native,
        ],
        owner,
        name,
    )
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    first(&[data_stores::luau_source, http::luau_source], owner, name)
}
