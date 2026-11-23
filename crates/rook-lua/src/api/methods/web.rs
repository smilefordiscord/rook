//! Members Roblox answers from its servers: HttpService, users, groups,
//! badges, text filtering, MemoryStores, MessagingService and
//! MarketplaceService, through [`requests`] where Rook has an endpoint.
//! DataStores have their own topic (`super::data_stores`).

use crate::api::methods::{RawMethod, first};

mod badges;
pub(crate) mod filtering;
mod groups;
pub(crate) mod http;
mod json;
pub(crate) mod marketplace;
pub(crate) mod memory_stores;
pub(crate) mod messaging;
pub(crate) mod pages;
pub(crate) mod requests;
mod users;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[
            marketplace::native,
            http::native,
            users::native,
            groups::native,
            badges::native,
            filtering::native,
            memory_stores::native,
            messaging::native,
            pages::native,
        ],
        owner,
        name,
    )
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    first(
        &[
            http::luau_source,
            memory_stores::luau_source,
        ],
        owner,
        name,
    )
}
